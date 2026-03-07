use reqwest::Client;
use serde::Deserialize;

use crate::config::{AppConfig, TranslationType};
use crate::error::MstError;

/// Remove query-string parameters that may contain secrets.
fn strip_secrets(msg: &str) -> String {
    // Replace ?key=... or &key=... parameter values
    let re_key = regex_lite::Regex::new(r"([?&])(key|api_key|token|secret)=[^&\s)]+").unwrap();
    re_key.replace_all(msg, "$1$2=***").to_string()
}

#[derive(Debug, Clone)]
pub struct TranslationResult {
    pub primary: String,
    pub alternatives: Vec<String>,
}

pub async fn translate(
    config: &AppConfig,
    text: &str,
    source: &str,
    target: &str,
) -> Result<TranslationResult, MstError> {
    match config.translation_type {
        TranslationType::Simple => simple_translate(config, text, source, target).await,
        TranslationType::Ai => ai_translate(config, text, source, target).await,
    }
}

// --- Simple translation (MyMemory-compatible REST API) ---

#[derive(Deserialize)]
struct SimpleResponse {
    #[serde(rename = "responseData")]
    response_data: SimpleResponseData,
    #[serde(default, deserialize_with = "deserialize_matches")]
    matches: Vec<SimpleMatch>,
}

#[derive(Deserialize)]
struct SimpleResponseData {
    #[serde(rename = "translatedText")]
    translated_text: String,
}

#[derive(Deserialize)]
struct SimpleMatch {
    translation: String,
}

/// MyMemory returns `matches` as `""` (empty string) when there are no
/// matches, but as an array when there are. Handle both.
fn deserialize_matches<'de, D>(deserializer: D) -> Result<Vec<SimpleMatch>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde_json::Value;

    let v = Value::deserialize(deserializer)?;
    match v {
        Value::Array(arr) => {
            let mut out = Vec::new();
            for item in arr {
                if let Ok(m) = serde_json::from_value(item) {
                    out.push(m);
                }
            }
            Ok(out)
        }
        _ => Ok(vec![]),
    }
}

async fn simple_translate(
    config: &AppConfig,
    text: &str,
    source: &str,
    target: &str,
) -> Result<TranslationResult, MstError> {
    if source.eq_ignore_ascii_case(target) {
        return Err(MstError::Translation(
            "Source and target language must be different for simple translation".into(),
        ));
    }

    let client = Client::new();
    let langpair = format!("{}|{}", source, target);

    let mut req = client.get(&config.service_url).query(&[
        ("q", text),
        ("langpair", &langpair),
    ]);

    if let Some(ref key) = config.api_key {
        req = req.query(&[("key", key.as_str())]);
    }

    let resp: SimpleResponse = req
        .send()
        .await
        .map_err(|e| MstError::Translation(e.to_string()))?
        .json()
        .await
        .map_err(|e| MstError::Translation(e.to_string()))?;

    let primary = resp.response_data.translated_text.clone();
    let alternatives: Vec<String> = resp
        .matches
        .into_iter()
        .map(|m| m.translation)
        .filter(|t| !t.eq_ignore_ascii_case(&primary) && !t.is_empty())
        .collect();

    Ok(TranslationResult {
        primary,
        alternatives,
    })
}

// --- AI translation (OpenAI / Anthropic / Gemini) ---

const DEFAULT_AI_PROMPT: &str = "\
You are a translation service. Translate the following text into {target}. \
Provide up to 3 possible translations ranked by quality. \
Return ONLY a JSON array of strings, e.g. [\"translation1\", \"translation2\"]. \
No explanation, no markdown, just the JSON array.\n\n{text}";

fn build_ai_prompt(config: &AppConfig, text: &str, target: &str) -> String {
    let template = config
        .prompt
        .as_deref()
        .filter(|p| !p.is_empty())
        .unwrap_or(DEFAULT_AI_PROMPT);

    template
        .replace("{text}", text)
        .replace("{target}", target)
}

async fn ai_translate(
    config: &AppConfig,
    text: &str,
    _source: &str,
    target: &str,
) -> Result<TranslationResult, MstError> {
    let api_key = config
        .api_key
        .as_deref()
        .filter(|k| !k.is_empty())
        .ok_or_else(|| MstError::Translation("api_key is required for AI translation".into()))?;

    let model = config
        .model
        .as_deref()
        .filter(|m| !m.is_empty())
        .ok_or_else(|| MstError::Translation("model is required for AI translation".into()))?;

    let prompt = build_ai_prompt(config, text, target);

    let url = &config.service_url;
    let client = Client::new();

    let raw = if url.contains("anthropic") {
        anthropic_request(&client, url, api_key, model, &prompt).await?
    } else if url.contains("googleapis") || url.contains("gemini") {
        gemini_request(&client, url, api_key, model, &prompt).await?
    } else {
        // Default: OpenAI-compatible API
        openai_request(&client, url, api_key, model, &prompt).await?
    };

    parse_ai_response(&raw)
}

/// Try to parse the AI response as a JSON array of strings.
/// Falls back to treating the whole response as a single translation.
fn parse_ai_response(raw: &str) -> Result<TranslationResult, MstError> {
    let trimmed = raw.trim();

    // Strip markdown code fences if present
    let json_str = if trimmed.starts_with("```") {
        trimmed
            .trim_start_matches("```json")
            .trim_start_matches("```")
            .trim_end_matches("```")
            .trim()
    } else {
        trimmed
    };

    if let Ok(arr) = serde_json::from_str::<Vec<String>>(json_str) {
        if let Some(primary) = arr.first() {
            return Ok(TranslationResult {
                primary: primary.clone(),
                alternatives: arr.into_iter().skip(1).collect(),
            });
        }
    }

    // Fallback: single translation
    Ok(TranslationResult {
        primary: trimmed.to_string(),
        alternatives: vec![],
    })
}

