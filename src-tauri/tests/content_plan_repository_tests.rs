use openvideoai_lib::content_planning::error::ContentPlanningError;
use openvideoai_lib::content_planning::model::*;
use openvideoai_lib::content_planning::repository::ContentPlanRepository;
use openvideoai_lib::db::migrations::run_all_migrations;
use openvideoai_lib::projects::model::Project;
use openvideoai_lib::projects::repository::ProjectRepository;
use rusqlite::Connection;

fn setup_db() -> Connection {
    let mut conn = Connection::open_in_memory().unwrap();
    conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
    run_all_migrations(&mut conn).unwrap();

    let project = Project {
        id: "p_1".to_string(),
        name: "Test Project".to_string(),
        description: Some("Description".to_string()),
        created_at: "2026-09-12T10:00:00Z".to_string(),
        updated_at: "2026-09-12T10:00:00Z".to_string(),
    };
    ProjectRepository::create(&conn, &project).unwrap();

    conn
}

fn sample_plan(id: &str, project_id: &str, revision: i64) -> ContentPlan {
    ContentPlan {
        id: id.to_string(),
        project_id: project_id.to_string(),
        revision,
        status: PlanStatus::Draft,
        title: format!("Plan {id}"),
        summary: "Plan summary".to_string(),
        language: "en".to_string(),
        tone: "informative".to_string(),
        target_duration_ms: 60_000,
        estimated_duration_ms: 60_000,
        full_narration: "Narration for scene 1".to_string(),
        scenes: vec![ContentScene {
            id: "scene_1".to_string(),
            order: 1,
            title: "Scene 1".to_string(),
            narration: "Narration for scene 1".to_string(),
            duration_estimate_ms: 60_000,
            visual_description: "Visual 1".to_string(),
            media_intent: vec![MediaIntent::StockVideo],
            stock_search_queries: vec!["topic".to_string()],
            image_prompt: None,
            video_prompt: None,
            claims: vec![],
            source_refs: vec![],
        }],
        source_refs: vec![],
        generation_metadata: Some(GenerationMetadata {
            provider: "mock".to_string(),
            model: "mock-planner-v1".to_string(),
            primary_model: Some("mock-planner-v1".to_string()),
            fallback_used: Some(false),
            planner_profile_version: 1,
            prompt_template_version: 1,
            generated_at: "2026-09-12T10:00:00Z".to_string(),
            usage: GenerationUsage::default(),
            repaired_calls: None,
        }),
        created_at: "2026-09-12T10:00:00Z".to_string(),
        updated_at: "2026-09-12T10:00:00Z".to_string(),
    }
}

fn sample_source(source_id: &str, snapshot: &str) -> PlanningSource {
    PlanningSource {
        source_id: source_id.to_string(),
        source_type: "document".to_string(),
        title: format!("Source {source_id}"),
        author: Some("Author".to_string()),
        url: Some("https://example.com".to_string()),
        published_at: Some("2026-09-01T00:00:00Z".to_string()),
        locator: Some("p.1".to_string()),
        content_snapshot: snapshot.to_string(),
        content_hash: format!("hash_{snapshot}"),
    }
}

