use tauri_plugin_sql::{Migration, MigrationKind};

fn migrations() -> Vec<Migration> {
    vec![Migration {
        version: 1,
        description: "create sessions, segments and full-text index",
        sql: include_str!("../migrations/001_initial.sql"),
        kind: MigrationKind::Up,
    }]
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(
            tauri_plugin_sql::Builder::default()
                .add_migrations("sqlite:brief.db", migrations())
                .build(),
        )
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