/// Try to extract a human-readable error message from a JSON API error body.
fn extract_api_error(body: &str) -> String {
    // Most APIs return {"error": {"message": "..."}} or {"error": {"type": "...", "message": "..."}}
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(body) {
        if let Some(msg) = v["error"]["message"].as_str() {
            return msg.to_string();
        }
        if let Some(msg) = v["error"].as_str() {
            return msg.to_string();
        }
        if let Some(msg) = v["message"].as_str() {
            return msg.to_string();
        }
    }
    // Fallback: truncate raw body
    let cleaned = strip_secrets(body);
    if cleaned.len() > 200 {
        format!("{}…", &cleaned[..200])
    } else {
        cleaned
    }
}

async fn openai_request(
    client: &Client,
    url: &str,
    api_key: &str,
    model: &str,
    prompt: &str,
) -> Result<String, MstError> {
    let body = serde_json::json!({
        "model": model,
        "messages": [{"role": "user", "content": prompt}],
        "temperature": 0.3
    });

    #[derive(Deserialize)]
    struct Resp {
        choices: Vec<Choice>,
    }
    #[derive(Deserialize)]
    struct Choice {
        message: Msg,
    }
    #[derive(Deserialize)]
    struct Msg {
        content: String,
    }

    let response = client
        .post(url)
        .header("Authorization", format!("Bearer {api_key}"))
        .json(&body)
        .send()
        .await
        .map_err(|e| MstError::Translation(e.to_string()))?;

    if !response.status().is_success() {
        let status = response.status();
        let body_text = response.text().await.unwrap_or_default();
        log::error!("OpenAI API error {status}: {body_text}");
        let detail = extract_api_error(&body_text);
        return Err(MstError::Translation(format!(
            "OpenAI API error ({status}): {detail}"
        )));
    }

    let resp: Resp = response
        .json()
        .await
        .map_err(|e| MstError::Translation(e.to_string()))?;

    resp.choices
        .into_iter()
        .next()
        .map(|c| c.message.content)
        .ok_or_else(|| MstError::Translation("Empty response from AI".into()))
}

async fn anthropic_request(
    client: &Client,
    url: &str,
    api_key: &str,
    model: &str,
    prompt: &str,
) -> Result<String, MstError> {
    let body = serde_json::json!({
        "model": model,
        "max_tokens": 1024,
        "messages": [{"role": "user", "content": prompt}]
    });

    #[derive(Deserialize)]
    struct Resp {
        content: Vec<ContentBlock>,
    }
    #[derive(Deserialize)]
    struct ContentBlock {
        text: String,
    }

    let response = client
        .post(url)
        .header("x-api-key", api_key)
        .header("anthropic-version", "2023-06-01")
        .header("content-type", "application/json")
        .json(&body)
        .send()
        .await
        .map_err(|e| MstError::Translation(e.to_string()))?;

    if !response.status().is_success() {
        let status = response.status();
        let body_text = response.text().await.unwrap_or_default();
        log::error!("Anthropic API error {status}: {body_text}");
        let detail = extract_api_error(&body_text);
        return Err(MstError::Translation(format!(
            "Anthropic API error ({status}): {detail}"
        )));
    }

    let resp: Resp = response
        .json()
        .await
        .map_err(|e| MstError::Translation(e.to_string()))?;

    resp.content
        .into_iter()
        .next()
        .map(|c| c.text)
        .ok_or_else(|| MstError::Translation("Empty response from AI".into()))
}

async fn gemini_request(
    client: &Client,
    url: &str,
    api_key: &str,
    model: &str,
    prompt: &str,
) -> Result<String, MstError> {
    let endpoint = format!("{}/{}:generateContent?key={}", url, model, api_key);

    let body = serde_json::json!({
        "contents": [{"parts": [{"text": prompt}]}]
    });

    #[derive(Deserialize)]
    struct Resp {
        candidates: Vec<Candidate>,
    }
    #[derive(Deserialize)]
    struct Candidate {
        content: CandidateContent,
    }
    #[derive(Deserialize)]
    struct CandidateContent {
        parts: Vec<Part>,
    }
    #[derive(Deserialize)]
    struct Part {
        text: String,
    }

    let response = client
        .post(&endpoint)
        .header("content-type", "application/json")
        .json(&body)
        .send()
        .await
        .map_err(|e| MstError::Translation(e.to_string()))?;

    if !response.status().is_success() {
        let status = response.status();
        let body_text = response.text().await.unwrap_or_default();
        log::error!("Gemini API error {status}: {body_text}");
        let detail = extract_api_error(&body_text);
        return Err(MstError::Translation(format!(
            "Gemini API error ({status}): {detail}"
        )));
    }

    let resp: Resp = response
        .json()
        .await
        .map_err(|e| MstError::Translation(e.to_string()))?;

    resp.candidates
        .into_iter()
        .next()
        .and_then(|c| c.content.parts.into_iter().next())
        .map(|p| p.text)
        .ok_or_else(|| MstError::Translation("Empty response from AI".into()))
}
