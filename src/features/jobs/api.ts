import { invoke } from "@tauri-apps/api/core";
import type { DomainEvent, Job, JobCheckpoint, JobSubmission } from "./types";

export async function submitJob(submission: JobSubmission): Promise<Job> {
  return await invoke<Job>("submit_job", {
    jobType: submission.jobType,
    payload: submission.payload,
    priority: submission.priority ?? null,
    idempotencyKey: submission.idempotencyKey ?? null,
    projectId: submission.projectId ?? null,
    maxRetries: submission.maxRetries ?? null,
  });
}

export async function getJob(jobId: string): Promise<Job | null> {
  return await invoke<Job | null>("get_job", { jobId });
}

export async function cancelJob(jobId: string): Promise<Job> {
  return await invoke<Job>("cancel_job", { jobId });
}

export async function listJobs(
  projectId?: string | null,
  status?: string | null,
  limit?: number
): Promise<Job[]> {
  return await invoke<Job[]>("list_jobs", {
    projectId: projectId ?? null,
    status: status ?? null,
    limit: limit ?? null,
  });
}

export async function getJobCheckpoints(jobId: string): Promise<JobCheckpoint[]> {
  return await invoke<JobCheckpoint[]>("get_job_checkpoints", { jobId });
}

export async function listEvents(
  eventType?: string | null,
  jobId?: string | null,
  limit?: number
): Promise<DomainEvent[]> {
  return await invoke<DomainEvent[]>("list_events", {
    eventType: eventType ?? null,
    jobId: jobId ?? null,
    limit: limit ?? null,
  });
}
