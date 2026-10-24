// Explicit offline proposal only. Does not replace a lock, fetch or install.
import fs from 'node:fs';
import path from 'node:path';
import crypto from 'node:crypto';
import assert from 'node:assert/strict';
import {fileURLToPath} from 'node:url';
import {parseTlpdb, dependencyClosure} from './lib/tex-package-inputs.mjs';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const digest = bytes => crypto.createHash('sha256').update(bytes).digest('hex');
function local(value) {
  const file = path.resolve(root, value), rel = path.relative(root, file);
  assert(rel && rel !== '..' && !rel.startsWith('..' + path.sep) && !path.isAbsolute(rel), 'Path must stay in this workspace');
  let current = file;
  while (current !== root) {
    if (fs.existsSync(current)) assert(!fs.lstatSync(current).isSymbolicLink(), 'Linked proposal path');
    current = path.dirname(current);
  }
  return file;
}

export function selectUnifiedClosure(metadataBytes, expectedSha256, prior, installed) {
  assert.match(expectedSha256, /^[a-f0-9]{64}$/);
  assert.equal(digest(metadataBytes), expectedSha256, 'Unreviewed package database');
  const available = parseTlpdb(metadataBytes);
  // Seed every prior base package, then resolve all dependencies solely from
  // the new database. The old installed map must never short-circuit selection.
  const seeds = [...new Set([...prior.components.filter(c => c.base).map(c => c.name), ...prior.roots])];
  const components = dependencyClosure(available, new Map(), seeds, 'windows');
  for (const component of components) assert.match(component.revision, /^\d+$/, 'Explicit TeX revision required');
  const changes = components.map(next => {
    const old = prior.components.find(c => c.name === next.name);
    const oldContainers = old?.base ? {...installed.get(next.name)?.containers, ...old.containers} : old?.containers;
    return {name:next.name, oldRevision:old?.revision || null, newRevision:next.revision,
      containers:Object.entries(next.containers).map(([kind, value]) => ({kind,
        oldSha512:oldContainers?.[kind]?.sha512 || old?.unresolvedMaterials?.find(c => c.kind === kind)?.sha512 || null,
        newSha512:value.sha512, newBytes:value.size}))};
  });
  return {seeds, components, changes};
}

export function createProposal(options) {
  const metadataFile = local(options.metadata), stat = fs.lstatSync(metadataFile);
  assert(stat.isFile() && stat.size > 0 && stat.size <= 64 * 1048576, 'Metadata size budget');
  const metadataBytes = fs.readFileSync(metadataFile);
  const priorFile = local('tools/tex-publication-inputs.lock.json'), priorBytes = fs.readFileSync(priorFile), prior = JSON.parse(priorBytes);
  const installedFile = local('.cache/capability-upgrade/tex-inputs/TinyTeX/tlpkg/texlive.tlpdb');
  const installedBytes = fs.readFileSync(installedFile);
  const inventory = JSON.parse(fs.readFileSync(local('.cache/capability-upgrade/tex-inputs/inventory.json')));
  assert.equal(digest(installedBytes), inventory.files.find(f => f.path === 'tlpkg/texlive.tlpdb').sha256);
  assert.equal(prior.redistributionApproved, false);
  const selection = selectUnifiedClosure(metadataBytes, options.sha256, prior, parseTlpdb(installedBytes));
  const repository = new URL(options.repository);
  assert(repository.protocol === 'https:' && !repository.username && !repository.password && repository.pathname.endsWith('/archive/') && !repository.search && !repository.hash);
  assert(['ctan.math.illinois.edu','ftp.math.utah.edu','texlive.info'].includes(repository.hostname), 'Explicit official TeX repository required');
  const containers = selection.components.flatMap(c => Object.entries(c.containers).map(([kind, value]) => ({component:c.name, revision:c.revision,
    kind, ...value, url:new URL(value.name.replace('.tar.xz', '.r' + c.revision + '.tar.xz'), repository).href, acquired:false})));
  const out = local(options.out); fs.mkdirSync(out);
  const proposal = {schema:1, kind:'unified-tex-material-proposal', recordedAt:new Date().toISOString(), networkEnabled:false,
    platform:'windows-x64', redistributionApproved:false, d04Complete:false, d05Complete:false,
    metadata:{path:path.relative(root, metadataFile).replaceAll('\\','/'), bytes:metadataBytes.length, sha256:options.sha256},
    priorLock:{path:'tools/tex-publication-inputs.lock.json', sha256:digest(priorBytes), keptUnchanged:true},
    repository:repository.href, seeds:selection.seeds, components:selection.components,
    summary:{components:selection.components.length, containers:containers.length,
      compressedBytesByKind:Object.fromEntries(['run','source','doc'].map(kind => [kind,containers.filter(c => c.kind === kind).reduce((n,c) => n + c.size,0)]))},
    requiredBeforeApproval:['Acquire every selected run/source/doc container using this one database and verify its exact length/SHA-512 and declared file set.',
      'Map every native binary, interpreter, bundled DLL and embedded third-party payload to complete matching build sources, patches, scripts, licences and notices.',
      'Build the new runtime solely from these selected containers in a fresh maintainer directory; never layer new documentation onto the old TinyTeX base.',
      'Measure expanded/staging/rollback/same-volume peak space, then verify offline install, repair, upgrade, rollback, cancellation and native publication.',
      'Review the completed immutable manifest and lifecycle evidence separately before changing approval or the application catalogue.']};
  for (const [name, value] of [['proposal.json',proposal],['changes.json',selection.changes],['acquisition-plan.json',containers]])
    fs.writeFileSync(path.join(out,name), JSON.stringify(value,null,2) + '\n', {flag:'wx'});
  assert.equal(digest(fs.readFileSync(priorFile)), digest(priorBytes));
  return {proposal:path.relative(root,path.join(out,'proposal.json')).replaceAll('\\','/'),...proposal.summary,redistributionApproved:false};
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try {
    const args = process.argv.slice(2);
    if (args.length === 1 && ['--help','-h'].includes(args[0])) console.log(
      'Offline unified TeX proposal. Never fetches, installs, approves or replaces old locks.\n' +
      'node tools/plan-tex-material-closure.mjs --metadata WORKSPACE_TLPDB --sha256 REVIEWED_SHA256 --repository HTTPS_ARCHIVE_URL --out NEW_WORKSPACE_DIRECTORY');
    else {
      const options = {};
      for (let i = 0; i < args.length; i += 2) {assert(['--metadata','--sha256','--repository','--out'].includes(args[i]));assert(args[i + 1]);options[args[i].slice(2)] = args[i + 1];}
      for (const key of ['metadata','sha256','repository','out']) assert(options[key], 'All explicit proposal inputs are required');
      console.log(JSON.stringify(createProposal(options)));
    }
  } catch (error) {console.error(error.stack); process.exitCode = 1;}
}
