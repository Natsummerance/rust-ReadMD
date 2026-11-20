// A deliberately version-bound refresh: V0.0.7 stays at its original commit.
// Every new download is authenticated before the first public name changes.
import fs from 'node:fs';
import path from 'node:path';
import crypto from 'node:crypto';
import {PACKAGE_NAMES, parseChecksums} from './publish-release.mjs';

export const FIXED_TAG = 'V0.0.7';
export const FIXED_TAG_COMMIT = '76c7e22921fdfb926c5a0c678d65e0dc666d5e2e';
export const FIXED_RELEASE_ID = 451629011;
export const DOWNLOAD_NAMES = [...PACKAGE_NAMES, 'SHA256SUMS.txt'];
const base = 'repos/Natsummerance/rust-ReadMD';
const hash = file => crypto.createHash('sha256').update(fs.readFileSync(file)).digest('hex');
const authentic = (asset, file) => asset?.state === 'uploaded' && asset.size === file.size && asset.digest === 'sha256:' + file.sha256;
const unchanged = (asset, old) => asset?.id === old.id && asset.name === old.name && asset.size === old.size && asset.digest === old.digest && asset.state === 'uploaded';

export function assertCandidateRun(run, jobs, sourceCommit) {
  if (run.head_sha !== sourceCommit || !['main', 'codex/v007-export-layout-publish-20261120', 'codex/v007-pet-recovery-publish-20261120'].includes(run.head_branch) || run.status !== 'completed' || run.conclusion !== 'success'
      || run.event !== 'workflow_dispatch' || run.path !== '.github/workflows/release.yml'
      || run.repository?.full_name !== 'Natsummerance/rust-ReadMD') throw Error('The exact release.yml build must have passed');
  const required = ['Build · Windows x64', 'Build · Linux x64', 'Build · macOS ARM64', 'Build · macOS Intel',
    'Kernel tests · windows-latest', 'Kernel tests · ubuntu-latest', 'Kernel tests · macos-latest',
    'Frontend gates', 'UI quality (Playwright)', 'VS Code and MCP quality',
    'Owned small volume · ubuntu-latest', 'Owned small volume · windows-latest'];
  for (const name of required) {
    const latest = jobs.filter(job => job.name === name || job.name.endsWith(' / ' + name)).sort((a, b) => b.id - a.id)[0];
    if (latest?.conclusion !== 'success') throw Error('Required latest job has not passed: ' + name);
    if (['Build · Windows x64', 'Build · Linux x64'].includes(name)) {
      for (const step of ['P0 lifecycle against the actual Windows or Linux release binary', 'Verify platform image exports and installed PDFium'])
        if (!latest.steps?.some(item => item.name === step && item.conclusion === 'success')) throw Error('Required release-binary check has not passed: ' + name + ' / ' + step);
    }
  }
}

