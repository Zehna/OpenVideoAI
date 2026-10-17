import React, { useState } from 'react';
import type { InputType, PlannerProviderStatus, PlanningRequest, PlanningSource, Job } from '../types';
import { submitContentPlanJob } from '../api';
import { Button } from '../../../shared/components/Button';

interface NewPlanFormProps {
  projectId: string;
  providerStatus: PlannerProviderStatus | null;
  onJobCreated: (job: Job) => void;
  onCancel: () => void;
}

export const NewPlanForm: React.FC<NewPlanFormProps> = ({
  projectId,
  providerStatus,
  onJobCreated,
  onCancel,
}) => {
  const [inputType, setInputType] = useState<InputType>('topic');
  const [inputText, setInputText] = useState('');
  const [targetDurationSec, setTargetDurationSec] = useState(30);
  const [language, setLanguage] = useState('en');
  const [tone, setTone] = useState('informative');
  const [sources, setSources] = useState<PlanningSource[]>([]);
  const [isSubmitting, setIsSubmitting] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // Source form fields for adding a source
  const [sourceTitle, setSourceTitle] = useState('');
  const [sourceLocator, setSourceLocator] = useState('');
  const [sourceContent, setSourceContent] = useState('');
  const [showAddSource, setShowAddSource] = useState(false);

  const handleAddSource = () => {
    if (!sourceTitle.trim() || !sourceContent.trim()) return;
    const newSource: PlanningSource = {
      sourceId: 'src_' + Date.now().toString(36) + '_' + Math.random().toString(36).substring(2, 7),
      sourceType: 'document',
      title: sourceTitle.trim(),
      locator: sourceLocator.trim() || null,
      contentSnapshot: sourceContent.trim(),
      contentHash: 'client_hash',
    };
    setSources((prev) => [...prev, newSource]);
    setSourceTitle('');
    setSourceLocator('');
    setSourceContent('');
    setShowAddSource(false);
  };

  const handleRemoveSource = (id: string) => {
    setSources((prev) => prev.filter((s) => s.sourceId !== id));
  };

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!inputText.trim()) {
      setError('Please provide a topic or script.');
      return;
    }

    setIsSubmitting(true);
    setError(null);

    const request: PlanningRequest = {
      projectId,
      inputType,
      inputText: inputText.trim(),
      targetDurationMs: Math.max(1, targetDurationSec) * 1000,
      language: language.trim() || 'en',
      tone: tone.trim() || 'informative',
      sourceContext: sources,
      generationOptions: {},
    };

    try {
      const job = await submitContentPlanJob(projectId, request);
      onJobCreated(job);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setIsSubmitting(false);
    }
  };

  return (
    <form onSubmit={handleSubmit} className="max-w-3xl mx-auto space-y-6">
      <div className="flex items-center justify-between border-b border-gray-800 pb-3">
        <h2 className="text-lg font-bold text-gray-100 tracking-tight">Create AI Content Plan</h2>
        <Button variant="secondary" onClick={onCancel} className="text-xs px-3 py-1.5" type="button">
          Cancel
        </Button>
      </div>

      {/* Provider Status Alert */}
      {providerStatus && !providerStatus.configured && (
        <div className="p-3 rounded-lg bg-amber-950/60 border border-amber-800/70 text-amber-200 text-xs">
          <span className="font-semibold uppercase tracking-wider block mb-0.5">Notice: Gemini Not Configured</span>
          {providerStatus.message || 'GEMINI_API_KEY environment variable is not set. Generation will fail unless configured.'}
        </div>
      )}

      {error && (
        <div className="p-3 rounded-lg bg-rose-950/50 border border-rose-800/60 text-rose-300 text-xs">
          {error}
        </div>
      )}

      {/* Input Mode Selector */}
      <div className="space-y-2">
        <label className="block text-[11px] font-semibold text-gray-400 uppercase tracking-wider">
          Planning Mode
        </label>
        <div className="flex gap-2">
          <button
            type="button"
            onClick={() => setInputType('topic')}
            className={`px-4 py-2 rounded-lg text-xs font-semibold cursor-pointer transition-colors border ${
              inputType === 'topic'
                ? 'bg-indigo-600 text-white border-indigo-500'
                : 'bg-gray-900 text-gray-300 border-gray-800 hover:bg-gray-800'
            }`}
          >
            Topic / Idea
          </button>
          <button
            type="button"
            onClick={() => setInputType('script')}
            className={`px-4 py-2 rounded-lg text-xs font-semibold cursor-pointer transition-colors border ${
              inputType === 'script'
                ? 'bg-indigo-600 text-white border-indigo-500'
                : 'bg-gray-900 text-gray-300 border-gray-800 hover:bg-gray-800'
            }`}
          >
            Script
          </button>
        </div>
      </div>

      {/* Input Content */}
      <div className="space-y-1.5">
        <label className="block text-[11px] font-semibold text-gray-400 uppercase tracking-wider">
          {inputType === 'topic' ? 'Topic Description' : 'Full Script'}
        </label>
        <textarea
          value={inputText}
          onChange={(e) => setInputText(e.target.value)}
          placeholder={
            inputType === 'topic'
              ? 'Enter video topic or concept (e.g., A 60-second explanation of quantum computing)...'
              : 'Paste your full script or outline...'
          }
          rows={5}
          className="w-full px-3 py-2.5 bg-gray-900 border border-gray-700 rounded-lg text-gray-100 text-sm focus:outline-none focus:border-indigo-500 resize-none font-mono"
        />
      </div>

      {/* Settings Grid */}
      <div className="grid grid-cols-1 md:grid-cols-3 gap-4">
        <div>
          <label className="block text-[11px] font-semibold text-gray-400 uppercase tracking-wider mb-1">
            Target Duration (sec)
          </label>
          <input
            type="number"
            min={5}
            max={600}
            value={targetDurationSec}
            onChange={(e) => setTargetDurationSec(Number(e.target.value))}
            className="w-full px-3 py-2 bg-gray-900 border border-gray-700 rounded-lg text-gray-100 text-sm focus:outline-none focus:border-indigo-500"
          />
        </div>

        <div>
          <label className="block text-[11px] font-semibold text-gray-400 uppercase tracking-wider mb-1">
            Language
          </label>
          <input
            type="text"
            value={language}
            onChange={(e) => setLanguage(e.target.value)}
            className="w-full px-3 py-2 bg-gray-900 border border-gray-700 rounded-lg text-gray-100 text-sm focus:outline-none focus:border-indigo-500"
          />
        </div>

        <div>
          <label className="block text-[11px] font-semibold text-gray-400 uppercase tracking-wider mb-1">
            Tone / Style
          </label>
          <input
            type="text"
            value={tone}
            onChange={(e) => setTone(e.target.value)}
            className="w-full px-3 py-2 bg-gray-900 border border-gray-700 rounded-lg text-gray-100 text-sm focus:outline-none focus:border-indigo-500"
          />
        </div>
      </div>

      {/* Optional Sources Section */}
      <div className="space-y-3 pt-2">
        <div className="flex items-center justify-between">
          <label className="block text-[11px] font-semibold text-gray-400 uppercase tracking-wider">
            Grounding Sources ({sources.length})
          </label>
          <Button
            type="button"
            variant="secondary"
            className="text-xs px-2.5 py-1"
            onClick={() => setShowAddSource(!showAddSource)}
          >
            {showAddSource ? 'Cancel' : '+ Add Source Context'}
          </Button>
        </div>

        {showAddSource && (
          <div className="p-4 rounded-xl bg-gray-900 border border-gray-700 space-y-3">
            <div className="grid grid-cols-2 gap-3">
              <div>
                <label className="block text-[10px] text-gray-400 mb-1">Source Title *</label>
                <input
                  type="text"
                  value={sourceTitle}
                  onChange={(e) => setSourceTitle(e.target.value)}
                  placeholder="e.g. Research Paper or Article"
                  className="w-full px-3 py-1.5 bg-gray-800 border border-gray-700 rounded text-xs text-gray-100 focus:outline-none focus:border-indigo-500"
                />
              </div>
              <div>
                <label className="block text-[10px] text-gray-400 mb-1">Locator / Page (optional)</label>
                <input
                  type="text"
                  value={sourceLocator}
                  onChange={(e) => setSourceLocator(e.target.value)}
                  placeholder="e.g. p. 42 or Section 2"
                  className="w-full px-3 py-1.5 bg-gray-800 border border-gray-700 rounded text-xs text-gray-100 focus:outline-none focus:border-indigo-500"
                />
              </div>
            </div>
            <div>
              <label className="block text-[10px] text-gray-400 mb-1">Content Excerpt *</label>
              <textarea
                value={sourceContent}
                onChange={(e) => setSourceContent(e.target.value)}
                placeholder="Paste key factual excerpts..."
                rows={3}
                className="w-full px-3 py-1.5 bg-gray-800 border border-gray-700 rounded text-xs text-gray-100 focus:outline-none focus:border-indigo-500 font-mono"
              />
            </div>
            <Button
              type="button"
              className="text-xs px-3 py-1.5"
              onClick={handleAddSource}
              disabled={!sourceTitle.trim() || !sourceContent.trim()}
            >
              Add Source
            </Button>
          </div>
        )}

        {sources.length > 0 && (
          <div className="divide-y divide-gray-800 rounded-lg bg-gray-900 border border-gray-800 overflow-hidden">
            {sources.map((s) => (
              <div key={s.sourceId} className="p-2.5 flex items-center justify-between text-xs">
                <div>
                  <span className="font-semibold text-gray-200">{s.title}</span>
                  {s.locator && <span className="text-gray-500 ml-2 font-mono">({s.locator})</span>}
                  <p className="text-gray-400 text-[11px] truncate max-w-md mt-0.5">{s.contentSnapshot}</p>
                </div>
                <button
                  type="button"
                  onClick={() => handleRemoveSource(s.sourceId)}
                  className="text-rose-400 hover:text-rose-300 text-xs px-2 py-1 cursor-pointer"
                >
                  Remove
                </button>
              </div>
            ))}
          </div>
        )}
      </div>

      {/* Action Buttons */}
      <div className="flex items-center justify-end gap-3 pt-4 border-t border-gray-800">
        <Button variant="secondary" onClick={onCancel} type="button">
          Cancel
        </Button>
        <Button type="submit" disabled={isSubmitting || !inputText.trim()}>
          {isSubmitting ? 'Submitting Job...' : 'Generate Plan'}
        </Button>
      </div>
    </form>
  );
};
