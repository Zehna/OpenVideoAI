export type TrackKind = 'video' | 'audio';

export interface ClipTransform {
  x: number;
  y: number;
  scale: number;
  rotationDeg: number;
  opacity: number;
}

export interface TimelineClip {
  id: string;
  trackId: string;
  artifactId: string;
  sourceInMs: number;
  sourceOutMs: number;
  timelineStartMs: number;
  volume: number;
  muted: boolean;
  transform: ClipTransform;
}

export interface TimelineTrack {
  id: string;
  timelineId: string;
  kind: TrackKind;
  order: number;
  name: string;
  muted: boolean;
  clips: TimelineClip[];
}

export interface CanvasConfig {
  width: number;
  height: number;
  frameRate: number;
}

export interface TimelineDocument {
  id: string;
  projectId: string;
  name: string;
  revision: number;
  canvas: CanvasConfig;
  tracks: TimelineTrack[];
  createdAt: string;
  updatedAt: string;
}

export type TimelineMutation =
  | { type: 'add_track'; kind: TrackKind; name: string }
  | { type: 'reorder_track'; track_id: string; new_order: number }
  | { type: 'delete_track'; track_id: string }
  | {
      type: 'add_clip';
      track_id: string;
      artifact_id: string;
      timeline_start_ms: number;
      source_in_ms?: number | null;
      source_out_ms?: number | null;
    }
  | { type: 'remove_clip'; clip_id: string }
  | {
      type: 'move_clip';
      clip_id: string;
      new_track_id?: string | null;
      new_start_ms: number;
    }
  | {
      type: 'trim_clip';
      clip_id: string;
      source_in_ms: number;
      source_out_ms: number;
    }
  | {
      type: 'split_clip';
      clip_id: string;
      split_timeline_ms: number;
    }
  | {
      type: 'set_clip_audio';
      clip_id: string;
      volume: number;
      muted: boolean;
    }
  | {
      type: 'set_clip_transform';
      clip_id: string;
      transform: ClipTransform;
    }
  | {
      type: 'update_canvas';
      width: number;
      height: number;
      frame_rate: number;
    };
