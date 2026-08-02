use std::ffi::CString;
use std::os::raw::{c_char, c_float};
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

type LevelCallback = extern "C" fn(track: i32, rms: c_float, elapsed_ms: i64);
type SamplesCallback =
    extern "C" fn(track: i32, samples: *const i16, count: std::os::raw::c_int, start_ms: i64);

extern "C" {
    fn brief_capture_start(
        directory: *const c_char,
        callback: LevelCallback,
        samples: SamplesCallback,
    ) -> i32;
    fn brief_capture_stop() -> i64;
    fn brief_capture_is_running() -> i32;
}

static APP: OnceLock<AppHandle> = OnceLock::new();
static ACTIVE_DIRECTORY: Mutex<Option<PathBuf>> = Mutex::new(None);

pub fn remember_app_handle(app: AppHandle) {
    let _ = APP.set(app);
}

#[derive(Clone, Serialize)]
struct LevelEvent {
    track: &'static str,
    rms: f32,
    elapsed_ms: i64,
}

/// Chiamata da Swift a ogni finestra di livello, da una coda audio: deve
/// restare economica e non deve mai fare panic oltre il confine FFI.
extern "C" fn on_level(track: i32, rms: c_float, elapsed_ms: i64) {
    let Some(app) = APP.get() else { return };
    let track = match track {
        0 => "mic",
        1 => "system",
        _ => return,
    };
    let _ = app.emit(
        "audio://level",
        LevelEvent {
            track,
            rms,
            elapsed_ms,
        },
    );
}

fn describe(code: i32) -> &'static str {
    match code {
        1 => "Permesso microfono negato. Concedilo in Impostazioni di Sistema › Privacy e sicurezza › Microfono.",
        2 => "Permesso registrazione schermo negato: serve per catturare l'audio di sistema. Concedilo in Impostazioni di Sistema › Privacy e sicurezza › Registrazione schermo.",
        3 => "Impossibile avviare il motore audio: nessun dispositivo di ingresso disponibile.",
        4 => "Registrazione già in corso.",
        6 => "Impossibile scrivere i file audio su disco.",
        7 => "Richiesto macOS 13 o successivo.",
        _ => "Avvio della registrazione fallito.",
    }
}

#[derive(Serialize)]
pub struct StartedRecording {
    directory: String,
    started_at_ms: u64,
}

#[derive(Serialize)]
pub struct FinishedRecording {
    directory: String,
    duration_ms: i64,
    mic_path: String,
    system_path: String,
}

fn epoch_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis() as u64)
        .unwrap_or(0)
}

fn recordings_root(app: &AppHandle) -> Result<PathBuf, String> {
    let root = app
        .path()
        .app_data_dir()
        .map_err(|cause| format!("Cartella dati non disponibile: {cause}"))?
        .join("recordings");
    std::fs::create_dir_all(&root)
        .map_err(|cause| format!("Impossibile creare la cartella delle registrazioni: {cause}"))?;
    Ok(root)
}

#[tauri::command]
pub fn start_recording(app: AppHandle, session_id: i64) -> Result<StartedRecording, String> {
    let started_at_ms = epoch_millis();
    let directory = recordings_root(&app)?.join(started_at_ms.to_string());
    std::fs::create_dir_all(&directory)
        .map_err(|cause| format!("Impossibile creare la cartella della sessione: {cause}"))?;

    let path = CString::new(directory.to_string_lossy().as_bytes())
        .map_err(|_| "Percorso della sessione non valido.".to_string())?;

    // Il trascrittore parte per primo: se il modello manca o non si carica è
    // meglio saperlo prima di aver registrato qualcosa.
    crate::transcriber::start(&app, session_id)?;

    let code = unsafe { brief_capture_start(path.as_ptr(), on_level, crate::transcriber::on_samples) };
    if code != 0 {
        crate::transcriber::stop();
        let _ = std::fs::remove_dir_all(&directory);
        return Err(describe(code).to_string());
    }

    *ACTIVE_DIRECTORY.lock().unwrap() = Some(directory.clone());

    Ok(StartedRecording {
        directory: directory.to_string_lossy().into_owned(),
        started_at_ms,
    })
}

#[tauri::command]
pub fn stop_recording() -> Result<FinishedRecording, String> {
    let duration_ms = unsafe { brief_capture_stop() };
    crate::transcriber::stop();
    if duration_ms < 0 {
        return Err("Nessuna registrazione in corso.".into());
    }

    let directory = ACTIVE_DIRECTORY
        .lock()
        .unwrap()
        .take()
        .ok_or_else(|| "Cartella della sessione non trovata.".to_string())?;

    Ok(FinishedRecording {
        mic_path: directory.join("mic.wav").to_string_lossy().into_owned(),
        system_path: directory.join("system.wav").to_string_lossy().into_owned(),
        directory: directory.to_string_lossy().into_owned(),
        duration_ms,
    })
}

#[tauri::command]
pub fn is_recording() -> bool {
    unsafe { brief_capture_is_running() == 1 }
}
