# Brief

App macOS che registra, trascrive e trasforma in documenti le tue conversazioni. La trascrizione avviene **sul tuo Mac**: l'audio non esce mai dal computer.

## Cosa fa

**Registra due tracce separate** — microfono e audio di sistema, catturate via ScreenCaptureKit senza driver da installare. Tenerle distinte permette di sapere già chi parla.

**Riconosce le voci** — un modello di impronta vocale raggruppa gli interventi per persona. Puoi dare un nome a ciascuna voce, unirne due quando la stessa persona è stata divisa, e regolare quanto il riconoscimento è sensibile.

**Trascrive in italiano mentre parli** — whisper.cpp con Metal, tagliando sulle pause. Ogni finestra riceve il contesto della precedente, così i nomi propri restano coerenti.

**Scrive un documento, non un riassunto** — da 600 a 6000 parole a seconda di quanto lo vuoi esteso, con otto tagli diversi: riunione, sintesi direzionale, verbale, lezione, intervista, punto di avanzamento, brainstorming, oppure riconosciuto automaticamente.

**Archivia e ritrova** — SQLite con ricerca full-text che mostra il punto in cui compare il termine. Esporti in Markdown, PDF o audio, singolarmente o in blocco.

**Riascolti mentre leggi** — clicchi il minutaggio di una riga e senti quel punto della registrazione.

## Dove finiscono i dati

Tutto in `~/Library/Application Support/it.gmasiero.brief/`: database, registrazioni in AAC, modelli scaricati e la chiave del servizio di analisi (in un file leggibile solo dal tuo utente).

L'unica cosa che esce dal Mac è il **testo** della trascrizione, inviato al servizio che scegli per scrivere il report: Anthropic, OpenAI, Google, OpenRouter o qualsiasi servizio compatibile con l'interfaccia di OpenAI.

## Modelli

Scaricati al primo uso e verificati contro l'hash SHA-256 pubblicato. I download interrotti riprendono da dove erano rimasti.

| | File | Peso |
|---|---|---|
| Trascrizione veloce | `ggml-small-q5_1.bin` | 190 MB |
| Trascrizione accurata | `ggml-large-v3-turbo-q5_0.bin` | 574 MB |
| Riconoscimento voci | `wespeaker-resnet34.onnx` | 27 MB |

## Scorciatoie

| | |
|---|---|
| Spazio | Avvia o ferma la registrazione |
| ⌘N | Nuova sessione |
| ⌘F | Cerca |
| ⌘R | Genera o rigenera il report |
| ⌘P | Esporta in PDF |
| ⌘⇧C | Copia negli appunti |
| ⌘, | Impostazioni |
| ? | Elenco delle scorciatoie |

## Requisiti

macOS 13+ su Apple Silicon. Al primo avvio l'app chiede **microfono** e **registrazione schermo** — quest'ultimo è il permesso che macOS richiede per catturare l'audio di sistema. Dopo averlo concesso l'app va riavviata: lo impone il sistema.

## Sviluppo

```bash
npm install
npm run tauri dev     # sviluppo
npm run tauri build   # genera Brief.app e il .dmg
cargo test            # test unitari
```

Serve la toolchain Rust, gli Xcode Command Line Tools e `cmake` per compilare whisper.cpp.
