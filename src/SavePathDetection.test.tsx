import { useState } from 'react';
import { act, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { beforeEach, expect, it, vi } from 'vitest';
import { api } from './api';
import { SavePathDetection } from './SavePathDetection';
import { makeGame } from './testing/fixtures';
import { resetApiMock } from './testing/apiMock';
import type { SaveDetectionRequest } from './types';

vi.mock('./api', async (original) => {
  const { api } = await original<typeof import('./api')>();
  const { createApiMock } = await import('./testing/apiMock');
  return { api: createApiMock(api) };
});
const game = makeGame({
  install_path: 'D:/Games/Fixture',
  save_paths: ['<GAME>\\save', 'D:/Manual'],
});
const request: SaveDetectionRequest = {
  expected_install_path: game.install_path,
  engine: game.engine,
  working_directory: '.',
  executable: null,
};
const changed = vi.fn();
function Harness({
  config = request,
  locked = false,
}: {
  config?: SaveDetectionRequest;
  locked?: boolean;
}) {
  const [value, setValue] = useState(game.save_paths.join('\n'));
  return (
    <>
      <SavePathDetection
        game={game}
        request={config}
        value={value}
        onChange={(value) => {
          changed(value);
          setValue(value);
        }}
        locked={locked}
        active
      />
      <label>
        手动路径
        <textarea value={value} onChange={(event) => setValue(event.target.value)} />
      </label>
    </>
  );
}
function click(name: string) {
  fireEvent.click(screen.getByRole('button', { name }));
}
async function preview() {
  click('重新检测');
  return screen.findByRole('checkbox', { name: '<GAME>/SaveData' });
}
beforeEach(() => {
  resetApiMock(api);
  changed.mockReset();
  vi.mocked(api.previewSavePaths).mockResolvedValue(['<GAME>/save', '<GAME>/SaveData']);
});

it('previews without writing, then adds selected paths to the draft and deduplicates Windows spellings', async () => {
  render(<Harness />);
  const checkbox = await preview();
  expect(changed).not.toHaveBeenCalled();
  expect(api.saveGame).not.toHaveBeenCalled();
  expect((screen.getByRole('button', { name: '应用到草稿' }) as HTMLButtonElement).disabled).toBe(
    true,
  );
  expect((screen.getByRole('radio', { name: '新增到现有路径' }) as HTMLInputElement).checked).toBe(
    true,
  );
  expect(screen.getByText('已关联')).toBeTruthy();
  fireEvent.click(checkbox);
  fireEvent.click(screen.getByRole('checkbox', { name: '<GAME>/save' }));
  click('应用到草稿');
  await screen.findByText('已填入存档路径草稿，请保存资料。');
  expect(changed).toHaveBeenCalledWith('<GAME>\\save\nD:/Manual\n<GAME>/SaveData');
  expect(api.previewSavePaths).toHaveBeenCalledTimes(2);
  expect(api.previewSavePaths).toHaveBeenCalledWith(game.id, request);
  expect(api.saveGame).not.toHaveBeenCalled();
});

it('requires explicit replacement and previews removed associations before updating the draft', async () => {
  render(<Harness />);
  fireEvent.click(await preview());
  fireEvent.click(screen.getByRole('radio', { name: '替换全部关联路径' }));
  expect(screen.getByText(/取消 2 项关联/)).toBeTruthy();
  expect(screen.getAllByText('将取消关联')).toHaveLength(2);
  expect(changed).not.toHaveBeenCalled();
  click('应用到草稿');
  await waitFor(() => expect(changed).toHaveBeenCalledWith('<GAME>/SaveData'));
});

it('preserves the exact existing manual text when adding an already-associated candidate', async () => {
  render(<Harness />);
  const manual = '  D:/Games/Fixture/SAVE  \nD:/Manual\nD:/MANUAL\n';
  fireEvent.change(screen.getByRole('textbox', { name: '手动路径' }), {
    target: { value: manual },
  });
  await preview();
  fireEvent.click(screen.getByRole('checkbox', { name: '<GAME>/save' }));
  click('应用到草稿');
  await screen.findByText('所选路径已关联，无需修改。');
  expect(changed).toHaveBeenCalledWith(manual);
});

it('keeps manual paths on no candidates, detection failure, or cancellation', async () => {
  render(<Harness />);
  vi.mocked(api.previewSavePaths).mockResolvedValueOnce([]);
  click('重新检测');
  await screen.findByText(/未检测到可用路径/);
  expect(screen.queryByRole('button', { name: '应用到草稿' })).toBeNull();
  click('取消检测');
  vi.mocked(api.previewSavePaths).mockRejectedValueOnce('目录不可访问');
  click('重新检测');
  await screen.findByRole('alert');
  expect(screen.getByRole('alert').textContent).toBe('目录不可访问');
  click('取消检测');
  fireEvent.click(await preview());
  fireEvent.click(screen.getByRole('radio', { name: '替换全部关联路径' }));
  click('取消检测');
  expect(changed).not.toHaveBeenCalled();
  expect((screen.getByRole('textbox', { name: '手动路径' }) as HTMLTextAreaElement).value).toBe(
    game.save_paths.join('\n'),
  );
});

it('rechecks vanished candidates and failed application without changing the draft', async () => {
  render(<Harness />);
  fireEvent.click(await preview());
  vi.mocked(api.previewSavePaths).mockRejectedValueOnce('文件任务进行中');
  click('应用到草稿');
  await screen.findByText('文件任务进行中');
  expect(changed).not.toHaveBeenCalled();
  vi.mocked(api.previewSavePaths).mockResolvedValueOnce(['<GAME>/save']);
  click('应用到草稿');
  await screen.findByText(/部分候选路径已失效/);
  expect(screen.queryByRole('checkbox', { name: '<GAME>/SaveData' })).toBeNull();
  expect(changed).not.toHaveBeenCalled();
});

it('ignores cancelled and out-of-order requests', async () => {
  let finish!: (paths: string[]) => void;
  vi.mocked(api.previewSavePaths).mockImplementationOnce(
    () =>
      new Promise((resolve) => {
        finish = resolve;
      }),
  );
  render(<Harness />);
  click('重新检测');
  await screen.findByText('正在检测存档路径…');
  click('取消检测');
  await preview();
  await act(async () => finish(['<GAME>/late']));
  expect(screen.queryByRole('checkbox', { name: '<GAME>/late' })).toBeNull();
  expect(screen.getByRole('checkbox', { name: '<GAME>/SaveData' })).toBeTruthy();
  expect(changed).not.toHaveBeenCalled();
});

it.each(['configuration', 'manual paths', 'task lock'] as const)(
  'invalidates in-flight results when %s changes',
  async (change) => {
    let finish!: (paths: string[]) => void;
    vi.mocked(api.previewSavePaths).mockImplementationOnce(
      () =>
        new Promise((resolve) => {
          finish = resolve;
        }),
    );
    const view = render(<Harness />);
    click('重新检测');
    if (change === 'configuration')
      view.rerender(<Harness config={{ ...request, engine: 'Unity' }} />);
    else if (change === 'task lock') view.rerender(<Harness locked />);
    else
      fireEvent.change(screen.getByRole('textbox', { name: '手动路径' }), {
        target: { value: 'D:/NewManual' },
      });
    await act(async () => finish(['<GAME>/stale']));
    expect(screen.queryByRole('checkbox')).toBeNull();
    expect(changed).not.toHaveBeenCalled();
    if (change === 'task lock')
      expect((screen.getByRole('button', { name: '重新检测' }) as HTMLButtonElement).disabled).toBe(
        true,
      );
  },
);

it('discards a ready preview after manual edits and resets replacement on the next detection', async () => {
  render(<Harness />);
  await preview();
  fireEvent.click(screen.getByRole('radio', { name: '替换全部关联路径' }));
  fireEvent.change(screen.getByRole('textbox', { name: '手动路径' }), {
    target: { value: 'D:/NewManual' },
  });
  expect(screen.queryByRole('region', { name: '存档路径检测预览' })).toBeNull();
  await preview();
  expect((screen.getByRole('radio', { name: '新增到现有路径' }) as HTMLInputElement).checked).toBe(
    true,
  );
  expect(
    within(screen.getByRole('region', { name: '存档路径检测预览' })).getByText('D:/NewManual'),
  ).toBeTruthy();
});

it('does not overwrite a new manual edit when an earlier application finishes', async () => {
  render(<Harness />);
  fireEvent.click(await preview());
  let finish!: (paths: string[]) => void;
  vi.mocked(api.previewSavePaths).mockImplementationOnce(
    () =>
      new Promise((resolve) => {
        finish = resolve;
      }),
  );
  click('应用到草稿');
  fireEvent.change(screen.getByRole('textbox', { name: '手动路径' }), {
    target: { value: 'D:/NewManual' },
  });
  await act(async () => finish(['<GAME>/SaveData']));
  expect(changed).not.toHaveBeenCalled();
  expect((screen.getByRole('textbox', { name: '手动路径' }) as HTMLTextAreaElement).value).toBe(
    'D:/NewManual',
  );
});

it('discards a pending application when the detail is closed', async () => {
  const view = render(<Harness />);
  fireEvent.click(await preview());
  let finish!: (paths: string[]) => void;
  vi.mocked(api.previewSavePaths).mockImplementationOnce(
    () =>
      new Promise((resolve) => {
        finish = resolve;
      }),
  );
  click('应用到草稿');
  view.unmount();
  await act(async () => finish(['<GAME>/SaveData']));
  expect(changed).not.toHaveBeenCalled();
  expect(api.saveGame).not.toHaveBeenCalled();
});
