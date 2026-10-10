import { useEffect, useId, useLayoutEffect, useMemo, useRef, useState } from 'react';
import type { CSSProperties, RefObject } from 'react';
import { isTauri } from '@tauri-apps/api/core';
import { useVirtualizer } from '@tanstack/react-virtual';
import { useStableVirtualizer } from './useStableVirtualizer';
import { api } from './api';
import { useNotifications, NotificationToast } from './notifications';
import {
  AppearancePicker,
  appearanceIcons,
  appearanceIllustrations,
  defaultAppearance,
} from './appearance';
import {
  Icon,
  Modal,
  GameMark,
  engineTone,
  CardTitle,
  PageHeader,
  CodeBlock,
  SearchField,
  SortField,
  BrowseButton,
} from './ui';
import { iconLicense, virtualLicense } from './iconCredits';
import { sortGames, formatTime, displayVersion, versionInput, isAssociatedFile } from './library';
import type { LibrarySort } from './library';
import { activeJob, useJob } from './useJob';
import { TaskBanner } from './TaskBanner';
import { ProgressBar } from './ProgressBar';
import { scanResults, directoryTime, canSelectScanCandidate } from './scan';
import type { ScanSort, ScanChoice } from './scan';
import type {
  Appearance,
  Game,
  DeleteReport,
  GameEdit,
  LaunchConfiguration,
  JobPage,
  MToolRecipe,
  ScanCandidate,
  Settings,
  LibraryPathCheck,
  ToolCheck,
  ImportRecoveryIssue,
  VersionHistory,
} from './types';
import { GameMaintenance, PathBadge, RelocationEditor } from './maintenance';
import { MToolConfiguration } from './MToolConfiguration';
import { ImportPage } from './ImportPage';
import { QspConfiguration } from './QspConfiguration';
import { EngineSelect } from './EngineSelect';
import { FolderErrorDialog } from './FolderErrorDialog';
import { SaveEditor } from './SaveEditor';
import { useLibraryLayout, useSavedSort } from './preferences';
import {
  PlayBadge,
  LaunchBadge,
  LibraryFilters,
  EngineQuickFilters,
  engineQuickGroups,
  emptyFilters,
  matchesFilters,
  playLabels,
} from './gameStatus';
import { isQspFile, localQsp, qspConfig, suggestedPlayer } from './qsp';

const lines = (text: string) =>
  text
    .split(/\r?\n/)
    .map((s) => s.trim())
    .filter(Boolean);
const optional = (value: string) => value.trim() || null;
const parentOf = (value: string | null) =>
  value?.replaceAll('\\', '/').split('/').slice(0, -1).join('/') || '.';
const statusText: Record<string, string> = {
  pending: '待分析',
  ready: '待确认',
  skipped: '已跳过',
  incomplete: '扫描未完整完成',
  error: '需检查目录',
};
type SettingsSection = 'library' | 'mtool';
type Choice = ScanChoice;
const LIBRARY_ROW_HEIGHT = 72;
const LIBRARY_CARD_HEIGHT = 288;

function launchBlockReason(game: Game, check?: LibraryPathCheck): string | undefined {
  if (check && check.state !== 'available') return check.message;
  if (game.launch_type === 'DIRECT' && !game.main_executable) return '请先配置启动文件';
  if (game.launch_type === 'MTOOL' && !game.mtool_target_exe && !game.main_executable)
    return '请先配置游戏启动文件';
  if (
    game.launch_type === 'EXTERNAL_PLAYER' &&
    (!game.main_executable || !game.external_player?.game_file)
  )
    return '请先配置 QSP 播放器和游戏文件';
  return undefined;
}

