//! GeminiProvider: HTTP transport and response classification for Google Gemini API.
//!
//! # Verified REST API Contract
//! - Endpoint: `https://generativelanguage.googleapis.com/v1beta/models/{model}:generateContent`
//! - Auth Header: `x-goog-api-key: {api_key}` (never passed in query params or log output)
//! - Request Format:
//!   ```json
//!   {
//!     "systemInstruction": {
//!       "parts": [{ "text": "<system_prompt>" }]
//!     },
//!     "contents": [
//!       {
//!         "role": "user",
//!         "parts": [{ "text": "<user_prompt>" }]
//!       }
//!     ],
//!     "generationConfig": {
//!       "responseMimeType": "application/json"
//!     }
//!   }
//!   ```
//! - Response Parsing:
//!   - Candidates: `candidates[0].content.parts[0].text`
//!   - Usage: `usageMetadata.promptTokenCount`, `usageMetadata.candidatesTokenCount`, `usageMetadata.totalTokenCount` -> `GenerationUsage`

use serde::{Deserialize, Serialize};

use crate::content_planning::model::GenerationUsage;
use crate::content_planning::provider::config::GeminiRuntimeConfig;
use crate::content_planning::provider::{
    AIProvider, ProviderError, ProviderRequest, ProviderResponse,
};

pub const DEFAULT_GEMINI_BASE_URL: &str = "https://generativelanguage.googleapis.com";

/// Sanitizes error strings by redacting API keys, secret tokens, and auth headers.
/// Also bounds error message length to avoid leaking full prompts or huge payloads.
pub fn sanitize_error_message(msg: &str) -> String {
    // 0. Redact active GEMINI_API_KEY if present in environment
    let mut s0 = msg.to_string();
    if let Ok(key) = std::env::var("GEMINI_API_KEY") {
        let trimmed = key.trim();
        if !trimmed.is_empty() {
            s0 = s0.replace(trimmed, "[REDACTED]");
        }
    }

    // 1. Redact Google API key pattern (AIzaSy...)
    let re_aiza = regex::Regex::new(r"AIza[0-9A-Za-z_-]{35}").unwrap();
    let s1 = re_aiza.replace_all(&s0, "[REDACTED]");

    // 2. Redact key=..., secret=..., token=..., api_key=...
    let re_key =
        regex::Regex::new(r"(?i)\b(api[-_]?key|token|secret|key)\s*[:=]\s*[^\s,;'\x22\}]+")
            .unwrap();
    let s2 = re_key.replace_all(&s1, "$1=[REDACTED]");

    // 3. Redact header values like x-goog-api-key: ...
    let re_hdr =
        regex::Regex::new(r"(?i)(x-goog-api-key|authorization)\s*:\s*[^\r\n,;'\x22\}]+").unwrap();
    let s3 = re_hdr.replace_all(&s2, "$1: [REDACTED]");

    // 4. Safely truncate long messages on char boundary
    if s3.chars().count() > 500 {
        let truncated: String = s3.chars().take(500).collect();
        format!("{}... [truncated]", truncated)
    } else {
        s3.into_owned()
    }
}

#[derive(Deserialize)]
struct GeminiApiErrorDetail {
    message: Option<String>,
}

#[derive(Deserialize)]
struct GeminiApiErrorResponse {
    error: Option<GeminiApiErrorDetail>,
}

fn extract_error_message(status: u16, body: &str) -> String {
    if let Ok(err_resp) = serde_json::from_str::<GeminiApiErrorResponse>(body) {
        if let Some(err_detail) = err_resp.error {
            if let Some(msg) = err_detail.message {
                let sanitized = sanitize_error_message(&msg);
                return format!("HTTP {}: {}", status, sanitized);
            }
        }
    }

    let sanitized = sanitize_error_message(body);
    if sanitized.trim().is_empty() {
        format!("HTTP error {}", status)
    } else {
        format!("HTTP {}: {}", status, sanitized)
    }
}

