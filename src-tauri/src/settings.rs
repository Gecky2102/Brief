use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

use crate::models::{self, ModelSpec};

/// «Veloce» tiene i modelli piccoli: parte subito e consuma poco.
/// «Accurata» scarica modelli più grandi: trascrive meglio il parlato
/// spontaneo e produce riassunti più concreti, al prezzo di ~5 GB e più tempo.
#[derive(Clone, Copy, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "lowercase")]
pub enum Quality {
    #[default]
    Fast,
    Accurate,
}

#[derive(Clone, Copy, Serialize, Deserialize, Default)]
pub struct Settings {
    #[serde(default)]
    pub quality: Quality,
}

fn settings_path(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|cause| format!("Cartella dati non disponibile: {cause}"))?;
    std::fs::create_dir_all(&dir)
        .map_err(|cause| format!("Cartella dati non creata: {cause}"))?;
    Ok(dir.join("settings.json"))
}

pub fn load(app: &AppHandle) -> Settings {
    settings_path(app)
        .ok()
        .and_then(|path| std::fs::read_to_string(path).ok())
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_default()
}

pub fn whisper_model(app: &AppHandle) -> &'static ModelSpec {
    match load(app).quality {
        Quality::Fast => &models::WHISPER,
        Quality::Accurate => &models::WHISPER_ACCURATE,
    }
}

pub fn llm_model(app: &AppHandle) -> &'static ModelSpec {
    match load(app).quality {
        Quality::Fast => &models::LLM,
        Quality::Accurate => &models::LLM_ACCURATE,
    }
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
