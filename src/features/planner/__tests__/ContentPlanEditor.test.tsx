import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { ContentPlanEditor } from '../components/ContentPlanEditor';
import * as plannerApi from '../api';
import type { ContentPlan, ContentPlanRevisionSummary, Job } from '../types';

vi.mock('../api', () => ({
  getContentPlan: vi.fn(),
  saveContentPlanRevision: vi.fn(),
  getContentPlanHistory: vi.fn(),
  getContentPlanRevision: vi.fn(),
  submitContentPlanRegeneration: vi.fn(),
  approveContentPlan: vi.fn(),
  archiveContentPlan: vi.fn(),
}));

describe('ContentPlanEditor Component', () => {
  const mockPlan: ContentPlan = {
    id: 'plan-123',
    projectId: 'proj-1',
    revision: 1,
    status: 'draft',
    title: 'How Fusion Works',
    summary: 'An introductory overview of magnetic confinement fusion.',
    language: 'en',
    tone: 'informative',
    targetDurationMs: 30000,
    estimatedDurationMs: 30000,
    fullNarration: 'Welcome to stellar energy on Earth.\n\nMagnets confine hot plasma.',
    scenes: [
      {
        id: 'scene-1',
        order: 1,
        title: 'Introduction',
        narration: 'Welcome to stellar energy on Earth.',
        durationEstimateMs: 15000,
        visualDescription: 'Tokamak reactor core glowing.',
        mediaIntent: ['stock_video'],
        stockSearchQueries: ['fusion reactor glowing'],
        imagePrompt: null,
        videoPrompt: null,
        claims: [
          {
            id: 'claim-1',
            text: 'Stellar energy reproduced on Earth',
            sourceRefs: [{ sourceId: 'src-1', locator: 'p. 1' }],
            groundingStatus: 'supported',
            groundingValid: true,
          },
          {
            id: 'claim-2',
            text: 'Plasma temperature exceeds sun core',
            sourceRefs: [],
            groundingStatus: 'model_knowledge',
            groundingValid: true,
          },
        ],
        sourceRefs: [{ sourceId: 'src-1' }],
      },
      {
        id: 'scene-2',
        order: 2,
        title: 'Magnetic Confinement',
        narration: 'Magnets confine hot plasma.',
        durationEstimateMs: 15000,
        visualDescription: 'Magnetic field line animations.',
        mediaIntent: ['motion_graphics' as any],
        stockSearchQueries: ['magnetic fields'],
        imagePrompt: null,
        videoPrompt: null,
        claims: [
          {
            id: 'claim-3',
            text: 'Field coils generate 10 Tesla',
            sourceRefs: [],
            groundingStatus: 'uncertain',
            groundingValid: false,
          },
          {
            id: 'claim-4',
            text: 'Edited claim requiring review',
            sourceRefs: [],
            groundingStatus: 'modified',
            groundingValid: false,
          },
        ],
        sourceRefs: [],
      },
    ],
    sourceRefs: [{ sourceId: 'src-1' }],
    createdAt: '2026-09-12T10:00:00Z',
    updatedAt: '2026-09-12T10:00:00Z',
  };

  const mockHistory: ContentPlanRevisionSummary[] = [
    {
      planId: 'plan-123',
      revision: 1,
      parentRevision: null,
      operationType: 'create',
      createdBy: 'test-user',
      createdAt: '2026-09-12T10:00:00Z',
    },
  ];

  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(plannerApi.getContentPlan).mockImplementation(async () => JSON.parse(JSON.stringify(mockPlan)));
    vi.mocked(plannerApi.getContentPlanHistory).mockResolvedValue(mockHistory);
    vi.mocked(plannerApi.saveContentPlanRevision).mockImplementation(async (_id, expectedRev, plan) => ({
      ...plan,
      revision: expectedRev + 1,
    }));
  });

  it('renders loaded plan, scene list, and grounding badges for all statuses', async () => {
    render(<ContentPlanEditor planId="plan-123" onBack={vi.fn()} />);

    expect(await screen.findByText('How Fusion Works')).toBeInTheDocument();
    expect(screen.getByText(/1\.\s+Introduction/)).toBeInTheDocument();
    expect(screen.getByText(/2\.\s+Magnetic Confinement/)).toBeInTheDocument();

    // Verify grounding badge states
    expect(screen.getByText(/Supported/i)).toBeInTheDocument();
    expect(screen.getByText(/AI Knowledge/i)).toBeInTheDocument();

    // Select scene 2 to inspect uncertain & modified
    fireEvent.click(screen.getByText(/2\.\s+Magnetic Confinement/));
    expect(await screen.findByText(/Needs review/i)).toBeInTheDocument();
    expect(screen.getByText(/Modified/i)).toBeInTheDocument();
  });

  it('tracks dirty state upon editing scene and disables Save when not dirty', async () => {
    render(<ContentPlanEditor planId="plan-123" onBack={vi.fn()} />);

    const saveBtn = await screen.findByRole('button', { name: /Save Revision/i });
    expect(saveBtn).toBeDisabled();

    // Edit scene 1 narration
    const narrationInput = screen.getByDisplayValue('Welcome to stellar energy on Earth.');
    fireEvent.change(narrationInput, { target: { value: 'Welcome to future fusion energy.' } });

    expect(saveBtn).not.toBeDisabled();
    expect(screen.getByText(/Unsaved changes/i)).toBeInTheDocument();
  });

  it('saves revision with expectedRevision and increments revision number', async () => {
    render(<ContentPlanEditor planId="plan-123" onBack={vi.fn()} />);

    const saveBtn = await screen.findByRole('button', { name: /Save Revision/i });

    const narrationInput = screen.getByDisplayValue('Welcome to stellar energy on Earth.');
    fireEvent.change(narrationInput, { target: { value: 'Updated narration line.' } });

    fireEvent.click(saveBtn);

    await waitFor(() => {
      expect(plannerApi.saveContentPlanRevision).toHaveBeenCalledWith(
        'plan-123',
        1,
        expect.objectContaining({
          id: 'plan-123',
        })
      );
    });

    expect(await screen.findByText(/Rev 2/i)).toBeInTheDocument();
    expect(saveBtn).toBeDisabled();
  });

  it('displays conflict error alert when save returns content_plan_revision_conflict', async () => {
    vi.mocked(plannerApi.saveContentPlanRevision).mockRejectedValueOnce(
      new Error('content_plan_revision_conflict: expected revision 1 but actual is 2')
    );

    render(<ContentPlanEditor planId="plan-123" onBack={vi.fn()} />);

    const saveBtn = await screen.findByRole('button', { name: /Save Revision/i });
    const narrationInput = screen.getByDisplayValue('Welcome to stellar energy on Earth.');
    fireEvent.change(narrationInput, { target: { value: 'New text' } });

    fireEvent.click(saveBtn);

    expect(await screen.findByText('Revision Conflict')).toBeInTheDocument();
  });

  it('renders read-only full narration view tab', async () => {
    render(<ContentPlanEditor planId="plan-123" onBack={vi.fn()} />);

    await screen.findByText('How Fusion Works');
    const fullNarrationTab = screen.getByRole('button', { name: /Full Narration/i });
    fireEvent.click(fullNarrationTab);

    expect(screen.getByText('Complete Script Overview')).toBeInTheDocument();
    expect(screen.getAllByText(/Welcome to stellar energy on Earth/i).length).toBeGreaterThanOrEqual(2);
    expect(screen.getAllByText(/Magnets confine hot plasma/i).length).toBeGreaterThanOrEqual(2);
  });

  it('opens revision history and loads historical revision read-only', async () => {
    const planRev2: ContentPlan = {
      ...mockPlan,
      revision: 2,
    };
    const history: ContentPlanRevisionSummary[] = [
      {
        planId: 'plan-123',
        revision: 2,
        parentRevision: 1,
        operationType: 'manual_edit',
        createdBy: 'test-user',
        createdAt: '2026-09-12T11:00:00Z',
      },
      {
        planId: 'plan-123',
        revision: 1,
        parentRevision: null,
        operationType: 'create',
        createdBy: 'test-user',
        createdAt: '2026-09-12T10:00:00Z',
      },
    ];
    vi.mocked(plannerApi.getContentPlan).mockResolvedValueOnce(planRev2);
    vi.mocked(plannerApi.getContentPlanHistory).mockResolvedValueOnce(history);

    const historicalPlan: ContentPlan = {
      ...mockPlan,
      revision: 1,
      title: 'Historical Draft 1',
    };
    vi.mocked(plannerApi.getContentPlanRevision).mockResolvedValueOnce(historicalPlan);

    render(<ContentPlanEditor planId="plan-123" onBack={vi.fn()} />);

    const historyBtn = await screen.findByRole('button', { name: /History/i });
    fireEvent.click(historyBtn);

    expect(await screen.findByText('Revision 1')).toBeInTheDocument();
    fireEvent.click(screen.getByText('Revision 1'));

    await waitFor(() => {
      expect(plannerApi.getContentPlanRevision).toHaveBeenCalledWith('plan-123', 1);
    });

    expect(await screen.findByText(/Viewing Historical Revision 1/i)).toBeInTheDocument();
    const saveBtn = screen.getByRole('button', { name: /Save Revision/i });
    expect(saveBtn).toBeDisabled();
    const archiveBtn = screen.getByRole('button', { name: /Archive/i });
    expect(archiveBtn).toBeDisabled();
  });

  it('prompts user confirmation on back navigation when dirty and respects cancel/confirm', async () => {
    const onBack = vi.fn();
    const confirmSpy = vi.spyOn(window, 'confirm');

    render(<ContentPlanEditor planId="plan-123" onBack={onBack} />);

    await screen.findByText('How Fusion Works');
    const backBtn = screen.getByRole('button', { name: /Back/i });

    // When clean, back navigates immediately without prompt
    fireEvent.click(backBtn);
    expect(confirmSpy).not.toHaveBeenCalled();
    expect(onBack).toHaveBeenCalledTimes(1);

    // Now make it dirty
    const narrationInput = screen.getByDisplayValue('Welcome to stellar energy on Earth.');
    fireEvent.change(narrationInput, { target: { value: 'Modified text.' } });

    // User cancels confirm
    confirmSpy.mockReturnValueOnce(false);
    fireEvent.click(backBtn);
    expect(confirmSpy).toHaveBeenCalledTimes(1);
    expect(onBack).toHaveBeenCalledTimes(1); // Not called again

    // User confirms discard
    confirmSpy.mockReturnValueOnce(true);
    fireEvent.click(backBtn);
    expect(confirmSpy).toHaveBeenCalledTimes(2);
    expect(onBack).toHaveBeenCalledTimes(2);

    confirmSpy.mockRestore();
  });

  it('provides reload button upon revision conflict and refreshes latest plan', async () => {
    vi.mocked(plannerApi.saveContentPlanRevision).mockRejectedValueOnce(
      new Error('content_plan_revision_conflict: expected revision 1 but actual is 2')
    );

    render(<ContentPlanEditor planId="plan-123" onBack={vi.fn()} />);

    await screen.findByText('How Fusion Works');
    const narrationInput = screen.getByDisplayValue('Welcome to stellar energy on Earth.');
    fireEvent.change(narrationInput, { target: { value: 'Draft update' } });

    const saveBtn = screen.getByRole('button', { name: /Save Revision/i });
    fireEvent.click(saveBtn);

    expect(await screen.findByText('Revision Conflict')).toBeInTheDocument();
    const reloadBtn = screen.getByRole('button', { name: /Reload Latest Plan/i });
    expect(reloadBtn).toBeInTheDocument();

    fireEvent.click(reloadBtn);
    await waitFor(() => {
      expect(plannerApi.getContentPlan).toHaveBeenCalledTimes(2);
    });
  });

  it('blocks regeneration when dirty and submits plan regeneration job when clean', async () => {
    const onOpenJobs = vi.fn();
    const mockJob: Job = {
      id: 'job-regen-101',
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
    vi.mocked(plannerApi.submitContentPlanRegeneration).mockResolvedValueOnce(mockJob);

    render(<ContentPlanEditor planId="plan-123" onBack={vi.fn()} onOpenJobs={onOpenJobs} />);

    await screen.findByText('How Fusion Works');
    const regenBtn = screen.getByRole('button', { name: /Regenerate Plan/i });

    // When clean, can click Regenerate Plan
    fireEvent.click(regenBtn);
    expect(screen.getByText(/Regenerate Content Plan/i)).toBeInTheDocument();
    expect(screen.getByText(/preserved as Revision 1/i)).toBeInTheDocument();

    const instructionInput = screen.getByPlaceholderText(/Optional prompt instruction/i);
    fireEvent.change(instructionInput, { target: { value: 'Focus on stellarators' } });

    const submitBtn = screen.getByRole('button', { name: /Submit Regeneration/i });
    fireEvent.click(submitBtn);

    await waitFor(() => {
      expect(plannerApi.submitContentPlanRegeneration).toHaveBeenCalledWith(
        expect.objectContaining({
          planId: 'plan-123',
          baseRevision: 1,
          operation: 'regenerate_plan',
          userInstruction: 'Focus on stellarators',
        })
      );
    });

    expect(await screen.findByText(/job-regen-101/i)).toBeInTheDocument();
    const viewQueueBtn = screen.getByRole('button', { name: /View in Queue/i });
    fireEvent.click(viewQueueBtn);
    expect(onOpenJobs).toHaveBeenCalledWith('job-regen-101');

    // Now test dirty state blocks regeneration
    const narrationInput = screen.getByDisplayValue('Welcome to stellar energy on Earth.');
    fireEvent.change(narrationInput, { target: { value: 'Unsaved edit' } });
    expect(regenBtn).toBeDisabled();
  });

  it('triggers scene and narration regeneration with target sceneId', async () => {
    const mockJob: Job = {
      id: 'job-regen-scene-202',
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
    vi.mocked(plannerApi.submitContentPlanRegeneration).mockResolvedValue(mockJob);

    render(<ContentPlanEditor planId="plan-123" onBack={vi.fn()} />);

    await screen.findByText('How Fusion Works');

    // Click Regenerate Scene on scene-1
    const regenSceneBtn = screen.getByRole('button', { name: /Regenerate Scene/i });
    fireEvent.click(regenSceneBtn);

    expect(screen.getByText(/Regenerate Scene 1/i)).toBeInTheDocument();
    const submitBtn = screen.getByRole('button', { name: /Submit Regeneration/i });
    fireEvent.click(submitBtn);

    await waitFor(() => {
      expect(plannerApi.submitContentPlanRegeneration).toHaveBeenCalledWith(
        expect.objectContaining({
          planId: 'plan-123',
          baseRevision: 1,
          operation: 'regenerate_scene',
          sceneId: 'scene-1',
        })
      );
    });

    // Click Regenerate Narration
    const regenNarrationBtn = screen.getByRole('button', { name: /Regenerate Narration/i });
    fireEvent.click(regenNarrationBtn);

    expect(screen.getByText(/Regenerate Narration for Scene 1/i)).toBeInTheDocument();
    const submitNarrationBtn = screen.getByRole('button', { name: /Submit Regeneration/i });
    fireEvent.click(submitNarrationBtn);

    await waitFor(() => {
      expect(plannerApi.submitContentPlanRegeneration).toHaveBeenCalledWith(
        expect.objectContaining({
          planId: 'plan-123',
          baseRevision: 1,
          operation: 'regenerate_narration',
          sceneId: 'scene-1',
        })
      );
    });
  });

  it('opens approval modal with breakdown and approves plan successfully', async () => {
    vi.mocked(plannerApi.approveContentPlan).mockResolvedValueOnce({
      id: 'plan-123',
      projectId: 'proj-1',
      title: 'How Fusion Works',
      currentRevision: 1,
      status: 'approved',
      approvedRevision: 1,
      approvedAt: '2026-09-12T12:00:00Z',
      createdAt: '2026-09-12T10:00:00Z',
      updatedAt: '2026-09-12T12:00:00Z',
    });

    render(<ContentPlanEditor planId="plan-123" onBack={vi.fn()} />);

    await screen.findByText('How Fusion Works');
    const approveBtn = screen.getByRole('button', { name: /Approve Plan/i });
    fireEvent.click(approveBtn);

    // Approval modal displays summary metrics and warning for uncertain/modified
    expect(screen.getByText(/Approve Content Plan \(Revision 1\)/i)).toBeInTheDocument();
    expect(screen.getByText(/Total Scenes: 2/i)).toBeInTheDocument();
    expect(screen.getByText(/Estimated: 30s/i)).toBeInTheDocument();
    expect(screen.getByText(/• 1 supported/i)).toBeInTheDocument();
    expect(screen.getByText(/• 1 AI knowledge/i)).toBeInTheDocument();
    expect(screen.getByText(/• 1 uncertain/i)).toBeInTheDocument();
    expect(screen.getByText(/• 1 modified/i)).toBeInTheDocument();
    expect(screen.getByText(/Warning:.*ungrounded or modified/i)).toBeInTheDocument();

    const confirmApproveBtn = screen.getByRole('button', { name: /Confirm Approval/i });
    fireEvent.click(confirmApproveBtn);

    await waitFor(() => {
      expect(plannerApi.approveContentPlan).toHaveBeenCalledWith('plan-123', 1);
    });

    // Plan status badge is now approved
    expect(await screen.findByText('approved')).toBeInTheDocument();
  });

  it('displays warning in approval modal when plan has only model_knowledge claims', async () => {
    const planWithModelKnowledgeOnly: ContentPlan = {
      ...mockPlan,
      scenes: [
        {
          ...mockPlan.scenes[0],
          claims: [
            {
              id: 'claim-mk-1',
              text: 'Nuclear fusion produces clean energy.',
              groundingStatus: 'model_knowledge',
              groundingValid: true,
              sourceRefs: [],
            },
          ],
        },
      ],
    };
    vi.mocked(plannerApi.getContentPlan).mockResolvedValueOnce(planWithModelKnowledgeOnly);

    render(<ContentPlanEditor planId="plan-123" onBack={vi.fn()} />);

    await screen.findByText('How Fusion Works');
    const approveBtn = screen.getByRole('button', { name: /Approve Plan/i });
    fireEvent.click(approveBtn);

    expect(screen.getByText(/• 1 AI knowledge/i)).toBeInTheDocument();
    expect(screen.getByText(/Warning:.*ungrounded or modified/i)).toBeInTheDocument();
    expect(screen.getByText(/1 AI knowledge claims/i)).toBeInTheDocument();
  });

  it('displays error message when approveContentPlan rejects', async () => {
    vi.mocked(plannerApi.approveContentPlan).mockRejectedValueOnce(
      new Error('plan_duration_out_of_tolerance: duration 45s exceeds tolerance')
    );

    render(<ContentPlanEditor planId="plan-123" onBack={vi.fn()} />);

    await screen.findByText('How Fusion Works');
    fireEvent.click(screen.getByRole('button', { name: /Approve Plan/i }));

    const confirmApproveBtn = screen.getByRole('button', { name: /Confirm Approval/i });
    fireEvent.click(confirmApproveBtn);

    expect(await screen.findByText(/plan_duration_out_of_tolerance/i)).toBeInTheDocument();
  });

  it('archives plan after confirmation and returns to plan list', async () => {
    const onBack = vi.fn();
    const confirmSpy = vi.spyOn(window, 'confirm').mockReturnValue(true);

    vi.mocked(plannerApi.archiveContentPlan).mockResolvedValueOnce({
      id: 'plan-123',
      projectId: 'proj-1',
      title: 'How Fusion Works',
      currentRevision: 1,
      status: 'archived',
      approvedRevision: null,
      approvedAt: null,
      createdAt: '2026-09-12T10:00:00Z',
      updatedAt: '2026-09-12T12:00:00Z',
    });

    render(<ContentPlanEditor planId="plan-123" onBack={onBack} />);

    await screen.findByText('How Fusion Works');
    const archiveBtn = screen.getByRole('button', { name: /Archive/i });
    fireEvent.click(archiveBtn);

    expect(confirmSpy).toHaveBeenCalled();
    await waitFor(() => {
      expect(plannerApi.archiveContentPlan).toHaveBeenCalledWith('plan-123', 1);
      expect(onBack).toHaveBeenCalled();
    });

    confirmSpy.mockRestore();
  });
});
