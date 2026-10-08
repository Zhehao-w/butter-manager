import { fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { DeleteGameDialog } from './DeleteGameDialog';
import { api } from './api';
import type { Game, DeletePlan } from './types';
vi.mock('./api', () => ({ api: { previewDelete: vi.fn(), deleteFiles: vi.fn() } }));
const game = {
  id: 'g',
  display_title: 'Game',
  install_path: 'D:/Game',
  save_paths: ['save'],
} as Game;
const plan: DeletePlan = {
  token: 'preview',
  id: 'g',
  title: 'Game',
  game_path: 'D:/Game',
  saves: [{ path: 'D:/Game/save', action: 'recycle' }],
  blockers: [],
};
describe('delete confirmation', () => {
  beforeEach(() => vi.clearAllMocks());
  it('previews game and associated saves without deleting and requires explicit confirmation', async () => {
    vi.mocked(api.previewDelete).mockResolvedValue(plan);
    const report = {
      removed: true,
      recycled: ['D:/Game'],
      error: null,
    };
    vi.mocked(api.deleteFiles).mockResolvedValue(report);
    const onDeleted = vi.fn();
    render(
      <DeleteGameDialog game={game} onClose={vi.fn()} onDeleted={onDeleted} onBusy={vi.fn()} />,
    );
    await screen.findByText('移入回收站');
    expect(screen.queryByRole('checkbox')).toBeNull();
    expect(api.previewDelete).toHaveBeenCalledWith(game.id);
    expect(api.deleteFiles).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole('button', { name: '确认移入回收站' }));
    await screen.findByRole('dialog', { name: '文件操作结果' });
    expect(api.deleteFiles).toHaveBeenCalledWith('g', 'preview');
    expect(onDeleted).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole('button', { name: '完成' }));
    expect(onDeleted).toHaveBeenCalledWith(report);
  });
  it('blocks unsafe associated save plans without allowing save preservation', async () => {
    vi.mocked(api.previewDelete).mockResolvedValue({ ...plan, blockers: ['共用存档目录'] });
    render(<DeleteGameDialog game={game} onClose={vi.fn()} onDeleted={vi.fn()} onBusy={vi.fn()} />);
    await screen.findByText('共用存档目录');
    expect(
      (screen.getByRole('button', { name: '确认移入回收站' }) as HTMLButtonElement).disabled,
    ).toBe(true);
    expect(screen.queryByRole('checkbox')).toBeNull();
    expect(api.previewDelete).toHaveBeenCalledWith(game.id);
    expect(api.deleteFiles).not.toHaveBeenCalled();
  });
  it('retains partial results and never reports a failed deletion as removal', async () => {
    vi.mocked(api.previewDelete).mockResolvedValue(plan);
    vi.mocked(api.deleteFiles).mockResolvedValue({
      removed: false,
      recycled: ['D:/Saves/slot'],
      error: '游戏文件被占用，库记录保留',
    });
    const onDeleted = vi.fn();
    render(
      <DeleteGameDialog game={game} onClose={vi.fn()} onDeleted={onDeleted} onBusy={vi.fn()} />,
    );
    await waitFor(() =>
      expect(
        (screen.getByRole('button', { name: '确认移入回收站' }) as HTMLButtonElement).disabled,
      ).toBe(false),
    );
    fireEvent.click(screen.getByRole('button', { name: '确认移入回收站' }));
    const dialog = await screen.findByRole('dialog', { name: '文件操作结果' });
    expect(within(dialog).getByText('游戏文件被占用，库记录保留')).toBeTruthy();
    fireEvent.click(within(dialog).getByRole('button', { name: '完成' }));
    expect(onDeleted).toHaveBeenCalledWith(expect.objectContaining({ removed: false }));
  });
});
