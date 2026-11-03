// CI fixtures exercise strict handle/ancestor identity checks. Give tempfile a
// canonical owned root rather than /var aliases or Windows short-name TEMP.
import fs from 'node:fs';
import path from 'node:path';
if (!process.env.CI || !process.env.GITHUB_WORKSPACE || !process.env.GITHUB_ENV) throw Error('GitHub CI environment required');
const workspace = fs.realpathSync.native(process.env.GITHUB_WORKSPACE);
let directory = workspace;
for (const part of ['.cache', 'kernel-temp']) {
  directory = path.join(directory, part);
  fs.mkdirSync(directory, { recursive: true });
  const info = fs.lstatSync(directory);
  if (!info.isDirectory() || info.isSymbolicLink()) throw Error('Test temporary root must be an owned directory');
}
directory = fs.realpathSync.native(directory);
if (/[\r\n]/.test(directory)) throw Error('Invalid test temporary root');
fs.appendFileSync(process.env.GITHUB_ENV, ['TMPDIR', 'TMP', 'TEMP'].map(name => name + '=' + directory + '\n').join(''));
if (process.platform === 'darwin') {
  const home = path.join(directory, 'native-home');
  fs.mkdirSync(home, { recursive: true, mode: 0o700 });
  if (fs.lstatSync(home).isSymbolicLink()) throw Error('Native test home cannot be a symlink');
  fs.appendFileSync(process.env.GITHUB_ENV, 'CFFIXED_USER_HOME=' + fs.realpathSync.native(home) + '\n');
}
console.log('Configured canonical workspace-owned test temporary directory');
