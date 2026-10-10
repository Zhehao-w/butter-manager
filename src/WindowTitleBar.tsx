import { useEffect, useState } from 'react';
import { getCurrentWindow } from '@tauri-apps/api/window';

export function WindowTitleBar({
  desktop,
  icon,
  onError,
}: {
  desktop: boolean;
  icon: string;
  onError: (message: string) => void;
}) {
  const [maximized, setMaximized] = useState(false);
  useEffect(() => {
    if (!desktop) return;
    let active = true;
    const update = async () => {
      try {
        const value = await getCurrentWindow().isMaximized();
        if (active) setMaximized(value);
      } catch {
        // Window controls remain usable if the optional state query is unavailable.
      }
    };
    void update();
    window.addEventListener('resize', update);
    return () => {
      active = false;
      window.removeEventListener('resize', update);
    };
  }, [desktop]);
  if (!desktop) return null;

  async function act(action: 'minimize' | 'toggleMaximize' | 'close') {
    try {
      const current = getCurrentWindow();
      await current[action]();
      if (action === 'toggleMaximize') setMaximized(await current.isMaximized());
    } catch (error) {
      onError(`窗口操作失败：${String(error)}`);
    }
  }
  return (
    <header className="window-titlebar" aria-label="窗口标题栏">
      <div className="window-drag-region" data-tauri-drag-region>
        <img src={icon} alt="" draggable={false} />
        <span>butter-manager</span>
      </div>
      <div className="window-controls">
        <button aria-label="最小化窗口" title="最小化" onClick={() => void act('minimize')}>
          <svg viewBox="0 0 12 12" aria-hidden="true">
            <path d="M1 6h10" />
          </svg>
        </button>
        <button
          aria-label={maximized ? '还原窗口' : '最大化窗口'}
          title={maximized ? '还原' : '最大化'}
          onClick={() => void act('toggleMaximize')}
        >
          <svg viewBox="0 0 12 12" aria-hidden="true">
            {maximized ? (
              <path d="M4 4V1.5h6.5V8H8M1.5 4H8v6.5H1.5z" />
            ) : (
              <path d="M1.5 1.5h9v9h-9z" />
            )}
          </svg>
        </button>
        <button
          className="window-close"
          aria-label="关闭窗口"
          title="关闭"
          onClick={() => void act('close')}
        >
          <svg viewBox="0 0 12 12" aria-hidden="true">
            <path d="m1 1 10 10M11 1 1 11" />
          </svg>
        </button>
      </div>
    </header>
  );
}
