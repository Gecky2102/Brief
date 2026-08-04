# Contribuire

Grazie dell'interesse. Brief è un progetto personale: prima di scrivere codice,
apri una issue per discuterne — così evitiamo che tu spenda tempo su qualcosa
che non rientra nella direzione del progetto.

## Ambiente

```bash
npm install
npm run tauri dev
```

Serve la toolchain Rust, `cmake` per compilare whisper.cpp e, su macOS, gli
Xcode Command Line Tools.

## Prima di aprire una pull request

```bash
npx tsc --noEmit        # nessun errore di tipo
cd src-tauri
cargo build --release   # compila
cargo test              # test verdi
```

## Convenzioni

**Commit** secondo i [Conventional Commits](https://www.conventionalcommits.org/it/),
scritti in italiano: `feat(voci): …`, `fix(export): …`, `docs: …`.

**Codice** con nomi che si spiegano da soli. I commenti servono a dire *perché*
una scelta è stata fatta, non a ripetere cosa fa la riga sotto.

**Interfaccia** in italiano, senza gergo tecnico rivolto all'utente.

## Struttura

```
src/                  interfaccia React
src-tauri/src/        logica in Rust
  audio.rs            registrazione e mixer
  transcriber.rs      whisper e segmentazione
  diarization.rs      riconoscimento delle voci
  semantic.rs         ricerca per significato
  analysis.rs         generazione dei documenti
  provider.rs         servizi di intelligenza artificiale
src-tauri/swift/      cattura audio su macOS
```
