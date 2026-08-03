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

#[derive(Deserialize, Default)]
pub struct SessionContext {
    pub date: String,
    pub duration_minutes: i64,
    pub speakers: Vec<String>,
}

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

/// Segnala l'inizio di ogni fase con una stima del lavoro: su una riunione
/// lunga l'analisi dura minuti e senza indicazioni sembra bloccata.
#[derive(Clone, Serialize)]
pub struct AnalysisProgress {
    /// «reading» mentre legge i blocchi, «writing» mentre scrive il documento,
    /// «titling» per la breve chiamata che ne ricava titolo e tipo.
    phase: &'static str,
    step: usize,
    steps: usize,
    preview: String,
    /// Parole prodotte finora: dà la misura di quanto sta crescendo il documento.
    words: usize,
}

/// Struttura del documento in base al taglio scelto. Ogni taglio chiede sezioni
/// diverse: un verbale non somiglia a una sintesi per la direzione.
fn struttura(style: settings::ReportStyle) -> &'static str {
    use settings::ReportStyle::*;
    match style {
        Auto | Meeting => "Apri con un titolo di primo livello che nomini l'oggetto concreto della riunione.\nPoi, nell'ordine:\n- una sezione «Quadro generale» con contesto, partecipanti nominati, scopo e svolgimento;\n- una sezione per ciascun tema affrontato, intitolata con il tema stesso, in ordine di importanza, che spieghi il problema, le posizioni emerse, i dettagli tecnici, i numeri e i sistemi citati;\n- «Decisioni prese», tabella con colonne Decisione, Motivazione, Chi decide;\n- «Attivita da svolgere», tabella con colonne Attivita, Responsabile, Scadenza;\n- «Punti aperti», elenco delle questioni irrisolte con una riga di contesto ciascuna;\n- «Rischi e criticita», ostacoli e dipendenze segnalati.",

        Executive => "Apri con un titolo di primo livello che nomini l'oggetto concreto della discussione.\nPoi: «In sintesi» con l'essenziale in cinque righe; «Situazione» con contesto e problema di fondo senza tecnicismi; «Decisioni e implicazioni» come tabella con colonne Decisione, Impatto, Chi decide; «Cosa serve ora» come tabella con colonne Azione, Responsabile, Entro quando; «Rischi» con i tre o quattro che contano davvero e la loro gravita; «Approfondimento» con il dettaglio per tema.",

        Lecture => "Apri con un titolo di primo livello che nomini l'argomento della lezione.\nPoi: «Argomento della lezione» con inquadramento; una sezione per ciascun concetto spiegato, intitolata col concetto stesso, con definizione, spiegazione distesa ed esempi svolti a lezione; «Definizioni» come tabella con colonne Termine, Significato; «Esempi ed esercizi» con il ragionamento seguito; «Da studiare» con quanto assegnato; «Punti da chiarire» con cio che e rimasto in sospeso.",

        Interview => "Apri con un titolo di primo livello che nomini il tema dell'intervista.\nPoi: «Profilo» con chi e l'intervistato e il contesto; una sezione per ciascun tema toccato, con le posizioni espresse e le motivazioni, riportando fra virgolette i passaggi significativi; «Citazioni rilevanti» con le frasi da conservare testualmente; «Fatti e cifre» come tabella con colonne Dato, Contesto; «Domande rimaste senza risposta».",

        Standup => "Apri con un titolo di primo livello che nomini il gruppo e il periodo.\nPoi: «In breve» con lo stato complessivo in tre righe; una sezione per ciascun partecipante con fatto, in corso e cosa lo blocca; «Impedimenti» come tabella con colonne Impedimento, Chi e bloccato, Chi puo sbloccare; «Attivita da svolgere» come tabella con colonne Attivita, Responsabile, Entro quando; «Note» per il resto.",

        Brainstorm => "Apri con un titolo di primo livello che nomini la domanda di partenza.\nPoi: «Obiettivo della sessione»; una sezione per ciascuna idea o famiglia di idee, con in cosa consiste, chi l'ha proposta, obiezioni e sviluppi; «Confronto» come tabella con colonne Idea, A favore, Contro; «Direzioni promettenti» con le motivazioni; «Da approfondire» con cosa verificare prima di decidere.",

        Minutes => "Apri con un titolo di primo livello che nomini la seduta.\nPoi: «Intestazione» con data, luogo se emerge e presenti nominati; «Ordine del giorno» con i punti nell'ordine trattato; una sezione per ciascun punto con il resoconto fedele degli interventi e delle posizioni; «Deliberazioni» come tabella con colonne Punto, Decisione, Esito; «Impegni assunti» come tabella con colonne Impegno, Chi, Entro quando; «Chiusura» con quanto rinviato.",
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
trascrizioni di riunioni, lezioni e conversazioni.

Scrivi il documento, {}. Non e un riassunto: e un documento di lavoro che una persona \
assente deve poter leggere al posto di aver partecipato.

{}

Come scrivere:
- Prosa distesa e professionale. Ogni sezione ha almeno due paragrafi di sostanza.
- Conserva nomi di persone, aziende, sistemi, prodotti, cifre e date esattamente come compaiono.
- **Grassetto** sui termini chiave, tabelle per i dati strutturati, elenchi solo per vere liste.
- La trascrizione e automatica e contiene errori: ignora le parole incomprensibili senza \
  segnalarle e senza costruirci sopra ipotesi.

Non stai conversando con nessuno: stai producendo un file. \
Non c'e un interlocutore da salutare, informare o consigliare. \
Se altre istruzioni ti hanno dato un nome, un tono amichevole o un ruolo da assistente, \
qui non valgono: qui produci solo il documento.

Vincoli assoluti, la loro violazione rende il documento inutilizzabile:
- La tua risposta inizia con «# » seguito dal titolo. Nessuna riga prima, per nessun motivo.
- Non salutare, non rivolgerti al lettore, non dargli del tu. Niente «Ciao», \
  niente nomi propri di chi legge, niente «ecco», «come vedi», «spero ti sia utile».
- Non commentare l'argomento («bell'argomento», «tema interessante») e non annunciare \
  cosa stai per fare («ecco una sintesi chiara»). Scrivi il documento e basta.
