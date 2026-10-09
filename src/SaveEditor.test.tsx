import { fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { SaveEditor } from './SaveEditor';
import { api } from './api';
import type { Game } from './types';
import type { SaveDocument, SaveField } from './saveEditorTypes';

vi.mock('./api', () => ({
  api: {
    listEditableSaves: vi.fn(),
    readEditableSave: vi.fn(),
    applySaveEdits: vi.fn(),
  },
}));
const game = { id: 'game', display_title: '游戏', play_status: 'PLAYING' } as Game;
const field = (
  id: string,
  value: SaveField['value'],
  kind: SaveField['kind'],
  category = 'variables',
): SaveField => ({
  id,
  name: id,
  path: `/path/${id}`,
  value,
  kind,
  category,
  editable: true,
  reason: null,
  description: null,
});
function doc(id = 'one'): SaveDocument {
  return {
    slot: { id, name: `${id}.rpgsave`, format: 'MV', modified: 1000 },
    revision: 'revision-1',
    fields: [
      field('金钱', 5, 'number'),
      field('姓名', '勇者', 'string'),
      field('旗标', false, 'boolean', 'switches'),
      { ...field('复杂对象', null, 'readonly'), editable: false, reason: '复杂对象只读' },
    ],
    metadata: ['章节 1'],
    screenshot: null,
    warnings: [],
  };
}
beforeEach(() => {
  vi.resetAllMocks();
  vi.mocked(api.listEditableSaves).mockResolvedValue({
    slots: [doc().slot, doc('two').slot],
    warnings: [],
  });
  vi.mocked(api.readEditableSave).mockImplementation(async (_game, id) => doc(id));
  vi.mocked(api.applySaveEdits).mockResolvedValue({
    ...doc(),
    revision: 'revision-2',
    fields: [field('金钱', 25, 'number'), field('姓名', 'new', 'string')],
  });
});
afterEach(() => vi.restoreAllMocks());
describe('Save Editor Lite', () => {
  it('offers RenPy Variables and Persistent tabs with dirty slot-switch confirmation', async () => {
    const normal = doc();
    normal.slot.format = 'RenPy';
    const persistent = doc('persistent');
    persistent.slot.format = 'Persistent';
    persistent.fields = [field('persistent.money', 10, 'number', 'persistent')];
    vi.mocked(api.listEditableSaves).mockResolvedValue({
      slots: [normal.slot, persistent.slot],
      warnings: [],
    });
    vi.mocked(api.readEditableSave).mockImplementation(async (_game, id) =>
      id === 'persistent' ? persistent : normal,
    );
    render(<SaveEditor game={game} onClose={vi.fn()} />);
    fireEvent.change(await screen.findByLabelText('金钱'), { target: { value: '9' } });
    fireEvent.click(screen.getByRole('tab', { name: 'Persistent' }));
    expect(api.readEditableSave).toHaveBeenCalledTimes(1);
    fireEvent.click(
      within(await screen.findByRole('dialog', { name: '放弃未保存的存档修改？' })).getByRole(
        'button',
        { name: '放弃修改' },
      ),
    );
    await screen.findByLabelText('persistent.money');
    expect(screen.getByRole('tab', { name: 'Persistent' }).getAttribute('aria-selected')).toBe(
      'true',
    );
    fireEvent.click(screen.getByRole('tab', { name: 'Variables' }));
    await screen.findByLabelText('金钱');
  });
  it('virtualizes large field lists and preserves edits after scrolling and filtering', async () => {
    for (const prop of ['offsetHeight', 'clientHeight'] as const)
      vi.spyOn(HTMLElement.prototype, prop, 'get').mockImplementation(function (this: HTMLElement) {
        return this.classList.contains('save-editor-fields') ? 300 : 0;
      });
    vi.spyOn(HTMLElement.prototype, 'offsetWidth', 'get').mockReturnValue(700);
    const document = doc();
    document.fields = Array.from({ length: 300 }, (_, i) => field(`变量 ${i}`, i, 'number'));
    vi.mocked(api.readEditableSave).mockResolvedValue(document);
    render(<SaveEditor game={game} onClose={vi.fn()} />);
    fireEvent.change(await screen.findByLabelText('变量 0'), { target: { value: '123' } });
    expect(screen.queryByLabelText('变量 299')).toBeNull();
    const viewport = screen.getByRole('region', { name: '存档字段' });
    fireEvent.scroll(viewport, { target: { scrollTop: 16500 } });
    await screen.findByLabelText('变量 299');
    fireEvent.change(screen.getByRole('textbox', { name: '存档字段' }), {
      target: { value: '/path/变量 0' },
    });
    expect(((await screen.findByLabelText('变量 0')) as HTMLInputElement).value).toBe('123');
    expect(
      (screen.getByRole('button', { name: '保存修改 (1)' }) as HTMLButtonElement).disabled,
    ).toBe(false);
  });
  it('loads associated slots and saves typed multiple edits while the game is playing', async () => {
    render(<SaveEditor game={game} onClose={vi.fn()} />);
    fireEvent.change(await screen.findByLabelText('金钱'), { target: { value: '25' } });
    fireEvent.change(screen.getByLabelText('姓名'), { target: { value: 'new' } });
    expect(screen.getAllByText('已修改')).toHaveLength(2);
    fireEvent.click(screen.getByRole('button', { name: '保存修改 (2)' }));
    await waitFor(() =>
      expect(api.applySaveEdits).toHaveBeenCalledWith('game', 'one', 'revision-1', [
        { id: '金钱', value: 25 },
        { id: '姓名', value: 'new' },
      ]),
    );
    await screen.findByText('已保存，请回游戏重新读档。');
    expect((screen.getByRole('button', { name: '保存修改' }) as HTMLButtonElement).disabled).toBe(
      true,
    );
  });
  it('uses a boolean switch, filters by path, and retains changes between categories', async () => {
    render(<SaveEditor game={game} onClose={vi.fn()} />);
    await screen.findByLabelText('金钱');
    fireEvent.click(screen.getByRole('tab', { name: '开关' }));
    fireEvent.click(screen.getByRole('switch', { name: '旗标' }));
    expect((screen.getByRole('switch') as HTMLInputElement).checked).toBe(true);
    fireEvent.click(screen.getByRole('tab', { name: '变量 / 数值' }));
    fireEvent.change(screen.getByRole('textbox', { name: '存档字段' }), {
      target: { value: '/path/姓名' },
    });
    expect(screen.getByLabelText('姓名')).toBeTruthy();
    expect(screen.queryByLabelText('金钱')).toBeNull();
    expect(
      (screen.getByRole('button', { name: '保存修改 (1)' }) as HTMLButtonElement).disabled,
    ).toBe(false);
  });
  it('confirms discarding dirty edits for slot switching, refresh and closing', async () => {
    const close = vi.fn();
    render(<SaveEditor game={game} onClose={close} />);
    fireEvent.change(await screen.findByLabelText('金钱'), { target: { value: '8' } });
    fireEvent.change(screen.getByRole('combobox', { name: '存档槽位' }), {
      target: { value: 'two' },
    });
    const confirm = await screen.findByRole('dialog', { name: '放弃未保存的存档修改？' });
    expect(api.readEditableSave).toHaveBeenCalledTimes(1);
    fireEvent.click(within(confirm).getByRole('button', { name: '继续编辑' }));
    expect((screen.getByLabelText('金钱') as HTMLInputElement).value).toBe('8');
    fireEvent.click(screen.getByRole('button', { name: '刷新' }));
    fireEvent.click(
      within(await screen.findByRole('dialog', { name: '放弃未保存的存档修改？' })).getByRole(
        'button',
        { name: '放弃修改' },
      ),
    );
    await waitFor(() =>
      expect((screen.getByLabelText('金钱') as HTMLInputElement).value).toBe('5'),
    );
    fireEvent.change(screen.getByLabelText('金钱'), { target: { value: '9' } });
    fireEvent.click(screen.getByRole('button', { name: '关闭弹窗' }));
    fireEvent.click(
      within(await screen.findByRole('dialog', { name: '放弃未保存的存档修改？' })).getByRole(
        'button',
        { name: '放弃修改' },
      ),
    );
    expect(close).toHaveBeenCalledTimes(1);
  });
  it('retains drafts on source-change errors and explains unavailable signing keys', async () => {
    const document = doc();
    document.warnings = ['缺少签名私钥'];
    vi.mocked(api.readEditableSave).mockResolvedValue(document);
    vi.mocked(api.applySaveEdits).mockRejectedValue('存档已被游戏更新，请刷新');
    render(<SaveEditor game={game} onClose={vi.fn()} />);
    fireEvent.change(await screen.findByLabelText('金钱'), { target: { value: '8' } });
    expect(screen.getByText('缺少签名私钥')).toBeTruthy();
    expect(screen.getByText('复杂对象只读')).toBeTruthy();
    fireEvent.click(screen.getByRole('button', { name: '保存修改 (1)' }));
    await screen.findByRole('alert');
    expect((screen.getByLabelText('金钱') as HTMLInputElement).value).toBe('8');
  });
  it('explains missing slots and persistent runtime overwrite risk', async () => {
    vi.mocked(api.listEditableSaves).mockResolvedValueOnce({ slots: [], warnings: [] });
    const view = render(<SaveEditor game={game} onClose={vi.fn()} />);
    await screen.findByText(/支持 MV、MZ、Ren’Py/);
    expect(api.readEditableSave).not.toHaveBeenCalled();
    view.unmount();
    const document = doc();
    document.slot.format = 'Persistent';
    document.fields = [field('money', 8, 'number', 'persistent')];
    vi.mocked(api.readEditableSave).mockResolvedValue(document);
    render(<SaveEditor game={game} onClose={vi.fn()} />);
    await screen.findByLabelText('money');
    expect(screen.getByText(/运行中的游戏可能再次覆盖它/)).toBeTruthy();
  });
});