export async function refreshV007({directory, sourceCommit, runId, notes, verification, request, verifyDownload, checkpoint = () => {}}) {
  if (!/^[a-f\d]{40}$/.test(sourceCommit) || !Number.isSafeInteger(runId) || runId <= 0
      || verification?.sourceCommit !== sourceCommit || verification.runId !== runId
      || !/^[a-f\d]{40}$/.test(verification.publicationCommit)
      || typeof verifyDownload !== 'function' || !notes.includes(sourceCommit) || !notes.includes(FIXED_TAG_COMMIT)) throw Error('Bound CI verification and explicit source/tag notes are required');
  if (JSON.stringify(fs.readdirSync(directory).sort()) !== JSON.stringify([...DOWNLOAD_NAMES].sort())) throw Error('Exactly ten packages and their checksum manifest are required');
  const sums = parseChecksums(fs.readFileSync(path.join(directory, 'SHA256SUMS.txt'), 'utf8'));
  const files = DOWNLOAD_NAMES.map(name => {
    const file = path.join(directory, name);
    if (!fs.lstatSync(file).isFile()) throw Error('Expected regular release files');
    const item = {name, file, size: fs.statSync(file).size, sha256: hash(file)};
    if (!item.size || (sums.has(name) && sums.get(name) !== item.sha256)) throw Error('Local release checksum mismatch');
    if (name !== 'SHA256SUMS.txt') {
      const proof = verification.packages?.find(proof => proof.name === name);
      if (!proof || proof.sha256 !== item.sha256 || proof.size !== item.size || proof.artifactDigestVerified !== true || proof.sourceInspectionVerified !== true) throw Error('Package is not bound to an authenticated and source-inspected CI artifact');
    }
    return item;
  });
  const call = request, assetRoute = id => base + '/releases/assets/' + id;
  const assets = async () => {
    const result = await call('GET', base + '/releases/' + FIXED_RELEASE_ID + '/assets?per_page=100');
    if (result.length >= 100) throw Error('Unexpected release asset inventory');
    return result;
  };
  async function checkTag() {
    const ref = await call('GET', base + '/git/ref/tags/' + FIXED_TAG);
    if (ref.object?.type !== 'commit' || ref.object.sha !== FIXED_TAG_COMMIT) throw Error('The fixed original release tag changed');
    const main = await call('GET', base + '/git/ref/heads/main');
    if (main.object?.type !== 'commit' || main.object.sha !== verification.publicationCommit) throw Error('Public main changed since product/source inspection');
  }
  const release = await call('GET', base + '/releases/tags/' + FIXED_TAG);
  if (release.id !== FIXED_RELEASE_ID || release.tag_name !== FIXED_TAG || release.draft || release.immutable) throw Error('Unexpected existing release');
  await checkTag();
  if ((await call('GET', base + '/commits/' + sourceCommit)).sha !== sourceCommit) throw Error('Candidate source is not public');
  const main = await call('GET', base + '/compare/' + sourceCommit + '...main');
  if (!['identical', 'ahead'].includes(main.status)) throw Error('The built source must be integrated into public main before replacing downloads');
  const run = await call('GET', base + '/actions/runs/' + runId), inventory = await call('GET', base + '/actions/runs/' + runId + '/jobs?filter=all&per_page=100');
  if (inventory.total_count > 100) throw Error('Job history needs pagination review');
  assertCandidateRun(run, inventory.jobs, sourceCommit);
  const prefix = 'v007-refresh-' + sourceCommit.slice(0, 12) + '-';
  const journal = {releaseId: FIXED_RELEASE_ID, tag: FIXED_TAG, tagCommit: FIXED_TAG_COMMIT, sourceCommit, publicationCommit: verification.publicationCommit, runId, status: 'inspecting', replacements: []};
  let current = await assets();
  const previousName = asset => {
    const match = new RegExp('^' + prefix + 'previous-(\\d+)-(.+)$').exec(asset.name);
    return match && Number(match[1]) === asset.id && DOWNLOAD_NAMES.includes(match[2]) ? match[2] : null;
  };
  const allowed = asset => DOWNLOAD_NAMES.includes(asset.name) || DOWNLOAD_NAMES.some(file => asset.name === prefix + file) || previousName(asset);
  if (current.some(asset => !allowed(asset))) throw Error('Unrelated or another refresh asset requires inspection');
  const inspectedRecoverables = new Map(current.filter(asset => previousName(asset)).map(asset => [asset.id, asset]));
  for (const name of DOWNLOAD_NAMES) if (current.filter(asset => asset.name === name).length > 1) throw Error('Ambiguous public download');

  async function verifyFinal() {
    const final = await assets();
    for (const file of files) {
      const asset = final.find(asset => asset.name === file.name);
      if (!authentic(asset, file)) throw Error('Final release digest mismatch: ' + file.name);
      const downloaded = await verifyDownload(asset, file);
      if (downloaded.size !== file.size || downloaded.sha256 !== file.sha256) throw Error('Public download bytes differ: ' + file.name);
    }
    await checkTag();
    return final;
  }
  async function publishNotes() {
    await call('PATCH', base + '/releases/' + FIXED_RELEASE_ID, {body: notes});
    const result = await call('GET', base + '/releases/tags/' + FIXED_TAG);
    if (result.body !== notes || result.id !== FIXED_RELEASE_ID || result.tag_name !== FIXED_TAG) throw Error('Release notes verification failed');
  }
  async function cleanup() {
    journal.retainedBackups = [];
    // The known old identities are retained until the durable verified receipt.
    for (const asset of await assets()) {
      if (!asset.name.startsWith(prefix)) continue;
      try {
        const candidate = files.find(file => asset.name === prefix + file.name);
        const old = journal.originalAssets?.find(old => old.id === asset.id) || inspectedRecoverables.get(asset.id);
        if (!(candidate && authentic(asset, candidate)) && !(previousName(asset) && old?.size === asset.size && old?.digest === asset.digest)) throw Error('Recovery asset changed before cleanup');
        await call('DELETE', assetRoute(asset.id));
      }
      catch { journal.retainedBackups.push(asset.id); }
    }
    const final = await assets();
    if (!journal.retainedBackups.length && (final.length !== files.length || files.some(file => !authentic(final.find(asset => asset.name === file.name), file)))) throw Error('Final inventory must contain exactly eleven authenticated downloads');
    journal.assets = final.filter(asset => DOWNLOAD_NAMES.includes(asset.name)).map(({id, name, size, digest, browser_download_url}) => ({id, name, size, digest, url: browser_download_url}));
    journal.status = journal.retainedBackups.length ? 'verified_with_retained_backups' : 'complete';
    await checkpoint(journal);
    return journal;
  }

  // A killed job may have completed every switch or stopped while deleting old
  // copies. Reauthenticate all downloads before resuming its finalization.
  if (files.every(file => authentic(current.find(asset => asset.name === file.name), file))) {
    await verifyFinal(); await publishNotes();
    journal.status = 'verified'; await checkpoint(journal);
    return cleanup();
  }
  // Recover an interrupted same-source switch using only our deterministic old
  // asset names. A different source is refused above, never guessed at.
  for (const old of current.filter(asset => asset.name.startsWith(prefix + 'previous-'))) {
    const match = new RegExp('^' + prefix + 'previous-(\\d+)-(.+)$').exec(old.name);
    if (!match || Number(match[1]) !== old.id || !DOWNLOAD_NAMES.includes(match[2])) throw Error('Invalid previous asset identity');
    const name = match[2], file = files.find(file => file.name === name);
    const present = current.find(asset => asset.name === name), staged = current.find(asset => asset.name === prefix + name);
    if (present) {
      if (!authentic(present, file) || staged) throw Error('Interrupted refresh has conflicting assets');
      await call('PATCH', assetRoute(present.id), {name: prefix + name});
    }
    await call('PATCH', assetRoute(old.id), {name});
    current = await assets();
  }
  const original = DOWNLOAD_NAMES.map(name => current.find(asset => asset.name === name));
  if (original.some(asset => !asset || asset.state !== 'uploaded' || !/^sha256:[a-f\d]{64}$/.test(asset.digest))) throw Error('Original eleven-download inventory is incomplete');
  journal.originalAssets = original.map(({id, name, size, digest}) => ({id, name, size, digest}));
  journal.originalBody = release.body;
  journal.status = 'staging'; await checkpoint(journal);
  const staged = new Map();
  for (const file of files) {
    let candidate = (await assets()).find(asset => asset.name === prefix + file.name);
    if (candidate && !authentic(candidate, file)) throw Error('Existing same-source candidate has different bytes');
    if (!candidate) candidate = await call('POST', release.upload_url.replace(/\{.*$/, '') + '?name=' + encodeURIComponent(prefix + file.name), undefined, {file: file.file});
    if (!authentic(candidate, file)) throw Error('Candidate upload failed SHA-256 authentication');
    staged.set(file.name, candidate);
    journal.replacements.push({name: file.name, oldId: original.find(asset => asset.name === file.name).id, nextId: candidate.id, size: file.size, sha256: file.sha256});
    await checkpoint(journal);
  }
  current = await assets();
  if (original.some(old => !unchanged(current.find(asset => asset.name === old.name), old))) throw Error('Public downloads changed during staging');
  if ((await call('GET', base + '/releases/tags/' + FIXED_TAG)).body !== release.body) throw Error('Release notes changed during staging');
  await checkTag(); journal.status = 'replacing'; await checkpoint(journal);
  const moved = [];
  try {
    for (const file of files) {
      const old = original.find(asset => asset.name === file.name), next = staged.get(file.name);
      moved.push({old, next});
      await call('PATCH', assetRoute(old.id), {name: prefix + 'previous-' + old.id + '-' + file.name});
      await call('PATCH', assetRoute(next.id), {name: file.name});
      await checkpoint(journal);
    }
    await verifyFinal(); await publishNotes();
    journal.status = 'verified'; await checkpoint(journal);
  } catch (error) {
    let recovered = true;
    for (const {old, next} of moved.reverse()) {
      try {
        const list = await assets(), currentNext = list.find(asset => asset.id === next.id), currentOld = list.find(asset => asset.id === old.id);
        if (!currentNext || !currentOld) throw Error('Recovery identity missing');
        if (currentNext.name === old.name) await call('PATCH', assetRoute(next.id), {name: prefix + old.name});
        else if (currentNext.name !== prefix + old.name) throw Error('Candidate was changed externally');
        if (currentOld.name !== old.name) {
          if (currentOld.name !== prefix + 'previous-' + old.id + '-' + old.name) throw Error('Original was changed externally');
          await call('PATCH', assetRoute(old.id), {name: old.name});
        }
      } catch { recovered = false; }
    }
    try {
      const list = await assets();
      if (original.some(old => !unchanged(list.find(asset => asset.name === old.name), old))) recovered = false;
      const metadata = await call('GET', base + '/releases/tags/' + FIXED_TAG);
      if (metadata.body === notes) await call('PATCH', base + '/releases/' + FIXED_RELEASE_ID, {body: release.body});
      else if (metadata.body !== release.body) recovered = false;
    } catch { recovered = false; }
    journal.status = recovered ? 'rolled_back' : 'recovery_pending'; await checkpoint(journal);
    throw Error('V0.0.7 refresh failed; ' + journal.status + '. ' + error.message);
  }
  return cleanup();
}
