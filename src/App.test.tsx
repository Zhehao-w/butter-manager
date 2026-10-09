import { beforeEach, describe, expect, it, vi } from 'vitest';
import { fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { api } from './api';
import App from './App';
import type { Game, JobPage, ScanCandidate, Settings } from './types';

vi.mock('@tauri-apps/api/core', () => ({ isTauri: () => true }));
vi.mock('./api', () => ({
  api: {
    openDataDirectory: vi.fn().mockResolvedValue(undefined),
    appearance: vi.fn(),
    saveAppearance: vi.fn(),
    importPlans: vi.fn().mockResolvedValue([]),
    importRecoveryIssues: vi.fn().mockResolvedValue([]),
    openImportRecords: vi.fn(),
    games: vi.fn(),
    settings: vi.fn(),
    startScan: vi.fn(),
    job: vi.fn(),
    register: vi.fn(),
    suggestVersion: vi.fn(),
    chooseDirectory: vi.fn(),
    chooseSaveDirectory: vi.fn(),
    openSaveFolder: vi.fn().mockResolvedValue(undefined),
    openFolder: vi.fn(),
    saveSettings: vi.fn(),
    chooseLaunchFile: vi.fn(),
    choosePlayerFile: vi.fn(),
    launchHistory: vi.fn().mockResolvedValue([]),
    versionHistory: vi.fn().mockResolvedValue([]),
    clearLibrary: vi.fn(),
    refreshMetadata: vi.fn(),
    startLibraryCheck: vi.fn().mockResolvedValue(''),
    removeGame: vi.fn(),
    previewRelocation: vi.fn(),
    relocateGame: vi.fn(),
    checkMtool: vi.fn(),
    runMtool: vi.fn(),
    play: vi.fn(),
    cancel: vi.fn(),
    gamesByIds: vi.fn(),
    syncScanMtool: vi.fn().mockResolvedValue([]),
    previewMtoolLaunch: vi.fn().mockResolvedValue({
      shared_root: 'D:/Tools/MTool',
      target_exe: '任意.exe',
      architecture: 'x86',
      loader: 'loaders/mzHook32.dll',
      runtime: 'MTool.exe',
      working_directory: '.',
    }),
    deepAnalyze: vi.fn(),
    startGameAnalysis: vi.fn(),
    saveGame: vi.fn(),
  },
}));

const config: Settings = {
  game_root: 'E:/Butter',
  mtool_root: '',
  mtool_injector: 'loaders/inject.exe',
  mtool_runtime: 'MTool.exe',
  scan_workers: 2,
};
const candidate: ScanCandidate = {
  install_path: 'E:/Butter/游戏',
  directory_modified_ms: 1700000000000,
  suggested_title: '游戏',
  engine: 'Unknown',
  executables: [
    {
      relative_path: 'main-v1.2.exe',
      architecture: 'Unknown',
      score: 100,
      size_bytes: 1,
      modified_ms: 0,
    },
  ],
  bats: [],
  bundled_tool: false,
  warnings: ['不应显示的技术细节'],
  registered_id: null,
  suggested_version: 'v1.2',
  version_source: 'file_name',
  working_directory: '.',
  status: 'ready',
  entries_scanned: 1,
  elapsed_ms: 5,
};
const job: JobPage = {
  id: 'scan',
  kind: 'scan',
  root: config.game_root,
  status: 'completed',
  phase: '分析',
  total: 1,
  processed: 1,
  active: {},
  elapsed_ms: 5,
  idle_ms: 0,
  changes: [candidate],
  next_cursor: 1,
  change_count: 1,
  warnings: [],
  error: null,
  registered_ids: [],
};
const game: Game = {
  id: 'game',
  canonical_title: '已登记游戏',
  display_title: '已登记游戏',
  install_path: 'E:/Butter/已登记游戏',
  working_directory: '.',
  current_version: 'Final',
  version_source: 'manual',
  main_executable: '任意.exe',
  engine: 'Unknown',
  launch_type: 'DIRECT',
  mtool_target_exe: null,
  mtool_loader: null,
  created_at: '',
  last_launched_at: null,
  play_status: 'UNPLAYED',
  updated_at: '',
  aliases: [],
  save_paths: [],
};

function mockLibraryViewport() {
  // jsdom has no layout: provide real viewport and row sizes, keeping the real virtualizer.
  const viewport = { width: 900, height: 320 };
  for (const property of ['offsetWidth', 'clientWidth'] as const)
    vi.spyOn(HTMLElement.prototype, property, 'get').mockImplementation(function (
      this: HTMLElement,
    ) {
      return this.classList.contains('library-scroll') || this.classList.contains('scan-scroll')
        ? viewport.width
        : 0;
    });
  vi.spyOn(HTMLElement.prototype, 'offsetHeight', 'get').mockImplementation(function (
    this: HTMLElement,
  ) {
    if (this.classList.contains('library-scroll') || this.classList.contains('scan-scroll'))
      return viewport.height;
    if (this.classList.contains('virtual-row'))
      return this.querySelector('.game-link')?.textContent === 'Game 0' ? 96 : 64;
    if (this.classList.contains('virtual-card-row')) return 280;
    if (this.classList.contains('scan-virtual-row'))
      return this.querySelector('.override') ? 280 : 150;
    return 0;
  });
  vi.spyOn(HTMLElement.prototype, 'clientHeight', 'get').mockImplementation(function (
    this: HTMLElement,
  ) {
    return this.classList.contains('library-scroll') || this.classList.contains('scan-scroll')
      ? viewport.height
      : 0;
  });
  vi.spyOn(HTMLElement.prototype, 'scrollHeight', 'get').mockReturnValue(50000);
  return viewport;
}

beforeEach(() => {
  vi.clearAllMocks();
  vi.mocked(api.openFolder).mockResolvedValue();
  vi.mocked(api.appearance).mockResolvedValue({ icon: 'new', illustration: 'new' });
  vi.mocked(api.saveAppearance).mockImplementation(async (value) => value);
  vi.mocked(api.importRecoveryIssues).mockResolvedValue([]);
  vi.mocked(api.startLibraryCheck).mockResolvedValue('');
  vi.mocked(api.syncScanMtool).mockResolvedValue([]);
  vi.mocked(api.launchHistory).mockResolvedValue([]);
  vi.mocked(api.settings).mockResolvedValue(config);
  vi.mocked(api.games).mockResolvedValue([game]);
  vi.mocked(api.startScan).mockResolvedValue('scan');
  vi.mocked(api.job).mockResolvedValue(job);
  vi.mocked(api.suggestVersion).mockResolvedValue(['v2.0', 'file_name']);
  vi.mocked(api.register).mockResolvedValue('registration');
});

async function scan() {
  render(<App />);
  await waitFor(() =>
    expect((screen.getByRole('button', { name: '扫描目录' }) as HTMLButtonElement).disabled).toBe(
      false,
    ),
  );
  fireEvent.click(screen.getByRole('button', { name: '扫描目录' }));
  openScan();
  await screen.findByText('启动文件：main-v1.2.exe');
  await waitFor(() =>
    expect((screen.getByRole('button', { name: '加入游戏库' }) as HTMLButtonElement).disabled).toBe(
      false,
    ),
  );
}

function goLibrary() {
  fireEvent.click(
    within(screen.getByRole('navigation', { name: '主导航' })).getByRole('button', {
      name: /^游戏库/,
    }),
  );
}
function openScan() {
  fireEvent.click(
    within(screen.getByRole('navigation', { name: '主导航' })).getByRole('button', {
      name: '扫描入库',
    }),
  );
}
function openConfig(section: 'MTool' | '设置') {
  fireEvent.click(screen.getByRole('button', { name: section }));
}
describe('iteration interactions', () => {
  it.each([true, false])(
    'allows incomplete scan registration only with an available launch file: %s',
    async (hasFile) => {
      vi.mocked(api.job).mockResolvedValue({
        ...job,
        changes: [
          { ...candidate, status: 'incomplete', executables: hasFile ? candidate.executables : [] },
        ],
      });
      render(<App />);
      await waitFor(() =>
        expect(
          (screen.getByRole('button', { name: '扫描目录' }) as HTMLButtonElement).disabled,
        ).toBe(false),
      );
      fireEvent.click(screen.getByRole('button', { name: '扫描目录' }));
      openScan();
      await screen.findByText('扫描未完整完成');
      const row = screen.getByText(candidate.suggested_title).closest('.scan-item') as HTMLElement;
      const checkbox = within(row).getByRole('checkbox') as HTMLInputElement;
      expect(checkbox.disabled).toBe(!hasFile);
      expect(checkbox.checked).toBe(false);
      if (hasFile) {
        fireEvent.click(checkbox);
        fireEvent.click(screen.getByRole('button', { name: '加入游戏库' }));
        await waitFor(() =>
          expect(api.register).toHaveBeenCalledWith('scan', [
            expect.objectContaining({ executable: 'main-v1.2.exe', exe_override: false }),
          ]),
        );
      } else {
        expect(
          (screen.getByRole('button', { name: '加入游戏库' }) as HTMLButtonElement).disabled,
        ).toBe(true);
        expect(api.register).not.toHaveBeenCalled();
      }
    },
  );
  it('opens the active data directory from About and reports errors without leaving Settings', async () => {
    render(<App />);
    await screen.findByRole('button', { name: '扫描目录' });
    openConfig('设置');
    const about = screen.getByRole('region', { name: '关于应用' });
    const button = within(about).getByRole('button', { name: '打开数据目录' });
    fireEvent.click(button);
    await waitFor(() => expect(api.openDataDirectory).toHaveBeenCalledTimes(1));
    await waitFor(() => expect((button as HTMLButtonElement).disabled).toBe(false));
    vi.mocked(api.openDataDirectory).mockRejectedValueOnce(new Error('无法打开数据目录'));
    fireEvent.click(button);
    await screen.findByText('Error: 无法打开数据目录');
    expect(screen.getByRole('region', { name: '设置页面' })).toBeTruthy();
  });

  it('loads appearance and saves independent icon and illustration choices immediately', async () => {
    vi.mocked(api.appearance).mockResolvedValue({ icon: 'original', illustration: 'new' });
    const { container } = render(<App />);
    await screen.findByRole('button', { name: '扫描目录' });
    openConfig('设置');
    const oldIcon = await screen.findByRole('radio', { name: '原版应用图标' });
    await waitFor(() => expect((oldIcon as HTMLInputElement).checked).toBe(true));
    fireEvent.click(screen.getByRole('radio', { name: '新版应用图标' }));
    await waitFor(() =>
      expect(api.saveAppearance).toHaveBeenLastCalledWith({ icon: 'new', illustration: 'new' }),
    );
    await waitFor(() =>
      expect(
        (screen.getByRole('radio', { name: '新版应用图标' }) as HTMLInputElement).checked,
      ).toBe(true),
    );
    expect(container.querySelector('.brand img')?.getAttribute('src')).toContain('app-icon-new');
    expect(container.querySelector('.settings-about-identity img')?.getAttribute('src')).toContain(
      'app-icon-new',
    );
    fireEvent.click(screen.getByRole('radio', { name: '原版侧栏立绘' }));
    await waitFor(() =>
      expect(api.saveAppearance).toHaveBeenLastCalledWith({
        icon: 'new',
        illustration: 'original',
      }),
    );
    await waitFor(() =>
      expect(container.querySelector('.sidebar-art img')?.getAttribute('src')).not.toContain(
        'character-new',
      ),
    );
  });

  it('keeps the selected appearance when saving fails', async () => {
    vi.mocked(api.saveAppearance).mockRejectedValue(new Error('无法保存外观设置'));
    render(<App />);
    await screen.findByRole('button', { name: '扫描目录' });
    openConfig('设置');
    const oldIcon = await screen.findByRole('radio', { name: '原版应用图标' });
    await waitFor(() => expect((oldIcon as HTMLInputElement).disabled).toBe(false));
    fireEvent.click(oldIcon);
    await screen.findByRole('alert');
    expect(screen.getByRole('alert').textContent).toContain('无法保存外观设置');
    expect((screen.getByRole('radio', { name: '新版应用图标' }) as HTMLInputElement).checked).toBe(
      true,
    );
  });

  it('refreshes automatically detected saves in an untouched open detail without creating unsaved edits', async () => {
    let scanPage: JobPage = {
      ...job,
      status: 'running',
      changes: [],
      next_cursor: 0,
      change_count: 0,
    };
    vi.mocked(api.job).mockImplementation(async () => scanPage);
    vi.mocked(api.syncScanMtool).mockResolvedValue([{ ...game, save_paths: ['<GAME>/save'] }]);
    render(<App />);
    fireEvent.click(await screen.findByRole('button', { name: '扫描目录' }));
    await screen.findByRole('status', { name: '扫描目录进度' });
    fireEvent.click(screen.getByRole('button', { name: game.display_title }));
    const detail = screen.getByRole('dialog');
    scanPage = { ...job, changes: [{ ...candidate, registered_id: game.id }] };
    await waitFor(() =>
      expect(
        (within(detail).getByRole('textbox', { name: /存档位置/ }) as HTMLTextAreaElement).value,
      ).toBe('<GAME>/save'),
    );
    fireEvent.click(within(detail).getByRole('button', { name: '关闭弹窗' }));
    await waitFor(() => expect(screen.queryByRole('dialog')).toBeNull());
    expect(api.saveGame).not.toHaveBeenCalled();
  });
  it('scans in place and exposes results through the bottom banner without pushing the page header', async () => {
    render(<App />);
    fireEvent.click(await screen.findByRole('button', { name: '扫描目录' }));
    const banner = await screen.findByRole('status', { name: '扫描目录进度' });
    expect(banner.closest('.task-banner-rail')).toBeTruthy();
    expect(screen.getByRole('table')).toBeTruthy();
    expect(screen.queryByRole('region', { name: '扫描游戏列表' })).toBeNull();
    fireEvent.click(within(banner).getByRole('button', { name: '查看扫描结果' }));
    await screen.findByRole('region', { name: '扫描游戏列表' });
    expect(screen.getByText('启动文件：main-v1.2.exe')).toBeTruthy();
  });
  it('keeps the library usable and marks the import recovery entry when a journal is damaged', async () => {
    vi.mocked(api.importRecoveryIssues).mockResolvedValue([
      { record: 'bad.json', message: '记录损坏' },
    ]);
    render(<App />);
    await screen.findByRole('button', { name: game.display_title });
    const entry = await screen.findByRole('button', { name: /^导入\s*需处理$/ });
    fireEvent.click(entry);
    expect(await screen.findByRole('alert', { name: '导入记录恢复提示' })).toBeTruthy();
    expect(api.games).toHaveBeenCalled();
    expect(api.clearLibrary).not.toHaveBeenCalled();
  });
  it('keeps the hovered game highlighted after closing detail until another row is hovered', async () => {
    const otherGame = { ...game, id: 'other', display_title: '另一款游戏' };
    vi.mocked(api.games).mockResolvedValue([game, otherGame]);
    render(<App />);
    const first = await screen.findByRole('button', { name: game.display_title });
    const second = screen.getByRole('button', { name: otherGame.display_title });
    const firstRow = first.closest('tr')!;
    const secondRow = second.closest('tr')!;

    fireEvent.mouseEnter(firstRow);
    expect(firstRow.classList.contains('selected')).toBe(true);
    expect(screen.queryByRole('dialog')).toBeNull();
    fireEvent.click(first);
    fireEvent.click(screen.getByRole('button', { name: '关闭弹窗' }));
    expect(screen.queryByRole('dialog')).toBeNull();
    expect(firstRow.classList.contains('selected')).toBe(true);
    fireEvent.mouseLeave(firstRow);
    expect(firstRow.classList.contains('selected')).toBe(true);

    fireEvent.mouseEnter(secondRow);
    expect(firstRow.classList.contains('selected')).toBe(false);
    expect(secondRow.classList.contains('selected')).toBe(true);
    expect(screen.queryByRole('dialog')).toBeNull();
    fireEvent.focus(first);
    expect(firstRow.classList.contains('selected')).toBe(true);
    expect(secondRow.classList.contains('selected')).toBe(false);
  });

  it('edits local QSP configuration in the existing detail and guards unsaved changes', async () => {
    const qsp: Game = {
      ...game,
      engine: 'QSP',
      launch_type: 'EXTERNAL_PLAYER',
      main_executable: 'qspgui.exe',
      external_player: { player_type: 'QSP', scope: 'GAME_LOCAL', game_file: 'game.qsp' },
    };
    vi.mocked(api.games).mockResolvedValue([qsp]);
    vi.mocked(api.choosePlayerFile).mockResolvedValue('彼女の冒険.qsp');
    vi.mocked(api.saveGame).mockImplementation(async (edit) => ({ ...qsp, ...edit }));
    render(<App />);
    fireEvent.click(await screen.findByRole('button', { name: game.display_title }));
    expect((screen.getByRole('combobox', { name: 'QSP 播放器' }) as HTMLInputElement).value).toBe(
      'qspgui.exe',
    );
    fireEvent.click(screen.getByRole('button', { name: '更改 QSP 文件…' }));
    await waitFor(() =>
      expect(
        (screen.getByRole('combobox', { name: 'QSP 主游戏文件' }) as HTMLInputElement).value,
      ).toBe('彼女の冒険.qsp'),
    );
    expect(api.choosePlayerFile).toHaveBeenCalledWith(game.install_path, 'qsp');
    fireEvent.click(screen.getByRole('button', { name: '关闭弹窗' }));
    expect(screen.getByRole('dialog', { name: '放弃未保存修改？' })).toBeTruthy();
    fireEvent.click(screen.getByRole('button', { name: '继续编辑' }));
    fireEvent.click(screen.getByRole('button', { name: '保存资料' }));
    await waitFor(() =>
      expect(api.saveGame).toHaveBeenCalledWith(
        expect.objectContaining({
          launch_type: 'EXTERNAL_PLAYER',
          main_executable: 'qspgui.exe',
          working_directory: '.',
          external_player: { player_type: 'QSP', scope: 'GAME_LOCAL', game_file: '彼女の冒険.qsp' },
        }),
      ),
    );
  });
  it('offers an explicit QSP setup after analyzing an existing direct-launch game', async () => {
    vi.mocked(api.startGameAnalysis).mockResolvedValue('qsp-analysis');
    vi.mocked(api.job).mockResolvedValue({
      ...job,
      id: 'qsp-analysis',
      kind: 'analysis',
      status: 'completed',
      changes: [
        {
          ...candidate,
          registered_id: game.id,
          engine: 'QSP',
          qsp: {
            game_files: ['Girls Life 0.9.5.qsp', 'mod/addedflavour.qsp'],
            players: ['Qqsp-1.9.0-win64/Qqsp.exe'],
            recommended_player: 'Qqsp-1.9.0-win64/Qqsp.exe',
          },
        },
      ],
      next_cursor: 1,
      change_count: 1,
    });
    render(<App />);
    fireEvent.click(await screen.findByRole('button', { name: game.display_title }));
    expect(screen.queryByRole('combobox', { name: 'QSP 播放器' })).toBeNull();
    expect(screen.queryByRole('combobox', { name: 'QSP 主游戏文件' })).toBeNull();
    fireEvent.click(screen.getByRole('button', { name: '分析启动配置' }));
    fireEvent.click(await screen.findByRole('button', { name: '使用 QSP 启动' }));
    expect((screen.getByRole('combobox', { name: 'QSP 播放器' }) as HTMLInputElement).value).toBe(
      'Qqsp-1.9.0-win64/Qqsp.exe',
    );
    expect(
      (screen.getByRole('combobox', { name: 'QSP 主游戏文件' }) as HTMLInputElement).value,
    ).toBe('');
    expect((screen.getByRole('combobox', { name: /游戏引擎/ }) as HTMLSelectElement).value).toBe(
      'QSP',
    );
    expect(api.saveGame).not.toHaveBeenCalled();
    expect(api.play).not.toHaveBeenCalled();
  });
  it('scan registration passes the local QSP config and does not suggest association launch', async () => {
    const qsp = {
      ...candidate,
      engine: 'QSP',
      executables: [],
      qsp: { game_files: ['游戏.qsp'], players: [], recommended_player: null },
    };
    vi.mocked(api.job).mockResolvedValue({ ...job, changes: [qsp] });
    render(<App />);
    await waitFor(() =>
      expect((screen.getByRole('button', { name: '扫描目录' }) as HTMLButtonElement).disabled).toBe(
        false,
      ),
    );
    fireEvent.click(screen.getByRole('button', { name: '扫描目录' }));
    openScan();
    await screen.findByRole('combobox', { name: 'QSP 主游戏文件' });
    expect(
      (screen.getByRole('combobox', { name: 'QSP 主游戏文件' }) as HTMLInputElement).value,
    ).toBe('游戏.qsp');
    fireEvent.click(screen.getByRole('button', { name: '加入游戏库' }));
    await waitFor(() =>
      expect(api.register).toHaveBeenCalledWith('scan', [
        expect.objectContaining({
          executable: null,
          external_player: { player_type: 'QSP', scope: 'GAME_LOCAL', game_file: '游戏.qsp' },
        }),
      ]),
    );
  });
  it('uses only sidebar page navigation and opens scan results without starting a scan', async () => {
    render(<App />);
    await screen.findByRole('button', { name: game.display_title });
    openScan();
    expect(screen.getByRole('region', { name: '扫描结果页面' })).toBeTruthy();
    expect(screen.getByText('扫描本地游戏目录')).toBeTruthy();
    expect(api.startScan).not.toHaveBeenCalled();
    expect((screen.getByRole('button', { name: '加入游戏库' }) as HTMLButtonElement).disabled).toBe(
      true,
    );
    expect(screen.queryByRole('button', { name: '返回游戏库' })).toBeNull();
    openConfig('MTool');
    expect(screen.getByRole('region', { name: 'MTool页面' })).toBeTruthy();
    expect(screen.queryByRole('group', { name: '设置分类' })).toBeNull();
    openConfig('设置');
    const settingsPage = screen.getByRole('region', { name: '设置页面' });
    expect(within(settingsPage).getByRole('region', { name: '关于应用' })).toBeTruthy();
    expect(within(settingsPage).getByText('开源图标：Lucide · 许可说明')).toBeTruthy();
    expect(within(settingsPage).getByText('虚拟列表：TanStack Virtual · 许可说明')).toBeTruthy();
    expect(screen.getByRole('button', { name: '设置' }).closest('.sidebar-bottom')).toBeTruthy();
    expect(
      within(screen.getByRole('navigation', { name: '主导航' })).queryByRole('button', {
        name: '设置',
      }),
    ).toBeNull();
    const sidebar = screen.getByRole('complementary');
    expect(within(sidebar).getByText('by Zhehao-w')).toBeTruthy();
    expect(within(sidebar).getByText('v0.3.1').className).toBe('sidebar-version');
    expect(sidebar.querySelectorAll('img')).toHaveLength(2);
    expect(screen.queryByRole('button', { name: '关于' })).toBeNull();
    expect(screen.queryByRole('region', { name: '关于页面' })).toBeNull();
    expect(screen.getByRole('button', { name: '保存设置' })).toBeTruthy();
    expect(screen.queryByRole('button', { name: '返回游戏库' })).toBeNull();
    goLibrary();
    expect(screen.getByRole('button', { name: game.display_title })).toBeTruthy();
    expect(screen.queryByRole('button', { name: '补充引擎/版本' })).toBeNull();
  });

  it('searches and sorts scans while retaining hidden selections and manual edits', async () => {
    const newGame = {
      ...candidate,
      install_path: 'E:/Butter/new',
      suggested_title: '新游戏',
      directory_modified_ms: 100,
    };
    const oldGame = {
      ...candidate,
      install_path: 'E:/Butter/old',
      suggested_title: '已入库游戏',
      directory_modified_ms: 200,
      registered_id: game.id,
      suggested_version: 'v9.9',
    };
    vi.mocked(api.job).mockResolvedValue({
      ...job,
      changes: [oldGame, newGame],
      next_cursor: 2,
      change_count: 2,
      total: 2,
      processed: 2,
    });
    render(<App />);
    fireEvent.click(await screen.findByRole('button', { name: '扫描目录' }));
    openScan();
    await screen.findByText('新游戏');
    const rows = () =>
      Array.from(
        screen.getByRole('region', { name: '扫描游戏列表' }).querySelectorAll('.scan-item'),
      );
    expect(rows()[0].textContent).toContain('新游戏');
    expect(rows()[1].textContent).toContain('入库版本：Final');
    expect(
      within(rows()[1] as HTMLElement).queryByRole('button', { name: '更改启动文件' }),
    ).toBeNull();
    fireEvent.change(screen.getByRole('textbox', { name: '新游戏版本' }), {
      target: { value: 'Manual' },
    });
    fireEvent.click(screen.getByRole('button', { name: '未入库' }));
    expect(screen.getByRole('button', { name: '未入库' }).getAttribute('aria-pressed')).toBe(
      'true',
    );
    expect(rows().length).toBe(1);
    expect(rows()[0].textContent).toContain('新游戏');
    fireEvent.click(screen.getByRole('button', { name: '全不选' }));
    expect(screen.getByText('已选 0 项')).toBeTruthy();
    fireEvent.click(screen.getByRole('button', { name: '全选' }));
    expect(screen.getByText('已选 1 项')).toBeTruthy();
    goLibrary();
    openScan();
    expect(screen.getByRole('button', { name: '未入库' }).getAttribute('aria-pressed')).toBe(
      'true',
    );
    fireEvent.change(screen.getByRole('textbox', { name: '搜索扫描结果' }), {
      target: { value: 'old' },
    });
    expect(screen.getByText('没有匹配的扫描结果')).toBeTruthy();
    fireEvent.click(screen.getByRole('button', { name: '未入库' }));
    fireEvent.change(screen.getByRole('textbox', { name: '搜索扫描结果' }), {
      target: { value: '' },
    });
    expect(rows().length).toBe(2);
    fireEvent.change(screen.getByRole('textbox', { name: '搜索扫描结果' }), {
      target: { value: 'old' },
    });
    expect(rows().length).toBe(1);
    expect(screen.getByText('已选 1 项（当前显示 0 项）')).toBeTruthy();
    fireEvent.change(screen.getByRole('textbox', { name: '搜索扫描结果' }), {
      target: { value: '' },
    });
    expect((screen.getByRole('textbox', { name: '新游戏版本' }) as HTMLInputElement).value).toBe(
      'Manual',
    );
    fireEvent.change(screen.getByRole('combobox', { name: '扫描结果排序' }), {
      target: { value: 'modified-desc' },
    });
    expect(rows()[0].textContent).toContain('已入库游戏');
    fireEvent.change(screen.getByRole('combobox', { name: '扫描结果排序' }), {
      target: { value: 'unregistered-first' },
    });
    expect(rows()[0].textContent).toContain('新游戏');
    expect(
      (within(rows()[0] as HTMLElement).getByRole('checkbox') as HTMLInputElement).checked,
    ).toBe(true);
    fireEvent.change(screen.getByRole('textbox', { name: '搜索扫描结果' }), {
      target: { value: '无匹配' },
    });
    expect(screen.getByText('没有匹配的扫描结果')).toBeTruthy();
    goLibrary();
    openScan();
    expect((screen.getByRole('textbox', { name: '搜索扫描结果' }) as HTMLInputElement).value).toBe(
      '无匹配',
    );
  });
  it('colors games by engine and saves a manually chosen engine with a picked save folder', async () => {
    vi.mocked(api.games).mockResolvedValue([
      game,
      { ...game, id: 'second', display_title: '另一个游戏', engine: 'Unknown' },
      { ...game, id: 'unity', display_title: 'Unity游戏', engine: 'Unity' },
    ]);
    vi.mocked(api.saveGame).mockImplementation(async (edit) => ({ ...game, ...edit }));
    vi.mocked(api.chooseSaveDirectory).mockResolvedValue('D:/Saves/游戏');
    render(<App />);
    const first = await screen.findByRole('button', { name: game.display_title });
    const firstIcon = first.closest('tr')!.querySelector('.game-mark')!;
    const secondIcon = screen
      .getByRole('button', { name: '另一个游戏' })
      .closest('tr')!
      .querySelector('.game-mark')!;
    expect(firstIcon.className).toBe(secondIcon.className);
    expect(firstIcon.classList.contains('tone-unknown')).toBe(true);
    expect(
      screen
        .getByRole('button', { name: 'Unity游戏' })
        .closest('tr')!
        .querySelector('.game-mark')!
        .classList.contains('tone-unity'),
    ).toBe(true);
    fireEvent.click(first);
    const detail = screen.getByRole('dialog');
    const body = within(detail).getByRole('region', { name: '游戏详情内容' });
    expect(body.contains(within(detail).getByRole('button', { name: '关闭弹窗' }))).toBe(false);
    fireEvent.change(within(detail).getByRole('combobox', { name: /游戏引擎/ }), {
      target: { value: 'QSP' },
    });
    expect(detail.querySelector('.game-mark')!.classList.contains('tone-qsp')).toBe(true);
    fireEvent.click(within(detail).getByRole('button', { name: '添加存档目录…' }));
    await waitFor(() =>
      expect(
        (within(detail).getByRole('textbox', { name: /存档位置/ }) as HTMLTextAreaElement).value,
      ).toBe('D:/Saves/游戏'),
    );
    fireEvent.click(within(detail).getByRole('button', { name: '保存资料' }));
    await waitFor(() =>
      expect(api.saveGame).toHaveBeenCalledWith(
        expect.objectContaining({ engine: 'QSP', save_paths: ['D:/Saves/游戏'] }),
      ),
    );
    await screen.findByText('游戏资料已保存。');
    fireEvent.click(within(detail).getByRole('button', { name: '关闭弹窗' }));
    expect(
      screen
        .getByRole('button', { name: game.display_title })
        .closest('tr')!
        .querySelector('.game-mark')!
        .classList.contains('tone-qsp'),
    ).toBe(true);
  });

  it('retains typed save locations when folder browsing is cancelled, repeated or fails', async () => {
    render(<App />);
    fireEvent.click(await screen.findByRole('button', { name: game.display_title }));
    const detail = screen.getByRole('dialog');
    const paths = within(detail).getByRole('textbox', { name: /存档位置/ }) as HTMLTextAreaElement;
    const browse = within(detail).getByRole('button', { name: '添加存档目录…' });
    fireEvent.change(paths, { target: { value: '<GAME>/save\nD:/Save' } });
    vi.mocked(api.chooseSaveDirectory).mockResolvedValueOnce(null);
    fireEvent.click(browse);
    await waitFor(() => expect((browse as HTMLButtonElement).disabled).toBe(false));
    expect(paths.value).toBe('<GAME>/save\nD:/Save');
    vi.mocked(api.chooseSaveDirectory).mockResolvedValueOnce('D:/Save');
    fireEvent.click(browse);
    await waitFor(() => expect((browse as HTMLButtonElement).disabled).toBe(false));
    expect(paths.value).toBe('<GAME>/save\nD:/Save');
    vi.mocked(api.chooseSaveDirectory).mockRejectedValueOnce('无法选择目录');
    fireEvent.click(browse);
    await within(detail).findByText('无法选择目录');
    expect(paths.value).toBe('<GAME>/save\nD:/Save');
    expect(api.saveGame).not.toHaveBeenCalled();
  });
  it('opens the selected configured save without changing or saving the configuration', async () => {
    render(<App />);
    fireEvent.click(await screen.findByRole('button', { name: game.display_title }));
    const detail = screen.getByRole('dialog');
    const paths = within(detail).getByRole('textbox', { name: /存档位置/ });
    const open = within(detail).getByRole('button', { name: '打开存档位置' });
    expect((open as HTMLButtonElement).disabled).toBe(true);
    fireEvent.change(paths, { target: { value: '<GAME>/www/save\n%APPDATA%/RenPy/彼女' } });
    fireEvent.click(open);
    await waitFor(() =>
      expect(api.openSaveFolder).toHaveBeenCalledWith(game.id, '<GAME>/www/save'),
    );
    fireEvent.change(within(detail).getByRole('combobox', { name: '打开的存档位置' }), {
      target: { value: '%APPDATA%/RenPy/彼女' },
    });
    fireEvent.click(open);
    await waitFor(() =>
      expect(api.openSaveFolder).toHaveBeenCalledWith(game.id, '%APPDATA%/RenPy/彼女'),
    );
    expect(api.chooseSaveDirectory).not.toHaveBeenCalled();
    expect(api.saveGame).not.toHaveBeenCalled();
    expect((paths as HTMLTextAreaElement).value).toBe('<GAME>/www/save\n%APPDATA%/RenPy/彼女');
  });
  it('switches library layouts without losing a path search and opens the selected card', async () => {
    render(<App />);
    await screen.findByRole('button', { name: game.display_title });
    const search = screen.getByRole('textbox', { name: '搜索游戏或别名' });
    fireEvent.change(search, { target: { value: game.install_path } });
    fireEvent.scroll(screen.getByLabelText('游戏列表'), { target: { scrollTop: 420 } });
    fireEvent.click(screen.getByRole('button', { name: '卡片视图' }));
    expect(screen.getByLabelText('游戏列表').scrollTop).toBe(420);
    expect(screen.queryByRole('table')).toBeNull();
    expect((search as HTMLInputElement).value).toBe(game.install_path);
    fireEvent.click(screen.getByRole('button', { name: game.display_title }));
    expect(screen.getByRole('dialog')).toBeTruthy();
    fireEvent.click(screen.getByRole('button', { name: '关闭弹窗' }));
    fireEvent.click(screen.getByRole('button', { name: '列表视图' }));
    expect(screen.getByRole('table')).toBeTruthy();
    expect(screen.getByRole('button', { name: game.display_title })).toBeTruthy();
    fireEvent.click(
      within(screen.getByRole('navigation', { name: '主导航' })).getByRole('button', {
        name: /导入/,
      }),
    );
    goLibrary();
    expect(screen.getByLabelText('游戏列表').scrollTop).toBe(420);
    fireEvent.change(screen.getByRole('textbox', { name: '搜索游戏或别名' }), {
      target: { value: '查询' },
    });
    expect(screen.getByLabelText('游戏列表').scrollTop).toBe(0);
    expect(api.games).toHaveBeenCalledTimes(1);
  });
  it('opens batch import without starting a scan or moving files and preserves library filters', async () => {
    render(<App />);
    await screen.findByRole('button', { name: game.display_title });
    fireEvent.change(screen.getByRole('textbox', { name: '搜索游戏或别名' }), {
      target: { value: game.display_title },
    });
    fireEvent.click(
      within(screen.getByRole('navigation', { name: '主导航' })).getByRole('button', {
        name: /导入/,
      }),
    );
    expect(screen.getByRole('region', { name: '批量导入游戏' })).toBeTruthy();
    expect(screen.getByRole('button', { name: '添加文件夹' })).toBeTruthy();
    expect(api.startScan).not.toHaveBeenCalled();
    expect(api.register).not.toHaveBeenCalled();
    goLibrary();
    expect(
      (screen.getByRole('textbox', { name: '搜索游戏或别名' }) as HTMLInputElement).value,
    ).toBe(game.display_title);
    expect(screen.getByRole('button', { name: game.display_title })).toBeTruthy();
  });
  it('opens MTool settings directly and preserves edits across setting categories before saving', async () => {
    vi.mocked(api.saveSettings).mockImplementation(async (settings) => settings);
    render(<App />);
    await screen.findByRole('button', { name: game.display_title });
    fireEvent.click(
      within(screen.getByRole('navigation', { name: '主导航' })).getByRole('button', {
        name: 'MTool',
      }),
    );
    const modal = screen.getByRole('region', { name: 'MTool页面' });
    fireEvent.change(within(modal).getByRole('textbox', { name: /Shared MTool Root/ }), {
      target: { value: 'E:/Butter/Tool' },
    });
    openConfig('设置');
    fireEvent.change(within(modal).getByRole('textbox', { name: /Game Root/ }), {
      target: { value: 'D:/Games' },
    });
    openConfig('MTool');
    expect(
      (within(modal).getByRole('textbox', { name: /Shared MTool Root/ }) as HTMLInputElement).value,
    ).toBe('E:/Butter/Tool');
    fireEvent.click(within(modal).getByRole('button', { name: '保存设置' }));
    await waitFor(() =>
      expect(api.saveSettings).toHaveBeenCalledWith({
        ...config,
        game_root: 'D:/Games',
        mtool_root: 'E:/Butter/Tool',
      }),
    );
    await waitFor(() =>
      expect(
        (within(modal).getByRole('button', { name: '保存设置' }) as HTMLButtonElement).disabled,
      ).toBe(true),
    );
    expect(screen.queryByRole('dialog')).toBeNull();
    expect(
      within(screen.getByRole('navigation', { name: '主导航' }))
        .getByRole('button', { name: 'MTool' })
        .getAttribute('aria-current'),
    ).toBe('page');
  });
  it('remembers independent settings and MTool scroll offsets across sidebar navigation', async () => {
    render(<App />);
    await screen.findByRole('button', { name: game.display_title });
    openConfig('设置');
    const settingsScroll = () =>
      screen.getByRole('region', { name: /设置页面|MTool页面/ }).querySelector('.settings-scroll')!;
    fireEvent.scroll(settingsScroll(), { target: { scrollTop: 180 } });
    openConfig('MTool');
    expect(settingsScroll().scrollTop).toBe(0);
    fireEvent.scroll(settingsScroll(), { target: { scrollTop: 90 } });
    goLibrary();
    openConfig('设置');
    expect(settingsScroll().scrollTop).toBe(180);
    openConfig('MTool');
    expect(settingsScroll().scrollTop).toBe(90);
    goLibrary();
    openConfig('设置');
    expect(settingsScroll().scrollTop).toBe(180);
  });
  it('moves scan threads into settings, saves the choice and starts scans without a per-scan override', async () => {
    vi.mocked(api.saveSettings).mockImplementation(async (settings) => settings);
    render(<App />);
    await screen.findByRole('button', { name: game.display_title });
    expect(screen.queryByRole('combobox', { name: /扫描/ })).toBeNull();
    fireEvent.click(screen.getByRole('button', { name: '设置' }));
    const select = within(screen.getByRole('region', { name: '设置页面' })).getByRole('combobox', {
      name: /扫描线程/,
    });
    expect((select as HTMLSelectElement).value).toBe('2');
    fireEvent.change(select, { target: { value: '4' } });
    fireEvent.click(screen.getByRole('button', { name: '保存设置' }));
    await waitFor(() =>
      expect((screen.getByRole('button', { name: '保存设置' }) as HTMLButtonElement).disabled).toBe(
        true,
      ),
    );
    expect(api.saveSettings).toHaveBeenCalledWith({ ...config, scan_workers: 4 });
    fireEvent.click(screen.getByRole('button', { name: '设置' }));
    expect((screen.getByRole('combobox', { name: /扫描线程/ }) as HTMLSelectElement).value).toBe(
      '4',
    );
    goLibrary();
    fireEvent.click(screen.getByRole('button', { name: '扫描目录' }));
    openScan();
    await screen.findByText('启动文件：main-v1.2.exe');
    expect(api.startScan).toHaveBeenCalledWith();
  });
  it('accepts a document launch file in details and saves blank version as Unknown', async () => {
    const original = {
      ...game,
      current_version: 'Unknown',
      launch_type: 'MTOOL' as const,
      mtool_target_exe: 'Game.exe',
      mtool_loader: 'loader.dll',
    };
    vi.mocked(api.games).mockResolvedValue([original]);
    vi.mocked(api.chooseLaunchFile).mockResolvedValue('包装/游戏 & test.html');
    vi.mocked(api.saveGame).mockImplementation(async (edit) => ({ ...original, ...edit }));
    render(<App />);
    const title = await screen.findByRole('button', { name: game.display_title });
    const versionCell = within(title.closest('tr')!).getAllByRole('cell')[1];
    expect(versionCell.textContent).toBe('-');
    fireEvent.click(versionCell);
    const modal = screen.getByRole('dialog');
    const version = within(modal).getByRole('textbox', { name: '版本' }) as HTMLInputElement;
    expect(version.value).toBe('');
    expect(version.placeholder).toBe('-');
    fireEvent.click(within(modal).getByRole('button', { name: '浏览启动文件…' }));
    await waitFor(() =>
      expect(
        (within(modal).getByRole('combobox', { name: /启动文件（/ }) as HTMLInputElement).value,
      ).toBe('包装/游戏 & test.html'),
    );
    fireEvent.change(within(modal).getByRole('textbox', { name: '版本' }), {
      target: { value: '' },
    });
    fireEvent.click(within(modal).getByRole('button', { name: '保存资料' }));
    await waitFor(() =>
      expect(api.saveGame).toHaveBeenCalledWith(
        expect.objectContaining({
          main_executable: '包装/游戏 & test.html',
          working_directory: '包装',
          launch_type: 'DIRECT',
          current_version: 'Unknown',
        }),
      ),
    );
  });
  it('continuously scrolls past 100 games with bounded rendered rows and retains the position on returning', async () => {
    mockLibraryViewport();
    vi.mocked(api.games).mockResolvedValue(
      Array.from({ length: 570 }, (_, i) => ({
        ...game,
        id: String(i),
        display_title: `Game ${i}`,
      })),
    );
    render(<App />);
    await screen.findByRole('button', { name: 'Game 0' });
    await waitFor(() =>
      expect(screen.getByRole('button', { name: 'Game 1' }).closest('tr')?.style.transform).toBe(
        'translateY(94px)',
      ),
    );
    expect(screen.queryByRole('navigation', { name: /游戏库.*分页/ })).toBeNull();
    expect(screen.queryByRole('button', { name: 'Game 120' })).toBeNull();
    const rows = () => within(screen.getByRole('table')).getAllByRole('row');
    expect(rows().length).toBeLessThan(30);
    fireEvent.scroll(screen.getByLabelText('游戏列表'), { target: { scrollTop: 10923 } });
    fireEvent.click(await screen.findByRole('button', { name: 'Game 120' }));
    expect(screen.getByRole('dialog', { name: 'Game 120' })).toBeTruthy();
    fireEvent.click(within(screen.getByRole('dialog')).getByRole('button', { name: '关闭弹窗' }));
    expect(rows().length).toBeLessThan(30);
    fireEvent.click(screen.getByRole('button', { name: '扫描目录' }));
    openScan();
    await screen.findByText('启动文件：main-v1.2.exe');
    goLibrary();
    await screen.findByRole('button', { name: 'Game 120' });
    expect(screen.getByLabelText('游戏列表').scrollTop).toBe(10923);
    fireEvent.scroll(screen.getByLabelText('游戏列表'), { target: { scrollTop: 51693 } });
    await screen.findByRole('button', { name: 'Game 569' });
    expect(rows().length).toBeLessThan(30);
    fireEvent.change(screen.getByRole('textbox', { name: '搜索游戏或别名' }), {
      target: { value: 'Game 569' },
    });
    await screen.findByRole('button', { name: 'Game 569' });
    expect(screen.getByLabelText('游戏列表').scrollTop).toBe(0);
    expect(screen.getByText('找到 1 / 570 个游戏')).toBeTruthy();
  });
  it('virtualizes continuous card rows and adapts column count on resize without pagination', async () => {
    const viewport = mockLibraryViewport();
    vi.mocked(api.games).mockResolvedValue(
      Array.from({ length: 570 }, (_, i) => ({
        ...game,
        id: String(i),
        display_title: `Game ${i}`,
      })),
    );
    render(<App />);
    await screen.findByRole('button', { name: 'Game 0' });
    fireEvent.click(screen.getByRole('button', { name: '卡片视图' }));
    const cards = () => within(screen.getByLabelText('游戏卡片')).getAllByRole('article');
    await screen.findByRole('button', { name: 'Game 0' });
    expect(cards().length).toBeLessThan(100);
    fireEvent.scroll(screen.getByLabelText('游戏列表'), { target: { scrollTop: 35 * (288 + 16) } });
    await screen.findByRole('button', { name: 'Game 140' });
    expect(cards().length).toBeLessThan(100);
    viewport.width = 450;
    fireEvent(window, new Event('resize'));
    await waitFor(() =>
      expect(cards()[0].parentElement?.style.gridTemplateColumns).toBe('repeat(2, minmax(0, 1fr))'),
    );
    fireEvent.change(screen.getByRole('textbox', { name: '搜索游戏或别名' }), {
      target: { value: 'Game 140' },
    });
    await screen.findByRole('button', { name: 'Game 140' });
    expect(cards().length).toBe(1);
    expect(screen.getByLabelText('游戏列表').scrollTop).toBe(0);
  });
  it('registers a manually chosen QSP file on the secondary page and stays on the scan page with updated registration status', async () => {
    const documentCandidate = {
      ...candidate,
      executables: [],
      suggested_version: 'Unknown',
      version_source: 'unknown',
    };
    let scanPage = { ...job, changes: [documentCandidate] };
    vi.mocked(api.job).mockImplementation(async (id) =>
      id === 'scan'
        ? scanPage
        : {
            ...job,
            id: 'registration',
            kind: 'register',
            changes: [],
            next_cursor: 0,
            change_count: 0,
            registered_ids: ['qsp'],
          },
    );
    vi.mocked(api.suggestVersion).mockResolvedValue(['Unknown', 'unknown']);
    vi.mocked(api.chooseLaunchFile).mockResolvedValue('游戏.qsp');
    vi.mocked(api.gamesByIds).mockResolvedValue([
      {
        ...game,
        id: 'qsp',
        display_title: 'QSP game',
        engine: 'QSP',
        launch_type: 'EXTERNAL_PLAYER',
        external_player: { player_type: 'QSP', scope: 'GAME_LOCAL', game_file: '游戏.qsp' },
        main_executable: null,
        current_version: 'Unknown',
      },
    ]);
    vi.mocked(api.register).mockImplementation(async () => {
      scanPage = { ...scanPage, changes: [{ ...documentCandidate, registered_id: 'qsp' }] };
      return 'registration';
    });
    render(<App />);
    fireEvent.click(await screen.findByRole('button', { name: '扫描目录' }));
    openScan();
    const results = await screen.findByRole('region', { name: '扫描结果页面' });
    expect(screen.queryByRole('table')).toBeNull();
    fireEvent.click(await within(results).findByRole('button', { name: '更改启动文件' }));
    fireEvent.click(within(results).getByRole('button', { name: '浏览启动文件…' }));
    await screen.findByText('QSP 主游戏文件：游戏.qsp');
    fireEvent.click(within(results).getByRole('button', { name: '加入游戏库' }));
    await screen.findByText('已入库');
    expect(screen.getByRole('region', { name: '扫描结果页面' })).toBeTruthy();
    goLibrary();
    await screen.findByRole('button', { name: 'QSP game' });
    expect(api.register).toHaveBeenCalledWith('scan', [
      expect.objectContaining({
        executable: null,
        external_player: { player_type: 'QSP', scope: 'GAME_LOCAL', game_file: '游戏.qsp' },
        exe_override: true,
        version: 'Unknown',
        version_override: false,
      }),
    ]);
    openScan();
    expect(await screen.findByText('已入库')).toBeTruthy();
    expect(api.startScan).toHaveBeenCalledTimes(1);
  });
  it('bulk-selects all filtered scan results beyond the virtual viewport and preserves overrides and navigation', async () => {
    mockLibraryViewport();
    const all = Array.from({ length: 101 }, (_, i) => ({
      ...candidate,
      install_path: `E:/Butter/game${i}`,
      suggested_title: `Game ${String(i).padStart(3, '0')}`,
    }));
    vi.mocked(api.job).mockImplementation(async (_id, cursor) => ({
      ...job,
      changes: cursor === 0 ? all.slice(0, 100) : cursor === 100 ? all.slice(100) : [],
      next_cursor: cursor === 0 ? 100 : 101,
      change_count: 101,
      total: 101,
      processed: 101,
    }));
    render(<App />);
    fireEvent.click(await screen.findByRole('button', { name: '扫描目录' }));
    openScan();
    const scroll = await screen.findByRole('region', { name: '扫描游戏列表' });
    await screen.findByText('显示 101 / 101 项 · 未入库 101 项');
    await screen.findByText('Game 000');
    expect(screen.queryByRole('navigation', { name: /扫描结果.*分页/ })).toBeNull();
    expect(scroll.querySelectorAll('.scan-item').length).toBeLessThan(25);
    fireEvent.click(screen.getByRole('button', { name: '全不选' }));
    expect(screen.getByText('已选 0 项')).toBeTruthy();
    fireEvent.click(screen.getByRole('button', { name: '全选' }));
    expect(screen.getByText('已选 101 项')).toBeTruthy();
    fireEvent.change(screen.getByRole('textbox', { name: '搜索扫描结果' }), {
      target: { value: 'Game 00' },
    });
    expect(screen.queryByText('仅当前结果')).toBeNull();
    fireEvent.click(screen.getByRole('button', { name: '全不选' }));
    expect(screen.getByText('已选 91 项（当前显示 0 项）')).toBeTruthy();
    fireEvent.click(screen.getByRole('button', { name: '全选' }));
    expect(screen.getByText('已选 101 项（当前显示 10 项）')).toBeTruthy();
    fireEvent.change(screen.getByRole('textbox', { name: '搜索扫描结果' }), {
      target: { value: '' },
    });
    fireEvent.click(
      within(scroll.querySelector('.scan-item') as HTMLElement).getByRole('button', {
        name: '更改启动文件',
      }),
    );
    fireEvent.change(screen.getByRole('textbox', { name: 'Game 000版本' }), {
      target: { value: 'Custom' },
    });
    fireEvent.scroll(scroll, { target: { scrollTop: 16000 } });
    await screen.findByText('Game 100');
    expect(scroll.querySelectorAll('.scan-item').length).toBeLessThan(25);
    goLibrary();
    openScan();
    await screen.findByText('Game 100');
    expect(screen.getByRole('region', { name: '扫描游戏列表' }).scrollTop).toBe(16000);
    fireEvent.scroll(screen.getByRole('region', { name: '扫描游戏列表' }), {
      target: { scrollTop: 0 },
    });
    await screen.findByRole('combobox', { name: '选择候选启动文件' });
    expect((screen.getByRole('textbox', { name: 'Game 000版本' }) as HTMLInputElement).value).toBe(
      'Custom',
    );
    expect(
      (
        within(
          screen
            .getByRole('region', { name: '扫描游戏列表' })
            .querySelector('.scan-item') as HTMLElement,
        ).getByRole('checkbox') as HTMLInputElement
      ).checked,
    ).toBe(true);
    expect(api.startScan).toHaveBeenCalledTimes(1);
  });

  it('bulk selection skips registered games and unresolved launch configurations when registering', async () => {
    const items = [
      candidate,
      {
        ...candidate,
        install_path: 'E:/Butter/registered',
        suggested_title: '已入库',
        registered_id: game.id,
      },
      {
        ...candidate,
        install_path: 'E:/Butter/incomplete',
        suggested_title: '可入库',
        status: 'incomplete',
      },
      {
        ...candidate,
        install_path: 'E:/Butter/unresolved',
        suggested_title: '未找到启动文件',
        status: 'incomplete',
        executables: [],
      },
      {
        ...candidate,
        install_path: 'E:/Butter/qsp',
        suggested_title: '多个QSP文件',
        qsp: { game_files: ['a.qsp', 'b.qsp'], players: [], recommended_player: null },
        executables: [],
      },
    ];
    vi.mocked(api.job).mockResolvedValue({
      ...job,
      changes: items,
      next_cursor: 5,
      change_count: 5,
      total: 5,
      processed: 5,
    });
    render(<App />);
    await waitFor(() =>
      expect((screen.getByRole('button', { name: '扫描目录' }) as HTMLButtonElement).disabled).toBe(
        false,
      ),
    );
    fireEvent.click(screen.getByRole('button', { name: '扫描目录' }));
    openScan();
    await screen.findByText('可入库');
    expect(screen.queryByRole('button', { name: '全不选' })).toBeNull();
    expect(screen.getByRole('button', { name: '全选' }).getAttribute('aria-pressed')).toBe('false');
    fireEvent.click(screen.getByRole('button', { name: '全选' }));
    expect(screen.getByText('已选 2 项')).toBeTruthy();
    expect(screen.getByRole('button', { name: '全不选' }).getAttribute('aria-pressed')).toBe(
      'true',
    );
    const row = screen.getByText(candidate.suggested_title).closest('.scan-item') as HTMLElement;
    fireEvent.click(within(row).getByRole('checkbox'));
    expect(screen.getByRole('button', { name: '全选' }).getAttribute('aria-pressed')).toBe('false');
    expect(screen.getByText('已选 1 项')).toBeTruthy();
    fireEvent.click(screen.getByRole('button', { name: '全选' }));
    fireEvent.click(screen.getByRole('button', { name: '全不选' }));
    expect(screen.getByRole('button', { name: '全选' }).getAttribute('aria-pressed')).toBe('false');
    fireEvent.click(screen.getByRole('button', { name: '全选' }));
    expect(screen.getByText('已选 2 项')).toBeTruthy();
    expect(screen.queryByRole('button', { name: '全选' })).toBeNull();
    expect(
      within(screen.getByRole('group', { name: '批量选择扫描结果' })).getAllByRole('button'),
    ).toHaveLength(1);
    fireEvent.click(screen.getByRole('button', { name: '加入游戏库' }));
    await waitFor(() =>
      expect(api.register).toHaveBeenCalledWith('scan', [
        expect.objectContaining({
          install_path: candidate.install_path,
          executable: 'main-v1.2.exe',
          exe_override: false,
        }),
        expect.objectContaining({
          install_path: 'E:/Butter/incomplete',
          executable: 'main-v1.2.exe',
          exe_override: false,
        }),
      ]),
    );
  });

  it('requires exact confirmation, preserves data on cancel/error and clears session preview and settings on success', async () => {
    await scan();
    goLibrary();
    fireEvent.click(screen.getByRole('button', { name: '设置' }));
    const modal = screen.getByRole('region', { name: '设置页面' });
    fireEvent.click(within(modal).getByRole('button', { name: '清空库…' }));
    const confirm = within(modal).getByRole('button', { name: '确认清空数据库' });
    expect((confirm as HTMLButtonElement).disabled).toBe(true);
    fireEvent.change(within(modal).getByRole('textbox', { name: '清空确认' }), {
      target: { value: '清空' },
    });
    expect((confirm as HTMLButtonElement).disabled).toBe(true);
    fireEvent.click(within(modal).getByRole('button', { name: '取消清空' }));
    expect(api.clearLibrary).not.toHaveBeenCalled();
    expect(
      within(screen.getByRole('navigation', { name: '主导航' })).getByRole('button', {
        name: /游戏库1/,
      }),
    ).toBeTruthy();
    fireEvent.click(within(modal).getByRole('button', { name: '清空库…' }));
    fireEvent.change(within(modal).getByRole('textbox', { name: '清空确认' }), {
      target: { value: '清空数据库' },
    });
    vi.mocked(api.clearLibrary).mockRejectedValueOnce('任务仍在运行');
    fireEvent.click(within(modal).getByRole('button', { name: '确认清空数据库' }));
    await screen.findByText('任务仍在运行');
    expect(
      within(screen.getByRole('navigation', { name: '主导航' })).getByRole('button', {
        name: /游戏库1/,
      }),
    ).toBeTruthy();
    vi.mocked(api.clearLibrary).mockResolvedValue({
      settings: { ...config, game_root: '', mtool_root: '' },
      warning: null,
    });
    fireEvent.click(within(modal).getByRole('button', { name: '确认清空数据库' }));
    await waitFor(() => expect(screen.queryByRole('region', { name: '设置页面' })).toBeNull());
    expect(api.clearLibrary).toHaveBeenLastCalledWith('清空数据库');
    expect(screen.queryByRole('button', { name: game.display_title })).toBeNull();
    expect(screen.queryByText(/查看上次扫描结果/)).toBeNull();
    expect(screen.queryByText('启动文件：main-v1.2.exe')).toBeNull();
    expect((screen.getByRole('button', { name: '扫描目录' }) as HTMLButtonElement).disabled).toBe(
      true,
    );
  });
  it('shows saved times/history and merges metadata updates without reloading the library', async () => {
    const initial = {
      ...game,
      created_at: '2020-01-01T00:00:00.000Z',
      last_launched_at: '2026-01-01T00:00:00.000Z',
      current_version: 'Unknown',
      version_source: 'unknown',
    };
    vi.mocked(api.games).mockResolvedValue([initial]);
    vi.mocked(api.launchHistory).mockResolvedValue([initial.last_launched_at]);
    vi.mocked(api.refreshMetadata).mockResolvedValue('metadata');
    vi.mocked(api.gamesByIds).mockResolvedValue([
      { ...initial, current_version: 'Ver1.06', engine: "Ren'Py" },
    ]);
    vi.mocked(api.job).mockResolvedValue({
      ...job,
      id: 'metadata',
      kind: 'metadata',
      changes: [],
      change_count: 0,
      next_cursor: 0,
      registered_ids: [game.id],
    });
    render(<App />);
    fireEvent.click(await screen.findByRole('button', { name: game.display_title }));
    const modal = screen.getByRole('dialog');
    expect(within(modal).getByText(/入库时间：/)).toBeTruthy();
    expect(within(modal).getByText(/上次运行：/)).toBeTruthy();
    await waitFor(() => expect(within(modal).getAllByRole('listitem').length).toBe(1));
    fireEvent.click(within(modal).getByRole('button', { name: '关闭弹窗' }));
    expect(screen.queryByRole('button', { name: '补充引擎/版本' })).toBeNull();
    expect(api.refreshMetadata).not.toHaveBeenCalled();
    expect(api.games).toHaveBeenCalledTimes(1);
  });

  it('keeps manual EXE, version and selection when preview is hidden and reopened', async () => {
    await scan();
    fireEvent.click(screen.getByRole('button', { name: '更改启动文件' }));
    vi.mocked(api.chooseLaunchFile).mockResolvedValue('包装/new-v2.0.exe');
    fireEvent.click(screen.getByRole('button', { name: '浏览启动文件…' }));
    await screen.findByText('启动文件：包装/new-v2.0.exe');
    await waitFor(() =>
      expect((screen.getByRole('textbox', { name: '游戏版本' }) as HTMLInputElement).value).toBe(
        'v2.0',
      ),
    );
    fireEvent.change(screen.getByRole('textbox', { name: '游戏版本' }), {
      target: { value: 'Final / manual' },
    });
    goLibrary();
    expect(screen.queryByText('启动文件：包装/new-v2.0.exe')).toBeNull();
    openScan();
    expect((screen.getByRole('checkbox') as HTMLInputElement).checked).toBe(true);
    expect((screen.getByRole('textbox', { name: '游戏版本' }) as HTMLInputElement).value).toBe(
      'Final / manual',
    );
    expect(api.startScan).toHaveBeenCalledTimes(1);
    expect(screen.queryByText('不应显示的技术细节')).toBeNull();
    fireEvent.click(screen.getByRole('button', { name: '加入游戏库' }));
    await waitFor(() =>
      expect(api.register).toHaveBeenCalledWith('scan', [
        {
          install_path: candidate.install_path,
          executable: '包装/new-v2.0.exe',
          external_player: null,
          exe_override: true,
          version: 'Final / manual',
          version_override: true,
        },
      ]),
    );
  });

  it('uses the native directory picker inside settings and retains unsaved edits on rejected close', async () => {
    render(<App />);
    await waitFor(() =>
      expect((screen.getByRole('button', { name: '设置' }) as HTMLButtonElement).disabled).toBe(
        false,
      ),
    );
    fireEvent.click(screen.getByRole('button', { name: '设置' }));
    const modal = screen.getByRole('region', { name: '设置页面' });
    vi.mocked(api.chooseDirectory).mockResolvedValue('D:/Games');
    fireEvent.click(within(modal).getAllByRole('button', { name: '浏览目录…' })[0]);
    await waitFor(() =>
      expect(
        (within(modal).getByRole('textbox', { name: /Game Root/ }) as HTMLInputElement).value,
      ).toBe('D:/Games'),
    );
    goLibrary();
    expect(screen.getByRole('region', { name: '设置页面' })).toBe(modal);
    fireEvent.click(within(modal).getByRole('button', { name: '继续编辑' }));
    expect(
      (within(modal).getByRole('textbox', { name: /Game Root/ }) as HTMLInputElement).value,
    ).toBe('D:/Games');
    expect(api.saveSettings).not.toHaveBeenCalled();
    fireEvent.click(
      within(screen.getByRole('navigation', { name: '主导航' })).getByRole('button', {
        name: /游戏库/,
      }),
    );
    fireEvent.click(within(modal).getByRole('button', { name: '放弃并切换' }));
    expect(screen.queryByRole('region', { name: '设置页面' })).toBeNull();
    expect(screen.getByRole('table')).toBeTruthy();
  });

  it.each(['UNPLAYED', 'COMPLETED'] as const)(
    'launches %s directly from its row without opening details or duplicating the request',
    async (status) => {
      vi.mocked(api.games).mockResolvedValue([{ ...game, play_status: status }]);
      let complete!: () => void;
      vi.mocked(api.play).mockImplementation(
        () =>
          new Promise<Game>((resolve) => {
            complete = () =>
              resolve({
                ...game,
                play_status: status === 'COMPLETED' ? status : 'PLAYING',
                last_launched_at: '2026-10-07T12:00:00Z',
              });
          }),
      );
      render(<App />);
      const play = await screen.findByRole('button', { name: `启动 ${game.display_title}` });
      fireEvent.click(play);
      expect(screen.queryByRole('dialog')).toBeNull();
      expect(api.play).toHaveBeenCalledWith(game.id);
      expect((play as HTMLButtonElement).disabled).toBe(true);
      expect(play.textContent).toBe('启动中');
      fireEvent.click(play);
      expect(api.play).toHaveBeenCalledTimes(1);
      complete();
      await waitFor(() => expect((play as HTMLButtonElement).disabled).toBe(false));
      const row = play.closest('tr')!;
      expect(within(row).getByText(status === 'COMPLETED' ? '已通关' : '正在玩')).toBeTruthy();
      expect(screen.queryByRole('dialog')).toBeNull();
      expect(api.games).toHaveBeenCalledTimes(1);
    },
  );
  it('restores the row launch button after a failed request without changing play status', async () => {
    vi.mocked(api.play).mockRejectedValue('启动文件已被移动');
    render(<App />);
    const play = await screen.findByRole('button', { name: `启动 ${game.display_title}` });
    fireEvent.click(play);
    await screen.findByText('启动文件已被移动');
    expect((play as HTMLButtonElement).disabled).toBe(false);
    expect(within(play.closest('tr')!).getByText('从未玩过')).toBeTruthy();
    expect(screen.queryByRole('dialog')).toBeNull();
  });
  it.each([
    { launch_type: 'DIRECT' as const, main_executable: null },
    {
      launch_type: 'EXTERNAL_PLAYER' as const,
      main_executable: 'qspgui.exe',
      external_player: {
        player_type: 'QSP' as const,
        scope: 'GAME_LOCAL' as const,
        game_file: null,
      },
    },
  ])('disables row Play for an incomplete $launch_type configuration', async (config) => {
    vi.mocked(api.games).mockResolvedValue([{ ...game, ...config }]);
    render(<App />);
    const play = await screen.findByRole('button', { name: `启动 ${game.display_title}` });
    expect((play as HTMLButtonElement).disabled).toBe(true);
    fireEvent.click(play);
    expect(api.play).not.toHaveBeenCalled();
  });

  it('keeps detail and library mounted during Play, blocks duplicate launch and shows local feedback', async () => {
    let complete!: () => void;
    vi.mocked(api.play).mockImplementation(
      () =>
        new Promise<Game>((resolve) => {
          complete = () =>
            resolve({
              ...game,
              play_status: 'PLAYING',
              last_launched_at: '2026-10-05T12:00:00.000Z',
            });
        }),
    );
    render(<App />);
    fireEvent.click(await screen.findByRole('button', { name: game.display_title }));
    const modal = screen.getByRole('dialog');
    const list = screen.getByRole('table');
    fireEvent.click(within(modal).getByRole('button', { name: '启动' }));
    const launching = within(modal).getByRole('button', { name: '启动中…' });
    expect((launching as HTMLButtonElement).disabled).toBe(true);
    fireEvent.click(launching);
    expect(api.play).toHaveBeenCalledTimes(1);
    expect(screen.getByRole('dialog')).toBe(modal);
    expect(screen.getByRole('table')).toBe(list);
    expect(screen.queryByText('正在处理…')).toBeNull();
    fireEvent.click(within(modal).getByRole('button', { name: '关闭弹窗' }));
    expect(screen.queryByRole('dialog')).toBeNull();
    fireEvent.click(screen.getByRole('button', { name: game.display_title }));
    const reopened = screen.getByRole('dialog');
    expect(
      (within(reopened).getByRole('button', { name: '启动中…' }) as HTMLButtonElement).disabled,
    ).toBe(true);
    complete();
    await waitFor(() =>
      expect(
        (within(reopened).getByRole('button', { name: '启动' }) as HTMLButtonElement).disabled,
      ).toBe(false),
    );
    await waitFor(() => {
      expect(
        (within(reopened).getByRole('combobox', { name: /游玩状态/ }) as HTMLSelectElement).value,
      ).toBe('PLAYING');
      expect(
        (within(reopened).getByRole('button', { name: '保存资料' }) as HTMLButtonElement).disabled,
      ).toBe(true);
    });
    expect(api.saveGame).not.toHaveBeenCalled();
    expect(screen.getByRole('dialog')).toBe(reopened);
    expect(api.games).toHaveBeenCalledTimes(1);
  });

  it.each(['DIRECT', 'MTOOL', 'EXTERNAL_PLAYER'] as const)(
    'launches unsaved %s configuration without saving, then saves through the fixed header',
    async (mode) => {
      vi.mocked(api.startLibraryCheck).mockResolvedValue('draft-paths');
      vi.mocked(api.job).mockResolvedValue({
        ...job,
        id: 'draft-paths',
        kind: 'paths',
        changes: [],
        next_cursor: 0,
        change_count: 0,
        path_checks: [
          {
            id: game.id,
            install_path: game.install_path,
            state: 'missing_launch',
            message: '旧启动文件不存在',
          },
        ],
      });
      vi.mocked(api.play).mockResolvedValue({
        ...game,
        play_status: 'PLAYING',
        last_launched_at: '2026-10-08T12:00:00Z',
      });
      vi.mocked(api.saveGame).mockImplementation(async (edit) => ({ ...game, ...edit }));
      render(<App />);
      await screen.findByText('启动文件缺失');
      fireEvent.click(await screen.findByRole('button', { name: game.display_title }));
      const detail = screen.getByRole('dialog', { name: game.display_title });
      expect(
        (within(detail).getByRole('button', { name: '启动' }) as HTMLButtonElement).disabled,
      ).toBe(true);
      fireEvent.change(within(detail).getByRole('textbox', { name: '显示名称' }), {
        target: { value: '未保存标题' },
      });
      fireEvent.change(within(detail).getByRole('combobox', { name: '启动方式' }), {
        target: { value: mode },
      });
      if (mode === 'EXTERNAL_PLAYER') {
        fireEvent.change(within(detail).getByRole('combobox', { name: 'QSP 播放器' }), {
          target: { value: '新版/player.exe' },
        });
        fireEvent.change(within(detail).getByRole('combobox', { name: 'QSP 主游戏文件' }), {
          target: { value: '新版/story.qsp' },
        });
      } else {
        fireEvent.change(
          within(detail).getByRole('combobox', { name: /启动文件（相对游戏目录）/ }),
          { target: { value: '新版/new.exe' } },
        );
        if (mode === 'MTOOL')
          fireEvent.change(within(detail).getByRole('combobox', { name: 'MTool loader 选择' }), {
            target: { value: 'loaders/mzHook.dll' },
          });
      }
      fireEvent.click(within(detail).getByText('高级启动选项'));
      fireEvent.change(within(detail).getByRole('textbox', { name: /工作目录/ }), {
        target: { value: '新版' },
      });
      const save = within(detail).getByRole('button', { name: '保存资料' });
      expect(save.closest('.modal-header')).toBeTruthy();
      expect(detail.querySelector('.modal-body')?.contains(save)).toBe(false);
      fireEvent.click(within(detail).getByRole('button', { name: '启动' }));
      await within(detail).findByText('启动请求已发送。');
      expect(api.play).toHaveBeenCalledWith(game.id, {
        launch_type: mode,
        main_executable: mode === 'EXTERNAL_PLAYER' ? '新版/player.exe' : '新版/new.exe',
        working_directory: '新版',
        external_player:
          mode === 'EXTERNAL_PLAYER'
            ? { player_type: 'QSP', scope: 'GAME_LOCAL', game_file: '新版/story.qsp' }
            : null,
        mtool_target_exe: mode === 'MTOOL' ? '新版/new.exe' : null,
        mtool_loader: mode === 'MTOOL' ? 'loaders/mzHook.dll' : null,
      });
      expect(api.saveGame).not.toHaveBeenCalled();
      expect((save as HTMLButtonElement).disabled).toBe(false);
      expect(
        (within(detail).getByRole('textbox', { name: '显示名称' }) as HTMLInputElement).value,
      ).toBe('未保存标题');
      await waitFor(() =>
        expect(
          (within(detail).getByRole('combobox', { name: /游玩状态/ }) as HTMLSelectElement).value,
        ).toBe('PLAYING'),
      );
      fireEvent.click(save);
      await within(detail).findByText('游戏资料已保存。');
      expect(api.saveGame).toHaveBeenCalledWith(
        expect.objectContaining({
          display_title: '未保存标题',
          working_directory: '新版',
          launch_type: mode,
        }),
      );
      expect((save as HTMLButtonElement).disabled).toBe(true);
    },
  );

  it('reports and cancels optional EXE / BAT analysis inside the detail modal', async () => {
    let response: JobPage = {
      ...job,
      id: 'detail-analysis',
      kind: 'analysis',
      root: game.install_path,
      status: 'running',
      processed: 0,
      changes: [],
      next_cursor: 0,
      change_count: 0,
      active: { [game.install_path]: game.install_path },
    };
    vi.mocked(api.startGameAnalysis).mockResolvedValue('detail-analysis');
    vi.mocked(api.job).mockImplementation(() => Promise.resolve(response));
    vi.mocked(api.cancel).mockResolvedValue();
    render(<App />);
    fireEvent.click(await screen.findByRole('button', { name: game.display_title }));
    const modal = screen.getByRole('dialog');
    fireEvent.click(within(modal).getByRole('button', { name: '分析启动配置' }));
    fireEvent.click(await within(modal).findByRole('button', { name: '取消任务' }));
    await waitFor(() => expect(api.cancel).toHaveBeenCalledWith('detail-analysis'));
    response = { ...response, status: 'cancelled', active: {} };
    await within(modal).findByText('分析已取消，原有结果保留。');
    await waitFor(() =>
      expect(
        (within(modal).getByRole('button', { name: '分析启动配置' }) as HTMLButtonElement).disabled,
      ).toBe(false),
    );
    response = {
      ...response,
      id: 'detail-analysis-2',
      status: 'completed',
      processed: 1,
      changes: [{ ...candidate, registered_id: game.id }],
      next_cursor: 1,
      change_count: 1,
    };
    vi.mocked(api.startGameAnalysis).mockResolvedValue('detail-analysis-2');
    fireEvent.change(within(modal).getByRole('combobox', { name: '游戏引擎' }), {
      target: { value: 'Unity' },
    });
    response.changes = [{ ...candidate, registered_id: game.id, engine: 'WOLF RPG Editor' }];
    fireEvent.click(within(modal).getByRole('button', { name: '分析启动配置' }));
    await within(modal).findByText('分析结束，可查看下方详情。');
    expect(within(modal).getByText('main-v1.2.exe · Unknown')).toBeTruthy();
    expect((within(modal).getByRole('textbox', { name: '版本' }) as HTMLInputElement).value).toBe(
      'Final',
    );
    const engine = within(modal).getByRole('combobox', { name: '游戏引擎' }) as HTMLSelectElement;
    expect(engine.value).toBe('Unity');
    expect(api.saveGame).not.toHaveBeenCalled();
    fireEvent.click(within(modal).getByRole('button', { name: '使用识别引擎' }));
    expect(engine.value).toBe('WOLF RPG Editor');
    expect(api.saveGame).not.toHaveBeenCalled();
    expect(
      (
        within(modal).getByRole('combobox', {
          name: /^启动文件（相对游戏目录）/,
        }) as HTMLInputElement
      ).value,
    ).toBe(game.main_executable);
  });

  it('checks paths on startup, filters problems and blocks a missing game launch without deleting it', async () => {
    const other = {
      ...game,
      id: 'other',
      display_title: '可访问游戏',
      install_path: 'E:/Butter/other',
    };
    vi.mocked(api.games).mockResolvedValue([game, other]);
    vi.mocked(api.startLibraryCheck).mockResolvedValue('paths');
    vi.mocked(api.job).mockResolvedValue({
      ...job,
      id: 'paths',
      kind: 'paths',
      changes: [],
      next_cursor: 0,
      change_count: 0,
      path_checks: [
        {
          id: game.id,
          install_path: game.install_path,
          state: 'missing_directory',
          message: '游戏目录不存在',
        },
        {
          id: other.id,
          install_path: other.install_path,
          state: 'available',
          message: '目录和启动文件可访问',
        },
      ],
    });
    render(<App />);
    const badge = await screen.findByText('目录缺失');
    expect(badge.closest('.game-identity-text')?.querySelector('.game-link')?.textContent).toBe(
      game.display_title,
    );
    const filter = screen.getByRole('button', { name: '需处理 (1)' });
    expect(filter.getAttribute('aria-pressed')).toBe('false');
    fireEvent.click(filter);
    expect(filter.getAttribute('aria-pressed')).toBe('true');
    fireEvent.click(filter);
    expect(filter.getAttribute('aria-pressed')).toBe('false');
    fireEvent.click(filter);
    expect(screen.queryByRole('button', { name: other.display_title })).toBeNull();
    fireEvent.click(screen.getByRole('button', { name: game.display_title }));
    const modal = screen.getByRole('dialog');
    expect(
      (within(modal).getByRole('button', { name: '启动' }) as HTMLButtonElement).disabled,
    ).toBe(true);
    expect(api.play).not.toHaveBeenCalled();
    expect(api.removeGame).not.toHaveBeenCalled();
    expect(api.startScan).not.toHaveBeenCalled();
  });

  it.each(['DIRECT', 'MTOOL'] as const)(
    'rechecks paths after saving a replacement %s launch file instead of retaining a stale missing badge',
    async (mode) => {
      const initial = {
        ...game,
        launch_type: mode,
        mtool_target_exe: 'old.exe',
        mtool_loader: 'loaders/loader.dll',
      };
      const saved =
        mode === 'DIRECT'
          ? { ...initial, main_executable: 'new.exe' }
          : { ...initial, mtool_target_exe: 'new.exe' };
      vi.mocked(api.games).mockResolvedValue([initial]);
      vi.mocked(api.saveGame).mockResolvedValue(saved);
      vi.mocked(api.startLibraryCheck)
        .mockResolvedValueOnce('paths-before')
        .mockResolvedValue('paths-after');
      vi.mocked(api.job).mockImplementation((id) =>
        Promise.resolve({
          ...job,
          id,
          kind: 'paths',
          changes: [],
          next_cursor: 0,
          change_count: 0,
          path_checks: [
            {
              id: game.id,
              install_path: game.install_path,
              state: id === 'paths-before' ? 'missing_launch' : 'available',
              message: '检查结果',
            },
          ],
        }),
      );
      render(<App />);
      await screen.findByText('启动文件缺失');
      fireEvent.click(screen.getByRole('button', { name: game.display_title }));
      const modal = screen.getByRole('dialog');
      fireEvent.change(
        within(modal).getByRole('combobox', {
          name: /启动文件（相对游戏目录）/,
        }),
        { target: { value: 'new.exe' } },
      );
      fireEvent.click(within(modal).getByRole('button', { name: '保存资料' }));
      await waitFor(() => expect(api.startLibraryCheck).toHaveBeenCalledTimes(2));
      await waitFor(() =>
        expect(
          (within(modal).getByRole('button', { name: '启动' }) as HTMLButtonElement).disabled,
        ).toBe(false),
      );
      expect(within(modal).queryByText('启动文件缺失')).toBeNull();
      expect(api.play).not.toHaveBeenCalled();
    },
  );

  it('uses shared MTool and the selected game EXE with automatic loader instead of duplicate target fields', async () => {
    vi.mocked(api.settings).mockResolvedValue({ ...config, mtool_root: 'D:/Tools/MTool' });
    vi.mocked(api.previewMtoolLaunch).mockResolvedValue({
      shared_root: 'D:/Tools/MTool',
      target_exe: game.main_executable!,
      architecture: 'x86',
      loader: 'loaders/mzHook32.dll',
      runtime: 'MTool.exe',
      working_directory: game.install_path,
    });
    vi.mocked(api.saveGame).mockResolvedValue({
      ...game,
      launch_type: 'MTOOL',
      mtool_target_exe: game.main_executable,
    });
    render(<App />);
    fireEvent.click(await screen.findByRole('button', { name: game.display_title }));
    const detail = screen.getByRole('dialog', { name: game.display_title });
    fireEvent.change(within(detail).getByRole('combobox', { name: '启动方式' }), {
      target: { value: 'MTOOL' },
    });
    expect(within(detail).getByText('D:/Tools/MTool\\MTool.exe')).toBeTruthy();
    expect(within(detail).queryByRole('textbox', { name: 'MTool target EXE' })).toBeNull();
    expect(
      (within(detail).getByRole('combobox', { name: 'MTool loader 选择' }) as HTMLSelectElement)
        .value,
    ).toBe('');
    await waitFor(() =>
      expect(api.previewMtoolLaunch).toHaveBeenCalledWith(game.id, game.main_executable, null, '.'),
    );
    await within(detail).findByText(/游戏 任意.exe · 32 位 · mzHook32.dll/);
    fireEvent.click(within(detail).getByRole('button', { name: '保存资料' }));
    await waitFor(() =>
      expect(api.saveGame).toHaveBeenCalledWith(
        expect.objectContaining({
          launch_type: 'MTOOL',
          main_executable: game.main_executable,
          mtool_target_exe: game.main_executable,
          mtool_loader: null,
          working_directory: '.',
        }),
      ),
    );
    expect(api.play).not.toHaveBeenCalled();
  });

  it('opens a separate confirmation popup for unsaved detail edits and Escape retains the draft', async () => {
    render(<App />);
    fireEvent.click(await screen.findByRole('button', { name: game.display_title }));
    const detail = screen.getByRole('dialog', { name: game.display_title });
    fireEvent.change(within(detail).getByRole('textbox', { name: '版本' }), {
      target: { value: 'Unsaved' },
    });
    fireEvent.click(within(detail).getByRole('button', { name: '关闭弹窗' }));
    const popup = screen.getByRole('dialog', { name: '放弃未保存修改？' });
    expect((popup as HTMLDialogElement).open).toBe(true);
    expect(screen.getByRole('dialog', { name: game.display_title })).toBe(detail);
    fireEvent(popup, new Event('cancel', { bubbles: true, cancelable: true }));
    expect(screen.queryByRole('dialog', { name: '放弃未保存修改？' })).toBeNull();
    expect((within(detail).getByRole('textbox', { name: '版本' }) as HTMLInputElement).value).toBe(
      'Unsaved',
    );
    fireEvent.click(within(detail).getByRole('button', { name: '关闭弹窗' }));
    fireEvent.click(
      within(screen.getByRole('dialog', { name: '放弃未保存修改？' })).getByRole('button', {
        name: '放弃并关闭',
      }),
    );
    expect(screen.queryByRole('dialog')).toBeNull();
    expect(api.saveGame).not.toHaveBeenCalled();
  });

  it('displays a popup for unsaved settings navigation and changes pages only after discard', async () => {
    render(<App />);
    await screen.findByRole('button', { name: game.display_title });
    openConfig('设置');
    const page = screen.getByRole('region', { name: '设置页面' });
    fireEvent.change(within(page).getByRole('textbox', { name: /Game Root/ }), {
      target: { value: 'D:/Unsaved' },
    });
    goLibrary();
    const popup = screen.getByRole('dialog', { name: '放弃未保存修改？' });
    expect((popup as HTMLDialogElement).open).toBe(true);
    expect(screen.getByRole('region', { name: '设置页面' })).toBe(page);
    fireEvent.click(within(popup).getByRole('button', { name: '放弃并切换' }));
    expect(screen.getByRole('table')).toBeTruthy();
    expect(screen.queryByRole('dialog')).toBeNull();
    expect(api.saveSettings).not.toHaveBeenCalled();
  });

  it('syncs detected MTool defaults for existing records after a completed scan', async () => {
    const changed = {
      ...game,
      launch_type: 'MTOOL' as const,
      mtool_target_exe: game.main_executable,
    };
    vi.mocked(api.syncScanMtool).mockResolvedValue([changed]);
    const completed: JobPage = {
      ...job,
      changes: [{ ...candidate, registered_id: game.id, mtool_detected: true }],
      path_checks: [
        {
          id: game.id,
          install_path: game.install_path,
          state: 'missing_launch',
          message: '旧检查结果',
        },
      ],
    };
    vi.mocked(api.job)
      .mockResolvedValueOnce(completed)
      .mockResolvedValue({
        ...completed,
        path_checks: [
          {
            id: game.id,
            install_path: game.install_path,
            state: 'available',
            message: '目录和启动文件可访问',
          },
        ],
      });
    render(<App />);
    await waitFor(() =>
      expect((screen.getByRole('button', { name: '扫描目录' }) as HTMLButtonElement).disabled).toBe(
        false,
      ),
    );
    fireEvent.click(screen.getByRole('button', { name: '扫描目录' }));
    openScan();
    await screen.findByText('公共 MTool');
    expect(api.syncScanMtool).toHaveBeenCalledWith('scan');
    goLibrary();
    expect(within(screen.getByRole('table')).getByText('MTool')).toBeTruthy();
    await waitFor(() => expect(screen.queryByText('启动文件缺失')).toBeNull());
    expect(api.job).toHaveBeenLastCalledWith('scan', job.next_cursor);
    expect(api.startLibraryCheck).toHaveBeenCalledTimes(1);
    expect(api.register).not.toHaveBeenCalled();
  });

  it('cancels the unified scan and retains its completed path checks without removing unchecked records', async () => {
    const other = {
      ...game,
      id: 'other',
      display_title: '尚未检查游戏',
      install_path: 'E:/Butter/other',
    };
    vi.mocked(api.games).mockResolvedValue([game, other]);
    vi.mocked(api.startLibraryCheck).mockResolvedValue('');
    let response: JobPage = {
      ...job,
      id: 'scan',
      kind: 'scan',
      status: 'running',
      total: 2,
      processed: 1,
      changes: [],
      next_cursor: 0,
      change_count: 0,
      path_checks: [],
    };
    vi.mocked(api.job).mockImplementation(() => Promise.resolve(response));
    vi.mocked(api.cancel).mockResolvedValue();
    render(<App />);
    await waitFor(() =>
      expect((screen.getByRole('button', { name: '扫描目录' }) as HTMLButtonElement).disabled).toBe(
        false,
      ),
    );
    expect(screen.queryByRole('button', { name: '检查目录' })).toBeNull();
    fireEvent.click(screen.getByRole('button', { name: '扫描目录' }));
    fireEvent.click(await screen.findByRole('button', { name: '取消任务' }));
    await waitFor(() => expect(api.cancel).toHaveBeenCalledWith('scan'));
    response = {
      ...response,
      status: 'cancelled',
      path_checks: [
        {
          id: game.id,
          install_path: game.install_path,
          state: 'missing_launch',
          message: '启动文件不存在',
        },
      ],
    };
    await screen.findByText('启动文件缺失');
    expect(screen.getByRole('button', { name: other.display_title })).toBeTruthy();
    expect(api.removeGame).not.toHaveBeenCalled();
  });

  it('uses completed scan path checks without starting another full library check', async () => {
    vi.mocked(api.job).mockResolvedValue({
      ...job,
      path_checks: [
        {
          id: game.id,
          install_path: game.install_path,
          state: 'missing_launch',
          message: '启动文件不存在',
        },
      ],
    });
    render(<App />);
    await waitFor(() =>
      expect((screen.getByRole('button', { name: '扫描目录' }) as HTMLButtonElement).disabled).toBe(
        false,
      ),
    );
    expect(screen.queryByRole('button', { name: '检查目录' })).toBeNull();
    const automaticChecks = vi.mocked(api.startLibraryCheck).mock.calls.length;
    fireEvent.click(screen.getByRole('button', { name: '扫描目录' }));
    await screen.findByText('启动文件缺失');
    await waitFor(() => expect(api.syncScanMtool).toHaveBeenCalledWith('scan'));
    expect(api.startLibraryCheck).toHaveBeenCalledTimes(automaticChecks);
    expect(screen.getByText('完成 · 1 个目录，1 项需处理')).toBeTruthy();
    expect(api.removeGame).not.toHaveBeenCalled();
  });

  it('links a scanned new directory to an explicitly chosen existing identity and clears its import selection', async () => {
    const plan = {
      id: game.id,
      expected_install_path: game.install_path,
      install_path: candidate.install_path,
      main_executable: 'main-v1.2.exe',
      working_directory: '.',
      launch_type: 'DIRECT' as const,
      mtool_target_exe: null,
    };
    const saved = {
      ...game,
      install_path: candidate.install_path,
      main_executable: plan.main_executable,
    };
    vi.mocked(api.previewRelocation).mockResolvedValue(plan);
    vi.mocked(api.relocateGame).mockResolvedValue(saved);
    await scan();
    fireEvent.click(screen.getByRole('button', { name: '关联已有游戏…' }));
    const modal = screen.getByRole('dialog');
    expect(api.previewRelocation).not.toHaveBeenCalled();
    fireEvent.change(within(modal).getByRole('combobox', { name: '已有游戏记录' }), {
      target: { value: game.id },
    });
    await waitFor(() =>
      expect(
        (within(modal).getByRole('button', { name: '确认关联目录' }) as HTMLButtonElement).disabled,
      ).toBe(false),
    );
    vi.mocked(api.games).mockResolvedValue([saved]);
    vi.mocked(api.job).mockResolvedValue({
      ...job,
      changes: [{ ...candidate, registered_id: game.id }],
    });
    fireEvent.click(within(modal).getByRole('button', { name: '确认关联目录' }));
    await waitFor(() => expect(screen.queryByRole('dialog')).toBeNull());
    await screen.findByText('已入库');
    expect(api.register).not.toHaveBeenCalled();
    expect((screen.getByRole('button', { name: '加入游戏库' }) as HTMLButtonElement).disabled).toBe(
      true,
    );
    goLibrary();
    expect(screen.getByRole('button', { name: game.display_title })).toBeTruthy();
    expect(screen.getByRole('navigation', { name: '主导航' }).textContent).toContain('游戏库1');
  });

  it('requires explicit confirmation to remove only one library record and retains the other game', async () => {
    const other = {
      ...game,
      id: 'other',
      display_title: '保留游戏',
      install_path: 'E:/Butter/other',
    };
    vi.mocked(api.games).mockResolvedValueOnce([game, other]).mockResolvedValue([other]);
    vi.mocked(api.removeGame).mockResolvedValue();
    render(<App />);
    fireEvent.click(await screen.findByRole('button', { name: game.display_title }));
    const modal = screen.getByRole('dialog');
    fireEvent.click(within(modal).getByRole('button', { name: '从库中移除' }));
    const confirmation = screen.getByRole('dialog', { name: `从库中移除：${game.display_title}` });
    expect(api.removeGame).not.toHaveBeenCalled();
    expect(within(confirmation).getByText(/游戏与存档文件保留/)).toBeTruthy();
    expect(confirmation.classList.contains('modal-maintenance')).toBe(true);
    const cancel = within(confirmation).getByRole('button', { name: '取消移除' });
    expect(cancel.closest('.modal-footer')).toBeTruthy();
    fireEvent(confirmation, new Event('cancel', { cancelable: true }));
    expect(screen.queryByRole('dialog', { name: `从库中移除：${game.display_title}` })).toBeNull();
    expect(screen.getByRole('dialog', { name: game.display_title })).toBe(modal);
    expect(api.removeGame).not.toHaveBeenCalled();
    fireEvent.click(within(modal).getByRole('button', { name: '从库中移除' }));
    const reopened = screen.getByRole('dialog', { name: `从库中移除：${game.display_title}` });
    vi.mocked(api.removeGame).mockRejectedValueOnce('当前任务尚未完成');
    fireEvent.click(within(reopened).getByRole('button', { name: '确认移除记录' }));
    await within(reopened).findByRole('alert');
    expect(screen.getByRole('dialog', { name: `从库中移除：${game.display_title}` })).toBe(
      reopened,
    );
    fireEvent.click(within(reopened).getByRole('button', { name: '确认移除记录' }));
    await waitFor(() => expect(screen.queryByRole('dialog')).toBeNull());
    expect(api.removeGame).toHaveBeenCalledWith(game.id);
    expect(screen.queryByRole('button', { name: game.display_title })).toBeNull();
    expect(screen.getByRole('button', { name: other.display_title })).toBeTruthy();
  });

  it('shows folder failures in a modal and removes only the explicitly selected record, keeping failures retryable', async () => {
    const other = {
      ...game,
      id: 'other',
      display_title: '保留游戏',
      install_path: 'E:/Butter/other',
    };
    vi.mocked(api.games).mockResolvedValue([game, other]);
    vi.mocked(api.removeGame)
      .mockRejectedValueOnce(new Error('请先结束当前任务'))
      .mockResolvedValue();
    render(<App />);
    fireEvent.click(await screen.findByRole('button', { name: game.display_title }));
    const detail = screen.getByRole('dialog', { name: game.display_title });
    fireEvent.click(within(detail).getByRole('button', { name: '打开目录' }));
    await waitFor(() => expect(api.openFolder).toHaveBeenCalledWith(game.id));
    expect(screen.queryByRole('dialog', { name: '无法打开游戏目录' })).toBeNull();
    await waitFor(() =>
      expect(
        (within(detail).getByRole('button', { name: '打开目录' }) as HTMLButtonElement).disabled,
      ).toBe(false),
    );
    vi.mocked(api.openFolder).mockRejectedValue(
      '文件操作失败：The system cannot find the file specified. (os error 2)',
    );
    fireEvent.click(within(detail).getByRole('button', { name: '打开目录' }));
    let errorDialog = await screen.findByRole('dialog', { name: '无法打开游戏目录' });
    expect(within(errorDialog).getByText(game.install_path)).toBeTruthy();
    expect(within(errorDialog).getByText(/游戏与存档文件保留/)).toBeTruthy();
    expect(detail.querySelector('.inline-status')?.textContent).not.toContain('os error');
    expect(api.removeGame).not.toHaveBeenCalled();
    fireEvent.click(within(errorDialog).getByRole('button', { name: '返回详情' }));
    expect(screen.queryByRole('dialog', { name: '无法打开游戏目录' })).toBeNull();
    fireEvent.click(within(detail).getByRole('button', { name: '打开目录' }));
    errorDialog = await screen.findByRole('dialog', { name: '无法打开游戏目录' });
    fireEvent.click(within(errorDialog).getByRole('button', { name: '从库中移除' }));
    expect((await within(errorDialog).findByRole('alert')).textContent).toContain(
      '请先结束当前任务',
    );
    expect(screen.getByRole('dialog', { name: game.display_title })).toBeTruthy();
    await waitFor(() =>
      expect(
        (within(errorDialog).getByRole('button', { name: '从库中移除' }) as HTMLButtonElement)
          .disabled,
      ).toBe(false),
    );
    vi.mocked(api.games).mockResolvedValue([other]);
    fireEvent.click(within(errorDialog).getByRole('button', { name: '从库中移除' }));
    await waitFor(() => expect(screen.queryByRole('dialog')).toBeNull());
    expect(api.removeGame).toHaveBeenCalledWith(game.id);
    expect(screen.queryByRole('button', { name: game.display_title })).toBeNull();
    expect(screen.getByRole('button', { name: other.display_title })).toBeTruthy();
  });

  it('preserves unsaved detail edits and disables removing from the folder error modal', async () => {
    vi.mocked(api.openFolder).mockRejectedValue('文件操作失败：Access is denied. (os error 5)');
    render(<App />);
    fireEvent.click(await screen.findByRole('button', { name: game.display_title }));
    const detail = screen.getByRole('dialog', { name: game.display_title });
    fireEvent.change(within(detail).getByLabelText('显示名称'), {
      target: { value: '未保存名称' },
    });
    fireEvent.click(within(detail).getByRole('button', { name: '打开目录' }));
    const errorDialog = await screen.findByRole('dialog', { name: '无法打开游戏目录' });
    const remove = within(errorDialog).getByRole('button', {
      name: '从库中移除',
    }) as HTMLButtonElement;
    expect(remove.disabled).toBe(true);
    fireEvent.click(remove);
    expect(api.removeGame).not.toHaveBeenCalled();
    fireEvent.click(within(errorDialog).getByRole('button', { name: '返回详情' }));
    expect((within(detail).getByLabelText('显示名称') as HTMLInputElement).value).toBe(
      '未保存名称',
    );
  });

  it('previews relocation before committing and keeps identity and metadata in the existing detail', async () => {
    const path = 'E:/Butter/改名游戏';
    const plan = {
      id: game.id,
      expected_install_path: game.install_path,
      install_path: path,
      main_executable: game.main_executable,
      working_directory: '.',
      launch_type: 'DIRECT' as const,
      mtool_target_exe: null,
    };
    const saved = { ...game, install_path: path };
    vi.mocked(api.games).mockResolvedValueOnce([game]).mockResolvedValue([saved]);
    vi.mocked(api.chooseDirectory).mockResolvedValue(path);
    vi.mocked(api.previewRelocation).mockResolvedValue(plan);
    vi.mocked(api.relocateGame).mockResolvedValue(saved);
    render(<App />);
    fireEvent.click(await screen.findByRole('button', { name: game.display_title }));
    const modal = screen.getByRole('dialog');
    fireEvent.click(within(modal).getByRole('button', { name: '关联新目录' }));
    await waitFor(() =>
      expect(
        (within(modal).getByRole('button', { name: '确认关联目录' }) as HTMLButtonElement).disabled,
      ).toBe(false),
    );
    expect(api.previewRelocation).toHaveBeenCalledWith(game.id, path);
    expect(api.relocateGame).not.toHaveBeenCalled();
    fireEvent.click(within(modal).getByRole('button', { name: '确认关联目录' }));
    await within(modal).findByText('已关联新目录，原有游戏资料保留。');
    expect(api.relocateGame).toHaveBeenCalledWith(plan);
    expect(screen.getByRole('dialog')).toBe(modal);
    expect((within(modal).getByRole('textbox', { name: '版本' }) as HTMLInputElement).value).toBe(
      game.current_version,
    );
    expect(within(modal).getByText(path)).toBeTruthy();
  });

  it('uses saved MTool configuration, reports validation and disables launch while edits are unsaved', async () => {
    vi.mocked(api.settings).mockResolvedValue({ ...config, mtool_root: 'D:/Tools/MTool' });
    vi.mocked(api.checkMtool).mockResolvedValue([
      {
        label: 'MTool 主程序',
        path: 'D:/Tools/MTool/MTool.exe',
        available: true,
        message: '文件可访问',
      },
    ]);
    vi.mocked(api.runMtool).mockResolvedValue();
    render(<App />);
    await screen.findByRole('button', { name: game.display_title });
    fireEvent.click(
      within(screen.getByRole('navigation', { name: '主导航' })).getByRole('button', {
        name: 'MTool',
      }),
    );
    const page = screen.getByRole('region', { name: 'MTool页面' });
    fireEvent.click(within(page).getByRole('button', { name: '验证 MTool 文件' }));
    await within(page).findByText('MTool 主程序 · 可访问');
    fireEvent.click(within(page).getByRole('button', { name: '运行 MTool' }));
    await within(page).findByText('MTool 启动请求已发送。');
    expect(api.runMtool).toHaveBeenCalledTimes(1);
    expect(api.play).not.toHaveBeenCalled();
    fireEvent.change(within(page).getByRole('textbox', { name: /Shared MTool Root/ }), {
      target: { value: 'D:/Unsaved' },
    });
    expect(
      (within(page).getByRole('button', { name: '运行 MTool' }) as HTMLButtonElement).disabled,
    ).toBe(true);
    expect(
      (within(page).getByRole('button', { name: '验证 MTool 文件' }) as HTMLButtonElement).disabled,
    ).toBe(true);
  });

  it('reports MTool launch errors without claiming the tool started', async () => {
    vi.mocked(api.settings).mockResolvedValue({ ...config, mtool_root: 'D:/Tools/MTool' });
    vi.mocked(api.runMtool).mockRejectedValue(new Error('MTool.exe 不存在'));
    render(<App />);
    await screen.findByRole('button', { name: game.display_title });
    fireEvent.click(
      within(screen.getByRole('navigation', { name: '主导航' })).getByRole('button', {
        name: 'MTool',
      }),
    );
    const page = screen.getByRole('region', { name: 'MTool页面' });
    fireEvent.click(within(page).getByRole('button', { name: '运行 MTool' }));
    await within(page).findByText(/MTool.exe 不存在/);
    expect(within(page).queryByText('MTool 启动请求已发送。')).toBeNull();
  });

  it('allows cancellation while preview is hidden and retains already completed results', async () => {
    vi.mocked(api.job).mockResolvedValue({
      ...job,
      status: 'running',
      total: 10,
      processed: 1,
      active: { 'E:/Butter/慢游戏': 'E:/Butter/慢游戏/包装' },
    });
    vi.mocked(api.cancel).mockResolvedValue();
    render(<App />);
    await waitFor(() =>
      expect((screen.getByRole('button', { name: '扫描目录' }) as HTMLButtonElement).disabled).toBe(
        false,
      ),
    );
    fireEvent.click(screen.getByRole('button', { name: '扫描目录' }));
    openScan();
    await screen.findByText('启动文件：main-v1.2.exe');
    goLibrary();
    fireEvent.click(screen.getByRole('button', { name: '取消任务' }));
    await waitFor(() => expect(api.cancel).toHaveBeenCalledWith('scan'));
    vi.mocked(api.job).mockResolvedValue({
      ...job,
      status: 'cancelled',
      changes: [],
      next_cursor: 1,
    });
    await screen.findByText('已取消 · 已完成的结果保留');
    openScan();
    expect(screen.getByText('启动文件：main-v1.2.exe')).toBeTruthy();
    expect(api.startScan).toHaveBeenCalledTimes(1);
  });
  it('saves a manual completion status through the existing detail form', async () => {
    vi.mocked(api.saveGame).mockImplementation(async (edit) => ({ ...game, ...edit }));
    render(<App />);
    fireEvent.click(await screen.findByRole('button', { name: game.display_title }));
    fireEvent.change(screen.getByRole('combobox', { name: /游玩状态/ }), {
      target: { value: 'COMPLETED' },
    });
    fireEvent.click(screen.getByRole('button', { name: '保存资料' }));
    await waitFor(() =>
      expect(api.saveGame).toHaveBeenCalledWith(
        expect.objectContaining({ play_status: 'COMPLETED' }),
      ),
    );
    await screen.findByText('游戏资料已保存。');
    expect((screen.getByRole('combobox', { name: /游玩状态/ }) as HTMLSelectElement).value).toBe(
      'COMPLETED',
    );
  });
  it('applies state and engine filters together to the real library rows', async () => {
    const games = [
      {
        ...game,
        id: 'finished',
        display_title: 'Finished',
        play_status: 'COMPLETED' as const,
        engine: 'QSP',
      },
      {
        ...game,
        id: 'unity',
        display_title: 'Unity Game',
        play_status: 'PLAYING' as const,
        engine: 'Unity',
      },
      {
        ...game,
        id: 'qsp',
        display_title: 'QSP Game',
        play_status: 'PLAYING' as const,
        engine: 'QSP',
      },
    ];
    vi.mocked(api.games).mockResolvedValue(games);
    render(<App />);
    await screen.findByRole('button', { name: 'QSP Game' });
    fireEvent.click(screen.getByRole('button', { name: '筛选' }));
    const filters = screen.getByRole('dialog', { name: '筛选游戏' });
    fireEvent.click(within(filters).getByRole('checkbox', { name: '正在玩' }));
    fireEvent.click(within(filters).getByRole('checkbox', { name: 'QSP' }));
    fireEvent.click(within(filters).getByRole('button', { name: '完成' }));
    expect(screen.getByRole('button', { name: 'QSP Game' })).toBeTruthy();
    expect(screen.queryByRole('button', { name: 'Finished' })).toBeNull();
    expect(screen.queryByRole('button', { name: 'Unity Game' })).toBeNull();
    expect(screen.getByText('找到 1 / 3 个游戏')).toBeTruthy();
  });
});
