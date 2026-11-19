'use strict';
// Real Chrome CSS cascade and hit geometry with synthetic, isolated documents.
// Does not certify a native WebView, physical pen, or exported Office layout.
const test = require('node:test'), assert = require('node:assert/strict');
const fs = require('node:fs'), path = require('node:path'), http = require('node:http'), crypto = require('node:crypto');
const ROOT = path.resolve(__dirname, '../..');
const cssFiles = ['tokens', 'ink', 'reader', 'table-geometry', 'editor', 'panels', 'reader-article', 'export-html-layouts'];
const cells = ['left', 'center', 'right'].map((align, index) => `<td class="rd-num" align="${align}" valign="${['top', 'middle', 'bottom'][index]}">${align}</td>`).join('');
const table = `<table class="readmd-table-geometry"><tbody><tr>${cells}</tr></tbody></table>`;
const html = `<!doctype html><meta charset="utf-8">
  <link rel="stylesheet" href="/assets/css/tokens.css"><link rel="stylesheet" href="/assets/style.css">
  ${cssFiles.slice(1).map(name => `<link rel="stylesheet" href="/assets/css/${name}.css">`).join('')}
  <article class="rd-article"><div class="rd-table-wrap">${table}</div></article>
  <div class="readmd-html-layout" data-layout="reading"><article class="readmd-layout-article">${table}</article></div>
  <aside id="ai-panel" style="position:static;flex:none"><div id="ai-output"><div class="ai-msg ai"><div class="ai-msg-body">${table}</div></div></div></aside>
  <img class="office-preview-page" width="20" height="20" alt="Synthetic transparent page" src="data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' width='20' height='20'/%3E">
  <div id="table-cell-controls" class="table-cell-editor" style="width:240px;flex:none"><table class="table-cell-grid has-table-resize" style="--table-cell-columns:1;--table-fixed-width:200px;--table-auto-columns:0"><tbody><tr><td><button class="tb-btn">resize</button><span id="column-hit" class="table-resize-handle" data-table-resize="width" tabindex="0"></span><span id="row-hit" class="table-resize-handle" data-table-resize="height" tabindex="0"></span></td></tr></tbody></table></div>
  <script src="/fixture.js"></script><script src="/assets/js/core/modal.js"></script><script src="/assets/js/editor/ink.js"></script>`;
const fixture = `window.ownedTab={id:'style',path:'/owned/style.md'};window.getActiveTab=()=>ownedTab;
  window.imageEditTarget=()=>({tab:ownedTab,from:0,to:0,current:()=>true});window.showToast=()=>{};
  window.apiFetch=()=>{throw Error('Style fixture must not call an engine or provider');};`;

