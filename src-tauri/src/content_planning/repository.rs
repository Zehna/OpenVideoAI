use rusqlite::{params, Connection, OptionalExtension, Transaction};

use crate::content_planning::error::ContentPlanningError;
use crate::content_planning::model::{
    ContentPlan, ContentPlanRevisionSummary, ContentPlanSummary, PlanStatus, PlanningSource,
};

pub struct ContentPlanRepository;

fn parse_plan_status(s: &str) -> Result<PlanStatus, ContentPlanningError> {
    match s {
        "draft" => Ok(PlanStatus::Draft),
        "approved" => Ok(PlanStatus::Approved),
        "archived" => Ok(PlanStatus::Archived),
        other => Err(ContentPlanningError::ContentPlanPersistFailed(format!(
            "invalid plan status in database: {other}"
        ))),
    }
}

fn plan_status_str(status: PlanStatus) -> &'static str {
    match status {
        PlanStatus::Draft => "draft",
        PlanStatus::Approved => "approved",
        PlanStatus::Archived => "archived",
    }
}

fn get_summary_tx(
    tx: &Transaction,
    plan_id: &str,
) -> Result<ContentPlanSummary, ContentPlanningError> {
    let row = tx
        .query_row(
            "SELECT id, project_id, title, current_revision, status,
                    approved_revision, approved_at, created_at, updated_at
             FROM content_plans
             WHERE id = ?1;",
            params![plan_id],
            |row| {
                let status_str: String = row.get(4)?;
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, i64>(3)?,
                    status_str,
                    row.get::<_, Option<i64>>(5)?,
                    row.get::<_, Option<String>>(6)?,
                    row.get::<_, String>(7)?,
                    row.get::<_, String>(8)?,
                ))
            },
        )
        .optional()
        .map_err(|e| ContentPlanningError::ContentPlanPersistFailed(e.to_string()))?;

    let (
        id,
        project_id,
        title,
        current_revision,
        status_str,
        approved_revision,
        approved_at,
        created_at,
        updated_at,
    ) = match row {
        Some(data) => data,
        None => {
            return Err(ContentPlanningError::ContentPlanNotFound(
                plan_id.to_string(),
            ))
        }
    };

    let status = parse_plan_status(&status_str)?;
    Ok(ContentPlanSummary {
        id,
        project_id,
        title,
        current_revision,
        status,
        approved_revision,
        approved_at,
        created_at,
        updated_at,
    })
}

impl ContentPlanRepository {
    pub fn get_current(
        conn: &Connection,
        plan_id: &str,
    ) -> Result<Option<ContentPlan>, ContentPlanningError> {
        let current_revision: Option<i64> = conn
            .query_row(
                "SELECT current_revision FROM content_plans WHERE id = ?1;",
                params![plan_id],
                |row| row.get(0),
            )
            .optional()
            .map_err(|e| ContentPlanningError::ContentPlanPersistFailed(e.to_string()))?;

        match current_revision {
            Some(rev) => Self::get_revision(conn, plan_id, rev),
            None => Ok(None),
        }
    }

    pub fn get_revision(
        conn: &Connection,
        plan_id: &str,
        revision: i64,
    ) -> Result<Option<ContentPlan>, ContentPlanningError> {
        let row: Option<(String, Option<String>, String)> = conn
            .query_row(
                "SELECT r.content_json, r.generation_metadata_json, p.status
                 FROM content_plan_revisions r
                 JOIN content_plans p ON p.id = r.plan_id
                 WHERE r.plan_id = ?1 AND r.revision = ?2;",
                params![plan_id, revision],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .optional()
            .map_err(|e| ContentPlanningError::ContentPlanPersistFailed(e.to_string()))?;

        let (content_json, generation_metadata_json, status_str) = match row {
            Some(data) => data,
            None => return Ok(None),
        };

        let mut plan: ContentPlan = serde_json::from_str(&content_json)
            .map_err(|e| ContentPlanningError::ContentPlanPersistFailed(e.to_string()))?;

        if let Some(meta_json) = generation_metadata_json {
            if !meta_json.trim().is_empty() {
                let meta = serde_json::from_str(&meta_json)
                    .map_err(|e| ContentPlanningError::ContentPlanPersistFailed(e.to_string()))?;
                plan.generation_metadata = Some(meta);
            }
        }

        plan.status = parse_plan_status(&status_str)?;
        plan.revision = revision;

        Ok(Some(plan))
    }

    pub fn list_by_project(
        conn: &Connection,
        project_id: &str,
        include_archived: bool,
    ) -> Result<Vec<ContentPlanSummary>, ContentPlanningError> {
        let inc = if include_archived { 1 } else { 0 };
        let mut stmt = conn
            .prepare(
                "SELECT id, project_id, title, current_revision, status,
                        approved_revision, approved_at, created_at, updated_at
                 FROM content_plans
                 WHERE project_id = ?1 AND (?2 = 1 OR status <> 'archived')
                 ORDER BY updated_at DESC;",
            )
            .map_err(|e| ContentPlanningError::ContentPlanPersistFailed(e.to_string()))?;

        let rows = stmt
            .query_map(params![project_id, inc], |row| {
                let status_str: String = row.get(4)?;
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, i64>(3)?,
                    status_str,
                    row.get::<_, Option<i64>>(5)?,
                    row.get::<_, Option<String>>(6)?,
                    row.get::<_, String>(7)?,
                    row.get::<_, String>(8)?,
                ))
            })
            .map_err(|e| ContentPlanningError::ContentPlanPersistFailed(e.to_string()))?;

