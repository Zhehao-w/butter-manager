import { describe, expect, it } from 'vitest';
import { sortGames } from './library';
import type { Game } from './types';
const fixture = (id: string, title: string, added: string, played: string | null) =>
  ({ id, display_title: title, created_at: added, last_launched_at: played }) as Game;
const games = [
  fixture('a', '游戏10', '2020-01-01T00:00:00.000Z', null),
  fixture('b', '游戏2', '2023-01-01T00:00:00.000Z', '2025-01-01T00:00:00.000Z'),
  fixture('c', '游戏1', '2022-01-01T00:00:00.000Z', '2026-01-01T00:00:00.000Z'),
];
describe('library sorting', () => {
  it('sorts natural names and added time in both directions without changing records', () => {
    expect(sortGames(games, 'name-asc').map((g) => g.id)).toEqual(['c', 'b', 'a']);
    expect(sortGames(games, 'name-desc').map((g) => g.id)).toEqual(['a', 'b', 'c']);
    expect(sortGames(games, 'added-asc').map((g) => g.id)).toEqual(['a', 'c', 'b']);
    expect(sortGames(games, 'added-desc').map((g) => g.id)).toEqual(['b', 'c', 'a']);
    expect(games.map((g) => g.id)).toEqual(['a', 'b', 'c']);
  });
  it('keeps never-launched games last for either launch-time direction with deterministic ties', () => {
    expect(sortGames(games, 'played-desc').map((g) => g.id)).toEqual(['c', 'b', 'a']);
    expect(sortGames(games, 'played-asc').map((g) => g.id)).toEqual(['b', 'c', 'a']);
    const same = [fixture('b', 'Same', '2020', null), fixture('a', 'Same', '2020', null)];
    expect(sortGames(same, 'played-desc').map((g) => g.id)).toEqual(['a', 'b']);
  });
});
