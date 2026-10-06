// Exercise the real Rust routes and a local OpenAI-compatible provider.
const { test, expect } = require('@playwright/test');
const http = require('node:http');
const path = require('node:path');
let upstream, baseUrl, failure = false, calls = [];
const providerId = 'custom:discovery-regression';

test.beforeAll(async () => {
  upstream = http.createServer((req, res) => {
    calls.push({ url: req.url, auth: req.headers.authorization });
    const authenticated = req.headers.authorization === 'Bearer regression-fake-key';
    res.writeHead(failure || !authenticated ? 401 : 200, { 'Content-Type': 'application/json' });
    res.end(JSON.stringify(failure || !authenticated ? { error: 'regression-fake-key must not echo' } : { data: [{ id: 'regression-model' }, { id: 'another-model' }] }));
  });
  await new Promise(resolve => upstream.listen(0, '127.0.0.1', resolve));
  baseUrl = `http://127.0.0.1:${upstream.address().port}/v1`;
});
test.afterAll(async () => { await new Promise(resolve => upstream.close(resolve)); });
test.beforeEach(async ({ page }) => {
  failure = false; calls = [];
  await page.route('**/api/update/check', r => r.fulfill({ json: { ok: false } }));
  await page.addInitScript(() => localStorage.setItem('readmd_language', 'zh-CN'));
  await page.goto('/');
  await page.waitForFunction(() => typeof loadAiConfig === 'function');
  const saved = await page.evaluate(async ({ providerId, baseUrl }) => {
    const res = await apiFetch('/api/ai/config', { method: 'POST', headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ providers: [{ id: providerId, name: 'Test relay', base_url: baseUrl, api_key: 'regression-fake-key', models: [] }], current: { provider_id: providerId, model: '' } }) });
    const data = await res.json();
    await loadAiConfig();
    return { ok: res.ok && data.ok, containsKey: JSON.stringify(data).includes('regression-fake-key') };
  }, { providerId, baseUrl });
  expect(saved).toEqual({ ok: true, containsKey: false });
});
test.afterEach(async ({ page }) => {
  await page.evaluate(async providerId => {
    await apiFetch('/api/pets/configure', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ enabled: false, in_app: true }) });
    await apiFetch('/api/ai/config', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ providers: [{ id: providerId, name: 'Test relay', clear_key: true }], current: {} }) });
  }, providerId);
});

test('discover models with saved opaque credentials, persist selection and reuse it after reload', async ({ page }) => {
  await page.evaluate(async () => { await loadAiModels(); });
  expect(calls[0]).toEqual({ url: '/v1/models', auth: 'Bearer regression-fake-key' });
  expect(await page.locator('#ai-model').inputValue()).toBe('regression-model');
  expect(await page.locator('#ai-key').inputValue()).toBe('');
  await page.reload();
  await page.waitForFunction(() => typeof loadAiConfig === 'function');
  await page.evaluate(() => loadAiConfig());
  expect(await page.locator('#ai-model').inputValue()).toBe('regression-model');
  const actual = await page.evaluate(async () => {
    const res = await apiFetch('/api/ai/models', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: '{}' });
    return res.json();
  });
  expect(actual.source).toBe('provider');
  expect(actual.models).toEqual(['regression-model', 'another-model']);
});

test('authentication failure is visible, does not report success and never echoes provider secrets', async ({ page }) => {
  failure = true;
  await page.evaluate(() => loadAiModels());
  await expect(page.locator('#toast')).toContainText('401');
  expect(await page.locator('#toast').innerText()).not.toContain('regression-fake-key');
  expect(await page.locator('#ai-model').inputValue()).toBe('');
  expect(calls).toHaveLength(1);
});

test('compact checkboxes work by label and keyboard across themes without composer overflow', async ({ page }) => {
  for (const viewport of [{ width: 1160, height: 820 }, { width: 1024, height: 680 }, { width: 390, height: 844 }]) {
    await page.setViewportSize(viewport);
    await page.evaluate(() => { if ($('ai-panel').classList.contains('hidden')) toggleAiPanel(); });
    for (const theme of ['light', 'dark', 'sepia', 'green']) {
      await page.evaluate(theme => document.body.dataset.theme = theme, theme);
      const input = page.locator('#ai-incognito');
      const rect = await input.boundingBox();
      expect(rect.width).toBe(14); expect(rect.height).toBe(14);
      await input.locator('..').click();
      await expect(input).toBeChecked();
      await input.focus(); await page.keyboard.press('Space');
      await expect(input).not.toBeChecked();
      const fit = await page.locator('.composer-input-card').evaluate(el => el.scrollWidth <= el.clientWidth);
      expect(fit).toBe(true);
    }
  }
});

test('bundled desktop extension installs in user data and launches the real Rust overlay', async ({ page }) => {
  test.skip(process.platform !== 'win32' || !process.env.READMD_VERIFY_NATIVE_PET, 'requires a bundled native runtime');
  const result = await page.evaluate(async () => {
    const installed = await petGalleryRequest('/api/pets/runtime/install', { confirm: true });
    if (!installed.ok) return { installed };
    const configured = await requestConfigurePet({ enabled: true, in_app: false, renderer: 'hermes-sprite', character: 'bongocat', scale: .22 });
    return { installed, configured, status: await fetchPetRuntimeStatus() };
  });
  expect(result.installed.ok).toBe(true);
  if (process.env.READMD_DATA_DIR) {
    expect(path.resolve(result.installed.install_path)).toBe(path.resolve(process.env.READMD_DATA_DIR, 'plugins', 'pet', 'readmd-rust-host'));
  } else {
    expect(result.installed.install_path).toContain('readmd-ui-test-');
  }
  expect(result.configured.ok).toBe(true);
  expect(result.status.in_app).toBe(false);
  expect(result.status.adapter.rust.running).toBe(true);
  expect(result.status.adapter.rust.health.state).toBe('ready');
});

test('failed desktop installation preserves the real disabled state and does not silently switch runtimes', async ({ page }) => {
  const status = { enabled: false, in_app: true, installed: false, adapter: { available: false }, preferences: { renderer: 'hermes-sprite', scale: .22 } };
  let configured = 0;
  await page.route('**/api/pets/status', r => r.fulfill({ json: { ok: true, status } }));
  await page.route('**/api/pets/runtime/install', r => r.fulfill({ json: { ok: false, code: 'rust_runtime_install_failed' } }));
  await page.route('**/api/pets/configure', r => { configured++; return r.fulfill({ json: { ok: true } }); });
  const result = await page.evaluate(async status => {
    renderPetSettings(status);
    $('pet-runtime').value = 'desktop'; $('pet-enabled').checked = true;
    return savePetSettings();
  }, status);
  expect(result.ok).toBe(false);
  expect(configured).toBe(0);
  expect(await page.locator('#pet-enabled').isChecked()).toBe(false);
});
