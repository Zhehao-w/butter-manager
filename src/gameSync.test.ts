// @vitest-environment node
import { describe, expect, it } from 'vitest';
import { mergeDetailDraft, mergeScanGame } from './gameSync';
import { makeGame } from './testing/fixtures';

describe('scan and detail reconciliation', () => {
  const before = makeGame();
  const detected = makeGame({
    engine: 'RPG Developer Bakin',
    launch_type: 'MTOOL',
    mtool_strategy: 'BAKIN',
    mtool_target_exe: 'Game.exe',
    save_paths: ['<GAME>/Save'],
  });

  it('fills untouched defaults while preserving unsaved metadata and parameter edits', () => {
    const draft = { ...before, display_title: 'My draft', launch_arguments: '-dx11' };
    const next = mergeDetailDraft(draft, before, detected);
    expect(next.display_title).toBe('My draft');
    expect(next.engine).toBe('RPG Developer Bakin');
    expect(next.launch_arguments).toBe('-dx11');
    expect(next.launch_type).toBe('DIRECT');
    expect(mergeDetailDraft(before, before, detected)).toMatchObject(detected);
  });

  it('keeps later saved metadata and organization while accepting detected defaults', () => {
    const current = makeGame({
      display_title: 'Saved later',
      favorite: true,
      play_status: 'COMPLETED',
      tags: [{ id: 'tag', name: 'My tag' }],
      aliases: ['Manual alias'],
    });
    const next = mergeScanGame(current, before, detected);
    expect(next).toMatchObject({
      display_title: current.display_title,
      favorite: true,
      play_status: 'COMPLETED',
      tags: current.tags,
      aliases: current.aliases,
      engine: detected.engine,
      launch_type: 'MTOOL',
    });
  });

  it('keeps a manually changed launch configuration as one unit', () => {
    const current = makeGame({ main_executable: 'Manual.exe' });
    const next = mergeScanGame(current, before, detected);
    expect(next.main_executable).toBe('Manual.exe');
    expect(next.launch_type).toBe('DIRECT');
    expect(next.mtool_target_exe).toBeNull();
    expect(next.engine).toBe(detected.engine);
  });

  it('does not apply a result to a relocated or newly registered record', () => {
    const relocated = makeGame({ install_path: 'E:/New/Game' });
    expect(mergeScanGame(relocated, before, detected)).toBe(relocated);
    expect(mergeScanGame(before, undefined, detected)).toBe(before);
  });
});
