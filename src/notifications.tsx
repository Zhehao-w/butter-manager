import { useCallback, useEffect, useRef, useState } from 'react';
import { Icon } from './ui';

type Notice = { id: number; message: string; kind: 'info' | 'error' };
export function useNotifications() {
  const [notice, setNotice] = useState<Notice | null>(null);
  const serial = useRef(0);
  const show = useCallback((message: string, kind: Notice['kind']) => {
    if (message) setNotice({ id: ++serial.current, message, kind });
    else setNotice((current) => (current?.kind === kind ? null : current));
  }, []);
  const setToast = useCallback((message: string) => show(message, 'info'), [show]);
  const setError = useCallback((message: string) => show(message, 'error'), [show]);
  const dismiss = useCallback((id: number) => {
    setNotice((current) => (current?.id === id ? null : current));
  }, []);
  return { notice, setToast, setError, dismiss };
}
export function NotificationToast({
  notice,
  onDismiss,
}: {
  notice: Notice;
  onDismiss: (id: number) => void;
}) {
  const [hovered, setHovered] = useState(false);
  const [focused, setFocused] = useState(false);
  const paused = hovered || focused;
  useEffect(() => {
    if (paused) return;
    const timer = setTimeout(() => onDismiss(notice.id), notice.kind === 'error' ? 6000 : 3000);
    return () => clearTimeout(timer);
  }, [notice.id, notice.kind, paused, onDismiss]);
  return (
    <div
      className={`toast ${notice.kind === 'error' ? 'toast-error' : ''}`}
      role={notice.kind === 'error' ? 'alert' : 'status'}
      onMouseEnter={() => setHovered(true)}
      onMouseLeave={() => setHovered(false)}
      onFocusCapture={() => setFocused(true)}
      onBlurCapture={(event) => {
        if (!event.currentTarget.contains(event.relatedTarget as Node | null)) setFocused(false);
      }}
    >
      <Icon name={notice.kind === 'error' ? 'info' : 'check'} />
      <span>{notice.message}</span>
      <button aria-label="关闭通知" onClick={() => onDismiss(notice.id)}>
        <Icon name="close" size={16} />
      </button>
    </div>
  );
}
