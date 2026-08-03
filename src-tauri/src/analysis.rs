use std::num::NonZeroU32;

use llama_cpp_2::context::params::LlamaContextParams;
use llama_cpp_2::llama_backend::LlamaBackend;
use llama_cpp_2::llama_batch::LlamaBatch;
use llama_cpp_2::model::params::LlamaModelParams;
use llama_cpp_2::model::{AddBos, LlamaModel};
use llama_cpp_2::sampling::LlamaSampler;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter};

use crate::models;

const CONTEXT_TOKENS: u32 = 8192;
const BATCH_TOKENS: usize = 512;
/// Oltre questa lunghezza la trascrizione viene riassunta a blocchi invece che
/// troncata: su una riunione di un'ora il troncamento buttava via la parte
/// centrale, ed è lì che di solito stanno le decisioni.
const CHUNK_CHARS: usize = 9_000;
const MAX_OUTPUT_TOKENS: i32 = 900;
/// Il modello ha una finestra finita: oltre questa soglia la trascrizione viene
/// accorciata tenendo l'inizio e la fine, dove di solito stanno inquadramento e
/// conclusioni.
const MAX_TRANSCRIPT_CHARS: usize = 14_000;

#[derive(Deserialize)]
pub struct TranscriptLine {
    pub speaker: String,
    pub text: String,
}

#[derive(Serialize, Deserialize, Default)]
pub struct Analysis {
    pub kind: String,
    pub title: String,
    pub summary: String,
    pub decisions: Vec<String>,
    pub actions: Vec<String>,
    pub questions: Vec<String>,
}

pub fn is_model_ready(app: &AppHandle) -> bool {
    models::path_if_present(app, crate::settings::llm_model(app)).is_some()
}

fn shorten(transcript: &str) -> String {
    if transcript.chars().count() <= MAX_TRANSCRIPT_CHARS {
        return transcript.to_string();
    }
    let characters: Vec<char> = transcript.chars().collect();
    let half = MAX_TRANSCRIPT_CHARS / 2;
    let head: String = characters[..half].iter().collect();
    let tail: String = characters[characters.len() - half..].iter().collect();
    format!("{head}\n\n[…parte centrale omessa…]\n\n{tail}")
}

fn build_prompt(transcript: &str) -> String {
    // Niente JSON: un modello da 3 miliardi di parametri sbaglia spesso le
    // parentesi annidate. Righe etichettate sono molto più difficili da
    // sbagliare e si analizzano con altrettanta precisione.
    let system = "Sei un assistente che analizza trascrizioni di riunioni in italiano. \
Rispondi SOLO con righe etichettate, una per riga, in questo formato esatto:

TIPO: <work_call|meeting|lecture|interview|casual>
TITOLO: <massimo 8 parole>
RIASSUNTO: <3-6 frasi su una sola riga>
DECISIONE: <una decisione presa>
AZIONE: <una cosa da fare, all'infinito, con il responsabile fra parentesi se emerge>
DOMANDA: <una domanda rimasta aperta>

Ripeti le righe DECISIONE, AZIONE e DOMANDA una volta per ciascuna voce, \
al massimo 6 per tipo, tutte diverse fra loro. Ometti la riga se non hai nulla da dire. \
Conserva i nomi propri di persone, aziende e strumenti così come compaiono. \
La trascrizione è automatica e contiene errori: ignora le parole incomprensibili \
invece di inventarci sopra. Non aggiungere altro testo oltre alle righe etichettate.

IMPORTANTE: scrivi OGNI parola in italiano. Non usare mai spagnolo, inglese, portoghese \
o altre lingue. Se una frase ti viene in un'altra lingua, riscrivila in italiano.";

    format!(
        "<|im_start|>system\n{system}<|im_end|>\n\
         <|im_start|>user\nTrascrizione:\n\n{}\n<|im_end|>\n\
         <|im_start|>assistant\n",
        shorten(transcript)
    )
}

