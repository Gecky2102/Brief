use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

use crate::models::{self, ModelSpec};
use crate::provider::Provider;

const KEYCHAIN_SERVICE: &str = "it.gmasiero.brief";
const KEYCHAIN_ACCOUNT: &str = "provider-api-key";

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

#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub quality: Quality,
    pub provider: Provider,
    pub model: String,
    /// Vuoto significa «usa l'indirizzo predefinito del fornitore».
    pub base_url: String,
}

impl Default for Settings {
    fn default() -> Self {
        let provider = Provider::default();
        Self {
            quality: Quality::default(),
            model: provider.default_model().to_string(),
            provider,
            base_url: String::new(),
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

/// La chiave sta nel portachiavi di sistema, non in `settings.json`: un file di
/// configurazione finisce nei backup e si legge in chiaro.
fn keyring_entry() -> Result<keyring::Entry, String> {
    keyring::Entry::new(KEYCHAIN_SERVICE, KEYCHAIN_ACCOUNT)
        .map_err(|cause| format!("Portachiavi non accessibile: {cause}"))
}

pub fn api_key() -> String {
    keyring_entry()
        .and_then(|entry| {
            entry
                .get_password()
                .map_err(|cause| format!("Chiave non leggibile: {cause}"))
        })
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
#[tauri::command]
pub fn has_api_key() -> bool {
    !api_key().is_empty()
}

#[tauri::command]
pub fn set_api_key(key: String) -> Result<(), String> {
    let entry = keyring_entry()?;
    if key.trim().is_empty() {
        let _ = entry.delete_credential();
        return Ok(());
    }
    entry
        .set_password(key.trim())
        .map_err(|cause| format!("Chiave non salvata: {cause}"))
}