- Nessun commento sul tuo lavoro, sui tuoi limiti o su cosa potresti fare in seguito. \
  Niente frasi come «Capisco», «Posso fornirti», «Se vuoi posso», «Dimmi se procedere», \
  «Nota: sostituisci», «questa e una bozza». Non offrire alternative e non chiedere conferme.
- Nessun segnaposto e nessuna istruzione rivolta al lettore. Scrivi il contenuto vero, \
  non lo scheletro da riempire.
- Ometti del tutto una sezione o una riga di tabella se la trascrizione non contiene \
  l'informazione. Non produrre tabelle o elenchi fatti di «non indicato»: una sezione \
  assente e molto meglio di una sezione vuota.
- Non aggiungere sezioni su metodo, fonti da verificare o disclaimer.
- Il documento si chiude con l'ultima sezione di contenuto. Nessuna conclusione che parli \
  di se stessa, nessuna proposta finale.{}",
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

/// Riconosce le risposte in cui il modello si tira indietro invece di
/// scrivere: capita quando si convince di essere un assistente con strumenti,
/// e va ritentato con istruzioni più dirette.
/// Risposta scritta come in una chat invece che come documento: capita quando
/// il servizio inietta un proprio prompt di sistema con un tono da assistente.
fn is_conversational(testo: &str) -> bool {
    let inizio: String = testo.trim().chars().take(240).collect::<String>().to_lowercase();

    const APERTURE: [&str; 14] = [
        "ciao",
        "buongiorno",
        "salve",
        "certo,",
        "certamente",
        "ecco una",
        "ecco un",
        "ecco la",
        "ecco il",
        "volentieri",
        "bello argomento",
        "bell'argomento",
        "ottima domanda",
        "perfetto,",
    ];

    APERTURE.iter().any(|apertura| inizio.starts_with(apertura))
        || (!testo.contains("# ") && inizio.contains(" ti "))
}

fn is_refusal(testo: &str) -> bool {
    let normalizzato = testo.trim().to_lowercase();
    if normalizzato.contains("\n# ") || normalizzato.starts_with("# ") {
        return false;
    }

    const SEGNALI: [&str; 12] = [
        "mi serve accesso",
        "non posso eseguire",
        "non posso generare",
        "non sono in grado di",
        "non ho accesso",
        "senza le api",
        "strumenti esterni",
        "posso procedere fornendo",
        "posso fornirti un modello",
        "i need access",
        "i cannot generate",
        "i'm unable to",
    ];

    SEGNALI.iter().any(|segnale| normalizzato.contains(segnale))
}

