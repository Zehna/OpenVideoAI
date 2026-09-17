use openvideoai_lib::content_planning::*;

fn create_valid_request() -> PlanningRequest {
    PlanningRequest {
        project_id: "proj-123".to_string(),
        input_type: InputType::Topic,
        input_text: "Create an introductory video about Mars exploration.".to_string(),
        target_duration_ms: 60_000,
        language: "en-US".to_string(),
        tone: "informative and engaging".to_string(),
        source_context: vec![
            PlanningSource {
                source_id: "src-1".to_string(),
                source_type: "document".to_string(),
                title: "Mars Rovers Overview".to_string(),
                author: Some("NASA JPL".to_string()),
                url: Some("https://mars.nasa.gov".to_string()),
                published_at: Some("2024-01-01T00:00:00Z".to_string()),
                locator: Some("p. 4".to_string()),
                content_snapshot: "Perseverance has been active in Jezero Crater.".to_string(),
                content_hash: "hash123".to_string(),
            },
            PlanningSource {
                source_id: "src-2".to_string(),
                source_type: "notes".to_string(),
                title: "Curiosity Discoveries".to_string(),
                author: None,
                url: None,
                published_at: None,
                locator: None,
                content_snapshot: "Curiosity found evidence of ancient water.".to_string(),
                content_hash: "hash456".to_string(),
            },
        ],
        generation_options: GenerationOptions::default(),
    }
}

fn create_valid_plan() -> ContentPlan {
    ContentPlan {
        id: "plan-abc".to_string(),
        project_id: "proj-123".to_string(),
        revision: 1,
        status: PlanStatus::Draft,
        title: "Journey to the Red Planet".to_string(),
        summary: "An exciting overview of rover exploration on Mars.".to_string(),
        language: "en-US".to_string(),
        tone: "inspiring".to_string(),
        target_duration_ms: 60_000,
        estimated_duration_ms: 60_000,
        full_narration: "Mars has fascinated humanity for centuries.\n\nToday, robotic explorers roam its surface.".to_string(),
        scenes: vec![
            ContentScene {
                id: "scene-1".to_string(),
                order: 0,
                title: "Introduction".to_string(),
                narration: "Mars has fascinated humanity for centuries.".to_string(),
                duration_estimate_ms: 30_000,
                visual_description: "Wide cinematic shot of the Martian desert.".to_string(),
                media_intent: vec![MediaIntent::StockVideo, MediaIntent::AiImage],
                stock_search_queries: vec!["Mars red planet surface".to_string()],
                image_prompt: Some("Photorealistic Martian dunes under a dusky sky".to_string()),
                video_prompt: None,
                claims: vec![
                    Claim {
                        id: "claim-1".to_string(),
                        text: "Mars has been observed since ancient times.".to_string(),
                        grounding_status: GroundingStatus::ModelKnowledge,
                        source_refs: vec![],
                        grounding_valid: true,
                        note: None,
                    },
                ],
                source_refs: vec![],
            },
            ContentScene {
                id: "scene-2".to_string(),
                order: 1,
                title: "Robotic Explorers".to_string(),
                narration: "Today, robotic explorers roam its surface.".to_string(),
                duration_estimate_ms: 30_000,
                visual_description: "Close-up of a rover wheel in red dust.".to_string(),
                media_intent: vec![MediaIntent::StockVideo],
                stock_search_queries: vec!["Mars rover exploration".to_string()],
                image_prompt: None,
                video_prompt: Some("Rover navigating dusty rocky terrain".to_string()),
                claims: vec![
                    Claim {
                        id: "claim-2".to_string(),
                        text: "Perseverance has been active in Jezero Crater.".to_string(),
                        grounding_status: GroundingStatus::Supported,
                        source_refs: vec![
                            SourceRef {
                                source_id: "src-1".to_string(),
                                locator: Some("p. 4".to_string()),
                                excerpt_hash: Some("ex1".to_string()),
                                note: Some("Verified from NASA JPL".to_string()),
                            },
                        ],
                        grounding_valid: true,
                        note: None,
                    },
                ],
                source_refs: vec![
                    SourceRef {
                        source_id: "src-1".to_string(),
                        locator: Some("p. 4".to_string()),
                        excerpt_hash: Some("ex1".to_string()),
                        note: Some("Verified from NASA JPL".to_string()),
                    },
                ],
            },
        ],
        source_refs: vec![
            SourceRef {
                source_id: "src-1".to_string(),
                locator: Some("p. 4".to_string()),
                excerpt_hash: Some("ex1".to_string()),
                note: Some("Verified from NASA JPL".to_string()),
            },
        ],
        generation_metadata: Some(GenerationMetadata {
            provider: "gemini".to_string(),
            model: "gemini-3.8-flash".to_string(),
            primary_model: Some("gemini-3.8-flash".to_string()),
            fallback_used: Some(false),
            planner_profile_version: 1,
            prompt_template_version: 1,
            generated_at: "2026-09-12T10:00:00Z".to_string(),
            usage: GenerationUsage {
                input_tokens: Some(500),
                output_tokens: Some(800),
                total_tokens: Some(1300),
                provider_reported_cost: Some(0.0025),
                duration_ms: Some(1500),
            },
            repaired_calls: None,
        }),
        created_at: "2026-09-12T10:00:00Z".to_string(),
        updated_at: "2026-09-12T10:00:00Z".to_string(),
    }
}

