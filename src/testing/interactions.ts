import { screen, waitFor, type ByRoleOptions } from '@testing-library/react';
import { expect } from 'vitest';

// Keep readiness assertions explicit; callers still decide when to click or cancel.
export async function waitForEnabledButton(
  name: ByRoleOptions['name'],
  scope: Pick<typeof screen, 'getByRole'> = screen,
) {
  await waitFor(() => {
    expect((scope.getByRole('button', { name }) as HTMLButtonElement).disabled).toBe(false);
  });
}
