import React from 'react';
import type { GroundingStatus } from '../types';

interface GroundingBadgeProps {
  status: GroundingStatus;
  locator?: string | null;
  className?: string;
}

export const GroundingBadge: React.FC<GroundingBadgeProps> = ({
  status,
  locator,
  className = '',
}) => {
  switch (status) {
    case 'supported':
      return (
        <span
          title="Grounded in supplied sources"
          className={`inline-flex items-center gap-1 text-[10px] uppercase font-mono px-2 py-0.5 rounded border bg-emerald-950/80 text-emerald-400 border-emerald-800/60 ${className}`}
        >
          Supported {locator ? `(${locator})` : ''}
        </span>
      );
    case 'model_knowledge':
      return (
        <span
          title="AI knowledge — not grounded in supplied sources"
          className={`inline-flex items-center gap-1 text-[10px] uppercase font-mono px-2 py-0.5 rounded border bg-indigo-950/80 text-indigo-400 border-indigo-800/60 ${className}`}
        >
          AI Knowledge
        </span>
      );
    case 'uncertain':
      return (
        <span
          title="Uncertain citation — requires manual verification"
          className={`inline-flex items-center gap-1 text-[10px] uppercase font-mono px-2 py-0.5 rounded border bg-amber-950/80 text-amber-400 border-amber-800/60 ${className}`}
        >
          Needs review
        </span>
      );
    case 'modified':
      return (
        <span
          title="Modified after grounding — citation requires revalidation"
          className={`inline-flex items-center gap-1 text-[10px] uppercase font-mono px-2 py-0.5 rounded border bg-rose-950/80 text-rose-400 border-rose-800/60 ${className}`}
        >
          Modified
        </span>
      );
    default:
      return null;
  }
};