/// Ripulisce l'output dal contorno che i modelli aggiungono comunque: la
/// premessa prima del titolo e le offerte di aiuto in coda.
fn clean_report(raw: &str) -> String {
    let testo = raw.trim();

    // Tutto cio che precede il primo titolo di primo livello e preambolo:
    // saluti, commenti sull'argomento, annunci di cosa sta per fare.
    let corpo = match testo.find("\n# ") {
        Some(pos) if !testo.starts_with("# ") => &testo[pos + 1..],
        _ => testo,
    };

    const CODE: [&str; 10] = [
        "se vuoi, posso",
        "se vuoi posso",
        "dimmi se vuoi",
        "fammi sapere se",
        "posso convertire",
        "posso generare",
        "vuoi che proceda",
        "se preferisci posso",
        "resto a disposizione",
        "spero che questo",
    ];

    let mut righe: Vec<&str> = corpo.lines().collect();
    while let Some(ultima) = righe.last() {
        let normalizzata = ultima.trim().to_lowercase();
        let da_tagliare = normalizzata.is_empty()
            || CODE.iter().any(|marker| normalizzata.starts_with(marker))
            || normalizzata.starts_with("- ")
                && CODE.iter().any(|marker| normalizzata.contains(marker));
        if da_tagliare && !normalizzata.is_empty() {
            righe.pop();
        } else if normalizzata.is_empty() {
            righe.pop();
        } else {
            break;
        }
    }

    righe.join("\n").trim().to_string()
}

/// Intestazione con i dati oggettivi della sessione: senza, il modello non
/// sa quando si è svolta né chi c'era, e finisce per scrivere «non indicato».
fn context_header(context: &SessionContext) -> String {
    let mut righe = vec![format!("Data: {}", context.date)];
    if context.duration_minutes > 0 {
        righe.push(format!("Durata: {} minuti", context.duration_minutes));
    }
    if !context.speakers.is_empty() {
        righe.push(format!("Voci presenti: {}", context.speakers.join(", ")));
    }
    format!("Dati della sessione:\n{}\n\n", righe.join("\n"))
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
    /// Scrive il documento, riprovando una volta con istruzioni più dirette se
    /// il modello si tira indietro invece di produrlo.
    fn write_report(
        &self,
        system: &str,
        user: &str,
        step: usize,
        steps: usize,
        max_tokens: u32,
    ) -> Result<String, String> {
        // Il prefill funziona davvero solo con Anthropic: altrove il messaggio
        // dell'assistente apre un turno nuovo invece di continuarlo.
        let prefill = if self.settings.provider == crate::provider::Provider::Anthropic {
            Some("# ")
        } else {
            None
        };

        let primo = self.ask(system, user, "writing", step, steps, max_tokens, prefill)?;
        if !is_refusal(&primo) && !is_conversational(&primo) && primo.contains("# ") {
            return Ok(primo);
        }

        let insistente = format!(
            "{system}\n\nIl tentativo precedente e stato scartato perche non rispettava \
il formato. Ricorda: non e una conversazione, e un file. Nessun saluto, nessun nome, \
nessun commento, nessuna domanda. Hai gia tutto cio che serve nella trascrizione qui \
sotto: nessuno strumento esterno, nessuna API, nessuna conferma. \
Il primo carattere della tua risposta e «#», seguito da uno spazio e dal titolo."
        );

        let secondo = self.ask(
            &insistente,
            &format!("{user}\n\nScrivi ora il documento completo."),
            "writing",
            step,
            steps,
            max_tokens,
            prefill,
        )?;

        if is_refusal(&secondo) || !secondo.contains("# ") {
            return Err(format!(
                "Il modello «{}» non produce un documento nel formato richiesto. \
Provane un altro dalle impostazioni: i modelli piccoli o pensati per il codice \
spesso non reggono documenti lunghi.",
                self.settings.model
            ));
        }
        Ok(secondo)
    }

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
        prefill: Option<&str>,
    ) -> Result<String, String> {
        let mut accumulato = String::new();
        let mut ultimo = std::time::Instant::now();

        let mut testo = provider::stream(
            Request {
                provider: self.settings.provider,
                base_url: &self.settings.base_url,
                api_key: &self.api_key,
                model: &self.settings.model,
                system,
                user,
                max_tokens,
                prefill,
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
                            words: accumulato.split_whitespace().count(),
                            preview: accumulato.clone(),
                        },
                    );
                }
            },
        )?;

        // Il prefill non torna nella risposta: va rimesso davanti.
        if let Some(prefill) = prefill {
            testo = format!("{prefill}{testo}");
        }

        let _ = self.app.emit(
            "analysis://progress",
            AnalysisProgress {
                phase,
                step,
                steps,
                words: testo.split_whitespace().count(),
                preview: testo.clone(),
            },
        );
        Ok(testo)
    }
}