#[test]
fn test_create_initial_tx() {
    let mut conn = setup_db();
    let plan = sample_plan("cp_1", "p_1", 1);
    let sources = vec![
        sample_source("src_1", "Snapshot 1"),
        sample_source("src_2", "Snapshot 2"),
    ];

    {
        let tx = conn.transaction().unwrap();
        ContentPlanRepository::create_initial_tx(&tx, &plan, &sources, "ai_planner").unwrap();
        tx.commit().unwrap();
    }

    // Verify current plan read
    let current = ContentPlanRepository::get_current(&conn, "cp_1")
        .unwrap()
        .expect("plan should exist");
    assert_eq!(current.id, "cp_1");
    assert_eq!(current.revision, 1);
    assert_eq!(current.status, PlanStatus::Draft);
    assert_eq!(current.title, "Plan cp_1");
    assert_eq!(current.scenes.len(), 1);
    assert_eq!(
        current.generation_metadata.as_ref().unwrap().provider,
        "mock"
    );

    // Verify revision read
    let rev1 = ContentPlanRepository::get_revision(&conn, "cp_1", 1)
        .unwrap()
        .expect("revision 1 should exist");
    assert_eq!(rev1.id, "cp_1");
    assert_eq!(rev1.revision, 1);

    // Verify sources read
    let loaded_sources = ContentPlanRepository::get_sources(&conn, "cp_1", 1).unwrap();
    assert_eq!(loaded_sources.len(), 2);
    assert_eq!(loaded_sources[0].source_id, "src_1");
    assert_eq!(loaded_sources[0].content_snapshot, "Snapshot 1");
    assert_eq!(loaded_sources[1].source_id, "src_2");
    assert_eq!(loaded_sources[1].content_snapshot, "Snapshot 2");

    // Verify non-existent plan
    let none_plan = ContentPlanRepository::get_current(&conn, "cp_none").unwrap();
    assert!(none_plan.is_none());

    // Verify non-existent revision
    let none_rev = ContentPlanRepository::get_revision(&conn, "cp_1", 99).unwrap();
    assert!(none_rev.is_none());

    // Verify revision != 1 rejection on initial creation
    let invalid_plan = sample_plan("cp_invalid", "p_1", 2);
    let tx = conn.transaction().unwrap();
    let err = ContentPlanRepository::create_initial_tx(&tx, &invalid_plan, &[], "ai_planner")
        .unwrap_err();
    assert_eq!(
        err,
        ContentPlanningError::ContentPlanRevisionConflict {
            plan_id: "cp_invalid".to_string(),
            expected: 1,
            actual: 2,
        }
    );
}

#[test]
fn test_append_revision_tx_monotonic() {
    let mut conn = setup_db();
    let plan = sample_plan("cp_1", "p_1", 1);
    let sources_v1 = vec![sample_source("src_1", "Snap v1")];

    {
        let tx = conn.transaction().unwrap();
        ContentPlanRepository::create_initial_tx(&tx, &plan, &sources_v1, "ai_planner").unwrap();
        tx.commit().unwrap();
    }

    let mut plan_v2 = plan.clone();
    plan_v2.revision = 2;
    plan_v2.title = "Plan cp_1 Updated".to_string();
    plan_v2.updated_at = "2026-09-12T11:00:00Z".to_string();
    let sources_v2 = vec![sample_source("src_1", "Snap v2")];

    {
        let tx = conn.transaction().unwrap();
        ContentPlanRepository::append_revision_tx(
            &tx,
            &plan_v2,
            &sources_v2,
            "manual_edit",
            "test_user",
        )
        .unwrap();
        tx.commit().unwrap();
    }

    // Current is now revision 2
    let current = ContentPlanRepository::get_current(&conn, "cp_1")
        .unwrap()
        .unwrap();
    assert_eq!(current.revision, 2);
    assert_eq!(current.title, "Plan cp_1 Updated");

    // Revision 1 remains preserved and immutable
    let rev1 = ContentPlanRepository::get_revision(&conn, "cp_1", 1)
        .unwrap()
        .unwrap();
    assert_eq!(rev1.revision, 1);
    assert_eq!(rev1.title, "Plan cp_1");

    // Revision 2 is stored correctly
    let rev2 = ContentPlanRepository::get_revision(&conn, "cp_1", 2)
        .unwrap()
        .unwrap();
    assert_eq!(rev2.revision, 2);
    assert_eq!(rev2.title, "Plan cp_1 Updated");
}

#[test]
fn test_append_revision_tx_conflict() {
    let mut conn = setup_db();
    let plan = sample_plan("cp_1", "p_1", 1);

    {
        let tx = conn.transaction().unwrap();
        ContentPlanRepository::create_initial_tx(&tx, &plan, &[], "ai_planner").unwrap();
        tx.commit().unwrap();
    }

    // Non-monotonic skip: revision 3 when current is 1
    let mut plan_v3 = plan.clone();
    plan_v3.revision = 3;

    let tx = conn.transaction().unwrap();
    let err =
        ContentPlanRepository::append_revision_tx(&tx, &plan_v3, &[], "manual_edit", "test_user")
            .unwrap_err();
    assert_eq!(
        err,
        ContentPlanningError::ContentPlanRevisionConflict {
            plan_id: "cp_1".to_string(),
            expected: 2,
            actual: 3,
        }
    );

    // Stale revision: revision 1 when current is 1
    let err_stale =
        ContentPlanRepository::append_revision_tx(&tx, &plan, &[], "manual_edit", "test_user")
            .unwrap_err();
    assert_eq!(
        err_stale,
        ContentPlanningError::ContentPlanRevisionConflict {
            plan_id: "cp_1".to_string(),
            expected: 2,
            actual: 1,
        }
    );

    // Non-existent plan
    let mut non_existent = plan.clone();
    non_existent.id = "cp_non_existent".to_string();
    non_existent.revision = 2;
    let err_not_found = ContentPlanRepository::append_revision_tx(
        &tx,
        &non_existent,
        &[],
        "manual_edit",
        "test_user",
    )
    .unwrap_err();
    assert_eq!(
        err_not_found,
        ContentPlanningError::ContentPlanNotFound("cp_non_existent".to_string())
    );
}

