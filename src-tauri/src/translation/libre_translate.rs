use async_trait::async_trait;
use reqwest::Client;
use serde::Deserialize;

use super::provider::{Language, TranslationProvider, TranslationResult};

pub struct LibreTranslateProvider {
    client: Client,
    base_url: String,
    api_key: Option<String>,
}

#[derive(Deserialize)]
struct LTTranslateResponse {
    #[serde(rename = "translatedText")]
    translated_text: String,
    #[serde(default)]
    alternatives: Vec<String>,
    #[serde(rename = "detectedLanguage", default)]
    detected_language: Option<LTDetectedLanguage>,
}

#[derive(Deserialize)]
struct LTDetectedLanguage {
    language: String,
}

#[derive(Deserialize)]
struct LTLanguage {
    code: String,
    name: String,
}

impl LibreTranslateProvider {
    pub fn new(base_url: String, api_key: Option<String>) -> Self {
        Self {
            client: Client::new(),
            base_url: base_url.trim_end_matches('/').to_string(),
            api_key,
        }
    }
}

#[async_trait]
impl TranslationProvider for LibreTranslateProvider {
    fn id(&self) -> &str {
        "libretranslate"
    }

    fn display_name(&self) -> &str {
        "LibreTranslate"
    }

    async fn translate(
        &self,
        text: &str,
        source: &str,
        target: &str,
    ) -> Result<TranslationResult, Box<dyn std::error::Error + Send + Sync>> {
        let mut body = serde_json::json!({
            "q": text,
            "source": source,
            "target": target,
            "format": "text",
            "alternatives": 3
        });

        if let Some(ref key) = self.api_key {
            body["api_key"] = serde_json::Value::String(key.clone());
        }

        let resp: LTTranslateResponse = self
            .client
            .post(format!("{}/translate", self.base_url))
            .json(&body)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;

        Ok(TranslationResult {
            primary: resp.translated_text,
            alternatives: resp.alternatives,
            detected_language: resp.detected_language.map(|d| d.language),
        })
    }

    async fn supported_languages(
        &self,
    ) -> Result<Vec<Language>, Box<dyn std::error::Error + Send + Sync>> {
        let resp: Vec<LTLanguage> = self
            .client
            .get(format!("{}/languages", self.base_url))
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;

        Ok(resp
            .into_iter()
            .map(|l| Language {
                code: l.code,
                name: l.name,
            })
            .collect())
    }

    fn requires_api_key(&self) -> bool {
        false
    }
}
