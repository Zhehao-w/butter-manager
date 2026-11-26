import type { Game } from './types';

// All library views receive the same immutable array until a record changes.
const gameIndexes = new WeakMap<readonly Game[], ReadonlyMap<string, Game>>();
export function indexGames(games: readonly Game[]): ReadonlyMap<string, Game> {
  let index = gameIndexes.get(games);
  if (!index) {
    index = new Map(games.map((game) => [game.id, game]));
    gameIndexes.set(games, index);
  }
  return index;
}

export const foldCase = (text: string) => text.toLocaleLowerCase();
export const normalizeSearch = (text: string) => foldCase(text.normalize('NFKC'));

export function gameSearchFields(game: Game) {
  return [game.display_title, game.canonical_title, game.install_path, ...game.aliases];
}

export const bytesText = (bytes: number) =>
  bytes >= 1024 ** 3
    ? `${(bytes / 1024 ** 3).toFixed(2)} GB`
    : bytes >= 1024 ** 2
      ? `${(bytes / 1024 ** 2).toFixed(1)} MB`
      : bytes >= 1024
        ? `${(bytes / 1024).toFixed(1)} KB`
        : `${bytes} B`;
export type LibrarySort =
  'name-asc' | 'name-desc' | 'added-asc' | 'added-desc' | 'played-asc' | 'played-desc';
const collator = new Intl.Collator('zh-CN', { numeric: true, sensitivity: 'base' });
const compareNames = (a: Game, b: Game) =>
  collator.compare(a.display_title, b.display_title) || a.id.localeCompare(b.id);
export function sortGames(games: Game[], sort: LibrarySort, playingFirst = false): Game[] {
  const direction = sort.endsWith('desc') ? -1 : 1;
  const byName = sort.startsWith('name');
  const field = sort.startsWith('added') ? 'created_at' : 'last_launched_at';
  return [...games].sort((a, b) => {
    if (playingFirst) {
      const aPlaying = a.play_status === 'PLAYING';
      const bPlaying = b.play_status === 'PLAYING';
      if (aPlaying !== bPlaying) return aPlaying ? -1 : 1;
    }
    if (byName) return direction * compareNames(a, b);
    const left = a[field],
      right = b[field];
    if (!left || !right) return left ? -1 : right ? 1 : compareNames(a, b);
    return direction * left.localeCompare(right) || compareNames(a, b);
  });
}
export function formatTime(value: string | null | undefined): string {
  if (!value) return '尚未运行';
  const date = new Date(value);
  return Number.isNaN(date.getTime()) ? value : date.toLocaleString('zh-CN', { hour12: false });
}

export function versionInput(value: string): string {
  return !value.trim() || value.trim().toLowerCase() === 'unknown' ? '' : value;
}
export function displayVersion(value: string): string {
  return versionInput(value) || '-';
}
export function isAssociatedFile(value: string | null): boolean {
  return !!value && !value.toLowerCase().endsWith('.exe');
}
