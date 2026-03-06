use crate::config::AppConfig;
use crate::error::MisterTError;

#[tauri::command]
pub fn load_settings() -> Result<AppConfig, MisterTError> {
    AppConfig::load()
}

#[tauri::command]
pub fn save_settings(config: AppConfig) -> Result<(), MisterTError> {
    config.save()
}
