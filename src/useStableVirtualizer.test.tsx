import { act, fireEvent, renderHook } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { useStableVirtualizer } from './useStableVirtualizer';

function fixture() {
  const scroll = document.createElement('div');
  const row = document.createElement('div');
  row.dataset.index = '0';
  scroll.append(row);
  document.body.append(scroll);
  for (const [key, value] of Object.entries({
    clientWidth: 288,
    clientHeight: 200,
    scrollWidth: 288,
    scrollHeight: 10000,
  }))
    Object.defineProperty(scroll, key, { value });
  scroll.getBoundingClientRect = () => ({
    left: 0,
    top: 0,
    right: 300,
    bottom: 200,
    width: 300,
    height: 200,
    x: 0,
    y: 0,
    toJSON: () => ({}),
  });
  const scrollTo = vi.fn();
  scroll.scrollTo = scrollTo;
  let offset!: (offset: number, scrolling: boolean) => void;
  const hook = renderHook(() =>
    useStableVirtualizer(
      {
        count: 100,
        getScrollElement: () => scroll,
        estimateSize: () => 100,
        observeElementRect: (_instance, update) => {
          update({ width: 300, height: 200 });
        },
        observeElementOffset: (_instance, update) => {
          offset = update;
          update(0, false);
        },
      },
      'fixture',
    ),
  );
  const resize = (height: number) => {
    const instance = hook.result.current.virtualizer;
    const entry = {
      target: row,
      borderBoxSize: [{ blockSize: height, inlineSize: 288 }],
    } as unknown as ResizeObserverEntry;
    instance.resizeItem(0, instance.options.measureElement(row, entry, instance));
  };
  return {
    hook,
    scroll,
    scrollTo,
    resize,
    offset: (value: number, scrolling: boolean) => offset(value, scrolling),
    dispose: () => {
      hook.unmount();
      scroll.remove();
    },
  };
}

describe('stable dynamic scrolling', () => {
  it('keeps the total extent and scroll position stable while scrolling, then anchors at rest', () => {
    const f = fixture();
    act(() => f.resize(240));
    act(() => f.offset(1000, true));
    const total = f.hook.result.current.virtualizer.getTotalSize();
    f.scrollTo.mockClear();
    act(() => f.resize(500));
    expect(f.hook.result.current.virtualizer.getTotalSize()).toBe(total);
    expect(f.scrollTo).not.toHaveBeenCalled();
    act(() => f.offset(1000, false));
    act(() => f.resize(500));
    expect(f.hook.result.current.virtualizer.getTotalSize()).toBe(total + 260);
    expect(f.scrollTo).toHaveBeenCalledWith(expect.objectContaining({ top: 1260 }));
    f.dispose();
  });

  it('holds measurements through a paused scrollbar drag and releases on pointerup or blur', () => {
    const f = fixture();
    act(() => f.resize(240));
    const total = f.hook.result.current.virtualizer.getTotalSize();
    fireEvent(
      f.scroll,
      new MouseEvent('pointerdown', { bubbles: true, button: 0, clientX: 295, clientY: 80 }),
    );
    expect(f.hook.result.current.scrolling).toBe(true);
    act(() => f.resize(500));
    expect(f.hook.result.current.virtualizer.getTotalSize()).toBe(total);
    fireEvent(window, new MouseEvent('pointerup'));
    expect(f.hook.result.current.scrolling).toBe(false);
    act(() => f.resize(500));
    expect(f.hook.result.current.virtualizer.getTotalSize()).toBe(total + 260);
    fireEvent(
      f.scroll,
      new MouseEvent('pointerdown', { bubbles: true, button: 0, clientX: 295, clientY: 80 }),
    );
    fireEvent(window, new Event('blur'));
    expect(f.hook.result.current.scrolling).toBe(false);
    fireEvent(
      f.scroll,
      new MouseEvent('pointerdown', { bubbles: true, button: 0, clientX: 50, clientY: 80 }),
    );
    expect(f.hook.result.current.scrolling).toBe(false);
    f.dispose();
  });
});
