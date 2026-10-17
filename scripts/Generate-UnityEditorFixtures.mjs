// Synthetic data only. Node crypto/zlib provide an independent oracle for the Rust codecs.
import { createCipheriv, createDecipheriv, pbkdf2Sync } from 'node:crypto';
import { gzipSync, gunzipSync, deflateRawSync, inflateRawSync } from 'node:zlib';
import { readFileSync, writeFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { join } from 'node:path';
import assert from 'node:assert/strict';

const password = 'synthetic-only-密码';
const iv = Buffer.from('00112233445566778899aabbccddeeff', 'hex');
function encrypt(bytes) {
  const key = pbkdf2Sync(password, iv, 100, 16, 'sha1');
  const cipher = createCipheriv('aes-128-cbc', key, iv);
  return Buffer.concat([iv, cipher.update(bytes), cipher.final()]);
}
function decrypt(bytes) {
  const iv = bytes.subarray(0, 16);
  const key = pbkdf2Sync(password, iv, 100, 16, 'sha1');
  const cipher = createDecipheriv('aes-128-cbc', key, iv);
  return Buffer.concat([cipher.update(bytes.subarray(16)), cipher.final()]);
}
const plain = '{\n "coins" : {"__type":"int","value":7,"unknown":1e3},\n "player_name":{"__type":"string","value":"合成角色"},"met":{"__type":"bool","value":false},"large":{"__type":"long","value":9223372036854775807},"custom":{"__type":"Game.Custom, Assembly-CSharp","value":{"__id":1,"x":[1,2]}}\n}';
const expectedEs3 = plain.replace('"value":7,', '"value":99,').replace('"value":"合成角色"', '"value":"测试 / 😀"').replace('"value":false', '"value":true');
const variable = (name, type, numericValue, stringValue = '', booleanValue = false) => ({ name, scope: 0, value: { type, stringValue, numericValue, booleanValue }, extra: 'preserve' });
const currentState = JSON.stringify({ LocalVariables: [variable('coins', 1, 7), variable('player_name', 0, 0, '合成角色'), variable('met', 2, 0)], unknown: { keep: ['x', 1e3] } });
const oldState = JSON.stringify({ GlobalVariableMap: { keys: ['coins', 'player_name', 'met'], values: ['7', '合成角色', 'false'] }, unknown: 12 });
function nani(state, global = false) {
  return '{\n "objectJsonMap" : '+JSON.stringify({keys: [`Naninovel.CustomVariableManager+${global ? 'GlobalState' : 'GameState'}, Elringus.Naninovel.Runtime, Version=0.0.0.0, Culture=neutral, PublicKeyToken=null`, 'Game.SyntheticService+State, Assembly-CSharp'], values: [state, '{ "coins": 123, "keep": 1e+03 }']})+',\n "playbackSpot":{"scriptPath":"SyntheticChapter","lineIndex":42,"inlineIndex":0},"rollbackStackJson":"[{\\"keep\\":true}]","thumbnailBase64":"synthetic-placeholder"\n}';
}
const changedCurrent = currentState.replace('"numericValue":7', '"numericValue":99').replace('"stringValue":"合成角色"', '"stringValue":"测试 / 😀"').replace('"booleanValue":false},"extra":"preserve"}],', '"booleanValue":true},"extra":"preserve"}],');
const changedOld = oldState.replace('["7","合成角色","false"]', '["99","测试 / 😀","true"]');
const fixtures = [];
for (const wrapper of ['plain', 'gzip', 'aes', 'aes-gzip']) {
  let data = Buffer.from(plain);
  if (wrapper.includes('gzip')) data = gzipSync(data);
  if (wrapper.startsWith('aes')) data = encrypt(data);
  fixtures.push({ id: `es3-${wrapper}`, format: 'ES3', wrapper, password: wrapper.startsWith('aes') ? password : null, data: data.toString('base64'), expected: expectedEs3 });
}
for (const old of [false, true]) {
  const text = nani(old ? oldState : currentState, old);
  const expected = nani(old ? changedOld : changedCurrent, old);
  for (const wrapper of ['plain', 'deflate']) {
    const data = wrapper === 'plain' ? Buffer.from(text) : deflateRawSync(Buffer.from(text));
    fixtures.push({ id: `nani-${old ? 'old-global' : 'current'}-${wrapper}`, format: 'Naninovel', wrapper, password: null, data: data.toString('base64'), expected });
  }
}
const verify = process.argv.indexOf('--verify-dir');
if (verify !== -1) {
  const dir = process.argv[verify + 1];
  assert.ok(dir, 'Expected synthetic test output directory');
  for (const fixture of fixtures) {
    let bytes = readFileSync(join(dir, `unity-${fixture.id}.bin`));
    if (fixture.wrapper.startsWith('aes')) {
      assert.notDeepEqual(bytes.subarray(0,16), iv, 'Modified ciphertext must use a new IV');
      bytes = decrypt(bytes);
    }
    if (fixture.wrapper.includes('gzip')) bytes = gunzipSync(bytes);
    if (fixture.wrapper === 'deflate') bytes = inflateRawSync(bytes);
    assert.equal(bytes.toString('utf8'), fixture.expected, fixture.id);
  }
  console.log(`Independent Node verification passed: ${fixtures.length} synthetic Unity fixtures`);
} else {
  const out = fileURLToPath(new URL('../src-tauri/src/save_editor/fixtures/unity.json', import.meta.url));
  writeFileSync(out, JSON.stringify(fixtures, null, 2) + '\n');
  console.log(`Generated ${fixtures.length} synthetic Unity fixtures`);
}
