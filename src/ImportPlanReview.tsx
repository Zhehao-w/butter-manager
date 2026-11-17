import { MtoolCleanupStatus } from './ImportHistory';
import { displayVersion } from './library';
import type { ImportPlan } from './types';
import { Modal } from './ui';

const stateNames: Record<string, string> = {
  pending: '待移动',
  moving: '移动中',
  copying: '复制中',
  staged: '已暂存',
  publishing: '文件就位',
  registering: '登记中',
  cleanup: '已登记 · 清理来源',
  removing_source: '清理来源',
  finishing: '完成中',
  completed: '已完成',
  withdrawn: '已撤回',
  update_snapshot: '存档快照',
  update_copy: '准备新版本',
  update_ready: '待切换',
  update_isolate: '切换版本',
  update_publish: '安装新版本',
  update_restore: '恢复存档',
  update_commit: '提交更新',
  update_cleanup: '已更新 · 清理来源',
  update_recycle: '旧版本移入回收站',
  update_finish: '清理临时文件',
  update_identity: '更新名称与目录',
  rollback_retrieve: '从回收站恢复',
  rollback_cleanup: '清理临时文件',
  rollback_identity: '恢复原名称与目录',
  update_returning: '恢复旧版本',
  update_returned: '旧版本已恢复',
  rollback_snapshot: '保存当前存档',
  rollback_copy: '准备旧版本',
  rollback_restore: '恢复当前存档',
  rollback_isolate: '回退中',
  rollback_publish: '切换旧版本',
  rollback_commit: '提交回退',
  rolled_back: '已回退',
};

export function ImportPlanReview({
  plan,
  disabled,
  busy,
  error,
  onClose,
  onEdit,
  onApply,
  onWithdraw,
}: {
  plan: ImportPlan;
  disabled: boolean;
  busy: boolean;
  error: string;
  onClose: () => void;
  onEdit: () => void;
  onApply: () => void;
  onWithdraw: () => void;
}) {
  const preview = plan.status === 'preview';
  const canEdit = plan.items.every((item) => item.state === 'pending') && !disabled;
  const canApply = plan.items.length > 0 && plan.items.every((item) => !item.blockers.length);
  const canWithdraw =
    plan.items.some((item) => item.state !== 'pending') &&
    !plan.items.some((item) => item.state.startsWith('rollback_'));
  const close = () => {
    if (busy) return;
    if (canEdit) onEdit();
    else onClose();
  };
  return (
    <Modal
      variant="confirm"
      className="modal-import-plan"
      showClose={false}
      title={preview ? '确认导入与更新' : '恢复未完成导入'}
      headerActions={<span className="mode">{plan.items.length} 项</span>}
      onClose={close}
      footer={
        <div className="confirmation-actions">
          <button disabled={busy} onClick={close}>
            {canEdit ? '返回调整' : '关闭'}
          </button>
          {canWithdraw && (
            <button className="warning" disabled={disabled} onClick={onWithdraw}>
              撤回未完成项
            </button>
          )}
          {canApply && (
            <button className="primary" disabled={disabled} onClick={onApply}>
              {preview ? '确认并执行' : '继续未完成项'}
            </button>
          )}
        </div>
      }
    >
      <p className="import-review-intro">
        确认后移动或复制游戏目录。更新时按所选方式处理存档，旧版本移入回收站。
      </p>
      <div className="import-review-scroll" role="region" aria-label="导入项目列表" tabIndex={0}>
        {error && (
          <p role="alert" className="error">
            {error}
          </p>
        )}
        <ul className="import-review-items" aria-label="导入计划游戏">
          {plan.items.map((item) => {
            const { selection, update } = item;
            const updating = !!update || !!selection.existing_id;
            const version = displayVersion(selection.version);
            const oldVersion = update ? displayVersion(update.old_version) : version;
            const launchArguments = selection.launch_arguments ?? update?.old?.launch_arguments;
            const executable = selection.executable.replaceAll('\\', '/').toLowerCase();
            const recyclesBatch =
              !selection.mtool &&
              executable.endsWith('.bat') &&
              item.mtool_cleanup?.targets.some(({ path }) => {
                const target = path.replaceAll('\\', '/').toLowerCase();
                return executable === target || executable.startsWith(`${target}/`);
              });
            return (
              <li key={selection.source} className="panel">
                <div className="import-review-heading">
                  <strong>{selection.title}</strong>
                  <span className="mode">{updating ? '更新' : '新增'}</span>
                </div>
                <p className="import-review-meta muted">
                  {updating
                    ? `${oldVersion === version ? version : `${oldVersion} → ${version}`} · ${selection.preserve_saves === false ? '使用新包存档' : '保留旧存档'}`
                    : item.cross_volume
                      ? '跨盘复制后移除来源'
                      : '移动目录'}
                  {!preview && ` · ${stateNames[item.state] || item.state}`}
                </p>
                {preview && updating && !update ? (
                  <p className="path">目标文件夹：{selection.target_name}</p>
                ) : (
                  <p className="path">目标：{item.target}</p>
                )}
                {!selection.mtool &&
                  !selection.external_player &&
                  /\.exe$/i.test(selection.executable) &&
                  launchArguments?.trim() && <p className="path">启动参数：{launchArguments}</p>}
                {item.blockers.map((blocker, index) => (
                  <p className="error" role="alert" key={index}>
                    {blocker}
                  </p>
                ))}
                {item.error && (
                  <p className="error" role="alert">
                    {item.error}
                  </p>
                )}
                {item.mtool_cleanup &&
                  (preview ? (
                    recyclesBatch ? (
                      <p className="notice">
                        选定的 BAT 在随包 MTool 清理范围内。可返回调整，取消清理或更改启动文件。
                      </p>
                    ) : (
                      <p className="muted">导入后将随包 MTool 移入回收站。</p>
                    )
                  ) : (
                    <MtoolCleanupStatus item={item} />
                  ))}
              </li>
            );
          })}
        </ul>
      </div>
    </Modal>
  );
}
