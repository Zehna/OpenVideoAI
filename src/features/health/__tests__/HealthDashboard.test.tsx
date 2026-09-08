import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, waitFor, fireEvent } from '@testing-library/react';
import { HealthDashboard } from '../HealthDashboard';
import * as api from '../api';
import type { HealthSnapshot } from '../types';

vi.mock('../api', () => ({
  fetchHealthSnapshot: vi.fn(),
}));

describe('HealthDashboard', () => {
  const healthyMock: HealthSnapshot = {
    appVersion: '0.1.0',
    database: {
      ok: true,
      path: 'C:\\Users\\Ermi\\AppData\\Roaming\\OpenVideoAI\\openvideoai.db',
    },
    ffmpeg: {
      available: true,
      version: '9.0.1',
      path: 'C:\\bin\\ffmpeg.exe',
      error: null,
    },
    ffprobe: {
      available: true,
      version: '9.0.1',
      path: 'C:\\bin\\ffprobe.exe',
      error: null,
    },
  };

  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('renders healthy snapshot with Database Ready, FFmpeg detected, and FFprobe detected', async () => {
    vi.mocked(api.fetchHealthSnapshot).mockResolvedValueOnce(healthyMock);

    render(<HealthDashboard />);

    expect(screen.getByText(/loading system health/i)).toBeInTheDocument();

    await waitFor(() => {
      expect(screen.getByText(/database ready/i)).toBeInTheDocument();
      expect(screen.getByText(/ffmpeg detected \(v9\.0\.1\)/i)).toBeInTheDocument();
      expect(screen.getByText(/ffprobe detected \(v9\.0\.1\)/i)).toBeInTheDocument();
    });

    expect(screen.getByText('0.1.0')).toBeInTheDocument();
  });

  it('renders missing FFmpeg gracefully with clear warning and no crash', async () => {
    const missingFfmpegMock: HealthSnapshot = {
      ...healthyMock,
      ffmpeg: {
        available: false,
        version: null,
        path: null,
        error: 'Executable "ffmpeg" not found in PATH or standard directories',
      },
    };

    vi.mocked(api.fetchHealthSnapshot).mockResolvedValueOnce(missingFfmpegMock);

    render(<HealthDashboard />);

    await waitFor(() => {
      expect(screen.getByText(/ffmpeg missing/i)).toBeInTheDocument();
    });

    expect(
      screen.getByText(/executable "ffmpeg" not found in path/i)
    ).toBeInTheDocument();
    expect(
      screen.getByText(/video processing capabilities are limited/i)
    ).toBeInTheDocument();
  });

  it('clicking "Refresh Status" triggers get_health_snapshot again and updates UI', async () => {
    vi.mocked(api.fetchHealthSnapshot)
      .mockResolvedValueOnce(healthyMock)
      .mockResolvedValueOnce({
        ...healthyMock,
        ffmpeg: {
          available: false,
          version: null,
          path: null,
          error: 'ffmpeg removed',
        },
      });

    render(<HealthDashboard />);

    await waitFor(() => {
      expect(screen.getByText(/ffmpeg detected/i)).toBeInTheDocument();
    });

    const refreshButton = screen.getByRole('button', { name: /refresh status/i });
    fireEvent.click(refreshButton);

    await waitFor(() => {
      expect(screen.getByText(/ffmpeg missing/i)).toBeInTheDocument();
    });

    expect(api.fetchHealthSnapshot).toHaveBeenCalledTimes(2);
  });

  it('handles IPC error gracefully by displaying an alert banner', async () => {
    vi.mocked(api.fetchHealthSnapshot).mockRejectedValueOnce(
      new Error('Failed to communicate with Tauri backend')
    );

    render(<HealthDashboard />);

    await waitFor(() => {
      expect(
        screen.getByText(/failed to communicate with tauri backend/i)
      ).toBeInTheDocument();
    });
  });
});
