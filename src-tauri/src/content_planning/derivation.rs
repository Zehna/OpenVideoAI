use crate::content_planning::model::{Claim, ContentPlan, ContentScene, SourceRef};

/// Collects claim-level source refs for a single scene, deduplicates by `(source_id, locator)`,
/// and stably sorts by `(source_id, locator)`.
pub fn derive_scene_source_refs(claims: &[Claim]) -> Vec<SourceRef> {
    let mut collected: Vec<SourceRef> = Vec::new();
    for claim in claims {
        for sref in &claim.source_refs {
            if !collected
                .iter()
                .any(|r| r.source_id == sref.source_id && r.locator == sref.locator)
            {
                collected.push(sref.clone());
            }
        }
    }
    collected.sort_by(|a, b| {
        a.source_id
            .cmp(&b.source_id)
            .then_with(|| a.locator.cmp(&b.locator))
    });
    collected
}

/// Collects all claim-level source refs across all scenes, deduplicates by `(source_id, locator)`,
/// and stably sorts by `(source_id, locator)`.
pub fn derive_plan_source_refs(scenes: &[ContentScene]) -> Vec<SourceRef> {
    let mut collected: Vec<SourceRef> = Vec::new();
    for scene in scenes {
        for claim in &scene.claims {
            for sref in &claim.source_refs {
                if !collected
                    .iter()
                    .any(|r| r.source_id == sref.source_id && r.locator == sref.locator)
                {
                    collected.push(sref.clone());
                }
            }
        }
    }
    collected.sort_by(|a, b| {
        a.source_id
            .cmp(&b.source_id)
            .then_with(|| a.locator.cmp(&b.locator))
    });
    collected
}

/// Canonically derives aggregate and computed fields on a `ContentPlan`:
/// 1. Sorts `plan.scenes` by `scene.order`.
/// 2. Updates scene-level `source_refs` convenience summary from claim-level references.
/// 3. Recomputes `estimated_duration_ms` as the sum of scene durations.
/// 4. Recomputes `full_narration` by joining trimmed scene narrations with `"\n\n"`.
/// 5. Recomputes plan-level `source_refs` deduplicated and stably sorted.
pub fn derive_plan_fields(plan: &mut ContentPlan) {
    plan.scenes.sort_by_key(|s| s.order);
    for scene in &mut plan.scenes {
        scene.source_refs = derive_scene_source_refs(&scene.claims);
    }
    plan.estimated_duration_ms = plan.scenes.iter().map(|s| s.duration_estimate_ms).sum();
    plan.full_narration = plan
        .scenes
        .iter()
        .map(|s| s.narration.trim())
        .collect::<Vec<_>>()
        .join("\n\n");
    plan.source_refs = derive_plan_source_refs(&plan.scenes);
}
