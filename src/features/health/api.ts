import { invoke } from '@tauri-apps/api/core';
import type { HealthSnapshot } from './types';

export async function fetchHealthSnapshot(): Promise<HealthSnapshot> {
  return await invoke<HealthSnapshot>('get_health_snapshot');
}
