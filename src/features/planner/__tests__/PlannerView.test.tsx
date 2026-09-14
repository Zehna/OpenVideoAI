import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { PlannerView } from '../components/PlannerView';
import * as plannerApi from '../api';
import type { ContentPlanSummary, PlannerProviderStatus, Job, ContentPlan } from '../types';

vi.mock('../api', () => ({
  listProjectContentPlans: vi.fn(),
  getPlannerProviderStatus: vi.fn(),
  submitContentPlanJob: vi.fn(),
  getContentPlan: vi.fn(),
}));

describe('PlannerView Component', () => {
  const mockPlans: ContentPlanSummary[] = [
    {
      id: 'plan-1',
      projectId: 'proj-1',
      title: 'Explainer Video Plan',
      currentRevision: 2,
      status: 'draft',
      approvedRevision: null,
      approvedAt: null,
      createdAt: '2026-09-12T10:00:00Z',
      updatedAt: '2026-09-12T11:00:00Z',
    },
    {
      id: 'plan-2',
      projectId: 'proj-1',
      title: 'Product Teaser',
      currentRevision: 1,
      status: 'approved',
      approvedRevision: 1,
      approvedAt: '2026-09-12T12:00:00Z',
      createdAt: '2026-09-12T10:30:00Z',
      updatedAt: '2026-09-12T12:00:00Z',
    },
  ];

  const mockConfiguredStatus: PlannerProviderStatus = {
    provider: 'gemini',
    configured: true,
    model: 'gemini-2.5-flash',
    message: null,
  };

  const mockUnconfiguredStatus: PlannerProviderStatus = {
    provider: 'gemini',
    configured: false,
    model: 'gemini-2.5-flash',
    message: 'GEMINI_API_KEY environment variable is not set.',
  };

  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(plannerApi.listProjectContentPlans).mockResolvedValue(mockPlans);
    vi.mocked(plannerApi.getPlannerProviderStatus).mockResolvedValue(mockConfiguredStatus);
  });

  it('renders plan list and displays existing plans', async () => {
    render(<PlannerView projectId="proj-1" onBack={vi.fn()} />);

    expect(await screen.findByText('Explainer Video Plan')).toBeInTheDocument();
    expect(screen.getByText('Product Teaser')).toBeInTheDocument();
    expect(screen.getByText(/Rev 2/i)).toBeInTheDocument();
    expect(screen.getByText(/Rev 1/i)).toBeInTheDocument();
  });

  it('shows warning banner when AI provider is not configured', async () => {
    vi.mocked(plannerApi.getPlannerProviderStatus).mockResolvedValue(mockUnconfiguredStatus);

    render(<PlannerView projectId="proj-1" onBack={vi.fn()} />);

    expect(await screen.findByText(/GEMINI_API_KEY environment variable is not set/i)).toBeInTheDocument();
  });

  it('navigates to New Plan form and toggles between Topic and Script modes', async () => {
    render(<PlannerView projectId="proj-1" onBack={vi.fn()} />);

    const newPlanBtn = await screen.findByRole('button', { name: /\+ New Plan/i });
    fireEvent.click(newPlanBtn);

    expect(screen.getByText(/Create AI Content Plan/i)).toBeInTheDocument();
    expect(screen.getByPlaceholderText(/Enter video topic or concept/i)).toBeInTheDocument();

    const scriptModeBtn = screen.getByRole('button', { name: /Script/i });
    fireEvent.click(scriptModeBtn);

    expect(screen.getByPlaceholderText(/Paste your full script or outline/i)).toBeInTheDocument();
  });

  it('submits planning job and displays queued notification with View in Queue callback', async () => {
    const onOpenJobs = vi.fn();
    const mockJob: Job = {
      id: 'job-plan-999',
      jobType: 'content.plan',
      projectId: 'proj-1',
      status: 'queued',
      priority: 0,
      idempotencyKey: null,
      payload: '{}',
      payloadVersion: 1,
      result: null,
      error: null,
      retryCount: 0,
      maxRetries: 3,
      leasedBy: null,
      leaseToken: null,
      leaseExpiresAt: null,
      startedAt: null,
      completedAt: null,
      createdAt: '2026-09-12T10:00:00Z',
      updatedAt: '2026-09-12T10:00:00Z',
    };

    vi.mocked(plannerApi.submitContentPlanJob).mockResolvedValue(mockJob);

    render(<PlannerView projectId="proj-1" onBack={vi.fn()} onOpenJobs={onOpenJobs} />);

    fireEvent.click(await screen.findByRole('button', { name: /\+ New Plan/i }));

    const inputArea = screen.getByPlaceholderText(/Enter video topic or concept/i);
    fireEvent.change(inputArea, { target: { value: 'How quantum computing works' } });

    const generateBtn = screen.getByRole('button', { name: /Generate Plan/i });
    fireEvent.click(generateBtn);

    await waitFor(() => {
      expect(plannerApi.submitContentPlanJob).toHaveBeenCalledTimes(1);
    });

    expect(await screen.findByText(/Generation job submitted/i)).toBeInTheDocument();
    expect(screen.getByText(/job-plan-999/)).toBeInTheDocument();

    const viewQueueBtn = screen.getByRole('button', { name: /View in Queue/i });
    fireEvent.click(viewQueueBtn);
    expect(onOpenJobs).toHaveBeenCalledWith('job-plan-999');
  });

  it('opens ContentPlanEditor when plan is selected and mounts editor view', async () => {
    const mockPlan: ContentPlan = {
      id: 'plan-1',
      projectId: 'proj-1',
      revision: 2,
      status: 'draft',
      title: 'Explainer Video Plan',
      summary: 'Summary text',
      language: 'en',
      tone: 'informative',
      targetDurationMs: 60000,
      estimatedDurationMs: 60000,
      fullNarration: 'Narration text',
      scenes: [],
      sourceRefs: [],
      createdAt: '2026-09-12T10:00:00Z',
      updatedAt: '2026-09-12T11:00:00Z',
    };
    vi.mocked(plannerApi.getContentPlan).mockResolvedValueOnce(mockPlan);

    render(<PlannerView projectId="proj-1" onBack={vi.fn()} />);

    const openBtns = await screen.findAllByRole('button', { name: /Open/i });
    fireEvent.click(openBtns[0]);

    expect(await screen.findByRole('button', { name: /Save Revision/i })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: /Regenerate Plan/i })).toBeInTheDocument();
  });
});
