'use strict';
// A short real-browser controller check with explicit editor/API fixtures.
// This does not certify the Rust renderer, production boot or a physical pen.
const test = require('node:test'), assert = require('node:assert/strict'), fs = require('node:fs'), path = require('node:path'), http = require('node:http');
const ROOT = path.resolve(__dirname, '../..');
test('ink layers, pressure, recovery, masks and stale stage keep the original target', { timeout: 25000 }, async () => {
  const { chromium } = require(process.env.READMD_PLAYWRIGHT || require.resolve('playwright', { paths: [path.join(ROOT, 'ui-tests')] }));
  fs.mkdirSync(path.join(ROOT, '.cache'), { recursive: true });
  const out = fs.mkdtempSync(path.join(ROOT, '.cache/ink-controller-'));
  const paths = Object.fromEntries(['tmp', 'home', 'profile'].map(name => { const dir = path.join(out, name); fs.mkdirSync(dir); return [name, dir]; }));
  const scratch = Object.values(paths).map(file => ({ file, identity: fs.lstatSync(file) }));
  const fixture = `
    window.inkTest={a:{id:'a',path:'/owned/a.md',generation:0,text:'same',from:1,to:2},b:{id:'b',path:'/owned/b.md',generation:0,text:'same',from:0,to:0},calls:[],assets:[],insertions:[],hold:false};
    inkTest.active=inkTest.a;window.getActiveTab=()=>inkTest.active;window.showToast=()=>{};
    window.imageEditTarget=()=>{const tab=inkTest.active,generation=tab.generation;return{tab,from:tab.from,to:tab.to,current:()=>inkTest.active===tab&&tab.generation===generation};};
    window.registerStagedImage=(asset,tab)=>inkTest.assets.push({asset,tab:tab.id});
    window.cmInsertImage=(rel,target)=>{if(!target.current())throw Error('stale');inkTest.insertions.push({tab:target.tab.id,from:target.from,to:target.to,rel});target.tab.generation++;};
    window.apiFetch=async(url,options)=>{const body=JSON.parse(options.body);inkTest.calls.push(body);if(body.action==='stage'&&inkTest.hold)await new Promise(resolve=>inkTest.release=resolve);let result={ok:true};
      if(body.action==='stage')Object.assign(result,{asset:{path:'owned.svg',reference:'images/owned.svg'},rel:'images/owned.svg'});
      if(body.action==='render')result.svg='<svg xmlns="http://www.w3.org/2000/svg"><metadata>owned-fixture</metadata></svg>';
      if(body.action==='open')result.document=inkTest.importDocument;
      return{ok:true,json:async()=>result};};
    const originalSet=Storage.prototype.setItem;Storage.prototype.setItem=function(...args){if(inkTest.quota)throw new DOMException('owned quota','QuotaExceededError');return originalSet.apply(this,args);};
  `;
  const html = '<!doctype html><meta charset="utf-8"><link rel="stylesheet" href="/tokens.css"><link rel="stylesheet" href="/ink.css"><style>body{margin:0;font:14px sans-serif}.hidden{display:none}</style><button id="outside">Outside</button><script src="/fixture.js"></script><script src="/modal.js"></script><script src="/ink.js"></script>';
  const server = http.createServer((req, res) => {
    const files = { '/ink.js': 'assets/js/editor/ink.js', '/modal.js': 'assets/js/core/modal.js', '/ink.css': 'assets/css/ink.css', '/tokens.css': 'assets/css/tokens.css' };
    const file = files[req.url]; res.setHeader('Content-Type', req.url.endsWith('.js') ? 'text/javascript' : req.url.endsWith('.css') ? 'text/css' : 'text/html');
    res.end(file ? fs.readFileSync(path.join(ROOT, file)) : req.url === '/fixture.js' ? fixture : html);
  });
  await new Promise(resolve => server.listen(0, '127.0.0.1', resolve));
  let context;
  try {
    const env = Object.fromEntries(Object.entries(process.env).filter(([key]) => /^(PATH|SYSTEMROOT|WINDIR|COMSPEC|PATHEXT|LANG|LC_ALL)$/i.test(key)));
    Object.assign(env, { TMPDIR: paths.tmp, CFFIXED_USER_HOME: paths.home });
    context = await chromium.launchPersistentContext(paths.profile, { channel: 'chrome', headless: true, env, viewport: { width: 1280, height: 900 }, args: ['--disable-crash-reporter', '--disable-breakpad', '--disable-background-networking', '--disable-component-update'] });
    const page = context.pages()[0], errors = []; page.on('pageerror', error => errors.push(error.message)); page.setDefaultTimeout(4000);
    await page.goto('http://127.0.0.1:' + server.address().port); await page.evaluate(() => ReadMDInk.open());
    await page.locator('#ink-canvas').waitFor({ state: 'visible' }); assert.equal(await page.locator('#ink-insert').isEnabled(), true);
    const saved = () => page.evaluate(() => JSON.parse(localStorage.getItem('readmd.ink.draft.v1.' + encodeURIComponent(inkTest.a.path))).document);
    const canvas = page.locator('#ink-canvas'), box = await canvas.boundingBox(), cdp = await context.newCDPSession(page);
    await cdp.send('Input.dispatchMouseEvent', { type: 'mousePressed', x: box.x + 30, y: box.y + 30, button: 'left', buttons: 1, pointerType: 'pen', force: 0.2 });
    await cdp.send('Input.dispatchMouseEvent', { type: 'mouseMoved', x: box.x + 130, y: box.y + 90, button: 'left', buttons: 1, pointerType: 'pen', force: 0.9 });
    await cdp.send('Input.dispatchMouseEvent', { type: 'mouseReleased', x: box.x + 160, y: box.y + 110, button: 'left', buttons: 0, pointerType: 'pen', force: 0 });
    let doc = await saved(); assert.equal(doc.layers[0].strokes.length, 1); const pressures = doc.layers[0].strokes[0].points.map(point => point[2]); assert(pressures.some(p => p < 0.4) && pressures.some(p => p > 0.8));
    await page.locator('#ink-add-layer').click(); await page.locator('#ink-layer-name').fill('注释');
    await page.waitForTimeout(450); assert.equal(await page.locator('#ink-layer-name').inputValue(), '注释'); await page.locator('#ink-layer-name').press('Tab');
    await canvas.focus(); await page.keyboard.press('Space'); await page.keyboard.press('ArrowRight'); await page.keyboard.press('Space');
    doc = await saved(); assert.equal(doc.layers[1].name, '注释'); assert.equal(doc.layers[1].strokes.length, 2);
    await page.locator('#ink-undo').click(); assert.equal((await saved()).layers[1].strokes.length, 1); await page.locator('#ink-redo').click(); assert.equal((await saved()).layers[1].strokes.length, 2);
    await page.locator('#ink-layer-visible').uncheck(); assert.equal((await saved()).layers[1].visible, false); await page.locator('#ink-layer-visible').check();
    await page.locator('#ink-opacity').evaluate(node => { node.value = '35'; node.dispatchEvent(new Event('change', { bubbles: true })); }); assert.equal((await saved()).layers[1].opacity, 35);
    await page.locator('#ink-mask').click(); await canvas.focus(); await page.keyboard.press('Space'); assert.equal((await saved()).layers[1].mask.length, 1);
    await page.locator('#ink-clear-mask').click(); assert.equal((await saved()).layers[1].mask, undefined); assert.equal((await saved()).layers[1].strokes.length, 2);
    await page.locator('#ink-undo').click(); assert.equal((await saved()).layers[1].mask.length, 1); await page.locator('#ink-redo').click();
    await page.screenshot({ path: path.join(out, 'canvas.png') });
    await page.evaluate(() => { inkTest.importDocument = { schema: 1, width: 200, height: 100, layers: [{ name: '导入', visible: true, opacity: 100, strokes: [] }] }; });
    page.once('dialog', dialog => dialog.accept()); await page.locator('#ink-file').setInputFiles({ name: 'owned.svg', mimeType: 'image/svg+xml', buffer: Buffer.from('<svg/>') });
    await page.waitForFunction(() => document.getElementById('ink-document-width').value === '200'); assert.equal((await saved()).layers[0].name, '导入'); await page.locator('#ink-undo').click(); assert.equal((await saved()).layers.length, 2);
    const download = page.waitForEvent('download'); await page.locator('#ink-export').click(); await (await download).saveAs(path.join(out, 'drawing.svg'));
    await page.locator('#ink-close').click(); await page.evaluate(() => ReadMDInk.open()); assert.equal(await page.locator('#ink-insert').isDisabled(), true); await page.locator('#ink-retarget').click();
    await page.evaluate(() => { inkTest.hold = true; }); await page.locator('#ink-insert').click(); await page.waitForFunction(() => Boolean(inkTest.release));
    await page.evaluate(() => { inkTest.active = inkTest.b; inkTest.release(); }); await page.waitForFunction(() => document.getElementById('ink-insert').disabled && !document.querySelector('.ink-box').getAttribute('aria-busy').includes('true'));
    assert.equal(await page.evaluate(() => inkTest.insertions.length + inkTest.assets.length), 0);
    await page.locator('#ink-close').click(); await page.evaluate(() => { inkTest.active = inkTest.a; ReadMDInk.open(); }); assert.equal((await saved()).layers.length, 2); assert.equal(await page.locator('#ink-insert').isDisabled(), true);
    await page.locator('#ink-retarget').click(); await page.evaluate(() => { inkTest.hold = false; }); await page.locator('#ink-insert').click(); await page.waitForFunction(() => document.getElementById('ink-modal').classList.contains('hidden'));
    assert.deepEqual(await page.evaluate(() => inkTest.insertions.map(item => [item.tab, item.from, item.to])), [['a', 1, 2]]); assert.deepEqual(await page.evaluate(() => inkTest.assets.map(item => item.tab)), ['a']);
    await page.evaluate(() => { inkTest.active = inkTest.b; ReadMDInk.open(); inkTest.quota = true; }); await canvas.focus(); await page.keyboard.press('Space');
    page.once('dialog', dialog => dialog.dismiss()); await page.locator('#ink-close').click(); assert.equal(await page.locator('#ink-modal').isVisible(), true);
    await page.evaluate(() => { inkTest.quota = false; }); await page.locator('#ink-close').click(); assert.equal(await page.locator('#ink-modal').isHidden(), true); assert.deepEqual(errors, []);
    fs.writeFileSync(path.join(out, 'report.json'), JSON.stringify({ passed: true, modelsRequested: false, fixtureOnly: true, nativeUiCertified: false, physicalPenCertified: false, checked: ['browser pen pressure events', 'keyboard drawing', 'layers and opacity', 'non-destructive masks and undo/redo', 'SVG import/download controller', 'draft reopen requires explicit target', 'late stage cannot update another document', 'quota failure keeps draft open'], source: ['assets/js/editor/ink.js', 'assets/css/ink.css'] }, null, 2));
    console.log('Ink controller fixture: ' + path.relative(ROOT, out));
  } finally {
    if (context) await context.close(); await new Promise(resolve => server.close(resolve));
    for (const { file, identity } of scratch) { const now = fs.lstatSync(file); assert(now.isDirectory() && !now.isSymbolicLink() && now.ino === identity.ino && now.dev === identity.dev); fs.rmSync(file, { recursive: true }); }
  }
});
