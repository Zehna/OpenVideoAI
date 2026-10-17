use std::collections::{HashMap, HashSet};

use crate::content_planning::error::ContentPlanningError;
use crate::content_planning::model::{
    Claim, ContentPlan, ContentScene, GroundingStatus, PlanningSource,
};

/// Validates grounding integrity for a content plan against the supplied sources.
///
/// Rules:
/// 1. Build a whitelist of valid source IDs from `sources`.
/// 2. Validate that all `source_refs` at the plan level and scene level reference whitelisted source IDs.
/// 3. For every claim in every scene:
///    - All claim `source_refs` must reference whitelisted source IDs.
///    - `GroundingStatus::Supported`:
///      - Must have at least one source ref (`!claim.source_refs.is_empty()`).
///      - `claim.grounding_valid` must be `true`.
///    - `GroundingStatus::ModelKnowledge`:
///      - Must not cite supplied sources (`claim.source_refs.is_empty()`).
///    - `GroundingStatus::Modified`:
///      - `claim.grounding_valid` must be `false`.
///    - `GroundingStatus::Uncertain`:
///      - May or may not have source refs, but any cited source must be in the whitelist.
pub fn validate_grounding(
    plan: &ContentPlan,
    sources: &[PlanningSource],
) -> Result<(), ContentPlanningError> {
    let valid_source_ids: HashSet<&str> = sources.iter().map(|s| s.source_id.as_str()).collect();

    // 1. Check plan-level source refs
    for sref in &plan.source_refs {
        if !valid_source_ids.contains(sref.source_id.as_str()) {
            return Err(ContentPlanningError::SourceGroundingInvalid(format!(
                "plan references unknown source id: {}",
                sref.source_id
            )));
        }
    }

    // 2. Check each scene and its claims
    for scene in &plan.scenes {
        for sref in &scene.source_refs {
            if !valid_source_ids.contains(sref.source_id.as_str()) {
                return Err(ContentPlanningError::SourceGroundingInvalid(format!(
                    "scene {} references unknown source id: {}",
                    scene.id, sref.source_id
                )));
            }
        }

        for claim in &scene.claims {
            // Validate that all cited sources exist in the whitelist
            for sref in &claim.source_refs {
                if !valid_source_ids.contains(sref.source_id.as_str()) {
                    return Err(ContentPlanningError::SourceGroundingInvalid(format!(
                        "claim {} references unknown source id: {}",
                        claim.id, sref.source_id
                    )));
                }
            }

            match claim.grounding_status {
                GroundingStatus::Supported => {
                    if claim.source_refs.is_empty() {
                        return Err(ContentPlanningError::SourceGroundingInvalid(format!(
                            "supported claim {} must have at least one source ref",
                            claim.id
                        )));
                    }
                    if !claim.grounding_valid {
                        return Err(ContentPlanningError::SourceGroundingInvalid(format!(
                            "supported claim {} must have grounding_valid = true",
                            claim.id
                        )));
                    }
                }
                GroundingStatus::ModelKnowledge => {
                    if !claim.source_refs.is_empty() {
                        return Err(ContentPlanningError::SourceGroundingInvalid(
                            "model_knowledge claim must not cite supplied sources".into(),
                        ));
                    }
                }
                GroundingStatus::Modified => {
                    if claim.grounding_valid {
                        return Err(ContentPlanningError::SourceGroundingInvalid(
                            "modified claim cannot have grounding_valid = true".into(),
                        ));
                    }
                }
                GroundingStatus::Uncertain => {
                    // Allowed with or without source refs, provided any cited source was whitelisted
                }
            }
        }
    }

    Ok(())
}

/// Applies deterministic manual grounding invalidation to an edited content plan
/// based on differences against the base content plan.
///
/// Rules:
/// 1. Build lookup map of base scenes by `scene.id`.
/// 2. For each scene in `edited.scenes`:
///    - If matching base scene exists:
///      - If `base_scene.narration != edited_scene.narration`:
///        - Narration edited: all claims on this scene become `Modified` and `grounding_valid = false`.
///        - Historical `source_refs` on those claims are preserved.
///      - If `base_scene.narration == edited_scene.narration`:
///        - Build lookup map of base claims on `base_scene` by `claim.id`.
///        - For each claim in `edited_scene.claims`:
///          - If matching base claim exists:
///            - If `base_claim.text != edited_claim.text`:
///              - Direct claim text edit: `Modified`, `grounding_valid = false`.
///              - Historical `source_refs` preserved.
///            - If `base_claim.text == edited_claim.text`:
///              - Claim text unchanged: keep grounding status and validity as-is.
///          - If claim is new (not in base scene):
///            - `Modified`, `grounding_valid = false`.
///    - If scene is new (not in base plan):
///      - All claims in the new scene become `Modified` and `grounding_valid = false`.
/// 3. Non-factual edits (e.g. `visual_description`, `media_intent`, `prompts`) preserve claim grounding.
pub fn invalidate_manual_grounding(base: &ContentPlan, edited: &mut ContentPlan) {
    let base_scenes: HashMap<&str, &ContentScene> =
        base.scenes.iter().map(|s| (s.id.as_str(), s)).collect();

    for edited_scene in &mut edited.scenes {
        if let Some(base_scene) = base_scenes.get(edited_scene.id.as_str()) {
            if base_scene.narration != edited_scene.narration {
                // User edited narration -> invalidate all claims on this scene
                for claim in &mut edited_scene.claims {
                    claim.grounding_status = GroundingStatus::Modified;
                    claim.grounding_valid = false;
                }
            } else {
                // Narration unchanged -> inspect individual claims
                let base_claims: HashMap<&str, &Claim> = base_scene
                    .claims
                    .iter()
                    .map(|c| (c.id.as_str(), c))
                    .collect();

                for edited_claim in &mut edited_scene.claims {
                    if let Some(base_claim) = base_claims.get(edited_claim.id.as_str()) {
                        if base_claim.text != edited_claim.text {
                            // Direct claim text edit
                            edited_claim.grounding_status = GroundingStatus::Modified;
                            edited_claim.grounding_valid = false;
                        } else {
                            // Enforce that unchanged text preserves the base plan's status and validity
                            edited_claim.grounding_status = base_claim.grounding_status;
                            edited_claim.grounding_valid = base_claim.grounding_valid;
                        }
                    } else {
                        // New claim in existing scene
                        edited_claim.grounding_status = GroundingStatus::Modified;
                        edited_claim.grounding_valid = false;
                    }
                }
            }
        } else {
            // New scene not present in base plan -> invalidate all its claims
            for claim in &mut edited_scene.claims {
                claim.grounding_status = GroundingStatus::Modified;
                claim.grounding_valid = false;
            }
        }
    }
}
