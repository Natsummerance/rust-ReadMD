import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';
const source = fs.readFileSync(new URL('../../rust/readmd-kernel/src/main.rs', import.meta.url), 'utf8');
const script = source.slice(source.indexOf('async function bridgePick('), source.indexOf('// JS twin of `safe_external_url()`'));
async function pick(response, multiple = false) {
  const messages = [], requests = [];
  const context = vm.createContext({
    window: { showToast: text => messages.push(text), i18n: { t: key => key } },
    bridgeHeaders: json => ({ authenticated: true, json }),
    fetch: async (...args) => { requests.push(args); if (response instanceof Error) throw response; return { ok: true, json: async () => response }; },
  });
  vm.runInContext(script, context);
  const value = await context.bridgePick('/api/dialog/choose-file', { initial_dir: '' }, multiple);
  return { value: JSON.parse(JSON.stringify(value)), messages, requests };
}
test('the shipped native picker reports unavailable dialogs and transport failures once', async () => {
  for (const result of [{ ok: true, canceled: true, error_code: 'dialog_unavailable' }, new Error('network')]) {
    const single = await pick(result); assert.equal(single.value, null); assert.deepEqual(single.messages, ['error.dialog_unavailable']);
    const many = await pick(result, true); assert.deepEqual(many.value, []); assert.equal(many.messages.length, 1);
  }
});
test('user cancellation stays quiet, while selected paths keep the existing bridge shape', async () => {
  assert.deepEqual((await pick({ ok: true, canceled: true })).messages, []);
  assert.deepEqual((await pick({ ok: true, canceled: true, paths: [] }, true)).messages, []);
  const selected = await pick({ ok: true, path: '/demo/note.md' });
  assert.equal(selected.value, '/demo/note.md'); assert.deepEqual(selected.messages, []);
  assert.deepEqual((await pick({ ok: true, paths: ['/demo/a.md', '/demo/b.md'] }, true)).value, ['/demo/a.md', '/demo/b.md']);
  assert.equal(selected.requests[0][1].headers.authenticated, true);
});
