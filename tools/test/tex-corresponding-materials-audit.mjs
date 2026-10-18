// Maintainer QA only. Never changes a TeX lock, an input partial, a capability
// catalog or redistribution approval. Network access requires --download.
import fs from 'node:fs';
import path from 'node:path';
import crypto from 'node:crypto';
import assert from 'node:assert/strict';
import {fileURLToPath} from 'node:url';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');
const lockPath = 'tools/tex-publication-inputs.lock.json';
const priorPath = 'docs/evidence/capability-upgrade/2026-10-10/batch3-tex-publication-inputs.json';
const cache = '.cache/capability-upgrade/tex-inputs/containers';
const tagpdfCommit = '3b0bd42134023279685ec2b7c0c24f9b5cd00a19';
const xzMagic = Buffer.from('fd377a585a00', 'hex');
const hosts = new Set(['ctan.math.illinois.edu', 'texlive.info', 'api.github.com', 'raw.githubusercontent.com']);
const limits = Object.freeze({requests:48, responseBytes:20 * 1048576, singleBytes:6 * 1048576, requestMs:15000, overallMs:180000, concurrency:3});
const hash = (b, algorithm = 'sha256') => crypto.createHash(algorithm).update(b).digest('hex');
const relative = f => path.relative(root, f).replaceAll('\\', '/');

function localPath(value) {
  const resolved = path.resolve(root, value), rel = path.relative(root, resolved);
  if (!rel || rel === '..' || rel.startsWith('..' + path.sep) || path.isAbsolute(rel)) throw Error('Audit path must be a child of the workspace');
  return resolved;
}
function readFile(file, max = 8 * 1048576) {
  const stat = fs.lstatSync(file);
  if (!stat.isFile() || stat.isSymbolicLink() || stat.size > max) throw Error('Unexpected audit input: ' + relative(file));
  return fs.readFileSync(file);
}
function identity(file, max) {
  const bytes = readFile(file, max);
  return {path:relative(file), bytes:bytes.length, sha256:hash(bytes), sha512:hash(bytes, 'sha512')};
}
export function verifyContainer(bytes, expected) {
  return {lengthMatches:bytes.length === expected.size, digestMatches:hash(bytes, 'sha512') === expected.sha512,
    xzHeader:bytes.subarray(0, 6).equals(xzMagic), exact:bytes.length === expected.size && hash(bytes, 'sha512') === expected.sha512 && bytes.subarray(0, 6).equals(xzMagic)};
}
export function gitBlobSha1(bytes) {
  return hash(Buffer.concat([Buffer.from('blob ' + bytes.length + '\0'), bytes]), 'sha1');
}
function inputs() {
  const lock = JSON.parse(readFile(localPath(lockPath))), prior = JSON.parse(readFile(localPath(priorPath)));
  assert.equal(lock.redistributionApproved, false); assert.equal(prior.redistributionApproved, false);
  const targets = [];
  for (const name of ['pdfmanagement', 'tagpdf']) {
    const component = lock.components.find(c => c.name === name);
    for (const kind of ['source', 'doc']) targets.push({component:name, revision:component.revision, license:component.license, kind, ...component.containers[kind]});
  }
  for (const record of prior.unresolvedOlderMaterialRecords) {
    const component = lock.components.find(c => c.name === record.name);
    for (const material of record.materials) {
      const locked = component.unresolvedMaterials.find(m => m.kind === material.kind);
      assert.equal(locked.sha512, material.sha512); assert.equal(locked.size, material.size);
      targets.push({component:component.name, revision:component.revision, license:component.license, ...locked});
    }
  }
  assert.equal(targets.length, 8);
  for (const t of targets) {
    assert.match(t.name, /^[a-z0-9][a-z0-9_.-]*\.tar\.xz$/);
    assert.match(t.sha512, /^[a-f0-9]{128}$/); assert(Number.isSafeInteger(t.size) && t.size > 0 && t.size <= limits.singleBytes);
  }
  return {lock, prior, targets};
}
export function recheck(reportPath) {
  const report = JSON.parse(readFile(localPath(reportPath))), {targets} = inputs(), checks = [];
  assert.equal(report.schema, 1); assert.equal(report.d04Complete, false); assert.equal(report.redistributionApproved, false);
  for (const file of [...report.frozenInputs, ...report.localInputs]) {
    const actual = identity(localPath(file.path)); assert.equal(actual.sha256, file.sha256, file.path); assert.equal(actual.bytes, file.bytes, file.path); checks.push(file.path);
  }
  for (const attempt of report.attempts.filter(a => a.savedAs)) {
    const actual = identity(localPath(attempt.savedAs)); assert.equal(actual.sha256, attempt.sha256, attempt.savedAs); assert.equal(actual.bytes, attempt.bytes, attempt.savedAs); checks.push(attempt.savedAs);
  }
  for (const item of report.materials.filter(m => m.exactFile)) {
    const expected = targets.find(t => t.name === item.name); assert(expected);
    assert.equal(verifyContainer(readFile(localPath(item.exactFile)), expected).exact, true, item.name);
  }
  for (const candidate of report.sourceCandidates) {
    if (!candidate.files.length) continue;
    const commit = JSON.parse(readFile(localPath(candidate.commitEvidence))), tree = JSON.parse(readFile(localPath(candidate.treeEvidence)));
    assert.equal(commit.sha, tagpdfCommit); assert.equal(commit.commit.tree.sha, tree.sha); assert.equal(tree.truncated, false);
    for (const file of candidate.files) {
      const blob = tree.tree.find(t => t.type === 'blob' && t.path === file.upstreamPath); assert(blob);
      const bytes = readFile(localPath(file.path)); assert.equal(gitBlobSha1(bytes), blob.sha, file.path); assert.equal(blob.sha, file.gitBlobSha1, file.path); assert.equal(bytes.length, blob.size); assert.equal(hash(bytes), file.sha256, file.path);
    }
  }
  return {auditIntegrity:true, checkedFiles:checks.length, exactContainers:report.materials.filter(m => m.exactFile).length, d04Complete:false, redistributionApproved:false};
}

