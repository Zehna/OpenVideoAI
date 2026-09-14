import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { ProjectDetail } from '../components/ProjectDetail';
import * as projectApi from '../api';
import * as artifactApi from '../../artifacts/api';
import type { Project } from '../types';

vi.mock('../api', () => ({
  updateProject: vi.fn(),
}));

vi.mock('../../artifacts/api', () => ({
  listProjectArtifacts: vi.fn(),
  importArtifact: vi.fn(),
}));

describe('ProjectDetail Component', () => {
  const mockProject: Project = {
    id: 'proj-123',
    name: 'Vacation Reel',
    description: 'Holiday clip montage',
    createdAt: '2026-09-08T00:00:00Z',
    updatedAt: '2026-09-08T00:00:00Z',
  };

  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(artifactApi.listProjectArtifacts).mockResolvedValue([]);
  });

  it('renders project fields and back button', async () => {
    const onBack = vi.fn();
    render(
      <ProjectDetail
        project={mockProject}
        onBack={onBack}
        onProjectUpdated={vi.fn()}
      />
    );

    await waitFor(() => {
      expect(artifactApi.listProjectArtifacts).toHaveBeenCalledWith('proj-123');
    });

    expect(screen.getByDisplayValue('Vacation Reel')).toBeInTheDocument();
    expect(screen.getByDisplayValue('Holiday clip montage')).toBeInTheDocument();

    fireEvent.click(screen.getByText(/back to projects/i));
    expect(onBack).toHaveBeenCalled();
  });

  it('triggers debounced autosave when project name is changed', async () => {
    const onUpdated = vi.fn();
    vi.mocked(projectApi.updateProject).mockResolvedValueOnce({
      ...mockProject,
      name: 'Updated Reel',
    });

    render(
      <ProjectDetail
        project={mockProject}
        onBack={vi.fn()}
        onProjectUpdated={onUpdated}
      />
    );

    const nameInput = screen.getByDisplayValue('Vacation Reel');
    fireEvent.change(nameInput, { target: { value: 'Updated Reel' } });

    await waitFor(
      () => {
        expect(projectApi.updateProject).toHaveBeenCalledWith(
          'proj-123',
          'Updated Reel',
          'Holiday clip montage'
        );
      },
      { timeout: 2000 }
    );
  });

  it('renders AI Planner button and calls onOpenPlanner when clicked', async () => {
    const onOpenPlanner = vi.fn();
    render(
      <ProjectDetail
        project={mockProject}
        onBack={vi.fn()}
        onProjectUpdated={vi.fn()}
        onOpenPlanner={onOpenPlanner}
      />
    );

    await waitFor(() => {
      expect(artifactApi.listProjectArtifacts).toHaveBeenCalledWith('proj-123');
    });

    const plannerBtn = screen.getByRole('button', { name: /ai planner/i });
    expect(plannerBtn).toBeInTheDocument();
    fireEvent.click(plannerBtn);
    expect(onOpenPlanner).toHaveBeenCalledTimes(1);
  });
});
