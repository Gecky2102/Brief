use std::path::{Path, PathBuf};
use std::process::Command;

use tauri::AppHandle;
use tauri_plugin_dialog::DialogExt;

/// `afconvert` fa parte di macOS: comprimere in AAC riduce un'ora di parlato da
/// ~110 MB di WAV a ~28 MB, e i WAV a 16 kHz servivano solo alla trascrizione.
fn to_aac(source: &Path, destination: &Path) -> Result<(), String> {
    let status = Command::new("/usr/bin/afconvert")
        .arg("-f")
        .arg("m4af")
        .arg("-d")
        .arg("aac")
        .arg("-b")
        .arg("64000")
        .arg(source)
        .arg(destination)
        .status()
        .map_err(|cause| format!("Compressione audio non riuscita: {cause}"))?;

    if !status.success() {
        return Err("Compressione audio non riuscita.".into());
    }
    Ok(())
}

fn session_directory(app: &AppHandle, directory: &str) -> Result<PathBuf, String> {
    use tauri::Manager;

    let root = app
        .path()
        .app_data_dir()
        .map_err(|cause| format!("Cartella dati non disponibile: {cause}"))?
        .join("recordings")
        .canonicalize()
        .map_err(|cause| format!("Cartella delle registrazioni non trovata: {cause}"))?;

    let candidate = PathBuf::from(directory)
        .canonicalize()
        .map_err(|_| "Cartella della sessione non trovata.".to_string())?;

    // Il percorso arriva dal frontend: senza questo controllo un valore
    // manipolato potrebbe far leggere o cancellare file fuori dall'archivio.
    if !candidate.starts_with(&root) {
        return Err("Cartella della sessione non valida.".into());
    }
    Ok(candidate)
}

#[tauri::command]
pub async fn compress_recording(app: AppHandle, directory: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || compress_blocking(app, directory))
        .await
        .map_err(|cause| format!("Compressione interrotta: {cause}"))?
}

fn compress_blocking(app: AppHandle, directory: String) -> Result<(), String> {
    let directory = session_directory(&app, &directory)?;

    for track in ["mic", "system"] {
        let wav = directory.join(format!("{track}.wav"));
        if !wav.exists() {
            continue;
        }
        let m4a = directory.join(format!("{track}.m4a"));
        to_aac(&wav, &m4a)?;
        let _ = std::fs::remove_file(&wav);
    }
    Ok(())
}

#[tauri::command]
pub async fn export_markdown(
    app: AppHandle,
    file_name: String,
    contents: String,
) -> Result<bool, String> {
    tauri::async_runtime::spawn_blocking(move || export_markdown_blocking(app, file_name, contents))
        .await
        .map_err(|cause| format!("Esportazione interrotta: {cause}"))?
}

fn export_markdown_blocking(
    app: AppHandle,
    file_name: String,
    contents: String,
) -> Result<bool, String> {
    let safe_name = file_name
        .chars()
        .filter(|c| !matches!(c, '/' | '\\' | ':'))
        .collect::<String>();

    let target = app
        .dialog()
        .file()
        .set_file_name(&safe_name)
        .add_filter("Markdown", &["md"])
        .blocking_save_file();

    let Some(target) = target else { return Ok(false) };
    let path = target
        .into_path()
        .map_err(|cause| format!("Percorso non valido: {cause}"))?;

    std::fs::write(&path, contents)
        .map_err(|cause| format!("Impossibile scrivere il file: {cause}"))?;
    Ok(true)
}

#[tauri::command]
pub async fn export_audio(app: AppHandle, directory: String) -> Result<bool, String> {
    tauri::async_runtime::spawn_blocking(move || export_audio_blocking(app, directory))
        .await
        .map_err(|cause| format!("Esportazione interrotta: {cause}"))?
}

fn export_audio_blocking(app: AppHandle, directory: String) -> Result<bool, String> {
    let source = session_directory(&app, &directory)?;

    let target = app.dialog().file().blocking_pick_folder();
    let Some(target) = target else { return Ok(false) };
    let target = target
        .into_path()
        .map_err(|cause| format!("Percorso non valido: {cause}"))?;

    let label = source
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "sessione".into());

    let mut copied = false;
    for track in ["mic", "system"] {
        for extension in ["m4a", "wav"] {
            let file = source.join(format!("{track}.{extension}"));
            if file.exists() {
                std::fs::copy(&file, target.join(format!("brief-{label}-{track}.{extension}")))
                    .map_err(|cause| format!("Copia non riuscita: {cause}"))?;
                copied = true;
                break;
            }
        }
    }

    if !copied {
        return Err("Nessun file audio trovato per questa sessione.".into());
    }
    Ok(true)
}

/// Percorso del file audio di una sessione, se esiste. Serve all'interfaccia
/// per riprodurlo mentre si legge la trascrizione.
#[tauri::command]
pub fn audio_file(app: AppHandle, directory: String) -> Result<Option<String>, String> {
    let source = session_directory(&app, &directory)?;

    // La traccia di sistema contiene gli interlocutori: è quella che serve
    // riascoltare per verificare una parola dubbia.
    for track in ["system", "mic"] {
        for extension in ["m4a", "wav"] {
            let file = source.join(format!("{track}.{extension}"));
            if file.exists() {
                return Ok(Some(file.to_string_lossy().into_owned()));
            }
        }
    }
    Ok(None)
}

#[tauri::command]
pub fn delete_recording(app: AppHandle, directory: String) -> Result<(), String> {
    let directory = session_directory(&app, &directory)?;
    std::fs::remove_dir_all(&directory)
        .map_err(|cause| format!("Impossibile eliminare i file audio: {cause}"))?;
    Ok(())
}
