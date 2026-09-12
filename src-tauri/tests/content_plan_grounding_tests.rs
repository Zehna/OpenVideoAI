use openvideoai_lib::content_planning::*;

fn create_test_sources() -> Vec<PlanningSource> {
    vec![
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
    ]
}

fn create_valid_grounded_plan() -> ContentPlan {
    ContentPlan {
        id: "plan-1".to_string(),
        project_id: "proj-1".to_string(),
        revision: 1,
        status: PlanStatus::Draft,
        title: "Exploring Mars".to_string(),
        summary: "Overview of robotic Mars exploration.".to_string(),
        language: "en-US".to_string(),
        tone: "informative".to_string(),
        target_duration_ms: 60_000,
        estimated_duration_ms: 60_000,
        full_narration: "Mars has captured our imagination.\n\nPerseverance explores Jezero Crater.".to_string(),
        scenes: vec![
            ContentScene {
                id: "scene-1".to_string(),
                order: 0,
                title: "Introduction".to_string(),
                narration: "Mars has captured our imagination.".to_string(),
                duration_estimate_ms: 30_000,
                visual_description: "Wide shot of Mars surface.".to_string(),
                media_intent: vec![MediaIntent::StockVideo],
                stock_search_queries: vec!["Mars red planet".to_string()],
                image_prompt: None,
                video_prompt: None,
                claims: vec![
                    Claim {
                        id: "claim-1".to_string(),
                        text: "Mars is visible in the night sky.".to_string(),
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
                title: "Robotic Mission".to_string(),
                narration: "Perseverance explores Jezero Crater.".to_string(),
                duration_estimate_ms: 30_000,
                visual_description: "Rover rovering on Mars.".to_string(),
                media_intent: vec![MediaIntent::StockVideo],
                stock_search_queries: vec!["Perseverance rover".to_string()],
                image_prompt: None,
                video_prompt: None,
                claims: vec![
                    Claim {
                        id: "claim-2".to_string(),
                        text: "Perseverance explores Jezero Crater.".to_string(),
                        grounding_status: GroundingStatus::Supported,
                        source_refs: vec![
                            SourceRef {
                                source_id: "src-1".to_string(),
                                locator: Some("p. 4".to_string()),
                                excerpt_hash: Some("ex1".to_string()),
                                note: None,
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
                        note: None,
                    },
                ],
            },
        ],
        source_refs: vec![
            SourceRef {
                source_id: "src-1".to_string(),
                locator: Some("p. 4".to_string()),
                excerpt_hash: Some("ex1".to_string()),
                note: None,
            },
        ],
        generation_metadata: None,
        created_at: "2026-09-12T10:00:00Z".to_string(),
        updated_at: "2026-09-12T10:00:00Z".to_string(),
    }
}

// ---------------------------------------------------------------------------
// 1. Whitelist Grounding Validation Tests
// ---------------------------------------------------------------------------

#[test]
fn test_validate_grounding_valid_plan() {
    let plan = create_valid_grounded_plan();
    let sources = create_test_sources();
    assert!(validate_grounding(&plan, &sources).is_ok());
}

#[test]
fn test_validate_grounding_unknown_source_on_claim() {
    let mut plan = create_valid_grounded_plan();
    let sources = create_test_sources();

    plan.scenes[1].claims[0].source_refs = vec![SourceRef {
        source_id: "unknown-src".to_string(),
        locator: None,
        excerpt_hash: None,
        note: None,
    }];

    let err = validate_grounding(&plan, &sources).unwrap_err();
    assert_eq!(err.code(), "source_grounding_invalid");
    assert!(format!("{}", err).contains("unknown-src"));
}

#[test]
fn test_validate_grounding_unknown_source_on_scene() {
    let mut plan = create_valid_grounded_plan();
    let sources = create_test_sources();

    plan.scenes[0].source_refs = vec![SourceRef {
        source_id: "nonexistent-src".to_string(),
        locator: None,
        excerpt_hash: None,
        note: None,
    }];

    let err = validate_grounding(&plan, &sources).unwrap_err();
    assert_eq!(err.code(), "source_grounding_invalid");
    assert!(format!("{}", err).contains("nonexistent-src"));
}

#[test]
fn test_validate_grounding_unknown_source_on_plan() {
    let mut plan = create_valid_grounded_plan();
    let sources = create_test_sources();

    plan.source_refs = vec![SourceRef {
        source_id: "ghost-src".to_string(),
        locator: None,
        excerpt_hash: None,
        note: None,
    }];

    let err = validate_grounding(&plan, &sources).unwrap_err();
    assert_eq!(err.code(), "source_grounding_invalid");
    assert!(format!("{}", err).contains("ghost-src"));
}

#[test]
fn test_validate_grounding_supported_claim_without_sources() {
    let mut plan = create_valid_grounded_plan();
    let sources = create_test_sources();

    // Claim is Supported but has empty source_refs
    plan.scenes[1].claims[0].source_refs = vec![];
    plan.scenes[1].claims[0].grounding_valid = true;

    let err = validate_grounding(&plan, &sources).unwrap_err();
    assert_eq!(err.code(), "source_grounding_invalid");
}

#[test]
fn test_validate_grounding_supported_claim_with_grounding_valid_false() {
    let mut plan = create_valid_grounded_plan();
    let sources = create_test_sources();

    // Claim is Supported with source_refs, but grounding_valid is false
    plan.scenes[1].claims[0].grounding_valid = false;

    let err = validate_grounding(&plan, &sources).unwrap_err();
    assert_eq!(err.code(), "source_grounding_invalid");
}

#[test]
fn test_validate_grounding_model_knowledge_with_sources() {
    let mut plan = create_valid_grounded_plan();
    let sources = create_test_sources();

    // Claim is ModelKnowledge but has source_refs
    plan.scenes[0].claims[0].source_refs = vec![SourceRef {
        source_id: "src-1".to_string(),
        locator: None,
        excerpt_hash: None,
        note: None,
    }];

    let err = validate_grounding(&plan, &sources).unwrap_err();
    assert_eq!(err.code(), "source_grounding_invalid");
    assert!(format!("{}", err).contains("model_knowledge"));
}

#[test]
fn test_validate_grounding_model_knowledge_empty_sources_valid() {
    let plan = create_valid_grounded_plan();
    let sources = create_test_sources();

    // Scene 0 Claim 0 is ModelKnowledge with empty source_refs
    assert_eq!(plan.scenes[0].claims[0].grounding_status, GroundingStatus::ModelKnowledge);
    assert!(plan.scenes[0].claims[0].source_refs.is_empty());

    assert!(validate_grounding(&plan, &sources).is_ok());
}

#[test]
fn test_validate_grounding_modified_claim_with_grounding_valid_true() {
    let mut plan = create_valid_grounded_plan();
    let sources = create_test_sources();

    // Claim is Modified but has grounding_valid = true
    plan.scenes[1].claims[0].grounding_status = GroundingStatus::Modified;
    plan.scenes[1].claims[0].grounding_valid = true;

    let err = validate_grounding(&plan, &sources).unwrap_err();
    assert_eq!(err.code(), "source_grounding_invalid");
    assert!(format!("{}", err).contains("modified claim cannot have grounding_valid = true"));
}

#[test]
fn test_validate_grounding_modified_claim_with_grounding_valid_false_valid() {
    let mut plan = create_valid_grounded_plan();
    let sources = create_test_sources();

    // Claim is Modified and grounding_valid = false (source_refs preserved)
    plan.scenes[1].claims[0].grounding_status = GroundingStatus::Modified;
    plan.scenes[1].claims[0].grounding_valid = false;

    assert!(validate_grounding(&plan, &sources).is_ok());
}

#[test]
fn test_validate_grounding_uncertain_claim_cases() {
    let mut plan = create_valid_grounded_plan();
    let sources = create_test_sources();

    // Uncertain with valid source ref is allowed
    plan.scenes[1].claims[0].grounding_status = GroundingStatus::Uncertain;
    plan.scenes[1].claims[0].grounding_valid = false;
    assert!(validate_grounding(&plan, &sources).is_ok());

    // Uncertain without source refs is allowed
    plan.scenes[1].claims[0].source_refs = vec![];
    assert!(validate_grounding(&plan, &sources).is_ok());

    // Uncertain with unknown source ref is rejected
    plan.scenes[1].claims[0].source_refs = vec![SourceRef {
        source_id: "unknown-src-999".to_string(),
        locator: None,
        excerpt_hash: None,
        note: None,
    }];
    let err = validate_grounding(&plan, &sources).unwrap_err();
    assert_eq!(err.code(), "source_grounding_invalid");
}

// ---------------------------------------------------------------------------
// 2. Deterministic Manual-Grounding Invalidation Tests
// ---------------------------------------------------------------------------

#[test]
fn test_invalidate_manual_grounding_narration_edit() {
    let base = create_valid_grounded_plan();
    let mut edited = base.clone();

    // Multiple claims in scene 2 to verify all claims in that scene get invalidated
    edited.scenes[1].claims.push(Claim {
        id: "claim-2b".to_string(),
        text: "Curiosity analyzed rock samples.".to_string(),
        grounding_status: GroundingStatus::Supported,
        source_refs: vec![SourceRef {
            source_id: "src-2".to_string(),
            locator: None,
            excerpt_hash: None,
            note: None,
        }],
        grounding_valid: true,
        note: None,
    });

    // Edit narration of scene 2
    edited.scenes[1].narration = "Perseverance thoroughly explores Jezero Crater with new instruments.".to_string();

    invalidate_manual_grounding(&base, &mut edited);

    // Scene 2 claims: both must be Modified and grounding_valid = false
    assert_eq!(edited.scenes[1].claims[0].grounding_status, GroundingStatus::Modified);
    assert_eq!(edited.scenes[1].claims[0].grounding_valid, false);
    // Historical source refs must be preserved
    assert_eq!(edited.scenes[1].claims[0].source_refs.len(), 1);
    assert_eq!(edited.scenes[1].claims[0].source_refs[0].source_id, "src-1");

    assert_eq!(edited.scenes[1].claims[1].grounding_status, GroundingStatus::Modified);
    assert_eq!(edited.scenes[1].claims[1].grounding_valid, false);
    assert_eq!(edited.scenes[1].claims[1].source_refs.len(), 1);
    assert_eq!(edited.scenes[1].claims[1].source_refs[0].source_id, "src-2");

    // Scene 1 (narration unchanged) claims must remain untouched
    assert_eq!(edited.scenes[0].claims[0].grounding_status, GroundingStatus::ModelKnowledge);
    assert_eq!(edited.scenes[0].claims[0].grounding_valid, true);
}

#[test]
fn test_invalidate_manual_grounding_direct_claim_text_edit() {
    let mut base = create_valid_grounded_plan();

    // Give scene 2 two claims
    base.scenes[1].claims.push(Claim {
        id: "claim-2b".to_string(),
        text: "Curiosity analyzed rock samples.".to_string(),
        grounding_status: GroundingStatus::Supported,
        source_refs: vec![SourceRef {
            source_id: "src-2".to_string(),
            locator: None,
            excerpt_hash: None,
            note: None,
        }],
        grounding_valid: true,
        note: None,
    });

    let mut edited = base.clone();

    // Narration is UNCHANGED. Edit only claim-2's text:
    edited.scenes[1].claims[0].text = "Perseverance discovered organic molecules in Jezero Crater.".to_string();

    invalidate_manual_grounding(&base, &mut edited);

    // claim-2 text was modified: must be marked Modified, grounding_valid = false
    assert_eq!(edited.scenes[1].claims[0].grounding_status, GroundingStatus::Modified);
    assert_eq!(edited.scenes[1].claims[0].grounding_valid, false);
    assert_eq!(edited.scenes[1].claims[0].source_refs.len(), 1);
    assert_eq!(edited.scenes[1].claims[0].source_refs[0].source_id, "src-1");

    // claim-2b text was NOT modified: must remain Supported, grounding_valid = true
    assert_eq!(edited.scenes[1].claims[1].grounding_status, GroundingStatus::Supported);
    assert_eq!(edited.scenes[1].claims[1].grounding_valid, true);
    assert_eq!(edited.scenes[1].claims[1].source_refs.len(), 1);
    assert_eq!(edited.scenes[1].claims[1].source_refs[0].source_id, "src-2");

    // Scene 1 claims also unchanged
    assert_eq!(edited.scenes[0].claims[0].grounding_status, GroundingStatus::ModelKnowledge);
    assert_eq!(edited.scenes[0].claims[0].grounding_valid, true);
}

#[test]
fn test_invalidate_manual_grounding_non_factual_edits_preserve_grounding() {
    let base = create_valid_grounded_plan();
    let mut edited = base.clone();

    // Modify ONLY non-factual fields in scene 2
    edited.scenes[1].visual_description = "Ultra high-res cinematic aerial view of rover.".to_string();
    edited.scenes[1].media_intent = vec![MediaIntent::AiVideo, MediaIntent::StockVideo];
    edited.scenes[1].stock_search_queries = vec!["Mars helicopter Ingenuity".to_string()];
    edited.scenes[1].image_prompt = Some("Rover digging on Mars surface".to_string());
    edited.scenes[1].video_prompt = Some("Cinematic camera circling Perseverance".to_string());
    edited.scenes[1].title = "Updated Title: Robotic Frontiers".to_string();
    edited.scenes[1].duration_estimate_ms = 45_000;

    // Narration is strictly unchanged
    assert_eq!(base.scenes[1].narration, edited.scenes[1].narration);

    invalidate_manual_grounding(&base, &mut edited);

    // Claim on scene 2 must remain completely intact
    assert_eq!(edited.scenes[1].claims[0].grounding_status, GroundingStatus::Supported);
    assert_eq!(edited.scenes[1].claims[0].grounding_valid, true);
    assert_eq!(edited.scenes[1].claims[0].source_refs.len(), 1);

    // Scene 1 also untouched
    assert_eq!(edited.scenes[0].claims[0].grounding_status, GroundingStatus::ModelKnowledge);
    assert_eq!(edited.scenes[0].claims[0].grounding_valid, true);
}

#[test]
fn test_invalidate_manual_grounding_new_claim_in_existing_scene() {
    let base = create_valid_grounded_plan();
    let mut edited = base.clone();

    // Add a brand new claim to scene 2
    edited.scenes[1].claims.push(Claim {
        id: "claim-new".to_string(),
        text: "Ingenuity completed dozens of flights.".to_string(),
        grounding_status: GroundingStatus::Supported, // even if someone marked it supported
        source_refs: vec![SourceRef {
            source_id: "src-1".to_string(),
            locator: None,
            excerpt_hash: None,
            note: None,
        }],
        grounding_valid: true,
        note: None,
    });

    invalidate_manual_grounding(&base, &mut edited);

    // Existing claim-2 unchanged: Supported, valid = true
    assert_eq!(edited.scenes[1].claims[0].grounding_status, GroundingStatus::Supported);
    assert_eq!(edited.scenes[1].claims[0].grounding_valid, true);

    // New claim-new: must be marked Modified, valid = false
    assert_eq!(edited.scenes[1].claims[1].grounding_status, GroundingStatus::Modified);
    assert_eq!(edited.scenes[1].claims[1].grounding_valid, false);
}

#[test]
fn test_invalidate_manual_grounding_new_scene() {
    let base = create_valid_grounded_plan();
    let mut edited = base.clone();

    // Add a new scene (scene-3)
    edited.scenes.push(ContentScene {
        id: "scene-3".to_string(),
        order: 2,
        title: "Future of Mars".to_string(),
        narration: "Human exploration is the next giant leap.".to_string(),
        duration_estimate_ms: 20_000,
        visual_description: "Futuristic Mars base rendering.".to_string(),
        media_intent: vec![MediaIntent::AiImage],
        stock_search_queries: vec![],
        image_prompt: Some("Futuristic colony on Mars".to_string()),
        video_prompt: None,
        claims: vec![
            Claim {
                id: "claim-future".to_string(),
                text: "Human missions are planned for the 2030s.".to_string(),
                grounding_status: GroundingStatus::Supported,
                source_refs: vec![SourceRef {
                    source_id: "src-1".to_string(),
                    locator: None,
                    excerpt_hash: None,
                    note: None,
                }],
                grounding_valid: true,
                note: None,
            },
        ],
        source_refs: vec![],
    });

    invalidate_manual_grounding(&base, &mut edited);

    // New scene claims must be marked Modified and valid = false
    assert_eq!(edited.scenes[2].claims[0].grounding_status, GroundingStatus::Modified);
    assert_eq!(edited.scenes[2].claims[0].grounding_valid, false);

    // Existing scenes untouched
    assert_eq!(edited.scenes[0].claims[0].grounding_status, GroundingStatus::ModelKnowledge);
    assert_eq!(edited.scenes[0].claims[0].grounding_valid, true);
    assert_eq!(edited.scenes[1].claims[0].grounding_status, GroundingStatus::Supported);
    assert_eq!(edited.scenes[1].claims[0].grounding_valid, true);
}
