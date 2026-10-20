import { Channel, invoke } from '@tauri-apps/api/core';
import type { SaveCatalog, SaveDocument, SaveChange } from './saveEditorTypes';
import type {
  Game,
  LibraryTag,
  PlayStatus,
  RenpyAppdataPreview,
  LibraryRelocationPreview,
  LibraryRelocationReport,
  LibraryRelocationProgress,
  DuplicatePair,
  DuplicateReview,
  DuplicateStatus,
  DuplicateGameReference,
  DeletePlan,
  DeleteReport,
  GameEdit,
  LaunchConfiguration,
  Settings,
  ResetReport,
  JobPage,
  RegistrationSelection,
  RelocateGame,
  ToolCheck,
  MToolLaunchPreview,
  ImportSelection,
  ImportMatch,
  ImportMemory,
  ImportPlan,
  ImportSourceDiscovery,
  ImportRecoveryIssue,
  ImportDuplicatePlan,
  VersionHistory,
} from './types';
export const api = {
  previewLibraryRelocation: (
    oldRoot: string,
    newRoot: string,
    progress?: (value: LibraryRelocationProgress) => void,
  ) => {
    const onProgress = new Channel<LibraryRelocationProgress>(progress ?? (() => {}));
    return invoke<LibraryRelocationPreview>('preview_library_relocation', {
      oldRoot,
      newRoot,
      onProgress,
    });
  },
  applyLibraryRelocation: (token: string) =>
    invoke<LibraryRelocationReport>('apply_library_relocation', { token }),
  previewRenpyAppdata: (id: string) => invoke<RenpyAppdataPreview>('preview_renpy_appdata', { id }),
  recycleRenpyAppdata: (id: string, token: string) =>
    invoke<void>('recycle_renpy_appdata', { id, token }),
  listEditableSaves: (gameId: string) => invoke<SaveCatalog>('list_editable_saves', { gameId }),
  readEditableSave: (gameId: string, saveId: string, password?: string) =>
    invoke<SaveDocument>('read_editable_save', { gameId, saveId, password }),
  applySaveEdits: (
    gameId: string,
    saveId: string,
    revision: string,
    changes: SaveChange[],
    sourceTrusted?: boolean,
    password?: string,
  ) =>
    invoke<SaveDocument>('apply_save_edits', {
      gameId,
      saveId,
      revision,
      changes,
      sourceTrusted,
      password,
    }),
  resignRenpySave: (gameId: string, saveId: string, revision: string, sourceTrusted: boolean) =>
    invoke<SaveDocument>('resign_renpy_save', { gameId, saveId, revision, sourceTrusted }),
  chooseExternalRenpySave: (gameId: string) =>
    invoke<SaveDocument | null>('choose_external_renpy_save', { gameId }),
  releaseExternalSaves: (gameId: string) => invoke<void>('release_external_saves', { gameId }),
  openDataDirectory: () => invoke<void>('open_data_directory'),
  versionHistory: (id: string) => invoke<VersionHistory[]>('version_history', { id }),
  rollbackVersion: (planId: string, index: number) =>
    invoke<string>('start_version_rollback', { planId, index }),
  chooseImportSources: () => invoke<string[]>('choose_import_sources'),
  discoverImportSources: (sources: string[]) =>
    invoke<ImportSourceDiscovery>('discover_import_sources', { sources }),
  importAnalyze: (sources: string[]) => invoke<string>('start_import_analysis', { sources }),
  importMatches: (scanId: string) =>
    invoke<Record<string, ImportMatch[]>>('import_matches', { scanId }),
  importMemories: (gameId: string) => invoke<ImportMemory[]>('import_memories', { gameId }),
  forgetImportMemory: (gameId: string, memoryId: string) =>
    invoke<void>('forget_import_memory', { gameId, memoryId }),
  previewImportDuplicate: (scanId: string, source: string, existingId: string, version: string) =>
    invoke<ImportDuplicatePlan>('preview_import_duplicate', {
      scanId,
      source,
      existingId,
      version,
    }),
  recycleImportDuplicate: (token: string) => invoke<void>('recycle_import_duplicate', { token }),
  importPlans: () => invoke<ImportPlan[]>('import_plans'),
  importRecoveryIssues: () => invoke<ImportRecoveryIssue[]>('import_recovery_issues'),
  openImportRecords: () => invoke<void>('open_import_records'),
  importPlan: (scanId: string, selections: ImportSelection[]) =>
    invoke<string>('start_import_plan', { scanId, selections }),
  importApply: (planId: string, withdraw = false) =>
    invoke<string>('start_import_apply', { planId, withdraw }),
  discardImportPlan: (planId: string) => invoke<void>('discard_import_plan', { planId }),
  suggestVersion: (folder: string, executable: string | null) =>
    invoke<[string, string]>('suggest_version', { folder, executable }),
  games: () => invoke<Game[]>('list_games'),
  libraryTags: () => invoke<LibraryTag[]>('library_tags'),
  setFavorite: (id: string, favorite: boolean) =>
    invoke<Game>('set_game_favorite', { id, favorite }),
  setFavorites: (ids: string[], favorite: boolean) =>
    invoke<Game[]>('set_game_favorites', { ids, favorite }),
  setPlayStatuses: (ids: string[], status: PlayStatus) =>
    invoke<Game[]>('set_game_play_statuses', { ids, status }),
  renameTag: (id: string, name: string) => invoke<void>('rename_library_tag', { id, name }),
  deleteTag: (id: string) => invoke<void>('delete_library_tag', { id }),
  gamesByIds: (ids: string[]) => invoke<Game[]>('games_by_ids', { ids }),
  syncScanMtool: (scanId: string) => invoke<Game[]>('sync_scan_mtool', { scanId }),
  previewMtoolLaunch: (
    id: string,
    executable: string | null,
    loader: string | null,
    workingDirectory: string,
  ) =>
    invoke<MToolLaunchPreview>('preview_mtool_launch', {
      id,
      executable,
      loader,
      workingDirectory,
    }),
  startLibraryCheck: () => invoke<string>('start_library_check'),
  startDuplicateCheck: () => invoke<string>('start_duplicate_check'),
  duplicateResults: (jobId: string) => invoke<DuplicatePair[]>('duplicate_results', { jobId }),
  duplicateGameReferences: (ids: string[]) =>
    invoke<DuplicateGameReference[]>('duplicate_game_references', { ids }),
  duplicateReviews: () => invoke<DuplicateReview[]>('duplicate_reviews'),
  setDuplicateReview: (leftId: string, rightId: string, status: DuplicateStatus) =>
    invoke<void>('set_duplicate_review', { leftId, rightId, status }),
  removeGame: (id: string) => invoke<void>('remove_game', { id, confirmation: id }),
  previewDelete: (id: string) => invoke<DeletePlan>('preview_game_delete', { id }),
  deleteFiles: (id: string, token: string) =>
    invoke<DeleteReport>('delete_game_files', { id, token }),
  previewRelocation: (id: string, path: string) =>
    invoke<RelocateGame>('preview_relocation', { id, path }),
  relocateGame: (change: RelocateGame) => invoke<Game>('relocate_game', { change }),
  renameGameDirectory: (id: string, expectedInstallPath: string, name: string) =>
    invoke<Game>('rename_game_directory', { id, expectedInstallPath, name }),
  checkMtool: () => invoke<ToolCheck[]>('check_mtool'),
  runMtool: () => invoke<void>('run_mtool'),
  settings: () => invoke<Settings>('get_settings'),
  saveSettings: (settings: Settings) => invoke<Settings>('save_settings', { settings }),
  startScan: () => invoke<string>('start_scan'),
  job: (id: string, cursor: number) => invoke<JobPage>('job_page', { id, cursor }),
  cancel: (id: string) => invoke<void>('cancel_job', { id }),
  skip: (id: string, path: string) => invoke<void>('skip_game', { id, path }),
  register: (scanId: string, selections: RegistrationSelection[]) =>
    invoke<string>('start_registration', { scanId, selections }),
  deepAnalyze: (scanId: string, path: string) =>
    invoke<string>('start_deep_analysis', { scanId, path }),
  saveGame: (edit: GameEdit) => invoke<Game>('save_game', { edit }),
  startGameAnalysis: (id: string) => invoke<string>('start_game_analysis', { id }),
  play: (id: string, configuration?: LaunchConfiguration) =>
    invoke<Game>('play_game', { id, configuration }),
  launchHistory: (id: string) => invoke<string[]>('launch_history', { id }),
  clearLibrary: (confirmation: string) => invoke<ResetReport>('clear_library', { confirmation }),
  openFolder: (id: string) => invoke<void>('open_game_folder', { id }),
  openDirectory: (path: string) => invoke<void>('open_directory', { path }),
  openSaveFolder: (id: string, path: string) => invoke<void>('open_save_folder', { id, path }),
  chooseSaveDirectory: (id: string, path: string | null) =>
    invoke<string | null>('choose_save_directory', { id, path }),
  debugBat: (id: string) => invoke<string>('preview_mtool_bat', { id }),
  chooseDirectory: () => invoke<string | null>('choose_directory'),
  chooseWorkingDirectory: (root: string, current: string) =>
    invoke<string | null>('choose_working_directory', { root, current }),
  chooseLaunchFile: (root: string) =>
    invoke<string | null>('choose_launch_file', { root, extension: null }),
  choosePlayerFile: (root: string, extension: 'exe' | 'qsp') =>
    invoke<string | null>('choose_launch_file', { root, extension }),
};
