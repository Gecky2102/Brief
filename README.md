# Brief

App desktop macOS per registrare, trascrivere e riassumere conversazioni **interamente in locale**. Nessun audio e nessun testo lascia il Mac.

## Come funziona

- **Due tracce separate** — microfono (tu) e audio di sistema (gli interlocutori), catturate via ScreenCaptureKit. Tenerle distinte dà la diarizzazione senza costi aggiuntivi.
- **Trascrizione realtime** — whisper.cpp con Metal e Neural Engine, VAD per tagliare sui silenzi.
- **Analisi a fine sessione** — llama.cpp propone il tipo di sessione e produce riassunto, action item, decisioni e domande aperte. Il modello STT viene scaricato dalla memoria prima di caricare l'LLM, così i due non convivono mai in RAM.
- **Archivio ricercabile** — SQLite con FTS5, in `~/Library/Application Support/Brief/`. Audio compresso in AAC ed esportabile, trascrizioni esportabili in Markdown.

## Requisiti

macOS 13+ su Apple Silicon · Node 20+ · Rust stable

## Sviluppo

```bash
npm install
npm run tauri dev
```