async function audit(options) {
  const {lock, prior, targets} = inputs(), out = localPath(options.out);
  fs.mkdirSync(out); // Existing evidence is never overwritten.
  const artifacts = path.join(out, 'artifacts'); fs.mkdirSync(artifacts);
  const frozenInputs = [lockPath, 'tools/tex-runtime-inputs.lock.json', priorPath].map(f => identity(localPath(f)));
  const localInputs = [], materials = targets.map(t => {
    const item = {...t, status:'missing-exact-container', exactFile:null, partial:null, attempts:[]};
    for (const suffix of ['', '.partial']) {
      const file = localPath(cache + '/' + t.name + suffix);
      if (!fs.existsSync(file)) continue;
      const bytes = readFile(file), record = identity(file); localInputs.push(record);
      const verified = verifyContainer(bytes, t);
      if (!suffix && verified.exact) {item.status = 'verified-exact-container'; item.exactFile = record.path;}
      else if (suffix) item.partial = {...record, remainingBytes:t.size - bytes.length, ...verified, usableForResume:false};
      else item.localMismatch = {...record, ...verified};
    }
    return item;
  });
  const report = {schema:1, recordedAt:new Date().toISOString(), auditCompleted:false, d04Complete:false, redistributionApproved:false,
    scope:'D04 eight small locked package materials and a separately classified upstream source candidate; no engine download, build, license clearance or release approval.',
    script:identity(fileURLToPath(import.meta.url)), frozenInputs, localInputs, limits, networkEnabled:options.download,
    originalMaterialsAudit:prior.materialsAudit, originalAuditReexecuted:false, materials, attempts:[], sourceCandidates:[],
    binaryCorrespondingSources:{verified:false, reason:'Package .dtx candidates do not establish the exact TeX Live Windows engine, bundled DLL, tool and build-source correspondence.'},
    upgradePlan:[
      'Keep the original TinyTeX archive, locks, partials and this incomplete audit immutable; acquire old containers only when both length and the original SHA-512 match.',
      'If old material remains unavailable, select one complete dated TeX Live/TinyTeX base and matching package database, pin its complete declared CTeX dependency closure and all run/source/doc containers together in a new proposed lock.',
      'Record every old/new revision, source URL, container and expanded-file digest, license, notice and matching engine/DLL/tool build-source reference; do not treat an equal version string or current mutable CTAN URL as correspondence.',
      'Review binary build inputs, patches, scripts and third-party notices; retain source candidates as candidates until correspondence is demonstrated. Null license records require individual review.',
      'Measure compressed, expanded, staging, rollback and peak same-volume sizes for that new closure, then perform offline native compile, install, repair, upgrade, rollback, cancellation and interoperability checks against it.',
      'Only a separate completed review may change redistribution approval or enroll a new immutable formal pack. This audit changes neither.'
    ]};
  const started = Date.now(); let requests = 0, receivedBytes = 0;
  async function request(url, {method = 'GET', cap = 256 * 1024, format = 'text'} = {}) {
    const attempt = {id:report.attempts.length + 1, url, method, recordedAt:new Date().toISOString(), outcome:'not-requested'};
    report.attempts.push(attempt);
    let body = Buffer.alloc(0);
    try {
      const u = new URL(url);
      if (u.protocol !== 'https:' || u.username || u.password || !hosts.has(u.hostname)) throw Error('URL outside the audit allowlist');
      if (requests >= limits.requests || Date.now() - started >= limits.overallMs || receivedBytes >= limits.responseBytes) throw Error('Overall audit network budget exhausted');
      requests++;
      const response = await fetch(u, {method, redirect:'manual', signal:AbortSignal.timeout(Math.min(limits.requestMs, limits.overallMs - (Date.now() - started))), headers:{'User-Agent':'ReadMD-maintainer-materials-audit', 'Accept-Encoding':'identity'}});
      attempt.status = response.status;
      attempt.headers = Object.fromEntries(['content-length', 'content-type', 'content-encoding', 'last-modified', 'etag', 'date', 'location'].map(k => [k, response.headers.get(k)]).filter(([, v]) => v !== null));
      if (response.status !== 200 || method === 'HEAD') {await response.body?.cancel(); attempt.outcome = response.status === 200 ? 'headers-only' : 'http-unavailable'; return {attempt, body};}
      const html = /text\/html/i.test(response.headers.get('content-type') || '');
      const limit = Math.min(cap, limits.singleBytes, html ? 4096 : limits.singleBytes);
      const chunks = []; let length = 0;
      try {
        for await (const chunk of response.body) {
          receivedBytes += chunk.length;
          const available = Math.max(0, limit - length);
          chunks.push(chunk.subarray(0, available)); length += Math.min(chunk.length, available);
          if (receivedBytes > limits.responseBytes || chunk.length > available) throw Error('Response byte budget exceeded');
          if (format === 'xz' && length >= 6 && !Buffer.concat(chunks).subarray(0, 6).equals(xzMagic)) throw Error('Response is not an XZ archive (HTML/error responses are never input materials)');
        }
      } finally {body = Buffer.concat(chunks);}
      attempt.outcome = 'body-received';
    } catch (error) {attempt.error = error.message; attempt.outcome = 'rejected-or-unavailable';}
    if (body.length) {
      const file = path.join(artifacts, 'attempt-' + String(attempt.id).padStart(3, '0') + '.body');
      fs.writeFileSync(file, body, {flag:'wx'}); Object.assign(attempt, {savedAs:relative(file), bytes:body.length, sha256:hash(body), sha512:hash(body, 'sha512')});
    }
    return {attempt, body};
  }
  async function exactDownload(item, url) {
    const {attempt, body} = await request(url, {cap:item.size + 1, format:'xz'}); item.attempts.push(attempt.id);
    if (attempt.status === 200 && !attempt.error) {
      attempt.verification = verifyContainer(body, item);
      if (attempt.verification.exact) {item.status = 'verified-exact-container'; item.exactFile = attempt.savedAs; attempt.outcome = 'verified-exact-locked-container';}
      else attempt.outcome = 'locked-size-or-digest-mismatch';
    }
  }
  if (options.download) {
    for (let index = 0; index < materials.length; index += limits.concurrency) {
      const results = await Promise.allSettled(materials.slice(index, index + limits.concurrency).map(async item => {
        if (item.exactFile) return;
        const current = new URL(item.name, lock.archiveOrigin).href;
        const head = await request(current, {method:'HEAD'}); item.attempts.push(head.attempt.id);
        if (head.attempt.status === 200 && Number(head.attempt.headers?.['content-length']) === item.size) await exactDownload(item, current);
        else if (head.attempt.status === 200) head.attempt.outcome = 'remote-length-differs-from-old-lock';
        if (!item.exactFile) {
          const filename = item.name.replace('.tar.xz', '.r' + item.revision + '.tar.xz');
          await exactDownload(item, 'https://texlive.info/tlnet-archive/' + options.snapshot.replaceAll('-', '/') + '/tlnet/archive/' + filename);
        }
      }));
      for (const result of results) if (result.status === 'rejected') report.attempts.push({outcome:'audit-worker-error', error:result.reason.message});
      console.log(JSON.stringify({auditedMaterials:Math.min(index + limits.concurrency, materials.length), total:materials.length, exact:materials.filter(m => m.exactFile).length}));
    }
    if (options.tagpdfSource) {
      const candidate = {component:'tagpdf', version:'1.0g', commit:tagpdfCommit, repository:'https://github.com/latex3/tagpdf', classification:'upstream-corresponding-source-candidate', substitutesLockedContainer:false, completeLockedSourceFileSet:false, files:[]};
      report.sourceCandidates.push(candidate);
      const commitResult = await request('https://api.github.com/repos/latex3/tagpdf/commits/' + tagpdfCommit);
      try {
        assert.equal(commitResult.attempt.status, 200); assert(!commitResult.attempt.error);
        const commit = JSON.parse(commitResult.body); assert.equal(commit.sha, tagpdfCommit); assert.match(commit.commit.tree.sha, /^[a-f0-9]{40}$/);
        // Query the explicit tree identity from the fixed commit. A tree API
        // response addressed by a commitish may echo that commitish as .sha.
        const treeResult = await request('https://api.github.com/repos/latex3/tagpdf/git/trees/' + commit.commit.tree.sha + '?recursive=1');
        assert.equal(treeResult.attempt.status, 200); assert(!treeResult.attempt.error);
        const tree = JSON.parse(treeResult.body);
        assert.equal(commit.commit.tree.sha, tree.sha); assert.equal(tree.truncated, false);
        candidate.commitEvidence = commitResult.attempt.savedAs; candidate.treeEvidence = treeResult.attempt.savedAs;
        candidate.treeSha1 = tree.sha; candidate.commitDate = commit.commit.committer.date;
        const component = targets.find(t => t.component === 'tagpdf' && t.kind === 'source');
        const names = [...component.files.map(f => path.posix.basename(f)), 'doc/CTANREADME.md'];
        for (let index = 0; index < names.length; index += limits.concurrency) {
          const results = await Promise.allSettled(names.slice(index, index + limits.concurrency).map(async name => {
            const blob = tree.tree.find(t => t.path === name && t.type === 'blob'); assert(blob && blob.size <= 256 * 1024, name);
            const downloaded = await request('https://raw.githubusercontent.com/latex3/tagpdf/' + tagpdfCommit + '/' + name, {cap:blob.size});
            assert.equal(downloaded.attempt.status, 200, name); assert(!downloaded.attempt.error, name); assert.equal(downloaded.body.length, blob.size, name); assert.equal(gitBlobSha1(downloaded.body), blob.sha, name);
            candidate.files.push({upstreamPath:name, path:downloaded.attempt.savedAs, bytes:blob.size, gitBlobSha1:blob.sha, sha256:hash(downloaded.body)});
          }));
          for (const result of results) if (result.status === 'rejected') (candidate.errors ||= []).push(result.reason.message);
        }
        candidate.completeLockedSourceFileSet = component.files.every(f => candidate.files.some(s => s.upstreamPath === path.posix.basename(f)));
        candidate.note = 'Git blob identity binds these files to the fixed upstream commit. No original TeX Live tar SHA-512, generated runtime comparison, compiled documentation or engine source correspondence is inferred.';
      } catch (error) {candidate.error = error.message;}
    }
  }
  for (const original of [...frozenInputs, ...localInputs]) assert.equal(identity(localPath(original.path)).sha256, original.sha256, 'Original input changed: ' + original.path);
  report.auditCompleted = true;
  report.network = {requests, receivedBytes, elapsedMs:Date.now() - started};
  report.summary = {exactContainers:materials.filter(m => m.exactFile).length, missingExactContainers:materials.filter(m => !m.exactFile).length, totalLockedMaterialBytes:materials.reduce((n, m) => n + m.size, 0), originalInputsUnchanged:true};
  report.finishedAt = new Date().toISOString();
  fs.writeFileSync(path.join(out, 'audit.json'), JSON.stringify(report, null, 2) + '\n', {flag:'wx'});
  console.log(JSON.stringify({audit:relative(path.join(out, 'audit.json')), auditCompleted:true, ...report.summary, d04Complete:false, redistributionApproved:false}));
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const args = process.argv.slice(2);
  try {
    if (args.length === 1 && ['--help', '-h'].includes(args[0])) console.log(
      'Maintainer-only D04 material audit (never grants redistribution approval).\n' +
      '  node tools/test/tex-corresponding-materials-audit.mjs --out NEW_WORKSPACE_DIRECTORY\n' +
      '  node tools/test/tex-corresponding-materials-audit.mjs --out NEW_WORKSPACE_DIRECTORY --download [--tagpdf-source] [--snapshot YYYY-MM-DD]\n' +
      '  node tools/test/tex-corresponding-materials-audit.mjs --recheck AUDIT_JSON\n' +
      'Offline is the default. Output must not exist. Network runs are bounded to 48 requests,\n' +
      '20 MiB received, 6 MiB per response, 15 seconds per request and 180 seconds overall.\n' +
      'Existing locks, partials, input files and approval flags remain unchanged.\n' +
      'Exit 0 means the audit/recheck completed; audit.json separately reports missing materials\n' +
      'and keeps d04Complete=false. A source candidate never replaces a locked container.');
    else if (args[0] === '--recheck' && args.length === 2) console.log(JSON.stringify(recheck(args[1])));
    else {
      const options = {download:false, tagpdfSource:false, snapshot:'2026-10-01'};
      for (let i = 0; i < args.length; i++) {
        if (args[i] === '--out') options.out = args[++i];
        else if (args[i] === '--download') options.download = true;
        else if (args[i] === '--tagpdf-source') options.tagpdfSource = true;
        else if (args[i] === '--snapshot') options.snapshot = args[++i];
        else throw Error('Unknown argument: ' + args[i]);
      }
      if (!options.out || !/^20\d{2}-\d{2}-\d{2}$/.test(options.snapshot) || options.tagpdfSource && !options.download) throw Error('Usage: node tools/test/tex-corresponding-materials-audit.mjs --out NEW_WORKSPACE_DIRECTORY [--download] [--tagpdf-source] [--snapshot YYYY-MM-DD], or --recheck AUDIT_JSON');
      await audit(options);
    }
  } catch (error) {console.error(error.message); process.exitCode = 1;}
}
