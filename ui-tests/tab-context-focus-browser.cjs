'use strict';

const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const { openOwned, sha, until } = require('../tools/test/owned-browser-host.cjs');

const [binary, output] = process.argv.slice(2);
assert(path.isAbsolute(binary) && path.isAbsolute(output));
const report = { passed: false, nativeUiCertified: false, checks: [], resumeResponses: [] };
const fixtures = ['无审阅.md', '已有审阅.md', '焦点对照.md'].map((name, index) => ({ name, text: `# 测试 ${index}\n\n工作区专用 😀\n` }));

async function withHeldResume(page, fixture, expectedFound, activate, verify) {
  const endpoint = /\/api\/review$/;
  const released = Promise.withResolvers();
  let observed, failure, intercepted;
  const routeHandler = route => {
    const request = route.request();
    if (request.method() !== 'POST') return route.continue();
    const { action, path: requestedPath } = request.postDataJSON();
    if (action !== 'resume' || requestedPath !== fixture.path) return route.continue();
    if (intercepted) return route.abort();
    intercepted = (async () => {
      try {
        const response = await route.fetch({ timeout: 10000 });
        const body = await response.body();
        assert.equal(response.status(), 200);
        const value = JSON.parse(body);
        assert.equal(value.found, expectedFound);
        if (expectedFound) assert.equal(value.snapshot.content, fixture.text);
        observed = { found: value.found, sha256: sha(body) };
        await released.promise;
        await route.fulfill({ response });
      } catch (error) {
        failure = error;
        await route.abort().catch(() => {});
      }
    })();
    return intercepted;
  };
  await page.route(endpoint, routeHandler);
  try {
    await activate();
    await until(() => observed || failure, 'real resume response');
    if (failure) throw failure;
    await verify(async () => {
      released.resolve();
      await intercepted;
      if (failure) throw failure;
    });
    report.resumeResponses.push(observed);
  } finally {
    released.resolve();
    await page.unroute(endpoint, routeHandler);
    await intercepted;
  }
}

async function run() {
  const host = await openOwned(output, (_, dirs) => {
    for (const fixture of fixtures) {
      fixture.path = path.join(dirs.documents, fixture.name);
      fs.writeFileSync(fixture.path, fixture.text, { flag: 'wx' });
    }
    fs.writeFileSync(path.join(dirs.profile, 'settings.json'), JSON.stringify({ lang: 'zh-CN', autoCheckUpdate: false }), { flag: 'wx' });
  }, { binary });
  const { page } = host;
  try {
    for (const fixture of fixtures) {
      await page.evaluate(file => loadFile(file), fixture.path);
      fixture.id = await page.evaluate(file => state.tabs.find(tab => tab.path === file)?.id, fixture.path);
      assert(fixture.id);
    }
    assert.equal(await page.evaluate(() => state.activeTabId), fixtures[2].id);
    await host.api('/api/review', { action: 'open', path: fixtures[1].path });
    await page.evaluate(() => {
      addEventListener('keydown', event => {
        if (event.isTrusted && event.key === 'F10' && event.shiftKey) {
          window.__tabContextKeyResult = { tabId: event.target.dataset.tabId,
            immediateMenuFocus: document.getElementById('tab-context-menu').contains(document.activeElement) };
        }
      });
    });
    const tab = id => page.locator(`#doc-tabs-bar [role="tab"][data-tab-id="${id}"]`);
    const menu = page.locator('#tab-context-menu');
    for (const [index, fixture] of fixtures.entries()) {
      const row = tab(fixture.id);
      await withHeldResume(page, fixture, index === 1, () => row.click(), async deliver => {
        await until(() => page.evaluate(id => state.activeTabId === id, fixture.id), 'selected tab active');
        const keyboardTarget = index < 2 ? row : page.locator('#btn-ai');
        await keyboardTarget.focus();
        const previous = await row.elementHandle();
        assert(previous);
        try {
          assert(await keyboardTarget.evaluate(el => el === document.activeElement));
          await deliver();
          await until(() => previous.evaluate(el => !el.isConnected), 'real resume rebuilt the tab');
          assert(await keyboardTarget.evaluate(el => el === document.activeElement), 'Rebuild preserves existing keyboard focus');
          report.checks.push({ name: index < 2 ? 'Replacement tab retains focus' : 'Rebuild leaves other control focused', passed: true });
          if (index === 2) return;
          await page.keyboard.press('Shift+F10');
          assert.deepEqual(await page.evaluate(() => window.__tabContextKeyResult), { tabId: fixture.id, immediateMenuFocus: true });
          assert(await menu.isVisible());
          assert.equal(await menu.getAttribute('data-tab-id'), fixture.id);
          await page.keyboard.press('Escape');
          assert.equal(await menu.isVisible(), false);
          assert(await row.evaluate(el => el === document.activeElement), 'Escape returns focus to the replacement tab');
          report.checks.push({ name: 'Shift+F10 immediately focuses menu and Escape returns to tab', passed: true });
        } finally { await previous.dispose(); }
      });
    }
    for (const fixture of fixtures) assert.equal(fs.readFileSync(fixture.path, 'utf8'), fixture.text);
    assert.deepEqual(host.errors, []);
  } finally { await host.close(); }
  report.passed = true;
}

run().catch(error => {
  report.failure = { message: error.message.replaceAll(output, '<owned>') };
  process.exitCode = 1;
}).finally(() => {
  fs.writeFileSync(path.join(output, 'report.json'), JSON.stringify(report, null, 2) + '\n', { flag: 'wx' });
  console.log(JSON.stringify(report));
});
