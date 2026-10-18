import fs from 'node:fs';
import path from 'node:path';
import crypto from 'node:crypto';
import { fileURLToPath } from 'node:url';
const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const sha256 = bytes => crypto.createHash('sha256').update(bytes).digest('hex');
function inside(root, name) {
  if (typeof name !== 'string' || path.isAbsolute(name)) return null;
  const file = path.resolve(root, name), relative = path.relative(root, file);
  if (!relative || relative.startsWith('..') || path.isAbsolute(relative)) return null;
  return file;
}
export function sourceDigest(root, files) {
  const digest = crypto.createHash('sha256');
  for (const name of [...files].sort()) {
    const file = inside(root, name);
    if (!file || !fs.statSync(file).isFile()) throw Error('invalid implementation path');
    digest.update(name).update('\0').update(sha256(fs.readFileSync(file))).update('\n');
  }
  return digest.digest('hex');
}
export function validate(ledger, root, complete = false) {
  const problems = [], ids = new Set();
  const states = ['pending', 'in_progress', 'implemented', 'verified'];
  const kinds = ['unit', 'integration', 'native', 'stress', 'inspection'];
  if (ledger.schema !== 2 || !Array.isArray(ledger.requirements)) return ['invalid ledger schema'];
  for (const item of ledger.requirements) {
    if (!/^R\d{3}$/.test(item.id) || ids.has(item.id)) problems.push(`invalid or duplicate id: ${item.id}`);
    ids.add(item.id);
    if (!item.acceptance || !item.workflow || !item.requirement || !states.includes(item.status)) problems.push(`incomplete specification: ${item.id}`);
    if (!Array.isArray(item.implementation) || !Array.isArray(item.evidence) || !Array.isArray(item.remaining)) {
      problems.push(`incomplete tracking: ${item.id}`); continue;
    }
    let digest;
    try { digest = sourceDigest(root, item.implementation); }
    catch { problems.push(`missing implementation: ${item.id}`); }
    if (item.status === 'in_progress' && (!item.implementation.length || !item.remaining.length)) problems.push(`progress needs implementation and remaining work: ${item.id}`);
    if (item.status === 'verified' && (!item.implementation.length || item.remaining.length || !item.evidence.length)) problems.push(`unmet acceptance: ${item.id}`);
    for (const proof of item.evidence) {
      const file = inside(root, proof.path);
      if (!file || !fs.existsSync(file) || !fs.statSync(file).isFile()) { problems.push(`missing evidence: ${item.id}`); continue; }
      if (sha256(fs.readFileSync(file)) !== proof.sha256) problems.push(`changed evidence: ${item.id} ${proof.path}`);
      if (!kinds.includes(proof.kind) || !['passed', 'partial', 'failed'].includes(proof.outcome) || !proof.scope || !proof.recordedAt) problems.push(`invalid evidence description: ${item.id}`);
      if (proof.kind === 'native' && !proof.platform) problems.push(`native evidence needs platform: ${item.id}`);
      if (item.status === 'verified' && (proof.outcome !== 'passed' || proof.sourceDigest !== digest)) problems.push(`failed or stale certification: ${item.id}`);
    }
    if (item.status === 'verified' && !item.evidence.some(e => e.kind === 'native' && e.outcome === 'passed')) problems.push(`native workflow not verified: ${item.id}`);
    if (complete && item.status !== 'verified') problems.push(`not verified: ${item.id} ${item.requirement}`);
  }
  if (complete) {
    const required=['windows-x64','macos-x64','macos-arm64','linux-x64','linux-arm64','domestic-linux'];
    for(const id of required){
      const platform=(ledger.platformEvidence||[]).filter(p=>p.id===id);
      if(platform.length!==1||platform[0].formal!==true||platform[0].status!=='verified')problems.push(`native platform not verified: ${id}`);
      else if(!Array.isArray(platform[0].evidence)||!platform[0].evidence.length)problems.push(`native platform evidence missing: ${id}`);
    }
  }
  return problems;
}
if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const ledger = JSON.parse(fs.readFileSync(path.join(root, 'docs/design/capability-upgrade-acceptance.json'), 'utf8'));
  const problems = validate(ledger, root, process.argv.includes('--complete'));
  const counts = Object.fromEntries(['pending', 'in_progress', 'implemented', 'verified'].map(s => [s, ledger.requirements.filter(i => i.status === s).length]));
  console.log(JSON.stringify({ requirements: ledger.requirements.length, ...counts, problems }, null, 2));
  if (problems.length) process.exitCode = 1;
}
