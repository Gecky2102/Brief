use std::io::{Read, Write};
use std::path::PathBuf;

use serde::Serialize;
use sha2::{Digest, Sha256};
use tauri::{AppHandle, Emitter, Manager};

/// I modelli non sono nel bundle: pesano troppo. Vengono scaricati al primo uso
/// e verificati contro l'hash pubblicato, così una risposta manomessa o un
/// download troncato non finiscono mai in esecuzione.
pub struct ModelSpec {
    pub key: &'static str,
    pub label: &'static str,
    pub file_name: &'static str,
    pub url: &'static str,
    pub sha256: &'static str,
    pub bytes: u64,
}

pub const WHISPER: ModelSpec = ModelSpec {
    key: "whisper",
    label: "Modello di trascrizione",
    file_name: "ggml-small-q5_1.bin",
    url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-small-q5_1.bin",
    sha256: "ae85e4a935d7a567bd102fe55afc16bb595bdb618e11b2fc7591bc08120411bb",
    bytes: 190_085_487,
};

pub const LLM: ModelSpec = ModelSpec {
    key: "llm",
    label: "Modello di analisi",
    file_name: "qwen2.5-3b-instruct-q4_k_m.gguf",
    url: "https://huggingface.co/Qwen/Qwen2.5-3B-Instruct-GGUF/resolve/main/qwen2.5-3b-instruct-q4_k_m.gguf",
    sha256: "626b4a6678b86442240e33df819e00132d3ba7dddfe1cdc4fbb18e0a9615c62d",
    bytes: 2_104_932_768,
};

#[derive(Clone, Serialize)]
struct DownloadProgress {
    key: &'static str,
    label: &'static str,
    downloaded: u64,
    total: u64,
}

fn models_dir(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|cause| format!("Cartella dati non disponibile: {cause}"))?
        .join("models");
    std::fs::create_dir_all(&dir)
        .map_err(|cause| format!("Impossibile creare la cartella dei modelli: {cause}"))?;
    Ok(dir)
}

pub fn path_if_present(app: &AppHandle, spec: &ModelSpec) -> Option<PathBuf> {
    let path = models_dir(app).ok()?.join(spec.file_name);
    path.exists().then_some(path)
}

/// Restituisce il percorso del modello, scaricandolo se manca. Il download va
/// su un file temporaneo e viene promosso solo dopo la verifica dell'hash.
pub fn ensure(app: &AppHandle, spec: &ModelSpec) -> Result<PathBuf, String> {
    let destination = models_dir(app)?.join(spec.file_name);
    if destination.exists() {
        return Ok(destination);
    }

    let temporary = destination.with_extension("part");
    let response = ureq::get(spec.url)
        .call()
        .map_err(|cause| format!("Download di «{}» fallito: {cause}", spec.label))?;

    let total = response
        .headers()
        .get("content-length")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(spec.bytes);

    let mut reader = response.into_body().into_reader();
    let mut file = std::fs::File::create(&temporary)
        .map_err(|cause| format!("Impossibile scrivere il modello: {cause}"))?;

    let mut hasher = Sha256::new();
    let mut buffer = vec![0_u8; 1 << 20];
    let mut downloaded: u64 = 0;
    let mut last_reported: u64 = 0;

    loop {
        let read = reader
            .read(&mut buffer)
            .map_err(|cause| format!("Download interrotto: {cause}"))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
        file.write_all(&buffer[..read])
            .map_err(|cause| format!("Impossibile scrivere il modello: {cause}"))?;
        downloaded += read as u64;

        if downloaded - last_reported >= 4 << 20 {
            last_reported = downloaded;
            let _ = app.emit(
                "model://progress",
                DownloadProgress {
                    key: spec.key,
                    label: spec.label,
                    downloaded,
                    total,
                },
            );
        }
    }

    file.flush()
        .map_err(|cause| format!("Impossibile scrivere il modello: {cause}"))?;
    drop(file);

    let digest = hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    if digest != spec.sha256 {
        let _ = std::fs::remove_file(&temporary);
        return Err(format!(
            "Il file scaricato per «{}» non corrisponde all'hash atteso: scartato.",
            spec.label
        ));
    }

    std::fs::rename(&temporary, &destination)
        .map_err(|cause| format!("Impossibile salvare il modello: {cause}"))?;

    let _ = app.emit(
        "model://progress",
        DownloadProgress {
            key: spec.key,
            label: spec.label,
            downloaded: total,
            total,
        },
    );

    Ok(destination)
}
