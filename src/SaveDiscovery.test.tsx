import { act, cleanup, fireEvent, render, screen, within } from '@testing-library/react';
import { afterEach, expect, it, vi } from 'vitest';
import { api } from './api';
import { SaveDiscovery } from './SaveDiscovery';
import type { SaveHint } from './types';

vi.mock('./api', () => ({ api: { startSaveWatch: vi.fn(), finishSaveWatch: vi.fn() } }));
afterEach(() => {
  cleanup();
  vi.resetAllMocks();
});
const hint = (path: string, file: string): SaveHint => ({
  path,
  files: [file],
  can_add: true,
  reason: '记录期间有 1 个文件写入',
});
const props = { gameId: 'test', unity: true, locations: [], hints: [], onAdd: vi.fn() };
it('ranks a directory touched in both captures first and adds only the selected location', async () => {
  vi.mocked(api.startSaveWatch).mockResolvedValue({ token: 't', roots: ['%APPDATA%'] });
  const path = '%APPDATA%/Project42';
  const file1 = hint(path + '/001.bin', '001.bin');
  const file2 = hint(path + '/002.bin', '002.bin');
  vi.mocked(api.finishSaveWatch)
    .mockResolvedValueOnce({ candidates: [file1], partial: false })
    .mockResolvedValueOnce({
      candidates: [hint('<GAME>/Misc', 'other.bin'), file2],
      partial: false,
    });
  render(<SaveDiscovery {...props} hints={[{ ...file2, reason: '产品名称一致' }]} />);
  for (const start of ['开始定位', '再定位一次']) {
    fireEvent.click(await screen.findByRole('button', { name: start }));
    fireEvent.click(await screen.findByRole('button', { name: '完成定位' }));
  }
  await screen.findByText('两次均有写入 ·');
  const first = screen.getAllByRole('listitem')[0];
  expect(within(first).getByText(file2.path)).toBeTruthy();
  expect(screen.getAllByText(file2.path)).toHaveLength(1);
  expect(within(first).getByText(/产品名称一致 · 记录期间/)).toBeTruthy();
  expect(props.onAdd).not.toHaveBeenCalled();
  fireEvent.click(within(first).getByRole('button'));
  expect(props.onAdd).toHaveBeenCalledWith(file2.path);
});
it('releases the native session when a pending start completes after the detail closed', async () => {
  let started!: (r: { token: string; roots: string[] }) => void;
  vi.mocked(api.startSaveWatch).mockReturnValue(
    new Promise((r) => {
      started = r;
    }),
  );
  vi.mocked(api.finishSaveWatch).mockResolvedValue(null);
  const view = render(<SaveDiscovery {...props} />);
  fireEvent.click(screen.getByRole('button', { name: '开始定位' }));
  view.unmount();
  await act(async () => started({ token: 'late', roots: [] }));
  expect(api.finishSaveWatch).toHaveBeenCalledWith('late', true);
});
