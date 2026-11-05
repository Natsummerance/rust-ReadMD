'use strict';
// Real authenticated HTTP, fresh owned files and three actual backend PIDs.
// No GUI, model, installer, test API, response substitute or build is used.
const fs = require('node:fs'), path = require('node:path'), net = require('node:net');
const crypto = require('node:crypto'), assert = require('node:assert/strict');
const { spawn, execFileSync } = require('node:child_process');
const sha = value => crypto.createHash('sha256').update(value).digest('hex');
const pause = ms => new Promise(resolve => setTimeout(resolve, ms));
const LF = '前😀\nA orig C\nA orig C\n\n尾部 保持。\n[资源](./resources/owned.txt)\n';
const PENDING = '前😀\nA orig C\nA next😀 C\n\n尾部 保持。\n[资源](./resources/owned.txt)\n';
const MANUAL = '前😀\nA orig C\n【手动】A next😀 C【结束】\n\n尾部 保持。\n[资源](./resources/owned.txt)\n';
const ADJ = '😀ab!\n重复段落。\n重复段落。\n[资源](./resources/owned.txt)\n';
const physical = text => Buffer.from('\ufeff' + text.replaceAll('\n', '\r\n'), 'utf8');
const fixtures = () => [
  { id: 'enclosing', name: '包围审阅.md', initial: LF, pending: PENDING, manual: MANUAL,
    proposal: [{ from: 15, to: 19, insert: 'next😀' }],
    enclosing: [{ from: 13, to: 24, insert: '【手动】A next😀 C【结束】\n' }] },
  { id: 'adjacent-delete', name: '相邻删除.md', initial: ADJ, after: '😀!\n重复段落。\n重复段落。\n[资源](./resources/owned.txt)\n', replacement: '' },
  { id: 'adjacent-replace', name: '相邻替换.md', initial: ADJ, after: '😀β😀!\n重复段落。\n重复段落。\n[资源](./resources/owned.txt)\n', replacement: 'β😀' },
];
// Version/event counters remain monotonic; every stable stored review field is compared.
const FIELDS = ['id', 'from', 'to', 'original', 'replacement', 'decision', 'application', 'taken_over', 'origin', 'task_id', 'restore_original', 'candidate_versions', 'dependencies'];
const semantic = change => Object.fromEntries(FIELDS.map(key => [key, change[key]]));
function expectations() {
  assert.equal(LF.slice(15, 19), 'orig'); assert.equal(PENDING.slice(15, 21), 'next😀');
  assert.equal(PENDING.slice(13, 24), 'A next😀 C\n'); assert.equal(MANUAL.slice(13, 32), '【手动】A next😀 C【结束】\n');
  return { schema: 1, declaredBeforeExecution: true, syntheticAiProposalOnly: true,
    fixtures: fixtures().map(f => ({ ...f, initialPhysicalSha256: sha(physical(f.initial)),
      states: Object.fromEntries(['initial', 'pending', 'manual', 'after'].filter(k => f[k] !== undefined).map(k => [k, { logicalSha256: sha(f[k]), utf16Length: f[k].length, physicalSha256: sha(physical(f[k])) }])) })),
    pending: { from: 15, to: 21, original: 'orig', replacement: 'next😀', decision: 'pending', application: 'applied', taken_over: false },
    manual: { from: 13, to: 32, original: 'orig', replacement: 'next😀', decision: 'conflict', application: 'applied', taken_over: true },
    withdrawn: { from: 15, to: 19, original: 'orig', replacement: 'next😀', decision: 'withdrawn', application: 'proposed', taken_over: false },
    resourceSha256: sha(Buffer.from('OWNED_RESOURCE 中文😀\r\n')), untouchedSha256: sha(physical('不相关原件😀\n')),
    required: ['three distinct owned PIDs and three observed exit+close pairs', 'actual stdout listen port, root token, unchanged production CSP/index/boot and anonymous denial',
      'exact review semantics and UTF16 anchors in every phase', 'explicit BOM/CRLF save does not accept pending AI', 'durable service readback after new PIDs',
      'adjacent deletion/replacement inverse undo and redo across restart', 'physical source/resource/sentinel byte assertions', 'readOnly SQLite after actual exit+close when requested'],
    deadlinesMs: { readiness: 30000, request: 10000, stop: 10000, body: 180000 },
    notCertified: ['Mac native UI', 'physical keyboard/IME', 'normal Windows App quit', 'model/provider correctness', 'compiled producer source equivalence without root build receipt'] };
}
function options(argv) {
  if (argv.length === 1 && argv[0] === '--plan') return { plan: true };
  const values = {}, names = new Set(['--repo', '--bin', '--assets', '--evidence', '--evidence-root', '--sqlite']);
  for (let i = 0; i < argv.length; i += 2) { assert(names.has(argv[i]) && argv[i + 1] && !values[argv[i]], 'Explicit unique driver options required'); values[argv[i]] = argv[i + 1]; }
  for (const key of ['--repo', '--bin', '--evidence']) assert(values[key], 'Missing ' + key);
  const repo = path.resolve(values['--repo']);
  const result = { repo, bin: path.resolve(values['--bin']), assets: path.resolve(values['--assets'] || path.join(repo, 'assets')),
    evidence: path.resolve(values['--evidence']), evidenceRoot: path.resolve(values['--evidence-root'] || repo), sqlite: values['--sqlite'] || 'required' };
  assert(['required', 'service-only'].includes(result.sqlite), 'SQLite mode must be explicit required or service-only'); return result;
}
const identity = stat => Object.fromEntries(['dev', 'ino', 'size', 'mtimeNs', 'ctimeNs'].map(k => [k, String(stat[k])]));
function ordinary(file, max = Infinity) { const stat = fs.lstatSync(file, { bigint: true }); assert(stat.isFile() && !stat.isSymbolicLink(), 'Ordinary file required'); assert(Number(stat.size) <= max, 'Bounded input required'); return identity(stat); }
async function fileHash(file) { const hash = crypto.createHash('sha256'); for await (const chunk of fs.createReadStream(file)) hash.update(chunk); return hash.digest('hex'); }
async function boundedBody(response, max) { let size = 0; const chunks = []; for await (const chunk of response.body) { size += chunk.length; assert(size <= max, 'Bounded HTTP response'); chunks.push(Buffer.from(chunk)); } return Buffer.concat(chunks); }
async function timeout(promise, ms, label) { let timer; try { return await Promise.race([promise, new Promise((_, reject) => { timer = setTimeout(() => reject(Error(label)), ms); })]); } finally { clearTimeout(timer); } }
async function freePort() { const server = net.createServer(); await new Promise((resolve, reject) => { server.once('error', reject); server.listen(0, '127.0.0.1', resolve); }); const port = server.address().port; await new Promise((resolve, reject) => server.close(error => error ? reject(error) : resolve())); return port; }

