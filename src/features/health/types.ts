export interface DatabaseStatus {
  ok: boolean;
  path: string;
}

export interface ToolStatus {
  available: boolean;
  version: string | null;
  path: string | null;
  error: string | null;
}

export interface HealthSnapshot {
  appVersion: string;
  database: DatabaseStatus;
  ffmpeg: ToolStatus;
  ffprobe: ToolStatus;
}