export default function App() {
  const [appearance, setAppearance] = useState<Appearance>(defaultAppearance);
  const [appearanceReady, setAppearanceReady] = useState(false);
  const appIcon = appearanceIcons[appearance.icon];
  const sidebarCharacter = appearanceIllustrations[appearance.illustration];
  useEffect(() => {
    const favicon = document.querySelector<HTMLLinkElement>('link[rel="icon"]');
    if (favicon) favicon.href = appIcon;
  }, [appIcon]);
  const [games, setGames] = useState<Game[]>([]);
  const [pathChecks, setPathChecks] = useState<Record<string, LibraryPathCheck>>({});
  const [issuesOnly, setIssuesOnly] = useState(false);
  const checkQuiet = useRef(false);
  const [linkCandidate, setLinkCandidate] = useState<ScanCandidate | null>(null);
  const [linkGameId, setLinkGameId] = useState('');
  const [linkBusy, setLinkBusy] = useState(false);
  const [settings, setSettings] = useState<Settings | null>(null);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [settingsSection, setSettingsSection] = useState<SettingsSection>('library');
  const [importOpen, setImportOpen] = useState(false);
  const [importBusy, setImportBusy] = useState(false);
  const [importRecoveryIssues, setImportRecoveryIssues] = useState<ImportRecoveryIssue[]>([]);
  const [layout, setLayout] = useLibraryLayout();
  function openSettings(section: SettingsSection) {
    setSettingsSection(section);
    setSettingsOpen(true);
  }
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [highlightedGameId, setHighlightedGameId] = useState<string | null>(null);
  function openGame(id: string) {
    setHighlightedGameId(id);
    setSelectedId(id);
  }
  const [launchingIds, setLaunchingIds] = useState<Set<string>>(new Set());
  const launchRequests = useRef(new Set<string>());
  async function playGame(id: string, configuration?: LaunchConfiguration) {
    if (launchRequests.current.has(id)) return;
    launchRequests.current.add(id);
    setLaunchingIds((current) => new Set(current).add(id));
    try {
      const launched = await (configuration ? api.play(id, configuration) : api.play(id));
      if (launched)
        setGames((current) => current.map((game) => (game.id === id ? launched : game)));
    } finally {
      launchRequests.current.delete(id);
      setLaunchingIds((current) => {
        const next = new Set(current);
        next.delete(id);
        return next;
      });
    }
  }
  const [search, setSearch] = useState('');
  const [sort, setSort] = useSavedSort<LibrarySort>('library', 'name-asc', [
    'name-asc',
    'name-desc',
    'added-asc',
    'added-desc',
    'played-asc',
    'played-desc',
  ]);
  const [filters, setFilters] = useState(emptyFilters);
  const [otherEnginesOnly, setOtherEnginesOnly] = useState(false);
  const popularEngines = useMemo(
    () => new Set(engineQuickGroups(games).popular.map((group) => group.engine)),
    [games],
  );
  const libraryEpoch = useRef(0);
  const currentScanId = useRef<string | null>(null);
  const [scanSearch, setScanSearch] = useState('');
  const [scanUnregisteredOnly, setScanUnregisteredOnly] = useState(false);
  const [scanSort, setScanSort] = useSavedSort<ScanSort>('scan', 'unregistered-first', [
    'name-asc',
    'name-desc',
    'modified-asc',
    'modified-desc',
    'unregistered-first',
    'registered-first',
  ]);
  const [expandedCandidates, setExpandedCandidates] = useState<Set<string>>(new Set());
  const [previewOpen, setPreviewOpen] = useState(false);
  const scanScroll = useRef<HTMLDivElement>(null);
  const settingsGuard = useRef<((go: () => void) => void) | null>(null);
  function navigate(go: () => void) {
    if (settingsOpen && settingsGuard.current) settingsGuard.current(go);
    else go();
  }
  const libraryScroll = useRef<HTMLElement>(null);
  const libraryHeader = useRef<HTMLTableSectionElement>(null);
  const viewScroll = useRef({ library: 0, scan: 0 });
  const settingsScroll = useRef<Record<SettingsSection, number>>({
    library: 0,
    mtool: 0,
  });
  function showPreview(open: boolean, reset = false) {
    setSettingsOpen(false);
    setImportOpen(false);
    if (reset) viewScroll.current[open ? 'scan' : 'library'] = 0;
    setPreviewOpen(open);
  }
  useLayoutEffect(() => {
    if (scanScroll.current) scanScroll.current.scrollTop = viewScroll.current.scan;
    if (!settingsOpen && !previewOpen && !importOpen && libraryScroll.current)
      libraryScroll.current.scrollTop = viewScroll.current.library;
  }, [previewOpen, importOpen, layout, settingsOpen]);
  useLayoutEffect(() => {
    viewScroll.current.library = 0;
    if (libraryScroll.current) libraryScroll.current.scrollTop = 0;
  }, [search, sort, issuesOnly, filters, otherEnginesOnly]);
  const [scanId, setScanId] = useState<string | null>(null);
  const [taskId, setTaskId] = useState<string | null>(null);
  const [showCheckBanner, setShowCheckBanner] = useState(false);
  const [analysisGameId, setAnalysisGameId] = useState<string | null>(null);
  const [analyses, setAnalyses] = useState<Record<string, ScanCandidate>>({});
  const [analysisNotices, setAnalysisNotices] = useState<Record<string, string>>({});
  const [candidates, setCandidates] = useState<Record<string, ScanCandidate>>({});
  const [choices, setChoices] = useState<Record<string, Choice>>({});
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const autoSelected = useRef(new Set<string>());
  const [starting, setStarting] = useState(false);
  const { notice, setToast, setError, dismiss } = useNotifications();
  const desktop = isTauri();

  function mergeChanges(changes: ScanCandidate[]) {
    setCandidates((current) => {
      const next = { ...current };
      for (const candidate of changes) next[candidate.install_path] = candidate;
      return next;
    });
    setSelected((current) => {
      const next = new Set(current);
      for (const candidate of changes) {
        if (candidate.registered_id) next.delete(candidate.install_path);
        else if (
          candidate.status === 'ready' &&
          (!candidate.qsp || !!qspConfig(candidate)?.game_file) &&
          !autoSelected.current.has(candidate.install_path)
        ) {
          next.add(candidate.install_path);
          autoSelected.current.add(candidate.install_path);
        }
      }
      return next;
    });
  }
  function acceptPathChecks(checks: LibraryPathCheck[]) {
    setPathChecks((current) => {
      const next = { ...current };
      for (const check of checks)
        if (games.some((game) => game.id === check.id && game.install_path === check.install_path))
          next[check.id] = check;
      return next;
    });
  }
  const scanJob = useJob(
    scanId,
    mergeChanges,
    async (job) => {
      if (currentScanId.current !== job.id) return;
      acceptPathChecks(job.path_checks || []);
      if (job.status === 'completed') {
        try {
          const changed = await api.syncScanMtool(job.id);
          if (currentScanId.current !== job.id) return;
          if (changed.length) {
            libraryEpoch.current++;
            const updates = new Map(changed.map((game) => [game.id, game]));
            setGames((current) => current.map((game) => updates.get(game.id) || game));
            const checked = await api.job(job.id, job.next_cursor);
            if (currentScanId.current === job.id) acceptPathChecks(checked.path_checks || []);
          }
        } catch (reason) {
          setError(`扫描结果保留，识别信息同步失败：${String(reason)}`);
        }
      }
    },
    setError,
  );
  const taskJob = useJob(
    taskId,
    (changes) =>
      setAnalyses((current) => {
        const next = { ...current };
        for (const candidate of changes)
          if (candidate.registered_id) next[candidate.registered_id] = candidate;
        return next;
      }),
    (job) => {
      if (job.kind === 'paths') {
        const checks = job.path_checks || [];
        acceptPathChecks(checks);
        const issues = checks.filter((check) => check.state !== 'available').length;
        if (!checkQuiet.current || issues || job.status !== 'completed') setShowCheckBanner(true);
        return;
      }
      if (analysisGameId)
        setAnalysisNotices((current) => ({
          ...current,
          [analysisGameId]:
            job.status === 'completed'
              ? '分析结束，可查看下方详情。'
              : job.status === 'cancelled'
                ? '分析已取消，原有结果保留。'
                : job.error || '分析失败，原有结果保留。',
        }));
      setAnalysisGameId(null);
      if (job.status === 'completed') {
        if (job.registered_ids.length) {
          const epoch = libraryEpoch.current;
          const registeredScanId = scanId;
          void api
            .gamesByIds(job.registered_ids)
            .then((added) => {
              if (epoch !== libraryEpoch.current) return;
              setGames((current) => {
                const map = new Map(current.map((g) => [g.id, g]));
                for (const game of added) map.set(game.id, game);
                return [...map.values()].sort((a, b) =>
                  a.display_title.localeCompare(b.display_title),
                );
              });
              if (job.kind === 'register' && currentScanId.current === registeredScanId) {
                const ids = new Map(added.map((game) => [game.install_path, game.id]));
                setCandidates((current) =>
                  Object.fromEntries(
                    Object.entries(current).map(([path, candidate]) => [
                      path,
                      ids.has(path) ? { ...candidate, registered_id: ids.get(path)! } : candidate,
                    ]),
                  ),
                );
                setSelected((current) => new Set([...current].filter((path) => !ids.has(path))));
              }
            })
            .catch((reason) =>
              setError(`记录已保存，但列表更新失败：${String(reason)}。可重新打开应用查看。`),
            );
          setToast(
            job.kind === 'metadata'
              ? `已补充 ${job.registered_ids.length} 项记录。`
              : `已加入游戏库：${job.registered_ids.length} 项。`,
          );
        } else
          setToast(job.kind === 'metadata' ? '补充结束，没有需要修改的记录。' : '单独分析结束。');
        void scanJob.refresh().catch((reason) => setError(String(reason)));
      } else {
        setToast(
          job.status === 'cancelled'
            ? job.kind === 'register'
              ? '任务已取消，未提交的登记已回滚，选择仍保留。'
              : '分析已取消，原有结果保留。'
            : '任务失败，预览与选择仍保留。',
        );
        if (job.error && job.status === 'failed') setError(job.error);
      }
    },
    setError,
  );
  const pending = (id: string | null, page: JobPage | null) =>
    !!id && (!page || page.id !== id || activeJob(page));
  const scanning = pending(scanId, scanJob.page);
  const processing = pending(taskId, taskJob.page);
  const taskActive = starting || scanning || processing || importBusy;

  useEffect(() => {
    if (!desktop) return;
    let stale = false;
    api
      .appearance()
      .then((value) => {
        if (!stale) setAppearance(value);
      })
      .catch((reason) => {
        if (!stale) setError(String(reason));
      })
      .finally(() => {
        if (!stale) setAppearanceReady(true);
      });
    Promise.all([api.games(), api.settings()])
      .then(([library, config]) => {
        if (stale) return;
        setGames(library);
        setSettings(config);
        setSettingsOpen(!config.game_root);
        if (library.length) void startLibraryCheck(true);
      })
      .catch((reason) => {
        if (!stale) setError(String(reason));
      });
    api
      .importRecoveryIssues()
      .then((issues) => {
        if (!stale) setImportRecoveryIssues(issues);
      })
      .catch((reason) => {
        if (!stale) setError(String(reason));
      });
    return () => {
      stale = true;
    };
  }, [desktop]);

  async function action(fn: () => Promise<void>) {
    setError('');
    try {
      await fn();
    } catch (reason) {
      setError(String(reason));
    }
  }
  async function startLibraryCheck(quiet = false) {
    checkQuiet.current = quiet;
    setShowCheckBanner(!quiet);
    setStarting(true);
    await action(async () => {
      const id = await api.startLibraryCheck();
      if (id) setTaskId(id);
    });
    setStarting(false);
  }
  function acceptRelocated(saved: Game) {
    const old = games.find((game) => game.id === saved.id);
    if (old) autoSelected.current.add(old.install_path);
    refreshLibraryAfterMaintenance();
    setGames((current) => current.map((game) => (game.id === saved.id ? saved : game)));
    setPathChecks((current) => {
      const next = { ...current };
      delete next[saved.id];
      return next;
    });
    setAnalyses((current) => {
      const next = { ...current };
      delete next[saved.id];
      return next;
    });
    setAnalysisNotices((current) => {
      const next = { ...current };
      delete next[saved.id];
      return next;
    });
    setCandidates((current) =>
      Object.fromEntries(
        Object.entries(current).map(([path, candidate]) => [
          path,
          candidate.registered_id === saved.id
            ? { ...candidate, registered_id: path === saved.install_path ? saved.id : null }
            : path === saved.install_path
              ? { ...candidate, registered_id: saved.id }
              : candidate,
        ]),
      ),
    );
    setSelected(
      (current) =>
        new Set(
          [...current].filter((path) => path !== saved.install_path && path !== old?.install_path),
        ),
    );
    void scanJob.refresh().catch((reason) => setError(String(reason)));
    void startLibraryCheck(true);
  }
  async function removeRecord(id: string) {
    const removed = games.find((game) => game.id === id);
    await api.removeGame(id);
    forgetGame(id, removed);
    setToast('库记录已移除，游戏与存档文件保留。');
  }
  function forgetGame(id: string, removed?: Game) {
    if (removed) {
      autoSelected.current.add(removed.install_path);
      setSelected(
        (current) => new Set([...current].filter((path) => path !== removed.install_path)),
      );
    }
    refreshLibraryAfterMaintenance();
    setGames((current) => current.filter((game) => game.id !== id));
    setSelectedId(null);
    setHighlightedGameId((current) => (current === id ? null : current));
    setPathChecks((current) => {
      const next = { ...current };
      delete next[id];
      return next;
    });
    setCandidates((current) =>
      Object.fromEntries(
        Object.entries(current).map(([path, candidate]) => [
          path,
          candidate.registered_id === id ? { ...candidate, registered_id: null } : candidate,
        ]),
      ),
    );
    void scanJob.refresh().catch((reason) => setError(String(reason)));
  }
  function refreshLibraryAfterMaintenance() {
    const epoch = ++libraryEpoch.current;
    void api
      .games()
      .then((library) => {
        if (epoch === libraryEpoch.current) setGames(library);
      })
      .catch((reason) =>
        setError(`记录已保存，列表刷新失败：${String(reason)}。可重新打开应用查看。`),
      );
  }
  async function startScan() {
    setStarting(true);
    await action(async () => {
      const id = await api.startScan();
      setCandidates({});
      setChoices({});
      setSelected(new Set());
      autoSelected.current.clear();
      setScanSearch('');
      setScanUnregisteredOnly(false);
      setExpandedCandidates(new Set());
      viewScroll.current.scan = 0;
      if (scanScroll.current) scanScroll.current.scrollTop = 0;
      currentScanId.current = id;
      setScanId(id);
      setTaskId(null);
      setShowCheckBanner(false);
      setToast('');
    });
    setStarting(false);
  }
  async function register() {
    if (!scanId) return;
    setStarting(true);
    await action(async () => {
      const selections = [...selected].map((path) => {
        const candidate = candidates[path];
        const choice = choices[path];
        return {
          install_path: path,
          executable: choice?.exe === undefined ? suggestedPlayer(candidate) : choice.exe,
          external_player: choice?.external_player ?? qspConfig(candidate),
          exe_override: choice?.exe !== undefined,
          version: choice?.version ?? candidate.suggested_version,
          version_override: !!choice?.versionManual,
        };
      });
      setTaskId(await api.register(scanId, selections));
    });
    setStarting(false);
  }
  const needle = search.normalize('NFKC').toLocaleLowerCase();
  const problemCount = useMemo(
    () =>
      games.filter((game) => pathChecks[game.id] && pathChecks[game.id].state !== 'available')
        .length,
    [games, pathChecks],
  );
  const filtered = useMemo(
    () =>
      sortGames(
        games.filter(
          (game) =>
            matchesFilters(game, filters) &&
            (!otherEnginesOnly || !popularEngines.has(game.engine)) &&
            (!issuesOnly || (!!pathChecks[game.id] && pathChecks[game.id].state !== 'available')) &&
            [game.display_title, game.canonical_title, game.install_path, ...game.aliases].some(
              (s) => s.normalize('NFKC').toLocaleLowerCase().includes(needle),
            ),
        ),
        sort,
      ),
    [games, needle, sort, pathChecks, issuesOnly, filters, otherEnginesOnly, popularEngines],
  );
  const [cardColumns, setCardColumns] = useState(1);
  useLayoutEffect(() => {
    const element = libraryScroll.current;
    if (!element || layout !== 'grid') return;
    const resize = () => setCardColumns(Math.max(1, Math.floor((element.clientWidth + 16) / 226)));
    resize();
    const observer = typeof ResizeObserver === 'undefined' ? null : new ResizeObserver(resize);
    observer?.observe(element);
    window.addEventListener('resize', resize);
    return () => {
      observer?.disconnect();
      window.removeEventListener('resize', resize);
    };
  }, [layout, previewOpen, importOpen, settingsOpen]);
  const virtualized = filtered.length > 50;
  const virtualItemKey = useMemo(
    () => (index: number) =>
      `${layout}-${cardColumns}-${filtered[index * (layout === 'list' ? 1 : cardColumns)]?.id}`,
    [layout, cardColumns, filtered],
  );
  const virtualizer = useVirtualizer<HTMLElement, HTMLElement>({
    count: layout === 'list' ? filtered.length : Math.ceil(filtered.length / cardColumns),
    getScrollElement: () => libraryScroll.current,
    getItemKey: virtualItemKey,
    estimateSize: () => (layout === 'list' ? LIBRARY_ROW_HEIGHT : LIBRARY_CARD_HEIGHT),
    gap: layout === 'list' ? 3 : 16,
    paddingStart: 3,
    paddingEnd: 3,
    overscan: 6,
    initialOffset: () => viewScroll.current.library,
    enabled: virtualized && !previewOpen && !importOpen && !settingsOpen,
  });
  const libraryRows = virtualized
    ? virtualizer.getVirtualItems().map((row) => ({ game: filtered[row.index], row }))
    : filtered.map((game) => ({ game, row: null }));
  const preview = useMemo(() => Object.values(candidates), [candidates]);
  const scanFiltered = useMemo(
    () =>
      scanResults(preview, scanSearch, scanSort, choices).filter(
        (candidate) => !scanUnregisteredOnly || !candidate.registered_id,
      ),
    [preview, scanSearch, scanSort, choices, scanUnregisteredOnly],
  );
  const registeredGames = useMemo(() => new Map(games.map((game) => [game.id, game])), [games]);
  const visibleSelections = scanFiltered.filter((candidate) =>
    selected.has(candidate.install_path),
  ).length;
  const scanSelectable = useMemo(
    () =>
      scanFiltered.filter((candidate) =>
        canSelectScanCandidate(candidate, choices[candidate.install_path]),
      ),
    [scanFiltered, choices],
  );
  const scanAllSelected =
    visibleSelections > 0 &&
    scanSelectable.every((candidate) => selected.has(candidate.install_path));
  const virtualScan = scanFiltered.length > 50;
  const scanItemKey = useMemo(
    () => (index: number) => scanFiltered[index].install_path,
    [scanFiltered],
  );
  const { virtualizer: scanVirtualizer, scrolling: scanScrolling } = useStableVirtualizer(
    {
      count: scanFiltered.length,
      getScrollElement: () => scanScroll.current,
      getItemKey: scanItemKey,
      estimateSize: () => 150,
      gap: 10,
      overscan: 5,
      initialOffset: () => viewScroll.current.scan,
      enabled: virtualScan && previewOpen && !settingsOpen,
    },
    'scan',
  );
  useLayoutEffect(() => {
    viewScroll.current.scan = 0;
    if (scanScroll.current) scanScroll.current.scrollTop = 0;
  }, [scanSearch, scanSort, scanUnregisteredOnly]);
  const scanRows = virtualScan
    ? scanVirtualizer.getVirtualItems().map((row) => ({ candidate: scanFiltered[row.index], row }))
    : scanFiltered.map((candidate) => ({ candidate, row: null }));
  const game = games.find((g) => g.id === selectedId);
  const staleRoot = !!scanJob.page && !!settings && scanJob.page.root !== settings.game_root;
  function setChoice(path: string, choice: Choice) {
    setChoices((current) => ({ ...current, [path]: { ...current[path], ...choice } }));
    if ('exe' in choice) {
      const config =
        choice.external_player ?? choices[path]?.external_player ?? qspConfig(candidates[path]);
      void api
        .suggestVersion(
          candidates[path].suggested_title,
          config ? config.game_file : choice.exe || null,
        )
        .then(([version]) => {
          setChoices((current) =>
            current[path]?.exe === choice.exe &&
            (current[path]?.external_player?.game_file ?? null) === (config?.game_file ?? null) &&
            !current[path]?.versionManual
              ? { ...current, [path]: { ...current[path], version } }
              : current,
          );
        })
        .catch((reason) => setError(String(reason)));
    }
  }

  const libraryEmpty = !games.length ? (
    <div className="empty">
      <h3>从你的游戏目录开始</h3>
      <p>打开设置选择 Game Root，再扫描目录。游戏文件保留原位。</p>
    </div>
  ) : !filtered.length ? (
    <div className="empty">没有匹配的游戏。</div>
  ) : null;

  return (
    <div className="app-shell">
      <aside className="sidebar">
        <div className="brand">
          <img src={appIcon} alt="" />
          <div>
            <strong>butter-manager</strong>
            <p>by Zhehao-w</p>
          </div>
        </div>
        <nav className="sidebar-nav" aria-label="主导航">
          <button
            className={!settingsOpen && !previewOpen && !importOpen ? 'active' : ''}
            aria-current={!settingsOpen && !previewOpen && !importOpen ? 'page' : undefined}
            onClick={() => navigate(() => showPreview(false))}
          >
            <Icon name="library" />
            游戏库<span className="nav-count">{games.length}</span>
          </button>
          <button
            className={!settingsOpen && previewOpen ? 'active' : ''}
            aria-current={!settingsOpen && previewOpen ? 'page' : undefined}
            onClick={() => navigate(() => showPreview(true))}
          >
            <Icon name="scan" />
            扫描入库
          </button>
          <button
            className={!settingsOpen && importOpen ? 'active' : ''}
            aria-current={!settingsOpen && importOpen ? 'page' : undefined}
            onClick={() =>
              navigate(() => {
                showPreview(false);
                setImportOpen(true);
              })
            }
          >
            <Icon name="import" />
            导入
            {!!importRecoveryIssues.length && (
              <span className="nav-count recovery-count">需处理</span>
            )}
          </button>
          <button
            disabled={!settings}
            className={settingsOpen && settingsSection === 'mtool' ? 'active' : ''}
            aria-current={settingsOpen && settingsSection === 'mtool' ? 'page' : undefined}
            onClick={() => openSettings('mtool')}
          >
            <Icon name="tool" />
            MTool
          </button>
        </nav>
        <div className="sidebar-art" aria-hidden="true">
          <img src={sidebarCharacter} alt="" draggable={false} />
        </div>
        <div className="sidebar-bottom">
          <button
            type="button"
            disabled={!settings}
            className={settingsOpen && settingsSection === 'library' ? 'active' : ''}
            aria-current={settingsOpen && settingsSection === 'library' ? 'page' : undefined}
            title="设置"
            onClick={() => openSettings('library')}
          >
            <Icon name="settings" />
            设置
            <span className="sidebar-version" aria-hidden="true">
              v0.3.1
            </span>
          </button>
        </div>
      </aside>
      <div className={`workspace${importOpen && !settingsOpen ? ' import-workspace' : ''}`}>
        {!desktop && <div className="notice">浏览器预览。扫描、选择文件与启动需使用桌面程序。</div>}
        {notice && <NotificationToast key={notice.id} notice={notice} onDismiss={dismiss} />}
        {taskJob.page && processing && taskJob.page.kind !== 'paths' && (
          <TaskProgress
            page={taskJob.page}
            onCancel={() =>
              void action(async () => {
                await api.cancel(taskId!);
                taskJob.markCancelling();
              })
            }
          />
        )}
        <main
          className={
            settingsOpen
              ? 'settings-main'
              : previewOpen
                ? 'scan-main'
                : importOpen
                  ? 'page-main'
                  : 'library-main'
          }
        >
          {!settingsOpen && !previewOpen && !importOpen && (
            <section className="library panel">
              <PageHeader
                icon="library"
                title={
                  <>
                    游戏库 <span className="count">{games.length}</span>
                  </>
                }
                description="管理本地游戏、启动方式与存档信息。"
              >
                <div className="header-actions">
                  <button
                    className="primary"
                    disabled={taskActive || !settings?.game_root}
                    onClick={() => void startScan()}
                  >
                    <Icon name="scan" />
                    {starting ? '准备中…' : '扫描目录'}
                  </button>
                </div>
              </PageHeader>
              <div className="library-toolbar">
                <SearchField
                  label="搜索游戏或别名"
                  placeholder="搜索游戏（名称、别名、路径）"
                  value={search}
                  onChange={setSearch}
                />
                <div className="view-toggle" role="group" aria-label="游戏库视图">
                  <button
                    aria-label="列表视图"
                    aria-pressed={layout === 'list'}
                    className={layout === 'list' ? 'active' : ''}
                    onClick={() => setLayout('list')}
                  >
                    <Icon name="list" />
                  </button>
                  <button
                    aria-label="卡片视图"
                    aria-pressed={layout === 'grid'}
                    className={layout === 'grid' ? 'active' : ''}
                    onClick={() => setLayout('grid')}
                  >
                    <Icon name="grid" />
                  </button>
                </div>
                <div className="header-actions library-controls">
                  <LibraryFilters
                    games={games}
                    value={filters}
                    onChange={(value) => {
                      setFilters(value);
                      setOtherEnginesOnly(false);
                    }}
                  />
                  <button
                    aria-pressed={issuesOnly}
                    className={`issues-filter ${issuesOnly ? 'active' : ''}`}
                    disabled={!problemCount && !issuesOnly}
                    onClick={() => setIssuesOnly(!issuesOnly)}
                  >
                    需处理 ({problemCount})
                  </button>
                  <SortField
                    label="游戏排序"
                    value={sort}
                    onChange={(value) => setSort(value as LibrarySort)}
                  >
                    <option value="name-asc">游戏名 · 正序</option>
                    <option value="name-desc">游戏名 · 倒序</option>
                    <option value="added-desc">入库时间 · 最新</option>
                    <option value="added-asc">入库时间 · 最早</option>
                    <option value="played-desc">运行时间 · 最近</option>
                    <option value="played-asc">运行时间 · 最早</option>
                  </SortField>
                </div>
              </div>
              <EngineQuickFilters
                games={games}
                engines={filters.engines}
                other={otherEnginesOnly}
                onChange={(engines, other) => {
                  setFilters((current) => ({ ...current, engines }));
                  setOtherEnginesOnly(other);
                }}
              />
              <div
                className="library-content"
                style={
                  {
                    '--library-row-height': `${LIBRARY_ROW_HEIGHT}px`,
                    '--library-card-height': `${LIBRARY_CARD_HEIGHT}px`,
                  } as CSSProperties
                }
              >
                {layout === 'list' ? (
                  <table className="library-table" aria-rowcount={filtered.length + 1}>
                    <thead ref={libraryHeader}>
                      <tr>
                        <th scope="col">游戏</th>
                        <th scope="col">版本</th>
                        <th scope="col">启动方式</th>
                        <th scope="col">引擎</th>
                        <th scope="col">上次运行 / 入库</th>
                      </tr>
                    </thead>
                    <tbody
                      className="library-scroll"
                      ref={(element) => {
                        libraryScroll.current = element;
                      }}
                      aria-label="游戏列表"
                      tabIndex={0}
                      onScroll={(event) => {
                        viewScroll.current.library = event.currentTarget.scrollTop;
                        if (
                          libraryHeader.current &&
                          libraryHeader.current.scrollLeft !== event.currentTarget.scrollLeft
                        )
                          libraryHeader.current.scrollLeft = event.currentTarget.scrollLeft;
                      }}
                    >
                      {virtualized && (
                        <tr
                          className="virtual-spacer"
                          aria-hidden="true"
                          style={{ height: virtualizer.getTotalSize() }}
                        >
                          <td colSpan={5} />
                        </tr>
                      )}
                      {libraryRows.map(({ game: g, row }) => (
                        <tr
                          key={g.id}
                          data-index={row?.index}
                          aria-rowindex={row ? row.index + 2 : filtered.indexOf(g) + 2}
                          style={
                            row
                              ? {
                                  position: 'absolute',
                                  top: 0,
                                  left: 0,
                                  width: '100%',
                                  transform: `translateY(${row.start}px)`,
                                }
                              : undefined
                          }
                          className={`${g.id === highlightedGameId ? 'selected' : ''} ${row ? 'virtual-row' : ''}`}
                          onMouseEnter={() => setHighlightedGameId(g.id)}
                          onFocus={() => setHighlightedGameId(g.id)}
                          onClick={(event) => {
                            if ((event.target as Element).closest('.library-play-button')) return;
                            openGame(g.id);
                          }}
                        >
                          <td className="library-game-cell">
                            <div className="game-identity">
                              <GameMark game={g} />
                              <div className="game-identity-text">
                                <button className="game-link" title={g.display_title}>
                                  {g.display_title}
                                </button>
                                <div className="game-status-line">
                                  <PlayBadge game={g} />
                                  <PathBadge check={pathChecks[g.id]} />
                                </div>
                              </div>
                            </div>
                            <button
                              type="button"
                              className="library-play-button"
                              aria-label={`启动 ${g.display_title}`}
                              title={
                                taskActive
                                  ? '当前任务完成后可启动'
                                  : launchBlockReason(g, pathChecks[g.id]) ||
                                    `启动 ${g.display_title}`
                              }
                              disabled={
                                taskActive ||
                                launchingIds.has(g.id) ||
                                !!launchBlockReason(g, pathChecks[g.id])
                              }
                              onClick={(event) => {
                                event.stopPropagation();
                                void action(async () => {
                                  await playGame(g.id);
                                  setToast(`已发送“${g.display_title}”的启动请求。`);
                                });
                              }}
                            >
                              <Icon name="play" size={14} />
                              {launchingIds.has(g.id) ? '启动中' : '启动'}
                            </button>
                          </td>
                          <td title={g.current_version === 'Unknown' ? '版本未知' : undefined}>
                            {displayVersion(g.current_version)}
                          </td>
                          <td>
                            <LaunchBadge game={g} />
                          </td>
                          <td title={g.engine === 'Unknown' ? '引擎未识别' : undefined}>
                            {g.engine === 'Unknown' ? '-' : g.engine}
                          </td>
                          <td className="library-activity-cell">
                            <span title={`上次运行：${formatTime(g.last_launched_at)}`}>
                              {formatTime(g.last_launched_at)}
                            </span>
                            <small title={`入库时间：${formatTime(g.created_at)}`}>
                              入库 {formatTime(g.created_at)}
                            </small>
                          </td>
                        </tr>
                      ))}
                      {libraryEmpty && (
                        <tr className="empty-row">
                          <td colSpan={5}>{libraryEmpty}</td>
                        </tr>
                      )}
                    </tbody>
                  </table>
                ) : (
                  <div
                    className="library-scroll"
                    ref={(element) => {
                      libraryScroll.current = element;
                    }}
                    aria-label="游戏列表"
                    tabIndex={0}
                    onScroll={(event) => {
                      viewScroll.current.library = event.currentTarget.scrollTop;
                      if (
                        libraryHeader.current &&
                        libraryHeader.current.scrollLeft !== event.currentTarget.scrollLeft
                      )
                        libraryHeader.current.scrollLeft = event.currentTarget.scrollLeft;
                    }}
                  >
                    <div
                      className={`game-grid ${virtualized ? 'virtual-card-grid' : ''}`}
                      aria-label="游戏卡片"
                      style={virtualized ? { height: virtualizer.getTotalSize() } : undefined}
                    >
                      {(virtualized
                        ? virtualizer.getVirtualItems()
                        : [{ index: 0, key: 'all', start: 0 }]
                      ).map((row) => (
                        <div
                          key={row.key}
                          className={virtualized ? 'virtual-card-row' : 'card-contents'}
                          data-index={virtualized ? row.index : undefined}
                          style={
                            virtualized
                              ? {
                                  transform: `translateY(${row.start}px)`,
                                  gridTemplateColumns: `repeat(${cardColumns}, minmax(0, 1fr))`,
                                }
                              : undefined
                          }
                        >
                          {(virtualized
                            ? filtered.slice(row.index * cardColumns, (row.index + 1) * cardColumns)
                            : filtered
                          ).map((g) => (
                            <article
                              key={g.id}
                              className={`game-card ${g.id === highlightedGameId ? 'selected' : ''}`}
                              onMouseEnter={() => setHighlightedGameId(g.id)}
                              onFocus={() => setHighlightedGameId(g.id)}
                              onClick={(event) => {
                                if ((event.target as Element).closest('.library-play-button'))
                                  return;
                                openGame(g.id);
                              }}
                            >
                              <GameMark game={g} large />
                              <button
                                type="button"
                                className="game-card-title"
                                aria-label={g.display_title}
                                title={g.display_title}
                              >
                                {g.display_title}
                              </button>
                              <div className="game-status-line">
                                <PlayBadge game={g} />
                                <PathBadge check={pathChecks[g.id]} />
                              </div>
                              <div className="card-badges">
                                <LaunchBadge game={g} />
                                <span
                                  className="version-badge"
                                  title={displayVersion(g.current_version)}
                                >
                                  {displayVersion(g.current_version)}
                                </span>
                              </div>
                              <span
                                className="card-engine"
                                title={g.engine === 'Unknown' ? '引擎未识别' : g.engine}
                              >
                                {g.engine === 'Unknown' ? '引擎未识别' : g.engine}
                              </span>
                              <span className="card-last-played">
                                <Icon name="clock" size={14} />
                                {formatTime(g.last_launched_at)}
                              </span>
                              <button
                                type="button"
                                className="library-play-button card-play-button"
                                aria-label={`启动 ${g.display_title}`}
                                title={
                                  taskActive
                                    ? '当前任务完成后可启动'
                                    : launchBlockReason(g, pathChecks[g.id]) ||
                                      `启动 ${g.display_title}`
                                }
                                disabled={
                                  taskActive ||
                                  launchingIds.has(g.id) ||
                                  !!launchBlockReason(g, pathChecks[g.id])
                                }
                                onClick={(event) => {
                                  event.stopPropagation();
                                  void action(async () => {
                                    await playGame(g.id);
                                    setToast(`已发送“${g.display_title}”的启动请求。`);
                                  });
                                }}
                              >
                                <Icon name="play" size={14} />
                                {launchingIds.has(g.id) ? '启动中…' : '启动'}
                              </button>
                            </article>
                          ))}
                        </div>
                      ))}
                    </div>
                    {libraryEmpty}
                  </div>
                )}
              </div>
              <div className="library-footer">
                <p className="root-path">
                  <span>游戏目录</span>
                  <span className="root-path-value" title={settings?.game_root || '尚未设置'}>
                    {settings?.game_root || '尚未设置'}
                  </span>
                </p>
                <span className="library-total" role="status">
                  {search ||
                  filters.statuses.length ||
                  filters.engines.length ||
                  filters.modes.length ||
                  otherEnginesOnly ||
                  issuesOnly
                    ? `找到 ${filtered.length} / ${games.length} 个游戏`
                    : `共 ${games.length} 个游戏`}
                </span>
              </div>
            </section>
          )}
          {!settingsOpen && previewOpen && (
            <section className="panel scan-page" aria-label="扫描结果页面">
              <PageHeader
                icon="scan"
                title={
                  <>
                    {scanId ? '扫描结果' : '扫描入库'}{' '}
                    <span className="count">{preview.length}</span>
                  </>
                }
                description="确认启动文件和版本。切换页面后保留结果与选择。"
              >
                <div className="header-actions">
                  <span className="muted">
                    已选 {selected.size} 项
                    {visibleSelections < selected.size
                      ? `（当前显示 ${visibleSelections} 项）`
                      : ''}
                  </span>
                  <button
                    disabled={taskActive || !settings?.game_root}
                    onClick={() => void startScan()}
                  >
                    <Icon name="scan" />
                    {starting ? '准备中…' : scanId ? '重新扫描' : '扫描目录'}
                  </button>
                  <button
                    className="primary"
                    disabled={taskActive || staleRoot || !selected.size}
                    onClick={() => void register()}
                  >
                    加入游戏库
                  </button>
                </div>
              </PageHeader>
              <div className="scan-filters">
                <SearchField
                  label="搜索扫描结果"
                  placeholder="搜索文件夹、路径、启动文件或版本"
                  value={scanSearch}
                  onChange={setScanSearch}
                />
                <button
                  type="button"
                  className={`library-filter-button ${scanUnregisteredOnly ? 'active' : ''}`}
                  aria-pressed={scanUnregisteredOnly}
                  disabled={!preview.length}
                  onClick={() => setScanUnregisteredOnly((current) => !current)}
                >
                  <Icon name="filter" size={16} />
                  未入库
                </button>
                <SortField
                  label="扫描结果排序"
                  value={scanSort}
                  onChange={(value) => setScanSort(value as ScanSort)}
                >
                  <option value="unregistered-first">未入库优先</option>
                  <option value="registered-first">已入库优先</option>
                  <option value="name-asc">文件夹名称 · 正序</option>
                  <option value="name-desc">文件夹名称 · 倒序</option>
                  <option value="modified-desc">目录修改时间 · 最新</option>
                  <option value="modified-asc">目录修改时间 · 最早</option>
                </SortField>
                <div className="scan-selection-actions" role="group" aria-label="批量选择扫描结果">
                  <button
                    type="button"
                    aria-pressed={scanAllSelected}
                    title={
                      scanAllSelected
                        ? '取消当前筛选结果的所有勾选'
                        : '选择当前筛选结果中所有可入库的游戏'
                    }
                    disabled={
                      taskActive || staleRoot || (!scanSelectable.length && !visibleSelections)
                    }
                    onClick={() =>
                      setSelected((current) => {
                        const next = new Set(current);
                        if (scanAllSelected) {
                          for (const candidate of scanFiltered) next.delete(candidate.install_path);
                        } else {
                          for (const candidate of scanSelectable) next.add(candidate.install_path);
                        }
                        return next;
                      })
                    }
                  >
                    {scanAllSelected ? '全不选' : '全选'}
                  </button>
                </div>
              </div>
              {staleRoot && (
                <p className="notice">扫描结果来自之前的游戏目录，请重新扫描后入库。</p>
              )}
              <div
                className="scan-scroll"
                ref={scanScroll}
                role="region"
                aria-label="扫描游戏列表"
                onScroll={(event) => {
                  viewScroll.current.scan = event.currentTarget.scrollTop;
                }}
              >
                {!scanId ? (
                  <div className="empty">
                    <h3>扫描本地游戏目录</h3>
                    <p>
                      {settings?.game_root
                        ? '点击“扫描目录”，查找新游戏并确认入库。'
                        : '先在侧边栏设置中选择游戏库根目录。'}
                    </p>
                  </div>
                ) : !scanFiltered.length ? (
                  <div className="empty">
                    <h3>
                      {scanSearch
                        ? '没有匹配的扫描结果'
                        : scanUnregisteredOnly
                          ? '没有未入库的游戏'
                          : scanning
                            ? '正在查找游戏…'
                            : '没有扫描结果'}
                    </h3>
                    <p>
                      {scanSearch
                        ? '试试其他文件夹名称、路径或版本。'
                        : scanUnregisteredOnly
                          ? '关闭“未入库”可查看已入库游戏。'
                          : '扫描结果将在这里显示。'}
                    </p>
                  </div>
                ) : null}
                <div
                  className={virtualScan ? 'scan-items virtual-scan-items' : 'scan-items'}
                  style={virtualScan ? { height: scanVirtualizer.getTotalSize() } : undefined}
                >
                  {scanRows.map(({ candidate, row }) => (
                    <div
                      key={candidate.install_path}
                      className={row ? 'scan-virtual-row' : 'scan-row'}
                      data-index={row?.index}
                      ref={row ? scanVirtualizer.measureElement : undefined}
                      style={
                        row
                          ? {
                              transform: `translateY(${row.start}px)`,
                              height: scanScrolling ? row.size : undefined,
                              overflow: scanScrolling ? 'clip' : undefined,
                            }
                          : undefined
                      }
                    >
                      <CandidateRow
                        key={candidate.install_path}
                        candidate={candidate}
                        canLink={games.length > 0}
                        onLink={() => {
                          setLinkCandidate(candidate);
                          setLinkGameId('');
                        }}
                        registeredGame={
                          candidate.registered_id
                            ? registeredGames.get(candidate.registered_id)
                            : undefined
                        }
                        expanded={expandedCandidates.has(candidate.install_path)}
                        onExpanded={(expanded) =>
                          setExpandedCandidates((current) => {
                            const next = new Set(current);
                            if (expanded) next.add(candidate.install_path);
                            else next.delete(candidate.install_path);
                            return next;
                          })
                        }
                        choice={choices[candidate.install_path]}
                        selected={selected.has(candidate.install_path)}
                        disabled={taskActive || staleRoot}
                        onToggle={(checked) =>
                          setSelected((current) => {
                            const next = new Set(current);
                            if (checked) next.add(candidate.install_path);
                            else next.delete(candidate.install_path);
                            return next;
                          })
                        }
                        onChoice={(choice) => setChoice(candidate.install_path, choice)}
                        onDeep={() =>
                          void action(async () => {
                            if (scanId)
                              setTaskId(await api.deepAnalyze(scanId, candidate.install_path));
                          })
                        }
                        onError={setError}
                      />
                    </div>
                  ))}
                </div>
                {!!scanJob.page?.warnings.length && (
                  <details className="analysis">
                    <summary>查看扫描诊断</summary>
                    <Warnings warnings={scanJob.page.warnings} />
                  </details>
                )}
                {scanJob.page?.status === 'failed' && scanJob.page.error && (
                  <details className="analysis">
                    <summary>扫描未完成，查看原因</summary>
                    <p className="error">{scanJob.page.error}</p>
                  </details>
                )}
              </div>
              <div className="scan-summary">
                <span>
                  显示 {scanFiltered.length} / {preview.length} 项 · 未入库{' '}
                  {preview.filter((candidate) => !candidate.registered_id).length} 项
                </span>
                {scanJob.page && (
                  <span>
                    {
                      (
                        {
                          completed: '扫描完成',
                          cancelled: '扫描已取消',
                          failed: '扫描失败 · 已完成结果保留',
                          cancel_requested: '正在取消扫描',
                          running: '正在扫描',
                        } as const
                      )[scanJob.page.status]
                    }
                  </span>
                )}
                <span>目录修改时间来自文件夹自身</span>
              </div>
            </section>
          )}
          {settings && (
            <ImportPage
              recoveryIssues={importRecoveryIssues}
              settings={settings}
              games={games}
              visible={!settingsOpen && importOpen}
              locked={starting || scanning || processing || launchingIds.size > 0}
              onActive={setImportBusy}
              onUpdated={() => {
                void api
                  .games()
                  .then(setGames)
                  .catch((e) => setError(String(e)));
              }}
              onError={setError}
              onOpenGame={openGame}
            />
          )}
          {settingsOpen && settings && (
            <SettingsPage
              settings={settings}
              appearance={appearance}
              appearanceReady={appearanceReady}
              onAppearanceSave={async (next) => {
                const saved = await api.saveAppearance(next);
                setAppearance(saved);
              }}
              initialSection={settingsSection}
              scrollPositions={settingsScroll}
              navigationGuard={settingsGuard}
              locked={taskActive || launchingIds.size > 0}
              onReset={async (confirmation) => {
                const report = await api.clearLibrary(confirmation);
                libraryEpoch.current += 1;
                setGames([]);
                setPathChecks({});
                setIssuesOnly(false);
                setLinkCandidate(null);
                setSettings(report.settings);
                setSelectedId(null);
                setHighlightedGameId(null);
                currentScanId.current = null;
                setScanId(null);
                setTaskId(null);
                setAnalysisGameId(null);
                setAnalyses({});
                setAnalysisNotices({});
                setCandidates({});
                setChoices({});
                setSelected(new Set());
                autoSelected.current.clear();
                showPreview(false, true);
                setScanSearch('');
                setScanUnregisteredOnly(false);
                setExpandedCandidates(new Set());
                setSearch('');
                setSettingsOpen(false);
                setError('');
                setToast(report.warning || '数据库已清空，设置已重置。请重新选择游戏目录。');
              }}
              onSave={async (next) => {
                const saved = await api.saveSettings(next);
                setSettings(saved);
                setToast('设置已保存。');
              }}
            />
          )}
        </main>
        <div className="task-banner-rail">
          {scanJob.page && scanJob.page.id === scanId && (
            <TaskBanner
              key={scanJob.page.id}
              page={scanJob.page}
              onViewResults={() => navigate(() => showPreview(true))}
              onSkip={(path) => void action(() => api.skip(scanJob.page!.id, path))}
              onCancel={() =>
                void action(async () => {
                  await api.cancel(scanJob.page!.id);
                  scanJob.markCancelling();
                })
              }
            />
          )}
          {showCheckBanner && taskJob.page?.kind === 'paths' && taskJob.page.id === taskId && (
            <TaskBanner
              key={taskJob.page.id}
              page={taskJob.page}
              onCancel={() =>
                void action(async () => {
                  await api.cancel(taskJob.page!.id);
                  taskJob.markCancelling();
                })
              }
            />
          )}
        </div>
        {!settingsOpen && (previewOpen || importOpen) && (
          <footer className="app-footer">
            <span className="local-dot" />
            本地管理 · 游戏和存档文件保持原位
          </footer>
        )}
      </div>
      {linkCandidate && (
        <Modal
          title="关联已有游戏"
          variant="detail"
          onClose={() => {
            if (!linkBusy) setLinkCandidate(null);
          }}
        >
          <p>选择要关联到这个目录的现有库记录，预览后确认。不会作为新游戏重复入库。</p>
          <p className="path">新目录：{linkCandidate.install_path}</p>
          <label className="relocation-field">
            已有游戏记录
            <select
              aria-label="已有游戏记录"
              value={linkGameId}
              disabled={linkBusy}
              onChange={(event) => setLinkGameId(event.target.value)}
            >
              <option value="">请选择现有记录</option>
              {[...games]
                .sort(
                  (a, b) =>
                    Number(pathChecks[b.id]?.state === 'missing_directory') -
                      Number(pathChecks[a.id]?.state === 'missing_directory') ||
                    a.display_title.localeCompare(b.display_title),
                )
                .map((game) => (
                  <option key={game.id} value={game.id}>
                    {pathChecks[game.id]?.state === 'missing_directory' ? '[目录缺失] ' : ''}
                    {game.display_title} · {game.install_path}
                  </option>
                ))}
            </select>
          </label>
          {games.find((game) => game.id === linkGameId) && (
            <RelocationEditor
              game={games.find((game) => game.id === linkGameId)!}
              path={linkCandidate.install_path}
              onBusy={setLinkBusy}
              locked={taskActive}
              onCancel={() => setLinkCandidate(null)}
              onSaved={(saved) => {
                acceptRelocated(saved);
                setLinkCandidate(null);
                setToast('已关联现有记录，历史与资料保留。');
              }}
            />
          )}
        </Modal>
      )}
      {game && settings && (
        <GameDetail
          key={game.id}
          game={game}
          settings={settings}
          pathCheck={pathChecks[game.id]}
          onRemoved={() => removeRecord(game.id)}
          onDeleted={(report) => {
            if (report.removed) {
              forgetGame(game.id, game);
              setToast('游戏文件已送入回收站。');
            } else void startLibraryCheck(true);
          }}
          onRelocated={acceptRelocated}
          analysis={analyses[game.id] || null}
          analysisNotice={analysisNotices[game.id] || ''}
          analysisJob={analysisGameId === game.id ? taskJob.page : null}
          analyzing={analysisGameId === game.id && processing}
          analysisLocked={taskActive}
          onAnalyze={async () => {
            setAnalysisNotices((current) => ({ ...current, [game.id]: '' }));
            const id = await api.startGameAnalysis(game.id);
            setAnalysisGameId(game.id);
            setTaskId(id);
          }}
          onCancelAnalysis={async () => {
            await api.cancel(taskId!);
            taskJob.markCancelling();
          }}
          launching={launchingIds.has(game.id)}
          onPlay={(configuration) => playGame(game.id, configuration)}
          onSaved={(saved) => {
            setGames((current) => current.map((g) => (g.id === saved.id ? saved : g)));
            if (
              saved.main_executable !== game.main_executable ||
              saved.working_directory !== game.working_directory ||
              saved.launch_type !== game.launch_type ||
              JSON.stringify(saved.external_player ?? null) !==
                JSON.stringify(game.external_player ?? null) ||
              saved.mtool_target_exe !== game.mtool_target_exe
            ) {
              setPathChecks((current) => {
                const next = { ...current };
                delete next[saved.id];
                return next;
              });
              void startLibraryCheck(true);
            }
          }}
          onClose={() => setSelectedId(null)}
        />
      )}
    </div>
  );
}