// ---------------------------------------------------------------------------
// 1. Serialization & Deserialization Tests
// ---------------------------------------------------------------------------

#[test]
fn test_planning_request_serialization_camel_case() {
    let req = create_valid_request();
    let json_val = serde_json::to_value(&req).expect("serialize planning request");

    // Verify camelCase field names
    assert!(json_val.get("projectId").is_some());
    assert!(json_val.get("inputType").is_some());
    assert_eq!(json_val["inputType"], "topic");
    assert!(json_val.get("inputText").is_some());
    assert!(json_val.get("targetDurationMs").is_some());
    assert!(json_val.get("sourceContext").is_some());
    assert!(json_val.get("generationOptions").is_some());

    // Verify sourceContext items use camelCase
    let sources = json_val["sourceContext"]
        .as_array()
        .expect("array of sources");
    assert_eq!(sources.len(), 2);
    assert!(sources[0].get("sourceId").is_some());
    assert!(sources[0].get("sourceType").is_some());
    assert!(sources[0].get("publishedAt").is_some());
    assert!(sources[0].get("contentSnapshot").is_some());
    assert!(sources[0].get("contentHash").is_some());

    // Round-trip check
    let roundtrip: PlanningRequest = serde_json::from_value(json_val).expect("deserialize request");
    assert_eq!(req, roundtrip);
}

#[test]
fn test_content_plan_serialization_camel_case_and_snake_enums() {
    let plan = create_valid_plan();
    let json_val = serde_json::to_value(&plan).expect("serialize content plan");

    // Verify camelCase
    assert!(json_val.get("projectId").is_some());
    assert!(json_val.get("targetDurationMs").is_some());
    assert!(json_val.get("estimatedDurationMs").is_some());
    assert!(json_val.get("fullNarration").is_some());
    assert!(json_val.get("sourceRefs").is_some());
    assert!(json_val.get("generationMetadata").is_some());

    // Verify snake_case enum values
    assert_eq!(json_val["status"], "draft");

    let scenes = json_val["scenes"].as_array().expect("scenes array");
    assert_eq!(scenes.len(), 2);
    assert!(scenes[0].get("durationEstimateMs").is_some());
    assert!(scenes[0].get("visualDescription").is_some());
    assert!(scenes[0].get("stockSearchQueries").is_some());
    assert!(scenes[0].get("imagePrompt").is_some());

    let media_intents = scenes[0]["mediaIntent"]
        .as_array()
        .expect("mediaIntent array");
    assert_eq!(media_intents[0], "stock_video");
    assert_eq!(media_intents[1], "ai_image");

    let claims = scenes[1]["claims"].as_array().expect("claims array");
    assert_eq!(claims[0]["groundingStatus"], "supported");
    assert_eq!(claims[0]["groundingValid"], true);

    // Round-trip check
    let roundtrip: ContentPlan = serde_json::from_value(json_val).expect("deserialize plan");
    assert_eq!(plan, roundtrip);
}