fn extract_candidate_text(body: &str) -> Result<String, ProviderError> {
    let json_val: serde_json::Value = serde_json::from_str(body).map_err(|e| {
        ProviderError::StructuredOutputInvalid(format!("malformed JSON response: {e}"))
    })?;

    let text = json_val
        .get("candidates")
        .and_then(|c| c.as_array())
        .and_then(|arr| arr.first())
        .and_then(|c0| c0.get("content"))
        .and_then(|cnt| cnt.get("parts"))
        .and_then(|parts| parts.as_array())
        .and_then(|arr| arr.first())
        .and_then(|p0| p0.get("text"))
        .and_then(|t| t.as_str());

    match text {
        Some(t) if !t.trim().is_empty() => Ok(t.to_string()),
        _ => Err(ProviderError::StructuredOutputInvalid(
            "empty candidate response".to_string(),
        )),
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct GeminiUsageMetadata {
    prompt_token_count: Option<u32>,
    candidates_token_count: Option<u32>,
    total_token_count: Option<u32>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct GeminiRawResponse {
    usage_metadata: Option<GeminiUsageMetadata>,
}

fn extract_usage(body: &str, duration_ms: i64) -> Option<GenerationUsage> {
    serde_json::from_str::<GeminiRawResponse>(body)
        .ok()
        .and_then(|r| r.usage_metadata)
        .map(|u| GenerationUsage {
            input_tokens: u.prompt_token_count,
            output_tokens: u.candidates_token_count,
            total_tokens: u.total_token_count,
            provider_reported_cost: None,
            duration_ms: Some(duration_ms),
        })
}

/// Pure HTTP status and body response classifier.
/// Normalizes HTTP status codes and response bodies into `Result<String, ProviderError>`.
pub fn classify_http_status_and_body(status: u16, body: &str) -> Result<String, ProviderError> {
    match status {
        200..=299 => extract_candidate_text(body),
        401 | 403 => {
            let msg = extract_error_message(status, body);
            Err(ProviderError::AuthFailed(msg))
        }
        404 => {
            let msg = extract_error_message(status, body);
            Err(ProviderError::ModelUnavailable(msg))
        }
        429 => {
            let msg = extract_error_message(status, body);
            Err(ProviderError::RateLimited(msg))
        }
        408 => {
            let msg = extract_error_message(status, body);
            Err(ProviderError::Timeout(msg))
        }
        500..=599 => {
            let msg = extract_error_message(status, body);
            Err(ProviderError::Unavailable(msg))
        }
        400 => {
            let lower = body.to_lowercase();
            if lower.contains("api key not valid") || lower.contains("api_key_invalid") {
                let msg = extract_error_message(status, body);
                Err(ProviderError::AuthFailed(msg))
            } else if lower.contains("not found")
                || lower.contains("not supported")
                || lower.contains("is no longer available")
            {
                let msg = extract_error_message(status, body);
                Err(ProviderError::ModelUnavailable(msg))
            } else {
                let msg = extract_error_message(status, body);
                Err(ProviderError::StructuredOutputInvalid(msg))
            }
        }
        other => {
            let lower = body.to_lowercase();
            if lower.contains("not found")
                || lower.contains("not supported")
                || lower.contains("is no longer available")
            {
                let msg = extract_error_message(other, body);
                Err(ProviderError::ModelUnavailable(msg))
            } else {
                let msg = extract_error_message(other, body);
                Err(ProviderError::Transport(msg))
            }
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct GeminiPart {
    text: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct GeminiContent {
    #[serde(skip_serializing_if = "Option::is_none")]
    role: Option<String>,
    parts: Vec<GeminiPart>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct GeminiSystemInstruction {
    parts: Vec<GeminiPart>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct GeminiGenerationConfig {
    response_mime_type: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct GeminiGenerateContentRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    system_instruction: Option<GeminiSystemInstruction>,
    contents: Vec<GeminiContent>,
    generation_config: GeminiGenerationConfig,
}

/// Live Gemini provider implementing HTTP transport with blocking reqwest client.
pub struct GeminiProvider {
    config: GeminiRuntimeConfig,
    client: reqwest::blocking::Client,
    base_url: String,
}

impl std::fmt::Debug for GeminiProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GeminiProvider")
            .field("model", &self.config.model)
            .field("base_url", &self.base_url)
            .finish()
    }
}

impl GeminiProvider {
    pub fn new(config: GeminiRuntimeConfig) -> Result<Self, ProviderError> {
        Self::with_base_url(config, DEFAULT_GEMINI_BASE_URL.to_string())
    }

    pub fn with_base_url(
        config: GeminiRuntimeConfig,
        base_url: String,
    ) -> Result<Self, ProviderError> {
        if config.api_key.trim().is_empty() {
            return Err(ProviderError::AuthMissing("API key is empty".to_string()));
        }
        let client = reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(60))
            .build()
            .map_err(|e| ProviderError::Transport(format!("failed to build HTTP client: {e}")))?;

        Ok(Self {
            config,
            client,
            base_url,
        })
    }

    fn execute_request(&self, input: &ProviderRequest) -> Result<ProviderResponse, ProviderError> {
        let effective_model = input.model.as_deref().unwrap_or(&self.config.model);
        let model_path = if effective_model.starts_with("models/") {
            effective_model.to_string()
        } else {
            format!("models/{}", effective_model)
        };
        let url = format!(
            "{}/v1beta/{}:generateContent",
            self.base_url.trim_end_matches('/'),
            model_path
        );

        let req_body = GeminiGenerateContentRequest {
            system_instruction: if input.system_prompt.trim().is_empty() {
                None
            } else {
                Some(GeminiSystemInstruction {
                    parts: vec![GeminiPart {
                        text: input.system_prompt.clone(),
                    }],
                })
            },
            contents: vec![GeminiContent {
                role: Some("user".to_string()),
                parts: vec![GeminiPart {
                    text: input.user_prompt.clone(),
                }],
            }],
            generation_config: GeminiGenerationConfig {
                response_mime_type: "application/json".to_string(),
            },
        };

        let json_payload = serde_json::to_string(&req_body).map_err(|e| {
            ProviderError::StructuredOutputInvalid(format!("failed to serialize request: {e}"))
        })?;

        let start = std::time::Instant::now();

        let response = self
            .client
            .post(&url)
            .header("x-goog-api-key", &self.config.api_key)
            .header("content-type", "application/json")
            .body(json_payload)
            .send()
            .map_err(|e| {
                let raw_err = e.to_string();
                let sanitized = sanitize_error_message(&raw_err);
                if e.is_timeout() {
                    ProviderError::Timeout(sanitized)
                } else {
                    ProviderError::Transport(sanitized)
                }
            })?;

        let status = response.status().as_u16();
        let duration_ms = start.elapsed().as_millis() as i64;
        let body = response
            .text()
            .map_err(|e| ProviderError::Transport(sanitize_error_message(&e.to_string())))?;

        let candidate_text = classify_http_status_and_body(status, &body)?;
        let usage = extract_usage(&body, duration_ms);

        Ok(ProviderResponse {
            text: candidate_text,
            usage,
            model: effective_model.to_string(),
            provider: "gemini".to_string(),
        })
    }
}

impl AIProvider for GeminiProvider {
    fn name(&self) -> &'static str {
        "gemini"
    }

    fn generate_planning_draft(
        &self,
        input: &ProviderRequest,
    ) -> Result<ProviderResponse, ProviderError> {
        self.execute_request(input)
    }

    fn generate_content_plan(
        &self,
        input: &ProviderRequest,
    ) -> Result<ProviderResponse, ProviderError> {
        self.execute_request(input)
    }

    fn regenerate_plan(&self, input: &ProviderRequest) -> Result<ProviderResponse, ProviderError> {
        self.execute_request(input)
    }

    fn regenerate_scene(&self, input: &ProviderRequest) -> Result<ProviderResponse, ProviderError> {
        self.execute_request(input)
    }

    fn regenerate_narration(
        &self,
        input: &ProviderRequest,
    ) -> Result<ProviderResponse, ProviderError> {
        self.execute_request(input)
    }
}
