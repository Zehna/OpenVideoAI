import React, { useState, useEffect, useCallback } from 'react';
import type { ContentPlan, ContentScene, ContentPlanSummary } from '../types';
import { getContentPlan, saveContentPlanRevision, getContentPlanRevision } from '../api';
import { SceneEditor } from './SceneEditor';
import { RevisionHistory } from './RevisionHistory';
import { Button } from '../../../shared/components/Button';

interface ContentPlanEditorProps {
  planId: string;
  onBack: () => void;
  onPlanApproved?: (plan: ContentPlanSummary) => void;
  onPlanArchived?: (plan: ContentPlanSummary) => void;
}

export const ContentPlanEditor: React.FC<ContentPlanEditorProps> = ({
  planId,
  onBack,
}) => {
  const [currentPlan, setCurrentPlan] = useState<ContentPlan | null>(null);
  const [draftPlan, setDraftPlan] = useState<ContentPlan | null>(null);
  const [isLoading, setIsLoading] = useState(true);
  const [isSaving, setIsSaving] = useState(false);
  const [isDirty, setIsDirty] = useState(false);
  const [selectedSceneIndex, setSelectedSceneIndex] = useState(0);
  const [activeTab, setActiveTab] = useState<'scenes' | 'fullNarration'>('scenes');
  const [isHistoryOpen, setIsHistoryOpen] = useState(false);
  const [viewingRevision, setViewingRevision] = useState<number | null>(null);
  const [error, setError] = useState<string | null>(null);

  const isReadOnly =
    (viewingRevision !== null && currentPlan !== null && viewingRevision !== currentPlan.revision) ||
    currentPlan?.status === 'archived';

  const loadCurrentPlan = useCallback(async () => {
    setIsLoading(true);
    setError(null);
    try {
      const data = await getContentPlan(planId);
      if (data) {
        setCurrentPlan(data);
        setDraftPlan(JSON.parse(JSON.stringify(data)));
        setViewingRevision(null);
        setIsDirty(false);
      } else {
        setError('Content plan not found.');
      }
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setIsLoading(false);
    }
  }, [planId]);

  useEffect(() => {
    void loadCurrentPlan();
  }, [loadCurrentPlan]);

  const handleSceneChange = (updatedScene: ContentScene) => {
    if (isReadOnly || !draftPlan) return;
    const updatedScenes = [...draftPlan.scenes];
    updatedScenes[selectedSceneIndex] = updatedScene;

    const fullNarration = updatedScenes.map((s) => s.narration).join('\n\n');
    const estimatedDurationMs = updatedScenes.reduce((acc, s) => acc + s.durationEstimateMs, 0);

    setDraftPlan({
      ...draftPlan,
      scenes: updatedScenes,
      fullNarration,
      estimatedDurationMs,
    });
    setIsDirty(true);
  };

  const handleSave = async () => {
    if (!draftPlan || !currentPlan || !isDirty || isReadOnly) return;
    setIsSaving(true);
    setError(null);
    try {
      const saved = await saveContentPlanRevision(
        currentPlan.id,
        currentPlan.revision,
        draftPlan
      );
      setCurrentPlan(saved);
      setDraftPlan(JSON.parse(JSON.stringify(saved)));
      setIsDirty(false);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setIsSaving(false);
    }
  };

  const handleSelectRevision = async (rev: number) => {
    if (!currentPlan) return;
    if (rev === currentPlan.revision) {
      setViewingRevision(null);
      setDraftPlan(JSON.parse(JSON.stringify(currentPlan)));
      setIsDirty(false);
      return;
    }

    if (isDirty) {
      const discard = window.confirm('You have unsaved changes in the current draft. Discard and view revision?');
      if (!discard) return;
    }

    try {
      const historical = await getContentPlanRevision(planId, rev);
      if (historical) {
        setDraftPlan(historical);
        setViewingRevision(rev);
        setIsDirty(false);
        setSelectedSceneIndex(0);
      }
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    }
  };

  const handleBack = () => {
    if (isDirty) {
      const discard = window.confirm('You have unsaved changes. Discard and return to plans list?');
      if (!discard) return;
    }
    onBack();
  };

  if (isLoading) {
    return (
      <div className="flex flex-col items-center justify-center p-16 text-gray-400">
        <div className="w-8 h-8 border-2 border-indigo-500 border-t-transparent rounded-full animate-spin mb-3" />
        <p className="text-sm">Loading content plan editor...</p>
      </div>
    );
  }

  if (!draftPlan || !currentPlan) {
    return (
      <div className="p-8 text-center text-rose-300 bg-rose-950/40 rounded-xl border border-rose-800 space-y-4">
        <p className="text-sm font-semibold">{error || 'Could not load plan.'}</p>
        <Button onClick={onBack} variant="secondary">
          Back to List
        </Button>
      </div>
    );
  }

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

  const activeScene = draftPlan.scenes[selectedSceneIndex] || draftPlan.scenes[0];

  return (
    <div className="space-y-6 max-w-6xl mx-auto p-6">
      {/* Editor Header */}
      <div className="flex items-center justify-between border-b border-gray-800 pb-4">
        <div className="flex items-center gap-3">
          <Button variant="secondary" onClick={handleBack} className="text-xs px-3 py-1.5">
            ← Back
          </Button>
          <div>
            <div className="flex items-center gap-2.5">
              <h1 className="text-xl font-bold tracking-tight text-white">{draftPlan.title}</h1>
              <span className="text-xs font-mono px-2 py-0.5 rounded bg-gray-800 text-gray-300 border border-gray-700">
                Rev {draftPlan.revision}
              </span>
              <span
                className={`text-[10px] uppercase font-mono px-2 py-0.5 rounded border ${getStatusBadgeClass(
                  draftPlan.status
                )}`}
              >
                {draftPlan.status}
              </span>
            </div>
            <p className="text-xs text-gray-400 mt-0.5 line-clamp-1">{draftPlan.summary}</p>
          </div>
        </div>

        <div className="flex items-center gap-3">
          <div className="text-xs font-mono">
            {isSaving ? (
              <span className="text-amber-400 flex items-center gap-1.5">
                <span className="w-2 h-2 rounded-full bg-amber-400 animate-pulse" />
                Saving revision...
              </span>
            ) : isDirty ? (
              <span className="text-amber-400 font-semibold">Unsaved changes</span>
            ) : (
              <span className="text-gray-500">Saved</span>
            )}
          </div>

          <Button
            variant="secondary"
            className="text-xs"
            onClick={() => setIsHistoryOpen(!isHistoryOpen)}
          >
            History
          </Button>

          <Button
            onClick={handleSave}
            disabled={!isDirty || isReadOnly || isSaving}
            className="text-xs"
          >
            Save Revision
          </Button>
        </div>
      </div>

      {/* Historical Revision Banner */}
      {isReadOnly && (
        <div className="p-3 rounded-lg bg-indigo-950/60 border border-indigo-700 text-indigo-200 text-xs flex items-center justify-between">
          <span>
            Viewing Historical Revision {viewingRevision} (Read-Only). Changes are disabled.
          </span>
          <Button
            variant="secondary"
            className="text-xs px-2.5 py-1 bg-indigo-900 text-indigo-100 hover:bg-indigo-800"
            onClick={() => handleSelectRevision(currentPlan.revision)}
          >
            Return to Current (Rev {currentPlan.revision})
          </Button>
        </div>
      )}

      {/* Error Alert */}
      {error && (
        <div className="p-3 rounded-lg bg-rose-950/50 border border-rose-800/60 text-rose-300 text-xs flex items-center justify-between gap-3">
          <div className="space-y-0.5">
            <span className="font-bold block">
              {error.includes('content_plan_revision_conflict')
                ? 'Revision Conflict'
                : 'Error'}
            </span>
            <p>
              {error.includes('content_plan_revision_conflict')
                ? 'This plan has been updated in another session. Please reload to view and edit the latest revision.'
                : error}
            </p>
          </div>
          {error.includes('content_plan_revision_conflict') && (
            <Button
              variant="secondary"
              className="text-xs px-3 py-1 bg-rose-900/60 hover:bg-rose-800 text-rose-100 border-rose-700 whitespace-nowrap"
              onClick={() => {
                setError(null);
                setViewingRevision(null);
                void loadCurrentPlan();
              }}
            >
              Reload Latest Plan
            </Button>
          )}
        </div>
      )}

      {/* Revision History Drawer/Sidebar */}
      {isHistoryOpen && (
        <RevisionHistory
          planId={planId}
          currentRevision={currentPlan.revision}
          approvedRevision={currentPlan.status === 'approved' ? currentPlan.revision : null}
          selectedRevision={viewingRevision ?? currentPlan.revision}
          onSelectRevision={handleSelectRevision}
          onClose={() => setIsHistoryOpen(false)}
        />
      )}

      {/* Main Two-Column Layout */}
      <div className="grid grid-cols-1 lg:grid-cols-12 gap-6">
        {/* Left Column: Navigation Tabs & Scene List */}
        <div className="lg:col-span-4 space-y-4">
          <div className="flex border-b border-gray-800">
            <button
              onClick={() => setActiveTab('scenes')}
              className={`flex-1 py-2 text-xs font-semibold border-b-2 cursor-pointer transition-colors ${
                activeTab === 'scenes'
                  ? 'border-indigo-500 text-indigo-400'
                  : 'border-transparent text-gray-400 hover:text-gray-200'
              }`}
            >
              Scenes ({draftPlan.scenes.length})
            </button>
            <button
              onClick={() => setActiveTab('fullNarration')}
              className={`flex-1 py-2 text-xs font-semibold border-b-2 cursor-pointer transition-colors ${
                activeTab === 'fullNarration'
                  ? 'border-indigo-500 text-indigo-400'
                  : 'border-transparent text-gray-400 hover:text-gray-200'
              }`}
            >
              Full Narration
            </button>
          </div>

          {activeTab === 'scenes' ? (
            <div className="space-y-2">
              {draftPlan.scenes.map((scene, idx) => {
                const isSelected = selectedSceneIndex === idx;
                const claimCount = scene.claims.length;
                const durationSec = Math.round(scene.durationEstimateMs / 1000);

                return (
                  <div
                    key={scene.id}
                    onClick={() => setSelectedSceneIndex(idx)}
                    className={`p-3 rounded-xl border cursor-pointer transition-all ${
                      isSelected
                        ? 'bg-indigo-950/40 border-indigo-600'
                        : 'bg-gray-900 border-gray-800 hover:bg-gray-850'
                    }`}
                  >
                    <div className="flex items-center justify-between">
                      <span className="text-xs font-bold text-gray-200">
                        {scene.order}. {scene.title}
                      </span>
                      <span className="text-[11px] font-mono text-gray-400">{durationSec}s</span>
                    </div>
                    <p className="text-[11px] text-gray-400 truncate mt-1">{scene.narration}</p>
                    <div className="flex items-center gap-2 mt-2">
                      <span className="text-[10px] font-mono text-gray-500">
                        {claimCount} {claimCount === 1 ? 'claim' : 'claims'}
                      </span>
                    </div>
                  </div>
                );
              })}
            </div>
          ) : (
            <div className="p-4 rounded-xl bg-gray-900 border border-gray-800 space-y-2">
              <span className="text-[11px] font-semibold uppercase text-gray-400 block tracking-wider">
                Full Script Narration (Derived Aggregate)
              </span>
              <p className="text-xs text-gray-300 font-mono whitespace-pre-wrap leading-relaxed">
                {draftPlan.fullNarration}
              </p>
            </div>
          )}
        </div>

        {/* Right Column: Scene Editor */}
        <div className="lg:col-span-8">
          {activeTab === 'scenes' && activeScene ? (
            <SceneEditor
              scene={activeScene}
              onChange={handleSceneChange}
              isReadOnly={isReadOnly}
            />
          ) : (
            <div className="p-6 rounded-xl bg-gray-900 border border-gray-800 space-y-4">
              <h3 className="text-sm font-bold text-gray-200">Complete Script Overview</h3>
              <p className="text-xs text-gray-400">
                The full narration is deterministically derived from all scenes joined in order. To edit
                narration, select individual scenes from the left panel.
              </p>
              <div className="p-4 rounded-lg bg-gray-950 border border-gray-800 font-mono text-xs text-gray-200 whitespace-pre-wrap">
                {draftPlan.fullNarration}
              </div>
            </div>
          )}
        </div>
      </div>
    </div>
  );
};
