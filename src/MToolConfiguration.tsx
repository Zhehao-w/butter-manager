import { useEffect, useState } from 'react';
import { api } from './api';
import type { MToolLaunchPreview, Settings } from './types';

export function MToolConfiguration({
  id,
  executable,
  loader,
  workingDirectory,
  settings,
  onLoader,
}: {
  id: string;
  executable: string | null;
  loader: string | null;
  workingDirectory: string;
  settings: Settings;
  onLoader: (loader: string | null) => void;
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
        .previewMtoolLaunch(id, executable, loader, workingDirectory)
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
  }, [id, executable, loader, workingDirectory, settings]);
  const custom = !!loader && !['loaders/mzHook.dll', 'loaders/mzHook32.dll'].includes(loader);
  return (
    <div className="mtool-configuration full">
      <div className="shared-tool-summary">
        <strong>使用公共 MTool</strong>
        <span className="path">
          {settings.mtool_root
            ? `${settings.mtool_root}\\${settings.mtool_runtime.replaceAll('/', '\\')}`
            : '请在侧边栏 MTool 页面设置公共目录。'}
        </span>
        <span className="muted">游戏启动文件就是注入目标，无需重复配置工具路径。</span>
      </div>
      <label>
        游戏位数 / Loader
        <select
          aria-label="MTool loader 选择"
          value={loader || ''}
          onChange={(event) => onLoader(event.target.value || null)}
        >
          <option value="">自动识别（推荐）</option>
          <option value="loaders/mzHook32.dll">32 位 · mzHook32.dll</option>
          <option value="loaders/mzHook.dll">64 位 · mzHook.dll</option>
          {custom && <option value={loader!}>保留旧配置 · {loader}</option>}
        </select>
      </label>
      <div className="mtool-preview" role="status">
        {loading
          ? '正在检查启动配置…'
          : message ||
            (preview
              ? `游戏 ${preview.target_exe} · ${preview.architecture === 'x86' ? '32 位' : preview.architecture === 'x64' ? '64 位' : preview.architecture} · ${preview.loader.split(/[\\/]/).pop()}`
              : '')}
      </div>
      <p className="muted">
        先注入游戏，等待注入器成功退出后打开公共 MTool。不会运行目录中的 BAT
        或使用游戏自带的工具目录。
      </p>
    </div>
  );
}
