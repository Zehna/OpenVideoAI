import { invoke } from '@tauri-apps/api/core';
import type { ImportedArtifact } from './types';

export async function importArtifact(
  projectId: string,
  sourcePath: string
): Promise<ImportedArtifact> {
  return await invoke<ImportedArtifact>('import_artifact', {
    projectId,
    sourcePath,
  });
}

export async function listProjectArtifacts(
  projectId: string
): Promise<ImportedArtifact[]> {
  return await invoke<ImportedArtifact[]>('list_project_artifacts', {
    projectId,
  });
}
