import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { ContentPlanEditor } from '../components/ContentPlanEditor';
import * as plannerApi from '../api';
import type { ContentPlan, ContentPlanRevisionSummary } from '../types';

vi.mock('../api', () => ({
  getContentPlan: vi.fn(),
  saveContentPlanRevision: vi.fn(),
  getContentPlanHistory: vi.fn(),
  getContentPlanRevision: vi.fn(),
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
    vi.mocked(plannerApi.getContentPlan).mockResolvedValue(mockPlan);
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
});
