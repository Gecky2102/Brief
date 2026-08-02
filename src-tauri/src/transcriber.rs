use std::os::raw::c_int;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

use serde::Serialize;
use tauri::{AppHandle, Emitter};
use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

use crate::models;

const SAMPLE_RATE: i64 = 16_000;
/// Whisper lavora male sotto il secondo di contesto.
const MIN_SEGMENT_MS: i64 = 1_500;
/// Oltre questa soglia si taglia comunque, anche senza pause: meglio un taglio
/// netto che una latenza che cresce senza fine.
const MAX_SEGMENT_MS: i64 = 15_000;
const SILENCE_TO_CUT_MS: i64 = 600;
const SILENCE_RMS: f32 = 0.012;

struct Chunk {
    samples: Vec<i16>,
    start_ms: i64,
}

#[derive(Clone, Serialize)]
struct SegmentEvent {
    session_id: i64,
    track: &'static str,
    start_ms: i64,
    end_ms: i64,
    text: String,
}

struct Running {
    senders: Vec<Sender<Chunk>>,
    workers: Vec<JoinHandle<()>>,
}

static RUNNING: Mutex<Option<Running>> = Mutex::new(None);
static CONTEXT: Mutex<Option<Arc<WhisperContext>>> = Mutex::new(None);

/// Chiamata da Swift dalla coda audio: copia i campioni e torna subito, il
/// lavoro pesante è nei worker.
pub extern "C" fn on_samples(track: i32, samples: *const i16, count: c_int, start_ms: i64) {
    if samples.is_null() || count <= 0 {
        return;
    }
    let Ok(guard) = RUNNING.lock() else { return };
    let Some(running) = guard.as_ref() else { return };
    let Some(sender) = running.senders.get(track as usize) else {
        return;
    };

    let slice = unsafe { std::slice::from_raw_parts(samples, count as usize) };
    let _ = sender.send(Chunk {
        samples: slice.to_vec(),
        start_ms,
    });
}

fn rms(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    let sum: f32 = samples.iter().map(|value| value * value).sum();
    (sum / samples.len() as f32).sqrt()
}

fn transcribe(context: &WhisperContext, audio: &[f32]) -> Result<String, String> {
    let mut state = context
        .create_state()
        .map_err(|cause| format!("Stato Whisper non creato: {cause}"))?;

    let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
    params.set_translate(false);
    params.set_print_special(false);
    params.set_print_progress(false);
    params.set_print_realtime(false);
    params.set_print_timestamps(false);
    params.set_suppress_blank(true);
    params.set_n_threads(4);

    state
        .full(params, audio)
        .map_err(|cause| format!("Trascrizione fallita: {cause}"))?;

    let mut text = String::new();
    for index in 0..state.full_n_segments() {
        let Some(segment) = state.get_segment(index) else {
            continue;
        };
        if let Ok(piece) = segment.to_str_lossy() {
            text.push_str(piece.trim());
            text.push(' ');
        }
    }
    Ok(text.trim().to_string())
}

/// Whisper allucina volentieri sul silenzio, restituendo sigle di sottotitoli o
/// ringraziamenti presi dai video di addestramento.
fn is_noise(text: &str) -> bool {
    let cleaned = text
        .trim()
        .trim_matches(|c: char| !c.is_alphanumeric())
        .to_lowercase();
    if cleaned.chars().count() < 3 {
        return true;
    }
    const HALLUCINATIONS: [&str; 8] = [
        "sottotitoli e revisione a cura di qtss",
        "sottotitoli creati dalla comunità amara.org",
        "grazie per aver guardato il video",
        "thanks for watching",
        "sottotitoli e revisione a cura di",
        "amara.org",
        "www.donkeysanctuary.org.uk",
        "sous-titrage société radio-canada",
    ];
    HALLUCINATIONS
        .iter()
        .any(|pattern| cleaned.contains(pattern))
}

fn worker(
    app: AppHandle,
    session_id: i64,
    track: &'static str,
    context: Arc<WhisperContext>,
    receiver: Receiver<Chunk>,
) {
    let mut buffer: Vec<f32> = Vec::new();
    let mut buffer_start_ms: i64 = 0;
    let mut silence_ms: i64 = 0;

    let mut flush = |buffer: &mut Vec<f32>, buffer_start_ms: i64, end_ms: i64| {
        if buffer.is_empty() {
            return;
        }
        let audio = std::mem::take(buffer);
        match transcribe(&context, &audio) {
            Ok(text) if !is_noise(&text) => {
                let _ = app.emit(
                    "transcript://segment",
                    SegmentEvent {
                        session_id,
                        track,
                        start_ms: buffer_start_ms,
                        end_ms,
                        text,
                    },
                );
            }
            Ok(_) => {}
            Err(message) => {
                let _ = app.emit("transcript://error", message);
            }
        }
    };

    while let Ok(chunk) = receiver.recv() {
        if buffer.is_empty() {
            buffer_start_ms = chunk.start_ms;
            silence_ms = 0;
        }

        let converted: Vec<f32> = chunk
            .samples
            .iter()
            .map(|sample| *sample as f32 / 32768.0)
            .collect();
        let chunk_ms = (converted.len() as i64) * 1000 / SAMPLE_RATE;

        if rms(&converted) < SILENCE_RMS {
            silence_ms += chunk_ms;
        } else {
            silence_ms = 0;
        }

        buffer.extend_from_slice(&converted);
        let buffered_ms = (buffer.len() as i64) * 1000 / SAMPLE_RATE;
        let end_ms = buffer_start_ms + buffered_ms;

        let pause_reached = buffered_ms >= MIN_SEGMENT_MS && silence_ms >= SILENCE_TO_CUT_MS;
        if pause_reached || buffered_ms >= MAX_SEGMENT_MS {
            flush(&mut buffer, buffer_start_ms, end_ms);
            silence_ms = 0;
        }
    }

    let buffered_ms = (buffer.len() as i64) * 1000 / SAMPLE_RATE;
    if buffered_ms >= 500 {
        flush(&mut buffer, buffer_start_ms, buffer_start_ms + buffered_ms);
    }
}

pub fn is_model_ready(app: &AppHandle) -> bool {
    models::path_if_present(app, &models::WHISPER).is_some()
}

pub fn start(app: &AppHandle, session_id: i64) -> Result<(), String> {
    let path = models::ensure(app, &models::WHISPER)?;

    let context = Arc::new(
        WhisperContext::new_with_params(&path, WhisperContextParameters::default())
        .map_err(|cause| format!("Modello di trascrizione non caricato: {cause}"))?,
    );
    *CONTEXT.lock().unwrap() = Some(context.clone());

    let mut senders = Vec::new();
    let mut workers = Vec::new();
    for track in ["mic", "system"] {
        let (sender, receiver) = channel::<Chunk>();
        let app = app.clone();
        let context = context.clone();
        senders.push(sender);
        workers.push(std::thread::spawn(move || {
            worker(app, session_id, track, context, receiver)
        }));
    }

    *RUNNING.lock().unwrap() = Some(Running { senders, workers });
    Ok(())
}

/// Chiude i worker e libera il modello: l'analisi che segue deve avere la RAM
/// tutta per sé.
pub fn stop() {
    let running = RUNNING.lock().unwrap().take();
    if let Some(running) = running {
        drop(running.senders);
        for worker in running.workers {
            let _ = worker.join();
        }
    }
    *CONTEXT.lock().unwrap() = None;
}
