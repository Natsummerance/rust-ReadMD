'use strict';
// Actual owned Windows kernel + production-CSP UI in installed headless Edge.
// The loopback SSE provider is a declared deterministic protocol fixture, not
// real-provider evidence. No API responses are mocked and no GUI is opened.
const fs = require('node:fs'), path = require('node:path'), net = require('node:net'), http = require('node:http'), zlib = require('node:zlib');
const { spawn } = require('node:child_process');
const assert = require('node:assert/strict');
const { chromium } = require('playwright');
const root = path.resolve(__dirname, '..');
const sleep = ms => new Promise(resolve => setTimeout(resolve, ms));
function deferred() { let resolve; const promise = new Promise(done => { resolve = done; }); return { promise, resolve }; }
async function freePort() { const socket = net.createServer(); await new Promise(resolve => socket.listen(0, '127.0.0.1', resolve)); const port = socket.address().port; await new Promise(resolve => socket.close(resolve)); return port; }
async function until(predicate, label, timeout = 60000) { const deadline = Date.now() + timeout; while (Date.now() < deadline) { if (await predicate()) return; await sleep(50); } throw Error('Deadline: ' + label); }
async function boundary(promise, label) { let timer; try { return await Promise.race([promise, new Promise((_, reject) => { timer = setTimeout(() => reject(Error('Deadline: ' + label)), 60000); })]); } finally { clearTimeout(timer); } }
function crc32(buffer) { let crc = -1; for (const byte of buffer) { crc ^= byte; for (let i = 0; i < 8; i++) crc = (crc >>> 1) ^ (0xedb88320 & -(crc & 1)); } return (crc ^ -1) >>> 0; }
function png(red, green, blue) {
  function chunk(name, bytes) { const type = Buffer.from(name), length = Buffer.alloc(4), crc = Buffer.alloc(4); length.writeUInt32BE(bytes.length); crc.writeUInt32BE(crc32(Buffer.concat([type, bytes]))); return Buffer.concat([length, type, bytes, crc]); }
  const header = Buffer.alloc(13); header.writeUInt32BE(1, 0); header.writeUInt32BE(1, 4); header[8] = 8; header[9] = 6;
  return Buffer.concat([Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]), chunk('IHDR', header), chunk('IDAT', zlib.deflateSync(Buffer.from([0, red, green, blue, 255]))), chunk('IEND', Buffer.alloc(0))]);
}
function zipParts(bytes) {
  let end = bytes.length - 22; while (end >= Math.max(0, bytes.length - 65557) && bytes.readUInt32LE(end) !== 0x06054b50) end--;
  assert.ok(end >= 0, 'Real ZIP end directory'); const parts = new Map(); let at = bytes.readUInt32LE(end + 16);
  for (let index = 0; index < bytes.readUInt16LE(end + 10); index++) {
    assert.equal(bytes.readUInt32LE(at), 0x02014b50); const method = bytes.readUInt16LE(at + 10), size = bytes.readUInt32LE(at + 20), nameLength = bytes.readUInt16LE(at + 28), extra = bytes.readUInt16LE(at + 30), comment = bytes.readUInt16LE(at + 32), local = bytes.readUInt32LE(at + 42);
    const name = bytes.subarray(at + 46, at + 46 + nameLength).toString('utf8'), start = local + 30 + bytes.readUInt16LE(local + 26) + bytes.readUInt16LE(local + 28), data = bytes.subarray(start, start + size);
    assert.ok(method === 0 || method === 8, 'ZIP compression supported by independent inspector'); parts.set(name, method === 8 ? zlib.inflateRawSync(data) : data); at += 46 + nameLength + extra + comment;
  }
  return parts;
}
function safeMessage(error) { return String(error?.message || error).replace(/https?:\/\/\S+/g, '[owned endpoint]').replace(/scripted-local-(?:qa|model)/g, '[controlled fixture]').slice(0, 300); }