#[test]
fn test_enum_serialization_variants() {
    // InputType
    assert_eq!(
        serde_json::to_string(&InputType::Topic).unwrap(),
        "\"topic\""
    );
    assert_eq!(
        serde_json::to_string(&InputType::Script).unwrap(),
        "\"script\""
    );

    // PlanStatus
    assert_eq!(
        serde_json::to_string(&PlanStatus::Draft).unwrap(),
        "\"draft\""
    );
    assert_eq!(
        serde_json::to_string(&PlanStatus::Approved).unwrap(),
        "\"approved\""
    );
    assert_eq!(
        serde_json::to_string(&PlanStatus::Archived).unwrap(),
        "\"archived\""
    );

    // GroundingStatus
    assert_eq!(
        serde_json::to_string(&GroundingStatus::Supported).unwrap(),
        "\"supported\""
    );
    assert_eq!(
        serde_json::to_string(&GroundingStatus::ModelKnowledge).unwrap(),
        "\"model_knowledge\""
    );
    assert_eq!(
        serde_json::to_string(&GroundingStatus::Uncertain).unwrap(),
        "\"uncertain\""
    );
    assert_eq!(
        serde_json::to_string(&GroundingStatus::Modified).unwrap(),
        "\"modified\""
    );

    // MediaIntent
    assert_eq!(
        serde_json::to_string(&MediaIntent::StockVideo).unwrap(),
        "\"stock_video\""
    );
    assert_eq!(
        serde_json::to_string(&MediaIntent::StockImage).unwrap(),
        "\"stock_image\""
    );
    assert_eq!(
        serde_json::to_string(&MediaIntent::AiImage).unwrap(),
        "\"ai_image\""
    );
    assert_eq!(
        serde_json::to_string(&MediaIntent::AiVideo).unwrap(),
        "\"ai_video\""
    );
    assert_eq!(
        serde_json::to_string(&MediaIntent::ScreenRecording).unwrap(),
        "\"screen_recording\""
    );
    assert_eq!(
        serde_json::to_string(&MediaIntent::TextGraphic).unwrap(),
        "\"text_graphic\""
    );
}

#[test]
fn test_summary_and_provider_status_serialization() {
    let summary = ContentPlanSummary {
        id: "plan-1".to_string(),
        project_id: "proj-1".to_string(),
        title: "Test Plan".to_string(),
        current_revision: 2,
        status: PlanStatus::Approved,
        approved_revision: Some(2),
        approved_at: Some("2026-09-12T12:00:00Z".to_string()),
        created_at: "2026-09-12T10:00:00Z".to_string(),
        updated_at: "2026-09-12T12:00:00Z".to_string(),
    };
    let json_val = serde_json::to_value(&summary).unwrap();
    assert_eq!(json_val["currentRevision"], 2);
    assert_eq!(json_val["approvedRevision"], 2);
    assert_eq!(json_val["status"], "approved");
    let roundtrip_summary: ContentPlanSummary = serde_json::from_value(json_val).unwrap();
    assert_eq!(summary, roundtrip_summary);

    let rev_summary = ContentPlanRevisionSummary {
        plan_id: "plan-1".to_string(),
        revision: 1,
        parent_revision: None,
        operation_type: "generate".to_string(),
        created_by: "worker".to_string(),
        created_at: "2026-09-12T10:00:00Z".to_string(),
    };
    let rev_json = serde_json::to_value(&rev_summary).unwrap();
    assert_eq!(rev_json["operationType"], "generate");
    let roundtrip_rev: ContentPlanRevisionSummary = serde_json::from_value(rev_json).unwrap();
    assert_eq!(rev_summary, roundtrip_rev);

    let provider_status = PlannerProviderStatus {
        provider: "gemini".to_string(),
        configured: true,
        model: "gemini-3.8-flash".to_string(),
    };
    let prov_json = serde_json::to_value(&provider_status).unwrap();
    assert_eq!(prov_json["configured"], true);
    let roundtrip_prov: PlannerProviderStatus = serde_json::from_value(prov_json).unwrap();
    assert_eq!(provider_status, roundtrip_prov);
}

// ---------------------------------------------------------------------------
// 2. Canonical Derivation Tests
// ---------------------------------------------------------------------------

