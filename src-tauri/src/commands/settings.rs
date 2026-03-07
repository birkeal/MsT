use crate::config::AppConfig;
use crate::error::MstError;

#[tauri::command]
pub fn load_settings() -> Result<AppConfig, MstError> {
    AppConfig::load()
}

#[tauri::command]
pub fn save_settings(config: AppConfig) -> Result<(), MstError> {
    config.save()
}
