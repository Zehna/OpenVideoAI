import React, { useState } from 'react';
import { Button } from '../../../shared/components/Button';

interface ImportModalProps {
  isOpen: boolean;
  onClose: () => void;
  onImport: (sourcePath: string) => Promise<void>;
}

export const ImportModal: React.FC<ImportModalProps> = ({
  isOpen,
  onClose,
  onImport,
}) => {
  const [sourcePath, setSourcePath] = useState('');
  const [isSubmitting, setIsSubmitting] = useState(false);
  const [error, setError] = useState<string | null>(null);

  if (!isOpen) return null;

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    const trimmed = sourcePath.trim();
    if (!trimmed) {
      setError('Please provide a valid source file path');
      return;
    }

    setIsSubmitting(true);
    setError(null);
    try {
      await onImport(trimmed);
      setSourcePath('');
      onClose();
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setIsSubmitting(false);
    }
  };

  return (
    <div className="fixed inset-0 z-50 bg-black/70 flex items-center justify-center p-4 backdrop-blur-sm">
      <div className="bg-gray-900 border border-gray-800 rounded-xl max-w-lg w-full p-6 shadow-2xl space-y-4">
        <h2 className="text-xl font-bold text-gray-100">Import Media Asset</h2>
        <p className="text-xs text-gray-400">
          Specify the path to a local media file. It will be streamed, hashed (SHA-256), globally deduplicated, and linked to this project.
        </p>

        {error && (
          <div className="p-3 rounded-lg bg-rose-950/50 border border-rose-800/60 text-rose-300 text-xs break-all">
            {error}
          </div>
        )}

        <form onSubmit={handleSubmit} className="space-y-4">
          <div>
            <label className="block text-xs font-semibold text-gray-300 uppercase tracking-wider mb-1">
              File Path *
            </label>
            <input
              type="text"
              autoFocus
              value={sourcePath}
              onChange={(e) => setSourcePath(e.target.value)}
              placeholder="e.g. C:\Users\Media\video.mp4"
              className="w-full px-3 py-2 bg-gray-800 border border-gray-700 rounded-lg text-gray-100 text-xs font-mono focus:outline-none focus:border-indigo-500"
            />
            <p className="text-[11px] text-gray-500 mt-1">
              Supported: Video (MP4, MKV, MOV, WebM), Audio (MP3, WAV, AAC, FLAC), Image (PNG, JPG, WebP), Subtitles (SRT, VTT).
            </p>
          </div>

          <div className="flex justify-end gap-2 pt-2 border-t border-gray-800">
            <Button
              type="button"
              variant="secondary"
              onClick={onClose}
              disabled={isSubmitting}
            >
              Cancel
            </Button>
            <Button type="submit" isLoading={isSubmitting}>
              Import Asset
            </Button>
          </div>
        </form>
      </div>
    </div>
  );
};
