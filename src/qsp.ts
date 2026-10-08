import type { ExternalPlayer, Game, ScanCandidate } from './types';
import { isAssociatedFile } from './library';
export const isQspFile = (file: string | null): boolean => !!file && /\.qsp$/i.test(file);

export const localQsp = (file: string | null = null): ExternalPlayer => ({
  player_type: 'QSP',
  scope: 'GAME_LOCAL',
  game_file: file,
});
export const qspConfig = (candidate: ScanCandidate): ExternalPlayer | null =>
  candidate.qsp
    ? localQsp(
        candidate.status === 'ready' && candidate.qsp.game_files.length === 1
          ? candidate.qsp.game_files[0]
          : null,
      )
    : null;
export const suggestedPlayer = (candidate: ScanCandidate): string | null =>
  candidate.qsp
    ? candidate.qsp.recommended_player
    : candidate.executables[0]?.relative_path || null;
export const launchLabel = (game: Game): string =>
  game.launch_type === 'EXTERNAL_PLAYER'
    ? 'QSP 播放器'
    : game.launch_type === 'MTOOL'
      ? 'MTool'
      : isAssociatedFile(game.main_executable)
        ? '默认应用'
        : '直接启动';
