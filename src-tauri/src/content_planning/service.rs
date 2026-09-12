use chrono::Utc;
use serde_json::json;
use std::sync::Arc;

use crate::content_planning::derivation::derive_plan_fields;
use crate::content_planning::error::ContentPlanningError;
use crate::content_planning::grounding::{invalidate_manual_grounding, validate_grounding};
use crate::content_planning::model::{
    ContentPlan, ContentPlanRevisionSummary, ContentPlanSummary, PlanStatus, PlanningSource,
};
use crate::content_planning::repository::ContentPlanRepository;
use crate::content_planning::validation::{
    validate_content_plan, validate_duration_tolerance, DEFAULT_DURATION_TOLERANCE,
};
use crate::db::DbManager;
use crate::events::bus::EventBus;
use crate::events::model::DomainEvent;

pub struct ContentPlanService {
    db: Arc<DbManager>,
    event_bus: Arc<EventBus>,
}

impl ContentPlanService {
    pub fn new(db: Arc<DbManager>, event_bus: Arc<EventBus>) -> Self {
        Self { db, event_bus }
    }

    pub fn get_plan(&self, plan_id: &str) -> Result<Option<ContentPlan>, ContentPlanningError> {
        let conn = self
            .db
            .conn()
            .map_err(|e| ContentPlanningError::ContentPlanPersistFailed(e.to_string()))?;
        ContentPlanRepository::get_current(&conn, plan_id)
    }

    pub fn get_plan_revision(
        &self,
        plan_id: &str,
        revision: i64,
    ) -> Result<Option<ContentPlan>, ContentPlanningError> {
        let conn = self
            .db
            .conn()
            .map_err(|e| ContentPlanningError::ContentPlanPersistFailed(e.to_string()))?;
        ContentPlanRepository::get_revision(&conn, plan_id, revision)
    }

    pub fn list_project_plans(
        &self,
        project_id: &str,
        include_archived: bool,
    ) -> Result<Vec<ContentPlanSummary>, ContentPlanningError> {
        let conn = self
            .db
            .conn()
            .map_err(|e| ContentPlanningError::ContentPlanPersistFailed(e.to_string()))?;
        ContentPlanRepository::list_by_project(&conn, project_id, include_archived)
    }

    pub fn list_history(
        &self,
        plan_id: &str,
    ) -> Result<Vec<ContentPlanRevisionSummary>, ContentPlanningError> {
        let conn = self
            .db
            .conn()
            .map_err(|e| ContentPlanningError::ContentPlanPersistFailed(e.to_string()))?;
        ContentPlanRepository::list_history(&conn, plan_id)
    }

    pub fn save_manual_revision(
        &self,
        plan_id: &str,
        expected_revision: i64,
        mut edited: ContentPlan,
    ) -> Result<ContentPlan, ContentPlanningError> {
        let mut conn = self
            .db
            .conn()
            .map_err(|e| ContentPlanningError::ContentPlanPersistFailed(e.to_string()))?;

        // 2. Load current plan to verify it exists and is not archived
        let current = ContentPlanRepository::get_current(&conn, plan_id)?
            .ok_or_else(|| ContentPlanningError::ContentPlanNotFound(plan_id.to_string()))?;

        if current.status == PlanStatus::Archived {
            return Err(ContentPlanningError::ContentPlanArchived(format!(
                "plan {} is archived",
                plan_id
            )));
        }

        if current.revision != expected_revision {
            return Err(ContentPlanningError::ContentPlanRevisionConflict {
                plan_id: plan_id.to_string(),
                expected: expected_revision,
                actual: current.revision,
            });
        }

        // 3. Load base revision
        let base = ContentPlanRepository::get_revision(&conn, plan_id, expected_revision)?
            .ok_or_else(|| ContentPlanningError::ContentPlanRevisionNotFound {
                plan_id: plan_id.to_string(),
                revision: expected_revision,
            })?;

        // 4. Load sources from base revision
        let sources = ContentPlanRepository::get_sources(&conn, plan_id, expected_revision)?;

        // 5. Apply deterministic grounding invalidation
        invalidate_manual_grounding(&base, &mut edited);

        // 6. Recompute canonical plan fields
        derive_plan_fields(&mut edited);

        // 7. Validate domain structure
        validate_content_plan(&edited)?;

        // 8. Validate grounding integrity
        validate_grounding(&edited, &sources)?;

        // 9. Set monotonic revision and status
        edited.id = plan_id.to_string();
        edited.revision = expected_revision + 1;
        edited.status = PlanStatus::Draft;
        edited.updated_at = Utc::now().to_rfc3339();

        // 10. Begin transaction
        let tx = conn
            .transaction()
            .map_err(|e| ContentPlanningError::ContentPlanPersistFailed(e.to_string()))?;

        // 11. Append revision
        ContentPlanRepository::append_revision_tx(&tx, &edited, &sources, "manual_edit", "user")?;

        // 12. Persist domain event in same transaction
        let payload = json!({
            "planId": edited.id,
            "revision": edited.revision,
            "parentRevision": expected_revision,
            "operationType": "manual_edit",
            "status": "draft",
        });
        let event = DomainEvent::new(
            "content_plan.revision_created".to_string(),
            Some(edited.project_id.clone()),
            None,
            payload,
        );
        EventBus::record_event_tx(&tx, &event)
            .map_err(|e| ContentPlanningError::ContentPlanPersistFailed(e.to_string()))?;

        // 13. Commit transaction
        tx.commit()
            .map_err(|e| ContentPlanningError::ContentPlanPersistFailed(e.to_string()))?;

        // 14. Broadcast after commit
        self.event_bus.broadcast(&event);

        // 15. Return
        Ok(edited)
    }

