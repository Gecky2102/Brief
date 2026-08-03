use std::io::{BufRead, BufReader};

use serde::{Deserialize, Serialize};

/// Fornitori supportati. OpenRouter e «compatibile» parlano lo stesso dialetto
/// di OpenAI, quindi condividono lo stesso percorso di codice.
#[derive(Clone, Copy, PartialEq, Serialize, Deserialize, Default, Debug)]
#[serde(rename_all = "lowercase")]
pub enum Provider {
    #[default]
    Anthropic,
    Openai,
    Google,
    Openrouter,
    Compatible,
}

impl Provider {
    pub fn label(&self) -> &'static str {
        match self {
            Provider::Anthropic => "Anthropic",
            Provider::Openai => "OpenAI",
            Provider::Google => "Google",
            Provider::Openrouter => "OpenRouter",
            Provider::Compatible => "Compatibile OpenAI",
        }
    }

    pub fn default_model(&self) -> &'static str {
        match self {
            Provider::Anthropic => "claude-sonnet-5",
            Provider::Openai => "gpt-5",
            Provider::Google => "gemini-2.5-flash",
            Provider::Openrouter => "anthropic/claude-sonnet-5",
            Provider::Compatible => "",
        }
    }

    fn endpoint(&self, base_url: &str, model: &str) -> String {
        let base = base_url.trim_end_matches('/');
        match self {
            Provider::Anthropic => {
                let base = if base.is_empty() { "https://api.anthropic.com" } else { base };
                format!("{base}/v1/messages")
            }
            Provider::Openai => {
                let base = if base.is_empty() { "https://api.openai.com/v1" } else { base };
                format!("{base}/chat/completions")
            }
            Provider::Openrouter => {
                let base = if base.is_empty() { "https://openrouter.ai/api/v1" } else { base };
                format!("{base}/chat/completions")
            }
            Provider::Compatible => format!("{base}/chat/completions"),
            Provider::Google => {
                let base = if base.is_empty() {
                    "https://generativelanguage.googleapis.com/v1beta"
                } else {
                    base
                };
                format!("{base}/models/{model}:streamGenerateContent?alt=sse")
            }
        }
    }
}

pub struct Request<'a> {
    pub provider: Provider,
    pub base_url: &'a str,
    pub api_key: &'a str,
    pub model: &'a str,
    pub system: &'a str,
    pub user: &'a str,
    pub max_tokens: u32,
    /// Testo messo in bocca al modello come inizio della risposta. È il modo
    /// più efficace per impedire premesse: se la risposta comincia già con
    /// «# », non può esserci un «Capisco, posso fornirti…» prima.
    pub prefill: Option<&'a str>,
}

/// Esegue la richiesta in streaming e invoca `on_delta` a ogni pezzo di testo.
/// Restituisce il testo completo.
pub fn stream(request: Request, mut on_delta: impl FnMut(&str)) -> Result<String, String> {
    if request.api_key.trim().is_empty() {
        return Err("Manca la chiave API: impostala nelle impostazioni.".into());
    }
    if request.model.trim().is_empty() {
        return Err("Manca il nome del modello: impostalo nelle impostazioni.".into());
    }

    let url = request.provider.endpoint(request.base_url, request.model);
    let body = build_body(&request);

    // Un errore momentaneo del fornitore, dopo minuti di elaborazione, non deve
    // buttare via tutto: si riprova con attesa crescente.
    let mut tentativi = 0;
    let response = loop {
        tentativi += 1;
        let esito = match request.provider {
            Provider::Anthropic => ureq::post(&url)
                .header("content-type", "application/json")
                .header("x-api-key", request.api_key)
                .header("anthropic-version", "2023-06-01")
                .send_json(&body),
            Provider::Google => ureq::post(&url)
                .header("content-type", "application/json")
                .header("x-goog-api-key", request.api_key)
                .send_json(&body),
            _ => ureq::post(&url)
                .header("content-type", "application/json")
                .header("accept", "text/event-stream")
                .header("authorization", &format!("Bearer {}", request.api_key))
                .send_json(&body),
        };

        match esito {
            Ok(risposta) => break risposta,
            Err(errore) if tentativi < 3 && is_temporary(&errore) => {
                std::thread::sleep(std::time::Duration::from_secs(2 * tentativi));
            }
            Err(errore) => return Err(describe_error(errore, request.provider)),
        }
    };

    let tipo_contenuto = response
        .headers()
        .get("content-type")
        .and_then(|valore| valore.to_str().ok())
        .unwrap_or("")
        .to_string();

    // Alcuni gateway ignorano la richiesta di streaming e rispondono con un
    // JSON unico: va letto in un modo diverso, altrimenti sembra vuoto.
    if !tipo_contenuto.contains("event-stream") {
        let testo = response
            .into_body()
            .read_to_string()
            .map_err(|cause| format!("Risposta non leggibile: {cause}"))?;

        let valore: serde_json::Value = serde_json::from_str(&testo)
            .map_err(|_| "Il servizio ha risposto in un formato inatteso.".to_string())?;

        let completo = extract_complete(&valore, request.provider)
            .ok_or_else(|| "Il modello non ha restituito nulla.".to_string())?;
        on_delta(&completo);
        return Ok(completo);
    }

    let reader = BufReader::new(response.into_body().into_reader());
    let mut full = String::new();

    for line in reader.lines() {
        let line = line.map_err(|cause| format!("Connessione interrotta: {cause}"))?;
        let Some(payload) = line.strip_prefix("data:") else {
            continue;
        };
        let payload = payload.trim();
        if payload.is_empty() || payload == "[DONE]" {
            continue;
        }
        let Ok(value) = serde_json::from_str::<serde_json::Value>(payload) else {
            continue;
        };

        if let Some(delta) = extract_delta(&value, request.provider) {
            if !delta.is_empty() {
                full.push_str(&delta);
                on_delta(&delta);
            }
        }
    }

    if full.trim().is_empty() {
        return Err("Il modello non ha restituito nulla.".into());
    }
    Ok(full)
}

