// Explicit maintainer resource preparation. Never called by a build, runtime
// installer or feature. Default operation is fully offline verification.
import fs from 'node:fs';
import path from 'node:path';
import crypto from 'node:crypto';
import { once } from 'node:events';
import { fileURLToPath } from 'node:url';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const args = process.argv.slice(2);
if (args.some(a => !['--download'].includes(a))) throw Error('Usage: node tools/prepare-tex-inputs.mjs [--download]');
const lock = JSON.parse(fs.readFileSync(path.join(root, 'tools/tex-runtime-inputs.lock.json')));
const input = lock.archive;
const folder = path.join(root, '.cache/capability-upgrade/tex-inputs');
const target = path.join(folder, input.name);
const hash = () => crypto.createHash('sha256');
async function verify(file) {
  if (fs.statSync(file).size !== input.size) throw Error('Prepared TeX input length differs');
  const digest = hash();
  for await (const bytes of fs.createReadStream(file)) digest.update(bytes);
  if (digest.digest('hex') !== input.sha256) throw Error('Prepared TeX input digest differs');
}
if (!fs.existsSync(target)) {
  if (!args.includes('--download')) throw Error('Locked input is missing. Resource preparation requires explicit --download; ordinary builds remain offline.');
  fs.mkdirSync(folder, { recursive: true });
  const partial = target + '.partial';
  let received = fs.existsSync(partial) ? fs.statSync(partial).size : 0;
  if (received > input.size) throw Error('Partial input exceeds reviewed length; audit it before continuing');
  if (received < input.size) {
    const response = await fetch(input.url, {
      headers: received ? { Range: `bytes=${received}-` } : {},
      signal: AbortSignal.timeout(1200000),
    });
    if (!['github.com', 'release-assets.githubusercontent.com'].includes(new URL(response.url).hostname)) throw Error('Unexpected upstream resource host');
    if (received ? response.status !== 206 || response.headers.get('content-range') !== `bytes ${received}-${input.size - 1}/${input.size}` : response.status !== 200) throw Error('Official TeX input resume unavailable');
    const stream = fs.createWriteStream(partial, { flags: received ? 'a' : 'wx' });
    let writeError;
    stream.on('error', error => { writeError = error; });
    try {
      for await (const bytes of response.body) {
        received += bytes.length;
        if (received > input.size) throw Error('Official resource exceeds reviewed length');
        if (writeError) throw writeError;
        if (!stream.write(bytes)) await once(stream, 'drain');
      }
      stream.end();
      await once(stream, 'finish');
    } catch (error) {
      stream.destroy();
      throw error;
    }
  }
  await verify(partial);
  fs.renameSync(partial, target);
}
await verify(target);
// This record is provenance, not an application installation manifest.
fs.writeFileSync(path.join(folder, 'input.json'), JSON.stringify({ ...input, version: 'v2026.10', upstream: lock.source }, null, 2) + '\n');
console.log(JSON.stringify({ prepared: true, offline: !args.includes('--download'), bytes: input.size, sha256: input.sha256, redistributionApproved: lock.review.redistributionApproved }));