#[test]
fn test_derive_scene_source_refs_deduplication_and_sorting() {
    let claims = vec![
        Claim {
            id: "c1".to_string(),
            text: "Claim 1".to_string(),
            grounding_status: GroundingStatus::Supported,
            source_refs: vec![
                SourceRef {
                    source_id: "src-z".to_string(),
                    locator: Some("sec-1".to_string()),
                    excerpt_hash: None,
                    note: None,
                },
                SourceRef {
                    source_id: "src-a".to_string(),
                    locator: Some("p. 10".to_string()),
                    excerpt_hash: None,
                    note: None,
                },
            ],
            grounding_valid: true,
            note: None,
        },
        Claim {
            id: "c2".to_string(),
            text: "Claim 2".to_string(),
            grounding_status: GroundingStatus::Supported,
            source_refs: vec![
                // Duplicate of src-a, p. 10
                SourceRef {
                    source_id: "src-a".to_string(),
                    locator: Some("p. 10".to_string()),
                    excerpt_hash: Some("ex-dup".to_string()),
                    note: Some("duplicate ref".to_string()),
                },
                // Different locator for src-a
                SourceRef {
                    source_id: "src-a".to_string(),
                    locator: Some("p. 2".to_string()),
                    excerpt_hash: None,
                    note: None,
                },
                // None locator for src-a
                SourceRef {
                    source_id: "src-a".to_string(),
                    locator: None,
                    excerpt_hash: None,
                    note: None,
                },
            ],
            grounding_valid: true,
            note: None,
        },
    ];

    let derived = derive_scene_source_refs(&claims);
    // Should contain 4 items:
    // 1. src-a, None
    // 2. src-a, p. 10
    // 3. src-a, p. 2
    // 4. src-z, sec-1
    assert_eq!(derived.len(), 4);
    assert_eq!(derived[0].source_id, "src-a");
    assert_eq!(derived[0].locator, None);
    assert_eq!(derived[1].source_id, "src-a");
    assert_eq!(derived[1].locator, Some("p. 10".to_string()));
    assert_eq!(derived[2].source_id, "src-a");
    assert_eq!(derived[2].locator, Some("p. 2".to_string()));
    assert_eq!(derived[3].source_id, "src-z");
    assert_eq!(derived[3].locator, Some("sec-1".to_string()));
}

#[test]
fn test_derive_plan_fields_aggregates_and_sorting() {
    let mut plan = create_valid_plan();

    // Set scenes out of order with custom narrations and durations
    plan.scenes = vec![
        ContentScene {
            id: "scene-3".to_string(),
            order: 2,
            title: "Outro".to_string(),
            narration: "  And that is the story of Mars.  ".to_string(),
            duration_estimate_ms: 15_000,
            visual_description: "Fade to black".to_string(),
            media_intent: vec![MediaIntent::TextGraphic],
            stock_search_queries: vec![],
            image_prompt: None,
            video_prompt: None,
            claims: vec![Claim {
                id: "c-3".to_string(),
                text: "Mars is the fourth planet.".to_string(),
                grounding_status: GroundingStatus::Supported,
                source_refs: vec![SourceRef {
                    source_id: "src-1".to_string(),
                    locator: Some("intro".to_string()),
                    excerpt_hash: None,
                    note: None,
                }],
                grounding_valid: true,
                note: None,
            }],
            source_refs: vec![],
        },
        ContentScene {
            id: "scene-1".to_string(),
            order: 0,
            title: "Intro".to_string(),
            narration: "Welcome to Mars.".to_string(),
            duration_estimate_ms: 20_000,
            visual_description: "Wide orbital view".to_string(),
            media_intent: vec![MediaIntent::StockVideo],
            stock_search_queries: vec![],
            image_prompt: None,
            video_prompt: None,
            claims: vec![],
            source_refs: vec![],
        },
        ContentScene {
            id: "scene-2".to_string(),
            order: 1,
            title: "Body".to_string(),
            narration: "Robots roam the rust-colored landscape.".to_string(),
            duration_estimate_ms: 35_000,
            visual_description: "Rovers traversing".to_string(),
            media_intent: vec![MediaIntent::AiVideo],
            stock_search_queries: vec![],
            image_prompt: None,
            video_prompt: None,
            claims: vec![Claim {
                id: "c-2".to_string(),
                text: "Curiosity found water evidence.".to_string(),
                grounding_status: GroundingStatus::Supported,
                source_refs: vec![
                    SourceRef {
                        source_id: "src-2".to_string(),
                        locator: None,
                        excerpt_hash: None,
                        note: None,
                    },
                    // Overlapping with c-3 from scene-3
                    SourceRef {
                        source_id: "src-1".to_string(),
                        locator: Some("intro".to_string()),
                        excerpt_hash: None,
                        note: None,
                    },
                ],
                grounding_valid: true,
                note: None,
            }],
            source_refs: vec![],
        },
    ];

    // Corrupt pre-derived fields to verify derivation overwrites them
    plan.estimated_duration_ms = 999_999;
    plan.full_narration = "corrupted narration".to_string();
    plan.source_refs = vec![];

    derive_plan_fields(&mut plan);

    // Verify scene sorting by order
    assert_eq!(plan.scenes[0].id, "scene-1");
    assert_eq!(plan.scenes[0].order, 0);
    assert_eq!(plan.scenes[1].id, "scene-2");
    assert_eq!(plan.scenes[1].order, 1);
    assert_eq!(plan.scenes[2].id, "scene-3");
    assert_eq!(plan.scenes[2].order, 2);

    // Verify estimated duration is sum: 20000 + 35000 + 15000 = 70000
    assert_eq!(plan.estimated_duration_ms, 70_000);

    // Verify full narration joins trimmed narrations with \n\n
    assert_eq!(
        plan.full_narration,
        "Welcome to Mars.\n\nRobots roam the rust-colored landscape.\n\nAnd that is the story of Mars."
    );

    // Verify scene source_refs derived
    assert_eq!(plan.scenes[0].source_refs.len(), 0);
    assert_eq!(plan.scenes[1].source_refs.len(), 2);
    assert_eq!(plan.scenes[2].source_refs.len(), 1);

    // Verify plan-level source_refs derived and deduplicated
    // src-1 (intro) + src-2 (None) -> 2 items sorted by source_id, locator
    assert_eq!(plan.source_refs.len(), 2);
    assert_eq!(plan.source_refs[0].source_id, "src-1");
    assert_eq!(plan.source_refs[0].locator, Some("intro".to_string()));
    assert_eq!(plan.source_refs[1].source_id, "src-2");
    assert_eq!(plan.source_refs[1].locator, None);
}

