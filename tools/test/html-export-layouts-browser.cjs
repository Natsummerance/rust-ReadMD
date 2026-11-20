'use strict';
// Real Rust HTML exports, rendered by installed Chrome with network disabled.
// No renderer/API mocks, provider calls, shared profile, or native app session.
const fs = require('node:fs'), path = require('node:path'), crypto = require('node:crypto');
const assert = require('node:assert/strict'), { spawn, execFileSync } = require('node:child_process'), { pathToFileURL } = require('node:url');
const ROOT = path.resolve(__dirname, '../..'), sha = bytes => crypto.createHash('sha256').update(bytes).digest('hex');
const source = `# 让复杂内容，成为清楚的阅读

一份文稿可以有不同的阅读节奏。出版阅读让论述从容展开，技术文档让步骤和参数容易查找，长文杂志让标题、导语和正文形成有层次的叙事。本文是一份排版检查样稿。

## 先建立一条阅读线索

读者不必在每一段开始时重新寻找方向。标题说明接下来的问题，段落把一个意思说完整，图表则在需要比较时出现。清楚的层次既来自字号，也来自段落之间的距离。

同一份内容在桌面和手机上应保持连贯。正文留出足够的呼吸空间，链接可以辨认，较长的技术标识仍能完整保留，例如 \`document.publication.renderOptions\`。

> 好的版式让读者看清内容之间的关系。它应该在长文中保持节奏，也应该容得下一个复杂的表格或一行无法缩短的代码。

### 从结构走向细节

- 用主标题建立文章的主题。
- 用小标题划分可以独立理解的内容。
- 把需要逐项完成的工作写成顺序明确的步骤。

![三种阅读节奏的示意图](layout-rhythm.svg)

## 让信息有合适的位置

下表使用同一组虚构检查项，比较三种版式的阅读重点。文字、数据和列对齐都应在导出后继续保留。

| 版式 | 主要阅读方式 | 需要检查的细节 |
| :--- | :--- | :--- |
| 出版阅读 | 连续阅读与批注 | 行长、段落节奏、引用和链接 |
| 技术文档 | 查找步骤与参数 | 目录层级、表格与代码完整性 |
| 长文杂志 | 从导语进入叙事 | 标题张力、章节留白与图片比例 |

### 一个完整的操作顺序

1. 阅读标题和导语，确认文章的主题。
2. 通过目录或小标题找到需要的段落。
3. 对照示例检查关键参数，再回到上下文理解结论。

## 把技术细节完整留下

代码在屏幕上保留原始行，较窄的屏幕可以横向查看；打印时则展开长行，以免边缘的内容被裁掉。行内代码和普通段落使用不同的视觉节奏。

\`\`\`js
const article = {
  layout: "docs",
  title: "让复杂内容，成为清楚的阅读",
  options: { preserveSource: true, keepTableAlignment: true }
};
const completePath = "/documents/research/typography/export-layouts/keep-the-entire-original-code-line-visible-without-changing-its-source";
renderArticle(article, completePath);
\`\`\`

### 核对显示与原稿

样稿中的数字仅用于检查排印：12.50、108.25、1,024。中英文混排也应保持自然，例如 *A clear structure makes a long document easier to follow.*

## 留下完整的结尾

一篇长文的最后一段同样需要合适的留白。文末应结束在一个完整的意思上，表格、图片和代码不应该把页面的边界推到屏幕之外。

本文只用于 ReadMD HTML 导出版式的本地视觉检查。
`;

