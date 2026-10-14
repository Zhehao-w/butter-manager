import { beforeEach, expect, it, vi } from 'vitest';
import { act, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { LibraryRelocation } from './LibraryRelocation';
import { api } from './api';
import { makeGame, makeSettings } from './testing/fixtures';
import { resetApiMock } from './testing/apiMock';
import { waitForEnabledButton } from './testing/interactions';
import type { LibraryRelocationPreview } from './types';

vi.mock('./api', async (original) => {
  const { api } = await original<typeof import('./api')>();
  const { createApiMock } = await import('./testing/apiMock');
  return { api: createApiMock(api) };
});
const plan: LibraryRelocationPreview = {
  token: 'preview-token',
  old_root: 'E:/Butter',
  new_root: 'D:/Games/Butter',
  items: [
    {
      id: 'valid',
      title: '有效游戏',
      old_path: 'E:/Butter/Game',
      new_path: 'D:/Games/Butter/Game',
      reason: null,
    },
    {
      id: 'invalid',
      title: '缺失游戏',
      old_path: 'E:/Butter/Missing',
      new_path: 'D:/Games/Butter/Missing',
      reason: '主启动文件缺失',
    },
  ],
  mtool: {
    old_path: 'E:/Butter/任意工具',
    new_path: 'D:/Games/Butter/任意工具',
    state: 'blocked',
    reason: '注入器缺失；MTool 设置不变，有效游戏仍可迁移',
  },
};
const onMigrated = vi.fn();
const onBusy = vi.fn();
beforeEach(() => {
  resetApiMock(api);
  onMigrated.mockReset();
  onBusy.mockReset();
  vi.mocked(api.chooseDirectory).mockResolvedValue(plan.new_root);
  vi.mocked(api.previewLibraryRelocation).mockResolvedValue(plan);
});
async function preview(expected = '可迁移 1 个游戏，无法迁移 1 个游戏。') {
  render(
    <LibraryRelocation
      root={plan.old_root}
      locked={false}
      onBusy={onBusy}
      onMigrated={onMigrated}
    />,
  );
  fireEvent.click(screen.getByRole('button', { name: '选择新根目录…' }));
  await waitForEnabledButton('预览迁移');
  fireEvent.click(screen.getByRole('button', { name: '预览迁移' }));
  await screen.findByText(expected);
}
it.each(['empty', 'all-skipped'] as const)('blocks zero-match migration: %s', async (mode) => {
  const items =
    mode === 'empty' ? [] : plan.items.map((item) => ({ ...item, reason: '启动文件缺失' }));
  vi.mocked(api.previewLibraryRelocation).mockResolvedValue({
    ...plan,
    items,
    mtool: { ...plan.mtool, state: 'ready' },
  });
  await preview(`可迁移 0 个游戏，无法迁移 ${items.length} 个游戏。`);
  expect(screen.getByRole('alert').textContent).toBe('没有可迁移的游戏，请检查新旧根目录');
  const confirm = screen.getByRole('button', { name: '确认迁移' });
  expect((confirm as HTMLButtonElement).disabled).toBe(true);
  fireEvent.click(confirm);
  expect(screen.queryByRole('dialog', { name: '确认迁移游戏库路径？' })).toBeNull();
  expect(api.applyLibraryRelocation).not.toHaveBeenCalled();
  expect(onMigrated).not.toHaveBeenCalled();
});
it('previews exact roots, keeps partial failures explicit and cancels without applying', async () => {
  await preview();
  expect(api.previewLibraryRelocation).toHaveBeenCalledWith(
    plan.old_root,
    plan.new_root,
    expect.any(Function),
  );
  expect(screen.getByText(plan.mtool.reason)).toBeTruthy();
  fireEvent.click(screen.getByRole('button', { name: '确认迁移' }));
  const modal = screen.getByRole('dialog', { name: '确认迁移游戏库路径？' });
  expect(within(modal).getByText(/不移动任何文件/)).toBeTruthy();
  fireEvent.click(within(modal).getByRole('button', { name: '取消' }));
  expect(api.applyLibraryRelocation).not.toHaveBeenCalled();
  fireEvent.change(screen.getByRole('textbox', { name: '旧游戏库根目录' }), {
    target: { value: 'Z:/Missing' },
  });
  expect(screen.queryByRole('button', { name: '确认迁移' })).toBeNull();
});
it.each(['success', 'stale'] as const)(
  'applies only the server preview and handles %s',
  async (outcome) => {
    await preview();
    const report = {
      migrated: 1,
      skipped: 1,
      mtool_migrated: false,
      settings: makeSettings({ game_root: plan.new_root }),
      games: [makeGame({ install_path: 'D:/Games/Butter/Game' })],
    };
    if (outcome === 'success') vi.mocked(api.applyLibraryRelocation).mockResolvedValue(report);
    else vi.mocked(api.applyLibraryRelocation).mockRejectedValue('数据库已变化，请重新预览迁移');
    fireEvent.click(screen.getByRole('button', { name: '确认迁移' }));
    const modal = screen.getByRole('dialog', { name: '确认迁移游戏库路径？' });
    fireEvent.click(within(modal).getByRole('button', { name: '确认迁移' }));
    await waitFor(() =>
      expect(api.applyLibraryRelocation).toHaveBeenCalledExactlyOnceWith(plan.token),
    );
    if (outcome === 'success') {
      await screen.findByText('已迁移 1 个游戏，跳过 1 个。公共 MTool 设置未变。');
      expect(onMigrated).toHaveBeenCalledWith(report);
    } else {
      await screen.findByRole('alert');
      expect(onMigrated).not.toHaveBeenCalled();
    }
    expect(screen.queryByRole('button', { name: '确认迁移' })).toBeNull();
    expect(api.register).not.toHaveBeenCalled();
    expect(api.startScan).not.toHaveBeenCalled();
    expect(api.saveSettings).not.toHaveBeenCalled();
  },
);
it('blocks maintenance while other work or unsaved settings lock it', () => {
  render(<LibraryRelocation root={plan.old_root} locked onBusy={onBusy} onMigrated={onMigrated} />);
  expect(
    (screen.getByRole('button', { name: '选择新根目录…' }) as HTMLButtonElement).closest('fieldset')
      ?.disabled,
  ).toBe(true);
  expect(api.previewLibraryRelocation).not.toHaveBeenCalled();
  expect(screen.queryByRole('dialog')).toBeNull();
});
it('shows pending preview feedback and lazily displays readable filtered results', async () => {
  let finish!: (value: LibraryRelocationPreview) => void;
  vi.mocked(api.previewLibraryRelocation).mockReturnValue(
    new Promise((resolve) => {
      finish = resolve;
    }),
  );
  render(
    <LibraryRelocation
      root={plan.old_root}
      locked={false}
      onBusy={onBusy}
      onMigrated={onMigrated}
    />,
  );
  const section = screen.getByRole('region', { name: '批量迁移游戏库路径' });
  expect(section.className).toBe('library-relocation');
  expect(screen.getByRole('textbox', { name: '旧游戏库根目录' })).toBeTruthy();
  expect(screen.queryByRole('dialog')).toBeNull();
  fireEvent.click(screen.getByRole('button', { name: '选择新根目录…' }));
  await waitForEnabledButton('预览迁移');
  fireEvent.click(screen.getByRole('button', { name: '预览迁移' }));
  const popup = screen.getByRole('dialog', { name: '路径迁移预览' });
  expect(within(popup).queryByRole('textbox', { name: '旧游戏库根目录' })).toBeNull();
  await within(popup).findByRole('progressbar', { name: '迁移预览进度' });
  const progress = vi.mocked(api.previewLibraryRelocation).mock.calls[0][2]!;
  act(() => progress({ processed: 1, total: 3, current: '缺失游戏' }));
  expect(within(popup).getByText('已检查 1 / 3 项')).toBeTruthy();
  expect(within(popup).getByRole('progressbar').getAttribute('aria-valuenow')).toBe('1');
  expect(screen.queryByText('正在检查目标目录与启动配置，请稍候…')).toBeNull();
  expect(screen.getByRole('button', { name: '正在预览…' })).toBeTruthy();
  finish(plan);
  await screen.findByText('可迁移 1 个游戏，无法迁移 1 个游戏。');
  expect(screen.queryByText('有效游戏 · 可迁移')).toBeNull();
  fireEvent.click(within(popup).getByRole('button', { name: /查看逐项结果/ }));
  expect(popup.classList.contains('has-results')).toBe(true);
  await screen.findByText('有效游戏 · 可迁移');
  fireEvent.click(screen.getByRole('button', { name: '无法迁移（1）' }));
  expect(screen.queryByText('有效游戏 · 可迁移')).toBeNull();
  expect(screen.getByText('主启动文件缺失')).toBeTruthy();
  expect(screen.getByText(`新：${plan.items[1].new_path}`).getAttribute('title')).toBe(
    plan.items[1].new_path,
  );
  expect(
    within(popup).getByRole('button', { name: '确认迁移' }).classList.contains('migration-confirm'),
  ).toBe(true);
  fireEvent.click(within(popup).getByRole('button', { name: '关闭并清除预览' }));
  expect(screen.queryByRole('dialog')).toBeNull();
  expect(screen.queryByText(/可迁移 1 个游戏/)).toBeNull();
  expect((screen.getByRole('textbox', { name: '旧游戏库根目录' }) as HTMLInputElement).value).toBe(
    plan.old_root,
  );
  expect((screen.getByRole('textbox', { name: '新游戏库根目录' }) as HTMLInputElement).value).toBe(
    plan.new_root,
  );
  expect(api.applyLibraryRelocation).not.toHaveBeenCalled();
});
