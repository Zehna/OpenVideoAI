use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use crate::content_planning::model::GenerationUsage;
use crate::content_planning::provider::{
    AIProvider, ProviderError, ProviderRequest, ProviderResponse,
};

pub const MOCK_PLAN_JSON: &str = r#"{
  "id": "plan-mock",
  "projectId": "project-mock",
  "revision": 1,
  "status": "draft",
  "title": "Mock Content Plan",
  "summary": "Deterministic mock content plan for testing.",
  "language": "en",
  "tone": "informative",
  "targetDurationMs": 30000,
  "estimatedDurationMs": 30000,
  "fullNarration": "Welcome to this overview of the topic.\n\nIn conclusion, this summarizes the core concepts.",
  "scenes": [
    {
      "id": "scene-1",
      "order": 1,
      "title": "Introduction",
      "narration": "Welcome to this overview of the topic.",
      "durationEstimateMs": 15000,
      "visualDescription": "Opening shot establishing the core theme.",
      "mediaIntent": [
        "stock_video"
      ],
      "stockSearchQueries": [
        "technology overview"
      ],
      "imagePrompt": null,
      "videoPrompt": null,
      "claims": [
        {
          "id": "claim-1-1",
          "text": "This topic has seen rapid development recently.",
          "groundingStatus": "model_knowledge",
          "sourceRefs": [],
          "groundingValid": true,
          "note": null
        }
      ],
      "sourceRefs": []
    },
    {
      "id": "scene-2",
      "order": 2,
      "title": "Conclusion",
      "narration": "In conclusion, this summarizes the core concepts.",
      "durationEstimateMs": 15000,
      "visualDescription": "Closing visual summarizing key points.",
      "mediaIntent": [
        "stock_video",
        "text_graphic"
      ],
      "stockSearchQueries": [
        "summary graphic"
      ],
      "imagePrompt": null,
      "videoPrompt": null,
      "claims": [
        {
          "id": "claim-2-1",
          "text": "Core concepts provide foundational knowledge.",
          "groundingStatus": "model_knowledge",
          "sourceRefs": [],
          "groundingValid": true,
          "note": null
        }
      ],
      "sourceRefs": []
    }
  ],
  "sourceRefs": [],
  "generationMetadata": {
    "provider": "mock",
    "model": "mock-model",
    "plannerProfileVersion": 1,
    "promptTemplateVersion": 1,
    "generatedAt": "2026-09-12T00:00:00Z",
    "usage": {
      "inputTokens": 150,
      "outputTokens": 300,
      "totalTokens": 450,
      "providerReportedCost": 0.0,
      "durationMs": 100
    }
  },
  "createdAt": "2026-09-12T00:00:00Z",
  "updatedAt": "2026-09-12T00:00:00Z"
}"#;

pub const MOCK_MALFORMED_JSON: &str = r#"{ "id": "plan-mock", "scenes": [ incomplete json..."#;

pub const MOCK_DRAFT_TEXT: &str = "# Planning Draft: Mock Draft\n\n## Scene 1: Introduction\nOutline for introductory scene.\n\n## Scene 2: Conclusion\nOutline for concluding scene.\n";

pub const MOCK_MALFORMED_DRAFT: &str = "# Broken Draft <<incomplete>>";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MockOutcome {
    Success,
    Malformed,
    Repairable,
    RepairExhausted,
    RateLimited,
    Timeout,
    AuthFailed,
    Unavailable,
    ModelUnavailable,
    TransientUnavailableOnce,
    PrimaryUnavailableThenFallbackSuccess,
    FallbackUnavailable,
}

pub struct MockProvider {
    outcome: MockOutcome,
    call_count: AtomicUsize,
    repair_failed: AtomicBool,
}

impl std::fmt::Debug for MockProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MockProvider")
            .field("outcome", &self.outcome)
            .field("call_count", &self.call_count.load(Ordering::SeqCst))
            .finish()
    }
}

impl Clone for MockProvider {
    fn clone(&self) -> Self {
        Self {
            outcome: self.outcome,
            call_count: AtomicUsize::new(self.call_count.load(Ordering::SeqCst)),
            repair_failed: AtomicBool::new(self.repair_failed.load(Ordering::SeqCst)),
        }
    }
}

impl MockProvider {
    pub fn new(outcome: MockOutcome) -> Self {
        Self {
            outcome,
            call_count: AtomicUsize::new(0),
            repair_failed: AtomicBool::new(false),
        }
    }

    pub fn default_success() -> Self {
        Self::new(MockOutcome::Success)
    }

    pub fn outcome(&self) -> MockOutcome {
        self.outcome
    }

    pub fn call_count(&self) -> usize {
        self.call_count.load(Ordering::SeqCst)
    }