/// Legge le righe etichettate prodotte dal modello. Le righe che non
/// riconosce vengono ignorate: il modello ogni tanto aggiunge commenti.
fn parse_labelled(raw: &str) -> Analysis {
    let mut analysis = Analysis {
        kind: "unknown".into(),
        ..Default::default()
    };

    for line in raw.lines() {
        let line = line.trim().trim_start_matches(['-', '*', ' ']);
        let Some((label, value)) = line.split_once(':') else {
            continue;
        };
        let value = value.trim().trim_matches('"').to_string();
        if value.is_empty() {
            continue;
        }

        match label.trim().to_uppercase().as_str() {
            "TIPO" => analysis.kind = value.to_lowercase(),
            "TITOLO" => analysis.title = value,
            "RIASSUNTO" => analysis.summary = value,
            "DECISIONE" => push_unique(&mut analysis.decisions, value),
            "AZIONE" => push_unique(&mut analysis.actions, value),
            "DOMANDA" => push_unique(&mut analysis.questions, value),
            _ => {}
        }
    }

    analysis
}

fn push_unique(list: &mut Vec<String>, value: String) {
    let normalized = value.to_lowercase();
    if list.len() < 6 && !list.iter().any(|v| v.to_lowercase() == normalized) {
        list.push(value);
    }
}

/// Ricuce un JSON interrotto a metà: taglia la voce incompleta e richiude le
/// parentesi rimaste aperte. Meglio un'analisi con una voce in meno che un
/// errore secco dopo minuti di elaborazione.
fn repair_json(raw: &str) -> Option<String> {
    let start = raw.find('{')?;
    let body = &raw[start..];

    // Ultimo punto sicuro: la fine di una stringa seguita da virgola.
    let cut = body.rfind("\",")?;
    let mut fixed = body[..cut + 1].to_string();

    let apre_graffe = fixed.matches('{').count();
    let chiude_graffe = fixed.matches('}').count();
    let apre_quadre = fixed.matches('[').count();
    let chiude_quadre = fixed.matches(']').count();

    for _ in 0..apre_quadre.saturating_sub(chiude_quadre) {
        fixed.push(']');
    }
    for _ in 0..apre_graffe.saturating_sub(chiude_graffe) {
        fixed.push('}');
    }

    serde_json::from_str::<serde_json::Value>(&fixed)
        .ok()
        .map(|_| fixed)
}

/// Il modello, per quanto istruito, ogni tanto incornicia il JSON in un blocco
/// di codice o aggiunge una frase: si prende l'oggetto bilanciato più esterno.
fn extract_json(raw: &str) -> Option<String> {
    let bytes: Vec<char> = raw.chars().collect();
    let start = bytes.iter().position(|c| *c == '{')?;
    let mut depth = 0_i32;
    let mut in_string = false;
    let mut escaped = false;

    for index in start..bytes.len() {
        let character = bytes[index];
        if in_string {
            if escaped {
                escaped = false;
            } else if character == '\\' {
                escaped = true;
            } else if character == '"' {
                in_string = false;
            }
            continue;
        }
        match character {
            '"' => in_string = true,
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(bytes[start..=index].iter().collect());
                }
            }
            _ => {}
        }
    }
    repair_json(raw)
}

/// Modello caricato una volta e riusato per ogni blocco: ricaricarlo a ogni
/// chiamata costerebbe secondi e gigabyte di traffico in memoria.
struct Engine {
    backend: LlamaBackend,
    model: LlamaModel,
}

impl Engine {
    fn new(model_path: &std::path::Path) -> Result<Self, String> {
        let backend = LlamaBackend::init()
            .map_err(|cause| format!("Motore di analisi non inizializzato: {cause}"))?;

        // n_gpu_layers alto: su Apple Silicon il modello gira su GPU via Metal.
        let model_params = LlamaModelParams::default().with_n_gpu_layers(999);
        let model = LlamaModel::load_from_file(&backend, model_path, &model_params)
            .map_err(|cause| format!("Modello di analisi non caricato: {cause}"))?;

        Ok(Self { backend, model })
    }
}

