// Maintainer-only evidence. Never modifies inputs, locks, runtime or approval.
import fs from 'node:fs';
import path from 'node:path';
import crypto from 'node:crypto';
import assert from 'node:assert/strict';
import {fileURLToPath} from 'node:url';
import {parseTlpdb} from '../lib/tex-package-inputs.mjs';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');
const base = '.cache/capability-upgrade/tex-inputs';
const lockFile = 'tools/tex-publication-inputs.lock.json';
const runtimeLockFile = 'tools/tex-runtime-inputs.lock.json';
const targetNames = ['pdfmanagement', 'tagpdf', 'latexindent', 'texlive-scripts', 'texlive.infra', 'xifthen'];
const relative = file => path.relative(root, file).replaceAll('\\', '/');
export const sha256 = bytes => crypto.createHash('sha256').update(bytes).digest('hex');

function local(value) {
  assert.equal(typeof value, 'string');
  const resolved = path.resolve(root, value), rel = path.relative(root, resolved);
  assert(rel && rel !== '..' && !rel.startsWith('..' + path.sep) && !path.isAbsolute(rel), 'Evidence must stay in the workspace');
  // Reject symlinked parents, including a final file, before reading/writing.
  let current = resolved;
  while (current !== root) {
    if (fs.existsSync(current)) assert(!fs.lstatSync(current).isSymbolicLink(), 'Linked evidence path');
    current = path.dirname(current);
  }
  return resolved;
}
function readJSON(value) { return JSON.parse(fs.readFileSync(local(value), 'utf8')); }
async function identity(value, algorithm = 'sha256') {
  const file = local(value), stat = fs.lstatSync(file);
  assert(stat.isFile());
  const digest = crypto.createHash(algorithm);
  for await (const bytes of fs.createReadStream(file)) digest.update(bytes);
  return {path:relative(file), bytes:stat.size, [algorithm]:digest.digest('hex')};
}
function newOutput(value) {
  const out = local(value);
  fs.mkdirSync(out); // Never overwrite a previous run, including a failure.
  return out;
}
function writeJSON(file, value) {fs.writeFileSync(file, JSON.stringify(value, null, 2) + '\n', {flag:'wx'});}

export function verifyGitSource(bytes, record, tree) {
  const blob = tree.tree.find(b => b.type === 'blob' && b.path === record.upstreamPath);
  assert(blob, 'Missing upstream Git blob: ' + record.upstreamPath);
  assert.equal(bytes.length, blob.size, record.upstreamPath);
  const gitHash = crypto.createHash('sha1').update(Buffer.from('blob ' + bytes.length + '\0')).update(bytes).digest('hex');
  assert.equal(gitHash, blob.sha, record.upstreamPath);
  assert.equal(gitHash, record.gitBlobSha1, record.upstreamPath);
  assert.equal(sha256(bytes), record.sha256, record.upstreamPath);
}

// Exact bytes are always reported separately from newline-normalized equality.
export function compareBytes(generated, runtime) {
  return {exact:generated.equals(runtime),
    newlineNormalized:generated.toString('utf8').replaceAll('\r\n', '\n') === runtime.toString('utf8').replaceAll('\r\n', '\n')};
}

