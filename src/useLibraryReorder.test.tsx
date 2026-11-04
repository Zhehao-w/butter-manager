import { act, renderHook } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { useLibraryReorder } from './useLibraryReorder';

describe('launch reorder motion', () => {
  function fixture(reduced = false) {
    const element = document.createElement('div');
    const row = document.createElement('div');
    row.dataset.libraryGame = 'game';
    row.style.transform = 'translateY(200px)';
    element.append(row);
    element.scrollTop = 150;
    let top = 200;
    vi.spyOn(row, 'getBoundingClientRect').mockImplementation(() => ({ top, left: 0 }) as DOMRect);
    const animation = { cancel: vi.fn(), onfinish: null };
    const animate = vi.fn().mockReturnValue(animation);
    row.animate = animate;
    vi.stubGlobal('matchMedia', vi.fn().mockReturnValue({ matches: reduced }));
    return { element, row, animation, animate, move: () => (top = 120) };
  }

  it('preserves scroll and virtual transforms while moving visible items', () => {
    const f = fixture();
    const view = renderHook(() => useLibraryReorder({ current: f.element }, 'list', true));
    act(() => view.result.current());
    f.move();
    f.element.scrollTop = 0;
    view.rerender();
    expect(f.element.scrollTop).toBe(150);
    expect(f.animate).toHaveBeenCalledWith(
      [{ transform: 'translate(0px, 80px) translateY(200px)' }, { transform: 'translateY(200px)' }],
      { duration: 180, easing: 'cubic-bezier(0.2, 0.8, 0.2, 1)' },
    );
    view.unmount();
    expect(f.animation.cancel).toHaveBeenCalledOnce();
  });

  it('does not animate ordinary redraws without a launch capture', () => {
    const f = fixture();
    const view = renderHook(() => useLibraryReorder({ current: f.element }, 'list', true));
    f.move();
    view.rerender();
    expect(f.animate).not.toHaveBeenCalled();
  });

  it('respects reduced motion and still preserves the viewport', () => {
    const f = fixture(true);
    const view = renderHook(() => useLibraryReorder({ current: f.element }, 'list', true));
    act(() => view.result.current());
    f.move();
    f.element.scrollTop = 0;
    view.rerender();
    expect(f.element.scrollTop).toBe(150);
    expect(f.animate).not.toHaveBeenCalled();
  });

  it('discards pending motion when the user changes the sort or view', () => {
    const f = fixture();
    const view = renderHook(({ scope }) => useLibraryReorder({ current: f.element }, scope, true), {
      initialProps: { scope: 'list' },
    });
    act(() => view.result.current());
    f.move();
    view.rerender({ scope: 'grid' });
    expect(f.animate).not.toHaveBeenCalled();
  });
});
