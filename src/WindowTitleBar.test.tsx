import { beforeEach, describe, expect, it, vi } from 'vitest';
import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { WindowTitleBar } from './WindowTitleBar';

const native = vi.hoisted(() => ({
  isMaximized: vi.fn(),
  minimize: vi.fn(),
  toggleMaximize: vi.fn(),
  close: vi.fn(),
}));
vi.mock('@tauri-apps/api/window', () => ({ getCurrentWindow: () => native }));

beforeEach(() => {
  vi.resetAllMocks();
  native.isMaximized.mockResolvedValue(false);
  native.minimize.mockResolvedValue(undefined);
  native.toggleMaximize.mockResolvedValue(undefined);
  native.close.mockResolvedValue(undefined);
});

describe('WindowTitleBar', () => {
  it('minimizes and requests normal window close through Tauri', async () => {
    render(<WindowTitleBar desktop icon="icon.png" onError={vi.fn()} />);
    fireEvent.click(screen.getByRole('button', { name: '最小化窗口' }));
    await waitFor(() => expect(native.minimize).toHaveBeenCalledTimes(1));
    fireEvent.click(screen.getByRole('button', { name: '关闭窗口' }));
    await waitFor(() => expect(native.close).toHaveBeenCalledTimes(1));
  });

  it('switches maximize and restore controls and follows external window resizing', async () => {
    render(<WindowTitleBar desktop icon="icon.png" onError={vi.fn()} />);
    await waitFor(() => expect(native.isMaximized).toHaveBeenCalled());
    native.isMaximized.mockResolvedValue(true);
    fireEvent.click(screen.getByRole('button', { name: '最大化窗口' }));
    await screen.findByRole('button', { name: '还原窗口' });
    native.isMaximized.mockResolvedValue(false);
    fireEvent.click(screen.getByRole('button', { name: '还原窗口' }));
    await screen.findByRole('button', { name: '最大化窗口' });
    expect(native.toggleMaximize).toHaveBeenCalledTimes(2);
    native.isMaximized.mockResolvedValue(true);
    fireEvent(window, new Event('resize'));
    await screen.findByRole('button', { name: '还原窗口' });
  });

  it('reports failed window operations', async () => {
    const onError = vi.fn();
    native.minimize.mockRejectedValue(new Error('denied'));
    render(<WindowTitleBar desktop icon="icon.png" onError={onError} />);
    fireEvent.click(screen.getByRole('button', { name: '最小化窗口' }));
    await waitFor(() => expect(onError).toHaveBeenCalledWith('窗口操作失败：Error: denied'));
  });

  it('does not show or invoke desktop controls in browser previews', () => {
    const { container } = render(
      <WindowTitleBar desktop={false} icon="icon.png" onError={vi.fn()} />,
    );
    expect(container.childElementCount).toBe(0);
    expect(native.isMaximized).not.toHaveBeenCalled();
  });
});
