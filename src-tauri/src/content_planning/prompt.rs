use crate::content_planning::model::{ContentPlan, PlanningRequest, PlanningSource};
use crate::content_planning::provider::ProviderRequest;

pub const PROMPT_TEMPLATE_VERSION: i32 = 1;
pub const PLANNER_PROFILE_VERSION: i32 = 1;

pub struct PromptBuilder;

impl PromptBuilder {
    pub const PROMPT_TEMPLATE_VERSION: i32 = PROMPT_TEMPLATE_VERSION;
    pub const PLANNER_PROFILE_VERSION: i32 = PLANNER_PROFILE_VERSION;

    pub fn build_draft_prompt(request: &PlanningRequest) -> ProviderRequest {
        build_draft_prompt(request)
    }

    pub fn build_plan_prompt(request: &PlanningRequest, draft_text: &str) -> ProviderRequest {
        build_plan_prompt(request, draft_text)
    }

    pub fn build_regenerate_plan_prompt(
        base: &ContentPlan,
        instructions: &str,
        sources: &[PlanningSource],
    ) -> ProviderRequest {
        build_regenerate_plan_prompt(base, instructions, sources)
    }

    pub fn build_regenerate_scene_prompt(
        base: &ContentPlan,
        scene_id: &str,
        instructions: &str,
        sources: &[PlanningSource],
    ) -> ProviderRequest {
        build_regenerate_scene_prompt(base, scene_id, instructions, sources)
    }

    pub fn build_regenerate_narration_prompt(
        base: &ContentPlan,
        scene_id: &str,
        instructions: &str,
        sources: &[PlanningSource],
    ) -> ProviderRequest {
        build_regenerate_narration_prompt(base, scene_id, instructions, sources)
    }

    pub fn build_repair_prompt(malformed_json: &str, error_details: &str) -> ProviderRequest {
        build_repair_prompt(malformed_json, error_details)
    }
}

fn format_allowed_sources(sources: &[PlanningSource]) -> String {
    let source_ids: Vec<String> = sources
        .iter()
        .map(|s| format!("\"{}\"", s.source_id))
        .collect();
    format!("[{}]", source_ids.join(", "))
}

fn build_system_prompt_rules(sources: &[PlanningSource]) -> String {
    let allowed_sources = format_allowed_sources(sources);
    format!(
        r#"You are an expert video content planning assistant.

CITATION & GROUNDING RULES:
ALLOWED_SOURCES: {allowed_sources}
- Cite only IDs in ALLOWED_SOURCES.
- Never invent source IDs, titles, authors, URLs, or locators.
- Use model_knowledge or uncertain when supplied sources do not support a factual claim.
- If a claim is supported by a supplied source, set groundingStatus to "supported", include the matching sourceId in sourceRefs, and set groundingValid to true.
- If a claim draws upon general AI knowledge, set groundingStatus to "model_knowledge" and leave sourceRefs empty."#
    )
}

