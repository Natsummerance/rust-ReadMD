'use strict';
const assert = require('node:assert/strict');
const requiredRuntimeMask = 0x1200a9;

function hasRuntimeAccess(record) {
  assert(record && Number.isSafeInteger(record.allAppAllow) && Number.isSafeInteger(record.allAppDeny));
  return (record.allAppAllow & requiredRuntimeMask) === requiredRuntimeMask && (record.allAppDeny & requiredRuntimeMask) === 0;
}

function runtimeMaterialization(record, approvedCopy) {
  if (hasRuntimeAccess(record)) return 'hardlink';
  assert.equal(approvedCopy, true, 'An unreadable resource is outside the fixed byte-copy plan');
  return 'byte-copy';
}

function verifyRuntimeMaterialization(before, after, method, addedSourceLinks) {
  assert(['hardlink', 'byte-copy'].includes(method));
  assert(Number.isSafeInteger(addedSourceLinks) && addedSourceLinks >= 0);
  const {links: beforeLinks, ...beforeStable} = before;
  const {links: afterLinks, ...afterStable} = after.source;
  assert.deepEqual(afterStable, beforeStable, 'Source file identity, size or security descriptor changed');
  assert.equal(afterLinks, beforeLinks + addedSourceLinks, 'Source hardlink count changed unexpectedly');
  assert(after.target && hasRuntimeAccess(after.target), 'Owned runtime resource lacks required RX permission');
  assert.equal(after.target.bytes, before.bytes);
  if (method === 'hardlink') {
    assert(hasRuntimeAccess(before), 'An unreadable source was hardlinked');
    assert.deepEqual(after.target, after.source, 'Hardlink does not retain the exact source identity and descriptor');
  } else {
    assert(!hasRuntimeAccess(before), 'An already-readable source was unnecessarily byte-copied');
    assert.equal(after.target.protected, false, 'Owned byte copy did not inherit its parent descriptor');
    assert.equal(after.target.links, 1);
    assert.notEqual(after.target.identity, after.source.identity, 'Owned byte copy still shares source identity');
  }
}

module.exports = {requiredRuntimeMask, hasRuntimeAccess, runtimeMaterialization, verifyRuntimeMaterialization};
