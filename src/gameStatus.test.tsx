import React from 'react';
import { act, fireEvent, render, renderHook, screen, within } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import {
  EngineQuickFilters,
  engineQuickGroups,
  LibraryFilters,
  emptyFilters,
  matchesFilters,
} from './gameStatus';
import { useLibraryLayout, useSavedSort } from './preferences';
import type { Game } from './types';

export const statusGame: Game = {
  id: 'g',
  canonical_title: 'Game',
  display_title: 'Game',
  install_path: 'D:/Games/Game',
  working_directory: '.',
  current_version: 'Unknown',
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
};
describe('library status and preferences', () => {
  it('ranks seven engine types by total counts, keeping all remaining types and Unknown in Other', () => {
    const games = [
      'Unity',
      'Unity',
      "Ren'Py",
      'HTML',
      'Godot',
      'QSP',
      'KiriKiri',
      'RPG Maker MV',
      'RPG Maker MZ',
      ...Array<string>(8).fill('Unknown'),
    ].map((engine, index) => ({ ...statusGame, id: String(index), engine }));
    const { popular, otherCount } = engineQuickGroups(games);
    expect(popular[0]).toEqual({ engine: 'Unity', count: 2 });
    expect(popular).toHaveLength(7);
    expect(popular.some((group) => group.engine === 'Unknown')).toBe(false);
    expect(otherCount).toBe(9);
    expect(engineQuickGroups([...games].reverse())).toEqual({ popular, otherCount });
    const onChange = vi.fn();
    const { rerender } = render(
      <EngineQuickFilters games={games} engines={[]} other={false} onChange={onChange} />,
    );
    const buttons = screen.getAllByRole('button');
    expect(buttons[0].textContent).toBe('全部17');
    expect(buttons[1].textContent).toBe('Unity2');
    expect(buttons.at(-1)?.textContent).toBe('其他9');
    fireEvent.click(buttons.at(-1)!);
    expect(onChange).toHaveBeenLastCalledWith([], true);
    const changed = [
      ...games,
      ...Array.from({ length: 4 }, (_, index) => ({
        ...statusGame,
        id: `new-${index}`,
        engine: 'QSP',
      })),
    ];
    rerender(<EngineQuickFilters games={changed} engines={[]} other={true} onChange={onChange} />);
    expect(screen.getAllByRole('button')[1].textContent).toBe('QSP5');
    expect(screen.getAllByRole('button').at(-1)?.getAttribute('aria-pressed')).toBe('true');
    fireEvent.click(screen.getByRole('button', { name: 'QSP5' }));
    expect(onChange).toHaveBeenLastCalledWith(['QSP'], false);
    fireEvent.click(screen.getByRole('button', { name: '全部21' }));
    expect(onChange).toHaveBeenLastCalledWith([], false);
  });
  it('keeps an empty Other capsule last and does not mark a multi-engine filter as a single selection', () => {
    render(
      <EngineQuickFilters
        games={[statusGame]}
        engines={['Unity', 'QSP']}
        other={false}
        onChange={vi.fn()}
      />,
    );
    expect(
      screen.getAllByRole('button').map((button) => button.getAttribute('aria-pressed')),
    ).toEqual(['false', 'false', 'false']);
    expect((screen.getByRole('button', { name: '其他0' }) as HTMLButtonElement).disabled).toBe(
      true,
    );
    expect(engineQuickGroups([])).toEqual({ popular: [], otherCount: 0 });
  });
  it('combines groups with AND and alternatives within each group with OR', () => {
    const filters = {
      statuses: ['PLAYING', 'COMPLETED'] as const,
      engines: ['Unity', 'QSP'],
      modes: ['MTool'],
    };
    expect(
      matchesFilters(
        { ...statusGame, play_status: 'PLAYING', launch_type: 'MTOOL' },
        { ...filters, statuses: [...filters.statuses] },
      ),
    ).toBe(true);
    expect(
      matchesFilters(
        { ...statusGame, play_status: 'COMPLETED', launch_type: 'MTOOL' },
        { ...filters, statuses: [...filters.statuses] },
      ),
    ).toBe(true);
    expect(
      matchesFilters(
        { ...statusGame, play_status: 'PLAYING' },
        { ...filters, statuses: [...filters.statuses] },
      ),
    ).toBe(false);
    expect(
      matchesFilters(
        { ...statusGame, play_status: 'PLAYING', engine: 'HTML', launch_type: 'MTOOL' },
        { ...filters, statuses: [...filters.statuses] },
      ),
    ).toBe(false);
  });
  it('shows active filters and clears them without changing anything else', () => {
    const { result } = renderHook(() => {
      const [value, setValue] = React.useState(emptyFilters);
      return { value, setValue };
    });
    const { rerender } = render(
      <LibraryFilters
        games={[
          statusGame,
          { ...statusGame, id: 'unknown', engine: 'Unknown' },
          { ...statusGame, id: 'kiri', engine: 'KiriKiri' },
          { ...statusGame, id: 'wolf', engine: 'WOLF RPG Editor' },
        ]}
        value={result.current.value}
        onChange={result.current.setValue}
      />,
    );
    fireEvent.click(screen.getByRole('button', { name: '筛选' }));
    const dialog = screen.getByRole('dialog', { name: '筛选游戏' });
    const engines = within(within(dialog).getByRole('group', { name: '游戏引擎' })).getAllByRole(
      'checkbox',
    );
    expect(engines.map((input) => input.closest('label')?.textContent)).toEqual([
      'KiriKiri',
      'Unity',
      'WOLF RPG Editor',
      '未识别',
    ]);
    fireEvent.click(within(dialog).getByRole('checkbox', { name: '正在玩' }));
    expect(result.current.value).toEqual(emptyFilters());
    fireEvent.click(within(dialog).getByRole('button', { name: '完成' }));
    rerender(
      <LibraryFilters
        games={[statusGame]}
        value={result.current.value}
        onChange={result.current.setValue}
      />,
    );
    expect(screen.getByRole('button', { name: '筛选 (1)' }).getAttribute('aria-pressed')).toBe(
      'true',
    );
    fireEvent.click(screen.getByRole('button', { name: '筛选 (1)' }));
    const reopened = screen.getByRole('dialog', { name: '筛选游戏' });
    expect(
      (within(reopened).getByRole('checkbox', { name: '正在玩' }) as HTMLInputElement).checked,
    ).toBe(true);
    fireEvent.click(within(reopened).getByRole('button', { name: '清空筛选' }));
    expect(result.current.value.statuses).toEqual(['PLAYING']);
    fireEvent.click(within(reopened).getByRole('button', { name: '完成' }));
    expect(result.current.value).toEqual(emptyFilters());
  });
  it.each(['取消', '关闭弹窗', 'Escape'])(
    'discards draft changes with %s and restores applied filters when reopened',
    (action) => {
      const onChange = vi.fn();
      const value = { statuses: ['COMPLETED' as const], engines: ['Unity'], modes: [] };
      render(<LibraryFilters games={[statusGame]} value={value} onChange={onChange} />);
      fireEvent.click(screen.getByRole('button', { name: '筛选 (2)' }));
      const dialog = screen.getByRole('dialog', { name: '筛选游戏' });
      fireEvent.click(within(dialog).getByRole('button', { name: '清空筛选' }));
      fireEvent.click(within(dialog).getByRole('checkbox', { name: '正在玩' }));
      if (action === 'Escape')
        fireEvent(dialog, new Event('cancel', { bubbles: false, cancelable: true }));
      else fireEvent.click(within(dialog).getByRole('button', { name: action }));
      expect(onChange).not.toHaveBeenCalled();
      expect(screen.queryByRole('dialog')).toBeNull();
      fireEvent.click(screen.getByRole('button', { name: '筛选 (2)' }));
      const reopened = screen.getByRole('dialog', { name: '筛选游戏' });
      expect(
        (within(reopened).getByRole('checkbox', { name: '已通关' }) as HTMLInputElement).checked,
      ).toBe(true);
      expect(
        (within(reopened).getByRole('checkbox', { name: 'Unity' }) as HTMLInputElement).checked,
      ).toBe(true);
      expect(
        (within(reopened).getByRole('checkbox', { name: '正在玩' }) as HTMLInputElement).checked,
      ).toBe(false);
    },
  );
  it('restores each module sort independently and rejects unknown saved values', () => {
    const allowed = ['name-asc', 'name-desc'] as const;
    const hook = renderHook(() => useSavedSort('library', 'name-asc', allowed));
    act(() => hook.result.current[1]('name-desc'));
    hook.unmount();
    expect(renderHook(() => useSavedSort('library', 'name-asc', allowed)).result.current[0]).toBe(
      'name-desc',
    );
    expect(renderHook(() => useSavedSort('scan', 'name-asc', allowed)).result.current[0]).toBe(
      'name-asc',
    );
    localStorage.setItem('butter-manager.sort.import', 'invalid');
    expect(renderHook(() => useSavedSort('import', 'name-asc', allowed)).result.current[0]).toBe(
      'name-asc',
    );
  });
  it('remembers library layout across restarts and falls back for invalid storage', () => {
    const hook = renderHook(useLibraryLayout);
    expect(hook.result.current[0]).toBe('list');
    act(() => hook.result.current[1]('grid'));
    hook.unmount();
    const reopened = renderHook(useLibraryLayout);
    expect(reopened.result.current[0]).toBe('grid');
    act(() => reopened.result.current[1]('list'));
    reopened.unmount();
    expect(renderHook(useLibraryLayout).result.current[0]).toBe('list');
    localStorage.setItem('butter-manager.view.library', 'invalid');
    expect(renderHook(useLibraryLayout).result.current[0]).toBe('list');
  });
});
