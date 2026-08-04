//! Cattura audio su Windows.
//!
//! Su macOS il lavoro lo fa ScreenCaptureKit dal codice Swift; qui servono due
//! strade diverse: il microfono passa da cpal, mentre l'audio di sistema si
//! prende con il loopback di WASAPI, che è l'equivalente Windows della cattura
//! dell'uscita audio.

use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::sync::{Arc, Mutex};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

use crate::audio::{LevelCallback, SamplesCallback};

const TARGET_RATE: u32 = 16_000;
const TRACK_MIC: i32 = 0;
const TRACK_SYSTEM: i32 = 1;
const LEVEL_INTERVAL_MS: i64 = 50;

struct Running {
    stop: Arc<AtomicBool>,
    started_at: std::time::Instant,
    writers: Vec<Arc<Mutex<super::wav::WavWriter>>>,
    /// I flussi vanno tenuti in vita: rilasciandoli la cattura si ferma.
    _streams: Vec<Box<dyn std::any::Any + Send>>,
}

static RUNNING: Mutex<Option<Running>> = Mutex::new(None);
static SYSTEM_SAMPLES: AtomicI64 = AtomicI64::new(0);
static SYSTEM_STARTED: AtomicBool = AtomicBool::new(false);

/// Converte a 16 kHz mono prendendo un campione ogni N e mediando i canali:
/// per il parlato è sufficiente e costa una frazione di un ricampionatore
/// completo.
fn to_mono_16k(input: &[f32], channels: u16, rate: u32) -> Vec<i16> {
    if channels == 0 || rate == 0 {
        return Vec::new();
    }

    let frames = input.len() / channels as usize;
    let passo = rate as f32 / TARGET_RATE as f32;
    let attesi = (frames as f32 / passo) as usize;
    let mut uscita = Vec::with_capacity(attesi);

    let mut posizione = 0.0_f32;
    while (posizione as usize) < frames {
        let inizio = posizione as usize * channels as usize;
        let somma: f32 = input[inizio..inizio + channels as usize].iter().sum();
        let media = somma / channels as f32;
        uscita.push((media.clamp(-1.0, 1.0) * 32767.0) as i16);
        posizione += passo;
    }

    uscita
}

struct TrackState {
    track: i32,
    writer: Arc<Mutex<super::wav::WavWriter>>,
    level: LevelCallback,
    samples: SamplesCallback,
    started_at: std::time::Instant,
    last_level_ms: i64,
    peak: f32,
}

impl TrackState {
    fn push(&mut self, pcm: &[i16]) {
        if pcm.is_empty() {
            return;
        }

        let elapsed_ms = self.started_at.elapsed().as_millis() as i64;

        // Il guadagno impostato dal mixer vale anche qui, con lo stesso limite
        // per non saturare.
        let gain = crate::audio::current_gain(self.track);
        let mut dati = pcm.to_vec();
        if (gain - 1.0).abs() > f32::EPSILON {
            for campione in dati.iter_mut() {
                *campione = (*campione as f32 * gain).clamp(-32768.0, 32767.0) as i16;
            }
        }

        if let Ok(mut writer) = self.writer.lock() {
            let _ = writer.write(&dati);
        }

        (self.samples)(self.track, dati.as_ptr(), dati.len() as i32, elapsed_ms);

        if self.track == TRACK_SYSTEM {
            SYSTEM_SAMPLES.fetch_add(dati.len() as i64, Ordering::Relaxed);
        }

        let somma: f64 = dati
            .iter()
            .map(|c| {
                let n = *c as f64 / 32768.0;
                n * n
            })
            .sum();
        let rms = (somma / dati.len() as f64).sqrt() as f32;
        self.peak = self.peak.max(rms);

        if elapsed_ms - self.last_level_ms >= LEVEL_INTERVAL_MS {
            self.last_level_ms = elapsed_ms;
            (self.level)(self.track, self.peak, elapsed_ms);
            self.peak = 0.0;
        }
    }
}

fn start_microphone(
    directory: &std::path::Path,
    level: LevelCallback,
    samples: SamplesCallback,
    started_at: std::time::Instant,
) -> Result<(Box<dyn std::any::Any + Send>, Arc<Mutex<super::wav::WavWriter>>), i32> {
    let host = cpal::default_host();
    let device = host.default_input_device().ok_or(3)?;
    let config = device.default_input_config().map_err(|_| 3)?;

    let writer = Arc::new(Mutex::new(
        super::wav::WavWriter::create(&directory.join("mic.wav")).map_err(|_| 6)?,
    ));

    let canali = config.channels();
    let rate = config.sample_rate().0;
    let mut stato = TrackState {
        track: TRACK_MIC,
        writer: writer.clone(),
        level,
        samples,
        started_at,
        last_level_ms: 0,
        peak: 0.0,
    };

    let stream = device
        .build_input_stream(
            &config.into(),
            move |dati: &[f32], _| {
                stato.push(&to_mono_16k(dati, canali, rate));
            },
            |_| {},
            None,
        )
        .map_err(|_| 3)?;

    stream.play().map_err(|_| 3)?;
    Ok((Box::new(stream), writer))
}

