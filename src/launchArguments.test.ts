// @vitest-environment node
import { describe, expect, it } from 'vitest';
import { launchArgumentsError, parseLaunchArguments } from './launchArguments';
describe('launch arguments', () => {
  it('splits quoted spaces, empty strings, paths and literal shell symbols', () => {
    expect(parseLaunchArguments(' \t ')).toEqual([]);
    expect(parseLaunchArguments('-dx11 --profile "含空格 名称" "" & %PATH%')).toEqual([
      '-dx11',
      '--profile',
      '含空格 名称',
      '',
      '&',
      '%PATH%',
    ]);
    expect(parseLaunchArguments(String.raw`--path "C:\Game Saves\\" a\"b "a""b"`)).toEqual([
      '--path',
      'C:\\Game Saves\\',
      'a"b',
      'a"b',
    ]);
  });
  it.each(['"unfinished', 'arg "unfinished', String.raw`"trailing\"`])(
    'rejects unclosed quotes: %s',
    (text) => {
      expect(launchArgumentsError(text)).toContain('双引号未闭合');
    },
  );
  it.each(['x\0y', 'x\ny', 'x\ry'])('rejects controls: %j', (text) => {
    expect(launchArgumentsError(text)).toContain('控制字符');
  });
});