#[test]
fn test_append_revision_to_archived_plan() {
    let mut conn = setup_db();
    let plan = sample_plan("cp_1", "p_1", 1);

    {
        let tx = conn.transaction().unwrap();
        ContentPlanRepository::create_initial_tx(&tx, &plan, &[], "ai_planner").unwrap();
        ContentPlanRepository::archive_tx(&tx, "cp_1", 1, "2026-09-12T11:00:00Z").unwrap();
        tx.commit().unwrap();
    }

    let mut plan_v2 = plan.clone();
    plan_v2.revision = 2;

    let tx = conn.transaction().unwrap();
    let err =
        ContentPlanRepository::append_revision_tx(&tx, &plan_v2, &[], "manual_edit", "test_user")
            .unwrap_err();
    assert!(matches!(err, ContentPlanningError::ContentPlanArchived(_)));
}

#[test]
fn test_revision_scoped_sources() {
    let mut conn = setup_db();
    let plan = sample_plan("cp_1", "p_1", 1);
    let sources_v1 = vec![sample_source("src_common", "Content Snapshot V1")];

    {
        let tx = conn.transaction().unwrap();
        ContentPlanRepository::create_initial_tx(&tx, &plan, &sources_v1, "ai_planner").unwrap();
        tx.commit().unwrap();
    }

    let mut plan_v2 = plan.clone();
    plan_v2.revision = 2;
    let sources_v2 = vec![sample_source("src_common", "Content Snapshot V2")];

    {
        let tx = conn.transaction().unwrap();
        ContentPlanRepository::append_revision_tx(
            &tx,
            &plan_v2,
            &sources_v2,
            "regenerate",
            "ai_planner",
        )
        .unwrap();
        tx.commit().unwrap();
    }

    let loaded_v1 = ContentPlanRepository::get_sources(&conn, "cp_1", 1).unwrap();
    assert_eq!(loaded_v1.len(), 1);
    assert_eq!(loaded_v1[0].source_id, "src_common");
    assert_eq!(loaded_v1[0].content_snapshot, "Content Snapshot V1");

    let loaded_v2 = ContentPlanRepository::get_sources(&conn, "cp_1", 2).unwrap();
    assert_eq!(loaded_v2.len(), 1);
    assert_eq!(loaded_v2[0].source_id, "src_common");
    assert_eq!(loaded_v2[0].content_snapshot, "Content Snapshot V2");
}

#[test]
fn test_list_by_project() {
    let mut conn = setup_db();
    let plan1 = sample_plan("cp_1", "p_1", 1);
    let mut plan2 = sample_plan("cp_2", "p_1", 1);
    plan2.title = "Second Plan".to_string();

    {
        let tx = conn.transaction().unwrap();
        ContentPlanRepository::create_initial_tx(&tx, &plan1, &[], "user").unwrap();
        ContentPlanRepository::create_initial_tx(&tx, &plan2, &[], "user").unwrap();
        ContentPlanRepository::archive_tx(&tx, "cp_2", 1, "2026-09-12T10:30:00Z").unwrap();
        tx.commit().unwrap();
    }

    // Exclude archived (default listing behavior)
    let active_plans = ContentPlanRepository::list_by_project(&conn, "p_1", false).unwrap();
    assert_eq!(active_plans.len(), 1);
    assert_eq!(active_plans[0].id, "cp_1");
    assert_eq!(active_plans[0].status, PlanStatus::Draft);
    assert_eq!(active_plans[0].title, "Plan cp_1");
    assert_eq!(active_plans[0].current_revision, 1);

    // Include archived
    let all_plans = ContentPlanRepository::list_by_project(&conn, "p_1", true).unwrap();
    assert_eq!(all_plans.len(), 2);
    let ids: Vec<&str> = all_plans.iter().map(|p| p.id.as_str()).collect();
    assert!(ids.contains(&"cp_1"));
    assert!(ids.contains(&"cp_2"));
    let archived_summary = all_plans.iter().find(|p| p.id == "cp_2").unwrap();
    assert_eq!(archived_summary.status, PlanStatus::Archived);
}

