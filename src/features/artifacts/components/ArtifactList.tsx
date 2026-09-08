import React from 'react';
import type { ImportedArtifact } from '../types';

interface ArtifactListProps {
  artifacts: ImportedArtifact[];
  isLoading: boolean;
  onImportClick: () => void;
}

export function formatBytes(bytes: number): string {
  if (bytes === 0) return '0 B';
  const k = 1024;
  const sizes = ['B', 'KB', 'MB', 'GB', 'TB'];
  const i = Math.floor(Math.log(bytes) / Math.log(k));
  return `${parseFloat((bytes / Math.pow(k, i)).toFixed(2))} ${sizes[i]}`;
}

export function formatDuration(durationMs: number | null): string {
  if (durationMs === null || durationMs === undefined) return '--:--';
  const totalSeconds = Math.floor(durationMs / 1000);
  const minutes = Math.floor(totalSeconds / 60);
  const seconds = totalSeconds % 60;
  return `${minutes.toString().padStart(2, '0')}:${seconds.toString().padStart(2, '0')}`;
}

export const ArtifactList: React.FC<ArtifactListProps> = ({
  artifacts,
  isLoading,
  onImportClick,
}) => {
  if (isLoading) {
    return (
      <div className="flex flex-col items-center justify-center p-8 text-gray-400">
        <div className="w-6 h-6 border-2 border-indigo-500 border-t-transparent rounded-full animate-spin mb-3" />
        <p className="text-xs">Loading media assets...</p>
      </div>
    );
  }

  if (artifacts.length === 0) {
    return (
      <div className="border border-dashed border-gray-800 rounded-xl p-8 text-center bg-gray-900/40">
        <p className="text-sm font-semibold text-gray-300">No media assets in this project</p>
        <p className="text-xs text-gray-500 mt-1 mb-4">
          Import local video, audio, or image files to register them in the content-addressed store.
        </p>
        <button
          onClick={onImportClick}
          className="px-3 py-1.5 rounded-lg bg-indigo-600 hover:bg-indigo-500 text-white text-xs font-medium cursor-pointer transition-colors"
        >
          + Import Media
        </button>
      </div>
    );
  }

  return (
    <div className="space-y-3">
      {artifacts.map((item) => {
        const meta = item.metadata;
        const kindColors: Record<string, string> = {
          video: 'bg-indigo-950/80 text-indigo-300 border-indigo-700/60',
          audio: 'bg-emerald-950/80 text-emerald-300 border-emerald-700/60',
          image: 'bg-amber-950/80 text-amber-300 border-amber-700/60',
          subtitle: 'bg-purple-950/80 text-purple-300 border-purple-700/60',
          json: 'bg-cyan-950/80 text-cyan-300 border-cyan-700/60',
          other: 'bg-gray-800 text-gray-300 border-gray-700',
        };

        const kindBadgeClass =
          kindColors[item.artifact.kind] || kindColors.other;

        return (
          <div
            key={item.artifact.artifactId}
            data-testid={`artifact-${item.artifact.artifactId}`}
            className="p-4 rounded-xl bg-gray-900 border border-gray-800 hover:border-gray-700 transition-colors flex flex-col md:flex-row md:items-center justify-between gap-4"
          >
            <div className="space-y-1.5 min-w-0">
              <div className="flex items-center gap-2 flex-wrap">
                <span
                  className={`px-2 py-0.5 rounded text-[11px] font-semibold border uppercase tracking-wider ${kindBadgeClass}`}
                >
                  {item.artifact.kind}
                </span>
                <span className="font-semibold text-sm text-gray-100 truncate">
                  {item.originalFilename}
                </span>
                {item.isDeduplicated && (
                  <span className="px-2 py-0.5 rounded text-[10px] font-medium bg-cyan-950/80 text-cyan-300 border border-cyan-700/60">
                    Deduplicated
                  </span>
                )}
              </div>

              <div className="flex items-center gap-4 text-xs font-mono text-gray-400 flex-wrap">
                <span>{formatBytes(item.artifact.sizeBytes)}</span>
                <span>•</span>
                <span>URI: {item.artifact.uri}</span>
                <span>•</span>
                <span title={`SHA-256: ${item.artifact.sha256}`}>
                  Hash: {item.artifact.sha256.substring(0, 10)}...
                </span>
              </div>
            </div>

            {/* Metadata Badges & Stats */}
            <div className="flex items-center gap-3 text-xs shrink-0 flex-wrap">
              {meta ? (
                <>
                  {meta.probeStatus === 'ok' ? (
                    <div className="flex items-center gap-2 bg-gray-800/80 border border-gray-700/80 px-3 py-1.5 rounded-lg text-gray-300 font-mono text-[11px]">
                      {meta.width && meta.height && (
                        <span className="text-gray-200 font-semibold">
                          {meta.width}x{meta.height}
                        </span>
                      )}
                      {meta.durationMs !== null && (
                        <span>{formatDuration(meta.durationMs)}</span>
                      )}
                      {meta.videoCodec && (
                        <span className="text-indigo-300">{meta.videoCodec}</span>
                      )}
                      {meta.audioCodec && (
                        <span className="text-emerald-300">{meta.audioCodec}</span>
                      )}
                      {meta.frameRate && <span>{meta.frameRate} fps</span>}
                    </div>
                  ) : meta.probeStatus === 'unavailable' ? (
                    <span
                      title={meta.probeError || 'FFprobe was not found on system'}
                      className="px-2.5 py-1 rounded bg-gray-800 text-gray-400 border border-gray-700 text-[11px]"
                    >
                      Probe Unavailable
                    </span>
                  ) : (
                    <span
                      title={meta.probeError || 'Metadata probe failed'}
                      className="px-2.5 py-1 rounded bg-amber-950/60 text-amber-300 border border-amber-800/80 text-[11px]"
                    >
                      Probe Failed
                    </span>
                  )}
                </>
              ) : (
                <span className="text-gray-500 text-xs italic">No metadata</span>
              )}
            </div>
          </div>
        );
      })}
    </div>
  );
};
