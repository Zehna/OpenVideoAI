import React, { useState, useEffect, useCallback } from 'react';
import type { Job, JobState } from '../types';
import { listJobs, submitJob, cancelJob } from '../api';
import { Button } from '../../../shared/components/Button';
import { JobDetailModal } from './JobDetailModal';

interface JobListProps {
  projectId?: string | null;
}

type FilterStatus = 'all' | JobState;

export const JobList: React.FC<JobListProps> = ({ projectId }) => {
  const [jobs, setJobs] = useState<Job[]>([]);
  const [isLoading, setIsLoading] = useState<boolean>(true);
  const [error, setError] = useState<string | null>(null);
  const [filter, setFilter] = useState<FilterStatus>('all');
  const [selectedJob, setSelectedJob] = useState<Job | null>(null);
  const [isSubmittingTest, setIsSubmittingTest] = useState<boolean>(false);

  const loadJobs = useCallback(async () => {
    setIsLoading(true);
    setError(null);
    try {
      const statusFilter = filter === 'all' ? null : filter;
      const data = await listJobs(projectId ?? null, statusFilter);
      setJobs(data);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setIsLoading(false);
    }
  }, [projectId, filter]);

  useEffect(() => {
    void loadJobs();
  }, [loadJobs]);

  const handleCancel = async (jobId: string, e: React.MouseEvent) => {
    e.stopPropagation();
    try {
      const updated = await cancelJob(jobId);
      setJobs((prev) => prev.map((j) => (j.id === updated.id ? updated : j)));
      if (selectedJob && selectedJob.id === updated.id) {
        setSelectedJob(updated);
      }
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    }
  };

  const handleSubmitTestJob = async () => {
    setIsSubmittingTest(true);
    setError(null);
    try {
      const newJob = await submitJob({
        jobType: 'test_pipeline',
        payload: JSON.stringify({
          task: 'video_probe_simulation',
          timestamp: new Date().toISOString(),
        }),
        priority: 50,
        projectId: projectId ?? null,
        maxRetries: 3,
      });
      setJobs((prev) => [newJob, ...prev]);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setIsSubmittingTest(false);
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

  const isCancellable = (status: JobState) => {
    return (
      status === 'queued' ||
      status === 'running' ||
      status === 'waiting_for_capacity' ||
      status === 'retryable'
    );
  };

  const filterTabs: { label: string; value: FilterStatus }[] = [
    { label: 'All', value: 'all' },
    { label: 'Queued', value: 'queued' },
    { label: 'Running', value: 'running' },
    { label: 'Completed', value: 'completed' },
    { label: 'Failed', value: 'failed' },
    { label: 'Dead Letter', value: 'dead_letter' },
    { label: 'Cancelled', value: 'cancelled' },
  ];

  return (
    <div className="max-w-5xl mx-auto p-6 space-y-6">
      {/* Header */}
      <div className="flex flex-col sm:flex-row sm:items-center sm:justify-between gap-4 border-b border-gray-800 pb-4">
        <div>
          <h1 className="text-2xl font-bold text-gray-100 tracking-tight">
            Jobs & Queue Inspector
          </h1>
          <p className="text-sm text-gray-400 mt-1">
            Durable SQLite-backed queue with worker leasing, checkpoints & crash recovery
          </p>
        </div>
        <div className="flex items-center gap-2">
          <Button
            variant="secondary"
            onClick={loadJobs}
            isLoading={isLoading}
          >
            Refresh
          </Button>
          <Button
            onClick={handleSubmitTestJob}
            isLoading={isSubmittingTest}
          >
            + Submit Test Job
          </Button>
        </div>
      </div>

      {error && (
        <div className="p-3 rounded-lg bg-rose-950/60 border border-rose-800/70 text-rose-300 text-xs">
          {error}
        </div>
      )}

      {/* Filter Tabs */}
      <div className="flex flex-wrap items-center gap-1.5 border-b border-gray-800/80 pb-3">
        {filterTabs.map((tab) => (
          <button
            key={tab.value}
            onClick={() => setFilter(tab.value)}
            className={`px-3 py-1.5 rounded-lg text-xs font-medium cursor-pointer transition-colors ${
              filter === tab.value
                ? 'bg-indigo-600 text-white font-semibold'
                : 'bg-gray-900 text-gray-400 hover:text-gray-200 hover:bg-gray-800'
            }`}
          >
            {tab.label}
          </button>
        ))}
      </div>

      {/* Main List */}
      {isLoading ? (
        <div className="flex flex-col items-center justify-center min-h-[300px] text-gray-400">
          <div className="w-8 h-8 border-2 border-indigo-500 border-t-transparent rounded-full animate-spin mb-4" />
          <p className="text-sm">Loading jobs queue...</p>
        </div>
      ) : jobs.length === 0 ? (
        <div className="flex flex-col items-center justify-center min-h-[300px] border border-dashed border-gray-800 rounded-2xl p-8 text-center">
          <div className="w-12 h-12 rounded-full bg-indigo-950/60 border border-indigo-800/80 flex items-center justify-center text-indigo-400 font-bold mb-4">
            ?
          </div>
          <h3 className="text-base font-semibold text-gray-200">No jobs in queue</h3>
          <p className="text-xs text-gray-400 max-w-sm mt-1 mb-4">
            {filter === 'all'
              ? 'No background tasks have been queued yet. Submit a test job to test the durable queue.'
              : `No jobs currently in "${filter}" status.`}
          </p>
          <Button onClick={handleSubmitTestJob} isLoading={isSubmittingTest}>
            Submit Test Job
          </Button>
        </div>
      ) : (
        <div className="space-y-3">
          {jobs.map((job) => (
            <div
              key={job.id}
              onClick={() => setSelectedJob(job)}
              className="p-4 rounded-xl bg-gray-900 border border-gray-800 hover:border-indigo-600/70 hover:bg-gray-850 transition-all cursor-pointer shadow-sm flex flex-col sm:flex-row sm:items-center justify-between gap-4 group"
            >
              <div className="space-y-1.5 flex-1 min-w-0">
                <div className="flex items-center gap-2 flex-wrap">
                  <span
                    className={`px-2.5 py-0.5 rounded-full text-xs font-mono uppercase tracking-wider border ${getStatusBadge(
                      job.status
                    )}`}
                  >
                    {job.status.replace(/_/g, ' ')}
                  </span>
                  <span className="font-semibold text-gray-100 truncate text-sm group-hover:text-indigo-300 transition-colors">
                    {job.jobType}
                  </span>
                  <span className="text-[10px] font-mono px-1.5 py-0.5 rounded bg-gray-800 text-gray-400 border border-gray-700">
                    P{job.priority}
                  </span>
                  {job.retryCount > 0 && (
                    <span className="text-[10px] font-mono px-1.5 py-0.5 rounded bg-orange-950/80 text-orange-400 border border-orange-800/60">
                      Retries: {job.retryCount}/{job.maxRetries}
                    </span>
                  )}
                </div>

                <div className="flex items-center gap-4 text-xs text-gray-500 font-mono flex-wrap">
                  <span>ID: {job.id.slice(0, 8)}...</span>
                  <span>Created: {new Date(job.createdAt).toLocaleTimeString()}</span>
                  {job.leasedBy && (
                    <span className="text-amber-400/80 truncate max-w-[200px]">
                      Worker: {job.leasedBy}
                    </span>
                  )}
                  {job.error && (
                    <span className="text-rose-400 truncate max-w-[240px]">
                      Error: {job.error}
                    </span>
                  )}
                </div>
              </div>

              <div className="flex items-center gap-2 shrink-0">
                {isCancellable(job.status) && (
                  <Button
                    variant="danger"
                    className="text-xs px-2.5 py-1"
                    onClick={(e) => void handleCancel(job.id, e)}
                  >
                    Cancel
                  </Button>
                )}
                <Button
                  variant="secondary"
                  className="text-xs px-2.5 py-1"
                  onClick={() => setSelectedJob(job)}
                >
                  Inspect
                </Button>
              </div>
            </div>
          ))}
        </div>
      )}

      {/* Detail Modal */}
      <JobDetailModal
        job={selectedJob}
        isOpen={Boolean(selectedJob)}
        onClose={() => setSelectedJob(null)}
        onJobCancelled={(updated) => {
          setJobs((prev) => prev.map((j) => (j.id === updated.id ? updated : j)));
          setSelectedJob(updated);
        }}
      />
    </div>
  );
};