#[test]
fn test_list_history() {
    let mut conn = setup_db();
    let plan = sample_plan("cp_1", "p_1", 1);

    {
        let tx = conn.transaction().unwrap();
        ContentPlanRepository::create_initial_tx(&tx, &plan, &[], "creator_1").unwrap();
        tx.commit().unwrap();
    }

    let mut plan_v2 = plan.clone();
    plan_v2.revision = 2;
    plan_v2.updated_at = "2026-09-12T11:00:00Z".to_string();

    {
        let tx = conn.transaction().unwrap();
        ContentPlanRepository::append_revision_tx(&tx, &plan_v2, &[], "manual_edit", "editor_2")
            .unwrap();
        tx.commit().unwrap();
    }

    let mut plan_v3 = plan_v2.clone();
    plan_v3.revision = 3;
    plan_v3.updated_at = "2026-09-12T12:00:00Z".to_string();

    {
        let tx = conn.transaction().unwrap();
        ContentPlanRepository::append_revision_tx(&tx, &plan_v3, &[], "regenerate", "ai_planner")
            .unwrap();
        tx.commit().unwrap();
    }

    let history = ContentPlanRepository::list_history(&conn, "cp_1").unwrap();
    assert_eq!(history.len(), 3);

    assert_eq!(history[0].revision, 1);
    assert_eq!(history[0].parent_revision, None);
    assert_eq!(history[0].operation_type, "create");
    assert_eq!(history[0].created_by, "creator_1");

    assert_eq!(history[1].revision, 2);
    assert_eq!(history[1].parent_revision, Some(1));
    assert_eq!(history[1].operation_type, "manual_edit");
    assert_eq!(history[1].created_by, "editor_2");

    assert_eq!(history[2].revision, 3);
    assert_eq!(history[2].parent_revision, Some(2));
    assert_eq!(history[2].operation_type, "regenerate");
    assert_eq!(history[2].created_by, "ai_planner");
}

#[test]
fn test_set_approval_tx() {
    let mut conn = setup_db();
    let plan = sample_plan("cp_1", "p_1", 1);

    {
        let tx = conn.transaction().unwrap();
        ContentPlanRepository::create_initial_tx(&tx, &plan, &[], "creator").unwrap();
        tx.commit().unwrap();
    }

    // Success approval
    {
        let tx = conn.transaction().unwrap();
        let summary =
            ContentPlanRepository::set_approval_tx(&tx, "cp_1", 1, "2026-09-12T10:15:00Z").unwrap();
        assert_eq!(summary.status, PlanStatus::Approved);
        assert_eq!(summary.approved_revision, Some(1));
        assert_eq!(
            summary.approved_at,
            Some("2026-09-12T10:15:00Z".to_string())
        );
        tx.commit().unwrap();
    }

    // Verify synced status on plan
    let approved_plan = ContentPlanRepository::get_current(&conn, "cp_1")
        .unwrap()
        .unwrap();
    assert_eq!(approved_plan.status, PlanStatus::Approved);

    // Revision conflict on approval
    {
        let tx = conn.transaction().unwrap();
        let err = ContentPlanRepository::set_approval_tx(&tx, "cp_1", 2, "2026-09-12T10:20:00Z")
            .unwrap_err();
        assert_eq!(
            err,
            ContentPlanningError::ContentPlanRevisionConflict {
                plan_id: "cp_1".to_string(),
                expected: 2,
                actual: 1,
            }
        );
    }

    // Archive and then attempt approval -> ContentPlanArchived
    {
        let tx = conn.transaction().unwrap();
        ContentPlanRepository::archive_tx(&tx, "cp_1", 1, "2026-09-12T10:30:00Z").unwrap();
        let err = ContentPlanRepository::set_approval_tx(&tx, "cp_1", 1, "2026-09-12T10:35:00Z")
            .unwrap_err();
        assert!(matches!(err, ContentPlanningError::ContentPlanArchived(_)));
    }

    // Not found
    {
        let tx = conn.transaction().unwrap();
        let err = ContentPlanRepository::set_approval_tx(
            &tx,
            "cp_non_existent",
            1,
            "2026-09-12T10:40:00Z",
        )
        .unwrap_err();
        assert_eq!(
            err,
            ContentPlanningError::ContentPlanNotFound("cp_non_existent".to_string())
        );
    }
}

