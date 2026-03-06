use async_trait::async_trait;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranslationResult {
    pub primary: String,
    pub alternatives: Vec<String>,
    pub detected_language: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Language {
    pub code: String,
    pub name: String,
}

#[async_trait]
pub trait TranslationProvider: Send + Sync {
    fn id(&self) -> &str;
    fn display_name(&self) -> &str;

    async fn translate(
        &self,
        text: &str,
        source: &str,
        target: &str,
    ) -> Result<TranslationResult, Box<dyn std::error::Error + Send + Sync>>;

    async fn supported_languages(
        &self,
    ) -> Result<Vec<Language>, Box<dyn std::error::Error + Send + Sync>>;

    fn requires_api_key(&self) -> bool;
}
