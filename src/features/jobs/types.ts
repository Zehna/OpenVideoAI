export type JobState =
  | "queued"
  | "running"
  | "waiting_for_capacity"
  | "retryable"
  | "completed"
  | "failed"
  | "cancelled"
  | "dead_letter";

export interface Job {
  id: string;
  jobType: string;
  status: JobState;
  priority: number;
  idempotencyKey: string | null;
  projectId: string | null;
  payload: string;
  payloadVersion: number;
  result: string | null;
  error: string | null;
  retryCount: number;
  maxRetries: number;
  leasedBy: string | null;
  leaseToken: string | null;
  leaseExpiresAt: string | null;
  createdAt: string;
  updatedAt: string;
  startedAt: string | null;
  completedAt: string | null;
}

export interface JobCheckpoint {
  id: string;
  jobId: string;
  stage: string;
  checkpointData: string;
  createdAt: string;
}

export interface DomainEvent<T = unknown> {
  eventId: string;
  eventType: string;
  version: number;
  timestamp: string;
  projectId?: string | null;
  jobId?: string | null;
  payload: T;
}

export interface JobSubmission {
  jobType: string;
  payload: string;
  payloadVersion?: number;
  priority?: number;
  idempotencyKey?: string | null;
  projectId?: string | null;
  maxRetries?: number;
}
