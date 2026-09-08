export type ArtifactKind =
  | 'video'
  | 'audio'
  | 'image'
  | 'subtitle'
  | 'json'
  | 'other';

export interface ArtifactRef {
  artifactId: string;
  kind: ArtifactKind;
  uri: string;
  sha256: string;
  sizeBytes: number;
  mimeType: string | null;
}

export interface MediaMetadata {
  artifactId: string;
  probeStatus: 'ok' | 'unavailable' | 'failed';
  durationMs: number | null;
  width: number | null;
  height: number | null;
  videoCodec: string | null;
  audioCodec: string | null;
  frameRate: number | null;
  sampleRate: number | null;
  channels: number | null;
  probeError: string | null;
}

export interface ImportedArtifact {
  artifact: ArtifactRef;
  originalFilename: string;
  addedAt: string;
  metadata: MediaMetadata | null;
  isDeduplicated: boolean;
}
