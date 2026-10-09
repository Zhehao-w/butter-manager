import { useEffect, useState } from 'react';
import { api } from './api';
import { Modal } from './ui';
import type { DeletePlan, DeleteReport, Game } from './types';

const actions = {
  missing: '路径不存在，不操作',
  recycle: '移入回收站',
};
export function DeleteGameDialog({
  game,
  onClose,
  onDeleted,
  onBusy,
}: {
  game: Game;
  onClose: () => void;
  onDeleted: (report: DeleteReport) => void;
  onBusy: (busy: boolean) => void;
}) {
  const [plan, setPlan] = useState<DeletePlan | null>(null);
  const [report, setReport] = useState<DeleteReport | null>(null);
  const [loading, setLoading] = useState(true);
  const [waiting, setWaiting] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');
  useEffect(() => {
    let stale = false;
    setPlan(null);
    setLoading(true);
    setWaiting(false);
    setError('');
    const timer = setTimeout(() => {
      if (!stale) setWaiting(true);
    }, 500);
    void api
      .previewDelete(game.id)
      .then((plan) => {
        if (!stale) setPlan(plan);
      })
      .catch((e) => {
        if (!stale) setError(String(e));
      })
      .finally(() => {
        clearTimeout(timer);
        if (!stale) setLoading(false);
      });
    return () => {
      stale = true;
      clearTimeout(timer);
    };
  }, [game.id]);
  const close = () => {
    if (!busy) {
      if (report) onDeleted(report);
      else onClose();
    }
  };
  async function apply() {
    if (!plan || busy || loading || plan.blockers.length) return;
    setBusy(true);
    onBusy(true);
    setError('');
    try {
      setReport(await api.deleteFiles(game.id, plan.token));
    } catch (e) {
      setError(String(e));
      setPlan(null);
    } finally {
      setBusy(false);
      onBusy(false);
    }
  }
  return (
    <Modal
      title={report ? '文件操作结果' : `移入回收站：${game.display_title}`}
      variant="confirm"
      className="modal-maintenance"
      showClose={false}
      onClose={close}
      footer={
        <div className="confirmation-actions">
          <button disabled={busy} onClick={close}>
            {report ? '完成' : '取消'}
          </button>
          {!report && (
            <button
              className="danger danger-solid maintenance-confirm"
              aria-busy={loading || busy}
              title={loading ? '正在检查目录与存档范围' : undefined}
              disabled={busy || loading || !plan || !!plan.blockers.length}
              onClick={() => void apply()}
            >
              <span
                className={`maintenance-spinner${waiting && loading ? ' active' : ''}`}
                aria-hidden="true"
              />
              <span className="maintenance-confirm-label">
                {busy ? '正在处理…' : '确认移入回收站'}
              </span>
            </button>
          )}
        </div>
      }
    >
      {!report ? (
        <>
          <p>游戏目录和关联存档将一同移入回收站，完成后移除库记录。</p>
          <p className="path delete-path">{plan?.game_path ?? game.install_path}</p>
          {plan && plan.saves.length > 0 && (
            <ul className="delete-save-list">
              {plan.saves.map((save) => (
                <li key={save.path}>
                  <strong>{actions[save.action]}</strong>
                  <span className="path">{save.path}</span>
                </li>
              ))}
            </ul>
          )}
          {!!plan?.blockers.length && (
            <p className="error" role="alert">
              {plan.blockers.join('；')}
            </p>
          )}
        </>
      ) : (
        <>
          <p role={report.error ? 'alert' : 'status'} className={report.error ? 'error' : ''}>
            {report.error ?? '游戏文件已送入回收站，库记录已移除。'}
          </p>
          {!!report.recycled.length && (
            <>
              <h3>已送入回收站</h3>
              <ul className="delete-save-list">
                {report.recycled.map((path) => (
                  <li className="path" key={path}>
                    {path}
                  </li>
                ))}
              </ul>
            </>
          )}
        </>
      )}
      {error && (
        <p className="error" role="alert">
          {error}
        </p>
      )}
    </Modal>
  );
}
