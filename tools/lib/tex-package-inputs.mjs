// Maintainer input metadata, never loaded by the desktop application.
import crypto from 'node:crypto';

export function parseTlpdb(bytes) {
  const records = new Map();
  for (const raw of bytes.toString('utf8').replaceAll('\r\n', '\n').split(/\n\s*\n/)) {
    if (!raw.trim()) continue;
    const value = key => raw.match(new RegExp('^' + key + ' (.*)$', 'm'))?.[1];
    const name = value('name');
    if (!name || !/^[A-Za-z0-9][A-Za-z0-9_.-]*$/.test(name) || records.has(name)) throw Error('Invalid TeX package identity');
    const files = { run: [], source: [], doc: [] };
    let section;
    for (const line of raw.split('\n')) {
      if (!line.startsWith(' ')) {
        section = /^(runfiles|binfiles) /.test(line) ? 'run' : /^srcfiles /.test(line) ? 'source' : /^docfiles /.test(line) ? 'doc' : null;
      } else if (section) {
        const file = line.trim().split(/\s+/)[0];
        if (!file || /[\\:\x00-\x1f\x7f]/.test(file) || file.startsWith('/') || file.split('/').some(p => !p || p === '.' || p === '..')) throw Error('Invalid TeX package path');
        files[section].push(file);
      }
    }
    const containers = {};
    for (const [kind, prefix, suffix] of [['run', '', ''], ['source', 'src', '.source'], ['doc', 'doc', '.doc']]) {
      const checksum = value(prefix + 'containerchecksum');
      if (!checksum) continue;
      const size = Number(value(prefix + 'containersize'));
      if (!/^[a-f0-9]{128}$/.test(checksum) || !Number.isSafeInteger(size) || size < 1 || size > 256 * 1048576) throw Error('Invalid TeX container identity');
      containers[kind] = { name: name + suffix + '.tar.xz', size, sha512: checksum, files: files[kind] };
    }
    records.set(name, { name, revision: value('revision'), license: value('catalogue-license') || null, relocated: value('relocated') === '1', dependencies: [...raw.matchAll(/^depend (.+)$/gm)].map(m => m[1]), containers });
  }
  if (!records.size) throw Error('No TeX package records');
  return records;
}

export function dependencyClosure(available, installed, roots, platform = 'windows') {
  const selected = new Map(), pending = [...roots];
  while (pending.length) {
    const name = pending.pop().replace(/\.ARCH$/, '.' + platform);
    if (installed.has(name) || selected.has(name)) continue;
    const record = available.get(name);
    if (!record) throw Error('Missing TeX dependency: ' + name);
    selected.set(name, record);
    pending.push(...record.dependencies);
  }
  return [...selected.values()].sort((a, b) => a.name.localeCompare(b.name, 'en'));
}

export function verifyContainer(bytes, expected) {
  if (bytes.length !== expected.size || crypto.createHash('sha512').update(bytes).digest('hex') !== expected.sha512) throw Error('TeX container integrity differs: ' + expected.name);
}
