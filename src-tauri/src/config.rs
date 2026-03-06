use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

use crate::error::MisterTError;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderType {
    LibreTranslate,
    DeepL,
}

impl Default for ProviderType {
    fn default() -> Self {
        Self::LibreTranslate
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    #[serde(default = "default_provider")]
    pub provider: ProviderType,
    #[serde(default = "default_libre_url")]
    pub libre_translate_url: String,
    #[serde(default)]
    pub deepl_api_key: Option<String>,
    #[serde(default = "default_hotkey")]
    pub hotkey: String,
    #[serde(default = "default_target_lang")]
    pub default_target_language: String,
    #[serde(default = "default_source_lang")]
    pub default_source_language: String,
    #[serde(default = "default_injection_delay")]
    pub injection_delay_ms: u64,
}

fn default_provider() -> ProviderType {
    ProviderType::LibreTranslate
}
fn default_libre_url() -> String {
    "https://libretranslate.com".into()
}
fn default_hotkey() -> String {
    "CmdOrCtrl+Alt+T".into()
}
fn default_target_lang() -> String {
    "de".into()
}
fn default_source_lang() -> String {
    "auto".into()
}
fn default_injection_delay() -> u64 {
    100
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            provider: default_provider(),
            libre_translate_url: default_libre_url(),
            deepl_api_key: None,
            hotkey: default_hotkey(),
            default_target_language: default_target_lang(),
            default_source_language: default_source_lang(),
            injection_delay_ms: default_injection_delay(),
        }
    }
}

impl AppConfig {
    pub fn config_dir() -> PathBuf {
        dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("mister-t")
    }

    pub fn config_path() -> PathBuf {
        Self::config_dir().join("config.json")
    }

    pub fn load() -> Result<Self, MisterTError> {
        let path = Self::config_path();
        if path.exists() {
            let data = fs::read_to_string(&path)?;
            let config: AppConfig = serde_json::from_str(&data)?;
            Ok(config)
        } else {
            let config = Self::default();
            config.save()?;
            Ok(config)
        }
    }

    pub fn save(&self) -> Result<(), MisterTError> {
        let dir = Self::config_dir();
        fs::create_dir_all(&dir)?;
        let data = serde_json::to_string_pretty(self)?;
        fs::write(Self::config_path(), data)?;
        Ok(())
    }
}