export async function inventory(output) {
  const out = newOutput(output), lock = readJSON(lockFile), runtimeLock = readJSON(runtimeLockFile);
  assert.equal(lock.redistributionApproved, false);
  assert.equal(runtimeLock.review.redistributionApproved, false);
  assert.equal(lock.baseArchiveSha256, runtimeLock.archive.sha256);
  const frozenInputs = await Promise.all([lockFile, runtimeLockFile, base + '/inventory.json', base + '/publication-runtime.json',
    'docs/evidence/capability-upgrade/2026-10-10/batch3-tex-publication-inputs.json',
    'docs/evidence/capability-upgrade/2026-10-13/tex-corresponding-materials-audit/audit.json'].map(f => identity(f)));
  const baseInventory = readJSON(base + '/inventory.json');
  assert.deepEqual(baseInventory.archive, runtimeLock.archive);
  const archive = await identity(base + '/' + runtimeLock.archive.name);
  assert.equal(archive.bytes, runtimeLock.archive.size); assert.equal(archive.sha256, runtimeLock.archive.sha256);
  const dbFile = base + '/TinyTeX/tlpkg/texlive.tlpdb', database = await identity(dbFile);
  const expectedDB = baseInventory.files.find(f => f.path === 'tlpkg/texlive.tlpdb');
  assert.equal(database.sha256, expectedDB.sha256); assert.equal(database.bytes, expectedDB.size);
  const installed = parseTlpdb(fs.readFileSync(local(dbFile)));
  const expectedFiles = new Map(baseInventory.files.map(f => [f.path, f]));
  const containers = [], partials = [], componentLicenses = [];
  for (const component of lock.components) {
    componentLicenses.push({name:component.name, revision:component.revision, base:component.base, license:component.license,
      sourceKinds:Object.keys(component.containers).filter(k => k !== 'run'),
      unresolvedKinds:(component.unresolvedMaterials || []).map(c => c.kind), clearance:'unreviewed'});
    for (const [kind, container] of [...Object.entries(component.containers), ...(component.unresolvedMaterials || []).map(c => [c.kind, c])]) {
      const file = base + '/containers/' + container.name;
      const record = {component:component.name, revision:component.revision, kind, name:container.name,
        expectedBytes:container.size, expectedSha512:container.sha512, fileListEntries:container.files.length, status:'missing'};
      if (fs.existsSync(local(file))) {
        record.actual = await identity(file, 'sha512');
        record.status = record.actual.bytes === container.size && record.actual.sha512 === container.sha512 ? 'exact' : 'mismatch';
      }
      containers.push(record);
      if (fs.existsSync(local(file + '.partial'))) partials.push(await identity(file + '.partial'));
    }
  }
  const runMaterials = [];
  for (const name of targetNames) {
    const locked = lock.components.find(c => c.name === name), component = installed.get(name);
    assert.equal(component.revision, locked.revision, name + ' revision');
    const files = [];
    for (const file of component.containers.run.files) {
      const target = base + '/TinyTeX/' + file, expected = expectedFiles.get(file);
      // TinyTeX deliberately prunes files from installed TL package records.
      // Record that omission; do not invent an identity or hide it as a match.
      if (!expected && !fs.existsSync(local(target))) {
        files.push({path:target, present:false, omittedFromPinnedBaseArchive:true}); continue;
      }
      assert(expected, 'Unbound file outside base inventory: ' + file);
      const actual = await identity(target);
      assert.equal(actual.bytes, expected.size, file); assert.equal(actual.sha256, expected.sha256, file);
      // GPG key stores are never printed or copied into the materials report.
      const text = /\.(?:gpg|exe|dll|crt)$/i.test(file) ? '' : fs.readFileSync(local(actual.path), 'utf8');
      const lines = text.split(/\r?\n/), relevant = [];
      for (let i = 0; i < Math.min(lines.length, 150); i++) {
        if (/copyright|license|licence|public domain|redistribut|^.{0,4}\$Id:|^.{0,4}version|Provides(?:Expl)?Package/i.test(lines[i]))
          relevant.push({line:i + 1, text:lines[i].slice(0, 240)});
      }
      files.push({...actual, present:true, embeddedStatements:relevant});
    }
    runMaterials.push({component:name, revision:component.revision, catalogueLicense:component.license, files,
      note:'Hash-verified source-form run files and embedded statements; not blanket package licence clearance or a replacement for the missing locked documentation.'});
  }
  const binaryFiles = [];
  for (const expected of baseInventory.files.filter(f => /\.(?:exe|dll)$/i.test(f.path))) {
    const actual = await identity(base + '/TinyTeX/' + expected.path);
    assert.equal(actual.bytes, expected.size); assert.equal(actual.sha256, expected.sha256);
    const owners = [...installed.values()].filter(c => c.containers.run?.files.includes(expected.path)).map(c => ({name:c.name, revision:c.revision, license:c.license}));
    binaryFiles.push({...actual, owners, correspondingSourceVerified:false});
  }
  const metadataFile = base + '/metadata/yihui-texlive.tlpdb';
  const metadata = {expected:lock.metadata, localFile:metadataFile, available:fs.existsSync(local(metadataFile))};
  if (metadata.available) {
    metadata.actual = await identity(metadataFile);
    metadata.exact = metadata.actual.bytes === lock.metadata.size && metadata.actual.sha256 === lock.metadata.sha256;
  }
  const report = {schema:1, kind:'tex-material-closure-inventory', recordedAt:new Date().toISOString(),
    d04Complete:false, d05Complete:false, redistributionApproved:false, networkEnabled:false,
    script:await identity(relative(fileURLToPath(import.meta.url))), frozenInputs, archive, database, metadata,
    containers, partials, componentLicenses, runMaterials, binaryFiles,
    summary:{components:lock.components.length, baseComponents:lock.components.filter(c => c.base).length,
      additionalComponents:lock.components.filter(c => !c.base).length, containers:containers.length,
      exactContainers:containers.filter(c => c.status === 'exact').length,
      missingContainers:containers.filter(c => c.status === 'missing').length,
      mismatchedContainers:containers.filter(c => c.status === 'mismatch').length,
      missingBytes:containers.filter(c => c.status !== 'exact').reduce((n, c) => n + c.expectedBytes, 0),
      targetDeclaredRunFiles:runMaterials.reduce((n, c) => n + c.files.length, 0),
      targetPresentRunFiles:runMaterials.reduce((n, c) => n + c.files.filter(f => f.present).length, 0),
      targetOmittedRunFiles:runMaterials.flatMap(c => c.files).filter(f => !f.present).length, binaryFiles:binaryFiles.length,
      declaredLicenses:Object.fromEntries([...new Set(componentLicenses.map(c => c.license || 'not-declared'))].sort().map(l => [l,componentLicenses.filter(c => (c.license || 'not-declared') === l).length]))},
    limits:'A verified cache, package licence label or matching source candidate never grants redistribution approval. Binary build sources/patches, every component notice, immutable full metadata, and the formal pack lifecycle require a separate completed review.'};
  for (const input of [...frozenInputs, ...partials]) assert.deepEqual(await identity(input.path), input, input.path);
  writeJSON(path.join(out, 'inventory.json'), report);
  console.log(JSON.stringify({inventory:relative(path.join(out, 'inventory.json')), ...report.summary, metadataAvailable:metadata.available,
    d04Complete:false, d05Complete:false, redistributionApproved:false}));
  return report;
}

