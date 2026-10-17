use std::sync::Mutex;

use openvideoai_lib::content_planning::derivation::derive_plan_fields;
use openvideoai_lib::content_planning::error::ContentPlanningError;
use openvideoai_lib::content_planning::model::*;
use openvideoai_lib::content_planning::planner::{ContentPlanner, MAX_MODEL_CALLS_PER_STAGE};
use openvideoai_lib::content_planning::prompt::{
    build_draft_prompt, build_plan_prompt, PromptBuilder, PLANNER_PROFILE_VERSION,
    PROMPT_TEMPLATE_VERSION,
};
use openvideoai_lib::content_planning::provider::{
    classify_http_status_and_body, sanitize_error_message, AIProvider, GeminiProvider,
    GeminiRuntimeConfig, MockOutcome, MockProvider, ProviderConfigResolver, ProviderError,
    ProviderRequest,
};

static ENV_MUTEX: Mutex<()> = Mutex::new(());

#[test]
fn test_provider_config_missing_key() {
    let _lock = ENV_MUTEX.lock().unwrap();
    let prev = std::env::var("GEMINI_API_KEY").ok();

    // 1. Unset environment variable
    std::env::remove_var("GEMINI_API_KEY");
    let res = ProviderConfigResolver::resolve_gemini();
    assert!(res.is_err());
    match res {
        Err(ProviderError::AuthMissing(msg)) => {
            assert!(msg.contains("GEMINI_API_KEY"));
        }
        Err(other) => panic!("expected ProviderError::AuthMissing, got: {:?}", other),
        Ok(_) => panic!("expected Err, got Ok"),
    }

    // 2. Empty or whitespace-only string
    std::env::set_var("GEMINI_API_KEY", "   ");
    let res_empty = ProviderConfigResolver::resolve_gemini();
    assert!(matches!(res_empty, Err(ProviderError::AuthMissing(_))));

    // Restore
    if let Some(val) = prev {
        std::env::set_var("GEMINI_API_KEY", val);
    } else {
        std::env::remove_var("GEMINI_API_KEY");
    }
}

#[test]
fn test_provider_config_status_no_secret_leak() {
    let _lock = ENV_MUTEX.lock().unwrap();
    let prev = std::env::var("GEMINI_API_KEY").ok();

    // 1. Unset state
    std::env::remove_var("GEMINI_API_KEY");
    let status_unconfigured = ProviderConfigResolver::get_status();
    assert_eq!(status_unconfigured.provider, "gemini");
    assert!(!status_unconfigured.configured);
    assert_eq!(status_unconfigured.model, "gemini-3.8-flash");

    // 2. Set secret key
    let secret = "test-secret-key-xyz-98765";
    std::env::set_var("GEMINI_API_KEY", secret);

    let status_configured = ProviderConfigResolver::get_status();
    assert_eq!(status_configured.provider, "gemini");
    assert!(status_configured.configured);
    assert_eq!(status_configured.model, "gemini-3.8-flash");

    // Check serialization does NOT contain secret
    let json = serde_json::to_string(&status_configured).expect("must serialize");
    assert!(!json.contains(secret));

    // Check debug format does NOT contain secret
    let debug_str = format!("{:?}", status_configured);
    assert!(!debug_str.contains(secret));

    // Also verify resolve_gemini returns runtime config with secret and default model
    let runtime_config = ProviderConfigResolver::resolve_gemini().expect("must resolve");
    assert_eq!(runtime_config.api_key, secret);
    assert_eq!(runtime_config.model, "gemini-3.8-flash");

    // Restore
    if let Some(val) = prev {
        std::env::set_var("GEMINI_API_KEY", val);
    } else {
        std::env::remove_var("GEMINI_API_KEY");
    }
}

