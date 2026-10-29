'use strict';
// Evaluate the production auth helpers with synthetic meta/host values only.
// No actual runtime token, kernel, native window, or private profile is read.
const { test } = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const ROOT = path.join(__dirname, '..');
const stateSource = fs.readFileSync(path.join(ROOT, 'assets/js/core/state.js'), 'utf8');
const mainSource = fs.readFileSync(path.join(ROOT, 'rust/readmd-kernel/src/main.rs'), 'utf8');
const html = fs.readFileSync(path.join(ROOT, 'assets/index.html'), 'utf8');
const stateEnd = stateSource.indexOf('\nconst MD_RE');
const bridgeStart = mainSource.indexOf('function bridgeHeaders(json)');
const bridgeEnd = mainSource.indexOf('// All native pickers', bridgeStart);
assert(stateEnd > 0 && bridgeStart > 0 && bridgeEnd > bridgeStart, 'Production auth helper boundaries are recognized');
const APP = 'SYNTHETIC_APP_TOKEN_NOT_A_CREDENTIAL', LAN = 'SYNTHETIC_LAN_TOKEN_NOT_A_CREDENTIAL';
function environment(appToken = APP, lanToken) {
  return { window: { READMD_ENGINE: 'rust', ...(lanToken ? { LAN_TOKEN: lanToken } : {}) },
    document: { querySelector: selector => selector === 'meta[name="readmd-app-token"]' && appToken ? { content: appToken } : null,
      getElementById: () => null } };
}
function apiHeaders(appToken, lanToken) {
  const calls = [], context = { ...environment(appToken, lanToken), fetch(url, options) { calls.push({ url, options }); return Promise.resolve({ ok: true }); } };
  vm.runInNewContext(stateSource.slice(0, stateEnd) + '\napiFetch("/api/rename/refactor", { method: "POST", headers: { "Content-Type": "application/json" } });', context);
  assert.equal(calls.length, 1); return calls[0].options.headers;
}
function bridgeHeaders(appToken, lanToken, json = true) {
  return vm.runInNewContext(mainSource.slice(bridgeStart, bridgeEnd) + '\nbridgeHeaders(' + Boolean(json) + ');', environment(appToken, lanToken));
}

test('production apiFetch sends app and strict-mode headers from the meta token without a LAN placeholder', () => {
  const headers = apiHeaders(APP);
  assert.equal(headers['X-ReadMD-App-Token'], APP); assert.equal(headers['X-ReadMD-Token'], APP);
  assert.equal(headers['Content-Type'], 'application/json');
});

test('production apiFetch preserves an explicitly supplied LAN token separately from the app token', () => {
  const headers = apiHeaders(APP, LAN);
  assert.equal(headers['X-ReadMD-App-Token'], APP); assert.equal(headers['X-ReadMD-Token'], LAN);
});

test('production native bridge headers fall back to the same meta token for strict mode', () => {
  const headers = bridgeHeaders(APP);
  assert.equal(headers['X-ReadMD-App-Token'], APP); assert.equal(headers['X-ReadMD-Token'], APP);
  assert.equal(headers['Content-Type'], 'application/json');
});

test('production native bridge honors explicit LAN headers and omits JSON content type for reads', () => {
  const headers = bridgeHeaders(APP, LAN, false);
  assert.equal(headers['X-ReadMD-App-Token'], APP); assert.equal(headers['X-ReadMD-Token'], LAN);
  assert.equal(headers['Content-Type'], undefined);
});

test('missing bootstrap values do not fabricate either authorization token', () => {
  for (const headers of [apiHeaders(''), bridgeHeaders('')]) {
    assert.equal(headers['X-ReadMD-App-Token'], undefined); assert.equal(headers['X-ReadMD-Token'], undefined);
  }
});

test('the production CSP keeps inline/eval scripts disabled and loads the external boot', () => {
  const policy = html.match(/<meta\s+http-equiv="Content-Security-Policy"\s+content="([^"]+)"/i)?.[1];
  assert(policy, 'The real HTML has a CSP meta element');
  const scripts = policy.split(';').map(part => part.trim()).find(part => part.startsWith('script-src '));
  assert(scripts); assert.match(scripts, /'self'/); assert.doesNotMatch(scripts, /'unsafe-inline'|'unsafe-eval'/);
  assert.match(html, /<script\b[^>]*\bsrc="\/assets\/readmd\.boot\.js(?:\?[^\"]*)?"[^>]*><\/script>/);
});
