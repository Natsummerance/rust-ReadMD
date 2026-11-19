// Explicit CI input preparation; no shell syntax or normal build downloads.
import fs from 'node:fs';
import path from 'node:path';
import {execFileSync} from 'node:child_process';
import {fileURLToPath} from 'node:url';
import {select} from './prepare-node-runtime.mjs';

const repo = fs.realpathSync.native(path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..'));
if (!process.env.CI || !process.env.GITHUB_WORKSPACE || !process.env.GITHUB_ENV
    || fs.realpathSync.native(process.env.GITHUB_WORKSPACE) !== repo) throw Error('Dedicated GitHub workspace required');
const platform = ({darwin:'macos', win32:'windows', linux:'linux'}[process.platform] || '') + '-' + process.arch;
select(platform);
function ownedDirectory(name) {
  const root = path.join(repo, '.cache');
  for (const directory of [root, path.join(root, name)]) {
    fs.mkdirSync(directory, {recursive:true});
    const stat = fs.lstatSync(directory);
    if (!stat.isDirectory() || stat.isSymbolicLink()
        || fs.realpathSync.native(directory) !== directory) throw Error('Real owned CI directory required');
  }
  return path.join(root, name);
}
const inputs = ownedDirectory('p1-node-inputs'), output = path.join(repo, '.cache', 'p1-node-pack');
if (fs.existsSync(output)) throw Error('Preparation output already exists');
const run = args => execFileSync(process.execPath, args, {cwd:repo, stdio:'inherit'});
run(['tools/prepare-node-runtime.mjs', 'download', '--inputs', inputs, '--platform', platform]);
run(['tools/prepare-node-runtime.mjs', 'build', '--inputs', inputs, '--output', output, '--platform', platform]);
run(['tools/verify-node-pack.mjs', output, '--github-env']);
