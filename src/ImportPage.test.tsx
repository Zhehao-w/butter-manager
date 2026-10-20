import { waitForEnabledButton } from './testing/interactions';
import { resetApiMock } from './testing/apiMock';
import { makeSettings, makeCandidate, makeJob, makeGame } from './testing/fixtures';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { act, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { ImportPage } from './ImportPage';
import { api } from './api';
import type { Game, ImportPlan, JobPage, ScanCandidate, Settings } from './types';

vi.mock('./api', async (original) => {
  const { api } = await original<typeof import('./api')>();
  const { createApiMock } = await import('./testing/apiMock');
  return { api: createApiMock(api) };
});
const settings: Settings = makeSettings({ game_root: 'E:/Library' });
const candidate: ScanCandidate = makeCandidate({
  install_path: 'D:/Incoming/Game v1.2',
  suggested_title: 'Game v1.2',
  engine: 'Unity',
  executables: [
    { relative_path: 'game.exe', architecture: 'x64', score: 100, size_bytes: 7, modified_ms: 0 },
  ],
  suggested_version: 'v1.2',
  version_source: 'folder_name',
});
const page: JobPage = makeJob({
  id: 'analysis',
  kind: 'import_analysis',
  phase: '完成',
  total: 1,
  processed: 1,
  elapsed_ms: 1,
  changes: [candidate],
  next_cursor: 1,
  change_count: 1,
});
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
function renderNewImport(overrides: Partial<React.ComponentProps<typeof ImportPage>> = {}) {
  const view = render(<ImportPage {...props()} {...overrides} />);
  fireEvent.click(screen.getByRole('button', { name: '添加文件夹' }));
  return view;
}
async function expandSettings(...sections: string[]) {
  await screen.findByRole('button', { name: /展开设置|收起设置/ });
  const folded = screen.queryByRole('button', { name: '展开设置' });
  if (folded) fireEvent.click(folded);
  for (const section of sections.length ? sections : ['基本信息']) {
    const tab = await screen.findByRole('tab', { name: section });
    if (tab.getAttribute('aria-selected') !== 'true') fireEvent.click(tab);
  }
}
beforeEach(() => {
  vi.clearAllMocks();
  resetApiMock(api);
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
  it.each([false, true])(
    'requires confirming a remembered recommendation and allows opting out of remembering: %s',
    async (remember) => {
      const existing = makeGame({
        id: 'old',
        canonical_title: '月光庭院',
        display_title: '月光庭院',
      });
      vi.mocked(api.importMatches).mockResolvedValue({
        [candidate.install_path]: [
          {
            id: existing.id,
            title: existing.display_title,
            version: 'v0.7',
            path: existing.install_path,
            reason: '曾由你确认关联 · 来源名称一致；请确认',
            auto_associate: false,
            remembered: true,
          },
        ],
      });
      renderNewImport({ games: [existing] });
      const associate = await screen.findByRole('button', { name: '关联：月光庭院' });
      expect(screen.getByText('关联记忆')).toBeTruthy();
      const generate = screen.getByRole('button', { name: '生成导入与更新计划（1）' });
      expect((generate as HTMLButtonElement).disabled).toBe(true);
      expect(screen.queryByRole('checkbox', { name: '记住本次关联' })).toBeNull();
      fireEvent.click(associate);
      await expandSettings('游戏关联');
      const checkbox = screen.getByRole('checkbox', { name: '记住本次关联' }) as HTMLInputElement;
      expect(checkbox.checked).toBe(true);
      if (!remember) fireEvent.click(checkbox);
      expect(api.importMemories).not.toHaveBeenCalled();
      fireEvent.click(generate);
      await waitFor(() => expect(api.importPlan).toHaveBeenCalled());
      const selection = vi.mocked(api.importPlan).mock.calls.at(-1)![1][0];
      expect(selection.existing_id).toBe(existing.id);
      expect(!!selection.remember_association).toBe(remember);
      expect(api.importApply).not.toHaveBeenCalled();
    },
  );

  it('defaults each new association to remembered and clears it for a new game', async () => {
    const games = [
      makeGame({ id: 'a', display_title: '第一款游戏' }),
      makeGame({ id: 'b', display_title: '第二款游戏' }),
    ];
    vi.mocked(api.importMatches).mockResolvedValue({
      [candidate.install_path]: games.map((game) => ({
        id: game.id,
        title: game.display_title,
        path: game.install_path,
        version: 'v1',
        reason: '需要选择',
        auto_associate: false,
      })),
    });
    renderNewImport({ games });
    fireEvent.click(await screen.findByRole('button', { name: '关联：第一款游戏' }));
    await expandSettings('游戏关联');
    expect(
      (screen.getByRole('checkbox', { name: '记住本次关联' }) as HTMLInputElement).checked,
    ).toBe(true);
    fireEvent.click(screen.getByRole('checkbox', { name: '记住本次关联' }));
    fireEvent.click(screen.getByRole('button', { name: '关联：第二款游戏' }));
    expect(
      (screen.getByRole('checkbox', { name: '记住本次关联' }) as HTMLInputElement).checked,
    ).toBe(true);
    fireEvent.click(screen.getByRole('checkbox', { name: '记住本次关联' }));
    fireEvent.click(screen.getByRole('button', { name: '改为新游戏' }));
    expect(screen.queryByRole('checkbox', { name: '记住本次关联' })).toBeNull();
    fireEvent.click(screen.getByRole('button', { name: '关联：第一款游戏' }));
    expect(
      (screen.getByRole('checkbox', { name: '记住本次关联' }) as HTMLInputElement).checked,
    ).toBe(true);
    fireEvent.click(screen.getByRole('button', { name: '改为新游戏' }));
    fireEvent.click(screen.getByRole('button', { name: '生成导入与更新计划（1）' }));
    await waitFor(() => expect(api.importPlan).toHaveBeenCalled());
    const selection = vi.mocked(api.importPlan).mock.calls.at(-1)![1][0];
    expect(selection.existing_id).toBeNull();
    expect(selection.remember_association).toBe(false);
  });

  it.each(['unassociated', 'associated', 'failure'] as const)(
    'opens the source and only the selected associated game folder: %s',
    async (mode) => {
      const existing = makeGame({ id: 'linked-game', install_path: 'E:/Library/Existing' });
      if (mode === 'associated')
        vi.mocked(api.importMatches).mockResolvedValue({
          [candidate.install_path]: [
            {
              id: existing.id,
              title: existing.display_title,
              version: 'v1.0',
              path: existing.install_path,
              reason: '名称一致',
            },
          ],
        });
      if (mode === 'failure') vi.mocked(api.openDirectory).mockRejectedValue('来源目录不存在');
      const options = props();
      renderNewImport({ ...options, games: [existing] });
      const source = await screen.findByRole('button', { name: '打开源目录' });
      await waitForEnabledButton('打开源目录');
      fireEvent.click(source);
      await waitFor(() =>
        expect(api.openDirectory).toHaveBeenCalledExactlyOnceWith(candidate.install_path),
      );
      await waitForEnabledButton('打开源目录');
      if (mode === 'failure')
        await waitFor(() => expect(options.onError).toHaveBeenCalledWith('来源目录不存在'));
      if (mode === 'associated') {
        await expandSettings('基本信息');
        expect(screen.queryByRole('button', { name: '打开关联目录' })).toBeNull();
        await expandSettings('游戏关联');
        const linked = await screen.findByRole('button', { name: '打开关联目录' });
        const gameRow = linked.closest('.import-match-game-row')!;
        expect(
          within(gameRow as HTMLElement).getByRole('button', {
            name: `已关联：${existing.display_title}`,
          }),
        ).toBeTruthy();
        expect(linked.closest('.import-source-row')).toBeNull();
        fireEvent.click(linked);
        await waitFor(() => expect(api.openFolder).toHaveBeenCalledExactlyOnceWith(existing.id));
      } else {
        expect(screen.queryByRole('button', { name: '打开关联目录' })).toBeNull();
        expect(api.openFolder).not.toHaveBeenCalled();
      }
      expect(api.importPlan).not.toHaveBeenCalled();
      expect(api.importApply).not.toHaveBeenCalled();
      expect(api.saveGame).not.toHaveBeenCalled();
    },
  );
  it.each([true, false])(
    'reviews bundled MTool with default cleanup and opt-out: %s',
    async (keepChecked) => {
      const bundled: ScanCandidate = {
        ...candidate,
        bundled_mtool: [
          {
            folder: '辅助工具',
            bats: ['与工具一同启动.bat', 'StartWithTool.bat'],
            safe: true,
            reason: '已确认',
          },
        ],
      };
      vi.mocked(api.job).mockResolvedValue({ ...page, changes: [bundled] });
      render(<ImportPage {...props()} settings={{ ...settings, mtool_root: 'E:/SharedMTool' }} />);
      fireEvent.click(screen.getByRole('button', { name: '添加文件夹' }));
      await expandSettings('随包 MTool');
      const cleanup = await screen.findByRole('checkbox', { name: '导入成功后清理随包 MTool' });
      expect((cleanup as HTMLInputElement).checked).toBe(true);
      await expandSettings('启动配置');
      expect(
        (screen.getByRole('checkbox', { name: '公共 MTool' }) as HTMLInputElement).checked,
      ).toBe(true);
      await expandSettings('随包 MTool');
      expect(screen.getByText(/StartWithTool.bat/)).toBeTruthy();
      if (!keepChecked) fireEvent.click(cleanup);
      const selection = screen.getByRole('checkbox', {
        name: '选择 Game v1.2',
      }) as HTMLInputElement;
      if (!selection.checked) fireEvent.click(selection);
      fireEvent.click(screen.getByRole('button', { name: '生成导入与更新计划（1）' }));
      await waitFor(() => expect(api.importPlan).toHaveBeenCalledOnce());
      expect(vi.mocked(api.importPlan).mock.calls[0][1][0].cleanup_mtool).toBe(keepChecked);
    },
  );
  it('keeps ambiguous bundles without a cleanup option and explains the reason', async () => {
    vi.mocked(api.job).mockResolvedValue({
      ...page,
      changes: [
        {
          ...candidate,
          bundled_mtool: [
            { folder: 'tool', bats: [], safe: false, reason: '混有游戏资源，保留目录' },
          ],
        },
      ],
    });
    renderNewImport();
    await expandSettings('随包 MTool');
    await screen.findByText('混有游戏资源，保留目录');
    expect(screen.queryByRole('checkbox', { name: '导入成功后清理随包 MTool' })).toBeNull();
  });
  it('shows retained MTool paths and failure reasons in successful import history with retry', async () => {
    vi.mocked(api.importPlans).mockResolvedValue([
      {
        ...plan,
        status: 'completed',
        items: [
          {
            ...plan.items[0],
            state: 'completed',
            registered_id: oldGame.id,
            mtool_cleanup: {
              bundles: [
                { folder: 'tool', bats: ['StartWithTool.bat'], safe: true, reason: '已确认' },
              ],
              targets: [
                { path: 'tool', state: 'failed', error: '工具文件占用，保留文件' },
                { path: 'StartWithTool.bat', state: 'recycled', error: null },
              ],
            },
          },
        ],
      },
    ]);
    render(
      <ImportPage
        {...props()}
        games={[{ ...oldGame, current_version: plan.items[0].selection.version }]}
      />,
    );
    fireEvent.click(await screen.findByRole('tab', { name: /导入记录/ }));
    fireEvent.click(await screen.findByRole('button', { name: '详情' }));
    await screen.findByText('工具文件占用，保留文件');
    expect(screen.getByText('tool · 未清理')).toBeTruthy();
    fireEvent.click(screen.getByRole('button', { name: '重试随包 MTool 清理' }));
    await waitFor(() => expect(api.importApply).toHaveBeenCalledWith(plan.id));
  });
  it.each([
    [['completed', 'copying'], 1, 1],
    [['completed', 'withdrawn', 'rolled_back', 'pending', 'update_copy', 'rollback_restore'], 3, 2],
    [['completed', 'withdrawn', 'rolled_back'], 0, 2],
  ] as const)(
    'counts only actionable items in a partially finished plan: %j',
    async (states, pending, records) => {
      vi.mocked(api.importPlans).mockResolvedValue([
        {
          ...plan,
          status: 'failed',
          items: states.map((state, index) => ({
            ...plan.items[0],
            selection: { ...plan.items[0].selection, source: `D:/Incoming/Game${index}` },
            target: `E:/Library/Game${index}`,
            state,
            error:
              state === 'copying' || state === 'rollback_restore' ? '写入失败，等待恢复' : null,
          })),
        },
        {
          ...plan,
          id: 'finished',
          status: 'completed',
          items: [{ ...plan.items[0], state: 'completed' }],
        },
        {
          ...plan,
          id: 'withdrawn',
          status: 'withdrawn',
          items: [{ ...plan.items[0], state: 'withdrawn' }],
        },
      ]);
      render(<ImportPage {...props()} />);
      await waitFor(() =>
        expect(screen.getByRole('tab', { name: /导入记录/ }).textContent).toBe(
          `导入记录 (${records + 1})`,
        ),
      );
      expect(screen.getByRole('tab', { name: /待确认/ }).textContent).toBe(
        pending ? `待确认 (${pending})` : '待确认',
      );
      fireEvent.click(screen.getByRole('tab', { name: /待确认/ }));
      if (pending > 0) {
        expect(screen.getByRole('button', { name: '继续未完成项' })).toBeTruthy();
        if (!states.some((state) => state.startsWith('rollback_'))) {
          expect(screen.getByRole('button', { name: '撤回未完成项' })).toBeTruthy();
        }
      }
      expect(api.importApply).not.toHaveBeenCalled();
      expect(api.discardImportPlan).not.toHaveBeenCalled();
    },
  );
  it.each([
    [
      '妹系罗盘～在无人岛的悠闲慢活～シスターズコンパス～妹たちと無人島でラブラブスローライフ',
      '姊妹指南针～与姊妹们在无人岛上的甜蜜慢生活～シスターズコンパス～ AI汉化 Ver1.02',
      '日文主标题一致，一方缺少副标题；请确认',
    ],
    ['鸣人的假期 v1.0 内部赞助付费完结版', '鸣人的假期 v1.0', '中文标题一致，附加描述不同；请确认'],
    ['My Best Deal v4.8 女神的最佳交易', 'My_Best_Deal-4.3.5-pc', '英文标题词组一致；请确认'],
    [
      'ReSister ― 与妹妹两人的秘密同居生活 ― v1.1.0 DLC',
      'ReSister-与妹妹的秘密同居生活 v1.03',
      '中文标题相近；请确认 · DLC 标记不同，请确认是否为完整更新包',
    ],
  ])(
    'confirms the recommended existing game for %s before creating an update plan',
    async (title, oldTitle, reason) => {
      const incoming = {
        ...candidate,
        install_path: `D:/Incoming/${title}`,
        suggested_title: title,
      };
      const existing = { ...oldGame, canonical_title: oldTitle, display_title: oldTitle };
      vi.mocked(api.chooseImportSources).mockResolvedValue([incoming.install_path]);
      vi.mocked(api.job).mockResolvedValue({ ...page, changes: [incoming] });
      vi.mocked(api.importMatches).mockResolvedValue({
        [incoming.install_path]: [
          {
            id: 'old',
            title: oldTitle,
            version: 'v1.0',
            path: existing.install_path,
            reason,
            auto_associate: false,
          },
        ],
      });
      renderNewImport({ games: [existing] });
      const associate = await screen.findByRole('button', { name: `关联：${oldTitle}` });
      expect(screen.getByText(reason)).toBeTruthy();
      expect(screen.queryByText('已关联库中游戏')).toBeNull();
      expect(
        (screen.getByRole('checkbox', { name: `选择 ${title}` }) as HTMLInputElement).checked,
      ).toBe(true);
      const generate = screen.getByRole('button', {
        name: '生成导入与更新计划（1）',
      }) as HTMLButtonElement;
      expect(generate.disabled).toBe(true);
      fireEvent.click(associate);
      await expandSettings('游戏关联');
      await screen.findByRole('radiogroup', { name: '存档处理' });
      expect(generate.disabled).toBe(false);
      fireEvent.click(generate);
      await waitFor(() =>
        expect(api.importPlan).toHaveBeenCalledWith('analysis', [
          expect.objectContaining({ existing_id: 'old', source: incoming.install_path }),
        ]),
      );
      expect(api.importApply).not.toHaveBeenCalled();
    },
  );
  it('searches and explicitly links an existing game with a long multilingual title', async () => {
    const title = '阿克梅西亚～后宫怀孕猎人生活～ アクメシア〜ハーレム孕ませ狩人生活〜';
    const existing = {
      ...oldGame,
      display_title: title,
      canonical_title: title,
      install_path: `E:/Library/${title}`,
    };
    renderNewImport({ games: [existing] });
    await expandSettings('游戏关联');
    fireEvent.click(await screen.findByRole('button', { name: '关联已有游戏…' }));
    const modal = screen.getByRole('dialog', { name: '关联已有游戏' });
    fireEvent.change(within(modal).getByRole('textbox', { name: '关联游戏搜索' }), {
      target: { value: '阿克' },
    });
    const option = within(modal).getByRole('button', { name: new RegExp(title.split('～')[0]) });
    expect(within(option).getByText(title).getAttribute('title')).toBe(title);
    fireEvent.click(option);
    expect(screen.queryByRole('dialog', { name: '关联已有游戏' })).toBeNull();
    expect(screen.getByRole('button', { name: `已关联：${title}` })).toBeTruthy();
    expect(
      (screen.getByRole('checkbox', { name: '记住本次关联' }) as HTMLInputElement).checked,
    ).toBe(true);
    expect(api.importApply).not.toHaveBeenCalled();
  });
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
  const oldGame = makeGame({
    id: 'old',
    canonical_title: 'Game',
    display_title: 'Game',
    install_path: 'E:/Library/Game',
    current_version: 'v1.1',
    main_executable: 'game.exe',
    engine: 'Unity',
    created_at: '2026-01-01',
    updated_at: '2026-01-01',
    play_status: 'PLAYING',
    aliases: ['Game'],
    save_paths: ['<GAME>/SaveData'],
  });
  it('shows the confirmed new target folder in update plan summaries, not the old path', async () => {
    const updatePlan: ImportPlan = {
      ...plan,
      items: [
        {
          ...plan.items[0],
          selection: {
            ...plan.items[0].selection,
            existing_id: oldGame.id,
            target_name: '用户确认的新目录 v1.2',
          },
          target: oldGame.install_path,
        },
      ],
    };
    vi.mocked(api.importPlans).mockResolvedValue([updatePlan]);
    render(<ImportPage {...props()} games={[oldGame]} />);
    fireEvent.click(await screen.findByRole('tab', { name: /待确认/ }));
    const target = await screen.findByText('用户确认的新目录 v1.2', {
      selector: '.import-compact-facts > span',
    });
    expect(target.title).toBe('用户确认的新目录 v1.2');
    expect(screen.queryByText('Game', { selector: '.import-compact-facts > span' })).toBeNull();
    expect(api.importApply).not.toHaveBeenCalled();
  });
  it('shrinks resolved issue space without resetting edits or disclosure state', async () => {
    vi.mocked(api.importMatches).mockResolvedValue({
      [candidate.install_path]: [
        {
          id: oldGame.id,
          title: oldGame.display_title,
          reason: '需要确认',
          auto_associate: false,
          version: oldGame.current_version,
          path: oldGame.install_path,
        },
      ],
    });
    vi.mocked(api.job).mockResolvedValue({ ...page, changes: [{ ...candidate, executables: [] }] });
    renderNewImport({ games: [oldGame] });
    await screen.findByText('请手工选择启动文件。');
    const issues = screen
      .getByText('请手工选择启动文件。')
      .closest('.import-card-issues') as HTMLElement;
    expect(issues.style.minHeight).toBe('42px');
    await expandSettings('基本信息');
    fireEvent.change(screen.getByRole('textbox', { name: '版本' }), {
      target: { value: '手工版本' },
    });
    await expandSettings('游戏关联');
    fireEvent.click(screen.getByRole('button', { name: '关联：Game' }));
    expect(issues.style.minHeight).toBe('21px');
    vi.mocked(api.chooseLaunchFile).mockResolvedValue('replacement.exe');
    await expandSettings('启动配置');
    fireEvent.click(screen.getByRole('button', { name: '更改启动文件' }));
    await waitFor(() => expect(screen.queryByText('请手工选择启动文件。')).toBeNull());
    expect(issues.style.minHeight).toBe('21px');
    expect(screen.getByRole('button', { name: '收起设置' }).getAttribute('aria-expanded')).toBe(
      'true',
    );
    await expandSettings('基本信息');
    expect((screen.getByRole('textbox', { name: '版本' }) as HTMLInputElement).value).toBe(
      '手工版本',
    );
  });
  it('starts folded, synchronizes summaries after edits, and submits the same reviewed options', async () => {
    render(<ImportPage {...props()} settings={{ ...settings, mtool_root: 'E:/SharedMTool' }} />);
    fireEvent.click(screen.getByRole('button', { name: '添加文件夹' }));
    const expand = await screen.findByRole('button', { name: '展开设置' });
    expect(expand.getAttribute('aria-expanded')).toBe('false');
    expect(screen.queryByRole('textbox', { name: '游戏名称' })).toBeNull();
    fireEvent.click(expand);
    expect(screen.getByRole('tab', { name: '游戏关联' }).getAttribute('aria-selected')).toBe(
      'false',
    );
    fireEvent.change(screen.getByRole('textbox', { name: '游戏名称' }), {
      target: { value: 'Edited title' },
    });
    fireEvent.change(screen.getByRole('textbox', { name: '目标文件夹' }), {
      target: { value: 'Edited folder' },
    });
    fireEvent.change(screen.getByRole('textbox', { name: '版本' }), { target: { value: 'v2.0' } });
    await expandSettings('启动配置');
    fireEvent.click(screen.getByRole('checkbox', { name: '公共 MTool' }));
    fireEvent.click(screen.getByRole('button', { name: '收起设置' }));
    expect(screen.getByText('Edited title', { selector: 'strong' })).toBeTruthy();
    expect(
      screen.getByText('Edited folder', { selector: '.import-compact-facts > span' }),
    ).toBeTruthy();
    expect(screen.getByText('v2.0', { selector: '.import-compact-facts > span' })).toBeTruthy();
    expect(screen.getByText('公共 MTool 启动')).toBeTruthy();
    expect(screen.queryByRole('checkbox', { name: '公共 MTool' })).toBeNull();
    fireEvent.click(screen.getByRole('button', { name: '生成导入与更新计划（1）' }));
    await waitFor(() =>
      expect(api.importPlan).toHaveBeenCalledWith('analysis', [
        expect.objectContaining({
          title: 'Edited title',
          target_name: 'Edited folder',
          version: 'v2.0',
          mtool: true,
        }),
      ]),
    );
  });

  it('keeps the summary mounted and edits one settings panel at a time without losing changes', async () => {
    render(<ImportPage {...props()} />);
    const categories = screen.getByRole('tablist', { name: '导入分类' });
    expect(categories.closest('.import-toolbar')).toBe(
      screen.getByRole('textbox', { name: '导入搜索' }).closest('.import-toolbar'),
    );
    fireEvent.click(screen.getByRole('button', { name: '添加文件夹' }));
    const expand = await screen.findByRole('button', { name: '展开设置' });
    const facts = screen
      .getByText('game.exe', { selector: '.import-compact-facts > span' })
      .closest('.import-compact-facts');
    fireEvent.click(expand);
    expect(
      screen
        .getByText('game.exe', { selector: '.import-compact-facts > span' })
        .closest('.import-compact-facts'),
    ).toBe(facts);
    fireEvent.change(screen.getByRole('textbox', { name: '游戏名称' }), {
      target: { value: 'Keep this edit' },
    });
    const settingsTabs = screen.getByRole('tablist', { name: '游戏导入设置' });
    const basic = within(settingsTabs).getByRole('tab', { name: '基本信息' });
    fireEvent.keyDown(basic, { key: 'ArrowRight' });
    expect(
      within(settingsTabs).getByRole('tab', { name: '启动配置' }).getAttribute('aria-selected'),
    ).toBe('true');
    expect(document.activeElement).toBe(
      within(settingsTabs).getByRole('tab', { name: '启动配置' }),
    );
    expect(screen.queryByRole('textbox', { name: '游戏名称' })).toBeNull();
    expect(screen.getByRole('checkbox', { name: '公共 MTool' })).toBeTruthy();
    fireEvent.click(within(settingsTabs).getByRole('tab', { name: '游戏关联' }));
    expect(screen.queryByRole('checkbox', { name: '公共 MTool' })).toBeNull();
    expect(screen.getByRole('button', { name: '关联已有游戏…' })).toBeTruthy();
    fireEvent.click(basic);
    expect((screen.getByRole('textbox', { name: '游戏名称' }) as HTMLInputElement).value).toBe(
      'Keep this edit',
    );
    expect(
      screen
        .getAllByRole('tabpanel')
        .filter((panel) => panel.classList.contains('import-settings-panel')),
    ).toHaveLength(1);
    fireEvent.click(screen.getByRole('button', { name: '收起设置' }));
    expect(
      screen
        .getByText('game.exe', { selector: '.import-compact-facts > span' })
        .closest('.import-compact-facts'),
    ).toBe(facts);
    expect(screen.queryByRole('tablist', { name: '游戏导入设置' })).toBeNull();
  });

  it.each(['manual', 'automatic'])(
    'keeps %s association stable, bounds the title and reviews missing saves inline',
    async (mode) => {
      const longTitle = '爱欲之都 第一季 再启S1 v1.1.4.3 City Light sand Love Bites Season1';
      const existing = { ...oldGame, display_title: longTitle, save_paths: [] };
      vi.mocked(api.importMatches).mockResolvedValue({
        [candidate.install_path]: [
          {
            id: existing.id,
            title: longTitle,
            version: existing.current_version,
            path: existing.install_path,
            reason: '主要标题相近',
            auto_associate: mode === 'automatic',
          },
        ],
      });
      renderNewImport({ games: [existing] });
      const link = await screen.findByRole('button', {
        name: (mode === 'manual' ? '关联：' : '已关联：') + longTitle,
      });
      const tabs = screen.getByRole('tablist', { name: '游戏导入设置' });
      const labels = within(tabs)
        .getAllByRole('tab')
        .map((tab) => tab.textContent);
      if (mode === 'manual') fireEvent.click(link);
      const linked = await screen.findByRole('button', { name: '已关联：' + longTitle });
      const remember = screen.getByRole('checkbox', { name: '记住本次关联' }) as HTMLInputElement;
      expect(remember.checked).toBe(true);
      fireEvent.click(remember);
      expect(linked.textContent).toBe('已关联：' + longTitle);
      expect(within(linked).getByTitle(longTitle)).toBeTruthy();
      expect(
        within(tabs)
          .getAllByRole('tab')
          .map((tab) => tab.textContent),
      ).toEqual(labels);
      expect(
        within(tabs).getByRole('tab', { name: '游戏关联' }).getAttribute('aria-selected'),
      ).toBe('true');
      expect(within(tabs).queryByRole('tab', { name: '存档处理' })).toBeNull();
      expect(
        screen.getByRole('radiogroup', { name: '存档处理' }).closest('.import-associated-saves'),
      ).toBeTruthy();
      const capsule = screen.getByText(
        '关联已有游戏：' + Array.from(longTitle).slice(0, 32).join('') + '…',
      );
      expect(capsule.classList.contains('import-choice-text')).toBe(true);
      expect(capsule.closest('.import-linked-game')?.getAttribute('title')).toBe(longTitle);
      expect(screen.getByRole('alert').textContent).toContain('旧游戏未配置存档位置');
      fireEvent.click(screen.getByRole('button', { name: '收起设置' }));
      expect(screen.getByRole('alert').textContent).toContain('旧游戏未配置存档位置');
      await expandSettings('游戏关联');
      fireEvent.click(screen.getByRole('radio', { name: '使用新包存档' }));
      expect(
        (screen.getByRole('checkbox', { name: '记住本次关联' }) as HTMLInputElement).checked,
      ).toBe(false);
      fireEvent.click(screen.getByRole('button', { name: '收起设置' }));
      expect(screen.queryByRole('alert')).toBeNull();
      expect(screen.getByText('已关联所选游戏。')).toBeTruthy();
      expect(
        screen.getByText('使用新包存档', { selector: '.import-choice-summary .mode' }),
      ).toBeTruthy();
    },
  );

  it('opens an association conflict and keeps its warning visible when settings are folded', async () => {
    vi.mocked(api.importMatches).mockResolvedValue({
      [candidate.install_path]: [
        {
          id: oldGame.id,
          title: oldGame.display_title,
          version: 'v1.1',
          path: oldGame.install_path,
          reason: '名称相近',
          auto_associate: false,
        },
      ],
    });
    renderNewImport({ games: [oldGame] });
    await screen.findByRole('button', { name: '关联：Game' });
    expect(screen.getByRole('tab', { name: '游戏关联' }).getAttribute('aria-selected')).toBe(
      'true',
    );
    fireEvent.click(screen.getByRole('button', { name: '收起设置' }));
    expect(screen.getByRole('alert').textContent).toContain('请选择关联');
    expect(screen.getByText('关联待确认')).toBeTruthy();
    expect(
      (screen.getByRole('button', { name: '生成导入与更新计划（1）' }) as HTMLButtonElement)
        .disabled,
    ).toBe(true);
    await expandSettings('游戏关联');
    fireEvent.click(screen.getByRole('button', { name: '改为新游戏' }));
    fireEvent.click(screen.getByRole('button', { name: '收起设置' }));
    expect(screen.queryByRole('alert')).toBeNull();
    expect(screen.getByText('按新游戏导入')).toBeTruthy();
    expect(
      (screen.getByRole('button', { name: '生成导入与更新计划（1）' }) as HTMLButtonElement)
        .disabled,
    ).toBe(false);
  });

  it('exposes invalid MTool configuration while folded and updates its cleanup summary', async () => {
    vi.mocked(api.job).mockResolvedValue({
      ...page,
      changes: [
        {
          ...candidate,
          bundled_mtool: [
            { folder: 'tool', bats: ['StartWithTool.bat'], safe: true, reason: '已确认' },
          ],
        },
      ],
    });
    renderNewImport();
    await expandSettings('随包 MTool');
    await screen.findByRole('checkbox', { name: '导入成功后清理随包 MTool' });
    expect(screen.getByRole('tab', { name: '随包 MTool' }).getAttribute('aria-selected')).toBe(
      'true',
    );
    fireEvent.click(screen.getByRole('button', { name: '收起设置' }));
    expect(screen.getByRole('alert').textContent).toContain('请先配置公共 MTool');
    expect(screen.getByText('随包 MTool：回收清理')).toBeTruthy();
    await expandSettings('随包 MTool');
    fireEvent.click(screen.getByRole('checkbox', { name: '导入成功后清理随包 MTool' }));
    await expandSettings('启动配置');
    fireEvent.click(screen.getByRole('checkbox', { name: '公共 MTool' }));
    fireEvent.click(screen.getByRole('button', { name: '收起设置' }));
    expect(screen.queryByRole('alert')).toBeNull();
    expect(screen.getByText('随包 MTool：保留')).toBeTruthy();
  });

  it('keeps authoritative preview blockers and errors visible after folding', async () => {
    vi.mocked(api.importPlans).mockResolvedValue([
      {
        ...plan,
        items: [
          { ...plan.items[0], blockers: ['启动文件依赖待删除的工具目录'], error: '文件校验失败' },
        ],
      },
    ]);
    render(<ImportPage {...props()} />);
    fireEvent.click(await screen.findByRole('tab', { name: /待确认/ }));
    await screen.findByText('启动文件依赖待删除的工具目录');
    fireEvent.click(screen.getByRole('button', { name: '收起设置' }));
    expect(screen.getAllByRole('alert').map((alert) => alert.textContent)).toEqual(
      expect.arrayContaining(['启动文件依赖待删除的工具目录', '文件校验失败']),
    );
    expect(screen.queryByRole('button', { name: '确认执行计划' })).toBeNull();
    expect(api.importApply).not.toHaveBeenCalled();
  });

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
    renderNewImport({ games: [installed] });
    await screen.findByText('待确认关联');
    expect(screen.queryByRole('button', { name: '删除导入副本…' })).toBeNull();
    fireEvent.click(screen.getByRole('button', { name: '关联：Game' }));
    await expandSettings();
    const version = screen.getByRole('textbox', { name: '版本' });
    fireEvent.change(version, { target: { value: 'v1.3' } });
    expect(screen.queryByRole('button', { name: '删除导入副本…' })).toBeNull();
    fireEvent.change(version, { target: { value: 'v1.2' } });
    await expandSettings('游戏关联');
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
    await waitForEnabledButton('确认移入回收站');
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
    renderNewImport({ games: [oldGame] });
    await expandSettings('游戏关联');
    await screen.findByRole('radiogroup', { name: '存档处理' });
    await expandSettings();
    expect((screen.getByRole('textbox', { name: '目标文件夹' }) as HTMLInputElement).value).toBe(
      'Game v1.2',
    );
    expect((screen.getByRole('textbox', { name: '游戏名称' }) as HTMLInputElement).disabled).toBe(
      false,
    );
    const select = screen.getByRole('checkbox', { name: '选择 Game v1.2' });
    expect((select as HTMLInputElement).checked).toBe(true);
    await expandSettings('游戏关联');
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
    await expandSettings();
    fireEvent.change(screen.getByRole('textbox', { name: '游戏名称' }), {
      target: { value: '新名称 v1.2' },
    });
    fireEvent.change(screen.getByRole('textbox', { name: '目标文件夹' }), {
      target: { value: '新目录 v1.2' },
    });
    fireEvent.click(screen.getByRole('button', { name: '生成导入与更新计划（1）' }));
    await waitFor(() =>
      expect(api.importPlan).toHaveBeenCalledWith('analysis', [
        expect.objectContaining({
          existing_id: 'old',
          title: '新名称 v1.2',
          target_name: '新目录 v1.2',
          preserve_saves: false,
          saves_confirmed: true,
        }),
      ]),
    );
    expect(api.importApply).not.toHaveBeenCalled();
  });
  it.each([
    'inherit',
    'override',
    'automatic',
    'reset',
    'browse',
    'cancel-browse',
    'failed-browse',
  ] as const)(
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
      const options = props();
      renderNewImport({ ...options, games: [installed] });
      await expandSettings('游戏关联');
      await screen.findByRole('radiogroup', { name: '存档处理' });
      await expandSettings('启动配置');
      fireEvent.click(screen.getByRole('checkbox', { name: '公共 MTool' }));
      const launchDetails = screen.getByText('更新启动配置').closest('details')!;
      expect(launchDetails.open).toBe(false);
      fireEvent.click(screen.getByText('更新启动配置'));
      expect(launchDetails.open).toBe(true);
      const working = screen.getByLabelText('工作目录（相对新版游戏目录）') as HTMLInputElement;
      const loader = screen.getByLabelText(
        'MTool loader（相对公共 MTool 目录）',
      ) as HTMLInputElement;
      expect(working.value).toBe('manual-cwd');
      expect(loader.value).toBe('loaders/custom.dll');
      if (['override', 'automatic', 'reset'].includes(mode)) {
        fireEvent.change(working, { target: { value: 'new-cwd' } });
        fireEvent.change(loader, {
          target: { value: mode === 'automatic' ? '' : 'loaders/new.dll' },
        });
      }
      if (mode.includes('browse')) {
        if (mode === 'failed-browse')
          vi.mocked(api.chooseWorkingDirectory).mockRejectedValue('请选择游戏目录内的工作目录');
        else
          vi.mocked(api.chooseWorkingDirectory).mockResolvedValue(
            mode === 'browse' ? '包装/新版' : null,
          );
        fireEvent.click(screen.getByRole('button', { name: '浏览文件夹…' }));
        await waitFor(() =>
          expect(api.chooseWorkingDirectory).toHaveBeenCalledExactlyOnceWith(
            candidate.install_path,
            'manual-cwd',
          ),
        );
        await waitForEnabledButton('浏览文件夹…');
        if (mode === 'failed-browse')
          await waitFor(() =>
            expect(options.onError).toHaveBeenCalledWith('请选择游戏目录内的工作目录'),
          );
        expect(working.value).toBe(mode === 'browse' ? '包装/新版' : 'manual-cwd');
        fireEvent.click(screen.getByText('更新启动配置'));
        fireEvent.click(screen.getByText('更新启动配置'));
        expect(working.value).toBe(mode === 'browse' ? '包装/新版' : 'manual-cwd');
      }
      if (mode === 'reset') {
        fireEvent.click(screen.getByRole('button', { name: '恢复继承旧配置' }));
        expect(working.value).toBe('manual-cwd');
        expect(loader.value).toBe('loaders/custom.dll');
      }
      fireEvent.click(screen.getByRole('button', { name: '生成导入与更新计划（1）' }));
      await waitFor(() => expect(api.importPlan).toHaveBeenCalledOnce());
      const selected = vi.mocked(api.importPlan).mock.calls[0][1][0];
      expect(selected.working_directory).toBe(
        mode === 'browse'
          ? '包装/新版'
          : mode === 'override' || mode === 'automatic'
            ? 'new-cwd'
            : undefined,
      );
      expect(selected.mtool_loader).toBe(
        ['inherit', 'reset', 'browse', 'cancel-browse', 'failed-browse'].includes(mode)
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
      const rendered = renderNewImport({ games: [oldGame] });
      await screen.findByText('待确认关联');
      expect(screen.getByText('发现可能已有的游戏，请确认关联')).toBeTruthy();
      expect(screen.getAllByText(recommendation.reason).length).toBe(ambiguous ? 2 : 1);
      expect(screen.queryByText(`${recommendation.title}（${recommendation.reason}）`)).toBeNull();
      expect(screen.queryByRole('radiogroup', { name: '存档处理' })).toBeNull();
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
        await expandSettings('游戏关联');
        await screen.findByRole('radiogroup', { name: '存档处理' });
        await expandSettings('游戏关联');
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
          {
            ...plan.items[0],
            selection: { ...plan.items[0].selection, title: 'Unfinished', source: 'D:/Unfinished' },
          },
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
    await expandSettings();
    await screen.findByRole('textbox', { name: '游戏名称' });
    const viewport = () => screen.getByRole('tabpanel', { name: /新的导入/ });
    fireEvent.scroll(viewport(), { target: { scrollTop: 500 } });
    rerender(<ImportPage {...inputProps} visible={false} />);
    rerender(<ImportPage {...inputProps} visible />);
    expect(viewport().scrollTop).toBe(500);
    fireEvent.change(screen.getByRole('textbox', { name: '导入搜索' }), {
      target: { value: 'Game' },
    });
    expect(viewport().scrollTop).toBe(0);
  });
  it.each(['whole', 'children'] as const)(
    'selects wrapper scope %s with one click and keeps cancellation read-only',
    async (scope) => {
      const root = 'D:/Wrapped Game';
      vi.mocked(api.chooseImportSources).mockResolvedValue([root]);
      vi.mocked(api.discoverImportSources).mockResolvedValue({
        sources: [],
        choices: [{ root, children: [`${root}/bin`] }],
        warnings: [],
      });
      renderNewImport();
      let modal = await screen.findByRole('dialog', { name: '确认文件夹范围' });
      expect(
        (within(modal).getByRole('button', { name: '开始分析' }) as HTMLButtonElement).disabled,
      ).toBe(true);
      expect(api.importAnalyze).not.toHaveBeenCalled();
      expect(within(modal).queryByRole('combobox')).toBeNull();
      expect(
        within(modal)
          .getAllByRole('radio')
          .every((radio) => !(radio as HTMLInputElement).checked),
      ).toBe(true);
      fireEvent.click(within(modal).getByRole('button', { name: '取消' }));
      expect(api.importAnalyze).not.toHaveBeenCalled();
      fireEvent.click(screen.getByRole('button', { name: '添加文件夹' }));
      modal = await screen.findByRole('dialog', { name: '确认文件夹范围' });
      const option = within(modal).getByRole('radio', {
        name: scope === 'whole' ? '整个文件夹作为一个游戏' : '分别导入里面的 1 个游戏',
      });
      fireEvent.click(option.closest('label')!);
      expect((option as HTMLInputElement).checked).toBe(true);
      expect(option.closest('label')?.classList.contains('selected')).toBe(true);
      expect(api.importAnalyze).not.toHaveBeenCalled();
      fireEvent.click(within(modal).getByRole('button', { name: '开始分析' }));
      await waitFor(() =>
        expect(api.importAnalyze).toHaveBeenCalledWith([scope === 'whole' ? root : `${root}/bin`]),
      );
      expect(api.importApply).not.toHaveBeenCalled();
    },
  );
  it('preserves existing edits and an unchecked choice when adding more folders', async () => {
    renderNewImport();
    await expandSettings();
    fireEvent.change(await screen.findByRole('textbox', { name: '游戏名称' }), {
      target: { value: 'Kept Edit' },
    });
    fireEvent.click(screen.getByRole('checkbox', { name: '选择 Kept Edit' }));
    vi.mocked(api.chooseImportSources).mockResolvedValue(['D:/Incoming/Another']);
    vi.mocked(api.importAnalyze).mockResolvedValue('analysis-again');
    vi.mocked(api.job).mockResolvedValue({
      ...page,
      id: 'analysis-again',
      changes: [
        candidate,
        {
          ...candidate,
          install_path: 'D:/Incoming/Another',
          suggested_title: 'Another',
          executables: [],
          status: 'needs_manual',
        },
      ],
    });
    fireEvent.click(screen.getByRole('button', { name: '添加文件夹' }));
    expect(
      ((await screen.findByRole('checkbox', { name: '选择 Another' })) as HTMLInputElement).checked,
    ).toBe(true);
    await waitFor(() =>
      expect(api.importAnalyze).toHaveBeenLastCalledWith([
        candidate.install_path,
        'D:/Incoming/Another',
      ]),
    );
    await waitForEnabledButton('添加文件夹');
    expect((screen.getByRole('textbox', { name: '游戏名称' }) as HTMLInputElement).value).toBe(
      'Kept Edit',
    );
    expect(
      (screen.getByRole('checkbox', { name: '选择 Kept Edit' }) as HTMLInputElement).checked,
    ).toBe(false);
  });
  it('moves drafts into pending, restores edits on return and confirms with only two buttons', async () => {
    vi.mocked(api.discardImportPlan).mockResolvedValue(undefined);
    const options = props();
    render(<ImportPage {...options} />);
    fireEvent.click(screen.getByRole('button', { name: '添加文件夹' }));
    await expandSettings();
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
    expect(screen.queryByRole('textbox', { name: '游戏名称' })).toBeNull();
    expect(screen.queryByRole('checkbox', { name: '选择 Edited Game' })).toBeNull();
    expect(screen.getByRole('tab', { name: '新的导入' })).toBeTruthy();
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
    fireEvent.click(screen.getByRole('button', { name: '返回调整' }));
    await screen.findByRole('checkbox', { name: '选择 Edited Game' });
    await expandSettings();
    expect((screen.getByRole('textbox', { name: '游戏名称' }) as HTMLInputElement).value).toBe(
      'Edited Game',
    );
    expect(
      (screen.getByRole('checkbox', { name: '选择 Edited Game' }) as HTMLInputElement).checked,
    ).toBe(true);
    expect(api.discardImportPlan).toHaveBeenCalledWith('plan');
  });
  it.each(['completed', 'failed'] as const)(
    'keeps unplanned drafts and routes an %s apply to its category',
    async (outcome) => {
      const other = { ...candidate, install_path: 'D:/Other', suggested_title: 'Other Game' };
      let savedPlans: ImportPlan[] = [];
      vi.mocked(api.importPlans).mockImplementation(async () => savedPlans);
      vi.mocked(api.importPlan).mockImplementation(async () => {
        savedPlans = [plan];
        return 'plan';
      });
      vi.mocked(api.importApply).mockImplementation(async () => {
        savedPlans = [
          {
            ...plan,
            status: outcome,
            items: [
              {
                ...plan.items[0],
                state: outcome === 'completed' ? 'completed' : 'copying',
                registered_id: outcome === 'completed' ? 'new-game' : null,
                error: outcome === 'failed' ? 'copy failed' : null,
              },
            ],
          },
        ];
        return 'apply';
      });
      vi.mocked(api.job).mockImplementation(async (id) =>
        id === 'analysis'
          ? {
              ...page,
              changes: [candidate, other],
              total: 2,
              processed: 2,
              next_cursor: 2,
              change_count: 2,
            }
          : {
              ...page,
              id,
              kind: id === 'plan' ? 'import_plan' : 'import_apply',
              status: id === 'plan' ? 'completed' : outcome,
              changes: [],
              next_cursor: 0,
              change_count: 0,
              error: outcome === 'failed' && id === 'apply' ? 'copy failed' : null,
            },
      );
      const options = props();
      render(<ImportPage {...options} />);
      fireEvent.click(screen.getByRole('button', { name: '添加文件夹' }));
      const unplanned = await screen.findByRole('checkbox', { name: '选择 Other Game' });
      fireEvent.click(unplanned);
      fireEvent.click(screen.getByRole('button', { name: '生成导入与更新计划（1）' }));
      await screen.findByRole('button', { name: '确认执行计划' });
      fireEvent.click(screen.getByRole('tab', { name: '新的导入 (1)' }));
      expect(screen.queryByRole('checkbox', { name: '选择 Game v1.2' })).toBeNull();
      expect(
        (screen.getByRole('checkbox', { name: '选择 Other Game' }) as HTMLInputElement).checked,
      ).toBe(false);
      fireEvent.click(screen.getByRole('tab', { name: '待确认 (1)' }));
      fireEvent.click(screen.getByRole('button', { name: '确认执行计划' }));
      fireEvent.click(
        within(screen.getByRole('dialog', { name: '确认导入与更新' })).getByRole('button', {
          name: '确认',
        }),
      );
      if (outcome === 'completed') {
        await waitFor(() =>
          expect(
            screen.getByRole('tab', { name: '导入记录 (1)' }).getAttribute('aria-selected'),
          ).toBe('true'),
        );
        expect(screen.getByRole('article', { name: 'Game v1.2' })).toBeTruthy();
        expect(screen.getByRole('tab', { name: '待确认' })).toBeTruthy();
      } else {
        await waitFor(() => expect(options.onError).toHaveBeenCalledWith('copy failed'));
        expect(screen.getByRole('tab', { name: '待确认 (1)' }).getAttribute('aria-selected')).toBe(
          'true',
        );
        expect(screen.getByRole('tab', { name: '导入记录' })).toBeTruthy();
      }
      fireEvent.click(screen.getByRole('tab', { name: '新的导入 (1)' }));
      expect(screen.queryByRole('checkbox', { name: '选择 Game v1.2' })).toBeNull();
      expect(
        (screen.getByRole('checkbox', { name: '选择 Other Game' }) as HTMLInputElement).checked,
      ).toBe(false);
    },
  );
  it('retains selected drafts when generating the plan fails', async () => {
    const options = props();
    vi.mocked(api.importPlan).mockRejectedValue(new Error('plan failed'));
    render(<ImportPage {...options} />);
    fireEvent.click(screen.getByRole('button', { name: '添加文件夹' }));
    await screen.findByRole('checkbox', { name: '选择 Game v1.2' });
    fireEvent.click(screen.getByRole('button', { name: '生成导入与更新计划（1）' }));
    await waitFor(() => expect(options.onError).toHaveBeenCalledWith('Error: plan failed'));
    expect(
      (screen.getByRole('checkbox', { name: '选择 Game v1.2' }) as HTMLInputElement).checked,
    ).toBe(true);
    expect(screen.getByRole('tab', { name: '新的导入 (1)' }).getAttribute('aria-selected')).toBe(
      'true',
    );
    expect(screen.getByRole('tab', { name: '待确认' })).toBeTruthy();
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
    renderNewImport();
    const file = await screen.findByRole('combobox', { name: 'QSP 主游戏文件' });
    expect((file as HTMLInputElement).value).toBe('');
    expect((screen.getByRole('combobox', { name: 'QSP 播放器' }) as HTMLInputElement).value).toBe(
      '',
    );
    expect(
      (screen.getByRole('checkbox', { name: '选择 Game v1.2' }) as HTMLInputElement).checked,
    ).toBe(true);
    expect(screen.queryByLabelText('公共 MTool')).toBeNull();
    expect(
      (screen.getByRole('button', { name: '生成导入与更新计划（1）' }) as HTMLButtonElement)
        .disabled,
    ).toBe(true);
    expect(api.importPlan).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole('button', { name: '更改 QSP 文件…' }));
    await waitFor(() => expect((file as HTMLInputElement).value).toBe('mod.qsp'));
    expect(api.choosePlayerFile).toHaveBeenCalledWith(candidate.install_path, 'qsp');
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
    renderNewImport();
    await expandSettings('启动配置');
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
    await expandSettings();
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
  it('keeps matched existing games checked and allows explicit new-game override', async () => {
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
    renderNewImport();
    await expandSettings('游戏关联');
    await screen.findByRole('button', { name: '已关联：Game' });
    const checkbox = screen.getByRole('checkbox', { name: '选择 Game v1.2' }) as HTMLInputElement;
    expect(checkbox.checked).toBe(true);
    fireEvent.click(screen.getByRole('button', { name: '改为新游戏' }));
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
