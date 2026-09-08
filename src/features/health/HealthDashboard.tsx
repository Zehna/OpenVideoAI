import React, { useEffect, useState, useCallback } from 'react';
import { fetchHealthSnapshot } from './api';
import type { HealthSnapshot } from './types';
import { StatusCard } from './components/StatusCard';
import { HealthDetails } from './components/HealthDetails';
import { Button } from '../../shared/components/Button';

export const HealthDashboard: React.FC = () => {
  const [snapshot, setSnapshot] = useState<HealthSnapshot | null>(null);
  const [loading, setLoading] = useState<boolean>(true);
  const [isRefreshing, setIsRefreshing] = useState<boolean>(false);
  const [error, setError] = useState<string | null>(null);

  const loadHealth = useCallback(async (refresh = false) => {
    if (refresh) {
      setIsRefreshing(true);
    } else {
      setLoading(true);
    }
    setError(null);

    try {
      const data = await fetchHealthSnapshot();
      setSnapshot(data);
    } catch (err) {
      const message =
        err instanceof Error ? err.message : String(err);
      setError(message || 'Failed to communicate with Tauri backend');
    } finally {
      setLoading(false);
      setIsRefreshing(false);
    }
  }, []);

  useEffect(() => {
    void loadHealth();
  }, [loadHealth]);

  if (loading) {
    return (
      <div className="flex flex-col items-center justify-center min-h-[400px] p-6 text-gray-400">
        <div className="w-8 h-8 border-2 border-indigo-500 border-t-transparent rounded-full animate-spin mb-4" />
        <p className="text-sm font-medium">Loading system health...</p>
      </div>
    );
  }

  if (error) {
    return (
      <div className="max-w-4xl mx-auto p-6">
        <div className="p-4 rounded-xl bg-rose-950/40 border border-rose-800/60 text-rose-300 mb-6">
          <div className="flex items-center justify-between">
            <div>
              <h3 className="text-sm font-semibold mb-1">System Error</h3>
              <p className="text-xs">{error}</p>
            </div>
            <Button variant="danger" onClick={() => void loadHealth(false)}>
              Retry
            </Button>
          </div>
        </div>
      </div>
    );
  }

  if (!snapshot) {
    return null;
  }

  const hasMissingTools = !snapshot.ffmpeg.available || !snapshot.ffprobe.available;

  return (
    <div className="max-w-4xl mx-auto p-6 space-y-6">
      {/* Header */}
      <div className="flex items-center justify-between border-b border-gray-800 pb-4">
        <div>
          <h1 className="text-2xl font-bold text-gray-100 tracking-tight">
            System Health &amp; Environment
          </h1>
          <p className="text-sm text-gray-400 mt-1">
            Real-time status of local database and media toolchain
          </p>
        </div>
        <Button
          variant="secondary"
          onClick={() => void loadHealth(true)}
          isLoading={isRefreshing}
        >
          Refresh Status
        </Button>
      </div>

      {/* Non-blocking Warning Banner */}
      {hasMissingTools && (
        <div className="p-4 rounded-xl bg-amber-950/40 border border-amber-700/60 text-amber-300 text-xs leading-relaxed">
          <span className="font-semibold">Notice:</span> Video processing capabilities are limited
          because FFmpeg or FFprobe was not detected on this machine. You can continue using the application,
          but video rendering and audio extraction will require these tools.
        </div>
      )}

      {/* Status Cards Grid */}
      <div className="grid grid-cols-1 md:grid-cols-3 gap-4">
        <StatusCard
          title="Database"
          isReady={snapshot.database.ok}
          statusText={snapshot.database.ok ? 'Database Ready' : 'Database Error'}
          details={snapshot.database.path}
          warning={!snapshot.database.ok ? 'Database connection could not be established.' : null}
          testId="status-db"
        />

        <StatusCard
          title="FFmpeg"
          isReady={snapshot.ffmpeg.available}
          statusText={
            snapshot.ffmpeg.available
              ? `FFmpeg Detected (v${snapshot.ffmpeg.version ?? 'unknown'})`
              : 'FFmpeg Missing'
          }
          details={snapshot.ffmpeg.available ? snapshot.ffmpeg.path : null}
          warning={
            !snapshot.ffmpeg.available
              ? snapshot.ffmpeg.error ?? 'FFmpeg executable not found in PATH'
              : null
          }
          testId="status-ffmpeg"
        />

        <StatusCard
          title="FFprobe"
          isReady={snapshot.ffprobe.available}
          statusText={
            snapshot.ffprobe.available
              ? `FFprobe Detected (v${snapshot.ffprobe.version ?? 'unknown'})`
              : 'FFprobe Missing'
          }
          details={snapshot.ffprobe.available ? snapshot.ffprobe.path : null}
          warning={
            !snapshot.ffprobe.available
              ? snapshot.ffprobe.error ?? 'FFprobe executable not found in PATH'
              : null
          }
          testId="status-ffprobe"
        />
      </div>

      {/* Diagnostics */}
      <HealthDetails snapshot={snapshot} />
    </div>
  );
};
