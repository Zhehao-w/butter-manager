import { useEffect, useState } from 'react';
import { api } from './api';
import { QspConfiguration } from './QspConfiguration';
import { localQsp } from './qsp';
import { BrowseButton, Modal } from './ui';
import { DeleteGameDialog } from './DeleteGameDialog';
import { isAssociatedFile } from './library';
import type { Game, DeleteReport, LibraryPathCheck, RelocateGame } from './types';

export const pathStateText: Record<LibraryPathCheck['state'], string> = {
  available: '路径可访问',
  missing_directory: '目录缺失',
  missing_launch: '启动文件缺失',
  unconfigured: '未配置启动文件',
  unreadable: '路径需检查',
};
export function PathBadge({ check }: { check?: LibraryPathCheck }) {
  if (!check || check.state === 'available') return null;
  return (
    <span className={`path-status path-status-${check.state}`} title={check.message}>
      <span className="path-status-dot" aria-hidden="true" />
      <span className="path-status-label">{pathStateText[check.state]}</span>
    </span>
  );
}

export function RelocationEditor({
  game,
  path,
  onSaved,
  onCancel,
  onBusy,
  locked = false,
}: {
  game: Game;
  path: string;
  onSaved: (game: Game) => void;
  onCancel: () => void;
  onBusy?: (busy: boolean) => void;
  locked?: boolean;
}) {
  const [plan, setPlan] = useState<RelocateGame | null>(null);
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');
  useEffect(() => {
    let stale = false;
    setLoading(true);
    setPlan(null);
    setError('');
    void api
      .previewRelocation(game.id, path)
      .then((result) => {
        if (!stale) setPlan(result);
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
  }, [game.id, path]);
  async function run(action: () => Promise<void>) {
    setBusy(true);
    onBusy?.(true);
    setError('');
    try {
      await action();
    } catch (reason) {
      setError(String(reason));
    } finally {
      setBusy(false);
      onBusy?.(false);
    }
  }
  return (
    <section className="panel relocation-editor" aria-label="目录关联预览">
      <h3>关联现有游戏记录</h3>
      <p>保留游戏 ID、名称、别名、版本、引擎和运行历史。游戏文件保持原位。</p>
      <p className="path">原目录：{game.install_path}</p>
      <p className="path">新目录：{plan?.install_path || path}</p>
      <p className="muted">
        原游戏目录内的绝对存档路径会随新目录调整，外部路径与 &lt;GAME&gt;
        等占位路径保留。关联目录不会复制存档或更新版本。
      </p>
      {loading && <p role="status">正在检查新目录…</p>}
      {error && (
        <p className="error" role="alert">
          {error}
        </p>
      )}
      {plan && (
        <fieldset disabled={busy || locked}>
          {plan.launch_type === 'EXTERNAL_PLAYER' ? (
            <QspConfiguration
              root={plan.install_path}
              player={plan.main_executable}
              config={plan.external_player ?? localQsp()}
              disabled={busy || locked}
              onChange={(player, config) =>
                setPlan({ ...plan, main_executable: player, external_player: config })
              }
              onError={setError}
            />
          ) : (
            <label className="relocation-field">
              新目录启动文件
              <div className="path-input">
                <input
                  value={plan.main_executable || ''}
                  placeholder="可留待详情配置"
                  onChange={(event) => {
                    const file = event.target.value.trim() || null;
                    setPlan({
                      ...plan,
                      main_executable: file,
                      working_directory:
                        file?.replaceAll('\\', '/').split('/').slice(0, -1).join('/') || '.',
                      launch_type: isAssociatedFile(file) ? 'DIRECT' : plan.launch_type,
                      mtool_target_exe:
                        plan.launch_type === 'MTOOL' && file && !isAssociatedFile(file)
                          ? file
                          : plan.mtool_target_exe,
                    });
                  }}
                />
                <BrowseButton
                  type="button"
                  onClick={() =>
                    void run(async () => {
                      const file = await api.chooseLaunchFile(plan.install_path);
                      if (!file) return;
                      setPlan({
                        ...plan,
                        main_executable: file,
                        working_directory:
                          file.replaceAll('\\', '/').split('/').slice(0, -1).join('/') || '.',
                        launch_type: isAssociatedFile(file) ? 'DIRECT' : plan.launch_type,
                        mtool_target_exe:
                          plan.launch_type === 'MTOOL' && !isAssociatedFile(file)
                            ? file
                            : plan.mtool_target_exe,
                      });
                    })
                  }
                >
                  浏览启动文件…
                </BrowseButton>
              </div>
            </label>
          )}
          <label className="relocation-field">
            启动工作目录
            <input
              value={plan.working_directory}
              onChange={(event) => setPlan({ ...plan, working_directory: event.target.value })}
            />
          </label>
          {plan.launch_type === 'MTOOL' && (
            <label className="relocation-field">
              新目录 MTool target EXE
              <input
                value={plan.mtool_target_exe || ''}
                onChange={(event) =>
                  setPlan({ ...plan, mtool_target_exe: event.target.value || null })
                }
              />
            </label>
          )}
        </fieldset>
      )}
      <div className="footer-actions">
        <button type="button" disabled={busy} onClick={onCancel}>
          取消关联
        </button>
        <button
          type="button"
          className="primary"
          disabled={busy || locked || !plan || loading}
          onClick={() =>
            void run(async () => {
              if (plan) onSaved(await api.relocateGame(plan));
            })
          }
        >
          {busy ? '正在关联…' : '确认关联目录'}
        </button>
      </div>
    </section>
  );
}

export function GameMaintenance({
  game,
  locked,
  onRemoved,
  onDeleted,
  onRelocated,
  onBusy,
}: {
  game: Game;
  locked: boolean;
  onRemoved: () => Promise<void>;
  onDeleted: (report: DeleteReport) => void;
  onRelocated: (game: Game) => void;
  onBusy: (busy: boolean) => void;
}) {
  const [path, setPath] = useState<string | null>(null);
  const [confirmRemove, setConfirmRemove] = useState(false);
  const [confirmDelete, setConfirmDelete] = useState(false);
  const [renameName, setRenameName] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState('');
  async function run(action: () => Promise<void>) {
    setBusy(true);
    onBusy(true);
    setMessage('');
    try {
      await action();
    } catch (reason) {
      setMessage(String(reason));
    } finally {
      setBusy(false);
      onBusy(false);
    }
  }
  return (
    <section className="panel game-maintenance" aria-label="游戏记录维护">
      <h3>目录与库记录</h3>
      <p className="muted">
        可以重命名游戏目录并同步名称；目录已移动时，可以关联新目录保留原记录。
      </p>
      {message && !confirmRemove && renameName === null && <p role="status">{message}</p>}
      {locked && <p className="muted">请先保存或放弃资料编辑，并结束当前任务后再维护记录。</p>}
      {!path && (
        <div className="header-actions">
          <button
            type="button"
            disabled={locked || busy || confirmRemove || confirmDelete || renameName !== null}
            onClick={() => {
              setMessage('');
              setRenameName(
                game.install_path.replaceAll('\\', '/').split('/').pop() || game.display_title,
              );
            }}
          >
            重命名目录
          </button>
          <BrowseButton
            type="button"
            disabled={locked || busy || confirmRemove || confirmDelete || renameName !== null}
            onClick={() =>
              void run(async () => {
                const selected = await api.chooseDirectory();
                if (selected) setPath(selected);
              })
            }
          >
            关联新目录
          </BrowseButton>
          <button
            type="button"
            className="danger"
            disabled={locked || busy || confirmRemove || confirmDelete || renameName !== null}
            onClick={() => {
              setMessage('');
              setConfirmRemove(true);
            }}
          >
            从库中移除
          </button>
          <button
            type="button"
            className="danger danger-solid"
            disabled={locked || busy || confirmRemove || confirmDelete || renameName !== null}
            onClick={() => setConfirmDelete(true)}
          >
            删除游戏文件（回收站）
          </button>
        </div>
      )}
      {confirmDelete && (
        <DeleteGameDialog
          game={game}
          onClose={() => setConfirmDelete(false)}
          onBusy={onBusy}
          onDeleted={(report) => {
            setConfirmDelete(false);
            onDeleted(report);
          }}
        />
      )}
      {renameName !== null && (
        <Modal
          title="重命名游戏目录"
          variant="confirm"
          className="modal-rename"
          onClose={() => {
            if (!busy) setRenameName(null);
          }}
          footer={
            <div className="confirmation-actions">
              <button type="button" disabled={busy} onClick={() => setRenameName(null)}>
                取消
              </button>
              <button
                type="button"
                className="primary"
                disabled={busy || locked || !renameName.trim()}
                onClick={() =>
                  void run(async () => {
                    const saved = await api.renameGameDirectory(
                      game.id,
                      game.install_path,
                      renameName,
                    );
                    onRelocated(saved);
                    setRenameName(null);
                    setMessage('目录和游戏名称已更新，原有资料与记录保留。');
                  })
                }
              >
                {busy ? '正在重命名…' : '确认重命名'}
              </button>
            </div>
          }
        >
          <label className="relocation-field">
            新目录名称
            <input
              value={renameName}
              disabled={busy}
              onChange={(event) => setRenameName(event.target.value)}
            />
          </label>
          <p className="path">原目录：{game.install_path}</p>
          <p className="path">
            新目录：{game.install_path.replace(/[\\/][^\\/]+$/, '')}
            {game.install_path.includes('\\') ? '\\' : '/'}
            {renameName}
          </p>
          <p className="muted">
            同时修改显示名称和标准名称；新名称包含明确版本号时更新版本。游戏
            ID、别名、存档和游玩记录保留，内部绝对路径随目录调整。
            旧导入记录的回退可能因改名被阻止。
          </p>
          {message && (
            <p className="error" role="alert">
              {message}
            </p>
          )}
        </Modal>
      )}
      {path && (
        <RelocationEditor
          game={game}
          path={path}
          locked={locked}
          onBusy={onBusy}
          onCancel={() => setPath(null)}
          onSaved={(saved) => {
            onRelocated(saved);
            setPath(null);
            setMessage('已关联新目录，原有游戏资料保留。');
          }}
        />
      )}
      {confirmRemove && (
        <Modal
          title={`从库中移除：${game.display_title}`}
          variant="confirm"
          className="modal-maintenance"
          showClose={false}
          onClose={() => {
            if (!busy) setConfirmRemove(false);
          }}
          footer={
            <div className="confirmation-actions">
              <button disabled={busy} onClick={() => setConfirmRemove(false)}>
                取消移除
              </button>
              <button
                className="danger danger-solid"
                disabled={busy || locked}
                onClick={() => void run(onRemoved)}
              >
                {busy ? '正在移除…' : '确认移除记录'}
              </button>
            </div>
          }
        >
          <p>仅移除库记录、别名、存档路径记录和运行历史；游戏与存档文件保留。</p>
          <p className="path delete-path">{game.install_path}</p>
          {message && (
            <p className="error" role="alert">
              {message}
            </p>
          )}
        </Modal>
      )}
    </section>
  );
}
