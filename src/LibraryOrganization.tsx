import { useState } from 'react';
import type { ReactNode } from 'react';
import { api } from './api';
import { Icon, Modal } from './ui';
import { useWindowCloseGuard } from './windowClose';
import type { Game, LibraryTag, PlayStatus } from './types';
import { playLabels } from './gameStatus';

export const tagKey = (name: string) =>
  name.trim().normalize('NFKC').toLowerCase().replace(/\s+/gu, ' ');
export function appendTag(tags: LibraryTag[], name: string, available: LibraryTag[]) {
  const key = tagKey(name);
  if (!key || tags.some((tag) => tagKey(tag.name) === key)) return tags;
  return [
    ...tags,
    available.find((tag) => tagKey(tag.name) === key) ?? { id: '', name: name.trim() },
  ];
}

export function FavoriteButton({
  game,
  disabled,
  onChange,
}: {
  game: Game;
  disabled: boolean;
  onChange: () => void;
}) {
  const label = `${game.favorite ? '取消收藏' : '收藏'} ${game.display_title}`;
  return (
    <button
      type="button"
      className={`favorite-button ${game.favorite ? 'is-favorite' : ''}`}
      aria-label={label}
      title={label}
      aria-pressed={game.favorite}
      disabled={disabled}
      onClick={(event) => {
        event.stopPropagation();
        onChange();
      }}
    >
      <Icon name="star" size={17} filled={game.favorite} />
    </button>
  );
}

export function LibrarySelectAll({
  selected,
  total,
  disabled,
  onChange,
}: {
  selected: number;
  total: number;
  disabled: boolean;
  onChange: (checked: boolean) => void;
}) {
  return (
    <input
      type="checkbox"
      aria-label="全选当前筛选结果"
      title={`全选当前筛选结果（${total}）`}
      ref={(element) => {
        if (element) element.indeterminate = selected > 0 && selected < total;
      }}
      checked={total > 0 && selected === total}
      disabled={!total || disabled}
      onChange={(event) => onChange(event.target.checked)}
    />
  );
}

export function LibraryBatchActions({
  selected,
  total,
  disabled,
  waiting,
  busy,
  selectAll,
  onStatus,
  onFavorite,
  onClear,
}: {
  selected: number;
  total: number;
  disabled: boolean;
  waiting: boolean;
  busy: boolean;
  selectAll?: ReactNode;
  onStatus: (status: PlayStatus) => void;
  onFavorite: (favorite: boolean) => void;
  onClear: () => void;
}) {
  const locked = !selected || disabled || waiting;
  const title = waiting ? '启动请求完成后可修改' : !selected ? '请先选择游戏' : undefined;
  return (
    <div className="library-batch-actions" role="group" aria-label="批量操作" aria-busy={busy}>
      {selectAll && <label className="library-select-all">{selectAll}全选</label>}
      <span className="library-selected-count" role="status">
        已选 {selected} 个<span> / 共 {total} 个</span>
      </span>
      <div className="library-batch-buttons">
        <div className="play-quick-filters" role="group" aria-label="批量游玩状态">
          <span className="quick-filter-label">标记为</span>
          {(Object.keys(playLabels) as PlayStatus[]).map((status) => (
            <button
              type="button"
              key={status}
              className={`play-${status.toLowerCase()}`}
              aria-label={`批量标记为${playLabels[status]}`}
              disabled={locked}
              title={title}
              onClick={() => onStatus(status)}
            >
              <Icon
                name={status === 'UNPLAYED' ? 'library' : status === 'PLAYING' ? 'play' : 'flag'}
                size={14}
              />
              {playLabels[status]}
            </button>
          ))}
        </div>
        <div className="library-batch-favorites" role="group" aria-label="批量收藏">
          <button
            type="button"
            className="batch-favorite-add"
            aria-label="批量收藏"
            disabled={locked}
            title={title}
            onClick={() => onFavorite(true)}
          >
            <Icon name="star" size={14} filled />
            收藏
          </button>
          <button
            type="button"
            aria-label="批量取消收藏"
            disabled={locked}
            title={title}
            onClick={() => onFavorite(false)}
          >
            <Icon name="star" size={14} />
            取消收藏
          </button>
        </div>
      </div>
      <button
        type="button"
        className="library-clear-selection"
        disabled={!selected || disabled}
        onClick={onClear}
      >
        取消选择
      </button>
    </div>
  );
}

export function GameTags({ tags }: { tags: LibraryTag[] }) {
  if (!tags.length) return null;
  return (
    <span className="game-tags" title={tags.map((tag) => tag.name).join(' · ')}>
      {tags.slice(0, 2).map((tag) => (
        <span className="game-tag" key={tag.id || tag.name}>
          {tag.name}
        </span>
      ))}
      {tags.length > 2 && <span className="game-tag tag-overflow">+{tags.length - 2}</span>}
    </span>
  );
}

