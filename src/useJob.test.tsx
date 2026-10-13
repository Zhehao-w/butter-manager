import { makeJob } from './testing/fixtures';
import { act, renderHook, waitFor } from '@testing-library/react';
import { expect, it, vi } from 'vitest';
import { api } from './api';
import { useJob } from './useJob';
import type { JobPage } from './types';
vi.mock('./api', () => ({ api: { job: vi.fn() } }));
const page: JobPage = makeJob({ id: 'new' });

it('ignores a delayed response from the previous task ID', async () => {
  let oldResponse!: (value: JobPage) => void;
  vi.mocked(api.job).mockImplementation((id) =>
    id === 'old'
      ? new Promise((resolve) => {
          oldResponse = resolve;
        })
      : Promise.resolve(page),
  );
  const completed = vi.fn();
  const { result, rerender } = renderHook(({ id }) => useJob(id, vi.fn(), completed, vi.fn()), {
    initialProps: { id: 'old' },
  });
  rerender({ id: 'new' });
  await waitFor(() => expect(result.current.page?.id).toBe('new'));
  await act(async () => {
    oldResponse({ ...page, id: 'old' });
  });
  expect(result.current.page?.id).toBe('new');
  expect(completed).toHaveBeenCalledTimes(1);
});

it('drains all result pages before reporting task completion once', async () => {
  vi.mocked(api.job).mockImplementation((_, cursor) =>
    Promise.resolve({ ...page, next_cursor: Math.min(cursor + 100, 250), change_count: 250 }),
  );
  const completed = vi.fn();
  renderHook(() => useJob('paged', vi.fn(), completed, vi.fn()));
  await waitFor(() => expect(completed).toHaveBeenCalledTimes(1));
  expect(vi.mocked(api.job).mock.calls.slice(-3)).toEqual([
    ['paged', 0],
    ['paged', 100],
    ['paged', 200],
  ]);
});
