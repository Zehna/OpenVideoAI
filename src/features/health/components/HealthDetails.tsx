import React from 'react';
import type { HealthSnapshot } from '../types';

export interface HealthDetailsProps {
  snapshot: HealthSnapshot;
}

export const HealthDetails: React.FC<HealthDetailsProps> = ({ snapshot }) => {
  return (
    <div className="bg-gray-800/60 border border-gray-700/60 rounded-xl p-5 text-xs font-mono text-gray-300 space-y-2">
      <div className="text-gray-400 font-semibold uppercase tracking-wider text-[11px] mb-2">
        Diagnostic Details
      </div>
      <div className="flex flex-col gap-1.5">
        <div>
          <span className="text-gray-400">Application Version: </span>
          <span className="text-indigo-300 font-semibold">{snapshot.appVersion}</span>
        </div>
        <div>
          <span className="text-gray-400">SQLite Database Path: </span>
          <span className="text-gray-200 break-all">{snapshot.database.path}</span>
        </div>
        <div>
          <span className="text-gray-400">FFmpeg Binary Path: </span>
          <span className="text-gray-200 break-all">{snapshot.ffmpeg.path || 'Not detected'}</span>
        </div>
        <div>
          <span className="text-gray-400">FFprobe Binary Path: </span>
          <span className="text-gray-200 break-all">{snapshot.ffprobe.path || 'Not detected'}</span>
        </div>
      </div>
    </div>
  );
};
