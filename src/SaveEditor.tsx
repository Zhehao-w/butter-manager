import { useEffect, useMemo, useRef, useState, type CSSProperties } from 'react';
import { useVirtualizer } from '@tanstack/react-virtual';
import { api } from './api';
import { Modal, SearchField } from './ui';
import type { Game } from './types';
import type { SaveCatalog, SaveDocument, SaveField, SaveChange } from './saveEditorTypes';

const rowHeight = 56;
const signatureLabels = {
  local: '本机可信',
  foreign: '外来有效',
  unsigned: '无签名',
  invalid: '签名无效',
  unknown: '无法判断',
};
const categories: Record<string, string> = {
  variables: '变量 / 数值',
  switches: '开关',
  inventory: '背包',
  actors: '角色',
  persistent: 'Persistent',
};
export function SaveEditor({ game, onClose }: { game: Game; onClose: () => void }) {
  const [catalog, setCatalog] = useState<SaveCatalog>({ slots: [], warnings: [] });
  const [document, setDocument] = useState<SaveDocument | null>(null);
  const [slotId, setSlotId] = useState('');
  const [category, setCategory] = useState('variables');
  const [search, setSearch] = useState('');
  const [drafts, setDrafts] = useState<Record<string, string | boolean>>({});
  const [busy, setBusy] = useState(true);
  const [error, setError] = useState('');
  const [notice, setNotice] = useState('');
  const [pending, setPending] = useState<null | (() => void)>(null);
  const [pendingTrust, setPendingTrust] = useState<null | (() => void)>(null);
  const trusted = useRef(new Set<string>());
  const lastNormalSlot = useRef('');
  const externalSlots = useRef<SaveCatalog['slots']>([]);
  const alive = useRef(true);
  const active = useRef(false);
  const viewport = useRef<HTMLDivElement>(null);
  async function load(id: string) {
    setDrafts({});
    setSearch('');
    if (!id) {
      setDocument(null);
      setSlotId('');
      return;
    }
    const next = await api.readEditableSave(game.id, id);
    if (!alive.current) return;
    setSlotId(id);
    setDocument(next);
    if (next.slot.format === 'RenPy') lastNormalSlot.current = next.slot.id;
    setCategory(next.slot.format === 'Persistent' ? 'persistent' : 'variables');
  }
  async function run(action: () => Promise<void>) {
    if (active.current) return;
    active.current = true;
    setBusy(true);
    setError('');
    setNotice('');
    try {
      await action();
    } catch (reason) {
      if (alive.current) setError(String(reason));
    } finally {
      active.current = false;
      if (alive.current) setBusy(false);
    }
  }
  async function refresh() {
    const next = await api.listEditableSaves(game.id);
    if (!alive.current) return;
    next.slots = [...next.slots, ...externalSlots.current];
    setCatalog(next);
    await load(next.slots.some((slot) => slot.id === slotId) ? slotId : (next.slots[0]?.id ?? ''));
  }
  useEffect(() => {
    alive.current = true;
    void run(refresh);
    return () => {
      alive.current = false;
      void api.releaseExternalSaves(game.id).catch(() => {});
    };
    // The editor is mounted per game, and its first discovery uses no stale slot selection.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [game.id]);
  const dirty = Object.keys(drafts).length > 0;
  function guarded(action: () => void) {
    if (active.current) return;
    if (dirty) setPending(() => action);
    else action();
  }
  function edit(field: SaveField, value: string | boolean) {
    setNotice('');
    setDrafts((current) => {
      const next = { ...current };
      const original = field.kind === 'boolean' ? field.value : String(field.value ?? '');
      if (value === original) delete next[field.id];
      else next[field.id] = value;
      return next;
    });
  }
  async function save() {
    if (!document) return;
    await run(async () => {
      const changes: SaveChange[] = [];
      for (const field of document.fields) {
        if (!(field.id in drafts)) continue;
        const draft = drafts[field.id];
        if (field.kind === 'number') {
          const value = Number(draft);
          if (
            String(draft).trim() === '' ||
            !Number.isFinite(value) ||
            Math.abs(value) > Number.MAX_SAFE_INTEGER
          )
            throw new Error(`${field.name}：请输入有效数值`);
          changes.push({ id: field.id, value });
        } else changes.push({ id: field.id, value: draft });
      }
      const next = document.slot.external
        ? await api.applySaveEdits(
            game.id,
            document.slot.id,
            document.revision,
            changes,
            trusted.current.has(document.slot.id),
          )
        : await api.applySaveEdits(game.id, document.slot.id, document.revision, changes);
      if (!alive.current) return;
      setDocument(next);
      setDrafts({});
      setNotice('已保存，请回游戏重新读档。');
    });
  }
  function trustBeforeWrite(action: () => void) {
    if (
      document &&
      (document.slot.external || (document.signature && document.signature.status !== 'local')) &&
      !trusted.current.has(document.slot.id)
    )
      setPendingTrust(() => action);
    else action();
  }
  async function chooseExternal() {
    await run(async () => {
      const next = await api.chooseExternalRenpySave(game.id);
      if (!next || !alive.current) return;
      externalSlots.current.push(next.slot);
      setCatalog((current) => ({ ...current, slots: [...current.slots, next.slot] }));
      setDocument(next);
      setSlotId(next.slot.id);
      setDrafts({});
      setSearch('');
      setCategory(next.slot.format === 'Persistent' ? 'persistent' : 'variables');
      if (next.slot.format === 'RenPy') lastNormalSlot.current = next.slot.id;
    });
  }
  async function resign() {
    if (!document) return;
    await run(async () => {
      const next = await api.resignRenpySave(
        game.id,
        document.slot.id,
        document.revision,
        trusted.current.has(document.slot.id),
      );
      if (!alive.current) return;
      setDocument(next);
      setDrafts({});
      setNotice('已重新签名为本机存档，内容未修改。签名不保证不同游戏版本的兼容性。');
    });
  }
  const renpy = document?.slot.format === 'RenPy' || document?.slot.format === 'Persistent';
  const tabs = useMemo(
    () =>
      renpy
        ? ['variables', 'persistent']
        : [...new Set(document?.fields.map((field) => field.category) ?? [])],
    [document, renpy],
  );
  function categorySlot(tab: string) {
    if (tab === 'variables') {
      const remembered = catalog.slots.find(
        (slot) => slot.id === lastNormalSlot.current && slot.format === 'RenPy',
      );
      if (remembered) return remembered;
    }
    return catalog.slots.find(
      (slot) => slot.format === (tab === 'persistent' ? 'Persistent' : 'RenPy'),
    );
  }
  function chooseCategory(tab: string) {
    if (renpy && tab !== category) {
      const slot = categorySlot(tab);
      if (slot) guarded(() => void run(() => load(slot.id)));
    } else setCategory(tab);
  }
  const fields = useMemo(() => {
    const needle = search.trim().toLocaleLowerCase();
    return (document?.fields ?? []).filter(
      (field) =>
        field.category === category &&
        (!needle ||
          `${field.name} ${field.path} ${field.description ?? ''}`
            .toLocaleLowerCase()
            .includes(needle)),
    );
  }, [document, category, search]);
  const virtual = useVirtualizer({
    count: fields.length,
    getScrollElement: () => viewport.current,
    estimateSize: () => rowHeight,
    overscan: 8,
    enabled: fields.length > 80,
    getItemKey: (index) => fields[index].id,
  });
  useEffect(() => {
    if (viewport.current) viewport.current.scrollTop = 0;
    virtual.measure();
  }, [category, search, document, virtual]);
  function row(field: SaveField) {
    const changed = field.id in drafts;
    const value = changed ? drafts[field.id] : field.value;
    return (
      <div className={`save-editor-field ${changed ? 'is-changed' : ''}`} key={field.id}>
        <div className="save-editor-field-info">
          <label htmlFor={`save-${field.id}`} title={`${field.name}\n${field.path}`}>
            <strong>{field.name}</strong>
            {changed && <span className="save-editor-dirty">已修改</span>}
          </label>
          {(field.reason || field.description) && (
            <span className="muted" title={field.reason ?? field.description ?? undefined}>
              {field.reason ?? field.description}
            </span>
          )}
        </div>
        {field.editable && field.kind === 'boolean' ? (
          <input
            id={`save-${field.id}`}
            className="save-editor-switch"
            type="checkbox"
            role="switch"
            checked={Boolean(value)}
            disabled={busy}
            onChange={(event) => edit(field, event.target.checked)}
          />
        ) : field.editable ? (
          <input
            id={`save-${field.id}`}
            aria-label={field.name}
            type={field.kind === 'number' ? 'number' : 'text'}
            step="any"
            value={String(value ?? '')}
            disabled={busy}
            onChange={(event) => edit(field, event.target.value)}
          />
        ) : (
          <span className="save-editor-readonly" title={field.reason ?? undefined}>
            {field.kind === 'readonly' ? '只读' : String(field.value ?? '只读')}
          </span>
        )}
      </div>
    );
  }
  return (
    <>
      <Modal
        title={`编辑存档 · ${game.display_title}`}
        className="save-editor"
        variant="detail"
        onClose={() => guarded(onClose)}
        footer={
          <>
            <p className="muted save-editor-hint">
              游戏运行时也可修改；保存后回游戏重新读档。
              {document?.slot.format === 'Persistent' &&
                ' Persistent 可能需要重启；运行中的游戏可能再次覆盖它。'}
            </p>
            <button
              className="primary"
              disabled={busy || !dirty}
              onClick={() => trustBeforeWrite(() => void save())}
            >
              {busy && dirty
                ? '正在保存…'
                : `保存修改${dirty ? ` (${Object.keys(drafts).length})` : ''}`}
            </button>
          </>
        }
      >
        <div className="save-editor-toolbar">
          <label htmlFor="save-editor-slot">存档槽位</label>
          <select
            id="save-editor-slot"
            aria-label="存档槽位"
            value={slotId}
            disabled={busy || !catalog.slots.length}
            onChange={(event) => {
              const id = event.target.value;
              guarded(() => void run(() => load(id)));
            }}
          >
            {!catalog.slots.length && <option value="">没有发现支持的存档</option>}
            {catalog.slots.map((slot) => (
              <option key={slot.id} value={slot.id}>
                {slot.format} · {slot.name}
              </option>
            ))}
          </select>
          <button disabled={busy} onClick={() => guarded(() => void run(refresh))}>
            刷新
          </button>
        </div>
        {(renpy || game.engine === "Ren'Py") && (
          <div className="save-editor-signature">
            <span>
              签名：{document?.signature ? signatureLabels[document.signature.status] : '无法判断'}
            </span>
            <button disabled={busy} onClick={() => guarded(() => void chooseExternal())}>
              选择外部 Ren’Py 存档
            </button>
            <button
              className="save-editor-entry"
              title={document?.signature?.reason ?? undefined}
              disabled={busy || !document?.signature?.can_resign}
              onClick={() => guarded(() => trustBeforeWrite(() => void resign()))}
            >
              重新签名为本机存档
            </button>
          </div>
        )}
        {error && (
          <p className="error" role="alert">
            {error}
          </p>
        )}
        {notice && (
          <p role="status" className="save-editor-success">
            {notice}
          </p>
        )}
        {[...catalog.warnings, ...(document?.warnings ?? [])].map((warning, index) => (
          <p className="save-editor-warning" key={index}>
            {warning}
          </p>
        ))}
        {busy && !document && <p role="status">正在读取存档…</p>}
        {!busy && !catalog.slots.length && (
          <p className="muted">
            支持
            MV、MZ、Ren’Py。请确认已关联存档目录，且游戏已经保存过进度；新增的目录需要先保存资料。
          </p>
        )}
        {document && (
          <>
            <div className={`save-editor-metadata${renpy ? ' is-renpy' : ''}`}>
              <div className="save-editor-metadata-text">
                <span className="muted">
                  {new Date(document.slot.modified * 1000).toLocaleString()} ·{' '}
                  {document.slot.format}
                </span>
                {document.metadata.map((text, index) => (
                  <span key={index} title={text}>
                    {text}
                  </span>
                ))}
              </div>
              {document.screenshot && (
                <div className="save-editor-preview">
                  <img src={document.screenshot} alt="存档截图" />
                </div>
              )}
            </div>
            <div className="save-editor-filters">
              <div className="save-editor-tabs" role="tablist" aria-label="存档字段分类">
                {tabs.map((tab) => (
                  <button
                    key={tab}
                    role="tab"
                    aria-selected={category === tab}
                    disabled={busy || (renpy && !categorySlot(tab))}
                    title={
                      renpy && !categorySlot(tab)
                        ? '没有发现此类存档，请确认关联的存档目录'
                        : undefined
                    }
                    onClick={() => chooseCategory(tab)}
                  >
                    {renpy && tab === 'variables' ? 'Variables' : (categories[tab] ?? tab)}
                  </button>
                ))}
              </div>
              <SearchField
                label="存档字段"
                value={search}
                onChange={setSearch}
                placeholder="搜索名称、ID 或字段路径"
              />
            </div>
            <div
              className="save-editor-fields"
              ref={viewport}
              role="region"
              aria-label="存档字段"
              style={{ '--save-editor-row-height': `${rowHeight}px` } as CSSProperties}
            >
              {fields.length > 80 ? (
                <div style={{ height: virtual.getTotalSize(), position: 'relative' }}>
                  {virtual.getVirtualItems().map((item) => (
                    <div
                      key={item.key}
                      style={{
                        position: 'absolute',
                        width: '100%',
                        top: 0,
                        transform: `translateY(${item.start}px)`,
                        height: item.size,
                      }}
                    >
                      {row(fields[item.index])}
                    </div>
                  ))}
                </div>
              ) : (
                fields.map(row)
              )}
              {!fields.length && (
                <p className="muted save-editor-empty">
                  {search.trim() ? '未找到匹配字段，请调整搜索关键词。' : '此页暂无可显示的字段。'}
                </p>
              )}
            </div>
          </>
        )}
      </Modal>
      {pending && (
        <Modal title="放弃未保存的存档修改？" variant="confirm" onClose={() => setPending(null)}>
          <p>未保存的修改将被放弃，磁盘存档保持不变。</p>
          <div className="confirmation-actions">
            <button onClick={() => setPending(null)}>继续编辑</button>
            <button
              className="danger danger-solid"
              onClick={() => {
                const action = pending;
                setPending(null);
                setDrafts({});
                action();
              }}
            >
              放弃修改
            </button>
          </div>
        </Modal>
      )}
      {pendingTrust && (
        <Modal title="确认外部存档来源可信" variant="confirm" onClose={() => setPendingTrust(null)}>
          <p>
            仅对来源可信的存档重新签名。Ren’Py 读取 pickle
            时可能执行其中的代码；重新签名后，游戏可能不再显示外来存档警告。签名不会验证存档内容是否安全，也不保证游戏版本兼容。
          </p>
          <div className="confirmation-actions">
            <button onClick={() => setPendingTrust(null)}>取消</button>
            <button
              className="primary"
              onClick={() => {
                if (document) trusted.current.add(document.slot.id);
                const action = pendingTrust;
                setPendingTrust(null);
                action();
              }}
            >
              来源可信，继续
            </button>
          </div>
        </Modal>
      )}
    </>
  );
}
