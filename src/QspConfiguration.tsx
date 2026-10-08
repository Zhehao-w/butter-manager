import { useId, useState } from 'react';
import { api } from './api';
import { BrowseButton } from './ui';
import type { ExternalPlayer, QspDetection } from './types';

export function QspConfiguration({
  root,
  player,
  config,
  detection,
  disabled,
  onChange,
  onError,
}: {
  root: string;
  player: string | null;
  config: ExternalPlayer;
  detection?: QspDetection | null;
  disabled?: boolean;
  onChange: (player: string | null, config: ExternalPlayer) => void;
  onError: (message: string) => void;
}) {
  const id = useId();
  const [choosing, setChoosing] = useState(false);
  const browse = async (extension: 'exe' | 'qsp') => {
    setChoosing(true);
    try {
      const file = await api.choosePlayerFile(root, extension);
      if (file)
        onChange(
          extension === 'exe' ? file : player,
          extension === 'qsp' ? { ...config, game_file: file } : config,
        );
    } catch (error) {
      onError(String(error));
    } finally {
      setChoosing(false);
    }
  };
  return (
    <div className="form-grid full qsp-configuration">
      <label className="full">
        QSP 播放器（游戏目录内 EXE）
        <div className="path-input">
          <input
            aria-label="QSP 播放器"
            list={`${id}-players`}
            value={player || ''}
            placeholder="未配置，可入库后再选择"
            disabled={disabled || choosing}
            onChange={(e) => onChange(e.target.value.trim() || null, config)}
          />
          <BrowseButton disabled={disabled || choosing} onClick={() => void browse('exe')}>
            更改播放器…
          </BrowseButton>
        </div>
        <datalist id={`${id}-players`}>
          {detection?.players.map((file) => (
            <option key={file} value={file} />
          ))}
        </datalist>
      </label>
      <label className="full">
        QSP 主游戏文件（相对游戏目录）
        <div className="path-input">
          <input
            aria-label="QSP 主游戏文件"
            list={`${id}-files`}
            value={config.game_file || ''}
            placeholder="请选择 .qsp 文件"
            disabled={disabled || choosing}
            onChange={(e) =>
              onChange(player, { ...config, game_file: e.target.value.trim() || null })
            }
          />
          <BrowseButton disabled={disabled || choosing} onClick={() => void browse('qsp')}>
            更改 QSP 文件…
          </BrowseButton>
        </div>
        <datalist id={`${id}-files`}>
          {detection?.game_files.map((file) => (
            <option key={file} value={file} />
          ))}
        </datalist>
      </label>
      <p className="muted full">
        使用游戏自带播放器，默认在游戏根目录运行。{!player && '播放器未配置，暂时无法启动。'}
        {!config.game_file && '请选择主游戏文件；多个候选不会自动选择。'}
      </p>
    </div>
  );
}
