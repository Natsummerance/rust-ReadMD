'use strict';
// Reader syntax/capability diagnostics and deterministic preview candidates.
// The vendored compiler is the oracle for the regression-transform migration.
const test = require('node:test'), assert = require('node:assert/strict'), fs = require('node:fs'), vm = require('node:vm');
const diagnostics = require('../../assets/js/reader/diagram-diagnostics.js');
const sourceFile = require.resolve('../../assets/js/reader/diagram-diagnostics.js');
const vegaLite = require('../../assets/vendor/diagrams/vega-lite/vega-lite.min.js');
const figure = extra => ({ data: { values: [{ X: 1, Y: 3 }, { X: 2, Y: 5 }] }, mark: 'line',
  encoding: { x: { field: 'X', type: 'quantitative' }, y: { field: 'Y', type: 'quantitative' } }, ...extra });
const logger = warnings => ({ warn(...args) { warnings.push(args.join(' ')); }, error() {}, info() {}, debug() {}, level() { return this; } });

test('normal large inline data uses native JSON parsing without a path index', () => {
  let indexed = 0;
  class CountedMap extends Map { constructor(...args) { super(...args); indexed++; } }
  const context = { module: { exports: {} }, Map: CountedMap, TextEncoder };
  vm.runInNewContext(fs.readFileSync(sourceFile, 'utf8'), context);
  const source = JSON.stringify(figure({ data: { values: Array.from({ length: 40000 }, (_, X) => ({ X, Y: X * 2 })) } }));
  assert.equal(context.module.exports.inspect('vega-lite', source), null);
  assert.equal(indexed, 0);
});

test('malformed JSON reports the precise CRLF location instead of a generic failure', () => {
  const source = '{\r\n  "mark": "bar",\r\n  "data": { "values": [1,] }\r\n}';
  const result = diagnostics.inspect('vega-lite', source);
  assert.equal(result.messageKey, 'reader.diagramTrailingComma');
  assert.deepEqual({ line: result.line, column: result.column }, { line: 3, column: 25 });
  assert.equal(result.fix.kind, 'json-punctuation');
  assert.deepEqual(JSON.parse(result.fix.code), { mark: 'bar', data: { values: [1] } });
});

test('structural Chart.js failures identify the actual field and its source position', () => {
  const source = '{\n "type": "bar",\n "data": {"datasets": {}}\n}';
  const result = diagnostics.inspect('chartjs', source);
  assert.equal(result.field, '$.data.datasets');
  assert.equal(result.messageParams.expected, 'array');
  assert.equal(result.line, 3); assert.equal(result.column, 23);
  assert.equal(result.fix, null);
  assert.equal(diagnostics.inspect('chart', '{"data":{"datasets":[{"type":"line","data":[1]}]}}'), null);
});

test('WaveDrom and bitfield keep supported object-literal punctuation without executing code', () => {
  const source = String.raw`{signal:[{name:'O\'Brien "clock"',wave:'01.z',},],}`;
  assert.equal(diagnostics.inspect('wavedrom', source), null);
  assert.deepEqual(diagnostics.parseJson(source, true), { signal: [{ name: 'O\'Brien "clock"', wave: '01.z' }] });
  assert.equal(diagnostics.inspect('bitfield', "{reg:[{bits:8,name:'low',},{bits:8,name:'high'},],}"), null);
  assert.throws(() => diagnostics.parseJson('{signal:globalThis.__untrusted()}', true));
  assert.equal(diagnostics.inspect('chart', '{type:(()=>42)()}').fix, null);
});

test('local punctuation corrections preserve label content and own __proto__ fields', () => {
  const source = String.raw`{type:'bar',data:{labels:['comma,} and https://example.test/a'],datasets:[],},'__proto__':{safe:true},}`;
  const result = diagnostics.inspect('chart', source);
  assert(result.fix); assert.equal(diagnostics.inspect('chart', result.fix.code), null);
  const value = JSON.parse(result.fix.code);
  assert.equal(value.data.labels[0], 'comma,} and https://example.test/a');
  assert(Object.hasOwn(value, '__proto__')); assert.equal({}.safe, undefined);
  assert.equal(result.fix.code.includes('type:\'bar\''), false);
});