function TaskProgress({
  page,
  onCancel,
  onSkip,
}: {
  page: JobPage;
  onCancel: () => void;
  onSkip?: (path: string) => void;
}) {
  return (
    <section className="panel task-panel" role="status">
      <div className="section-heading">
        <div>
          <h3>
            {page.kind === 'scan'
              ? '扫描'
              : page.kind === 'analysis'
                ? '分析'
                : page.kind === 'paths'
                  ? '检查目录'
                  : page.kind === 'metadata'
                    ? '补充资料'
                    : '加入游戏库'}{' '}
            · {page.status === 'cancel_requested' ? '正在取消，等待磁盘操作返回…' : page.phase}
          </h3>
          <p>
            {page.total
              ? `${page.processed} / ${page.total}`
              : `已发现 ${page.change_count} 个目录`}{' '}
            · {(page.elapsed_ms / 1000).toFixed(1)} 秒
          </p>
        </div>
        <button disabled={page.status === 'cancel_requested'} onClick={onCancel}>
          取消任务
        </button>
      </div>
      {page.total > 0 && (
        <ProgressBar label="任务完成进度" max={page.total} value={page.processed} />
      )}
      {page.idle_ms >= 3000 && page.status === 'running' && (
        <p className="muted">目录读取较久，可能正在等待磁盘响应；可以取消或跳过当前游戏。</p>
      )}
      {Object.entries(page.active).map(([path, current]) => (
        <div className="active-item" key={path}>
          <span className="path">{current}</span>
          {onSkip && <button onClick={() => onSkip(path)}>跳过此游戏</button>}
        </div>
      ))}
    </section>
  );
}
function CandidateRow({
  candidate,
  canLink,
  onLink,
  registeredGame,
  expanded,
  onExpanded,
  choice,
  selected,
  disabled,
  onToggle,
  onChoice,
  onDeep,
  onError,
}: {
  candidate: ScanCandidate;
  canLink: boolean;
  onLink: () => void;
  registeredGame?: Game;
  expanded: boolean;
  onExpanded: (expanded: boolean) => void;
  choice?: Choice;
  selected: boolean;
  disabled: boolean;
  onToggle: (value: boolean) => void;
  onChoice: (value: Choice) => void;
  onDeep: () => void;
  onError: (error: string) => void;
}) {
  const [choosing, setChoosing] = useState(false);
  const exe = registeredGame
    ? registeredGame.main_executable
    : choice?.exe === undefined
      ? suggestedPlayer(candidate)
      : choice.exe;
  const qsp = registeredGame
    ? (registeredGame.external_player ?? null)
    : (choice?.external_player ?? qspConfig(candidate));
  const canSelect = canSelectScanCandidate(candidate, choice);
  return (
    <div className="scan-item">
      <div className="candidate-heading">
        <label className="check">
          <input
            type="checkbox"
            disabled={disabled || !!candidate.registered_id || !canSelect}
            checked={selected}
            onChange={(event) => onToggle(event.target.checked)}
          />
          <strong>{candidate.suggested_title}</strong>
        </label>
        <span className="muted">
          {candidate.registered_id ? '已入库' : statusText[candidate.status]}
        </span>
      </div>
      <div className="candidate-controls">
        {!qsp &&
          (registeredGame?.launch_type === 'MTOOL' ||
            (!candidate.registered_id &&
              candidate.mtool_detected &&
              !isAssociatedFile(exe) &&
              !!exe)) && <LaunchBadge label="公共 MTool" />}
        <span className="path" title={candidate.install_path}>
          {qsp
            ? `QSP 主游戏文件：${qsp.game_file || '未选择'}`
            : exe
              ? `启动文件：${exe}`
              : candidate.registered_id
                ? '启动文件未配置，请在游戏详情中设置'
                : candidate.status === 'pending'
                  ? '等待分析…'
                  : '请选择启动文件'}
        </span>
        {!candidate.registered_id && !qsp && (
          <button disabled={disabled} onClick={() => onExpanded(!expanded)}>
            更改启动文件
          </button>
        )}
        {candidate.registered_id ? (
          <span className="registered-version">
            入库版本：
            {displayVersion(registeredGame?.current_version || candidate.suggested_version)}
          </span>
        ) : (
          <label>
            版本
            <input
              aria-label={`${candidate.suggested_title}版本`}
              disabled={disabled || !!candidate.registered_id}
              placeholder="-"
              value={versionInput(choice?.version ?? candidate.suggested_version)}
              onChange={(event) =>
                onChoice({ version: event.target.value || 'Unknown', versionManual: true })
              }
            />
          </label>
        )}
      </div>
      <div className="candidate-meta">
        <span className="path" title={candidate.install_path}>
          {candidate.install_path}
        </span>
        <span>目录修改：{directoryTime(candidate.directory_modified_ms)}</span>
      </div>
      {!!candidate.save_paths?.length && (
        <p className="detected-saves">
          识别存档：{candidate.save_paths.map((path) => path.replace('<GAME>/', '')).join('、')}
        </p>
      )}
      {!candidate.registered_id && canLink && (
        <button className="link-record-button" disabled={disabled} onClick={onLink}>
          关联已有游戏…
        </button>
      )}
      {qsp && !candidate.registered_id && (
        <QspConfiguration
          root={candidate.install_path}
          player={exe}
          config={qsp}
          detection={candidate.qsp}
          disabled={disabled}
          onChange={(player, config) => onChoice({ exe: player, external_player: config })}
          onError={onError}
        />
      )}
      {expanded && !candidate.registered_id && !qsp && (
        <div className="override">
          <select
            aria-label="选择候选启动文件"
            disabled={disabled}
            value={exe || ''}
            onChange={(event) => onChoice({ exe: optional(event.target.value) })}
          >
            <option value="">留待配置</option>
            {exe && !candidate.executables.some((e) => e.relative_path === exe) && (
              <option value={exe}>{exe}</option>
            )}
            {candidate.executables.map((e) => (
              <option key={e.relative_path} value={e.relative_path}>
                {e.relative_path}
              </option>
            ))}
          </select>
          <BrowseButton
            disabled={disabled || choosing}
            onClick={() => {
              setChoosing(true);
              void api
                .chooseLaunchFile(candidate.install_path)
                .then((value) => {
                  if (value)
                    onChoice(
                      isQspFile(value)
                        ? {
                            exe: candidate.qsp?.recommended_player || null,
                            external_player: localQsp(value),
                          }
                        : { exe: value },
                    );
                })
                .catch((error) => onError(String(error)))
                .finally(() => setChoosing(false));
            }}
          >
            {choosing ? '选择中…' : '浏览启动文件…'}
          </BrowseButton>
          <button disabled={disabled} onClick={onDeep}>
            单独深度分析
          </button>
        </div>
      )}
      {candidate.status === 'error' && (
        <p className="error">目录无法完整读取，可检查路径后重试。</p>
      )}
    </div>
  );
}

