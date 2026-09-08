import { describe, it, expect, vi } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import { ProjectList } from '../components/ProjectList';
import type { Project } from '../types';

describe('ProjectList Component', () => {
  const mockProjects: Project[] = [
    {
      id: 'p1',
      name: 'Project 1',
      description: 'First test project',
      createdAt: '2026-09-08T00:00:00Z',
      updatedAt: '2026-09-08T01:00:00Z',
    },
    {
      id: 'p2',
      name: 'Project 2',
      description: null,
      createdAt: '2026-09-08T02:00:00Z',
      updatedAt: '2026-09-08T03:00:00Z',
    },
  ];

  it('renders list of projects correctly', () => {
    render(
      <ProjectList
        projects={mockProjects}
        isLoading={false}
        onSelectProject={vi.fn()}
        onCreateProject={vi.fn()}
        onRefresh={vi.fn()}
      />
    );

    expect(screen.getByText('Project 1')).toBeInTheDocument();
    expect(screen.getByText('First test project')).toBeInTheDocument();
    expect(screen.getByText('Project 2')).toBeInTheDocument();
  });

  it('renders empty state when there are no projects', () => {
    render(
      <ProjectList
        projects={[]}
        isLoading={false}
        onSelectProject={vi.fn()}
        onCreateProject={vi.fn()}
        onRefresh={vi.fn()}
      />
    );

    expect(screen.getByText(/no projects yet/i)).toBeInTheDocument();
    expect(screen.getByText(/create first project/i)).toBeInTheDocument();
  });

  it('clicking project triggers onSelectProject callback', () => {
    const onSelect = vi.fn();
    render(
      <ProjectList
        projects={mockProjects}
        isLoading={false}
        onSelectProject={onSelect}
        onCreateProject={vi.fn()}
        onRefresh={vi.fn()}
      />
    );

    fireEvent.click(screen.getByText('Project 1'));
    expect(onSelect).toHaveBeenCalledWith(mockProjects[0]);
  });
});
