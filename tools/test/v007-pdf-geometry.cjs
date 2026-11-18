'use strict';

const assert = require('node:assert/strict');
const { spawnSync } = require('node:child_process');
const { createHash } = require('node:crypto');
const fs = require('node:fs');
const path = require('node:path');
const { parseArgs } = require('node:util');
const { startKernel } = require('./v006-refactor-host.cjs');

const fixtures = path.join(__dirname, '../../tests/fixtures/table-geometry');
const inspector = path.join(__dirname, 'v007-pdf-geometry.py');
const formats = { pdf: 'pdf', png: 'png', 'png-pages': 'zip' };
const cases = { ordinary: formats, merged: formats, grid: { pdf: 'pdf' } };
const options = {
  page: { size: 'A5', marginTop: 16, marginBottom: 16, marginLeft: 16, marginRight: 16 },
  typography: { font: 'Arial', size: 11, color: '#141414' },
  table: { style: 'three-line', widthPct: 90, cellSize: 11, cellPadding: 6, borderWidth: 1, borderColor: '#144fad' },
  footer: { pageNumbers: false },
  frontMatter: { mode: 'none' },
};
const gridOptions = {
  ...options,
  table: { ...options.table, style: 'grid', banded: false, headerBg: '#ffffff', headerColor: '#141414' },
};

function fileHash(file) {
  return new Promise((resolve, reject) => {
    const hash = createHash('sha256');
    const stream = fs.createReadStream(file);
    stream.on('data', chunk => hash.update(chunk));
    stream.on('end', () => resolve(hash.digest('hex')));
    stream.on('error', reject);
  });
}

