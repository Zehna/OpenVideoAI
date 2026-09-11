use openvideoai_lib::db::migrations::run_all_migrations;
use openvideoai_lib::projects::model::Project;
use openvideoai_lib::projects::repository::ProjectRepository;
use openvideoai_lib::timeline::error::TimelineError;
use openvideoai_lib::timeline::model::*;
use openvideoai_lib::timeline::mutation::TimelineMutation;
use openvideoai_lib::timeline::repository::TimelineRepository;
use openvideoai_lib::timeline::service::TimelineService;
use rusqlite::Connection;

fn setup_test_db() -> (Connection, String) {
    let mut conn = Connection::open_in_memory().unwrap();
    conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
    run_all_migrations(&mut conn).unwrap();

    let project = Project {
        id: "p1".to_string(),
        name: "Test Project".to_string(),
        description: None,
        created_at: "2026-09-10T12:00:00Z".to_string(),
        updated_at: "2026-09-10T12:00:00Z".to_string(),
    };
    ProjectRepository::create(&conn, &project).unwrap();

    let canvas = CanvasConfig {
        width: 1920,
        height: 1080,
        frame_rate: 30.0,
    };
    TimelineRepository::create(&mut conn, "tl1", "p1", "Main Timeline", canvas).unwrap();

    (conn, "tl1".to_string())
}

#[test]
fn test_first_edit_can_be_undone_to_initial_blank_timeline() {
    let (mut conn, tl_id) = setup_test_db();
    let service = TimelineService::new();

    // 1. Initial timeline: revision = 1, tracks = []
    let initial = TimelineRepository::get_by_id(&conn, &tl_id)
        .unwrap()
        .unwrap();
    assert_eq!(initial.revision, 1);
    assert_eq!(initial.tracks.len(), 0);

    // 2. First edit: AddTrack -> revision = 2
    let after_edit = service
        .apply_mutation(
            &mut conn,
            &tl_id,
            1,
            TimelineMutation::AddTrack {
                kind: TrackKind::Video,
                name: "V1".to_string(),
            },
        )
        .unwrap();
    assert_eq!(after_edit.revision, 2);
    assert_eq!(after_edit.tracks.len(), 1);

    // 3. Undo with expected_revision = 2 -> must succeed!
    // Invariant: revision strictly increments to 3, state restores to 0 tracks
    let undone = service.undo(&mut conn, &tl_id, 2).unwrap();
    assert_eq!(undone.revision, 3);
    assert_eq!(
        undone.tracks.len(),
        0,
        "Initial blank state must be restored on undoing first edit"
    );

    // 4. Redo with expected_revision = 3 -> must restore V1 track at revision 4
    let redone = service.redo(&mut conn, &tl_id, 3).unwrap();
    assert_eq!(redone.revision, 4);
    assert_eq!(redone.tracks.len(), 1);
    assert_eq!(redone.tracks[0].name, "V1");
}

#[test]
fn test_undo_and_redo_strictly_monotonically_increment_revision() {
    let (mut conn, tl_id) = setup_test_db();
    let service = TimelineService::new();

    // Rev 1 -> 2
    service
        .apply_mutation(
            &mut conn,
            &tl_id,
            1,
            TimelineMutation::AddTrack {
                kind: TrackKind::Video,
                name: "V1".to_string(),
            },
        )
        .unwrap();

    // Rev 2 -> 3
    service
        .apply_mutation(
            &mut conn,
            &tl_id,
            2,
            TimelineMutation::AddTrack {
                kind: TrackKind::Audio,
                name: "A1".to_string(),
            },
        )
        .unwrap();

    // Undo: Rev 3 -> 4 (restores V1 only)
    let doc = service.undo(&mut conn, &tl_id, 3).unwrap();
    assert_eq!(doc.revision, 4);
    assert_eq!(doc.tracks.len(), 1);
    assert_eq!(doc.tracks[0].name, "V1");

    // Redo: Rev 4 -> 5 (restores V1 + A1)
    let doc = service.redo(&mut conn, &tl_id, 4).unwrap();
    assert_eq!(doc.revision, 5);
    assert_eq!(doc.tracks.len(), 2);
}