    pub fn approve(
        &self,
        plan_id: &str,
        expected_revision: i64,
    ) -> Result<ContentPlanSummary, ContentPlanningError> {
        let mut conn = self
            .db
            .conn()
            .map_err(|e| ContentPlanningError::ContentPlanPersistFailed(e.to_string()))?;

        let current = ContentPlanRepository::get_current(&conn, plan_id)?
            .ok_or_else(|| ContentPlanningError::ContentPlanNotFound(plan_id.to_string()))?;

        if current.status == PlanStatus::Archived {
            return Err(ContentPlanningError::ContentPlanArchived(format!(
                "plan {} is archived",
                plan_id
            )));
        }

        if current.revision != expected_revision {
            return Err(ContentPlanningError::ContentPlanRevisionConflict {
                plan_id: plan_id.to_string(),
                expected: expected_revision,
                actual: current.revision,
            });
        }

        let sources = ContentPlanRepository::get_sources(&conn, plan_id, current.revision)?;

        validate_content_plan(&current)?;
        validate_grounding(&current, &sources)?;
        validate_duration_tolerance(
            current.target_duration_ms,
            current.estimated_duration_ms,
            DEFAULT_DURATION_TOLERANCE,
        )?;

        let now = Utc::now().to_rfc3339();

        let tx = conn
            .transaction()
            .map_err(|e| ContentPlanningError::ContentPlanPersistFailed(e.to_string()))?;

        let summary =
            ContentPlanRepository::set_approval_tx(&tx, plan_id, expected_revision, &now)?;

        let payload = json!({
            "planId": plan_id,
            "approvedRevision": expected_revision,
            "approvedAt": now,
        });
        let event = DomainEvent::new(
            "content_plan.approved".to_string(),
            Some(current.project_id.clone()),
            None,
            payload,
        );
        EventBus::record_event_tx(&tx, &event)
            .map_err(|e| ContentPlanningError::ContentPlanPersistFailed(e.to_string()))?;

        tx.commit()
            .map_err(|e| ContentPlanningError::ContentPlanPersistFailed(e.to_string()))?;

        self.event_bus.broadcast(&event);

        Ok(summary)
    }