        let mut summaries = Vec::new();
        for row in rows {
            let (
                id,
                project_id,
                title,
                current_revision,
                status_str,
                approved_revision,
                approved_at,
                created_at,
                updated_at,
            ) = row.map_err(|e| ContentPlanningError::ContentPlanPersistFailed(e.to_string()))?;
            let status = parse_plan_status(&status_str)?;
            summaries.push(ContentPlanSummary {
                id,
                project_id,
                title,
                current_revision,
                status,
                approved_revision,
                approved_at,
                created_at,
                updated_at,
            });
        }

        Ok(summaries)
    }

    pub fn list_history(
        conn: &Connection,
        plan_id: &str,
    ) -> Result<Vec<ContentPlanRevisionSummary>, ContentPlanningError> {
        let mut stmt = conn
            .prepare(
                "SELECT plan_id, revision, parent_revision, operation_type, created_by, created_at
                 FROM content_plan_revisions
                 WHERE plan_id = ?1
                 ORDER BY revision ASC;",
            )
            .map_err(|e| ContentPlanningError::ContentPlanPersistFailed(e.to_string()))?;

        let rows = stmt
            .query_map(params![plan_id], |row| {
                Ok(ContentPlanRevisionSummary {
                    plan_id: row.get(0)?,
                    revision: row.get(1)?,
                    parent_revision: row.get(2)?,
                    operation_type: row.get(3)?,
                    created_by: row.get(4)?,
                    created_at: row.get(5)?,
                })
            })
            .map_err(|e| ContentPlanningError::ContentPlanPersistFailed(e.to_string()))?;

        let mut history = Vec::new();
        for row in rows {
            history.push(
                row.map_err(|e| ContentPlanningError::ContentPlanPersistFailed(e.to_string()))?,
            );
        }

        Ok(history)
    }

    pub fn get_sources(
        conn: &Connection,
        plan_id: &str,
        revision: i64,
    ) -> Result<Vec<PlanningSource>, ContentPlanningError> {
        let mut stmt = conn
            .prepare(
                "SELECT source_id, source_type, title, author, url, published_at, locator, content_snapshot, content_hash
                 FROM content_plan_sources
                 WHERE plan_id = ?1 AND revision = ?2
                 ORDER BY source_id ASC;",
            )
            .map_err(|e| ContentPlanningError::ContentPlanPersistFailed(e.to_string()))?;

        let rows = stmt
            .query_map(params![plan_id, revision], |row| {
                Ok(PlanningSource {
                    source_id: row.get(0)?,
                    source_type: row.get(1)?,
                    title: row.get(2)?,
                    author: row.get(3)?,
                    url: row.get(4)?,
                    published_at: row.get(5)?,
                    locator: row.get(6)?,
                    content_snapshot: row.get(7)?,
                    content_hash: row.get(8)?,
                })
            })
            .map_err(|e| ContentPlanningError::ContentPlanPersistFailed(e.to_string()))?;

        let mut sources = Vec::new();
        for row in rows {
            sources.push(
                row.map_err(|e| ContentPlanningError::ContentPlanPersistFailed(e.to_string()))?,
            );
        }

        Ok(sources)
    }

    pub fn create_initial_tx(
        tx: &Transaction,
        plan: &ContentPlan,
        sources: &[PlanningSource],
        created_by: &str,
    ) -> Result<(), ContentPlanningError> {
        if plan.revision != 1 {
            return Err(ContentPlanningError::ContentPlanRevisionConflict {
                plan_id: plan.id.clone(),
                expected: 1,
                actual: plan.revision,
            });
        }

        tx.execute(
            "INSERT INTO content_plans (id, project_id, title, current_revision, status, approved_revision, approved_at, created_at, updated_at)
             VALUES (?1, ?2, ?3, 1, ?4, NULL, NULL, ?5, ?6);",
            params![
                plan.id,
                plan.project_id,
                plan.title,
                plan_status_str(plan.status),
                plan.created_at,
                plan.updated_at,
            ],
        )
        .map_err(|e| ContentPlanningError::ContentPlanPersistFailed(e.to_string()))?;

        let content_json = serde_json::to_string(plan)
            .map_err(|e| ContentPlanningError::ContentPlanPersistFailed(e.to_string()))?;
        let generation_metadata_json = plan
            .generation_metadata
            .as_ref()
            .map(serde_json::to_string)
            .transpose()
            .map_err(|e| ContentPlanningError::ContentPlanPersistFailed(e.to_string()))?;

        tx.execute(
            "INSERT INTO content_plan_revisions (plan_id, revision, parent_revision, operation_type, content_json, generation_metadata_json, created_by, created_at)
             VALUES (?1, 1, NULL, 'create', ?2, ?3, ?4, ?5);",
            params![
                plan.id,
                content_json,
                generation_metadata_json,
                created_by,
                plan.created_at,
            ],
        )
        .map_err(|e| ContentPlanningError::ContentPlanPersistFailed(e.to_string()))?;

        for source in sources {
            tx.execute(
                "INSERT INTO content_plan_sources (plan_id, revision, source_id, source_type, title, author, url, published_at, locator, content_snapshot, content_hash, created_at)
                 VALUES (?1, 1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11);",
                params![
                    plan.id,
                    source.source_id,
                    source.source_type,
                    source.title,
                    source.author,
                    source.url,
                    source.published_at,
                    source.locator,
                    source.content_snapshot,
                    source.content_hash,
                    plan.created_at,
                ],
            )
            .map_err(|e| ContentPlanningError::ContentPlanPersistFailed(e.to_string()))?;
        }

        Ok(())
    }

    pub fn append_revision_tx(
        tx: &Transaction,
        plan: &ContentPlan,
        sources: &[PlanningSource],
        operation_type: &str,
        created_by: &str,
    ) -> Result<(), ContentPlanningError> {
        let row = tx
            .query_row(
                "SELECT current_revision, status FROM content_plans WHERE id = ?1;",
                params![plan.id],
                |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)),
            )
            .optional()
            .map_err(|e| ContentPlanningError::ContentPlanPersistFailed(e.to_string()))?;

        let (current_revision, status_str) = match row {
            Some(pair) => pair,
            None => return Err(ContentPlanningError::ContentPlanNotFound(plan.id.clone())),
        };

        if status_str == "archived" {
            return Err(ContentPlanningError::ContentPlanArchived(format!(
                "cannot append revision to archived plan {}",
                plan.id
            )));
        }

        let expected_rev = current_revision + 1;
        if plan.revision != expected_rev {
            return Err(ContentPlanningError::ContentPlanRevisionConflict {
                plan_id: plan.id.clone(),
                expected: expected_rev,
                actual: plan.revision,
            });
        }

        let content_json = serde_json::to_string(plan)
            .map_err(|e| ContentPlanningError::ContentPlanPersistFailed(e.to_string()))?;
        let generation_metadata_json = plan
            .generation_metadata
            .as_ref()
            .map(serde_json::to_string)
            .transpose()
            .map_err(|e| ContentPlanningError::ContentPlanPersistFailed(e.to_string()))?;

        tx.execute(
            "INSERT INTO content_plan_revisions (plan_id, revision, parent_revision, operation_type, content_json, generation_metadata_json, created_by, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8);",
            params![
                plan.id,
                plan.revision,
                current_revision,
                operation_type,
                content_json,
                generation_metadata_json,
                created_by,
                plan.updated_at,
            ],
        )
        .map_err(|e| ContentPlanningError::ContentPlanPersistFailed(e.to_string()))?;

        for source in sources {
            tx.execute(
                "INSERT INTO content_plan_sources (plan_id, revision, source_id, source_type, title, author, url, published_at, locator, content_snapshot, content_hash, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12);",
                params![
                    plan.id,
                    plan.revision,
                    source.source_id,
                    source.source_type,
                    source.title,
                    source.author,
                    source.url,
                    source.published_at,
                    source.locator,
                    source.content_snapshot,
                    source.content_hash,
                    plan.updated_at,
                ],
            )
            .map_err(|e| ContentPlanningError::ContentPlanPersistFailed(e.to_string()))?;
        }

        let rows_affected = tx
            .execute(
                "UPDATE content_plans
                 SET title = ?1, current_revision = ?2, status = ?3, updated_at = ?4
                 WHERE id = ?5 AND current_revision = ?6;",
                params![
                    plan.title,
                    plan.revision,
                    plan_status_str(plan.status),
                    plan.updated_at,
                    plan.id,
                    current_revision,
                ],
            )
            .map_err(|e| ContentPlanningError::ContentPlanPersistFailed(e.to_string()))?;

        if rows_affected != 1 {
            let actual_rev = tx
                .query_row(
                    "SELECT current_revision FROM content_plans WHERE id = ?1;",
                    params![plan.id],
                    |r| r.get::<_, i64>(0),
                )
                .optional()
                .map_err(|e| ContentPlanningError::ContentPlanPersistFailed(e.to_string()))?;

            return match actual_rev {
                Some(actual) => Err(ContentPlanningError::ContentPlanRevisionConflict {
                    plan_id: plan.id.clone(),
                    expected: current_revision,
                    actual,
                }),
                None => Err(ContentPlanningError::ContentPlanNotFound(plan.id.clone())),
            };
        }

        Ok(())
    }

    pub fn set_approval_tx(
        tx: &Transaction,
        plan_id: &str,
        expected_revision: i64,
        now: &str,
    ) -> Result<ContentPlanSummary, ContentPlanningError> {
        let rows_affected = tx
            .execute(
                "UPDATE content_plans
                 SET status = 'approved', approved_revision = ?1, approved_at = ?2, updated_at = ?2
                 WHERE id = ?3 AND current_revision = ?1 AND status <> 'archived';",
                params![expected_revision, now, plan_id],
            )
            .map_err(|e| ContentPlanningError::ContentPlanPersistFailed(e.to_string()))?;

        if rows_affected == 1 {
            return get_summary_tx(tx, plan_id);
        }

        let row = tx
            .query_row(
                "SELECT current_revision, status FROM content_plans WHERE id = ?1;",
                params![plan_id],
                |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)),
            )
            .optional()
            .map_err(|e| ContentPlanningError::ContentPlanPersistFailed(e.to_string()))?;

        match row {
            None => Err(ContentPlanningError::ContentPlanNotFound(
                plan_id.to_string(),
            )),
            Some((_, status)) if status == "archived" => {
                Err(ContentPlanningError::ContentPlanArchived(format!(
                    "cannot approve archived plan {plan_id}"
                )))
            }
            Some((actual, _)) if actual != expected_revision => {
                Err(ContentPlanningError::ContentPlanRevisionConflict {
                    plan_id: plan_id.to_string(),
                    expected: expected_revision,
                    actual,
                })
            }
            Some(_) => Err(ContentPlanningError::ContentPlanPersistFailed(
                "approval update affected 0 rows".to_string(),
            )),
        }
    }

    pub fn archive_tx(
        tx: &Transaction,
        plan_id: &str,
        expected_revision: i64,
        now: &str,
    ) -> Result<ContentPlanSummary, ContentPlanningError> {
        let rows_affected = tx
            .execute(
                "UPDATE content_plans
                 SET status = 'archived', updated_at = ?1
                 WHERE id = ?2 AND current_revision = ?3;",
                params![now, plan_id, expected_revision],
            )
            .map_err(|e| ContentPlanningError::ContentPlanPersistFailed(e.to_string()))?;

        if rows_affected == 1 {
            return get_summary_tx(tx, plan_id);
        }

        let actual_rev = tx
            .query_row(
                "SELECT current_revision FROM content_plans WHERE id = ?1;",
                params![plan_id],
                |r| r.get::<_, i64>(0),
            )
            .optional()
            .map_err(|e| ContentPlanningError::ContentPlanPersistFailed(e.to_string()))?;

        match actual_rev {
            None => Err(ContentPlanningError::ContentPlanNotFound(
                plan_id.to_string(),
            )),
            Some(actual) => Err(ContentPlanningError::ContentPlanRevisionConflict {
                plan_id: plan_id.to_string(),
                expected: expected_revision,
                actual,
            }),
        }
    }
}