#[test]
fn test_undo_redo_enforces_optimistic_revision_concurrency() {
    let (mut conn, tl_id) = setup_test_db();
    let service = TimelineService::new();

    // Rev 1 -> 2
    service
        .apply_mutation(
            &mut conn,
            &tl_id,
            1,
            TimelineMutation::AddTrack {
                kind: TrackKind::Video,
                name: "V1".to_string(),
            },
        )
        .unwrap();

    // Stale undo with expected_revision = 99 -> must fail with RevisionConflict
    let err = service.undo(&mut conn, &tl_id, 99).unwrap_err();
    assert_eq!(
        err,
        TimelineError::RevisionConflict {
            expected: 99,
            actual: 2
        }
    );

    // Correct undo -> Rev 2 -> 3
    service.undo(&mut conn, &tl_id, 2).unwrap();

    // Stale redo with expected_revision = 2 -> must fail with RevisionConflict
    let err = service.redo(&mut conn, &tl_id, 2).unwrap_err();
    assert_eq!(
        err,
        TimelineError::RevisionConflict {
            expected: 2,
            actual: 3
        }
    );
}

#[test]
fn test_new_edit_after_undo_prunes_redo_branch() {
    let (mut conn, tl_id) = setup_test_db();
    let service = TimelineService::new();

    // Edit 1: V1 (rev 2)
    service
        .apply_mutation(
            &mut conn,
            &tl_id,
            1,
            TimelineMutation::AddTrack {
                kind: TrackKind::Video,
                name: "V1".to_string(),
            },
        )
        .unwrap();

    // Edit 2: V2 (rev 3)
    service
        .apply_mutation(
            &mut conn,
            &tl_id,
            2,
            TimelineMutation::AddTrack {
                kind: TrackKind::Video,
                name: "V2".to_string(),
            },
        )
        .unwrap();

    // Undo back to V1 (rev 4)
    service.undo(&mut conn, &tl_id, 3).unwrap();

    // New edit: A1 (rev 5). This MUST invalidate the redo branch (V2)
    service
        .apply_mutation(
            &mut conn,
            &tl_id,
            4,
            TimelineMutation::AddTrack {
                kind: TrackKind::Audio,
                name: "A1".to_string(),
            },
        )
        .unwrap();

    // Attempt redo on rev 5 -> must return InvalidOperation("Nothing to redo")
    let err = service.redo(&mut conn, &tl_id, 5).unwrap_err();
    assert_eq!(
        err,
        TimelineError::InvalidOperation("Nothing to redo".to_string())
    );
}

#[test]
fn test_history_bounded_to_50_snapshots() {
    let (mut conn, tl_id) = setup_test_db();
    let service = TimelineService::new();

    // Apply 55 canvas update mutations
    let mut current_rev = 1;
    for i in 0..55 {
        let doc = service
            .apply_mutation(
                &mut conn,
                &tl_id,
                current_rev,
                TimelineMutation::UpdateCanvas {
                    width: 1920,
                    height: 1080,
                    frame_rate: 30.0 + (i as f64 * 0.1),
                },
            )
            .unwrap();
        current_rev = doc.revision;
    }

    let count: i64 = conn
        .query_row(
            "SELECT count(*) FROM timeline_history WHERE timeline_id = ?1;",
            [&tl_id],
            |r| r.get(0),
        )
        .unwrap();
    assert!(
        count <= 50,
        "History snapshot count must be capped at 50, found: {}",
        count
    );

    // Verify current history cursor snapshot is present
    let cursor: i64 = conn
        .query_row(
            "SELECT history_cursor FROM timelines WHERE id = ?1;",
            [&tl_id],
            |r| r.get(0),
        )
        .unwrap();

    let cursor_exists: bool = conn
        .query_row(
            "SELECT count(*) > 0 FROM timeline_history WHERE timeline_id = ?1 AND history_index = ?2;",
            rusqlite::params![&tl_id, cursor],
            |r| r.get(0),
        )
        .unwrap();
    assert!(
        cursor_exists,
        "Current cursor snapshot must never be pruned"
    );
}
