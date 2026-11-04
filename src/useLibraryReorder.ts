import { useLayoutEffect, useRef, type RefObject } from 'react';

type Snapshot = {
  element: HTMLElement;
  scope: string;
  scrollTop: number;
  scrollLeft: number;
  positions: Map<string, DOMRect>;
};

// Capture immediately before a successful launch updates the sorted data.
// Animate only mounted items; virtual row transforms remain part of the final pose.
export function useLibraryReorder(
  container: RefObject<HTMLElement | null>,
  scope: string,
  enabled: boolean,
) {
  const pending = useRef<Snapshot | null>(null);
  const animations = useRef(new Set<Animation>());
  const cancel = () => {
    for (const animation of animations.current) animation.cancel();
    animations.current.clear();
  };
  useLayoutEffect(() => {
    const snapshot = pending.current;
    pending.current = null;
    if (!snapshot || !enabled || snapshot.scope !== scope) return;
    const element = container.current;
    if (!element || element !== snapshot.element) return;
    element.scrollTop = snapshot.scrollTop;
    element.scrollLeft = snapshot.scrollLeft;
    const win = element.ownerDocument.defaultView;
    if (!win || win.matchMedia?.('(prefers-reduced-motion: reduce)').matches) return;
    const limit = Math.min(120, element.clientHeight || 120);
    for (const row of element.querySelectorAll<HTMLElement>('[data-library-game]')) {
      const before = snapshot.positions.get(row.dataset.libraryGame!);
      if (!before || typeof row.animate !== 'function') continue;
      const after = row.getBoundingClientRect();
      const dx = Math.max(-limit, Math.min(limit, before.left - after.left));
      const dy = Math.max(-limit, Math.min(limit, before.top - after.top));
      if (Math.abs(dx) < 1 && Math.abs(dy) < 1) continue;
      const transform = win.getComputedStyle(row).transform || 'none';
      const base = transform === 'none' ? '' : ` ${transform}`;
      const animation = row.animate(
        [{ transform: `translate(${dx}px, ${dy}px)${base}` }, { transform }],
        { duration: 180, easing: 'cubic-bezier(0.2, 0.8, 0.2, 1)' },
      );
      animations.current.add(animation);
      animation.onfinish = () => animations.current.delete(animation);
    }
  });
  useLayoutEffect(() => cancel, []);
  return () => {
    const element = container.current;
    if (!enabled || !element) return;
    const positions = new Map<string, DOMRect>();
    for (const row of element.querySelectorAll<HTMLElement>('[data-library-game]')) {
      positions.set(row.dataset.libraryGame!, row.getBoundingClientRect());
    }
    cancel();
    pending.current = {
      element,
      scope,
      scrollTop: element.scrollTop,
      scrollLeft: element.scrollLeft,
      positions,
    };
  };
}
