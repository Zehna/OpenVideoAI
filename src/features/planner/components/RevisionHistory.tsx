import React, { useEffect, useState } from 'react';
import type { ContentPlanRevisionSummary } from '../types';
import { getContentPlanHistory } from '../api';
import { Button } from '../../../shared/components/Button';

interface RevisionHistoryProps {
  planId: string;
  currentRevision: number;
  approvedRevision?: number | null;
  selectedRevision: number | null;
  onSelectRevision: (revision: number) => void;
  onClose: () => void;
}

export const RevisionHistory: React.FC<RevisionHistoryProps> = ({
  planId,
  currentRevision,
  approvedRevision,
  selectedRevision,
  onSelectRevision,
  onClose,
}) => {
  const [history, setHistory] = useState<ContentPlanRevisionSummary[]>([]);
  const [isLoading, setIsLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let active = true;
    async function load() {
      setIsLoading(true);
      setError(null);
      try {
        const list = await getContentPlanHistory(planId);
        if (active) {
          // Sort descending so newest revisions appear first
          setHistory([...list].sort((a, b) => b.revision - a.revision));
        }
      } catch (err) {
        if (active) {
          setError(err instanceof Error ? err.message : String(err));
        }
      } finally {
        if (active) setIsLoading(false);
      }
    }
    void load();
    return () => {
      active = false;
    };
  }, [planId]);

  return (
    <div className="rounded-xl bg-gray-900 border border-gray-800 p-4 space-y-3">
      <div className="flex items-center justify-between border-b border-gray-800 pb-2">
        <h3 className="text-sm font-bold text-gray-200">Revision History</h3>
        <button
          onClick={onClose}
          className="text-xs text-gray-400 hover:text-gray-200 cursor-pointer"
        >
          Close
        </button>
      </div>

      {isLoading ? (
        <div className="p-4 text-center text-xs text-gray-500">Loading history...</div>
      ) : error ? (
        <div className="p-2 text-xs text-rose-300 bg-rose-950/50 rounded border border-rose-800/60">
          {error}
        </div>
      ) : history.length === 0 ? (
        <p className="text-xs text-gray-500">No revisions found.</p>
      ) : (
        <div className="divide-y divide-gray-800">
          {history.map((rev) => {
            const isSelected = selectedRevision === rev.revision;
            const isCurrent = currentRevision === rev.revision;
            const isApproved = approvedRevision === rev.revision;

            return (
              <div
                key={rev.revision}
                onClick={() => onSelectRevision(rev.revision)}
                className={`py-2.5 px-2 rounded cursor-pointer transition-colors flex items-center justify-between ${
                  isSelected
                    ? 'bg-indigo-950/60 border border-indigo-700/60'
                    : 'hover:bg-gray-850/60'
                }`}
              >
                <div className="space-y-0.5">
                  <div className="flex items-center gap-2">
                    <span className="text-xs font-semibold text-gray-100">
                      Revision {rev.revision}
                    </span>
                    <span className="text-[10px] font-mono px-1.5 py-0.2 rounded bg-gray-800 text-gray-300 border border-gray-700">
                      {rev.operationType}
                    </span>
                    {isCurrent && (
                      <span className="text-[10px] font-mono px-1.5 py-0.2 rounded bg-indigo-950 text-indigo-300 border border-indigo-800">
                        Current
                      </span>
                    )}
                    {isApproved && (
                      <span className="text-[10px] font-mono px-1.5 py-0.2 rounded bg-emerald-950 text-emerald-300 border border-emerald-800">
                        Approved
                      </span>
                    )}
                  </div>
                  <div className="text-[10px] text-gray-500 font-mono">
                    By: {rev.createdBy} • {new Date(rev.createdAt).toLocaleString()}
                  </div>
                </div>
                <Button
                  variant="secondary"
                  className="text-[11px] px-2 py-0.5"
                  onClick={(e) => {
                    e.stopPropagation();
                    onSelectRevision(rev.revision);
                  }}
                >
                  View
                </Button>
              </div>
            );
          })}
        </div>
      )}
    </div>
  );
};