export async function sourceRecheck(manifestFile) {
  const manifest = readJSON(manifestFile);
  assert.equal(manifest.schema, 1); assert.equal(manifest.substitutesLockedContainers, false);
  assert(Array.isArray(manifest.files) && manifest.files.length > 0, 'An empty source manifest is not evidence');
  assert.equal(new Set(manifest.files.map(f => f.upstreamPath)).size, manifest.files.length, 'Duplicate source path');
  const component = readJSON(lockFile).components.find(c => c.name === manifest.component);
  assert(component, 'Unknown locked component'); assert.equal(manifest.lockedRevision, component.revision);
  const commit = readJSON(manifest.commitEvidence), tree = readJSON(manifest.treeEvidence);
  assert.match(manifest.commit, /^[a-f0-9]{40}$/); assert.equal(commit.sha, manifest.commit);
  assert.equal(commit.commit.tree.sha, tree.sha); assert.equal(tree.sha, manifest.treeSha1); assert.equal(tree.truncated, false);
  for (const file of manifest.files) verifyGitSource(fs.readFileSync(local(file.path)), file, tree);
  return {sourceIntegrity:true, component:manifest.component, commit:manifest.commit, files:manifest.files.length,
    substitutesLockedContainers:false, redistributionApproved:false};
}

export async function compareRuntime(planFile, output) {
  const out = newOutput(output), plan = readJSON(planFile), baseInventory = readJSON(base + '/inventory.json');
  assert.equal(plan.schema, 1); assert.equal(plan.substitutesLockedContainers, false);
  const databaseFile = base + '/TinyTeX/tlpkg/texlive.tlpdb', database = await identity(databaseFile);
  assert.equal(database.sha256, baseInventory.files.find(f => f.path === 'tlpkg/texlive.tlpdb').sha256);
  const installed = parseTlpdb(fs.readFileSync(local(databaseFile))), components = [], evidence = [await identity(planFile), database];
  for (const item of plan.components) {
    const locked = readJSON(lockFile).components.find(c => c.name === item.component), runtime = installed.get(item.component);
    assert(locked && runtime); assert.equal(locked.revision, runtime.revision); assert.equal(item.lockedRevision, locked.revision);
    const sourceFiles = [], sourceChecks = [];
    for (const file of item.sourceManifests) {
      sourceChecks.push(await sourceRecheck(file)); evidence.push(await identity(file));
      const manifest = readJSON(file); assert.equal(manifest.component, item.component);
      sourceFiles.push(...manifest.files);
    }
    assert.equal(new Set(sourceChecks.map(c => c.commit)).size, 1, 'All source files for a component must share one fixed commit');
    const sourceInputs = [];
    if (item.generatedDirectory) {
      for (const source of sourceFiles.filter(f => /\.(?:dtx|ins)$/.test(f.upstreamPath))) {
        const actual = await identity(item.generatedDirectory + '/' + path.posix.basename(source.upstreamPath));
        assert.equal(actual.sha256, source.sha256, 'Docstrip input must match the fixed source: ' + source.upstreamPath);
        sourceInputs.push(actual);
      }
      assert(sourceInputs.some(f => f.path.endsWith('.ins')), 'A fixed docstrip installer is required');
    }
    const files = [];
    for (const file of runtime.containers.run.files) {
      const expected = baseInventory.files.find(f => f.path === file);
      const runtimeFile = base + '/TinyTeX/' + file;
      if (!expected) {files.push({runtimeFile, presentInBaseArchive:false, exact:false}); continue;}
      const actual = await identity(runtimeFile);
      assert.equal(actual.bytes, expected.size); assert.equal(actual.sha256, expected.sha256);
      const generated = item.generatedDirectory && item.generatedDirectory + '/' + path.posix.basename(file);
      let candidate, origin;
      if (generated && fs.existsSync(local(generated))) {candidate = generated; origin = 'offline-docstrip';}
      else {
        const matches = sourceFiles.filter(f => path.posix.basename(f.upstreamPath) === path.posix.basename(file));
        assert(new Set(matches.map(f => f.sha256)).size <= 1, 'Ambiguous source-form run file: ' + file);
        if (matches.length) {candidate = matches[0].path; origin = 'fixed-upstream-file';}
      }
      const record = {runtime:actual, presentInBaseArchive:true, candidate:candidate ? await identity(candidate) : null, origin:origin || null, exact:false};
      if (candidate) Object.assign(record, compareBytes(fs.readFileSync(local(candidate)), fs.readFileSync(local(runtimeFile))));
      files.push(record);
    }
    let generation = null;
    if (item.generation) {
      generation = {...item.generation, sourceInputs, engine:await identity(item.generation.engine), console:await identity(item.generation.consoleLog), transcript:await identity(item.generation.transcriptLog)};
      const engineRelative = relative(local(item.generation.engine)).slice((base + '/TinyTeX/').length);
      const expectedEngine = baseInventory.files.find(f => f.path === engineRelative); assert(expectedEngine);
      assert.equal(generation.engine.sha256, expectedEngine.sha256); assert.equal(generation.engine.bytes, expectedEngine.size);
      assert.equal(generation.exitCode, 0); assert(item.generation.arguments.includes('-no-shell-escape'));
    }
    components.push({component:item.component, lockedRevision:locked.revision, commit:sourceChecks[0].commit, sourceChecks, generation, files,
      summary:{declaredRuntimeFiles:files.length, presentRuntimeFiles:files.filter(f => f.presentInBaseArchive).length,
        exactFiles:files.filter(f => f.exact).length, missingSourceFiles:files.filter(f => f.presentInBaseArchive && !f.candidate).length,
        mismatchedFiles:files.filter(f => f.candidate && !f.exact).length}});
  }
  const report = {schema:1, kind:'fixed-source-runtime-comparison', recordedAt:new Date().toISOString(),
    script:await identity(relative(fileURLToPath(import.meta.url))), evidence, components,
    substitutesLockedContainers:false, d04Complete:false, d05Complete:false, redistributionApproved:false,
    note:'Exact file correspondence is scoped to the listed package run files. Missing original container digests, compiled documentation, native binary sources/build inputs and pack lifecycle remain separate requirements.'};
  writeJSON(path.join(out, 'comparison.json'), report);
  console.log(JSON.stringify({comparison:relative(path.join(out, 'comparison.json')),components:components.map(c => ({component:c.component,...c.summary})),
    substitutesLockedContainers:false, d04Complete:false, d05Complete:false, redistributionApproved:false}));
  return report;
}

