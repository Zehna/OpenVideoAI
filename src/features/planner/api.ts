import { invoke } from '@tauri-apps/api/core';
import type {
  ContentPlan,
  ContentPlanRevisionSummary,
  ContentPlanSummary,
  PlannerProviderStatus,
  PlanningRequest,
  RegenerationJobParams,
  Job,
} from './types';

export async function submitContentPlanJob(
  projectId: string,
  request: PlanningRequest,
  provider?: string,
  providerConfigRef?: string
): Promise<Job> {
  return await invoke<Job>('submit_content_plan_job', {
    projectId,
    request,
    provider,
    providerConfigRef,
  });
}

export async function getContentPlan(planId: string): Promise<ContentPlan | null> {
  return await invoke<ContentPlan | null>('get_content_plan', { planId });
}

export async function listProjectContentPlans(
  projectId: string,
  includeArchived?: boolean
): Promise<ContentPlanSummary[]> {
  return await invoke<ContentPlanSummary[]>('list_project_content_plans', {
    projectId,
    includeArchived,
  });
}

export async function saveContentPlanRevision(
  planId: string,
  expectedRevision: number,
  contentPlan: ContentPlan
): Promise<ContentPlan> {
  return await invoke<ContentPlan>('save_content_plan_revision', {
    planId,
    expectedRevision,
    contentPlan,
  });
}

export async function submitContentPlanRegeneration(
  params: RegenerationJobParams
): Promise<Job> {
  return await invoke<Job>('submit_content_plan_regeneration', { params });
}

export async function approveContentPlan(
  planId: string,
  expectedRevision: number
): Promise<ContentPlanSummary> {
  return await invoke<ContentPlanSummary>('approve_content_plan', {
    planId,
    expectedRevision,
  });
}

export async function archiveContentPlan(
  planId: string,
  expectedRevision: number
): Promise<ContentPlanSummary> {
  return await invoke<ContentPlanSummary>('archive_content_plan', {
    planId,
    expectedRevision,
  });
}

export async function getContentPlanHistory(
  planId: string
): Promise<ContentPlanRevisionSummary[]> {
  return await invoke<ContentPlanRevisionSummary[]>('get_content_plan_history', {
    planId,
  });
}

export async function getContentPlanRevision(
  planId: string,
  revision: number
): Promise<ContentPlan | null> {
  return await invoke<ContentPlan | null>('get_content_plan_revision', {
    planId,
    revision,
  });
}

export async function getPlannerProviderStatus(): Promise<PlannerProviderStatus> {
  return await invoke<PlannerProviderStatus>('get_planner_provider_status');
}
