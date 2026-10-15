import { useEffect, useLayoutEffect, useMemo, useRef, useState } from 'react';
import type { ReactNode } from 'react';
import { api } from './api';
import { activeJob, useJob } from './useJob';
import { useStableVirtualizer } from './useStableVirtualizer';
import { Icon, PageHeader, SearchField } from './ui';
import { DeleteGameDialog } from './DeleteGameDialog';
import { displayVersion } from './library';
import { directoryTime } from './scan';
import { LaunchBadge } from './gameStatus';
import type { DeleteReport, DuplicatePair, DuplicateReview, DuplicateStatus, Game } from './types';

// Relationship identity is independent of presentation order.
const pairKey = (pair: Pick<DuplicatePair, 'left_id' | 'right_id'>) =>
  JSON.stringify([pair.left_id, pair.right_id].sort());
const statusLabels: Record<DuplicateStatus, string> = {
  pending: '待确认',
  confirmed: '已确认',
  excluded: '已排除',
};
const viewLabels = { all: '全部', ...statusLabels };
const meaningful = (text: string) =>
  !!text.trim() && !/^(unknown|n\/a|none|-|未知)$/i.test(text.trim());

export function LibraryDuplicates({
  games,
  visible,
  locked,
  tabs,
  onActive,
  onOpen,
  onError,
  onDeleted,
}: {
  games: Game[];
  visible: boolean;
  locked: boolean;
  tabs: ReactNode;
  onActive: (active: boolean) => void;
  onOpen: (id: string) => void;
  onError: (error: string) => void;
  onDeleted: (id: string, report: DeleteReport) => void;
}) {
  const [jobId, setJobId] = useState<string | null>(null);
  const [pairs, setPairs] = useState<DuplicatePair[] | null>(null);
  const [reviews, setReviews] = useState<DuplicateReview[]>([]);
  const reviewGeneration = useRef(0);
  const [view, setView] = useState<DuplicateStatus | 'all'>('all');
  const [search, setSearch] = useState('');
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState('');
  const [deleting, setDeleting] = useState<{ game: Game; kept: Game } | null>(null);
  const [selectedForDeletion, setSelectedForDeletion] = useState<Record<string, string>>({});
  const [directoryTimes, setDirectoryTimes] = useState<Record<string, number | null>>({});
  const loadedReferences = useRef(new Set<string>());
  const referenceGeneration = useRef(0);
  const [checkedRevision, setCheckedRevision] = useState<string | null>(null);
  const revision = useMemo(
    () =>
      JSON.stringify(
        games.map((game) => [
          game.id,
          game.updated_at,
          game.display_title,
          game.canonical_title,
          game.aliases,
          game.install_path,
          game.current_version,
          game.engine,
          game.main_executable,
          game.mtool_target_exe,
          game.mtool_loader,
          game.external_player,
          game.save_paths,
        ]),
      ),
    [games],
  );
  const pendingRevision = useRef(revision);
  const [loadingResults, setLoadingResults] = useState(false);
  const job = useJob(
    jobId,
    () => {},
    (page) => {
      if (page.status === 'completed') {
        reviewGeneration.current++;
        setLoadingResults(true);
        void Promise.all([api.duplicateResults(page.id), api.duplicateReviews()])
          .then(([results, stored]) => {
            setPairs(results);
            setReviews(stored);
            setCheckedRevision(pendingRevision.current);
          })
          .catch((error) => onError(String(error)))
          .finally(() => setLoadingResults(false));
      } else if (page.status === 'failed') onError(page.error || '重复检查失败');
    },
    onError,
  );
  const checking = !!jobId && (!job.page || job.page.id !== jobId || activeJob(job.page));
  const active = busy || loadingResults || checking || !!deleting;
  useEffect(() => {
    onActive(active);
  }, [active, onActive]);
  useEffect(() => {
    if (!visible) return;
    let stale = false;
    const generation = reviewGeneration.current;
    void api
      .duplicateReviews()
      .then((rows) => {
        if (!stale && generation === reviewGeneration.current) setReviews(rows);
      })
      .catch((error) => {
        if (!stale) onError(String(error));
      });
    return () => {
      stale = true;
    };
  }, [visible, onError]);

  const gamesById = useMemo(() => new Map(games.map((game) => [game.id, game])), [games]);
  const statuses = useMemo(
    () => new Map(reviews.map((review) => [pairKey(review), review.status])),
    [reviews],
  );
  const allPairs = useMemo(() => {
    const source = new Map((pairs || []).map((pair) => [pairKey(pair), pair]));
    // Saved decisions remain accessible after restarting, without forcing a fresh check.
    for (const review of reviews)
      if (!source.has(pairKey(review)))
        source.set(pairKey(review), {
          ...review,
          score: 0,
          confidence: 'review',
          reason: review.status === 'confirmed' ? '已确认重复，待处理' : '已标记为不是同一游戏',
        });
    return [...source.values()]
      .filter((pair) => gamesById.has(pair.left_id) && gamesById.has(pair.right_id))
      .sort((a, b) => b.score - a.score || pairKey(a).localeCompare(pairKey(b)));
  }, [pairs, reviews, gamesById]);
  const counts = useMemo(() => {
    const result = { pending: 0, confirmed: 0, excluded: 0 };
    for (const pair of allPairs) result[statuses.get(pairKey(pair)) || 'pending']++;
    return result;
  }, [allPairs, statuses]);
  const rows = useMemo(() => {
    const needle = search.normalize('NFKC').toLocaleLowerCase();
    return allPairs.filter((pair) => {
      const state = statuses.get(pairKey(pair)) || 'pending';
      return (
        (view === 'all' || state === view) &&
        [gamesById.get(pair.left_id)!, gamesById.get(pair.right_id)!].some((game) =>
          [
            game.display_title,
            game.canonical_title,
            game.install_path,
            game.current_version,
            game.engine,
            ...game.aliases,
          ].some((text) => text.normalize('NFKC').toLocaleLowerCase().includes(needle)),
        )
      );
    });
  }, [allPairs, statuses, view, search, gamesById]);
  const scroll = useRef<HTMLDivElement>(null);
  const scrollPositions = useRef({ all: 0, pending: 0, confirmed: 0, excluded: 0 });
  useLayoutEffect(() => {
    if (visible && scroll.current) scroll.current.scrollTop = scrollPositions.current[view];
  }, [visible, view]);
  useLayoutEffect(() => {
    scrollPositions.current = { all: 0, pending: 0, confirmed: 0, excluded: 0 };
    if (scroll.current) scroll.current.scrollTop = 0;
  }, [search]);
  const virtual = rows.length > 80;
  const { virtualizer } = useStableVirtualizer(
    {
      count: rows.length,
      getScrollElement: () => scroll.current,
      estimateSize: () => 190,
      getItemKey: (index) => pairKey(rows[index]),
      overscan: 4,
      enabled: virtual && visible,
    },
    'library-duplicates',
  );
  const rendered = virtual
    ? virtualizer.getVirtualItems().map((row) => ({ pair: rows[row.index], row }))
    : rows.map((pair) => ({ pair, row: undefined }));
  const referenceIds = JSON.stringify(
    [...new Set(rendered.flatMap(({ pair }) => [pair.left_id, pair.right_id]))].sort(),
  );
  useEffect(() => {
    referenceGeneration.current++;
    loadedReferences.current.clear();
    setDirectoryTimes({});
  }, [revision, jobId]);
  useEffect(() => {
    if (!visible) return;
    const ids = (JSON.parse(referenceIds) as string[]).filter(
      (id) => !loadedReferences.current.has(id),
    );
    if (!ids.length) return;
    const generation = referenceGeneration.current;
    ids.forEach((id) => loadedReferences.current.add(id));
    void api
      .duplicateGameReferences(ids)
      .then((rows) => {
        if (generation !== referenceGeneration.current) return;
        setDirectoryTimes((current) => ({
          ...current,
          ...Object.fromEntries(ids.map((id) => [id, null])),
          ...Object.fromEntries(rows.map((row) => [row.id, row.directory_modified_ms])),
        }));
      })
      .catch((error) => {
        if (generation !== referenceGeneration.current) return;
        ids.forEach((id) => loadedReferences.current.delete(id));
        setDirectoryTimes((current) => ({
          ...current,
          ...Object.fromEntries(ids.map((id) => [id, null])),
        }));
        onError(String(error));
      });
  }, [visible, referenceIds, onError, revision, jobId]);

  async function start() {
    setBusy(true);
    setMessage('');
    try {
      pendingRevision.current = revision;
      setJobId(await api.startDuplicateCheck());
    } catch (error) {
      onError(String(error));
    } finally {
      setBusy(false);
    }
  }
  async function changeState(pair: DuplicatePair, status: DuplicateStatus) {
    setBusy(true);
    setMessage('');
    reviewGeneration.current++;
    try {
      await api.setDuplicateReview(pair.left_id, pair.right_id, status);
      setSelectedForDeletion((current) => {
        const next = { ...current };
        delete next[pairKey(pair)];
        return next;
      });
      if (status === 'pending') {
        setPairs((current) =>
          current?.some((row) => pairKey(row) === pairKey(pair))
            ? current
            : [
                ...(current || []),
                {
                  ...pair,
                  reason: pair.score ? pair.reason : '已撤销决定，请重新检查匹配程度',
                },
              ],
        );
      }
      setReviews((current) => [
        ...current.filter((row) => pairKey(row) !== pairKey(pair)),
        ...(status === 'pending'
          ? []
          : [
              {
                left_id: pair.left_id,
                right_id: pair.right_id,
                created_at: new Date().toISOString(),
                status,
              },
            ]),
      ]);
      setMessage(
        status === 'confirmed'
          ? '已确认重复，请选中要删除的一份，或稍后处理。'
          : status === 'excluded'
            ? '已标记为不是同一游戏。'
            : statuses.get(pairKey(pair)) === 'excluded'
              ? '已撤销排除，请重新确认。'
              : '已撤销确认，请重新确认。',
      );
    } catch (error) {
      onError(String(error));
    } finally {
      setBusy(false);
    }
  }
  function deleted(report: DeleteReport) {
    const id = deleting!.game.id;
    setDeleting(null);
    if (report.removed) {
      reviewGeneration.current++;
      setPairs(
        (current) => current?.filter((pair) => pair.left_id !== id && pair.right_id !== id) ?? null,
      );
      setReviews((current) =>
        current.filter((pair) => pair.left_id !== id && pair.right_id !== id),
      );
      setMessage('重复游戏已送入回收站，保留的一份未改动。');
    }
    onDeleted(id, report);
  }
  return (
    <section className="panel scan-page duplicate-page" hidden={!visible} aria-label="重复检查页面">
      <PageHeader
        icon="scan"
        title="游戏整理"
        description="检查已入库游戏的疑似重复，或扫描目录发现新游戏。"
      >
        <div className="header-actions">
          {activeJob(job.page) && (
            <button
              onClick={() => {
                void api
                  .cancel(jobId!)
                  .then(() => job.markCancelling())
                  .catch((error) => onError(String(error)));
              }}
              disabled={job.page?.status === 'cancel_requested'}
            >
              取消检查
            </button>
          )}
          <button
            className="primary"
            disabled={locked || active || games.length < 2}
            onClick={() => void start()}
          >
            <Icon name="search" />
            {checking || loadingResults ? '检查中…' : pairs === null ? '查找重复' : '重新检查'}
          </button>
        </div>
      </PageHeader>
      <div className="library-toolbar organizer-toolbar duplicate-toolbar">
        <SearchField
          label="搜索重复组合"
          placeholder="搜索名称、版本、引擎或路径"
          value={search}
          onChange={setSearch}
        />
        {tabs}
        <div className="import-tabs duplicate-status-tabs" role="tablist" aria-label="重复处理状态">
          {(['all', 'pending', 'confirmed', 'excluded'] as const).map((status) => (
            <button
              key={status}
              role="tab"
              aria-selected={view === status}
              className={view === status ? 'active' : ''}
              onClick={() => {
                setView(status);
              }}
            >
              {viewLabels[status]}{' '}
              <span>{status === 'all' ? allPairs.length : counts[status]}</span>
            </button>
          ))}
        </div>
      </div>
      {checkedRevision && checkedRevision !== revision && (
        <p className="notice">游戏库资料已变化，重新检查可更新匹配结果。</p>
      )}
      <div
        className="scan-scroll"
        ref={scroll}
        aria-label="重复游戏组合"
        role="region"
        onScroll={(event) => {
          scrollPositions.current[view] = event.currentTarget.scrollTop;
        }}
      >
        {!rows.length && (
          <div className="empty">
            <h3>
              {(view === 'pending' || view === 'all') && pairs === null && !allPairs.length
                ? '查找库内的疑似重复游戏'
                : view === 'all'
                  ? '没有重复组合'
                  : '没有' + statusLabels[view] + '的组合'}
            </h3>
            <p>
              {games.length < 2
                ? '至少需要两个已入库游戏。未入库文件夹请使用目录扫描。'
                : '确认不会立即删除；回收游戏前仍需预检和二次确认。'}
            </p>
          </div>
        )}
        <div
          className={virtual ? 'duplicate-list duplicate-list-virtual' : 'duplicate-list'}
          style={virtual ? { height: virtualizer.getTotalSize() } : undefined}
        >
          {rendered.map(({ pair, row }) => {
            const status = statuses.get(pairKey(pair)) || 'pending';
            const reason = pair.score
              ? pair.reason
              : status === 'pending'
                ? '已撤销决定，请重新检查匹配程度'
                : status === 'confirmed'
                  ? '已确认重复，待处理'
                  : '已标记为不是同一游戏';
            return (
              <div
                key={pairKey(pair)}
                data-index={row?.index}
                ref={row ? virtualizer.measureElement : undefined}
                className={row ? 'duplicate-virtual-row' : undefined}
                style={row ? { transform: 'translateY(' + row.start + 'px)' } : undefined}
              >
                <article className="duplicate-card">
                  <div className="duplicate-card-heading">
                    <span
                      className={
                        'mode duplicate-confidence-' +
                        (status === 'excluded'
                          ? 'ignored'
                          : status === 'confirmed'
                            ? 'confirmed'
                            : 'review')
                      }
                    >
                      {status === 'confirmed' ? '已确认重复' : statusLabels[status]}
                    </span>
                    <p title={pair.score ? `${reason} · 匹配程度 ${pair.score}` : reason}>
                      {reason}
                    </p>
                    <div className="duplicate-card-actions">
                      {status === 'pending' ? (
                        <>
                          <button
                            disabled={locked || active}
                            onClick={() => void changeState(pair, 'excluded')}
                          >
                            不是同一游戏
                          </button>
                          <button
                            className="primary"
                            disabled={locked || active}
                            onClick={() => void changeState(pair, 'confirmed')}
                          >
                            确认重复
                          </button>
                        </>
                      ) : status === 'excluded' ? (
                        <button
                          disabled={locked || active}
                          onClick={() => void changeState(pair, 'pending')}
                        >
                          撤销排除
                        </button>
                      ) : (
                        <>
                          <button
                            disabled={locked || active}
                            onClick={() => void changeState(pair, 'pending')}
                          >
                            撤销确认
                          </button>
                          <button
                            className="danger"
                            disabled={
                              locked ||
                              active ||
                              ![pair.left_id, pair.right_id].includes(
                                selectedForDeletion[pairKey(pair)],
                              )
                            }
                            onClick={() =>
                              setDeleting({
                                game: gamesById.get(selectedForDeletion[pairKey(pair)])!,
                                kept: gamesById.get(
                                  selectedForDeletion[pairKey(pair)] === pair.left_id
                                    ? pair.right_id
                                    : pair.left_id,
                                )!,
                              })
                            }
                          >
                            删除选中游戏…
                          </button>
                        </>
                      )}
                    </div>
                  </div>
                  <div
                    className="duplicate-game-grid"
                    role={status === 'confirmed' ? 'radiogroup' : undefined}
                    aria-label={status === 'confirmed' ? '选择要删除的游戏' : undefined}
                  >
                    {[pair.left_id, pair.right_id].map((id) => {
                      const game = gamesById.get(id)!;
                      const meta = [
                        meaningful(game.current_version)
                          ? displayVersion(game.current_version)
                          : '',
                        meaningful(game.engine) ? game.engine : '',
                      ]
                        .filter(Boolean)
                        .join(' · ');
                      return (
                        <div
                          key={id}
                          className={
                            'duplicate-game' +
                            (status === 'confirmed' ? ' duplicate-game-selectable' : '') +
                            (status === 'confirmed' && selectedForDeletion[pairKey(pair)] === id
                              ? ' is-selected'
                              : status === 'confirmed' && selectedForDeletion[pairKey(pair)]
                                ? ' is-kept'
                                : '')
                          }
                          onClick={(event) => {
                            if (
                              status === 'confirmed' &&
                              !locked &&
                              !active &&
                              !(event.target as HTMLElement).closest('button,input,label')
                            )
                              setSelectedForDeletion((current) => ({
                                ...current,
                                [pairKey(pair)]: id,
                              }));
                          }}
                        >
                          {status === 'confirmed' && (
                            <label className="duplicate-game-selection">
                              <input
                                type="radio"
                                name={'delete-' + pairKey(pair)}
                                aria-label={'选择删除：' + game.display_title}
                                checked={selectedForDeletion[pairKey(pair)] === id}
                                disabled={locked || active}
                                onChange={() =>
                                  setSelectedForDeletion((current) => ({
                                    ...current,
                                    [pairKey(pair)]: id,
                                  }))
                                }
                              />
                              <span>
                                {selectedForDeletion[pairKey(pair)] && (
                                  <Icon
                                    name={
                                      selectedForDeletion[pairKey(pair)] === id ? 'close' : 'check'
                                    }
                                    size={15}
                                  />
                                )}
                                {selectedForDeletion[pairKey(pair)] === id
                                  ? '移入回收站'
                                  : selectedForDeletion[pairKey(pair)]
                                    ? '保留'
                                    : '选择删除这份'}
                              </span>
                            </label>
                          )}
                          <div className="duplicate-game-title">
                            <strong title={game.display_title}>{game.display_title}</strong>
                            <div className="duplicate-game-links">
                              <button onClick={() => onOpen(id)} disabled={!!deleting}>
                                查看详情
                              </button>
                              <button
                                disabled={!!deleting}
                                onClick={() =>
                                  void api.openFolder(id).catch((error) => onError(String(error)))
                                }
                              >
                                <Icon name="folder" size={13} /> 打开目录
                              </button>
                            </div>
                          </div>
                          {meta && <span className="muted">{meta}</span>}
                          <div className="duplicate-game-reference">
                            <LaunchBadge game={game} />
                            <span className="muted">
                              目录修改：
                              {id in directoryTimes ? directoryTime(directoryTimes[id]) : '读取中…'}
                            </span>
                          </div>
                          <span className="path" title={game.install_path}>
                            {game.install_path}
                          </span>
                        </div>
                      );
                    })}
                  </div>
                </article>
              </div>
            );
          })}
        </div>
      </div>
      <div className="scan-summary" role="status">
        <span>
          显示 {rows.length} 组 · 游戏库 {games.length} 个游戏
        </span>
        <span>
          {checking
            ? (job.page?.phase || '准备检查') +
              ' · ' +
              (job.page?.processed || 0) +
              '/' +
              (job.page?.total || games.length)
            : job.page?.status === 'cancelled'
              ? '检查已取消，之前的结果保留'
              : job.page?.status === 'failed'
                ? '检查失败，请重试；之前的结果保留'
                : message ||
                  (checkedRevision ? '按匹配程度排序，请根据详情确认' : '点击“查找重复”开始')}
        </span>
      </div>
      {deleting && (
        <DeleteGameDialog
          game={deleting.game}
          keptGame={deleting.kept}
          onBusy={setBusy}
          onClose={() => setDeleting(null)}
          onDeleted={deleted}
        />
      )}
    </section>
  );
}