#[test]
fn test_archive_tx() {
    let mut conn = setup_db();
    let plan = sample_plan("cp_1", "p_1", 1);

    {
        let tx = conn.transaction().unwrap();
        ContentPlanRepository::create_initial_tx(&tx, &plan, &[], "creator").unwrap();
        tx.commit().unwrap();
    }

    // Revision conflict on archive
    {
        let tx = conn.transaction().unwrap();
        let err =
            ContentPlanRepository::archive_tx(&tx, "cp_1", 99, "2026-09-12T10:20:00Z").unwrap_err();
        assert_eq!(
            err,
            ContentPlanningError::ContentPlanRevisionConflict {
                plan_id: "cp_1".to_string(),
                expected: 99,
                actual: 1,
            }
        );
    }

    // Success archive
    {
        let tx = conn.transaction().unwrap();
        let summary =
            ContentPlanRepository::archive_tx(&tx, "cp_1", 1, "2026-09-12T10:25:00Z").unwrap();
        assert_eq!(summary.status, PlanStatus::Archived);
        tx.commit().unwrap();
    }

    let archived_plan = ContentPlanRepository::get_current(&conn, "cp_1")
        .unwrap()
        .unwrap();
    assert_eq!(archived_plan.status, PlanStatus::Archived);

    // Not found
    {
        let tx = conn.transaction().unwrap();
        let err =
            ContentPlanRepository::archive_tx(&tx, "cp_non_existent", 1, "2026-09-12T10:30:00Z")
                .unwrap_err();
        assert_eq!(
            err,
            ContentPlanningError::ContentPlanNotFound("cp_non_existent".to_string())
        );
    }
}

#[test]
fn test_project_cascade_delete() {
    let mut conn = setup_db();
    let plan = sample_plan("cp_1", "p_1", 1);
    let sources = vec![sample_source("src_1", "Snapshot")];

    {
        let tx = conn.transaction().unwrap();
        ContentPlanRepository::create_initial_tx(&tx, &plan, &sources, "creator").unwrap();
        tx.commit().unwrap();
    }

    let mut plan_v2 = plan.clone();
    plan_v2.revision = 2;
    {
        let tx = conn.transaction().unwrap();
        ContentPlanRepository::append_revision_tx(&tx, &plan_v2, &sources, "manual_edit", "editor")
            .unwrap();
        tx.commit().unwrap();
    }

    // Verify rows exist before delete
    let plan_count: i64 = conn
        .query_row(
            "SELECT count(*) FROM content_plans WHERE project_id = 'p_1';",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(plan_count, 1);

    let rev_count: i64 = conn
        .query_row(
            "SELECT count(*) FROM content_plan_revisions WHERE plan_id = 'cp_1';",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(rev_count, 2);

    let src_count: i64 = conn
        .query_row(
            "SELECT count(*) FROM content_plan_sources WHERE plan_id = 'cp_1';",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(src_count, 2);

    // Cascade delete project
    conn.execute("DELETE FROM projects WHERE id = 'p_1';", [])
        .unwrap();

    let plan_count_after: i64 = conn
        .query_row(
            "SELECT count(*) FROM content_plans WHERE id = 'cp_1';",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(plan_count_after, 0);

    let rev_count_after: i64 = conn
        .query_row(
            "SELECT count(*) FROM content_plan_revisions WHERE plan_id = 'cp_1';",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(rev_count_after, 0);

    let src_count_after: i64 = conn
        .query_row(
            "SELECT count(*) FROM content_plan_sources WHERE plan_id = 'cp_1';",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(src_count_after, 0);
}
