import { useEffect, useState } from 'react';
import { api } from './api';
import { Modal } from './ui';
import { ProgressBar } from './ProgressBar';
import { displayVersion } from './library';
import type { ImportDuplicatePlan } from './types';

export function sameImportVersion(incoming: string, installed: string) {
  const key = (value: string) =>
    value
      .normalize('NFKC')
      .trim()
      .toLowerCase()
      .replace(/^(?:version|ver|v)[. _-]*(?=\d)/, '');
  const version = key(incoming);
  return !['', '-', 'unknown', '未知', '未识别'].includes(version) && version === key(installed);
}

export function ImportDuplicateDialog({
  scanId,
  source,
  existingId,
  version,
  onClose,
  onRecycled,
  onBusy,
}: {
  scanId: string;
  source: string;
  existingId: string;
  version: string;
  onClose: () => void;
  onRecycled: () => void;
  onBusy: (busy: boolean) => void;
}) {
  const [plan, setPlan] = useState<ImportDuplicatePlan | null>(null);
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');
  useEffect(() => {
    let stale = false;
    void api
      .previewImportDuplicate(scanId, source, existingId, version)
      .then((value) => {
        if (!stale) setPlan(value);
      })
      .catch((error) => {
        if (!stale) setError(String(error));
      })
      .finally(() => {
        if (!stale) setLoading(false);
      });
    return () => {
      stale = true;
    };
  }, [scanId, source, existingId, version]);
  async function apply() {
    if (!plan || busy || loading || plan.blockers.length) return;
    setBusy(true);
    onBusy(true);
    setError('');
    try {
      await api.recycleImportDuplicate(plan.token);
      onRecycled();
    } catch (error) {
      setError(String(error));
      setPlan(null);
    } finally {
      setBusy(false);
      onBusy(false);
    }
  }
  return (
    <Modal
      title="删除导入副本"
      variant="confirm"
      showClose={false}
      onClose={() => {
        if (!busy) onClose();
      }}
    >
      <p>
        版本号相同（{displayVersion(version)}
        ），文件内容可能不同。确认将下面的导入目录及其中的存档移入回收站。
      </p>
      <p className="path delete-path">{plan?.game_path ?? source}</p>
      {plan && (
        <p className="muted">
          库中保留：{plan.existing_title}
          <br />
          <span className="path">{plan.existing_path}</span>
        </p>
      )}
      <p className="muted">库中游戏及其存档不操作，删除后从本次导入列表移除。</p>
      {loading && <p role="status">正在检查删除范围…</p>}
      {busy && (
        <div className="import-progress" role="status">
          <span>正在移入回收站…</span>
          <ProgressBar label="删除导入副本进度" />
        </div>
      )}
      {(error || !!plan?.blockers.length) && (
        <p className="error" role="alert">
          {error || plan!.blockers.join('；')}
        </p>
      )}
      <div className="confirmation-actions">
        <button disabled={busy} onClick={onClose}>
          取消
        </button>
        <button
          className="danger danger-solid"
          disabled={loading || busy || !plan || !!plan.blockers.length}
          onClick={() => void apply()}
        >
          {busy ? '正在处理…' : '确认移入回收站'}
        </button>
      </div>
    </Modal>
  );
}
