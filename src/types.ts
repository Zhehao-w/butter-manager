export type MToolStrategy = 'INJECT' | 'BAKIN';
export type PlayStatus = 'UNPLAYED' | 'PLAYING' | 'COMPLETED';
export interface DeletePlan {
  token: string;
  id: string;
  title: string;
  game_path: string;
  saves: {
    path: string;
    action: 'missing' | 'recycle';
  }[];
  blockers: string[];
}
export interface DeleteReport {
  removed: boolean;
  recycled: string[];
  error: string | null;
}
export interface ImportDuplicatePlan extends DeletePlan {
  existing_id: string;
  existing_title: string;
  existing_path: string;
  version: string;
}
export interface Settings {
  game_root: string;
  mtool_root: string;
  mtool_injector: string;
  mtool_runtime: string;
  scan_workers: number;
}

export interface ResetReport {
  settings: Settings;
  warning: string | null;
}

export interface RenpyAppdataPreview {
  token: string | null;
  path: string | null;
  reason: string | null;
}

export interface LibraryRelocationPreview {
  token: string;
  old_root: string;
  new_root: string;
  items: {
    id: string;
    title: string;
    old_path: string;
    new_path: string | null;
    reason: string | null;
  }[];
  mtool: {
    old_path: string;
    new_path: string | null;
    state: 'ready' | 'blocked' | 'unchanged';
    reason: string;
  };
}
export interface LibraryRelocationProgress {
  processed: number;
  total: number;
  current: string;
}
export interface LibraryRelocationReport {
  migrated: number;
  skipped: number;
  mtool_migrated: boolean;
  settings: Settings;
  games: Game[];
}

export interface LibraryTag {
  id: string;
  name: string;
}

export interface Game {
  mtool_strategy: MToolStrategy;
  launch_arguments: string;
  id: string;
  canonical_title: string;
  display_title: string;
  install_path: string;
  working_directory: string;
  current_version: string;
  version_source: string;
  main_executable: string | null;
  engine: string;
  launch_type: 'DIRECT' | 'MTOOL' | 'CUSTOM_BAT' | 'EXTERNAL_PLAYER';
  external_player?: ExternalPlayer | null;
  mtool_target_exe: string | null;
  mtool_loader: string | null;
  created_at: string;
  updated_at: string;
  last_launched_at: string | null;
  play_status: PlayStatus;
  favorite: boolean;
  tags: LibraryTag[];
  aliases: string[];
  save_paths: string[];
}

export interface DuplicatePair {
  left_id: string;
  right_id: string;
  score: number;
  confidence: 'high' | 'review';
  reason: string;
}
export type DuplicateStatus = 'pending' | 'confirmed' | 'excluded';
export interface DuplicateGameReference {
  id: string;
  directory_modified_ms: number | null;
}
export interface DuplicateReview {
  left_id: string;
  right_id: string;
  created_at: string;
  status: Exclude<DuplicateStatus, 'pending'>;
}

export type LaunchConfiguration = Pick<
  Game,
  | 'main_executable'
  | 'working_directory'
  | 'launch_type'
  | 'external_player'
  | 'mtool_target_exe'
  | 'mtool_loader'
> & { launch_arguments?: string | null; mtool_strategy?: MToolStrategy | null };

export type GameEdit = Pick<
  Game,
  | 'launch_arguments'
  | 'id'
  | 'canonical_title'
  | 'display_title'
  | 'current_version'
  | 'engine'
  | 'play_status'
  | 'main_executable'
  | 'working_directory'
  | 'launch_type'
  | 'external_player'
  | 'mtool_target_exe'
  | 'mtool_loader'
  | 'aliases'
  | 'save_paths'
> & { tags?: LibraryTag[]; mtool_strategy?: MToolStrategy };

export interface MToolRecipe {
  mtool_strategy?: MToolStrategy;
  target_exe: string;
  loader: string;
  injector: string;
  runtime: string;
  observed_root: string;
}

export interface LibraryPathCheck {
  id: string;
  install_path: string;
  state: 'available' | 'missing_directory' | 'missing_launch' | 'unconfigured' | 'unreadable';
  message: string;
}
export interface RelocateGame {
  id: string;
  expected_install_path: string;
  install_path: string;
  main_executable: string | null;
  working_directory: string;
  launch_type: 'DIRECT' | 'MTOOL' | 'EXTERNAL_PLAYER';
  external_player?: ExternalPlayer | null;
  mtool_target_exe: string | null;
}
export interface ToolCheck {
  label: string;
  path: string;
  available: boolean;
  message: string;
}

export interface BatAnalysis {
  path: string;
  status: 'supported' | 'unsupported';
  recipe: MToolRecipe | null;
  messages: string[];
}

