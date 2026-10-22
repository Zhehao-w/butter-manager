import { LaunchArgumentsField } from './LaunchArgumentsField';
import { launchArgumentsError } from './launchArguments';
import {
  Children,
  isValidElement,
  useEffect,
  useId,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
} from 'react';
import type { ReactNode } from 'react';
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
import { ImportHistory, isImportRecord, MtoolCleanupStatus } from './ImportHistory';
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
const capsuleTitle = (title: string) => {
  const characters = Array.from(title);
  return characters.length > 32 ? characters.slice(0, 32).join('') + '…' : title;
};

// Keep form controls mounted so folding a card never discards edits.
function ImportDisclosure({
  title,
  attention = false,
  preview,
  children,
}: {
  title: string;
  attention?: boolean;
  preview?: ReactNode;
  children: ReactNode;
}) {
  const [open, setOpen] = useState(attention);
  const contentId = useId();
  useEffect(() => {
    if (attention) setOpen(true);
  }, [attention]);
  return (
    <div className="import-disclosure">
      {preview}
      <button
        className="import-disclosure-toggle"
        aria-expanded={open}
        aria-controls={contentId}
        onClick={() => setOpen((value) => !value)}
      >
        <Icon name="next" size={14} />
        {title === '展开设置' && open ? '收起设置' : title}
        {attention && <span className="import-attention-dot" aria-hidden="true" />}
      </button>
      <div id={contentId} hidden={!open} className="import-disclosure-content">
        {children}
      </div>
    </div>
  );
}

type ImportSettingSectionProps = { title: string; attention?: boolean; children: ReactNode };
function ImportSettingSection({ children }: ImportSettingSectionProps) {
  return <>{children}</>;
}

function ImportCardIssues({ issues, linked }: { issues: string[]; linked: boolean }) {
  const hadIssues = useRef(false);
  if (issues.length) hadIssues.current = true;
  if (!hadIssues.current) return null;
  return (
    <div
      className="import-card-issues"
      role={issues.length ? 'alert' : 'status'}
      style={{ minHeight: Math.max(1, issues.length) * 21 }}
    >
      {issues.length ? (
        issues.map((issue) => (
          <p key={issue}>
            <Icon name="info" size={14} />
            {issue}
          </p>
        ))
      ) : (
        <p className="muted">
          <Icon name="check" size={14} />
          {linked ? '已关联所选游戏。' : '已更新当前选择。'}
        </p>
      )}
    </div>
  );
}

