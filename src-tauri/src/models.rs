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

/// Modello di trascrizione grande: qualità nettamente superiore su parlato
/// spontaneo e dialetti, e grazie alla variante «turbo» resta veloce.
pub const WHISPER_ACCURATE: ModelSpec = ModelSpec {
    key: "whisper",
    label: "Modello di trascrizione accurato",
    file_name: "ggml-large-v3-turbo-q5_0.bin",
    url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-large-v3-turbo-q5_0.bin",
    sha256: "394221709cd5ad1f40c46e6031ca61bce88931e6e088c188294c6d5a55ffa7e2",
    bytes: 574_041_195,
};

/// Impronta vocale: serve a capire chi parla, raggruppando le porzioni di
/// parlato che hanno la stessa voce.
pub const SPEAKER: ModelSpec = ModelSpec {
    key: "speaker",
    label: "Modello di riconoscimento voci",
    file_name: "wespeaker-resnet34.onnx",
    url: "https://huggingface.co/onnx-community/wespeaker-voxceleb-resnet34-LM/resolve/main/onnx/model.onnx",
    sha256: "3955447b0499dc9e0a4541a895df08b03c69098eba4e56c02b5603e9f7f4fcbb",
    bytes: 26_535_549,
};

/// Ricerca per significato: trasforma il testo in un vettore, così «ritardi
/// dei fornitori» trova anche chi diceva «non arrivano in tempo».
pub const EMBEDDING: ModelSpec = ModelSpec {
    key: "embedding",
    label: "Modello di ricerca per significato",
    file_name: "multilingual-e5-small.onnx",
    url: "https://huggingface.co/Xenova/multilingual-e5-small/resolve/main/onnx/model_quantized.onnx",
    sha256: "f80102d3f2a1229f387d3c81909990d8945513e347b0eab049f7de3c6f98c193",
    bytes: 118_308_185,
};

