use std::sync::Mutex;

use openvideoai_lib::content_planning::error::ContentPlanningError;
use openvideoai_lib::content_planning::model::*;
use openvideoai_lib::content_planning::prompt::{
    build_draft_prompt, build_plan_prompt, PromptBuilder, PLANNER_PROFILE_VERSION,
    PROMPT_TEMPLATE_VERSION,
};
use openvideoai_lib::content_planning::provider::{
    AIProvider, MockOutcome, MockProvider, ProviderConfigResolver, ProviderError, ProviderRequest,
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
    assert_eq!(status_unconfigured.model, "gemini-2.5-flash");

    // 2. Set secret key
    let secret = "test-secret-key-xyz-98765";
    std::env::set_var("GEMINI_API_KEY", secret);

    let status_configured = ProviderConfigResolver::get_status();
    assert_eq!(status_configured.provider, "gemini");
    assert!(status_configured.configured);
    assert_eq!(status_configured.model, "gemini-2.5-flash");

    // Check serialization does NOT contain secret
    let json = serde_json::to_string(&status_configured).expect("must serialize");
    assert!(!json.contains(secret));

    // Check debug format does NOT contain secret
    let debug_str = format!("{:?}", status_configured);
    assert!(!debug_str.contains(secret));

    // Also verify resolve_gemini returns runtime config with secret and default model
    let runtime_config = ProviderConfigResolver::resolve_gemini().expect("must resolve");
    assert_eq!(runtime_config.api_key, secret);
    assert_eq!(runtime_config.model, "gemini-2.5-flash");

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
