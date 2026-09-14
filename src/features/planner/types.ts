import type { Job } from '../jobs/types';

export type PlanStatus = 'draft' | 'approved' | 'archived';

export type GroundingStatus = 'supported' | 'model_knowledge' | 'uncertain' | 'modified';

export type MediaIntent =
  | 'stock_video'
  | 'generated_video'
  | 'stock_image'
  | 'generated_image'
  | 'motion_graphics'
  | 'text_overlay';

export type InputType = 'topic' | 'script';

export type PlanningOperation =
  | 'generate'
  | 'regenerate_plan'
  | 'regenerate_scene'
  | 'regenerate_narration';

export interface PlanningSource {
  sourceId: string;
  sourceType: string;
  title: string;
  author?: string | null;
  url?: string | null;
  publishedAt?: string | null;
  locator?: string | null;
  contentSnapshot: string;
  contentHash: string;
}

export interface SourceRef {
  sourceId: string;
  locator?: string | null;
  excerptHash?: string | null;
  note?: string | null;
}

export interface Claim {
  id: string;
  text: string;
  sourceRefs: SourceRef[];
  groundingStatus: GroundingStatus;
  groundingValid: boolean;
  note?: string | null;
}

export interface ContentScene {
  id: string;
  order: number;
  title: string;
  narration: string;
  durationEstimateMs: number;
  visualDescription: string;
  mediaIntent: MediaIntent[];
  stockSearchQueries: string[];
  imagePrompt?: string | null;
  videoPrompt?: string | null;
  claims: Claim[];
  sourceRefs: SourceRef[];
}

export interface GenerationMetadata {
  provider: string;
  model: string;
  plannerProfileVersion: number;
  promptTemplateVersion: number;
  inputHash: string;
  contextHash: string;
  generatedAt: string;
  usage?: {
    promptTokens?: number;
    responseTokens?: number;
    totalTokens?: number;
  };
}

export interface ContentPlan {
  id: string;
  projectId: string;
  revision: number;
  status: PlanStatus;
  title: string;
  summary: string;
  language: string;
  tone: string;
  targetDurationMs: number;
  estimatedDurationMs: number;
  fullNarration: string;
  scenes: ContentScene[];
  sourceRefs: SourceRef[];
  generationMetadata?: GenerationMetadata | null;
  createdAt: string;
  updatedAt: string;
}

export interface ContentPlanSummary {
  id: string;
  projectId: string;
  title: string;
  currentRevision: number;
  status: PlanStatus;
  approvedRevision?: number | null;
  approvedAt?: string | null;
  createdAt: string;
  updatedAt: string;
}

export interface ContentPlanRevisionSummary {
  planId: string;
  revision: number;
  parentRevision?: number | null;
  operationType: string;
  createdBy: string;
  createdAt: string;
}

export interface PlannerProviderStatus {
  provider: string;
  configured: boolean;
  model: string;
  message?: string | null;
}

export interface PlanningRequest {
  projectId: string;
  inputType: InputType;
  inputText: string;
  targetDurationMs: number;
  language: string;
  tone: string;
  sourceContext: PlanningSource[];
  generationOptions: Record<string, never>;
}

export interface RegenerationJobParams {
  projectId: string;
  planId: string;
  baseRevision: number;
  operation: PlanningOperation;
  sceneId?: string | null;
  userInstruction?: string | null;
  planningRequest: PlanningRequest;
  sourceContext: PlanningSource[];
  provider?: string | null;
  providerConfigRef?: string | null;
}

export type { Job };
