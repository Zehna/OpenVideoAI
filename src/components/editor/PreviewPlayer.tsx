import React, { useRef, useState, useEffect } from 'react';
import type { CanvasConfig, TimelineClip } from '../../types/timeline';

interface PreviewPlayerProps {
  canvas: CanvasConfig;
  activeClip?: TimelineClip | null;
  currentTimeMs: number;
  totalDurationMs: number;
  onSeek: (timeMs: number) => void;
}

export const PreviewPlayer: React.FC<PreviewPlayerProps> = ({
  canvas,
  activeClip,
  currentTimeMs,
  totalDurationMs,
  onSeek,
}) => {
  const videoRef = useRef<HTMLVideoElement>(null);
  const [isPlaying, setIsPlaying] = useState<boolean>(false);

  const formatTime = (ms: number) => {
    const totalSeconds = Math.floor(ms / 1000);
    const minutes = Math.floor(totalSeconds / 60);
    const seconds = totalSeconds % 60;
    const millis = Math.floor((ms % 1000) / 100);
    return `${String(minutes).padStart(2, '0')}:${String(seconds).padStart(2, '0')}.${millis}`;
  };

  const mediaSrc = activeClip
    ? `openvideoai-media://artifact/${activeClip.artifactId}`
    : undefined;

  // Toggle play/pause
  const togglePlay = () => {
    if (!videoRef.current) return;
    if (isPlaying) {
      videoRef.current.pause();
      setIsPlaying(false);
    } else {
      videoRef.current.play().catch(() => {});
      setIsPlaying(true);
    }
  };

  useEffect(() => {
    if (videoRef.current && activeClip) {
      // Seek video relative to clip source_in
      const clipRelSec = (currentTimeMs - activeClip.timelineStartMs + activeClip.sourceInMs) / 1000;
      if (clipRelSec >= 0 && Math.abs(videoRef.current.currentTime - clipRelSec) > 0.3) {
        videoRef.current.currentTime = clipRelSec;
      }
    }
  }, [currentTimeMs, activeClip]);

  const aspectRatio = `${canvas.width} / ${canvas.height}`;

  return (
    <div className="flex-1 flex flex-col items-center justify-between p-4 bg-gray-950 overflow-hidden">
      {/* Viewport Canvas Frame */}
      <div className="flex-1 w-full flex items-center justify-center min-h-0 relative">
        <div
          style={{ aspectRatio }}
          className="relative max-h-full max-w-full bg-black rounded shadow-2xl border border-gray-800 overflow-hidden flex items-center justify-center"
        >
          {mediaSrc ? (
            <video
              ref={videoRef}
              src={mediaSrc}
              className="w-full h-full object-contain"
              onTimeUpdate={() => {
                if (videoRef.current && activeClip && isPlaying) {
                  const mediaTimeMs = Math.round(videoRef.current.currentTime * 1000);
                  const newTlMs = activeClip.timelineStartMs + (mediaTimeMs - activeClip.sourceInMs);
                  onSeek(newTlMs);
                }
              }}
              onEnded={() => setIsPlaying(false)}
            />
          ) : (
            <div className="text-center p-6 select-none">
              <p className="text-sm font-semibold text-gray-400">Canvas Preview</p>
              <p className="text-xs font-mono text-gray-600 mt-1">
                {canvas.width} × {canvas.height} @ {canvas.frameRate.toFixed(0)}fps
              </p>
              <p className="text-[11px] text-gray-600 mt-2">
                Add clips to the timeline to preview playback
              </p>
            </div>
          )}

          {/* Timecode overlay */}
          <div className="absolute top-2 right-2 bg-black/70 backdrop-blur px-2 py-0.5 rounded font-mono text-xs text-white/90 border border-white/10">
            {formatTime(currentTimeMs)} / {formatTime(totalDurationMs)}
          </div>
        </div>
      </div>

      {/* Playback Controls & Scrub Bar */}
      <div className="w-full max-w-2xl mt-3 space-y-2">
        {/* Scrub Bar */}
        <div className="flex items-center gap-3">
          <input
            type="range"
            min={0}
            max={Math.max(totalDurationMs, 1000)}
            value={currentTimeMs}
            onChange={(e) => onSeek(Number(e.target.value))}
            className="w-full h-1.5 bg-gray-800 rounded-lg appearance-none cursor-pointer accent-indigo-500"
          />
        </div>

        {/* Buttons */}
        <div className="flex items-center justify-between">
          <div className="flex items-center gap-2">
            <button
              onClick={togglePlay}
              className="px-3 py-1 rounded text-xs font-medium bg-gray-800 hover:bg-gray-700 text-white transition-colors"
            >
              {isPlaying ? 'Pause' : 'Play'}
            </button>
            <button
              onClick={() => onSeek(0)}
              className="px-2 py-1 rounded text-xs bg-gray-800 hover:bg-gray-700 text-gray-300"
              title="Jump to Start"
            >
              ⏮
            </button>
          </div>

          <span className="font-mono text-xs text-gray-400">
            {formatTime(currentTimeMs)}
          </span>
        </div>
      </div>
    </div>
  );
};
