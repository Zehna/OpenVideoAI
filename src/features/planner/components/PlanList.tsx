import React from 'react';
import type { ContentPlanSummary } from '../types';
import { Button } from '../../../shared/components/Button';

interface PlanListProps {
  plans: ContentPlanSummary[];
  isLoading: boolean;
  onSelectPlan: (planId: string) => void;
  onCreateNew: () => void;
  showArchived: boolean;
  onToggleShowArchived: (show: boolean) => void;
}

export const PlanList: React.FC<PlanListProps> = ({
  plans,
  isLoading,
  onSelectPlan,
  onCreateNew,
  showArchived,
  onToggleShowArchived,
}) => {
  const filteredPlans = plans.filter((p) => (showArchived ? true : p.status !== 'archived'));

  const getStatusBadgeClass = (status: string) => {
    switch (status) {
      case 'approved':
        return 'bg-emerald-950/80 text-emerald-400 border-emerald-800/60';
      case 'archived':
        return 'bg-gray-800 text-gray-400 border-gray-700';
      default:
        return 'bg-amber-950/80 text-amber-400 border-amber-800/60';
    }
  };

  return (
    <div className="space-y-4">
      <div className="flex items-center justify-between">
        <div className="flex items-center gap-4">
          <h2 className="text-lg font-bold text-gray-100 tracking-tight">
            Content Plans ({filteredPlans.length})
          </h2>
          <label className="flex items-center gap-2 text-xs text-gray-400 cursor-pointer select-none">
            <input
              type="checkbox"
              checked={showArchived}
              onChange={(e) => onToggleShowArchived(e.target.checked)}
              className="rounded bg-gray-800 border-gray-700 text-indigo-600 focus:ring-indigo-500"
            />
            Show archived
          </label>
        </div>
        <Button onClick={onCreateNew}>+ New Plan</Button>
      </div>

      {isLoading ? (
        <div className="flex flex-col items-center justify-center p-12 text-gray-400">
          <div className="w-6 h-6 border-2 border-indigo-500 border-t-transparent rounded-full animate-spin mb-3" />
          <p className="text-xs">Loading content plans...</p>
        </div>
      ) : filteredPlans.length === 0 ? (
        <div className="p-8 text-center rounded-xl bg-gray-900 border border-gray-800 space-y-3">
          <p className="text-sm text-gray-400">No content plans found for this project.</p>
          <Button onClick={onCreateNew} variant="secondary" className="text-xs">
            Create First AI Plan
          </Button>
        </div>
      ) : (
        <div className="divide-y divide-gray-800 rounded-xl bg-gray-900 border border-gray-800 overflow-hidden">
          {filteredPlans.map((plan) => (
            <div
              key={plan.id}
              className="p-4 flex items-center justify-between hover:bg-gray-850/50 transition-colors"
            >
              <div className="space-y-1">
                <div className="flex items-center gap-2.5">
                  <span className="font-medium text-gray-100 text-sm">{plan.title}</span>
                  <span
                    className={`text-[10px] uppercase font-mono px-2 py-0.5 rounded border ${getStatusBadgeClass(
                      plan.status
                    )}`}
                  >
                    {plan.status}
                  </span>
                  <span className="text-xs font-mono text-gray-500">Rev {plan.currentRevision}</span>
                </div>
                <div className="text-xs text-gray-500 font-mono">
                  Updated: {new Date(plan.updatedAt).toLocaleString()}
                </div>
              </div>
              <div className="flex items-center gap-2">
                <Button
                  variant="secondary"
                  className="text-xs px-3 py-1.5"
                  onClick={() => onSelectPlan(plan.id)}
                >
                  Open
                </Button>
              </div>
            </div>
          ))}
        </div>
      )}
    </div>
  );
};
