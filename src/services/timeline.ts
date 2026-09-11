import { invoke } from '@tauri-apps/api/core';
import type { TimelineDocument, TimelineMutation } from '../types/timeline';

export const timelineService = {
  async createTimeline(
    projectId: string,
    name: string,
    width = 1920,
    height = 1080,
    frameRate = 30.0
  ): Promise<TimelineDocument> {
    return await invoke<TimelineDocument>('cmd_create_timeline', {
      projectId,
      name,
      width,
      height,
      frameRate,
    });
  },

  async getTimeline(timelineId: string): Promise<TimelineDocument> {
    return await invoke<TimelineDocument>('cmd_get_timeline', { timelineId });
  },

  async listProjectTimelines(projectId: string): Promise<TimelineDocument[]> {
    return await invoke<TimelineDocument[]>('cmd_list_project_timelines', {
      projectId,
    });
  },

  async applyMutation(
    timelineId: string,
    expectedRevision: number,
    mutation: TimelineMutation
  ): Promise<TimelineDocument> {
    return await invoke<TimelineDocument>('cmd_apply_timeline_mutation', {
      timelineId,
      expectedRevision,
      mutation,
    });
  },

  async undoTimeline(
    timelineId: string,
    expectedRevision: number
  ): Promise<TimelineDocument> {
    return await invoke<TimelineDocument>('cmd_undo_timeline', {
      timelineId,
      expectedRevision,
    });
  },

  async redoTimeline(
    timelineId: string,
    expectedRevision: number
  ): Promise<TimelineDocument> {
    return await invoke<TimelineDocument>('cmd_redo_timeline', {
      timelineId,
      expectedRevision,
    });
  },

  async getRenderSpec(timelineId: string): Promise<unknown> {
    return await invoke('cmd_build_render_spec', { timelineId });
  },

  async submitRender(
    timelineId: string,
    expectedRevision: number
  ): Promise<string> {
    return await invoke<string>('cmd_submit_timeline_render', {
      timelineId,
      expectedRevision,
    });
  },
};
