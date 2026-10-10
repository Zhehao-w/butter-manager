import { useMemo, useState } from 'react';
import { launchLabel } from './qsp';
import { engineTone, Icon, Modal } from './ui';
import type { IconName } from './ui';
import type { Game, PlayStatus } from './types';

export const playLabels: Record<PlayStatus, string> = {
  UNPLAYED: '从未玩过',
  PLAYING: '正在玩',
  COMPLETED: '已通关',
};
export const launchModes = ['直接启动', '默认应用', 'MTool', 'QSP 播放器'] as const;
export const launchTone = (label: string) =>
  ({
    直接启动: 'direct',
    默认应用: 'associated',
    MTool: 'mtool',
    '公共 MTool': 'mtool',
    'QSP 播放器': 'qsp',
  })[label] || 'direct';
export function LaunchBadge({ game, label }: { game?: Game; label?: string }) {
  const text = label ?? (game ? launchLabel(game) : '直接启动');
  return <span className={`mode launch-${launchTone(text)}`}>{text}</span>;
}
export function PlayBadge({ game }: { game: Game }) {
  const status = game.play_status ?? (game.last_launched_at ? 'PLAYING' : 'UNPLAYED');
  return <span className={`play-status play-${status.toLowerCase()}`}>{playLabels[status]}</span>;
}
export interface GameFilters {
  statuses: PlayStatus[];
  engines: string[];
  modes: string[];
}
export const emptyFilters = (): GameFilters => ({ statuses: [], engines: [], modes: [] });
function filterAppearance(group: keyof GameFilters, key: string): { icon: IconName; tone: string } {
  if (group === 'statuses') {
    return {
      icon: 'library',
      tone: { UNPLAYED: 'slate', PLAYING: 'teal', COMPLETED: 'violet' }[key] || 'slate',
    };
  }
  if (group === 'modes') {
    return {
      icon:
        (
          { 直接启动: 'play', 默认应用: 'grid', MTool: 'tool', 'QSP 播放器': 'folder' } as Record<
            string,
            IconName
          >
        )[key] || 'play',
      tone:
        { 直接启动: 'blue', 默认应用: 'teal', MTool: 'violet', 'QSP 播放器': 'amber' }[key] ||
        'blue',
    };
  }
  return (
    (
      {
        Godot: { icon: 'bot', tone: 'sky' },
        QSP: { icon: 'feather', tone: 'blue' },
        "Ren'Py": { icon: 'flower', tone: 'pink' },
        'RPG Maker (legacy)': { icon: 'swords', tone: 'violet' },
        'RPG Maker MV': { icon: 'swords', tone: 'violet' },
        'RPG Maker MZ': { icon: 'wand', tone: 'sky' },
        Unity: { icon: 'boxes', tone: 'slate' },
        Unknown: { icon: 'help', tone: 'slate' },
      } as Record<string, { icon: IconName; tone: string }>
    )[key] || { icon: 'cpu', tone: 'slate' }
  );
}
export function matchesFilters(game: Game, filters: GameFilters) {
  const status = game.play_status ?? (game.last_launched_at ? 'PLAYING' : 'UNPLAYED');
  return (
    (!filters.statuses.length || filters.statuses.includes(status)) &&
    (!filters.engines.length || filters.engines.includes(game.engine)) &&
    (!filters.modes.length || filters.modes.includes(launchLabel(game)))
  );
}
export function engineQuickGroups(games: Game[]) {
  const counts = new Map<string, number>();
  for (const game of games) counts.set(game.engine, (counts.get(game.engine) ?? 0) + 1);
  const popular = [...counts]
    .filter(([engine]) => engine !== 'Unknown')
    .sort(([a, ac], [b, bc]) => bc - ac || a.localeCompare(b, 'zh-CN'))
    .slice(0, 7)
    .map(([engine, count]) => ({ engine, count }));
  return {
    popular,
    otherCount: games.length - popular.reduce((sum, group) => sum + group.count, 0),
  };
}
export function EngineQuickFilters({
  games,
  engines,
  other,
  onChange,
}: {
  games: Game[];
  engines: string[];
  other: boolean;
  onChange: (engines: string[], other: boolean) => void;
}) {
  const { popular, otherCount } = useMemo(() => engineQuickGroups(games), [games]);
  const options = [
    {
      key: 'all',
      label: '全部',
      count: games.length,
      icon: 'library' as IconName,
      active: !other && !engines.length,
      engines: [],
      other: false,
    },
    ...popular.map(({ engine, count }) => ({
      key: `engine:${engine}`,
      label: engine,
      count,
      icon: filterAppearance('engines', engine).icon,
      active: !other && engines.length === 1 && engines[0] === engine,
      engines: [engine],
      other: false,
    })),
    {
      key: 'other',
      label: '其他',
      count: otherCount,
      icon: 'grid' as IconName,
      active: other,
      engines: [],
      other: true,
    },
  ];
  return (
    <div className="engine-quick-filters" role="group" aria-label="引擎快捷筛选">
      {options.map((option) => (
        <button
          key={option.key}
          className={
            option.key === 'all' ? undefined : `tone-${engineTone(option.engines[0] ?? 'Unknown')}`
          }
          type="button"
          aria-pressed={option.active}
          disabled={!option.count && !option.active}
          onClick={() => onChange(option.engines, option.other)}
        >
          <Icon name={option.icon} size={14} />
          {option.label}
          <span className="quick-filter-count">{option.count}</span>
        </button>
      ))}
    </div>
  );
}
export function LibraryFilters({
  games,
  value,
  onChange,
}: {
  games: Game[];
  value: GameFilters;
  onChange: (value: GameFilters) => void;
}) {
  const [open, setOpen] = useState(false);
  const [draft, setDraft] = useState<GameFilters>(emptyFilters);
  const count = value.statuses.length + value.engines.length + value.modes.length;
  const engines = useMemo(
    () =>
      [...new Set([...games.map((g) => g.engine), ...value.engines])].sort(
        (a, b) => Number(a === 'Unknown') - Number(b === 'Unknown') || a.localeCompare(b, 'zh-CN'),
      ),
    [games, value.engines],
  );
  const toggle = (group: keyof GameFilters, key: string) => {
    setDraft((previous) => {
      const current: string[] = previous[group];
      return {
        ...previous,
        [group]: current.includes(key) ? current.filter((s) => s !== key) : [...current, key],
      };
    });
  };
  return (
    <>
      <button
        type="button"
        className={`library-filter-button ${count ? 'active' : ''}`}
        aria-pressed={count > 0}
        aria-haspopup="dialog"
        onClick={() => {
          setDraft(value);
          setOpen(true);
        }}
      >
        <Icon name="filter" size={16} />
        筛选{count ? ` (${count})` : ''}
      </button>
      {open && (
        <Modal
          title="筛选游戏"
          variant="filters"
          closeIconOnly
          onClose={() => setOpen(false)}
          footer={
            <div className="filter-actions">
              <button type="button" onClick={() => setDraft(emptyFilters())}>
                清空筛选
              </button>
              <div className="filter-actions-end">
                <button type="button" onClick={() => setOpen(false)}>
                  取消
                </button>
                <button
                  type="button"
                  className="primary"
                  onClick={() => {
                    onChange(draft);
                    setOpen(false);
                  }}
                >
                  完成
                </button>
              </div>
            </div>
          }
        >
          <div className="library-filter-groups">
            {(
              [
                ['statuses', '游玩状态', Object.entries(playLabels)],
                ['engines', '游戏引擎', engines.map((e) => [e, e === 'Unknown' ? '未识别' : e])],
                ['modes', '启动方式', launchModes.map((m) => [m, m])],
              ] as [keyof GameFilters, string, string[][]][]
            ).map(([group, title, options]) => (
              <fieldset key={group}>
                <legend>{title}</legend>
                <div className={`filter-options filter-options-${group}`}>
                  {options.map(([key, label]) => {
                    const appearance = filterAppearance(group, key);
                    const selected = (draft[group] as string[]).includes(key);
                    return (
                      <label
                        key={key}
                        className={`filter-option filter-tone-${appearance.tone}${selected ? ' selected' : ''}`}
                      >
                        <input
                          type="checkbox"
                          checked={selected}
                          onChange={() => toggle(group, key)}
                        />
                        <span className="filter-option-icon">
                          <Icon name={appearance.icon} size={group === 'engines' ? 19 : 23} />
                        </span>
                        <span className="filter-option-label">{label}</span>
                      </label>
                    );
                  })}
                </div>
              </fieldset>
            ))}
          </div>
        </Modal>
      )}
    </>
  );
}
