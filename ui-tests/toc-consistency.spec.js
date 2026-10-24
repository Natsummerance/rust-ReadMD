const path = require('node:path');
const { test, expect } = require('@playwright/test');
const { waitForPage } = require('./page-condition.cjs');
const fixture = require('../tests/fixtures/toc-consistency.cjs');
const T = require('../assets/js/editor/md-transforms.js');
const H = require('../assets/js/reader/heading-identity.js');
const entities = require('../assets/vendor/html5-character-references.json');

test.use({ bypassCSP: false });

async function loadStandalone(page) {
  // A real origin is required by the production URL sanitizer. All requests
  // stay inside this fixture route; no native host or network is needed.
  await page.route('http://readmd-toc.test/**', route => route.fulfill({
    contentType: 'text/html', body: '<main id="body"></main><nav id="toc"></nav>',
  }));
  await page.goto('http://readmd-toc.test/');
  await page.evaluate(() => { window.state = { breakOnSingleNewline: false }; });
  for (const file of ['vendor/marked.min.js', 'js/reader/heading-identity.js', 'js/reader/formula.js', 'js/reader/render.js', 'js/editor/md-transforms.js']) {
    await page.addScriptTag({ path: path.resolve(__dirname, '../assets', file) });
  }
  await page.evaluate(() => {
    window.renderTocFixture = source => {
      const transformed = transformAcademicCallouts(transformWikilinks(readerPrepare(source)));
      const protectedMath = protectMath(transformed);
      return sanitizeRenderedHtml(restoreMath(parseMarkdownWithSourceMap(protectedMath.src), protectedMath.saved));
    };
  });
}

test('standalone TOC matches Node and every rendered fragment reaches its exact heading', async ({ page }) => {
  await loadStandalone(page);
  const result = await page.evaluate(source => {
    const body = document.getElementById('body');
    body.innerHTML = renderTocFixture(source);
    ReadMDHeadings.applyDom(body);
    const toc = ReadMDTransforms.tocMarkdown(source);
    document.getElementById('toc').innerHTML = renderTocFixture(toc);
    return {
      toc,
      headings: Array.from(body.querySelectorAll('h1,h2,h3'), h => ({ id: h.id, text: h.dataset.readmdHeadingText, level: +h.tagName.slice(1) })),
      labels: Array.from(document.querySelectorAll('#toc a'), a => a.textContent),
      destinations: Array.from(document.querySelectorAll('#toc a'), a => decodeURIComponent(a.getAttribute('href').slice(1))),
    };
  }, fixture.source);
  expect(result.toc).toBe(T.tocMarkdown(fixture.source));
  expect(result.headings).toEqual(fixture.headings);
  expect(result.labels).toEqual(fixture.headings.map(h => h.text));
  expect(result.destinations).toEqual(fixture.headings.map(h => h.id));
  await expect(page.locator('#body h1[id="ref"] a')).toHaveAttribute('href', '/docs');
  const links = page.locator('#toc a');
  for (let i = 0; i < fixture.headings.length; i++) {
    await links.nth(i).click();
    await expect.poll(() => page.evaluate(() => document.querySelector(':target')?.id)).toBe(fixture.headings[i].id);
  }
  // Reapplying IDs must not accumulate suffixes or overwrite a raw explicit ID.
  expect(await page.evaluate(() => {
    ReadMDHeadings.applyDom(document.getElementById('body'));
    return Array.from(document.querySelectorAll('#body h1,#body h2,#body h3'), h => h.id);
  })).toEqual(fixture.headings.map(h => h.id));
});

test('standalone literal TOC labels survive math protection and opt out of later typesetting', async ({ page }) => {
  await loadStandalone(page);
  const source = '# Literal `\\[x\\] \\(y\\) $$z$$`\n\n# Genuine $x^2$';
  const result = await page.evaluate(source => {
    const toc = ReadMDTransforms.tocMarkdown(source);
    const protectedMath = protectMath(toc);
    document.getElementById('toc').innerHTML = renderTocFixture(toc);
    return {
      protectedCount: protectedMath.saved.length,
      links: Array.from(document.querySelectorAll('#toc a'), a => ({
        text: a.textContent, id: decodeURIComponent(a.getAttribute('href').slice(1)), ignoresMath: a.classList.contains('mathjax_ignore'),
      })),
      genuineMathStillAllowed: !parseMarkdownWithSourceMap(protectMath('[Formula $x^2$](#genuine-x2)').src).includes('mathjax_ignore'),
    };
  }, source);
  expect(result.protectedCount).toBe(0);
  expect(result.genuineMathStillAllowed).toBe(true);
  expect(result.links).toEqual([
    { text: 'Literal \\[x\\] \\(y\\) $$z$$', id: 'literal-x-y-z', ignoresMath: true },
    { text: 'Genuine $x^2$', id: 'genuine-x2', ignoresMath: true },
  ]);
  await page.evaluate(source => {
    const body = document.getElementById('body');
    body.innerHTML = renderTocFixture(source);
    ReadMDHeadings.applyDom(body);
    window.MathJax = {
      tex: { inlineMath: [['$', '$'], ['\\(', '\\)']], displayMath: [['$$', '$$'], ['\\[', '\\]']] },
      startup: { typeset: false },
    };
  }, source);
  await page.addScriptTag({ path: path.resolve(__dirname, '../assets/vendor/mathjax/tex-svg.js') });
  await page.evaluate(async () => {
    await MathJax.startup.promise;
    await MathJax.typesetPromise([document.getElementById('body'), document.getElementById('toc')]);
  });
  await expect(page.locator('#body h1[id="genuine-x2"] mjx-container')).toHaveCount(1);
  await expect(page.locator('#body h1[id="literal-x-y-z"] mjx-container')).toHaveCount(0);
  await expect(page.locator('#toc a mjx-container')).toHaveCount(0);
  expect(await page.locator('#toc a').allTextContents()).toEqual(result.links.map(link => link.text));
});

