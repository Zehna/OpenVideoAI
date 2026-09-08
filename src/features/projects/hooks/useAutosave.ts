import { useState, useEffect, useRef, useCallback } from 'react';

export interface UseAutosaveOptions<T> {
  value: T;
  onSave: (val: T) => Promise<void>;
  delay?: number;
}

export function useAutosave<T>({
  value,
  onSave,
  delay = 400,
}: UseAutosaveOptions<T>) {
  const [isSaving, setIsSaving] = useState(false);
  const [lastSaved, setLastSaved] = useState<T>(value);
  const [error, setError] = useState<string | null>(null);

  const onSaveRef = useRef(onSave);
  onSaveRef.current = onSave;

  const valueRef = useRef(value);
  valueRef.current = value;

  useEffect(() => {
    if (JSON.stringify(value) === JSON.stringify(lastSaved)) {
      return;
    }

    const timer = setTimeout(async () => {
      setIsSaving(true);
      setError(null);
      try {
        await onSaveRef.current(valueRef.current);
        setLastSaved(valueRef.current);
      } catch (err) {
        setError(err instanceof Error ? err.message : String(err));
      } finally {
        setIsSaving(false);
      }
    }, delay);

    return () => clearTimeout(timer);
  }, [value, lastSaved, delay]);

  const saveNow = useCallback(async () => {
    setIsSaving(true);
    setError(null);
    try {
      await onSaveRef.current(valueRef.current);
      setLastSaved(valueRef.current);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setIsSaving(false);
    }
  }, []);

  const isDirty = JSON.stringify(value) !== JSON.stringify(lastSaved);

  return { isSaving, isDirty, error, saveNow };
}