const figure = `<svg xmlns="http://www.w3.org/2000/svg" width="960" height="270" viewBox="0 0 960 270"><rect width="960" height="270" rx="14" fill="#e9efe9"/><path d="M80 180H880" stroke="#adbdaf" stroke-width="2"/><g fill="#3e715d"><rect x="100" y="80" width="180" height="10" rx="5"/><rect x="100" y="110" width="180" height="5" rx="2.5"/><rect x="100" y="128" width="160" height="5" rx="2.5"/><rect x="100" y="146" width="170" height="5" rx="2.5"/></g><g fill="#4569ba"><rect x="385" y="76" width="48" height="82" rx="4"/><rect x="450" y="80" width="150" height="8" rx="4"/><rect x="450" y="105" width="120" height="5" rx="2.5"/><rect x="450" y="125" width="150" height="5" rx="2.5"/><rect x="450" y="145" width="130" height="5" rx="2.5"/></g><g fill="#a76745"><rect x="700" y="68" width="140" height="18" rx="4"/><rect x="700" y="107" width="170" height="7" rx="3"/><rect x="700" y="133" width="130" height="5" rx="2.5"/><rect x="700" y="149" width="150" height="5" rx="2.5"/></g><g font-family="Arial,PingFang SC,sans-serif" font-size="23" text-anchor="middle" fill="#314339"><text x="190" y="224">连续阅读</text><text x="490" y="224">查找与执行</text><text x="785" y="224">叙事与停顿</text></g></svg>`;

function mcp(binary, documents, profile, temporary) {
  const child = spawn(binary, ['--mcp', '--data-dir', profile, '--workspace', documents, '--assets', path.join(ROOT, 'assets')], {
    cwd: ROOT, stdio: ['pipe', 'pipe', 'ignore'], env: { ...process.env, READMD_DATA_DIR: profile, READMD_WORKSPACE: documents, READMD_EXTRA_ROOTS: '', READMD_ASSETS_DIR: '', TMPDIR: temporary, CFFIXED_USER_HOME: path.join(profile, 'home') },
  });
  let buffer = '', next = 1; const pending = new Map();
  const fail = reason => { for (const call of pending.values()) { clearTimeout(call.timer); call.reject(Error(reason)); } pending.clear(); };
  child.stdout.setEncoding('utf8'); child.stdout.on('data', chunk => {
    buffer += chunk; if (buffer.length > 8 * 1024 * 1024) { fail('MCP response budget exceeded'); return; }
    for (;;) { const end = buffer.indexOf('\n'); if (end < 0) break; const line = buffer.slice(0, end); buffer = buffer.slice(end + 1); if (!line.trim()) continue;
      let value; try { value = JSON.parse(line); } catch { fail('Invalid MCP JSON'); return; }
      const call = pending.get(value.id); if (!call) continue; pending.delete(value.id); clearTimeout(call.timer); value.error ? call.reject(Error('MCP protocol error')) : call.resolve(value.result);
    }
  });
  child.on('error', () => fail('Owned MCP failed to start')); child.on('exit', () => fail('Owned MCP exited')); child.stdin.on('error', () => fail('Owned MCP input closed'));
  const request = (method, params) => new Promise((resolve, reject) => { const id = next++, timer = setTimeout(() => { pending.delete(id); reject(Error('MCP deadline: ' + method)); }, 30000); pending.set(id, { resolve, reject, timer }); child.stdin.write(JSON.stringify({ jsonrpc: '2.0', id, method, params }) + '\n'); });
  return {
    initialize: async () => { await request('initialize', { protocolVersion: '2024-11-05', capabilities: {}, clientInfo: { name: 'readmd-html-layout-qa', version: '1' } }); child.stdin.write(JSON.stringify({ jsonrpc: '2.0', method: 'notifications/initialized' }) + '\n'); },
    export: async (file, options, preset) => { const result = await request('tools/call', { name: 'readmd_export_document', arguments: { markdown_content: source, output_path: file, output_format: 'html', base_dir: documents, style_preset: preset, options, title: 'HTML 排版检查样稿', confirm: true } });
      const value = result.structuredContent || JSON.parse(result.content.find(item => item.type === 'text').text); assert.equal(value.ok, true, JSON.stringify(value)); return value; },
    close: async () => { fail('Closing owned MCP'); const exited = new Promise(resolve => child.once('exit', resolve)); child.stdin.end(); if (child.exitCode === null && child.signalCode === null) await Promise.race([exited, new Promise(resolve => setTimeout(resolve, 1000))]); if (child.exitCode === null && child.signalCode === null) { child.kill(); await exited; } },
  };
}