test('regression spelling is diagnosed and migrated without losing authored fields/data', () => {
  const spec = { layer: [figure({ transform: [{ type: 'regression', on: 'X', y: 'Y', method: 'linear', as: ['fitX', 'fitY'] }] })] };
  const source = JSON.stringify(spec, null, 2), before = JSON.stringify(spec);
  const result = diagnostics.inspect('vega-lite', source);
  assert.equal(result.field, '$.layer[0].transform[0].regression');
  assert(result.line > 1); assert(result.column > 1);
  assert.equal(result.fix.kind, 'vega-lite-regression');
  const corrected = JSON.parse(result.fix.code);
  assert.deepEqual(corrected.layer[0].transform, [{ on: 'X', method: 'linear', as: ['fitX', 'fitY'], regression: 'Y' }]);
  assert.deepEqual(corrected.layer[0].data, spec.layer[0].data); assert.equal(JSON.stringify(spec), before);
  const warnings = []; vegaLite.compile(corrected, { logger: logger(warnings) });
  assert(!warnings.some(message => message.includes('invalid transform')));
  assert.equal(diagnostics.inspect('vega-lite', result.fix.code), null);
});

test('compiler-invalid transforms receive a short field diagnostic without source dumping', () => {
  const source = JSON.stringify(figure({ transform: [{ unknown: 'private chart label' }] }), null, 2), warnings = [];
  vegaLite.compile(JSON.parse(source), { logger: logger(warnings) });
  const warning = warnings.find(message => message.startsWith('Ignoring an invalid transform:'));
  assert(warning);
  const result = diagnostics.invalidTransform(source, warning);
  assert.equal(result.field, '$.transform[0]'); assert(result.line > 1);
  assert.equal(result.detail, 'Ignoring an invalid transform.');
  assert(!JSON.stringify(result).includes('private chart label'));
});

test('offline URL errors explain the field without copying the URL or ordinary data records', () => {
  const source = JSON.stringify(figure({ data: { url: 'https://private.example/data?token=PRIVATE_TOKEN' } }));
  const result = diagnostics.inspect('vega-lite', source);
  assert.equal(result.messageKey, 'reader.diagramExternalData'); assert.equal(result.field, '$.data.url');
  assert(!JSON.stringify(result).includes('private.example')); assert(!JSON.stringify(result).includes('PRIVATE_TOKEN'));
  assert.equal(diagnostics.inspect('vega-lite', JSON.stringify(figure({ data: { values: [{ data: { url: 'a label' } }] } }))), null);
});

test('engine details retain explanatory text while redacting secrets, locations and source excerpts', () => {
  const raw = 'Parse error on line 2: "token":"PRIVATE_TOKEN" /Users/summer/private.md /srv/custom/file.js https://host.test/api?token=PRIVATE_TOKEN\n...private chart source\n----^\nExpecting NAME, got EOF\n    at /workspace/private.js:3:4';
  const result = diagnostics.inspect('mermaid', 'graph TD\nA -->', new Error(raw));
  assert.equal(result.line, 2); assert.equal(result.column, null);
  for (const secret of ['PRIVATE_TOKEN', '/Users/', '/srv/', 'host.test', 'private chart source', 'private.js']) assert(!result.detail.includes(secret), secret);
  assert(result.detail.includes('Expecting NAME, got EOF'));
  assert(result.detail.length <= 520);
  assert.equal(diagnostics.inspect('mermaid', 'graph TD\nA -->', new Error('diagram_engine_timeout')).messageKey, 'reader.diagramTimeout');
});
