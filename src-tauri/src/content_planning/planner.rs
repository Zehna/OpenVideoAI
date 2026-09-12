use crate::content_planning::derivation::derive_plan_fields;
use crate::content_planning::error::ContentPlanningError;
use crate::content_planning::grounding::validate_grounding;
use crate::content_planning::model::{
    Claim, ContentPlan, ContentScene, GenerationMetadata, PlanStatus, PlanningRequest,
    PlanningSource, SourceRef,
};
use crate::content_planning::prompt::PromptBuilder;
use crate::content_planning::provider::{AIProvider, ProviderError};
use crate::content_planning::validation::{
    validate_content_plan, validate_duration_tolerance, validate_planning_request,
    DEFAULT_DURATION_TOLERANCE,
};

pub const MAX_MODEL_CALLS_PER_STAGE: usize = 3;

fn classify_provider_error(err: ProviderError) -> Result<String, ContentPlanningError> {
    match err {
        ProviderError::AuthMissing(m) => Err(ContentPlanningError::ProviderAuthMissing(m)),
        ProviderError::AuthFailed(m) => Err(ContentPlanningError::ProviderAuthFailed(m)),
        ProviderError::RateLimited(m) => Err(ContentPlanningError::ProviderRateLimited(m)),
        ProviderError::Timeout(m) => Err(ContentPlanningError::ProviderTimeout(m)),
        ProviderError::Unavailable(m) => Err(ContentPlanningError::ProviderUnavailable(m)),
        ProviderError::Transport(m) => Err(ContentPlanningError::ProviderUnavailable(m)),
        ProviderError::StructuredOutputInvalid(m) => Ok(m),
    }
}

fn sanitize_error_message(msg: &str) -> String {
    let first_line = msg.lines().next().unwrap_or(msg);
    first_line.to_string()
}

pub struct ContentPlanner<P: AIProvider> {
    provider: P,
}

impl<P: AIProvider> ContentPlanner<P> {
    pub fn new(provider: P) -> Self {
        Self { provider }
    }

    pub fn provider(&self) -> &P {
        &self.provider
    }

    fn execute_draft_stage(
        &self,
        request: &PlanningRequest,
    ) -> Result<String, ContentPlanningError> {
        let mut prompt = PromptBuilder::build_draft_prompt(request);
        for attempt in 0..MAX_MODEL_CALLS_PER_STAGE {
            match self.provider.generate_planning_draft(&prompt) {
                Ok(resp) => {
                    let trimmed = resp.text.trim();
                    if trimmed.is_empty() || trimmed.contains("<<incomplete>>") {
                        if attempt + 1 >= MAX_MODEL_CALLS_PER_STAGE {
                            return Err(ContentPlanningError::StructuredOutputInvalid(
                                "Draft generation failed: incomplete or empty draft".to_string(),
                            ));
                        }
                        prompt = PromptBuilder::build_repair_prompt(
                            &resp.text,
                            "Draft outline is incomplete or empty. Please provide a complete outline.",
                        );
                        continue;
                    }
                    return Ok(resp.text);
                }
                Err(err) => {
                    let err_detail = classify_provider_error(err)?;
                    if attempt + 1 >= MAX_MODEL_CALLS_PER_STAGE {
                        return Err(ContentPlanningError::StructuredOutputInvalid(err_detail));
                    }
                    prompt = PromptBuilder::build_repair_prompt(
                        "",
                        &sanitize_error_message(&err_detail),
                    );
                }
            }
        }
        Err(ContentPlanningError::StructuredOutputInvalid(
            "Draft generation repair budget exhausted".to_string(),
        ))
    }

