import type { Game } from './types';
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
export function sortGames(games: Game[], sort: LibrarySort): Game[] {
  const descending = sort.endsWith('desc');
  return [...games].sort((a, b) => {
    const tie = collator.compare(a.display_title, b.display_title) || a.id.localeCompare(b.id);
    if (sort.startsWith('name')) return (descending ? -1 : 1) * tie;
    const field = sort.startsWith('added') ? 'created_at' : 'last_launched_at';
    const left = a[field],
      right = b[field];
    if (!left || !right) return left ? -1 : right ? 1 : tie;
    return (descending ? -1 : 1) * left.localeCompare(right) || tie;
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
