import { useEffect, useState } from 'react';
import { api } from './api';
import type { Game, RenpyAppdataPreview } from './types';
import { Modal } from './ui';

export function RenpyAppdataCleanup({
  game,
  active,
  disabled,
  onBusy,
}: {
  game: Game;
  active: boolean;
  disabled: boolean;
  onBusy: (busy: boolean) => void;
}) {
  const [preview, setPreview] = useState<RenpyAppdataPreview | null>(null);
  const [confirmation, setConfirmation] = useState<RenpyAppdataPreview | null>(null);
  const [busy, setBusy] = useState(false);
  const [notice, setNotice] = useState('');
  const [error, setError] = useState('');
  useEffect(() => {
    if (!active) return;
    let stale = false;
    setPreview(null);
    void api.previewRenpyAppdata(game.id).then(
      (value) => {
        if (!stale) setPreview(value);
      },
      (reason) => {
        if (!stale) setError(String(reason));
      },
    );
    return () => {
      stale = true;
    };
  }, [active, game.id, game.install_path, game.working_directory]);

  async function act(recycle: boolean) {
    if (busy || disabled) return;
    setBusy(true);
    onBusy(true);
    setNotice('');
    setError('');
    try {
      if (recycle && confirmation?.token) {
        await api.recycleRenpyAppdata(game.id, confirmation.token);
        setConfirmation(null);
        setPreview(null);
        setNotice('AppData 存档已移入回收站。');
        try {
          setPreview(await api.previewRenpyAppdata(game.id));
        } catch (reason) {
          setError(`目录已回收，但状态刷新失败：${String(reason)}`);
        }
      } else {
        const current = await api.previewRenpyAppdata(game.id);
        setPreview(current);
        if (current.token && current.path) setConfirmation(current);
      }
    } catch (reason) {
      setConfirmation(null);
      setError(String(reason));
      // A consumed preview cannot be reused after failure; reacquire before a retry.
      setPreview(null);
      try {
        setPreview(await api.previewRenpyAppdata(game.id));
      } catch {
        /* Keep the operation error. */
      }
    } finally {
      setBusy(false);
      onBusy(false);
    }
  }

  return (
    <div className="full">
      <button
        type="button"
        disabled={disabled || busy || !preview?.token}
        title={preview?.reason ?? undefined}
        onClick={() => void act(false)}
      >
        清理 AppData 存档
      </button>
      {preview?.reason && <p className="muted">{preview.reason}</p>}
      {notice && <p role="status">{notice}</p>}
      {error && (
        <p className="error" role="alert">
          {error}
        </p>
      )}
      {confirmation && (
        <Modal
          title="清理 AppData 存档？"
          variant="confirm"
          showClose={false}
          onClose={() => {
            if (!busy) setConfirmation(null);
          }}
          footer={
            <>
              <button type="button" disabled={busy} onClick={() => setConfirmation(null)}>
                取消
              </button>
              <button
                type="button"
                className="danger danger-solid"
                disabled={busy}
                onClick={() => void act(true)}
              >
                {busy ? '正在移入回收站…' : '移入回收站'}
              </button>
            </>
          }
        >
          <p>将以下目录及其全部内容移入 Windows 回收站：</p>
          <p className="path">{confirmation.path}</p>
          <p>游戏目录中的存档不受影响。</p>
        </Modal>
      )}
    </div>
  );
}
