import React from 'react';
import type { ContentScene, MediaIntent, Claim } from '../types';
import { GroundingBadge } from './GroundingBadge';

interface SceneEditorProps {
  scene: ContentScene;
  onChange: (updated: ContentScene) => void;
  isReadOnly: boolean;
}

const ALL_MEDIA_INTENTS: { value: MediaIntent; label: string }[] = [
  { value: 'stock_video', label: 'Stock Video' },
  { value: 'stock_image', label: 'Stock Image' },
  { value: 'ai_image', label: 'AI Image' },
  { value: 'ai_video', label: 'AI Video' },
  { value: 'screen_recording', label: 'Screen Recording' },
  { value: 'text_graphic', label: 'Text Graphic' },
];

export const SceneEditor: React.FC<SceneEditorProps> = ({
  scene,
  onChange,
  isReadOnly,
}) => {
  const handleTitleChange = (val: string) => {
    onChange({ ...scene, title: val });
  };

  const handleNarrationChange = (val: string) => {
    onChange({ ...scene, narration: val });
  };

  const handleDurationChange = (sec: number) => {
    onChange({ ...scene, durationEstimateMs: Math.max(1, sec) * 1000 });
  };

  const handleVisualChange = (val: string) => {
    onChange({ ...scene, visualDescription: val });
  };

  const handleToggleIntent = (intent: MediaIntent) => {
    if (isReadOnly) return;
    const exists = scene.mediaIntent.includes(intent);
    const updated = exists
      ? scene.mediaIntent.filter((i) => i !== intent)
      : [...scene.mediaIntent, intent];
    onChange({ ...scene, mediaIntent: updated });
  };

  const handleQueriesChange = (text: string) => {
    const queries = text
      .split(',')
      .map((q) => q.trim())
      .filter(Boolean);
    onChange({ ...scene, stockSearchQueries: queries });
  };

  const handleClaimTextChange = (claimId: string, newText: string) => {
    const updatedClaims: Claim[] = scene.claims.map((c) => {
      if (c.id === claimId) {
        return {
          ...c,
          text: newText,
          groundingStatus: 'modified',
          groundingValid: false,
        };
      }
      return c;
    });
    onChange({ ...scene, claims: updatedClaims });
  };

  return (
    <div className="space-y-6 bg-gray-900 border border-gray-800 rounded-xl p-6">
      {/* Scene Header */}
      <div className="grid grid-cols-1 md:grid-cols-3 gap-4 border-b border-gray-800 pb-4">
        <div className="md:col-span-2">
          <label className="block text-[11px] font-semibold text-gray-400 uppercase tracking-wider mb-1">
            Scene Title
          </label>
          <input
            type="text"
            disabled={isReadOnly}
            value={scene.title}
            onChange={(e) => handleTitleChange(e.target.value)}
            className="w-full px-3 py-1.5 bg-gray-800 border border-gray-700 rounded-lg text-gray-100 font-semibold focus:outline-none focus:border-indigo-500 disabled:opacity-60"
          />
        </div>
        <div>
          <label className="block text-[11px] font-semibold text-gray-400 uppercase tracking-wider mb-1">
            Duration (sec)
          </label>
          <input
            type="number"
            disabled={isReadOnly}
            min={1}
            value={Math.round(scene.durationEstimateMs / 1000)}
            onChange={(e) => handleDurationChange(Number(e.target.value))}
            className="w-full px-3 py-1.5 bg-gray-800 border border-gray-700 rounded-lg text-gray-100 font-mono text-sm focus:outline-none focus:border-indigo-500 disabled:opacity-60"
          />
        </div>
      </div>

      {/* Narration */}
      <div className="space-y-1.5">
        <label className="block text-[11px] font-semibold text-gray-400 uppercase tracking-wider">
          Voiceover Narration
        </label>
        <textarea
          disabled={isReadOnly}
          value={scene.narration}
          onChange={(e) => handleNarrationChange(e.target.value)}
          rows={3}
          className="w-full px-3 py-2 bg-gray-800 border border-gray-700 rounded-lg text-gray-100 text-sm focus:outline-none focus:border-indigo-500 resize-none font-mono disabled:opacity-60"
        />
      </div>

      {/* Visual Description */}
      <div className="space-y-1.5">
        <label className="block text-[11px] font-semibold text-gray-400 uppercase tracking-wider">
          Visual Description / Staging
        </label>
        <textarea
          disabled={isReadOnly}
          value={scene.visualDescription}
          onChange={(e) => handleVisualChange(e.target.value)}
          rows={2}
          className="w-full px-3 py-2 bg-gray-800 border border-gray-700 rounded-lg text-gray-200 text-xs focus:outline-none focus:border-indigo-500 resize-none disabled:opacity-60"
        />
      </div>

      {/* Media Intent Toggles */}
      <div className="space-y-2">
        <label className="block text-[11px] font-semibold text-gray-400 uppercase tracking-wider">
          Media Intent
        </label>
        <div className="flex flex-wrap gap-2">
          {ALL_MEDIA_INTENTS.map((intent) => {
            const isSelected = scene.mediaIntent.includes(intent.value);
            return (
              <button
                key={intent.value}
                type="button"
                disabled={isReadOnly}
                onClick={() => handleToggleIntent(intent.value)}
                className={`text-xs px-2.5 py-1 rounded-md border font-medium cursor-pointer transition-colors ${
                  isSelected
                    ? 'bg-indigo-600 text-white border-indigo-500'
                    : 'bg-gray-800 text-gray-300 border-gray-700 hover:bg-gray-750'
                } disabled:opacity-50 disabled:cursor-not-allowed`}
              >
                {intent.label}
              </button>
            );
          })}
        </div>
      </div>

      {/* Stock Search Queries */}
      <div className="space-y-1.5">
        <label className="block text-[11px] font-semibold text-gray-400 uppercase tracking-wider">
          Stock Search Queries (comma-separated)
        </label>
        <input
          type="text"
          disabled={isReadOnly}
          value={scene.stockSearchQueries.join(', ')}
          onChange={(e) => handleQueriesChange(e.target.value)}
          placeholder="e.g. tokamak interior, glowing plasma"
          className="w-full px-3 py-1.5 bg-gray-800 border border-gray-700 rounded-lg text-xs text-gray-200 focus:outline-none focus:border-indigo-500 disabled:opacity-60"
        />
      </div>

      {/* Prompts (Optional Hints) */}
      <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
        <div>
          <label className="block text-[11px] font-semibold text-gray-400 uppercase tracking-wider mb-1">
            Image Prompt Hint (optional)
          </label>
          <input
            type="text"
            disabled={isReadOnly}
            value={scene.imagePrompt || ''}
            onChange={(e) => onChange({ ...scene, imagePrompt: e.target.value || null })}
            placeholder="AI image prompt suggestion..."
            className="w-full px-3 py-1.5 bg-gray-800 border border-gray-700 rounded-lg text-xs text-gray-200 focus:outline-none focus:border-indigo-500 disabled:opacity-60"
          />
        </div>
        <div>
          <label className="block text-[11px] font-semibold text-gray-400 uppercase tracking-wider mb-1">
            Video Prompt Hint (optional)
          </label>
          <input
            type="text"
            disabled={isReadOnly}
            value={scene.videoPrompt || ''}
            onChange={(e) => onChange({ ...scene, videoPrompt: e.target.value || null })}
            placeholder="AI video prompt suggestion..."
            className="w-full px-3 py-1.5 bg-gray-800 border border-gray-700 rounded-lg text-xs text-gray-200 focus:outline-none focus:border-indigo-500 disabled:opacity-60"
          />
        </div>
      </div>

      {/* Factual Claims & Grounding */}
      <div className="space-y-3 pt-2 border-t border-gray-800">
        <label className="block text-[11px] font-semibold text-gray-400 uppercase tracking-wider">
          Factual Claims & Citations ({scene.claims.length})
        </label>
        {scene.claims.length === 0 ? (
          <p className="text-xs text-gray-500 italic">No individual claims identified for this scene.</p>
        ) : (
          <div className="space-y-2">
            {scene.claims.map((claim) => (
              <div
                key={claim.id}
                className="p-3 rounded-lg bg-gray-800/80 border border-gray-700/80 space-y-2"
              >
                <div className="flex items-center justify-between gap-3">
                  <input
                    type="text"
                    disabled={isReadOnly}
                    value={claim.text}
                    onChange={(e) => handleClaimTextChange(claim.id, e.target.value)}
                    className="flex-1 px-2.5 py-1 bg-gray-900 border border-gray-700 rounded text-xs text-gray-100 focus:outline-none focus:border-indigo-500 disabled:opacity-60 font-medium"
                  />
                  <GroundingBadge
                    status={claim.groundingStatus}
                    locator={claim.sourceRefs[0]?.locator}
                  />
                </div>
                {claim.note && (
                  <p className="text-[11px] text-gray-400 font-sans italic">{claim.note}</p>
                )}
              </div>
            ))}
          </div>
        )}
      </div>
    </div>
  );
};
