import React, { useState, useEffect, useCallback } from 'react';
import type { TimelineDocument, TimelineClip, TrackKind } from '../../types/timeline';
import type { ImportedArtifact } from '../../features/artifacts/types';
import { timelineService } from '../../services/timeline';
import { EditorLayout } from './EditorLayout';
import { TimelineToolbar } from './TimelineToolbar';
import { AssetBin } from './AssetBin';
import { PreviewPlayer } from './PreviewPlayer';
import { TimelineTrackView } from './TimelineTrackView';

interface EditorViewProps {
  projectId: string;
  initialTimelineId?: string;
  onBack?: () => void;
}

export const EditorView: React.FC<EditorViewProps> = ({
  projectId,
  initialTimelineId,
  onBack,
}) => {
  const [timeline, setTimeline] = useState<TimelineDocument | null>(null);
  const [currentTimeMs, setCurrentTimeMs] = useState<number>(0);
  const [selectedClipId, setSelectedClipId] = useState<string | null>(null);
  const [isRendering, setIsRendering] = useState<boolean>(false);
  const [activeJobId, setActiveJobId] = useState<string | null>(null);
  const [errorMessage, setErrorMessage] = useState<string | null>(null);

  const loadTimeline = useCallback(async () => {
    try {
      if (initialTimelineId) {
        const doc = await timelineService.getTimeline(initialTimelineId);
        setTimeline(doc);
        return;
      }

      const list = await timelineService.listProjectTimelines(projectId);
      if (list && list.length > 0) {
        setTimeline(list[0]);
      } else {
        const created = await timelineService.createTimeline(
          projectId,
          'Main Timeline',
          1920,
          1080,
          30.0
        );
        setTimeline(created);
      }
    } catch (err) {
      setErrorMessage(err instanceof Error ? err.message : String(err));
    }
  }, [projectId, initialTimelineId]);

  useEffect(() => {
    void loadTimeline();
  }, [loadTimeline]);

  const handleUndo = async () => {
    if (!timeline) return;
    try {
      const updated = await timelineService.undoTimeline(timeline.id, timeline.revision);
      setTimeline(updated);
      setErrorMessage(null);
    } catch (err) {
      setErrorMessage(err instanceof Error ? err.message : String(err));
    }
  };

  const handleRedo = async () => {
    if (!timeline) return;
    try {
      const updated = await timelineService.redoTimeline(timeline.id, timeline.revision);
      setTimeline(updated);
      setErrorMessage(null);
    } catch (err) {
      setErrorMessage(err instanceof Error ? err.message : String(err));
    }
  };

  const handleAddTrack = async (kind: TrackKind) => {
    if (!timeline) return;
    try {
      const count = timeline.tracks.filter((t) => t.kind === kind).length + 1;
      const name = `${kind === 'video' ? 'V' : 'A'}${count}`;
      const updated = await timelineService.applyMutation(timeline.id, timeline.revision, {
        type: 'add_track',
        kind,
        name,
      });
      setTimeline(updated);
      setErrorMessage(null);
    } catch (err) {
      setErrorMessage(err instanceof Error ? err.message : String(err));
    }
  };

  const handleUpdateCanvas = async (width: number, height: number, frameRate: number) => {
    if (!timeline) return;
    try {
      const updated = await timelineService.applyMutation(timeline.id, timeline.revision, {
        type: 'update_canvas',
        width,
        height,
        frame_rate: frameRate,
      });
      setTimeline(updated);
      setErrorMessage(null);
    } catch (err) {
      setErrorMessage(err instanceof Error ? err.message : String(err));
    }
  };

  const handleRender = async () => {
    if (!timeline) return;
    setIsRendering(true);
    setErrorMessage(null);
    try {
      const jobId = await timelineService.submitRender(timeline.id, timeline.revision);
      setActiveJobId(jobId);
    } catch (err) {
      setErrorMessage(err instanceof Error ? err.message : String(err));
    } finally {
      setIsRendering(false);
    }
  };

  const handleAddClipToTimeline = async (item: ImportedArtifact) => {
    if (!timeline) return;
    try {
      const targetKind: TrackKind = item.artifact.kind === 'audio' ? 'audio' : 'video';
      let targetTrack = timeline.tracks.find((t) => t.kind === targetKind);

      let currentRevision = timeline.revision;
      if (!targetTrack) {
        // Create track first
        const count = timeline.tracks.filter((t) => t.kind === targetKind).length + 1;
        const name = `${targetKind === 'video' ? 'V' : 'A'}${count}`;
        const withTrack = await timelineService.applyMutation(timeline.id, currentRevision, {
          type: 'add_track',
          kind: targetKind,
          name,
        });
        targetTrack = withTrack.tracks[withTrack.tracks.length - 1];
        currentRevision = withTrack.revision;
      }

      // Calculate placement start: after last clip on track
      const lastClipEnd = targetTrack.clips.reduce(
        (max, c) => Math.max(max, c.timelineStartMs + (c.sourceOutMs - c.sourceInMs)),
        0
      );

      const sourceDuration = item.metadata?.durationMs ?? 5000;
      const updated = await timelineService.applyMutation(timeline.id, currentRevision, {
        type: 'add_clip',
        track_id: targetTrack.id,
        artifact_id: item.artifact.artifactId,
        timeline_start_ms: lastClipEnd,
        source_in_ms: 0,
        source_out_ms: sourceDuration,
      });

      setTimeline(updated);
      setErrorMessage(null);
    } catch (err) {
      setErrorMessage(err instanceof Error ? err.message : String(err));
    }
  };

  const handleDeleteClip = async (clipId: string) => {
    if (!timeline) return;
    try {
      const updated = await timelineService.applyMutation(timeline.id, timeline.revision, {
        type: 'remove_clip',
        clip_id: clipId,
      });
      setTimeline(updated);
      if (selectedClipId === clipId) setSelectedClipId(null);
      setErrorMessage(null);
    } catch (err) {
      setErrorMessage(err instanceof Error ? err.message : String(err));
    }
  };

  const handleToggleClipMute = async (clip: TimelineClip) => {
    if (!timeline) return;
    try {
      const updated = await timelineService.applyMutation(timeline.id, timeline.revision, {
        type: 'set_clip_audio',
        clip_id: clip.id,
        volume: clip.volume,
        muted: !clip.muted,
      });
      setTimeline(updated);
      setErrorMessage(null);
    } catch (err) {
      setErrorMessage(err instanceof Error ? err.message : String(err));
    }
  };

  const handleSplitClip = async (clip: TimelineClip) => {
    if (!timeline) return;
    const clipEnd = clip.timelineStartMs + (clip.sourceOutMs - clip.sourceInMs);
    if (currentTimeMs <= clip.timelineStartMs || currentTimeMs >= clipEnd) {
      setErrorMessage('Playhead must be strictly inside the clip to split');
      return;
    }

    try {
      const updated = await timelineService.applyMutation(timeline.id, timeline.revision, {
        type: 'split_clip',
        clip_id: clip.id,
        split_timeline_ms: currentTimeMs,
      });
      setTimeline(updated);
      setErrorMessage(null);
    } catch (err) {
      setErrorMessage(err instanceof Error ? err.message : String(err));
    }
  };

  const handleMoveClip = async (clip: TimelineClip, deltaMs: number) => {
    if (!timeline) return;
    const newStart = Math.max(0, clip.timelineStartMs + deltaMs);
    try {
      const updated = await timelineService.applyMutation(timeline.id, timeline.revision, {
        type: 'move_clip',
        clip_id: clip.id,
        new_start_ms: newStart,
      });
      setTimeline(updated);
      setErrorMessage(null);
    } catch (err) {
      setErrorMessage(err instanceof Error ? err.message : String(err));
    }
  };

  const handleTrimClip = async (clip: TimelineClip, inDeltaMs: number, outDeltaMs: number) => {
    if (!timeline) return;
    const newIn = Math.max(0, clip.sourceInMs + inDeltaMs);
    const newOut = Math.max(newIn + 100, clip.sourceOutMs + outDeltaMs);
    try {
      const updated = await timelineService.applyMutation(timeline.id, timeline.revision, {
        type: 'trim_clip',
        clip_id: clip.id,
        source_in_ms: newIn,
        source_out_ms: newOut,
      });
      setTimeline(updated);
      setErrorMessage(null);
    } catch (err) {
      setErrorMessage(err instanceof Error ? err.message : String(err));
    }
  };

  const handleSetClipVolume = async (clip: TimelineClip, volume: number) => {
    if (!timeline) return;
    try {
      const updated = await timelineService.applyMutation(timeline.id, timeline.revision, {
        type: 'set_clip_audio',
        clip_id: clip.id,
        volume,
        muted: clip.muted,
      });
      setTimeline(updated);
      setErrorMessage(null);
    } catch (err) {
      setErrorMessage(err instanceof Error ? err.message : String(err));
    }
  };

  const handleDeleteTrack = async (trackId: string) => {
    if (!timeline) return;
    try {
      const updated = await timelineService.applyMutation(timeline.id, timeline.revision, {
        type: 'delete_track',
        track_id: trackId,
      });
      setTimeline(updated);
      setErrorMessage(null);
    } catch (err) {
      setErrorMessage(err instanceof Error ? err.message : String(err));
    }
  };

  if (!timeline) {
    return (
      <div className="flex items-center justify-center h-64 text-sm text-gray-500 font-mono">
        Loading Timeline Editor...
      </div>
    );
  }

  // Derive total duration from all clips
  const totalDurationMs = timeline.tracks.reduce((maxTrack, t) => {
    const trackEnd = t.clips.reduce(
      (maxClip, c) => Math.max(maxClip, c.timelineStartMs + (c.sourceOutMs - c.sourceInMs)),
      0
    );
    return Math.max(maxTrack, trackEnd);
  }, 0);

  // Active clip under playhead for preview
  const activeVideoTrack = timeline.tracks.find((t) => t.kind === 'video');
  const activeClip =
    activeVideoTrack?.clips.find(
      (c) =>
        currentTimeMs >= c.timelineStartMs &&
        currentTimeMs < c.timelineStartMs + (c.sourceOutMs - c.sourceInMs)
    ) || null;

  const selectedClip =
    timeline.tracks
      .flatMap((t) => t.clips)
      .find((c) => c.id === selectedClipId) || null;

  return (
    <div className="flex flex-col h-full">
      {errorMessage && (
        <div className="bg-red-950/80 border-b border-red-800 text-red-300 px-4 py-1.5 text-xs flex items-center justify-between">
          <span>Error: {errorMessage}</span>
          <button onClick={() => setErrorMessage(null)} className="text-red-400 hover:text-white">
            ×
          </button>
        </div>
      )}

      <EditorLayout
        toolbar={
          <TimelineToolbar
            timeline={timeline}
            onUndo={handleUndo}
            onRedo={handleRedo}
            onAddTrack={handleAddTrack}
            onUpdateCanvas={handleUpdateCanvas}
            onRender={handleRender}
            isRendering={isRendering}
            activeJobId={activeJobId}
            onBack={onBack}
          />
        }
        assetBin={
          <AssetBin
            projectId={projectId}
            onAddClipToTimeline={handleAddClipToTimeline}
          />
        }
        previewPlayer={
          <PreviewPlayer
            canvas={timeline.canvas}
            activeClip={activeClip}
            currentTimeMs={currentTimeMs}
            totalDurationMs={totalDurationMs}
            onSeek={setCurrentTimeMs}
          />
        }
        timelineTracks={
          <div className="min-w-full flex flex-col">
            {selectedClip && (
              <div className="bg-gray-900/90 border-b border-gray-800 px-4 py-2 flex flex-wrap items-center justify-between gap-3 text-xs font-mono sticky top-0 z-20">
                <div className="flex items-center gap-3">
                  <span className="text-gray-400 font-semibold">
                    Clip: <span className="text-white">{selectedClip.artifactId.slice(0, 8)}</span>
                  </span>
                  <div className="flex items-center gap-1">
                    <span className="text-gray-500">Move:</span>
                    <button
                      onClick={() => handleMoveClip(selectedClip, -500)}
                      className="px-1.5 py-0.5 bg-gray-800 hover:bg-gray-700 rounded text-gray-200 cursor-pointer"
                    >
                      -0.5s
                    </button>
                    <span className="text-indigo-400">
                      {(selectedClip.timelineStartMs / 1000).toFixed(1)}s
                    </span>
                    <button
                      onClick={() => handleMoveClip(selectedClip, 500)}
                      className="px-1.5 py-0.5 bg-gray-800 hover:bg-gray-700 rounded text-gray-200 cursor-pointer"
                    >
                      +0.5s
                    </button>
                  </div>
                  <div className="flex items-center gap-1">
                    <span className="text-gray-500">Trim In:</span>
                    <button
                      onClick={() => handleTrimClip(selectedClip, -500, 0)}
                      className="px-1.5 py-0.5 bg-gray-800 hover:bg-gray-700 rounded text-gray-200 cursor-pointer"
                    >
                      -0.5s
                    </button>
                    <span className="text-emerald-400">
                      {(selectedClip.sourceInMs / 1000).toFixed(1)}s
                    </span>
                    <button
                      onClick={() => handleTrimClip(selectedClip, 500, 0)}
                      className="px-1.5 py-0.5 bg-gray-800 hover:bg-gray-700 rounded text-gray-200 cursor-pointer"
                    >
                      +0.5s
                    </button>
                  </div>
                  <div className="flex items-center gap-1">
                    <span className="text-gray-500">Trim Out:</span>
                    <button
                      onClick={() => handleTrimClip(selectedClip, 0, -500)}
                      className="px-1.5 py-0.5 bg-gray-800 hover:bg-gray-700 rounded text-gray-200 cursor-pointer"
                    >
                      -0.5s
                    </button>
                    <span className="text-emerald-400">
                      {(selectedClip.sourceOutMs / 1000).toFixed(1)}s
                    </span>
                    <button
                      onClick={() => handleTrimClip(selectedClip, 0, 500)}
                      className="px-1.5 py-0.5 bg-gray-800 hover:bg-gray-700 rounded text-gray-200 cursor-pointer"
                    >
                      +0.5s
                    </button>
                  </div>
                </div>

                <div className="flex items-center gap-3">
                  <div className="flex items-center gap-1.5">
                    <span className="text-gray-500">Vol:</span>
                    <input
                      type="range"
                      min={0}
                      max={2}
                      step={0.1}
                      value={selectedClip.volume}
                      onChange={(e) => handleSetClipVolume(selectedClip, Number(e.target.value))}
                      className="w-16 h-1 bg-gray-800 rounded accent-indigo-500 cursor-pointer"
                    />
                    <span className="text-gray-300 w-8">
                      {(selectedClip.volume * 100).toFixed(0)}%
                    </span>
                  </div>

                  <button
                    onClick={() => handleToggleClipMute(selectedClip)}
                    className={`px-2 py-0.5 rounded text-xs transition-colors cursor-pointer ${
                      selectedClip.muted
                        ? 'bg-red-900/80 text-red-200'
                        : 'bg-gray-800 hover:bg-gray-700 text-gray-200'
                    }`}
                  >
                    {selectedClip.muted ? 'Muted (Audio)' : 'Mute Audio'}
                  </button>
                  <button
                    onClick={() => handleSplitClip(selectedClip)}
                    className="px-2 py-0.5 rounded bg-gray-800 hover:bg-gray-700 text-gray-200 cursor-pointer"
                  >
                    Split
                  </button>
                  <button
                    onClick={() => handleDeleteClip(selectedClip.id)}
                    className="px-2 py-0.5 rounded bg-red-950/80 hover:bg-red-900 text-red-300 cursor-pointer"
                  >
                    Delete
                  </button>
                  <button
                    onClick={() => setSelectedClipId(null)}
                    className="text-gray-500 hover:text-gray-300 px-1 cursor-pointer"
                    title="Deselect"
                  >
                    ✕
                  </button>
                </div>
              </div>
            )}
            <div className="min-w-full divide-y divide-gray-800">
              {timeline.tracks.length === 0 ? (
                <div className="p-8 text-center text-xs text-gray-500 font-mono">
                  No tracks yet. Click &quot;+ Video Track&quot; or &quot;+ Audio Track&quot; above to begin.
                </div>
              ) : (
              timeline.tracks.map((track) => (
                <TimelineTrackView
                  key={track.id}
                  track={track}
                  totalDurationMs={totalDurationMs}
                  selectedClipId={selectedClipId}
                  onSelectClip={setSelectedClipId}
                  onDeleteClip={handleDeleteClip}
                  onToggleClipMute={handleToggleClipMute}
                  onSplitClip={handleSplitClip}
                  onDeleteTrack={handleDeleteTrack}
                  onTrackLaneClick={setCurrentTimeMs}
                />
              ))
              )}
            </div>
          </div>
        }
      />
    </div>
  );
};