#[test]
fn test_prompt_builder_contains_source_whitelist() {
    let request = PlanningRequest {
        project_id: "proj-1".to_string(),
        input_type: InputType::Topic,
        input_text: "The History of Quantum Computing".to_string(),
        target_duration_ms: 60000,
        language: "en".to_string(),
        tone: "informative".to_string(),
        source_context: vec![
            PlanningSource {
                source_id: "src-doc-1".to_string(),
                source_type: "document".to_string(),
                title: "Quantum Mechanics Paper".to_string(),
                author: Some("Dr. Physicist".to_string()),
                url: Some("https://example.com/quantum".to_string()),
                published_at: Some("2024-01-01".to_string()),
                locator: Some("Section 2".to_string()),
                content_snapshot: "Quantum computers use qubits instead of classical bits."
                    .to_string(),
                content_hash: "hash1".to_string(),
            },
            PlanningSource {
                source_id: "src-video-2".to_string(),
                source_type: "video".to_string(),
                title: "Quantum Algorithms Talk".to_string(),
                author: None,
                url: None,
                published_at: None,
                locator: None,
                content_snapshot: "Shor's algorithm can factor integers in polynomial time."
                    .to_string(),
                content_hash: "hash2".to_string(),
            },
        ],
        generation_options: GenerationOptions::default(),
    };

    // Test build_draft_prompt
    let draft_prompt = PromptBuilder::build_draft_prompt(&request);
    assert!(draft_prompt
        .system_prompt
        .contains("ALLOWED_SOURCES: [\"src-doc-1\", \"src-video-2\"]"));
    assert!(draft_prompt
        .system_prompt
        .contains("Cite only IDs in ALLOWED_SOURCES."));
    assert!(draft_prompt
        .system_prompt
        .contains("Never invent source IDs, titles, authors, URLs, or locators."));
    assert!(draft_prompt.system_prompt.contains(
        "Use model_knowledge or uncertain when supplied sources do not support a factual claim."
    ));

    assert!(draft_prompt
        .user_prompt
        .contains("The History of Quantum Computing"));
    assert!(draft_prompt.user_prompt.contains("60000"));
    assert!(draft_prompt.user_prompt.contains("en"));
    assert!(draft_prompt.user_prompt.contains("informative"));
    assert!(draft_prompt.user_prompt.contains("src-doc-1"));
    assert!(draft_prompt.user_prompt.contains("src-video-2"));

    // Test build_plan_prompt
    let plan_prompt = PromptBuilder::build_plan_prompt(&request, "Draft Outline: Scene 1, Scene 2");
    assert!(plan_prompt
        .system_prompt
        .contains("ALLOWED_SOURCES: [\"src-doc-1\", \"src-video-2\"]"));
    assert!(plan_prompt
        .system_prompt
        .contains("Cite only IDs in ALLOWED_SOURCES."));
    assert!(plan_prompt
        .system_prompt
        .contains("Never invent source IDs, titles, authors, URLs, or locators."));
    assert!(plan_prompt
        .user_prompt
        .contains("Draft Outline: Scene 1, Scene 2"));
    assert_eq!(plan_prompt.schema_name, Some("content_plan".to_string()));

    // Test standalone function equivalent
    let standalone_draft = build_draft_prompt(&request);
    assert_eq!(standalone_draft.system_prompt, draft_prompt.system_prompt);
    assert_eq!(standalone_draft.user_prompt, draft_prompt.user_prompt);

    let standalone_plan = build_plan_prompt(&request, "Draft Outline: Scene 1, Scene 2");
    assert_eq!(standalone_plan.system_prompt, plan_prompt.system_prompt);
    assert_eq!(standalone_plan.user_prompt, plan_prompt.user_prompt);
}

#[test]
fn test_prompt_builder_versions() {
    assert_eq!(PROMPT_TEMPLATE_VERSION, 1);
    assert_eq!(PLANNER_PROFILE_VERSION, 1);
    assert_eq!(PromptBuilder::PROMPT_TEMPLATE_VERSION, 1);
    assert_eq!(PromptBuilder::PLANNER_PROFILE_VERSION, 1);
}

