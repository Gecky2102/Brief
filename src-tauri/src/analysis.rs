use std::num::NonZeroU32;

use llama_cpp_2::context::params::LlamaContextParams;
use llama_cpp_2::llama_backend::LlamaBackend;
use llama_cpp_2::llama_batch::LlamaBatch;
use llama_cpp_2::model::params::LlamaModelParams;
use llama_cpp_2::model::{AddBos, LlamaModel};
use llama_cpp_2::sampling::LlamaSampler;
use serde::{Deserialize, Serialize};
use tauri::AppHandle;

use crate::models;

const CONTEXT_TOKENS: u32 = 8192;
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
    models::path_if_present(app, &models::LLM).is_some()
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
    let system = "Sei un assistente che analizza trascrizioni di conversazioni in italiano. \
Rispondi SEMPRE ed esclusivamente con un oggetto JSON valido, senza testo prima o dopo, \
senza blocchi di codice. Lo schema è:
{\"kind\": \"work_call|meeting|lecture|interview|casual\", \"title\": \"titolo breve\", \
\"summary\": \"riassunto in 3-6 frasi\", \"decisions\": [\"...\"], \"actions\": [\"...\"], \
\"questions\": [\"...\"]}
Regole: `kind` è il tipo di conversazione che deduci. `title` massimo 8 parole. \
`decisions` sono le decisioni prese, `actions` le cose da fare (con il responsabile se emerge), \
`questions` le domande rimaste aperte. Se una lista è vuota lascia []. \
Non inventare nulla che non sia nella trascrizione. Scrivi in italiano.";

    format!(
        "<|im_start|>system\n{system}<|im_end|>\n\
         <|im_start|>user\nTrascrizione:\n\n{}\n<|im_end|>\n\
         <|im_start|>assistant\n",
        shorten(transcript)
    )
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
    None
}

fn generate(model_path: &std::path::Path, prompt: &str) -> Result<String, String> {
    let backend = LlamaBackend::init()
        .map_err(|cause| format!("Motore di analisi non inizializzato: {cause}"))?;

    // n_gpu_layers alto: su Apple Silicon il modello gira su GPU via Metal.
    let model_params = LlamaModelParams::default().with_n_gpu_layers(999);
    let model = LlamaModel::load_from_file(&backend, model_path, &model_params)
        .map_err(|cause| format!("Modello di analisi non caricato: {cause}"))?;

    let context_params = LlamaContextParams::default()
        .with_n_ctx(NonZeroU32::new(CONTEXT_TOKENS))
        .with_n_batch(512);
    let mut context = model
        .new_context(&backend, context_params)
        .map_err(|cause| format!("Contesto di analisi non creato: {cause}"))?;

    let tokens = model
        .str_to_token(prompt, AddBos::Always)
        .map_err(|cause| format!("Prompt non tokenizzato: {cause}"))?;

    if tokens.len() as u32 >= CONTEXT_TOKENS {
        return Err("La trascrizione è troppo lunga per il modello di analisi.".into());
    }

    let mut batch = LlamaBatch::new(512, 1);
    let last = tokens.len() - 1;
    for (position, token) in tokens.iter().enumerate() {
        batch
            .add(*token, position as i32, &[0], position == last)
            .map_err(|cause| format!("Analisi fallita: {cause}"))?;
    }
    context
        .decode(&mut batch)
        .map_err(|cause| format!("Analisi fallita: {cause}"))?;

    let mut sampler = LlamaSampler::chain_simple([
        LlamaSampler::temp(0.3),
        LlamaSampler::top_p(0.9, 1),
        LlamaSampler::dist(1234),
    ]);

    let mut output = String::new();
    let mut position = batch.n_tokens();
    let mut decoder = encoding_rs::UTF_8.new_decoder();

    for _ in 0..MAX_OUTPUT_TOKENS {
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

fn render_transcript(lines: &[TranscriptLine]) -> String {
    lines
        .iter()
        .map(|line| format!("{}: {}", line.speaker, line.text.trim()))
        .collect::<Vec<_>>()
        .join("\n")
}

#[tauri::command]
pub fn analyze_session(app: AppHandle, lines: Vec<TranscriptLine>) -> Result<Analysis, String> {
    if lines.is_empty() {
        return Err("Non c'è nulla da analizzare: la trascrizione è vuota.".into());
    }

    // Se una registrazione fosse ancora in corso avremmo Whisper e l'LLM in RAM
    // insieme: su 16 GB è proprio il caso da evitare.
    crate::transcriber::stop();

    let path = models::ensure(&app, &models::LLM)?;
    let prompt = build_prompt(&render_transcript(&lines));
    let raw = generate(&path, &prompt)?;

    let json = extract_json(&raw)
        .ok_or_else(|| "Il modello non ha prodotto un risultato leggibile.".to_string())?;

    let mut analysis: Analysis = serde_json::from_str(&json)
        .map_err(|_| "Il modello non ha prodotto un risultato leggibile.".to_string())?;

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
