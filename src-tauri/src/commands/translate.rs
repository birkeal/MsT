use serde::Serialize;
use tauri::State;

use crate::error::MisterTError;
use crate::translation::ProviderRegistry;

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
    registry: State<'_, ProviderRegistry>,
) -> Result<Vec<TranslationSuggestion>, MisterTError> {
    let provider = registry
        .active()
        .ok_or_else(|| MisterTError::Translation("No translation provider configured".into()))?;

    let result = provider
        .translate(&text, &source, &target)
        .await
        .map_err(|e| MisterTError::Translation(e.to_string()))?;

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