async function modelFixture() {
  const gates = new Map(), counts = new Map(); let requests = 0, calls = 0;
  const gate = name => { const value = { entered: deferred(), release: deferred(), blocked: true }; gates.set(name, value); return value; };
  const server = http.createServer(async (request, response) => {
    if (request.method === 'GET' && request.url.endsWith('/models')) { response.writeHead(200, { 'content-type': 'application/json' }); response.end(JSON.stringify({ object: 'list', data: [{ id: 'scripted-local-model', object: 'model' }] })); return; }
    if (request.method !== 'POST' || !request.url.endsWith('/chat/completions')) { response.writeHead(404); response.end(); return; }
    try {
      let bytes = ''; for await (const chunk of request) bytes += chunk;
      const body = JSON.parse(bytes), messages = body.messages || [], decoded = messages.map(message => { try { return { ...message, parsed: JSON.parse(message.content) }; } catch { return message; } });
      assert.equal(body.stream, true);
      if (!body.tools?.length) {
        assert.ok(decoded.some(message => message.role === 'user' && typeof message.parsed?.prefix === 'string'), 'Native completion supplies bounded cursor context');
        requests++; counts.set('COMPLETION', (counts.get('COMPLETION') || 0) + 1);
        response.writeHead(200, { 'content-type': 'text/event-stream', 'cache-control': 'no-cache' });
        response.write('data: ' + JSON.stringify({ choices: [{ index: 0, delta: { role: 'assistant', content: 'form a clear ' }, finish_reason: null }] }) + '\n\n'); await sleep(25);
        if (!response.destroyed) response.end('data: ' + JSON.stringify({ choices: [{ index: 0, delta: { content: 'conclusion.' }, finish_reason: null }] }) + '\n\ndata: ' + JSON.stringify({ choices: [{ index: 0, delta: {}, finish_reason: 'stop' }] }) + '\n\ndata: [DONE]\n\n');
        return;
      }
      const initial = decoded.find(message => message.role === 'user' && message.parsed?.document_id)?.parsed;
      assert.ok(initial?.document_id, 'Native driver supplies authoritative identity');
      requests++; const mode = /QA_(NAV|PAUSE|CRASH)/.exec(initial.instruction)?.[1] || 'OTHER'; counts.set(mode, (counts.get(mode) || 0) + 1);
      const toolCalls = messages.filter(message => message.role === 'assistant').flatMap(message => message.tool_calls || []), last = toolCalls.at(-1)?.function?.name;
      const reads = decoded.filter(message => message.role === 'tool' && typeof message.parsed?.content === 'string'), snapshot = reads.at(-1)?.parsed;
      let name, args;
      if (!last) { name = 'read_document'; args = { document_id: initial.document_id }; }
      else if (last === 'read_document' && !toolCalls.some(call => call.function.name === 'propose_review')) {
        assert.ok(snapshot); const from = snapshot.content.indexOf('Original passage.'); assert.ok(from >= 0);
        name = 'propose_review'; args = { document_id: initial.document_id, generation: snapshot.generation, review_revision: snapshot.review_revision,
          edits: [{ from, to: from + 'Original passage.'.length, original: 'Original passage.', insert: 'Revised passage.' }] };
        const hold = gates.get(mode); if (hold?.blocked) { hold.entered.resolve(); await hold.release.promise; }
      } else if (last === 'propose_review') { name = 'read_document'; args = { document_id: initial.document_id }; }
      else if (last === 'read_document') { name = 'inspect_review'; args = { document_id: initial.document_id }; }
      else if (last === 'inspect_review') {
        const inspection = [...decoded].reverse().find(message => message.role === 'tool' && Array.isArray(message.parsed?.changes))?.parsed;
        assert.ok(inspection); name = 'task_complete'; args = { document_id: initial.document_id, generation: inspection.generation, review_revision: inspection.review_revision };
      } else throw Error('Unexpected controlled tool phase');
      if (response.destroyed) return;
      const id = 'controlled-call-' + (++calls), encoded = JSON.stringify(args), split = Math.floor(encoded.length / 2);
      response.writeHead(200, { 'content-type': 'text/event-stream', 'cache-control': 'no-cache' });
      const send = delta => response.write('data: ' + JSON.stringify({ choices: [delta] }) + '\n\n');
      send({ index: 0, delta: { role: 'assistant', tool_calls: [{ index: 0, id, type: 'function', function: { name, arguments: encoded.slice(0, split) } }] }, finish_reason: null });
      send({ index: 0, delta: { tool_calls: [{ index: 0, function: { arguments: encoded.slice(split) } }] }, finish_reason: null });
      send({ index: 0, delta: {}, finish_reason: 'tool_calls' }); response.end('data: [DONE]\n\n');
    } catch (error) { if (!response.destroyed) { response.writeHead(500, { 'content-type': 'application/json' }); response.end(JSON.stringify({ error: 'controlled_protocol_fixture_failure' })); } server.fixtureError = safeMessage(error); }
  });
  await new Promise(resolve => server.listen(0, '127.0.0.1', resolve));
  return { endpoint: 'http://127.0.0.1:' + server.address().port + '/v1', gate, release(name) { const hold = gates.get(name); if (hold) { hold.blocked = false; hold.release.resolve(); } }, count: () => requests, counts,
    async close() { for (const hold of gates.values()) hold.release.resolve(); server.closeAllConnections(); await new Promise(resolve => server.close(resolve)); }, error: () => server.fixtureError };
}