#[test]
fn test_mock_provider_deterministic_success() {
    let provider = MockProvider::new(MockOutcome::Success);
    assert_eq!(provider.name(), "mock");

    let req = ProviderRequest {
        system_prompt: "sys prompt".to_string(),
        user_prompt: "user prompt".to_string(),
        schema_name: Some("content_plan".to_string()),
        model: None,
    };

    let resp1 = provider.generate_content_plan(&req).expect("must succeed");
    let resp2 = provider.generate_content_plan(&req).expect("must succeed");

    // Byte-stable deterministic response
    assert_eq!(resp1.text, resp2.text);
    assert_eq!(resp1.provider, "mock");
    assert!(!resp1.model.is_empty());
    assert!(resp1.usage.is_some());

    // Deserialize to ContentPlan and verify fields
    let plan: ContentPlan = serde_json::from_str(&resp1.text).expect("must parse ContentPlan");
    assert_eq!(plan.id, "plan-mock");
    assert_eq!(plan.scenes.len(), 2);
    assert_eq!(plan.scenes[0].id, "scene-1");
    assert_eq!(plan.scenes[1].id, "scene-2");
    assert!(!plan.scenes[0].claims.is_empty());
    assert_eq!(plan.scenes[0].claims[0].id, "claim-1-1");
    assert_eq!(plan.target_duration_ms, plan.estimated_duration_ms);

    // Also verify draft generation works
    let draft_resp = provider
        .generate_planning_draft(&req)
        .expect("draft succeeds");
    assert!(!draft_resp.text.is_empty());
    assert_eq!(draft_resp.provider, "mock");

    // Verify regenerate methods work deterministically
    let regen_plan = provider.regenerate_plan(&req).expect("regen plan succeeds");
    assert_eq!(regen_plan.text, resp1.text);

    let regen_scene = provider
        .regenerate_scene(&req)
        .expect("regen scene succeeds");
    assert!(!regen_scene.text.is_empty());

    let regen_narration = provider
        .regenerate_narration(&req)
        .expect("regen narration succeeds");
    assert!(!regen_narration.text.is_empty());
}

#[test]
fn test_mock_provider_repairable() {
    let provider = MockProvider::new(MockOutcome::Repairable);
    let req = ProviderRequest {
        system_prompt: "sys prompt".to_string(),
        user_prompt: "user prompt".to_string(),
        schema_name: Some("content_plan".to_string()),
        model: None,
    };

    // First call returns malformed JSON
    let resp1 = provider
        .generate_content_plan(&req)
        .expect("first call produces response");
    assert!(
        serde_json::from_str::<ContentPlan>(&resp1.text).is_err(),
        "first call must return malformed content"
    );

    // Second call returns valid JSON
    let resp2 = provider
        .generate_content_plan(&req)
        .expect("second call produces response");
    let plan: ContentPlan =
        serde_json::from_str(&resp2.text).expect("second call must be valid ContentPlan");
    assert_eq!(plan.id, "plan-mock");
    assert_eq!(plan.scenes.len(), 2);

    // Third call continues to return valid JSON
    let resp3 = provider
        .generate_content_plan(&req)
        .expect("third call produces response");
    let plan3: ContentPlan =
        serde_json::from_str(&resp3.text).expect("third call must be valid ContentPlan");
    assert_eq!(plan3.id, "plan-mock");
}

#[test]
fn test_mock_provider_error_modes() {
    let req = ProviderRequest {
        system_prompt: "sys prompt".to_string(),
        user_prompt: "user prompt".to_string(),
        schema_name: Some("content_plan".to_string()),
        model: None,
    };

    // RateLimited
    let rate_limited = MockProvider::new(MockOutcome::RateLimited);
    let err = rate_limited.generate_content_plan(&req).unwrap_err();
    assert!(matches!(err, ProviderError::RateLimited(_)));

    // Timeout
    let timeout = MockProvider::new(MockOutcome::Timeout);
    let err = timeout.generate_content_plan(&req).unwrap_err();
    assert!(matches!(err, ProviderError::Timeout(_)));

    // AuthFailed
    let auth_failed = MockProvider::new(MockOutcome::AuthFailed);
    let err = auth_failed.generate_content_plan(&req).unwrap_err();
    assert!(matches!(err, ProviderError::AuthFailed(_)));

    // Unavailable
    let unavailable = MockProvider::new(MockOutcome::Unavailable);
    let err = unavailable.generate_content_plan(&req).unwrap_err();
    assert!(matches!(err, ProviderError::Unavailable(_)));

    // Malformed
    let malformed = MockProvider::new(MockOutcome::Malformed);
    let resp = malformed.generate_content_plan(&req).unwrap();
    assert!(serde_json::from_str::<ContentPlan>(&resp.text).is_err());

    // RepairExhausted
    let exhausted = MockProvider::new(MockOutcome::RepairExhausted);
    for _ in 0..5 {
        let resp = exhausted.generate_content_plan(&req).unwrap();
        assert!(serde_json::from_str::<ContentPlan>(&resp.text).is_err());
    }
}

