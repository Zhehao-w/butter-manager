import { vi } from 'vitest';
import type { api } from '../api';

type Api = typeof api;

// Align mock keys with the real API, without invoking any native command.
export function createApiMock(actual: Api): Api {
  const mocked = Object.fromEntries(
    Object.keys(actual).map((name) => [name, vi.fn()]),
  ) as unknown as Api;
  resetApiMock(mocked);
  return mocked;
}

export function resetApiMock(mocked: Api) {
  for (const [name, fn] of Object.entries(mocked)) {
    if (vi.isMockFunction(fn))
      fn.mockReset().mockRejectedValue(new Error(`Unconfigured API mock: ${name}`));
  }
  const mocks = vi.mocked(mocked);
  for (const fn of [
    mocks.importPlans,
    mocks.libraryTags,
    mocks.importRecoveryIssues,
    mocks.launchHistory,
    mocks.versionHistory,
    mocks.duplicateResults,
    mocks.duplicateGameReferences,
    mocks.duplicateReviews,
    mocks.syncScanMtool,
  ])
    fn.mockResolvedValue([]);
  for (const fn of [
    mocks.openDataDirectory,
    mocks.openImportRecords,
    mocks.openFolder,
    mocks.openDirectory,
    mocks.openSaveFolder,
    mocks.releaseExternalSaves,
    mocks.cancel,
  ])
    fn.mockResolvedValue(undefined);
  mocks.listEditableSaves.mockResolvedValue({ detected_locations: [], slots: [], warnings: [] });
  mocks.previewRenpyAppdata.mockResolvedValue({
    token: null,
    path: null,
    reason: '对应 AppData 存档目录不存在',
  });
  mocks.startLibraryCheck.mockResolvedValue('');
  mocks.previewMtoolLaunch.mockResolvedValue({
    shared_root: 'D:/Tools/MTool',
    target_exe: '任意.exe',
    architecture: 'x86',
    loader: 'loaders/mzHook32.dll',
    runtime: 'MTool.exe',
    working_directory: '.',
  });
}
