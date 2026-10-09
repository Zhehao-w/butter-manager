import { useEffect, useId, useRef } from 'react';
import type { ButtonHTMLAttributes, ReactNode } from 'react';
import type { Game } from './types';
import libraryIcon from './assets/lucide/gamepad-2.svg';
import scanIcon from './assets/lucide/folder-search.svg';
import importIcon from './assets/lucide/download.svg';
import toolIcon from './assets/lucide/wrench.svg';
import settingsIcon from './assets/lucide/settings.svg';
import infoIcon from './assets/lucide/info.svg';
import searchIcon from './assets/lucide/search.svg';
import listIcon from './assets/lucide/list.svg';
import gridIcon from './assets/lucide/layout-grid.svg';
import playIcon from './assets/lucide/play.svg';
import folderIcon from './assets/lucide/folder.svg';
import clockIcon from './assets/lucide/clock.svg';
import tagIcon from './assets/lucide/tag.svg';
import saveIcon from './assets/lucide/save.svg';
import arrowIcon from './assets/lucide/chevron-left.svg';
import checkIcon from './assets/lucide/check.svg';
import closeIcon from './assets/lucide/x.svg';
import nextIcon from './assets/lucide/chevron-right.svg';
import botIcon from './assets/lucide/bot.svg';
import featherIcon from './assets/lucide/feather.svg';
import flowerIcon from './assets/lucide/flower-2.svg';
import boxesIcon from './assets/lucide/boxes.svg';
import helpIcon from './assets/lucide/circle-help.svg';
import cpuIcon from './assets/lucide/cpu.svg';
import filterIcon from './assets/lucide/filter.svg';
import swordsIcon from './assets/lucide/swords.svg';
import wandIcon from './assets/lucide/wand-sparkles.svg';

