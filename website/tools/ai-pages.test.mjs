import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import test from 'node:test';
import vm from 'node:vm';
import { aiCopies, renderAiPage, buildAiPages, aiIllustrationProblems } from './build-ai-pages.mjs';

const script = fs.readFileSync(new URL('../public/assets/ai-illustration.js', import.meta.url), 'utf8');
const decode = value => value.replaceAll('&quot;', '"').replaceAll('&gt;', '>').replaceAll('&lt;', '<').replaceAll('&amp;', '&');
class TinyElement {
  constructor(text = '', dataset = {}) { this.textContent = text; this.dataset = dataset; this.hidden = false; this.disabled = false; this.listeners = new Map(); this.focused = false; }
  addEventListener(type, listener) { if (!this.listeners.has(type)) this.listeners.set(type, new Set()); this.listeners.get(type).add(listener); }
  removeEventListener(type, listener) { this.listeners.get(type)?.delete(listener); }
  emit(type, values = {}) {
    const event = { key: '', target: this, defaultPrevented: false, preventDefault() { this.defaultPrevented = true; }, ...values };
    for (const listener of this.listeners.get(type) || []) listener(event);
    return event;
  }
  focus() { this.focused = true; }
  listenerCount() { return [...this.listeners.values()].reduce((count, listeners) => count + listeners.size, 0); }
}
function fixture(copy) {
  const html = renderAiPage(copy);
  const text = name => decode(new RegExp('<span data-ai-' + name + '[^>]*>([^<]*)</span>').exec(html)[1]);
  const statusTag = /<p\b[^>]*data-ai-status[^>]*>/.exec(html)[0];
  const messages = Object.fromEntries([...statusTag.matchAll(/data-(pending|accepted|dismissed|restored)="([^"]*)"/g)].map(match => [match[1], decode(match[2])]));
  const nodes = Object.fromEntries(['editor', 'suggestion', 'inserted', 'status', 'draft'].map(name => [name, new TinyElement(['suggestion', 'inserted', 'draft'].includes(name) ? text(name) : '', name === 'status' ? messages : {})]));
  const buttons = Object.fromEntries(['accept', 'dismiss', 'undo', 'reset'].map(action => [action, new TinyElement()]));
  const root = new TinyElement(), window = new TinyElement();
  root.querySelector = selector => {
    const action = /data-ai-action="(\w+)"/.exec(selector)?.[1];
    return action ? buttons[action] : nodes[/^\[data-ai-(\w+)\]$/.exec(selector)?.[1]];
  };
  const document = { querySelectorAll(selector) { assert.equal(selector, '[data-ai-illustration="offline"]'); return [root]; } };
  vm.runInNewContext(script, { document, window }, { timeout: 1000 });
  return { root, nodes, buttons, window, current: () => nodes.draft.textContent + nodes.inserted.textContent };
}

for (const [index, copy] of aiCopies.entries()) {
  test(copy.lang + ' page explains the fixed-text flow and has no recording dependency', () => {
    const html = renderAiPage(copy);
    assert.deepEqual(aiIllustrationProblems(html, copy.lang), []);
    assert.match(html, new RegExp('<html lang="' + copy.lang + '"'));
    assert.match(html, /<link rel="stylesheet" href="\/assets\/motion.css">/);
    assert.match(html, /<script src="\/assets\/ai-illustration.js" defer><\/script>/);
    assert.ok(!/<video|<source|\.mp4|\.webm|v005-ai-autocomplete|onkeydown=|onclick=/i.test(html));
    assert.ok(html.includes('Tab') && html.includes('Esc') && html.includes('⌘Z'));
    assert.ok(html.includes(copy.context) && html.includes(copy.suggestion) && html.includes(copy.disclosure));
    if (copy.lang !== 'en') for (const key of ['disclosure', 'context', 'suggestion', 'hint', 'pending', 'restored']) assert.notEqual(copy[key], aiCopies[0][key]);
    assert.equal((html.match(/data-ai-action=/g) || []).length, 4);
    assert.ok(aiIllustrationProblems(html.replace('data-ai-context', 'data-context'), copy.lang).length);
    assert.ok(aiIllustrationProblems(html.replace(copy.disclosure, ''), copy.lang).length);
    assert.ok(aiIllustrationProblems(html.replace('</figure>', '<video src="/private.mp4"></video></figure>'), copy.lang).length);
  });
  test(copy.lang + ' actual script accepts once, releases Tab, dismisses and undoes without I/O', () => {
    const { root, nodes, buttons, window, current } = fixture(copy);
    assert.equal(root.dataset.aiReady, 'true');
    assert.equal(current(), copy.draft);
    assert.equal(nodes.suggestion.hidden, false);
    assert.equal(nodes.editor.emit('keydown', { key: 'Tab', shiftKey: true }).defaultPrevented, false);
    assert.equal(nodes.editor.emit('keydown', { key: 'Tab', isComposing: true }).defaultPrevented, false);
    assert.equal(nodes.editor.emit('keydown', { key: 'Tab', target: buttons.reset }).defaultPrevented, false);
    assert.equal(nodes.editor.emit('keydown', { key: 'Tab' }).defaultPrevented, true);
    assert.equal(current(), copy.draft + copy.suggestion);
    assert.equal(nodes.status.textContent, copy.accepted);
    assert.equal(nodes.suggestion.hidden, true);
    assert.equal(buttons.undo.disabled, false);
    assert.equal(nodes.editor.emit('keydown', { key: 'Tab' }).defaultPrevented, false, 'second Tab navigates normally');
    assert.equal(nodes.editor.emit('keydown', { key: 'z', [index % 2 ? 'metaKey' : 'ctrlKey']: true }).defaultPrevented, true);
    assert.equal(current(), copy.draft);
    assert.equal(nodes.status.textContent, copy.restored);
    assert.equal(buttons.undo.disabled, true);
    assert.equal(nodes.editor.emit('keydown', { key: 'z', ctrlKey: true }).defaultPrevented, false, 'no second undo is captured');
    buttons.reset.emit('click');
    assert.equal(nodes.editor.focused, true);
    assert.equal(nodes.editor.emit('keydown', { key: 'Escape' }).defaultPrevented, true);
    assert.equal(current(), copy.draft);
    assert.equal(nodes.status.textContent, copy.dismissed);
    assert.equal(buttons.undo.disabled, true);
    buttons.reset.emit('click'); buttons.accept.emit('click'); buttons.undo.emit('click');
    assert.equal(current(), copy.draft);
    assert.equal(nodes.status.textContent, copy.restored);
    window.emit('pagehide', { persisted: true });
    assert.equal(root.dataset.aiReady, 'false');
    for (const element of [...Object.values(nodes), ...Object.values(buttons)]) assert.equal(element.listenerCount(), 0);
    window.emit('pageshow', { persisted: true }); window.emit('pageshow', { persisted: true });
    assert.equal(nodes.editor.listenerCount(), 1, 'back navigation remounts without duplicate listeners');
    window.emit('pagehide', { persisted: false });
    assert.equal(window.listenerCount(), 0);
    for (const element of [...Object.values(nodes), ...Object.values(buttons)]) assert.equal(element.listenerCount(), 0);
  });
}

test('illustration script has no model, persistence, media or animation API', () => {
  assert.doesNotMatch(script, /\b(?:fetch|XMLHttpRequest|WebSocket|sendBeacon|localStorage|sessionStorage|setTimeout|setInterval|requestAnimationFrame|matchMedia)\b/);
  assert.equal(aiCopies.length, 4);
  assert.throws(() => renderAiPage(aiCopies[0], 'invalid'), /Invalid AI introduction version/);
});

test('builder only writes the four AI pages and does not need private assets', () => {
  const directory = fs.mkdtempSync(path.join(os.tmpdir(), 'readmd-ai-pages-'));
  try {
    const sentinels = ['index.html', 'zh-cn/index.html', 'zh-tw/index.html', 'ja/index.html', 'sitemap.xml', 'feed.xml', 'llms.txt', 'showcase/catalog.json'];
    for (const name of sentinels) { fs.mkdirSync(path.dirname(path.join(directory, name)), { recursive: true }); fs.writeFileSync(path.join(directory, name), 'independent owner: ' + name); }
    const output = buildAiPages(directory);
    assert.equal(output.length, 4);
    for (const [index, file] of output.entries()) assert.equal(fs.readFileSync(file, 'utf8'), renderAiPage(aiCopies[index]));
    for (const name of sentinels) assert.equal(fs.readFileSync(path.join(directory, name), 'utf8'), 'independent owner: ' + name);
    assert.equal(fs.existsSync(path.join(directory, 'media')), false);
  } finally {
    const resolved = fs.realpathSync(directory), parent = fs.realpathSync(os.tmpdir());
    assert.equal(path.dirname(resolved), parent);
    assert.ok(path.basename(resolved).startsWith('readmd-ai-pages-'));
    fs.rmSync(resolved, { recursive: true, force: true });
  }
});