fn generate_with(engine: &Engine, prompt: &str, max_tokens: i32) -> Result<String, String> {
    let backend = &engine.backend;
    let model = &engine.model;

    let context_params = LlamaContextParams::default()
        .with_n_ctx(NonZeroU32::new(CONTEXT_TOKENS))
        .with_n_batch(BATCH_TOKENS as u32);
    let mut context = model
        .new_context(&backend, context_params)
        .map_err(|cause| format!("Contesto di analisi non creato: {cause}"))?;

    let tokens = model
        .str_to_token(prompt, AddBos::Always)
        .map_err(|cause| format!("Prompt non tokenizzato: {cause}"))?;

    if tokens.len() as u32 >= CONTEXT_TOKENS {
        return Err("La trascrizione è troppo lunga per il modello di analisi.".into());
    }

    // Il batch ha una capienza fissa: un prompt più lungo va consegnato a
    // blocchi, altrimenti llama.cpp rifiuta con "Insufficient Space".
    let mut batch = LlamaBatch::new(BATCH_TOKENS, 1);
    for (index, chunk) in tokens.chunks(BATCH_TOKENS).enumerate() {
        batch.clear();
        let base = index * BATCH_TOKENS;
        let is_final_chunk = base + chunk.len() == tokens.len();

        for (offset, token) in chunk.iter().enumerate() {
            // Solo l'ultimo token dell'ultimo blocco produce i logit da cui
            // parte la generazione.
            let wants_logits = is_final_chunk && offset == chunk.len() - 1;
            batch
                .add(*token, (base + offset) as i32, &[0], wants_logits)
                .map_err(|cause| format!("Analisi fallita: {cause}"))?;
        }

        context
            .decode(&mut batch)
            .map_err(|cause| format!("Analisi fallita: {cause}"))?;
    }

    // Senza penalità il modello si impunta e ripete la stessa voce finché
    // esaurisce i token, lasciando il JSON tronco.
    let mut sampler = LlamaSampler::chain_simple([
        LlamaSampler::penalties(256, 1.15, 0.4, 0.4),
        LlamaSampler::temp(0.3),
        LlamaSampler::top_p(0.9, 1),
        LlamaSampler::dist(1234),
    ]);

    let mut output = String::new();
    let mut position = tokens.len() as i32;
    let mut decoder = encoding_rs::UTF_8.new_decoder();

    for _ in 0..max_tokens {
        let token = sampler.sample(&context, -1);
        sampler.accept(token);

        if model.is_eog_token(token) {
            break;
        }

        let piece = model
            .token_to_piece(token, &mut decoder, false, None)
            .unwrap_or_default();
        output.push_str(&piece);

        batch.clear();
        batch
            .add(token, position, &[0], true)
            .map_err(|cause| format!("Analisi fallita: {cause}"))?;
        position += 1;

        context
            .decode(&mut batch)
            .map_err(|cause| format!("Analisi fallita: {cause}"))?;
    }

    Ok(output)
}

#[cfg(test)]
fn generate(model_path: &std::path::Path, prompt: &str) -> Result<String, String> {
    let engine = Engine::new(model_path)?;
    generate_with(&engine, prompt, MAX_OUTPUT_TOKENS)
}

/// Riassume un pezzo di trascrizione in punti, senza JSON: serve come materiale
/// per la sintesi finale.
fn summarize_chunk(engine: &Engine, chunk: &str, indice: usize, totale: usize) -> Result<String, String> {
    let prompt = format!(
        "<|im_start|>system\nSei un assistente che riassume trascrizioni di riunioni in italiano. \
Elenca in punti sintetici ciò che viene detto in questo estratto: argomenti trattati, \
decisioni prese, cose da fare, domande rimaste aperte. \
Conserva i nomi propri di persone, aziende, strumenti e prodotti così come compaiono. \
La trascrizione è automatica e contiene errori: ignora le parole incomprensibili invece di inventarle. \
Rispondi solo con l'elenco puntato, scritto interamente in italiano: \
non usare mai spagnolo, inglese o altre lingue.<|im_end|>\n\
<|im_start|>user\nEstratto {} di {}:\n\n{chunk}<|im_end|>\n\
<|im_start|>assistant\n",
        indice + 1,
        totale
    );
    generate_with(engine, &prompt, 400)
}

