//! Cattura audio su Windows.
//!
//! Su macOS il lavoro lo fa ScreenCaptureKit dal codice Swift. Qui entrambe le
//! tracce passano da cpal: il microfono dal dispositivo di ingresso, l'audio di
//! sistema aprendo in ingresso il dispositivo di uscita, che è il modo in cui
//! WASAPI espone ciò che sta suonando.

use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::sync::{Arc, Mutex};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

use crate::audio::{LevelCallback, SamplesCallback};

const TARGET_RATE: u32 = 16_000;
const TRACK_MIC: i32 = 0;
const TRACK_SYSTEM: i32 = 1;
const LEVEL_INTERVAL_MS: i64 = 50;

struct Running {
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
    let rate = config.sample_rate();
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
            config.into(),
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

/// Audio di sistema: su Windows il dispositivo di uscita si può aprire in
/// ingresso, ed è così che WASAPI espone ciò che sta suonando. Non tutte le
/// schede audio lo permettono, quindi il fallimento non è un errore fatale.
fn start_system_loopback(
    directory: &std::path::Path,
    level: LevelCallback,
    samples: SamplesCallback,
    started_at: std::time::Instant,
) -> Result<(Box<dyn std::any::Any + Send>, Arc<Mutex<super::wav::WavWriter>>), i32> {
    let host = cpal::default_host();
    let device = host.default_output_device().ok_or(2)?;

    // Configurazione di ingresso sul dispositivo di uscita: è la richiesta di
    // loopback vera e propria.
    let config = device.default_input_config().map_err(|_| 2)?;

    let writer = Arc::new(Mutex::new(
        super::wav::WavWriter::create(&directory.join("system.wav")).map_err(|_| 6)?,
    ));

    let canali = config.channels();
    let rate = config.sample_rate();
    let mut stato = TrackState {
        track: TRACK_SYSTEM,
        writer: writer.clone(),
        level,
        samples,
        started_at,
        last_level_ms: 0,
        peak: 0.0,
    };

    let stream = device
        .build_input_stream(
            config.into(),
            move |dati: &[f32], _| {
                SYSTEM_SAMPLES.fetch_add(dati.len() as i64, Ordering::Relaxed);
                stato.push(&to_mono_16k(dati, canali, rate));
            },
            |_| {},
            None,
        )
        .map_err(|_| 2)?;

    stream.play().map_err(|_| 2)?;
    SYSTEM_STARTED.store(true, Ordering::Relaxed);
    Ok((Box::new(stream), writer))
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

    let (mic_stream, mic_writer) = match start_microphone(directory, level, samples, started_at) {
        Ok(valore) => valore,
        Err(codice) => return codice,
    };

    let mut writers = vec![mic_writer];
    let mut streams: Vec<Box<dyn std::any::Any + Send>> = vec![mic_stream];

    // Come su macOS, l'audio di sistema è un di più: se non parte si registra
    // comunque il microfono.
    if let Ok((stream, writer)) = start_system_loopback(directory, level, samples, started_at) {
        writers.push(writer);
        streams.push(stream);
    }

    *RUNNING.lock().unwrap() = Some(Running {
        started_at,
        writers,
        _streams: streams,
    });

    0
}

pub fn stop() -> i64 {
    let Some(running) = RUNNING.lock().unwrap().take() else {
        return -1;
    };

    // I flussi si fermano quando vengono rilasciati con `running`: un istante
    // perché le ultime richiamate finiscano prima di chiudere i file.
    std::thread::sleep(std::time::Duration::from_millis(200));

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
