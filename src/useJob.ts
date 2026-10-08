import { useEffect, useRef, useState, useCallback } from 'react';
import { api } from './api';
import type { JobPage, ScanCandidate } from './types';
export const activeJob = (job: JobPage | null) =>
  !!job && ['running', 'cancel_requested'].includes(job.status);
export function useJob(
  id: string | null,
  onChanges: (changes: ScanCandidate[]) => void,
  onFinished: (job: JobPage) => void,
  onError: (error: string) => void,
) {
  const [page, setPage] = useState<JobPage | null>(null);
  const cursor = useRef(0);
  const epoch = useRef(0);
  const cancelAcknowledged = useRef(false);
  const callbacks = useRef({ onChanges, onFinished, onError });
  callbacks.current = { onChanges, onFinished, onError };
  const consume = useCallback((result: JobPage) => {
    cursor.current = result.next_cursor;
    setPage(
      cancelAcknowledged.current && result.status === 'running'
        ? { ...result, status: 'cancel_requested' }
        : result,
    );
    if (result.changes.length) callbacks.current.onChanges(result.changes);
  }, []);
  const markCancelling = useCallback(() => {
    cancelAcknowledged.current = true;
    setPage((current) =>
      current && activeJob(current) ? { ...current, status: 'cancel_requested' } : current,
    );
  }, []);
  const refresh = useCallback(async () => {
    if (!id) return;
    const generation = epoch.current;
    let result: JobPage;
    do {
      result = await api.job(id, cursor.current);
      if (epoch.current !== generation) return;
      consume(result);
    } while (result.next_cursor < result.change_count);
  }, [id, consume]);
  useEffect(() => {
    const generation = ++epoch.current;
    cursor.current = 0;
    cancelAcknowledged.current = false;
    setPage(null);
    if (!id) return;
    let timer: ReturnType<typeof setTimeout>;
    async function poll() {
      try {
        const result = await api.job(id!, cursor.current);
        if (epoch.current !== generation) return;
        consume(result);
        if (result.next_cursor < result.change_count || activeJob(result))
          timer = setTimeout(() => void poll(), result.next_cursor < result.change_count ? 0 : 400);
        else callbacks.current.onFinished(result);
      } catch (error) {
        if (epoch.current !== generation) return;
        callbacks.current.onError(String(error));
        timer = setTimeout(() => void poll(), 1500);
      }
    }
    void poll();
    return () => {
      epoch.current++;
      clearTimeout(timer);
    };
  }, [id, consume]);
  return { page, refresh, markCancelling };
}
