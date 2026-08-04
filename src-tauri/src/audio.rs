#[cfg(target_os = "macos")]
use std::os::raw::c_char;
use std::os::raw::c_float;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Emitter, Manager};

pub type LevelCallback = extern "C" fn(track: i32, rms: c_float, elapsed_ms: i64);
pub type SamplesCallback =
    extern "C" fn(track: i32, samples: *const i16, count: std::os::raw::c_int, start_ms: i64);

/// Guadagno per traccia, condiviso fra le due implementazioni di cattura.
static GAIN: [std::sync::atomic::AtomicU32; 2] = [
    std::sync::atomic::AtomicU32::new(1_000),
    std::sync::atomic::AtomicU32::new(1_000),
];

pub fn current_gain(track: i32) -> f32 {
    let indice = track.clamp(0, 1) as usize;
    GAIN[indice].load(std::sync::atomic::Ordering::Relaxed) as f32 / 1000.0
}

#[cfg(target_os = "macos")]
extern "C" {
    fn brief_capture_start(
        directory: *const c_char,
        callback: LevelCallback,
        samples: SamplesCallback,
    ) -> i32;
    fn brief_capture_stop() -> i64;
    fn brief_capture_is_running() -> i32;
    fn brief_capture_system_health() -> i64;
    fn brief_set_gain(track: i32, value: f32);
}

/// Le due piattaforme catturano l'audio in modo completamente diverso: macOS
/// con ScreenCaptureKit dal codice Swift, Windows con WASAPI in loopback.
mod backend {
    #[allow(unused_imports)]
    use super::{LevelCallback, SamplesCallback};

    #[cfg(target_os = "macos")]
    pub fn start(
        directory: &std::path::Path,
        level: LevelCallback,
        samples: SamplesCallback,
    ) -> i32 {
        let Ok(path) = std::ffi::CString::new(directory.to_string_lossy().as_bytes())
        else {
            return 6;
        };
        unsafe { super::brief_capture_start(path.as_ptr(), level, samples) }
    }

    #[cfg(target_os = "macos")]
    pub fn stop() -> i64 {
        unsafe { super::brief_capture_stop() }
    }

    #[cfg(target_os = "macos")]
    pub fn is_running() -> bool {
        unsafe { super::brief_capture_is_running() == 1 }
    }

    #[cfg(target_os = "macos")]
    pub fn system_health() -> i64 {
        unsafe { super::brief_capture_system_health() }
    }

    #[cfg(target_os = "macos")]
    pub fn set_gain(track: i32, value: f32) {
        unsafe { super::brief_set_gain(track, value) }
    }

    #[cfg(windows)]
    pub use crate::capture_windows::{is_running, start, stop, system_health};

    #[cfg(windows)]
    pub fn set_gain(_track: i32, _value: f32) {
        // Su Windows il guadagno lo legge direttamente la cattura da GAIN.
    }
}

static APP: OnceLock<AppHandle> = OnceLock::new();
static ACTIVE_DIRECTORY: Mutex<Option<PathBuf>> = Mutex::new(None);
static TRAY: Mutex<Option<tauri::tray::TrayIcon>> = Mutex::new(None);

/// Un indicatore nella barra dei menu ricorda che Brief sta registrando anche
/// quando la finestra è nascosta dietro ad altro.
fn show_recording_indicator(app: &AppHandle) {
    let tray = TrayIconBuilder::new()
        .title("● REC")
        .tooltip("Brief sta registrando")
        .on_tray_icon_event(|tray, _| {
            if let Some(window) = tray.app_handle().get_webview_window("main") {
                let _ = window.show();
                let _ = window.set_focus();
            }
        })
        .build(app);

    if let Ok(tray) = tray {
        *TRAY.lock().unwrap() = Some(tray);
    }
}

fn hide_recording_indicator() {
    *TRAY.lock().unwrap() = None;
}

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
pub async fn start_recording(
    app: AppHandle,
    session_id: i64,
) -> Result<StartedRecording, String> {
    tauri::async_runtime::spawn_blocking(move || start_blocking(app, session_id))
        .await
        .map_err(|cause| format!("Avvio interrotto: {cause}"))?
}

fn start_blocking(app: AppHandle, session_id: i64) -> Result<StartedRecording, String> {
    let started_at_ms = epoch_millis();
    let directory = recordings_root(&app)?.join(started_at_ms.to_string());
    std::fs::create_dir_all(&directory)
        .map_err(|cause| format!("Impossibile creare la cartella della sessione: {cause}"))?;

    // Il trascrittore parte per primo: se il modello manca o non si carica è
    // meglio saperlo prima di aver registrato qualcosa.
    crate::transcriber::start(&app, session_id)?;

    let code = backend::start(&directory, on_level, crate::transcriber::on_samples);
    if code != 0 {
        crate::transcriber::stop();
        let _ = std::fs::remove_dir_all(&directory);
        return Err(describe(code).to_string());
    }

    *ACTIVE_DIRECTORY.lock().unwrap() = Some(directory.clone());
    show_recording_indicator(&app);

    Ok(StartedRecording {
        directory: directory.to_string_lossy().into_owned(),
        started_at_ms,
    })
}

#[tauri::command]
pub async fn stop_recording() -> Result<FinishedRecording, String> {
    tauri::async_runtime::spawn_blocking(stop_blocking)
        .await
        .map_err(|cause| format!("Arresto interrotto: {cause}"))?
}

fn stop_blocking() -> Result<FinishedRecording, String> {
    let duration_ms = backend::stop();
    hide_recording_indicator();
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

/// `-1` traccia di sistema non avviata, `0` avviata ma senza campioni,
/// altrimenti il numero di campioni ricevuti finora.
#[tauri::command]
pub fn system_track_health() -> i64 {
    backend::system_health()
}

/// Guadagno di una traccia, come un cursore di mixer: 1 lascia il livello
/// originale, valori più alti alzano il segnale fino a quattro volte.
#[tauri::command]
pub fn set_track_gain(track: String, value: f32) {
    let indice = match track.as_str() {
        "mic" => 0,
        "system" => 1,
        _ => return,
    };
    let limitato = value.clamp(0.0, 4.0);
    GAIN[indice.clamp(0, 1) as usize].store(
        (limitato * 1000.0) as u32,
        std::sync::atomic::Ordering::Relaxed,
    );
    backend::set_gain(indice, limitato);
}

#[tauri::command]
pub fn is_recording() -> bool {
    backend::is_running()
}