fn render_transcript(lines: &[TranscriptLine]) -> String {
    lines
        .iter()
        .map(|line| format!("{}: {}", line.speaker, line.text.trim()))
        .collect::<Vec<_>>()
        .join("\n")
}

#[tauri::command]
pub async fn analyze_session(
    app: AppHandle,
    lines: Vec<TranscriptLine>,
) -> Result<Analysis, String> {
    tauri::async_runtime::spawn_blocking(move || analyze_blocking(app, lines))
        .await
        .map_err(|cause| format!("Analisi interrotta: {cause}"))?
}

fn analyze_blocking(app: AppHandle, lines: Vec<TranscriptLine>) -> Result<Analysis, String> {
    if lines.is_empty() {
        return Err("Non c'è nulla da analizzare: la trascrizione è vuota.".into());
    }

    // Se una registrazione fosse ancora in corso avremmo Whisper e l'LLM in RAM
    // insieme: su 16 GB è proprio il caso da evitare.
    crate::transcriber::stop();

    let path = models::ensure(&app, crate::settings::llm_model(&app))?;
    let transcript = render_transcript(&lines);
    let engine = Engine::new(&path)?;

    let raw = if transcript.chars().count() <= MAX_TRANSCRIPT_CHARS {
        generate_with(&engine, &build_prompt(&transcript), MAX_OUTPUT_TOKENS)?
    } else {
        // Riassume ogni blocco, poi costruisce la sintesi finale sui riassunti:
        // così nessuna parte della riunione resta fuori.
        let characters: Vec<char> = transcript.chars().collect();
        let blocchi: Vec<String> = characters
            .chunks(CHUNK_CHARS)
            .map(|blocco| blocco.iter().collect())
            .collect();
        let totale = blocchi.len();

        let mut parziali = String::new();
        for (indice, blocco) in blocchi.iter().enumerate() {
            let _ = app.emit(
                "analysis://progress",
                AnalysisProgress {
                    done: indice,
                    total: totale + 1,
                },
            );
            parziali.push_str(&summarize_chunk(&engine, blocco, indice, totale)?);
            parziali.push('\n');
        }

        let _ = app.emit(
            "analysis://progress",
            AnalysisProgress {
                done: totale,
                total: totale + 1,
            },
        );
        generate_with(&engine, &build_prompt(&parziali), MAX_OUTPUT_TOKENS)?
    };

    let mut analysis = parse_labelled(&raw);
    if analysis.summary.trim().is_empty() {
        return Err("Il modello non ha prodotto un risultato leggibile.".into());
    }

    const KINDS: [&str; 5] = ["work_call", "meeting", "lecture", "interview", "casual"];
    if !KINDS.contains(&analysis.kind.as_str()) {
        analysis.kind = "unknown".into();
    }

    Ok(analysis)
}

#[tauri::command]
pub fn models_status(app: AppHandle) -> ModelsStatus {
    ModelsStatus {
        transcription: crate::transcriber::is_model_ready(&app),
        analysis: is_model_ready(&app),
    }
}

#[derive(Clone, Serialize)]
pub struct AnalysisProgress {
    done: usize,
    total: usize,
}

#[derive(Serialize)]
pub struct ModelsStatus {
    transcription: bool,
    analysis: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn estrae_il_json_da_una_risposta_incorniciata() {
        let raw = "Ecco il risultato:\n```json\n{\"kind\":\"meeting\",\"title\":\"Test\"}\n```\nSpero vada bene.";
        assert_eq!(
            extract_json(raw).unwrap(),
            "{\"kind\":\"meeting\",\"title\":\"Test\"}"
        );
    }

