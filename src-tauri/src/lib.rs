mod analysis;
mod diarization;
mod audio;
mod export;
mod import;
mod models;
mod provider;
mod settings;
mod transcriber;

use tauri_plugin_sql::{Migration, MigrationKind};

fn migrations() -> Vec<Migration> {
    vec![
        Migration {
            version: 1,
            description: "create sessions, segments and full-text index",
            sql: include_str!("../migrations/001_initial.sql"),
            kind: MigrationKind::Up,
        },
        Migration {
            version: 2,
            description: "add speakers and link segments to them",
            sql: include_str!("../migrations/002_speakers.sql"),
            kind: MigrationKind::Up,
        },
    ]
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(
            tauri_plugin_sql::Builder::default()
                .add_migrations("sqlite:brief.db", migrations())
                .build(),
        )
        .setup(|app| {
            audio::remember_app_handle(app.handle().clone());

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            audio::start_recording,
            audio::stop_recording,
            audio::is_recording,
            audio::system_track_health,
            analysis::analyze_session,
            analysis::models_status,
            export::compress_recording,
            export::export_markdown,
            export::export_many,
            export::export_audio,
            export::delete_recording,
            export::audio_file,
            import::import_audio,
            settings::get_settings,
            settings::set_settings,
            settings::has_api_key,
            settings::set_api_key,
            settings::test_provider,
            models::storage_report,
            models::delete_model
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
