const { defineConfig } = require('@playwright/test');
// Run after compiling packages/vscode-extension. No desktop server or network.
module.exports = defineConfig({
  testDir: '.', testMatch: 'vscode-preview.spec.cjs', timeout: 30000, workers: 1,
  use: { bypassCSP: false, viewport: { width: 1024, height: 680 } },
  projects: [{ name: 'chromium', use: { browserName: 'chromium' } }, { name: 'webkit', use: { browserName: 'webkit' } }],
});