#[test]
fn test_provider_error_conversion_to_content_planning_error() {
    let cases = vec![
        (
            ProviderError::AuthMissing("missing api key".to_string()),
            ContentPlanningError::ProviderAuthMissing("missing api key".to_string()),
            "provider_auth_missing",
        ),
        (
            ProviderError::AuthFailed("invalid key".to_string()),
            ContentPlanningError::ProviderAuthFailed("invalid key".to_string()),
            "provider_auth_failed",
        ),
        (
            ProviderError::RateLimited("too many requests".to_string()),
            ContentPlanningError::ProviderRateLimited("too many requests".to_string()),
            "provider_rate_limited",
        ),
        (
            ProviderError::Timeout("deadline exceeded".to_string()),
            ContentPlanningError::ProviderTimeout("deadline exceeded".to_string()),
            "provider_timeout",
        ),
        (
            ProviderError::Unavailable("service down".to_string()),
            ContentPlanningError::ProviderUnavailable("service down".to_string()),
            "provider_unavailable",
        ),
        (
            ProviderError::StructuredOutputInvalid("malformed json".to_string()),
            ContentPlanningError::StructuredOutputInvalid("malformed json".to_string()),
            "structured_output_invalid",
        ),
        (
            ProviderError::Transport("connection reset".to_string()),
            ContentPlanningError::ProviderUnavailable("connection reset".to_string()),
            "provider_unavailable",
        ),
    ];

    for (provider_err, expected_cpe, expected_code) in cases {
        let converted = provider_err.to_content_planning_error();
        assert_eq!(converted, expected_cpe);
        assert_eq!(converted.code(), expected_code);
    }
}

fn fixture_planning_request() -> PlanningRequest {
    PlanningRequest {
        project_id: "project-mock".to_string(),
        input_type: InputType::Topic,
        input_text: "Mock topic for planner testing".to_string(),
        target_duration_ms: 30000,
        language: "en".to_string(),
        tone: "informative".to_string(),
        source_context: vec![PlanningSource {
            source_id: "src-1".to_string(),
            source_type: "document".to_string(),
            title: "Source Document".to_string(),
            author: Some("Author".to_string()),
            url: None,
            published_at: None,
            locator: None,
            content_snapshot: "Snapshot text".to_string(),
            content_hash: "hash1".to_string(),
        }],
        generation_options: GenerationOptions::default(),
    }
}

#[test]
fn test_planner_generate_plan_success() {
    let provider = MockProvider::new(MockOutcome::Success);
    let planner = ContentPlanner::new(provider);
    let request = fixture_planning_request();

    let plan = planner
        .generate_plan(&request)
        .expect("generate_plan should succeed");

    assert_eq!(plan.id, "plan-mock");
    assert_eq!(plan.project_id, "project-mock");
    assert_eq!(plan.scenes.len(), 2);
    assert_eq!(plan.estimated_duration_ms, 30000);
    assert_eq!(plan.target_duration_ms, 30000);
    assert!(!plan.full_narration.is_empty());
    assert_eq!(planner.provider().call_count(), 2); // 1 draft + 1 plan expansion
    assert!(plan.generation_metadata.is_some());
}

