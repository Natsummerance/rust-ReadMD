import assert from 'node:assert/strict';
import test from 'node:test';
import { buildManifest, diffManifest, promptViolations, stripRustTests, checkMathComponents } from '../check-assets.mjs';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import crypto from 'node:crypto';

test('offline math components reject corruption, missing scripts and path escapes',()=>{
  assert.deepEqual(checkMathComponents(),[]);
  const directory=fs.mkdtempSync(path.join(os.tmpdir(),'readmd-math-resources-'));
  try{
    fs.writeFileSync(path.join(directory,'renderer.js'),'locked');
    const entry={path:'renderer.js',bytes:6,sha256:crypto.createHash('sha256').update('locked').digest('hex')};
    const lock={schema:1,version:'3.2.2',license:'Apache-2.0',commit:'a'.repeat(40),files:[entry]};
    const write=()=>fs.writeFileSync(path.join(directory,'components.lock.json'),JSON.stringify(lock));write();assert.deepEqual(checkMathComponents(directory),[]);
    fs.writeFileSync(path.join(directory,'renderer.js'),'changed');assert.ok(checkMathComponents(directory).some(p=>p.includes('hash mismatch')));
    fs.unlinkSync(path.join(directory,'renderer.js'));assert.ok(checkMathComponents(directory).some(p=>p.includes('missing')));
    lock.files[0]={...entry,path:'../outside.js'};write();assert.ok(checkMathComponents(directory).some(p=>p.includes('path')));
  }finally{const checked=path.resolve(directory);assert.equal(path.dirname(checked),path.resolve(os.tmpdir()));assert.match(path.basename(checked),/^readmd-math-resources-/);fs.rmSync(checked,{recursive:true,force:true});}
});

test('manifest diff reports missing, added and changed files', () => {
  const actual = { files: [{ path: 'a', bytes: 1, sha256: 'x' }, { path: 'b', bytes: 2, sha256: 'y' }] };
  assert.deepEqual(diffManifest({ files: actual.files }, actual), []);
  const expected = { files: [{ path: 'a', bytes: 1, sha256: 'z' }, { path: 'c', bytes: 1, sha256: 'q' }] };
  const problems = diffManifest(expected, actual);
  assert.equal(problems.length, 2);
  assert.match(problems[0], /missing=\["c"\] added=\["b"\]/);
  assert.match(problems[1], /hash mismatch: a/);
});

test('checked-in manifest matches the vendored tree', () => {
  const built = buildManifest();
  assert.ok(built.files.length > 0);
  assert.ok(built.files.every(f => f.path.startsWith('assets/upstream/') && f.path !== 'assets/upstream/manifest.json'));
});

test('prompt policy: embedded system prompts fail, test modules do not', () => {
  const bad = 'let m = json!({"role": "system", "content": "You are a helpful assistant that"});';
  assert.equal(promptViolations(new Map([['src/a.rs', bad]])).length, 1);
  const inTest = `fn f() {}\n#[cfg(test)]\nmod tests {\n    ${bad}\n}\n`;
  assert.deepEqual(promptViolations(new Map([['src/a.rs', inTest]])), []);
  assert.equal(stripRustTests(inTest).split('\n').length, inTest.split('\n').length);
});
