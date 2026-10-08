import type { ScanCandidate, ExternalPlayer } from './types';
import { suggestedPlayer } from './qsp';

export type ScanSort =
  | 'unregistered-first'
  | 'registered-first'
  | 'name-asc'
  | 'name-desc'
  | 'modified-desc'
  | 'modified-asc';
export type ScanChoice = {
  exe?: string | null;
  external_player?: ExternalPlayer | null;
  version?: string;
  versionManual?: boolean;
};
const collator = new Intl.Collator('zh-CN', { numeric: true, sensitivity: 'base' });
export const folderName = (path: string) =>
  path
    .replace(/[\\/]+$/, '')
    .split(/[\\/]/)
    .pop() || path;

export function scanResults(
  candidates: ScanCandidate[],
  search: string,
  sort: ScanSort,
  choices: Record<string, ScanChoice> = {},
): ScanCandidate[] {
  const needle = search.trim().normalize('NFKC').toLocaleLowerCase();
  return candidates
    .filter((candidate) => {
      const choice = choices[candidate.install_path];
      return [
        candidate.suggested_title,
        candidate.install_path,
        candidate.engine,
        choice?.exe ?? suggestedPlayer(candidate) ?? '',
        choice?.external_player?.game_file ?? candidate.qsp?.game_files.join('\n') ?? '',
        choice?.version ?? candidate.suggested_version,
      ]
        .join('\n')
        .normalize('NFKC')
        .toLocaleLowerCase()
        .includes(needle);
    })
    .sort((a, b) => {
      const name =
        collator.compare(folderName(a.install_path), folderName(b.install_path)) ||
        a.install_path.localeCompare(b.install_path);
      if (sort.startsWith('name')) return sort === 'name-desc' ? -name : name;
      if (sort.endsWith('first')) {
        const registered = Number(!!a.registered_id) - Number(!!b.registered_id);
        if (registered) return sort === 'unregistered-first' ? registered : -registered;
      }
      const left = a.directory_modified_ms,
        right = b.directory_modified_ms;
      if (left == null || right == null) return left != null ? -1 : right != null ? 1 : name;
      return (sort === 'modified-asc' ? left - right : right - left) || name;
    });
}

export function directoryTime(ms: number | null): string {
  return ms == null ? '-' : new Date(ms).toLocaleString('zh-CN', { hour12: false });
}