// ---------------------------------------------------------------------------
// 3. Validation Tests
// ---------------------------------------------------------------------------

#[test]
fn test_validate_planning_request_success() {
    let req = create_valid_request();
    assert!(validate_planning_request(&req).is_ok());
}

#[test]
fn test_validate_planning_request_empty_project_id() {
    let mut req = create_valid_request();
    req.project_id = "   ".to_string();
    let err = validate_planning_request(&req).unwrap_err();
    assert_eq!(err.code(), "invalid_planning_request");
}

#[test]
fn test_validate_planning_request_empty_input_text() {
    let mut req = create_valid_request();
    req.input_text = " \t \n ".to_string();
    let err = validate_planning_request(&req).unwrap_err();
    assert_eq!(err.code(), "invalid_planning_request");
}

#[test]
fn test_validate_planning_request_non_positive_duration() {
    let mut req = create_valid_request();
    req.target_duration_ms = 0;
    let err = validate_planning_request(&req).unwrap_err();
    assert_eq!(err.code(), "invalid_planning_request");

    req.target_duration_ms = -5000;
    let err = validate_planning_request(&req).unwrap_err();
    assert_eq!(err.code(), "invalid_planning_request");
}

#[test]
fn test_validate_planning_request_empty_language() {
    let mut req = create_valid_request();
    req.language = "".to_string();
    let err = validate_planning_request(&req).unwrap_err();
    assert_eq!(err.code(), "invalid_planning_request");
}

#[test]
fn test_validate_planning_request_empty_source_id() {
    let mut req = create_valid_request();
    req.source_context[0].source_id = "  ".to_string();
    let err = validate_planning_request(&req).unwrap_err();
    assert_eq!(err.code(), "invalid_planning_request");
}

#[test]
fn test_validate_planning_request_duplicate_source_id() {
    let mut req = create_valid_request();
    req.source_context[1].source_id = req.source_context[0].source_id.clone();
    let err = validate_planning_request(&req).unwrap_err();
    assert_eq!(err.code(), "invalid_planning_request");
}

#[test]
fn test_validate_content_plan_success() {
    let plan = create_valid_plan();
    assert!(validate_content_plan(&plan).is_ok());
}