async function run() {
  assert.ok(process.env.READMD_BIN, 'Explicit prebuilt READMD_BIN is required');
  const base = path.resolve(process.env.READMD_BATCH2_OUTPUT || path.join(root, '.cache/capability-upgrade/v006-batch2-20261027'));
  assert.ok(base.startsWith(path.join(root, '.cache') + path.sep), 'Evidence must stay in the owned workspace cache');
  fs.mkdirSync(base, { recursive: true }); const output = fs.mkdtempSync(path.join(base, 'process-')), profile = path.join(output, 'profile'), docs = path.join(output, 'documents'), exported = path.join(docs, 'outputs');
  for (const folder of [profile, docs, exported, path.join(docs, 'images')]) fs.mkdirSync(folder);
  const controlled = await modelFixture(), port = await freePort(), origin = 'http://127.0.0.1:' + port;
  const fixtureProviderId = 'custom:00000000000000000000000000000006';
  fs.writeFileSync(path.join(profile, 'ai.json'), JSON.stringify({ schema_version: 3, providers: [{ id: fixtureProviderId, name: 'Controlled local QA', base_url: controlled.endpoint, format: 'openai', models: ['scripted-local-model'], category: 'local', mode: 'chat', endpoint_mode: 'prefix' }], current: { provider_id: fixtureProviderId, model: 'scripted-local-model' } }));
  const evidence = { passed: false, guiDriven: false, realBackend: true, productionCSP: true, browser: 'installed headless msedge', providerEvidence: 'controlled loopback native SSE fixture; not a real provider', checks: [], pageErrors: [] };
  const record = (name, details = {}) => { evidence.checks.push({ name, ...details }); console.log(name + ': PASS'); };
  const step = name => { evidence.lastStep = name; console.log('STEP ' + name); };
  const originals = new Map(), prepared = new Map(), commits = [], oldImage = png(220, 40, 40), changedImage = png(40, 40, 220);
  const aiConfigRequests = new Set(), completionTraffic = { sent: 0, responses: [], failed: 0 };
  let child, browser, context, page;
  async function stop() { if (child && child.exitCode === null) { const owned = child; owned.kill(); await Promise.race([new Promise(resolve => owned.once('exit', resolve)), sleep(5000)]); } child = null; }
  async function start() {
    child = spawn(path.resolve(process.env.READMD_BIN), ['--no-window', '--port', String(port), '--data-dir', profile, '--workspace', docs, '--assets', path.join(root, 'assets')], { cwd: root, windowsHide: true, stdio: 'ignore', env: { ...process.env, READMD_DATA_DIR: profile } });
    await until(async () => { if (child.exitCode !== null) throw Error('Owned kernel exited before readiness'); try { return (await fetch(origin)).ok; } catch { return false; } }, 'owned kernel readiness', 30000);
  }
  async function waitFor(predicate, arg, label = 'production-CSP state') { await until(() => page.evaluate(predicate, arg), label); }
  async function completionState() {
    return page.evaluate(() => {
      const view = typeof cmView === 'undefined' ? null : cmView, selection = view?.state.selection, ghost = document.querySelector('.readmd-ai-ghost');
      const popup = view && window.ReadMDCodeMirror?.completionStatus(view.state), status = document.getElementById('status-ai-completion')?.dataset.state || '';
      return { editing: Boolean(state.editing), enabled: Boolean(editorPrefs.aiCompletion), viewReady: Boolean(view), viewFocused: Boolean(view?.hasFocus), documentFocused: document.hasFocus(), hidden: document.hidden, composing: Boolean(view?.composing),
        singleCursor: selection?.ranges.length === 1 && selection.main.empty, cursorAtEnd: Boolean(view && selection.main.head === view.state.doc.length), documentCharacters: view?.state.doc.length || 0,
        popup: ['pending', 'active'].includes(popup) ? popup : null, slashOpen: typeof slash !== 'undefined' && Boolean(slash.open),
        configPresent: Boolean(state.ai.config), providerReady: Boolean(currentAiProvider()), modelConfigured: Boolean(state.ai.config?.current?.model), aiBusy: Boolean(state.ai.busy), configEvents: window.__batch2AIConfigEvents || 0,
        status: /^editor\.aiCompletion(?:Accept|Loading|Unavailable)$/.test(status) ? status : '', ghostPresent: Boolean(ghost), ghostCharacters: ghost?.textContent.length || 0, ghostStreaming: Boolean(ghost?.dataset.streaming) };
    });
  }
  async function api(route, body) { return page.evaluate(async ({ route, body }) => { const response = await apiFetch(route, { method: 'POST', headers: { 'content-type': 'application/json' }, body: JSON.stringify(body) }); return { status: response.status, data: await response.json() }; }, { route, body }); }
  async function surface(storageState) {
    context = await browser.newContext({ viewport: { width: 1160, height: 820 }, locale: 'zh-CN', bypassCSP: false, storageState }); page = await context.newPage();
    aiConfigRequests.clear();
    page.on('pageerror', error => evidence.pageErrors.push(safeMessage(error)));
    page.on('request', request => {
      if (request.method() === 'GET' && request.url().endsWith('/api/ai/config')) aiConfigRequests.add(request);
      if (request.method() === 'POST' && request.url().endsWith('/api/ai/complete')) completionTraffic.sent++;
    });
    page.on('requestfinished', request => aiConfigRequests.delete(request));
    page.on('requestfailed', request => { aiConfigRequests.delete(request); if (request.method() === 'POST' && request.url().endsWith('/api/ai/complete')) completionTraffic.failed++; });
    page.on('response', async response => {
      if (response.request().method() === 'POST' && response.url().endsWith('/api/ai/complete') && completionTraffic.responses.length < 16) completionTraffic.responses.push({ status: response.status(), streaming: (response.headers()['content-type'] || '').includes('text/event-stream') });
      if (response.request().method() !== 'POST' || !response.url().endsWith('/api/export/snapshot')) return;
      const body = response.request().postDataJSON(); if (body.action !== 'prepare' || !response.ok()) return;
      const data = await response.json().catch(() => null); if (data?.snapshot_id) prepared.set(data.document_id + '|' + body.format, data);
    });
    // The sole network interception adds the explicitly owned output path.
    // Every request still reaches the actual handler and real atomic writer.
    await page.route('**/api/export/snapshot', async route => {
      const body = route.request().postDataJSON(); if (body.action !== 'commit') { await route.continue(); return; }
      const frozen = [...prepared.values()].find(value => value.snapshot_id === body.snapshot_id); assert.ok(frozen, 'Commit uses a real prepared snapshot');
      const outPath = path.join(exported, 'reviewed-' + frozen.format + '.' + frozen.format); commits.push({ snapshot_id: body.snapshot_id, format: frozen.format, accept_pending: body.accept_pending });
      await route.continue({ postData: JSON.stringify({ ...body, out_path: outPath }) });
    });
    await page.addInitScript(() => { localStorage.setItem('readmd_language', 'zh-CN'); window.__batch2AIConfigEvents = 0; window.addEventListener('readmd:ai-config-changed', () => window.__batch2AIConfigEvents++); }); await page.goto(origin); await page.evaluate(() => window.__readmdSessionReady);
    await waitFor(() => window.__readmdAppReady && window.ReadMDReview && window.ReadMDAiHarness, null, 'frontend ready');
  }
  async function restart() { const storage = await context.storageState(); await stop(); await context.close(); await start(); await surface(storage); }
  function document(name, content) { const file = path.join(docs, name); const bytes = Buffer.concat([Buffer.from([239, 187, 191]), Buffer.from(content.replace(/\n/g, '\r\n'))]); fs.writeFileSync(file, bytes); originals.set(file, bytes); return file; }
  async function open(file, edit = true) { await page.evaluate(file => loadFile(file), file); if (edit && !await page.evaluate(() => state.editing)) await page.locator('#btn-edit').click(); if (edit) await waitFor(() => state.editing && window.cmView); await page.evaluate(() => ReadMDReview.flushAll()); }
  async function propose(find, replacement) { return page.evaluate(async ({ find, replacement }) => { const tab = getActiveTab(), stamp = await ReadMDReview.capture(tab), from = stamp.content.indexOf(find); if (from < 0) throw Error('Owned fixture phrase not found'); const before = new Set(ReadMDReview.snapshot(tab).changes.map(change => change.id)); const result = await ReadMDReview.propose([{ from, to: from + find.length, insert: replacement }], stamp, tab); return result.changes.find(change => !before.has(change.id)).id; }, { find, replacement }); }
  async function layout(name) {
    for (const size of [{ width: 1024, height: 680 }, { width: 1160, height: 820 }]) {
      await page.setViewportSize(size);
      // Measure the settled control, not an intermediate modal scale frame.
      // Keep production animations enabled and the original 60s boundary.
      await boundary(page.evaluate(async () => {
        const finite = document.getAnimations().filter(animation => Number.isFinite(animation.effect?.getComputedTiming().endTime));
        await Promise.allSettled(finite.map(animation => animation.finished));
        await new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve)));
      }), 'settled production layout animations');
      const values = await page.evaluate(() => ({ overflow: document.documentElement.scrollWidth > innerWidth + 1, controls: [...document.querySelectorAll('#choice-actions button,.ai-task-row > button,.review-action')].filter(element => element.getClientRects().length).map(element => ({ id: element.id, classes: element.className, width: element.getBoundingClientRect().width, height: element.getBoundingClientRect().height })) }));
      (evidence.layouts ||= []).push({ name, ...size, ...values });
      assert.equal(values.overflow, false); assert.ok(values.controls.length); assert.ok(values.controls.every(control => control.width >= 44 && control.height >= 44));
      await page.screenshot({ path: path.join(output, name + '-' + size.width + '.png') }); record(name + '-layout', { ...size, controls: values.controls.length });
    }
  }
  async function openAiSidePanel() {
    if (await page.locator('#ai-panel').evaluate(element => element.classList.contains('hidden'))) {
      // With split preview open the established AI button opens the inline
      // editing assistant. Select the ordinary no-preview layout first when
      // this journey specifically exercises the persistent sidebar.
      if (await page.evaluate(() => state.editing && state.pvLayout !== 'none')) {
        await page.locator('#pv-trigger').click(); await page.locator('#pv-menu [data-pv="none"]').click();
        await waitFor(() => state.pvLayout === 'none', null, 'existing single-editor layout');
      }
      await page.locator('#btn-ai').click();
    }
    await page.locator('#ai-prompt').waitFor({ state: 'visible' });
  }
  async function startTask(file, marker) {
    await open(file); await openAiSidePanel();
    await page.evaluate(() => loadAiOnDemand()); await waitFor(() => Boolean(currentAiProvider()) && !state.ai.busy, null, 'owned AI connection loaded');
    await page.locator('#ai-prompt').fill(marker + ': Replace only Original passage. with Revised passage. and verify the result.');
    await page.locator('#ai-review-edit').click();
    await waitFor(() => ReadMDAiHarness.list().some(task => task.document_id === ReadMDReview.snapshot()?.document_id && task.state === 'running'), null, 'durable UI task started');
    return page.evaluate(() => { const tab = getActiveTab(), documentId = ReadMDReview.snapshot(tab).document_id; return { tabId: tab.id, documentId, taskId: ReadMDAiHarness.list().filter(task => task.document_id === documentId).at(-1).id }; });
  }
  try {
    await start(); browser = await chromium.launch({ channel: 'msedge', headless: true }); await surface();
    // Run completion as a full regression after the snapshot/task journeys.
    const formats = (process.env.READMD_BATCH2_FORMATS || 'html,pdf,docx,epub,tex').split(',');
    for (const format of formats) {
      assert.ok(['html', 'pdf', 'docx', 'epub', 'tex'].includes(format));
      step('export-' + format);
      const image = path.join(docs, 'images', format + '.png'); fs.writeFileSync(image, oldImage);
      const file = document('Export-' + format + '.md', '# Frozen export 😀\n\nOriginal passage.\n\nC original.\n\n![Owned image](images/' + format + '.png)\n');
      await open(file); const acceptedId = await propose('Original passage.', 'A improved passage.');
      const documentId = await page.evaluate(() => ReadMDReview.snapshot().document_id);
      await page.evaluate(async format => { state.export.fmt = format; await openExportModal(); }, format);
      if (['pdf', 'docx', 'epub'].includes(format)) {
        await waitFor(() => Boolean(nativeExportPreview.data || nativeExportPreview.missingCapability), null, 'real preview or explicit missing capability');
        const missing = await page.evaluate(() => nativeExportPreview.missingCapability?.id || null);
        if (missing) {
          assert.equal(missing, 'fonts', 'Only the explicitly supplied offline font pack may be installed');
          assert.ok(process.env.READMD_FONTS_PACK, 'READMD_FONTS_PACK is required for the real Chinese default publication preset');
          const upload = path.join(profile, 'uploads', 'Owned-fonts.readmd-pack'); fs.mkdirSync(path.dirname(upload), { recursive: true });
          // Share the read-only archive bytes; the actual installer validates
          // its locked manifest and creates its own managed installation.
          fs.linkSync(path.resolve(process.env.READMD_FONTS_PACK), upload);
          const installed = await api('/api/capabilities/packs', { action: 'install', id: 'fonts', path: upload });
          assert.equal(installed.status, 200); assert.equal(installed.data.ok, true);
          await page.locator('#export-preview-capability button').click();
          record('real-offline-font-install-and-existing-preview-resume', { copiedPrivateProfile: false, downloadedDependencies: false });
        }
        await waitFor(() => Boolean(nativeExportPreview.data), null, 'real frozen native preview'); record('real-snapshot-preview-' + format);
      }
      await page.locator('#export-run').click(); await page.locator('#choice-modal').waitFor({ state: 'visible' });
      await until(() => prepared.has(documentId + '|' + format), 'real snapshot preparation'); const frozen = prepared.get(documentId + '|' + format); assert.deepEqual(frozen.pending_ids, [acceptedId]);
      const newId = await propose('C original.', 'C newer pending.'); fs.writeFileSync(image, changedImage);
      if (format === 'html') await layout('snapshot-consent');
      const consent = await page.evaluate(() => i18n.t('export.acceptAllAndExport')); await page.locator('#choice-actions').getByRole('button', { name: consent, exact: true }).click();
      await waitFor(() => document.getElementById('export-result').classList.contains('ok') && !ReadMDTask.isRunning('export'), null, 'verified real export');
      const mark = await page.evaluate(() => ReadMDReview.snapshot()); assert.equal(mark.changes.find(change => change.id === acceptedId).decision, 'accepted'); assert.equal(mark.changes.find(change => change.id === newId).decision, 'pending');
      const outPath = path.join(exported, 'reviewed-' + format + '.' + format), bytes = fs.readFileSync(outPath); assert.ok(bytes.length > 0); assert.deepEqual(fs.readFileSync(file), originals.get(file));
      if (format === 'html') { const html = bytes.toString('utf8'); assert.ok(html.includes('A improved passage.')); assert.ok(html.includes('C original.')); assert.ok(!html.includes('C newer pending.')); assert.ok(html.includes(oldImage.toString('base64'))); assert.ok(!html.includes(changedImage.toString('base64'))); }
      if (format === 'pdf') assert.equal(bytes.subarray(0, 5).toString(), '%PDF-');
      if (format === 'docx') assert.ok(zipParts(bytes).get('word/document.xml').toString().includes('A improved passage.'));
      if (format === 'epub') assert.equal(zipParts(bytes).get('mimetype').toString(), 'application/epub+zip');
      if (format === 'tex') assert.ok(bytes.toString('utf8').includes('\\documentclass'));
      assert.equal(commits.filter(commit => commit.snapshot_id === frozen.snapshot_id).length, 1); assert.equal(commits.at(-1).accept_pending, true);
      record('real-accepted-export-' + format, { bytes: bytes.length, laterChangePending: true, sourceUnchanged: true }); await page.evaluate(() => closeExportModal());
    }

    const navFile = document('Task-navigation.md', '# Task navigation\n\nOriginal passage.\n'), pauseFile = document('Task-pause.md', '# Task pause\n\nOriginal passage.\n'), crashFile = document('Task-crash.md', '# Task restart\n\nOriginal passage.\n'), otherFile = document('Independent.md', '# Independent\n\nOther source stays untouched.\n');
    step('task-navigation-and-panel-close');
    await open(otherFile, false); const otherTab = await page.evaluate(() => getActiveTab().id);
    const navGate = controlled.gate('NAV'), navigation = await startTask(navFile, 'QA_NAV'); await boundary(navGate.entered.promise, 'navigation proposal barrier');
    await page.locator('#ai-close').click(); await page.locator('.tab-item[data-tab-id="' + otherTab + '"]').first().click();
    controlled.release('NAV');
    await until(async () => (await api('/api/ai/tasks', { action: 'status', task_id: navigation.taskId })).data.task.state === 'completed', 'background continuing native tool rounds');
    assert.equal(await page.evaluate(() => getActiveTab().path), otherFile); assert.deepEqual(fs.readFileSync(otherFile), originals.get(otherFile));
    const navState = await api('/api/review', { action: 'read', document_id: navigation.documentId }); assert.ok(navState.data.content.includes('Revised passage.')); assert.equal(navState.data.changes.length, 1);
    assert.equal(controlled.counts.get('NAV'), 5); record('real-task-five-native-rounds-after-panel-close-and-navigation', { modelCalls: controlled.counts.get('NAV'), pendingChanges: 1 });

    step('task-pause-late-proposal');
    const pauseGate = controlled.gate('PAUSE'), paused = await startTask(pauseFile, 'QA_PAUSE'); await boundary(pauseGate.entered.promise, 'pause proposal barrier');
    const cancel = await page.evaluate(() => i18n.t('ai.taskCancel')); await page.locator('#ai-task-status').getByRole('button', { name: cancel, exact: true }).click();
    await waitFor(id => ReadMDAiHarness.list().find(task => task.id === id)?.state === 'paused', paused.taskId, 'real resumable cancellation');
    await layout('task-paused'); controlled.release('PAUSE'); await sleep(300);
    const pauseState = await api('/api/review', { action: 'read', document_id: paused.documentId }); assert.equal(pauseState.data.changes.length, 0); assert.ok(pauseState.data.content.includes('Original passage.'));
    record('real-pause-wins-over-late-proposal', { pendingChanges: 0 });

    step('task-crash-restart');
    const crashGate = controlled.gate('CRASH'), crashed = await startTask(crashFile, 'QA_CRASH'); await boundary(crashGate.entered.promise, 'crash proposal barrier');
    await page.evaluate(() => ReadMDSession.flush()); const beforeRestartCalls = controlled.count(); await restart();
    const recovered = await api('/api/ai/tasks', { action: 'status', task_id: crashed.taskId }); assert.equal(recovered.data.task.state, 'needs_user_resume'); await sleep(500); assert.equal(controlled.count(), beforeRestartCalls);
    record('real-crash-restart-needs-user-resume-without-model-calls');
    step('task-explicit-resume');
    controlled.release('CRASH');
    if (await page.evaluate(() => getActiveTab()?.path) !== crashFile) await open(crashFile);
    await openAiSidePanel();
    await waitFor(id => ReadMDAiHarness.list().some(task => task.id === id && task.state === 'needs_user_resume'), crashed.taskId, 'restored task status strip');
    const resume = await page.evaluate(() => i18n.t('ai.taskResume')); await page.locator('#ai-task-status').getByRole('button', { name: resume, exact: true }).click();
    await until(async () => (await api('/api/ai/tasks', { action: 'status', task_id: crashed.taskId })).data.task.state === 'completed', 'explicit real UI task resume');
    const afterResume = await api('/api/review', { action: 'read', document_id: crashed.documentId }); assert.equal(afterResume.data.changes.length, 1); assert.ok(afterResume.data.content.includes('Revised passage.'));
    assert.ok(controlled.count() > beforeRestartCalls); record('real-ui-resume-completes-without-duplicate-proposal', { pendingChanges: 1 });
    step('completion-open');
    const completionFile = document('Completion.md', '# Owned completion\n\nWe compare controlled evidence to '), completionText = 'form a clear conclusion.';
    await open(completionFile); await page.evaluate(() => loadAiOnDemand());
    // Loading can fail softly, and module polling can have another config read
    // in flight. Its config-changed event resets inline completion, so settle
    // those reads before enabling the editor and triggering the real shortcut.
    await until(async () => !aiConfigRequests.size && await page.evaluate(() => Boolean(currentAiProvider()) && Boolean(state.ai.config?.current?.model) && !state.ai.busy), 'owned AI connection settled');
    await page.evaluate(async () => { await new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve))); setEditorPref('aiCompletion', true); cmView.dispatch({ selection: { anchor: cmView.state.doc.length } }); cmView.focus(); });
    evidence.completionReady = await completionState();
    const completionBefore = await page.evaluate(() => getEditContent());
    const ghost = async () => {
      await page.locator('.cm-content').focus();
      await waitFor(() => cmView.hasFocus && !cmView.composing && !document.hidden && state.editing && editorPrefs.aiCompletion && cmView.state.selection.ranges.length === 1 && cmView.state.selection.main.empty && !ReadMDCodeMirror.completionStatus(cmView.state) && (typeof slash === 'undefined' || !slash.open), null, 'completion editor ready');
      await page.keyboard.press('Control+Alt+Space'); await waitFor(text => { const node = document.querySelector('.readmd-ai-ghost'); return node?.textContent === text && !node.dataset.streaming; }, completionText, 'real streamed completion ghost');
    };
    const completionResponse = page.waitForResponse(response => response.url().endsWith('/api/ai/complete') && response.request().method() === 'POST');
    step('completion-stream-ghost');
    await ghost(); assert.equal((await completionResponse).status(), 200); assert.equal(await page.evaluate(() => getEditContent()), completionBefore);
    step('completion-escape');
    await page.keyboard.press('Escape'); assert.equal(await page.locator('.readmd-ai-ghost').count(), 0); assert.equal(await page.evaluate(() => getEditContent()), completionBefore);
    step('completion-tab-accept');
    await ghost(); await page.keyboard.press('Tab'); assert.equal(await page.evaluate(() => getEditContent()), completionBefore + completionText);
    step('completion-undo-accepted');
    await page.keyboard.press('Control+z'); await waitFor(expected => getEditContent() === expected, completionBefore, 'review undo of accepted completion'); assert.equal(await page.evaluate(() => getEditContent()), completionBefore);
    step('completion-typed-input-clears-ghost');
    await ghost();
    const typed = await page.evaluate(() => { const end = cmView.state.doc.length; cmView.dispatch({ changes: { from: end, insert: 'hand input' }, selection: { anchor: end + 10 }, userEvent: 'input.type' }); const cleared = !document.querySelector('.readmd-ai-ghost'); setEditorPref('aiCompletion', false); return { cleared, content: getEditContent() }; });
    assert.equal(typed.cleared, true); assert.equal(typed.content, completionBefore + 'hand input');
    step('completion-undo-typed-input');
    await page.keyboard.press('Control+z'); await waitFor(expected => getEditContent() === expected, completionBefore, 'review undo of typed input'); assert.equal(await page.evaluate(() => getEditContent()), completionBefore);
    step('completion-composition-clears-ghost');
    await page.evaluate(() => { setEditorPref('aiCompletion', true); cmView.focus(); }); await ghost();
    await page.locator('.cm-content').dispatchEvent('compositionstart'); assert.equal(await page.locator('.readmd-ai-ghost').count(), 0); assert.equal(await page.evaluate(() => getEditContent()), completionBefore);
    await page.evaluate(() => setEditorPref('aiCompletion', false)); await page.locator('.cm-content').dispatchEvent('compositionend'); await page.evaluate(() => ReadMDReview.flushAll());
    assert.deepEqual(fs.readFileSync(completionFile), originals.get(completionFile));
    record('real-CSP-controlled-stream-completion-escape-tab-undo-input-and-ime', { providerCalls: controlled.counts.get('COMPLETION'), characters: completionText.length, sourceUnchanged: true });
    for (const [file, bytes] of originals) assert.deepEqual(fs.readFileSync(file), bytes);
    assert.equal(controlled.error(), undefined); assert.deepEqual(evidence.pageErrors, []); record('all-owned-originals-unchanged-and-no-production-CSP-page-errors'); evidence.passed = true;
  } catch (error) {
    evidence.failure = safeMessage(error);
    evidence.completionDiagnostic = { ...(page ? await completionState().catch(() => ({ available: false })) : { available: false }), pendingConfigReads: aiConfigRequests.size, traffic: completionTraffic, controlledProviderCalls: controlled.count(), controlledCompletionCalls: controlled.counts.get('COMPLETION') || 0, controlledProtocolFailure: Boolean(controlled.error()) };
    process.exitCode = 1; console.error(evidence.failure); console.log('completion diagnostic: ' + JSON.stringify(evidence.completionDiagnostic));
  }
  finally {
    if (page) await page.screenshot({ path: path.join(output, 'final.png') }).catch(() => {});
    if (browser) await browser.close().catch(() => {}); await stop(); await controlled.close();
    fs.writeFileSync(path.join(output, 'report.json'), JSON.stringify(evidence, null, 2)); console.log('batch2 process report: ' + path.relative(root, path.join(output, 'report.json')));
  }
}
if (require.main === module) run().catch(error => { console.error(safeMessage(error)); process.exitCode = 1; });
module.exports = { png, zipParts };
