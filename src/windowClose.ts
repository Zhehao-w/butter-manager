import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useId,
  useLayoutEffect,
  useRef,
  useState,
} from 'react';
import { getCurrentWindow } from '@tauri-apps/api/window';

interface CloseGuard {
  dirty: boolean;
  busy: boolean;
  save: () => Promise<boolean>;
}

export const WindowCloseGuards = createContext<Map<string, CloseGuard> | null>(null);

// Register the existing form state and save action; navigation keeps its own destination.
export function useWindowCloseGuard(guard: CloseGuard) {
  const guards = useContext(WindowCloseGuards);
  const id = useId();
  useLayoutEffect(() => {
    guards?.set(id, guard);
    return () => {
      guards?.delete(id);
    };
  });
}

export function useWindowClosing(
  desktop: boolean,
  busy: boolean,
  onError: (message: string) => void,
) {
  const guards = useRef(new Map<string, CloseGuard>());
  const current = useRef({ busy, onError });
  useLayoutEffect(() => {
    current.current = { busy, onError };
  });
  const phase = useRef<'idle' | 'confirm' | 'saving' | 'closing'>('idle');
  const [confirm, setConfirm] = useState(false);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState('');
  const blocked = useCallback(() => {
    if (!current.current.busy && ![...guards.current.values()].some((guard) => guard.busy))
      return false;
    current.current.onError('当前任务或保存操作尚未完成，请稍后关闭窗口。');
    return true;
  }, []);
  const finish = useCallback(async () => {
    phase.current = 'closing';
    try {
      // destroy does not emit another close request; all requests reach this point after guarding.
      await getCurrentWindow().destroy();
      return true;
    } catch (reason) {
      phase.current = 'idle';
      current.current.onError(`关闭窗口失败：${String(reason)}`);
      return false;
    }
  }, []);
  const requestClose = useCallback(() => {
    if (phase.current !== 'idle' || blocked()) return;
    if ([...guards.current.values()].some((guard) => guard.dirty)) {
      phase.current = 'confirm';
      setError('');
      setConfirm(true);
    } else void finish();
  }, [blocked, finish]);
  useEffect(() => {
    if (!desktop) return;
    let active = true;
    let unlisten: (() => void) | undefined;
    void getCurrentWindow()
      .onCloseRequested((event) => {
        event.preventDefault();
        if (active) requestClose();
      })
      .then((stop) => {
        if (active) unlisten = stop;
        else stop();
      })
      .catch((reason) => current.current.onError(`窗口关闭保护初始化失败：${String(reason)}`));
    return () => {
      active = false;
      unlisten?.();
    };
  }, [desktop, requestClose]);
  const cancel = () => {
    if (phase.current === 'saving') return;
    phase.current = 'idle';
    setConfirm(false);
    setError('');
  };
  async function approve(save: boolean) {
    if (phase.current !== 'confirm' || blocked()) return;
    phase.current = 'saving';
    setSaving(true);
    setError('');
    let closed = false;
    try {
      if (save) {
        for (const guard of [...guards.current.values()].reverse()) {
          if (guard.dirty && !(await guard.save()))
            throw new Error('保存未完成，请检查设置或游戏资料后重试。');
        }
      }
      if (current.current.busy) throw new Error('当前任务尚未完成，请稍后关闭窗口。');
      closed = await finish();
      if (!closed) throw new Error('关闭窗口失败，请重试。');
    } catch (reason) {
      setError(String(reason));
    } finally {
      if (!closed) phase.current = 'confirm';
      setSaving(false);
    }
  }
  return { guards: guards.current, requestClose, confirm, saving, error, cancel, approve };
}