async function main() {
  const { values: args } = parseArgs({ options: {
    bin: { type: 'string' }, evidence: { type: 'string' }, python: { type: 'string' }, 'pdfium-pack': { type: 'string' },
  } });
  assert(args.bin && args.evidence && args.python,
    'Usage: node tools/test/v007-pdf-geometry.cjs --bin READMD --evidence NEW_DIR --python PYTHON [--pdfium-pack PACK]');
  const binary = path.resolve(args.bin);
  const evidence = path.resolve(args.evidence);
  assert(fs.statSync(binary).isFile(), 'Supply a prebuilt ReadMD executable');
  const needsPdfium = process.platform !== 'darwin' && process.platform !== 'win32';
  assert(!needsPdfium || args['pdfium-pack'], 'Supply the production PDFium pack with --pdfium-pack');
  fs.mkdirSync(path.dirname(evidence), { recursive: true });
  fs.mkdirSync(evidence);
  const docs = path.join(evidence, 'outputs');
  const profile = path.join(evidence, 'profile');
  const temp = path.join(evidence, 'temp');
  for (const dir of [docs, profile, temp]) fs.mkdirSync(dir);
  Object.assign(process.env, { TMPDIR: temp, TMP: temp, TEMP: temp });

  const report = {
    passed: false, platform: process.platform, arch: process.arch, options, gridOptions,
    rasterBackend: needsPdfium ? 'PDFium capability pack' : process.platform === 'darwin' ? 'Quartz' : 'Windows.Data.Pdf',
    formatRoutes: Object.keys(formats).map(format => ({ format, route: '/api/export' })),
    inputs: [], exports: [], failureProtection: [],
  };
  let phase = 'dependencies';
  let host;
  const post = async (route, payload) => {
    const response = await host.api(route, {
      method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(payload),
    });
    return { status: response.status, ok: response.ok, result: await response.json() };
  };
  const exportFile = (format, content, output, settings = options) => post('/api/export', {
    format, content, options: settings, baseDir: docs, suggestedName: path.basename(output), out_path: output,
  });
  const launch = () => startKernel(binary, profile, docs, { requestTimeoutMs: 130000 });

  try {
    report.binarySha256 = await fileHash(binary);
    report.driverSha256 = await fileHash(__filename);
    report.inspectorSha256 = await fileHash(inspector);
    const pythonReady = spawnSync(args.python, ['-B', '-c', 'import pdfplumber, PIL'], { encoding: 'utf8', timeout: 15000 });
    assert.equal(pythonReady.status, 0, 'Install pdfplumber and Pillow in the chosen Python: ' + (pythonReady.error?.message || pythonReady.stderr));
    const sources = {};
    for (const name of [...Object.keys(cases), 'invalid']) {
      const source = path.join(fixtures, name + '.md');
      const destination = path.join(docs, name + '.md');
      fs.copyFileSync(source, destination, fs.constants.COPYFILE_EXCL);
      sources[name] = fs.readFileSync(destination, 'utf8');
      report.inputs.push({ name, sha256: await fileHash(destination) });
    }

    phase = 'starting owned kernel';
    host = await launch();
    if (needsPdfium) {
      phase = 'installing PDFium capability';
      const pack = path.join(docs, 'PDFium.readmd-pack');
      fs.copyFileSync(path.resolve(args['pdfium-pack']), pack, fs.constants.COPYFILE_EXCL);
      report.packSha256 = await fileHash(pack);
      const installed = await post('/api/capabilities/packs', { action: 'install', id: 'pdfium', path: pack });
      assert(installed.ok && installed.result.ok, JSON.stringify(installed));
      await host.stop();
      host = await launch();
      const status = await post('/api/capabilities/packs', { action: 'status', id: 'pdfium' });
      assert(status.ok && status.result.installed, JSON.stringify(status));
      fs.unlinkSync(pack);
    }

    for (const [name, exportedFormats] of Object.entries(cases)) {
      for (const [format, extension] of Object.entries(exportedFormats)) {
        phase = `${name}/${format}`;
        const file = `${name}.${extension}`;
        const target = path.join(docs, file);
        const { status, ok, result } = await exportFile(format, sources[name], target, name === 'grid' ? gridOptions : options);
        const receipt = { name, format, file, status, passed: ok && result.ok === true };
        report.exports.push(receipt);
        if (!receipt.passed) {
          receipt.errorCode = result.error_code || result.error;
          receipt.details = result.details;
          continue;
        }
        receipt.bytes = fs.statSync(target).size;
        assert(receipt.bytes > 0, `${file} must exist and contain bytes`);
        receipt.sha256 = await fileHash(target);
      }
    }

    for (const [format, extension] of Object.entries(formats)) {
      phase = `${format} invalid span write protection`;
      const existing = path.join(docs, `ordinary.${extension}`);
      const missing = path.join(docs, `invalid.${extension}`);
      const expected = report.exports.find(receipt => receipt.file === path.basename(existing))?.sha256;
      assert(expected, `A real ${format} export is required for replacement testing`);
      const contentsBefore = fs.readdirSync(docs).sort();
      for (const target of [existing, missing]) {
        const { status, result } = await exportFile(format, sources.invalid, target);
        const preservesTarget = target === existing ? await fileHash(target) === expected : !fs.existsSync(target);
        const passed = result.ok === false && result.error_code === 'publication_table_unsupported' && preservesTarget;
        report.failureProtection.push({ format, target: path.basename(target), status, errorCode: result.error_code, preservesTarget, passed });
      }
      assert.deepEqual(fs.readdirSync(docs).sort(), contentsBefore, `${format} failure left an unexpected file or staging directory`);
    }
    for (const input of report.inputs) {
      assert.equal(await fileHash(path.join(docs, input.name + '.md')), input.sha256, 'Source file changed during export');
    }

    phase = 'inspecting physical PDF and PNG geometry';
    const inspected = spawnSync(args.python, ['-B', inspector, docs], {
      encoding: 'utf8', timeout: 90000, maxBuffer: 1024 * 1024, windowsHide: true,
    });
    if (inspected.stdout) report.geometry = JSON.parse(inspected.stdout);
    assert(inspected.status === 0 && report.geometry?.passed, inspected.error?.message || inspected.stderr ||
      report.geometry?.failures.map(failure => `${failure.case}: ${failure.error}`).join('; '));
    assert(report.exports.every(entry => entry.passed), 'A required export failed');
    assert(report.failureProtection.every(entry => entry.passed), 'An invalid span changed an existing file or failed to refuse a new one');

    phase = 'exercising the frozen export snapshot';
    report.snapshot = await require('./table-geometry-export-snapshot.cjs')({ host, docs, options });
    phase = 'inspecting the research preset export';
    report.research = await require('./research-table-output.cjs')({ host, docs, options, python: args.python });
    phase = 'exercising table geometry budget failure';
    report.overBudget = await require('./table-geometry-over-budget.cjs')({ host, docs, options });
    phase = 'checking candidate identity';
    assert.equal(await fileHash(binary), report.binarySha256, 'Candidate executable changed during the run');
    report.passed = true;
  } catch (error) {
    report.failure = { phase, error: error.message };
    process.exitCode = 1;
  } finally {
    try {
      await host?.stop();
    } finally {
      for (const dir of [profile, temp]) fs.rmSync(dir, { recursive: true, force: true });
      fs.writeFileSync(path.join(evidence, 'report.json'), JSON.stringify(report, null, 2) + '\n', { flag: 'wx' });
      console.log(JSON.stringify({ passed: report.passed, platform: report.platform, failure: report.failure }));
    }
  }
}

main().catch(error => { console.error(error.message); process.exitCode = 1; });