// Unmodified Lucide 0.468.0 assets; see assets/lucide/README.md for upstream and licenses.
const iconAssets = {
  library: libraryIcon,
  scan: scanIcon,
  import: importIcon,
  tool: toolIcon,
  settings: settingsIcon,
  info: infoIcon,
  search: searchIcon,
  list: listIcon,
  grid: gridIcon,
  play: playIcon,
  folder: folderIcon,
  clock: clockIcon,
  tag: tagIcon,
  save: saveIcon,
  arrow: arrowIcon,
  check: checkIcon,
  close: closeIcon,
  next: nextIcon,
  bot: botIcon,
  feather: featherIcon,
  flower: flowerIcon,
  boxes: boxesIcon,
  help: helpIcon,
  cpu: cpuIcon,
  filter: filterIcon,
  swords: swordsIcon,
  wand: wandIcon,
};
export type IconName = keyof typeof iconAssets;
export function SearchField({
  label,
  placeholder,
  value,
  onChange,
}: {
  label: string;
  placeholder: string;
  value: string;
  onChange: (value: string) => void;
}) {
  const inputId = useId();
  const input = useRef<HTMLInputElement>(null);
  return (
    <div className="search">
      <Icon name="search" />
      <label className="sr-only" htmlFor={inputId}>
        {label}
      </label>
      <input
        id={inputId}
        ref={input}
        placeholder={placeholder}
        value={value}
        onChange={(event) => onChange(event.target.value)}
      />
      {value.length > 0 && (
        <button
          type="button"
          className="search-clear"
          aria-label={`清空${label}`}
          title="清空搜索"
          onMouseDown={(event) => event.preventDefault()}
          onClick={() => {
            onChange('');
            input.current?.focus();
          }}
        >
          <Icon name="close" size={16} />
        </button>
      )}
    </div>
  );
}
export function SortField({
  label,
  value,
  onChange,
  children,
}: {
  label: string;
  value: string;
  onChange: (value: string) => void;
  children: ReactNode;
}) {
  return (
    <label className="sort-control">
      排序
      <select aria-label={label} value={value} onChange={(event) => onChange(event.target.value)}>
        {children}
      </select>
    </label>
  );
}
export function BrowseButton({
  children,
  className = '',
  ...props
}: ButtonHTMLAttributes<HTMLButtonElement>) {
  return (
    <button type="button" {...props} className={`browse-button ${className}`}>
      <Icon name="folder" size={16} />
      {children}
    </button>
  );
}
export function CodeBlock({ children }: { children: ReactNode }) {
  return (
    <div className="code-scroll-shell">
      <pre>{children}</pre>
    </div>
  );
}
export function PageHeader({
  icon,
  title,
  description,
  children,
}: {
  icon: IconName;
  title: ReactNode;
  description: string;
  children?: ReactNode;
}) {
  return (
    <header className="page-heading">
      <div className="page-heading-main">
        <span className="page-icon">
          <Icon name={icon} size={30} />
        </span>
        <div className="page-heading-text">
          <h1>{title}</h1>
          <p>{description}</p>
        </div>
      </div>
      {children}
    </header>
  );
}
export function Icon({ name, size = 20 }: { name: IconName; size?: number }) {
  const mask = `url("${iconAssets[name]}")`;
  return (
    <span
      className="icon"
      aria-hidden="true"
      style={{ width: size, height: size, maskImage: mask, WebkitMaskImage: mask }}
    />
  );
}
export function GameMark({ game, large = false }: { game: Game; large?: boolean }) {
  const tones: Record<string, string> = {
    Unity: 'unity',
    "Ren'Py": 'renpy',
    'RPG Maker MV': 'rpg',
    'RPG Maker MZ': 'rpg',
    'RPG Maker (legacy)': 'rpg',
    Godot: 'godot',
    'NW.js': 'web',
    HTML: 'web',
    QSP: 'qsp',
  };
  const tone = tones[game.engine] || 'unknown';
  return (
    <span className={`game-mark tone-${tone}${large ? ' large' : ''}`} aria-hidden="true">
      <Icon name="library" size={large ? 50 : 23} />
    </span>
  );
}
export function CardTitle({
  icon,
  children,
  description,
}: {
  icon: IconName;
  children: ReactNode;
  description?: string;
}) {
  return (
    <div className="card-title">
      <span className="section-icon">
        <Icon name={icon} />
      </span>
      <div>
        <h3>{children}</h3>
        {description && <p>{description}</p>}
      </div>
    </div>
  );
}
export function Modal({
  title,
  onClose,
  children,
  variant,
  showClose = true,
  closeIconOnly = false,
  footer,
  headerActions,
  className = '',
}: {
  variant?: 'detail' | 'confirm' | 'filters' | 'record';
  showClose?: boolean;
  closeIconOnly?: boolean;
  footer?: ReactNode;
  headerActions?: ReactNode;
  className?: string;
  title: string;
  onClose: () => void;
  children: ReactNode;
}) {
  const ref = useRef<HTMLDialogElement>(null);
  useEffect(() => {
    const dialog = ref.current!;
    dialog.showModal();
    return () => dialog.close();
  }, []);
  return (
    <dialog
      ref={ref}
      className={`modal ${variant ? `modal-${variant}` : ''} ${className}`}
      aria-label={title}
      onCancel={(event) => {
        event.preventDefault();
        event.stopPropagation();
        onClose();
      }}
    >
      <div className="modal-header">
        <h2>{title}</h2>
        <div className="modal-header-actions">
          {headerActions}
          {showClose && (
            <button
              type="button"
              className={closeIconOnly ? 'modal-close-icon' : undefined}
              aria-label="关闭弹窗"
              onClick={onClose}
            >
              <Icon name="close" size={16} />
              {!closeIconOnly && '关闭'}
            </button>
          )}
        </div>
      </div>
      <div className="modal-scroll-shell">
        <div
          className="modal-body"
          role="region"
          aria-label={
            variant === 'confirm'
              ? '确认内容'
              : variant === 'filters'
                ? '筛选内容'
                : variant === 'record'
                  ? '导入记录内容'
                  : '游戏详情内容'
          }
        >
          {children}
        </div>
      </div>
      {footer && <div className="modal-footer">{footer}</div>}
    </dialog>
  );
}