export async function recheck(manifestFile) {
  const report = readJSON(manifestFile);
  assert.equal(report.schema, 1); assert.equal(report.kind, 'tex-material-closure-inventory');
  assert.equal(report.d04Complete, false); assert.equal(report.d05Complete, false); assert.equal(report.redistributionApproved, false);
  const currentLock = readJSON(lockFile);
  const expectedNames = new Set(currentLock.components.flatMap(c => [...Object.values(c.containers), ...(c.unresolvedMaterials || [])].map(m => m.name)));
  assert.equal(report.containers.length, expectedNames.size);
  assert.deepEqual(new Set(report.containers.map(c => c.name)), expectedNames);
  let checkedFiles = 0;
  for (const entry of [report.script, ...report.frozenInputs, report.archive, report.database, ...report.partials,
    ...report.runMaterials.flatMap(m => m.files).filter(f => f.present), ...report.binaryFiles]) {
    const actual = await identity(entry.path);
    assert.equal(actual.bytes, entry.bytes, entry.path); assert.equal(actual.sha256, entry.sha256, entry.path); checkedFiles++;
  }
  for (const container of report.containers) {
    const component = currentLock.components.find(c => c.name === container.component);
    const expected = Object.values(component.containers).concat(component.unresolvedMaterials || []).find(c => c.name === container.name);
    assert.equal(container.expectedBytes, expected.size); assert.equal(container.expectedSha512, expected.sha512);
    if (container.status === 'exact') {
      const actual = await identity(container.actual.path, 'sha512');
      assert.equal(actual.bytes, expected.size); assert.equal(actual.sha512, expected.sha512); checkedFiles++;
    } else assert(['missing', 'mismatch'].includes(container.status));
  }
  for (const [key, expected] of Object.entries({containers:report.containers.length,
    exactContainers:report.containers.filter(c => c.status === 'exact').length,
    missingContainers:report.containers.filter(c => c.status === 'missing').length,
    mismatchedContainers:report.containers.filter(c => c.status === 'mismatch').length,
    missingBytes:report.containers.filter(c => c.status !== 'exact').reduce((n,c) => n + c.expectedBytes,0)}))
    assert.equal(report.summary[key], expected, 'Inventory summary: ' + key);
  return {auditIntegrity:true, checkedFiles, d04Complete:false, d05Complete:false, redistributionApproved:false};
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try {
    const args = process.argv.slice(2);
    if (args.length === 1 && ['--help','-h'].includes(args[0])) console.log(
      'Offline maintainer evidence only; never grants redistribution approval.\n' +
      '  node tools/test/tex-material-closure-evidence.mjs --inventory NEW_WORKSPACE_DIRECTORY\n' +
      '  node tools/test/tex-material-closure-evidence.mjs --recheck INVENTORY_JSON\n' +
      '  node tools/test/tex-material-closure-evidence.mjs --source-recheck SOURCE_MANIFEST_JSON\n' +
      '  node tools/test/tex-material-closure-evidence.mjs --compare-runtime SOURCE_PLAN_JSON NEW_WORKSPACE_DIRECTORY');
    else if (args.length === 2 && args[0] === '--inventory') await inventory(args[1]);
    else if (args.length === 2 && args[0] === '--recheck') console.log(JSON.stringify(await recheck(args[1])));
    else if (args.length === 2 && args[0] === '--source-recheck') console.log(JSON.stringify(await sourceRecheck(args[1])));
    else if (args.length === 3 && args[0] === '--compare-runtime') await compareRuntime(args[1], args[2]);
    else throw Error('Use --help for the explicit offline evidence commands');
  } catch (error) {console.error(error.stack); process.exitCode = 1;}
}