/// Audio di sistema tramite il loopback di WASAPI: si apre il dispositivo di
/// uscita in modalità cattura e si legge ciò che sta suonando.
fn start_system_loopback(
    directory: &std::path::Path,
    level: LevelCallback,
    samples: SamplesCallback,
    started_at: std::time::Instant,
    stop: Arc<AtomicBool>,
) -> Result<Arc<Mutex<super::wav::WavWriter>>, i32> {
    let writer = Arc::new(Mutex::new(
        super::wav::WavWriter::create(&directory.join("system.wav")).map_err(|_| 6)?,
    ));

    let writer_thread = writer.clone();

    std::thread::spawn(move || {
        if wasapi::initialize_mta().is_err() {
            return;
        }

        let Ok(device) = wasapi::get_default_device(&wasapi::Direction::Render) else {
            return;
        };
        let Ok(mut client) = device.get_iaudioclient() else {
            return;
        };

        let formato = wasapi::WaveFormat::new(32, 32, &wasapi::SampleType::Float, 48000, 2, None);
        if client
            .initialize_client(
                &formato,
                0,
                &wasapi::Direction::Capture,
                &wasapi::ShareMode::Shared,
                true,
            )
            .is_err()
        {
            return;
        }

        let Ok(evento) = client.set_get_eventhandle() else {
            return;
        };
        let Ok(cattura) = client.get_audiocaptureclient() else {
            return;
        };
        if client.start_stream().is_err() {
            return;
        }

        SYSTEM_STARTED.store(true, Ordering::Relaxed);

        let mut stato = TrackState {
            track: TRACK_SYSTEM,
            writer: writer_thread,
            level,
            samples,
            started_at,
            last_level_ms: 0,
            peak: 0.0,
        };

        let mut coda: std::collections::VecDeque<u8> = std::collections::VecDeque::new();

        while !stop.load(Ordering::Relaxed) {
            if evento.wait_for_event(200).is_err() {
                continue;
            }
            if cattura.read_from_device_to_deque(&mut coda).is_err() {
                continue;
            }

            if coda.is_empty() {
                continue;
            }

            let grezzi: Vec<u8> = coda.drain(..).collect();
            let campioni: Vec<f32> = grezzi
                .chunks_exact(4)
                .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
                .collect();

            stato.push(&to_mono_16k(&campioni, 2, 48000));
        }

        let _ = client.stop_stream();
    });

    Ok(writer)
}

pub fn start(
    directory: &std::path::Path,
    level: LevelCallback,
    samples: SamplesCallback,
) -> i32 {
    if RUNNING.lock().map(|g| g.is_some()).unwrap_or(false) {
        return 4;
    }

    SYSTEM_SAMPLES.store(0, Ordering::Relaxed);
    SYSTEM_STARTED.store(false, Ordering::Relaxed);

    let started_at = std::time::Instant::now();
    let stop = Arc::new(AtomicBool::new(false));

    let (stream, mic_writer) = match start_microphone(directory, level, samples, started_at) {
        Ok(valore) => valore,
        Err(codice) => return codice,
    };

    // Come su macOS, l'audio di sistema è un di più: se non parte si registra
    // comunque il microfono.
    let system_writer =
        start_system_loopback(directory, level, samples, started_at, stop.clone()).ok();

    let mut writers = vec![mic_writer];
    if let Some(writer) = system_writer {
        writers.push(writer);
    }

    *RUNNING.lock().unwrap() = Some(Running {
        stop,
        started_at,
        writers,
        _streams: vec![stream],
    });

    0
}

pub fn stop() -> i64 {
    let Some(running) = RUNNING.lock().unwrap().take() else {
        return -1;
    };

    running.stop.store(true, Ordering::Relaxed);
    // Un istante perché il thread del loopback esca dal ciclo prima di chiudere
    // i file.
    std::thread::sleep(std::time::Duration::from_millis(250));

    for writer in &running.writers {
        if let Ok(mut writer) = writer.lock() {
            writer.close();
        }
    }

    running.started_at.elapsed().as_millis() as i64
}

pub fn is_running() -> bool {
    RUNNING.lock().map(|g| g.is_some()).unwrap_or(false)
}

pub fn system_health() -> i64 {
    if !SYSTEM_STARTED.load(Ordering::Relaxed) {
        return -1;
    }
    SYSTEM_SAMPLES.load(Ordering::Relaxed)
}
