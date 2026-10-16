import { waitForEnabledButton } from './testing/interactions';
import { resetApiMock } from './testing/apiMock';
import { makeSettings, makeCandidate, makeJob, makeGame } from './testing/fixtures';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { act, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { api } from './api';
import App from './App';
import { engineQuickGroups } from './gameStatus';
import type { Game, JobPage, ScanCandidate, Settings } from './types';

vi.mock('@tauri-apps/api/core', () => ({ isTauri: () => true }));
const nativeWindow = vi.hoisted(() => ({
  isMaximized: vi.fn().mockResolvedValue(false),
  destroy: vi.fn().mockResolvedValue(undefined),
  onCloseRequested: vi.fn(),
}));
vi.mock('@tauri-apps/api/window', () => ({ getCurrentWindow: () => nativeWindow }));
vi.mock('./api', async (original) => {
  const { api } = await original<typeof import('./api')>();
  const { createApiMock } = await import('./testing/apiMock');
  return { api: createApiMock(api) };
});

const config: Settings = makeSettings({});
const candidate: ScanCandidate = makeCandidate({
  install_path: 'E:/Butter/游戏',
  suggested_title: '游戏',
  executables: [
    {
      relative_path: 'main-v1.2.exe',
      architecture: 'Unknown',
      score: 100,
      size_bytes: 1,
      modified_ms: 0,
    },
  ],
  warnings: ['不应显示的技术细节'],
  suggested_version: 'v1.2',
  version_source: 'file_name',
  elapsed_ms: 5,
});
const job: JobPage = makeJob({
  id: 'scan',
  root: config.game_root,
  phase: '分析',
  total: 1,
  processed: 1,
  elapsed_ms: 5,
  changes: [candidate],
  next_cursor: 1,
  change_count: 1,
});
const game: Game = makeGame({
  canonical_title: '已登记游戏',
  display_title: '已登记游戏',
  install_path: 'E:/Butter/已登记游戏',
  current_version: 'Final',
  main_executable: '任意.exe',
});

async function openDetail() {
  render(<App />);
  fireEvent.click(await screen.findByRole('button', { name: game.display_title }));
  return screen.getByRole('dialog', { name: game.display_title });
}

it.each(['list', 'grid'] as const)(
  'blocks %s and detail launches while a task is active',
  async (layout) => {
    vi.mocked(api.job).mockResolvedValue({ ...job, status: 'running' });
    vi.mocked(api.play).mockResolvedValue(game);
    render(<App />);
    await waitForEnabledButton('扫描目录');
    fireEvent.click(screen.getByRole('button', { name: '扫描目录' }));
    await waitFor(() => expect(api.job).toHaveBeenCalledWith('scan', 0));
    goLibrary();
    if (layout === 'grid') fireEvent.click(screen.getByRole('button', { name: '卡片视图' }));
    expect(
      (screen.getByRole('button', { name: `启动 ${game.display_title}` }) as HTMLButtonElement)
        .disabled,
    ).toBe(true);
    fireEvent.click(screen.getByRole('button', { name: game.display_title }));
    const modal = screen.getByRole('dialog', { name: game.display_title });
    const launch = within(modal).getByRole('button', { name: '启动' });
    expect((launch as HTMLButtonElement).disabled).toBe(true);
    fireEvent.click(launch);
    expect(api.play).not.toHaveBeenCalled();
    vi.mocked(api.job).mockResolvedValue({ ...job, changes: [], next_cursor: 1 });
    await waitFor(() => expect((launch as HTMLButtonElement).disabled).toBe(false));
    fireEvent.click(launch);
    await waitFor(() => expect(api.play).toHaveBeenCalledOnce());
  },
);
function showDetailTab(name: string) {
  fireEvent.click(screen.getByRole('tab', { name }));
}

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
  resetApiMock(api);
  nativeWindow.destroy.mockResolvedValue(undefined);
  nativeWindow.onCloseRequested.mockResolvedValue(vi.fn());
  vi.mocked(api.saveSettings).mockReset();
  vi.mocked(api.saveGame).mockReset();
  vi.mocked(api.openFolder).mockResolvedValue();
  vi.mocked(api.importRecoveryIssues).mockResolvedValue([]);
  vi.mocked(api.startLibraryCheck).mockResolvedValue('');
  vi.mocked(api.duplicateReviews).mockResolvedValue([]);
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
  await waitForEnabledButton('扫描目录');
  fireEvent.click(screen.getByRole('button', { name: '扫描目录' }));
  openScan();
  await screen.findByText('启动文件：main-v1.2.exe');
  await waitForEnabledButton('加入游戏库');
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
      name: '游戏整理',
    }),
  );
  fireEvent.click(screen.getByRole('tab', { name: '目录扫描' }));
}
function openConfig(section: 'MTool' | '设置') {
  fireEvent.click(screen.getByRole('button', { name: section }));
}
describe('iteration interactions', () => {
  function systemClose() {
    const event = { preventDefault: vi.fn() };
    act(() => nativeWindow.onCloseRequested.mock.calls.at(-1)![0](event));
    expect(event.preventDefault).toHaveBeenCalledOnce();
  }
  it.each(['titlebar', 'system'])(
    'closes an unchanged window through %s and unregisters its listener',
    async (entry) => {
      const stop = vi.fn();
      nativeWindow.onCloseRequested.mockResolvedValue(stop);
      const { unmount } = render(<App />);
      await screen.findByRole('button', { name: game.display_title });
      if (entry === 'system') systemClose();
      else fireEvent.click(screen.getByRole('button', { name: '关闭窗口' }));
      await waitFor(() => expect(nativeWindow.destroy).toHaveBeenCalledOnce());
      expect(screen.queryByRole('dialog')).toBeNull();
      unmount();
      expect(stop).toHaveBeenCalledOnce();
    },
  );
  it.each(['settings', 'detail'])(
    'guards both close entry points and preserves unsaved %s on cancel',
    async (page) => {
      render(<App />);
      await screen.findByRole('button', { name: game.display_title });
      if (page === 'settings') openConfig('设置');
      else fireEvent.click(screen.getByRole('button', { name: game.display_title }));
      const input = screen.getByRole('textbox', {
        name: page === 'settings' ? /Game Root/ : '版本',
      });
      fireEvent.change(input, { target: { value: 'Unsaved' } });
      fireEvent.click(screen.getByRole('button', { name: '关闭窗口' }));
      const popup = screen.getByRole('dialog', { name: '关闭前保存修改？' });
      systemClose();
      expect(screen.getAllByRole('dialog', { name: '关闭前保存修改？' })).toHaveLength(1);
      fireEvent.click(within(popup).getByRole('button', { name: '取消关闭' }));
      expect((input as HTMLInputElement).value).toBe('Unsaved');
      expect(nativeWindow.destroy).not.toHaveBeenCalled();
      systemClose();
      fireEvent.click(
        within(screen.getByRole('dialog', { name: '关闭前保存修改？' })).getByRole('button', {
          name: '放弃并关闭',
        }),
      );
      await waitFor(() => expect(nativeWindow.destroy).toHaveBeenCalledOnce());
      expect(api.saveSettings).not.toHaveBeenCalled();
      expect(api.saveGame).not.toHaveBeenCalled();
    },
  );
  it.each(['settings', 'detail'])(
    'saves dirty %s before closing and retains the draft on save failure',
    async (page) => {
      vi.mocked(api.saveSettings)
        .mockRejectedValueOnce(new Error('fixture save failed'))
        .mockImplementation(async (value) => value);
      vi.mocked(api.saveGame)
        .mockRejectedValueOnce(new Error('fixture save failed'))
        .mockImplementation(async (value) => ({ ...game, ...value }));
      render(<App />);
      await screen.findByRole('button', { name: game.display_title });
      if (page === 'settings') openConfig('设置');
      else fireEvent.click(screen.getByRole('button', { name: game.display_title }));
      const input = screen.getByRole('textbox', {
        name: page === 'settings' ? /Game Root/ : '版本',
      });
      fireEvent.change(input, {
        target: { value: page === 'settings' ? 'D:/Edited' : 'Edited v2' },
      });
      systemClose();
      const popup = screen.getByRole('dialog', { name: '关闭前保存修改？' });
      fireEvent.click(within(popup).getByRole('button', { name: '保存并关闭' }));
      await within(popup).findByRole('alert');
      expect(nativeWindow.destroy).not.toHaveBeenCalled();
      expect((input as HTMLInputElement).value).toBe(
        page === 'settings' ? 'D:/Edited' : 'Edited v2',
      );
      fireEvent.click(within(popup).getByRole('button', { name: '保存并关闭' }));
      await waitFor(() => expect(nativeWindow.destroy).toHaveBeenCalledOnce());
      const save = page === 'settings' ? api.saveSettings : api.saveGame;
      expect(save).toHaveBeenCalledTimes(2);
      expect(save).toHaveBeenLastCalledWith(
        expect.objectContaining(
          page === 'settings' ? { game_root: 'D:/Edited' } : { current_version: 'Edited v2' },
        ),
      );
    },
  );
  it('blocks titlebar and system close during a running task, then permits normal close', async () => {
    let complete!: (id: string) => void;
    vi.mocked(api.startScan).mockImplementationOnce(
      () =>
        new Promise((resolve) => {
          complete = resolve;
        }),
    );
    render(<App />);
    await screen.findByRole('button', { name: game.display_title });
    fireEvent.click(screen.getByRole('button', { name: '扫描目录' }));
    fireEvent.click(screen.getByRole('button', { name: '关闭窗口' }));
    systemClose();
    expect(nativeWindow.destroy).not.toHaveBeenCalled();
    expect(api.cancel).not.toHaveBeenCalled();
    await screen.findByText('当前任务或保存操作尚未完成，请稍后关闭窗口。');
    vi.mocked(api.job).mockResolvedValue({ ...job, status: 'running' });
    act(() => complete('scan'));
    await waitFor(() => expect(api.job).toHaveBeenCalledWith('scan', 0));
    fireEvent.click(screen.getByRole('button', { name: '关闭窗口' }));
    systemClose();
    expect(nativeWindow.destroy).not.toHaveBeenCalled();
    expect(api.cancel).not.toHaveBeenCalled();
    vi.mocked(api.job).mockResolvedValue(job);
    openScan();
    await screen.findByText('启动文件：main-v1.2.exe');
    await waitForEnabledButton('加入游戏库');
    expect(api.cancel).not.toHaveBeenCalled();
    systemClose();
    await waitFor(() => expect(nativeWindow.destroy).toHaveBeenCalledOnce());
  });
  it('orders organizing after import and defaults to duplicates without starting either task', async () => {
    render(<App />);
    await screen.findByRole('button', { name: game.display_title });
    const navigation = within(screen.getByRole('navigation', { name: '主导航' }));
    expect(navigation.getAllByRole('button').map((button) => button.textContent)).toEqual([
      '游戏库1',
      '导入',
      '游戏整理',
      'MTool',
    ]);
    fireEvent.click(navigation.getByRole('button', { name: '游戏整理' }));
    expect(screen.getByRole('tab', { name: '重复检查' }).getAttribute('aria-selected')).toBe(
      'true',
    );
    expect(api.startScan).not.toHaveBeenCalled();
    expect(api.startDuplicateCheck).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole('tab', { name: '目录扫描' }));
    fireEvent.change(screen.getByRole('textbox', { name: '搜索扫描结果' }), {
      target: { value: 'retained' },
    });
    fireEvent.click(screen.getByRole('tab', { name: '重复检查' }));
    fireEvent.click(screen.getByRole('tab', { name: '目录扫描' }));
    expect((screen.getByRole('textbox', { name: '搜索扫描结果' }) as HTMLInputElement).value).toBe(
      'retained',
    );
  });
  it('defaults an empty library to directory scanning without starting a task', async () => {
    vi.mocked(api.games).mockResolvedValue([]);
    render(<App />);
    await screen.findByText('从你的游戏目录开始');
    fireEvent.click(screen.getByRole('button', { name: '游戏整理' }));
    expect(screen.getByRole('tab', { name: '目录扫描' }).getAttribute('aria-selected')).toBe(
      'true',
    );
    expect(api.startScan).not.toHaveBeenCalled();
    expect(api.startDuplicateCheck).not.toHaveBeenCalled();
  });
  it('opens the same save editor from the hero action and the retained save tab entry', async () => {
    const detail = await openDetail();
    const actions = detail.querySelector('.detail-actions') as HTMLElement;
    expect(
      within(actions)
        .getAllByRole('button')
        .map((button) => button.textContent),
    ).toEqual(['启动', '打开目录', '编辑存档', '分析启动配置']);
    fireEvent.click(within(actions).getByRole('button', { name: '编辑存档' }));
    const editor = await screen.findByRole('dialog', { name: `编辑存档 · ${game.display_title}` });
    await waitFor(() => expect(api.listEditableSaves).toHaveBeenCalledWith(game.id));
    fireEvent.click(within(editor).getByRole('button', { name: '关闭弹窗' }));
    expect(screen.queryByRole('dialog', { name: `编辑存档 · ${game.display_title}` })).toBeNull();
    showDetailTab('存档位置');
    const panel = within(detail).getByRole('tabpanel', { name: '存档位置' });
    fireEvent.click(within(panel).getByRole('button', { name: '编辑存档' }));
    expect(
      await screen.findByRole('dialog', { name: `编辑存档 · ${game.display_title}` }),
    ).toBeTruthy();
    await waitFor(() => expect(api.listEditableSaves).toHaveBeenCalledTimes(2));
    expect(api.saveGame).not.toHaveBeenCalled();
  });
  it('preserves drafts across detail tabs, submits all changes together and supports keyboard navigation', async () => {
    vi.mocked(api.saveGame).mockImplementation(async (edit) => ({ ...game, ...edit }));
    const detail = await openDetail();
    expect(
      within(detail)
        .getAllByRole('tabpanel')
        .map((panel) => panel.getAttribute('data-detail-tab')),
    ).toEqual(['basic']);
    fireEvent.change(within(detail).getByRole('textbox', { name: '显示名称' }), {
      target: { value: '修改标题' },
    });
    fireEvent.click(within(detail).getByRole('radio', { name: '已通关' }));
    showDetailTab('启动配置');
    fireEvent.change(within(detail).getByRole('combobox', { name: /启动文件（/ }), {
      target: { value: 'new.exe' },
    });
    showDetailTab('存档位置');
    fireEvent.change(within(detail).getByRole('textbox', { name: /存档位置/ }), {
      target: { value: '<GAME>/save' },
    });
    showDetailTab('别名管理');
    fireEvent.change(within(detail).getByRole('textbox', { name: /别名（/ }), {
      target: { value: '另一个名称' },
    });
    fireEvent.keyDown(within(detail).getByRole('tab', { name: '别名管理' }), { key: 'ArrowRight' });
    expect(
      within(detail).getByRole('tab', { name: '记录与管理' }).getAttribute('aria-selected'),
    ).toBe('true');
    expect(within(detail).queryByRole('textbox', { name: '显示名称' })).toBeNull();
    fireEvent.keyDown(within(detail).getByRole('tab', { name: '记录与管理' }), { key: 'Home' });
    expect(
      (within(detail).getByRole('textbox', { name: '显示名称' }) as HTMLInputElement).value,
    ).toBe('修改标题');
    expect(api.saveGame).not.toHaveBeenCalled();
    fireEvent.click(within(detail).getByRole('button', { name: '保存资料' }));
    await waitFor(() =>
      expect(api.saveGame).toHaveBeenCalledWith(
        expect.objectContaining({
          display_title: '修改标题',
          play_status: 'COMPLETED',
          main_executable: 'new.exe',
          save_paths: ['<GAME>/save'],
          aliases: ['另一个名称'],
        }),
      ),
    );
    await screen.findByText('游戏资料已保存。');
    expect(
      (within(detail).getByRole('radio', { name: '已通关' }) as HTMLInputElement).checked,
    ).toBe(true);
    showDetailTab('存档位置');
    expect(
      (within(detail).getByRole('textbox', { name: /存档位置/ }) as HTMLTextAreaElement).value,
    ).toBe('<GAME>/save');
    expect(api.play).not.toHaveBeenCalled();
  });
  it('applies Other and engine capsules with existing search and status filters', async () => {
    const games = ['Unity', 'Unity', "Ren'Py", 'HTML', 'Godot', 'QSP', 'KiriKiri', 'Unknown'].map(
      (engine, index) => ({ ...game, id: String(index), display_title: `Title ${index}`, engine }),
    );
    vi.mocked(api.games).mockResolvedValue(games);
    render(<App />);
    await screen.findByRole('button', { name: 'Title 0' });
    const quick = within(screen.getByRole('group', { name: '引擎快捷筛选' }));
    const popular = engineQuickGroups(games).popular.map((group) => group.engine);
    fireEvent.click(quick.getByRole('button', { name: '其他' }));
    for (const entry of games)
      expect(!!screen.queryByRole('button', { name: entry.display_title })).toBe(
        !popular.includes(entry.engine),
      );
    expect(screen.getByText('找到 2 / 8 个游戏')).toBeTruthy();
    fireEvent.change(screen.getByRole('textbox', { name: '搜索游戏或别名' }), {
      target: { value: 'Title 7' },
    });
    expect(screen.getByText('找到 1 / 8 个游戏')).toBeTruthy();
    expect(quick.getByRole('button', { name: 'Unity' })).toBeTruthy();
    fireEvent.change(screen.getByRole('textbox', { name: '搜索游戏或别名' }), {
      target: { value: '' },
    });
    const all = within(screen.getByRole('group', { name: '快捷筛选' })).getByRole('button', {
      name: '全部',
    });
    expect(all.getAttribute('aria-pressed')).toBe('false');
    fireEvent.click(all);
    expect(quick.getByRole('button', { name: '其他' }).getAttribute('aria-pressed')).toBe('false');
    expect(screen.getByRole('button', { name: 'Title 7' })).toBeTruthy();
    expect(screen.getByRole('button', { name: 'Title 0' })).toBeTruthy();
    fireEvent.click(quick.getByRole('button', { name: 'Unity' }));
    expect(screen.getByRole('button', { name: 'Title 0' })).toBeTruthy();
    expect(screen.queryByRole('button', { name: 'Title 7' })).toBeNull();
    fireEvent.click(screen.getByRole('button', { name: '筛选 (1)' }));
    const filters = within(screen.getByRole('dialog', { name: '筛选游戏' }));
    fireEvent.click(filters.getByRole('checkbox', { name: '正在玩' }));
    fireEvent.click(filters.getByRole('button', { name: '完成' }));
    fireEvent.click(quick.getByRole('button', { name: 'Unity' }));
    expect(screen.queryByRole('button', { name: 'Title 0' })).toBeNull();
    expect(screen.getByRole('button', { name: '筛选 (1)' })).toBeTruthy();
  });
  it('combines play status shortcuts with engines and keeps the filter dialog in sync', async () => {
    vi.mocked(api.games).mockResolvedValue([
      {
        ...game,
        id: 'unplayed',
        display_title: 'Unplayed Unity',
        engine: 'Unity',
        play_status: 'UNPLAYED',
      },
      {
        ...game,
        id: 'playing',
        display_title: 'Playing Unity',
        engine: 'Unity',
        play_status: 'PLAYING',
      },
      {
        ...game,
        id: 'done',
        display_title: 'Completed Godot',
        engine: 'Godot',
        play_status: 'COMPLETED',
      },
    ]);
    render(<App />);
    await screen.findByRole('button', { name: 'Unplayed Unity' });
    const statuses = within(screen.getByRole('group', { name: '游玩状态快捷筛选' }));
    const engines = within(screen.getByRole('group', { name: '引擎快捷筛选' }));
    const all = within(screen.getByRole('group', { name: '快捷筛选' })).getByRole('button', {
      name: '全部',
    });
    expect(all.getAttribute('aria-pressed')).toBe('true');
    fireEvent.click(statuses.getByRole('button', { name: '正在玩' }));
    expect(all.getAttribute('aria-pressed')).toBe('false');
    expect(screen.getByRole('button', { name: 'Playing Unity' })).toBeTruthy();
    expect(screen.queryByRole('button', { name: 'Unplayed Unity' })).toBeNull();
    fireEvent.click(engines.getByRole('button', { name: 'Godot' }));
    expect(screen.queryByRole('button', { name: 'Playing Unity' })).toBeNull();
    fireEvent.click(statuses.getByRole('button', { name: '已通关' }));
    expect(screen.getByRole('button', { name: 'Completed Godot' })).toBeTruthy();
    fireEvent.click(screen.getByRole('button', { name: '筛选 (2)' }));
    const dialog = within(screen.getByRole('dialog', { name: '筛选游戏' }));
    expect((dialog.getByRole('checkbox', { name: '已通关' }) as HTMLInputElement).checked).toBe(
      true,
    );
    expect((dialog.getByRole('checkbox', { name: 'Godot' }) as HTMLInputElement).checked).toBe(
      true,
    );
    expect(screen.queryByRole('button', { name: 'Playing Unity' })).toBeNull();
    expect(screen.queryByRole('button', { name: 'Unplayed Unity' })).toBeNull();
    expect(screen.getByText('找到 1 / 3 个游戏')).toBeTruthy();
    fireEvent.click(dialog.getByRole('checkbox', { name: '已通关' }));
    fireEvent.click(dialog.getByRole('checkbox', { name: '未启动' }));
    fireEvent.click(dialog.getByRole('button', { name: '完成' }));
    expect(statuses.getByRole('button', { name: '未启动' }).getAttribute('aria-pressed')).toBe(
      'true',
    );
    fireEvent.click(all);
    expect(all.getAttribute('aria-pressed')).toBe('true');
    expect(engines.getByRole('button', { name: 'Godot' }).getAttribute('aria-pressed')).toBe(
      'false',
    );
    expect(screen.getByRole('button', { name: 'Completed Godot' })).toBeTruthy();
    expect(screen.getByRole('button', { name: 'Playing Unity' })).toBeTruthy();
    expect(screen.getByRole('button', { name: 'Unplayed Unity' })).toBeTruthy();
    fireEvent.click(engines.getByRole('button', { name: 'Godot' }));
    expect(all.getAttribute('aria-pressed')).toBe('false');
    fireEvent.click(all);
    expect(screen.getByRole('button', { name: 'Playing Unity' })).toBeTruthy();
    expect(all.getAttribute('aria-pressed')).toBe('true');
  });
  it('returns to a hidden invalid field without discarding edits in another tab', async () => {
    const detail = await openDetail();
    const title = within(detail).getByRole('textbox', { name: '显示名称' });
    fireEvent.change(title, { target: { value: '' } });
    showDetailTab('别名管理');
    fireEvent.change(within(detail).getByRole('textbox', { name: /别名（/ }), {
      target: { value: '保留的别名' },
    });
    fireEvent.invalid(title);
    expect(
      within(detail).getByRole('tab', { name: '基础信息' }).getAttribute('aria-selected'),
    ).toBe('true');
    await waitFor(() => expect(document.activeElement).toBe(title));
    showDetailTab('别名管理');
    expect(
      (within(detail).getByRole('textbox', { name: /别名（/ }) as HTMLTextAreaElement).value,
    ).toBe('保留的别名');
    fireEvent.click(within(detail).getByRole('button', { name: '关闭弹窗' }));
    expect(screen.getByRole('dialog', { name: '放弃未保存修改？' })).toBeTruthy();
    expect(api.saveGame).not.toHaveBeenCalled();
  });
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
      await waitForEnabledButton('扫描目录');
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
    showDetailTab('存档位置');
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
    await openDetail();
    showDetailTab('启动配置');
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
    await openDetail();
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
    showDetailTab('基础信息');
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
    await waitForEnabledButton('扫描目录');
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
    expect(within(settingsPage).queryByRole('region', { name: '外观设置' })).toBeNull();
    expect(sidebar.querySelector('.brand img')?.getAttribute('src')).toContain('app-icon-new');
    expect(sidebar.querySelector('.sidebar-art img')?.getAttribute('src')).toContain(
      'character-new',
    );
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
    expect(detail.querySelector('.engine-badge')!.classList.contains('tone-qsp')).toBe(true);
    showDetailTab('存档位置');
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
    const detail = await openDetail();
    showDetailTab('存档位置');
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
    const detail = await openDetail();
    showDetailTab('存档位置');
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
  it('integrates bulk path relocation in advanced settings and refreshes the existing library', async () => {
    const migrated = { ...game, install_path: 'D:/Games/已登记游戏' };
    const savedSettings = { ...config, game_root: 'D:/Games' };
    vi.mocked(api.chooseDirectory).mockResolvedValue('D:/Games');
    vi.mocked(api.previewLibraryRelocation).mockResolvedValue({
      token: 'bulk-token',
      old_root: config.game_root,
      new_root: 'D:/Games',
      items: [
        {
          id: game.id,
          title: game.display_title,
          old_path: game.install_path,
          new_path: migrated.install_path,
          reason: null,
        },
      ],
      mtool: {
        old_path: '',
        new_path: null,
        state: 'unchanged',
        reason: '公共 MTool 未配置，设置保持原样',
      },
    });
    vi.mocked(api.applyLibraryRelocation).mockResolvedValue({
      migrated: 1,
      skipped: 0,
      mtool_migrated: false,
      settings: savedSettings,
      games: [migrated],
    });
    render(<App />);
    await screen.findByRole('button', { name: game.display_title });
    openConfig('设置');
    const page = screen.getByRole('region', { name: '设置页面' });
    const advanced = page.querySelector('.settings-maintenance') as HTMLDetailsElement;
    expect(advanced.open).toBe(false);
    fireEvent.click(advanced.querySelector('summary')!);
    const editor = within(page).getByRole('region', { name: '批量迁移游戏库路径' });
    expect(
      (within(editor).getByRole('textbox', { name: '旧游戏库根目录' }) as HTMLInputElement).value,
    ).toBe(config.game_root);
    fireEvent.click(within(editor).getByRole('button', { name: '选择新根目录…' }));
    await waitForEnabledButton('预览迁移', within(editor));
    fireEvent.click(within(editor).getByRole('button', { name: '预览迁移' }));
    await within(editor).findByText('可迁移 1 个游戏，无法迁移 0 个游戏。');
    vi.mocked(api.games).mockResolvedValue([migrated]);
    fireEvent.click(within(editor).getByRole('button', { name: '确认迁移' }));
    const modal = screen.getByRole('dialog', { name: '确认迁移游戏库路径？' });
    fireEvent.click(within(modal).getByRole('button', { name: '确认迁移' }));
    await within(editor).findByText('已迁移 1 个游戏，跳过 0 个。公共 MTool 设置未变。');
    await waitFor(() =>
      expect(
        (within(page).getByRole('textbox', { name: /Game Root/ }) as HTMLInputElement).value,
      ).toBe('D:/Games'),
    );
    goLibrary();
    const search = screen.getByRole('textbox', { name: '搜索游戏或别名' });
    fireEvent.change(search, { target: { value: migrated.install_path } });
    expect(screen.getByRole('button', { name: game.display_title })).toBeTruthy();
    expect(api.startLibraryCheck).toHaveBeenCalled();
    expect(api.startScan).not.toHaveBeenCalled();
    expect(api.register).not.toHaveBeenCalled();
  });
  it('shows AppData cleanup only for RenPy and disables it without a valid directory', async () => {
    const detail = await openDetail();
    showDetailTab('存档位置');
    expect(within(detail).queryByRole('button', { name: '清理 AppData 存档' })).toBeNull();
    expect(api.previewRenpyAppdata).not.toHaveBeenCalled();
    fireEvent.click(within(detail).getByRole('button', { name: '关闭弹窗' }));
  });
  it('disables RenPy AppData cleanup when static detection finds no valid directory', async () => {
    vi.mocked(api.games).mockResolvedValue([{ ...game, engine: "Ren'Py" }]);
    const detail = await openDetail();
    showDetailTab('存档位置');
    await within(detail).findByText('对应 AppData 存档目录不存在');
    expect(
      (within(detail).getByRole('button', { name: '清理 AppData 存档' }) as HTMLButtonElement)
        .disabled,
    ).toBe(true);
    expect(api.recycleRenpyAppdata).not.toHaveBeenCalled();
  });
  it.each(['success', 'failure'] as const)(
    'cleans only the confirmed AppData directory: %s',
    async (outcome) => {
      vi.mocked(api.games).mockResolvedValue([{ ...game, engine: "Ren'Py" }]);
      vi.mocked(api.previewRenpyAppdata).mockResolvedValue({
        token: 'appdata-token',
        path: 'C:/Fixture/RenPy/彼女-1234',
        reason: null,
      });
      const detail = await openDetail();
      showDetailTab('存档位置');
      const paths = within(detail).getByRole('textbox', {
        name: /存档位置/,
      }) as HTMLTextAreaElement;
      fireEvent.change(paths, {
        target: { value: '<GAME>/game/saves\n%APPDATA%/RenPy/彼女-1234' },
      });
      await waitForEnabledButton('清理 AppData 存档', within(detail));
      const open = within(detail).getByRole('button', { name: '清理 AppData 存档' });
      const saveActions = open.closest('.save-location-actions');
      expect(saveActions).not.toBeNull();
      expect(
        within(saveActions as HTMLElement).getByRole('button', { name: '编辑存档' }),
      ).toBeTruthy();
      fireEvent.click(open);
      const cancelDialog = await screen.findByRole('dialog', { name: '清理 AppData 存档？' });
      expect(within(cancelDialog).getByText('C:/Fixture/RenPy/彼女-1234')).toBeTruthy();
      expect(within(cancelDialog).getByText('游戏目录中的存档不受影响。')).toBeTruthy();
      fireEvent.click(within(cancelDialog).getByRole('button', { name: '取消' }));
      expect(api.recycleRenpyAppdata).not.toHaveBeenCalled();
      await waitForEnabledButton('清理 AppData 存档', within(detail));
      fireEvent.click(open);
      const confirm = await screen.findByRole('dialog', { name: '清理 AppData 存档？' });
      if (outcome === 'success') {
        vi.mocked(api.recycleRenpyAppdata).mockResolvedValueOnce(undefined);
        vi.mocked(api.previewRenpyAppdata).mockResolvedValueOnce({
          token: null,
          path: null,
          reason: '对应 AppData 存档目录不存在',
        });
      } else {
        vi.mocked(api.recycleRenpyAppdata).mockRejectedValueOnce('回收失败：目录被占用');
      }
      fireEvent.click(within(confirm).getByRole('button', { name: '移入回收站' }));
      await waitFor(() =>
        expect(api.recycleRenpyAppdata).toHaveBeenCalledExactlyOnceWith(game.id, 'appdata-token'),
      );
      if (outcome === 'success') {
        await within(detail).findByText('AppData 存档已移入回收站。');
        await waitFor(() => expect((open as HTMLButtonElement).disabled).toBe(true));
      } else {
        await within(detail).findByRole('alert');
        expect(within(detail).getByText('回收失败：目录被占用')).toBeTruthy();
        expect(within(detail).queryByText('AppData 存档已移入回收站。')).toBeNull();
        await waitForEnabledButton('清理 AppData 存档', within(detail));
      }
      expect(paths.value).toBe('<GAME>/game/saves\n%APPDATA%/RenPy/彼女-1234');
      expect(api.saveGame).not.toHaveBeenCalled();
      expect(api.deleteFiles).not.toHaveBeenCalled();
    },
  );
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
    const save = screen.getByRole('button', { name: '保存设置' }) as HTMLButtonElement;
    expect(save.disabled).toBe(true);
    expect(save.closest('.page-heading')).toBeTruthy();
    expect(save.form).toBe(select.closest('form'));
    expect((select as HTMLSelectElement).value).toBe('2');
    fireEvent.change(select, { target: { value: '4' } });
    expect(save.disabled).toBe(false);
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
    showDetailTab('启动配置');
    fireEvent.click(within(modal).getByRole('button', { name: '浏览启动文件…' }));
    await waitFor(() =>
      expect(
        (within(modal).getByRole('combobox', { name: /启动文件（/ }) as HTMLInputElement).value,
      ).toBe('包装/游戏 & test.html'),
    );
    showDetailTab('基础信息');
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
        'translateY(78px)',
      ),
    );
    expect(screen.queryByRole('navigation', { name: /游戏库.*分页/ })).toBeNull();
    expect(screen.queryByRole('button', { name: 'Game 120' })).toBeNull();
    const rows = () => within(screen.getByRole('table')).getAllByRole('row');
    expect(rows().length).toBeLessThan(30);
    fireEvent.scroll(screen.getByLabelText('游戏列表'), { target: { scrollTop: 9003 } });
    fireEvent.click(await screen.findByRole('button', { name: 'Game 120' }));
    expect(screen.getByRole('dialog', { name: 'Game 120' })).toBeTruthy();
    fireEvent.click(within(screen.getByRole('dialog')).getByRole('button', { name: '关闭弹窗' }));
    expect(rows().length).toBeLessThan(30);
    fireEvent.click(screen.getByRole('button', { name: '扫描目录' }));
    openScan();
    await screen.findByText('启动文件：main-v1.2.exe');
    goLibrary();
    await screen.findByRole('button', { name: 'Game 120' });
    expect(screen.getByLabelText('游戏列表').scrollTop).toBe(9003);
    fireEvent.scroll(screen.getByLabelText('游戏列表'), { target: { scrollTop: 42678 } });
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
    await waitForEnabledButton('扫描目录');
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
    const maintenance = modal.querySelector<HTMLDetailsElement>('.settings-maintenance')!;
    expect(maintenance.open).toBe(false);
    fireEvent.click(maintenance.querySelector('summary')!);
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
  it('shows saved times and launch history without reloading the library', async () => {
    const initial = {
      ...game,
      created_at: '2020-01-01T00:00:00.000Z',
      last_launched_at: '2026-01-01T00:00:00.000Z',
      current_version: 'Unknown',
      version_source: 'unknown',
    };
    vi.mocked(api.games).mockResolvedValue([initial]);
    vi.mocked(api.launchHistory).mockResolvedValue([initial.last_launched_at]);
    const modal = await openDetail();
    expect(within(modal).getByText(/入库时间：/)).toBeTruthy();
    expect(within(modal).getByText(/上次运行：/)).toBeTruthy();
    showDetailTab('记录与管理');
    fireEvent.click(within(modal).getByText('运行历史（最近 100 次启动请求）'));
    await waitFor(() => expect(within(modal).getAllByRole('listitem').length).toBe(1));
    fireEvent.click(within(modal).getByRole('button', { name: '关闭弹窗' }));
    expect(screen.queryByRole('button', { name: '补充引擎/版本' })).toBeNull();
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
    await waitForEnabledButton('设置');
    fireEvent.click(screen.getByRole('button', { name: '设置' }));
    const modal = screen.getByRole('region', { name: '设置页面' });
    vi.mocked(api.chooseDirectory).mockResolvedValue('D:/Games');
    fireEvent.click(within(modal).getAllByRole('button', { name: '浏览目录…' })[0]);
    await waitFor(() =>
      expect(
        (within(modal).getByRole('textbox', { name: /Game Root/ }) as HTMLInputElement).value,
      ).toBe('D:/Games'),
    );
    // The picker publishes its path before clearing busy; navigation is guarded until it finishes.
    await waitForEnabledButton('保存设置', within(modal));
    goLibrary();
    const confirmation = await screen.findByRole('dialog', { name: '放弃未保存修改？' });
    expect(screen.getByRole('region', { name: '设置页面' })).toBe(modal);
    fireEvent.click(within(confirmation).getByRole('button', { name: '继续编辑' }));
    expect(
      (within(modal).getByRole('textbox', { name: /Game Root/ }) as HTMLInputElement).value,
    ).toBe('D:/Games');
    expect(api.saveSettings).not.toHaveBeenCalled();
    fireEvent.click(
      within(screen.getByRole('navigation', { name: '主导航' })).getByRole('button', {
        name: /游戏库/,
      }),
    );
    fireEvent.click(
      within(await screen.findByRole('dialog', { name: '放弃未保存修改？' })).getByRole('button', {
        name: '放弃并切换',
      }),
    );
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
      expect(play.textContent).toBe('启动');
      expect(play.getAttribute('data-launch-pending')).toBe('true');
      fireEvent.click(play);
      expect(api.play).toHaveBeenCalledTimes(1);
      await waitFor(() => expect(play.textContent).toBe('启动中'));
      expect(play.getAttribute('data-launch-pending')).toBe('false');
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
    expect(within(play.closest('tr')!).getByText('未启动')).toBeTruthy();
    expect(screen.queryByRole('dialog')).toBeNull();
  });
  it.each(['library', 'detail'] as const)(
    'keeps a fast %s launch visually stable and cancels its delayed progress indicator',
    async (surface) => {
      vi.mocked(api.play).mockResolvedValue({ ...game, play_status: 'PLAYING' });
      let play: HTMLElement;
      if (surface === 'detail') {
        const modal = await openDetail();
        play = within(modal).getByRole('button', { name: '启动' });
      } else {
        render(<App />);
        play = await screen.findByRole('button', { name: `启动 ${game.display_title}` });
      }
      vi.useFakeTimers();
      try {
        fireEvent.click(play);
        expect(play.textContent).toBe('启动');
        expect((play as HTMLButtonElement).disabled).toBe(true);
        expect(play.getAttribute('data-launch-pending')).toBe('true');
        fireEvent.click(play);
        expect(api.play).toHaveBeenCalledTimes(1);
        await act(async () => {});
        expect((play as HTMLButtonElement).disabled).toBe(false);
        await act(async () => {
          await vi.advanceTimersByTimeAsync(500);
        });
        expect(play.textContent).toBe('启动');
        expect(play.getAttribute('data-launch-pending')).toBe('false');
        expect(api.play).toHaveBeenCalledTimes(1);
        if (surface === 'detail') {
          expect(within(screen.getByRole('dialog')).getByRole('button', { name: '启动' })).toBe(
            play,
          );
          expect(api.saveGame).not.toHaveBeenCalled();
        }
      } finally {
        vi.useRealTimers();
      }
    },
  );
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
    const modal = await openDetail();
    const list = screen.getByRole('table');
    fireEvent.click(within(modal).getByRole('button', { name: '启动' }));
    const launching = within(modal).getByRole('button', { name: '启动' });
    expect(launching.getAttribute('data-launch-pending')).toBe('true');
    expect((launching as HTMLButtonElement).disabled).toBe(true);
    fireEvent.click(launching);
    expect(api.play).toHaveBeenCalledTimes(1);
    expect(screen.getByRole('dialog')).toBe(modal);
    expect(screen.getByRole('table')).toBe(list);
    expect(screen.queryByText('正在处理…')).toBeNull();
    await waitFor(() => expect(launching.textContent).toBe('启动中…'));
    fireEvent.click(within(modal).getByRole('button', { name: '关闭弹窗' }));
    expect(screen.queryByRole('dialog')).toBeNull();
    fireEvent.click(screen.getByRole('button', { name: game.display_title }));
    const reopened = screen.getByRole('dialog');
    expect(
      (within(reopened).getByRole('button', { name: '启动中…' }) as HTMLButtonElement).disabled,
    ).toBe(true);
    complete();
    await waitForEnabledButton('启动', within(reopened));
    await waitFor(() => {
      expect(
        (within(reopened).getByRole('radio', { name: '正在玩' }) as HTMLInputElement).checked,
      ).toBe(true);
      expect(
        (within(reopened).getByRole('button', { name: '保存资料' }) as HTMLButtonElement).disabled,
      ).toBe(true);
    });
    expect(api.saveGame).not.toHaveBeenCalled();
    expect(screen.getByRole('dialog')).toBe(reopened);
    expect(api.games).toHaveBeenCalledTimes(1);
  });

  it.each(['DIRECT', 'MTOOL', 'EXTERNAL_PLAYER'] as const)(
    'launches unsaved %s configuration without saving, then saves through the fixed hero toolbar',
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
      showDetailTab('启动配置');
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
      expect(save.closest('.detail-toolbar')).toBeTruthy();
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
      showDetailTab('基础信息');
      expect(
        (within(detail).getByRole('textbox', { name: '显示名称' }) as HTMLInputElement).value,
      ).toBe('未保存标题');
      await waitFor(() =>
        expect(
          (within(detail).getByRole('radio', { name: '正在玩' }) as HTMLInputElement).checked,
        ).toBe(true),
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
    const modal = await openDetail();
    fireEvent.click(within(modal).getByRole('button', { name: '分析启动配置' }));
    fireEvent.click(await within(modal).findByRole('button', { name: '取消任务' }));
    await waitFor(() => expect(api.cancel).toHaveBeenCalledWith('detail-analysis'));
    response = { ...response, status: 'cancelled', active: {} };
    await within(modal).findByText('分析已取消，原有结果保留。');
    await waitForEnabledButton('分析启动配置', within(modal));
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
    showDetailTab('基础信息');
    fireEvent.change(within(modal).getByRole('combobox', { name: '游戏引擎' }), {
      target: { value: 'Unity' },
    });
    response.changes = [{ ...candidate, registered_id: game.id, engine: 'WOLF RPG Editor' }];
    fireEvent.click(within(modal).getByRole('button', { name: '分析启动配置' }));
    await within(modal).findByText('已填入分析启动配置，请检查后保存资料。');
    expect(within(modal).getByText('main-v1.2.exe · Unknown')).toBeTruthy();
    showDetailTab('基础信息');
    expect((within(modal).getByRole('textbox', { name: '版本' }) as HTMLInputElement).value).toBe(
      'Final',
    );
    const engine = within(modal).getByRole('combobox', { name: '游戏引擎' }) as HTMLSelectElement;
    expect(engine.value).toBe('Unity');
    expect(api.saveGame).not.toHaveBeenCalled();
    showDetailTab('启动配置');
    fireEvent.click(within(modal).getByRole('button', { name: '使用识别引擎' }));
    expect(engine.value).toBe('WOLF RPG Editor');
    expect(api.saveGame).not.toHaveBeenCalled();
    expect(
      (
        within(modal).getByRole('combobox', {
          name: /^启动文件（相对游戏目录）/,
        }) as HTMLInputElement
      ).value,
    ).toBe('main-v1.2.exe');
  });

  it('fills the analyzed launcher and MTool mode without trusting downloaded BAT paths or saving automatically', async () => {
    const launch = {
      main_executable: 'Game.exe',
      working_directory: '.',
      launch_type: 'MTOOL' as const,
      external_player: null,
      mtool_target_exe: 'Game.exe',
      mtool_loader: null,
    };
    vi.mocked(api.startGameAnalysis).mockResolvedValue('mtool-analysis');
    vi.mocked(api.job).mockResolvedValue({
      ...job,
      id: 'mtool-analysis',
      kind: 'analysis',
      status: 'completed',
      changes: [
        {
          ...candidate,
          registered_id: game.id,
          recommended_launch: launch,
          mtool_detected: true,
          bats: [
            {
              path: 'StartWithTool.bat',
              status: 'supported',
              messages: [],
              recipe: {
                target_exe: 'Wrong.exe',
                loader: 'loaders/wrong.dll',
                observed_root: 'F:/Downloaded/Tool',
                injector: 'loaders/inject.exe',
                runtime: 'nw.exe',
              },
            },
          ],
        },
      ],
      next_cursor: 1,
      change_count: 1,
    });
    vi.mocked(api.saveGame).mockImplementation(async (edit) => ({ ...game, ...edit }));
    await openDetail();
    fireEvent.click(screen.getByRole('button', { name: '分析启动配置' }));
    await screen.findByText('已填入分析启动配置，请检查后保存资料。');
    expect((screen.getByRole('combobox', { name: '启动方式' }) as HTMLSelectElement).value).toBe(
      'MTOOL',
    );
    expect(
      (screen.getByRole('combobox', { name: /启动文件（相对游戏目录）/ }) as HTMLInputElement)
        .value,
    ).toBe('Game.exe');
    expect(api.saveGame).not.toHaveBeenCalled();
    expect(api.play).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole('button', { name: '保存资料' }));
    await waitFor(() => expect(api.saveGame).toHaveBeenCalledWith(expect.objectContaining(launch)));
  });

  it('keeps edits made during analysis and permits explicitly applying the recommendation', async () => {
    let response: JobPage = {
      ...job,
      id: 'draft-analysis',
      kind: 'analysis',
      status: 'running',
      changes: [],
      next_cursor: 0,
      change_count: 0,
    };
    vi.mocked(api.startGameAnalysis).mockResolvedValue(response.id);
    vi.mocked(api.job).mockImplementation(async () => response);
    await openDetail();
    fireEvent.click(screen.getByRole('button', { name: '分析启动配置' }));
    await within(screen.getByRole('dialog')).findByRole('button', { name: '取消任务' });
    fireEvent.change(screen.getByRole('combobox', { name: /启动文件（相对游戏目录）/ }), {
      target: { value: 'Manual.exe' },
    });
    response = {
      ...response,
      status: 'completed',
      changes: [
        {
          ...candidate,
          registered_id: game.id,
          recommended_launch: {
            main_executable: 'New.exe',
            working_directory: '.',
            launch_type: 'DIRECT',
            external_player: null,
            mtool_target_exe: null,
            mtool_loader: null,
          },
        },
      ],
      next_cursor: 1,
      change_count: 1,
    };
    await screen.findByText('分析完成；当前启动配置已修改，可手动使用分析结果。');
    expect(
      (screen.getByRole('combobox', { name: /启动文件（相对游戏目录）/ }) as HTMLInputElement)
        .value,
    ).toBe('Manual.exe');
    fireEvent.click(screen.getByRole('button', { name: '使用分析启动配置' }));
    expect(
      (screen.getByRole('combobox', { name: /启动文件（相对游戏目录）/ }) as HTMLInputElement)
        .value,
    ).toBe('New.exe');
    expect(api.saveGame).not.toHaveBeenCalled();
  });

  it('renames the actual directory through maintenance, keeps errors reviewable and refreshes the existing record', async () => {
    const saved = {
      ...game,
      display_title: '新游戏 v2.0',
      canonical_title: '新游戏 v2.0',
      current_version: 'v2.0',
      install_path: 'E:/Butter/新游戏 v2.0',
    };
    vi.mocked(api.renameGameDirectory)
      .mockRejectedValueOnce('目标目录已存在，不会覆盖')
      .mockResolvedValue(saved);
    await openDetail();
    showDetailTab('记录与管理');
    fireEvent.click(screen.getByRole('button', { name: '重命名目录' }));
    const rename = screen.getByRole('dialog', { name: '重命名游戏目录' });
    fireEvent.change(within(rename).getByRole('textbox', { name: '新目录名称' }), {
      target: { value: saved.display_title },
    });
    expect(api.renameGameDirectory).not.toHaveBeenCalled();
    fireEvent.click(within(rename).getByRole('button', { name: '确认重命名' }));
    await within(rename).findByRole('alert');
    expect(within(rename).getByRole('textbox', { name: '新目录名称' })).toHaveProperty(
      'value',
      saved.display_title,
    );
    vi.mocked(api.games).mockResolvedValue([saved]);
    fireEvent.click(within(rename).getByRole('button', { name: '确认重命名' }));
    await waitFor(() =>
      expect(screen.queryByRole('dialog', { name: '重命名游戏目录' })).toBeNull(),
    );
    expect(api.renameGameDirectory).toHaveBeenCalledWith(
      game.id,
      game.install_path,
      saved.display_title,
    );
    expect(screen.getByRole('dialog', { name: saved.display_title })).toBeTruthy();
    fireEvent.click(screen.getByRole('button', { name: '关闭弹窗' }));
    expect(await screen.findByRole('button', { name: saved.display_title })).toBeTruthy();
    expect(api.removeGame).not.toHaveBeenCalled();
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
      showDetailTab('启动配置');
      fireEvent.change(
        within(modal).getByRole('combobox', {
          name: /启动文件（相对游戏目录）/,
        }),
        { target: { value: 'new.exe' } },
      );
      fireEvent.click(within(modal).getByRole('button', { name: '保存资料' }));
      await waitFor(() => expect(api.startLibraryCheck).toHaveBeenCalledTimes(2));
      await waitForEnabledButton('启动', within(modal));
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
    const detail = await openDetail();
    showDetailTab('启动配置');
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
    const detail = await openDetail();
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
    await waitForEnabledButton('扫描目录');
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
    await waitForEnabledButton('扫描目录');
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
    await waitForEnabledButton('扫描目录');
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
    await waitForEnabledButton('确认关联目录', within(modal));
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
    const modal = await openDetail();
    showDetailTab('记录与管理');
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
    const detail = await openDetail();
    fireEvent.click(within(detail).getByRole('button', { name: '打开目录' }));
    await waitFor(() => expect(api.openFolder).toHaveBeenCalledWith(game.id));
    expect(screen.queryByRole('dialog', { name: '无法打开游戏目录' })).toBeNull();
    await waitForEnabledButton('打开目录', within(detail));
    vi.mocked(api.openFolder).mockRejectedValue(
      '文件操作失败：The system cannot find the file specified. (os error 2)',
    );
    fireEvent.click(within(detail).getByRole('button', { name: '打开目录' }));
    let errorDialog = await screen.findByRole('dialog', { name: '无法打开游戏目录' });
    expect(within(errorDialog).getByText(game.install_path)).toBeTruthy();
    expect(within(errorDialog).getByText(/游戏与存档文件保留/)).toBeTruthy();
    expect(detail.querySelector('.detail-feedback')?.textContent).not.toContain('os error');
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
    await waitForEnabledButton('从库中移除', within(errorDialog));
    vi.mocked(api.games).mockResolvedValue([other]);
    fireEvent.click(within(errorDialog).getByRole('button', { name: '从库中移除' }));
    await waitFor(() => expect(screen.queryByRole('dialog')).toBeNull());
    expect(api.removeGame).toHaveBeenCalledWith(game.id);
    expect(screen.queryByRole('button', { name: game.display_title })).toBeNull();
    expect(screen.getByRole('button', { name: other.display_title })).toBeTruthy();
  });

  it('preserves unsaved detail edits and disables removing from the folder error modal', async () => {
    vi.mocked(api.openFolder).mockRejectedValue('文件操作失败：Access is denied. (os error 5)');
    const detail = await openDetail();
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
    const modal = await openDetail();
    showDetailTab('记录与管理');
    fireEvent.click(within(modal).getByRole('button', { name: '关联新目录' }));
    await waitForEnabledButton('确认关联目录', within(modal));
    expect(api.previewRelocation).toHaveBeenCalledWith(game.id, path);
    expect(api.relocateGame).not.toHaveBeenCalled();
    fireEvent.click(within(modal).getByRole('button', { name: '确认关联目录' }));
    await within(modal).findByText('已关联新目录，原有游戏资料保留。');
    expect(api.relocateGame).toHaveBeenCalledWith(plan);
    expect(screen.getByRole('dialog')).toBe(modal);
    showDetailTab('基础信息');
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
    await waitForEnabledButton('扫描目录');
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
});
