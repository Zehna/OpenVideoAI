import React, { useState } from 'react';
import type { Project } from '../types';
import { Button } from '../../../shared/components/Button';
import { CreateProjectModal } from './CreateProjectModal';

interface ProjectListProps {
  projects: Project[];
  isLoading: boolean;
  onSelectProject: (project: Project) => void;
  onCreateProject: (name: string, description: string | null) => Promise<void>;
  onRefresh: () => void;
}

export const ProjectList: React.FC<ProjectListProps> = ({
  projects,
  isLoading,
  onSelectProject,
  onCreateProject,
  onRefresh,
}) => {
  const [isModalOpen, setIsModalOpen] = useState(false);

  return (
    <div className="max-w-5xl mx-auto p-6 space-y-6">
      <div className="flex items-center justify-between border-b border-gray-800 pb-4">
        <div>
          <h1 className="text-2xl font-bold text-gray-100 tracking-tight">
            Projects
          </h1>
          <p className="text-sm text-gray-400 mt-1">
            Create or select a project to manage imported media assets
          </p>
        </div>
        <div className="flex gap-2">
          <Button variant="secondary" onClick={onRefresh}>
            Refresh
          </Button>
          <Button onClick={() => setIsModalOpen(true)}>
            + New Project
          </Button>
        </div>
      </div>

      {isLoading ? (
        <div className="flex flex-col items-center justify-center min-h-[300px] text-gray-400">
          <div className="w-8 h-8 border-2 border-indigo-500 border-t-transparent rounded-full animate-spin mb-4" />
          <p className="text-sm">Loading projects...</p>
        </div>
      ) : projects.length === 0 ? (
        <div className="flex flex-col items-center justify-center min-h-[300px] border border-dashed border-gray-800 rounded-2xl p-8 text-center">
          <div className="w-12 h-12 rounded-full bg-indigo-950/60 border border-indigo-800/80 flex items-center justify-center text-indigo-400 font-bold mb-4">
            ??
          </div>
          <h3 className="text-base font-semibold text-gray-200">No projects yet</h3>
          <p className="text-xs text-gray-400 max-w-sm mt-1 mb-4">
            Get started by creating your first video project to organize media assets.
          </p>
          <Button onClick={() => setIsModalOpen(true)}>
            Create First Project
          </Button>
        </div>
      ) : (
        <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4">
          {projects.map((project) => (
            <div
              key={project.id}
              onClick={() => onSelectProject(project)}
              className="p-5 rounded-xl bg-gray-900 border border-gray-800 hover:border-indigo-600/70 hover:bg-gray-850 transition-all cursor-pointer shadow-sm group flex flex-col justify-between"
            >
              <div>
                <div className="flex items-center justify-between mb-2">
                  <h3 className="font-semibold text-gray-100 group-hover:text-indigo-300 transition-colors truncate">
                    {project.name}
                  </h3>
                </div>
                {project.description ? (
                  <p className="text-xs text-gray-400 line-clamp-2 leading-relaxed">
                    {project.description}
                  </p>
                ) : (
                  <p className="text-xs text-gray-500 italic">No description provided</p>
                )}
              </div>
              <div className="mt-4 pt-3 border-t border-gray-800/80 flex items-center justify-between text-[11px] text-gray-500 font-mono">
                <span>Updated {new Date(project.updatedAt).toLocaleDateString()}</span>
                <span className="text-indigo-400 opacity-0 group-hover:opacity-100 transition-opacity">
                  Open ?
                </span>
              </div>
            </div>
          ))}
        </div>
      )}

      <CreateProjectModal
        isOpen={isModalOpen}
        onClose={() => setIsModalOpen(false)}
        onCreate={onCreateProject}
      />
    </div>
  );
};
