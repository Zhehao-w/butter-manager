import { beforeEach, describe, expect, it, vi } from 'vitest';
import { fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { api } from './api';
import { LibraryDuplicates } from './LibraryDuplicates';
import type { DuplicatePair, Game, DuplicateReview, JobPage } from './types';

vi.mock('./api', () => ({
  api: {
    startDuplicateCheck: vi.fn(),
    duplicateResults: vi.fn(),
    duplicateGameReferences: vi.fn().mockResolvedValue([]),
    duplicateReviews: vi.fn(),
    setDuplicateReview: vi.fn(),
    job: vi.fn(),
    cancel: vi.fn(),
    previewDelete: vi.fn(),
    deleteFiles: vi.fn(),
    openFolder: vi.fn().mockResolvedValue(undefined),
  },
}));
const games: Game[] = ['a', 'b'].map((id, index) => ({
  id,
  canonical_title: 'Title',
  display_title: `Title v1.${index * 2}`,
  install_path: index ? 'Z:/new' : 'Z:/old',
  current_version: `v1.${index * 2}`,
  working_directory: '.',
  version_source: 'manual',
  main_executable: 'Game.exe',
  engine: 'Unity',
  launch_type: 'DIRECT',
  mtool_target_exe: null,
  mtool_loader: null,
  created_at: '',
  updated_at: '',
  last_launched_at: null,
  play_status: 'UNPLAYED',
  aliases: [],
  save_paths: [],
}));
const pair: DuplicatePair = {
  left_id: 'a',
  right_id: 'b',
  score: 95,
  confidence: 'high',
  reason: '去除明确版本后名称一致',
};
const finished: JobPage = {
  id: 'duplicates',
  kind: 'duplicates',
  root: '',
  status: 'completed',
  phase: '匹配',
  total: 2,
  processed: 2,
  active: {},
  elapsed_ms: 5,
  idle_ms: 0,
  changes: [],
  next_cursor: 0,
  change_count: 0,
  warnings: [],
  error: null,
  registered_ids: [],
};
let exclusions: DuplicateReview[];
beforeEach(() => {
  vi.clearAllMocks();
  exclusions = [];
  vi.mocked(api.startDuplicateCheck).mockResolvedValue('duplicates');
  vi.mocked(api.duplicateResults).mockResolvedValue([pair]);
  vi.mocked(api.job).mockResolvedValue(finished);
  vi.mocked(api.duplicateReviews).mockImplementation(async () => exclusions);
  vi.mocked(api.setDuplicateReview).mockImplementation(async (left_id, right_id, value) => {
    exclusions =
      value === 'pending' ? [] : [{ left_id, right_id, created_at: 'now', status: value }];
  });
  vi.mocked(api.previewDelete).mockImplementation(async (id) => ({
    id,
    token: `token-${id}`,
    title: games.find((game) => game.id === id)!.display_title,
    game_path: games.find((game) => game.id === id)!.install_path,
    saves: [],
    blockers: [],
  }));
  vi.mocked(api.deleteFiles).mockResolvedValue({
    removed: true,
    recycled: ['Z:/new'],
    error: null,
  });
});
const properties = () => ({
  games,
  visible: true,
  locked: false,
  tabs: null,
  onActive: vi.fn(),
  onOpen: vi.fn(),
  onError: vi.fn(),
  onDeleted: vi.fn(),
});

describe('library duplicate review', () => {
  it('checks only on request, opens both details and supports ignore and undo without checking again', async () => {
    const props = properties();
    render(<LibraryDuplicates {...props} />);
    expect(api.startDuplicateCheck).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole('button', { name: '查找重复' }));
    await screen.findByText('去除明确版本后名称一致');
    expect(screen.getByText('Z:/old')).toBeTruthy();
    expect(screen.getByText('Z:/new')).toBeTruthy();
    const details = screen.getAllByRole('button', { name: '查看详情' });
    details.forEach((button) => fireEvent.click(button));
    expect(props.onOpen.mock.calls.map((call) => call[0])).toEqual(['a', 'b']);
    fireEvent.click(screen.getAllByRole('button', { name: '打开目录' })[0]);
    expect(api.openFolder).toHaveBeenCalledWith('a');
    const reviewRequests = vi.mocked(api.duplicateReviews).mock.calls.length;
    vi.mocked(api.duplicateReviews).mockRejectedValue(new Error('refresh failed'));
    fireEvent.click(screen.getByRole('tab', { name: '待确认 1' }));
    fireEvent.click(screen.getByRole('button', { name: '不是同一游戏' }));
    await screen.findByText('没有待确认的组合');
    fireEvent.click(screen.getByRole('tab', { name: '已排除 1' }));
    fireEvent.click(screen.getByRole('button', { name: '撤销排除' }));
    await screen.findByText('没有已排除的组合');
    fireEvent.click(screen.getByRole('tab', { name: '待确认 1' }));
    expect(screen.getByText('Title v1.2')).toBeTruthy();
    expect(screen.getByText('Title v1.0')).toBeTruthy();
    expect(api.startDuplicateCheck).toHaveBeenCalledTimes(1);
    expect(api.duplicateReviews).toHaveBeenCalledTimes(reviewRequests);
    expect(props.onError).not.toHaveBeenCalled();
  });
  it('loads saved ignores after reopening and can undo them before a new check', async () => {
    exclusions = [{ left_id: 'a', right_id: 'b', created_at: 'now', status: 'excluded' }];
    render(<LibraryDuplicates {...properties()} />);
    fireEvent.click(await screen.findByRole('tab', { name: '已排除 1' }));
    expect(screen.getByText('Title v1.2')).toBeTruthy();
    fireEvent.click(screen.getByRole('button', { name: '撤销排除' }));
    await screen.findByText('没有已排除的组合');
    fireEvent.click(screen.getByRole('tab', { name: '待确认 1' }));
    expect(screen.getByText('Title v1.2')).toBeTruthy();
    expect(api.startDuplicateCheck).not.toHaveBeenCalled();
  });
  it('preserves filters and results across tabs and preserves the last complete results on cancellation', async () => {
    const props = properties();
    const view = render(<LibraryDuplicates {...props} />);
    fireEvent.click(screen.getByRole('button', { name: '查找重复' }));
    await screen.findByText('Title v1.0');
    fireEvent.change(screen.getByRole('textbox', { name: '搜索重复组合' }), {
      target: { value: 'Title' },
    });
    const region = screen.getByRole('region', { name: '重复游戏组合' });
    region.scrollTop = 120;
    fireEvent.scroll(region);
    view.rerender(<LibraryDuplicates {...props} visible={false} />);
    view.rerender(<LibraryDuplicates {...props} />);
    expect((screen.getByRole('textbox', { name: '搜索重复组合' }) as HTMLInputElement).value).toBe(
      'Title',
    );
    expect(region.scrollTop).toBe(120);
    vi.mocked(api.startDuplicateCheck).mockResolvedValue('second');
    let page: JobPage = { ...finished, id: 'second', status: 'running' };
    vi.mocked(api.job).mockImplementation(async () => page);
    vi.mocked(api.cancel).mockImplementation(async () => {
      page = { ...page, status: 'cancelled' };
    });
    fireEvent.click(screen.getByRole('button', { name: '重新检查' }));
    fireEvent.click(await screen.findByRole('button', { name: '取消检查' }));
    await screen.findByText('检查已取消，之前的结果保留');
    expect(screen.getByText('Title v1.2')).toBeTruthy();
    expect(api.duplicateResults).toHaveBeenCalledTimes(1);
  });
  it.each(['不是同一游戏', '确认重复'])(
    'keeps a suggestion when saving %s fails',
    async (action) => {
      const props = properties();
      render(<LibraryDuplicates {...props} />);
      fireEvent.click(screen.getByRole('button', { name: '查找重复' }));
      await screen.findByText('Title v1.0');
      vi.mocked(api.setDuplicateReview).mockRejectedValue(new Error('database locked'));
      fireEvent.click(screen.getByRole('button', { name: action }));
      await waitFor(() => expect(props.onError).toHaveBeenCalledWith('Error: database locked'));
      expect(screen.getByText('Title v1.2')).toBeTruthy();
      expect(screen.getByRole('tab', { name: '待确认 1' })).toBeTruthy();
    },
  );

  it('updates all in place, moves confirmed pairs out of pending immediately, and supports undo', async () => {
    render(<LibraryDuplicates {...properties()} />);
    fireEvent.click(screen.getByRole('button', { name: '查找重复' }));
    await screen.findByText('Title v1.0');
    fireEvent.click(screen.getByRole('button', { name: '确认重复' }));
    await screen.findByRole('button', { name: '删除选中游戏…' });
    expect(api.deleteFiles).not.toHaveBeenCalled();
    expect(screen.getByRole('tab', { name: '待确认 0' })).toBeTruthy();
    expect(screen.getByRole('tab', { name: '已确认 1' })).toBeTruthy();
    expect(screen.getByRole('tab', { name: '全部 1' }).getAttribute('aria-selected')).toBe('true');
    expect(screen.getByText('已确认重复')).toBeTruthy();
    expect(screen.queryByText('已确认待处理')).toBeNull();
    expect(screen.queryByRole('button', { name: '暂不处理' })).toBeNull();
    fireEvent.click(screen.getByRole('tab', { name: '待确认 0' }));
    await screen.findByText('没有待确认的组合');
    fireEvent.click(screen.getByRole('tab', { name: '已确认 1' }));
    fireEvent.click(screen.getByRole('button', { name: '撤销确认' }));
    await screen.findByText('没有已确认的组合');
    fireEvent.click(screen.getByRole('tab', { name: '待确认 1' }));
    expect(screen.getByRole('button', { name: '确认重复' })).toBeTruthy();
    fireEvent.click(screen.getByRole('button', { name: '确认重复' }));
    await screen.findByText('没有待确认的组合');
    expect(screen.queryByRole('button', { name: '删除选中游戏…' })).toBeNull();
    fireEvent.click(screen.getByRole('tab', { name: '全部 1' }));
    expect(screen.getByRole('button', { name: '删除选中游戏…' })).toBeTruthy();
  });

  it('loads persisted confirmation after restarting, even before checking', async () => {
    exclusions = [{ left_id: 'b', right_id: 'a', created_at: 'now', status: 'confirmed' }];
    render(<LibraryDuplicates {...properties()} />);
    fireEvent.click(await screen.findByRole('tab', { name: '已确认 1' }));
    expect(screen.getByText('Title v1.2')).toBeTruthy();
    expect(screen.getByRole('button', { name: '删除选中游戏…' })).toBeTruthy();
    expect(api.startDuplicateCheck).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole('button', { name: '撤销确认' }));
    await screen.findByRole('tab', { name: '待确认 1' });
    fireEvent.click(screen.getByRole('tab', { name: '待确认 1' }));
    expect(screen.getByRole('button', { name: '确认重复' })).toBeTruthy();
  });

  it.each([
    ['b', 'a'],
    ['a', 'b'],
  ])(
    'reuses the original deletion dialog for %s and only removes the selected UUID',
    async (deletedId, keptId) => {
      const props = properties();
      const view = render(<LibraryDuplicates {...props} />);
      fireEvent.click(screen.getByRole('button', { name: '查找重复' }));
      await screen.findByText('Title v1.0');
      fireEvent.click(screen.getByRole('button', { name: '确认重复' }));
      await screen.findByRole('radiogroup', { name: '选择要删除的游戏' });
      fireEvent.click(
        screen.getByRole('radio', {
          name: '选择删除：' + games.find((game) => game.id === deletedId)!.display_title,
        }),
      );
      fireEvent.click(screen.getByRole('button', { name: '删除选中游戏…' }));
      const dialog = screen.getByRole('dialog');
      expect(
        within(dialog).getByText(games.find((game) => game.id === deletedId)!.install_path),
      ).toBeTruthy();
      expect(
        within(dialog).getByText(games.find((game) => game.id === keptId)!.install_path),
      ).toBeTruthy();
      expect(within(dialog).getByText('保留：')).toBeTruthy();
      expect(api.deleteFiles).not.toHaveBeenCalled();
      await waitFor(() =>
        expect(
          (within(dialog).getByRole('button', { name: '确认移入回收站' }) as HTMLButtonElement)
            .disabled,
        ).toBe(false),
      );
      fireEvent.click(within(dialog).getByRole('button', { name: '确认移入回收站' }));
      await screen.findByRole('dialog', { name: '文件操作结果' });
      fireEvent.click(screen.getByRole('button', { name: '完成' }));
      await waitFor(() =>
        expect(props.onDeleted).toHaveBeenCalledWith(
          deletedId,
          expect.objectContaining({ removed: true }),
        ),
      );
      expect(api.previewDelete).toHaveBeenCalledWith(deletedId);
      expect(api.deleteFiles).toHaveBeenCalledExactlyOnceWith(deletedId, `token-${deletedId}`);
      expect(api.setDuplicateReview).toHaveBeenCalledTimes(1);
      view.rerender(
        <LibraryDuplicates {...props} games={games.filter((game) => game.id !== deletedId)} />,
      );
      expect(screen.getByRole('tab', { name: '已确认 0' })).toBeTruthy();
      expect(screen.queryByRole('button', { name: '删除选中游戏…' })).toBeNull();
    },
  );

  it('retains confirmed state and both game references on a partial deletion failure', async () => {
    const props = properties();
    vi.mocked(api.deleteFiles).mockResolvedValue({
      removed: false,
      recycled: ['Z:/new/save'],
      error: '文件被占用，库记录保留',
    });
    render(<LibraryDuplicates {...props} />);
    fireEvent.click(screen.getByRole('button', { name: '查找重复' }));
    await screen.findByText('Title v1.0');
    fireEvent.click(screen.getByRole('button', { name: '确认重复' }));
    fireEvent.click(await screen.findByRole('radio', { name: '选择删除：Title v1.2' }));
    fireEvent.click(screen.getByRole('button', { name: '删除选中游戏…' }));
    await waitFor(() =>
      expect(
        (screen.getByRole('button', { name: '确认移入回收站' }) as HTMLButtonElement).disabled,
      ).toBe(false),
    );
    fireEvent.click(screen.getByRole('button', { name: '确认移入回收站' }));
    await screen.findByText('文件被占用，库记录保留');
    fireEvent.click(screen.getByRole('button', { name: '完成' }));
    expect(screen.getByText('Title v1.0')).toBeTruthy();
    expect(screen.getByText('Title v1.2')).toBeTruthy();
    expect(screen.getByRole('tab', { name: '已确认 1' })).toBeTruthy();
    expect(api.setDuplicateReview).toHaveBeenCalledTimes(1);
    expect(props.onDeleted).toHaveBeenCalledWith('b', expect.objectContaining({ removed: false }));
  });

  it('blocks deletion using the existing preflight, allows cancellation and preserves confirmation', async () => {
    vi.mocked(api.previewDelete).mockResolvedValue({
      id: 'b',
      token: 'blocked',
      title: 'Title v1.2',
      game_path: 'Z:/new',
      saves: [],
      blockers: ['此路径包含共用存档'],
    });
    render(<LibraryDuplicates {...properties()} />);
    fireEvent.click(screen.getByRole('button', { name: '查找重复' }));
    await screen.findByText('Title v1.0');
    fireEvent.click(screen.getByRole('button', { name: '确认重复' }));
    fireEvent.click(await screen.findByRole('radio', { name: '选择删除：Title v1.2' }));
    fireEvent.click(screen.getByRole('button', { name: '删除选中游戏…' }));
    await screen.findByText('此路径包含共用存档');
    expect(
      (screen.getByRole('button', { name: '确认移入回收站' }) as HTMLButtonElement).disabled,
    ).toBe(true);
    fireEvent.click(screen.getByRole('button', { name: '取消' }));
    expect(api.deleteFiles).not.toHaveBeenCalled();
    expect(screen.getByRole('tab', { name: '已确认 1' })).toBeTruthy();
  });
  it('sorts comparisons by score, hides unknown metadata and exposes complete titles and paths', async () => {
    const third = {
      ...games[1],
      id: 'c',
      display_title: '第三份很长的游戏名称',
      install_path: 'Z:/third',
      engine: 'Unknown',
      current_version: 'Unknown',
    };
    vi.mocked(api.duplicateResults).mockResolvedValue([
      { ...pair, right_id: 'c', score: 70, confidence: 'review', reason: '相近名称' },
      pair,
    ]);
    render(<LibraryDuplicates {...properties()} games={[...games, third]} />);
    fireEvent.click(screen.getByRole('button', { name: '查找重复' }));
    await screen.findByText('相近名称');
    const cards = screen.getAllByRole('article');
    expect(within(cards[0]).getByText('待确认')).toBeTruthy();
    expect(within(cards[0]).getByText(pair.reason).getAttribute('title')).toContain('95');
    expect(within(cards[1]).queryByText(/Unknown/)).toBeNull();
    expect(screen.getByText(third.display_title).getAttribute('title')).toBe(third.display_title);
    expect(screen.getByText(third.install_path).getAttribute('title')).toBe(third.install_path);
  });
  it('requires selecting one card, keeps detail clicks separate, and displays directory dates and MTool mode', async () => {
    const props = properties();
    vi.mocked(api.duplicateGameReferences).mockResolvedValue([
      { id: 'a', directory_modified_ms: Date.UTC(2026, 0, 1, 12) },
      { id: 'b', directory_modified_ms: null },
    ]);
    render(
      <LibraryDuplicates
        {...props}
        games={[games[0], { ...games[1], launch_type: 'MTOOL', mtool_target_exe: 'Game.exe' }]}
      />,
    );
    fireEvent.click(screen.getByRole('button', { name: '查找重复' }));
    await screen.findByText('Title v1.0');
    await waitFor(() => expect(screen.queryByText('目录修改：读取中…')).toBeNull());
    expect(api.duplicateGameReferences).toHaveBeenCalledExactlyOnceWith(['a', 'b']);
    expect(screen.getByText('MTool')).toBeTruthy();
    expect(screen.getByText('直接启动')).toBeTruthy();
    expect(screen.getAllByText(/目录修改：/).length).toBe(2);
    fireEvent.click(screen.getByRole('button', { name: '确认重复' }));
    const remove = await screen.findByRole('button', { name: '删除选中游戏…' });
    expect((remove as HTMLButtonElement).disabled).toBe(true);
    fireEvent.click(screen.getAllByRole('button', { name: '查看详情' })[0]);
    expect(props.onOpen).toHaveBeenCalledWith('a');
    expect((remove as HTMLButtonElement).disabled).toBe(true);
    vi.mocked(api.openFolder).mockRejectedValueOnce(new Error('目录不存在'));
    fireEvent.click(screen.getAllByRole('button', { name: '打开目录' })[1]);
    await waitFor(() => expect(props.onError).toHaveBeenCalledWith('Error: 目录不存在'));
    expect(api.openFolder).toHaveBeenLastCalledWith('b');
    expect((remove as HTMLButtonElement).disabled).toBe(true);
    fireEvent.click(screen.getByText('Title v1.0'));
    expect(
      (screen.getByRole('radio', { name: '选择删除：Title v1.0' }) as HTMLInputElement).checked,
    ).toBe(true);
    expect((remove as HTMLButtonElement).disabled).toBe(false);
    expect(screen.getByText('移入回收站')).toBeTruthy();
    expect(screen.getByText('保留')).toBeTruthy();
    expect(
      screen.getByText('Title v1.0').closest('.duplicate-game')?.classList.contains('is-selected'),
    ).toBe(true);
    expect(
      screen.getByText('Title v1.2').closest('.duplicate-game')?.classList.contains('is-kept'),
    ).toBe(true);
    fireEvent.click(screen.getByRole('radio', { name: '选择删除：Title v1.2' }));
    expect(
      (screen.getByRole('radio', { name: '选择删除：Title v1.0' }) as HTMLInputElement).checked,
    ).toBe(false);
    expect(api.previewDelete).not.toHaveBeenCalled();
    expect(api.duplicateGameReferences).toHaveBeenCalledTimes(1);
  });
});
