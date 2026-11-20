'use strict';
// Browser behavior of the shared HTML layout mount. The optional companion
// runner exports actual files with an explicit candidate before reusing these
// checks; this fixture alone does not certify a native WebView or the backend.
const test = require('node:test'), assert = require('node:assert/strict');
const fs = require('node:fs'), path = require('node:path'), http = require('node:http');
const ROOT = path.resolve(__dirname, '../..');
const fixture = `<!doctype html><html lang="zh-CN"><meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1">
<style>*{box-sizing:border-box}body{margin:0;font:16px/1.8 Arial,sans-serif}#content{margin:auto}h1{font-size:35px}h2{font-size:25px}h3{font-size:20px}table{border-collapse:collapse;width:100%}th,td{border:1px solid #ddd;padding:8px}pre{background:#f5f6f8;border:1px solid #dfe3e8;border-radius:8px;padding:12px;white-space:pre-wrap;overflow:auto;font:14px/1.5 monospace}</style>
<link rel="stylesheet" href="/assets/css/tokens.css"><link rel="stylesheet" href="/assets/css/table-geometry.css"><link rel="stylesheet" href="/assets/css/export-html-layouts.css">
<article id="content"><h1>让长文保持清楚，也让细节有位置</h1><p>文字、链接和排印设置应在切换版式之后继续保留。</p>
<h2 id="authored-section">原有锚点</h2><p><a href="#authored-section">保留正文内部链接</a></p>
<h2>自动章节</h2><h3 id="readmd-section-1">保留的后续编号</h3><h2 id="authored-section">重名章节</h2>
<span id="body-marker">正文标记</span><h2 id="body-marker">与正文重名的章节</h2>
<blockquote><h3>引用中的标题不应污染目录</h3><p>引用内容仍然完整。</p></blockquote>
<pre><code>const route = "${'unbroken_path_'.repeat(22)}";\nconsole.log(route);</code></pre>
<table><caption>可横向滚动的参数表</caption><thead><tr><th>参数</th><th>完整示例</th></tr></thead><tbody><tr><td>保持原文</td><td>${'non_breaking_identifier_'.repeat(18)}</td></tr></tbody></table>
<table class="readmd-table-geometry readmd-table-geometry--columns" style="width:80%;table-layout:fixed"><colgroup><col style="width:30%"><col style="width:70%"></colgroup><tbody><tr><td align="right" valign="middle">17.25</td><td align="left" valign="bottom">显式宽度与对齐</td></tr></tbody></table>
</article><script src="/assets/js/reader/export-html-layout.js"></script></html>`;

async function browserEnvironment(run) {
  const { chromium } = require(process.env.READMD_PLAYWRIGHT || require.resolve('playwright', { paths: [path.join(ROOT, 'ui-tests')] }));
  const out = fs.mkdtempSync(path.join(ROOT, '.cache/html-layout-browser-'));
  const temporary = path.join(out, 'tmp'), home = path.join(out, 'home');
  fs.mkdirSync(temporary); fs.mkdirSync(home);
  const allowed = new Set(['assets/css/tokens.css', 'assets/css/table-geometry.css', 'assets/css/export-html-layouts.css', 'assets/js/reader/export-html-layout.js']);
  const server = http.createServer((req, res) => {
    const rel = req.url.slice(1); res.setHeader('Content-Type', rel.endsWith('.css') ? 'text/css' : rel.endsWith('.js') ? 'text/javascript' : 'text/html');
    res.end(allowed.has(rel) ? fs.readFileSync(path.join(ROOT, rel)) : fixture);
  });
  await new Promise(resolve => server.listen(0, '127.0.0.1', resolve));
  let context;
  try {
    const env = Object.fromEntries(Object.entries(process.env).filter(([key]) => /^(PATH|SYSTEMROOT|WINDIR|COMSPEC|PATHEXT|LANG|LC_ALL)$/i.test(key)));
    Object.assign(env, { TMPDIR: temporary, CFFIXED_USER_HOME: home });
    context = await chromium.launchPersistentContext(path.join(out, 'profile'), { channel: 'chrome', headless: true, env, viewport: { width: 1440, height: 1000 }, args: ['--disable-crash-reporter', '--disable-breakpad', '--disable-background-networking', '--disable-component-update'] });
    await run(context.pages()[0], 'http://127.0.0.1:' + server.address().port);
  } finally {
    if (context) await context.close();
    await new Promise(resolve => server.close(resolve));
    fs.rmSync(out, { recursive: true, force: true });
  }
}

