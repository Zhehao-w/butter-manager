import { act, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, expect, it, vi } from 'vitest';
import { TaskBanner } from './TaskBanner';
import type { JobPage } from './types';

const page: JobPage = {
  id: 'scan',
  kind: 'scan',
  root: 'E:/Games',
  status: 'running',
  phase: '分析',
  total: 10,
  processed: 4,
  change_count: 4,
  next_cursor: 4,
  changes: [],
  active: {},
  elapsed_ms: 1200,
  idle_ms: 0,
  warnings: [],
  error: null,
  registered_ids: [],
};
afterEach(() => vi.useRealTimers());

it('keeps active progress and cancellation visible, then fades after completion', () => {
  vi.useFakeTimers();
  const cancel = vi.fn();
  const view = vi.fn();
  const { rerender } = render(<TaskBanner page={page} onCancel={cancel} onViewResults={view} />);
  expect(screen.getByRole('progressbar').getAttribute('aria-valuenow')).toBe('4');
  act(() => vi.advanceTimersByTime(8000));
  expect(screen.getByRole('status')).toBeTruthy();
  fireEvent.click(screen.getByRole('button', { name: '取消任务' }));
  expect(cancel).toHaveBeenCalledOnce();
  fireEvent.click(screen.getByRole('button', { name: '查看扫描结果' }));
  expect(view).toHaveBeenCalledOnce();
  rerender(
    <TaskBanner
      page={{ ...page, status: 'completed', processed: 10 }}
      onCancel={cancel}
      onViewResults={view}
    />,
  );
  act(() => vi.advanceTimersByTime(3000));
  expect(screen.getByRole('status').classList.contains('fading')).toBe(true);
  expect(
    screen.getByRole('status').closest('.task-banner-slot')?.classList.contains('expanded'),
  ).toBe(true);
  act(() => vi.advanceTimersByTime(300));
  expect(screen.getByRole('status')).toBeTruthy();
  expect(
    screen.getByRole('status').closest('.task-banner-slot')?.classList.contains('expanded'),
  ).toBe(false);
  act(() => vi.advanceTimersByTime(300));
  expect(screen.queryByRole('status')).toBeNull();
});

it('pauses dismissal while hovering and keeps failures available until dismissed', () => {
  vi.useFakeTimers();
  const { rerender } = render(
    <TaskBanner page={{ ...page, status: 'completed' }} onCancel={() => {}} />,
  );
  fireEvent.mouseEnter(screen.getByRole('status'));
  act(() => vi.advanceTimersByTime(10000));
  expect(screen.getByRole('status').classList.contains('fading')).toBe(false);
  fireEvent.mouseLeave(screen.getByRole('status'));
  act(() => vi.advanceTimersByTime(3000));
  act(() => vi.advanceTimersByTime(300));
  act(() => vi.advanceTimersByTime(300));
  expect(screen.queryByRole('status')).toBeNull();
  rerender(
    <TaskBanner
      page={{ ...page, id: 'retry', status: 'failed', error: '目录不可访问' }}
      onCancel={() => {}}
    />,
  );
  act(() => vi.advanceTimersByTime(30000));
  expect(screen.getByRole('alert').textContent).toContain('目录不可访问');
  fireEvent.click(screen.getByRole('button', { name: '关闭扫描目录提示' }));
  expect(screen.getByRole('alert').classList.contains('fading')).toBe(true);
  fireEvent.mouseLeave(screen.getByRole('alert'));
  act(() => vi.advanceTimersByTime(300));
  act(() => vi.advanceTimersByTime(300));
  expect(screen.queryByRole('alert')).toBeNull();
});