test('ink themes, white document paper, authored table alignment and resize targets survive style gates', { timeout: 25000 }, async () => {
  const { chromium } = require(process.env.READMD_PLAYWRIGHT || require.resolve('playwright', { paths: [path.join(ROOT, 'ui-tests')] }));
  fs.mkdirSync(path.join(ROOT, '.cache'), { recursive: true });
  const out = fs.mkdtempSync(path.join(ROOT, '.cache/v007-css-surfaces-'));
  const profile = path.join(out, 'profile'), temporary = path.join(out, 'tmp'), home = path.join(out, 'home');
  for (const directory of [profile, temporary, home]) fs.mkdirSync(directory);
  const files = new Set(['assets/style.css', ...cssFiles.map(name => 'assets/css/' + name + '.css'), 'assets/js/core/modal.js', 'assets/js/editor/ink.js']);
  const server = http.createServer((req, res) => {
    const rel = req.url.slice(1);
    res.setHeader('Content-Type', req.url.endsWith('.css') ? 'text/css' : req.url.endsWith('.js') ? 'text/javascript' : 'text/html');
    res.end(files.has(rel) ? fs.readFileSync(path.join(ROOT, rel)) : req.url === '/fixture.js' ? fixture : html);
  });
  await new Promise(resolve => server.listen(0, '127.0.0.1', resolve));
  let context;
  try {
    const env = Object.fromEntries(Object.entries(process.env).filter(([key]) => /^(PATH|SYSTEMROOT|WINDIR|COMSPEC|PATHEXT|LANG|LC_ALL)$/i.test(key)));
    Object.assign(env, { TMPDIR: temporary, CFFIXED_USER_HOME: home });
    context = await chromium.launchPersistentContext(profile, { channel: 'chrome', headless: true, env, viewport: { width: 1024, height: 900 }, args: ['--disable-crash-reporter', '--disable-breakpad', '--disable-background-networking', '--disable-component-update'] });
    const page = context.pages()[0], errors = [], samples = []; page.on('pageerror', error => errors.push(error.message));
    await page.goto('http://127.0.0.1:' + server.address().port); await page.evaluate(() => ReadMDInk.open());
    await page.locator('#ink-canvas').waitFor({ state: 'visible' });
    await page.locator('#ink-canvas').focus(); await page.keyboard.press('Space');
    const draft = await page.evaluate(() => localStorage.getItem('readmd.ink.draft.v1.' + encodeURIComponent(ownedTab.path)));
    assert.equal(JSON.parse(draft).document.layers[0].strokes.length, 1);
    for (const theme of ['light', 'dark', 'sepia']) {
      await page.evaluate(value => { document.body.dataset.theme = value; }, theme);
      for (const width of [640, 641, 700, 1024]) {
        await page.setViewportSize({ width, height: 900 });
        const sample = await page.evaluate(() => {
          const canvas = document.getElementById('ink-canvas'), style = getComputedStyle(canvas), box = canvas.getBoundingClientRect();
          const primary = getComputedStyle(document.getElementById('ink-insert'));
          return { width: innerWidth, theme: document.body.dataset.theme, canvas: { width: box.width, height: box.height, x: box.x, right: box.right, background: style.backgroundColor, checker: style.backgroundImage, touch: style.touchAction },
            paper: getComputedStyle(document.querySelector('.office-preview-page')).backgroundColor,
            foreground: primary.color, background: primary.backgroundColor, workspaceColumns: getComputedStyle(document.querySelector('.ink-workspace')).gridTemplateColumns,
            alignment: [...document.querySelectorAll('table.readmd-table-geometry td')].map(cell => [getComputedStyle(cell).textAlign, getComputedStyle(cell).verticalAlign]) };
        });
        assert.ok(sample.canvas.width > 0 && sample.canvas.height > 0);
        assert.ok(sample.canvas.x >= 0 && sample.canvas.right <= width, JSON.stringify(sample));
        assert.equal(sample.canvas.touch, 'none'); assert.equal(sample.canvas.background, 'rgb(255, 255, 255)');
        assert.notEqual(sample.canvas.checker, 'none'); assert.equal(sample.paper, 'rgb(255, 255, 255)');
        assert.deepEqual(sample.alignment, Array.from({ length: 3 }, () => [['left', 'top'], ['center', 'middle'], ['right', 'bottom']]).flat());
        assert.equal(sample.workspaceColumns.split(' ').length, width <= 640 ? 1 : 2);
        samples.push(sample);
      }
      await page.locator('#ink-insert').hover();
      const colors = await page.locator('#ink-insert').evaluate(node => { const style = getComputedStyle(node); return [style.color, style.backgroundColor]; });
      const contrast = ([a, b]) => {
        const luminance = text => { const rgb = text.startsWith('color(srgb') ? text.match(/[\d.]+/g).map(Number) : text.match(/[\d.]+/g).map(value => Number(value) / 255); return rgb.slice(0, 3).map(value => value <= .04045 ? value / 12.92 : ((value + .055) / 1.055) ** 2.4).reduce((sum, value, index) => sum + value * [.2126, .7152, .0722][index], 0); };
        const x = luminance(a), y = luminance(b); return (Math.max(x, y) + .05) / (Math.min(x, y) + .05);
      };
      assert.ok(contrast(colors) >= 4.5, theme + ' primary hover contrast: ' + colors);
      await page.mouse.move(0, 0);
      assert.equal(await page.evaluate(() => localStorage.getItem('readmd.ink.draft.v1.' + encodeURIComponent(ownedTab.path))), draft);
      await page.screenshot({ path: path.join(out, theme + '.png') });
    }
    await page.evaluate(() => ReadMDInk.close());
    // The three preceding reader fixtures push this separate editor fixture
    // below the viewport. Host it visibly before testing actual pointer hits.
    await page.locator('#table-cell-controls').evaluate(node => Object.assign(node.style, {
      position: 'fixed', inset: '16px auto auto 16px', zIndex: '1200',
    }));
    for (const [id, dimension, minimum] of [['column-hit', 'width', 12], ['row-hit', 'height', 11]]) {
      const hit = await page.locator('#' + id).evaluate(node => { const r = node.getBoundingClientRect(), target = document.elementFromPoint(r.x + r.width / 2, r.y + r.height / 2); return { width: r.width, height: r.height, x: r.x, y: r.y, touch: getComputedStyle(node).touchAction, inert: !!node.closest('[inert]'), target: target?.id || target?.tagName, reachable: target === node }; });
      assert.ok(hit[dimension] >= minimum, id); assert.equal(hit.touch, 'none'); assert.equal(hit.reachable, true, id + ': ' + JSON.stringify(hit));
    }
    assert.deepEqual(errors, []);
    const source = Object.fromEntries(['ink', 'reader', 'table-geometry'].map(name => [name + '.css', crypto.createHash('sha256').update(fs.readFileSync(path.join(ROOT, 'assets/css/' + name + '.css'))).digest('hex')]));
    fs.writeFileSync(path.join(out, 'report.json'), JSON.stringify({ passed: true, fixtureOnly: true, nativeUiCertified: false, samples, source }, null, 2));
    console.log('CSS surfaces: ' + path.relative(ROOT, out));
  } finally {
    if (context) await context.close(); await new Promise(resolve => server.close(resolve));
    for (const directory of [profile, temporary, home]) fs.rmSync(directory, { recursive: true, force: true });
  }
});
