import { useEffect, useState } from 'react';
import { api } from './api';
import { BrowseButton, Modal } from './ui';
import { ProgressBar } from './ProgressBar';
import type {
  LibraryRelocationPreview,
  LibraryRelocationReport,
  LibraryRelocationProgress,
} from './types';

export function LibraryRelocation({
  root,
  locked,
  lockReason,
  onBusy,
  onMigrated,
}: {
  root: string;
  locked: boolean;
  lockReason?: string;
  onBusy: (busy: boolean) => void;
  onMigrated: (report: LibraryRelocationReport) => void;
}) {
  const [oldRoot, setOldRoot] = useState(root);
  const [newRoot, setNewRoot] = useState('');
  const [preview, setPreview] = useState<LibraryRelocationPreview | null>(null);
  const [open, setOpen] = useState(false);
  const [progress, setProgress] = useState<LibraryRelocationProgress | null>(null);
  const [confirm, setConfirm] = useState(false);
  const [busy, setBusy] = useState(false);
  const [phase, setPhase] = useState<'choose' | 'preview' | 'apply' | null>(null);
  const [expanded, setExpanded] = useState(false);
  const [onlySkipped, setOnlySkipped] = useState(false);
  const [error, setError] = useState('');
  const [notice, setNotice] = useState('');
  useEffect(() => {
    setOldRoot(root);
    setPreview(null);
    setConfirm(false);
  }, [root]);
  async function run(action: () => Promise<void>, kind: 'choose' | 'preview' | 'apply' = 'choose') {
    if (busy || locked) return;
    setBusy(true);
    setPhase(kind);
    onBusy(true);
    setError('');
    setNotice('');
    try {
      await action();
    } catch (reason) {
      setError(String(reason));
      setPreview(null);
      setConfirm(false);
    } finally {
      setBusy(false);
      setPhase(null);
      onBusy(false);
    }
  }
  function closePreview() {
    if (busy) return;
    setOpen(false);
    setPreview(null);
    setProgress(null);
    setExpanded(false);
    setOnlySkipped(false);
    setError('');
  }
  const valid = preview?.items.filter((item) => !item.reason).length ?? 0;
  return (
    <section className="library-relocation" aria-label="批量迁移游戏库路径" aria-busy={busy}>
      <header>
        <h3>批量迁移游戏库路径</h3>
        <p className="muted">重新关联已有记录，文件保持原位。</p>
      </header>
      <fieldset className="library-relocation-fields" disabled={busy || locked}>
        <label>
          旧游戏库根目录
          <input
            value={oldRoot}
            onChange={(event) => {
              setOldRoot(event.target.value);
              setPreview(null);
            }}
          />
        </label>
        <label>
          新游戏库根目录
          <input value={newRoot} readOnly placeholder="选择已经复制好游戏的目录" />
        </label>
        <div className="library-relocation-actions" title={locked ? lockReason : undefined}>
          <BrowseButton
            onClick={() =>
              void run(async () => {
                const path = await api.chooseDirectory();
                if (path) {
                  setNewRoot(path);
                  setPreview(null);
                }
              })
            }
          >
            选择新根目录…
          </BrowseButton>
          <button
            type="button"
            className="primary"
            disabled={!oldRoot.trim() || !newRoot}
            onClick={() =>
              void run(async () => {
                setOpen(true);
                setPreview(null);
                setProgress(null);
                setPreview(await api.previewLibraryRelocation(oldRoot, newRoot, setProgress));
                setExpanded(false);
                setOnlySkipped(false);
              }, 'preview')
            }
          >
            {phase === 'preview' ? '正在预览…' : '预览迁移'}
          </button>
        </div>
      </fieldset>
      {open && (
        <Modal
          title="路径迁移预览"
          className={`modal-library-relocation${expanded && preview ? ' has-results' : ''}`}
          variant="record"
          showClose={!busy}
          onClose={closePreview}
          footer={
            <>
              <button type="button" disabled={busy} onClick={closePreview}>
                关闭并清除预览
              </button>
              {preview && (
                <button
                  type="button"
                  className="migration-confirm"
                  disabled={busy || locked || valid === 0}
                  onClick={() => setConfirm(true)}
                >
                  确认迁移
                </button>
              )}
            </>
          }
        >
          {phase === 'preview' && (
            <div className="library-relocation-progress" role="status">
              <ProgressBar label="迁移预览进度" value={progress?.processed} max={progress?.total} />
              <p>
                {progress ? `已检查 ${progress.processed} / ${progress.total} 项` : '准备检查…'}
              </p>
              <p className="path" title={progress?.current}>
                {progress?.current}
              </p>
            </div>
          )}
          {preview && (
            <div className="library-relocation-preview">
              <div className="library-relocation-root-map">
                <span className="path" title={preview.old_root}>
                  {preview.old_root}
                </span>
                <span aria-hidden="true">→</span>
                <span className="path" title={preview.new_root}>
                  {preview.new_root}
                </span>
              </div>
              <p className="library-relocation-counts" role="status">
                可迁移 {valid} 个游戏，无法迁移 {preview.items.length - valid} 个游戏。
              </p>
              {valid === 0 && (
                <p className="error" role="alert">
                  没有可迁移的游戏，请检查新旧根目录
                </p>
              )}
              <div className="library-relocation-tool">
                <strong>
                  公共 MTool ·{' '}
                  {preview.mtool.state === 'ready'
                    ? '可迁移'
                    : preview.mtool.state === 'blocked'
                      ? '无法迁移'
                      : '保持原样'}
                </strong>
                <p
                  className="path"
                  title={`${preview.mtool.old_path} → ${preview.mtool.new_path ?? ''}`}
                >
                  {preview.mtool.old_path || '未配置'}
                  {preview.mtool.new_path && ` → ${preview.mtool.new_path}`}
                </p>
              </div>
              <p className={preview.mtool.state === 'blocked' ? 'error' : 'muted'}>
                {preview.mtool.reason}
              </p>
              {valid > 0 && (
                <p className="muted">
                  将更新游戏库根目录{preview.mtool.state === 'ready' ? '及公共 MTool 根目录' : ''}
                  ；保留原启动配置与游戏资料。
                </p>
              )}
              <div className="library-relocation-details">
                <button
                  type="button"
                  className="library-relocation-toggle"
                  aria-expanded={expanded}
                  onClick={() => setExpanded(!expanded)}
                >
                  <span aria-hidden="true">{expanded ? '▾' : '▸'}</span> 查看逐项结果（
                  {preview.items.length}）
                </button>
                {expanded && (
                  <div className="library-relocation-result-panel">
                    <div className="library-relocation-actions">
                      <button
                        type="button"
                        aria-pressed={!onlySkipped}
                        onClick={() => setOnlySkipped(false)}
                      >
                        全部（{preview.items.length}）
                      </button>
                      <button
                        type="button"
                        aria-pressed={onlySkipped}
                        onClick={() => setOnlySkipped(true)}
                      >
                        无法迁移（{preview.items.length - valid}）
                      </button>
                    </div>
                    <ul className="library-relocation-results">
                      {preview.items
                        .filter((item) => !onlySkipped || item.reason)
                        .map((item) => (
                          <li key={item.id} className={item.reason ? 'is-skipped' : ''}>
                            <strong>
                              {item.title} · {item.reason ? '无法迁移' : '可迁移'}
                            </strong>
                            <p className="path" title={item.old_path}>
                              旧：{item.old_path}
                            </p>
                            {item.new_path && (
                              <p className="path" title={item.new_path}>
                                新：{item.new_path}
                              </p>
                            )}
                            {item.reason && <p className="error">{item.reason}</p>}
                          </li>
                        ))}
                    </ul>
                    {onlySkipped && valid === preview.items.length && (
                      <p className="muted">没有无法迁移的游戏。</p>
                    )}
                  </div>
                )}
              </div>
            </div>
          )}
          {error && (
            <p className="error" role="alert">
              {error}
            </p>
          )}
        </Modal>
      )}
      {notice && <p role="status">{notice}</p>}
      {confirm && preview && valid > 0 && (
        <Modal
          title="确认迁移游戏库路径？"
          variant="confirm"
          showClose={false}
          onClose={() => {
            if (!busy) setConfirm(false);
          }}
          footer={
            <>
              <button type="button" disabled={busy} onClick={() => setConfirm(false)}>
                取消
              </button>
              <button
                type="button"
                className="migration-confirm"
                disabled={busy || locked || valid === 0}
                onClick={() =>
                  void run(async () => {
                    const report = await api.applyLibraryRelocation(preview.token);
                    setConfirm(false);
                    setOpen(false);
                    setPreview(null);
                    onMigrated(report);
                    setNotice(
                      `已迁移 ${report.migrated} 个游戏，跳过 ${report.skipped} 个。${report.mtool_migrated ? '公共 MTool 已迁移。' : '公共 MTool 设置未变。'}`,
                    );
                  }, 'apply')
                }
              >
                {busy ? '正在迁移…' : '确认迁移'}
              </button>
            </>
          }
        >
          <p className="path">
            {preview.old_root} → {preview.new_root}
          </p>
          <p>
            迁移 {valid} 个游戏的路径关联，跳过 {preview.items.length - valid} 个；不移动任何文件。
          </p>
          <p>历史导入记录保持原样；依赖原电脑回收站的版本回退可能不可用。</p>
        </Modal>
      )}
    </section>
  );
}
