use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter};

use crate::provider::{self, Request};
use crate::settings;

const MAX_OUTPUT_TOKENS: u32 = 2000;
/// Oltre questa lunghezza la trascrizione viene riassunta a blocchi: i modelli
/// hanno finestre ampie, ma un testo enorme fa perdere i dettagli.
const CHUNK_CHARS: usize = 24_000;
const SINGLE_PASS_CHARS: usize = 40_000;

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

#[derive(Clone, Serialize)]
pub struct AnalysisProgress {
    /// «reading» mentre riassume i blocchi, «writing» durante la sintesi finale.
    phase: &'static str,
    step: usize,
    steps: usize,
    preview: String,
}

const SYSTEM_FINALE: &str = "Sei un assistente che analizza trascrizioni di riunioni in italiano. \
Rispondi SOLO con righe etichettate, una per riga, in questo formato esatto:

TIPO: <work_call|meeting|lecture|interview|casual>
TITOLO: <massimo 8 parole>
RIASSUNTO: <3-6 frasi su una sola riga>
DECISIONE: <una decisione presa>
AZIONE: <una cosa da fare, all'infinito, con il responsabile fra parentesi se emerge>
DOMANDA: <una domanda rimasta aperta>

Ripeti le righe DECISIONE, AZIONE e DOMANDA una volta per ciascuna voce, al massimo 8 per tipo, \
tutte diverse fra loro. Ometti la riga se non hai nulla da dire. \
Conserva i nomi propri di persone, aziende e strumenti così come compaiono. \
La trascrizione è automatica e contiene errori di riconoscimento: ignora le parole \
incomprensibili invece di inventarci sopra. Scrivi ogni parola in italiano. \
Non aggiungere altro testo oltre alle righe etichettate.";

const SYSTEM_BLOCCO: &str = "Sei un assistente che riassume trascrizioni di riunioni in italiano. \
Elenca in punti sintetici ciò che viene detto in questo estratto: argomenti trattati, \
decisioni prese, cose da fare, domande rimaste aperte. \
Conserva i nomi propri di persone, aziende, strumenti e prodotti così come compaiono. \
La trascrizione è automatica e contiene errori: ignora le parole incomprensibili invece di \
inventarle. Rispondi solo con l'elenco puntato, scritto interamente in italiano.";

/// Legge le righe etichettate prodotte dal modello. Le righe che non riconosce
/// vengono ignorate: capita che il modello aggiunga commenti.
fn parse_labelled(raw: &str) -> Analysis {
    let mut analysis = Analysis {
        kind: "unknown".into(),
        ..Default::default()
    };

    for line in raw.lines() {
        let line = line.trim().trim_start_matches(['-', '*', '#', ' ']);
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
    if list.len() < 8 && !list.iter().any(|v| v.to_lowercase() == normalized) {
        list.push(value);
    }
}

fn render_transcript(lines: &[TranscriptLine]) -> String {
    lines
        .iter()
        .map(|line| format!("{}: {}", line.speaker, line.text.trim()))
        .collect::<Vec<_>>()
        .join("\n")
}

struct Session<'a> {
    app: &'a AppHandle,
    settings: settings::Settings,
    api_key: String,
}

impl Session<'_> {
    /// Esegue una richiesta riportando il testo mentre arriva, così
    /// l'interfaccia può mostrarlo scorrere invece di uno spinner muto.
    fn ask(
        &self,
        system: &str,
        user: &str,
        phase: &'static str,
        step: usize,
        steps: usize,
    ) -> Result<String, String> {
        let mut accumulato = String::new();
        let mut ultimo = std::time::Instant::now();

        let testo = provider::stream(
            Request {
                provider: self.settings.provider,
                base_url: &self.settings.base_url,
                api_key: &self.api_key,
                model: &self.settings.model,
                system,
                user,
                max_tokens: MAX_OUTPUT_TOKENS,
            },
            |delta| {
                accumulato.push_str(delta);
                if ultimo.elapsed().as_millis() >= 120 {
                    ultimo = std::time::Instant::now();
                    let _ = self.app.emit(
                        "analysis://progress",
                        AnalysisProgress {
                            phase,
                            step,
                            steps,
                            preview: accumulato.clone(),
                        },
                    );
                }
            },
        )?;

        let _ = self.app.emit(
            "analysis://progress",
            AnalysisProgress {
                phase,
                step,
                steps,
                preview: testo.clone(),
            },
        );
        Ok(testo)
    }
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

    let session = Session {
        settings: settings::load(&app),
        api_key: settings::api_key(),
        app: &app,
    };

    let transcript = render_transcript(&lines);

    let raw = if transcript.chars().count() <= SINGLE_PASS_CHARS {
        session.ask(SYSTEM_FINALE, &transcript, "writing", 0, 1)?
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
            let testo = session.ask(
                SYSTEM_BLOCCO,
                &format!("Estratto {} di {}:\n\n{blocco}", indice + 1, totale),
                "reading",
                indice,
                totale + 1,
            )?;
            parziali.push_str(&testo);
            parziali.push('\n');
        }

        session.ask(SYSTEM_FINALE, &parziali, "writing", totale, totale + 1)?
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

#[derive(Serialize)]
pub struct ModelsStatus {
    transcription: bool,
    analysis: bool,
}

#[tauri::command]
pub fn models_status(app: AppHandle) -> ModelsStatus {
    ModelsStatus {
        transcription: crate::transcriber::is_model_ready(&app),
        analysis: !settings::api_key().is_empty(),
    }
}

#[cfg(test)]
mod tests {
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
        for _ in 0..12 {
            raw.push_str("AZIONE: Sentire Marco\n");
        }
        for indice in 0..12 {
            raw.push_str(&format!("DECISIONE: Decisione {indice}\n"));
        }

        let analysis = parse_labelled(&raw);
        assert_eq!(analysis.actions.len(), 1, "i doppioni vanno scartati");
        assert_eq!(analysis.decisions.len(), 8, "al massimo otto voci");
    }

    #[test]
    fn tollera_elenchi_puntati() {
        let analysis =
            parse_labelled("- AZIONE: Inviare il preventivo\n* DECISIONE: Chiudere a 4000");
        assert_eq!(analysis.actions, vec!["Inviare il preventivo"]);
        assert_eq!(analysis.decisions, vec!["Chiudere a 4000"]);
    }
}
