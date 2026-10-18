// Input preparation only. Ordinary builds and application tasks never invoke
// this file, download packages, run tlmgr or use a host TeX installation.
import fs from 'node:fs';
import path from 'node:path';
import crypto from 'node:crypto';
import { once } from 'node:events';
import { fileURLToPath } from 'node:url';
const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const args = process.argv.slice(2);
if (args.some(a => !['--download', '--runtime-only'].includes(a))) throw Error('Usage: node tools/prepare-tex-publication-inputs.mjs [--download] [--runtime-only]');
const lock = JSON.parse(fs.readFileSync(path.join(root, 'tools/tex-publication-inputs.lock.json')));
if (lock.schema !== 1 || lock.redistributionApproved !== false) throw Error('Unexpected maintainer lock');
const folder = path.join(root, '.cache/capability-upgrade/tex-inputs/containers');
fs.mkdirSync(folder, { recursive: true });
const containers = lock.components.flatMap(p => Object.entries(p.containers).filter(([kind]) => !args.includes('--runtime-only') || kind === 'run').map(([, c]) => c));
const names = new Set();
for (const c of containers) {
  if (!/^[A-Za-z0-9][A-Za-z0-9_.-]*\.tar\.xz$/.test(c.name) || names.has(c.name) || !/^[a-f0-9]{128}$/.test(c.sha512) || !Number.isSafeInteger(c.size) || c.size < 1 || c.size > 256 * 1048576) throw Error('Invalid locked container');
  names.add(c.name);
}
async function verify(file, c) {
  if (fs.lstatSync(file).isSymbolicLink() || fs.statSync(file).size !== c.size) throw Error('TeX container length differs: ' + c.name);
  const digest = crypto.createHash('sha512');
  for await (const bytes of fs.createReadStream(file)) digest.update(bytes);
  if (digest.digest('hex') !== c.sha512) throw Error('TeX container digest differs: ' + c.name);
}
let received = 0, completed = 0, next = 0;
async function prepare(c) {
  const target = path.join(folder, c.name);
  if (fs.existsSync(target)) { await verify(target, c); completed++; return; }
  if (!args.includes('--download')) throw Error('Missing locked TeX input: ' + c.name + '. Network preparation requires explicit --download.');
  const partial = target + '.partial';
  let size = fs.existsSync(partial) ? fs.statSync(partial).size : 0;
  if (fs.existsSync(partial) && fs.lstatSync(partial).isSymbolicLink() || size > c.size) throw Error('Audit unexpected partial TeX input: ' + c.name);
  if (size < c.size) {
    const response = await fetch(new URL(c.name, lock.archiveOrigin), { headers: size ? { Range: `bytes=${size}-` } : {}, signal: AbortSignal.timeout(300000) });
    if (new URL(response.url).origin !== new URL(lock.archiveOrigin).origin || (size ? response.status !== 206 || response.headers.get('content-range') !== `bytes ${size}-${c.size - 1}/${c.size}` : response.status !== 200)) { await response.body?.cancel(); throw Error('Locked upstream input unavailable: ' + c.name); }
    const stream = fs.createWriteStream(partial, { flags: size ? 'a' : 'wx' });
    let writeError;
    stream.on('error', error => { writeError = error; });
    try {
      for await (const bytes of response.body) {
        size += bytes.length; received += bytes.length;
        if (size > c.size) throw Error('Upstream TeX input exceeds lock: ' + c.name);
        if (writeError) throw writeError;
        if (!stream.write(bytes)) await once(stream, 'drain');
      }
      stream.end(); await once(stream, 'finish');
    } catch (error) {
      // Some mirrors close a chunked response after the final payload rather
      // than sending its HTTP terminator. Only the full pinned size AND hash
      // can make that transport failure usable; a prefix remains incomplete.
      if (!writeError && size === c.size && error?.name === 'TypeError') {
        stream.end(); await once(stream, 'finish');
      } else { stream.destroy(); throw error; }
    }
  }
  await verify(partial, c);
  fs.renameSync(partial, target); completed++;
  if (completed % 25 === 0 || completed === containers.length) console.log(JSON.stringify({ completed, total: containers.length, received }));
}
const failures = [];
const workers = Array.from({ length: Math.min(4, containers.length) }, async () => {
  while (next < containers.length) {
    const container = containers[next++];
    try { await prepare(container); }
    catch (error) { failures.push({ name: container.name, error: String(error.message) }); }
  }
});
// Wait for all in-flight writes even on error. Failed files retain bounded
// partials for explicit resume; no background transfers outlive this process.
const results = await Promise.allSettled(workers);
for (const result of results) if (result.status === 'rejected') failures.push({ error: String(result.reason.message) });
console.log(JSON.stringify({ prepared: failures.length === 0, offline: !args.includes('--download'), runtimeOnly: args.includes('--runtime-only'), containers: completed, total: containers.length, ...(failures.length ? { failedInputs: failures } : {}), redistributionApproved: false }));
if (failures.length) process.exitCode = 1;
