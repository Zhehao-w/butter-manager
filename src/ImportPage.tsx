import { useEffect, useLayoutEffect, useMemo, useRef, useState } from 'react';
import { useStableVirtualizer } from './useStableVirtualizer';
import { api } from './api';
import { activeJob, useJob } from './useJob';
import { BrowseButton, Modal, Icon, PageHeader, SearchField, SortField } from './ui';
import { bytesText, displayVersion, formatTime } from './library';
import { QspConfiguration } from './QspConfiguration';
import { EngineSelect } from './EngineSelect';
import { useSavedSort } from './preferences';
import { LaunchBadge } from './gameStatus';
import { ProgressBar } from './ProgressBar';
import { ImportDuplicateDialog, sameImportVersion } from './ImportDuplicateDialog';
import { ImportHistory, isImportRecord } from './ImportHistory';
import { isQspFile, localQsp, qspConfig, suggestedPlayer } from './qsp';
import type {
  Game,
  ImportMatch,
  ImportPlan,
  ImportRecoveryIssue,
  ImportSelection,
  ImportSourceDiscovery,
  ScanCandidate,
  Settings,
  JobPage,
} from './types';

const leaf = (path: string) => path.replaceAll('\\', '/').split('/').filter(Boolean).at(-1) || '';

function ImportProgress({ page }: { page: JobPage | null }) {
  const total = page?.overall_total || 0;
  const done = Math.min(total, page?.overall_done || 0);
  const measured = total > 0 && !page?.indeterminate;
  const percent = measured ? Math.floor((done / total) * 100) : null;
  const remaining = page?.remaining_seconds;
  const time =
    remaining == null
      ? null
      : remaining < 1
        ? '不足 1 秒'
        : remaining < 60
          ? `约 ${Math.ceil(remaining)} 秒`
          : `约 ${Math.ceil(remaining / 60)} 分钟`;
  const current = Object.values(page?.active || {}).at(-1);
  return (
    <>
      <div className="import-progress-heading">
        <span>
          {page?.phase || '准备中'} · {page?.processed || 0}/{page?.total || 0} 项
        </span>
        {percent != null && <strong>{percent}%</strong>}
      </div>
      <ProgressBar
        label="导入与更新整体进度"
        value={measured ? done : undefined}
        max={total || 1}
      />
      {page?.current_game && (
        <strong className="import-current-game" title={page.current_game}>
          {page.current_game}
        </strong>
      )}
      <div className="import-transfer-details">
        {!!page?.bytes_total && (
          <span>
            本次传输 {bytesText(page.bytes_done || 0)} / {bytesText(page.bytes_total)}
          </span>
        )}
        {!!page?.transfer_rate && (
          <>
            <span>{bytesText(page.transfer_rate)}/s</span>
            {time && <span>预计传输剩余 {time}</span>}
          </>
        )}
      </div>
      {current && (
        <span className="path" title={current}>
          {current}
        </span>
      )}
    </>
  );
}
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
  rollback_retrieve: '从回收站恢复',
  rollback_cleanup: '清理临时文件',
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