export function TagEditor({
  tags,
  available,
  value,
  onInput,
  onChange,
}: {
  tags: LibraryTag[];
  available: LibraryTag[];
  value: string;
  onInput: (value: string) => void;
  onChange: (tags: LibraryTag[]) => void;
}) {
  const add = () => {
    onChange(appendTag(tags, value, available));
    onInput('');
  };
  return (
    <fieldset className="tag-editor">
      <legend>自定义标签</legend>
      <div className="tag-editor-selected">
        {tags.map((tag, index) => (
          <button
            type="button"
            className="game-tag"
            key={tag.id || tag.name}
            title={`移除标签 ${tag.name}`}
            aria-label={`移除标签 ${tag.name}`}
            onClick={() => onChange(tags.filter((_, i) => i !== index))}
          >
            <span>{tag.name}</span>
            <Icon name="close" size={12} />
          </button>
        ))}
        {!tags.length && <span className="muted">尚未添加标签</span>}
      </div>
      <div className="tag-editor-input">
        <input
          aria-label="新标签"
          placeholder="输入标签名称"
          value={value}
          onChange={(event) => onInput(event.target.value)}
          onKeyDown={(event) => {
            if (event.key === 'Enter' && !event.nativeEvent.isComposing) {
              event.preventDefault();
              add();
            }
          }}
        />
        <button type="button" disabled={!value.trim() || tags.length >= 32} onClick={add}>
          添加
        </button>
      </div>
      <span className="muted">每个标签最多 40 个字符，每个游戏最多 32 个；保存资料后生效。</span>
      {available.some((tag) => !tags.some((selected) => selected.id === tag.id)) && (
        <details>
          <summary>选择已有标签</summary>
          <div className="tag-editor-options">
            {available
              .filter((tag) => !tags.some((selected) => selected.id === tag.id))
              .map((tag) => (
                <button
                  type="button"
                  key={tag.id}
                  disabled={tags.length >= 32}
                  onClick={() => onChange([...tags, tag])}
                >
                  {tag.name}
                </button>
              ))}
          </div>
        </details>
      )}
    </fieldset>
  );
}

export function TagManager({
  tags,
  games,
  locked,
  onChanged,
  onClose,
}: {
  tags: LibraryTag[];
  games: Game[];
  locked: boolean;
  onChanged: () => Promise<void>;
  onClose: () => void;
}) {
  const [editing, setEditing] = useState<LibraryTag | null>(null);
  const [name, setName] = useState('');
  const [deleting, setDeleting] = useState<LibraryTag | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');
  const [discard, setDiscard] = useState(false);
  const dirty = !!editing && name !== editing.name;
  async function run(action: () => Promise<void>) {
    if (busy || locked) return false;
    setBusy(true);
    setError('');
    try {
      await action();
      await onChanged();
      setEditing(null);
      setDeleting(null);
      return true;
    } catch (reason) {
      setError(String(reason));
      return false;
    } finally {
      setBusy(false);
    }
  }
  const rename = () =>
    editing ? run(() => api.renameTag(editing.id, name)) : Promise.resolve(true);
  useWindowCloseGuard({ dirty, busy, save: rename });
  return (
    <Modal
      title="管理标签"
      variant="confirm"
      onClose={() => {
        if (!busy) {
          if (dirty) setDiscard(true);
          else onClose();
        }
      }}
      footer={
        <button
          type="button"
          disabled={busy}
          onClick={() => (dirty ? setDiscard(true) : onClose())}
        >
          关闭
        </button>
      }
    >
      <p className="muted">重命名会同步所有关联游戏；删除只移除标签及关联标记。</p>
      {error && <p role="alert">{error}</p>}
      <div className="tag-manager-list">
        {tags.map((tag) => (
          <div className="tag-manager-row" key={tag.id}>
            <span className="tag-manager-name" title={tag.name}>
              {tag.name}
            </span>
            <span className="muted">
              {games.filter((game) => game.tags.some((value) => value.id === tag.id)).length} 个游戏
            </span>
            <button
              type="button"
              disabled={busy || locked || !!editing || !!deleting}
              aria-label={`重命名标签 ${tag.name}`}
              onClick={() => {
                setEditing(tag);
                setName(tag.name);
              }}
            >
              重命名
            </button>
            <button
              type="button"
              className="danger-text"
              disabled={busy || locked || !!editing || !!deleting}
              aria-label={`删除标签 ${tag.name}`}
              onClick={() => setDeleting(tag)}
            >
              删除
            </button>
          </div>
        ))}
        {!tags.length && <p className="muted">暂无标签，可在游戏详情中添加。</p>}
      </div>
      {editing && (
        <form
          className="tag-manager-edit"
          onSubmit={(event) => {
            event.preventDefault();
            void rename();
          }}
        >
          <label>
            标签名称
            <input
              autoFocus
              required
              value={name}
              onChange={(event) => setName(event.target.value)}
              disabled={busy || locked}
            />
          </label>
          <div className="header-actions">
            <button type="button" disabled={busy} onClick={() => setEditing(null)}>
              取消
            </button>
            <button type="submit" className="primary" disabled={!dirty || busy || locked}>
              保存名称
            </button>
          </div>
        </form>
      )}
      {deleting && (
        <div className="tag-manager-edit" role="group" aria-label="确认删除标签">
          <p>删除「{deleting.name}」及所有游戏上的此标记？</p>
          <div className="header-actions">
            <button type="button" disabled={busy} onClick={() => setDeleting(null)}>
              取消
            </button>
            <button
              type="button"
              className="danger"
              disabled={busy || locked}
              onClick={() => void run(() => api.deleteTag(deleting.id))}
            >
              确认删除标签
            </button>
          </div>
        </div>
      )}
      {discard && (
        <Modal
          title="放弃标签名称修改？"
          variant="confirm"
          onClose={() => setDiscard(false)}
          footer={
            <>
              <button type="button" onClick={() => setDiscard(false)}>
                继续编辑
              </button>
              <button type="button" onClick={onClose}>
                放弃修改
              </button>
            </>
          }
        >
          <p>尚未保存的名称修改将被放弃。</p>
        </Modal>
      )}
    </Modal>
  );
}