    fn execute_plan_expansion_stage(
        &self,
        request: &PlanningRequest,
        draft_text: &str,
    ) -> Result<ContentPlan, ContentPlanningError> {
        let mut prompt = PromptBuilder::build_plan_prompt(request, draft_text);
        let mut repair_count: usize = 0;

        for attempt in 0..MAX_MODEL_CALLS_PER_STAGE {
            let resp_res = self.provider.generate_content_plan(&prompt);
            let resp = match resp_res {
                Ok(r) => r,
                Err(err) => {
                    let err_detail = classify_provider_error(err)?;
                    if attempt + 1 >= MAX_MODEL_CALLS_PER_STAGE {
                        return Err(ContentPlanningError::StructuredOutputInvalid(err_detail));
                    }
                    repair_count += 1;
                    prompt = PromptBuilder::build_repair_prompt(
                        "",
                        &sanitize_error_message(&err_detail),
                    );
                    continue;
                }
            };

            let mut plan: ContentPlan = match serde_json::from_str(&resp.text) {
                Ok(p) => p,
                Err(e) => {
                    let err_msg = format!("JSON deserialization failed: {}", e);
                    if attempt + 1 >= MAX_MODEL_CALLS_PER_STAGE {
                        return Err(ContentPlanningError::StructuredOutputInvalid(err_msg));
                    }
                    repair_count += 1;
                    prompt = PromptBuilder::build_repair_prompt(
                        &resp.text,
                        &sanitize_error_message(&err_msg),
                    );
                    continue;
                }
            };

            derive_plan_fields(&mut plan);

            if let Err(e) = validate_content_plan(&plan) {
                if attempt + 1 >= MAX_MODEL_CALLS_PER_STAGE {
                    return Err(e);
                }
                repair_count += 1;
                prompt = PromptBuilder::build_repair_prompt(
                    &resp.text,
                    &sanitize_error_message(&e.to_string()),
                );
                continue;
            }

            if let Err(e) = validate_duration_tolerance(
                request.target_duration_ms,
                plan.estimated_duration_ms,
                DEFAULT_DURATION_TOLERANCE,
            ) {
                if attempt + 1 >= MAX_MODEL_CALLS_PER_STAGE {
                    return Err(e);
                }
                repair_count += 1;
                prompt = PromptBuilder::build_repair_prompt(
                    &resp.text,
                    &sanitize_error_message(&e.to_string()),
                );
                continue;
            }

            if let Err(e) = validate_grounding(&plan, &request.source_context) {
                if attempt + 1 >= MAX_MODEL_CALLS_PER_STAGE {
                    return Err(e);
                }
                repair_count += 1;
                prompt = PromptBuilder::build_repair_prompt(
                    &resp.text,
                    &sanitize_error_message(&e.to_string()),
                );
                continue;
            }

            plan.project_id = request.project_id.clone();
            plan.language = request.language.clone();
            plan.tone = request.tone.clone();
            plan.target_duration_ms = request.target_duration_ms;
            plan.status = PlanStatus::Draft;
            plan.revision = 1;

            if let Some(ref mut meta) = plan.generation_metadata {
                meta.repaired_calls = if repair_count > 0 {
                    Some(repair_count as u32)
                } else {
                    None
                };
            } else {
                plan.generation_metadata = Some(GenerationMetadata {
                    provider: resp.provider.clone(),
                    model: resp.model.clone(),
                    planner_profile_version: PromptBuilder::PLANNER_PROFILE_VERSION,
                    prompt_template_version: PromptBuilder::PROMPT_TEMPLATE_VERSION,
                    generated_at: chrono::Utc::now().to_rfc3339(),
                    usage: resp.usage.unwrap_or_default(),
                    repaired_calls: if repair_count > 0 {
                        Some(repair_count as u32)
                    } else {
                        None
                    },
                });
            }

            return Ok(plan);
        }

        Err(ContentPlanningError::StructuredOutputInvalid(
            "Plan expansion repair budget exhausted".to_string(),
        ))
    }

    /// Stage 1 (Outline/Draft) -> Stage 2 (Expansion/Grounding) inside one job.
    pub fn generate_plan(
        &self,
        request: &PlanningRequest,
    ) -> Result<ContentPlan, ContentPlanningError> {
        validate_planning_request(request)?;
        let draft_text = self.execute_draft_stage(request)?;
        self.execute_plan_expansion_stage(request, &draft_text)
    }