/// Stima grossolana del lavoro richiesto, per dire in anticipo quante
/// chiamate serviranno e quanto testo verrà inviato al fornitore.
#[derive(Serialize)]
pub struct AnalysisEstimate {
    characters: usize,
    chunks: usize,
    calls: usize,
}

const SYSTEM_DOMANDA: &str = "Rispondi a domande su una trascrizione, in italiano.

Regole:
- Rispondi solo con quanto risulta dalla trascrizione. Se non c'e, dillo chiaramente \
  invece di ipotizzare.
- Cita i passaggi rilevanti fra virgolette, indicando chi li ha detti.
- Sii diretto: poche righe se la domanda e semplice, di piu se serve.
- La trascrizione e automatica e contiene errori: ignora le parole incomprensibili.
- Niente premesse né commenti sul tuo lavoro.";

/// Domanda libera sulla trascrizione, con risposta in streaming.
#[tauri::command]
pub async fn ask_transcript(
    app: AppHandle,
    lines: Vec<TranscriptLine>,
    question: String,
) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
        if question.trim().is_empty() {
            return Err("Scrivi una domanda.".into());
        }

        let session = Session {
            settings: settings::load(&app),
            api_key: settings::api_key(&app),
            app: &app,
        };

        // Su trascrizioni molto lunghe si tiene inizio e fine: la finestra dei
        // modelli è ampia ma non infinita.
        let transcript = render_transcript(&lines);
        let caratteri: Vec<char> = transcript.chars().collect();
        let contesto = if caratteri.len() <= 60_000 {
            transcript
        } else {
            let testa: String = caratteri[..30_000].iter().collect();
            let coda: String = caratteri[caratteri.len() - 30_000..].iter().collect();
            format!("{testa}\n[…parte centrale omessa…]\n{coda}")
        };

        session.ask(
            SYSTEM_DOMANDA,
            &format!("Trascrizione:\n\n{contesto}\n\nDomanda: {}", question.trim()),
            "writing",
            0,
            1,
            2_000,
            None,
        )
    })
    .await
    .map_err(|cause| format!("Richiesta interrotta: {cause}"))?
}

#[tauri::command]
pub fn estimate_analysis(lines: Vec<TranscriptLine>) -> AnalysisEstimate {
    let caratteri: usize = lines
        .iter()
        .map(|line| line.speaker.chars().count() + line.text.chars().count() + 2)
        .sum();

    let chunks = if caratteri <= SINGLE_PASS_CHARS {
        0
    } else {
        caratteri.div_ceil(CHUNK_CHARS)
    };

    AnalysisEstimate {
        characters: caratteri,
        chunks,
        // Una per la classificazione, una per blocco, una per il documento,
        // una per l'intestazione.
        calls: 1 + chunks + 1 + 1,
    }
}

#[tauri::command]
pub async fn analyze_session(
    app: AppHandle,
    lines: Vec<TranscriptLine>,
    context: Option<SessionContext>,
) -> Result<Analysis, String> {
    tauri::async_runtime::spawn_blocking(move || {
        analyze_blocking(app, lines, context.unwrap_or_default())
    })
        .await
        .map_err(|cause| format!("Analisi interrotta: {cause}"))?
}

