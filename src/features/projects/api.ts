import { invoke } from '@tauri-apps/api/core';
import type { Project } from './types';

export async function createProject(
  name: string,
  description?: string | null
): Promise<Project> {
  return await invoke<Project>('create_project', { name, description });
}

export async function listProjects(): Promise<Project[]> {
  return await invoke<Project[]>('list_projects');
}

export async function getProject(projectId: string): Promise<Project> {
  return await invoke<Project>('get_project', { projectId });
}

export async function updateProject(
  projectId: string,
  name: string,
  description?: string | null
): Promise<Project> {
  return await invoke<Project>('update_project', {
    projectId,
    name,
    description,
  });
}
