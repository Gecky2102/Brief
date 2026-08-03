use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter};

use crate::provider::{self, Request};
use crate::settings;

/// Un report di 1500-3000 parole richiede spazio: con un tetto basso il
/// documento veniva troncato a metà.
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

/// Struttura del documento in base al taglio scelto. Ogni taglio chiede sezioni
/// diverse: un verbale non somiglia a una sintesi per la direzione.
fn struttura(style: settings::ReportStyle) -> &'static str {
    use settings::ReportStyle::*;
    match style {
        Auto | Meeting => "## Quadro generale\nContesto, partecipanti con i nomi che compaiono, scopo dell'incontro, come si è svolto.\n\n## <un titolo per ciascun tema affrontato>\nUna sezione per ogni argomento di sostanza, in ordine di importanza. Spiega il problema, le posizioni emerse, i dettagli tecnici, i numeri, i sistemi e gli strumenti citati. Usa sottosezioni, elenchi e tabelle dove aiutano.\n\n## Decisioni prese\nTabella | Decisione | Motivazione | Chi decide |\n\n## Attivita da svolgere\nTabella | Attivita | Responsabile | Scadenza |\n\n## Punti aperti\nQuestioni irrisolte, ciascuna con una riga di contesto.\n\n## Rischi e criticita\nOstacoli, dipendenze e problemi segnalati.",

        Executive => "## In sintesi\nCinque righe che dicono l'essenziale a chi ha due minuti.\n\n## Situazione\nContesto e problema di fondo, senza tecnicismi.\n\n## Decisioni e implicazioni\nTabella | Decisione | Impatto | Chi decide |\n\n## Cosa serve ora\nTabella | Azione | Responsabile | Entro quando |\n\n## Rischi\nI tre o quattro rischi che contano davvero, con la loro gravita.\n\n## Approfondimento\nIl dettaglio per chi vuole andare a fondo, organizzato per tema.",

        Lecture => "## Argomento della lezione\nDi cosa tratta e come si inserisce nel percorso.\n\n## <un titolo per ciascun concetto spiegato>\nUna sezione per concetto: definizione, spiegazione distesa, esempi fatti a lezione, formule o passaggi se ci sono.\n\n## Definizioni\nTabella | Termine | Significato |\n\n## Esempi ed esercizi\nGli esempi svolti, con il ragionamento seguito.\n\n## Da studiare\nCosa e stato assegnato o consigliato.\n\n## Punti da chiarire\nCio che e rimasto oscuro o e stato rimandato.",

        Interview => "## Profilo\nChi e l'intervistato, ruolo, contesto dell'intervista.\n\n## <un titolo per ciascun tema toccato>\nUna sezione per tema, con le posizioni espresse e le motivazioni addotte. Riporta fra virgolette i passaggi piu significativi.\n\n## Citazioni rilevanti\nLe frasi che meritano di essere riportate testualmente.\n\n## Fatti e cifre\nTabella | Dato | Contesto |\n\n## Domande rimaste senza risposta\nCio su cui non si e arrivati in fondo.",

        Standup => "## In breve\nTre righe sullo stato complessivo.\n\n## Per persona\nUna sezione per partecipante: fatto, in corso, bloccato da.\n\n## Impedimenti\nTabella | Impedimento | Chi e bloccato | Chi puo sbloccare |\n\n## Attivita da svolgere\nTabella | Attivita | Responsabile | Entro quando |\n\n## Note\nTutto il resto che vale la pena ricordare.",

        Brainstorm => "## Obiettivo della sessione\nQual era la domanda di partenza.\n\n## Idee emerse\nUna sezione per idea o famiglia di idee: in cosa consiste, chi l'ha proposta, obiezioni e sviluppi.\n\n## Confronto\nTabella | Idea | A favore | Contro |\n\n## Direzioni promettenti\nSu cosa vale la pena insistere e perche.\n\n## Da approfondire\nCosa va verificato prima di decidere.",

        Minutes => "## Intestazione\nData, ora, luogo se emerge, presenti con i nomi che compaiono.\n\n## Ordine del giorno\nI punti trattati, nell'ordine in cui sono stati affrontati.\n\n## Svolgimento\nUna sezione per punto, con il resoconto fedele della discussione: interventi, posizioni, obiezioni.\n\n## Deliberazioni\nTabella | Punto | Decisione | Esito |\n\n## Impegni assunti\nTabella | Impegno | Chi | Entro quando |\n\n## Chiusura\nCome si e conclusa la seduta e cosa e stato rinviato.",
    }
}

fn lunghezza(length: settings::ReportLength) -> &'static str {
    use settings::ReportLength::*;
    match length {
        Brief => "da 600 a 1000 parole",
        Standard => "da 1500 a 3000 parole",
        Deep => "da 3500 a 6000 parole, entrando nel dettaglio di ogni passaggio",
    }
}