export interface ScanCandidate {
  recommended_launch?: LaunchConfiguration | null;
  bundled_mtool?: MtoolBundle[];
  save_paths?: string[];
  qsp?: QspDetection | null;
  mtool_detected?: boolean;
  external_mtool_translation?: boolean;
  install_path: string;
  directory_modified_ms: number | null;
  suggested_title: string;
  engine: string;
  executables: {
    relative_path: string;
    architecture: string;
    score: number;
    size_bytes: number;
    modified_ms: number;
  }[];
  bats: BatAnalysis[];
  bundled_tool: boolean;
  warnings: string[];
  registered_id: string | null;
  suggested_version: string;
  version_source: string;
  working_directory: string;
  status: string;
  entries_scanned: number;
  elapsed_ms: number;
}

export interface MToolLaunchPreview {
  mtool_strategy?: MToolStrategy;
  launcher?: string;
  shared_root: string;
  target_exe: string;
  architecture: string;
  loader: string;
  runtime: string;
  working_directory: string;
}

export interface ScanReport {
  candidates: ScanCandidate[];
  warnings: string[];
}

export interface JobPage {
  id: string;
  kind:
    | 'scan'
    | 'register'
    | 'analysis'
    | 'metadata'
    | 'paths'
    | 'duplicates'
    | 'import_analysis'
    | 'import_plan'
    | 'import_apply'
    | 'import_rollback'
    | 'import_withdraw';
  bytes_done?: number;
  bytes_total?: number;
  overall_done?: number;
  overall_total?: number;
  indeterminate?: boolean;
  current_game?: string | null;
  transfer_rate?: number | null;
  remaining_seconds?: number | null;
  path_checks?: LibraryPathCheck[];
  root: string;
  status: 'running' | 'cancel_requested' | 'completed' | 'cancelled' | 'failed';
  phase: string;
  total: number;
  processed: number;
  active: Record<string, string>;
  elapsed_ms: number;
  idle_ms: number;
  changes: ScanCandidate[];
  next_cursor: number;
  change_count: number;
  warnings: string[];
  error: string | null;
  registered_ids: string[];
}
export interface RegistrationSelection {
  external_player?: ExternalPlayer | null;
  install_path: string;
  executable: string | null;
  exe_override: boolean;
  version: string;
  version_override: boolean;
}

export interface ImportSelection {
  mtool_strategy?: MToolStrategy;
  // Omitted/null inherits; an empty string clears.
  launch_arguments?: string | null;
  cleanup_mtool?: boolean;
  external_player?: ExternalPlayer | null;
  working_directory?: string | null;
  // Omitted/null inherits; an empty string explicitly selects automatic detection.
  mtool_loader?: string | null;
  source: string;
  title: string;
  target_name: string;
  version: string;
  engine: string;
  executable: string;
  mtool: boolean;
  existing_id: string | null;
  remember_association?: boolean;
  new_override?: boolean;
  preserve_saves?: boolean;
  saves_confirmed?: boolean;
}
export interface ImportSourceDiscovery {
  sources: string[];
  choices: { root: string; children: string[] }[];
  warnings: string[];
}

export interface ExternalPlayer {
  player_type: 'QSP';
  scope: 'GAME_LOCAL' | 'GLOBAL';
  game_file: string | null;
}
export interface QspDetection {
  game_files: string[];
  players: string[];
  recommended_player: string | null;
}
export interface ImportMatch {
  id: string;
  title: string;
  version: string;
  path: string;
  reason: string;
  auto_associate?: boolean;
  remembered?: boolean;
}

export interface ImportMemory {
  id: string;
  game_id: string;
  profile: { format_version: number; title: string; name_key: string; identities: string[] };
  confirmed_at: string;
}
export interface ImportItem {
  mtool_cleanup?: {
    bundles: MtoolBundle[];
    targets: {
      path: string;
      state: 'pending' | 'recycling' | 'recycled' | 'failed';
      error: string | null;
    }[];
  } | null;
  completed_at_ms?: number | null;
  update?: {
    old?: { launch_arguments?: string; mtool_strategy?: MToolStrategy };
    old_version: string;
    saves: {
      configured: string;
      source: string;
      relative: string | null;
      present: boolean;
      bytes: number;
    }[];
    required_bytes: number;
    quarantine: string;
    rollback_available: boolean;
  } | null;
  selection: ImportSelection;
  target: string;
  bytes: number;
  files: number;
  state: string;
  error: string | null;
  registered_id: string | null;
  cross_volume: boolean;
  blockers: string[];
}
export interface VersionHistory {
  operation: string;
  old_version: string;
  new_version: string;
  status: 'committed' | 'rolled_back';
  created_at: string;
}
export interface ImportPlan {
  recorded_at_ms?: number | null;
  id: string;
  root: string;
  status: string;
  items: ImportItem[];
}
export interface MtoolBundle {
  folder: string;
  bats: string[];
  safe: boolean;
  reason: string;
}
export interface ImportRecoveryIssue {
  record: string;
  message: string;
}
