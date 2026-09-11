import React from 'react';
import type { TimelineDocument } from '../../types/timeline';

interface TimelineToolbarProps {
  timeline: TimelineDocument;
  onUndo: () => void;
  onRedo: () => void;
  onAddTrack: (kind: 'video' | 'audio') => void;
  onUpdateCanvas: (width: number, height: number, frameRate: number) => void;
  onRender: () => void;
  isRendering?: boolean;
  activeJobId?: string | null;
  onBack?: () => void;
}

export const TimelineToolbar: React.FC<TimelineToolbarProps> = ({
  timeline,
  onUndo,
  onRedo,
  onAddTrack,
  onUpdateCanvas,
  onRender,
  isRendering = false,
  activeJobId = null,
  onBack,
}) => {
  return (
    <header className="border-b border-gray-800 bg-gray-900/80 backdrop-blur px-4 py-2.5 flex items-center justify-between gap-4 sticky top-0 z-30">
      {/* Left: Navigation & Info */}
      <div className="flex items-center gap-3">
        {onBack && (
          <button
            onClick={onBack}
            className="text-xs px-2.5 py-1 rounded bg-gray-800 hover:bg-gray-700 text-gray-300 transition-colors"
          >
            ← Back
          </button>
        )}
        <div className="flex items-baseline gap-2">
          <h1 className="font-bold text-sm tracking-tight text-white">{timeline.name}</h1>
          <span className="text-[10px] font-mono uppercase bg-indigo-950/80 text-indigo-400 px-1.5 py-0.5 rounded border border-indigo-800/60">
            Rev {timeline.revision}
          </span>
        </div>
      </div>

      {/* Center: Canvas Config & Presets */}
      <div className="flex items-center gap-2">
        <select
          value={`${timeline.canvas.width}x${timeline.canvas.height}`}
          onChange={(e) => {
            const [w, h] = e.target.value.split('x').map(Number);
            onUpdateCanvas(w, h, timeline.canvas.frameRate);
          }}
          className="bg-gray-950 border border-gray-800 text-gray-200 text-xs rounded px-2 py-1 focus:outline-none focus:border-indigo-500 font-mono"
        >
          <option value="1920x1080">16:9 Landscape (1920x1080)</option>
          <option value="1080x1920">9:16 Portrait (1080x1920)</option>
          <option value="1080x1080">1:1 Square (1080x1080)</option>
        </select>
        <span className="text-[11px] font-mono text-gray-400">
          @{timeline.canvas.frameRate.toFixed(0)}fps
        </span>
      </div>

      {/* Right: Actions (Undo, Redo, Add Track, Render) */}
      <div className="flex items-center gap-2">
        <div className="flex items-center rounded border border-gray-800 overflow-hidden">
          <button
            onClick={onUndo}
            title="Undo"
            className="px-2.5 py-1 text-xs bg-gray-800 hover:bg-gray-700 text-gray-200 transition-colors border-r border-gray-700"
          >
            Undo
          </button>
          <button
            onClick={onRedo}
            title="Redo"
            className="px-2.5 py-1 text-xs bg-gray-800 hover:bg-gray-700 text-gray-200 transition-colors"
          >
            Redo
          </button>
        </div>

        <div className="flex items-center gap-1">
          <button
            onClick={() => onAddTrack('video')}
            className="px-2.5 py-1 text-xs rounded bg-gray-800 hover:bg-gray-700 text-indigo-300 border border-indigo-900/50 transition-colors"
          >
            + Video Track
          </button>
          <button
            onClick={() => onAddTrack('audio')}
            className="px-2.5 py-1 text-xs rounded bg-gray-800 hover:bg-gray-700 text-emerald-300 border border-emerald-900/50 transition-colors"
          >
            + Audio Track
          </button>
        </div>

        {activeJobId && (
          <span className="text-[10px] font-mono text-amber-300 bg-amber-950/80 border border-amber-800/60 px-2 py-0.5 rounded animate-pulse">
            Job: {activeJobId.slice(0, 8)}...
          </span>
        )}

        <button
          onClick={onRender}
          disabled={isRendering}
          className={`px-3 py-1 text-xs font-semibold rounded shadow transition-colors ${
            isRendering
              ? 'bg-indigo-900 text-indigo-400 cursor-not-allowed'
              : 'bg-indigo-600 hover:bg-indigo-500 text-white cursor-pointer'
          }`}
        >
          {isRendering ? 'Rendering...' : 'Render'}
        </button>
      </div>
    </header>
  );
};
