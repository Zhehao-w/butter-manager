import { useEffect, useState } from 'react';
import { api } from './api';
import { MToolStrategyField } from './MToolStrategyField';
import type { MToolLaunchPreview, MToolStrategy, Settings } from './types';

export function MToolConfiguration({
  id,
  executable,
  loader,
  workingDirectory,
  settings,
  onLoader,
  strategy,
  onStrategy,
  disabled = false,
}: {
  id: string;
  executable: string | null;
  loader: string | null;
  workingDirectory: string;
  settings: Settings;
  onLoader: (loader: string | null) => void;
  strategy: MToolStrategy;
  onStrategy: (strategy: MToolStrategy) => void;
  disabled?: boolean;
}) {
  const [preview, setPreview] = useState<MToolLaunchPreview | null>(null);
  const [message, setMessage] = useState('');
  const [loading, setLoading] = useState(false);
  useEffect(() => {
    let stale = false;
    setPreview(null);
    setMessage('');
    if (!executable) {
      setMessage('请先选择游戏启动 EXE。');
      setLoading(false);
      return;
    }
    setLoading(true);
    const timer = setTimeout(() => {
      void api
        .previewMtoolLaunch(id, executable, loader, workingDirectory, strategy)
        .then((result) => {
          if (!stale) setPreview(result);
        })
        .catch((reason) => {
          if (!stale) setMessage(String(reason));
        })
        .finally(() => {
          if (!stale) setLoading(false);
        });
    }, 150);
    return () => {
      stale = true;
      clearTimeout(timer);
    };
  }, [id, executable, loader, workingDirectory, settings, strategy]);
  const custom = !!loader && !['loaders/mzHook.dll', 'loaders/mzHook32.dll'].includes(loader);
  return (
    <>
      <div className="full launch-tool-info">
        <span>公共 MTool</span>
        <span className="path">
          {settings.mtool_root
            ? `${settings.mtool_root}\\${settings.mtool_runtime.replaceAll('/', '\\')}`
            : '请在侧边栏 MTool 页面设置公共目录。'}
        </span>
      </div>
      <MToolStrategyField value={strategy} onChange={onStrategy} disabled={disabled} />
      {strategy === 'INJECT' && (
        <label className="full">
          游戏位数 / Loader
          <select
            aria-label="MTool loader 选择"
            value={loader || ''}
            disabled={disabled}
            onChange={(event) => onLoader(event.target.value || null)}
          >
            <option value="">自动识别（推荐）</option>
            <option value="loaders/mzHook32.dll">32 位 · mzHook32.dll</option>
            <option value="loaders/mzHook.dll">64 位 · mzHook.dll</option>
            {custom && <option value={loader!}>保留旧配置 · {loader}</option>}
          </select>
        </label>
      )}
      <span className="muted full" role="status">
        {loading
          ? '正在检查启动配置…'
          : message ||
            (preview
              ? strategy === 'BAKIN'
                ? 'BakinLauncher.exe · 游戏根目录'
                : `${preview.architecture === 'x86' ? '32 位' : preview.architecture === 'x64' ? '64 位' : preview.architecture} · ${preview.loader.split(/[\\/]/).pop()}`
              : '')}
      </span>
    </>
  );
}