    /// Full plan regeneration given operator instructions and context sources.
    pub fn regenerate_plan(
        &self,
        base: &ContentPlan,
        instructions: &str,
        sources: &[PlanningSource],
    ) -> Result<ContentPlan, ContentPlanningError> {
        let mut prompt = PromptBuilder::build_regenerate_plan_prompt(base, instructions, sources);
        let mut repair_count: usize = 0;

        for attempt in 0..MAX_MODEL_CALLS_PER_STAGE {
            let resp = match self.provider.regenerate_plan(&prompt) {
                Ok(r) => r,
                Err(err) => {
                    let err_detail = classify_provider_error(err)?;
                    if attempt + 1 >= MAX_MODEL_CALLS_PER_STAGE {
                        return Err(ContentPlanningError::StructuredOutputInvalid(err_detail));
                    }
                    repair_count += 1;
                    prompt = PromptBuilder::build_repair_prompt(
                        "",
                        &sanitize_error_message(&err_detail),
                    );
                    continue;
                }
            };

            let mut plan: ContentPlan = match serde_json::from_str(&resp.text) {
                Ok(p) => p,
                Err(e) => {
                    let err_msg = format!("JSON deserialization failed: {}", e);
                    if attempt + 1 >= MAX_MODEL_CALLS_PER_STAGE {
                        return Err(ContentPlanningError::StructuredOutputInvalid(err_msg));
                    }
                    repair_count += 1;
                    prompt = PromptBuilder::build_repair_prompt(
                        &resp.text,
                        &sanitize_error_message(&err_msg),
                    );
                    continue;
                }
            };

            derive_plan_fields(&mut plan);

            if let Err(e) = validate_content_plan(&plan) {
                if attempt + 1 >= MAX_MODEL_CALLS_PER_STAGE {
                    return Err(e);
                }
                repair_count += 1;
                prompt = PromptBuilder::build_repair_prompt(
                    &resp.text,
                    &sanitize_error_message(&e.to_string()),
                );
                continue;
            }

            if let Err(e) = validate_duration_tolerance(
                base.target_duration_ms,
                plan.estimated_duration_ms,
                DEFAULT_DURATION_TOLERANCE,
            ) {
                if attempt + 1 >= MAX_MODEL_CALLS_PER_STAGE {
                    return Err(e);
                }
                repair_count += 1;
                prompt = PromptBuilder::build_repair_prompt(
                    &resp.text,
                    &sanitize_error_message(&e.to_string()),
                );
                continue;
            }

            if let Err(e) = validate_grounding(&plan, sources) {
                if attempt + 1 >= MAX_MODEL_CALLS_PER_STAGE {
                    return Err(e);
                }
                repair_count += 1;
                prompt = PromptBuilder::build_repair_prompt(
                    &resp.text,
                    &sanitize_error_message(&e.to_string()),
                );
                continue;
            }

            plan.id = base.id.clone();
            plan.project_id = base.project_id.clone();
            plan.revision = base.revision + 1;
            plan.status = PlanStatus::Draft;
            plan.target_duration_ms = base.target_duration_ms;

            if let Some(ref mut meta) = plan.generation_metadata {
                meta.repaired_calls = if repair_count > 0 {
                    Some(repair_count as u32)
                } else {
                    None
                };
            } else {
                plan.generation_metadata = Some(GenerationMetadata {
                    provider: resp.provider.clone(),
                    model: resp.model.clone(),
                    planner_profile_version: PromptBuilder::PLANNER_PROFILE_VERSION,
                    prompt_template_version: PromptBuilder::PROMPT_TEMPLATE_VERSION,
                    generated_at: chrono::Utc::now().to_rfc3339(),
                    usage: resp.usage.unwrap_or_default(),
                    repaired_calls: if repair_count > 0 {
                        Some(repair_count as u32)
                    } else {
                        None
                    },
                });
            }

            return Ok(plan);
        }

        Err(ContentPlanningError::StructuredOutputInvalid(
            "Regenerate plan repair budget exhausted".to_string(),
        ))
    }