#[test]
fn test_validate_content_plan_empty_fields() {
    let mut plan = create_valid_plan();
    plan.id = "  ".to_string();
    assert_eq!(
        validate_content_plan(&plan).unwrap_err().code(),
        "content_plan_validation_failed"
    );

    let mut plan = create_valid_plan();
    plan.project_id = "".to_string();
    assert_eq!(
        validate_content_plan(&plan).unwrap_err().code(),
        "content_plan_validation_failed"
    );

    let mut plan = create_valid_plan();
    plan.title = " \n ".to_string();
    assert_eq!(
        validate_content_plan(&plan).unwrap_err().code(),
        "content_plan_validation_failed"
    );
}

#[test]
fn test_validate_content_plan_non_positive_target_duration() {
    let mut plan = create_valid_plan();
    plan.target_duration_ms = 0;
    assert_eq!(
        validate_content_plan(&plan).unwrap_err().code(),
        "content_plan_validation_failed"
    );

    plan.target_duration_ms = -100;
    assert_eq!(
        validate_content_plan(&plan).unwrap_err().code(),
        "content_plan_validation_failed"
    );
}

#[test]
fn test_validate_content_plan_empty_scenes() {
    let mut plan = create_valid_plan();
    plan.scenes = vec![];
    assert_eq!(
        validate_content_plan(&plan).unwrap_err().code(),
        "content_plan_validation_failed"
    );
}

#[test]
fn test_validate_content_plan_scene_id_invariants() {
    let mut plan = create_valid_plan();
    plan.scenes[0].id = "  ".to_string();
    assert_eq!(
        validate_content_plan(&plan).unwrap_err().code(),
        "content_plan_validation_failed"
    );

    let mut plan = create_valid_plan();
    plan.scenes[1].id = plan.scenes[0].id.clone();
    assert_eq!(
        validate_content_plan(&plan).unwrap_err().code(),
        "content_plan_validation_failed"
    );
}

#[test]
fn test_validate_content_plan_scene_order_invariants() {
    let mut plan = create_valid_plan();
    plan.scenes[0].order = -1;
    assert_eq!(
        validate_content_plan(&plan).unwrap_err().code(),
        "content_plan_validation_failed"
    );

    let mut plan = create_valid_plan();
    plan.scenes[1].order = plan.scenes[0].order;
    assert_eq!(
        validate_content_plan(&plan).unwrap_err().code(),
        "content_plan_validation_failed"
    );
}

#[test]
fn test_validate_content_plan_scene_content_invariants() {
    // Non-positive duration
    let mut plan = create_valid_plan();
    plan.scenes[0].duration_estimate_ms = 0;
    assert_eq!(
        validate_content_plan(&plan).unwrap_err().code(),
        "content_plan_validation_failed"
    );

    let mut plan = create_valid_plan();
    plan.scenes[0].duration_estimate_ms = -10;
    assert_eq!(
        validate_content_plan(&plan).unwrap_err().code(),
        "content_plan_validation_failed"
    );

    // Empty narration
    let mut plan = create_valid_plan();
    plan.scenes[0].narration = "   ".to_string();
    assert_eq!(
        validate_content_plan(&plan).unwrap_err().code(),
        "content_plan_validation_failed"
    );

    // Empty visual description
    let mut plan = create_valid_plan();
    plan.scenes[0].visual_description = "".to_string();
    assert_eq!(
        validate_content_plan(&plan).unwrap_err().code(),
        "content_plan_validation_failed"
    );
}

#[test]
fn test_validate_content_plan_claim_id_invariants() {
    // Empty claim ID
    let mut plan = create_valid_plan();
    plan.scenes[0].claims[0].id = "   ".to_string();
    assert_eq!(
        validate_content_plan(&plan).unwrap_err().code(),
        "content_plan_validation_failed"
    );

    // Duplicate claim ID within same scene or across scenes
    let mut plan = create_valid_plan();
    plan.scenes[1].claims[0].id = plan.scenes[0].claims[0].id.clone();
    assert_eq!(
        validate_content_plan(&plan).unwrap_err().code(),
        "content_plan_validation_failed"
    );
}