#[test]
fn test_planner_generate_plan_repairable() {
    let provider = MockProvider::new(MockOutcome::Repairable);
    let planner = ContentPlanner::new(provider);
    let request = fixture_planning_request();

    let plan = planner
        .generate_plan(&request)
        .expect("generate_plan with repair should succeed");

    assert_eq!(plan.id, "plan-mock");
    assert_eq!(plan.scenes.len(), 2);
    assert_eq!(planner.provider().call_count(), 3); // 1 draft + 1 failed expansion + 1 repair expansion
    let meta = plan
        .generation_metadata
        .as_ref()
        .expect("generation metadata should be present");
    assert_eq!(meta.repaired_calls, Some(1));
}

#[test]
fn test_planner_repair_exhaustion() {
    let provider = MockProvider::new(MockOutcome::RepairExhausted);
    let planner = ContentPlanner::new(provider);
    let request = fixture_planning_request();

    let err = planner.generate_plan(&request).unwrap_err();
    assert!(matches!(
        err,
        ContentPlanningError::StructuredOutputInvalid(_)
    ));
    // Draft was 1 call, then expansion made 3 attempts (initial + 2 repairs = MAX_MODEL_CALLS_PER_STAGE)
    assert_eq!(
        planner.provider().call_count(),
        1 + MAX_MODEL_CALLS_PER_STAGE
    );
}

#[test]
fn test_planner_auth_missing_non_retryable() {
    let provider = MockProvider::new(MockOutcome::AuthFailed);
    let planner = ContentPlanner::new(provider);
    let request = fixture_planning_request();

    let err = planner.generate_plan(&request).unwrap_err();
    assert!(matches!(err, ContentPlanningError::ProviderAuthFailed(_)));
    // Fails immediately on 1st call without burning repair budget
    assert_eq!(planner.provider().call_count(), 1);
}

#[test]
fn test_planner_rate_limited_retryable() {
    let provider = MockProvider::new(MockOutcome::RateLimited);
    let planner = ContentPlanner::new(provider);
    let request = fixture_planning_request();

    let err = planner.generate_plan(&request).unwrap_err();
    assert!(matches!(err, ContentPlanningError::ProviderRateLimited(_)));
    // Fails immediately on 1st call without burning repair budget
    assert_eq!(planner.provider().call_count(), 1);
}

#[test]
fn test_planner_regenerate_plan() {
    let provider = MockProvider::new(MockOutcome::Success);
    let planner = ContentPlanner::new(provider);
    let request = fixture_planning_request();

    let base_plan = planner
        .generate_plan(&request)
        .expect("base plan generation should succeed");
    assert_eq!(base_plan.revision, 1);

    let sources = request.source_context.clone();
    let regen_plan = planner
        .regenerate_plan(&base_plan, "Make the tone more dramatic", &sources)
        .expect("regenerate_plan should succeed");

    assert_eq!(regen_plan.id, base_plan.id);
    assert_eq!(regen_plan.revision, 2);
    assert_eq!(regen_plan.status, PlanStatus::Draft);
    assert_eq!(regen_plan.scenes.len(), 2);
    assert_eq!(planner.provider().call_count(), 3); // 2 from generate_plan + 1 from regenerate_plan
}

#[test]
fn test_planner_regenerate_scene_isolation() {
    let provider = MockProvider::new(MockOutcome::Success);
    let planner = ContentPlanner::new(provider);
    let request = fixture_planning_request();

    let mut base_plan = planner
        .generate_plan(&request)
        .expect("base plan generation should succeed");
    // Modify scene-2 in base plan so it is distinguishable from mock default
    base_plan.scenes[1].title = "Custom Protected Scene 2 Title".to_string();
    base_plan.scenes[1].visual_description = "Custom protected visual description".to_string();
    base_plan.scenes[1].narration = "Custom protected narration for scene 2.".to_string();
    derive_plan_fields(&mut base_plan);

    let sources = request.source_context.clone();

    // 1. Error if target scene not found
    let err = planner
        .regenerate_scene(
            &base_plan,
            "non-existent-scene-id",
            "Rewrite scene",
            &sources,
        )
        .unwrap_err();
    assert!(matches!(
        err,
        ContentPlanningError::ContentPlanValidationFailed(_)
    ));

    // 2. Regenerate scene-1
    let new_plan = planner
        .regenerate_scene(
            &base_plan,
            "scene-1",
            "Rewrite scene 1 with new visuals",
            &sources,
        )
        .expect("regenerate_scene should succeed");

    assert_eq!(new_plan.revision, 2);
    assert_eq!(new_plan.status, PlanStatus::Draft);
    // Target scene-1 is updated
    assert_eq!(new_plan.scenes[0].id, "scene-1");
    // Non-target scene-2 is COMPLETELY PRESERVED
    assert_eq!(new_plan.scenes[1].id, "scene-2");
    assert_eq!(new_plan.scenes[1].title, "Custom Protected Scene 2 Title");
    assert_eq!(
        new_plan.scenes[1].visual_description,
        "Custom protected visual description"
    );
    assert_eq!(
        new_plan.scenes[1].narration,
        "Custom protected narration for scene 2."
    );
    assert_eq!(new_plan.scenes[1].order, 2);
    // Aggregate fields canonically derived
    assert_eq!(
        new_plan.estimated_duration_ms,
        new_plan.scenes[0].duration_estimate_ms + new_plan.scenes[1].duration_estimate_ms
    );
}

