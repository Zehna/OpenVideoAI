use openvideoai_lib::content_planning::model::{
    GenerationOptions, InputType, PlanningRequest, PlanningSource,
};
use openvideoai_lib::content_planning::planner::ContentPlanner;
use openvideoai_lib::content_planning::provider::{
    GeminiProvider, GeminiRuntimeConfig, DEFAULT_GEMINI_MODEL,
};

#[test]
fn live_gemini_content_plan_smoke() {
    let Ok(api_key) = std::env::var("GEMINI_API_KEY") else {
        eprintln!("SKIP: GEMINI_API_KEY not set");
        return;
    };
    if api_key.trim().is_empty() {
        eprintln!("SKIP: GEMINI_API_KEY is empty");
        return;
    }

    let model = std::env::var("GEMINI_MODEL")
        .ok()
        .map(|m| m.trim().to_string())
        .filter(|m| !m.is_empty())
        .unwrap_or_else(|| DEFAULT_GEMINI_MODEL.to_string());

    let config = GeminiRuntimeConfig { api_key, model };
    let provider = GeminiProvider::new(config).expect("failed to instantiate GeminiProvider");
    let planner = ContentPlanner::new(provider);

    let request = PlanningRequest {
        project_id: "proj-live-test".to_string(),
        input_type: InputType::Topic,
        input_text: "A brief history of paper".to_string(),
        target_duration_ms: 30000,
        language: "en".to_string(),
        tone: "informative".to_string(),
        source_context: vec![PlanningSource {
            source_id: "src-paper-hist".to_string(),
            source_type: "document".to_string(),
            title: "Paper History".to_string(),
            author: None,
            url: None,
            published_at: None,
            locator: None,
            content_snapshot:
                "Paper was invented in ancient China during the Han dynasty by Cai Lun.".to_string(),
            content_hash: "hashpaper123".to_string(),
        }],
        generation_options: GenerationOptions::default(),
    };

    let plan = planner
        .generate_plan(&request)
        .expect("live plan generation should succeed");

    assert!(
        !plan.scenes.is_empty(),
        "plan must contain at least one scene"
    );
    assert!(
        !plan.full_narration.is_empty(),
        "plan full narration must not be empty"
    );
    assert_eq!(plan.project_id, "proj-live-test");

    // Verify grounding citation integrity: no invented source IDs
    for scene in &plan.scenes {
        for claim in &scene.claims {
            for source_ref in &claim.source_refs {
                assert_eq!(
                    source_ref.source_id, "src-paper-hist",
                    "live provider must not invent unknown source IDs"
                );
            }
        }
    }
}