async function run(binary, output) {
  assert(path.isAbsolute(binary) && fs.statSync(binary).isFile(), 'Explicit candidate binary required');
  assert(output.startsWith(ROOT + path.sep), 'Evidence must stay in the workspace');
  fs.mkdirSync(output, { mode: 0o700 });
  const documents = path.join(output, 'documents'), profile = path.join(output, 'profile'), temporary = path.join(output, 'tmp'), home = path.join(output, 'home');
  for (const directory of [documents, profile, temporary, home, path.join(profile, 'home')]) fs.mkdirSync(directory);
  const printOutput = path.join(output, 'output/pdf'); fs.mkdirSync(printOutput, { recursive: true });
  fs.writeFileSync(path.join(documents, 'sample.md'), source); fs.writeFileSync(path.join(documents, 'layout-rhythm.svg'), figure);
  const report = { passed: false, realExporter: true, browser: 'installed Chrome', nativeWebViewCertified: false, providerCalls: 0, artifacts: [], samples: [], print: [], candidateSha256: sha(fs.readFileSync(binary)), cssSha256: sha(fs.readFileSync(path.join(ROOT, 'assets/css/export-html-layouts.css'))), mountSha256: sha(fs.readFileSync(path.join(ROOT, 'assets/js/reader/export-html-layout.js'))) };
  const templates = JSON.parse(fs.readFileSync(path.join(ROOT, 'assets/data/export-templates.json'))).profiles.filter(value => value.formats.includes('html'));
  const client = mcp(binary, documents, profile, temporary); let context;
  try {
    await client.initialize();
    for (const template of templates) for (const tone of ['light', 'dark', 'sepia']) {
      const file = path.join(documents, template.options.htmlLayout + '-' + tone + '.html');
      const result = await client.export(file, { htmlTheme: tone }, template.id); const bytes = fs.readFileSync(file), html = bytes.toString();
      assert(html.includes(fs.readFileSync(path.join(ROOT, 'assets/css/export-html-layouts.css'), 'utf8').trim()), 'Candidate must embed the current HTML layout CSS');
      assert(html.includes(fs.readFileSync(path.join(ROOT, 'assets/js/reader/export-html-layout.js'), 'utf8').trim()), 'Candidate must embed the current layout mount');
      report.artifacts.push({ path: path.relative(output, file), layout: template.options.htmlLayout, tone, bytes: bytes.length, sha256: sha(bytes), warnings: result.warnings });
    }
    await client.close();
    const { chromium } = require(process.env.READMD_PLAYWRIGHT || require.resolve('playwright', { paths: [path.join(ROOT, 'ui-tests')] }));
    const env = Object.fromEntries(Object.entries(process.env).filter(([key]) => /^(PATH|SYSTEMROOT|WINDIR|COMSPEC|PATHEXT|LANG|LC_ALL)$/i.test(key)));
    Object.assign(env, { TMPDIR: temporary, CFFIXED_USER_HOME: home });
    context = await chromium.launchPersistentContext(path.join(output, 'chrome-profile'), { channel: 'chrome', headless: true, env, viewport: { width: 1440, height: 1050 }, args: ['--disable-crash-reporter', '--disable-breakpad', '--disable-background-networking', '--disable-component-update'] });
    const page = context.pages()[0], errors = []; page.on('pageerror', error => errors.push(error.message));
    await context.route(/^https?:/, route => route.abort());
    for (const artifact of report.artifacts) {
      await page.goto(pathToFileURL(path.join(output, artifact.path)).href);
      await page.waitForFunction(() => document.querySelector('.readmd-layout-article')?.textContent.includes('本文只用于 ReadMD'));
      await page.evaluate(async () => { await document.fonts.ready; if (window.MathJax?.startup?.promise) await MathJax.startup.promise; await Promise.all([...document.images].map(image => image.decode())); });
      for (const width of [375, 768, 1440]) {
        await page.setViewportSize({ width, height: 1050 });
        const sample = await page.evaluate(() => {
          const article = document.querySelector('.readmd-layout-article'), pre = article.querySelector('pre'), flow = document.querySelector('.readmd-layout-flow');
          const canvas = document.createElement('canvas'); canvas.width = canvas.height = 1; const ctx = canvas.getContext('2d');
          const rgb = color => { ctx.clearRect(0, 0, 1, 1); ctx.fillStyle = color; ctx.fillRect(0, 0, 1, 1); return [...ctx.getImageData(0, 0, 1, 1).data].map((value, i) => i === 3 ? value / 255 : value); };
          const contrast = node => {
            const layers = []; for (let current = node; current; current = current.parentElement) layers.push(rgb(getComputedStyle(current).backgroundColor));
            const background = layers.reverse().reduce((base, layer) => layer.slice(0, 3).map((value, i) => value * layer[3] + base[i] * (1 - layer[3])), [255, 255, 255]);
            const luminance = color => color.slice(0, 3).map(value => { value /= 255; return value <= .04045 ? value / 12.92 : ((value + .055) / 1.055) ** 2.4; }).reduce((sum, value, i) => sum + value * [.2126, .7152, .0722][i], 0);
            const a = luminance(rgb(getComputedStyle(node).color)), b = luminance(background); return (Math.max(a, b) + .05) / (Math.min(a, b) + .05);
          };
          return { width: innerWidth, overflow: document.documentElement.scrollWidth > innerWidth + 1, articleWidth: article.getBoundingClientRect().width, columns: getComputedStyle(flow).gridTemplateColumns,
            bodyColor: getComputedStyle(article).color, titleSize: getComputedStyle(article.querySelector('h1')).fontSize, paragraphSize: getComputedStyle(article.querySelector('p')).fontSize, codeWrap: getComputedStyle(pre).whiteSpace,
            navigation: [...document.querySelectorAll('.readmd-layout-nav a')].map(link => !!document.getElementById(decodeURIComponent(link.hash.slice(1)))), image: { width: article.querySelector('img').getBoundingClientRect().width, decoded: article.querySelector('img').naturalWidth > 0 },
            tableAlign: [...article.querySelectorAll('thead th')].map(cell => getComputedStyle(cell).textAlign), contrast: Object.fromEntries(['p', 'h1', 'h2', 'blockquote p', 'pre code', 'p code', 'th', 'td'].map(selector => [selector, contrast(article.querySelector(selector))])),
            allText: article.textContent.includes('renderArticle(article, completePath);') && article.textContent.includes('1,024') };
        });
        assert.equal(sample.overflow, false, artifact.layout + '/' + artifact.tone + '/' + width + ' overflow'); assert.equal(sample.codeWrap, 'pre'); assert.equal(sample.allText, true); assert.equal(sample.image.decoded, true); assert(sample.navigation.every(Boolean));
        assert.deepEqual(sample.tableAlign, ['left', 'left', 'left']);
        for (const [selector, ratio] of Object.entries(sample.contrast)) assert(ratio >= (selector === 'h1' || selector === 'h2' ? 3 : 4.5), artifact.layout + '/' + artifact.tone + ' low contrast ' + selector + ': ' + ratio);
        report.samples.push({ layout: artifact.layout, tone: artifact.tone, ...sample });
        if (artifact.tone === 'light' && [375, 1440].includes(width)) await page.screenshot({ path: path.join(output, artifact.layout + (width === 375 ? '-mobile.png' : '-desktop.png')) });
      }
      if (artifact.tone === 'dark') await page.screenshot({ path: path.join(output, artifact.layout + '-dark.png') });
      if (artifact.layout === 'docs') {
        const link = page.locator('.readmd-layout-nav a').last(), hash = await link.getAttribute('href');
        await link.click();
        const navigation = await page.evaluate(hash => { const heading = document.getElementById(decodeURIComponent(hash.slice(1))), rect = heading.getBoundingClientRect(); return { hash: location.hash, visible: rect.top >= 0 && rect.bottom <= innerHeight }; }, hash);
        assert.equal(navigation.hash, hash); assert.equal(navigation.visible, true);
        await page.evaluate(() => window.scrollTo(0, 0));
      }
    }
    // Chromium can keep print fragments across file navigations in the same
    // renderer. Complete every screen check before beginning the print pass.
    for (const artifact of report.artifacts.filter(value => value.tone === 'light')) {
        const printPage = await context.newPage(); printPage.on('pageerror', error => errors.push(error.message));
        await printPage.goto(pathToFileURL(path.join(output, artifact.path)).href);
        await printPage.waitForSelector('.readmd-layout-article');
        await printPage.evaluate(async () => { await document.fonts.ready; await Promise.all([...document.images].map(image => image.decode())); });
        await printPage.emulateMedia({ media: 'print' });
        const print = await printPage.evaluate(() => { const article = document.querySelector('.readmd-layout-article'); return { padding: getComputedStyle(article).paddingTop, border: getComputedStyle(article).borderTopWidth, nav: document.querySelector('.readmd-layout-nav') && getComputedStyle(document.querySelector('.readmd-layout-nav')).display, pre: getComputedStyle(article.querySelector('pre')).whiteSpace }; });
        assert.equal(print.padding, '0px'); assert.equal(print.border, '0px'); assert.equal(print.pre, 'pre-wrap'); if (artifact.layout === 'docs') assert.equal(print.nav, 'none');
        const pdf = path.join(printOutput, artifact.layout + '-print.pdf');
        await printPage.pdf({ path: pdf, printBackground: true, preferCSSPageSize: true, displayHeaderFooter: false });
        const toolEnv = { ...process.env, TMPDIR: temporary, CFFIXED_USER_HOME: home };
        const info = execFileSync('pdfinfo', [pdf], { encoding: 'utf8', env: toolEnv });
        const extracted = execFileSync(process.env.READMD_PDFTOTEXT || 'pdftotext', ['-layout', pdf, '-'], { encoding: 'utf8', env: toolEnv });
        const text = extracted.replace(/\s/g, ''), pageText = extracted.split('\f').map(value => value.replace(/\s/g, ''));
        const pages = Number(/^Pages:\s+(\d+)/m.exec(info)?.[1]); assert(pages > 0 && pages < 8);
        for (const phrase of ['让复杂内容，成为清楚的阅读', 'renderArticle(article,completePath);', '12.50、108.25、1,024', '本文只用于ReadMDHTML导出版式的本地视觉检查。']) assert(text.includes(phrase), artifact.layout + ' print missing ' + phrase);
        assert(pageText.some(value => ['连续阅读与批注', '查找步骤与参数', '从导语进入叙事'].every(phrase => value.includes(phrase))), artifact.layout + ' small table should stay on one page');
        assert(pageText.some(value => value.includes('constarticle={') && value.includes('renderArticle(article,completePath);')), artifact.layout + ' small code block should stay on one page');
        execFileSync('pdftoppm', ['-r', '100', '-png', pdf, path.join(printOutput, artifact.layout + '-page')], { env: toolEnv, stdio: 'ignore' });
        report.print.push({ layout: artifact.layout, pdf: path.relative(output, pdf), pages, textPreserved: true, smallTableTogether: true, smallCodeBlockTogether: true });
        await printPage.close();
    }
    assert.deepEqual(errors, []); report.passed = true;
  } catch (error) { report.error = error.message; throw error;
  } finally {
    if (context) await context.close(); await client.close();
    fs.writeFileSync(path.join(output, 'report.json'), JSON.stringify(report, null, 2) + '\n');
    for (const directory of [profile, temporary, home, path.join(output, 'chrome-profile')]) fs.rmSync(directory, { recursive: true, force: true });
  }
  console.log(JSON.stringify({ passed: true, actualHtmlFiles: report.artifacts.length, checkedViewports: report.samples.length, output }, null, 2));
}

if (require.main === module) {
  const [binary, output] = process.argv.slice(2);
  if (!binary || !output) throw Error('Usage: html-export-layouts-browser.cjs <explicit candidate binary> <new workspace evidence directory>');
  run(path.resolve(binary), path.resolve(output)).catch(error => { console.error(error.message); process.exitCode = 1; });
}
module.exports = { run, source };
