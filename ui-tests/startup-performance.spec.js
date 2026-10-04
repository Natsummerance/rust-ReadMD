const { test, expect } = require('@playwright/test');

function median(values) {
  return [...values].sort((left, right) => left - right)[Math.floor(values.length / 2)];
}

test('welcome startup stays lightweight and interactive below one second', async ({ browser, request }, testInfo) => {
  test.skip(testInfo.project.name !== 'desktop', 'Startup budget is measured once on the desktop project.');
  const samples = [];
  // Earlier persistence tests intentionally enable pets on the shared server.
  // Restore the real default configuration rather than fetching and rewriting
  // intercepted responses while a sample's browser context is closing.
  const reset = await request.post('/api/pets/configure', { data: { enabled: false, in_app: true } });
  expect(reset.ok()).toBe(true);

  for (let index = 0; index < 3; index += 1) {
    const context = await browser.newContext({ viewport: { width: 1280, height: 800 } });
    const page = await context.newPage();
    await page.route('**/api/update/check', route => route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({ ok: false }),
    }));
    await page.addInitScript(() => localStorage.setItem('readmd_language', 'zh-CN'));

    let initialRequests = [];
    page.on('request', request => {
      if (request.url().startsWith('http://')) initialRequests.push(request.url());
    });

    await page.goto('/', { waitUntil: 'domcontentloaded' });
    await page.waitForFunction(() => window.__readmdAppReady === true, null, { timeout: 5000 });
    const metrics = await page.evaluate(() => {
      const navigation = performance.getEntriesByType('navigation')[0];
      const paint = performance.getEntriesByType('paint').find(entry => entry.name === 'first-contentful-paint');
      const resources = performance.getEntriesByType('resource');
      return {
        ready: performance.now(),
        domContentLoaded: navigation.domContentLoadedEventEnd,
        load: navigation.loadEventEnd,
        firstContentfulPaint: paint?.startTime ?? Number.POSITIVE_INFINITY,
        transferredBytes: resources.reduce((total, entry) => total + (entry.transferSize || entry.encodedBodySize || 0), 0),
        requestCount: resources.length,
      };
    });
    metrics.initialRequests = initialRequests;
    samples.push(metrics);
    await page.unrouteAll({ behavior: 'wait' });
    await context.close();
  }

  const ready = median(samples.map(sample => sample.ready));
  const firstContentfulPaint = median(samples.map(sample => sample.firstContentfulPaint));
  const transferredBytes = Math.max(...samples.map(sample => sample.transferredBytes));
  expect(ready).toBeLessThan(1200);
  expect(firstContentfulPaint).toBeLessThan(850);
  expect(transferredBytes).toBeLessThan(1_250_000);
  expect(Math.max(...samples.map(sample => sample.requestCount))).toBeLessThanOrEqual(35);
  for (const sample of samples) {
    expect(sample.initialRequests.some(url => url.includes('/vendor/qrcode.min.js'))).toBe(false);
    expect(sample.initialRequests.some(url => url.includes('cdn.jsdelivr'))).toBe(false);
    expect(sample.initialRequests.some(url => url.includes('/api/pets/thumb'))).toBe(false);
  }
});
