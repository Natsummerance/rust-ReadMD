const { test, expect } = require('@playwright/test');
const fs = require('node:fs');
const path = require('node:path');
const { pathToFileURL } = require('node:url');
const { getWebviewContent } = require('../packages/vscode-extension/out/webview.js');

for (const presentation of [false,true]) {
  test(`VS Code ${presentation ? 'presentation' : 'preview'} renders locally with CSP, formulas and Mermaid`, async ({ browser }) => {
    const context = await browser.newContext({ bypassCSP: false });
    const page = await context.newPage();
    const network = [], errors = [];
    page.on('pageerror', error => errors.push(error.message));
    const media = path.resolve(__dirname, '../packages/vscode-extension/media');
    await page.route('https://readmd-preview.test/**', async route => {
      const relative = new URL(route.request().url()).pathname.replace(/^\/media\//, '');
      const file = path.resolve(media, relative);
      if (!file.startsWith(media + path.sep) || !fs.existsSync(file)) return route.abort();
      const types = { '.html': 'text/html', '.js': 'text/javascript', '.css': 'text/css', '.woff2': 'font/woff2', '.woff': 'font/woff', '.ttf': 'font/ttf' };
      await route.fulfill({ body: fs.readFileSync(file), contentType: types[path.extname(file)] || 'application/octet-stream', headers: { 'Access-Control-Allow-Origin': '*' } });
    });
    page.on('request', request => { if (!request.url().startsWith('https://readmd-preview.test/')) network.push(request.url()); });
    await page.route('https://readmd-preview.test/documents/**', route => route.fulfill({body: Buffer.from('iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVQIHWP4z8DwHwAFgAI/ScLbtAAAAABJRU5ErkJggg==','base64'),contentType:'image/png'}));
    const markdown = '---\ntitle: Metadata must not become a slide\npresentation:\n  theme: white\n  transition: fade\n---\n\n# Offline acceptance\n\n[Jump](#second-slide)\n\n![Inline](<local image.png>)\n\n![Reference][image]\n\n[image]: local.png\n\n```markdown\n![Literal](example.png)\n```\n\n$x^2$\n\n```mermaid\ngraph LR\n A-->B\n```\n\n<script>window.pwned=true</script>\n<img src="https://private.invalid/tracker" onerror="window.pwned=true">\n\n---\n\n# Second slide';
    const html = getWebviewContent(markdown, '</title><script>window.pwned=true</script>', {
      base: 'https://readmd-preview.test/media', cspSource: 'https://readmd-preview.test', language: 'zh-CN',
      documentBase: 'https://readmd-preview.test/documents/',
    }, presentation);
    await page.setContent(html);
    await expect(page.locator('.katex')).toHaveCount(1);
    await expect(page.locator('.mermaid svg')).toHaveCount(1);
    if (presentation) {
      await expect(page.locator('.slides > section')).toHaveCount(2);
      await expect(page.locator('#readmd-slide-theme')).toHaveAttribute('href', /\/white.css$/);
      await expect(page.locator('.reveal')).toHaveClass(/fade/);
    }
    await expect(page.locator('code.language-markdown')).toHaveText('![Literal](example.png)\n');
    await expect(page.locator('img[alt="Inline"]')).toHaveAttribute('src', /documents\/local%20image.png$/);
    await expect(page.locator('img[alt="Reference"]')).toHaveAttribute('src', /documents\/local.png$/);
    await expect(page.locator('#second-slide')).toHaveCount(1);
    expect(await page.evaluate(() => window.pwned)).toBeUndefined();
    expect(network).toEqual([]); expect(errors).toEqual([]);
    await context.close();
  });
}

const diagrams = {
  wavedrom: '{signal:[{name:"clk",wave:"p...."},{name:"data",wave:"x.23x",data:["A","B"]}]}',
  bitfield: '{reg:[{bits:8,name:"A"},{bits:8,name:"B"}]}',
  graphviz: 'digraph { A -> B }',
  'vega-lite': JSON.stringify({ data: { values: [{x:'A', y:12},{x:'B', y:20}] }, mark:'bar', encoding:{x:{field:'x',type:'nominal'},y:{field:'y',type:'quantitative'}} }),
  chart: JSON.stringify({type:'bar',data:{labels:['A'],datasets:[{data:[12]}]}}),
  tikz: '\\begin{tikzpicture}\\draw (0,0) -- (1,1);\\end{tikzpicture}',
};
for (const theme of ['black','night']) test(`native exported ${theme} presentation renders with no network requests`, async ({ page }) => {
  test.skip(!process.env.READMD_PRESENTATION_TEST_ROOT, 'Run the real MCP fixture generator first');
  const file = path.join(process.env.READMD_PRESENTATION_TEST_ROOT, `slides-${theme}.html`);
  const requests = [], errors = [];
  page.on('request', request => { if (!/^(data:|file:)/.test(request.url())) requests.push(request.url()); });
  page.on('pageerror', error => errors.push(error.message));
  await page.route('**/*', route => route.request().url().startsWith('file:') ? route.continue() : route.abort());
  await page.goto(pathToFileURL(file).href);
  await expect.poll(()=>page.evaluate(()=>window.deck?.isReady())).toBe(true);
  await expect(page.locator('.slides section')).toHaveCount(2);
  await expect(page.locator('.katex')).toHaveCount(1);
  await expect(page.locator('.mermaid svg')).toHaveCount(1);
  await expect(page.locator('.mermaid .error-text')).toHaveCount(0);
  await expect.poll(()=>page.locator('img[alt="Local"]').evaluate(element=>element.complete && element.naturalWidth>0)).toBe(true);
  expect(requests).toEqual([]);expect(errors).toEqual([]);
});
for (const [engine, code] of Object.entries(diagrams)) test(`${engine} renders inside a disposable offline sandbox`, async ({ page }) => {
  test.setTimeout(120000);
  const media = path.resolve(__dirname, '../packages/vscode-extension/media');
  const errors = [], external = [];
  page.on('pageerror', error => errors.push(error.message));
  await page.route('https://readmd-preview.test/**', async route => {
    const relative = new URL(route.request().url()).pathname.replace(/^\/media\//, '');
    const file = path.resolve(media, relative);
    if (!file.startsWith(media + path.sep) || !fs.existsSync(file)) return route.abort();
    const types = { '.html': 'text/html', '.js': 'text/javascript', '.css': 'text/css', '.wasm': 'application/wasm' };
    await route.fulfill({ body: fs.readFileSync(file), contentType: types[path.extname(file)] || 'application/octet-stream', headers: { 'Access-Control-Allow-Origin': '*' } });
  });
  page.on('request', request => { if (!request.url().startsWith('https://readmd-preview.test/')) external.push(request.url()); });
  const markdown = '```' + engine + '\n' + code + '\n```';
  await page.setContent(getWebviewContent(markdown, 'Engineering', {base:'https://readmd-preview.test/media',cspSource:'https://readmd-preview.test'}));
  try { await expect(page.locator('figure.diagram > img')).toHaveCount(1, { timeout: 100000 }); }
  catch (error) { console.log(JSON.stringify({ errors, external, figures: await page.locator('figure.diagram').allTextContents() })); throw error; }
  for (const image of await page.locator('figure.diagram > img').all()) await expect.poll(() => image.evaluate(element => element.complete && element.naturalWidth > 0)).toBe(true);
  expect(await page.locator('iframe').count()).toBe(0);
  expect(external).toEqual([]); expect(errors).toEqual([]);
});
