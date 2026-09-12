use crate::content_planning::model::PlannerProviderStatus;
use crate::content_planning::provider::ProviderError;

pub const DEFAULT_GEMINI_MODEL: &str = "gemini-2.5-flash";

/// Runtime configuration containing the sensitive API key.
/// Intentionally does NOT derive `Debug` or `Serialize` to prevent secret leakage.
#[derive(Clone, PartialEq, Eq)]
pub struct GeminiRuntimeConfig {
    pub api_key: String,
    pub model: String,
}

pub struct ProviderConfigResolver;

impl ProviderConfigResolver {
    /// Resolves runtime Gemini configuration by reading `GEMINI_API_KEY` from the environment.
    /// Returns `ProviderError::AuthMissing` if the key is missing or empty.
    pub fn resolve_gemini() -> Result<GeminiRuntimeConfig, ProviderError> {
        let api_key = match std::env::var("GEMINI_API_KEY") {
            Ok(key) if !key.trim().is_empty() => key.trim().to_string(),
            _ => {
                return Err(ProviderError::AuthMissing(
                    "GEMINI_API_KEY is not set or empty".to_string(),
                ))
            }
        };

        let model = std::env::var("GEMINI_MODEL")
            .ok()
            .map(|m| m.trim().to_string())
            .filter(|m| !m.is_empty())
            .unwrap_or_else(|| DEFAULT_GEMINI_MODEL.to_string());

        Ok(GeminiRuntimeConfig { api_key, model })
    }

    /// Returns a safe status descriptor indicating whether Gemini is configured.
    /// Never returns or exposes the API key.
    pub fn get_status() -> PlannerProviderStatus {
        let configured = match std::env::var("GEMINI_API_KEY") {
            Ok(key) => !key.trim().is_empty(),
            Err(_) => false,
        };

        let model = std::env::var("GEMINI_MODEL")
            .ok()
            .map(|m| m.trim().to_string())
            .filter(|m| !m.is_empty())
            .unwrap_or_else(|| DEFAULT_GEMINI_MODEL.to_string());

        PlannerProviderStatus {
            provider: "gemini".to_string(),
            configured,
            model,
        }
    }
}
