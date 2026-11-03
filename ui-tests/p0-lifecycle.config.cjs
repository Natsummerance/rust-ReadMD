// Shared production-CSP / real HTTP regression lane. This is browser evidence,
// not certification of a Windows/Linux native chooser or window host.
const path = require('node:path');
const base = require('./playwright.config.js');
const output = path.resolve(__dirname, '../.cache/p0-lifecycle');
module.exports = {
  ...base,
  testDir: __dirname,
  testMatch: ['document-decision-lifecycle.spec.js', 'document-lifecycle.spec.js', 'session-recovery.spec.js'],
  outputDir: path.join(output, 'traces'),
  reporter: [['list'], ['json', { outputFile: path.join(output, 'report.json') }]],
  projects: [{ name: 'p0-desktop', use: { browserName: 'chromium', headless: true, viewport: { width: 1160, height: 820 } } }],
};