    #[test]
    fn regge_le_graffe_dentro_le_stringhe() {
        let raw = r#"{"summary":"ha detto } e poi {","actions":[]}"#;
        assert_eq!(extract_json(raw).unwrap(), raw);
    }

    #[test]
    fn segnala_quando_non_ce_json() {
        assert!(extract_json("nessun oggetto qui").is_none());
        assert!(extract_json("{ mai chiuso").is_none());
    }

    #[test]
    fn accorcia_le_trascrizioni_troppo_lunghe() {
        let lunga = "a".repeat(MAX_TRANSCRIPT_CHARS * 2);
        let corta = shorten(&lunga);
        assert!(corta.chars().count() < lunga.chars().count());
        assert!(corta.contains("parte centrale omessa"));

        let breve = "ciao";
        assert_eq!(shorten(breve), breve);
    }
}

/// Test end-to-end dell'analisi. Richiede il modello scaricato e verifica anche
/// che whisper.cpp e llama.cpp convivano nello stesso binario nonostante i
/// simboli ggml duplicati segnalati dal linker.
/// `BRIEF_TEST_LLM=... cargo test -- --ignored`
#[cfg(test)]
mod integration {
    use super::*;

    /// Analizza una trascrizione reale letta da file, per giudicare la qualità
    /// del riassunto su una riunione vera invece che su un esempio costruito.
    /// `BRIEF_TEST_TRANSCRIPT=... BRIEF_TEST_LLM=... cargo test -- --ignored reale`
    #[test]
    #[ignore]
    fn analizza_trascrizione_reale() {
        let model = std::env::var("BRIEF_TEST_LLM").expect("BRIEF_TEST_LLM");
        let path = std::env::var("BRIEF_TEST_TRANSCRIPT").expect("BRIEF_TEST_TRANSCRIPT");
        let transcript = std::fs::read_to_string(&path).expect("trascrizione leggibile");

        let engine = Engine::new(std::path::Path::new(&model)).expect("motore");

        // Stesso percorso a blocchi del comando reale.
        let raw = if transcript.chars().count() <= MAX_TRANSCRIPT_CHARS {
            generate_with(&engine, &build_prompt(&transcript), MAX_OUTPUT_TOKENS).unwrap()
        } else {
            let characters: Vec<char> = transcript.chars().collect();
            let blocchi: Vec<String> = characters
                .chunks(CHUNK_CHARS)
                .map(|b| b.iter().collect())
                .collect();
            let totale = blocchi.len();
            println!("=== BLOCCHI: {totale}");
            let mut parziali = String::new();
            for (indice, blocco) in blocchi.iter().enumerate() {
                parziali.push_str(&summarize_chunk(&engine, blocco, indice, totale).unwrap());
                parziali.push('\n');
            }
            generate_with(&engine, &build_prompt(&parziali), MAX_OUTPUT_TOKENS).unwrap()
        };

        let analysis = parse_labelled(&raw);

        println!("=== TIPO: {}", analysis.kind);
        println!("=== TITOLO: {}", analysis.title);
        println!("=== RIASSUNTO: {}", analysis.summary);
        println!("=== DECISIONI:");
        for voce in &analysis.decisions {
            println!("  - {voce}");
        }
        println!("=== DA FARE:");
        for voce in &analysis.actions {
            println!("  - {voce}");
        }
        println!("=== DOMANDE APERTE:");
        for voce in &analysis.questions {
            println!("  - {voce}");
        }
        assert!(!analysis.summary.trim().is_empty());
    }

