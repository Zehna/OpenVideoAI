import React, { useState, useEffect, useCallback } from 'react';
import type { ContentPlanSummary, PlannerProviderStatus, Job } from '../types';
import { listProjectContentPlans, getPlannerProviderStatus } from '../api';
import { PlanList } from './PlanList';
import { NewPlanForm } from './NewPlanForm';
import { Button } from '../../../shared/components/Button';

interface PlannerViewProps {
  projectId: string;
  onBack: () => void;
  onOpenJobs?: (jobId?: string) => void;
  onSelectPlan?: (planId: string) => void;
}

export const PlannerView: React.FC<PlannerViewProps> = ({
  projectId,
  onBack,
  onOpenJobs,
  onSelectPlan,
}) => {
  const [plans, setPlans] = useState<ContentPlanSummary[]>([]);
  const [isLoadingPlans, setIsLoadingPlans] = useState(true);
  const [providerStatus, setProviderStatus] = useState<PlannerProviderStatus | null>(null);
  const [isCreating, setIsCreating] = useState(false);
  const [selectedPlanId, setSelectedPlanId] = useState<string | null>(null);
  const [activeJobId, setActiveJobId] = useState<string | null>(null);
  const [showArchived, setShowArchived] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const loadPlans = useCallback(async () => {
    setIsLoadingPlans(true);
    setError(null);
    try {
      const list = await listProjectContentPlans(projectId, true);
      setPlans(list);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setIsLoadingPlans(false);
    }
  }, [projectId]);

  const loadStatus = useCallback(async () => {
    try {
      const status = await getPlannerProviderStatus();
      setProviderStatus(status);
    } catch (err) {
      console.error('Failed to load planner provider status:', err);
    }
  }, []);

  useEffect(() => {
    void loadPlans();
    void loadStatus();
  }, [loadPlans, loadStatus]);

  const handleJobCreated = (job: Job) => {
    setActiveJobId(job.id);
    setIsCreating(false);
    void loadPlans();
  };

  const handleSelectPlan = (planId: string) => {
    setSelectedPlanId(planId);
    if (onSelectPlan) {
      onSelectPlan(planId);
    }
  };

  return (
    <div className="max-w-5xl mx-auto p-6 space-y-6">
      {/* Planner Header */}
      <div className="flex items-center justify-between border-b border-gray-800 pb-4">
        <div className="flex items-center gap-3">
          <Button
            variant="secondary"
            onClick={selectedPlanId ? () => setSelectedPlanId(null) : isCreating ? () => setIsCreating(false) : onBack}
            className="text-xs px-3 py-1.5"
          >
            {selectedPlanId || isCreating ? '← Back to Plan List' : '← Back to Project'}
          </Button>
          <div>
            <h1 className="text-xl font-bold tracking-tight text-white flex items-center gap-2">
              AI Content Planner
            </h1>
            <span className="text-xs font-mono text-gray-500">Project: {projectId}</span>
          </div>
        </div>

        {/* Provider Status Indicator */}
        <div className="flex items-center gap-2 text-xs font-mono">
          {providerStatus ? (
            providerStatus.configured ? (
              <span className="text-emerald-400 flex items-center gap-1.5 bg-emerald-950/40 px-2.5 py-1 rounded border border-emerald-800/40">
                <span className="w-2 h-2 rounded-full bg-emerald-400" />
                Gemini Active ({providerStatus.model})
              </span>
            ) : (
              <span className="text-amber-400 flex items-center gap-1.5 bg-amber-950/40 px-2.5 py-1 rounded border border-amber-800/40">
                <span className="w-2 h-2 rounded-full bg-amber-400 animate-pulse" />
                Gemini Not Configured
              </span>
            )
          ) : (
            <span className="text-gray-500">Checking provider...</span>
          )}
        </div>
      </div>

      {/* General Error */}
      {error && (
        <div className="p-3 rounded-lg bg-rose-950/50 border border-rose-800/60 text-rose-300 text-xs">
          {error}
        </div>
      )}

      {/* Active Job Alert */}
      {activeJobId && (
        <div className="p-4 rounded-xl bg-indigo-950/40 border border-indigo-800/60 flex items-center justify-between">
          <div className="flex items-center gap-3">
            <span className="w-2.5 h-2.5 rounded-full bg-indigo-400 animate-pulse" />
            <div>
              <p className="text-xs font-semibold text-indigo-200">
                Generation job submitted successfully!
              </p>
              <p className="text-[11px] font-mono text-indigo-400">
                Job ID: <span>{activeJobId}</span>
              </p>
            </div>
          </div>
          <div className="flex items-center gap-2">
            {onOpenJobs && (
              <Button
                variant="secondary"
                className="text-xs px-3 py-1 bg-indigo-900/60 hover:bg-indigo-800 text-indigo-200 border-indigo-700"
                onClick={() => onOpenJobs(activeJobId)}
              >
                View in Queue
              </Button>
            )}
            <button
              onClick={() => setActiveJobId(null)}
              className="text-xs text-gray-500 hover:text-gray-300 px-2 py-1 cursor-pointer"
            >
              Dismiss
            </button>
          </div>
        </div>
      )}

      {/* Unconfigured Provider Banner */}
      {providerStatus && !providerStatus.configured && !isCreating && (
        <div className="p-3 rounded-lg bg-amber-950/60 border border-amber-800/70 text-amber-200 text-xs">
          <span className="font-semibold uppercase tracking-wider block mb-0.5">Notice: Gemini Not Configured</span>
          {providerStatus.message || 'GEMINI_API_KEY environment variable is not set. Generation will fail unless configured.'}
        </div>
      )}

      {/* View Switcher */}
      {isCreating ? (
        <NewPlanForm
          projectId={projectId}
          providerStatus={providerStatus}
          onJobCreated={handleJobCreated}
          onCancel={() => setIsCreating(false)}
        />
      ) : selectedPlanId ? (
        <div className="p-8 text-center rounded-xl bg-gray-900 border border-gray-800 space-y-4">
          <h2 className="text-lg font-bold text-gray-200">Plan Loaded: {selectedPlanId}</h2>
          <p className="text-xs text-gray-400">Structured editor will be mounted here.</p>
          <Button variant="secondary" onClick={() => setSelectedPlanId(null)}>
            Back to Plan List
          </Button>
        </div>
      ) : (
        <PlanList
          plans={plans}
          isLoading={isLoadingPlans}
          onSelectPlan={handleSelectPlan}
          onCreateNew={() => setIsCreating(true)}
          showArchived={showArchived}
          onToggleShowArchived={setShowArchived}
        />
      )}
    </div>
  );
};
