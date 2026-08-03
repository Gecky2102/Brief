use std::path::PathBuf;
use std::process::Command;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_dialog::DialogExt;

use crate::models;

/// Whisper vuole PCM mono a 16 kHz: qualunque sia il formato di partenza lo si
/// normalizza con `afconvert`, che gestisce m4a, mp3, aiff, wav e caf.
fn to_whisper_wav(source: &std::path::Path, destination: &std::path::Path) -> Result<(), String> {
    let status = Command::new("/usr/bin/afconvert")
        .args(["-f", "WAVE", "-d", "LEI16@16000", "-c", "1"])
        .arg(source)
        .arg(destination)
        .status()
        .map_err(|cause| format!("Conversione non riuscita: {cause}"))?;

    if !status.success() {
        return Err("Formato audio non riconosciuto.".into());
    }
    Ok(())
}

fn read_samples(path: &std::path::Path) -> Result<Vec<f32>, String> {
    let bytes = std::fs::read(path).map_err(|cause| format!("Audio illeggibile: {cause}"))?;
    if bytes.len() <= 44 {
        return Err("Il file audio è vuoto.".into());
    }
    Ok(bytes[44..]
        .chunks_exact(2)
        .map(|pair| i16::from_le_bytes([pair[0], pair[1]]) as f32 / 32768.0)
        .collect())
}

#[derive(Clone, Serialize)]
pub struct ImportProgress {
    pub done_ms: i64,
    pub total_ms: i64,
}

#[derive(Serialize)]
pub struct ImportedAudio {
    pub file_name: String,
    pub duration_ms: i64,
    pub directory: String,
}

/// Importa un file audio, lo trascrive per intero e ne emette i segmenti con
/// gli stessi eventi della registrazione dal vivo.
#[tauri::command]
pub fn import_audio(app: AppHandle, session_id: i64) -> Result<ImportedAudio, String> {
    let picked = app
        .dialog()
        .file()
        // Tutti verificati con afconvert: la decodifica passa da CoreAudio,
        // che copre molto più dei formati Apple.
        .add_filter(
            "Audio",
            &[
                "m4a", "mp3", "wav", "aiff", "aif", "caf", "aac", "mp4", "m4b", "flac", "opus",
                "ogg", "mov", "wma", "amr",
            ],
        )
        .blocking_pick_file()
        .ok_or_else(|| "Nessun file scelto.".to_string())?;

    let source = picked
        .into_path()
        .map_err(|cause| format!("Percorso non valido: {cause}"))?;

    let file_name = source
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "audio".into());

    let directory: PathBuf = app
        .path()
        .app_data_dir()
        .map_err(|cause| format!("Cartella dati non disponibile: {cause}"))?
        .join("recordings")
        .join(format!("import-{session_id}"));
    std::fs::create_dir_all(&directory)
        .map_err(|cause| format!("Cartella non creata: {cause}"))?;

    // La traccia si chiama "system" perché l'audio importato non è la voce di
    // chi usa Brief: viene mostrato come interlocutore.
    let wav = directory.join("system.wav");
    to_whisper_wav(&source, &wav)?;

    let samples = read_samples(&wav)?;
    let model = models::ensure(&app, &models::WHISPER)?;
    let duration_ms = (samples.len() as i64) * 1000 / 16_000;

    crate::transcriber::transcribe_samples(&app, session_id, "system", &samples, &model)?;

    let _ = app.emit(
        "import://progress",
        ImportProgress {
            done_ms: duration_ms,
            total_ms: duration_ms,
        },
    );

    Ok(ImportedAudio {
        file_name,
        duration_ms,
        directory: directory.to_string_lossy().into_owned(),
    })
}