    /// Prompt volutamente oltre i 512 token del batch: è il caso che faceva
    /// fallire l'analisi con «Insufficient Space».
    #[test]
    #[ignore]
    fn analizza_una_conversazione_lunga() {
        let model = std::env::var("BRIEF_TEST_LLM").expect("BRIEF_TEST_LLM");

        let mut transcript = String::new();
        for turno in 1..=40 {
            transcript.push_str(&format!(
                "Io: Punto numero {turno}, dobbiamo rivedere le stime di consegna del modulo di fatturazione e capire se il fornitore riesce a rispettare i tempi concordati.\nInterlocutore: Sono d'accordo, però prima serve la conferma scritta dal reparto acquisti, altrimenti rischiamo di bloccare tutto un'altra volta.\n"
            ));
        }

        let prompt = build_prompt(&transcript);
        let raw = generate(std::path::Path::new(&model), &prompt).expect("generazione");
        let json = extract_json(&raw).expect("json presente");
        let analysis: Analysis = serde_json::from_str(&json).expect("json valido");
        println!("LUNGO -> TITOLO: {} | AZIONI: {}", analysis.title, analysis.actions.len());
        assert!(!analysis.summary.trim().is_empty());
    }

    #[test]
    #[ignore]
    fn analizza_una_conversazione() {
        let model = std::env::var("BRIEF_TEST_LLM").expect("BRIEF_TEST_LLM");

        let transcript = "Io: Allora, per il preventivo del cliente Rossi, direi di chiudere a quattromila euro.\n\
Interlocutore: D'accordo, ma serve la conferma di Marco prima di mandarlo.\n\
Io: Va bene, lo sento domani mattina e poi lo spedisco entro mercoledì.\n\
Interlocutore: Perfetto. Resta da capire se includiamo anche la manutenzione annuale.\n\
Io: Quello lo decidiamo dopo aver parlato con Marco.";

        let prompt = build_prompt(transcript);
        let raw = generate(std::path::Path::new(&model), &prompt).expect("generazione");
        println!("GREZZO: {raw}");

        let json = extract_json(&raw).expect("json presente");
        let analysis: Analysis = serde_json::from_str(&json).expect("json valido");

        println!(
            "TIPO: {} | TITOLO: {} | AZIONI: {:?} | DOMANDE: {:?}",
            analysis.kind, analysis.title, analysis.actions, analysis.questions
        );

        assert!(!analysis.summary.trim().is_empty(), "riassunto vuoto");
        assert!(
            !analysis.actions.is_empty() || !analysis.decisions.is_empty(),
            "né decisioni né cose da fare estratte"
        );
    }
}

#[cfg(test)]
mod parsing {
    use super::*;

    #[test]
    fn legge_le_righe_etichettate() {
        let raw = "TIPO: meeting\n\
TITOLO: Riunione gestionale\n\
RIASSUNTO: Si è parlato del nuovo gestionale.\n\
DECISIONE: Sostituire OneNote\n\
AZIONE: Preparare le query SQL (io)\n\
DOMANDA: Quale database usare?\n\
Nota finale ignorata";

        let analysis = parse_labelled(raw);
        assert_eq!(analysis.kind, "meeting");
        assert_eq!(analysis.title, "Riunione gestionale");
        assert_eq!(analysis.decisions, vec!["Sostituire OneNote"]);
        assert_eq!(analysis.actions, vec!["Preparare le query SQL (io)"]);
        assert_eq!(analysis.questions, vec!["Quale database usare?"]);
    }

    #[test]
    fn scarta_i_doppioni_e_limita_le_voci() {
        let mut raw = String::from("RIASSUNTO: prova\n");
        for _ in 0..10 {
            raw.push_str("AZIONE: Sentire Marco\n");
        }
        for indice in 0..10 {
            raw.push_str(&format!("DECISIONE: Decisione {indice}\n"));
        }

        let analysis = parse_labelled(&raw);
        assert_eq!(analysis.actions.len(), 1, "i doppioni vanno scartati");
        assert_eq!(analysis.decisions.len(), 6, "al massimo sei voci");
    }

    #[test]
    fn tollera_elenchi_puntati() {
        let analysis = parse_labelled("- AZIONE: Inviare il preventivo\n* DECISIONE: Chiudere a 4000");
        assert_eq!(analysis.actions, vec!["Inviare il preventivo"]);
        assert_eq!(analysis.decisions, vec!["Chiudere a 4000"]);
    }
}
