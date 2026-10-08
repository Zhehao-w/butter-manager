import { useEffect, useState } from 'react';
import type { JobPage } from './types';
import { activeJob } from './useJob';
import { Icon } from './ui';
import { ProgressBar } from './ProgressBar';

export function TaskBanner({
  page,
  onCancel,
  onViewResults,
  onSkip,
}: {
  page: JobPage;
  onCancel: () => void;
  onViewResults?: () => void;
  onSkip?: (path: string) => void;
}) {
  const [hidden, setHidden] = useState(false);
  const [collapsed, setCollapsed] = useState(false);
  const [fading, setFading] = useState(false);
  const [entered, setEntered] = useState(false);
  const [dismissed, setDismissed] = useState(false);
  const [hovered, setHovered] = useState(false);
  const [focused, setFocused] = useState(false);
  const running = activeJob(page);
  const paused = hovered || focused;
  const closing = fading || dismissed;
  useEffect(() => {
    setHidden(false);
    setCollapsed(false);
    setFading(false);
    setDismissed(false);
    setEntered(false);
    let enterFrame = 0;
    const frame = requestAnimationFrame(() => {
      enterFrame = requestAnimationFrame(() => setEntered(true));
    });
    return () => {
      cancelAnimationFrame(frame);
      cancelAnimationFrame(enterFrame);
    };
  }, [page.id]);
  useEffect(() => {
    if (dismissed || collapsed) return;
    setFading(false);
    if (running || paused || page.status === 'failed') return;
    const timer = setTimeout(() => setFading(true), 5000);
    return () => clearTimeout(timer);
  }, [page.id, page.status, running, paused, dismissed, collapsed]);
  useEffect(() => {
    if (!closing) return;
    const timer = setTimeout(() => setCollapsed(true), 300);
    return () => clearTimeout(timer);
  }, [closing]);
  useEffect(() => {
    if (!collapsed) return;
    const timer = setTimeout(() => setHidden(true), 300);
    return () => clearTimeout(timer);
  }, [collapsed]);
  if (hidden) return null;
  const title = page.kind === 'scan' ? '扫描目录' : '检查目录';
  const issues = page.path_checks?.filter((check) => check.state !== 'available').length ?? 0;
  const summary = running
    ? page.status === 'cancel_requested'
      ? '正在取消…'
      : page.phase
    : page.status === 'completed'
      ? page.kind === 'scan'
        ? `完成 · ${page.total} 个目录`
        : `完成 · ${page.processed} 项，${issues} 项需处理`
      : page.status === 'cancelled'
        ? '已取消 · 已完成的结果保留'
        : '失败 · 已完成的结果保留';
  return (
    <div className={`task-banner-slot${entered && !collapsed ? ' expanded' : ''}`}>
      <div className="task-banner-slot-content">
        <section
          className={`task-banner${closing ? ' fading' : ''}${page.status === 'failed' ? ' task-banner-error' : ''}`}
          role={page.status === 'failed' ? 'alert' : 'status'}
          aria-label={`${title}进度`}
          onMouseEnter={() => setHovered(true)}
          onMouseLeave={() => setHovered(false)}
          onFocusCapture={() => setFocused(true)}
          onBlurCapture={(event) => {
            if (!event.currentTarget.contains(event.relatedTarget as Node | null))
              setFocused(false);
          }}
        >
          <Icon
            name={running ? 'folder' : page.status === 'completed' ? 'check' : 'info'}
            size={20}
          />
          <div className="task-banner-content">
            <div className="task-banner-summary">
              <strong>{title}</strong>
              <span>{summary}</span>
              {running && (
                <span>
                  {page.total
                    ? `${page.processed} / ${page.total}`
                    : `已发现 ${page.change_count} 个目录`}
                </span>
              )}
            </div>
            {running ? (
              <ProgressBar
                label={`${title}完成进度`}
                max={page.total || 1}
                value={page.total ? page.processed : undefined}
              />
            ) : page.error ? (
              <p>{page.error}</p>
            ) : (
              page.kind === 'scan' && (
                <p>
                  确认启动文件后加入游戏库
                  {page.warnings.length ? '；部分目录需查看扫描诊断。' : '。'}
                </p>
              )
            )}
            {running &&
              page.idle_ms >= 3000 &&
              onSkip &&
              Object.entries(page.active).length > 0 && (
                <details className="task-banner-slow">
                  <summary>目录读取较久，可跳过当前游戏</summary>
                  {Object.entries(page.active).map(([path, current]) => (
                    <div key={path}>
                      <span className="path" title={current}>
                        {current}
                      </span>
                      <button
                        disabled={page.status === 'cancel_requested'}
                        onClick={() => onSkip(path)}
                      >
                        跳过此游戏
                      </button>
                    </div>
                  ))}
                </details>
              )}
          </div>
          <div className="task-banner-actions">
            {onViewResults && (
              <button onClick={onViewResults}>
                <Icon name="scan" size={16} />
                查看扫描结果
              </button>
            )}
            {running ? (
              <button disabled={page.status === 'cancel_requested'} onClick={onCancel}>
                取消任务
              </button>
            ) : (
              <button
                className="task-banner-close"
                aria-label={`关闭${title}提示`}
                onClick={() => setDismissed(true)}
              >
                <Icon name="close" size={16} />
              </button>
            )}
          </div>
        </section>
      </div>
    </div>
  );
}