#[test]
fn test_planner_regenerate_narration_isolation() {
    let provider = MockProvider::new(MockOutcome::Success);
    let planner = ContentPlanner::new(provider);
    let request = fixture_planning_request();

    let mut base_plan = planner
        .generate_plan(&request)
        .expect("base plan generation should succeed");
    // Set custom visual planning and media intent on scene-1
    base_plan.scenes[0].title = "Protected Scene 1 Title".to_string();
    base_plan.scenes[0].visual_description = "Protected Scene 1 Visuals".to_string();
    base_plan.scenes[0].media_intent = vec![MediaIntent::AiVideo];
    base_plan.scenes[0].stock_search_queries = vec!["custom query".to_string()];
    base_plan.scenes[0].image_prompt = Some("custom image prompt".to_string());
    base_plan.scenes[0].video_prompt = Some("custom video prompt".to_string());
    base_plan.scenes[0].duration_estimate_ms = 15000;
    base_plan.scenes[0].narration = "Old narration to be replaced".to_string();

    // Also customize scene-2
    base_plan.scenes[1].title = "Protected Scene 2 Title".to_string();
    base_plan.scenes[1].narration = "Protected Scene 2 Narration".to_string();
    derive_plan_fields(&mut base_plan);

    let sources = request.source_context.clone();

    // Regenerate narration for scene-1
    let new_plan = planner
        .regenerate_narration(
            &base_plan,
            "scene-1",
            "Make narration more punchy",
            &sources,
        )
        .expect("regenerate_narration should succeed");

    assert_eq!(new_plan.revision, 2);
    assert_eq!(new_plan.status, PlanStatus::Draft);

    // Target scene-1: narration & claims updated from mock
    assert_ne!(new_plan.scenes[0].narration, "Old narration to be replaced");
    assert_eq!(
        new_plan.scenes[0].narration,
        "Welcome to this overview of the topic."
    );

    // Target scene-1: non-narration fields are completely preserved
    assert_eq!(new_plan.scenes[0].title, "Protected Scene 1 Title");
    assert_eq!(
        new_plan.scenes[0].visual_description,
        "Protected Scene 1 Visuals"
    );
    assert_eq!(new_plan.scenes[0].media_intent, vec![MediaIntent::AiVideo]);
    assert_eq!(
        new_plan.scenes[0].stock_search_queries,
        vec!["custom query".to_string()]
    );
    assert_eq!(
        new_plan.scenes[0].image_prompt,
        Some("custom image prompt".to_string())
    );
    assert_eq!(
        new_plan.scenes[0].video_prompt,
        Some("custom video prompt".to_string())
    );
    assert_eq!(new_plan.scenes[0].duration_estimate_ms, 15000);
    assert_eq!(new_plan.scenes[0].order, 1);

    // Non-target scene-2 is completely untouched
    assert_eq!(new_plan.scenes[1].title, "Protected Scene 2 Title");
    assert_eq!(new_plan.scenes[1].narration, "Protected Scene 2 Narration");
    assert_eq!(new_plan.scenes[1].order, 2);

    // Plan aggregates canonically recomputed
    assert!(new_plan
        .full_narration
        .contains("Welcome to this overview of the topic."));
    assert!(new_plan
        .full_narration
        .contains("Protected Scene 2 Narration"));
}

