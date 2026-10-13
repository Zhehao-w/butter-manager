import { afterEach, vi } from 'vitest';

// Pure data tests use Node; only DOM suites need React cleanup and native-dialog doubles.
if (typeof document !== 'undefined') {
  const { cleanup } = await import('@testing-library/react');
  HTMLDialogElement.prototype.showModal = function () {
    this.open = true;
  };
  HTMLDialogElement.prototype.close = function () {
    this.open = false;
  };
  window.scrollTo = vi.fn();
  afterEach(() => {
    cleanup();
    localStorage.clear();
  });
}
afterEach(() => vi.unstubAllGlobals());
