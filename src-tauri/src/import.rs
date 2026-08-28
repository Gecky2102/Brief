use std::path::PathBuf;
use std::process::Command;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_dialog::DialogExt;

use crate::models;

/// Whisper vuole PCM mono a 16 kHz. La decodifica usa Symphonia in puro Rust
/// (multipiattaforma) con eventuale fallback ad `afconvert` su macOS.
fn decode_with_symphonia(source: &std::path::Path) -> Result<Vec<f32>, String> {
    use symphonia::core::codecs::{DecoderOptions, CODEC_TYPE_NULL};
    use symphonia::core::errors::Error;
    use symphonia::core::formats::FormatOptions;
    use symphonia::core::io::MediaSourceStream;
    use symphonia::core::meta::MetadataOptions;
    use symphonia::core::probe::Hint;

    let file = std::fs::File::open(source)
        .map_err(|cause| format!("Impossibile aprire il file audio: {cause}"))?;
    let mss = MediaSourceStream::new(Box::new(file), Default::default());

    let mut hint = Hint::new();
    if let Some(ext) = source.extension().and_then(|e| e.to_str()) {
        hint.with_extension(ext);
    }

    let format_opts = FormatOptions {
        enable_gapless: true,
        ..Default::default()
    };
    let metadata_opts: MetadataOptions = Default::default();
    let decoder_opts: DecoderOptions = Default::default();

    let probed = symphonia::default::get_probe()
        .format(&hint, mss, &format_opts, &metadata_opts)
        .map_err(|cause| format!("Formato audio non supportato: {cause}"))?;

    let mut format = probed.format;

    let track = format
        .tracks()
        .iter()
        .find(|t| t.codec_params.codec != CODEC_TYPE_NULL)
        .ok_or_else(|| "Nessuna traccia audio trovata nel file.".to_string())?;

    let track_id = track.id;
    let sample_rate = track.codec_params.sample_rate.unwrap_or(16_000);
    let channels = track
        .codec_params
        .channels
        .map(|c| c.count() as u16)
        .unwrap_or(1);

    let mut decoder = symphonia::default::get_codecs()
        .make(&track.codec_params, &decoder_opts)
        .map_err(|cause| format!("Codec audio non supportato: {cause}"))?;

    let mut interleaved_f32: Vec<f32> = Vec::new();

    loop {
        let packet = match format.next_packet() {
            Ok(packet) => packet,
            Err(Error::IoError(err)) if err.kind() == std::io::ErrorKind::UnexpectedEof => break,
            Err(Error::ResetRequired) => break,
            Err(err) => return Err(format!("Errore durante la lettura dell'audio: {err}")),
        };

        if packet.track_id() != track_id {
            continue;
        }

        match decoder.decode(&packet) {
            Ok(decoded) => {
                let mut sample_buf = symphonia::core::audio::SampleBuffer::<f32>::new(
                    decoded.capacity() as u64,
                    *decoded.spec(),
                );
                sample_buf.copy_interleaved_ref(decoded);
                interleaved_f32.extend_from_slice(sample_buf.samples());
            }
            Err(Error::DecodeError(_)) => continue,
            Err(err) => return Err(format!("Errore durante la decodifica dell'audio: {err}")),
        }
    }

    if interleaved_f32.is_empty() {
        return Err("Il file audio non contiene campioni validi.".into());
    }

    let num_channels = channels as usize;
    let frames = interleaved_f32.len() / num_channels.max(1);
    let step = sample_rate as f32 / 16_000.0;
    let expected_frames = (frames as f32 / step) as usize;
    let mut mono_16k = Vec::with_capacity(expected_frames);

    let mut pos = 0.0_f32;
    while (pos as usize) < frames {
        let frame_idx = pos as usize;
        let start = frame_idx * num_channels;
        let end = (start + num_channels).min(interleaved_f32.len());
        let sum: f32 = interleaved_f32[start..end].iter().sum();
        let avg = sum / (end - start).max(1) as f32;
        mono_16k.push(avg.clamp(-1.0, 1.0));
        pos += step;
    }

    Ok(mono_16k)
}

fn to_whisper_wav(
    source: &std::path::Path,
    destination: &std::path::Path,
) -> Result<Vec<f32>, String> {
    if let Ok(samples) = decode_with_symphonia(source) {
        let i16_samples: Vec<i16> = samples
            .iter()
            .map(|s| (s.clamp(-1.0, 1.0) * 32767.0) as i16)
            .collect();
        if let Ok(mut writer) = crate::wav::WavWriter::create(destination) {
            let _ = writer.write(&i16_samples);
            writer.close();
        }
        return Ok(samples);
    }

    #[cfg(target_os = "macos")]
    {
        let status = Command::new("/usr/bin/afconvert")
            .args(["-f", "WAVE", "-d", "LEI16@16000", "-c", "1"])
            .arg(source)
            .arg(destination)
            .status()
            .map_err(|cause| format!("Conversione non riuscita: {cause}"))?;

        if !status.success() {
            return Err("Formato audio non riconosciuto.".into());
        }
        return read_samples(destination);
    }

    #[cfg(not(target_os = "macos"))]
    Err("Formato audio non supportato.".into())
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
pub async fn import_audio(app: AppHandle, session_id: i64) -> Result<ImportedAudio, String> {
    tauri::async_runtime::spawn_blocking(move || import_blocking(app, session_id))
        .await
        .map_err(|cause| format!("Importazione interrotta: {cause}"))?
}

fn import_blocking(app: AppHandle, session_id: i64) -> Result<ImportedAudio, String> {
    let picked = app
        .dialog()
        .file()
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
    let samples = to_whisper_wav(&source, &wav)?;
    let duration_ms = (samples.len() as i64) * 1000 / 16_000;

    // Un file muto o cortissimo non produrrebbe nulla: meglio dirlo subito che
    // dopo aver caricato il modello.
    if duration_ms < 500 {
        let _ = std::fs::remove_dir_all(&directory);
        return Err("Il file audio è troppo breve o non contiene parlato.".into());
    }

    let model = models::ensure(&app, crate::settings::whisper_model(&app))?;

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