pub const EMBEDDING_TOKENIZER: ModelSpec = ModelSpec {
    key: "embedding",
    label: "Vocabolario della ricerca",
    file_name: "multilingual-e5-small-tokenizer.json",
    url: "https://huggingface.co/Xenova/multilingual-e5-small/resolve/main/tokenizer.json",
    sha256: "0b44a9d7b51c3c62626640cda0e2c2f70fdacdc25bbbd68038369d14ebdf4c39",
    bytes: 17_082_730,
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

    // Un modello può pesare gigabyte: se un tentativo precedente si è
    // interrotto si riparte da dove era arrivato, non da capo.
    let already = std::fs::metadata(&temporary)
        .map(|meta| meta.len())
        .unwrap_or(0);

    let mut hasher = Sha256::new();
    let mut downloaded = 0_u64;

    if already > 0 && already < spec.bytes {
        // L'hash si calcola sull'intero file: la parte già scaricata va
        // ripassata nell'hasher prima di riprendere.
        let mut existing = std::fs::File::open(&temporary)
            .map_err(|cause| format!("Ripresa del download fallita: {cause}"))?;
        let mut buffer = vec![0_u8; 1 << 20];
        loop {
            let read = existing
                .read(&mut buffer)
                .map_err(|cause| format!("Ripresa del download fallita: {cause}"))?;
            if read == 0 {
                break;
            }
            hasher.update(&buffer[..read]);
            downloaded += read as u64;
        }
    } else if already >= spec.bytes {
        let _ = std::fs::remove_file(&temporary);
    }

    let mut request = ureq::get(spec.url);
    if downloaded > 0 {
        request = request.header("Range", &format!("bytes={downloaded}-"));
    }

    let response = request
        .call()
        .map_err(|cause| format!("Download di «{}» fallito: {cause}", spec.label))?;

    // Se il server ignora la richiesta di ripresa si ricomincia da zero.
    let resuming = response.status().as_u16() == 206;
    if downloaded > 0 && !resuming {
        hasher = Sha256::new();
        downloaded = 0;
    }

    let total = spec.bytes;

    let mut reader = response.into_body().into_reader();
    let mut file = if resuming {
        std::fs::OpenOptions::new()
            .append(true)
            .open(&temporary)
            .map_err(|cause| format!("Impossibile scrivere il modello: {cause}"))?
    } else {
        std::fs::File::create(&temporary)
            .map_err(|cause| format!("Impossibile scrivere il modello: {cause}"))?
    };

    let mut buffer = vec![0_u8; 1 << 20];
    let mut last_reported: u64 = downloaded;

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

#[derive(Serialize)]
pub struct ModelStatus {
    pub key: &'static str,
    pub label: &'static str,
    pub file_name: &'static str,
    pub bytes: u64,
    /// Byte già presenti su disco: uguale a `bytes` se completo, minore se il
    /// download si è interrotto, zero se assente.
    pub on_disk: u64,
    pub complete: bool,
    pub in_use: bool,
}

pub const ALL: [&ModelSpec; 5] = [
    &WHISPER,
    &WHISPER_ACCURATE,
    &SPEAKER,
    &EMBEDDING,
    &EMBEDDING_TOKENIZER,
];

fn size_on_disk(dir: &std::path::Path, spec: &ModelSpec) -> (u64, bool) {
    let complete = dir.join(spec.file_name);
    if let Ok(meta) = std::fs::metadata(&complete) {
        return (meta.len(), true);
    }
    let partial = complete.with_extension("part");
    match std::fs::metadata(&partial) {
        Ok(meta) => (meta.len(), false),
        Err(_) => (0, false),
    }
}

#[derive(Serialize)]
pub struct StorageReport {
    pub models: Vec<ModelStatus>,
    pub used_bytes: u64,
    pub free_bytes: u64,
}

#[tauri::command]
pub fn storage_report(app: AppHandle) -> Result<StorageReport, String> {
    let dir = models_dir(&app)?;
    let quality = crate::settings::load(&app).quality;
    let in_uso = [crate::settings::whisper_model_for(quality).file_name, SPEAKER.file_name];

    let mut models = Vec::new();
    let mut used_bytes = 0;

    for spec in ALL {
        let (on_disk, complete) = size_on_disk(&dir, spec);
        used_bytes += on_disk;
        models.push(ModelStatus {
            key: spec.key,
            label: spec.label,
            file_name: spec.file_name,
            bytes: spec.bytes,
            on_disk,
            complete,
            in_use: in_uso.contains(&spec.file_name),
        });
    }

    // Spazio libero sul volume che ospita i modelli.
    let free_bytes = free_space(&dir).unwrap_or(0);

    Ok(StorageReport {
        models,
        used_bytes,
        free_bytes,
    })
}

fn free_space(path: &std::path::Path) -> Option<u64> {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;

    let raw = CString::new(path.as_os_str().as_bytes()).ok()?;
    let mut stats: libc::statvfs = unsafe { std::mem::zeroed() };
    if unsafe { libc::statvfs(raw.as_ptr(), &mut stats) } != 0 {
        return None;
    }
    Some(stats.f_bavail as u64 * stats.f_frsize as u64)
}

/// Ricontrolla l'hash di un modello già presente: un file corrotto da un
/// disco pieno o da un'interruzione produce errori incomprensibili al primo uso.
#[tauri::command]
pub async fn verify_model(app: AppHandle, file_name: String) -> Result<bool, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let spec = ALL
            .iter()
            .find(|spec| spec.file_name == file_name)
            .ok_or_else(|| "Modello sconosciuto.".to_string())?;

        let path = models_dir(&app)?.join(spec.file_name);
        if !path.exists() {
            return Ok(false);
        }

        let mut file = std::fs::File::open(&path)
            .map_err(|cause| format!("Modello non leggibile: {cause}"))?;
        let mut hasher = Sha256::new();
        let mut buffer = vec![0_u8; 1 << 20];
        loop {
            let letti = file
                .read(&mut buffer)
                .map_err(|cause| format!("Lettura interrotta: {cause}"))?;
            if letti == 0 {
                break;
            }
            hasher.update(&buffer[..letti]);
        }

        let digest = hasher
            .finalize()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();

        Ok(digest == spec.sha256)
    })
    .await
    .map_err(|cause| format!("Verifica interrotta: {cause}"))?
}

/// Elimina un modello scaricato, o il residuo di un download interrotto.
#[tauri::command]
pub fn delete_model(app: AppHandle, file_name: String) -> Result<(), String> {
    // Solo i nomi noti: il valore arriva dal frontend e non deve poter
    // indicare un file qualsiasi.
    let spec = ALL
        .iter()
        .find(|spec| spec.file_name == file_name)
        .ok_or_else(|| "Modello sconosciuto.".to_string())?;

    let dir = models_dir(&app)?;
    for path in [dir.join(spec.file_name), dir.join(spec.file_name).with_extension("part")] {
        if path.exists() {
            std::fs::remove_file(&path)
                .map_err(|cause| format!("Impossibile eliminare il modello: {cause}"))?;
        }
    }
    Ok(())
}
