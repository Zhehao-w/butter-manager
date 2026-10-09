import { describe, expect, it } from 'vitest';
import { directoryTime, scanResults } from './scan';
import type { ScanCandidate } from './types';

const candidate = (folder: string, modified: number | null, registered = false): ScanCandidate => ({
  install_path: `E:\\Butter\\${folder}`,
  directory_modified_ms: modified,
  suggested_title: '建议标题',
  engine: 'Unity',
  executables: [],
  bats: [],
  bundled_tool: false,
  warnings: [],
  registered_id: registered ? folder : null,
  suggested_version: 'v1.2',
  version_source: 'folder_name',
  working_directory: '.',
  status: 'ready',
  entries_scanned: 1,
  elapsed_ms: 0,
});
const names = (values: ScanCandidate[]) =>
  values.map((value) => value.install_path.split('\\').pop());

describe('scan result search and sorting', () => {
  it('prioritizes unregistered games, then folder modification date, without changing the input', () => {
    const input = [
      candidate('Game 10', 30, true),
      candidate('Game 2', 20),
      candidate('Game 1', 10),
      candidate('Game 3', null),
    ];
    expect(names(scanResults(input, '', 'unregistered-first'))).toEqual([
      'Game 2',
      'Game 1',
      'Game 3',
      'Game 10',
    ]);
    expect(names(scanResults(input, '', 'registered-first'))).toEqual([
      'Game 10',
      'Game 2',
      'Game 1',
      'Game 3',
    ]);
    expect(names(input)).toEqual(['Game 10', 'Game 2', 'Game 1', 'Game 3']);
  });
  it('sorts natural folder names rather than suggested titles and supports both directions', () => {
    const input = [candidate('Game 10', 10), candidate('Game 2', 30), candidate('Game 1', 20)];
    expect(names(scanResults(input, '', 'name-asc'))).toEqual(['Game 1', 'Game 2', 'Game 10']);
    expect(names(scanResults(input, '', 'name-desc'))).toEqual(['Game 10', 'Game 2', 'Game 1']);
  });
  it('sorts folder dates globally and puts unavailable dates last in either direction', () => {
    const input = [candidate('Old', 0), candidate('No date', null), candidate('New', 100, true)];
    expect(names(scanResults(input, '', 'modified-asc'))).toEqual(['Old', 'New', 'No date']);
    expect(names(scanResults(input, '', 'modified-desc'))).toEqual(['New', 'Old', 'No date']);
    expect(directoryTime(null)).toBe('-');
    for (const ms of [0, 1700000000000, Date.UTC(2026, 9, 8, 4)]) {
      expect(directoryTime(ms)).toBe(new Date(ms).toLocaleString('zh-CN', { hour12: false }));
    }
  });
  it('normalizes Unicode/case and searches paths, engine and manually chosen file/version', () => {
    const game = candidate('游戏 ABC', 10);
    const choices = { [game.install_path]: { exe: '包装/启动.QSP', version: 'Custom 2' } };
    for (const needle of [' ａｂｃ ', 'E:\\Butter', 'unity', '启动.qsp', 'custom 2'])
      expect(scanResults([game], needle, 'name-asc', choices)).toEqual([game]);
    expect(scanResults([game], '不存在', 'name-asc', choices)).toEqual([]);
  });
});
