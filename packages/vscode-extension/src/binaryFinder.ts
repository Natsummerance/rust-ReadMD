import * as vscode from 'vscode';
import * as cp from 'child_process';
import * as path from 'path';
import * as fs from 'fs';

/**
 * Locate the ReadMD executable that serves `readmd --mcp`.
 * Order:
 * 1. `readmd.executablePath` from settings.json
 * 2. A binary staged inside the VSIX (`core/bin/`)
 * 3. Default install locations of the desktop app
 * 4. `readmd` / `ReadMD` on PATH
 * Probes are asynchronous so a missing runtime cannot freeze the editor.
 */
export function candidatePaths(extensionPath: string, env: NodeJS.ProcessEnv = process.env,
    platform: NodeJS.Platform = process.platform): string[] {
  const exe = platform === 'win32' ? 'readmd.exe' : 'readmd';
  const list: string[] = [path.join(extensionPath, 'core', 'bin', exe)];
  if (platform === 'win32') {
    const roots = [
      env.USERPROFILE ? path.join(env.USERPROFILE, 'Applications') : '',
      env.LOCALAPPDATA ? path.join(env.LOCALAPPDATA, 'Programs') : '',
      env.ProgramFiles || '',
      env['ProgramFiles(x86)'] || '',
    ].filter(Boolean);
    for (const root of roots) list.push(path.join(root, 'ReadMD', 'ReadMD.exe'));
  } else if (platform === 'darwin') {
    list.push('/Applications/ReadMD.app/Contents/MacOS/ReadMD', '/Applications/ReadMD.app/Contents/MacOS/readmd');
    if (env.HOME) list.push(path.join(env.HOME, 'Applications', 'ReadMD.app', 'Contents', 'MacOS', 'ReadMD'));
  } else {
    list.push('/usr/bin/readmd', '/usr/local/bin/readmd', '/opt/readmd/ReadMD');
    if (env.HOME) list.push(path.join(env.HOME, '.local', 'bin', 'readmd'));
  }
  list.push(...(platform === 'win32' ? ['readmd', 'ReadMD'] : ['readmd']));
  return [...new Set(list)];
}

/** `readmd --version` exits 0 and prints `readmd-rust <version>`. */
export async function probeBinary(candidate: string): Promise<boolean> {
  if (path.isAbsolute(candidate) && !fs.existsSync(candidate)) return false;
  try {
    return await new Promise(resolve => cp.execFile(candidate, ['--version'],
      { timeout: 3000, windowsHide: true, encoding: 'utf8', maxBuffer: 8192 },
      (error, stdout) => resolve(!error && /^readmd-rust\s+\d+\./im.test(stdout))));
  } catch {
    return false;
  }
}

export async function findReadmdBinary(extensionPath: string): Promise<string> {
  const configured = vscode.workspace.getConfiguration('readmd').get<string>('executablePath', '').trim();
  if (configured) {
    if (await probeBinary(configured)) return configured;
    throw new Error('readmd_binary_invalid');
  }
  for (const candidate of candidatePaths(extensionPath)) {
    if (await probeBinary(candidate)) return candidate;
  }
  throw new Error('readmd_binary_not_found');
}
