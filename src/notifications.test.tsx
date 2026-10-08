import { act, fireEvent, render, renderHook, screen } from '@testing-library/react';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { NotificationToast, useNotifications } from './notifications';

beforeEach(() => vi.useFakeTimers());
afterEach(() => vi.useRealTimers());
function Fixture() {
  const { notice, setToast, setError, dismiss } = useNotifications();
  return (
    <>
      <button onClick={() => setToast('扫描完成')}>成功</button>
      <button onClick={() => setError('扫描失败')}>失败</button>
      {notice && <NotificationToast key={notice.id} notice={notice} onDismiss={dismiss} />}
    </>
  );
}
it('dismisses success after six seconds and errors after ten without resurrecting an older message', () => {
  render(<Fixture />);
  fireEvent.click(screen.getByText('成功'));
  act(() => vi.advanceTimersByTime(5999));
  expect(screen.getByRole('status').textContent).toContain('扫描完成');
  act(() => vi.advanceTimersByTime(1));
  expect(screen.queryByRole('status')).toBeNull();
  fireEvent.click(screen.getByText('成功'));
  act(() => vi.advanceTimersByTime(5000));
  fireEvent.click(screen.getByText('失败'));
  act(() => vi.advanceTimersByTime(1000));
  expect(screen.getByRole('alert').textContent).toContain('扫描失败');
  act(() => vi.advanceTimersByTime(9000));
  expect(screen.queryByRole('alert')).toBeNull();
  expect(screen.queryByRole('status')).toBeNull();
});
it('pauses while hovered or keyboard-focused, resumes after both leave, and allows dismissal', () => {
  render(<Fixture />);
  fireEvent.click(screen.getByText('成功'));
  const toast = screen.getByRole('status');
  const close = screen.getByRole('button', { name: '关闭通知' });
  fireEvent.mouseEnter(toast);
  fireEvent.focus(close);
  act(() => vi.advanceTimersByTime(20000));
  expect(screen.getByRole('status')).toBe(toast);
  fireEvent.mouseLeave(toast);
  act(() => vi.advanceTimersByTime(20000));
  expect(screen.getByRole('status')).toBe(toast);
  fireEvent.blur(close);
  act(() => vi.advanceTimersByTime(6000));
  expect(screen.queryByRole('status')).toBeNull();
  fireEvent.click(screen.getByText('失败'));
  fireEvent.click(screen.getByRole('button', { name: '关闭通知' }));
  expect(screen.queryByRole('alert')).toBeNull();
});
it('gives repeated messages a fresh timer and ignores stale dismissals', () => {
  const { result } = renderHook(() => useNotifications());
  act(() => result.current.setToast('完成'));
  const oldId = result.current.notice!.id;
  act(() => result.current.setToast('完成'));
  expect(result.current.notice!.id).not.toBe(oldId);
  act(() => result.current.dismiss(oldId));
  expect(result.current.notice!.message).toBe('完成');
  act(() => result.current.setError('失败'));
  act(() => result.current.setToast(''));
  expect(result.current.notice!.kind).toBe('error');
  act(() => result.current.setError(''));
  expect(result.current.notice).toBeNull();
});
it('does not postpone expiry when unrelated renders occur', () => {
  const dismiss = vi.fn();
  const notice = { id: 1, message: '完成', kind: 'info' as const };
  const { rerender } = render(<NotificationToast notice={notice} onDismiss={dismiss} />);
  act(() => vi.advanceTimersByTime(4000));
  rerender(<NotificationToast notice={{ ...notice }} onDismiss={dismiss} />);
  act(() => vi.advanceTimersByTime(2000));
  expect(dismiss).toHaveBeenCalledExactlyOnceWith(1);
});
