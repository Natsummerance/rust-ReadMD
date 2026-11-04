'use strict';
// Bounded owned OpenAPI baseline: genuine HTTP/MCP, headless application UI,
// explicit save and a fresh kernel's durable undo. Never launches VS Code.
const fs = require('node:fs'), path = require('node:path'), crypto = require('node:crypto'), assert = require('node:assert/strict');
const { execFileSync } = require('node:child_process');
const ROOT = path.resolve(__dirname, '../..'), sha = value => crypto.createHash('sha256').update(value).digest('hex');
const pause = ms => new Promise(resolve => setTimeout(resolve, ms));
const { options } = require('./v006-table-clipboard-kernel.cjs');
const { startKernel } = require('./v006-refactor-host.cjs');
const { stdio } = require('./code-lifecycle-http-stdio.cjs');
const { buildLaunchPlan, publicPlan } = require('../../ui-tests/run-vscode-openapi-native.cjs');
const FIXTURES = path.join(ROOT, 'tests/fixtures/openapi/v007');
const ORIGINAL = '# API guide 😀\n\nAuthor notes and repeated heading stay intact.\n\n## 重复标题\n\nManual prose.\n';
const encoded = text => Buffer.from('\ufeff' + text.replaceAll('\n', '\r\n'));
const cas = snapshot => ({ document_id: snapshot.document_id, generation: snapshot.generation, review_revision: snapshot.review_revision, call_id: crypto.randomUUID() });
async function until(check, label, ms = 15000) {
  const end = Date.now() + ms;
  while (Date.now() < end) { if (await check()) return; await pause(20); }
  throw Error('Deadline: ' + label);
}
async function openOwned(output, configure, { binary, playwright }) {
  assert.equal(fs.readdirSync(output).length, 0, 'Fresh owned evidence directory');
  const paths = Object.fromEntries(['profile', 'documents', 'tmp', 'home', 'browser-profile'].map(name => [name, path.join(output, name)]));
  for (const directory of Object.values(paths)) fs.mkdirSync(directory, { mode: 0o700 });
  const scratch = ['tmp', 'home', 'browser-profile'].map(name => ({ path: paths[name], stat: fs.lstatSync(paths[name]) }));
  const cleanup = () => { for (const item of scratch) { const now = fs.lstatSync(item.path); assert(now.isDirectory() && !now.isSymbolicLink() && now.ino === item.stat.ino && now.dev === item.stat.dev); fs.rmSync(item.path, { recursive: true }); } };
  Object.assign(process.env, { TMPDIR: paths.tmp, TMP: paths.tmp, TEMP: paths.tmp });
  let kernel, context;
  try {
    await configure(null, paths); kernel = await startKernel(binary, paths.profile, paths.documents);
    const root = await kernel.api('/'), base = new URL(root.url).origin;
    const { chromium } = require(playwright || require.resolve('playwright', { paths: [path.join(ROOT, 'ui-tests')] }));
    const env = Object.fromEntries(Object.entries(process.env).filter(([name]) => /^(PATH|SYSTEMROOT|WINDIR|COMSPEC|PATHEXT|LANG|LC_ALL)$/i.test(name)));
    Object.assign(env, { TMPDIR: paths.tmp, CFFIXED_USER_HOME: paths.home, CHROME_LOG_FILE: path.join(output, 'chrome.log') });
    const channel = process.env.READMD_CODE_BROWSER || 'chrome';
    context = await chromium.launchPersistentContext(paths['browser-profile'], { channel: channel === 'chromium' ? undefined : channel, headless: true, env, viewport: { width: 1160, height: 820 }, bypassCSP: false, serviceWorkers: 'block', args: ['--disable-crash-reporter', '--disable-breakpad', '--disable-background-networking', '--disable-component-update', '--password-store=basic'] });
    const page = context.pages()[0] || await context.newPage(), errors = []; page.setDefaultTimeout(10000);
    page.on('pageerror', error => errors.push(String(error.message).replaceAll(output, '<owned>').slice(0, 300)));
    await context.route('**/*', route => new URL(route.request().url()).origin === base ? route.continue() : route.abort());
    // Settings/session bootstrap only. No file chooser or authentication stub.
    await page.addInitScript(() => {
      const headers = () => { const token = document.querySelector('meta[name="readmd-app-token"]')?.content || ''; return { 'Content-Type': 'application/json', 'X-ReadMD-App-Token': token, 'X-ReadMD-Token': token }; };
      window.pywebview = { api: {
        get_settings: async () => { const value = await (await fetch('/api/settings', { headers: headers() })).json(); return value?.settings && typeof value.ok === 'boolean' ? value.settings : value; },
        save_settings: async value => (await fetch('/api/settings', { method: 'POST', headers: headers(), body: JSON.stringify(value) })).json(),
        get_system_language: async () => 'zh-CN', get_autostart: async () => false, get_app_info: async () => ({}), start_modules: async () => true,
      } };
    });
    const boot = page.waitForResponse(response => new URL(response.url()).pathname === '/assets/readmd.boot.js'); await page.goto(base);
    assert.equal(sha(await (await boot).body()), sha(fs.readFileSync(path.join(ROOT, 'assets/readmd.boot.js'))));
    const csp = await page.locator('meta[http-equiv="Content-Security-Policy"]').getAttribute('content'); assert(csp.includes("script-src 'self'")); assert(!/script-src[^;]*'unsafe-(inline|eval)'/.test(csp));
    await until(() => page.evaluate(() => Boolean(window.__readmdAppReady && window.ReadMDOpenAPI && window.ReadMDReview)), 'owned OpenAPI ready'); await page.evaluate(() => window.__readmdSessionReady);
    return { ...paths, host: kernel, context, page, errors, async close() { try { await context.close(); } finally { await kernel.stop(); cleanup(); } } };
  } catch (error) { if (context) await context.close(); if (kernel) await kernel.stop(); cleanup(); throw error; }
}
function files(directory, relative = '') {
  return fs.readdirSync(path.join(directory, relative), { withFileTypes: true }).flatMap(entry => {
    assert(!entry.isSymbolicLink(), 'Fixed input must be ordinary files and directories');
    const name = path.join(relative, entry.name);
    return entry.isDirectory() ? files(directory, name) : [{ name: name.split(path.sep).join('/'), bytes: fs.statSync(path.join(directory, name)).size, sha256: sha(fs.readFileSync(path.join(directory, name))) }];
  }).sort((a, b) => a.name.localeCompare(b.name));
}
function totalBytes(directory) {
  return fs.readdirSync(directory, { withFileTypes: true }).reduce((sum, entry) => {
    const file = path.join(directory, entry.name);
    return sum + (entry.isDirectory() && !entry.isSymbolicLink() ? totalBytes(file) : fs.lstatSync(file).size);
  }, 0);
}
function extensionPreflight(output, binary) {
  // A deliberately incomplete owned checkout reproduces the launch planner's
  // actual boundary without touching the user's installed or staged extension.
  const area = path.join(output, 'extension-plan-fixture');
  for (const name of ['assets', 'packages/vscode-extension', 'ui-tests']) fs.mkdirSync(path.join(area, name), { recursive: true });
  fs.writeFileSync(path.join(area, 'packages/vscode-extension/package.json'), JSON.stringify({ name: 'owned-plan-fixture', main: './out/extension.js' }));
  for (const name of ['vscode-openapi-native.cjs', 'vscode-openapi-driver.cjs']) fs.writeFileSync(path.join(area, 'ui-tests', name), '// Owned path-check fixture; never executed.\n');
  const stub = path.join(area, 'vscode-not-executable'); fs.writeFileSync(stub, 'Owned read-only plan fixture; never executed.\n');
  const plan = buildLaunchPlan({ root: area, out: path.join(area, 'audit'), environment: { READMD_BIN: binary, READMD_VSCODE_BIN: stub } });
  const entrypoint = path.join(area, 'packages/vscode-extension/out/extension.js');
  assert.equal(plan.ready, false); assert.equal(fs.existsSync(entrypoint), false); assert.equal(plan.build.reason, 'extension_entrypoint_missing');
  const extensionRoot = path.join(ROOT, 'packages/vscode-extension');
  const source = fs.readFileSync(path.join(extensionRoot, 'src/openapi.ts'), 'utf8');
  const stagedFile = path.join(extensionRoot, 'out/openapi.js'), staged = fs.existsSync(stagedFile) ? fs.readFileSync(stagedFile, 'utf8') : null;
  const languages = text => text?.match(/new Set\('([^']+)'\.split\(' '\)\)/)?.[1]?.split(' ') || [];
  return {
    launched: false, extensionHostCertified: false, nativeUiCertified: false,
    observedGap: null, guardedFailure: 'Missing manifest.main compilation output is rejected before launching.',
    fixturePlan: publicPlan(plan), compiledEntrypointPresent: false,
    sourceOpenapiSha256: sha(source), stagedOpenapiPresent: staged !== null,
    ...(staged === null ? {} : { stagedOpenapiSha256: sha(staged), sourceLocalesMissingFromStaged: languages(source).filter(locale => !languages(staged).includes(locale)) }),
    repairFiles: ['ui-tests/run-vscode-openapi-native.cjs'],
    existingNativeTestBoundary: 'Four fixed multi-file operations and independent UTF-16 offset checks are prepared for the real Extension Host; graphical execution remains unrun.',
  };
}
async function main() {
  const args = options(process.argv.slice(2)); assert(args.bin && !args.base);
  const output = args.evidence, manifest = files(FIXTURES), initialExtraRoots = process.env.READMD_EXTRA_ROOTS;
  process.env.READMD_EXTRA_ROOTS = '';
  const binding = () => ({ head: execFileSync('git', ['rev-parse', 'HEAD'], { cwd: ROOT, encoding: 'utf8' }).trim(), moduleSha256: sha(fs.readFileSync(path.join(ROOT, 'rust/readmd-kernel/src/openapi_document.rs'))), driverSha256: sha(fs.readFileSync(__filename)), fixtureManifestSha256: sha(JSON.stringify(files(FIXTURES))), bootSha256: sha(fs.readFileSync(path.join(ROOT, 'assets/readmd.boot.js'))), indexSha256: sha(fs.readFileSync(path.join(ROOT, 'assets/index.html'))) });
  const sourceBefore = binding();
  const report = {
    passed: false, headless: true, nativeUiCertified: false, extensionHostCertified: false, modelsRequested: false,
    binarySha256: sha(fs.readFileSync(args.bin)), bootSha256: sha(fs.readFileSync(path.join(ROOT, 'assets/readmd.boot.js'))),
    moduleSha256: sha(fs.readFileSync(path.join(ROOT, 'rust/readmd-kernel/src/openapi_document.rs'))), driverSha256: sha(fs.readFileSync(__filename)),
    sourceBinding: sourceBefore,
    openapiModuleMatchesHead: sourceBefore.moduleSha256 === sha(execFileSync('git', ['show', 'HEAD:rust/readmd-kernel/src/openapi_document.rs'], { cwd: ROOT })),
    binaryOrigin: 'Explicit prebuilt artifact identified by its digest. Matching the current OpenAPI module does not independently certify the complete binary source or packaging; the caller must supply that binding.',
    inputManifest: manifest, originalDocumentSha256: sha(encoded(ORIGINAL)), checks: [], processes: [], historyRequests: [],
    artifactScope: 'Explicit prebuilt artifact and served boot; this is a baseline, not final packaged-source or native Extension Host certification.',
    serviceBoundary: 'HTTP and MCP call the same Rust OpenAPI endpoint. Desktop edits enter durable review history; VS Code uses its own native editor undo history.',
  };
  let test, host, mcp, testClosed = false, phase = 'startup', nextTmp, nextTmpIdentity;
  const links = [], responseReads = [];
  const mark = next => { phase = next; console.log(JSON.stringify({ phase })); };
  const write = (name, value) => fs.writeFileSync(path.join(output, name), JSON.stringify(value, null, 2) + '\n', { flag: 'wx', mode: 0o600 });
  const currentHost = () => host || test.host;
  async function raw(route, body) {
    const response = await currentHost().api(route, body === undefined ? {} : { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(body) });
    return { status: response.status, body: await response.json() };
  }
  async function ok(route, body) { const result = await raw(route, body); assert.equal(result.status, 200, result.body.error_code); assert.notEqual(result.body.ok, false, result.body.error_code); return result.body; }
  const apiArgs = (file, content = ORIGINAL, extra = {}) => ({ file_path: file, content, base_revision: sha(content), locale: 'en', ...extra });
  async function generate(file, content = ORIGINAL, extra = {}) { return ok('/api/developer/openapi', apiArgs(file, content, extra)); }
  async function rejected(file, content, expected, status = 422, extra = {}) {
    const result = await raw('/api/developer/openapi', apiArgs(file, content, extra));
    assert.equal(result.status, status); assert.equal(result.body.error_code, expected); return { status: result.status, errorCode: result.body.error_code };
  }
  async function tool(name, arguments_, expectedError) {
    const result = await mcp.request('tools/call', { name, arguments: arguments_ }).result;
    assert.deepEqual(JSON.parse(result.content[0].text), result.structuredContent);
    if (expectedError) { assert.equal(result.isError, true); assert.equal(result.structuredContent.error_code, expectedError); }
    else { assert.notEqual(result.isError, true); assert.equal(result.structuredContent.ok, true); }
    return result.structuredContent;
  }
  try {
    test = await openOwned(output, async (_api, paths) => {
      for (const input of manifest) { const destination = path.join(paths.documents, input.name); fs.mkdirSync(path.dirname(destination), { recursive: true }); fs.copyFileSync(path.join(FIXTURES, input.name), destination, fs.constants.COPYFILE_EXCL); }
      fs.writeFileSync(path.join(paths.documents, 'API-guide.md'), encoded(ORIGINAL), { flag: 'wx' });
    }, { binary: args.bin, playwright: args.playwright });
    report.processes.push({ surface: 'headless-ui-http', pid: test.host.pid });
    const { page } = test, source = path.join(test.documents, 'bundle/api.yaml'), target = path.join(test.documents, 'API-guide.md'), scalar = path.join(test.documents, 'bundle/defs/scalar.yaml');
    const scalarOriginal = fs.readFileSync(scalar), unchangedDisk = text => assert.deepEqual(fs.readFileSync(target), encoded(text));
    report.extensionPreflight = extensionPreflight(output, args.bin); write('extension-host-preflight.json', report.extensionPreflight);
    const root = await test.host.api('/'), anonymous = await fetch(new URL('/api/developer/openapi', root.url), { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(apiArgs(source)) });
    assert([401, 403].includes(anonymous.status)); report.anonymousStatus = anonymous.status;
    const mcpProfile = path.join(output, 'mcp-profile'); fs.mkdirSync(mcpProfile);
    mcp = stdio(args.bin, mcpProfile, test.documents); report.processes.push({ surface: 'MCP-stdio', pid: mcp.child.pid });
    await mcp.request('initialize', { protocolVersion: '2024-11-05', capabilities: {}, clientInfo: { name: 'owned-v007-openapi-baseline', version: '1' } }).result; mcp.notify('notifications/initialized', {});

    mark('complex multi-file HTTP and MCP projection');
    const first = await generate(source), generated = first.result.content;
    assert.deepEqual(await tool('readmd_openapi_document', apiArgs(source)), first);
    assert.equal(first.warnings.length, 0); assert.equal(first.source_map.length, 4);
    assert.equal(new Set(first.source_map.map(row => row.pointer)).size, 4);
    const expectedOperations = [
      ['get', '/items/{id}', 'getItem', '/paths/~1items~1{id}/get'],
      ['post', '/items/{id}', 'createItem', '/paths/~1items~1{id}/post'],
      ['post', '/other', 'otherItem', '/paths/~1other/post'],
      ['get', '/search', 'searchItems', '/paths/~1search/get'],
    ];
    assert.deepEqual(first.source_map.map(row => [row.method, row.path, row.operation_id, row.pointer]), expectedOperations);
    const independentPrefix = '😀Z\n'; assert.equal(independentPrefix.length, 4);
    const shifted = await generate(source, independentPrefix + ORIGINAL);
    for (let index = 0; index < expectedOperations.length; index++) {
      const [method, operationPath] = expectedOperations[index], heading = '## ' + method.toUpperCase() + ' ` ' + operationPath + ' ` — 重复标题';
      const next = expectedOperations[index + 1], boundary = next ? '## ' + next[0].toUpperCase() + ' ` ' + next[1] + ' ` — 重复标题' : '#### Data models';
      assert.equal(generated.indexOf(heading), generated.lastIndexOf(heading)); assert.equal(generated.indexOf(heading), first.source_map[index].from);
      assert.equal(generated.indexOf(boundary, first.source_map[index].from + heading.length), first.source_map[index].to);
      assert.equal(shifted.source_map[index].from, first.source_map[index].from + 4); assert.equal(shifted.source_map[index].to, first.source_map[index].to + 4);
    }
    assert(generated.startsWith(ORIGINAL)); assert(!generated.includes(output)); assert(generated.includes('OPENAPI_V007_MULTIFILE_RESPONSE'));
    for (const keyword of ['oneOf', 'anyOf', 'allOf', 'prefixItems', 'unevaluatedProperties', 'bearerFormat', 'authorizationCode', 'read:items', 'write:items']) assert(generated.includes(keyword), 'Retained complex schema/security field ' + keyword);
    assert(!generated.includes('OPENAPI_V007_LITERAL_MUST_NOT_BE_RESOLVED'));
    assert(!first.references.some(row => row.source.endsWith('literal-no-read.yaml')));
    for (const row of first.source_map) {
      const section = generated.slice(row.from, row.to); assert(section.startsWith('## ' + row.method.toUpperCase())); assert(section.includes(row.operation_id));
      if (row.path === '/items/{id}') assert.deepEqual(row.definition, { source: 'paths/items.yaml', pointer: '/' + row.method });
      if (row.path === '/search') assert(section.includes('```json\n[]\n```'), 'Explicit empty security clears inheritance');
      if (row.operation_id === 'getItem') { assert(section.includes('bearer')); assert(section.includes('read:items')); }
      if (row.operation_id === 'createItem') { assert(section.includes('write:items')); assert(!section.includes('read:items')); }
    }
    unchangedDisk(ORIGINAL); write('generation-http.json', first);
    report.projection = { sourceRevision: first.source_revision, maps: first.source_map, references: first.references, generatedSha256: sha(generated), bytes: Buffer.byteLength(generated), fixedOperations: expectedOperations, independentUtf16Delta: 4 };
    report.checks.push('Real HTTP/MCP projections match; nested JSON/YAML schema and security references preserve complex fields, public security, repeated summaries and four distinct UTF-16 source maps; literal examples cause no file read.');

    mark('cycles missing sources and explicit path boundaries');
    const cycle = await generate(path.join(test.documents, 'cycle/api.yaml'));
    assert(cycle.warnings.some(row => row.code === 'recursive_reference')); assert(cycle.result.content.includes('cycle.yaml#/A'));
    const missingFile = path.join(test.documents, 'missing/api.yaml'), missing = await generate(missingFile);
    assert(missing.warnings.some(row => row.code === 'missing_reference'));
    fs.writeFileSync(path.join(test.documents, 'missing/absent.yaml'), 'Ok: {description: Now present}\n', { flag: 'wx' });
    report.missingReceiptProtection = await rejected(missingFile, ORIGINAL, 'transaction_conflict', 409, { expected_source_revision: missing.source_revision });
    const scoped = await generate(path.join(test.documents, 'scoped/api.yaml'));
    assert(scoped.warnings.some(row => row.code === 'external_reference')); assert.deepEqual(scoped.references, []); assert(!scoped.result.content.includes('OPENAPI_V007_MULTIFILE_RESPONSE'));
    const outside = path.join(output, 'outside'); fs.mkdirSync(outside); const outsideFile = path.join(outside, 'api.yaml');
    const outsideText = "openapi: 3.1.1\ninfo: {title: 'EXPLICIT_OWNED_ABSOLUTE_SOURCE', version: '1'}\npaths: {}\n"; fs.writeFileSync(outsideFile, outsideText);
    const absolute = await generate(outsideFile); assert(absolute.result.content.includes('EXPLICIT_OWNED_ABSOLUTE_SOURCE'));
    const relativeDenied = await raw('/api/developer/openapi', apiArgs('../outside/api.yaml'));
    assert.equal(relativeDenied.status, 403); assert.equal(relativeDenied.body.error_code, 'forbidden');
    const link = path.join(test.documents, 'scoped/linked'); fs.symlinkSync(outside, link, process.platform === 'win32' ? 'junction' : 'dir'); links.push({ path: link, target: outside });
    const escapeFile = path.join(test.documents, 'scoped/escape.json');
    const escaped = { openapi: '3.1.1', info: { title: 'Owned link escape', version: '1' }, paths: { '/escape': { get: { responses: { '200': { $ref: './linked/api.yaml' } } } } } };
    fs.writeFileSync(escapeFile, JSON.stringify(escaped)); const blocked = await generate(escapeFile);
    assert(blocked.warnings.some(row => row.code === 'external_reference')); assert.deepEqual(blocked.references, []); assert(!JSON.stringify(blocked).includes('EXPLICIT_OWNED_ABSOLUTE_SOURCE'));
    assert.equal(fs.readFileSync(outsideFile, 'utf8'), outsideText);
    const absoluteMcp = await tool('readmd_openapi_document', apiArgs(outsideFile)); assert(absoluteMcp.result.content.includes('EXPLICIT_OWNED_ABSOLUTE_SOURCE'));
    report.pathBoundary = { httpAndMcpAbsoluteReadWithoutPriorDocumentOpen: true, singleFileGrantCertification: 'not established: token and explicit absolute-path compatibility alone do not prove the required authorization policy', relativeParent: { status: relativeDenied.status, errorCode: relativeDenied.body.error_code }, dependencyOutsideSourceDirectory: 'preserved as external reference without reading', directoryLinkEscape: 'preserved as external reference without reading' };
    report.invalidInput = await rejected(path.join(test.documents, 'invalid/api.yaml'), ORIGINAL, 'openapi_invalid');
    report.staleTarget = await rejected(source, ORIGINAL, 'transaction_conflict', 409, { base_revision: sha('different target') });
    unchangedDisk(ORIGINAL); report.checks.push('Cycles and missing targets remain diagnosed and qualified; missing-file creation invalidates a receipt. Token, relative-root and dependency-directory boundaries hold. HTTP/MCP also accept an explicit owned absolute source without a prior document grant: this observation does not certify single-file authorization. Invalid input and stale target revisions preserve the original.');

    mark('real review cancel apply explicit save and reopen');
    page.on('request', request => {
      if (new URL(request.url()).pathname !== '/api/review' || request.method() !== 'POST') return;
      const body = request.postDataJSON(); if (['undo', 'redo'].includes(body?.action)) report.historyRequests.push(body.action);
    });
    page.on('response', response => { if (new URL(response.url()).pathname === '/api/developer/openapi') responseReads.push(response.json()); });
    await page.evaluate(async target => { await i18n.setLanguage('en', false); await loadFile(target); await toggleEdit(); }, target);
    await until(() => page.evaluate(() => Boolean(cmView && ReadMDOpenAPI && ReadMDReview.snapshot(getActiveTab()))), 'OpenAPI target editor');
    assert.equal(await page.evaluate(source => ReadMDOpenAPI.generate(source), source), true);
    assert((await page.locator('#openapi-review-after').innerText()).includes('OPENAPI_V007_MULTIFILE_RESPONSE'));
    await page.locator('#openapi-review-cancel').click(); assert.equal(await page.evaluate(() => cmView.state.doc.toString()), ORIGINAL); unchangedDisk(ORIGINAL);
    assert.equal(await page.evaluate(source => ReadMDOpenAPI.generate(source), source), true);
    await page.locator('#openapi-review-apply').click(); await page.locator('#openapi-review-modal').waitFor({ state: 'hidden' });
    await until(async () => await page.evaluate(() => cmView.state.doc.toString()) === generated, 'exact real generated body'); unchangedDisk(ORIGINAL);
    assert.equal(await page.evaluate(() => ReadMDReview.flush(getActiveTab())), true);
    assert.equal(await page.evaluate(() => saveEdit()), true); unchangedDisk(generated);
    await page.evaluate(async target => { await exitEdit(); await loadFile(target, { force: true }); await toggleEdit(); }, target);
    assert.equal(await page.evaluate(() => cmView.state.doc.toString()), generated);
    assert.equal(await page.evaluate(source => ReadMDOpenAPI.generate(source), source), true);
    assert.equal(await page.locator('#openapi-review-apply').isDisabled(), true); await page.locator('#openapi-review-cancel').click();
    report.checks.push('Actual headless review Cancel changes nothing; Accept applies the exact shared transaction without saving; explicit save and real file reopen preserve UTF-8 BOM/CRLF and idempotence.');

    mark('dependency-only conflict update and manual-region protection');
    assert.equal(await page.evaluate(source => ReadMDOpenAPI.generate(source), source), true);
    fs.writeFileSync(scalar, scalarOriginal.toString().replace('minLength: 2', 'minLength: 5'));
    assert.equal(await page.evaluate(() => ReadMDOpenAPI.accept()), false);
    assert.equal(await page.locator('#openapi-review-status').innerText(), await page.evaluate(() => i18n.t('error.review_stale_generation')));
    assert.equal(await page.evaluate(() => cmView.state.doc.toString()), generated); unchangedDisk(generated); await page.locator('#openapi-review-cancel').click();
    report.staleDependencyHttp = await rejected(source, ORIGINAL, 'transaction_conflict', 409, { expected_source_revision: first.source_revision });
    await tool('readmd_openapi_document', apiArgs(source, ORIGINAL, { expected_source_revision: first.source_revision }), 'transaction_conflict');
    const notes = '\n\n## Author tail\nKeep this outside the generated region.\n', prior = generated + notes, updated = await generate(source, prior);
    assert(updated.result.changed); assert.equal(updated.changes.length, 3); assert(updated.changes.every(change => change.kind === 'changed')); assert(updated.result.content.endsWith(notes));
    assert.equal((await generate(source, updated.result.content)).result.changed, false);
    const applied = await ok('/api/documents/transaction', { content: prior, transaction: updated.transaction }); assert.equal(applied.result.content, updated.result.content);
    const inverse = await tool('readmd_content_transaction', { content: applied.result.content, transaction: { origin: 'openapi', base_revision: sha(applied.result.content), edits: applied.result.inverse } }); assert.equal(inverse.result.content, prior);
    report.manualProtection = await rejected(source, generated.replace('OPENAPI_V007_MULTIFILE_RESPONSE', 'MANUAL_GENERATED_REGION_EDIT'), 'openapi_local_changes');
    report.markerProtection = await rejected(source, generated.replace('<!-- readmd:openapi:end ', '<!-- damaged:openapi:end '), 'openapi_region_invalid');
    write('update-projection.json', updated); fs.writeFileSync(scalar, scalarOriginal); unchangedDisk(generated);
    assert.equal(await page.evaluate(() => cmView.state.doc.toString()), generated); assert.equal((await generate(source, generated)).result.changed, false);
    assert.deepEqual(test.errors, []); await Promise.all(responseReads);
    await page.screenshot({ path: path.join(output, 'generated-document.png') });
    await mcp.stop(); mcp = null; await test.close(); testClosed = true;
    report.checks.push('A dependency-only change is refused by actual desktop Accept and both transports; a fresh update preserves author prose and is one reversible shared transaction. Manual region edits and damaged markers are refused without changing the saved body.');

    mark('fresh kernel persistent history one undo and save');
    nextTmp = path.join(output, 'next-process-tmp'); fs.mkdirSync(nextTmp); nextTmpIdentity = fs.lstatSync(nextTmp); Object.assign(process.env, { TMPDIR: nextTmp, TMP: nextTmp, TEMP: nextTmp });
    host = await startKernel(args.bin, test.profile, test.documents); report.processes.push({ surface: 'fresh-http', pid: host.pid });
    const reopenedFile = await ok('/api/file?p=' + encodeURIComponent(target)); assert.equal((reopenedFile.original || reopenedFile.content).replaceAll('\r\n', '\n'), generated);
    const resumed = await ok('/api/review', { action: 'resume', path: target }); assert.equal(resumed.found, true); assert.equal(resumed.snapshot.content, generated); assert.equal(resumed.snapshot.can_undo, true);
    const undone = await ok('/api/review', { action: 'undo', ...cas(resumed.snapshot) }); assert.equal(undone.content, ORIGINAL); unchangedDisk(generated);
    report.historyRequests.push('undo-fresh-process');
    const savedUndo = await ok('/api/save', { path: target, content: undone.content.replaceAll('\n', '\r\n'), encoding: 'utf-8-sig', review_session: cas(undone) }); assert.equal(savedUndo.ok, true); unchangedDisk(ORIGINAL);
    report.durableUndo = { fromGeneration: resumed.snapshot.generation, toGeneration: undone.generation, oneUndoRestoresOriginal: true, diskChangedOnlyOnSave: true, savedOriginalSha256: sha(fs.readFileSync(target)) };
    await host.stop(); host = null;
    assert.deepEqual(files(FIXTURES), manifest); report.fixedInputsUnchanged = true;
    assert.equal(sha(fs.readFileSync(args.bin)), report.binarySha256, 'Selected binary changed during baseline');
    report.checks.push('After the UI and first kernel really stop, a distinct kernel restores the generated body and durable history; exactly one undo restores the entire original, and only the subsequent explicit save restores original BOM/CRLF disk bytes.');
    report.passed = true;
  } catch (error) {
    report.failure = { phase, name: error.name, message: String(error.message).replaceAll(output, '<owned>').replaceAll(ROOT, '<repo>').slice(0, 1400) }; process.exitCode = 1;
    write('first-failure.json', report.failure);
    if (test && !testClosed) { report.ui = await test.page.evaluate(() => ({ editing: state.editing, active: getActiveTab()?.name, status: document.getElementById('openapi-review-status')?.textContent, snapshot: ReadMDReview.snapshot(getActiveTab()) && { generation: ReadMDReview.snapshot(getActiveTab()).generation, canUndo: ReadMDReview.snapshot(getActiveTab()).can_undo } })).catch(() => ({ unavailable: true })); await test.page.screenshot({ path: path.join(output, 'first-failure.png') }).catch(() => {}); }
  } finally {
    if (mcp) { if (report.failure && mcp.diagnostic()) write('mcp-diagnostic.json', { message: mcp.diagnostic().replaceAll(output, '<owned>').replaceAll(ROOT, '<repo>').slice(0, 1500) }); if (mcp.child.exitCode === null) { mcp.child.kill(); await mcp.close; } }
    if (host) await host.stop(); if (test && !testClosed) await test.close();
    for (const link of links) { const stat = fs.lstatSync(link.path); assert(stat.isSymbolicLink()); assert.equal(fs.readlinkSync(link.path), link.target); fs.unlinkSync(link.path); }
    if (nextTmp) { const now = fs.lstatSync(nextTmp); assert(now.ino === nextTmpIdentity.ino && now.dev === nextTmpIdentity.dev && now.isDirectory() && !now.isSymbolicLink()); fs.rmSync(nextTmp, { recursive: true }); }
    if (initialExtraRoots === undefined) delete process.env.READMD_EXTRA_ROOTS; else process.env.READMD_EXTRA_ROOTS = initialExtraRoots;
    report.pageErrors = test?.errors || [];
    report.ownedBrowserScratchRemoved = ['tmp', 'home', 'browser-profile'].every(name => { try { fs.lstatSync(path.join(output, name)); return false; } catch (error) { if (error.code === 'ENOENT') return true; throw error; } });
    report.sourceBindingAfter = binding(); report.sourceUnchanged = JSON.stringify(sourceBefore) === JSON.stringify(report.sourceBindingAfter);
    report.retainedBytes = totalBytes(output); report.budgetBytes = 100 * 1048576; assert(report.retainedBytes < report.budgetBytes);
    write('report.json', report); console.log(JSON.stringify({ passed: report.passed, checks: report.checks.length, failure: report.failure, extensionHostPreflightGap: Boolean(report.extensionPreflight?.observedGap), retainedBytes: report.retainedBytes }));
  }
}
if (require.main === module) main().catch(error => { console.error('Owned OpenAPI baseline failed: ' + String(error.message).slice(0, 200)); process.exitCode = 1; });
module.exports = { main };
