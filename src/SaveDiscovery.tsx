import { useEffect, useRef, useState } from 'react';
import { api } from './api';
import { Icon } from './ui';
import type { SaveHint, SaveWatchReport } from './types';

export const pathKey = (path: string) =>
  path.trim().replaceAll('\\', '/').replace(/\/+$/, '').toLowerCase();

function comparisonKey(item: SaveHint) {
  const key = pathKey(item.path);
  const slash = key.lastIndexOf('/');
  const parent = key.slice(0, slash);
  return item.files.some((f) => f.toLowerCase() === key.slice(slash + 1)) &&
    parent.includes('/') &&
    parent !== '%userprofile%/appdata/locallow'
    ? parent
    : key;
}

function Candidates({
  items,
  locations,
  onAdd,
  repeated,
}: {
  items: SaveHint[];
  locations: string[];
  onAdd: (path: string) => void;
  repeated?: Set<string>;
}) {
  const [limit, setLimit] = useState(3);
  const keys = new Set(locations.map(pathKey));
  return (
    <>
      <ul className="save-discovery-list">
        {items.slice(0, limit).map((item) => {
          const added = keys.has(pathKey(item.path));
          return (
            <li key={item.path} data-added={added || undefined}>
              <div>
                <strong dir="auto">{item.path}</strong>
                <small>
                  {repeated?.has(comparisonKey(item)) && <b>两次均有写入 · </b>}
                  {item.reason}
                </small>
                {item.files.length > 0 && (
                  <small className="save-evidence">{item.files.join('、')}</small>
                )}
              </div>
              {item.can_add && (
                <button
                  type="button"
                  disabled={added}
                  aria-label={`${added ? '已加入' : '加入位置'} ${item.path}`}
                  onClick={() => onAdd(item.path)}
                >
                  {added && <Icon name="check" size={14} />} {added ? '已加入' : '加入位置'}
                </button>
              )}
            </li>
          );
        })}
      </ul>
      {items.length > 3 && (
        <button type="button" onClick={() => setLimit(limit < items.length ? limit + 10 : 3)}>
          {limit >= items.length ? '收起列表' : '查看更多位置'}
        </button>
      )}
    </>
  );
}

export function SaveDiscovery({
  gameId,
  hints,
  locations,
  onAdd,
  unity,
}: {
  gameId: string;
  hints: SaveHint[];
  locations: string[];
  onAdd: (path: string) => void;
  unity: boolean;
}) {
  const [phase, setPhase] = useState<'idle' | 'starting' | 'watching' | 'finishing'>('idle');
  const [roots, setRoots] = useState<string[]>([]);
  const [report, setReport] = useState<SaveWatchReport | null>(null);
  const [error, setError] = useState('');
  const [repeated, setRepeated] = useState(new Set<string>());
  const active = useRef<string | null>(null);
  const generation = useRef(0);
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null);
  useEffect(
    () => () => {
      generation.current++;
      if (timer.current) clearTimeout(timer.current);
      const token = active.current;
      active.current = null;
      if (token) void api.finishSaveWatch(token, true).catch(() => {});
    },
    [gameId],
  );
  async function finish(cancel: boolean, previous: SaveWatchReport | null) {
    const token = active.current;
    if (!token) return;
    const owner = generation.current;
    active.current = null;
    if (timer.current) clearTimeout(timer.current);
    setPhase('finishing');
    try {
      const result = await api.finishSaveWatch(token, cancel);
      if (owner !== generation.current || !result || cancel) return;
      const previousPaths = new Set(previous?.candidates.map(comparisonKey));
      const overlap = new Set(
        result.candidates.map(comparisonKey).filter((p) => previousPaths.has(p)),
      );
      const ordered = [...result.candidates].sort(
        (a, b) => Number(overlap.has(comparisonKey(b))) - Number(overlap.has(comparisonKey(a))),
      );
      setRepeated(overlap);
      setReport({ ...result, candidates: ordered });
    } catch (e) {
      if (owner === generation.current) setError(String(e));
    } finally {
      if (owner === generation.current) setPhase('idle');
    }
  }
  async function start() {
    const owner = ++generation.current;
    setError('');
    setPhase('starting');
    try {
      const started = await api.startSaveWatch(gameId);
      if (owner !== generation.current) {
        await api.finishSaveWatch(started.token, true);
        return;
      }
      active.current = started.token;
      setRoots(
        started.roots.map((s) =>
          s === '<GAME>'
            ? '游戏目录'
            : s.includes('LocalLow')
              ? 'LocalLow'
              : s === '%APPDATA%'
                ? 'Roaming'
                : 'Local',
        ),
      );
      setPhase('watching');
      timer.current = setTimeout(() => void finish(false, report), 180000);
    } catch (e) {
      if (owner === generation.current) {
        setError(String(e));
        setPhase('idle');
      }
    }
  }
  const props = { locations, onAdd };
  const pendingHints = new Map(hints.map((hint) => [pathKey(hint.path), hint]));
  const observed = (phase === 'idle' ? (report?.candidates ?? []) : []).map((item) => {
    const key = pathKey(item.path);
    const hint = pendingHints.get(key);
    pendingHints.delete(key);
    if (!hint?.can_add) return item;
    return {
      ...item,
      reason: [hint.reason, item.reason].join(' · '),
      files: [...new Set([...item.files, ...hint.files])].slice(0, 4),
    };
  });
  return (
    <div className="save-discovery full">
      {pendingHints.size > 0 && (
        <section>
          <h4>
            <Icon name="folder" size={16} /> 存档候选位置
          </h4>
          <Candidates items={[...pendingHints.values()]} {...props} />
        </section>
      )}
      <section>
        <div className="save-discovery-heading">
          <h4>
            <Icon name="scan" size={17} /> 通过一次存档定位
          </h4>
          {phase === 'watching' && <span className="save-recording">正在记录</span>}
        </div>
        <p className="muted">
          {phase === 'watching'
            ? `切回游戏存一次档，再点击完成。记录范围：${roots.join('、')}。3 分钟后自动完成。`
            : report?.candidates.length
              ? '可换一个槽位再存一次，两次都写入的位置会排在前面。'
              : '先在游戏中打开存档界面，再开始记录。各引擎均可使用，监测范围为游戏目录和 AppData。'}
        </p>
        <div className="save-location-actions">
          {phase === 'watching' ? (
            <>
              <button type="button" className="primary" onClick={() => void finish(false, report)}>
                完成定位
              </button>
              <button type="button" onClick={() => void finish(true, report)}>
                取消
              </button>
            </>
          ) : (
            <button type="button" disabled={phase !== 'idle'} onClick={() => void start()}>
              {phase === 'starting'
                ? '正在准备…'
                : phase === 'finishing'
                  ? '正在整理…'
                  : report
                    ? '再定位一次'
                    : '开始定位'}
            </button>
          )}
        </div>
        <div aria-live="polite">
          {error && <p className="notice">{error}</p>}
          {report && phase === 'idle' && (
            <>
              {report.partial && <p className="muted">部分文件事件未能记录，可再定位一次。</p>}
              {observed.length > 0 ? (
                <Candidates items={observed} repeated={repeated} {...props} />
              ) : (
                <p className="muted">
                  本次未发现可关联的文件。请确认游戏已实际保存，再试一次。
                  {unity && '部分 Unity 游戏会将进度写入注册表 PlayerPrefs。'}
                </p>
              )}
            </>
          )}
        </div>
      </section>
    </div>
  );
}
