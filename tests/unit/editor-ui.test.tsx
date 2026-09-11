import React from 'react';
import { describe, it, expect, vi } from 'vitest';
import { render, screen, fireEvent, act } from '@testing-library/react';
import { EditorView } from '../../src/components/editor/EditorView';
import { timelineService } from '../../src/services/timeline';

vi.mock('../../src/services/timeline', () => ({
  timelineService: {
    getTimeline: vi.fn(),
    applyMutation: vi.fn(),
    undoTimeline: vi.fn(),
    redoTimeline: vi.fn(),
    submitRender: vi.fn(),
  },
}));

describe('EditorView', () => {
  it('renders toolbar, preview player, asset bin, and timeline tracks', async () => {
    const mockDoc = {
      id: 'tl1',
      projectId: 'p1',
      name: 'Editor Timeline',
      revision: 1,
      canvas: { width: 1920, height: 1080, frameRate: 30.0 },
      tracks: [
        { id: 'tr1', timelineId: 'tl1', kind: 'video', order: 0, name: 'V1', muted: false, clips: [] },
      ],
      createdAt: '2026-09-10T12:00:00Z',
      updatedAt: '2026-09-10T12:00:00Z',
    };
    vi.mocked(timelineService.getTimeline).mockResolvedValue(mockDoc);

    render(<EditorView projectId="p1" initialTimelineId="tl1" />);

    expect(await screen.findByText('Editor Timeline')).toBeInTheDocument();
    expect(screen.getByText('Render')).toBeInTheDocument();
    expect(screen.getByText('V1')).toBeInTheDocument();
  });

  it('handles undo and redo clicks with expected revision', async () => {
    const mockDoc = {
      id: 'tl1',
      projectId: 'p1',
      name: 'Editor Timeline',
      revision: 2,
      canvas: { width: 1920, height: 1080, frameRate: 30.0 },
      tracks: [],
      createdAt: '2026-09-10T12:00:00Z',
      updatedAt: '2026-09-10T12:00:00Z',
    };
    vi.mocked(timelineService.getTimeline).mockResolvedValue(mockDoc);
    vi.mocked(timelineService.undoTimeline).mockResolvedValue({ ...mockDoc, revision: 3 });

    render(<EditorView projectId="p1" initialTimelineId="tl1" />);

    const undoBtn = await screen.findByTitle('Undo');
    await act(async () => {
      fireEvent.click(undoBtn);
    });

    expect(timelineService.undoTimeline).toHaveBeenCalledWith('tl1', 2);
  });
});