function ImportSettings({
  preview,
  attention,
  children,
}: {
  preview: ReactNode;
  attention: boolean;
  children: ReactNode;
}) {
  const sections = Children.toArray(children).filter(isValidElement<ImportSettingSectionProps>);
  const [active, setActive] = useState(
    sections.find((section) => section.props.attention)?.props.title || sections[0]?.props.title,
  );
  const attentionKey = sections
    .filter((section) => section.props.attention)
    .map((section) => section.props.title)
    .join('|');
  const id = useId();
  useEffect(() => {
    if (attentionKey) setActive(attentionKey.split('|')[0]);
  }, [attentionKey]);
  const selected = sections.some((section) => section.props.title === active)
    ? active
    : sections[0]?.props.title;
  return (
    <ImportDisclosure title="展开设置" preview={preview} attention={attention}>
      <div className="import-settings-tabs" role="tablist" aria-label="游戏导入设置">
        {sections.map((section, index) => (
          <button
            key={section.props.title}
            id={`${id}-tab-${index}`}
            role="tab"
            aria-selected={section.props.title === selected}
            aria-controls={`${id}-panel-${index}`}
            tabIndex={section.props.title === selected ? 0 : -1}
            onClick={() => setActive(section.props.title)}
            onKeyDown={(event) => {
              const next =
                event.key === 'ArrowRight'
                  ? (index + 1) % sections.length
                  : event.key === 'ArrowLeft'
                    ? (index + sections.length - 1) % sections.length
                    : event.key === 'Home'
                      ? 0
                      : event.key === 'End'
                        ? sections.length - 1
                        : null;
              if (next == null) return;
              event.preventDefault();
              setActive(sections[next].props.title);
              document.getElementById(`${id}-tab-${next}`)?.focus();
            }}
          >
            {section.props.title}
            {section.props.attention && (
              <span className="import-attention-dot" aria-hidden="true" />
            )}
          </button>
        ))}
      </div>
      {sections.map((section, index) => (
        <div
          key={section.props.title}
          id={`${id}-panel-${index}`}
          role="tabpanel"
          aria-labelledby={`${id}-tab-${index}`}
          hidden={section.props.title !== selected}
          className="import-settings-panel"
        >
          {section.props.children}
        </div>
      ))}
    </ImportDisclosure>
  );
}

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
  const plannedSources = useMemo(
    () =>
      new Set(
        plans
          .filter((p) => !['completed', 'withdrawn'].includes(p.status))
          .flatMap((p) => p.items.map((item) => item.selection.source)),
      ),
    [plans],
  );
  const newCandidates = useMemo(
    () => candidates.filter((candidate) => !plannedSources.has(candidate.install_path)),
    [candidates, plannedSources],
  );
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
              mtool:
                !!c.bundled_mtool?.some((bundle) => bundle.safe) || (!c.qsp && !!c.mtool_detected),
              ...(c.bundled_mtool?.some((bundle) => bundle.safe)
                ? { cleanup_mtool: true, external_player: null }
                : {}),
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
            ...changes.filter((c) => !choices[c.install_path]).map((c) => c.install_path),
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
                  next[source] = {
                    ...next[source],
                    existing_id: suggestions[0].id,
                    remember_association: true,
                  };
              return next;
            });
          })
          .catch((e) => callbacks.current.onError(String(e)))
          .finally(() => setMatching(false));
      } else {
        void api
          .importPlans()
          .then((values) => {
            setPlans(values);
            if (result.kind === 'import_plan' && result.status === 'completed') {
              const generated = values.find((p) => p.id === result.id);
              if (generated) {
                const sources = new Set(generated.items.map((item) => item.selection.source));
                setSelected(
                  (current) => new Set([...current].filter((source) => !sources.has(source))),
                );
              }
              setSearch('');
              setPlanId(result.id);
              setView('pending');
            } else if (result.kind === 'import_apply' || result.kind === 'import_rollback') {
              const applied = values.find((p) => p.id === planId);
              if (result.kind === 'import_apply' && applied) {
                const completedSources = new Set(
                  applied.items
                    .filter((item) => item.state === 'completed')
                    .map((item) => item.selection.source),
                );
                if (completedSources.size) {
                  setCandidates((current) =>
                    current.filter((c) => !completedSources.has(c.install_path)),
                  );
                  setChoices((current) =>
                    Object.fromEntries(
                      Object.entries(current).filter(([source]) => !completedSources.has(source)),
                    ),
                  );
                  setMatches((current) =>
                    Object.fromEntries(
                      Object.entries(current).filter(([source]) => !completedSources.has(source)),
                    ),
                  );
                  setSelected(
                    (current) =>
                      new Set([...current].filter((source) => !completedSources.has(source))),
                  );
                }
              }
              if (applied?.status === 'completed') setView('completed');
              else if (applied) setView('pending');
              if (
                result.kind === 'import_apply' &&
                result.status === 'completed' &&
                applied?.items.some(
                  (item) =>
                    item.state === 'completed' &&
                    item.mtool_cleanup?.targets.some((target) => target.state !== 'recycled'),
                )
              )
                setNotice(
                  '导入操作已完成，部分随包 MTool 尚未清理。请查看导入记录中的原因与未清理内容。',
                );
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
    const all = [...new Set([...newCandidates.map((c) => c.install_path), ...sources])];
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
          ? {
              working_directory: undefined,
              launch_arguments: undefined,
              mtool_loader: undefined,
              remember_association: !!change.existing_id && change.new_override !== true,
            }
          : {}),
        ...change,
      },
    }));
  }
  const rows = useMemo(() => {
    const key = search.trim().toLocaleLowerCase();
    return newCandidates
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
  }, [newCandidates, choices, search, sort]);
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
      estimateSize: () => 190,
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
  const pendingCount = pendingPlans.reduce(
    (count, plan) =>
      count +
      plan.items.filter((item) => !['completed', 'withdrawn', 'rolled_back'].includes(item.state))
        .length,
    0,
  );
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
  const missingLaunchFiles = validSelections.filter((selection) =>
    selection.external_player ? !selection.external_player.game_file : !selection.executable,
  ).length;
  const invalidArguments = validSelections
    .map((selection) =>
      launchArgumentsError(
        selection.launch_arguments ??
          games.find((game) => game.id === selection.existing_id)?.launch_arguments ??
          '',
      ),
    )
    .find(Boolean);
  const planBlockReason = unresolvedMatches
    ? `还有 ${unresolvedMatches} 项需要确认关联，请选择已有游戏或“改为新游戏”。`
    : missingLaunchFiles
      ? `还有 ${missingLaunchFiles} 项需要选择启动文件。`
      : invalidArguments || '';
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
            className="primary"
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
      <div className="library-toolbar import-toolbar">
        <SearchField
          label="导入搜索"
          placeholder="搜索游戏、来源目录或版本"
          value={search}
          onChange={setSearch}
        />
        <div className="import-tabs" role="tablist" aria-label="导入分类">
          {(
            [
              ['new', '新的导入', newCandidates.length],
              ['pending', '待确认', pendingCount],
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
            已选 {selected.size} / {newCandidates.length} 项
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
          onRetryCleanup={(id) =>
            void action(async () => {
              setPlanId(id);
              setJobId(await api.importApply(id));
            })
          }
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
                        remembered: false,
                      },
                      ...recommendations,
                    ]
                  : recommendations;
              const associationIssue =
                !item && !!recommendations.length && !choice.existing_id && !choice.new_override;
              const saveIssue =
                !item &&
                !!existing &&
                !existing.save_paths.length &&
                choice.preserve_saves !== false;
              const toolLaunch = !!choice.mtool || !!choice.cleanup_mtool;
              const launchArguments =
                choice.launch_arguments ??
                item?.update?.old?.launch_arguments ??
                existing?.launch_arguments ??
                '';
              const hasLaunchArguments =
                !toolLaunch &&
                !choice.external_player &&
                /\.exe$/i.test(choice.executable) &&
                !!launchArguments.trim();
              const bundledLaunch =
                !!choice.cleanup_mtool &&
                !!candidate?.bundled_mtool?.some((bundle) => {
                  const executable = choice.executable.replaceAll('\\', '/').toLocaleLowerCase();
                  return (
                    executable.startsWith(bundle.folder.toLocaleLowerCase() + '/') ||
                    bundle.bats.some((bat) => executable === bat.toLocaleLowerCase())
                  );
                });
              const mtoolIssue =
                !item &&
                toolLaunch &&
                (!settings.mtool_root || !/\.exe$/i.test(choice.executable) || bundledLaunch);
              const issues = item
                ? item.blockers
                : [
                    ...(associationIssue
                      ? ['发现可能已有的游戏，请选择关联或明确按新游戏导入。']
                      : []),
                    ...(saveIssue
                      ? ['旧游戏未配置存档位置：请先在游戏详情添加位置，或选择使用新包存档。']
                      : []),
                    ...(toolLaunch && !settings.mtool_root
                      ? ['请先配置公共 MTool，或取消公共 MTool 和随包清理。']
                      : []),
                    ...(toolLaunch && !/\.exe$/i.test(choice.executable)
                      ? ['MTool 需要有效的游戏 EXE，请调整启动文件。']
                      : []),
                    ...(bundledLaunch ? ['启动文件位于待清理的工具目录中，请改为游戏 EXE。'] : []),
                    ...(choice.external_player
                      ? !choice.external_player.game_file
                        ? ['请选择 QSP 主游戏文件。']
                        : []
                      : !choice.executable
                        ? ['请手工选择启动文件。']
                        : []),
                    ...(!choice.title.trim() ? ['游戏名称不能为空。'] : []),
                    ...(!choice.target_name.trim() ? ['目标文件夹不能为空。'] : []),
                  ];
              const summary = (
                <div className="import-compact-facts">
                  <span>
                    <span className="muted">版本</span> {displayVersion(choice.version)}
                  </span>
                  <span>
                    <span className="muted">引擎</span>{' '}
                    {choice.engine === 'Unknown' ? '待确认' : choice.engine}
                  </span>
                  <span title={choice.executable}>
                    <span className="muted">{choice.external_player ? '播放器' : '启动文件'}</span>{' '}
                    {choice.executable || '未配置'}
                  </span>
                  <span title={choice.target_name}>
                    <span className="muted">目标文件夹</span> {choice.target_name}
                  </span>
                </div>
              );
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
                      <strong title={choice.title}>{choice.title}</strong>
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
                    <div className="import-source-row">
                      <p className="path import-source-path" title={source}>
                        来源：{source}
                      </p>
                      <div className="import-folder-actions">
                        <BrowseButton
                          disabled={disabled}
                          title={source}
                          onClick={() => void action(() => api.openDirectory(source))}
                        >
                          打开源目录
                        </BrowseButton>
                      </div>
                    </div>
                    <div className="import-choice-summary" aria-label="当前导入配置">
                      <span
                        className={
                          'mode' +
                          (choice.existing_id ? ' import-linked-game' : '') +
                          (associationIssue ? ' association-pending' : '')
                        }
                        title={existing?.display_title}
                      >
                        {choice.existing_id && <Icon name="check" size={14} />}
                        <span className="import-choice-text">
                          {choice.existing_id
                            ? `关联已有游戏：${capsuleTitle(existing?.display_title || choice.existing_id)}`
                            : associationIssue
                              ? '关联待确认'
                              : '按新游戏导入'}
                        </span>
                      </span>
                      <span className="mode">
                        {existing || item?.update
                          ? choice.preserve_saves !== false
                            ? '保留旧存档'
                            : '使用新包存档'
                          : '使用随包存档'}
                      </span>
                      {choice.remember_association && choice.existing_id && (
                        <span className="mode">记住关联</span>
                      )}
                      {!!(candidate?.bundled_mtool?.length || item?.mtool_cleanup) && (
                        <span className={'mode' + (mtoolIssue ? ' association-pending' : '')}>
                          {choice.cleanup_mtool ? '随包 MTool：回收清理' : '随包 MTool：保留'}
                        </span>
                      )}
                      {toolLaunch && <span className="mode">公共 MTool 启动</span>}
                      {hasLaunchArguments && (
                        <span className="mode" title={`启动参数：${launchArguments}`}>
                          <Icon name="play" size={12} />
                          含启动参数
                        </span>
                      )}
                    </div>
                    <ImportCardIssues issues={issues} linked={!!choice.existing_id} />
                    {item?.error && (
                      <p
                        className={item.state === 'completed' ? 'muted' : 'error'}
                        role={item.state === 'completed' ? 'status' : 'alert'}
                      >
                        {item.error}
                      </p>
                    )}
                    {item ? (
                      <ImportSettings attention={!!issues.length || !!item.error} preview={summary}>
                        <ImportSettingSection title="基本信息">
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
                          {hasLaunchArguments && (
                            <p className="path">启动参数：{launchArguments}</p>
                          )}
                          {choice.existing_id && !item.update && (
                            <p className="muted">
                              关联已有游戏：{existing?.display_title || choice.existing_id}
                            </p>
                          )}
                        </ImportSettingSection>
                        {item.update && (
                          <ImportSettingSection title="存档处理">
                            <div className="update-save-review">
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
                                  className="warning"
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
                          </ImportSettingSection>
                        )}
                        {item.mtool_cleanup && (
                          <ImportSettingSection
                            title="随包 MTool"
                            attention={item.mtool_cleanup.targets.some(
                              (target) => target.state === 'failed',
                            )}
                          >
                            <MtoolCleanupStatus item={item} />
                          </ImportSettingSection>
                        )}
                      </ImportSettings>
                    ) : (
                      <ImportSettings attention={!!issues.length} preview={summary}>
                        <ImportSettingSection
                          title="基本信息"
                          attention={!choice.title.trim() || !choice.target_name.trim()}
                        >
                          <div className="form-grid import-edit-grid">
                            <label>
                              游戏名称
                              <input
                                value={choice.title}
                                disabled={disabled}
                                onChange={(e) => update(source, { title: e.target.value })}
                              />
                            </label>
                            <label>
                              目标文件夹
                              <input
                                value={choice.target_name}
                                disabled={disabled}
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
                        </ImportSettingSection>
                        <ImportSettingSection
                          title="启动配置"
                          attention={
                            mtoolIssue ||
                            (choice.external_player
                              ? !choice.external_player.game_file
                              : !choice.executable)
                          }
                        >
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
                              <div className="import-launch-file">
                                <strong>启动文件</strong>
                                <span className="path" title={choice.executable}>
                                  {choice.executable || '请手工选择'}
                                </span>
                              </div>
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
                                  checked={!!choice.cleanup_mtool || choice.mtool}
                                  disabled={disabled || !!choice.cleanup_mtool}
                                  onChange={(e) => update(source, { mtool: e.target.checked })}
                                />
                                公共 MTool
                              </label>
                            </div>
                          )}
                          {!choice.mtool &&
                            !choice.external_player &&
                            /\.exe$/i.test(choice.executable) && (
                              <div className="form-grid">
                                <LaunchArgumentsField
                                  value={
                                    choice.launch_arguments ?? existing?.launch_arguments ?? ''
                                  }
                                  onChange={(value) => update(source, { launch_arguments: value })}
                                  disabled={disabled}
                                />
                              </div>
                            )}
                          {existing && (
                            <details className="import-launch-update">
                              <summary>更新启动配置</summary>
                              <p className="muted">
                                默认继承旧配置；新版目录或游戏位数改变时，可在此调整。
                              </p>
                              <div className="form-grid">
                                <div className="import-working-directory">
                                  <label
                                    htmlFor={`import-working-directory-${encodeURIComponent(source)}`}
                                  >
                                    工作目录（相对新版游戏目录）
                                  </label>
                                  <div className="import-working-directory-input">
                                    <input
                                      id={`import-working-directory-${encodeURIComponent(source)}`}
                                      value={choice.working_directory ?? existing.working_directory}
                                      disabled={disabled}
                                      onChange={(e) =>
                                        update(source, { working_directory: e.target.value })
                                      }
                                    />
                                    <BrowseButton
                                      disabled={disabled}
                                      onClick={() =>
                                        void action(async () => {
                                          const directory = await api.chooseWorkingDirectory(
                                            source,
                                            choice.working_directory ?? existing.working_directory,
                                          );
                                          if (directory !== null)
                                            update(source, { working_directory: directory });
                                        })
                                      }
                                    >
                                      浏览文件夹…
                                    </BrowseButton>
                                  </div>
                                </div>
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
                                    launch_arguments: undefined,
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
                        </ImportSettingSection>
                        <ImportSettingSection
                          title="游戏关联"
                          attention={associationIssue || saveIssue}
                        >
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
                                  {choice.existing_id && !choice.new_override && (
                                    <label
                                      className="check import-memory-choice"
                                      title="导入提交成功后保存；下次优先推荐，仍需确认。不添加名称别名。"
                                    >
                                      <input
                                        type="checkbox"
                                        aria-label="记住本次关联"
                                        disabled={disabled}
                                        checked={!!choice.remember_association}
                                        onChange={(event) =>
                                          update(source, {
                                            remember_association: event.target.checked,
                                          })
                                        }
                                      />
                                      记住关联
                                      <Icon name="info" size={13} />
                                    </label>
                                  )}
                                </div>
                                {associations.map((match) => (
                                  <div className="import-match-option" key={match.id}>
                                    <div className="import-match-game-row">
                                      <button
                                        aria-label={
                                          (choice.existing_id === match.id
                                            ? '已关联：'
                                            : '关联：') + match.title
                                        }
                                        className={
                                          choice.existing_id === match.id
                                            ? 'import-match-selected'
                                            : 'import-match-candidate'
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
                                        <span
                                          className="import-association-title"
                                          title={match.title}
                                        >
                                          {choice.existing_id === match.id ? '已关联：' : '关联：'}
                                          {match.title}
                                        </span>
                                      </button>
                                      {choice.existing_id === match.id && (
                                        <BrowseButton
                                          disabled={disabled || !existing}
                                          title={existing?.install_path}
                                          onClick={() =>
                                            void action(() => api.openFolder(match.id))
                                          }
                                        >
                                          打开关联目录
                                        </BrowseButton>
                                      )}
                                    </div>
                                    <div className="import-match-explanation">
                                      <span className="import-match-kind">
                                        {match.remembered
                                          ? '关联记忆'
                                          : choice.existing_id === match.id
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
                            <section
                              className="import-associated-saves"
                              aria-label="关联游戏存档处理"
                            >
                              <strong>存档处理</strong>
                              <div className="update-save-review">
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
                                  <label
                                    className={choice.preserve_saves !== false ? 'selected' : ''}
                                  >
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
                                  <label
                                    className={choice.preserve_saves === false ? 'selected' : ''}
                                  >
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
                            </section>
                          )}
                        </ImportSettingSection>
                        {!!candidate?.bundled_mtool?.length && (
                          <ImportSettingSection title="随包 MTool" attention={mtoolIssue}>
                            <div className="import-mtool-review">
                              {candidate.bundled_mtool.map((bundle) => (
                                <div key={bundle.folder}>
                                  <span className="path">
                                    {bundle.folder}/
                                    {bundle.bats.length ? ` · ${bundle.bats.join('、')}` : ''}
                                  </span>
                                  {!bundle.safe && <p className="muted">{bundle.reason}</p>}
                                </div>
                              ))}
                              {candidate.bundled_mtool.some((bundle) => bundle.safe) && (
                                <label>
                                  <input
                                    type="checkbox"
                                    checked={!!choice.cleanup_mtool}
                                    disabled={disabled}
                                    onChange={(event) =>
                                      update(source, {
                                        cleanup_mtool: event.target.checked,
                                        ...(event.target.checked
                                          ? { mtool: true, external_player: null }
                                          : {}),
                                      })
                                    }
                                  />
                                  导入成功后清理随包 MTool
                                </label>
                              )}
                              <p className="muted">
                                回收整个已确认的一级工具目录（包含工具自己的记录和备份）及关联
                                BAT。勾选后使用公共 MTool 启动，请确认游戏 EXE 与公共工具配置。
                              </p>
                              {!!choice.cleanup_mtool && !settings.mtool_root && (
                                <p className="notice">
                                  请先在 MTool 设置中配置公共工具，或取消清理。
                                </p>
                              )}
                            </div>
                          </ImportSettingSection>
                        )}
                      </ImportSettings>
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
                      const sources = new Set(plan.items.map((item) => item.selection.source));
                      const missing = plan.items.filter(
                        (item) => !candidates.some((c) => c.install_path === item.selection.source),
                      );
                      setChoices((current) => ({
                        ...Object.fromEntries(
                          plan.items.map((item) => [item.selection.source, item.selection]),
                        ),
                        ...current,
                      }));
                      setSelected((current) => new Set([...current, ...sources]));
                      setPlans((current) => current.filter((p) => p.id !== plan.id));
                      setPlanId(null);
                      setSearch('');
                      setView('new');
                      if (missing.length)
                        await analyzeSources(missing.map((item) => item.selection.source));
                    })
                  }
                >
                  返回调整
                </button>
              ) : !['completed', 'withdrawn'].includes(plan.status) &&
                !plan.items.some((item) => item.state.startsWith('rollback_')) ? (
                <button
                  className="warning"
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
                disabled={disabled || !newCandidates.length}
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
          className="modal-source-scope"
          showClose={false}
          title="确认文件夹范围"
          onClose={() => {
            if (!starting) setSourceReview(null);
          }}
        >
          <p className="source-scope-hint">请选择导入范围。此步仅分析，不移动文件。</p>
          {sourceReview.choices.map((choice) => (
            <div className="source-scope" key={choice.root}>
              <strong>{leaf(choice.root)}</strong>
              <span className="path">{choice.root}</span>
              <div
                className="source-scope-options"
                role="radiogroup"
                aria-label={`导入范围：${choice.root}`}
              >
                {(['whole', 'children'] as const).map((scope) => (
                  <label
                    className={`source-scope-option${sourceScopes[choice.root] === scope ? ' selected' : ''}`}
                    key={scope}
                  >
                    <input
                      type="radio"
                      name={`source-scope-${choice.root}`}
                      value={scope}
                      checked={sourceScopes[choice.root] === scope}
                      disabled={starting}
                      onChange={() =>
                        setSourceScopes((current) => ({ ...current, [choice.root]: scope }))
                      }
                    />
                    <Icon name={scope === 'whole' ? 'folder' : 'grid'} size={18} />
                    <span>
                      {scope === 'whole'
                        ? '整个文件夹作为一个游戏'
                        : `分别导入里面的 ${choice.children.length} 个游戏`}
                    </span>
                  </label>
                ))}
              </div>
              <details>
                <summary>识别到的游戏文件夹</summary>
                {choice.children.map((child) => (
                  <p className="path" key={child}>
                    {leaf(child)}
                  </p>
                ))}
              </details>
            </div>
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
              className={confirmation === 'move' ? 'primary' : 'warning'}
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
        <Modal
          variant="confirm"
          className="modal-import-link"
          title="关联已有游戏"
          onClose={() => setLinkSource(null)}
        >
          <p>选择要更新的游戏。编号与历史保留，名称和目录默认采用新版本，可在生成计划前修改。</p>
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
          <p className="muted import-link-hint">最多显示 50 项，可搜索缩小范围。</p>
        </Modal>
      )}
    </section>
  );
}
