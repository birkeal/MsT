pub mod deepl;
pub mod libre_translate;
pub mod provider;

use std::collections::HashMap;
use std::sync::Mutex;

use crate::config::{AppConfig, ProviderType};
use provider::TranslationProvider;

pub struct ProviderRegistry {
    providers: HashMap<String, Box<dyn TranslationProvider>>,
    active_id: Mutex<String>,
}

impl ProviderRegistry {
    pub fn from_config(config: &AppConfig) -> Self {
        let mut providers: HashMap<String, Box<dyn TranslationProvider>> = HashMap::new();

        // Always register LibreTranslate
        providers.insert(
            "libretranslate".into(),
            Box::new(libre_translate::LibreTranslateProvider::new(
                config.libre_translate_url.clone(),
                None,
            )),
        );

        // Register DeepL if API key is configured
        if let Some(ref key) = config.deepl_api_key {
            if !key.is_empty() {
                providers.insert("deepl".into(), Box::new(deepl::DeepLProvider::new(key.clone())));
            }
        }

        let active_id = match config.provider {
            ProviderType::LibreTranslate => "libretranslate",
            ProviderType::DeepL => "deepl",
        };

        Self {
            providers,
            active_id: Mutex::new(active_id.into()),
        }
    }

    pub fn active(&self) -> Option<&dyn TranslationProvider> {
        let id = self.active_id.lock().unwrap();
        self.providers.get(id.as_str()).map(|p| p.as_ref())
    }
}
