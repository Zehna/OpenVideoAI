import React, { useState, useEffect, useCallback } from "react";
import "./App.css";
import { HealthDashboard } from "../features/health/HealthDashboard";
import { ProjectList } from "../features/projects/components/ProjectList";
import { ProjectDetail } from "../features/projects/components/ProjectDetail";
import { JobList } from "../features/jobs/components/JobList";
import { listProjects, createProject } from "../features/projects/api";
import type { Project } from "../features/projects/types";

export const App: React.FC = () => {
  const [activeTab, setActiveTab] = useState<"projects" | "jobs" | "health">("projects");
  const [projects, setProjects] = useState<Project[]>([]);
  const [selectedProject, setSelectedProject] = useState<Project | null>(null);
  const [isLoading, setIsLoading] = useState<boolean>(true);

  const loadProjects = useCallback(async () => {
    setIsLoading(true);
    try {
      const data = await listProjects();
      setProjects(data);
    } catch (err) {
      console.error("Failed to load projects:", err);
    } finally {
      setIsLoading(false);
    }
  }, []);

  useEffect(() => {
    void loadProjects();
  }, [loadProjects]);

  const handleCreateProject = async (name: string, description: string | null) => {
    const newProject = await createProject(name, description);
    setProjects((prev) => [newProject, ...prev]);
    setSelectedProject(newProject);
  };

  const handleProjectUpdated = (updated: Project) => {
    setProjects((prev) =>
      prev.map((p) => (p.id === updated.id ? updated : p))
    );
    setSelectedProject(updated);
  };

  return (
    <div className="min-h-screen bg-gray-950 text-gray-100 flex flex-col font-sans">
      {/* App Header */}
      <header className="border-b border-gray-800 bg-gray-900/60 backdrop-blur px-6 py-3 flex items-center justify-between sticky top-0 z-40">
        <div className="flex items-center gap-6">
          <div className="flex items-center gap-2">
            <div className="w-3 h-3 rounded-full bg-indigo-500 shadow-sm shadow-indigo-500/50" />
            <span className="font-bold text-lg tracking-tight text-white">
              OpenVideoAI
            </span>
            <span className="text-[10px] font-mono uppercase bg-indigo-950/80 text-indigo-400 px-2 py-0.5 rounded border border-indigo-800/60 ml-2">
              Phase 2 Jobs & Events
            </span>
          </div>

          <nav className="flex items-center gap-1">
            <button
              onClick={() => {
                setActiveTab("projects");
              }}
              className={`px-3 py-1.5 rounded-lg text-xs font-medium cursor-pointer transition-colors ${
                activeTab === "projects"
                  ? "bg-gray-800 text-indigo-300 font-semibold"
                  : "text-gray-400 hover:text-gray-200"
              }`}
            >
              Projects
            </button>
            <button
              onClick={() => setActiveTab("jobs")}
              className={`px-3 py-1.5 rounded-lg text-xs font-medium cursor-pointer transition-colors ${
                activeTab === "jobs"
                  ? "bg-gray-800 text-indigo-300 font-semibold"
                  : "text-gray-400 hover:text-gray-200"
              }`}
            >
              Jobs & Queue
            </button>
            <button
              onClick={() => setActiveTab("health")}
              className={`px-3 py-1.5 rounded-lg text-xs font-medium cursor-pointer transition-colors ${
                activeTab === "health"
                  ? "bg-gray-800 text-indigo-300 font-semibold"
                  : "text-gray-400 hover:text-gray-200"
              }`}
            >
              System Health
            </button>
          </nav>
        </div>
      </header>

      {/* Main Content */}
      <main className="flex-1 py-4">
        {activeTab === "health" ? (
          <HealthDashboard />
        ) : activeTab === "jobs" ? (
          <JobList />
        ) : selectedProject ? (
          <ProjectDetail
            project={selectedProject}
            onBack={() => {
              setSelectedProject(null);
              void loadProjects();
            }}
            onProjectUpdated={handleProjectUpdated}
          />
        ) : (
          <ProjectList
            projects={projects}
            isLoading={isLoading}
            onSelectProject={(p) => setSelectedProject(p)}
            onCreateProject={handleCreateProject}
            onRefresh={loadProjects}
          />
        )}
      </main>
    </div>
  );
};

export default App;