    /// Scene regeneration: replaces ONLY the target scene in a cloned base plan.
    /// Non-target scenes are protected against modification.
    pub fn regenerate_scene(
        &self,
        base: &ContentPlan,
        scene_id: &str,
        instructions: &str,
        sources: &[PlanningSource],
    ) -> Result<ContentPlan, ContentPlanningError> {
        let target_idx = base
            .scenes
            .iter()
            .position(|s| s.id == scene_id)
            .ok_or_else(|| {
                ContentPlanningError::ContentPlanValidationFailed(format!(
                    "target scene '{}' not found",
                    scene_id
                ))
            })?;

        let mut prompt =
            PromptBuilder::build_regenerate_scene_prompt(base, scene_id, instructions, sources);
        let mut repair_count: usize = 0;

        for attempt in 0..MAX_MODEL_CALLS_PER_STAGE {
            let resp = match self.provider.regenerate_scene(&prompt) {
                Ok(r) => r,
                Err(err) => {
                    let err_detail = classify_provider_error(err)?;
                    if attempt + 1 >= MAX_MODEL_CALLS_PER_STAGE {
                        return Err(ContentPlanningError::StructuredOutputInvalid(err_detail));
                    }
                    repair_count += 1;
                    prompt = PromptBuilder::build_repair_prompt(
                        "",
                        &sanitize_error_message(&err_detail),
                    );
                    continue;
                }
            };

            let candidate_scene: ContentScene =
                if let Ok(scene) = serde_json::from_str::<ContentScene>(&resp.text) {
                    scene
                } else if let Ok(plan) = serde_json::from_str::<ContentPlan>(&resp.text) {
                    match plan.scenes.into_iter().find(|s| s.id == scene_id) {
                        Some(s) => s,
                        None => {
                            let err_msg =
                                format!("ContentPlan response missing target scene '{}'", scene_id);
                            if attempt + 1 >= MAX_MODEL_CALLS_PER_STAGE {
                                return Err(ContentPlanningError::StructuredOutputInvalid(err_msg));
                            }
                            repair_count += 1;
                            prompt = PromptBuilder::build_repair_prompt(
                                &resp.text,
                                &sanitize_error_message(&err_msg),
                            );
                            continue;
                        }
                    }
                } else {
                    let err_msg =
                        "Failed to parse scene regeneration output as ContentScene or ContentPlan"
                            .to_string();
                    if attempt + 1 >= MAX_MODEL_CALLS_PER_STAGE {
                        return Err(ContentPlanningError::StructuredOutputInvalid(err_msg));
                    }
                    repair_count += 1;
                    prompt = PromptBuilder::build_repair_prompt(
                        &resp.text,
                        &sanitize_error_message(&err_msg),
                    );
                    continue;
                };

            let mut plan = base.clone();
            let mut updated_scene = candidate_scene;
            updated_scene.id = scene_id.to_string();
            updated_scene.order = base.scenes[target_idx].order;
            plan.scenes[target_idx] = updated_scene;

            derive_plan_fields(&mut plan);

            if let Err(e) = validate_content_plan(&plan) {
                if attempt + 1 >= MAX_MODEL_CALLS_PER_STAGE {
                    return Err(e);
                }
                repair_count += 1;
                prompt = PromptBuilder::build_repair_prompt(
                    &resp.text,
                    &sanitize_error_message(&e.to_string()),
                );
                continue;
            }

            if let Err(e) = validate_grounding(&plan, sources) {
                if attempt + 1 >= MAX_MODEL_CALLS_PER_STAGE {
                    return Err(e);
                }
                repair_count += 1;
                prompt = PromptBuilder::build_repair_prompt(
                    &resp.text,
                    &sanitize_error_message(&e.to_string()),
                );
                continue;
            }

            if let Err(e) = validate_duration_tolerance(
                base.target_duration_ms,
                plan.estimated_duration_ms,
                DEFAULT_DURATION_TOLERANCE,
            ) {
                if attempt + 1 >= MAX_MODEL_CALLS_PER_STAGE {
                    return Err(e);
                }
                repair_count += 1;
                prompt = PromptBuilder::build_repair_prompt(
                    &resp.text,
                    &sanitize_error_message(&e.to_string()),
                );
                continue;
            }

            plan.revision = base.revision + 1;
            plan.status = PlanStatus::Draft;

            plan.generation_metadata = Some(GenerationMetadata {
                provider: resp.provider.clone(),
                model: resp.model.clone(),
                planner_profile_version: PromptBuilder::PLANNER_PROFILE_VERSION,
                prompt_template_version: PromptBuilder::PROMPT_TEMPLATE_VERSION,
                generated_at: chrono::Utc::now().to_rfc3339(),
                usage: resp.usage.unwrap_or_default(),
                repaired_calls: if repair_count > 0 {
                    Some(repair_count as u32)
                } else {
                    None
                },
            });

            return Ok(plan);
        }

        Err(ContentPlanningError::StructuredOutputInvalid(
            "Regenerate scene repair budget exhausted".to_string(),
        ))
    }

