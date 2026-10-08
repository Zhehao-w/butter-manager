import { useLayoutEffect, useRef, useState } from 'react';
import { measureElement, useVirtualizer } from '@tanstack/react-virtual';

type Options = Parameters<typeof useVirtualizer<HTMLDivElement, HTMLDivElement>>[0];

// Dynamic cards keep their measured (or estimated) extent while scrolling.
// Removing the temporary height at rest lets ResizeObserver measure natural
// content again; only rows fully above the viewport may compensate scrollTop.
export function useStableVirtualizer(options: Options, scope: string) {
  const dragging = useRef(false);
  const [draggingScrollbar, setDraggingScrollbar] = useState(false);
  const virtualizer = useVirtualizer<HTMLDivElement, HTMLDivElement>({
    ...options,
    useCachedMeasurements: options.enabled === false,
    useAnimationFrameWithResizeObserver: true,
    measureElement: (element, entry, instance) => {
      if (dragging.current || instance.isScrolling) {
        const index = instance.indexFromElement(element);
        const key = instance.options.getItemKey(index);
        return instance.itemSizeCache.get(key) ?? instance.options.estimateSize(index);
      }
      return measureElement(element, entry, instance);
    },
  });
  virtualizer.shouldAdjustScrollPositionOnItemSizeChange = (item, _delta, instance) =>
    !dragging.current && !instance.isScrolling && item.end <= (instance.scrollOffset ?? 0);

  useLayoutEffect(() => {
    const element = options.getScrollElement();
    if (!element || options.enabled === false) return;
    const win = element.ownerDocument.defaultView;
    if (!win) return;
    const end = () => {
      dragging.current = false;
      setDraggingScrollbar(false);
    };
    const start = (event: PointerEvent) => {
      if (event.button !== 0 || event.pointerType === 'touch') return;
      const rect = element.getBoundingClientRect();
      const right = rect.left + element.clientLeft + element.clientWidth;
      const bottom = rect.top + element.clientTop + element.clientHeight;
      const vertical =
        element.scrollHeight > element.clientHeight &&
        event.clientX >= right &&
        event.clientX < rect.right &&
        event.clientY >= rect.top &&
        event.clientY < bottom;
      const horizontal =
        element.scrollWidth > element.clientWidth &&
        event.clientY >= bottom &&
        event.clientY < rect.bottom &&
        event.clientX >= rect.left &&
        event.clientX < right;
      if (!vertical && !horizontal) return;
      dragging.current = true;
      setDraggingScrollbar(true);
    };
    element.addEventListener('pointerdown', start, { capture: true, passive: true });
    win.addEventListener('pointerup', end);
    win.addEventListener('pointercancel', end);
    win.addEventListener('blur', end);
    return () => {
      element.removeEventListener('pointerdown', start, true);
      win.removeEventListener('pointerup', end);
      win.removeEventListener('pointercancel', end);
      win.removeEventListener('blur', end);
      dragging.current = false;
      setDraggingScrollbar(false);
    };
    // scope changes when the same component switches its scroll surface.
  }, [virtualizer, options.enabled, scope]);

  return { virtualizer, scrolling: draggingScrollbar || virtualizer.isScrolling };
}