#[test]
fn test_gemini_classify_http_success() {
    let body = r#"{
        "candidates": [
            {
                "content": {
                    "parts": [
                        {
                            "text": "{\"id\": \"plan-123\", \"title\": \"Test Plan\"}"
                        }
                    ],
                    "role": "model"
                },
                "finishReason": "STOP",
                "index": 0
            }
        ],
        "usageMetadata": {
            "promptTokenCount": 50,
            "candidatesTokenCount": 100,
            "totalTokenCount": 150
        }
    }"#;

    let res = classify_http_status_and_body(200, body);
    assert!(res.is_ok());
    let text = res.unwrap();
    assert_eq!(text, "{\"id\": \"plan-123\", \"title\": \"Test Plan\"}");
}

#[test]
fn test_gemini_classify_http_auth_failed() {
    let body_401 =
        r#"{"error": {"code": 401, "message": "API key not valid", "status": "UNAUTHENTICATED"}}"#;
    let err_401 = classify_http_status_and_body(401, body_401).unwrap_err();
    assert!(matches!(err_401, ProviderError::AuthFailed(_)));
    assert!(err_401.to_string().contains("API key not valid"));

    let body_403 = r#"{"error": {"code": 403, "message": "Permission denied", "status": "PERMISSION_DENIED"}}"#;
    let err_403 = classify_http_status_and_body(403, body_403).unwrap_err();
    assert!(matches!(err_403, ProviderError::AuthFailed(_)));
    assert!(err_403.to_string().contains("Permission denied"));
}

#[test]
fn test_gemini_classify_http_rate_limited() {
    let body = r#"{"error": {"code": 429, "message": "Resource has been exhausted", "status": "RESOURCE_EXHAUSTED"}}"#;
    let err = classify_http_status_and_body(429, body).unwrap_err();
    assert!(matches!(err, ProviderError::RateLimited(_)));
    assert!(err.to_string().contains("Resource has been exhausted"));
}

#[test]
fn test_gemini_classify_http_timeout() {
    let body =
        r#"{"error": {"code": 408, "message": "Request timeout", "status": "DEADLINE_EXCEEDED"}}"#;
    let err = classify_http_status_and_body(408, body).unwrap_err();
    assert!(matches!(err, ProviderError::Timeout(_)));
    assert!(err.to_string().contains("Request timeout"));
}

#[test]
fn test_gemini_classify_http_unavailable() {
    let body_500 =
        r#"{"error": {"code": 500, "message": "Internal server error", "status": "INTERNAL"}}"#;
    let err_500 = classify_http_status_and_body(500, body_500).unwrap_err();
    assert!(matches!(err_500, ProviderError::Unavailable(_)));
    assert!(err_500.to_string().contains("Internal server error"));

    let body_503 =
        r#"{"error": {"code": 503, "message": "Service unavailable", "status": "UNAVAILABLE"}}"#;
    let err_503 = classify_http_status_and_body(503, body_503).unwrap_err();
    assert!(matches!(err_503, ProviderError::Unavailable(_)));
    assert!(err_503.to_string().contains("Service unavailable"));
}