    /// Narration regeneration: copies ONLY target narration, claims, and sourceRefs
    /// into the target scene of a cloned base plan.
    /// Non-factual planning fields (visual, media, prompts, duration, title, order) are preserved.
    pub fn regenerate_narration(
        &self,
        base: &ContentPlan,
        scene_id: &str,
        instructions: &str,
        sources: &[PlanningSource],
    ) -> Result<ContentPlan, ContentPlanningError> {
        let target_idx = base
            .scenes
            .iter()
            .position(|s| s.id == scene_id)
            .ok_or_else(|| {
                ContentPlanningError::ContentPlanValidationFailed(format!(
                    "target scene '{}' not found",
                    scene_id
                ))
            })?;

        let mut prompt =
            PromptBuilder::build_regenerate_narration_prompt(base, scene_id, instructions, sources);
        let mut repair_count: usize = 0;

        for attempt in 0..MAX_MODEL_CALLS_PER_STAGE {
            let resp = match self.provider.regenerate_narration(&prompt) {
                Ok(r) => r,
                Err(err) => {
                    let err_detail = classify_provider_error(err)?;
                    if attempt + 1 >= MAX_MODEL_CALLS_PER_STAGE {
                        return Err(ContentPlanningError::StructuredOutputInvalid(err_detail));
                    }
                    repair_count += 1;
                    prompt = PromptBuilder::build_repair_prompt(
                        "",
                        &sanitize_error_message(&err_detail),
                    );
                    continue;
                }
            };

            let (new_narration, new_claims, new_source_refs) =
                if let Ok(scene) = serde_json::from_str::<ContentScene>(&resp.text) {
                    (scene.narration, scene.claims, scene.source_refs)
                } else if let Ok(plan) = serde_json::from_str::<ContentPlan>(&resp.text) {
                    match plan.scenes.into_iter().find(|s| s.id == scene_id) {
                        Some(s) => (s.narration, s.claims, s.source_refs),
                        None => {
                            let err_msg =
                                format!("ContentPlan response missing target scene '{}'", scene_id);
                            if attempt + 1 >= MAX_MODEL_CALLS_PER_STAGE {
                                return Err(ContentPlanningError::StructuredOutputInvalid(err_msg));
                            }
                            repair_count += 1;
                            prompt = PromptBuilder::build_repair_prompt(
                                &resp.text,
                                &sanitize_error_message(&err_msg),
                            );
                            continue;
                        }
                    }
                } else if let Ok(val) = serde_json::from_str::<serde_json::Value>(&resp.text) {
                    let narration = val
                        .get("narration")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string());
                    let claims: Option<Vec<Claim>> = val
                        .get("claims")
                        .and_then(|v| serde_json::from_value(v.clone()).ok());
                    let source_refs: Option<Vec<SourceRef>> = val
                        .get("sourceRefs")
                        .and_then(|v| serde_json::from_value(v.clone()).ok());
                    if let (Some(n), Some(c)) = (narration, claims) {
                        (n, c, source_refs.unwrap_or_default())
                    } else {
                        let err_msg =
                            "Missing 'narration' or 'claims' in narration regeneration output"
                                .to_string();
                        if attempt + 1 >= MAX_MODEL_CALLS_PER_STAGE {
                            return Err(ContentPlanningError::StructuredOutputInvalid(err_msg));
                        }
                        repair_count += 1;
                        prompt = PromptBuilder::build_repair_prompt(
                            &resp.text,
                            &sanitize_error_message(&err_msg),
                        );
                        continue;
                    }
                } else {
                    let err_msg = "Failed to parse narration output as JSON".to_string();
                    if attempt + 1 >= MAX_MODEL_CALLS_PER_STAGE {
                        return Err(ContentPlanningError::StructuredOutputInvalid(err_msg));
                    }
                    repair_count += 1;
                    prompt = PromptBuilder::build_repair_prompt(
                        &resp.text,
                        &sanitize_error_message(&err_msg),
                    );
                    continue;
                };

            let mut plan = base.clone();
            plan.scenes[target_idx].narration = new_narration;
            plan.scenes[target_idx].claims = new_claims;
            plan.scenes[target_idx].source_refs = new_source_refs;

            derive_plan_fields(&mut plan);

            if let Err(e) = validate_content_plan(&plan) {
                if attempt + 1 >= MAX_MODEL_CALLS_PER_STAGE {
                    return Err(e);
                }
                repair_count += 1;
                prompt = PromptBuilder::build_repair_prompt(
                    &resp.text,
                    &sanitize_error_message(&e.to_string()),
                );
                continue;
            }

            if let Err(e) = validate_grounding(&plan, sources) {
                if attempt + 1 >= MAX_MODEL_CALLS_PER_STAGE {
                    return Err(e);
                }
                repair_count += 1;
                prompt = PromptBuilder::build_repair_prompt(
                    &resp.text,
                    &sanitize_error_message(&e.to_string()),
                );
                continue;
            }

            plan.revision = base.revision + 1;
            plan.status = PlanStatus::Draft;

            plan.generation_metadata = Some(GenerationMetadata {
                provider: resp.provider.clone(),
                model: resp.model.clone(),
                planner_profile_version: PromptBuilder::PLANNER_PROFILE_VERSION,
                prompt_template_version: PromptBuilder::PROMPT_TEMPLATE_VERSION,
                generated_at: chrono::Utc::now().to_rfc3339(),
                usage: resp.usage.unwrap_or_default(),
                repaired_calls: if repair_count > 0 {
                    Some(repair_count as u32)
                } else {
                    None
                },
            });

            return Ok(plan);
        }

        Err(ContentPlanningError::StructuredOutputInvalid(
            "Regenerate narration repair budget exhausted".to_string(),
        ))
    }
}
