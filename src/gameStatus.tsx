import { engineLabel, supportedEngines } from './engines';
import { useEffect, useId, useLayoutEffect, useMemo, useRef, useState } from 'react';
import { launchLabel } from './qsp';
import { engineTone, Icon, Modal } from './ui';
import type { IconName } from './ui';
import type { Game, LibraryTag, PlayStatus } from './types';

export const playLabels: Record<PlayStatus, string> = {
  UNPLAYED: '未启动',
  PLAYING: '正在玩',
  COMPLETED: '已通关',
};
const playAppearance: Record<PlayStatus, { icon: IconName; tone: string }> = {
  UNPLAYED: { icon: 'library', tone: 'slate' },
  PLAYING: { icon: 'play', tone: 'playing' },
  COMPLETED: { icon: 'flag', tone: 'teal' },
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
  favoritesOnly?: boolean;
  tags?: string[];
}
export const emptyFilters = (): GameFilters => ({
  statuses: [],
  engines: [],
  modes: [],
  favoritesOnly: false,
  tags: [],
});
type FilterGroup = 'statuses' | 'engines' | 'modes' | 'tags';
export function PlayStatusControl({
  value,
  onChange,
  disabled = false,
  saving = false,
}: {
  value: PlayStatus;
  onChange: (status: PlayStatus) => void;
  disabled?: boolean;
  saving?: boolean;
}) {
  const name = useId();
  return (
    <div
      className="play-status-control"
      role="radiogroup"
      aria-label="游玩状态"
      aria-busy={saving || undefined}
    >
      {Object.entries(playLabels).map(([key, label]) => {
        const status = key as PlayStatus;
        return (
          <label
            key={status}
            className={`play-status-choice play-${status.toLowerCase()}${value === status ? ' is-selected' : ''}`}
          >
            <input
              className="sr-only"
              type="radio"
              name={name}
              value={status}
              checked={value === status}
              disabled={disabled}
              onChange={() => onChange(status)}
            />
            <Icon name={playAppearance[status].icon} size={14} />
            <span>{label}</span>
          </label>
        );
      })}
    </div>
  );
}
function filterAppearance(group: FilterGroup, key: string): { icon: IconName; tone: string } {
  if (group === 'tags') return { icon: 'tag', tone: 'blue' };
  if (group === 'statuses') {
    return playAppearance[key as PlayStatus];
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
        'RPG Developer Bakin': { icon: 'boxes', tone: 'amber' },
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
    (!filters.modes.length || filters.modes.includes(launchLabel(game))) &&
    (!filters.favoritesOnly || game.favorite) &&
    (filters.tags ?? []).every((id) => game.tags.some((tag) => tag.id === id))
  );
}
export function engineQuickGroups(games: Game[]) {
  const counts = new Map<string, number>();
  for (const game of games) counts.set(game.engine, (counts.get(game.engine) ?? 0) + 1);
  const popular = [...counts]
    .filter(([engine]) => engine !== 'Unknown')
    .sort(([a, ac], [b, bc]) => bc - ac || a.localeCompare(b, 'zh-CN'))
    .slice(0, 5)
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
  onVisibleCountChange,
}: {
  games: Game[];
  engines: string[];
  other: boolean;
  onChange: (engines: string[], other: boolean) => void;
  onVisibleCountChange?: (count: number) => void;
}) {
  const { popular } = useMemo(() => engineQuickGroups(games), [games]);
  const container = useRef<HTMLDivElement>(null);
  const [visibleCount, setVisibleCount] = useState(5);
  useLayoutEffect(() => {
    const element = container.current;
    if (!element) return;
    const fit = () => {
      const available = element.clientWidth;
      if (!available) return;
      const buttons = Array.from(element.querySelectorAll('button'));
      const gap = parseFloat(getComputedStyle(element).columnGap) || 0;
      let used = buttons.at(-1)?.getBoundingClientRect().width ?? 0;
      let count = 0;
      for (const button of buttons.slice(0, -1)) {
        used += button.getBoundingClientRect().width + gap;
        if (used > available) break;
        count++;
      }
      setVisibleCount(count);
      onVisibleCountChange?.(count);
    };
    fit();
    const observer = typeof ResizeObserver === 'undefined' ? null : new ResizeObserver(fit);
    observer?.observe(element);
    window.addEventListener('resize', fit);
    return () => {
      observer?.disconnect();
      window.removeEventListener('resize', fit);
    };
  }, [popular, onVisibleCountChange]);
  const otherCount =
    games.length - popular.slice(0, visibleCount).reduce((sum, group) => sum + group.count, 0);
  const options = [
    ...popular.map(({ engine, count }) => ({
      key: `engine:${engine}`,
      label: engine === 'RPG Maker (legacy)' ? 'RPG Maker' : engineLabel(engine),
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
    <div ref={container} className="engine-quick-filters" role="group" aria-label="引擎快捷筛选">
      {options.map((option, index) => (
        <button
          key={option.key}
          className={`quick-filter-chip tone-${engineTone(option.engines[0] ?? 'Unknown')}${index >= visibleCount && !option.other ? ' quick-engine-overflow' : ''}`}
          type="button"
          aria-pressed={option.active}
          disabled={!option.count && !option.active}
          onClick={() =>
            onChange(option.active ? [] : option.engines, !option.active && option.other)
          }
        >
          <Icon name={option.icon} size={16} />
          {option.label}
        </button>
      ))}
    </div>
  );
}
export function PlayQuickFilters({
  statuses,
  onChange,
}: {
  statuses: PlayStatus[];
  onChange: (statuses: PlayStatus[]) => void;
}) {
  return (
    <div className="play-quick-filters" role="group" aria-label="游玩状态快捷筛选">
      {Object.entries(playLabels).map(([key, label]) => {
        const status = key as PlayStatus;
        const appearance = playAppearance[status];
        const active = statuses.length === 1 && statuses[0] === status;
        return (
          <button
            key={status}
            type="button"
            className={`quick-filter-chip play-${status.toLowerCase()}`}
            aria-pressed={active}
            onClick={() => onChange(active ? [] : [status])}
          >
            <Icon name={appearance.icon} size={16} />
            {label}
          </button>
        );
      })}
    </div>
  );
}
export function LibraryFilters({
  games,
  value,
  onChange,
  tags = [],
  onManageTags,
}: {
  tags?: LibraryTag[];
  onManageTags?: () => void;
  games: Game[];
  value: GameFilters;
  onChange: (value: GameFilters) => void;
}) {
  const [open, setOpen] = useState(false);
  const [draft, setDraft] = useState<GameFilters>(emptyFilters);
  useEffect(() => {
    setDraft((current) => {
      const remaining = (current.tags ?? []).filter((id) => tags.some((tag) => tag.id === id));
      return remaining.length === (current.tags?.length ?? 0)
        ? current
        : { ...current, tags: remaining };
    });
  }, [tags]);
  const count =
    value.statuses.length +
    value.engines.length +
    value.modes.length +
    (value.tags?.length ?? 0) +
    Number(!!value.favoritesOnly);
  const engines = useMemo(
    () =>
      [...new Set([...supportedEngines, ...games.map((g) => g.engine), ...value.engines])].sort(
        (a, b) =>
          Number(a === 'Unknown') - Number(b === 'Unknown') ||
          engineLabel(a).localeCompare(engineLabel(b), 'zh-CN'),
      ),
    [games, value.engines],
  );
  const toggle = (group: FilterGroup, key: string) => {
    setDraft((previous) => {
      const current: string[] = previous[group] ?? [];
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
        className={`library-filter-button library-filter-trigger ${count ? 'active' : ''}`}
        aria-label={count ? `筛选 (${count})` : '筛选'}
        aria-pressed={count > 0}
        aria-haspopup="dialog"
        onClick={() => {
          setDraft(value);
          setOpen(true);
        }}
      >
        <Icon name="filter" size={16} />
        筛选
        {count > 0 && (
          <span className="library-filter-count" aria-hidden="true">
            {count}
          </span>
        )}
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
            <fieldset className="filter-group-favorites">
              <legend>收藏</legend>
              <div className="filter-options filter-options-favorites">
                <label
                  className={`filter-option filter-tone-pink${draft.favoritesOnly ? ' selected' : ''}`}
                >
                  <input
                    type="checkbox"
                    checked={!!draft.favoritesOnly}
                    onChange={(event) =>
                      setDraft((current) => ({ ...current, favoritesOnly: event.target.checked }))
                    }
                  />
                  <span className="filter-option-icon">
                    <Icon name="star" size={18} filled={!!draft.favoritesOnly} />
                  </span>
                  <span className="filter-option-label">只看收藏</span>
                </label>
              </div>
            </fieldset>
            {(
              [
                ['statuses', '游玩状态', Object.entries(playLabels)],
                ['engines', '游戏引擎', engines.map((e) => [e, engineLabel(e)])],
                ['modes', '启动方式', launchModes.map((m) => [m, m])],
                ['tags', '自定义标签（同时满足）', tags.map((tag) => [tag.id, tag.name])],
              ] as [FilterGroup, string, string[][]][]
            ).map(([group, title, options]) => (
              <fieldset key={group} className={`filter-group-${group}`}>
                <legend>{title}</legend>
                <div className={`filter-options filter-options-${group}`}>
                  {options.map(([key, label]) => {
                    const appearance = filterAppearance(group, key);
                    const selected = (draft[group] ?? []).includes(key);
                    return (
                      <label
                        key={key}
                        className={`filter-option filter-tone-${appearance.tone}${selected ? ' selected' : ''}`}
                        title={group === 'engines' ? key : undefined}
                      >
                        <input
                          type="checkbox"
                          checked={selected}
                          onChange={() => toggle(group, key)}
                        />
                        <span className="filter-option-icon">
                          <Icon name={appearance.icon} size={18} />
                        </span>
                        <span className="filter-option-label">{label}</span>
                      </label>
                    );
                  })}
                </div>
                {group === 'tags' && (
                  <>
                    {!options.length && <p className="muted">尚无标签，可在游戏详情中添加。</p>}
                    {onManageTags && (
                      <button type="button" className="manage-tags-button" onClick={onManageTags}>
                        <Icon name="tag" size={14} />
                        管理标签
                      </button>
                    )}
                  </>
                )}
              </fieldset>
            ))}
          </div>
        </Modal>
      )}
    </>
  );
}
