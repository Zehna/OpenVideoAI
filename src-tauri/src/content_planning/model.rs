use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InputType {
    Topic,
    Script,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GenerationOptions {}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanningSource {
    pub source_id: String,
    pub source_type: String,
    pub title: String,
    pub author: Option<String>,
    pub url: Option<String>,
    pub published_at: Option<String>,
    pub locator: Option<String>,
    pub content_snapshot: String,
    pub content_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceRef {
    pub source_id: String,
    pub locator: Option<String>,
    pub excerpt_hash: Option<String>,
    pub note: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanningRequest {
    pub project_id: String,
    pub input_type: InputType,
    pub input_text: String,
    pub target_duration_ms: i64,
    pub language: String,
    pub tone: String,
    pub source_context: Vec<PlanningSource>,
    pub generation_options: GenerationOptions,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MediaIntent {
    StockVideo,
    StockImage,
    AiImage,
    AiVideo,
    ScreenRecording,
    TextGraphic,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GroundingStatus {
    Supported,
    ModelKnowledge,
    Uncertain,
    Modified,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Claim {
    pub id: String,
    pub text: String,
    pub grounding_status: GroundingStatus,
    pub source_refs: Vec<SourceRef>,
    pub grounding_valid: bool,
    pub note: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContentScene {
    pub id: String,
    pub order: i32,
    pub title: String,
    pub narration: String,
    pub duration_estimate_ms: i64,
    pub visual_description: String,
    pub media_intent: Vec<MediaIntent>,
    pub stock_search_queries: Vec<String>,
    pub image_prompt: Option<String>,
    pub video_prompt: Option<String>,
    pub claims: Vec<Claim>,
    pub source_refs: Vec<SourceRef>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlanStatus {
    Draft,
    Approved,
    Archived,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GenerationUsage {
    pub input_tokens: Option<u32>,
    pub output_tokens: Option<u32>,
    pub total_tokens: Option<u32>,
    pub provider_reported_cost: Option<f64>,
    pub duration_ms: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GenerationMetadata {
    pub provider: String,
    pub model: String,
    pub planner_profile_version: i32,
    pub prompt_template_version: i32,
    pub generated_at: String,
    pub usage: GenerationUsage,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repaired_calls: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContentPlan {
    pub id: String,
    pub project_id: String,
    pub revision: i64,
    pub status: PlanStatus,
    pub title: String,
    pub summary: String,
    pub language: String,
    pub tone: String,
    pub target_duration_ms: i64,
    pub estimated_duration_ms: i64,
    pub full_narration: String,
    pub scenes: Vec<ContentScene>,
    pub source_refs: Vec<SourceRef>,
    pub generation_metadata: Option<GenerationMetadata>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContentPlanSummary {
    pub id: String,
    pub project_id: String,
    pub title: String,
    pub current_revision: i64,
    pub status: PlanStatus,
    pub approved_revision: Option<i64>,
    pub approved_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContentPlanRevisionSummary {
    pub plan_id: String,
    pub revision: i64,
    pub parent_revision: Option<i64>,
    pub operation_type: String,
    pub created_by: String,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlannerProviderStatus {
    pub provider: String,
    pub configured: bool,
    pub model: String,
}