#[test]
fn test_gemini_classify_http_structured_invalid() {
    // 1. Status 400 bad request
    let body_400 = r#"{"error": {"code": 400, "message": "Invalid JSON payload", "status": "INVALID_ARGUMENT"}}"#;
    let err_400 = classify_http_status_and_body(400, body_400).unwrap_err();
    assert!(matches!(err_400, ProviderError::StructuredOutputInvalid(_)));
    assert!(err_400.to_string().contains("Invalid JSON payload"));

    // 2. Status 200 but malformed JSON body
    let err_malformed = classify_http_status_and_body(200, "not a json string").unwrap_err();
    assert!(matches!(
        err_malformed,
        ProviderError::StructuredOutputInvalid(_)
    ));

    // 3. Status 200 with empty candidates
    let body_empty_cand = r#"{"candidates": []}"#;
    let err_empty = classify_http_status_and_body(200, body_empty_cand).unwrap_err();
    assert!(matches!(
        err_empty,
        ProviderError::StructuredOutputInvalid(_)
    ));
    assert!(err_empty.to_string().contains("empty candidate response"));

    // 4. Status 200 with blank text in candidate
    let body_blank_text = r#"{"candidates": [{"content": {"parts": [{"text": "   "}]}}]}"#;
    let err_blank = classify_http_status_and_body(200, body_blank_text).unwrap_err();
    assert!(matches!(
        err_blank,
        ProviderError::StructuredOutputInvalid(_)
    ));
    assert!(err_blank.to_string().contains("empty candidate response"));
}

#[test]
fn test_gemini_error_sanitization() {
    let raw_secret_key = "AIzaSyD12345678901234567890123456789012";
    let raw_body = format!(
        r#"{{"error": {{"code": 401, "message": "Invalid API key: {}", "status": "UNAUTHENTICATED"}}}}"#,
        raw_secret_key
    );

    let err = classify_http_status_and_body(401, &raw_body).unwrap_err();
    let err_msg = err.to_string();
    assert!(
        !err_msg.contains(raw_secret_key),
        "Error message must not contain raw API key"
    );
    assert!(
        err_msg.contains("[REDACTED]"),
        "Error message should indicate redacted key"
    );

    // Test direct sanitize_error_message
    let sample = format!(
        "Request failed with key={} and secret=abcdef123456",
        raw_secret_key
    );
    let sanitized = sanitize_error_message(&sample);
    assert!(!sanitized.contains(raw_secret_key));
    assert!(!sanitized.contains("abcdef123456"));
}

#[test]
fn test_gemini_classify_http_model_unavailable() {
    // 1. Status 404 model not found / not supported
    let body_404 = r#"{"error": {"code": 404, "message": "models/gemini-2.5-flash is not found for API version v1beta, or is not supported for generateContent", "status": "NOT_FOUND"}}"#;
    let err_404 = classify_http_status_and_body(404, body_404).unwrap_err();
    assert!(matches!(err_404, ProviderError::ModelUnavailable(_)));
    assert!(err_404.to_string().contains("not found"));

    let cp_err = err_404.to_content_planning_error();
    assert_eq!(cp_err.code(), "provider_model_unavailable");
    assert!(matches!(
        cp_err,
        ContentPlanningError::ProviderModelUnavailable(_)
    ));

    // 2. Status 400 with model retirement message
    let body_400_model = r#"{"error": {"code": 400, "message": "The requested model is no longer available to new users.", "status": "INVALID_ARGUMENT"}}"#;
    let err_400_model = classify_http_status_and_body(400, body_400_model).unwrap_err();
    assert!(matches!(err_400_model, ProviderError::ModelUnavailable(_)));

    // 3. Status 400 with API key invalid maps to AuthFailed, not StructuredOutputInvalid
    let body_key_invalid = r#"{"error": {"code": 400, "message": "API key not valid. Please pass a valid API key.", "status": "INVALID_ARGUMENT"}}"#;
    let err_key = classify_http_status_and_body(400, body_key_invalid).unwrap_err();
    assert!(matches!(err_key, ProviderError::AuthFailed(_)));
}

#[test]
fn test_gemini_provider_construction() {
    let empty_config = GeminiRuntimeConfig {
        api_key: "".to_string(),
        model: "gemini-3.8-flash".to_string(),
    };
    let err = GeminiProvider::new(empty_config).unwrap_err();
    assert!(matches!(err, ProviderError::AuthMissing(_)));

    let valid_config = GeminiRuntimeConfig {
        api_key: "valid-test-key".to_string(),
        model: "gemini-3.8-flash".to_string(),
    };
    let provider = GeminiProvider::with_base_url(
        valid_config,
        "https://custom.endpoint.example.com".to_string(),
    )
    .expect("should instantiate GeminiProvider");
    assert_eq!(provider.name(), "gemini");
}
