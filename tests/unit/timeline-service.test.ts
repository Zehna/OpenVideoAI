import { describe, it, expect, vi, beforeEach } from 'vitest';
import { invoke } from '@tauri-apps/api/core';
import { timelineService } from '../../src/services/timeline';

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
}));

describe('timelineService', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('creates timeline with default canvas', async () => {
    const mockTimeline = {
      id: 'tl1',
      projectId: 'p1',
      name: 'Main Timeline',
      revision: 1,
      canvas: { width: 1920, height: 1080, frameRate: 30.0 },
      tracks: [],
      createdAt: '2026-09-10T12:00:00Z',
      updatedAt: '2026-09-10T12:00:00Z',
    };
    vi.mocked(invoke).mockResolvedValueOnce(mockTimeline);

    const res = await timelineService.createTimeline('p1', 'Main Timeline', 1920, 1080, 30.0);
    expect(invoke).toHaveBeenCalledWith('cmd_create_timeline', {
      projectId: 'p1',
      name: 'Main Timeline',
      width: 1920,
      height: 1080,
      frameRate: 30.0,
    });
    expect(res.id).toBe('tl1');
  });

  it('applies mutation with expected revision', async () => {
    const updated = {
      id: 'tl1',
      revision: 2,
      tracks: [{ id: 'tr1', name: 'V1', kind: 'video', order: 0, muted: false, clips: [] }],
    };
    vi.mocked(invoke).mockResolvedValueOnce(updated);

    const mutation = { type: 'add_track', kind: 'video', name: 'V1' };
    const res = await timelineService.applyMutation('tl1', 1, mutation);

    expect(invoke).toHaveBeenCalledWith('cmd_apply_timeline_mutation', {
      timelineId: 'tl1',
      expectedRevision: 1,
      mutation,
    });
    expect(res.revision).toBe(2);
  });

  it('submits render with expected revision', async () => {
    vi.mocked(invoke).mockResolvedValueOnce('job-render-123');

    const jobId = await timelineService.submitRender('tl1', 2);
    expect(invoke).toHaveBeenCalledWith('cmd_submit_timeline_render', {
      timelineId: 'tl1',
      expectedRevision: 2,
    });
    expect(jobId).toBe('job-render-123');
  });
});
