import React from 'react';

export interface StatusCardProps {
  title: string;
  isReady: boolean;
  statusText: string;
  details?: string | null;
  warning?: string | null;
  testId?: string;
}

export const StatusCard: React.FC<StatusCardProps> = ({
  title,
  isReady,
  statusText,
  details,
  warning,
  testId,
}) => {
  return (
    <div
      data-testid={testId}
      className="p-5 rounded-xl bg-gray-800/80 border border-gray-700/80 shadow-sm flex flex-col gap-2"
    >
      <div className="flex items-center justify-between">
        <h3 className="text-base font-semibold text-gray-100">{title}</h3>
        <span
          className={`inline-flex items-center px-2.5 py-0.5 rounded-full text-xs font-semibold ${
            isReady
              ? 'bg-emerald-950/80 text-emerald-400 border border-emerald-700/80'
              : 'bg-rose-950/80 text-rose-400 border border-rose-700/80'
          }`}
        >
          {statusText}
        </span>
      </div>
      {details && <p className="text-xs text-gray-400 break-all font-mono">{details}</p>}
      {warning && (
        <div className="mt-2 p-2.5 rounded-lg bg-amber-950/40 border border-amber-700/50 text-amber-300 text-xs leading-relaxed">
          {warning}
        </div>
      )}
    </div>
  );
};
