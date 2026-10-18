'use strict';
const {test} = require('node:test');
const assert = require('node:assert/strict');
const {runtimeMaterialization, verifyRuntimeMaterialization} = require('./publication-runtime-reuse.cjs');
const readable = {sdSha256: 'a'.repeat(64), identity: '01'.repeat(12), protected: true, allAppAllow: 0x1200a9, allAppDeny: 0, bytes: 1234, links: 2};
const privateSource = {...readable, allAppAllow: 0};
const independentCopy = {...readable, sdSha256: 'b'.repeat(64), identity: '02'.repeat(12), protected: false, links: 1};
const linked = {...readable, links: readable.links + 1};

test('readable resources are hardlinked even when the historical copy plan contains them', () => {
  assert.equal(runtimeMaterialization(readable, false), 'hardlink');
  assert.equal(runtimeMaterialization(readable, true), 'hardlink');
});
test('a private resource needs the exact approved byte-copy plan', () => {
  assert.equal(runtimeMaterialization(privateSource, true), 'byte-copy');
  assert.throws(() => runtimeMaterialization(privateSource, false), /outside the fixed/);
});
test('required runtime SID denial prevents hardlink reuse', () => {
  const denied = {...readable, allAppDeny: 0x20};
  assert.equal(runtimeMaterialization(denied, true), 'byte-copy');
  assert.throws(() => runtimeMaterialization(denied, false), /outside the fixed/);
});
test('the real hardlink contract preserves identity and SD with the exact link increment', () => {
  verifyRuntimeMaterialization(readable, {source: linked, target: linked}, 'hardlink', 1);
  const twoAliases = {...readable, links: readable.links + 2};
  verifyRuntimeMaterialization(readable, {source: twoAliases, target: twoAliases}, 'hardlink', 2);
});
test('the real byte-copy contract keeps source SD/identity and gives the target inherited RX', () => {
  verifyRuntimeMaterialization(privateSource, {source: privateSource, target: independentCopy}, 'byte-copy', 0);
});
test('source descriptor changes fail even if the target is readable', () => {
  const changed = {...linked, sdSha256: 'c'.repeat(64)};
  assert.throws(() => verifyRuntimeMaterialization(readable, {source: changed, target: changed}, 'hardlink', 1), /security descriptor changed/);
});
test('unexpected source hardlink counts fail', () => {
  assert.throws(() => verifyRuntimeMaterialization(readable, {source: readable, target: readable}, 'hardlink', 1), /count changed/);
});
test('copy identity aliasing and hardlink identity substitution fail', () => {
  assert.throws(() => verifyRuntimeMaterialization(privateSource, {source: privateSource, target: {...independentCopy, identity: privateSource.identity}}, 'byte-copy', 0), /shares source identity/);
  assert.throws(() => verifyRuntimeMaterialization(readable, {source: linked, target: {...linked, identity: independentCopy.identity}}, 'hardlink', 1), /exact source identity/);
});
test('missing RX, explicit denial and a protected byte-copy target fail', () => {
  for (const target of [{...independentCopy, allAppAllow: 0x80}, {...independentCopy, allAppDeny: 0x20}]) {
    assert.throws(() => verifyRuntimeMaterialization(privateSource, {source: privateSource, target}, 'byte-copy', 0), /lacks required RX/);
  }
  assert.throws(() => verifyRuntimeMaterialization(privateSource, {source: privateSource, target: {...independentCopy, protected: true}}, 'byte-copy', 0), /inherit/);
});
test('unnecessary copies and unreadable hardlinks fail', () => {
  assert.throws(() => verifyRuntimeMaterialization(readable, {source: readable, target: independentCopy}, 'byte-copy', 0), /unnecessarily/);
  assert.throws(() => verifyRuntimeMaterialization(privateSource, {source: {...privateSource, links: 3}, target: {...privateSource, links: 3}}, 'hardlink', 1), /lacks required RX/);
});
