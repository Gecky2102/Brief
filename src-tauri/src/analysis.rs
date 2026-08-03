use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter};

use crate::provider::{self, Request};
use crate::settings;

/// Un report di 1500-3000 parole richiede spazio: con un tetto basso il
/// documento veniva troncato a metà.
const REPORT_TOKENS: u32 = 16_000;
const NOTES_TOKENS: u32 = 2_000;
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
    /// Due frasi per la libreria e l'anteprima.
    pub summary: String,
    /// Il report vero e proprio, in Markdown.
    #[serde(default)]
    pub report: String,
}

#[derive(Clone, Serialize)]
pub struct AnalysisProgress {
    /// «reading» mentre riassume i blocchi, «writing» durante la sintesi finale.
    phase: &'static str,
    step: usize,
    steps: usize,
    preview: String,
}

const SYSTEM_REPORT: &str = "Sei un analista che redige report professionali in italiano \
a partire da trascrizioni di riunioni, lezioni e conversazioni di lavoro.

Produci un documento in Markdown, lungo e approfondito: da 1500 a 3000 parole. \
Non è un riassunto, è un documento di lavoro che una persona assente deve poter leggere \
al posto di aver partecipato.

Struttura da seguire, adattandola al contenuto reale:

# <titolo del documento>

## Quadro generale
Contesto, partecipanti (con i nomi che compaiono), scopo dell'incontro, come si è svolto.

## <un titolo per ciascun tema affrontato>
Una sezione per ogni argomento di sostanza, in ordine di importanza. Dentro ogni sezione \
spiega il problema, le posizioni emerse, i dettagli tecnici, i numeri, i sistemi e gli \
strumenti citati. Usa sottosezioni, elenchi puntati e tabelle dove aiutano a leggere.

## Decisioni prese
Tabella con le colonne | Decisione | Motivazione | Chi decide |

## Attività da svolgere
Tabella con le colonne | Attività | Responsabile | Scadenza |
Se una informazione non emerge dalla trascrizione scrivi «non indicato».

## Punti aperti
Elenco delle questioni rimaste irrisolte, ciascuna con una riga di contesto.

## Rischi e criticità
Ostacoli, dipendenze e problemi segnalati durante la discussione.

Regole: scrivi in italiano corretto e professionale, in prosa distesa, non telegrafica. \
Conserva nomi di persone, aziende, sistemi, prodotti, cifre e date esattamente come compaiono. \
La trascrizione è automatica e contiene errori di riconoscimento: ignora le parole \
incomprensibili senza segnalarle e senza inventarci sopra. \
Non aggiungere premesse, scuse o commenti sul tuo lavoro: produci solo il documento.";

const SYSTEM_BLOCCO: &str = "Sei un assistente che prende appunti dettagliati da trascrizioni \
di riunioni in italiano. Riporta tutto ciò che di rilevante viene detto in questo estratto: \
argomenti, posizioni espresse, problemi concreti, dettagli tecnici, decisioni, cose da fare, \
domande aperte. Conserva nomi di persone, aziende, sistemi, prodotti, cifre e date esattamente \
come compaiono: serviranno per il report finale. Non sintetizzare troppo: questi appunti \
sostituiscono la trascrizione originale. La trascrizione è automatica e contiene errori: \
ignora le parole incomprensibili invece di inventarle. Rispondi con un elenco puntato in italiano.";

const SYSTEM_INTESTAZIONE: &str = "Leggi il report e rispondi con tre righe etichettate, \
nient'altro:

TIPO: <work_call|meeting|lecture|interview|casual>
TITOLO: <titolo breve, massimo 8 parole, in italiano>
SOMMARIO: <due frasi che dicono di cosa tratta il documento>";

/// Legge le tre righe di intestazione. Le righe non riconosciute vengono
/// ignorate: capita che il modello aggiunga commenti.
fn parse_header(raw: &str, analysis: &mut Analysis) {
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
            "SOMMARIO" => analysis.summary = value,
            _ => {}
        }
    }
}

/// Ricava un titolo dal primo heading del report, quando il modello non
/// produce l'intestazione richiesta.
fn title_from_report(report: &str) -> String {
    report
        .lines()
        .find(|line| line.starts_with("# "))
        .map(|line| line.trim_start_matches('#').trim().to_string())
        .unwrap_or_else(|| "Report della sessione".into())
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
        max_tokens: u32,
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
                max_tokens,
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

    let report = if transcript.chars().count() <= SINGLE_PASS_CHARS {
        session.ask(SYSTEM_REPORT, &transcript, "writing", 0, 1, REPORT_TOKENS)?
    } else {
        // Prima note dettagliate blocco per blocco, poi il documento finale:
        // così nessuna parte della riunione resta fuori dal report.
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
                NOTES_TOKENS,
            )?;
            parziali.push_str(&testo);
            parziali.push_str("\n\n");
        }

        session.ask(
            SYSTEM_REPORT,
            &format!(
                "Note prese durante la riunione, in ordine cronologico:\n\n{parziali}"
            ),
            "writing",
            totale,
            totale + 1,
            REPORT_TOKENS,
        )?
    };

    if report.trim().is_empty() {
        return Err("Il modello non ha prodotto alcun report.".into());
    }

    // Intestazione a parte: chiedere titolo e tipo insieme al documento faceva
    // sprecare al modello l'inizio della risposta.
    let mut analysis = Analysis {
        kind: "unknown".into(),
        title: title_from_report(&report),
        summary: String::new(),
        report: report.clone(),
    };

    if let Ok(header) = session.ask(
        SYSTEM_INTESTAZIONE,
        &report.chars().take(6000).collect::<String>(),
        "writing",
        0,
        1,
        200,
    ) {
        parse_header(&header, &mut analysis);
    }

    if analysis.title.trim().is_empty() {
        analysis.title = title_from_report(&report);
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
    fn legge_l_intestazione() {
        let mut analysis = Analysis::default();
        parse_header(
            "TIPO: meeting\nTITOLO: Riunione gestionale\nSOMMARIO: Documento sul nuovo gestionale.\nNota ignorata",
            &mut analysis,
        );
        assert_eq!(analysis.kind, "meeting");
        assert_eq!(analysis.title, "Riunione gestionale");
        assert_eq!(analysis.summary, "Documento sul nuovo gestionale.");
    }

    #[test]
    fn ricava_il_titolo_dal_report() {
        let report = "Premessa\n\n# Analisi del gestionale\n\nTesto…";
        assert_eq!(title_from_report(report), "Analisi del gestionale");
        assert_eq!(title_from_report("nessun titolo"), "Report della sessione");
    }
}
