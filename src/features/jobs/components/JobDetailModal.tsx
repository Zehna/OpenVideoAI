import React, { useEffect, useState } from 'react';
import type { Job, JobCheckpoint } from '../types';
import { cancelJob, getJobCheckpoints } from '../api';
import { Button } from '../../../shared/components/Button';

interface JobDetailModalProps {
  job: Job | null;
  isOpen: boolean;
  onClose: () => void;
  onJobCancelled?: (job: Job) => void;
}

export const JobDetailModal: React.FC<JobDetailModalProps> = ({
  job,
  isOpen,
  onClose,
  onJobCancelled,
}) => {
  const [checkpoints, setCheckpoints] = useState<JobCheckpoint[]>([]);
  const [isLoadingCheckpoints, setIsLoadingCheckpoints] = useState<boolean>(false);
  const [isCancelling, setIsCancelling] = useState<boolean>(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!isOpen || !job) {
      setCheckpoints([]);
      setError(null);
      return;
    }

    let isMounted = true;
    setIsLoadingCheckpoints(true);
    getJobCheckpoints(job.id)
      .then((data) => {
        if (isMounted) setCheckpoints(data);
      })
      .catch((err) => {
        if (isMounted) {
          setError(err instanceof Error ? err.message : String(err));
        }
      })
      .finally(() => {
        if (isMounted) setIsLoadingCheckpoints(false);
      });

    return () => {
      isMounted = false;
    };
  }, [isOpen, job]);

  if (!isOpen || !job) return null;

  const handleCancel = async () => {
    setIsCancelling(true);
    setError(null);
    try {
      const updated = await cancelJob(job.id);
      if (onJobCancelled) {
        onJobCancelled(updated);
      }
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setIsCancelling(false);
    }
  };

  const isCancellable =
    job.status === 'queued' ||
    job.status === 'running' ||
    job.status === 'waiting_for_capacity' ||
    job.status === 'retryable';

  const formatJson = (text: string | null) => {
    if (!text) return 'None';
    try {
      const parsed = JSON.parse(text);
      return JSON.stringify(parsed, null, 2);
    } catch {
      return text;
    }
  };

  const getStatusBadge = (status: string) => {
    switch (status) {
      case 'queued':
        return 'bg-sky-950/80 text-sky-400 border-sky-800/80';
      case 'running':
        return 'bg-amber-950/80 text-amber-400 border-amber-800/80 animate-pulse';
      case 'waiting_for_capacity':
        return 'bg-purple-950/80 text-purple-400 border-purple-800/80';
      case 'retryable':
        return 'bg-orange-950/80 text-orange-400 border-orange-800/80';
      case 'completed':
        return 'bg-emerald-950/80 text-emerald-400 border-emerald-800/80';
      case 'failed':
        return 'bg-rose-950/80 text-rose-400 border-rose-800/80';
      case 'cancelled':
        return 'bg-gray-800 text-gray-400 border-gray-700';
      case 'dead_letter':
        return 'bg-red-950 text-red-300 border-red-700 font-semibold';
      default:
        return 'bg-gray-800 text-gray-300 border-gray-700';
    }
  };

  return (
    <div className="fixed inset-0 z-50 bg-black/80 flex items-center justify-center p-4 backdrop-blur-sm overflow-y-auto">
      <div className="bg-gray-900 border border-gray-800 rounded-2xl max-w-3xl w-full p-6 shadow-2xl space-y-6 my-8">
        {/* Header */}
        <div className="flex items-start justify-between border-b border-gray-800 pb-4">
          <div>
            <div className="flex items-center gap-3">
              <h2 className="text-xl font-bold text-gray-100">{job.jobType}</h2>
              <span
                className={`px-2.5 py-0.5 rounded-full text-xs font-mono uppercase tracking-wider border ${getStatusBadge(
                  job.status
                )}`}
              >
                {job.status.replace(/_/g, ' ')}
              </span>
            </div>
            <p className="text-xs font-mono text-gray-500 mt-1">ID: {job.id}</p>
          </div>
          <button
            onClick={onClose}
            className="text-gray-400 hover:text-gray-200 text-lg font-bold p-1 cursor-pointer"
            aria-label="Close"
          >
            &times;
          </button>
        </div>

        {error && (
          <div className="p-3 rounded-lg bg-rose-950/60 border border-rose-800/70 text-rose-300 text-xs">
            {error}
          </div>
        )}

        {/* Metadata Grid */}
        <div className="grid grid-cols-2 md:grid-cols-4 gap-4 bg-gray-950/60 p-4 rounded-xl border border-gray-800/80 text-xs">
          <div>
            <span className="text-gray-500 block uppercase text-[10px] font-semibold">Priority</span>
            <span className="text-gray-200 font-mono font-medium">{job.priority}</span>
          </div>
          <div>
            <span className="text-gray-500 block uppercase text-[10px] font-semibold">Retries</span>
            <span className="text-gray-200 font-mono font-medium">
              {job.retryCount} / {job.maxRetries}
            </span>
          </div>
          <div>
            <span className="text-gray-500 block uppercase text-[10px] font-semibold">Project</span>
            <span className="text-gray-200 font-mono truncate block" title={job.projectId ?? 'None'}>
              {job.projectId ?? 'None'}
            </span>
          </div>
          <div>
            <span className="text-gray-500 block uppercase text-[10px] font-semibold">Idempotency Key</span>
            <span className="text-gray-200 font-mono truncate block" title={job.idempotencyKey ?? 'None'}>
              {job.idempotencyKey ?? 'None'}
            </span>
          </div>
          <div>
            <span className="text-gray-500 block uppercase text-[10px] font-semibold">Worker Lease</span>
            <span className="text-gray-200 font-mono truncate block" title={job.leasedBy ?? 'None'}>
              {job.leasedBy ?? 'Unassigned'}
            </span>
          </div>
          <div>
            <span className="text-gray-500 block uppercase text-[10px] font-semibold">Created</span>
            <span className="text-gray-300">
              {new Date(job.createdAt).toLocaleTimeString()}
            </span>
          </div>
          <div>
            <span className="text-gray-500 block uppercase text-[10px] font-semibold">Started</span>
            <span className="text-gray-300">
              {job.startedAt ? new Date(job.startedAt).toLocaleTimeString() : 'Not started'}
            </span>
          </div>
          <div>
            <span className="text-gray-500 block uppercase text-[10px] font-semibold">Completed</span>
            <span className="text-gray-300">
              {job.completedAt ? new Date(job.completedAt).toLocaleTimeString() : 'In progress'}
            </span>
          </div>
        </div>

        {/* Checkpoints Timeline */}
        <div className="space-y-2">
          <div className="flex items-center justify-between">
            <h3 className="text-sm font-semibold text-gray-200">Execution Checkpoints</h3>
            {isLoadingCheckpoints && (
              <span className="text-xs text-gray-500">Loading checkpoints...</span>
            )}
          </div>
          {checkpoints.length === 0 ? (
            <div className="bg-gray-950/40 border border-dashed border-gray-800 rounded-xl p-4 text-center text-xs text-gray-500">
              No checkpoints recorded yet
            </div>
          ) : (
            <div className="space-y-2 max-h-48 overflow-y-auto pr-1">
              {checkpoints.map((cp, idx) => (
                <div
                  key={cp.id}
                  className="bg-gray-950/70 border border-gray-800 rounded-lg p-3 text-xs space-y-1"
                >
                  <div className="flex items-center justify-between">
                    <span className="font-semibold text-indigo-400 font-mono">
                      #{idx + 1} {cp.stage}
                    </span>
                    <span className="text-gray-500 text-[11px] font-mono">
                      {new Date(cp.createdAt).toLocaleTimeString()}
                    </span>
                  </div>
                  <pre className="text-[11px] font-mono text-gray-300 bg-gray-900 p-2 rounded border border-gray-800/80 overflow-x-auto">
                    {formatJson(cp.checkpointData)}
                  </pre>
                </div>
              ))}
            </div>
          )}
        </div>

        {/* Payload */}
        <div className="space-y-1">
          <h3 className="text-sm font-semibold text-gray-200">Payload</h3>
          <pre className="bg-gray-950 p-3 rounded-lg border border-gray-800 text-xs font-mono text-gray-300 max-h-40 overflow-y-auto">
            {formatJson(job.payload)}
          </pre>
        </div>

        {/* Result if present */}
        {job.result && (
          <div className="space-y-1">
            <h3 className="text-sm font-semibold text-emerald-400">Result</h3>
            <pre className="bg-gray-950 p-3 rounded-lg border border-emerald-950 text-xs font-mono text-emerald-200 max-h-40 overflow-y-auto">
              {formatJson(job.result)}
            </pre>
          </div>
        )}

        {/* Error if present */}
        {job.error && (
          <div className="space-y-1">
            <h3 className="text-sm font-semibold text-rose-400">Execution Error</h3>
            <div className="bg-rose-950/40 p-3 rounded-lg border border-rose-900/60 text-xs font-mono text-rose-300 max-h-40 overflow-y-auto">
              {job.error}
            </div>
          </div>
        )}

        {/* Footer Actions */}
        <div className="flex items-center justify-between pt-4 border-t border-gray-800">
          <div>
            {isCancellable && (
              <Button
                variant="danger"
                onClick={handleCancel}
                isLoading={isCancelling}
              >
                Cancel Job
              </Button>
            )}
          </div>
          <Button variant="secondary" onClick={onClose}>
            Close
          </Button>
        </div>
      </div>
    </div>
  );
};
