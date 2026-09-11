import React from 'react';
import type { TimelineClip, TrackKind } from '../../types/timeline';

interface TimelineClipViewProps {
  clip: TimelineClip;
  trackKind: TrackKind;
  pxPerSecond?: number;
  isSelected?: boolean;
  onSelect?: () => void;
  onDelete?: () => void;
  onToggleMute?: () => void;
  onSplit?: () => void;
}

export const TimelineClipView: React.FC<TimelineClipViewProps> = ({
  clip,
  trackKind,
  pxPerSecond = 50,
  isSelected = false,
  onSelect,
  onDelete,
  onToggleMute,
  onSplit,
}) => {
  const durationMs = clip.sourceOutMs - clip.sourceInMs;
  const leftPx = (clip.timelineStartMs / 1000) * pxPerSecond;
  const widthPx = Math.max((durationMs / 1000) * pxPerSecond, 24);

  const isVideo = trackKind === 'video';
  const bgClass = isVideo
    ? isSelected
      ? 'bg-indigo-600 border-indigo-400'
      : 'bg-indigo-900/80 hover:bg-indigo-800 border-indigo-700/60'
    : isSelected
    ? 'bg-emerald-600 border-emerald-400'
    : 'bg-emerald-900/80 hover:bg-emerald-800 border-emerald-700/60';

  return (
    <div
      onClick={(e) => {
        e.stopPropagation();
        onSelect?.();
      }}
      style={{ left: `${leftPx}px`, width: `${widthPx}px` }}
      className={`absolute top-1 bottom-1 rounded border text-xs px-2 py-1 select-none cursor-pointer flex flex-col justify-between overflow-hidden shadow-sm transition-colors ${bgClass}`}
      title={`Clip: ${clip.artifactId} (${(durationMs / 1000).toFixed(1)}s)`}
    >
      <div className="flex items-center justify-between gap-1">
        <span className="font-mono text-[10px] text-white/90 truncate font-semibold">
          {clip.artifactId.slice(0, 8)}
        </span>
        <div className="flex items-center gap-1">
          {clip.muted && (
            <span className="text-[9px] bg-red-950/80 text-red-300 px-1 rounded border border-red-800/60 font-mono">
              MUTED
            </span>
          )}
          {onDelete && (
            <button
              onClick={(e) => {
                e.stopPropagation();
                onDelete();
              }}
              title="Delete clip"
              className="text-white/60 hover:text-white px-0.5 text-[10px]"
            >
              ×
            </button>
          )}
        </div>
      </div>

      <div className="flex items-center justify-between text-[9px] font-mono text-white/70">
        <span>{(clip.timelineStartMs / 1000).toFixed(1)}s</span>
        <span>{((clip.timelineStartMs + durationMs) / 1000).toFixed(1)}s</span>
      </div>

      {isSelected && (
        <div className="flex items-center gap-1 mt-1 pt-1 border-t border-white/10">
          {onToggleMute && (
            <button
              onClick={(e) => {
                e.stopPropagation();
                onToggleMute();
              }}
              className="text-[9px] bg-black/40 hover:bg-black/60 px-1 py-0.5 rounded text-white"
            >
              {clip.muted ? 'Unmute Audio' : 'Mute Audio'}
            </button>
          )}
          {onSplit && (
            <button
              onClick={(e) => {
                e.stopPropagation();
                onSplit();
              }}
              className="text-[9px] bg-black/40 hover:bg-black/60 px-1 py-0.5 rounded text-white"
            >
              Split
            </button>
          )}
        </div>
      )}
    </div>
  );
};
