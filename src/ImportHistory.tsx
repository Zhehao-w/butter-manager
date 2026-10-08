import { useLayoutEffect, useMemo, useRef, useState } from 'react';
import { useStableVirtualizer } from './useStableVirtualizer';
import { Icon, Modal } from './ui';
import { bytesText, displayVersion, formatTime } from './library';
import { LaunchBadge } from './gameStatus';
import type { Game, ImportItem, ImportPlan } from './types';

export function isImportRecord(item: ImportItem) {
  return item.state === 'completed' || item.state === 'rolled_back';
}

function resultText(item: ImportItem) {
  if (item.state === 'rolled_back')
    return `已回退到 ${displayVersion(item.update?.old_version || '')}`;
  return item.update
    ? `更新 ${displayVersion(item.update.old_version)} → ${displayVersion(item.selection.version)}`
    : '新增到游戏库';
}

export function ImportHistory({
  plans,
  games,
  search,
  visible,
  disabled,
  rollbackDisabled,
  initialOffset,
  onScrollOffset,
  onOpenGame,
  onRollback,
}: {
  plans: ImportPlan[];
  games: Game[];
  search: string;
  visible: boolean;
  disabled: boolean;
  rollbackDisabled: boolean;
  initialOffset: number;
  onScrollOffset: (offset: number) => void;
  onOpenGame: (id: string) => void;
  onRollback: (planId: string, index: number) => void;
}) {
  const [detailKey, setDetailKey] = useState<string | null>(null);
  const [highlightedKey, setHighlightedKey] = useState<string | null>(null);
  const scroll = useRef<HTMLDivElement>(null);
  const offset = useRef(initialOffset);
  const lastSearch = useRef(search);
  const records = useMemo(
    () =>
      plans
        .flatMap((plan) =>
          plan.items.flatMap((item, index) => {
            if (!isImportRecord(item)) return [];
            const id = item.registered_id || item.selection.existing_id;
            const game = games.find((game) =>
              id
                ? game.id === id
                : game.install_path.replaceAll('\\', '/').toLowerCase() ===
                  item.target.replaceAll('\\', '/').toLowerCase(),
            );
            const timestamp = item.completed_at_ms ?? plan.recorded_at_ms;
            const date = timestamp != null ? new Date(timestamp) : null;
            const validDate = date && !Number.isNaN(date.getTime()) ? date : null;
            return [
              {
                key: `${plan.id}:${index}`,
                planId: plan.id,
                index,
                item,
                game,
                timestamp: validDate?.getTime() ?? 0,
                dateLabel:
                  validDate?.toLocaleDateString('zh-CN', {
                    year: 'numeric',
                    month: 'long',
                    day: 'numeric',
                  }) || '较早记录',
                timeLabel:
                  validDate?.toLocaleTimeString('zh-CN', {
                    hour: '2-digit',
                    minute: '2-digit',
                    hour12: false,
                  }) || '时间未记录',
              },
            ];
          }),
        )
        .sort((a, b) => b.timestamp - a.timestamp || a.key.localeCompare(b.key)),
    [plans, games],
  );
  const rows = useMemo(() => {
    const keyword = search.trim().toLocaleLowerCase();
    return records.filter(({ item, game }) =>
      [
        item.selection.title,
        game?.display_title,
        item.selection.source,
        item.target,
        item.selection.version,
        item.update?.old_version,
      ].some((value) => value?.toLocaleLowerCase().includes(keyword)),
    );
  }, [records, search]);
  const detail = records.find((record) => record.key === detailKey);
  useLayoutEffect(() => {
    if (search !== lastSearch.current) {
      offset.current = 0;
      onScrollOffset(0);
    }
    lastSearch.current = search;
    if (visible && scroll.current) scroll.current.scrollTop = offset.current;
  }, [visible, search, onScrollOffset]);
  const virtual = rows.length > 50;
  const { virtualizer, scrolling } = useStableVirtualizer(
    {
      count: rows.length,
      getScrollElement: () => scroll.current,
      estimateSize: (index) =>
        index === 0 || rows[index].dateLabel !== rows[index - 1].dateLabel ? 132 : 94,
      getItemKey: (index) => rows[index].key,
      overscan: 6,
      enabled: virtual && visible,
      initialOffset: () => offset.current,
    },
    'import-history',
  );
  const indices = virtual
    ? virtualizer.getVirtualItems().map(({ index, start, size }) => ({ index, start, size }))
    : rows.map((_, index) => ({ index, start: 0, size: 0 }));
  return (
    <>
      <div
        className="import-scroll import-history"
        ref={scroll}
        id="import-content"
        role="tabpanel"
        aria-labelledby="import-tab-completed"
        onScroll={(event) => {
          offset.current = event.currentTarget.scrollTop;
          onScrollOffset(offset.current);
        }}
      >
        {!rows.length && (
          <div className="empty">
            <h3>{search ? '没有匹配的导入记录' : '暂无导入记录'}</h3>
            <p>
              {search ? '可搜索游戏名称、版本或路径。' : '完成导入或更新后，可在这里查看记录。'}
            </p>
          </div>
        )}
        <div
          style={virtual ? { height: virtualizer.getTotalSize(), position: 'relative' } : undefined}
        >
          {indices.map(({ index, start, size }) => {
            const record = rows[index];
            const { item, game } = record;
            const groupStart = index === 0 || record.dateLabel !== rows[index - 1].dateLabel;
            return (
              <div
                key={record.key}
                data-index={index}
                ref={virtual ? virtualizer.measureElement : undefined}
                className={virtual ? 'import-virtual-row' : undefined}
                style={
                  virtual
                    ? {
                        transform: `translateY(${start}px)`,
                        height: scrolling ? size : undefined,
                        overflow: scrolling ? 'clip' : undefined,
                      }
                    : undefined
                }
              >
                {groupStart && <h3 className="import-history-date">{record.dateLabel}</h3>}
                <article
                  className={`import-history-row${record.key === highlightedKey ? ' selected' : ''}`}
                  aria-label={item.selection.title}
                  onMouseEnter={() => setHighlightedKey(record.key)}
                  onFocus={() => setHighlightedKey(record.key)}
                >
                  <span className={`import-history-icon ${item.update ? 'updated' : 'added'}`}>
                    <Icon name={item.update ? 'save' : 'import'} size={20} />
                  </span>
                  <div className="import-history-summary">
                    <strong title={item.selection.title}>{item.selection.title}</strong>
                    <div className="import-history-meta">
                      <span
                        className={`import-history-result ${item.state === 'rolled_back' ? 'reverted' : item.update ? 'updated' : 'added'}`}
                      >
                        {resultText(item)}
                      </span>
                      <time
                        title={
                          item.completed_at_ms != null ? '完成时间' : '旧记录按记录文件时间显示'
                        }
                      >
                        {record.timeLabel}
                      </time>
                    </div>
                  </div>
                  <div className="import-history-actions">
                    <button
                      disabled={disabled || !game}
                      title={!game ? '游戏已不在库中' : undefined}
                      onClick={() => {
                        setHighlightedKey(record.key);
                        if (game) onOpenGame(game.id);
                      }}
                    >
                      查看游戏
                    </button>
                    <button
                      disabled={disabled}
                      onClick={() => {
                        setHighlightedKey(record.key);
                        setDetailKey(record.key);
                      }}
                    >
                      详情
                      <Icon name="next" size={14} />
                    </button>
                  </div>
                </article>
              </div>
            );
          })}
        </div>
      </div>
      {detail && (
        <Modal
          variant="record"
          title="导入记录详情"
          onClose={() => setDetailKey(null)}
          footer={
            <div className="import-history-detail-actions">
              {detail.game && (
                <button
                  disabled={disabled}
                  onClick={() => {
                    setDetailKey(null);
                    onOpenGame(detail.game!.id);
                  }}
                >
                  查看游戏
                </button>
              )}
              {detail.item.update &&
                detail.item.state === 'completed' &&
                detail.item.update.rollback_available && (
                  <button
                    className="danger danger-solid"
                    disabled={
                      rollbackDisabled ||
                      !detail.game ||
                      detail.game.current_version !== detail.item.selection.version
                    }
                    onClick={() => {
                      setDetailKey(null);
                      onRollback(detail.planId, detail.index);
                    }}
                  >
                    回退旧版本
                  </button>
                )}
            </div>
          }
        >
          <div className="import-history-detail-heading">
            <h3>{detail.item.selection.title}</h3>
            <span className="import-history-result">{resultText(detail.item)}</span>
          </div>
          <dl className="import-record-facts">
            <div>
              <dt>{detail.item.completed_at_ms != null ? '完成时间' : '记录时间'}</dt>
              <dd>
                {detail.timestamp ? formatTime(new Date(detail.timestamp).toISOString()) : '未记录'}
                {detail.item.completed_at_ms == null && detail.timestamp > 0 && (
                  <span className="muted">旧记录按记录文件时间显示</span>
                )}
              </dd>
            </div>
            <div>
              <dt>来源目录</dt>
              <dd>{detail.item.selection.source}</dd>
            </div>
            <div>
              <dt>游戏目录</dt>
              <dd>{detail.item.target}</dd>
            </div>
            <div>
              <dt>文件</dt>
              <dd>
                {detail.item.files} 个文件 · {bytesText(detail.item.bytes)}
              </dd>
            </div>
            <div>
              <dt>启动方式</dt>
              <dd>
                <LaunchBadge
                  label={
                    detail.item.selection.external_player
                      ? 'QSP 播放器'
                      : detail.item.selection.mtool
                        ? 'MTool'
                        : '直接启动'
                  }
                />
              </dd>
            </div>
            <div>
              <dt>启动文件</dt>
              <dd>{detail.item.selection.executable || '未配置'}</dd>
            </div>
            {detail.item.selection.external_player && (
              <div>
                <dt>QSP 游戏文件</dt>
                <dd>{detail.item.selection.external_player.game_file || '未配置'}</dd>
              </div>
            )}
          </dl>
          {detail.item.update && (
            <section className="update-save-review">
              <strong>
                {detail.item.selection.preserve_saves !== false ? '保留内部旧存档' : '使用新包存档'}
              </strong>
              <p>
                {detail.item.selection.preserve_saves !== false
                  ? '内部旧存档覆盖到新版相同位置；外部存档不操作。'
                  : '不迁移内部旧存档；外部存档不操作。'}
              </p>
              {detail.item.update.saves.map((save) => (
                <p className="path" key={save.source}>
                  {save.configured}
                  {!save.present ? ' · 不存在，跳过' : ''}
                </p>
              ))}
              <p className="muted">旧版移入回收站，清空后无法回退。</p>
              {detail.item.state === 'completed' &&
                (!detail.item.update.rollback_available ? (
                  <p className="muted">旧版本不在回收站，无法回退。</p>
                ) : !detail.game ? (
                  <p className="muted">游戏已不在库中，无法回退。</p>
                ) : detail.game.current_version !== detail.item.selection.version ? (
                  <p className="muted">游戏已变更版本，无法从这条记录回退。</p>
                ) : null)}
            </section>
          )}
          {detail.item.error && <p className="notice">{detail.item.error}</p>}
        </Modal>
      )}
    </>
  );
}
