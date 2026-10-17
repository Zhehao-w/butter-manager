import { useEffect, useState } from 'react';
import { api } from './api';
import type { ImportMemory } from './types';
import { formatTime } from './library';

export function ImportMemories({ gameId, disabled }: { gameId: string; disabled: boolean }) {
  const [open, setOpen] = useState(false);
  const [memories, setMemories] = useState<ImportMemory[]>([]);
  const [loading, setLoading] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');
  const [retry, setRetry] = useState(0);
  useEffect(() => {
    if (!open) return;
    let stale = false;
    setLoading(true);
    setError('');
    void api
      .importMemories(gameId)
      .then((values) => {
        if (!stale) setMemories(values);
      })
      .catch((reason) => {
        if (!stale) setError(String(reason));
      })
      .finally(() => {
        if (!stale) setLoading(false);
      });
    return () => {
      stale = true;
    };
  }, [gameId, open, retry]);
  async function forget(memory: ImportMemory) {
    setBusy(true);
    setError('');
    try {
      await api.forgetImportMemory(gameId, memory.id);
      setMemories((current) => current.filter((value) => value.id !== memory.id));
    } catch (reason) {
      setError(String(reason));
    } finally {
      setBusy(false);
    }
  }
  return (
    <details className="import-memories" onToggle={(event) => setOpen(event.currentTarget.open)}>
      <summary>导入关联记忆</summary>
      {open && (
        <div className="import-memories-body">
          <p className="muted">
            仅保存你明确记住的导入关联。忘记不改变游戏或别名；普通名称匹配仍可能推荐。
          </p>
          {loading ? (
            <p role="status">正在读取关联记忆…</p>
          ) : !memories.length && !error ? (
            <p className="muted">尚未记住任何来源。</p>
          ) : null}
          {error && (
            <div role="alert" className="import-memory-error">
              <span>{error}</span>
              <button
                type="button"
                disabled={disabled || busy || loading}
                onClick={() => setRetry((value) => value + 1)}
              >
                刷新
              </button>
            </div>
          )}
          {!loading &&
            memories.map((memory) => (
              <div className="import-memory-row" key={memory.id}>
                <div className="import-memory-source">
                  <strong title={memory.profile.title}>{memory.profile.title}</strong>
                  <span className="muted">
                    来源名称{memory.profile.identities.length ? '及静态项目标识' : ''} ·{' '}
                    {formatTime(memory.confirmed_at)}
                  </span>
                </div>
                <button
                  type="button"
                  disabled={disabled || busy}
                  aria-label={`忘记关联：${memory.profile.title}`}
                  onClick={() => void forget(memory)}
                >
                  忘记
                </button>
              </div>
            ))}
        </div>
      )}
    </details>
  );
}
