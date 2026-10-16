import { useEffect, useId, useRef, useState } from 'react';
import { api } from './api';
import { CardTitle, Icon } from './ui';
import type { Game, SaveDetectionRequest } from './types';

function paths(value: string) {
  return value
    .split('\n')
    .map((path) => path.trim())
    .filter(Boolean);
}

// The desktop target is Windows. Keep original spelling in stored paths.
function pathKey(value: string, root: string) {
  let path = value.replaceAll('\\', '/').replace(/\/+$/, '');
  if (path.startsWith('<GAME>/')) path = `${root}/${path.slice(7)}`;
  else if (!path.startsWith('%') && !path.startsWith('/') && !/^[a-z]:/i.test(path))
    path = `${root}/${path}`;
  return path.replaceAll('\\', '/').replace(/\/+/g, '/').toLowerCase();
}

export function SavePathDetection({
  game,
  request,
  value,
  onChange,
  locked,
  active,
}: {
  game: Game;
  request: SaveDetectionRequest;
  value: string;
  onChange: (value: string) => void;
  locked: boolean;
  active: boolean;
}) {
  const previewId = useId();
  const [preview, setPreview] = useState<string[] | null>(null);
  const [selected, setSelected] = useState<string[]>([]);
  const [mode, setMode] = useState<'add' | 'replace'>('add');
  const [loading, setLoading] = useState<'detect' | 'apply' | null>(null);
  const [error, setError] = useState('');
  const [notice, setNotice] = useState('');
  const sequence = useRef(0);
  const context = JSON.stringify([
    game.id,
    game.install_path,
    game.updated_at,
    game.save_paths,
    request,
    value,
  ]);
  const live = useRef({ context, locked, active });
  live.current = { context, locked, active };
  useEffect(() => {
    sequence.current++;
    setPreview(null);
    setLoading(null);
    setError('');
    if (preview !== null || loading) setNotice('配置或任务状态已变化，请重新检测。');
  }, [context, locked, active]);
  useEffect(
    () => () => {
      sequence.current++;
    },
    [],
  );

  const current = paths(value);
  const key = (path: string) => pathKey(path, game.install_path);
  const selectedKeys = new Set(selected.map(key));
  const currentKeys = new Set(current.map(key));
  const removed = mode === 'replace' ? current.filter((path) => !selectedKeys.has(key(path))) : [];
  function cancel() {
    sequence.current++;
    setPreview(null);
    setLoading(null);
    setError('');
    setNotice('已取消检测，当前路径未更改。');
  }
  async function detect(apply = false) {
    if (locked || !active || loading) return;
    const ticket = ++sequence.current;
    const valid = () =>
      ticket === sequence.current &&
      live.current.context === context &&
      !live.current.locked &&
      live.current.active;
    setLoading(apply ? 'apply' : 'detect');
    setError('');
    setNotice('');
    if (!apply) {
      setPreview(null);
      setSelected([]);
      setMode('add');
    }
    try {
      // Recheck on application: candidates may have disappeared since the preview.
      const found = await api.previewSavePaths(game.id, request);
      if (!valid()) return;
      if (apply) {
        if (selected.some((path) => !found.includes(path))) {
          setPreview(found);
          setSelected(selected.filter((path) => found.includes(path)));
          setError('部分候选路径已失效，请检查更新后的结果并重新选择。');
          return;
        }
        const seen = mode === 'add' ? new Set(currentKeys) : new Set<string>();
        const chosen = selected
          .filter((path) => {
            const identity = key(path);
            if (seen.has(identity)) return false;
            seen.add(identity);
            return true;
          })
          .join('\n');
        // Adding candidates must not rewrite any existing manual entries or spelling.
        const next =
          mode === 'replace'
            ? chosen
            : !chosen
              ? value
              : value.trim()
                ? `${value}${value.endsWith('\n') ? '' : '\n'}${chosen}`
                : chosen;
        setPreview(null);
        onChange(next);
        setNotice(
          next === value ? '所选路径已关联，无需修改。' : '已填入存档路径草稿，请保存资料。',
        );
      } else setPreview(found);
    } catch (reason) {
      if (valid()) setError(String(reason));
    } finally {
      if (valid()) setLoading(null);
    }
  }
  const expanded = preview !== null || loading !== null || !!error;
  return (
    <>
      <div className="save-location-heading">
        <CardTitle icon="save">存档位置</CardTitle>
        <button
          type="button"
          disabled={locked || !!loading}
          onClick={() => void detect()}
          aria-expanded={expanded}
          aria-controls={previewId}
        >
          <Icon name="scan" size={16} />
          重新检测
        </button>
      </div>
      {expanded && (
        <section
          id={previewId}
          className="save-detection-preview"
          aria-label="存档路径检测预览"
          aria-busy={!!loading}
        >
          <p className="muted">
            按当前引擎和启动配置查找已有路径。仅更新关联，不移动或改写存档；应用后需保存资料。
          </p>
          {loading && (
            <p role="status">{loading === 'apply' ? '正在复核所选路径…' : '正在检测存档路径…'}</p>
          )}
          {error && (
            <p className="error" role="alert">
              {error}
            </p>
          )}
          {preview !== null && (
            <>
              <div className="save-detection-columns">
                <div className="save-detection-column">
                  <h4>
                    当前路径 <span className="muted">{current.length}</span>
                  </h4>
                  {current.length ? (
                    <ul className="save-detection-list">
                      {current.map((path, index) => (
                        <li key={`${index}-${path}`}>
                          <span className="save-detection-path">{path}</span>
                          <span
                            className={removed.includes(path) ? 'save-detection-removal' : 'muted'}
                          >
                            {removed.includes(path) ? '将取消关联' : '保留'}
                          </span>
                        </li>
                      ))}
                    </ul>
                  ) : (
                    <p className="muted">尚未配置存档路径。</p>
                  )}
                </div>
                <div className="save-detection-column">
                  <h4>
                    检测候选 <span className="muted">{preview.length}</span>
                  </h4>
                  {preview.length ? (
                    <ul className="save-detection-list">
                      {preview.map((path) => (
                        <li key={path}>
                          <label className="save-detection-option">
                            <input
                              type="checkbox"
                              aria-label={path}
                              checked={selected.includes(path)}
                              disabled={!!loading || locked}
                              onChange={(event) =>
                                setSelected((current) =>
                                  event.target.checked
                                    ? [...current, path]
                                    : current.filter((value) => value !== path),
                                )
                              }
                            />
                            <span className="save-detection-path">{path}</span>
                          </label>
                          {currentKeys.has(key(path)) && <span className="muted">已关联</span>}
                        </li>
                      ))}
                    </ul>
                  ) : (
                    <p className="muted" role="status">
                      未检测到可用路径。可先在游戏内保存一次，再重新检测，或手动添加目录。
                    </p>
                  )}
                </div>
              </div>
              {!!preview.length && (
                <fieldset className="save-detection-modes" disabled={!!loading || locked}>
                  <legend className="sr-only">路径应用方式</legend>
                  <label>
                    <input
                      type="radio"
                      name={`save-detection-mode-${game.id}`}
                      checked={mode === 'add'}
                      onChange={() => setMode('add')}
                    />
                    新增到现有路径
                  </label>
                  <label>
                    <input
                      type="radio"
                      name={`save-detection-mode-${game.id}`}
                      checked={mode === 'replace'}
                      onChange={() => setMode('replace')}
                    />
                    替换全部关联路径
                  </label>
                </fieldset>
              )}
              {mode === 'replace' && !!preview.length && (
                <p className="save-detection-removal" role="status">
                  将用所选候选替换全部关联路径，取消 {removed.length} 项关联；对应文件保持原位。
                </p>
              )}
            </>
          )}
          <div className="save-detection-actions">
            <button type="button" onClick={cancel}>
              取消检测
            </button>
            {!!preview?.length && (
              <button
                type="button"
                className="primary"
                disabled={!selected.length || !!loading || locked}
                onClick={() => void detect(true)}
              >
                应用到草稿
              </button>
            )}
          </div>
        </section>
      )}
      {notice && (
        <p className="save-detection-notice muted" role="status">
          {notice}
        </p>
      )}
    </>
  );
}