async function main(argv = process.argv.slice(2)) {
  const args = options(argv); if (args.plan) { console.log(JSON.stringify({ executed: false, ...expectations() }, null, 2)); return; }
  const root = fs.realpathSync.native(args.evidenceRoot), rel = path.relative(root, args.evidence);
  assert(rel && rel !== '..' && !rel.startsWith('..' + path.sep) && !path.isAbsolute(rel), 'Fresh evidence must stay in explicit owned root');
  assert.equal(fs.realpathSync.native(path.dirname(args.evidence)), path.dirname(args.evidence), 'Canonical evidence parent required');
  fs.mkdirSync(args.evidence, { mode: 0o700 });
  const dirs = Object.fromEntries(['profile', 'documents', 'tmp', 'home'].map(name => [name, path.join(args.evidence, name)]));
  for (const directory of Object.values(dirs)) fs.mkdirSync(directory, { mode: 0o700 });
  fs.mkdirSync(path.join(dirs.documents, 'resources'), { mode: 0o700 });
  const tokens = [], redactions = [[args.evidence, '<owned>'], [args.repo, '<repo>'], [args.assets, '<assets>'], [args.bin, '<binary>'], [path.dirname(__filename), '<driver-dir>']].sort((a, b) => b[0].length - a[0].length);
  function safe(value) { if (typeof value === 'string') { for (const [from, to] of redactions) value = value.replaceAll(from, to); for (const token of tokens) value = value.replaceAll(token, '<token>'); return value; } if (Array.isArray(value)) return value.map(safe); if (value && typeof value === 'object') return Object.fromEntries(Object.entries(value).map(([key, item]) => [key, /^(?:token|app_token|authorization|secret|credential)$/i.test(key) ? '<redacted>' : safe(item)])); return value; }
  const write = (name, value) => fs.writeFileSync(path.join(args.evidence, name), JSON.stringify(safe(value), null, 2) + '\n', { flag: 'wx', mode: 0o600 });
  const spec = expectations(), rows = fixtures().map(f => ({ ...f, file: path.join(dirs.documents, f.name) }));
  const expectedDisk = new Map(rows.map(f => [f.id, physical(f.initial)])), heads = new Map(), ids = new Map();
  for (const f of rows) fs.writeFileSync(f.file, physical(f.initial), { flag: 'wx', mode: 0o600 });
  const resource = path.join(dirs.documents, 'resources', 'owned.txt'), untouched = path.join(dirs.documents, '不相关.md');
  fs.writeFileSync(resource, Buffer.from('OWNED_RESOURCE 中文😀\r\n'), { flag: 'wx', mode: 0o600 }); fs.writeFileSync(untouched, physical('不相关原件😀\n'), { flag: 'wx', mode: 0o600 });
  write('independent-expectations.json', spec);
  const report = { schema: 1, passed: false, platform: process.platform, arch: process.arch, nodeVersion: process.version, sqliteMode: args.sqlite,
    productionAuth: false, productionCsp: false, httpServiceOnly: true, appUiReadyCertified: false, nativeUiCertified: false, modelsRequested: false, syntheticAiProposalOnly: true,
    driverSha256: sha(fs.readFileSync(__filename)), inputSha256: sha(Buffer.from(JSON.stringify(spec))), expectationsFileSha256: sha(fs.readFileSync(path.join(args.evidence, 'independent-expectations.json'))), checks: [], snapshots: [], processes: [], databaseInspections: [], cleanup: {}, phase: 'preflight' };
  const processes = [], scratch = ['tmp', 'home'].map(key => ({ path: dirs[key], stat: fs.lstatSync(dirs[key], { bigint: true }) }));
  const start = performance.now(), deadline = start + spec.deadlinesMs.body;
  let host, DatabaseSync, bindings, binaryIdentity, boot, index;
  const remaining = () => { const left = deadline - performance.now(); assert(left > 0, 'Overall HTTP regression deadline'); return Math.max(1, Math.min(spec.deadlinesMs.request, left)); };
  function fail(error) { if (!report.failure) { report.failure = { phase: report.phase, name: error.name, message: String(error.message).slice(0, 1400), stack: String(error.stack || '').split('\n').slice(0, 8).join('\n') }; write('first-failure.json', report.failure); } }
  const check = name => report.checks.push({ name, passed: true });
  function disks() { for (const f of rows) assert.deepEqual(fs.readFileSync(f.file), expectedDisk.get(f.id), f.id + ' physical bytes'); assert.equal(sha(fs.readFileSync(resource)), spec.resourceSha256); assert.equal(sha(fs.readFileSync(untouched)), spec.untouchedSha256); }
  function validate(f, s, body, state) {
    assert.equal(s.document_id, ids.get(f.id) || s.document_id); assert.equal(typeof s.document_id, 'string'); assert(s.document_id.length);
    ids.set(f.id, s.document_id); assert.equal(s.content, body); assert.equal(s.revision, sha(body)); assert.equal(s.disk_revision, sha(expectedDisk.get(f.id)));
    assert.equal(s.encoding, 'utf-8-sig'); assert.equal(s.line_ending, 'crlf'); assert.equal(s.status, 'ready'); assert(!s.recovery && !s.save_intent);
    assert(Number.isSafeInteger(s.generation) && Number.isSafeInteger(s.review_revision));
    if (f.id === 'enclosing' && state !== 'initial') { assert.equal(s.changes.length, 1); const actual = s.changes[0]; const expected = spec[state];
      assert.equal(actual.id, ids.get('pending')); for (const [key, value] of Object.entries(expected)) assert.deepEqual(actual[key], value, state + '/' + key);
      assert.equal(actual.origin, 'ai'); assert.equal(actual.task_id, 'synthetic-portable-undo');
      assert.deepEqual(actual.candidate_versions, ['next😀']); assert.deepEqual(actual.dependencies, []); assert.equal(actual.restore_original, undefined);
      assert(Number.isSafeInteger(actual.version) && actual.version > 0); assert(Number.isSafeInteger(actual.last_event_id) && actual.last_event_id > 0);
    } else assert.equal(s.changes.length, 0);
    heads.set(f.id, s); disks(); return s;
  }
  function snapshot(f, phase, s) { report.snapshots.push({ fixture: f.id, phase, content: s.content, document_id: s.document_id, generation: s.generation,
    review_revision: s.review_revision, revision: s.revision, disk_revision: s.disk_revision, saved_revision: s.saved_revision,
    can_undo: s.can_undo, can_redo: s.can_redo, changes: s.changes.map(semantic) }); }
  async function api(route, body) { const response = await host.api(route, body); report.lastHttp = { route, status: response.status, error_code: response.body?.error_code }; assert.equal(response.status, 200, JSON.stringify(report.lastHttp)); return response.body; }
  const cas = s => ({ document_id: s.document_id, generation: s.generation, review_revision: s.review_revision, call_id: crypto.randomUUID() });
  async function mutate(f, action, extra, body, state) { const previous = heads.get(f.id); const s = await api('/api/review', { action, ...cas(previous), ...extra });
    assert.equal(s.document_id, previous.document_id); assert(s.generation >= previous.generation); assert(s.review_revision > previous.review_revision);
    validate(f, s, body, state); snapshot(f, action, s); return s; }
  async function fileRead(f) { const data = await api('/api/file?p=' + encodeURIComponent(f.file)); const decoded = expectedDisk.get(f.id).subarray(3).toString('utf8');
    const original = data.original ?? data.content; assert.equal(original, decoded, 'file.original is source; content may be a reading projection'); assert.equal(data.encoding, 'utf-8-sig'); assert.equal(data.revision, sha(decoded));
    report.snapshots.push({ fixture: f.id, phase: 'file-source-vs-projection', sourceSha256: sha(original), projectionSha256: sha(data.content), originalFieldPresent: data.original !== null && data.original !== undefined }); return data; }
  async function open(f, resume = false, body = f.initial, state = 'initial') { await fileRead(f); const value = await api('/api/review', { action: resume ? 'resume' : 'open', path: f.file });
    if (resume) assert.equal(value.found, true); const s = resume ? value.snapshot : value; validate(f, s, body, state); snapshot(f, resume ? 'resume' : 'open', s); return s; }
  async function save(f, body, state) { const previous = heads.get(f.id), file = await fileRead(f); const saved = await api('/api/save', { path: f.file, content: body.replaceAll('\n', '\r\n'), encoding: 'utf-8-sig', expected_revision: file.revision, review_session: cas(previous) });
    assert.equal(saved.ok, true); assert(saved.review); expectedDisk.set(f.id, physical(body)); validate(f, saved.review, body, state);
    assert.equal(saved.review.generation, previous.generation); snapshot(f, 'explicit-save', saved.review); check(f.id + '/explicit-save-retains-review-and-BOM-CRLF'); return saved.review; }

  async function launch(epoch) {
    report.phase = 'start PID ' + epoch; const requestedPort = await freePort();
    const env = Object.fromEntries(Object.entries(process.env).filter(([key]) => /^(PATH|SYSTEMROOT|WINDIR|COMSPEC|PATHEXT|LANG|LC_[A-Z_]+|LD_LIBRARY_PATH|DYLD_LIBRARY_PATH)$/i.test(key)));
    Object.assign(env, { READMD_DATA_DIR: dirs.profile, READMD_WORKSPACE: dirs.documents, READMD_EXTRA_ROOTS: '', READMD_REQUIRE_TOKEN: '1',
      CFFIXED_USER_HOME: dirs.home, XDG_CONFIG_HOME: dirs.home, XDG_DATA_HOME: dirs.home, XDG_CACHE_HOME: dirs.tmp,
      TMPDIR: dirs.tmp, TMP: dirs.tmp, TEMP: dirs.tmp, SQLITE_TMPDIR: dirs.tmp });
    const child = spawn(args.bin, ['--no-window', '--host', '127.0.0.1', '--port', String(requestedPort), '--require-token', '--data-dir', dirs.profile, '--workspace', dirs.documents, '--assets', args.assets], { cwd: args.repo, env, windowsHide: true, stdio: ['ignore', 'pipe', 'pipe'] });
    const record = { epoch, pid: child.pid || null, requestedPort, exitObserved: false, closeObserved: false, errors: [], stopRequested: false, stdout: '', stderr: '' };
    report.processes.push(record); let actualPort, kernelVersion, token, resolveClose;
    const close = new Promise(resolve => { resolveClose = resolve; });
    child.on('error', error => record.errors.push({ name: error.name, code: error.code || null }));
    child.once('exit', (code, signal) => { record.exitObserved = true; record.exit = { code, signal }; });
    child.once('close', (code, signal) => { record.closeObserved = true; record.close = { code, signal }; resolveClose(); });
    child.stdout.on('data', bytes => { record.stdout += bytes.toString('utf8'); if (record.stdout.length > 65536) { record.stdout = record.stdout.slice(0, 65536); record.logOverflow = true; }
      const match = record.stdout.match(/^ReadMD rust kernel ([^\r\n ]+) listening on http:\/\/127\.0\.0\.1:(\d+)\r?$/m); if (match) { kernelVersion = match[1]; actualPort = Number(match[2]); } });
    child.stderr.on('data', bytes => { record.stderr += bytes.toString('utf8'); if (record.stderr.length > 65536) { record.stderr = record.stderr.slice(0, 65536); record.logOverflow = true; } });
    const owned = { record, child, close, async stop() {
      if (!record.stopRequested) { record.stopRequested = true; record.stopRequestedAtMs = performance.now() - start;
        assert(!record.exitObserved && !record.closeObserved, 'Backend exited before controlled stop');
        try { record.killAccepted = child.kill('SIGTERM'); } catch (error) { record.killError = { name: error.name, code: error.code || null }; throw error; }
        assert.equal(record.killAccepted, true, 'Kill return is not a termination receipt');
      }
      const stopDeadline = start + record.stopRequestedAtMs + spec.deadlinesMs.stop;
      assert(performance.now() < stopDeadline, 'Owned stop absolute deadline');
      await timeout(close, Math.max(1, stopDeadline - performance.now()), 'Owned backend exit+close deadline');
      assert(performance.now() < stopDeadline, 'Owned stop absolute deadline after exit+close');
      assert(record.pid && record.exitObserved && record.closeObserved, 'Both real exit and close events required'); assert.deepEqual(record.close, record.exit); assert.equal(record.errors.length, 0); assert(!record.killError);
      if (process.platform !== 'win32') assert.deepEqual(record.exit, { code: 0, signal: null });
      record.termination = process.platform === 'win32' ? 'controlled Windows SIGTERM; not native normal quit' : 'headless SIGTERM handler exited normally';
      record.normalNativeAppQuitCertified = false; record.terminalConfirmed = true;
    } };
    processes.push(owned);
    const until = Math.min(deadline, performance.now() + spec.deadlinesMs.readiness);
    while (performance.now() < until) {
      assert(!record.errors.length && !record.exitObserved && !record.closeObserved && !record.logOverflow, 'Owned backend failed before HTTP readiness');
      if (actualPort) {
        assert(Number.isInteger(actualPort) && actualPort > 0 && actualPort < 65536); record.actualPort = actualPort;
        try {
          const response = await fetch('http://127.0.0.1:' + actualPort + '/', { signal: AbortSignal.timeout(Math.min(1500, remaining())) });
          if (response.ok) {
            const html = (await boundedBody(response, 2 * 1024 * 1024)).toString('utf8');
            const value = html.match(/<meta name="readmd-app-token" content="([^"]+)">/)?.[1];
            if (value) { token = value; tokens.push(token); assert(token.length <= 256);
              const csp = html.match(/<meta http-equiv="Content-Security-Policy" content="([^"]+)">/)?.[1] || '';
              assert(csp.includes("script-src 'self'")); assert(!/script-src[^;]*'unsafe-(inline|eval)'/.test(csp));
              const injection = '<head><script>window.READMD_ENGINE="rust";window.READMD_KERNEL_VERSION="' + kernelVersion + '";window.APP_TOKEN="' + token + '";</script>';
              const normalized = html.replace(injection, '<head>').replaceAll('window.LAN_TOKEN="' + token + '";', 'window.LAN_TOKEN=null;').replaceAll('<meta name="readmd-app-token" content="' + token + '">', '<meta name="readmd-app-token" content="">');
              assert.equal(sha(normalized), sha(index), 'Actual root must bind complete frozen index except known production token injection');
              const bootResponse = await fetch('http://127.0.0.1:' + actualPort + '/assets/readmd.boot.js', { signal: AbortSignal.timeout(remaining()) }); assert.equal(bootResponse.status, 200);
              assert.equal(sha(await boundedBody(bootResponse, 16 * 1024 * 1024)), sha(boot));
              record.root = { servedHtmlSha256: sha(html), normalizedIndexSha256: sha(normalized), bootSha256: sha(boot), cspSha256: sha(csp), tokenPresentInMemoryOnly: true, appUiReadyClaimed: false };
              break;
            }
          }
        } catch (error) { if (error.name !== 'TypeError' && error.name !== 'TimeoutError') throw error; }
      }
      await pause(40);
    }
    assert(performance.now() < until && token, 'Actual root/token/CSP/boot readiness deadline');
    const base = 'http://127.0.0.1:' + actualPort;
    owned.api = async (route, body) => { assert(!record.exitObserved && !record.closeObserved, 'Owned backend remains alive'); const response = await fetch(base + route, { method: body === undefined ? 'GET' : 'POST',
      headers: { 'X-ReadMD-App-Token': token, 'X-ReadMD-Token': token, 'Content-Type': 'application/json' }, ...(body === undefined ? {} : { body: JSON.stringify(body) }), signal: AbortSignal.timeout(remaining()) });
      const bytes = await boundedBody(response, 1024 * 1024); remaining(); assert(!record.exitObserved && !record.closeObserved && !record.errors.length, 'Owned backend remains alive after response'); return { status: response.status, body: JSON.parse(bytes.toString('utf8')) }; };
    const denied = await fetch(base + '/api/review', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ action: 'read', document_id: 'synthetic-unauthenticated-negative' }), signal: AbortSignal.timeout(remaining()) });
    assert([401, 403].includes(denied.status)); await boundedBody(denied, 8192); remaining();
    record.anonymousReviewDenied = true; check('PID-' + epoch + '/actual-listen-root-CSP-boot-auth'); return owned;
  }

  function database(epoch) {
    assert(processes.every(p => p.record.terminalConfirmed && p.record.exitObserved && p.record.closeObserved), 'SQLite is read only after all owned backends actually terminate');
    if (args.sqlite === 'service-only') { report.databaseInspections.push({ epoch, independentReaderUsed: false, serviceDurableReadbackOnly: true }); return; }
    const file = path.join(dirs.profile, 'review.sqlite3'); ordinary(file, 64 * 1024 * 1024); const before = sha(fs.readFileSync(file)), db = new DatabaseSync(file, { readOnly: true });
    const result = { epoch, independentReaderUsed: true, readOnly: true, afterAllExitAndClose: true, documents: [] };
    try {
      const integrity = db.prepare('PRAGMA quick_check').all(); assert.equal(integrity.length, 1); assert.equal(Object.values(integrity[0])[0], 'ok');
      result.schemaVersion = db.prepare('PRAGMA user_version').get().user_version; assert.equal(result.schemaVersion, report.expectedSchemaVersion);
      for (const f of rows) {
        const row = db.prepare('SELECT payload FROM review_documents WHERE id=?').get(ids.get(f.id)); assert(row && Buffer.byteLength(row.payload) < 1024 * 1024); let references = 0;
        function hydrate(value) { if (Array.isArray(value)) return value.map(hydrate); if (value && typeof value === 'object') { if (typeof value.$review_text === 'string') { assert.deepEqual(Object.keys(value), ['$review_text']); const text = db.prepare('SELECT payload FROM review_texts WHERE hash=?').get(value.$review_text); assert(text); assert.equal(sha(text.payload), value.$review_text); references++; return text.payload; } return Object.fromEntries(Object.entries(value).map(([k, v]) => [k, hydrate(v)])); } return value; }
        const doc = hydrate(JSON.parse(row.payload)), s = heads.get(f.id);
        for (const key of ['document_id', 'content', 'revision', 'generation', 'review_revision', 'disk_revision']) assert.deepEqual(doc[key], s[key], 'durable row/' + key);
        assert.deepEqual(doc.changes.map(semantic), s.changes.map(semantic)); assert.equal(doc.undo.length > 0, s.can_undo); assert.equal(doc.redo.length > 0, s.can_redo); assert.equal(sha(doc.checkpoint.content), s.saved_revision);
        for (const action of [...doc.undo, ...doc.redo]) { assert.equal(action.edits.length, action.inverse.length); assert.equal(action.inverse.length, action.inverse_expected.length); }
        if (epoch === 1 && f.id !== 'enclosing') { const action = doc.undo.find(a => a.edits.length === 2 && a.edits[0].from === 2 && a.edits[1].from === 3); assert(action);
          assert.deepEqual(action.inverse, [{ from: 2, to: 2, insert: 'a' }, { from: 2, to: f.replacement ? 5 : 2, insert: 'b' }]); assert.deepEqual(action.inverse_expected, ['', f.replacement]); }
        result.documents.push({ fixture: f.id, payloadSha256: sha(row.payload), contentSha256: sha(doc.content), changes: doc.changes.map(semantic),
          undoActions: doc.undo.length, redoActions: doc.redo.length, allRawFragmentLengthsMatch: true, hydratedTextReferences: references, matchesActualHttpSnapshot: true });
      }
    } finally { db.close(); }
    assert.equal(sha(fs.readFileSync(file)), before, 'Independent readOnly inspection cannot change main database bytes'); result.databaseSha256 = before;
    report.databaseInspections.push(result); write('database-readback-' + epoch + '.json', result); check('epoch-' + epoch + '/actual-stopped-SQLite-matches-HTTP-and-raw-fragments');
  }
  async function stopAndRead(epoch) { report.phase = 'persistent HTTP readback PID ' + epoch;
    for (const f of rows) { const saved = heads.get(f.id), actual = await api('/api/review', { action: 'read', document_id: saved.document_id });
      for (const key of ['document_id', 'content', 'revision', 'generation', 'review_revision', 'disk_revision', 'saved_revision', 'can_undo', 'can_redo']) assert.deepEqual(actual[key], saved[key]);
      assert.deepEqual(actual.changes.map(semantic), saved.changes.map(semantic)); }
    check('epoch-' + epoch + '/real-durable-HTTP-readback'); report.phase = 'stop PID ' + epoch; await host.stop(); host = null; database(epoch); }
  try {
    if (args.sqlite === 'required') { ({ DatabaseSync } = require('node:sqlite')); assert.equal(typeof DatabaseSync, 'function'); }
    binaryIdentity = ordinary(args.bin); report.binarySha256 = await fileHash(args.bin); assert.deepEqual(ordinary(args.bin), binaryIdentity);
    ordinary(path.join(args.assets, 'index.html'), 2 * 1024 * 1024); ordinary(path.join(args.assets, 'readmd.boot.js'), 16 * 1024 * 1024);
    index = fs.readFileSync(path.join(args.assets, 'index.html')); boot = fs.readFileSync(path.join(args.assets, 'readmd.boot.js'));
    report.assets = { indexSha256: sha(index), rootBootSha256: sha(boot) };
    const sourceFiles = ['document_transaction.rs', 'review.rs', 'review_api.rs', 'review_store.rs', 'server.rs', 'content.rs', 'text_encoding.rs', 'main.rs'];
    const optional = 'review_resource_history.rs'; report.optionalSourceAbsent = [];
    if (fs.existsSync(path.join(args.repo, 'rust/readmd-kernel/src', optional))) sourceFiles.push(optional); else report.optionalSourceAbsent.push(optional);
    bindings = () => sourceFiles.map(name => ({ file: 'rust/readmd-kernel/src/' + name, sha256: sha(fs.readFileSync(path.join(args.repo, 'rust/readmd-kernel/src', name))) }));
    report.sourceBinding = bindings(); report.expectedSchemaVersion = Number(fs.readFileSync(path.join(args.repo, 'rust/readmd-kernel/src/review_store.rs'), 'utf8').match(/SCHEMA_VERSION:\s*i64\s*=\s*(\d+)/)?.[1]); assert(Number.isSafeInteger(report.expectedSchemaVersion));
    report.sourceCommit = execFileSync('git', ['rev-parse', 'HEAD'], { cwd: args.repo, encoding: 'utf8', maxBuffer: 4096 }).trim(); assert(/^[a-f0-9]{40}$/.test(report.sourceCommit));
    report.trackedWorktreeClean = !execFileSync('git', ['status', '--porcelain', '--untracked-files=no'], { cwd: args.repo, encoding: 'utf8', maxBuffer: 65536 }).trim();
    report.binaryOrigin = 'Explicit prebuilt; source equivalence is separately bound by root build receipt.';
    const enclosing = rows[0]; host = await launch(1); await open(enclosing);
    report.phase = 'pending AI and enclosing manual';
    const pending = await api('/api/review', { action: 'propose', ...cas(heads.get(enclosing.id)), origin: 'ai', task_id: 'synthetic-portable-undo', edits: enclosing.proposal });
    assert.equal(pending.changes.length, 1); ids.set('pending', pending.changes[0].id); validate(enclosing, pending, PENDING, 'pending'); snapshot(enclosing, 'propose', pending);
    await mutate(enclosing, 'edit', { origin: 'editor', edits: enclosing.enclosing }, MANUAL, 'manual');
    await mutate(enclosing, 'undo', {}, PENDING, 'pending'); await mutate(enclosing, 'redo', {}, MANUAL, 'manual'); await mutate(enclosing, 'undo', {}, PENDING, 'pending');
    check('enclosing/manual-undo-redo-restores-exact-pending-semantics'); await save(enclosing, PENDING, 'pending');
    for (const f of rows.slice(1)) { report.phase = f.id + '/initial-adjacent-edit'; await open(f); await mutate(f, 'edit', { origin: 'editor', edits: [{ from: 2, to: 3, insert: '' }, { from: 3, to: 4, insert: f.replacement }] }, f.after); await save(f, f.after); }
    await stopAndRead(1);
    host = await launch(2); report.phase = 'new PID exact pending and withdrawal'; await open(enclosing, true, PENDING, 'pending');
    await mutate(enclosing, 'resolve', { ids: [ids.get('pending')], decision: 'withdraw' }, LF, 'withdrawn');
    await mutate(enclosing, 'undo', {}, PENDING, 'pending'); await mutate(enclosing, 'redo', {}, LF, 'withdrawn'); await save(enclosing, LF, 'withdrawn'); check('enclosing/new-PID-exact-withdraw-undo-redo');
    for (const f of rows.slice(1)) { report.phase = f.id + '/persistent-undo-redo'; await open(f, true, f.after); await mutate(f, 'undo', {}, f.initial); await mutate(f, 'redo', {}, f.after); const undone = await mutate(f, 'undo', {}, f.initial); assert.equal(undone.can_redo, true); await save(f, f.initial); assert.equal(heads.get(f.id).can_redo, true); check(f.id + '/new-PID-one-undo-redo-and-save'); }
    await stopAndRead(2);
    host = await launch(3); report.phase = 'third PID saved withdrawal'; await open(enclosing, true, LF, 'withdrawn');
    for (const f of rows.slice(1)) { report.phase = f.id + '/third-PID-durable-redo'; const reopened = await open(f, true, f.initial); assert.equal(reopened.can_redo, true); await mutate(f, 'redo', {}, f.after); await save(f, f.after); await mutate(f, 'undo', {}, f.initial); await save(f, f.initial); check(f.id + '/third-PID-redo-undo-closes-inverse'); }
    disks(); for (const f of rows) assert.deepEqual(expectedDisk.get(f.id), physical(f.initial));
    check('all-originals-restored-and-owned-resource-sentinel-unchanged'); await stopAndRead(3); remaining();
    assert.equal(new Set(report.processes.map(p => p.pid)).size, 3); assert.equal(report.processes.length, 3);
    assert.equal(report.databaseInspections.length, 3); check('three-distinct-real-PIDs-stopped-and-durable-readbacks-complete');
    report.scenarios = rows.map(f => ({ id: f.id, passed: true })); report.bodyCompleted = true;
  } catch (error) { fail(error); process.exitCode = 1; }
  finally {
    for (const owned of processes) if (!owned.record.terminalConfirmed) { try { await owned.stop(); } catch (error) { fail(error); report.cleanup.errors = [...(report.cleanup.errors || []), { epoch: owned.record.epoch, name: error.name, message: String(error.message).slice(0, 300) }]; } }
    report.cleanup.allOwnedTerminalConfirmed = processes.length > 0 && processes.every(p => p.record.terminalConfirmed && p.record.exitObserved && p.record.closeObserved);
    try {
      if (report.binarySha256) { report.binaryUnchanged = report.binarySha256 === await fileHash(args.bin) && JSON.stringify(ordinary(args.bin)) === JSON.stringify(binaryIdentity); assert(report.binaryUnchanged); }
      report.driverUnchanged = sha(fs.readFileSync(__filename)) === report.driverSha256; assert(report.driverUnchanged);
      report.inputReceiptUnchanged = sha(fs.readFileSync(path.join(args.evidence, 'independent-expectations.json'))) === report.expectationsFileSha256; assert(report.inputReceiptUnchanged);
      if (bindings) { report.sourceBindingAfter = bindings(); report.sourceUnchanged = JSON.stringify(report.sourceBindingAfter) === JSON.stringify(report.sourceBinding); assert(report.sourceUnchanged); }
      if (index && boot) { report.assetsUnchanged = sha(fs.readFileSync(path.join(args.assets, 'index.html'))) === sha(index) && sha(fs.readFileSync(path.join(args.assets, 'readmd.boot.js'))) === sha(boot); assert(report.assetsUnchanged); }
      if (report.cleanup.allOwnedTerminalConfirmed) { for (const item of scratch) { const now = fs.lstatSync(item.path, { bigint: true }); assert(now.isDirectory() && !now.isSymbolicLink() && now.dev === item.stat.dev && now.ino === item.stat.ino); fs.rmSync(item.path, { recursive: true }); } report.cleanup.scratchRemoved = true; }
      else report.cleanup.scratchRemoved = false;
      report.physicalInventory = rows.map(f => ({ fixture: f.id, file: path.relative(args.evidence, f.file), bytes: fs.statSync(f.file).size, sha256: sha(fs.readFileSync(f.file)) }));
      report.physicalInventory.push({ file: 'documents/resources/owned.txt', sha256: sha(fs.readFileSync(resource)) }, { file: 'documents/不相关.md', sha256: sha(fs.readFileSync(untouched)) });
    } catch (error) { fail(error); process.exitCode = 1; }
    for (const owned of processes) { const r = owned.record; try { fs.writeFileSync(path.join(args.evidence, 'backend-' + r.epoch + '.log'), safe(r.stdout + '\nSTDERR\n' + r.stderr), { flag: 'wx', mode: 0o600 }); } catch (error) { fail(error); } delete r.stdout; delete r.stderr;
      if (!r.terminalConfirmed) { r.unresolvedOwnedProcess = true; try { owned.child.unref(); owned.child.stdout?.destroy(); owned.child.stderr?.destroy(); } catch (error) { r.unresolvedHandleReleaseError = { name: error.name, code: error.code || null }; fail(error); } } }
    report.elapsedMs = performance.now() - start;
    report.productionAuth = report.processes.length === 3 && report.processes.every(p => p.anonymousReviewDenied && p.root?.tokenPresentInMemoryOnly);
    report.productionCsp = report.processes.length === 3 && report.processes.every(p => p.root?.cspSha256);
    report.passed = Boolean(report.bodyCompleted && !report.failure && report.binaryUnchanged && report.sourceUnchanged && report.assetsUnchanged && report.driverUnchanged && report.inputReceiptUnchanged && report.productionAuth && report.productionCsp && report.cleanup.allOwnedTerminalConfirmed && report.cleanup.scratchRemoved);
    if (!report.passed) process.exitCode = 1; write('report.json', report);
    console.log(JSON.stringify({ passed: report.passed, checks: report.checks.length, pids: report.processes.length, sqlite: report.databaseInspections.length, failure: safe(report.failure || null) }));
  }
}
if (require.main === module) main().catch(() => { console.error('Owned portable review regression preparation/execution failed; inspect its fresh evidence.'); process.exitCode = 1; });
module.exports = { main, fixtures, expectations, options };
