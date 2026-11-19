'use strict';

const assert = require('node:assert/strict');
const { spawnSync } = require('node:child_process');
const { createHash } = require('node:crypto');
const fs = require('node:fs');
const path = require('node:path');

const fixture = path.join(__dirname, '../../tests/fixtures/table-geometry/research.md');
const inspector = path.join(__dirname, 'research-table-check.py');
const sha256 = data => createHash('sha256').update(data).digest('hex');

module.exports = async function researchTableOutput({ host, docs, options, python }) {
  const sourceFile = path.join(docs, 'research.md');
  fs.copyFileSync(fixture, sourceFile, fs.constants.COPYFILE_EXCL);
  const source = fs.readFileSync(sourceFile);
  const content = source.toString('utf8');
  const style = {
    ...options,
    page: { ...options.page, size: 'A5', marginTop: 16, marginBottom: 16, marginLeft: 16, marginRight: 16 },
    table: {
      ...options.table, style: 'three-line', widthPct: 100, cellSize: 10,
      cellPadding: 6, borderColor: '#144fad', borderWidth: 1,
    },
  };
  const exports = [];
  for (const [format, extension] of [['pdf', 'pdf'], ['png', 'png'], ['png-pages', 'zip']]) {
    const name = 'research.' + extension;
    const target = path.join(docs, name);
    assert(!fs.existsSync(target), 'An earlier artifact occupies ' + target);
    const response = await host.api('/api/export', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({
        format, content, options: style, baseDir: docs,
        out_path: target, suggestedName: 'research.md',
      }),
    });
    const result = await response.json();
    assert(response.ok && result.ok === true, format + ' export failed. ' + JSON.stringify(result));
    const bytes = fs.readFileSync(target);
    assert(bytes.length > 0, name + ' is empty');
    exports.push({ format, file: name, bytes: bytes.length, sha256: sha256(bytes) });
  }
  assert.deepEqual(fs.readFileSync(sourceFile), source, 'Export changed the source');
  const run = spawnSync(python, ['-B', inspector, docs], {
    encoding: 'utf8', timeout: 90000, maxBuffer: 1024 * 1024, windowsHide: true,
  });
  assert.equal(run.status, 0, run.error?.message || run.stderr);
  const inspection = JSON.parse(run.stdout);
  assert.equal(inspection.passed, true);
  return {
    source: { file: 'research.md', sha256: sha256(source) },
    options: style, exports, inspection,
  };
};