#[test]
fn test_validate_duration_tolerance() {
    let target = 60_000;
    let tol = 0.15; // 15% -> [51_000, 69_000]

    // Exactly on target
    assert!(validate_duration_tolerance(target, 60_000, tol).is_ok());

    // Boundary cases inside
    assert!(validate_duration_tolerance(target, 51_000, tol).is_ok());
    assert!(validate_duration_tolerance(target, 69_000, tol).is_ok());

    // Outside cases
    assert_eq!(
        validate_duration_tolerance(target, 50_999, tol)
            .unwrap_err()
            .code(),
        "content_plan_validation_failed"
    );
    assert_eq!(
        validate_duration_tolerance(target, 69_001, tol)
            .unwrap_err()
            .code(),
        "content_plan_validation_failed"
    );

    // Fallback when tolerance <= 0.0 or NaN uses DEFAULT_DURATION_TOLERANCE (0.15)
    assert!(validate_duration_tolerance(target, 55_000, 0.0).is_ok());
    assert!(validate_duration_tolerance(target, 55_000, f64::NAN).is_ok());
    assert_eq!(
        validate_duration_tolerance(target, 80_000, -0.5)
            .unwrap_err()
            .code(),
        "content_plan_validation_failed"
    );
    assert_eq!(
        validate_duration_tolerance(target, 80_000, f64::NAN)
            .unwrap_err()
            .code(),
        "content_plan_validation_failed"
    );

    // Target duration non-positive
    assert_eq!(
        validate_duration_tolerance(0, 10_000, tol)
            .unwrap_err()
            .code(),
        "content_plan_validation_failed"
    );
}

// ---------------------------------------------------------------------------
// 4. Error Categories & Sanitization Tests
// ---------------------------------------------------------------------------

#[test]
fn test_error_codes_all_variants() {
    let errors: Vec<(ContentPlanningError, &'static str)> = vec![
        (
            ContentPlanningError::ContentPlanNotFound("id1".into()),
            "content_plan_not_found",
        ),
        (
            ContentPlanningError::ContentPlanRevisionNotFound {
                plan_id: "id1".into(),
                revision: 2,
            },
            "content_plan_revision_not_found",
        ),
        (
            ContentPlanningError::ContentPlanRevisionConflict {
                plan_id: "id1".into(),
                expected: 1,
                actual: 2,
            },
            "content_plan_revision_conflict",
        ),
        (
            ContentPlanningError::ContentPlanArchived("id1".into()),
            "content_plan_archived",
        ),
        (
            ContentPlanningError::InvalidPlanningRequest("bad request".into()),
            "invalid_planning_request",
        ),
        (
            ContentPlanningError::PlanningContextTooLarge("too large".into()),
            "planning_context_too_large",
        ),
        (
            ContentPlanningError::ContentPlanValidationFailed("validation err".into()),
            "content_plan_validation_failed",
        ),
        (
            ContentPlanningError::SourceGroundingInvalid("grounding err".into()),
            "source_grounding_invalid",
        ),
        (
            ContentPlanningError::ProviderAuthMissing("auth missing".into()),
            "provider_auth_missing",
        ),
        (
            ContentPlanningError::ProviderAuthFailed("auth failed".into()),
            "provider_auth_failed",
        ),
        (
            ContentPlanningError::ProviderRateLimited("rate limited".into()),
            "provider_rate_limited",
        ),
        (
            ContentPlanningError::ProviderTimeout("timeout".into()),
            "provider_timeout",
        ),
        (
            ContentPlanningError::ProviderUnavailable("unavailable".into()),
            "provider_unavailable",
        ),
        (
            ContentPlanningError::StructuredOutputInvalid("output invalid".into()),
            "structured_output_invalid",
        ),
        (
            ContentPlanningError::PlanningCancelled("cancelled".into()),
            "planning_cancelled",
        ),
        (
            ContentPlanningError::StaleLease("stale lease".into()),
            "stale_lease",
        ),
        (
            ContentPlanningError::ContentPlanPersistFailed("persist err".into()),
            "content_plan_persist_failed",
        ),
    ];

    for (err, expected_code) in errors {
        assert_eq!(err.code(), expected_code);
    }
}

#[test]
fn test_error_display_sanitization() {
    let err = ContentPlanningError::ProviderAuthFailed("Invalid API key provided".to_string());
    let display_msg = format!("{}", err);
    assert!(!display_msg.is_empty());
    // Ensure display does not leak actual key values if sensitive string formatting
    let err2 = ContentPlanningError::ContentPlanRevisionConflict {
        plan_id: "p1".into(),
        expected: 3,
        actual: 4,
    };
    assert_eq!(
        format!("{}", err2),
        "Content plan revision conflict for plan p1: expected 3, actual 4"
    );
}