    fn next_response(
        &self,
        input: &ProviderRequest,
        valid_text: &'static str,
        malformed_text: &'static str,
    ) -> Result<ProviderResponse, ProviderError> {
        let _count = self.call_count.fetch_add(1, Ordering::SeqCst);
        let target_model = input
            .model
            .as_deref()
            .unwrap_or(crate::content_planning::provider::PRIMARY_GEMINI_MODEL);
        match self.outcome {
            MockOutcome::Success => Ok(self.build_response_with_model(valid_text, target_model)),
            MockOutcome::Malformed => {
                Ok(self.build_response_with_model(malformed_text, target_model))
            }
            MockOutcome::Repairable => {
                if !self.repair_failed.swap(true, Ordering::SeqCst) {
                    Ok(self.build_response_with_model(malformed_text, target_model))
                } else {
                    Ok(self.build_response_with_model(valid_text, target_model))
                }
            }
            MockOutcome::RepairExhausted => {
                Ok(self.build_response_with_model(malformed_text, target_model))
            }
            MockOutcome::RateLimited => Err(ProviderError::RateLimited(
                "mock rate limit exceeded".to_string(),
            )),
            MockOutcome::Timeout => {
                Err(ProviderError::Timeout("mock request timed out".to_string()))
            }
            MockOutcome::AuthFailed => Err(ProviderError::AuthFailed(
                "mock authentication failed".to_string(),
            )),
            MockOutcome::Unavailable => Err(ProviderError::Unavailable(
                "mock provider service unavailable".to_string(),
            )),
            MockOutcome::ModelUnavailable => Err(ProviderError::ModelUnavailable(
                "mock model is not supported or retired".to_string(),
            )),
            MockOutcome::TransientUnavailableOnce => {
                if self.call_count.load(Ordering::SeqCst) <= 1 {
                    Err(ProviderError::Unavailable(
                        "mock 503 high demand (transient)".to_string(),
                    ))
                } else {
                    Ok(self.build_response_with_model(valid_text, target_model))
                }
            }
            MockOutcome::PrimaryUnavailableThenFallbackSuccess => {
                if target_model == crate::content_planning::provider::FALLBACK_GEMINI_MODEL {
                    Ok(self.build_response_with_model(valid_text, target_model))
                } else {
                    Err(ProviderError::Unavailable(
                        "mock 503 high demand on primary model".to_string(),
                    ))
                }
            }
            MockOutcome::FallbackUnavailable => Err(ProviderError::Unavailable(
                "mock 503 high demand on fallback model".to_string(),
            )),
        }
    }

    pub fn build_response(&self, text: &str) -> ProviderResponse {
        self.build_response_with_model(text, "mock-model")
    }

    pub fn build_response_with_model(&self, text: &str, model: &str) -> ProviderResponse {
        ProviderResponse {
            text: text.to_string(),
            usage: Some(GenerationUsage {
                input_tokens: Some(150),
                output_tokens: Some(300),
                total_tokens: Some(450),
                provider_reported_cost: Some(0.0),
                duration_ms: Some(100),
            }),
            model: model.to_string(),
            provider: "mock".to_string(),
        }
    }
}

impl AIProvider for MockProvider {
    fn name(&self) -> &'static str {
        "mock"
    }

    fn generate_planning_draft(
        &self,
        input: &ProviderRequest,
    ) -> Result<ProviderResponse, ProviderError> {
        match self.outcome {
            MockOutcome::Repairable | MockOutcome::RepairExhausted => {
                self.call_count.fetch_add(1, Ordering::SeqCst);
                let target_model = input.model.as_deref().unwrap_or("mock-model");
                Ok(self.build_response_with_model(MOCK_DRAFT_TEXT, target_model))
            }
            _ => self.next_response(input, MOCK_DRAFT_TEXT, MOCK_MALFORMED_DRAFT),
        }
    }

    fn generate_content_plan(
        &self,
        input: &ProviderRequest,
    ) -> Result<ProviderResponse, ProviderError> {
        self.next_response(input, MOCK_PLAN_JSON, MOCK_MALFORMED_JSON)
    }

    fn regenerate_plan(&self, input: &ProviderRequest) -> Result<ProviderResponse, ProviderError> {
        self.next_response(input, MOCK_PLAN_JSON, MOCK_MALFORMED_JSON)
    }

    fn regenerate_scene(&self, input: &ProviderRequest) -> Result<ProviderResponse, ProviderError> {
        self.next_response(input, MOCK_PLAN_JSON, MOCK_MALFORMED_JSON)
    }

    fn regenerate_narration(
        &self,
        input: &ProviderRequest,
    ) -> Result<ProviderResponse, ProviderError> {
        self.next_response(input, MOCK_PLAN_JSON, MOCK_MALFORMED_JSON)
    }
}
