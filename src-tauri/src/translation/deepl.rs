use async_trait::async_trait;
use reqwest::Client;
use serde::Deserialize;

use super::provider::{Language, TranslationProvider, TranslationResult};

pub struct DeepLProvider {
    client: Client,
    api_key: String,
}

#[derive(Deserialize)]
struct DeepLResponse {
    translations: Vec<DeepLTranslation>,
}

#[derive(Deserialize)]
struct DeepLTranslation {
    text: String,
    detected_source_language: Option<String>,
}

#[derive(Deserialize)]
struct DeepLLanguageResponse {
    language: String,
    name: String,
}

impl DeepLProvider {
    pub fn new(api_key: String) -> Self {
        Self {
            client: Client::new(),
            api_key,
        }
    }

    fn base_url(&self) -> &str {
        if self.api_key.ends_with(":fx") {
            "https://api-free.deepl.com/v2"
        } else {
            "https://api.deepl.com/v2"
        }
    }
}

#[async_trait]
impl TranslationProvider for DeepLProvider {
    fn id(&self) -> &str {
        "deepl"
    }

    fn display_name(&self) -> &str {
        "DeepL"
    }

    async fn translate(
        &self,
        text: &str,
        source: &str,
        target: &str,
    ) -> Result<TranslationResult, Box<dyn std::error::Error + Send + Sync>> {
        let mut params = vec![
            ("text", text.to_string()),
            ("target_lang", target.to_uppercase()),
        ];

        if source != "auto" {
            params.push(("source_lang", source.to_uppercase()));
        }

        let resp: DeepLResponse = self
            .client
            .post(format!("{}/translate", self.base_url()))
            .header("Authorization", format!("DeepL-Auth-Key {}", self.api_key))
            .form(&params)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;

        let first = resp
            .translations
            .first()
            .ok_or("No translation returned")?;

        Ok(TranslationResult {
            primary: first.text.clone(),
            alternatives: vec![],
            detected_language: first.detected_source_language.clone(),
        })
    }

    async fn supported_languages(
        &self,
    ) -> Result<Vec<Language>, Box<dyn std::error::Error + Send + Sync>> {
        let resp: Vec<DeepLLanguageResponse> = self
            .client
            .get(format!("{}/languages", self.base_url()))
            .header("Authorization", format!("DeepL-Auth-Key {}", self.api_key))
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;

        Ok(resp
            .into_iter()
            .map(|l| Language {
                code: l.language.to_lowercase(),
                name: l.name,
            })
            .collect())
    }

    fn requires_api_key(&self) -> bool {
        true
    }
}
