import React from 'react';
import type { TimelineClip, TimelineTrack } from '../../types/timeline';
import { TimelineClipView } from './TimelineClipView';

interface TimelineTrackViewProps {
  track: TimelineTrack;
  pxPerSecond?: number;
  totalDurationMs?: number;
  selectedClipId?: string | null;
  onSelectClip?: (clipId: string) => void;
  onDeleteClip?: (clipId: string) => void;
  onToggleClipMute?: (clip: TimelineClip) => void;
  onSplitClip?: (clip: TimelineClip) => void;
  onDeleteTrack?: (trackId: string) => void;
  onTrackLaneClick?: (timelineMs: number) => void;
}

export const TimelineTrackView: React.FC<TimelineTrackViewProps> = ({
  track,
  pxPerSecond = 50,
  totalDurationMs = 10000,
  selectedClipId,
  onSelectClip,
  onDeleteClip,
  onToggleClipMute,
  onSplitClip,
  onDeleteTrack,
  onTrackLaneClick,
}) => {
  const isVideo = track.kind === 'video';
  const trackLaneWidth = Math.max((totalDurationMs / 1000) * pxPerSecond + 200, 800);

  const handleLaneClick = (e: React.MouseEvent<HTMLDivElement>) => {
    const rect = e.currentTarget.getBoundingClientRect();
    const clickX = e.clientX - rect.left;
    const timelineMs = Math.max(0, Math.round((clickX / pxPerSecond) * 1000));
    onTrackLaneClick?.(timelineMs);
  };

  return (
    <div className="flex items-stretch border-b border-gray-800 bg-gray-900/40 hover:bg-gray-900/60 transition-colors">
      {/* Track Control Header */}
      <div className="w-48 shrink-0 p-2.5 border-r border-gray-800 bg-gray-900/90 flex items-center justify-between z-10">
        <div className="flex items-center gap-2 overflow-hidden">
          <span
            className={`w-5 h-5 rounded flex items-center justify-center text-[10px] font-bold font-mono text-white ${
              isVideo ? 'bg-indigo-600' : 'bg-emerald-600'
            }`}
          >
            {isVideo ? 'V' : 'A'}
          </span>
          <span className="text-xs font-semibold text-gray-200 truncate">{track.name}</span>
        </div>

        <div className="flex items-center gap-1.5">
          {onDeleteTrack && (
            <button
              onClick={() => onDeleteTrack(track.id)}
              className="text-gray-500 hover:text-red-400 p-1 text-xs"
              title="Delete Track"
            >
              ×
            </button>
          )}
        </div>
      </div>

      {/* Track Lane */}
      <div
        onClick={handleLaneClick}
        style={{ width: `${trackLaneWidth}px` }}
        className="relative h-16 bg-gray-950/60 border-l border-gray-800 cursor-pointer overflow-hidden"
      >
        {/* Subtle grid ticks */}
        <div className="absolute inset-0 bg-[linear-gradient(to_right,#374151_1px,transparent_1px)] bg-[size:50px_100%] opacity-20 pointer-events-none" />

        {/* Clips */}
        {track.clips.map((clip) => (
          <TimelineClipView
            key={clip.id}
            clip={clip}
            trackKind={track.kind}
            pxPerSecond={pxPerSecond}
            isSelected={selectedClipId === clip.id}
            onSelect={() => onSelectClip?.(clip.id)}
            onDelete={() => onDeleteClip?.(clip.id)}
            onToggleMute={() => onToggleClipMute?.(clip)}
            onSplit={() => onSplitClip?.(clip)}
          />
        ))}
      </div>
    </div>
  );
};