test('HTML layout retains authored anchors and content across repeated mounts', { timeout: 30000 }, async () => browserEnvironment(async (page, origin) => {
  await page.goto(origin);
  const result = await page.evaluate(() => {
    const root = document.getElementById('content'), source = root.textContent;
    const snapshot = () => ({
      targets: [...root.querySelectorAll('.readmd-layout-nav a')].map(link => ({ label: link.textContent, id: decodeURIComponent(link.hash.slice(1)), target: document.getElementById(decodeURIComponent(link.hash.slice(1)))?.textContent })),
      ids: [...root.querySelectorAll('[id]')].map(node => node.id),
      content: root.querySelector('.readmd-layout-article').textContent,
      tables: root.querySelectorAll('.readmd-layout-table').length,
      cards: root.querySelectorAll('.readmd-html-layout').length,
    });
    ReadMDExportHtmlLayout.mount(root, 'docs'); const first = snapshot();
    ReadMDExportHtmlLayout.mount(root, 'reading');
    ReadMDExportHtmlLayout.mount(root, 'docs'); const second = snapshot();
    return { source, first, second, internalLink: root.querySelector('.readmd-layout-article a').hash };
  });
  assert.equal(result.first.content, result.source);
  assert.deepEqual(result.second, result.first);
  assert.equal(result.first.targets.length, 5);
  assert(result.first.targets.every(({ label, target }) => label === target));
  assert.equal(new Set(result.first.ids).size, result.first.ids.length);
  assert.equal(result.first.targets[0].id, 'authored-section');
  assert.equal(result.first.targets[2].id, 'readmd-section-1');
  assert.equal(result.internalLink, '#authored-section');
  assert.equal(result.first.tables, 2); assert.equal(result.first.cards, 1);
}));

test('HTML layouts contain wide content, use the embedded width, and flatten for print', { timeout: 30000 }, async () => browserEnvironment(async (page, origin) => {
  await page.goto(origin);
  for (const layout of ['reading', 'docs', 'magazine']) {
    await page.evaluate(layout => { document.body.dataset.readmdHtmlLayout = layout; ReadMDExportHtmlLayout.mount(document.getElementById('content'), layout); }, layout);
    for (const width of [375, 768, 1440]) {
      await page.setViewportSize({ width, height: 1000 });
      const sample = await page.evaluate(() => {
        const root = document.querySelector('.readmd-html-layout'), article = root.querySelector('.readmd-layout-article'), pre = article.querySelector('pre'), scroll = article.querySelector('.readmd-layout-table');
        const fixed = article.querySelector('.readmd-table-geometry'), cell = fixed.querySelector('td'), nav = root.querySelector('.readmd-layout-nav');
        return { overflow: document.documentElement.scrollWidth > innerWidth + 1, preOverflow: pre.scrollWidth > pre.clientWidth, preWrap: getComputedStyle(pre).whiteSpace,
          tableOverflow: scroll.scrollWidth > scroll.clientWidth, tableRole: scroll.getAttribute('role'), tableTab: scroll.tabIndex,
          tableRatio: fixed.getBoundingClientRect().width / fixed.parentElement.clientWidth,
          alignment: [getComputedStyle(cell).textAlign, getComputedStyle(cell).verticalAlign],
          columns: getComputedStyle(root.querySelector('.readmd-layout-flow')).gridTemplateColumns, nav: nav && getComputedStyle(nav).position };
      });
      assert.equal(sample.overflow, false, layout + ' ' + width + ': page overflow');
      assert.equal(sample.preWrap, 'pre'); assert.equal(sample.preOverflow, true);
      assert.equal(sample.tableOverflow, true); assert.equal(sample.tableRole, 'region'); assert.equal(sample.tableTab, 0);
      assert(Math.abs(sample.tableRatio - .8) < .02); assert.deepEqual(sample.alignment, ['right', 'middle']);
      if (layout === 'docs') { assert.equal(sample.columns.split(' ').length, width >= 1440 ? 2 : 1); assert.equal(sample.nav, width >= 1440 ? 'sticky' : 'static'); }
    }
    // A narrow preview in a wide application must collapse its own navigation.
    await page.evaluate(() => { document.getElementById('content').style.width = '360px'; });
    if (layout === 'docs') {
      assert.equal(await page.locator('.readmd-layout-flow').evaluate(node => getComputedStyle(node).gridTemplateColumns.split(' ').length), 1);
      assert.equal(await page.locator('.readmd-layout-nav').evaluate(node => getComputedStyle(node).position), 'static');
    }
    await page.evaluate(() => { document.getElementById('content').style.width = ''; });
    await page.emulateMedia({ media: 'print' });
    const print = await page.evaluate(() => {
      const article = document.querySelector('.readmd-layout-article'), nav = document.querySelector('.readmd-layout-nav');
      return { border: getComputedStyle(article).borderTopWidth, padding: getComputedStyle(article).paddingTop, codeWrap: getComputedStyle(article.querySelector('pre')).whiteSpace, codeKeep: getComputedStyle(article.querySelector('pre')).breakInside, tableKeep: getComputedStyle(article.querySelector('.readmd-layout-table')).breakInside, tableOverflow: getComputedStyle(article.querySelector('.readmd-layout-table')).overflowX, nav: nav && getComputedStyle(nav).display };
    });
    assert.equal(print.border, '0px'); assert.equal(print.padding, '0px'); assert.equal(print.codeWrap, 'pre-wrap'); assert.equal(print.tableOverflow, 'visible');
    assert.equal(print.codeKeep, 'avoid'); assert.equal(print.tableKeep, 'avoid');
    if (layout === 'docs') assert.equal(print.nav, 'none');
    await page.emulateMedia({ media: 'screen' });
  }
}));