    pub fn archive(
        &self,
        plan_id: &str,
        expected_revision: i64,
    ) -> Result<ContentPlanSummary, ContentPlanningError> {
        let mut conn = self
            .db
            .conn()
            .map_err(|e| ContentPlanningError::ContentPlanPersistFailed(e.to_string()))?;

        let current = ContentPlanRepository::get_current(&conn, plan_id)?
            .ok_or_else(|| ContentPlanningError::ContentPlanNotFound(plan_id.to_string()))?;

        if current.revision != expected_revision {
            return Err(ContentPlanningError::ContentPlanRevisionConflict {
                plan_id: plan_id.to_string(),
                expected: expected_revision,
                actual: current.revision,
            });
        }

        let now = Utc::now().to_rfc3339();

        let tx = conn
            .transaction()
            .map_err(|e| ContentPlanningError::ContentPlanPersistFailed(e.to_string()))?;

        let summary = ContentPlanRepository::archive_tx(&tx, plan_id, expected_revision, &now)?;

        let payload = json!({
            "planId": plan_id,
            "revision": expected_revision,
            "archivedAt": now,
        });
        let event = DomainEvent::new(
            "content_plan.archived".to_string(),
            Some(current.project_id.clone()),
            None,
            payload,
        );
        EventBus::record_event_tx(&tx, &event)
            .map_err(|e| ContentPlanningError::ContentPlanPersistFailed(e.to_string()))?;

        tx.commit()
            .map_err(|e| ContentPlanningError::ContentPlanPersistFailed(e.to_string()))?;

        self.event_bus.broadcast(&event);

        Ok(summary)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn publish_generated_revision(
        &self,
        plan_id: &str,
        base_revision: Option<i64>,
        mut candidate: ContentPlan,
        sources: Vec<PlanningSource>,
        operation_type: &str,
        created_by: &str,
        job_id: Option<&str>,
    ) -> Result<ContentPlan, ContentPlanningError> {
        let mut conn = self
            .db
            .conn()
            .map_err(|e| ContentPlanningError::ContentPlanPersistFailed(e.to_string()))?;

        match base_revision {
            None => {
                // Initial generation
                candidate.id = plan_id.to_string();
                candidate.revision = 1;
                candidate.status = PlanStatus::Draft;
                derive_plan_fields(&mut candidate);
                validate_content_plan(&candidate)?;
                validate_grounding(&candidate, &sources)?;

                let tx = conn
                    .transaction()
                    .map_err(|e| ContentPlanningError::ContentPlanPersistFailed(e.to_string()))?;

                ContentPlanRepository::create_initial_tx(&tx, &candidate, &sources, created_by)?;

                let payload = json!({
                    "planId": candidate.id,
                    "revision": 1,
                    "operationType": operation_type,
                    "status": "draft",
                });
                let event = DomainEvent::new(
                    "content_plan.created".to_string(),
                    Some(candidate.project_id.clone()),
                    job_id.map(str::to_string),
                    payload,
                );
                EventBus::record_event_tx(&tx, &event)
                    .map_err(|e| ContentPlanningError::ContentPlanPersistFailed(e.to_string()))?;

                tx.commit()
                    .map_err(|e| ContentPlanningError::ContentPlanPersistFailed(e.to_string()))?;

                self.event_bus.broadcast(&event);

                Ok(candidate)
            }
            Some(base_rev) => {
                // Regeneration
                candidate.id = plan_id.to_string();

                let current =
                    ContentPlanRepository::get_current(&conn, plan_id)?.ok_or_else(|| {
                        ContentPlanningError::ContentPlanNotFound(plan_id.to_string())
                    })?;

                if current.status == PlanStatus::Archived {
                    return Err(ContentPlanningError::ContentPlanArchived(format!(
                        "plan {} is archived",
                        plan_id
                    )));
                }

                if current.revision != base_rev {
                    return Err(ContentPlanningError::ContentPlanRevisionConflict {
                        plan_id: plan_id.to_string(),
                        expected: base_rev,
                        actual: current.revision,
                    });
                }

                candidate.revision = base_rev + 1;
                candidate.status = PlanStatus::Draft;
                candidate.updated_at = Utc::now().to_rfc3339();
                derive_plan_fields(&mut candidate);
                validate_content_plan(&candidate)?;
                validate_grounding(&candidate, &sources)?;

                let tx = conn
                    .transaction()
                    .map_err(|e| ContentPlanningError::ContentPlanPersistFailed(e.to_string()))?;

                ContentPlanRepository::append_revision_tx(
                    &tx,
                    &candidate,
                    &sources,
                    operation_type,
                    created_by,
                )?;

                let payload = json!({
                    "planId": candidate.id,
                    "revision": candidate.revision,
                    "parentRevision": base_rev,
                    "operationType": operation_type,
                    "status": "draft",
                });
                let event = DomainEvent::new(
                    "content_plan.revision_created".to_string(),
                    Some(candidate.project_id.clone()),
                    job_id.map(str::to_string),
                    payload,
                );
                EventBus::record_event_tx(&tx, &event)
                    .map_err(|e| ContentPlanningError::ContentPlanPersistFailed(e.to_string()))?;

                tx.commit()
                    .map_err(|e| ContentPlanningError::ContentPlanPersistFailed(e.to_string()))?;

                self.event_bus.broadcast(&event);

                Ok(candidate)
            }
        }
    }
}