fn analyze_blocking(
    app: AppHandle,
    lines: Vec<TranscriptLine>,
    context: SessionContext,
) -> Result<Analysis, String> {
    if lines.is_empty() {
        return Err("Non c'è nulla da analizzare: la trascrizione è vuota.".into());
    }

    let session = Session {
        settings: settings::load(&app),
        api_key: settings::api_key(&app),
        app: &app,
    };

    let transcript = format!("{}{}", context_header(&context), render_transcript(&lines));

    // Riconosce il tipo di conversazione prima di scrivere: il taglio del
    // documento dipende da quello, e con «Auto» è l'unico modo per sceglierlo.
    let mut kind = "meeting".to_string();
    // Guarda inizio e fine: le riunioni cominciano con convenevoli che non
    // dicono nulla sul contenuto, e la natura vera emerge più avanti.
    let campione = {
        let caratteri: Vec<char> = transcript.chars().collect();
        if caratteri.len() <= 6000 {
            transcript.clone()
        } else {
            let testa: String = caratteri[..3000].iter().collect();
            let coda: String = caratteri[caratteri.len() - 3000..].iter().collect();
            format!("{testa}\n[…]\n{coda}")
        }
    };

    if let Ok(risposta) = session.ask(
        SYSTEM_CLASSIFICA,
        &campione,
        "reading",
        0,
        1,
        30,
        Some("TIPO: "),
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
        session.write_report(&system_report, &transcript, 0, 1, report_tokens)?
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
                None,
            )?;
            parziali.push_str(&testo);
            parziali.push_str("\n\n");
        }

        session.write_report(
            &system_report,
            &format!("Note prese durante la riunione, in ordine cronologico:\n\n{parziali}"),
            totale,
            totale + 1,
            report_tokens,
        )?
    };

    let report = clean_report(&report);

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
        "titling",
        0,
        1,
        200,
        Some("TIPO: "),
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

#[cfg(test)]
mod pulizia {
    use super::*;

    #[test]
    fn taglia_la_premessa_prima_del_titolo() {
        let raw = "Capisco. Posso fornirti un modello di documento.\n\n\
Di seguito trovi una bozza formattata.\n\n\
# Riunione sul gestionale\n\n## Quadro generale\nTesto.";
        let pulito = clean_report(raw);
        assert!(pulito.starts_with("# Riunione sul gestionale"));
        assert!(!pulito.contains("Capisco"));
    }

    #[test]
    fn taglia_le_offerte_finali() {
        let raw = "# Titolo\n\n## Sezione\nContenuto vero.\n\n\
Se vuoi, posso convertire questa bozza in una versione finale.\n\
Dimmi se vuoi che proceda.";
        let pulito = clean_report(raw);
        assert!(pulito.ends_with("Contenuto vero."));
        assert!(!pulito.contains("Se vuoi"));
    }

    #[test]
    fn lascia_intatto_un_documento_pulito() {
        let raw = "# Titolo\n\n## Quadro generale\nContenuto.\n\n## Rischi\nAltro contenuto.";
        assert_eq!(clean_report(raw), raw);
    }
}

#[cfg(test)]
mod rifiuti {
    use super::*;

    #[test]
    fn riconosce_il_rifiuto() {
        assert!(is_refusal(
            "Mi serve accesso agli strumenti esterni per generare il documento."
        ));
        assert!(is_refusal(
            "Attualmente non posso eseguire quell'elaborazione senza le API richieste."
        ));
        assert!(is_refusal("Se vuoi, posso fornirti un modello strutturato."));
    }

    #[test]
    fn non_scambia_un_documento_per_rifiuto() {
        assert!(!is_refusal(
            "# Riunione sul gestionale\n\n## Quadro generale\nSi è parlato di API e strumenti esterni."
        ));
    }
}

#[cfg(test)]
mod conversazione {
    use super::*;

    #[test]
    fn riconosce_le_aperture_da_chat() {
        assert!(is_conversational(
            "Ciao Giacomo! Bello argomento: AI e matematica sta cambiando parecchio."
        ));
        assert!(is_conversational("Ecco una sintesi chiara e utile:"));
        assert!(is_conversational("Certo, procedo subito."));
    }

    #[test]
    fn non_scambia_un_documento_per_chat() {
        assert!(!is_conversational(
            "# Intelligenza artificiale e matematica\n\n## Quadro generale\nLa discussione…"
        ));
    }

    #[test]
    fn ripulisce_una_premessa_conversazionale() {
        let raw = "Ciao Giacomo! Ecco una sintesi utile:\n\n# Il documento\n\n## Sezione\nTesto.";
        assert!(clean_report(raw).starts_with("# Il documento"));
    }
}