export function ImportPage({
  settings,
  games,
  visible,
  locked,
  onActive,
  onUpdated,
  onError,
  onOpenGame,
  recoveryIssues = [],
}: {
  settings: Settings;
  games: Game[];
  visible: boolean;
  locked: boolean;
  onActive: (active: boolean) => void;
  onUpdated: () => void;
  onError: (message: string) => void;
  onOpenGame: (id: string) => void;
  recoveryIssues?: ImportRecoveryIssue[];
}) {
  const [candidates, setCandidates] = useState<ScanCandidate[]>([]);
  const [choices, setChoices] = useState<Record<string, ImportSelection>>({});
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [matches, setMatches] = useState<Record<string, ImportMatch[]>>({});
  const [scanId, setScanId] = useState<string | null>(null);
  const [jobId, setJobId] = useState<string | null>(null);
  const [starting, setStarting] = useState(false);
  const [picking, setPicking] = useState(false);
  const [recycling, setRecycling] = useState(false);
  const [duplicateSource, setDuplicateSource] = useState<string | null>(null);
  const [matching, setMatching] = useState(false);
  const [plans, setPlans] = useState<ImportPlan[]>([]);
  const [planId, setPlanId] = useState<string | null>(null);
  const [view, setView] = useState<'new' | 'pending' | 'completed'>('new');
  const [notice, setNotice] = useState('');
  const [sourceReview, setSourceReview] = useState<ImportSourceDiscovery | null>(null);
  const [sourceScopes, setSourceScopes] = useState<Record<string, 'whole' | 'children' | ''>>({});
  const [sourceError, setSourceError] = useState('');
  const [search, setSearch] = useState('');
  const [sort, setSort] = useSavedSort<string>('import', 'name-asc', [
    'name-asc',
    'name-desc',
    'modified-asc',
    'modified-desc',
  ]);
  const [confirmation, setConfirmation] = useState<
    'move' | 'withdraw' | 'clear' | 'rollback' | null
  >(null);
  const [rollbackIndex, setRollbackIndex] = useState(0);
  const [linkSource, setLinkSource] = useState<string | null>(null);
  const [linkSearch, setLinkSearch] = useState('');
  const [initialized, setInitialized] = useState(false);
  const scroll = useRef<HTMLDivElement>(null);
  const scrollPositions = useRef<Record<string, number>>({});
  const lastFilter = useRef({ search, sort });
  const callbacks = useRef({ onError, onUpdated });
  callbacks.current = { onError, onUpdated };
  const plan = view === 'new' ? undefined : plans.find((p) => p.id === planId);
  const scrollScope = view === 'completed' ? 'import-history' : `${view}-${planId ?? 'new'}`;
  useLayoutEffect(() => {
    if (lastFilter.current.search !== search || lastFilter.current.sort !== sort)
      scrollPositions.current[scrollScope] = 0;
    lastFilter.current = { search, sort };
    if (visible && scroll.current)
      scroll.current.scrollTop = scrollPositions.current[scrollScope] ?? 0;
  }, [scrollScope, visible, search, sort]);
  const job = useJob(
    jobId,
    (changes) => {
      setCandidates((current) => {
        const byPath = new Map(current.map((c) => [c.install_path, c]));
        for (const c of changes) byPath.set(c.install_path, c);
        return [...byPath.values()];
      });
      setChoices((current) => {
        const next = { ...current };
        for (const c of changes)
          if (!next[c.install_path])
            next[c.install_path] = {
              source: c.install_path,
              title: c.suggested_title,
              target_name: leaf(c.install_path),
              version: c.suggested_version,
              engine: c.engine,
              executable: suggestedPlayer(c) || '',
              external_player: qspConfig(c),
              mtool: !c.qsp && !!c.mtool_detected,
              existing_id: null,
              new_override: false,
              preserve_saves: true,
              saves_confirmed: false,
            };
        return next;
      });
      setSelected(
        (current) =>
          new Set([
            ...current,
            ...changes
              .filter(
                (c) =>
                  !choices[c.install_path] &&
                  c.status === 'ready' &&
                  (c.qsp ? !!qspConfig(c)?.game_file : c.executables.length),
              )
              .map((c) => c.install_path),
          ]),
      );
    },
    (result) => {
      if (result.kind === 'import_analysis') {
        setMatching(true);
        void api
          .importMatches(result.id)
          .then((values) => {
            setMatches(values);
            setChoices((current) => {
              const next = { ...current };
              for (const [source, suggestions] of Object.entries(values))
                if (
                  suggestions.length === 1 &&
                  suggestions[0].auto_associate !== false &&
                  next[source] &&
                  !next[source].new_override &&
                  !next[source].existing_id
                )
                  next[source] = { ...next[source], existing_id: suggestions[0].id };
              return next;
            });
            setSelected(
              (current) =>
                new Set(
                  [...current].filter(
                    (source) => !values[source]?.length || choices[source]?.new_override,
                  ),
                ),
            );
          })
          .catch((e) => callbacks.current.onError(String(e)))
          .finally(() => setMatching(false));
      } else {
        void api
          .importPlans()
          .then((values) => {
            setPlans(values);
            if (result.kind === 'import_plan' && result.status === 'completed') {
              setPlanId(result.id);
              setView('pending');
            } else if (result.kind === 'import_apply' || result.kind === 'import_rollback') {
              const applied = values.find((p) => p.id === planId);
              if (applied?.status === 'completed') setView('completed');
              else if (applied) setView('pending');
            } else if (result.kind === 'import_withdraw') {
              const remaining = values.find((p) => !['completed', 'withdrawn'].includes(p.status));
              setPlanId(remaining?.id || null);
              if (!remaining) setView('new');
            }
          })
          .catch((e) => callbacks.current.onError(String(e)));
        if (['import_apply', 'import_withdraw', 'import_rollback'].includes(result.kind))
          callbacks.current.onUpdated();
      }
      if (result.error) callbacks.current.onError(result.error);
      else if (result.status === 'completed')
        setNotice(
          result.kind === 'import_plan'
            ? '导入与更新计划已生成，请确认。'
            : result.kind === 'import_analysis'
              ? '分析完成，请选择要导入或更新的游戏。'
              : '本次导入操作已完成。',
        );
    },
    (e) => callbacks.current.onError(e),
  );
  const busy =
    starting ||
    matching ||
    (!!jobId && (!job.page || job.page.id !== jobId || activeJob(job.page)));
  const disabled = busy || picking || recycling || locked;
  useEffect(() => {
    onActive(busy || picking || recycling);
  }, [busy, picking, recycling, onActive]);
  useEffect(() => {
    if (!visible || initialized) return;
    let stale = false;
    void api
      .importPlans()
      .then((values) => {
        if (stale) return;
        setPlans(values);
        setInitialized(true);
        const recovery = values.find(
          (p) => !['preview', 'completed', 'withdrawn'].includes(p.status),
        );
        if (recovery) {
          setPlanId(recovery.id);
          setView('pending');
        }
      })
      .catch((e) => {
        if (!stale) callbacks.current.onError(String(e));
      });
    return () => {
      stale = true;
    };
  }, [visible, initialized]);
  useEffect(() => {
    if (!busy || !planId) return;
    let stale = false;
    const timer = setInterval(
      () =>
        void api
          .importPlans()
          .then((values) => {
            if (!stale) setPlans(values);
          })
          .catch(() => {}),
      800,
    );
    return () => {
      stale = true;
      clearInterval(timer);
    };
  }, [busy, planId]);
  async function action(fn: () => Promise<void>) {
    setStarting(true);
    setNotice('');
    try {
      await fn();
    } catch (e) {
      callbacks.current.onError(String(e));
    } finally {
      setStarting(false);
    }
  }
  async function analyzeSources(sources: string[]) {
    if (!sources.length) return;
    const all = [...new Set([...candidates.map((c) => c.install_path), ...sources])];
    const id = await api.importAnalyze(all);
    setPlanId(null);
    setView('new');
    setScanId(id);
    setJobId(id);
  }
  async function choose() {
    let selected: string[];
    setPicking(true);
    try {
      selected = await api.chooseImportSources();
    } catch (error) {
      callbacks.current.onError(String(error));
      return;
    } finally {
      setPicking(false);
    }
    if (!selected.length) return;
    await action(async () => {
      const discovery = await api.discoverImportSources(selected);
      if (discovery.choices.length) {
        setSourceReview(discovery);
        setSourceScopes({});
        setSourceError('');
      } else {
        await analyzeSources(discovery.sources);
        if (discovery.warnings.length) setNotice(discovery.warnings.join('；'));
      }
    });
  }
  function update(source: string, change: Partial<ImportSelection>) {
    setChoices((current) => ({
      ...current,
      [source]: {
        ...current[source],
        ...(Object.hasOwn(change, 'existing_id') &&
        change.existing_id !== current[source].existing_id
          ? { working_directory: undefined, mtool_loader: undefined }
          : {}),
        ...change,
      },
    }));
  }
  const rows = useMemo(() => {
    const key = search.trim().toLocaleLowerCase();
    return candidates
      .filter((c) =>
        [
          c.suggested_title,
          c.install_path,
          choices[c.install_path]?.version,
          choices[c.install_path]?.title,
          choices[c.install_path]?.external_player?.game_file,
          choices[c.install_path]?.executable,
        ].some((v) => v?.toLocaleLowerCase().includes(key)),
      )
      .sort((a, b) =>
        sort.startsWith('modified')
          ? (Number(a.directory_modified_ms || 0) - Number(b.directory_modified_ms || 0)) *
            (sort.endsWith('desc') ? -1 : 1)
          : a.suggested_title.localeCompare(b.suggested_title, 'zh-CN') *
            (sort.endsWith('desc') ? -1 : 1),
      );
  }, [candidates, choices, search, sort]);
  const visiblePlanItems = useMemo(
    () =>
      plan?.items.filter((item) =>
        [item.selection.title, item.selection.source, item.target].some((v) =>
          v.toLocaleLowerCase().includes(search.trim().toLocaleLowerCase()),
        ),
      ) || [],
    [plan, search],
  );
  const count =
    view === 'completed' ? 0 : plan ? visiblePlanItems.length : view === 'new' ? rows.length : 0;
  const virtual = count > 50;
  const itemKey = useMemo(
    () => (index: number) =>
      `${view}-${plan?.id ?? 'new'}:${plan ? visiblePlanItems[index].selection.source : rows[index].install_path}`,
    [view, plan, visiblePlanItems, rows],
  );
  const { virtualizer, scrolling } = useStableVirtualizer(
    {
      count,
      getScrollElement: () => scroll.current,
      estimateSize: () => (plan ? 160 : 290),
      overscan: 5,
      enabled: virtual && visible,
      getItemKey: itemKey,
      initialOffset: () => scrollPositions.current[scrollScope] ?? 0,
    },
    scrollScope,
  );
  const indices = virtual
    ? virtualizer.getVirtualItems().map((v) => ({ index: v.index, start: v.start, size: v.size }))
    : Array.from({ length: count }, (_, index) => ({ index, start: 0, size: 0 }));
  const pendingPlans = plans.filter((p) => !['completed', 'withdrawn'].includes(p.status));
  const recordCount = plans.reduce(
    (count, plan) => count + plan.items.filter(isImportRecord).length,
    0,
  );
  const categoryPlans = pendingPlans;
  function switchView(next: typeof view) {
    setView(next);
    setSearch('');
    setNotice('');
    const entries = next === 'pending' ? pendingPlans : plans;
    setPlanId(
      next === 'new' ? null : entries.find((p) => p.id === planId)?.id || entries[0]?.id || null,
    );
  }
  const validSelections = [...selected].map((s) => choices[s]).filter(Boolean);
  const unresolvedMatches = validSelections.filter(
    (selection) =>
      !selection.existing_id && !selection.new_override && !!matches[selection.source]?.length,
  ).length;
  const planBlockReason = unresolvedMatches
    ? `还有 ${unresolvedMatches} 项需要确认关联，请选择已有游戏或“改为新游戏”。`
    : '';
  const canApply =
    !!plan &&
    !['completed', 'withdrawn'].includes(plan.status) &&
    plan.items.every((i) => !i.blockers.length);
  const manualGames = games
    .filter((g) =>
      [g.display_title, g.canonical_title, g.install_path, ...g.aliases].some((v) =>
        v.toLocaleLowerCase().includes(linkSearch.toLocaleLowerCase()),
      ),
    )
    .sort((a, b) => {
      const recommendations = linkSource ? matches[linkSource] || [] : [];
      const rank = (id: string) => {
        const index = recommendations.findIndex((match) => match.id === id);
        return index < 0 ? recommendations.length : index;
      };
      return rank(a.id) - rank(b.id);
    })
    .slice(0, 50);
  return (
    <section className="import-page batch-import" hidden={!visible} aria-label="批量导入游戏">
      <PageHeader
        icon="import"
        title="导入游戏"
        description="批量导入新游戏，或关联已有游戏更新版本。内部存档可保留，旧版本移入回收站。"
      >
        <div className="header-actions">
          <BrowseButton
            disabled={disabled || !!recoveryIssues.length || view !== 'new'}
            onClick={() => void choose()}
          >
            添加文件夹
          </BrowseButton>
        </div>
      </PageHeader>
      {!!recoveryIssues.length && (
        <section className="import-recovery-notice" role="alert" aria-label="导入记录恢复提示">
          <strong>有 {recoveryIssues.length} 条导入记录需要检查</strong>
          <p>
            游戏文件和原始记录均已保留。导入文件操作、删除及目录关联暂时停用；可查看正常任务。修复记录后重新打开管理器。
          </p>
          <details>
            <summary>查看记录详情</summary>
            <div className="recovery-records">
              {recoveryIssues.map((issue) => (
                <p key={issue.record}>
                  <strong>{issue.record}</strong>
                  <br />
                  {issue.message}
                </p>
              ))}
            </div>
          </details>
          <BrowseButton onClick={() => void action(() => api.openImportRecords())}>
            打开记录文件夹
          </BrowseButton>
        </section>
      )}
      <div className="import-tabs" role="tablist" aria-label="导入分类">
        {(
          [
            ['new', '新的导入', candidates.length],
            ['pending', '待确认', pendingPlans.length],
            ['completed', '导入记录', recordCount],
          ] as const
        ).map(([key, label, total]) => (
          <button
            key={key}
            id={`import-tab-${key}`}
            role="tab"
            tabIndex={view === key ? 0 : -1}
            aria-selected={view === key}
            disabled={disabled}
            aria-controls="import-content"
            onClick={() => switchView(key)}
            onKeyDown={(event) => {
              const keys = ['new', 'pending', 'completed'] as const;
              const index = keys.indexOf(key);
              const next =
                event.key === 'ArrowRight'
                  ? keys[(index + 1) % 3]
                  : event.key === 'ArrowLeft'
                    ? keys[(index + 2) % 3]
                    : event.key === 'Home'
                      ? keys[0]
                      : event.key === 'End'
                        ? keys[2]
                        : null;
              if (next) {
                event.preventDefault();
                switchView(next);
                document.getElementById(`import-tab-${next}`)?.focus();
              }
            }}
          >
            {label}
            {total > 0 ? ` (${total})` : ''}
          </button>
        ))}
      </div>
      <div className="library-toolbar import-toolbar">
        <SearchField
          label="导入搜索"
          placeholder="搜索游戏、来源目录或版本"
          value={search}
          onChange={setSearch}
        />
        <div
          className={`import-sort-slot${view === 'new' ? '' : ' inactive'}`}
          aria-hidden={view !== 'new'}
        >
          <SortField label="导入排序" value={sort} onChange={setSort}>
            <option value="name-asc">文件夹名称 · 正序</option>
            <option value="name-desc">文件夹名称 · 倒序</option>
            <option value="modified-desc">修改时间 · 最新</option>
            <option value="modified-asc">修改时间 · 最早</option>
          </SortField>
        </div>
      </div>
      {view === 'pending' && categoryPlans.length > 1 && (
        <div className="import-plan-list" aria-label="本分类导入记录">
          {categoryPlans.map((p) => (
            <button
              key={p.id}
              disabled={disabled}
              aria-pressed={p.id === planId}
              onClick={() => {
                setPlanId(p.id);
                setSearch('');
                if (scroll.current) scroll.current.scrollTop = 0;
              }}
            >
              {p.items[0]?.selection.title || '导入计划'} · {p.items.length} 个游戏
              {p.status !== 'preview' && p.status !== 'completed' ? ' · 待恢复' : ''}
            </button>
          ))}
        </div>
      )}
      {view === 'new' && !!rows.length && (
        <div className="import-selection-bar">
          <label>
            <input
              type="checkbox"
              disabled={disabled}
              checked={rows.every((c) => selected.has(c.install_path))}
              onChange={(e) =>
                setSelected((current) => {
                  const next = new Set(current);
                  for (const c of rows) {
                    if (e.target.checked) next.add(c.install_path);
                    else next.delete(c.install_path);
                  }
                  return next;
                })
              }
            />
            选择当前搜索结果
          </label>
          <span>
            已选 {selected.size} / {candidates.length} 项
          </span>
        </div>
      )}
      {view === 'completed' ? (
        <ImportHistory
          plans={plans}
          games={games}
          search={search}
          visible={visible}
          disabled={disabled}
          rollbackDisabled={disabled || !!recoveryIssues.length}
          initialOffset={scrollPositions.current['import-history'] ?? 0}
          onScrollOffset={(offset) => {
            scrollPositions.current['import-history'] = offset;
          }}
          onOpenGame={onOpenGame}
          onRollback={(id, index) => {
            setPlanId(id);
            setRollbackIndex(index);
            setConfirmation('rollback');
          }}
        />
      ) : (
        <div
          className="import-scroll"
          id="import-content"
          ref={scroll}
          onScroll={(event) => {
            if (visible) scrollPositions.current[scrollScope] = event.currentTarget.scrollTop;
          }}
          role="tabpanel"
          aria-labelledby={`import-tab-${view}`}
          aria-label="导入游戏列表"
        >
          {!count && (
            <div className="empty">
              <h3>
                {search
                  ? '没有匹配结果'
                  : view === 'pending'
                    ? '暂无待确认的导入计划'
                    : '选择要整理到游戏库的新游戏'}
              </h3>
              <p>
                {view === 'new'
                  ? '可选择单个游戏或装有多个游戏的文件夹，也支持多选。自动识别后确认移动，游戏文件与存档完整保留。'
                  : '在“新的导入”中选择目录并生成导入与更新计划。'}
              </p>
            </div>
          )}
          <div
            style={
              virtual ? { height: virtualizer.getTotalSize(), position: 'relative' } : undefined
            }
          >
            {indices.map(({ index, start, size }) => {
              const item = plan ? visiblePlanItems[index] : null;
              const candidate = !plan ? rows[index] : null;
              const source = item?.selection.source || candidate!.install_path;
              const choice = item?.selection || choices[source];
              if (!choice) return null;
              const existing = games.find((g) => g.id === choice.existing_id);
              const recommendations = matches[source] || [];
              const associations =
                existing && !recommendations.some((match) => match.id === existing.id)
                  ? [
                      {
                        id: existing.id,
                        title: existing.display_title,
                        reason: '手动选择的关联游戏',
                        auto_associate: false,
                      },
                      ...recommendations,
                    ]
                  : recommendations;
              return (
                <div
                  key={source}
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
                  <article className="panel import-item">
                    <div className="candidate-heading">
                      {!plan && (
                        <input
                          type="checkbox"
                          aria-label={`选择 ${choice.title}`}
                          disabled={disabled}
                          checked={selected.has(source)}
                          onChange={(e) =>
                            setSelected((current) => {
                              const next = new Set(current);
                              if (e.target.checked) next.add(source);
                              else next.delete(source);
                              return next;
                            })
                          }
                        />
                      )}
                      <strong>{choice.title}</strong>
                      <span
                        className={`mode${!item && recommendations.length && !choice.existing_id && !choice.new_override ? ' association-pending' : ''}`}
                      >
                        {item
                          ? stateNames[item.state] || item.state
                          : choice.existing_id
                            ? existing &&
                              sameImportVersion(choice.version, existing.current_version)
                              ? '已有同版本'
                              : '更新已有游戏'
                            : matches[source]?.length && !choice.new_override
                              ? '待确认关联'
                              : '新游戏'}
                      </span>
                    </div>
                    <p className="path">来源：{source}</p>
                    {item ? (
                      <>
                        <p className="path">目标：{item.target}</p>
                        <div className="import-selection-bar">
                          <span>
                            {item.update
                              ? `${displayVersion(item.update.old_version)} → ${displayVersion(choice.version)}`
                              : displayVersion(choice.version)}{' '}
                            · {item.files} 个文件 · {bytesText(item.bytes)}
                          </span>
                          <span>
                            {item.update
                              ? '替换已有游戏'
                              : item.cross_volume
                                ? '跨盘：复制完成后移除来源'
                                : '同盘：移动目录'}{' '}
                            ·{' '}
                            <LaunchBadge
                              label={
                                choice.external_player
                                  ? 'QSP 播放器'
                                  : choice.mtool
                                    ? 'MTool'
                                    : '直接启动'
                              }
                            />
                          </span>
                        </div>
                        <p className="path">
                          {choice.external_player ? 'QSP 播放器' : '启动文件'}：
                          {choice.executable || '未配置'}
                        </p>
                        {choice.external_player && (
                          <p className="path">
                            QSP 主游戏文件：{choice.external_player.game_file || '未选择'} ·
                            工作目录：游戏根目录
                          </p>
                        )}
                        {choice.existing_id && !item.update && (
                          <p className="muted">
                            关联已有游戏：{existing?.display_title || choice.existing_id}
                          </p>
                        )}
                        {item.update && (
                          <div className="update-save-review">
                            <strong>
                              {choice.preserve_saves !== false ? '保留内部旧存档' : '使用新包存档'}
                            </strong>
                            <p>
                              {choice.preserve_saves !== false
                                ? '覆盖到同一相对位置，外部存档不操作。'
                                : '不迁移内部旧存档，外部存档不操作。'}
                            </p>
                            {item.update.saves.length ? (
                              item.update.saves.map((save) => (
                                <p className="path" key={save.source}>
                                  {save.configured}
                                  {!save.present ? ' · 不存在，跳过' : ''}
                                </p>
                              ))
                            ) : (
                              <p className="muted">未配置内部存档。</p>
                            )}
                            <p className="muted">
                              旧版移入回收站，清空后无法回退。额外空间约{' '}
                              {bytesText(item.update.required_bytes)}。
                            </p>
                            {item.state === 'completed' && item.update.rollback_available && (
                              <button
                                className="danger danger-solid"
                                disabled={
                                  disabled ||
                                  !!recoveryIssues.length ||
                                  !existing ||
                                  existing.current_version !== choice.version
                                }
                                onClick={() => {
                                  setRollbackIndex(plan!.items.indexOf(item));
                                  setConfirmation('rollback');
                                }}
                              >
                                回退旧版本
                              </button>
                            )}
                            {item.state === 'completed' && !item.update.rollback_available && (
                              <p className="muted">旧版本不在回收站，无法回退。</p>
                            )}
                          </div>
                        )}
                        {item.blockers.map((b) => (
                          <p className="notice" key={b}>
                            {b}
                          </p>
                        ))}
                        {item.error && (
                          <p
                            className={item.state === 'completed' ? 'muted' : 'error'}
                            role={item.state === 'completed' ? 'status' : 'alert'}
                          >
                            {item.error}
                          </p>
                        )}
                      </>
                    ) : (
                      <>
                        <div className="form-grid import-edit-grid">
                          <label>
                            游戏名称
                            <input
                              value={existing?.display_title || choice.title}
                              disabled={disabled || !!existing}
                              onChange={(e) => update(source, { title: e.target.value })}
                            />
                          </label>
                          <label>
                            目标文件夹
                            <input
                              value={existing ? leaf(existing.install_path) : choice.target_name}
                              disabled={disabled || !!existing}
                              onChange={(e) => update(source, { target_name: e.target.value })}
                            />
                          </label>
                          <label>
                            版本
                            <input
                              value={choice.version === 'Unknown' ? '' : choice.version}
                              placeholder="-"
                              disabled={disabled}
                              onChange={(e) => update(source, { version: e.target.value })}
                            />
                          </label>
                          <label>
                            引擎
                            <EngineSelect
                              label="引擎"
                              value={choice.engine}
                              disabled={disabled}
                              onChange={(engine) =>
                                update(
                                  source,
                                  engine === 'QSP'
                                    ? {
                                        engine: 'QSP',
                                        mtool: false,
                                        external_player:
                                          choice.external_player ??
                                          (candidate ? qspConfig(candidate) : null) ??
                                          localQsp(),
                                        executable: choice.external_player
                                          ? choice.executable
                                          : (candidate ? suggestedPlayer(candidate) : null) || '',
                                      }
                                    : { engine, external_player: null },
                                )
                              }
                            />
                          </label>
                        </div>
                        {choice.external_player ? (
                          <QspConfiguration
                            root={source}
                            player={choice.executable || null}
                            config={choice.external_player}
                            detection={candidate?.qsp}
                            disabled={disabled}
                            onChange={(player, config) =>
                              update(source, {
                                executable: player || '',
                                external_player: config,
                                mtool: false,
                              })
                            }
                            onError={onError}
                          />
                        ) : (
                          <div className="import-launch-row">
                            <span className="path">
                              启动文件：{choice.executable || '请手工选择'}
                            </span>
                            <BrowseButton
                              disabled={disabled}
                              onClick={() =>
                                void action(async () => {
                                  const file = await api.chooseLaunchFile(source);
                                  if (file)
                                    update(
                                      source,
                                      isQspFile(file)
                                        ? {
                                            engine: 'QSP',
                                            executable: candidate?.qsp?.recommended_player || '',
                                            external_player: localQsp(file),
                                            mtool: false,
                                          }
                                        : { executable: file },
                                    );
                                })
                              }
                            >
                              更改启动文件
                            </BrowseButton>
                            <label>
                              <input
                                type="checkbox"
                                checked={choice.mtool}
                                disabled={disabled}
                                onChange={(e) => update(source, { mtool: e.target.checked })}
                              />
                              公共 MTool
                            </label>
                          </div>
                        )}
                        {existing && (
                          <details className="full">
                            <summary>更新启动配置</summary>
                            <p className="muted">
                              默认继承旧配置；新版目录或游戏位数改变时，可在此调整。
                            </p>
                            <div className="form-grid">
                              <label>
                                工作目录（相对新版游戏目录）
                                <input
                                  value={choice.working_directory ?? existing.working_directory}
                                  disabled={disabled}
                                  onChange={(e) =>
                                    update(source, { working_directory: e.target.value })
                                  }
                                />
                              </label>
                              {choice.mtool && (
                                <label>
                                  MTool loader（相对公共 MTool 目录）
                                  <input
                                    value={choice.mtool_loader ?? existing.mtool_loader ?? ''}
                                    placeholder="自动识别；留空可改为自动"
                                    disabled={disabled}
                                    onChange={(e) =>
                                      update(source, { mtool_loader: e.target.value })
                                    }
                                  />
                                </label>
                              )}
                            </div>
                            <button
                              disabled={disabled}
                              onClick={() =>
                                update(source, {
                                  working_directory: undefined,
                                  mtool_loader: undefined,
                                })
                              }
                            >
                              恢复继承旧配置
                            </button>
                          </details>
                        )}
                        {!!candidate?.save_paths?.length && (
                          <p className="detected-saves">
                            识别存档：
                            {candidate.save_paths
                              .map((path) => path.replace('<GAME>/', ''))
                              .join('、')}
                          </p>
                        )}
                        <div
                          className={
                            associations.length
                              ? `import-match-recommendations${choice.existing_id ? ' associated' : choice.new_override ? ' overridden' : ' needs-confirmation'}`
                              : undefined
                          }
                        >
                          {!!associations.length && (
                            <>
                              <div className="import-match-heading">
                                <Icon name={choice.existing_id ? 'check' : 'info'} size={16} />
                                <strong>
                                  {choice.existing_id
                                    ? '已关联库中游戏'
                                    : choice.new_override
                                      ? '已有游戏推荐 · 当前按新游戏导入'
                                      : '发现可能已有的游戏，请确认关联'}
                                </strong>
                              </div>
                              {associations.map((match) => (
                                <div className="import-match-option" key={match.id}>
                                  <button
                                    className={
                                      choice.existing_id === match.id
                                        ? 'import-match-selected'
                                        : 'primary'
                                    }
                                    disabled={disabled || choice.existing_id === match.id}
                                    onClick={() =>
                                      update(source, {
                                        existing_id: match.id,
                                        new_override: false,
                                        saves_confirmed: false,
                                        preserve_saves: true,
                                      })
                                    }
                                  >
                                    {choice.existing_id === match.id ? '已关联：' : '关联：'}
                                    {match.title}
                                  </button>
                                  <div className="import-match-explanation">
                                    <span className="import-match-kind">
                                      {choice.existing_id === match.id
                                        ? '关联理由'
                                        : match.auto_associate === false
                                          ? '弱关联'
                                          : '名称匹配'}
                                    </span>
                                    <span>{match.reason}</span>
                                  </div>
                                </div>
                              ))}
                            </>
                          )}
                          <div className="import-association-actions">
                            {(choice.existing_id ||
                              (!!recommendations.length && !choice.new_override)) && (
                              <button
                                disabled={disabled}
                                onClick={() =>
                                  update(source, {
                                    existing_id: null,
                                    new_override: true,
                                    saves_confirmed: false,
                                  })
                                }
                              >
                                改为新游戏
                              </button>
                            )}
                            <button
                              disabled={disabled || !games.length}
                              onClick={() => {
                                setLinkSource(source);
                                setLinkSearch('');
                              }}
                            >
                              关联已有游戏…
                            </button>
                            {existing &&
                              sameImportVersion(choice.version, existing.current_version) && (
                                <button
                                  className="danger"
                                  disabled={disabled || !scanId || !!recoveryIssues.length}
                                  onClick={() => setDuplicateSource(source)}
                                >
                                  删除导入副本…
                                </button>
                              )}
                          </div>
                        </div>
                        {existing && (
                          <div className="update-save-review">
                            <strong>存档处理</strong>
                            <p>
                              {displayVersion(existing.current_version)} →{' '}
                              {displayVersion(choice.version)} · {existing.install_path}
                            </p>
                            {existing.save_paths.length ? (
                              existing.save_paths.map((path) => (
                                <p className="path" key={path}>
                                  {path}
                                </p>
                              ))
                            ) : (
                              <p>未配置存档位置。需要保留存档时，请先在游戏详情中添加位置。</p>
                            )}
                            <div
                              className="import-save-options"
                              role="radiogroup"
                              aria-label="存档处理"
                            >
                              <label className={choice.preserve_saves !== false ? 'selected' : ''}>
                                <input
                                  type="radio"
                                  name={`save-treatment-${source}`}
                                  checked={choice.preserve_saves !== false}
                                  disabled={disabled}
                                  onChange={() => update(source, { preserve_saves: true })}
                                />
                                保留旧存档
                                <span className="muted">推荐</span>
                              </label>
                              <label className={choice.preserve_saves === false ? 'selected' : ''}>
                                <input
                                  type="radio"
                                  name={`save-treatment-${source}`}
                                  checked={choice.preserve_saves === false}
                                  disabled={disabled}
                                  onChange={() => update(source, { preserve_saves: false })}
                                />
                                使用新包存档
                              </label>
                            </div>
                            <p>
                              {choice.preserve_saves !== false
                                ? '内部旧存档覆盖到新版相同位置；外部存档不操作。'
                                : '不迁移内部旧存档，使用新包存档；外部存档不操作。'}
                            </p>
                            <p className="muted">旧版移入回收站，清空后无法回退。</p>
                          </div>
                        )}
                        <div className="import-row-meta">
                          <span className="muted">
                            修改时间：
                            {formatTime(
                              candidate?.directory_modified_ms
                                ? new Date(candidate.directory_modified_ms).toISOString()
                                : null,
                            )}
                          </span>
                        </div>
                        {!!candidate?.warnings.length && (
                          <details className="analysis">
                            <summary>分析说明</summary>
                            {candidate.warnings.map((warning) => (
                              <p key={warning}>{warning}</p>
                            ))}
                          </details>
                        )}
                      </>
                    )}
                  </article>
                </div>
              );
            })}
          </div>
        </div>
      )}
      <div className="import-bottom">
        <div
          className={`import-progress${!busy && view === 'new' ? ' import-status' : ''}`}
          role="status"
        >
          {busy ? (
            <ImportProgress page={job.page} />
          ) : (
            <span>
              {notice ||
                (view === 'completed'
                  ? `共 ${recordCount} 条导入记录 · 最新在前`
                  : plan
                    ? `导入与更新计划 · ${plan.items.length} 项 · ${bytesText(plan.items.reduce((n, i) => n + i.bytes, 0))}`
                    : `目标：${settings.game_root || '请先在设置中选择游戏库目录'}`)}
            </span>
          )}
          {!busy && view === 'new' && (
            <span
              className="import-plan-block-reason"
              id="import-plan-block-reason"
              aria-hidden={!planBlockReason}
              title={planBlockReason || undefined}
            >
              {planBlockReason || '\u00a0'}
            </span>
          )}
        </div>
        <div className="header-actions">
          {busy ? (
            <button
              disabled={matching || job.page?.status === 'cancel_requested' || !jobId}
              onClick={() =>
                void action(async () => {
                  await api.cancel(jobId!);
                  job.markCancelling();
                })
              }
            >
              {job.page?.status === 'cancel_requested' ? '正在停止…' : '取消任务'}
            </button>
          ) : view === 'completed' ? (
            <button onClick={() => switchView('new')}>继续导入</button>
          ) : plan ? (
            <>
              {plan.items.every((item) => item.state === 'pending') ? (
                <button
                  disabled={locked || !!recoveryIssues.length}
                  onClick={() =>
                    void action(async () => {
                      await api.discardImportPlan(plan.id);
                      setPlans((current) => current.filter((p) => p.id !== plan.id));
                      setPlanId(null);
                      setView('new');
                    })
                  }
                >
                  返回调整
                </button>
              ) : !['completed', 'withdrawn'].includes(plan.status) &&
                !plan.items.some((item) => item.state.startsWith('rollback_')) ? (
                <button
                  disabled={locked || !!recoveryIssues.length}
                  onClick={() => setConfirmation('withdraw')}
                >
                  撤回未完成项
                </button>
              ) : (
                <button
                  onClick={() => {
                    setPlanId(null);
                    setView('new');
                    setCandidates([]);
                    setChoices({});
                    setSelected(new Set());
                  }}
                >
                  开始下一批
                </button>
              )}
              {canApply && (
                <button
                  className="primary"
                  disabled={locked || !!recoveryIssues.length}
                  onClick={() => setConfirmation('move')}
                >
                  <Icon name="import" size={16} />
                  {plan.status === 'preview' ? '确认执行计划' : '继续未完成项'}
                </button>
              )}
            </>
          ) : view === 'new' ? (
            <>
              <button
                disabled={disabled || !candidates.length}
                onClick={() => setConfirmation('clear')}
              >
                清空分析结果
              </button>
              <button
                className="primary import-generate-plan"
                disabled={
                  disabled || !scanId || !selected.size || !settings.game_root || !!planBlockReason
                }
                aria-describedby={planBlockReason ? 'import-plan-block-reason' : undefined}
                title={planBlockReason || undefined}
                onClick={() =>
                  void action(async () =>
                    setJobId(
                      await api.importPlan(
                        scanId!,
                        validSelections.map((selection) => ({
                          ...selection,
                          saves_confirmed: !!selection.existing_id,
                        })),
                      ),
                    ),
                  )
                }
              >
                <Icon name="list" size={16} />
                生成导入与更新计划（{selected.size}）
              </button>
            </>
          ) : null}
        </div>
      </div>
      {duplicateSource && scanId && choices[duplicateSource]?.existing_id && (
        <ImportDuplicateDialog
          key={duplicateSource}
          scanId={scanId}
          source={duplicateSource}
          existingId={choices[duplicateSource].existing_id!}
          version={choices[duplicateSource].version}
          onClose={() => setDuplicateSource(null)}
          onBusy={setRecycling}
          onRecycled={() => {
            setCandidates((current) =>
              current.filter((candidate) => candidate.install_path !== duplicateSource),
            );
            setSelected(
              (current) => new Set([...current].filter((source) => source !== duplicateSource)),
            );
            setChoices((current) => {
              const next = { ...current };
              delete next[duplicateSource];
              return next;
            });
            setMatches((current) => {
              const next = { ...current };
              delete next[duplicateSource];
              return next;
            });
            setDuplicateSource(null);
            setNotice('导入副本已移入回收站，库中游戏保留。');
          }}
        />
      )}
      {sourceReview && (
        <Modal
          variant="confirm"
          showClose={false}
          title="确认文件夹范围"
          onClose={() => {
            if (!starting) setSourceReview(null);
          }}
        >
          <p>这些文件夹可能包含包装层或共用资源，请选择导入范围。这里只分析文件，不会移动。</p>
          {sourceReview.choices.map((choice) => (
            <label className="source-scope" key={choice.root}>
              <strong>{leaf(choice.root)}</strong>
              <span className="path">{choice.root}</span>
              <select
                disabled={starting}
                aria-label={`导入范围：${choice.root}`}
                value={sourceScopes[choice.root] || ''}
                onChange={(event) =>
                  setSourceScopes((current) => ({
                    ...current,
                    [choice.root]: event.target.value as 'whole' | 'children',
                  }))
                }
              >
                <option value="" disabled>
                  请选择导入范围
                </option>
                <option value="whole">整个文件夹作为一个游戏</option>
                <option value="children">分别导入里面的 {choice.children.length} 个游戏</option>
              </select>
              <details>
                <summary>识别到的游戏文件夹</summary>
                {choice.children.map((child) => (
                  <p className="path" key={child}>
                    {leaf(child)}
                  </p>
                ))}
              </details>
            </label>
          ))}
          {sourceReview.warnings.map((warning) => (
            <p className="muted" key={warning}>
              {warning}
            </p>
          ))}
          {sourceError && (
            <p className="error" role="alert">
              {sourceError}
            </p>
          )}
          <div className="confirmation-actions">
            <button disabled={starting} onClick={() => setSourceReview(null)}>
              取消
            </button>
            <button
              className="primary"
              disabled={
                starting ||
                locked ||
                sourceReview.choices.some((choice) => !sourceScopes[choice.root])
              }
              onClick={() =>
                void action(async () => {
                  const sources = [
                    ...sourceReview.sources,
                    ...sourceReview.choices.flatMap((choice) =>
                      sourceScopes[choice.root] === 'whole' ? [choice.root] : choice.children,
                    ),
                  ];
                  setSourceError('');
                  try {
                    await analyzeSources(sources);
                  } catch (error) {
                    setSourceError(String(error));
                    return;
                  }
                  setSourceReview(null);
                })
              }
            >
              开始分析
            </button>
          </div>
        </Modal>
      )}
      {confirmation && (
        <Modal
          variant="confirm"
          showClose={false}
          title={
            confirmation === 'move'
              ? '确认导入与更新'
              : confirmation === 'rollback'
                ? '确认回退旧版本'
                : confirmation === 'withdraw'
                  ? '撤回未登记项'
                  : '清空分析结果'
          }
          onClose={() => setConfirmation(null)}
        >
          <p className="confirmation-copy">
            {confirmation === 'move'
              ? '按计划导入或替换游戏目录。\n内部存档按所选方式处理，旧版本移入回收站。'
              : confirmation === 'rollback'
                ? '从回收站恢复旧版本，当前版本移入回收站。\n当前内部存档带回旧版；回收站已清空时无法回退。'
                : confirmation === 'withdraw'
                  ? '恢复未登记项，已入库游戏保留。\n仅移除已验证的暂存副本，变化的文件保留。'
                  : '仅清空本次分析与选择，不处理任何游戏文件。'}
          </p>
          <div className="confirmation-actions">
            <button onClick={() => setConfirmation(null)}>取消</button>
            <button
              className={confirmation === 'rollback' ? 'danger danger-solid' : 'primary'}
              onClick={() => {
                const mode = confirmation;
                setConfirmation(null);
                if (mode === 'clear') {
                  setCandidates([]);
                  setChoices({});
                  setSelected(new Set());
                  setScanId(null);
                } else if (plan && mode === 'rollback')
                  void action(async () =>
                    setJobId(await api.rollbackVersion(plan.id, rollbackIndex)),
                  );
                else if (plan)
                  void action(async () =>
                    setJobId(await api.importApply(plan.id, mode === 'withdraw')),
                  );
              }}
            >
              {confirmation === 'rollback' ? '确认回退' : '确认'}
            </button>
          </div>
        </Modal>
      )}
      {linkSource && (
        <Modal variant="confirm" title="关联已有游戏" onClose={() => setLinkSource(null)}>
          <p>选择要更新的游戏。编号、名称与历史保留；生成计划后再确认执行。</p>
          <SearchField
            label="关联游戏搜索"
            placeholder="搜索名称、别名或路径"
            value={linkSearch}
            onChange={setLinkSearch}
          />
          <div className="import-link-list">
            {manualGames.map((g) => (
              <button
                key={g.id}
                onClick={() => {
                  update(linkSource, {
                    existing_id: g.id,
                    new_override: false,
                    saves_confirmed: false,
                    preserve_saves: true,
                  });
                  setLinkSource(null);
                }}
              >
                <strong title={g.display_title}>{g.display_title}</strong>
                <span className="path" title={g.install_path}>
                  {displayVersion(g.current_version)} · {g.install_path}
                </span>
              </button>
            ))}
          </div>
          <p className="muted">最多显示 50 项，可搜索缩小范围。</p>
          <button onClick={() => setLinkSource(null)}>关闭</button>
        </Modal>
      )}
    </section>
  );
}
