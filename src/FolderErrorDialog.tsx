import { useRef, useState } from 'react';
import { Icon, Modal } from './ui';
import type { Game } from './types';

export function FolderErrorDialog({
  game,
  reason,
  locked,
  onClose,
  onRemoved,
  onBusy,
}: {
  game: Game;
  reason: string;
  locked: boolean;
  onClose: () => void;
  onRemoved: () => Promise<void>;
  onBusy: (busy: boolean) => void;
}) {
  const [busy, setBusy] = useState(false);
  const removing = useRef(false);
  const [error, setError] = useState('');
  async function remove() {
    if (locked || removing.current) return;
    removing.current = true;
    setBusy(true);
    onBusy(true);
    setError('');
    try {
      await onRemoved();
    } catch (reason) {
      setError(String(reason));
    } finally {
      removing.current = false;
      setBusy(false);
      onBusy(false);
    }
  }
  return (
    <Modal
      title="无法打开游戏目录"
      variant="confirm"
      closeIconOnly
      onClose={() => {
        if (!removing.current) onClose();
      }}
    >
      <div className="folder-error-identity">
        <span className="section-icon">
          <Icon name="folder" />
        </span>
        <strong>{game.display_title}</strong>
      </div>
      <p>目录可能已移动、改名、删除，或暂时无法访问。若游戏仍在，可以返回详情关联新目录。</p>
      <p className="path folder-error-path">{game.install_path}</p>
      <details className="folder-error-details">
        <summary>错误详情</summary>
        <p className="path">{reason}</p>
      </details>
      <p className="folder-error-removal-note">
        从库中移除只清除这条游戏记录、关联资料和运行历史；游戏与存档文件保留。
      </p>
      {locked && <p className="muted">请返回详情保存或放弃编辑，并结束当前任务后再移除。</p>}
      {error && (
        <p className="error" role="alert">
          {error}
        </p>
      )}
      <div className="confirmation-actions folder-error-actions">
        <button type="button" disabled={busy} onClick={onClose}>
          返回详情
        </button>
        <button
          type="button"
          className="danger danger-solid"
          disabled={busy || locked}
          onClick={() => void remove()}
        >
          {busy ? '正在移除…' : '从库中移除'}
        </button>
      </div>
    </Modal>
  );
}