function DiscardConfirmation({
  onDiscard,
  onKeep,
  actionLabel = '放弃并关闭',
}: {
  onDiscard: () => void;
  onKeep: () => void;
  actionLabel?: string;
}) {
  return (
    <Modal title="放弃未保存修改？" variant="confirm" onClose={onKeep}>
      <p>资料尚未保存，可以继续编辑或放弃修改。</p>
      <div className="footer-actions">
        <button onClick={onKeep}>继续编辑</button>
        <button onClick={onDiscard}>{actionLabel}</button>
      </div>
    </Modal>
  );
}
function SettingsPage({
  settings,
  appearance,
  appearanceReady,
  onAppearanceSave,
  initialSection,
  scrollPositions,
  navigationGuard,
  locked,
  onSave,
  onReset,
}: {
  settings: Settings;
  appearance: Appearance;
  appearanceReady: boolean;
  onAppearanceSave: (next: Appearance) => Promise<void>;
  initialSection: SettingsSection;
  scrollPositions: RefObject<Record<SettingsSection, number>>;
  navigationGuard: RefObject<((go: () => void) => void) | null>;
  locked: boolean;
  onSave: (settings: Settings) => Promise<void>;
  onReset: (confirmation: string) => Promise<void>;
}) {
  const [draft, setDraft] = useState(settings);
  const section = initialSection;
  const sectionScroll = useRef<HTMLDivElement>(null);
  useLayoutEffect(() => {
    if (sectionScroll.current) sectionScroll.current.scrollTop = scrollPositions.current[section];
  }, [section, scrollPositions]);
  const pendingNavigation = useRef<(() => void) | null>(null);
  useEffect(() => {
    setDraft(settings);
  }, [settings]);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');
  const [confirmClose, setConfirmClose] = useState(false);
  const [resetOpen, setResetOpen] = useState(false);
  const [confirmation, setConfirmation] = useState('');
  const dirty = JSON.stringify(settings) !== JSON.stringify(draft);
  const [toolChecks, setToolChecks] = useState<ToolCheck[]>([]);
  const [toolMessage, setToolMessage] = useState('');
  useEffect(() => {
    setToolChecks([]);
    setToolMessage('');
  }, [settings]);
  async function toolAction(action: () => Promise<void>) {
    setBusy(true);
    setError('');
    setToolMessage('');
    try {
      await action();
    } catch (reason) {
      setError(String(reason));
    } finally {
      setBusy(false);
    }
  }
  function requestNavigation(go: () => void) {
    if (busy) return;
    if (dirty) {
      pendingNavigation.current = go;
      setConfirmClose(true);
    } else go();
  }
  useEffect(() => {
    navigationGuard.current = requestNavigation;
    return () => {
      navigationGuard.current = null;
    };
  });
  function field(key: keyof Settings, value: string) {
    setDraft((current) => ({ ...current, [key]: value }));
  }
  async function browse(key: 'game_root' | 'mtool_root') {
    setBusy(true);
    setError('');
    try {
      const path = await api.chooseDirectory();
      if (path) field(key, path);
    } catch (reason) {
      setError(String(reason));
    } finally {
      setBusy(false);
    }
  }
  return (
    <section className="settings-page" aria-label={section === 'mtool' ? 'MTool页面' : '设置页面'}>
      <PageHeader
        icon={section === 'mtool' ? 'tool' : 'settings'}
        title={section === 'mtool' ? 'MTool' : '设置'}
        description={
          section === 'mtool'
            ? '配置共享 MTool 的目录与启动程序，配置仅保存在本机。'
            : '选择已有游戏目录，配置扫描线程与本地资料。'
        }
      />
      {confirmClose && (
        <DiscardConfirmation
          actionLabel="放弃并切换"
          onDiscard={() => {
            setConfirmClose(false);
            setDraft(settings);
            pendingNavigation.current?.();
            pendingNavigation.current = null;
          }}
          onKeep={() => {
            setConfirmClose(false);
            pendingNavigation.current = null;
          }}
        />
      )}
      <div
        className="settings-scroll"
        ref={sectionScroll}
        onScroll={(event) => {
          scrollPositions.current[section] = event.currentTarget.scrollTop;
        }}
      >
        <form
          onSubmit={(event) => {
            event.preventDefault();
            setBusy(true);
            setError('');
            void onSave(draft)
              .catch((reason) => setError(String(reason)))
              .finally(() => setBusy(false));
          }}
        >
          {(error || locked) && (
            <div className={error ? 'error' : 'notice'} role={error ? 'alert' : 'status'}>
              {error || '当前任务或启动请求结束后可修改设置或清空库。'}
            </div>
          )}
          <fieldset disabled={busy || locked || confirmClose || resetOpen}>
            <section className="settings-card panel" hidden={section !== 'library'}>
              <CardTitle icon="folder" description="扫描根目录中的游戏，游戏文件保留原位。">
                游戏库目录
              </CardTitle>
              <div className="form-grid">
                <label className="full">
                  游戏库根目录（Game Root）
                  <div className="path-input">
                    <input
                      value={draft.game_root}
                      onChange={(event) => field('game_root', event.target.value)}
                    />
                    <BrowseButton type="button" onClick={() => void browse('game_root')}>
                      浏览目录…
                    </BrowseButton>
                  </div>
                </label>
                <label>
                  扫描线程
                  <select
                    value={draft.scan_workers}
                    onChange={(event) =>
                      setDraft((current) => ({
                        ...current,
                        scan_workers: Number(event.target.value),
                      }))
                    }
                  >
                    <option value={1}>低占用 · 1 线程</option>
                    <option value={2}>均衡 · 2 线程（默认）</option>
                    <option value={4}>性能 · 4 线程</option>
                  </select>
                  <span className="muted">通常保留默认即可，保存后用于下一次扫描。</span>
                </label>
              </div>
            </section>
            <section className="settings-card panel" hidden={section !== 'mtool'}>
              <CardTitle icon="tool" description="让多个游戏使用同一套 MTool。">
                共享 MTool
              </CardTitle>
              <div className="header-actions mtool-actions">
                <button
                  type="button"
                  className="primary"
                  disabled={dirty || !settings.mtool_root}
                  onClick={() =>
                    void toolAction(async () => {
                      await api.runMtool();
                      setToolMessage('MTool 启动请求已发送。');
                    })
                  }
                >
                  <Icon name="play" />
                  运行 MTool
                </button>
                <button
                  type="button"
                  disabled={dirty || !settings.mtool_root}
                  onClick={() =>
                    void toolAction(async () => {
                      setToolChecks(await api.checkMtool());
                      setToolMessage(
                        '文件检查完成，已检查公共 MTool、注入器及 32 / 64 位 Loader。',
                      );
                    })
                  }
                >
                  验证 MTool 文件
                </button>
              </div>
              <p className="muted">
                直接打开共享目录中的 MTool.exe，工作目录使用共享目录。修改配置后请先保存。
              </p>
              {toolMessage && <p role="status">{toolMessage}</p>}
              {!!toolChecks.length && (
                <ul className="tool-checks">
                  {toolChecks.map((check) => (
                    <li key={check.label}>
                      <strong>
                        {check.label} · {check.available ? '可访问' : '需检查'}
                      </strong>
                      <span className="path">{check.path}</span>
                      <span className="muted">{check.message}</span>
                    </li>
                  ))}
                </ul>
              )}
              <div className="form-grid">
                <label className="full">
                  共享 MTool 目录（Shared MTool Root）
                  <div className="path-input">
                    <input
                      value={draft.mtool_root}
                      onChange={(event) => field('mtool_root', event.target.value)}
                    />
                    <BrowseButton type="button" onClick={() => void browse('mtool_root')}>
                      浏览目录…
                    </BrowseButton>
                  </div>
                </label>
                <label>
                  注入器路径（相对共享目录）
                  <input
                    required
                    value={draft.mtool_injector}
                    onChange={(event) => field('mtool_injector', event.target.value)}
                  />
                </label>
                <label>
                  运行程序路径（相对共享目录）
                  <input
                    required
                    value={draft.mtool_runtime}
                    onChange={(event) => field('mtool_runtime', event.target.value)}
                  />
                </label>
              </div>
            </section>
            <div className="footer-actions">
              <span className="muted">配置保存在本地。</span>
              <button className="primary" disabled={!dirty}>
                保存设置
              </button>
            </div>
          </fieldset>
        </form>
        {section === 'library' && (
          <AppearancePicker
            value={appearance}
            disabled={!appearanceReady || busy || locked || confirmClose}
            onSave={onAppearanceSave}
          />
        )}
        <section className="reset-library panel" hidden={section !== 'library'}>
          <h3>清空游戏库</h3>
          <p>
            删除数据库中的全部游戏、别名、存档路径记录、运行历史和设置。游戏与存档文件保持原位。
          </p>
          {resetOpen ? (
            <div role="alert">
              <label>
                输入“清空数据库”确认
                <input
                  aria-label="清空确认"
                  value={confirmation}
                  disabled={busy}
                  onChange={(e) => setConfirmation(e.target.value)}
                />
              </label>
              <div className="footer-actions">
                <button
                  disabled={busy}
                  onClick={() => {
                    setResetOpen(false);
                    setConfirmation('');
                  }}
                >
                  取消清空
                </button>
                <button
                  className="danger"
                  disabled={busy || locked || confirmation !== '清空数据库'}
                  onClick={() => {
                    setBusy(true);
                    setError('');
                    void onReset(confirmation)
                      .catch((reason) => setError(String(reason)))
                      .finally(() => setBusy(false));
                  }}
                >
                  {busy ? '正在清空…' : '确认清空数据库'}
                </button>
              </div>
            </div>
          ) : (
            <button
              className="danger"
              disabled={busy || locked || confirmClose}
              onClick={() => setResetOpen(true)}
            >
              清空库…
            </button>
          )}
        </section>
        <section className="about-card panel" hidden={section !== 'library'} aria-label="关于应用">
          <div className="settings-about-heading">
            <div className="settings-about-identity">
              <img src={appearanceIcons[appearance.icon]} alt="" />
              <div>
                <h3>关于 butter-manager</h3>
                <p>by Zhehao-w</p>
              </div>
            </div>
            <span className="mode">v0.3.1</span>
          </div>
          <p>本地游戏库 · 整理游戏、批量导入、保留存档更新与快捷启动。</p>
          <BrowseButton disabled={busy} onClick={() => void toolAction(api.openDataDirectory)}>
            打开数据目录
          </BrowseButton>
          <details className="icon-credits">
            <summary>开源图标：Lucide · 许可说明</summary>
            <CodeBlock>{iconLicense}</CodeBlock>
          </details>
          <details className="icon-credits">
            <summary>虚拟列表：TanStack Virtual · 许可说明</summary>
            <CodeBlock>{virtualLicense}</CodeBlock>
          </details>
        </section>
      </div>
    </section>
  );
}

