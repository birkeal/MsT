use serde::Serialize;
use tauri::State;

use crate::config::AppConfig;
use crate::error::MstError;
use crate::translation;

#[derive(Debug, Clone, Serialize)]
pub struct TranslationSuggestion {
    pub text: String,
    pub hint: String,
}

#[tauri::command]
pub async fn translate(
    text: String,
    source: String,
    target: String,
    config: State<'_, AppConfig>,
) -> Result<Vec<TranslationSuggestion>, MstError> {
    let result = match translation::translate(&config, &text, &source, &target).await {
        Ok(r) => r,
        Err(e) => {
            log::error!("Translation failed: {e}");
            return Err(e);
        }
    };

    let mut suggestions = vec![TranslationSuggestion {
        text: result.primary.clone(),
        hint: "best match".into(),
    }];

    for alt in result.alternatives {
        if alt != result.primary {
            suggestions.push(TranslationSuggestion {
                text: alt,
                hint: "alternative".into(),
            });
        }
    }

    Ok(suggestions)
}
