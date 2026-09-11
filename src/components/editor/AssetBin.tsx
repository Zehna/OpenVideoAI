import React, { useEffect, useState, useCallback } from 'react';
import { listProjectArtifacts } from '../../features/artifacts/api';
import type { ImportedArtifact } from '../../features/artifacts/types';

interface AssetBinProps {
  projectId: string;
  onAddClipToTimeline: (artifact: ImportedArtifact) => void;
  onSelectAssetForPreview?: (artifact: ImportedArtifact) => void;
}

export const AssetBin: React.FC<AssetBinProps> = ({
  projectId,
  onAddClipToTimeline,
  onSelectAssetForPreview,
}) => {
  const [artifacts, setArtifacts] = useState<ImportedArtifact[]>([]);
  const [isLoading, setIsLoading] = useState<boolean>(true);
  const [error, setError] = useState<string | null>(null);

  const loadAssets = useCallback(async () => {
    setIsLoading(true);
    setError(null);
    try {
      const data = await listProjectArtifacts(projectId);
      setArtifacts(data);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setIsLoading(false);
    }
  }, [projectId]);

  useEffect(() => {
    void loadAssets();
  }, [loadAssets]);

  return (
    <div className="flex flex-col h-full bg-gray-900/50 border-r border-gray-800 w-72 shrink-0">
      <div className="p-3 border-b border-gray-800 flex items-center justify-between">
        <h2 className="text-xs font-semibold uppercase tracking-wider text-gray-400">
          Media Assets
        </h2>
        <button
          onClick={() => void loadAssets()}
          className="text-xs text-gray-500 hover:text-gray-300"
          title="Refresh Assets"
        >
          ↻
        </button>
      </div>

      <div className="flex-1 overflow-y-auto p-2 space-y-1.5">
        {isLoading ? (
          <div className="text-center py-6 text-xs text-gray-500">Loading assets...</div>
        ) : error ? (
          <div className="p-2 text-xs text-red-400 bg-red-950/40 rounded border border-red-900/50">
            {error}
          </div>
        ) : artifacts.length === 0 ? (
          <div className="text-center py-6 text-xs text-gray-500">
            No assets imported yet.
          </div>
        ) : (
          artifacts.map((item) => {
            const isVideo = item.artifact.kind === 'video';
            const dur = item.metadata?.durationMs
              ? `${(item.metadata.durationMs / 1000).toFixed(1)}s`
              : 'Unknown dur';

            return (
              <div
                key={item.artifact.artifactId}
                onClick={() => onSelectAssetForPreview?.(item)}
                className="group p-2 rounded bg-gray-950/70 hover:bg-gray-800/80 border border-gray-800/80 hover:border-gray-700 transition-all flex items-center justify-between gap-2 cursor-pointer"
              >
                <div className="flex items-center gap-2 overflow-hidden">
                  <span
                    className={`w-6 h-6 rounded flex items-center justify-center text-[9px] font-mono font-bold text-white shrink-0 ${
                      isVideo ? 'bg-indigo-600' : 'bg-emerald-600'
                    }`}
                  >
                    {isVideo ? 'MP4' : 'AUD'}
                  </span>
                  <div className="overflow-hidden">
                    <p className="text-xs font-medium text-gray-200 truncate" title={item.originalFilename}>
                      {item.originalFilename}
                    </p>
                    <p className="text-[10px] font-mono text-gray-500">{dur}</p>
                  </div>
                </div>

                <button
                  onClick={(e) => {
                    e.stopPropagation();
                    onAddClipToTimeline(item);
                  }}
                  className="px-2 py-1 text-[11px] rounded bg-indigo-600/80 hover:bg-indigo-500 text-white shrink-0 opacity-80 group-hover:opacity-100 transition-opacity"
                  title="Add to Timeline"
                >
                  + Add
                </button>
              </div>
            );
          })
        )}
      </div>
    </div>
  );
};