function GameDetail({
  game,
  settings,
  pathCheck,
  onRemoved,
  onDeleted,
  onRelocated,
  analysis,
  analysisNotice,
  analysisJob,
  analyzing,
  analysisLocked,
  onAnalyze,
  onCancelAnalysis,
  launching,
  onPlay,
  onSaved,
  onClose,
}: {
  game: Game;
  settings: Settings;
  pathCheck?: LibraryPathCheck;
  onRemoved: () => Promise<void>;
  onDeleted: (report: DeleteReport) => void;
  onRelocated: (game: Game) => void;
  analysis: ScanCandidate | null;
  analysisNotice: string;
  analysisJob: JobPage | null;
  analyzing: boolean;
  analysisLocked: boolean;
  onAnalyze: () => Promise<void>;
  onCancelAnalysis: () => Promise<void>;
  launching: boolean;
  onPlay: (configuration: LaunchConfiguration) => Promise<void>;
  onSaved: (game: Game) => void;
  onClose: () => void;
}) {
  const [draft, setDraft] = useState<GameEdit>({ ...game });
  const formId = useId();
  const detailTabs = [
    ['basic', '基础信息', 'info'],
    ['launch', '启动配置', 'play'],
    ['saves', '存档位置', 'save'],
    ['aliases', '别名管理', 'tag'],
    ['records', '记录与管理', 'clock'],
  ] as const;
  type DetailTab = (typeof detailTabs)[number][0];
  const [detailTab, setDetailTab] = useState<DetailTab>('basic');
  function selectDetailTab(tab: DetailTab) {
    setDetailTab(tab);
    const body = document.getElementById(formId)?.closest('.modal-body');
    if (body) body.scrollTop = 0;
  }
  const panelProps = (tab: DetailTab) => ({
    id: `${formId}-${tab}-panel`,
    role: 'tabpanel',
    'aria-labelledby': `${formId}-${tab}-tab`,
    hidden: detailTab !== tab,
    'data-detail-tab': tab,
  });
  const previousStatus = useRef(game.play_status);
  useEffect(() => {
    const old = previousStatus.current;
    setDraft((current) =>
      current.play_status === old ? { ...current, play_status: game.play_status } : current,
    );
    previousStatus.current = game.play_status;
  }, [game.play_status]);
  const [aliases, setAliases] = useState(game.aliases.join('\n'));
  const [saves, setSaves] = useState(game.save_paths.join('\n'));
  const [saveToOpen, setSaveToOpen] = useState('');
  const [saveEditorOpen, setSaveEditorOpen] = useState(false);
  const saveLocations = lines(saves);
  const selectedSave = saveLocations.includes(saveToOpen) ? saveToOpen : saveLocations[0];
  const previousSaves = useRef(game.save_paths.join('\n'));
  useEffect(() => {
    const previous = previousSaves.current;
    const next = game.save_paths.join('\n');
    setSaves((current) => (current === previous ? next : current));
    previousSaves.current = next;
  }, [game.save_paths]);
  const [history, setHistory] = useState<string[]>([]);
  const [versions, setVersions] = useState<VersionHistory[]>([]);
  const [versionError, setVersionError] = useState('');
  useEffect(() => {
    let stale = false;
    setVersionError('');
    void api
      .versionHistory(game.id)
      .then((values) => {
        if (!stale) setVersions(values);
      })
      .catch((error) => {
        if (!stale) setVersionError(String(error));
      });
    return () => {
      stale = true;
    };
  }, [game.id, game.current_version]);
  const [historyError, setHistoryError] = useState('');
  useEffect(() => {
    let stale = false;
    setHistoryError('');
    api
      .launchHistory(game.id)
      .then((times) => {
        if (!stale) setHistory(times);
      })
      .catch((reason) => {
        if (!stale) setHistoryError(String(reason));
      });
    return () => {
      stale = true;
    };
  }, [game.id, game.last_launched_at]);
  const [debugText, setDebugText] = useState('');
  const [requestBusy, setBusy] = useState(false);
  const [maintenanceBusy, setMaintenanceBusy] = useState(false);
  const [folderError, setFolderError] = useState<string | null>(null);
  const busy = requestBusy || maintenanceBusy;
  const [message, setMessage] = useState('');
  const [confirmClose, setConfirmClose] = useState(false);
  const edit = { ...draft, aliases: lines(aliases), save_paths: lines(saves) };
  const configuration: LaunchConfiguration = {
    main_executable: draft.main_executable,
    working_directory: draft.working_directory,
    launch_type: draft.launch_type,
    external_player: draft.external_player ?? null,
    mtool_target_exe: draft.mtool_target_exe,
    mtool_loader: draft.mtool_loader,
  };
  const launchChanged = Object.keys(configuration).some(
    (key) =>
      JSON.stringify(configuration[key as keyof LaunchConfiguration] ?? null) !==
      JSON.stringify(game[key as keyof LaunchConfiguration] ?? null),
  );
  // A check of the saved launch file must not block a newly selected file.
  // The backend validates the current paths again before executing anything.
  const launchCheck =
    !launchChanged || pathCheck?.state === 'missing_directory' ? pathCheck : undefined;
  const dirty =
    (
      [
        'canonical_title',
        'display_title',
        'current_version',
        'engine',
        'play_status',
        'main_executable',
        'working_directory',
        'launch_type',
        'mtool_target_exe',
        'mtool_loader',
      ] as const
    ).some((key) => draft[key] !== game[key]) ||
    JSON.stringify(draft.external_player ?? null) !==
      JSON.stringify(game.external_player ?? null) ||
    aliases !== game.aliases.join('\n') ||
    saves !== game.save_paths.join('\n');
  const detailStatus =
    message || analysisNotice || (dirty ? '资料尚未保存；启动使用当前配置。' : '');
  const close = () => {
    if (busy) return;
    if (dirty) setConfirmClose(true);
    else onClose();
  };
  function field<K extends keyof GameEdit>(key: K, value: GameEdit[K]) {
    setDraft((current) => ({ ...current, [key]: value }));
    setDebugText('');
  }
  async function run(action: () => Promise<void>) {
    setBusy(true);
    setMessage('');
    try {
      await action();
    } catch (reason) {
      setMessage(String(reason));
    } finally {
      setBusy(false);
    }
  }
  function useRecipe(recipe: MToolRecipe) {
    setDraft((current) => ({
      ...current,
      launch_type: 'MTOOL',
      external_player: null,
      main_executable: recipe.target_exe,
      working_directory:
        analysis?.bats
          .find((bat) => bat.recipe === recipe)
          ?.path.replaceAll('\\', '/')
          .split('/')
          .slice(0, -1)
          .join('/') || '.',
      mtool_target_exe: recipe.target_exe,
      mtool_loader: recipe.loader,
    }));
    setDebugText('');
  }
  function useQsp() {
    setDraft((current) => ({
      ...current,
      engine: 'QSP',
      launch_type: 'EXTERNAL_PLAYER',
      main_executable:
        (current.launch_type === 'EXTERNAL_PLAYER' ? current.main_executable : null) ||
        analysis?.qsp?.recommended_player ||
        (current.main_executable?.toLowerCase().endsWith('.exe') ? current.main_executable : null),
      external_player:
        current.external_player ??
        (isQspFile(current.main_executable) ? localQsp(current.main_executable) : null) ??
        (analysis ? qspConfig(analysis) : null) ??
        localQsp(),
      working_directory: '.',
      mtool_target_exe: null,
      mtool_loader: null,
    }));
    setDebugText('');
  }
  return (
    <Modal
      title={game.display_title}
      onClose={close}
      variant="detail"
      headerActions={
        <button
          type="submit"
          form={formId}
          className="primary"
          disabled={!dirty || busy || launching || confirmClose}
        >
          保存资料
        </button>
      }
    >
      <div className="detail-hero panel">
        <GameMark game={{ ...game, engine: draft.engine }} large />
        <div className="detail-hero-info">
          <h1>{game.display_title}</h1>
          <div className="alias-chips">
            {game.aliases.map((alias) => (
              <span key={alias}>{alias}</span>
            ))}
          </div>
          <div className="card-badges">
            <span className="mode detail-version-badge">
              <Icon name="tag" size={14} />
              {displayVersion(game.current_version)}
            </span>
            <LaunchBadge game={{ ...game, ...configuration }} />
            <PlayBadge game={game} />
            <span className={`mode engine-badge tone-${engineTone(draft.engine)}`}>
              {draft.engine === 'Unknown' ? '引擎未识别' : draft.engine}
            </span>
          </div>
          <div className="record-times">
            <p>入库时间：{formatTime(game.created_at)}</p>
            <p>上次运行：{formatTime(game.last_launched_at)}</p>
          </div>
        </div>
        <div className="detail-actions">
          <button
            className="primary play-button"
            disabled={
              busy ||
              launching ||
              confirmClose ||
              !!launchBlockReason({ ...game, ...configuration }, launchCheck)
            }
            onClick={() => {
              setMessage('');
              void onPlay(configuration)
                .then(() => setMessage('启动请求已发送。'))
                .catch((reason) => setMessage(String(reason)));
            }}
          >
            <Icon name="play" />
            {launching ? '启动中…' : '启动'}
          </button>
          <button
            disabled={busy || launching}
            onClick={() =>
              void run(async () => {
                try {
                  await api.openFolder(game.id);
                } catch (reason) {
                  setFolderError(String(reason));
                }
              })
            }
          >
            打开目录
          </button>
          <button
            disabled={busy || launching || analysisLocked}
            onClick={() => {
              selectDetailTab('launch');
              void run(onAnalyze);
            }}
          >
            分析启动配置
          </button>
          <button
            type="button"
            className="save-editor-entry"
            disabled={busy || confirmClose}
            onClick={() => setSaveEditorOpen(true)}
          >
            <Icon name="save" size={16} />
            编辑存档
          </button>
        </div>
      </div>
      {confirmClose && (
        <DiscardConfirmation onDiscard={onClose} onKeep={() => setConfirmClose(false)} />
      )}
      {folderError !== null && (
        <FolderErrorDialog
          game={game}
          reason={folderError}
          locked={dirty || launching || analysisLocked}
          onClose={() => setFolderError(null)}
          onRemoved={onRemoved}
          onBusy={setMaintenanceBusy}
        />
      )}
      <div className="detail-meta-row">
        <p className="path detail-install-path" title={game.install_path}>
          {game.install_path}
        </p>
        <div className="inline-status" role="status" title={detailStatus}>
          {detailStatus}
        </div>
      </div>
      <PathBadge check={pathCheck} />
      {pathCheck && pathCheck.state !== 'available' && (
        <p className="muted">{pathCheck.message}。可在下方关联新目录或移除库记录。</p>
      )}
      {analyzing && analysisJob && (
        <TaskProgress page={analysisJob} onCancel={() => void run(onCancelAnalysis)} />
      )}
      <div className="detail-tabs" role="tablist" aria-label="游戏详情分类">
        {detailTabs.map(([key, label, icon], index) => (
          <button
            type="button"
            key={key}
            id={`${formId}-${key}-tab`}
            role="tab"
            aria-selected={detailTab === key}
            aria-controls={`${formId}-${key}-panel`}
            tabIndex={detailTab === key ? 0 : -1}
            onClick={() => selectDetailTab(key)}
            onKeyDown={(event) => {
              const offset = event.key === 'ArrowRight' ? 1 : event.key === 'ArrowLeft' ? -1 : 0;
              if (!offset && event.key !== 'Home' && event.key !== 'End') return;
              event.preventDefault();
              const next =
                event.key === 'Home'
                  ? 0
                  : event.key === 'End'
                    ? detailTabs.length - 1
                    : (index + offset + detailTabs.length) % detailTabs.length;
              const tab = detailTabs[next][0];
              selectDetailTab(tab);
              document.getElementById(`${formId}-${tab}-tab`)?.focus();
            }}
          >
            <Icon name={icon} size={16} />
            {label}
          </button>
        ))}
      </div>
      <form
        id={formId}
        onInvalidCapture={(event) => {
          const panel = (event.target as HTMLElement).closest<HTMLElement>('[data-detail-tab]');
          if (panel) {
            event.preventDefault();
            selectDetailTab(panel.dataset.detailTab as DetailTab);
            const input = event.target as HTMLElement;
            const details = input.closest('details');
            if (details) details.open = true;
            requestAnimationFrame(() => input.focus());
          }
        }}
        onSubmit={(event) => {
          event.preventDefault();
          void run(async () => {
            const saved = await api.saveGame(edit);
            setDraft({ ...saved });
            setAliases(saved.aliases.join('\n'));
            setSaves(saved.save_paths.join('\n'));
            onSaved(saved);
            setMessage('游戏资料已保存。');
          });
        }}
      >
        <fieldset disabled={busy || launching || confirmClose}>
          <div className="detail-card-grid">
            <section className="panel detail-card" {...panelProps('basic')}>
              <CardTitle icon="info">基础信息</CardTitle>
              <div className="form-grid">
                <label>
                  显示名称
                  <input
                    required
                    value={draft.display_title}
                    onChange={(event) => field('display_title', event.target.value)}
                  />
                </label>
                <label>
                  标准名称
                  <input
                    required
                    value={draft.canonical_title}
                    onChange={(event) => field('canonical_title', event.target.value)}
                  />
                </label>
                <label>
                  游戏引擎
                  <EngineSelect
                    value={draft.engine}
                    onChange={(engine) => {
                      if (engine === 'QSP') useQsp();
                      else field('engine', engine);
                    }}
                  />
                  <span className="muted">手动修改后会保留；也可选择自定义引擎。</span>
                </label>
                <label>
                  游玩状态
                  <select
                    value={draft.play_status ?? 'UNPLAYED'}
                    onChange={(event) =>
                      field('play_status', event.target.value as GameEdit['play_status'])
                    }
                  >
                    {Object.entries(playLabels).map(([key, label]) => (
                      <option key={key} value={key}>
                        {label}
                      </option>
                    ))}
                  </select>
                  <span className="muted">成功启动会标记为正在玩；已通关会保留。</span>
                </label>
                <label>
                  版本
                  <input
                    placeholder="-"
                    value={versionInput(draft.current_version)}
                    onChange={(event) => field('current_version', event.target.value || 'Unknown')}
                  />
                </label>
              </div>
            </section>
            <section className="panel detail-card" {...panelProps('launch')}>
              <CardTitle icon="play">启动配置</CardTitle>
              {(analysis?.qsp || draft.engine === 'QSP') &&
                draft.launch_type !== 'EXTERNAL_PLAYER' && (
                  <div className="qsp-detection-hint">
                    <span className="muted">目录中发现 .qsp 文件，可选择 QSP 启动。</span>
                    <button type="button" onClick={useQsp}>
                      使用 QSP 启动
                    </button>
                  </div>
                )}
              <div className="form-grid">
                <label className="full">
                  启动方式
                  <select
                    value={draft.launch_type}
                    onChange={(event) => {
                      const mode = event.target.value as GameEdit['launch_type'];
                      if (mode === 'EXTERNAL_PLAYER') {
                        useQsp();
                        return;
                      }
                      setDraft((current) => ({
                        ...current,
                        launch_type: mode,
                        mtool_target_exe: mode === 'MTOOL' ? current.main_executable : null,
                        working_directory:
                          mode === 'MTOOL' ? '.' : parentOf(current.main_executable),
                        external_player: null,
                      }));
                      setDebugText('');
                    }}
                  >
                    <option value="DIRECT">直接启动 / 默认应用</option>
                    <option value="MTOOL">MTool</option>
                    <option value="EXTERNAL_PLAYER">QSP / 外部播放器</option>
                  </select>
                </label>
                {draft.launch_type === 'EXTERNAL_PLAYER' ? (
                  <QspConfiguration
                    root={game.install_path}
                    player={draft.main_executable}
                    config={draft.external_player ?? localQsp()}
                    detection={analysis?.qsp}
                    disabled={busy || analyzing}
                    onChange={(player, config) => {
                      field('main_executable', player);
                      field('external_player', config);
                    }}
                    onError={setMessage}
                  />
                ) : (
                  <label className="full">
                    启动文件（相对游戏目录）
                    <div className="path-input">
                      <input
                        list="exe-candidates"
                        onBlur={(event) => {
                          const file = optional(event.target.value);
                          if (isQspFile(file))
                            setDraft((current) => ({
                              ...current,
                              engine: 'QSP',
                              launch_type: 'EXTERNAL_PLAYER',
                              main_executable: null,
                              external_player: localQsp(file),
                              working_directory: '.',
                            }));
                          else if (isAssociatedFile(file)) field('launch_type', 'DIRECT');
                        }}
                        value={draft.main_executable || ''}
                        onChange={(event) => {
                          const exe = optional(event.target.value);
                          field('main_executable', exe);
                          if (draft.launch_type === 'MTOOL') field('mtool_target_exe', exe);
                          else field('working_directory', parentOf(exe));
                        }}
                      />
                      <BrowseButton
                        type="button"
                        onClick={() =>
                          void run(async () => {
                            const exe = await api.chooseLaunchFile(game.install_path);
                            if (exe) {
                              if (isQspFile(exe)) {
                                setDraft((current) => ({
                                  ...current,
                                  engine: 'QSP',
                                  launch_type: 'EXTERNAL_PLAYER',
                                  main_executable: analysis?.qsp?.recommended_player || null,
                                  external_player: localQsp(exe),
                                  working_directory: '.',
                                }));
                                return;
                              }
                              field('main_executable', exe);
                              if (draft.launch_type === 'MTOOL') field('mtool_target_exe', exe);
                              else field('working_directory', parentOf(exe));
                              if (isAssociatedFile(exe)) {
                                field('launch_type', 'DIRECT');
                                field('working_directory', parentOf(exe));
                              }
                            }
                          })
                        }
                      >
                        浏览启动文件…
                      </BrowseButton>
                    </div>
                    <span className="muted">
                      EXE 直接运行，其他文档用 Windows 默认应用打开。QSP 使用本地播放器启动。
                    </span>
                  </label>
                )}
                <details className="full advanced-launch">
                  <summary>高级启动选项</summary>
                  <label>
                    启动工作目录（相对游戏目录，. 表示根目录）
                    <input
                      required
                      value={draft.working_directory}
                      onChange={(event) => field('working_directory', event.target.value)}
                    />
                  </label>
                </details>
                {draft.launch_type === 'MTOOL' && (
                  <MToolConfiguration
                    id={game.id}
                    executable={draft.mtool_target_exe || draft.main_executable}
                    loader={draft.mtool_loader}
                    workingDirectory={draft.working_directory}
                    settings={settings}
                    onLoader={(loader) => field('mtool_loader', loader)}
                  />
                )}
              </div>

              {analysis && (
                <details className="analysis" open>
                  <summary>查看分析详情</summary>
                  <p>
                    识别引擎：{analysis.engine === 'Unknown' ? '未识别' : analysis.engine}{' '}
                    {analysis.status === 'ready' &&
                      analysis.engine !== 'Unknown' &&
                      analysis.engine !== draft.engine && (
                        <button
                          type="button"
                          disabled={busy || launching || analyzing}
                          onClick={() => field('engine', analysis.engine)}
                        >
                          使用识别引擎
                        </button>
                      )}
                  </p>
                  <Warnings warnings={analysis.warnings} />
                  <ul>
                    {analysis.executables.map((exe) => (
                      <li key={exe.relative_path}>
                        {exe.relative_path} · {exe.architecture}
                      </li>
                    ))}
                  </ul>
                  <BatResults candidate={analysis} onUse={useRecipe} disabled={busy || launching} />
                </details>
              )}
              {game.launch_type === 'MTOOL' && (
                <div className="analysis">
                  <button
                    type="button"
                    disabled={busy || dirty}
                    onClick={() =>
                      void run(async () => {
                        setDebugText(await api.debugBat(game.id));
                      })
                    }
                  >
                    预览调试 BAT
                  </button>
                  {debugText && <CodeBlock>{debugText}</CodeBlock>}
                </div>
              )}
            </section>
            <section className="panel detail-card" {...panelProps('aliases')}>
              <CardTitle icon="tag">别名管理</CardTitle>
              <div className="form-grid">
                <label className="full">
                  别名（每行一个）
                  <div className="text-scroll-shell">
                    <textarea
                      rows={3}
                      value={aliases}
                      onChange={(event) => setAliases(event.target.value)}
                    />
                  </div>
                </label>
              </div>
            </section>
            <section className="panel detail-card" {...panelProps('saves')}>
              <CardTitle icon="save">存档位置</CardTitle>
              <div className="form-grid">
                <label className="full">
                  存档位置（每行一个）
                  <div className="text-scroll-shell">
                    <textarea
                      rows={3}
                      value={saves}
                      onChange={(event) => setSaves(event.target.value)}
                      placeholder={'<GAME>\\save\n%APPDATA%\\游戏名'}
                    />
                  </div>
                  <span className="muted">
                    关联存档路径；更新时可选择迁移，删除游戏时一并移入回收站。
                  </span>
                </label>
                {saveLocations.length > 1 && (
                  <label className="full">
                    打开的存档位置
                    <select
                      value={selectedSave}
                      onChange={(event) => setSaveToOpen(event.target.value)}
                    >
                      {saveLocations.map((path) => (
                        <option key={path} value={path}>
                          {path}
                        </option>
                      ))}
                    </select>
                  </label>
                )}
                <div className="save-location-actions full">
                  <button
                    type="button"
                    className="save-editor-entry"
                    onClick={() => setSaveEditorOpen(true)}
                  >
                    <Icon name="save" size={16} />
                    编辑存档
                  </button>
                  <BrowseButton
                    type="button"
                    disabled={!selectedSave}
                    onClick={() => void run(() => api.openSaveFolder(game.id, selectedSave!))}
                  >
                    打开存档位置
                  </BrowseButton>
                  <button
                    type="button"
                    onClick={() =>
                      void run(async () => {
                        const path = await api.chooseSaveDirectory(game.id, selectedSave ?? null);
                        if (path)
                          setSaves((current) => [...new Set([...lines(current), path])].join('\n'));
                      })
                    }
                  >
                    添加存档目录…
                  </button>
                </div>
              </div>
            </section>
          </div>
          <datalist id="exe-candidates">
            {analysis?.executables.map((exe) => (
              <option key={exe.relative_path} value={exe.relative_path} />
            ))}
          </datalist>
          <div className="footer-actions" hidden={detailTab !== 'basic'}>
            <span className="muted">
              版本来源：
              {(
                {
                  folder_name: '目录名',
                  file_name: '启动文件名',
                  manual: '手动设置',
                  unknown: '未识别',
                } as Record<string, string>
              )[game.version_source] || '未识别'}
            </span>
          </div>
        </fieldset>
      </form>
      <div {...panelProps('records')}>
        <GameMaintenance
          game={game}
          locked={dirty || launching || analysisLocked}
          onBusy={setMaintenanceBusy}
          onRemoved={onRemoved}
          onDeleted={onDeleted}
          onRelocated={(saved) => {
            setDraft({ ...saved });
            setAliases(saved.aliases.join('\n'));
            setSaves(saved.save_paths.join('\n'));
            onRelocated(saved);
          }}
        />

        <details className="analysis">
          <summary>版本更新历史</summary>
          {versionError ? (
            <p className="error">{versionError}</p>
          ) : versions.length ? (
            <ol>
              {versions.map((version) => (
                <li key={version.operation}>
                  {displayVersion(version.old_version)} → {displayVersion(version.new_version)} ·{' '}
                  {version.status === 'rolled_back' ? '已回退' : '已更新'} ·{' '}
                  {formatTime(version.created_at)}
                </li>
              ))}
            </ol>
          ) : (
            <p>暂无更新记录。版本回退可在导入页的已导入记录中操作。</p>
          )}
        </details>
        <details className="analysis">
          <summary>运行历史（最近 100 次启动请求）</summary>
          {historyError ? (
            <p className="error">{historyError}</p>
          ) : history.length ? (
            <ol>
              {history.map((time, i) => (
                <li key={`${time}-${i}`}>{formatTime(time)}</li>
              ))}
            </ol>
          ) : (
            <p>暂无运行记录。</p>
          )}
        </details>
      </div>
      {saveEditorOpen && <SaveEditor game={game} onClose={() => setSaveEditorOpen(false)} />}
    </Modal>
  );
}
function Warnings({ warnings }: { warnings: string[] }) {
  return warnings.length ? (
    <ul className="warnings">
      {warnings.map((warning, index) => (
        <li key={index}>{warning}</li>
      ))}
    </ul>
  ) : null;
}
function BatResults({
  candidate,
  onUse,
  disabled,
}: {
  candidate: ScanCandidate;
  onUse: (recipe: MToolRecipe) => void;
  disabled: boolean;
}) {
  return (
    <>
      {candidate.bats.map((bat) => (
        <details className="bat-result" key={bat.path}>
          <summary>
            {bat.path} · {bat.status === 'supported' ? '已识别 MTool' : '需人工检查'}
          </summary>
          <Warnings warnings={bat.messages} />
          {bat.recipe && (
            <>
              <dl>
                <dt>Target</dt>
                <dd>{bat.recipe.target_exe}</dd>
                <dt>Loader</dt>
                <dd>{bat.recipe.loader}</dd>
                <dt>原 MTool Root</dt>
                <dd>{bat.recipe.observed_root}</dd>
                <dt>Runtime</dt>
                <dd>{bat.recipe.runtime}</dd>
              </dl>
              <p className="muted">采用 target / loader；共享 root 和 runtime 另在设置确认。</p>
              <button type="button" disabled={disabled} onClick={() => onUse(bat.recipe!)}>
                采用此 target / loader
              </button>
            </>
          )}
        </details>
      ))}
    </>
  );
}
