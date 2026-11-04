import type { Game, GameEdit } from './types';

const launchKeys = [
  'main_executable',
  'working_directory',
  'launch_type',
  'external_player',
  'mtool_target_exe',
  'mtool_loader',
  'mtool_strategy',
  'launch_arguments',
] as const;
const same = (left: unknown, right: unknown) =>
  JSON.stringify(left ?? null) === JSON.stringify(right ?? null);

// Launch settings form one unit: never mix a manual target with a stale detected mode.
function mergeUnedited<T extends GameEdit>(current: T, before: Game, incoming: Game): T {
  const next = { ...current };
  const launchUnchanged = launchKeys.every((key) => same(current[key], before[key]));
  for (const key of Object.keys(incoming) as (keyof Game)[]) {
    if (launchKeys.some((field) => field === key) && !launchUnchanged) continue;
    if (same(current[key as keyof T], before[key])) {
      Object.assign(next, { [key]: incoming[key] });
    }
  }
  return next;
}

export function mergeDetailDraft(draft: GameEdit, before: Game, incoming: Game): GameEdit {
  return mergeUnedited(draft, before, incoming);
}

export function mergeScanGame(current: Game, before: Game | undefined, incoming: Game): Game {
  if (
    !before ||
    current.install_path !== before.install_path ||
    incoming.install_path !== before.install_path
  )
    return current;
  return {
    ...mergeUnedited(current, before, incoming),
    favorite: current.favorite,
    tags: current.tags,
    play_status: current.play_status,
  };
}
