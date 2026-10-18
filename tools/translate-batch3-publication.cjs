'use strict';
// The shared translation workflow timed out on two complete dictionaries.
// Use the same configured provider and native API with bounded, single-locale
// key groups. Keep each actual response/failure; never fill missing English.
const fs = require('node:fs'), path = require('node:path'), crypto = require('node:crypto'), assert = require('node:assert/strict');
const {chromium} = require('playwright');
const {startNative} = require('../ui-tests/native-session.cjs');
const root = path.resolve(__dirname, '..'), base = path.join(root, '.cache/capability-upgrade/translations/i18n-batch3-publication-metadata');
const specFile = path.join(root, 'tools/i18n-batch3-publication-metadata.json');
const spec = JSON.parse(fs.readFileSync(specFile, 'utf8'));
const meta = JSON.parse(fs.readFileSync(path.join(root, 'assets/i18n/meta.json'), 'utf8'));
const output = path.join(base, 'translated-small-groups.json'), reportFile = path.join(base, 'small-groups-report.json');
const sha = data => crypto.createHash('sha256').update(data).digest('hex');
if (fs.existsSync(output)) {
  const previous = JSON.parse(fs.readFileSync(output, 'utf8'));
  for (const [key, values] of Object.entries(previous)) if (spec[key]) for (const [locale, value] of Object.entries(values)) {
    if (!spec[key][locale] && Object.hasOwn(meta, locale)) spec[key][locale] = value;
  }
}
const keyBatch = Math.max(1, Math.min(48, Number(process.env.READMD_TRANSLATION_KEY_BATCH) || 24));
const concurrency = Math.max(1, Math.min(4, Number(process.env.READMD_TRANSLATION_CONCURRENCY) || 3));
const profile = fs.mkdtempSync(path.join(base, 'small-native-')), serviceBinary = path.join(profile, 'readmd-translation-service.exe');
const requests = path.join(base, 'small-requests'); fs.mkdirSync(requests, {recursive: true});
const runId = Date.now().toString(36);
const runReportFile = path.join(base, 'small-groups-report-' + runId + '.json');
const report = {schemaVersion: 1, runId, startedAt: new Date().toISOString(), provider: 'configured Antigravity relay', scriptSha256: sha(fs.readFileSync(__filename)), keyBatch, concurrency, failures: [], completedGroups: [], humanLinguisticReviewRecorded: false};
let native, phase = 'isolated service startup', consecutiveFailures = 0;
const persist = () => {
  fs.writeFileSync(output, JSON.stringify(spec, null, 2) + '\n');
  fs.writeFileSync(reportFile, JSON.stringify(report, null, 2) + '\n');
  fs.writeFileSync(runReportFile, JSON.stringify(report, null, 2) + '\n');
};
const placeholders = value => (value.match(/\{[\w.-]+\}/g) || []).sort();
(async () => {
  try {
    const binary = fs.readFileSync(process.env.READMD_BIN);
    assert.equal(sha(binary), '2a3fdf1752dfc9b9ab887fe3985c86be9ad7a7c2825f9cae52392429b0c40b55', 'Approved old service binary changed');
    const config = JSON.parse(fs.readFileSync(process.env.READMD_AI_SOURCE_CONFIG, 'utf8'));
    const relay = config.providers?.find(provider => /antigravity/i.test(provider.name || provider.label || ''));
    assert(relay && config.current, 'The explicitly authorized provider is unavailable');
    const proven = JSON.parse(fs.readFileSync(path.join(base, 'last-working-model.json'), 'utf8'));
    assert.equal(typeof proven.model, 'string'); report.model = proven.model; report.actualProbeCheckedAt = proven.checkedAt;
    config.current.provider_id = relay.id; config.current.model = proven.model;
    fs.writeFileSync(serviceBinary, binary); fs.writeFileSync(path.join(profile, 'ai.json'), JSON.stringify(config));
    process.env.READMD_BIN = serviceBinary;
    native = await startNative(root, profile, chromium, 28859);
    const api = body => native.page.evaluate(async body => {
      const response = await apiFetch('/api/ai/chat', {method: 'POST', headers: {'Content-Type': 'application/json'}, body: JSON.stringify(body), signal: AbortSignal.timeout(180000)});
      return {status: response.status, data: await response.json()};
    }, body);
    phase = 'single-locale small-group translation';
    const jobs = [];
    for (const locale of Object.keys(meta)) {
      const missing = Object.keys(spec).filter(key => !spec[key][locale]);
      for (let at = 0; at < missing.length; at += keyBatch) jobs.push({locale, keys: missing.slice(at, at + keyBatch)});
    }
    let cursor = 0;
    async function worker() {
      while (cursor < jobs.length && consecutiveFailures < 3) {
        const index = cursor++, job = jobs[index], id = runId + '-' + String(index + 1).padStart(4, '0');
        const record = {id, model: proven.model, locale: job.locale, keys: job.keys, startedAt: new Date().toISOString(), passed: false};
        try {
          const values = Object.fromEntries(job.keys.map(key => [key, spec[key].en]));
          const prompt = 'Translate these publication metadata and document editor UI values into ' + meta[job.locale].native + ' (' + job.locale + '). Translate all ordinary English; preserve ORCID, ISBN, CSL, TeX, XML, YAML, language tags and every {placeholder} exactly. Copy each key character for character. Return ONLY one valid JSON object with precisely these keys and translated string values, without Markdown or explanation. Escape all internal double quotes and backslashes using valid JSON string escaping. Source:\n' + JSON.stringify(values);
          const response = await api({model: proven.model, stream: false, messages: [{role: 'user', content: prompt}], session: 'readmd-publication-small-' + id, max_tokens: 8192});
          record.status = response.status;
          const raw = typeof response.data.content === 'string' ? response.data.content : '';
          fs.writeFileSync(path.join(requests, id + '.response.txt'), raw); record.responseSha256 = sha(raw);
          assert.equal(response.status, 200, 'Actual translation request rejected');
          const parsed = JSON.parse(raw.trim().replace(/^```(?:json)?\s*/i, '').replace(/\s*```$/, ''));
          assert.deepEqual(Object.keys(parsed).sort(), [...job.keys].sort(), 'Actual translation key coverage');
          for (const key of job.keys) {
            assert.equal(typeof parsed[key], 'string'); assert(parsed[key].trim(), 'Empty translation');
            assert.deepEqual(placeholders(parsed[key]), placeholders(spec[key].en), 'Translation placeholder mismatch');
          }
          for (const key of job.keys) spec[key][job.locale] = parsed[key];
          record.passed = true; consecutiveFailures = 0; report.completedGroups.push({id, locale: job.locale, keys: job.keys.length});
          console.log(job.locale + ': validated ' + job.keys.length + ' entries from an actual provider response');
        } catch (error) {
          record.errorName = error.name; record.error = error.message; consecutiveFailures++;
          report.failures.push({id, locale: job.locale, keys: job.keys.length, status: record.status ?? null, errorName: error.name});
          console.log(job.locale + ': failed ' + job.keys.length + '-key group (' + error.name + '); original failure retained, no automatic retry');
        }
        record.finishedAt = new Date().toISOString(); fs.writeFileSync(path.join(requests, id + '.json'), JSON.stringify(record, null, 2) + '\n'); persist();
      }
    }
    await Promise.all(Array.from({length: concurrency}, worker));
    report.missing = Object.fromEntries(Object.keys(meta).map(locale => [locale, Object.keys(spec).filter(key => !spec[key][locale])]).filter(([, keys]) => keys.length));
    report.finishedAt = new Date().toISOString(); report.complete = Object.keys(report.missing).length === 0; persist();
    assert(report.complete, 'Actual provider translation coverage is incomplete');
    fs.writeFileSync(specFile, JSON.stringify(spec, null, 2) + '\n');
    console.log('All 46 locales have actual validated translations; human linguistic review remains pending.');
  } finally {
    if (native) {
      await native.browser.close().catch(() => {});
      const child = native.server.child;
      if (child.exitCode === null) { const closed = new Promise(resolve => child.once('exit', resolve)); child.kill(); await Promise.race([closed, new Promise(resolve => setTimeout(resolve, 2000))]); }
    }
    fs.rmSync(path.join(profile, 'ai.json'), {force: true});
    try { fs.rmSync(serviceBinary, {force: true}); } catch { console.log('Owned service copy remains locked in its isolated profile.'); }
  }
})().catch(error => {report.stoppedAt = phase; report.errorName = error.name; persist(); console.error('Translation stopped at ' + phase + ' (' + error.name + '); original failures and validated entries retained.'); process.exitCode = 1;});
