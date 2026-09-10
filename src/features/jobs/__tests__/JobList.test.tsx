import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { JobList } from '../components/JobList';
import * as jobsApi from '../api';
import type { Job, JobCheckpoint } from '../types';

vi.mock('../api', () => ({
  listJobs: vi.fn(),
  submitJob: vi.fn(),
  cancelJob: vi.fn(),
  getJobCheckpoints: vi.fn(),
}));

describe('JobList Component', () => {
  const mockJobs: Job[] = [
    {
      id: 'job-11111111',
      jobType: 'video_probe',
      status: 'running',
      priority: 80,
      idempotencyKey: 'idemp-1',
      projectId: 'proj-1',
      payload: JSON.stringify({ file: 'clip.mp4' }),
      result: null,
      error: null,
      retryCount: 0,
      maxRetries: 3,
      leasedBy: 'worker-node-1',
      leaseExpiresAt: '2026-09-10T12:00:00Z',
      leaseToken: 'token-abc-123',
      payloadVersion: 1,
      createdAt: '2026-09-10T00:00:00Z',
      updatedAt: '2026-09-10T00:01:00Z',
      startedAt: '2026-09-10T00:01:00Z',
      completedAt: null,
    },
    {
      id: 'job-22222222',
      jobType: 'audio_transcribe',
      status: 'completed',
      priority: 50,
      idempotencyKey: null,
      projectId: null,
      payload: JSON.stringify({ audio: 'track.wav' }),
      result: JSON.stringify({ text: 'Hello world' }),
      error: null,
      retryCount: 1,
      maxRetries: 3,
      leasedBy: null,
      leaseExpiresAt: null,
      leaseToken: null,
      payloadVersion: 1,
      createdAt: '2026-09-10T00:02:00Z',
      updatedAt: '2026-09-10T00:05:00Z',
      startedAt: '2026-09-10T00:03:00Z',
      completedAt: '2026-09-10T00:05:00Z',
    },
  ];

  const mockCheckpoints: JobCheckpoint[] = [
    {
      id: 'cp-1',
      jobId: 'job-11111111',
      stage: 'extract_stream',
      checkpointData: JSON.stringify({ progress: 50 }),
      createdAt: '2026-09-10T00:01:30Z',
    },
  ];

  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(jobsApi.listJobs).mockResolvedValue(mockJobs);
    vi.mocked(jobsApi.getJobCheckpoints).mockResolvedValue(mockCheckpoints);
  });

  it('renders list of jobs with badges and worker lease info', async () => {
    render(<JobList />);

    await waitFor(() => {
      expect(jobsApi.listJobs).toHaveBeenCalledWith(null, null);
    });

    expect(screen.getByText('video_probe')).toBeInTheDocument();
    expect(screen.getByText('running')).toBeInTheDocument();
    expect(screen.getByText('P80')).toBeInTheDocument();
    expect(screen.getByText(/worker-node-1/i)).toBeInTheDocument();

    expect(screen.getByText('audio_transcribe')).toBeInTheDocument();
    expect(screen.getByText('completed')).toBeInTheDocument();
    expect(screen.getByText('P50')).toBeInTheDocument();
    expect(screen.getByText(/Retries: 1\/3/i)).toBeInTheDocument();
  });

  it('renders empty state when there are no jobs', async () => {
    vi.mocked(jobsApi.listJobs).mockResolvedValueOnce([]);
    render(<JobList />);

    await waitFor(() => {
      expect(jobsApi.listJobs).toHaveBeenCalled();
    });

    expect(screen.getByText(/no jobs in queue/i)).toBeInTheDocument();
  });

  it('filters jobs when filter tab is clicked', async () => {
    render(<JobList />);

    await waitFor(() => {
      expect(jobsApi.listJobs).toHaveBeenCalledTimes(1);
    });

    const queuedTab = screen.getByRole('button', { name: 'Queued' });
    fireEvent.click(queuedTab);

    await waitFor(() => {
      expect(jobsApi.listJobs).toHaveBeenCalledWith(null, 'queued');
    });
  });

  it('submits a test job when clicking Submit Test Job button', async () => {
    const newTestJob: Job = {
      id: 'job-33333333',
      jobType: 'test_pipeline',
      status: 'queued',
      priority: 50,
      idempotencyKey: null,
      projectId: null,
      payload: '{"task":"video_probe_simulation"}',
      result: null,
      error: null,
      retryCount: 0,
      maxRetries: 3,
      leasedBy: null,
      leaseExpiresAt: null,
      leaseToken: null,
      payloadVersion: 1,
      createdAt: '2026-09-10T00:10:00Z',
      updatedAt: '2026-09-10T00:10:00Z',
      startedAt: null,
      completedAt: null,
    };
    vi.mocked(jobsApi.submitJob).mockResolvedValueOnce(newTestJob);

    render(<JobList />);

    await waitFor(() => {
      expect(jobsApi.listJobs).toHaveBeenCalled();
    });

    const submitBtn = screen.getByRole('button', { name: /\+ Submit Test Job/i });
    fireEvent.click(submitBtn);

    await waitFor(() => {
      expect(jobsApi.submitJob).toHaveBeenCalledWith(
        expect.objectContaining({
          jobType: 'test_pipeline',
          priority: 50,
        })
      );
    });

    expect(await screen.findByText(/job-3333/)).toBeInTheDocument();
  });

  it('cancels an active job when clicking Cancel button', async () => {
    const cancelledJob: Job = {
      ...mockJobs[0],
      status: 'cancelled',
    };
    vi.mocked(jobsApi.cancelJob).mockResolvedValueOnce(cancelledJob);

    render(<JobList />);

    await waitFor(() => {
      expect(jobsApi.listJobs).toHaveBeenCalled();
    });

    const cancelButtons = screen.getAllByRole('button', { name: 'Cancel' });
    fireEvent.click(cancelButtons[0]);

    await waitFor(() => {
      expect(jobsApi.cancelJob).toHaveBeenCalledWith('job-11111111');
    });
  });

  it('opens JobDetailModal when clicking on a job row', async () => {
    render(<JobList />);

    await waitFor(() => {
      expect(jobsApi.listJobs).toHaveBeenCalled();
    });

    const jobRow = screen.getByText('video_probe');
    fireEvent.click(jobRow);

    await waitFor(() => {
      expect(jobsApi.getJobCheckpoints).toHaveBeenCalledWith('job-11111111');
    });

    expect(screen.getByText('Execution Checkpoints')).toBeInTheDocument();
    expect(screen.getByText(/#1 extract_stream/i)).toBeInTheDocument();
  });
});
