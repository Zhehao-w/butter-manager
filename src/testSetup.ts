import { afterEach, vi } from 'vitest';
import { cleanup } from '@testing-library/react';

// DOM tests exercise React state; native dialogs and WebView rendering require desktop validation.
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
  vi.unstubAllGlobals();
});
