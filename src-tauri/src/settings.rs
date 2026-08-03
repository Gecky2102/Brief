use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

use crate::models::{self, ModelSpec};
use crate::provider::Provider;

/// «Veloce» tiene il modello di trascrizione piccolo: parte subito e consuma
/// poco. «Accurata» ne scarica uno grande: regge molto meglio parlato
/// spontaneo, dialetti e più voci sovrapposte.
#[derive(Clone, Copy, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "lowercase")]
pub enum Quality {
    #[default]
    Fast,
    Accurate,
}

/// Quanto è facile che due interventi vengano attribuiti alla stessa persona.
/// Più alta la soglia, più voci distinte vengono create.
#[derive(Clone, Copy, Serialize, Deserialize, PartialEq, Default, Debug)]
#[serde(rename_all = "lowercase")]
pub enum VoiceSensitivity {
    /// Poche voci, tende a fondere persone con timbro simile.
    Low,
    #[default]
    Medium,
    /// Molte voci, tende a spezzare la stessa persona in più gruppi.
    High,
}

impl VoiceSensitivity {
    pub fn threshold(self) -> f32 {
        match self {
            VoiceSensitivity::Low => 0.52,
            VoiceSensitivity::Medium => 0.62,
            VoiceSensitivity::High => 0.72,
        }
    }
}

/// Taglio del report. `Auto` lascia che sia il modello a riconoscere di che
/// tipo di conversazione si tratta e ad adattare il documento.
#[derive(Clone, Copy, Serialize, Deserialize, PartialEq, Default, Debug)]
#[serde(rename_all = "lowercase")]
pub enum ReportStyle {
    #[default]
    Auto,
    Meeting,
    Executive,
    Lecture,
    Interview,
    Standup,
    Brainstorm,
    Minutes,
}

/// Quanto deve essere lungo il documento.
#[derive(Clone, Copy, Serialize, Deserialize, PartialEq, Default, Debug)]
#[serde(rename_all = "lowercase")]
pub enum ReportLength {
    Brief,
    #[default]
    Standard,
    Deep,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub quality: Quality,
    pub provider: Provider,
    pub model: String,
    /// Vuoto significa «usa l'indirizzo predefinito del fornitore».
    pub base_url: String,
    pub report_style: ReportStyle,
    pub report_length: ReportLength,
    /// Istruzioni aggiuntive dell'utente, aggiunte in coda al prompt.
    pub report_notes: String,
    /// Nomi propri, termini aziendali e sigle che Whisper sbaglia: passati come
    /// suggerimento, riducono di molto le storpiature che poi finiscono nel report.
    pub vocabulary: String,
    pub voice_sensitivity: VoiceSensitivity,
    /// Zero significa «nessun limite noto»: il riconoscimento decide da solo.
    pub expected_speakers: u32,
}

impl Default for Settings {
    fn default() -> Self {
        let provider = Provider::default();
        Self {
            quality: Quality::default(),
            model: provider.default_model().to_string(),
            provider,
            base_url: String::new(),
            report_style: ReportStyle::default(),
            report_length: ReportLength::default(),
            report_notes: String::new(),
            vocabulary: String::new(),
            voice_sensitivity: VoiceSensitivity::default(),
            expected_speakers: 0,
        }
    }
}

fn settings_path(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|cause| format!("Cartella dati non disponibile: {cause}"))?;
    std::fs::create_dir_all(&dir).map_err(|cause| format!("Cartella dati non creata: {cause}"))?;
    Ok(dir.join("settings.json"))
}

pub fn load(app: &AppHandle) -> Settings {
    settings_path(app)
        .ok()
        .and_then(|path| std::fs::read_to_string(path).ok())
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_default()
}

pub fn whisper_model_for(quality: Quality) -> &'static ModelSpec {
    match quality {
        Quality::Fast => &models::WHISPER,
        Quality::Accurate => &models::WHISPER_ACCURATE,
    }
}

pub fn whisper_model(app: &AppHandle) -> &'static ModelSpec {
    whisper_model_for(load(app).quality)
}

/// La chiave sta in un file separato dalle impostazioni, leggibile solo
/// dall'utente. Il portachiavi sarebbe più solido, ma con una firma ad-hoc —
/// che cambia a ogni build — macOS chiederebbe l'autorizzazione ogni volta.
fn key_path(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|cause| format!("Cartella dati non disponibile: {cause}"))?;
    std::fs::create_dir_all(&dir).map_err(|cause| format!("Cartella dati non creata: {cause}"))?;
    Ok(dir.join("provider.key"))
}

pub fn api_key(app: &AppHandle) -> String {
    key_path(app)
        .ok()
        .and_then(|path| std::fs::read_to_string(path).ok())
        .map(|raw| raw.trim().to_string())
        .unwrap_or_default()
}

#[tauri::command]
pub fn get_settings(app: AppHandle) -> Settings {
    load(&app)
}

#[tauri::command]
pub fn set_settings(app: AppHandle, settings: Settings) -> Result<(), String> {
    let path = settings_path(&app)?;
    let raw = serde_json::to_string_pretty(&settings)
        .map_err(|cause| format!("Impostazioni non salvate: {cause}"))?;
    std::fs::write(path, raw).map_err(|cause| format!("Impostazioni non salvate: {cause}"))
}

/// Riporta solo se una chiave esiste: il valore non torna mai all'interfaccia.
/// Manda una richiesta minima al fornitore per dire subito se chiave, modello
/// e indirizzo funzionano, invece di scoprirlo a fine trascrizione.
#[tauri::command]
pub async fn test_provider(app: AppHandle) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let impostazioni = load(&app);
        let chiave = api_key(&app);

        let mut risposta = String::new();
        crate::provider::stream(
            crate::provider::Request {
                provider: impostazioni.provider,
                base_url: &impostazioni.base_url,
                api_key: &chiave,
                model: &impostazioni.model,
                system: "Rispondi con una sola parola: pronto",
                user: "Sei operativo?",
                max_tokens: 20,
                prefill: None,
            },
            |delta| risposta.push_str(delta),
        )?;

        Ok(format!(
            "{} risponde con «{}»",
            impostazioni.provider.label(),
            risposta.trim()
        ))
    })
    .await
    .map_err(|cause| format!("Prova interrotta: {cause}"))?
}

#[tauri::command]
pub fn has_api_key(app: AppHandle) -> bool {
    !api_key(&app).is_empty()
}

#[tauri::command]
pub fn set_api_key(app: AppHandle, key: String) -> Result<(), String> {
    let path = key_path(&app)?;

    if key.trim().is_empty() {
        let _ = std::fs::remove_file(&path);
        return Ok(());
    }

    std::fs::write(&path, key.trim())
        .map_err(|cause| format!("Chiave non salvata: {cause}"))?;

    // Solo il proprietario può leggerla: senza questo il file nascerebbe
    // leggibile da chiunque abbia accesso al Mac.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
    }

    Ok(())
}