pub fn build_draft_prompt(request: &PlanningRequest) -> ProviderRequest {
    let system_prompt = format!(
        "{}\n\nTASK: Generate a high-level video planning draft (outline, scene structure, duration allocation, narrative arc) based on the user's input.",
        build_system_prompt_rules(&request.source_context)
    );

    let sources_summary = if request.source_context.is_empty() {
        "No sources supplied.".to_string()
    } else {
        request
            .source_context
            .iter()
            .map(|s| {
                format!(
                    "- [{}] \"{}\" (type: {}, locator: {:?}):\n  Snapshot: {}",
                    s.source_id, s.title, s.source_type, s.locator, s.content_snapshot
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    };

    let user_prompt = format!(
        "PLANNING REQUEST:\n\
         - Input Type: {:?}\n\
         - Input Text: {}\n\
         - Target Duration (ms): {}\n\
         - Language: {}\n\
         - Tone: {}\n\n\
         SUPPLIED SOURCES:\n\
         {}\n\n\
         Please provide a structured draft outline for this video.",
        request.input_type,
        request.input_text,
        request.target_duration_ms,
        request.language,
        request.tone,
        sources_summary
    );

    ProviderRequest {
        system_prompt,
        user_prompt,
        schema_name: None,
    }
}

pub fn build_plan_prompt(request: &PlanningRequest, draft_text: &str) -> ProviderRequest {
    let schema_description = r#"OUTPUT FORMAT:
You must output a single, valid JSON object strictly adhering to the ContentPlan schema:
{
  "id": "plan-<unique_id>",
  "projectId": "<project_id>",
  "revision": 1,
  "status": "draft",
  "title": "<title>",
  "summary": "<summary>",
  "language": "<language>",
  "tone": "<tone>",
  "targetDurationMs": <target_duration_ms>,
  "estimatedDurationMs": <sum_of_scene_durations>,
  "fullNarration": "<all_scene_narrations_joined_by_double_newlines>",
  "scenes": [
    {
      "id": "scene-1",
      "order": 1,
      "title": "<scene_title>",
      "narration": "<spoken_narration>",
      "durationEstimateMs": <scene_duration_ms>,
      "visualDescription": "<visual_description>",
      "mediaIntent": ["stock_video" | "stock_image" | "ai_image" | "ai_video" | "screen_recording" | "text_graphic"],
      "stockSearchQueries": ["<search_query>"],
      "imagePrompt": "<optional_prompt_or_null>",
      "videoPrompt": "<optional_prompt_or_null>",
      "claims": [
        {
          "id": "claim-1-1",
          "text": "<factual_claim_text>",
          "groundingStatus": "supported" | "model_knowledge" | "uncertain",
          "sourceRefs": [
            {
              "sourceId": "<source_id_from_allowed_sources>",
              "locator": "<optional_locator_or_null>",
              "excerptHash": null,
              "note": null
            }
          ],
          "groundingValid": true,
          "note": null
        }
      ],
      "sourceRefs": []
    }
  ],
  "sourceRefs": [],
  "generationMetadata": null,
  "createdAt": "<iso_timestamp>",
  "updatedAt": "<iso_timestamp>"
}"#;

    let system_prompt = format!(
        "{}\n\n{}\n\nOutput only the raw JSON object without markdown formatting or code fences.",
        build_system_prompt_rules(&request.source_context),
        schema_description
    );

    let user_prompt = format!(
        "PLANNING REQUEST:\n\
         - Project ID: {}\n\
         - Input Type: {:?}\n\
         - Input Text: {}\n\
         - Target Duration (ms): {}\n\
         - Language: {}\n\
         - Tone: {}\n\n\
         DRAFT OUTLINE:\n\
         {}\n\n\
         Generate the complete, detailed ContentPlan JSON.",
        request.project_id,
        request.input_type,
        request.input_text,
        request.target_duration_ms,
        request.language,
        request.tone,
        draft_text
    );

    ProviderRequest {
        system_prompt,
        user_prompt,
        schema_name: Some("content_plan".to_string()),
    }
}

pub fn build_regenerate_plan_prompt(
    base: &ContentPlan,
    instructions: &str,
    sources: &[PlanningSource],
) -> ProviderRequest {
    let schema_description = r#"OUTPUT FORMAT:
Output a single, valid JSON object strictly adhering to the ContentPlan schema."#;

    let system_prompt = format!(
        "{}\n\n{}\n\nTASK: Regenerate the entire ContentPlan following the user's instructions while maintaining grounding and schema rules.",
        build_system_prompt_rules(sources),
        schema_description
    );

    let user_prompt = format!(
        "BASE PLAN:\n\
         - Title: {}\n\
         - Target Duration (ms): {}\n\
         - Current Narration:\n{}\n\n\
         USER REGENERATION INSTRUCTIONS:\n\
         {}\n\n\
         Generate the regenerated ContentPlan JSON.",
        base.title, base.target_duration_ms, base.full_narration, instructions
    );

    ProviderRequest {
        system_prompt,
        user_prompt,
        schema_name: Some("content_plan".to_string()),
    }
}

pub fn build_regenerate_scene_prompt(
    base: &ContentPlan,
    scene_id: &str,
    instructions: &str,
    sources: &[PlanningSource],
) -> ProviderRequest {
    let target_scene = base.scenes.iter().find(|s| s.id == scene_id);
    let scene_context = match target_scene {
        Some(s) => format!(
            "Target Scene ID: {}\nOrder: {}\nCurrent Title: {}\nCurrent Narration: {}\nDuration: {} ms\nVisuals: {}",
            s.id, s.order, s.title, s.narration, s.duration_estimate_ms, s.visual_description
        ),
        None => format!("Target Scene ID: {scene_id} (not found in base plan)"),
    };

    let system_prompt = format!(
        "{}\n\nTASK: Regenerate scene '{}' within the content plan. Non-target scenes must remain consistent.",
        build_system_prompt_rules(sources),
        scene_id
    );

    let user_prompt = format!(
        "PLAN CONTEXT:\n\
         - Title: {}\n\
         - Target Duration: {} ms\n\n\
         TARGET SCENE TO REGENERATE:\n\
         {}\n\n\
         USER INSTRUCTIONS:\n\
         {}\n\n\
         Regenerate this scene conforming to the content plan scene schema.",
        base.title, base.target_duration_ms, scene_context, instructions
    );

    ProviderRequest {
        system_prompt,
        user_prompt,
        schema_name: Some("content_scene".to_string()),
    }
}

pub fn build_regenerate_narration_prompt(
    base: &ContentPlan,
    scene_id: &str,
    instructions: &str,
    sources: &[PlanningSource],
) -> ProviderRequest {
    let target_scene = base.scenes.iter().find(|s| s.id == scene_id);
    let scene_context = match target_scene {
        Some(s) => format!(
            "Target Scene ID: {}\nOrder: {}\nCurrent Title: {}\nCurrent Narration: {}\nDuration: {} ms",
            s.id, s.order, s.title, s.narration, s.duration_estimate_ms
        ),
        None => format!("Target Scene ID: {scene_id}"),
    };

    let system_prompt = format!(
        "{}\n\nTASK: Regenerate ONLY the narration and claims for scene '{}'. Do NOT change scene title, duration, visual description, or media intent.",
        build_system_prompt_rules(sources),
        scene_id
    );

    let user_prompt = format!(
        "PLAN CONTEXT:\n\
         - Title: {}\n\n\
         TARGET SCENE:\n\
         {}\n\n\
         USER INSTRUCTIONS:\n\
         {}\n\n\
         Provide the rewritten narration and updated claims.",
        base.title, scene_context, instructions
    );

    ProviderRequest {
        system_prompt,
        user_prompt,
        schema_name: Some("scene_narration".to_string()),
    }
}

pub fn build_repair_prompt(malformed_json: &str, error_details: &str) -> ProviderRequest {
    let system_prompt = "You are a JSON repair specialist. Output only valid JSON conforming strictly to the requested schema. Do not include markdown code fences or explanatory text.".to_string();

    let user_prompt = format!(
        "The previous output was malformed or failed validation.\n\n\
         Validation errors:\n\
         {}\n\n\
         Malformed output:\n\
         {}\n\n\
         Fix all issues and output valid JSON only.",
        error_details, malformed_json
    );

    ProviderRequest {
        system_prompt,
        user_prompt,
        schema_name: Some("content_plan".to_string()),
    }
}
