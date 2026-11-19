'use strict';
// Executed against an explicit prebuilt kernel and the production boot bundle.
const fs = require('node:fs');
const path = require('node:path');
const assert = require('node:assert/strict');
const { ROOT, openOwned, until, sha } = require('./owned-browser-host.cjs');

async function run() {
  const binary = process.env.READMD_BIN;
  assert(binary && path.isAbsolute(binary));
  fs.mkdirSync(path.join(ROOT, '.cache'), { recursive: true });
  const output = fs.mkdtempSync(path.join(ROOT, '.cache/ink-roundtrip-'));
  const original = '# Canvas\n\nKeep this paragraph.\n';
  let file;
  const host = await openOwned(output, (_api, paths) => {
    file = path.join(paths.documents, '画稿.md');
    fs.writeFileSync(file, original, { flag: 'wx' });
  }, { binary, playwright: process.env.READMD_PLAYWRIGHT });
  try {
    const { page } = host;
    await page.evaluate(async file => { await loadFile(file); await toggleEdit(); }, file);
    await page.locator('[data-menu="md-insert-menu"]').click();
    await page.locator('[data-md="ink"]').click();
    const canvas = page.locator('#ink-canvas');
    await canvas.focus();
    await page.keyboard.press('Space');
    const assets = path.join(path.dirname(file), 'readmd-assets');
    assert(!fs.existsSync(assets));
    await page.locator('#ink-insert').click();
    await until(() => page.evaluate(() => cmView.state.doc.toString().includes('/raw?p=')), 'drawing staged into Markdown');
    assert.equal(fs.readFileSync(file, 'utf8'), original);
    assert(!fs.existsSync(assets));
    await page.evaluate(() => cmUndo());
    await until(() => page.evaluate(source => cmView.state.doc.toString() === source, original), 'single-step undo');
    await page.evaluate(() => cmRedo());
    await until(() => page.evaluate(() => cmView.state.doc.toString().includes('/raw?p=')), 'single-step redo');
    assert.equal(await page.evaluate(() => saveEdit()), true);
    const text = fs.readFileSync(file, 'utf8');
    const relative = text.match(/!\[[^\]]*\]\((readmd-assets\/[^)]+)\)/)?.[1];
    assert(relative, 'saved Markdown must use a relative resource');
    const svg = fs.readFileSync(path.join(path.dirname(file), decodeURIComponent(relative)), 'utf8');
    const reopened = await host.api('/api/drawing', { action: 'open', svg });
    assert.equal(reopened.document.layers[0].strokes.length, 1);
    assert.equal(reopened.document.layers[0].strokes[0].points.length, 1);
    await page.reload();
    await until(() => page.evaluate(() => Boolean(window.__readmdAppReady)), 'fresh page');
    await page.evaluate(async file => { await window.__readmdSessionReady; await loadFile(file, { force: true }); }, file);
    assert.equal(await page.evaluate(() => getActiveTab().content), text);
    fs.writeFileSync(path.join(output, 'verified.json'), JSON.stringify({
      productionCsp: true, headless: true, native: false,
      boot: sha(fs.readFileSync(path.join(ROOT, 'assets/readmd.boot.js'))),
      kernel: sha(fs.readFileSync(binary)), markdown: sha(text), svg: sha(svg),
      checked: ['insert-menu', 'draft-preserves-original', 'undo', 'redo', 'save-relative-asset', 'reopen-svg', 'reload-markdown']
    }, null, 2));
    console.log('PASS ink editor → staged drawing → undo/redo → saved SVG → fresh Markdown');
  } finally {
    await host.close();
  }
}
run().catch(error => { console.error(error.message); process.exitCode = 1; });
