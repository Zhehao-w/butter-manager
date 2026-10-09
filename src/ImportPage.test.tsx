import { beforeEach, describe, expect, it, vi } from 'vitest';
import { act, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { ImportPage } from './ImportPage';
import { api } from './api';
import type { Game, ImportPlan, JobPage, ScanCandidate, Settings } from './types';

vi.mock('./api', () => ({
  api: {
    importPlans: vi.fn(),
    chooseImportSources: vi.fn(),
    discoverImportSources: vi.fn(),
    importAnalyze: vi.fn(),
    importMatches: vi.fn(),
    previewImportDuplicate: vi.fn(),
    recycleImportDuplicate: vi.fn(),
    importPlan: vi.fn(),
    importApply: vi.fn(),
    rollbackVersion: vi.fn(),
    discardImportPlan: vi.fn(),
    chooseLaunchFile: vi.fn(),
    choosePlayerFile: vi.fn(),
    job: vi.fn(),
    cancel: vi.fn(),
    openImportRecords: vi.fn().mockResolvedValue(undefined),
  },
}));
const settings: Settings = {
  game_root: 'E:/Library',
  mtool_root: '',
  mtool_injector: 'loaders/inject.exe',
  mtool_runtime: 'MTool.exe',
  scan_workers: 2,
};
const candidate: ScanCandidate = {
  install_path: 'D:/Incoming/Game v1.2',
  directory_modified_ms: 1700000000000,
  suggested_title: 'Game v1.2',
  engine: 'Unity',
  executables: [
    { relative_path: 'game.exe', architecture: 'x64', score: 100, size_bytes: 7, modified_ms: 0 },
  ],
  bats: [],
  bundled_tool: false,
  warnings: [],
  registered_id: null,
  suggested_version: 'v1.2',
  version_source: 'folder_name',
  working_directory: '.',
  status: 'ready',
  entries_scanned: 1,
  elapsed_ms: 1,
};
const page: JobPage = {
  id: 'analysis',
  kind: 'import_analysis',
  root: '',
  status: 'completed',
  phase: '完成',
  total: 1,
  processed: 1,
  active: {},
  elapsed_ms: 1,
  idle_ms: 0,
  changes: [candidate],
  next_cursor: 1,
  change_count: 1,
  warnings: [],
  error: null,
  registered_ids: [],
};
const plan: ImportPlan = {
  id: 'plan',
  root: settings.game_root,
  status: 'preview',
  items: [
    {
      selection: {
        source: candidate.install_path,
        title: candidate.suggested_title,
        target_name: 'Game v1.2',
        version: 'v1.2',
        engine: 'Unity',
        executable: 'game.exe',
        mtool: false,
        existing_id: null,
      },
      target: 'E:/Library/Game v1.2',
      bytes: 7,
      files: 1,
      state: 'pending',
      error: null,
      registered_id: null,
      cross_volume: true,
      blockers: [],
    },
  ],
};
const props = () => ({
  settings,
  games: [],
  visible: true,
  locked: false,
  onActive: vi.fn(),
  onUpdated: vi.fn(),
  onError: vi.fn(),
  onOpenGame: vi.fn(),
});
beforeEach(() => {
  vi.clearAllMocks();
  vi.mocked(api.importPlans).mockResolvedValue([]);
  vi.mocked(api.chooseImportSources).mockResolvedValue([candidate.install_path]);
  vi.mocked(api.discoverImportSources).mockImplementation(async (sources) => ({
    sources,
    choices: [],
    warnings: [],
  }));
  vi.mocked(api.importAnalyze).mockResolvedValue('analysis');
  vi.mocked(api.importMatches).mockResolvedValue({});
  vi.mocked(api.job).mockResolvedValue(page);
  vi.mocked(api.importPlan).mockResolvedValue('plan');
  vi.mocked(api.importApply).mockResolvedValue('apply');
  vi.mocked(api.cancel).mockResolvedValue();
});
describe('batch import flow', () => {
  it('shows progress only after folder selection, and leaves cancellation unchanged', async () => {
    let finishPicking!: (sources: string[]) => void;
    vi.mocked(api.chooseImportSources).mockImplementation(
      () =>
        new Promise((resolve) => {
          finishPicking = resolve;
        }),
    );
    let finishStarting!: (id: string) => void;
    vi.mocked(api.importAnalyze).mockImplementation(
      () =>
        new Promise((resolve) => {
          finishStarting = resolve;
        }),
    );
    render(<ImportPage {...props()} />);
    const add = screen.getByRole('button', { name: '添加文件夹' });
    const before = screen.getByText(`目标：${settings.game_root}`);
    fireEvent.click(add);
    expect((add as HTMLButtonElement).disabled).toBe(true);
    expect(screen.queryByRole('progressbar')).toBeNull();
    expect(screen.queryByRole('button', { name: '取消任务' })).toBeNull();
    expect(screen.getByText(`目标：${settings.game_root}`)).toBe(before);
    await act(async () => finishPicking([]));
    expect(api.discoverImportSources).not.toHaveBeenCalled();
    expect(api.importAnalyze).not.toHaveBeenCalled();
    expect((add as HTMLButtonElement).disabled).toBe(false);
    fireEvent.click(add);
    await act(async () => finishPicking([candidate.install_path]));
    await waitFor(() => expect(api.importAnalyze).toHaveBeenCalledOnce());
    const progress = screen.getByRole('progressbar', { name: '导入与更新整体进度' });
    expect(progress.getAttribute('aria-valuenow')).toBeNull();
    expect(progress.classList.contains('indeterminate')).toBe(true);
    await act(async () => finishStarting('analysis'));
    await screen.findByText('分析完成，请选择要导入或更新的游戏。');
    expect(screen.queryByRole('progressbar')).toBeNull();
  });
  const oldGame: Game = {
    id: 'old',
    canonical_title: 'Game',
    display_title: 'Game',
    install_path: 'E:/Library/Game',
    working_directory: '.',
    current_version: 'v1.1',
    version_source: 'manual',
    main_executable: 'game.exe',
    engine: 'Unity',
    launch_type: 'DIRECT',
    mtool_target_exe: null,
    mtool_loader: null,
    created_at: '2026-01-01',
    updated_at: '2026-01-01',
    last_launched_at: null,
    play_status: 'PLAYING',
    aliases: ['Game'],
    save_paths: ['<GAME>/SaveData'],
  };
  it('offers disposal only for associated equal versions and requires explicit confirmation', async () => {
    const installed = { ...oldGame, current_version: '1.2' };
    vi.mocked(api.importMatches).mockResolvedValue({
      [candidate.install_path]: [
        {
          id: installed.id,
          title: installed.display_title,
          version: installed.current_version,
          path: installed.install_path,
          reason: '中文名称一致，请确认',
          auto_associate: false,
        },
      ],
    });
    vi.mocked(api.previewImportDuplicate).mockResolvedValue({
      token: 'duplicate-token',
      id: 'incoming:duplicate-token',
      title: candidate.suggested_title,
      game_path: candidate.install_path,
      saves: [],
      blockers: [],
      existing_id: installed.id,
      existing_title: installed.display_title,
      existing_path: installed.install_path,
      version: installed.current_version,
    });
    vi.mocked(api.recycleImportDuplicate).mockResolvedValue(undefined);
    render(<ImportPage {...props()} games={[installed]} />);
    fireEvent.click(screen.getByRole('button', { name: '添加文件夹' }));
    await screen.findByText('待确认关联');
    expect(screen.queryByRole('button', { name: '删除导入副本…' })).toBeNull();
    fireEvent.click(screen.getByRole('button', { name: '关联：Game' }));
    const version = screen.getByRole('textbox', { name: '版本' });
    fireEvent.change(version, { target: { value: 'v1.3' } });
    expect(screen.queryByRole('button', { name: '删除导入副本…' })).toBeNull();
    fireEvent.change(version, { target: { value: 'v1.2' } });
    fireEvent.click(screen.getByRole('button', { name: '删除导入副本…' }));
    const confirmation = await screen.findByRole('button', { name: '确认移入回收站' });
    await waitFor(() => expect((confirmation as HTMLButtonElement).disabled).toBe(false));
    expect(api.previewImportDuplicate).toHaveBeenCalledWith(
      'analysis',
      candidate.install_path,
      installed.id,
      'v1.2',
    );
    expect(screen.getByText(/库中保留：Game/)).toBeTruthy();
    expect(api.recycleImportDuplicate).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole('button', { name: '取消' }));
    expect(api.recycleImportDuplicate).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole('button', { name: '删除导入副本…' }));
    await waitFor(() =>
      expect(
        (screen.getByRole('button', { name: '确认移入回收站' }) as HTMLButtonElement).disabled,
      ).toBe(false),
    );
    fireEvent.click(screen.getByRole('button', { name: '确认移入回收站' }));
    await screen.findByText('导入副本已移入回收站，库中游戏保留。');
    expect(api.recycleImportDuplicate).toHaveBeenCalledOnce();
    expect(api.recycleImportDuplicate).toHaveBeenCalledWith('duplicate-token');
    expect(screen.queryByRole('checkbox', { name: '选择 Game v1.2' })).toBeNull();
    expect(api.importPlan).not.toHaveBeenCalled();
    expect(api.importApply).not.toHaveBeenCalled();
  });
  it('defaults to preserving saves and lets matched updates choose new saves without an extra confirmation', async () => {
    vi.mocked(api.importMatches).mockResolvedValue({
      [candidate.install_path]: [
        {
          id: 'old',
          title: 'Game',
          version: 'v1.1',
          path: oldGame.install_path,
          reason: '名称一致',
        },
      ],
    });
    render(<ImportPage {...props()} games={[oldGame]} />);
    fireEvent.click(screen.getByRole('button', { name: '添加文件夹' }));
    await screen.findByRole('radiogroup', { name: '存档处理' });
    expect((screen.getByRole('textbox', { name: '目标文件夹' }) as HTMLInputElement).value).toBe(
      'Game',
    );
    expect((screen.getByRole('textbox', { name: '游戏名称' }) as HTMLInputElement).disabled).toBe(
      true,
    );
    const select = screen.getByRole('checkbox', { name: '选择 Game v1.2' });
    expect((select as HTMLInputElement).checked).toBe(false);
    fireEvent.click(select);
    expect((screen.getByRole('radio', { name: /保留旧存档/ }) as HTMLInputElement).checked).toBe(
      true,
    );
    expect(screen.queryByRole('checkbox', { name: /我已确认/ })).toBeNull();
    expect(
      (screen.getByRole('button', { name: '生成导入与更新计划（1）' }) as HTMLButtonElement)
        .disabled,
    ).toBe(false);
    fireEvent.click(screen.getByRole('radio', { name: '使用新包存档' }));
    expect((screen.getByRole('radio', { name: /保留旧存档/ }) as HTMLInputElement).checked).toBe(
      false,
    );
    expect(screen.getByText('不迁移内部旧存档，使用新包存档；外部存档不操作。')).toBeTruthy();
    fireEvent.click(screen.getByRole('button', { name: '生成导入与更新计划（1）' }));
    await waitFor(() =>
      expect(api.importPlan).toHaveBeenCalledWith('analysis', [
        expect.objectContaining({
          existing_id: 'old',
          preserve_saves: false,
          saves_confirmed: true,
        }),
      ]),
    );
    expect(api.importApply).not.toHaveBeenCalled();
  });
  it.each(['inherit', 'override', 'automatic', 'reset'] as const)(
    'passes only explicit launch configuration changes to update plans: %s',
    async (mode) => {
      const installed: Game = {
        ...oldGame,
        launch_type: 'MTOOL',
        working_directory: 'manual-cwd',
        mtool_loader: 'loaders/custom.dll',
      };
      vi.mocked(api.importMatches).mockResolvedValue({
        [candidate.install_path]: [
          {
            id: installed.id,
            title: installed.display_title,
            version: installed.current_version,
            path: installed.install_path,
            reason: '名称一致',
          },
        ],
      });
      render(<ImportPage {...props()} games={[installed]} />);
      fireEvent.click(screen.getByRole('button', { name: '添加文件夹' }));
      await screen.findByRole('radiogroup', { name: '存档处理' });
      fireEvent.click(screen.getByRole('checkbox', { name: '公共 MTool' }));
      fireEvent.click(screen.getByText('更新启动配置'));
      const working = screen.getByLabelText('工作目录（相对新版游戏目录）') as HTMLInputElement;
      const loader = screen.getByLabelText(
        'MTool loader（相对公共 MTool 目录）',
      ) as HTMLInputElement;
      expect(working.value).toBe('manual-cwd');
      expect(loader.value).toBe('loaders/custom.dll');
      if (mode !== 'inherit') {
        fireEvent.change(working, { target: { value: 'new-cwd' } });
        fireEvent.change(loader, {
          target: { value: mode === 'automatic' ? '' : 'loaders/new.dll' },
        });
      }
      if (mode === 'reset') {
        fireEvent.click(screen.getByRole('button', { name: '恢复继承旧配置' }));
        expect(working.value).toBe('manual-cwd');
        expect(loader.value).toBe('loaders/custom.dll');
      }
      fireEvent.click(screen.getByRole('checkbox', { name: '选择 Game v1.2' }));
      fireEvent.click(screen.getByRole('button', { name: '生成导入与更新计划（1）' }));
      await waitFor(() => expect(api.importPlan).toHaveBeenCalledOnce());
      const selected = vi.mocked(api.importPlan).mock.calls[0][1][0];
      expect(selected.working_directory).toBe(
        mode === 'inherit' || mode === 'reset' ? undefined : 'new-cwd',
      );
      expect(selected.mtool_loader).toBe(
        mode === 'inherit' || mode === 'reset'
          ? undefined
          : mode === 'automatic'
            ? ''
            : 'loaders/new.dll',
      );
    },
  );
  it('keeps weak and ambiguous recommendations unassociated until the user chooses', async () => {
    for (const ambiguous of [false, true]) {
      const recommendation = {
        id: oldGame.id,
        title: oldGame.display_title,
        version: oldGame.current_version,
        path: oldGame.install_path,
        reason: '英文名称一致，译名或包描述不同；请确认',
        auto_associate: ambiguous,
      };
      vi.mocked(api.importMatches).mockResolvedValue({
        [candidate.install_path]: ambiguous
          ? [recommendation, { ...recommendation, id: 'other', title: 'Other Game' }]
          : [recommendation],
      });
      const rendered = render(<ImportPage {...props()} games={[oldGame]} />);
      fireEvent.click(screen.getByRole('button', { name: '添加文件夹' }));
      await screen.findByText('待确认关联');
      expect(screen.getByText('发现可能已有的游戏，请确认关联')).toBeTruthy();
      expect(screen.getAllByText(recommendation.reason).length).toBe(ambiguous ? 2 : 1);
      expect(screen.queryByText(`${recommendation.title}（${recommendation.reason}）`)).toBeNull();
      expect(screen.queryByRole('radiogroup', { name: '存档处理' })).toBeNull();
      fireEvent.click(screen.getByRole('checkbox', { name: '选择 Game v1.2' }));
      const generate = screen.getByRole('button', {
        name: '生成导入与更新计划（1）',
      }) as HTMLButtonElement;
      expect(generate.disabled).toBe(true);
      expect(screen.getByText(/还有 1 项需要确认关联/)).toBeTruthy();
      const status = screen.getByText(/还有 1 项需要确认关联/).parentElement!;
      const noticeRow = status.firstElementChild;
      if (ambiguous) {
        fireEvent.click(screen.getByRole('button', { name: '改为新游戏' }));
        expect(generate.disabled).toBe(false);
        expect(screen.queryByText(/还有 1 项需要确认关联/)).toBeNull();
      } else {
        fireEvent.click(screen.getByRole('button', { name: '关联：Game' }));
        await screen.findByRole('radiogroup', { name: '存档处理' });
        expect(screen.getByRole('button', { name: '已关联：Game' })).toBeTruthy();
        expect(screen.getByText('已关联库中游戏')).toBeTruthy();
        expect(screen.queryByText(/还有 1 项需要确认关联/)).toBeNull();
        expect(generate.disabled).toBe(false);
        expect(status.firstElementChild).toBe(noticeRow);
        expect(status.querySelector('#import-plan-block-reason')?.getAttribute('aria-hidden')).toBe(
          'true',
        );
        fireEvent.click(generate);
        await waitFor(() =>
          expect(api.importPlan).toHaveBeenCalledWith('analysis', [
            expect.objectContaining({
              existing_id: 'old',
              preserve_saves: true,
              saves_confirmed: true,
              new_override: false,
            }),
          ]),
        );
        expect(api.importApply).not.toHaveBeenCalled();
      }
      rendered.unmount();
    }
  });
  it('lists games across batches chronologically, searches all records, and opens the associated game', async () => {
    const options = props();
    const completedItem = {
      ...plan.items[0],
      state: 'completed',
      registered_id: 'old',
      completed_at_ms: Date.parse('2026-10-07T12:00:00Z'),
    };
    vi.mocked(api.importPlans).mockResolvedValue([
      {
        ...plan,
        status: 'cancelled',
        items: [
          completedItem,
          { ...plan.items[0], selection: { ...plan.items[0].selection, title: 'Unfinished' } },
        ],
      },
      {
        ...plan,
        id: 'newer',
        status: 'completed',
        recorded_at_ms: Date.parse('2026-10-08T12:00:00Z'),
        items: [
          {
            ...completedItem,
            completed_at_ms: undefined,
            registered_id: 'removed',
            selection: { ...plan.items[0].selection, title: 'Newest Game', source: 'D:/Newest' },
          },
          {
            ...completedItem,
            completed_at_ms: Date.parse('2026-10-08T11:00:00Z'),
            selection: { ...plan.items[0].selection, title: 'Second Game', source: 'D:/Second' },
          },
        ],
      },
    ]);
    render(<ImportPage {...options} games={[oldGame]} />);
    fireEvent.click(await screen.findByRole('tab', { name: '导入记录 (3)' }));
    expect(screen.queryByLabelText('本分类导入记录')).toBeNull();
    expect(screen.getAllByRole('article').map((row) => row.getAttribute('aria-label'))).toEqual([
      'Newest Game',
      'Second Game',
      'Game v1.2',
    ]);
    expect(screen.queryByText('Unfinished')).toBeNull();
    expect(screen.queryByText('来源：D:/Newest')).toBeNull();
    expect(
      (
        within(screen.getByRole('article', { name: 'Newest Game' })).getByRole('button', {
          name: '查看游戏',
        }) as HTMLButtonElement
      ).disabled,
    ).toBe(true);
    fireEvent.change(screen.getByRole('textbox', { name: '导入搜索' }), {
      target: { value: 'v1.2' },
    });
    expect(screen.getAllByRole('article')).toHaveLength(3);
    fireEvent.change(screen.getByRole('textbox', { name: '导入搜索' }), {
      target: { value: 'D:/Second' },
    });
    expect(screen.getAllByRole('article')).toHaveLength(1);
    fireEvent.click(screen.getByRole('button', { name: '查看游戏' }));
    expect(options.onOpenGame).toHaveBeenCalledWith('old');
    fireEvent.change(screen.getByRole('textbox', { name: '导入搜索' }), {
      target: { value: 'Newest' },
    });
    fireEvent.click(screen.getByRole('button', { name: '详情' }));
    const detail = screen.getByRole('dialog', { name: '导入记录详情' });
    expect(within(detail).getByText('D:/Newest')).toBeTruthy();
    expect(within(detail).getByText('旧记录按记录文件时间显示')).toBeTruthy();
    expect(api.importApply).not.toHaveBeenCalled();
  });
  it('explains recycle-bin rollback and confirms without immediately invoking it', async () => {
    const updated: ImportPlan = {
      ...plan,
      status: 'completed',
      items: [
        {
          ...plan.items[0],
          state: 'completed',
          selection: {
            ...plan.items[0].selection,
            existing_id: 'old',
            preserve_saves: true,
            saves_confirmed: true,
          },
          update: {
            old_version: 'v1.1',
            saves: [
              {
                configured: '<GAME>/SaveData',
                source: 'E:/Library/Game/SaveData',
                relative: 'SaveData',
                present: true,
                bytes: 3,
              },
            ],
            required_bytes: 20,
            quarantine: 'E:/Library/.butter-import-fixture',
            rollback_available: true,
          },
        },
      ],
    };
    vi.mocked(api.importPlans).mockResolvedValue([updated]);
    vi.mocked(api.rollbackVersion).mockResolvedValue('rollback');
    render(<ImportPage {...props()} games={[{ ...oldGame, current_version: 'v1.2' }]} />);
    fireEvent.click(await screen.findByRole('tab', { name: /导入记录/ }));
    expect(screen.getByText('更新 v1.1 → v1.2')).toBeTruthy();
    expect(screen.queryByText('保留内部旧存档')).toBeNull();
    fireEvent.click(screen.getByRole('button', { name: '详情' }));
    expect(screen.getByText('1 个文件 · 7 B')).toBeTruthy();
    expect(screen.getByText('保留内部旧存档')).toBeTruthy();
    expect(screen.queryByText(/关联已有游戏：/)).toBeNull();
    expect(screen.queryByText(/保留游戏名称、别名/)).toBeNull();
    fireEvent.click(await screen.findByRole('button', { name: '回退旧版本' }));
    expect(api.rollbackVersion).not.toHaveBeenCalled();
    const dialog = screen.getByRole('dialog', { name: '确认回退旧版本' });
    expect(within(dialog).getByText(/回收站已清空时无法回退/)).toBeTruthy();
    expect(
      within(dialog)
        .getAllByRole('button')
        .map((button) => button.textContent),
    ).toEqual(['取消', '确认回退']);
    fireEvent.click(within(dialog).getByRole('button', { name: '确认回退' }));
    await waitFor(() => expect(api.rollbackVersion).toHaveBeenCalledWith('plan', 0));
  });
  it('shows damaged-record recovery without enabling new imports or hiding valid plans', async () => {
    const options = props();
    vi.mocked(api.importPlans).mockResolvedValue([plan]);
    render(
      <ImportPage
        {...options}
        recoveryIssues={[{ record: 'broken.json', message: '记录不完整' }]}
      />,
    );
    expect(screen.getByRole('alert', { name: '导入记录恢复提示' }).textContent).toContain(
      '游戏文件和原始记录均已保留',
    );
    expect((screen.getByRole('button', { name: '添加文件夹' }) as HTMLButtonElement).disabled).toBe(
      true,
    );
    fireEvent.click(screen.getByText('查看记录详情'));
    expect(screen.getByText('broken.json')).toBeTruthy();
    fireEvent.click(screen.getByRole('button', { name: '打开记录文件夹' }));
    await waitFor(() => expect(api.openImportRecords).toHaveBeenCalledOnce());
    await waitFor(() =>
      expect(screen.getByRole('tab', { name: /待确认/ }).textContent).toContain('1'),
    );
    fireEvent.click(screen.getByRole('tab', { name: /待确认/ }));
    expect(
      ((await screen.findByRole('button', { name: '确认执行计划' })) as HTMLButtonElement).disabled,
    ).toBe(true);
    expect(api.importApply).not.toHaveBeenCalled();
  });
  it('restores the import viewport after hiding and resets it for a different search', async () => {
    const inputProps = props();
    const { rerender } = render(<ImportPage {...inputProps} />);
    fireEvent.click(screen.getByRole('button', { name: '添加文件夹' }));
    await screen.findByRole('textbox', { name: '游戏名称' });
    const viewport = () => screen.getByRole('tabpanel');
    fireEvent.scroll(viewport(), { target: { scrollTop: 500 } });
    rerender(<ImportPage {...inputProps} visible={false} />);
    rerender(<ImportPage {...inputProps} visible />);
    expect(viewport().scrollTop).toBe(500);
    fireEvent.change(screen.getByRole('textbox', { name: '导入搜索' }), {
      target: { value: 'Game' },
    });
    expect(viewport().scrollTop).toBe(0);
  });
  it('requires an explicit scope for a wrapper and keeps cancellation read-only', async () => {
    const root = 'D:/Wrapped Game';
    vi.mocked(api.chooseImportSources).mockResolvedValue([root]);
    vi.mocked(api.discoverImportSources).mockResolvedValue({
      sources: [],
      choices: [{ root, children: [`${root}/bin`] }],
      warnings: [],
    });
    render(<ImportPage {...props()} />);
    fireEvent.click(screen.getByRole('button', { name: '添加文件夹' }));
    let modal = await screen.findByRole('dialog', { name: '确认文件夹范围' });
    expect(
      (within(modal).getByRole('button', { name: '开始分析' }) as HTMLButtonElement).disabled,
    ).toBe(true);
    expect(api.importAnalyze).not.toHaveBeenCalled();
    fireEvent.click(within(modal).getByRole('button', { name: '取消' }));
    expect(api.importAnalyze).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole('button', { name: '添加文件夹' }));
    modal = await screen.findByRole('dialog', { name: '确认文件夹范围' });
    fireEvent.change(within(modal).getByRole('combobox'), { target: { value: 'whole' } });
    fireEvent.click(within(modal).getByRole('button', { name: '开始分析' }));
    await waitFor(() => expect(api.importAnalyze).toHaveBeenCalledWith([root]));
    expect(api.importApply).not.toHaveBeenCalled();
  });
  it('preserves existing edits and an unchecked choice when adding more folders', async () => {
    render(<ImportPage {...props()} />);
    fireEvent.click(screen.getByRole('button', { name: '添加文件夹' }));
    fireEvent.change(await screen.findByRole('textbox', { name: '游戏名称' }), {
      target: { value: 'Kept Edit' },
    });
    fireEvent.click(screen.getByRole('checkbox', { name: '选择 Kept Edit' }));
    vi.mocked(api.chooseImportSources).mockResolvedValue(['D:/Incoming/Another']);
    vi.mocked(api.importAnalyze).mockResolvedValue('analysis-again');
    vi.mocked(api.job).mockResolvedValue({ ...page, id: 'analysis-again' });
    fireEvent.click(screen.getByRole('button', { name: '添加文件夹' }));
    await waitFor(() =>
      expect(api.importAnalyze).toHaveBeenLastCalledWith([
        candidate.install_path,
        'D:/Incoming/Another',
      ]),
    );
    await waitFor(() =>
      expect(
        (screen.getByRole('button', { name: '添加文件夹' }) as HTMLButtonElement).disabled,
      ).toBe(false),
    );
    expect((screen.getByRole('textbox', { name: '游戏名称' }) as HTMLInputElement).value).toBe(
      'Kept Edit',
    );
    expect(
      (screen.getByRole('checkbox', { name: '选择 Kept Edit' }) as HTMLInputElement).checked,
    ).toBe(false);
  });
  it('groups plans into tabs, retains new-import edits, and confirms with only two buttons', async () => {
    const options = props();
    render(<ImportPage {...options} />);
    fireEvent.click(screen.getByRole('button', { name: '添加文件夹' }));
    fireEvent.change(await screen.findByRole('textbox', { name: '游戏名称' }), {
      target: { value: 'Edited Game' },
    });
    const completed = {
      ...plan,
      id: 'done',
      status: 'completed',
      items: [
        {
          ...plan.items[0],
          state: 'completed',
          selection: { ...plan.items[0].selection, title: 'Finished Game' },
        },
      ],
    };
    vi.mocked(api.importPlans).mockResolvedValue([plan, completed]);
    vi.mocked(api.job).mockImplementation((id) =>
      Promise.resolve(
        id === 'plan'
          ? {
              ...page,
              id,
              kind: 'import_plan',
              changes: [],
              next_cursor: 0,
              change_count: 0,
            }
          : page,
      ),
    );
    fireEvent.click(screen.getByRole('button', { name: '生成导入与更新计划（1）' }));
    await screen.findByRole('button', { name: '确认执行计划' });
    expect(screen.getByRole('tab', { name: /待确认/ }).getAttribute('aria-selected')).toBe('true');
    expect(document.querySelector('.toast')).toBeNull();
    expect(screen.getByText('导入与更新计划已生成，请确认。')).toBeTruthy();
    fireEvent.click(screen.getByRole('tab', { name: /新的导入/ }));
    expect((screen.getByRole('textbox', { name: '游戏名称' }) as HTMLInputElement).value).toBe(
      'Edited Game',
    );
    fireEvent.click(screen.getByRole('tab', { name: /导入记录/ }));
    expect(screen.getByText('Finished Game')).toBeTruthy();
    expect(screen.queryByRole('button', { name: '确认执行计划' })).toBeNull();
    fireEvent.click(screen.getByRole('tab', { name: /待确认/ }));
    fireEvent.click(screen.getByRole('button', { name: '确认执行计划' }));
    const dialog = screen.getByRole('dialog', { name: '确认导入与更新' });
    expect(
      within(dialog)
        .getAllByRole('button')
        .map((button) => button.textContent),
    ).toEqual(['取消', '确认']);
    fireEvent.click(within(dialog).getByRole('button', { name: '取消' }));
    expect(api.importApply).not.toHaveBeenCalled();
  });
  it('keeps ambiguous QSP files blank and submits the explicit choice without a player', async () => {
    const qsp = {
      ...candidate,
      engine: 'QSP',
      executables: [],
      qsp: { game_files: ['game.qsp', 'mod.qsp'], players: [], recommended_player: null },
    };
    vi.mocked(api.job).mockResolvedValue({ ...page, changes: [qsp] });
    vi.mocked(api.choosePlayerFile).mockResolvedValue('mod.qsp');
    render(<ImportPage {...props()} />);
    fireEvent.click(screen.getByRole('button', { name: '添加文件夹' }));
    const file = await screen.findByRole('combobox', { name: 'QSP 主游戏文件' });
    expect((file as HTMLInputElement).value).toBe('');
    expect((screen.getByRole('combobox', { name: 'QSP 播放器' }) as HTMLInputElement).value).toBe(
      '',
    );
    expect(
      (screen.getByRole('checkbox', { name: '选择 Game v1.2' }) as HTMLInputElement).checked,
    ).toBe(false);
    expect(screen.queryByLabelText('公共 MTool')).toBeNull();
    fireEvent.click(screen.getByRole('button', { name: '更改 QSP 文件…' }));
    await waitFor(() => expect((file as HTMLInputElement).value).toBe('mod.qsp'));
    expect(api.choosePlayerFile).toHaveBeenCalledWith(candidate.install_path, 'qsp');
    fireEvent.click(screen.getByRole('checkbox', { name: '选择 Game v1.2' }));
    fireEvent.click(screen.getByRole('button', { name: '生成导入与更新计划（1）' }));
    await waitFor(() =>
      expect(api.importPlan).toHaveBeenCalledWith('analysis', [
        expect.objectContaining({
          engine: 'QSP',
          executable: '',
          mtool: false,
          external_player: { player_type: 'QSP', scope: 'GAME_LOCAL', game_file: 'mod.qsp' },
        }),
      ]),
    );
    expect(api.importApply).not.toHaveBeenCalled();
  });
  it('automatically selects one QSP file and the recommended bundled player', async () => {
    const qsp = {
      ...candidate,
      engine: 'QSP',
      qsp: {
        game_files: ['彼女の冒険.qsp'],
        players: ['qspgui.exe'],
        recommended_player: 'qspgui.exe',
      },
    };
    vi.mocked(api.job).mockResolvedValue({ ...page, changes: [qsp] });
    render(<ImportPage {...props()} />);
    fireEvent.click(screen.getByRole('button', { name: '添加文件夹' }));
    expect(
      ((await screen.findByRole('combobox', { name: 'QSP 主游戏文件' })) as HTMLInputElement).value,
    ).toBe('彼女の冒険.qsp');
    expect((screen.getByRole('combobox', { name: 'QSP 播放器' }) as HTMLInputElement).value).toBe(
      'qspgui.exe',
    );
    expect(
      (screen.getByRole('checkbox', { name: '选择 Game v1.2' }) as HTMLInputElement).checked,
    ).toBe(true);
  });
  it('resolves collection and game folders through one picker and retains reviewed drafts without moving', async () => {
    const sources = [candidate.install_path, 'D:/Incoming/Other'];
    const selected = ['D:/Incoming', 'D:/Incoming/Other'];
    vi.mocked(api.chooseImportSources).mockResolvedValue(selected);
    vi.mocked(api.discoverImportSources).mockResolvedValue({ sources, choices: [], warnings: [] });
    const options = props();
    const rendered = render(<ImportPage {...options} />);
    fireEvent.click(screen.getByRole('button', { name: '添加文件夹' }));
    await screen.findByRole('textbox', { name: '游戏名称' });
    await waitFor(() => expect(api.importAnalyze).toHaveBeenCalledWith(sources));
    expect(api.chooseImportSources).toHaveBeenCalledWith();
    expect(api.discoverImportSources).toHaveBeenCalledWith(selected);
    expect(api.importApply).not.toHaveBeenCalled();
    fireEvent.change(screen.getByRole('textbox', { name: '目标文件夹' }), {
      target: { value: 'New name' },
    });
    rendered.rerender(<ImportPage {...options} visible={false} />);
    rendered.rerender(<ImportPage {...options} />);
    expect(screen.getByRole('textbox', { name: '目标文件夹' }).getAttribute('value')).toBe(
      'New name',
    );
    fireEvent.click(screen.getByRole('button', { name: '生成导入与更新计划（1）' }));
    await waitFor(() =>
      expect(api.importPlan).toHaveBeenCalledWith('analysis', [
        expect.objectContaining({ target_name: 'New name', source: candidate.install_path }),
      ]),
    );
    expect(api.importApply).not.toHaveBeenCalled();
  });
  it('requires popup confirmation before moving or resuming a reviewed plan', async () => {
    vi.mocked(api.importPlans).mockResolvedValue([{ ...plan, status: 'interrupted' }]);
    const options = props();
    render(<ImportPage {...options} />);
    fireEvent.click(await screen.findByRole('button', { name: '继续未完成项' }));
    expect(api.importApply).not.toHaveBeenCalled();
    const dialog = screen.getByRole('dialog', { name: '确认导入与更新' });
    fireEvent.click(within(dialog).getByRole('button', { name: '确认' }));
    await waitFor(() => expect(api.importApply).toHaveBeenCalledWith('plan', false));
  });
  it('blocks apply for an update whose save review is incomplete', async () => {
    vi.mocked(api.importPlans).mockResolvedValue([
      {
        ...plan,
        status: 'failed',
        items: [{ ...plan.items[0], blockers: ['请确认已有游戏的存档范围和迁移方式'] }],
      },
    ]);
    render(<ImportPage {...props()} />);
    await screen.findByText('请确认已有游戏的存档范围和迁移方式');
    expect(screen.queryByRole('button', { name: '继续未完成项' })).toBeNull();
    expect(api.importApply).not.toHaveBeenCalled();
  });
  it('keeps matched existing games unchecked and allows explicit new-game override', async () => {
    vi.mocked(api.importMatches).mockResolvedValue({
      [candidate.install_path]: [
        {
          id: 'old',
          title: 'Game',
          version: 'v1.1',
          path: 'E:/Library/Game',
          reason: '去除明确版本后名称一致',
        },
      ],
    });
    render(<ImportPage {...props()} />);
    fireEvent.click(screen.getByRole('button', { name: '添加文件夹' }));
    await screen.findByRole('button', { name: '已关联：Game' });
    const checkbox = screen.getByRole('checkbox', { name: '选择 Game v1.2' }) as HTMLInputElement;
    expect(checkbox.checked).toBe(false);
    fireEvent.click(screen.getByRole('button', { name: '改为新游戏' }));
    fireEvent.click(checkbox);
    fireEvent.click(screen.getByRole('button', { name: '生成导入与更新计划（1）' }));
    await waitFor(() =>
      expect(api.importPlan).toHaveBeenCalledWith('analysis', [
        expect.objectContaining({ existing_id: null }),
      ]),
    );
  });
  it('cancels an active copy and disables repeated cancellation', async () => {
    vi.mocked(api.importPlans).mockResolvedValue([{ ...plan, status: 'interrupted' }]);
    const copying: JobPage = {
      ...page,
      id: 'apply',
      kind: 'import_apply',
      status: 'running',
      changes: [],
      phase: '跨盘移动 · 复制',
      bytes_done: 3,
      bytes_total: 7,
      overall_done: 45,
      overall_total: 100,
      current_game: '正在导入的游戏',
      transfer_rate: 1024 ** 2,
      remaining_seconds: 90,
      indeterminate: false,
    };
    vi.mocked(api.job).mockResolvedValue({
      ...copying,
      phase: '检查游戏运行状态',
      indeterminate: true,
      overall_done: 0,
      bytes_done: 0,
      bytes_total: 0,
      transfer_rate: null,
      remaining_seconds: null,
    });
    render(<ImportPage {...props()} />);
    fireEvent.click(await screen.findByRole('button', { name: '继续未完成项' }));
    fireEvent.click(within(screen.getByRole('dialog')).getByRole('button', { name: '确认' }));
    await screen.findByText('正在导入的游戏');
    expect(
      screen.getByRole('progressbar', { name: '导入与更新整体进度' }).getAttribute('aria-valuenow'),
    ).toBeNull();
    expect(screen.queryByText('0%')).toBeNull();
    vi.mocked(api.job).mockResolvedValue(copying);
    await screen.findByText('45%');
    expect(
      screen.getByRole('progressbar', { name: '导入与更新整体进度' }).getAttribute('aria-valuenow'),
    ).toBe('45');
    expect(screen.getByText('45%')).toBeTruthy();
    expect(screen.getByText('1.0 MB/s')).toBeTruthy();
    expect(screen.getByText('预计传输剩余 约 2 分钟')).toBeTruthy();
    fireEvent.click(await screen.findByRole('button', { name: '取消任务' }));
    await waitFor(() => expect(api.cancel).toHaveBeenCalledWith('apply'));
    expect((screen.getByRole('button', { name: '正在停止…' }) as HTMLButtonElement).disabled).toBe(
      true,
    );
  });
});
