import React, { useState, useEffect, useCallback } from 'react';
import type { Project } from '../types';
import type { ImportedArtifact } from '../../artifacts/types';
import { updateProject } from '../api';
import { listProjectArtifacts, importArtifact } from '../../artifacts/api';
import { useAutosave } from '../hooks/useAutosave';
import { ArtifactList } from '../../artifacts/components/ArtifactList';
import { ImportModal } from '../../artifacts/components/ImportModal';
import { Button } from '../../../shared/components/Button';

interface ProjectDetailProps {
  project: Project;
  onBack: () => void;
  onProjectUpdated: (updated: Project) => void;
  onOpenEditor?: () => void;
  onOpenPlanner?: () => void;
}

export const ProjectDetail: React.FC<ProjectDetailProps> = ({
  project,
  onBack,
  onProjectUpdated,
  onOpenEditor,
  onOpenPlanner,
}) => {
  const [name, setName] = useState(project.name);
  const [description, setDescription] = useState(project.description || '');
  const [artifacts, setArtifacts] = useState<ImportedArtifact[]>([]);
  const [isLoadingArtifacts, setIsLoadingArtifacts] = useState(true);
  const [isImportModalOpen, setIsImportModalOpen] = useState(false);
  const [generalError, setGeneralError] = useState<string | null>(null);

  // Debounced Autosave for Name & Description
  const handleSave = useCallback(
    async (values: { name: string; description: string }) => {
      const trimmedName = values.name.trim();
      if (!trimmedName) return; // Don't save empty project name

      const updated = await updateProject(
        project.id,
        trimmedName,
        values.description.trim() || null
      );
      onProjectUpdated(updated);
    },
    [project.id, onProjectUpdated]
  );

  const { isSaving, isDirty, error: autosaveError } = useAutosave({
    value: { name, description },
    onSave: handleSave,
    delay: 400,
  });

  const loadArtifacts = useCallback(async () => {
    setIsLoadingArtifacts(true);
    setGeneralError(null);
    try {
      const list = await listProjectArtifacts(project.id);
      setArtifacts(list);
    } catch (err) {
      setGeneralError(err instanceof Error ? err.message : String(err));
    } finally {
      setIsLoadingArtifacts(false);
    }
  }, [project.id]);

  useEffect(() => {
    void loadArtifacts();
  }, [loadArtifacts]);

  const handleImport = async (sourcePath: string) => {
    const newArtifact = await importArtifact(project.id, sourcePath);
    // Refresh artifacts list
    setArtifacts((prev) => {
      const exists = prev.some(
        (a) => a.artifact.artifactId === newArtifact.artifact.artifactId
      );
      if (exists) return prev;
      return [newArtifact, ...prev];
    });
  };

  return (
    <div className="max-w-5xl mx-auto p-6 space-y-6">
      {/* Navigation & Header */}
      <div className="flex items-center justify-between border-b border-gray-800 pb-4">
        <div className="flex items-center gap-3">
          <Button variant="secondary" onClick={onBack} className="text-xs px-3 py-1.5">
            ? Back to Projects
          </Button>
          <div className="text-xs font-mono text-gray-500">
            ID: <span className="text-gray-400">{project.id}</span>
          </div>
        </div>
        <div className="flex items-center gap-3">
          <div className="text-xs font-mono">
            {isSaving ? (
              <span className="text-amber-400 flex items-center gap-1.5">
                <span className="w-2 h-2 rounded-full bg-amber-400 animate-pulse" />
                Saving...
              </span>
            ) : isDirty ? (
              <span className="text-gray-500">Unsaved changes</span>
            ) : (
              <span className="text-emerald-400 flex items-center gap-1.5">
                <span className="w-2 h-2 rounded-full bg-emerald-400" />
                All changes saved
              </span>
            )}
          </div>
          {onOpenPlanner && (
            <Button onClick={onOpenPlanner} variant="secondary">
              AI Planner
            </Button>
          )}
          {onOpenEditor && (
            <Button onClick={onOpenEditor} variant="secondary">
              Open Timeline Editor
            </Button>
          )}
          <Button onClick={() => setIsImportModalOpen(true)}>
            + Import Media
          </Button>
        </div>
      </div>

      {/* Errors */}
      {(generalError || autosaveError) && (
        <div className="p-3 rounded-lg bg-rose-950/50 border border-rose-800/60 text-rose-300 text-xs">
          {generalError || autosaveError}
        </div>
      )}

      {/* Project Meta Editor */}
      <div className="p-5 rounded-xl bg-gray-900 border border-gray-800 space-y-4">
        <div>
          <label className="block text-[11px] font-semibold text-gray-400 uppercase tracking-wider mb-1">
            Project Name
          </label>
          <input
            type="text"
            value={name}
            onChange={(e) => setName(e.target.value)}
            placeholder="Project name"
            className="w-full px-3 py-2 bg-gray-800/80 border border-gray-700 rounded-lg text-gray-100 font-semibold text-lg focus:outline-none focus:border-indigo-500"
          />
        </div>

        <div>
          <label className="block text-[11px] font-semibold text-gray-400 uppercase tracking-wider mb-1">
            Description
          </label>
          <textarea
            value={description}
            onChange={(e) => setDescription(e.target.value)}
            placeholder="Write notes, storyline, or project description..."
            rows={2}
            className="w-full px-3 py-2 bg-gray-800/80 border border-gray-700 rounded-lg text-gray-200 text-sm focus:outline-none focus:border-indigo-500 resize-none"
          />
        </div>
      </div>

      {/* Media Assets Section */}
      <div className="space-y-3">
        <div className="flex items-center justify-between">
          <h2 className="text-lg font-bold text-gray-100 tracking-tight">
            Media Assets ({artifacts.length})
          </h2>
          <Button
            variant="secondary"
            onClick={loadArtifacts}
            className="text-xs px-2.5 py-1"
          >
            Refresh Assets
          </Button>
        </div>

        <ArtifactList
          artifacts={artifacts}
          isLoading={isLoadingArtifacts}
          onImportClick={() => setIsImportModalOpen(true)}
        />
      </div>

      <ImportModal
        isOpen={isImportModalOpen}
        onClose={() => setIsImportModalOpen(false)}
        onImport={handleImport}
      />
    </div>
  );
};
