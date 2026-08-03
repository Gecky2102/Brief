# Brief

App desktop macOS per registrare, trascrivere e riassumere conversazioni **interamente in locale**. Nessun audio e nessun testo lascia il Mac.

## Come funziona

- **Due tracce separate** — microfono (tu) e audio di sistema (gli interlocutori), catturate via ScreenCaptureKit. Tenerle distinte dà la diarizzazione senza costi aggiuntivi: ogni riga sa già chi parla.
- **Trascrizione realtime** — whisper.cpp con Metal. L'audio viene tagliato sulle pause: un segmento si chiude dopo ~600 ms di silenzio, o comunque entro 15 secondi.
- **Analisi a fine sessione** — Qwen2.5 3B via llama.cpp deduce il tipo di conversazione e produce riassunto, decisioni, cose da fare e domande aperte. Gira dopo lo stop, mai insieme a Whisper: i due modelli non convivono in memoria.
- **Archivio ricercabile** — SQLite con FTS5. Ricerca full-text su tutte le trascrizioni, export in Markdown, audio compresso in AAC ed esportabile.

## Modelli

Non sono nel bundle. Vengono scaricati al primo uso in `~/Library/Application Support/it.gmasiero.brief/models/` e **verificati contro l'hash SHA-256 pubblicato**: un download troncato o manomesso viene scartato.

| | File | Peso |
|---|---|---|
| Trascrizione | `ggml-small-q5_1.bin` | 190 MB |
| Analisi | `qwen2.5-3b-instruct-q4_k_m.gguf` | 2,1 GB |

## Stato

Trascrizione, cattura audio e analisi sono state verificate end-to-end. I test unitari girano con `cargo test`; quelli che richiedono i modelli scaricati sono marcati `#[ignore]`:

```bash
cargo test                                    # 7 test unitari
BRIEF_TEST_WAV=… BRIEF_TEST_MODEL=… \
BRIEF_TEST_LLM=… cargo test -- --ignored      # trascrizione e analisi reali
```

## Requisiti

macOS 13+ su Apple Silicon. Al primo avvio l'app chiede **microfono** e **registrazione schermo** (quest'ultimo è il permesso che macOS richiede per catturare l'audio di sistema). Dopo aver concesso la registrazione schermo l'app va riavviata: lo impone il sistema.

## Sviluppo

```bash
npm install
npm run tauri dev     # sviluppo
npm run tauri build   # genera Brief.app e il .dmg
```

Serve la toolchain Rust, gli Xcode Command Line Tools e `cmake` (`brew install cmake`) per compilare whisper.cpp e llama.cpp.

## Dati

Tutto sotto `~/Library/Application Support/it.gmasiero.brief/`:

```
brief.db                  sessioni, trascrizioni, analisi
recordings/<timestamp>/   mic.m4a, system.m4a
models/                   modelli scaricati
```