fn tokens_per(length: settings::ReportLength) -> u32 {
    use settings::ReportLength::*;
    match length {
        Brief => 6_000,
        Standard => 16_000,
        Deep => 32_000,
    }
}

fn build_report_prompt(settings: &settings::Settings, kind: &str) -> String {
    // Con «Auto» il taglio viene dal tipo riconosciuto nella trascrizione.
    let style = if settings.report_style == settings::ReportStyle::Auto {
        match kind {
            "lecture" => settings::ReportStyle::Lecture,
            "interview" => settings::ReportStyle::Interview,
            "casual" => settings::ReportStyle::Brainstorm,
            _ => settings::ReportStyle::Meeting,
        }
    } else {
        settings.report_style
    };

    let extra = if settings.report_notes.trim().is_empty() {
        String::new()
    } else {
        format!(
            "\n\nIstruzioni aggiuntive di chi legge, da rispettare:\n{}",
            settings.report_notes.trim()
        )
    };

    format!(
        "Sei un analista che redige documenti professionali in italiano a partire da \
trascrizioni di riunioni, lezioni e conversazioni di lavoro.

Produci un documento in Markdown, {}. Non e un riassunto: e un documento di lavoro che \
una persona assente deve poter leggere al posto di aver partecipato.

Struttura da seguire, adattandola al contenuto reale:

# <titolo del documento>

{}

Regole di scrittura:
- Prosa distesa e professionale, mai telegrafica. Ogni sezione ha almeno due paragrafi \
  di sostanza, non un elenco secco.
- Conserva nomi di persone, aziende, sistemi, prodotti, cifre e date esattamente come compaiono.
- Usa **grassetto** per i termini chiave, tabelle per i dati strutturati, elenchi solo \
  quando l'informazione e davvero una lista.
- Dove un'informazione non emerge dalla trascrizione scrivi «non indicato», senza inventare.
- La trascrizione e automatica e contiene errori di riconoscimento: ignora le parole \
  incomprensibili senza segnalarle e senza costruirci sopra ipotesi.
- Niente premesse, scuse o commenti sul tuo lavoro: produci solo il documento.{}",
        lunghezza(settings.report_length),
        struttura(style),
        extra
    )
}

const SYSTEM_CLASSIFICA: &str = "Leggi la trascrizione e rispondi con una sola riga:

TIPO: <work_call|meeting|lecture|interview|casual>

Nient'altro.";

const SYSTEM_BLOCCO: &str = "Sei un assistente che prende appunti dettagliati da trascrizioni \
di riunioni in italiano. Riporta tutto cio che di rilevante viene detto in questo estratto: \
argomenti, posizioni espresse, problemi concreti, dettagli tecnici, decisioni, cose da fare, \
domande aperte. Conserva nomi di persone, aziende, sistemi, prodotti, cifre e date esattamente \
come compaiono: serviranno per il report finale. Non sintetizzare troppo: questi appunti \
sostituiscono la trascrizione originale. La trascrizione e automatica e contiene errori: \
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
        api_key: settings::api_key(&app),
        app: &app,
    };

    let transcript = render_transcript(&lines);

    // Riconosce il tipo di conversazione prima di scrivere: il taglio del
    // documento dipende da quello, e con «Auto» è l'unico modo per sceglierlo.
    let mut kind = "meeting".to_string();
    if let Ok(risposta) = session.ask(
        SYSTEM_CLASSIFICA,
        &transcript.chars().take(4000).collect::<String>(),
        "reading",
        0,
        1,
        30,
    ) {
        let mut provvisoria = Analysis::default();
        parse_header(&risposta, &mut provvisoria);
        if !provvisoria.kind.is_empty() {
            kind = provvisoria.kind;
        }
    }

    let system_report = build_report_prompt(&session.settings, &kind);
    let report_tokens = tokens_per(session.settings.report_length);

    let report = if transcript.chars().count() <= SINGLE_PASS_CHARS {
        session.ask(&system_report, &transcript, "writing", 0, 1, report_tokens)?
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
            &system_report,
            &format!("Note prese durante la riunione, in ordine cronologico:\n\n{parziali}"),
            "writing",
            totale,
            totale + 1,
            report_tokens,
        )?
    };

    if report.trim().is_empty() {
        return Err("Il modello non ha prodotto alcun report.".into());
    }

    // Intestazione a parte: chiedere titolo e tipo insieme al documento faceva
    // sprecare al modello l'inizio della risposta.
    let mut analysis = Analysis {
        kind: kind.clone(),
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
        analysis: !settings::api_key(&app).is_empty(),
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
