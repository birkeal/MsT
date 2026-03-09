use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

use crate::error::MstError;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TranslationType {
    #[default]
    Simple,
    Ai,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    #[serde(default)]
    pub translation_type: TranslationType,
    #[serde(default = "default_service_url")]
    pub service_url: String,
    #[serde(default)]
    pub api_key: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub prompt: Option<String>,
    #[serde(default = "default_hotkey")]
    pub hotkey: String,
    #[serde(default = "default_target_lang")]
    pub default_target_language: String,
    #[serde(default = "default_source_lang")]
    pub default_source_language: String,
    #[serde(default = "default_injection_delay")]
    pub injection_delay_ms: u64,
    #[serde(default = "default_hotkey_tap_interval")]
    pub hotkey_tap_interval_ms: u64,
    #[serde(default = "default_selection_hotkey")]
    pub selection_hotkey: Option<String>,
}

fn default_service_url() -> String {
    "https://api.mymemory.translated.net/get".into()
}
fn default_hotkey() -> String {
    "CmdOrCtrl+CmdOrCtrl".into()
}
fn default_target_lang() -> String {
    "en".into()
}
fn default_source_lang() -> String {
    "de".into()
}
fn default_injection_delay() -> u64 {
    100
}
fn default_hotkey_tap_interval() -> u64 {
    300
}
fn default_selection_hotkey() -> Option<String> {
    Some("CmdOrCtrl+CmdOrCtrl".into())
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            translation_type: TranslationType::default(),
            service_url: default_service_url(),
            api_key: None,
            model: None,
            prompt: None,
            hotkey: default_hotkey(),
            default_target_language: default_target_lang(),
            default_source_language: default_source_lang(),
            injection_delay_ms: default_injection_delay(),
            hotkey_tap_interval_ms: default_hotkey_tap_interval(),
            selection_hotkey: default_selection_hotkey(),
        }
    }
}

impl AppConfig {
    pub fn config_dir() -> PathBuf {
        dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("mst")
    }

    pub fn config_path() -> PathBuf {
        Self::config_dir().join("config.json")
    }

    pub fn load() -> Result<Self, MstError> {
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

    pub fn save(&self) -> Result<(), MstError> {
        let dir = Self::config_dir();
        fs::create_dir_all(&dir)?;
        let data = serde_json::to_string_pretty(self)?;
        fs::write(Self::config_path(), data)?;
        Ok(())
    }
}
