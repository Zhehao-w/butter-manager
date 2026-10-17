import { act, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { beforeEach, expect, it, vi } from 'vitest';
import { api } from './api';
import { ImportMemories } from './ImportMemories';
import { resetApiMock } from './testing/apiMock';
import type { ImportMemory } from './types';

vi.mock('./api', async (original) => {
  const { api } = await original<typeof import('./api')>();
  const { createApiMock } = await import('./testing/apiMock');
  return { api: createApiMock(api) };
});
const memory: ImportMemory = {
  id: 'memory',
  game_id: 'game',
  confirmed_at: '2026-10-17T00:00:00Z',
  profile: {
    format_version: 1,
    title: 'Moonlight Garden v0.8 汉化版',
    name_key: 'synthetic',
    identities: ['unity:fixture:moonlight garden'],
  },
};
beforeEach(() => resetApiMock(api));

async function expand(container: HTMLElement) {
  const details = container.querySelector('details')!;
  await act(async () => {
    details.open = true;
    fireEvent(details, new Event('toggle'));
  });
}

it('loads only when expanded, preserves a failed forget and removes only after success', async () => {
  vi.mocked(api.importMemories).mockResolvedValue([memory]);
  vi.mocked(api.forgetImportMemory)
    .mockRejectedValueOnce(new Error('当前任务进行中'))
    .mockResolvedValueOnce();
  const view = render(<ImportMemories gameId="game" disabled={false} />);
  expect(api.importMemories).not.toHaveBeenCalled();
  await expand(view.container);
  await screen.findByText(memory.profile.title);
  fireEvent.click(screen.getByRole('button', { name: `忘记关联：${memory.profile.title}` }));
  expect((await screen.findByRole('alert')).textContent).toContain('当前任务进行中');
  expect(screen.getByText(memory.profile.title)).toBeTruthy();
  fireEvent.click(screen.getByRole('button', { name: `忘记关联：${memory.profile.title}` }));
  await screen.findByText('尚未记住任何来源。');
  expect(api.forgetImportMemory).toHaveBeenLastCalledWith('game', 'memory');
  expect(api.saveGame).not.toHaveBeenCalled();
});

it('can retry a failed load and prevents forgetting while the detail is locked', async () => {
  vi.mocked(api.importMemories)
    .mockRejectedValueOnce(new Error('读取失败'))
    .mockResolvedValueOnce([memory]);
  const view = render(<ImportMemories gameId="game" disabled={false} />);
  await expand(view.container);
  expect((await screen.findByRole('alert')).textContent).toContain('读取失败');
  fireEvent.click(screen.getByRole('button', { name: '刷新' }));
  const forget = await screen.findByRole('button', { name: `忘记关联：${memory.profile.title}` });
  view.rerender(<ImportMemories gameId="game" disabled />);
  await waitFor(() => expect((forget as HTMLButtonElement).disabled).toBe(true));
  fireEvent.click(forget);
  expect(api.forgetImportMemory).not.toHaveBeenCalled();
});
