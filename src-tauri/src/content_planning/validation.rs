use std::collections::HashSet;

use crate::content_planning::error::ContentPlanningError;
use crate::content_planning::model::{ContentPlan, PlanningRequest};

pub const DEFAULT_DURATION_TOLERANCE: f64 = 0.15;

/// Validates that a planning request satisfies all domain invariants:
/// - `project_id.trim()` is non-empty
/// - `input_text.trim()` is non-empty
/// - `target_duration_ms > 0`
/// - `language.trim()` is non-empty
/// - `source_context`: all `source_id`s must be non-empty and mutually unique
pub fn validate_planning_request(req: &PlanningRequest) -> Result<(), ContentPlanningError> {
    if req.project_id.trim().is_empty() {
        return Err(ContentPlanningError::InvalidPlanningRequest(
            "project_id must not be empty".to_string(),
        ));
    }
    if req.input_text.trim().is_empty() {
        return Err(ContentPlanningError::InvalidPlanningRequest(
            "input_text must not be empty".to_string(),
        ));
    }
    if req.target_duration_ms <= 0 {
        return Err(ContentPlanningError::InvalidPlanningRequest(
            "target_duration_ms must be positive".to_string(),
        ));
    }
    if req.language.trim().is_empty() {
        return Err(ContentPlanningError::InvalidPlanningRequest(
            "language must not be empty".to_string(),
        ));
    }

    let mut seen_source_ids = HashSet::new();
    for source in &req.source_context {
        if source.source_id.trim().is_empty() {
            return Err(ContentPlanningError::InvalidPlanningRequest(
                "source_id must not be empty in source_context".to_string(),
            ));
        }
        if !seen_source_ids.insert(&source.source_id) {
            return Err(ContentPlanningError::InvalidPlanningRequest(format!(
                "duplicate source_id in source_context: {}",
                source.source_id
            )));
        }
    }

    Ok(())
}

/// Validates that a `ContentPlan` satisfies all domain invariants:
/// - `plan.id`, `plan.project_id`, and `plan.title` are non-empty
/// - `plan.target_duration_ms > 0`
/// - `plan.scenes` contains at least one scene
/// - Scene IDs are non-empty and mutually unique
/// - Scene `order` values are non-negative and mutually unique
/// - Each scene's `duration_estimate_ms > 0`
/// - Each scene's `narration.trim()` is non-empty
/// - Each scene's `visual_description.trim()` is non-empty
/// - Claim IDs across all scenes are non-empty and mutually unique
pub fn validate_content_plan(plan: &ContentPlan) -> Result<(), ContentPlanningError> {
    if plan.id.trim().is_empty() {
        return Err(ContentPlanningError::ContentPlanValidationFailed(
            "plan id cannot be empty".to_string(),
        ));
    }
    if plan.project_id.trim().is_empty() {
        return Err(ContentPlanningError::ContentPlanValidationFailed(
            "plan project_id cannot be empty".to_string(),
        ));
    }
    if plan.title.trim().is_empty() {
        return Err(ContentPlanningError::ContentPlanValidationFailed(
            "plan title cannot be empty".to_string(),
        ));
    }
    if plan.target_duration_ms <= 0 {
        return Err(ContentPlanningError::ContentPlanValidationFailed(
            "plan target_duration_ms must be positive".to_string(),
        ));
    }
    if plan.scenes.is_empty() {
        return Err(ContentPlanningError::ContentPlanValidationFailed(
            "plan must contain at least one scene".to_string(),
        ));
    }

    let mut scene_ids = HashSet::new();
    let mut scene_orders = HashSet::new();
    let mut claim_ids = HashSet::new();

    for scene in &plan.scenes {
        if scene.id.trim().is_empty() {
            return Err(ContentPlanningError::ContentPlanValidationFailed(
                "scene id cannot be empty".to_string(),
            ));
        }
        if !scene_ids.insert(&scene.id) {
            return Err(ContentPlanningError::ContentPlanValidationFailed(format!(
                "duplicate scene id: {}",
                scene.id
            )));
        }
        if scene.order < 0 {
            return Err(ContentPlanningError::ContentPlanValidationFailed(format!(
                "scene order cannot be negative: {}",
                scene.order
            )));
        }
        if !scene_orders.insert(scene.order) {
            return Err(ContentPlanningError::ContentPlanValidationFailed(format!(
                "duplicate scene order: {}",
                scene.order
            )));
        }
        if scene.duration_estimate_ms <= 0 {
            return Err(ContentPlanningError::ContentPlanValidationFailed(format!(
                "scene duration_estimate_ms must be positive for scene {}",
                scene.id
            )));
        }
        if scene.narration.trim().is_empty() {
            return Err(ContentPlanningError::ContentPlanValidationFailed(format!(
                "scene narration cannot be empty for scene {}",
                scene.id
            )));
        }
        if scene.visual_description.trim().is_empty() {
            return Err(ContentPlanningError::ContentPlanValidationFailed(format!(
                "scene visual_description cannot be empty for scene {}",
                scene.id
            )));
        }

        for claim in &scene.claims {
            if claim.id.trim().is_empty() {
                return Err(ContentPlanningError::ContentPlanValidationFailed(
                    "claim id cannot be empty".to_string(),
                ));
            }
            if !claim_ids.insert(&claim.id) {
                return Err(ContentPlanningError::ContentPlanValidationFailed(format!(
                    "duplicate claim id: {}",
                    claim.id
                )));
            }
        }
    }

    Ok(())
}

/// Validates that `estimated_ms` is within the allowable tolerance range around `target_ms`:
/// `[target_ms * (1 - tol), target_ms * (1 + tol)]`.
/// If `tolerance <= 0.0`, defaults to `DEFAULT_DURATION_TOLERANCE` (15%).
pub fn validate_duration_tolerance(
    target_ms: i64,
    estimated_ms: i64,
    tolerance: f64,
) -> Result<(), ContentPlanningError> {
    if target_ms <= 0 {
        return Err(ContentPlanningError::ContentPlanValidationFailed(
            "target duration must be positive".to_string(),
        ));
    }
    let eff_tol = if tolerance <= 0.0 || tolerance.is_nan() {
        DEFAULT_DURATION_TOLERANCE
    } else {
        tolerance
    };
    let min_ms = (target_ms as f64 * (1.0 - eff_tol)).round() as i64;
    let max_ms = (target_ms as f64 * (1.0 + eff_tol)).round() as i64;

    if estimated_ms < min_ms || estimated_ms > max_ms {
        return Err(ContentPlanningError::ContentPlanValidationFailed(format!(
            "estimated duration {}ms is outside acceptable tolerance [{}ms, {}ms] for target {}ms",
            estimated_ms, min_ms, max_ms, target_ms
        )));
    }

    Ok(())
}