fn build_body(request: &Request) -> serde_json::Value {
    match request.provider {
        Provider::Anthropic => {
            let mut messages = vec![serde_json::json!({
                "role": "user", "content": request.user
            })];
            if let Some(prefill) = request.prefill {
                messages.push(serde_json::json!({
                    "role": "assistant", "content": prefill
                }));
            }
            serde_json::json!({
                "model": request.model,
                "max_tokens": request.max_tokens,
                "stream": true,
                "system": request.system,
                "messages": messages,
            })
        }
        Provider::Google => serde_json::json!({
            "systemInstruction": {"parts": [{"text": request.system}]},
            "contents": [{"role": "user", "parts": [{"text": request.user}]}],
            "generationConfig": {"maxOutputTokens": request.max_tokens},
        }),
        _ => {
            let mut messages = vec![
                serde_json::json!({"role": "system", "content": request.system}),
                serde_json::json!({"role": "user", "content": request.user}),
            ];
            if let Some(prefill) = request.prefill {
                messages.push(serde_json::json!({
                    "role": "assistant", "content": prefill
                }));
            }
            // I gateway compatibili accettano l'uno o l'altro campo a seconda
            // della versione: mandarli entrambi evita risposte troncate.
            serde_json::json!({
                "model": request.model,
                "max_completion_tokens": request.max_tokens,
                "max_tokens": request.max_tokens,
                "stream": true,
                "messages": messages,
            })
        }
    }
}

/// Estrae il testo da una risposta non in streaming.
fn extract_complete(valore: &serde_json::Value, provider: Provider) -> Option<String> {
    match provider {
        Provider::Anthropic => valore
            .get("content")?
            .get(0)?
            .get("text")?
            .as_str()
            .map(str::to_string),
        Provider::Google => valore
            .get("candidates")?
            .get(0)?
            .get("content")?
            .get("parts")?
            .get(0)?
            .get("text")?
            .as_str()
            .map(str::to_string),
        _ => valore
            .get("choices")?
            .get(0)?
            .get("message")?
            .get("content")?
            .as_str()
            .map(str::to_string),
    }
}

fn extract_delta(value: &serde_json::Value, provider: Provider) -> Option<String> {
    match provider {
        Provider::Anthropic => value
            .get("delta")
            .and_then(|delta| delta.get("text"))
            .and_then(|text| text.as_str())
            .map(str::to_string),
        Provider::Google => value
            .get("candidates")?
            .get(0)?
            .get("content")?
            .get("parts")?
            .get(0)?
            .get("text")?
            .as_str()
            .map(str::to_string),
        _ => value
            .get("choices")?
            .get(0)?
            .get("delta")?
            .get("content")?
            .as_str()
            .map(str::to_string),
    }
}

/// Un errore che ha senso riprovare: limite di frequenza o guasto passeggero.
fn is_temporary(errore: &ureq::Error) -> bool {
    match errore {
        ureq::Error::StatusCode(429) => true,
        ureq::Error::StatusCode(code) => (500..600).contains(code),
        ureq::Error::Io(_) => true,
        _ => false,
    }
}

/// I messaggi dei fornitori sono in inglese e spesso criptici: qui diventano
/// istruzioni su cosa fare, senza mai riportare la chiave.
fn describe_error(error: ureq::Error, provider: Provider) -> String {
    let etichetta = provider.label();
    match error {
        ureq::Error::StatusCode(401) | ureq::Error::StatusCode(403) => {
            format!("{etichetta} ha rifiutato la chiave API: controllala nelle impostazioni.")
        }
        ureq::Error::StatusCode(404) => {
            format!("{etichetta} non conosce questo modello: controlla il nome nelle impostazioni.")
        }
        ureq::Error::StatusCode(429) => {
            format!("{etichetta} ha applicato un limite di frequenza: riprova fra poco.")
        }
        ureq::Error::StatusCode(code) if (500..600).contains(&code) => {
            format!("{etichetta} ha un problema momentaneo (errore {code}): riprova fra poco.")
        }
        ureq::Error::StatusCode(code) => format!("{etichetta} ha risposto con errore {code}."),
        other => format!("Impossibile contattare {etichetta}: {other}"),
    }
}
