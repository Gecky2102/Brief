use std::sync::Mutex;

use serde::Serialize;
use tauri::AppHandle;

use crate::models;

/// Il modello e5 vuole i testi prefissati: distingue la domanda dai documenti
/// indicizzati e senza i prefissi la qualità cala parecchio.
const PREFIX_QUERY: &str = "query: ";
const PREFIX_PASSAGE: &str = "passage: ";
const MAX_TOKENS: usize = 512;

pub struct Embedder {
    session: Mutex<ort::session::Session>,
    tokenizer: tokenizers::Tokenizer,
}

impl Embedder {
    pub fn load(app: &AppHandle) -> Result<Self, String> {
        let modello = models::ensure(app, &models::EMBEDDING)?;
        let vocabolario = models::ensure(app, &models::EMBEDDING_TOKENIZER)?;

        let session = ort::session::Session::builder()
            .map_err(|cause| format!("Motore di ricerca non inizializzato: {cause}"))?
            .commit_from_file(&modello)
            .map_err(|cause| format!("Modello di ricerca non caricato: {cause}"))?;

        let tokenizer = tokenizers::Tokenizer::from_file(&vocabolario)
            .map_err(|cause| format!("Vocabolario non caricato: {cause}"))?;

        Ok(Self {
            session: Mutex::new(session),
            tokenizer,
        })
    }

    fn run(&self, testo: &str) -> Result<Vec<f32>, String> {
        let codifica = self
            .tokenizer
            .encode(testo, true)
            .map_err(|cause| format!("Testo non tokenizzato: {cause}"))?;

        let mut ids: Vec<i64> = codifica.get_ids().iter().map(|v| *v as i64).collect();
        let mut mask: Vec<i64> = codifica
            .get_attention_mask()
            .iter()
            .map(|v| *v as i64)
            .collect();
        ids.truncate(MAX_TOKENS);
        mask.truncate(MAX_TOKENS);

        let lunghezza = ids.len();
        if lunghezza == 0 {
            return Err("Testo vuoto.".into());
        }

        let tensore_ids = ort::value::Tensor::from_array((vec![1, lunghezza], ids))
            .map_err(|cause| format!("Ingresso non preparato: {cause}"))?;
        let tensore_mask = ort::value::Tensor::from_array((vec![1, lunghezza], mask.clone()))
            .map_err(|cause| format!("Ingresso non preparato: {cause}"))?;

        let mut session = self
            .session
            .lock()
            .map_err(|_| "Motore di ricerca non disponibile.".to_string())?;

        let uscite = session
            .run(ort::inputs![
                "input_ids" => tensore_ids,
                "attention_mask" => tensore_mask,
            ])
            .map_err(|cause| format!("Ricerca fallita: {cause}"))?;

        let (forma, dati) = uscite[0]
            .try_extract_tensor::<f32>()
            .map_err(|cause| format!("Risultato non leggibile: {cause}"))?;

        // Media sui token validi: è il pooling con cui e5 è stato addestrato.
        let dimensione = *forma.last().unwrap_or(&384) as usize;
        let mut vettore = vec![0.0_f32; dimensione];
        let mut validi = 0.0_f32;

        for (indice, attivo) in mask.iter().enumerate() {
            if *attivo == 0 {
                continue;
            }
            validi += 1.0;
            let inizio = indice * dimensione;
            for posizione in 0..dimensione {
                vettore[posizione] += dati[inizio + posizione];
            }
        }

        if validi > 0.0 {
            for valore in vettore.iter_mut() {
                *valore /= validi;
            }
        }

        let norma = vettore.iter().map(|v| v * v).sum::<f32>().sqrt().max(1e-8);
        Ok(vettore.iter().map(|v| v / norma).collect())
    }

    pub fn embed_passage(&self, testo: &str) -> Result<Vec<f32>, String> {
        self.run(&format!("{PREFIX_PASSAGE}{testo}"))
    }

    pub fn embed_query(&self, testo: &str) -> Result<Vec<f32>, String> {
        self.run(&format!("{PREFIX_QUERY}{testo}"))
    }
}

#[derive(Serialize)]
pub struct SemanticHit {
    pub segment_id: i64,
    pub score: f32,
}

/// Calcola le impronte di un elenco di righe. Il frontend le conserva nel
/// database, così l'indicizzazione avviene una volta sola per sessione.
#[tauri::command]
pub async fn embed_segments(
    app: AppHandle,
    texts: Vec<String>,
) -> Result<Vec<Vec<f32>>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let embedder = Embedder::load(&app)?;
        texts
            .iter()
            .map(|testo| embedder.embed_passage(testo))
            .collect()
    })
    .await
    .map_err(|cause| format!("Indicizzazione interrotta: {cause}"))?
}

/// Confronta una domanda con le impronte già calcolate e restituisce le righe
/// più vicine per significato.
#[tauri::command]
pub async fn search_semantic(
    app: AppHandle,
    query: String,
    candidates: Vec<(i64, Vec<f32>)>,
    limit: usize,
) -> Result<Vec<SemanticHit>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let embedder = Embedder::load(&app)?;
        let domanda = embedder.embed_query(&query)?;

        let mut punteggi: Vec<SemanticHit> = candidates
            .into_iter()
            .map(|(segment_id, vettore)| SemanticHit {
                segment_id,
                score: crate::diarization::similarity(&domanda, &vettore),
            })
            .collect();

        punteggi.sort_by(|a, b| b.score.total_cmp(&a.score));
        punteggi.truncate(limit.max(1));

        // Sotto questa soglia i risultati sono rumore: meglio dire «niente»
        // che mostrare righe scollegate dalla domanda.
        punteggi.retain(|hit| hit.score > 0.78);
        Ok(punteggi)
    })
    .await
    .map_err(|cause| format!("Ricerca interrotta: {cause}"))?
}

#[tauri::command]
pub fn semantic_ready(app: AppHandle) -> bool {
    models::path_if_present(&app, &models::EMBEDDING).is_some()
        && models::path_if_present(&app, &models::EMBEDDING_TOKENIZER).is_some()
}