test('standalone HTML5 entity decoding matches the browser for the entire named table', async ({ page }) => {
  await loadStandalone(page);
  const references = Object.keys(entities).filter(name => name.endsWith(';')).map(name => '&' + name);
  references.push('&#0;', '&#xD800;', '&#x110000;', '&#128;', '&#13;', '&notit;', '&copy', '&#12', '&unknown;');
  const source = references.map(reference => '# Ref ' + reference + '\n').join('\n');
  const browser = await page.evaluate(source => {
    const headings = ReadMDHeadings.fromMarkdown(source).map(({id, text}) => ({id, text}));
    const body = document.getElementById('body');
    body.innerHTML = marked.parse(source);
    return { headings, displayed: Array.from(body.querySelectorAll('h1'), h => h.textContent) };
  }, source);
  const node = H.fromMarkdown(source).map(({id, text}) => ({id, text}));
  expect(browser.headings).toEqual(node);
  expect(browser.displayed).toEqual(node.map(h => h.text));
});

test('native reader, Rust inspection and slash TOC retain exact source identities through preview links', async ({ page }) => {
  const source = fixture.source + '\n';
  const ids = fixture.headings.map(h => h.id);
  await page.goto('/');
  await waitForPage(page, () => window.__readmdAppReady);
  await page.evaluate(async source => { await renderVirtual('clipboard', 'TOC-consistency.md', '', source, []); }, source);
  const readerHeadings = page.locator('#content h1,#content h2,#content h3');
  expect(await readerHeadings.evaluateAll(nodes => nodes.map(n => n.id))).toEqual(ids);
  expect(await page.evaluate(source => ReadMDTransforms.tocMarkdown(source), source)).toBe(T.tocMarkdown(source));
  const inspected = await page.evaluate(async content => {
    const response = await apiFetch('/api/document/analyze', {
      method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ content }),
    });
    return response.json();
  }, source);
  expect(inspected.ok).toBe(true);
  expect(inspected.headings.map(h => h.id)).toEqual(ids);
  await page.evaluate(async () => { await toggleEdit(); setEditorPref('aiCompletion', false); setPvLayout('right'); });
  expect(await page.evaluate(() => getEditContent())).toBe(source);
  await page.evaluate(() => { cmView.dispatch({ selection: { anchor: cmView.state.doc.length } }); cmView.focus(); });
  await page.keyboard.type('/toc');
  await expect(page.locator('#cm-slash-menu')).toBeVisible();
  await page.keyboard.press('Enter');
  await expect(page.locator('#cm-slash-menu')).toBeHidden();
  expect(await page.evaluate(() => getEditContent())).toBe(source + T.tocMarkdown(source));
  const previewHeadings = page.locator('#preview-pane h1,#preview-pane h2,#preview-pane h3');
  await expect(previewHeadings).toHaveCount(ids.length);
  expect(await previewHeadings.evaluateAll(nodes => nodes.map(n => n.id))).toEqual(ids);
  const links = page.locator('#preview-pane li > a[href^="#"]');
  await expect(links).toHaveCount(ids.length);
  await expect(page.locator('#preview-pane h1[id="genuine-x2"] mjx-container')).toHaveCount(1);
  await expect(links.locator('mjx-container')).toHaveCount(0);
  expect(await links.allTextContents()).toEqual(fixture.headings.map(h => h.text));
  for (let i = 0; i < ids.length; i++) {
    await links.nth(i).click();
    await expect(previewHeadings.nth(i)).toBeFocused();
  }
  expect(await page.evaluate(() => getEditContent())).toBe(source + T.tocMarkdown(source));
});
