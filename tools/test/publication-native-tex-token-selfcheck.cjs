'use strict';
// QA-only contract calibration: no ReadMD process or product request.
const fs = require('node:fs'), path = require('node:path'), crypto = require('node:crypto'), assert = require('node:assert/strict');
const {checkNativeTexMetadata} = require('../../ui-tests/publication-engines-native.cjs');
const root = path.resolve(__dirname, '../..'), data = path.join(root, '.cache/capability-upgrade/publication-context-inputs/native-tex-token-selfcheck-20261014-first');
assert(!fs.existsSync(data), 'Preserve the first calibration evidence'); fs.mkdirSync(data);
const digest = file => crypto.createHash('sha256').update(fs.readFileSync(file)).digest('hex');
const fontName = 'readmd-core-7da195a74c55bef988d0d48f9508bd5d849425c1770dba5d7bfc6ce9ed848954.ttf';
const licenseName = fontName.slice(0, -4) + '.LICENSE.txt';
const font = fs.readFileSync(path.join(root, 'assets/fonts/DejaVuSans.ttf'));
const license = fs.readFileSync(path.join(root, 'assets/fonts/DejaVuSans.LICENSE.txt'));
const correct = path.join(data, 'correct'), missing = path.join(data, 'missing-font'), missingLicense = path.join(data, 'missing-license'), corrupt = path.join(data, 'corrupt-font');
for (const folder of [correct, missing, missingLicense, corrupt]) fs.mkdirSync(folder);
for (const folder of [correct, missingLicense]) fs.writeFileSync(path.join(folder, fontName), font, {flag: 'wx'});
for (const folder of [correct, missing, corrupt]) fs.writeFileSync(path.join(folder, licenseName), license, {flag: 'wx'});
fs.writeFileSync(path.join(corrupt, fontName), Buffer.alloc(font.length), {flag: 'wx'});
const declaration = '\\DeclareRobustCommand{\\readmdunicode}[1]{{\\normalfont\\readmdcorefallback #1}}\n';
const text = declaration + 'Keywords: Unicode \\readmdunicode{😀}\nExact, BMP 中文\n';
const tokens = ['Unicode 😀', 'Exact, BMP 中文'];
const cases = [];
function record(name, expected, operation) {
  let accepted = false, rejection = null, proof = null;
  try {proof = operation(); accepted = true;} catch (error) {rejection = error.code || error.name;}
  cases.push({name, expectedAccepted: expected, actualAccepted: accepted, rejection, proof});
  assert.equal(accepted, expected, name);
}
record('exact-original-smp-macro-and-locked-font-license', true, () => checkNativeTexMetadata(text, tokens, correct));
record('original-bmp-tokens-stay-literal-and-need-no-smp-resource', true, () => checkNativeTexMetadata('Exact, BMP 中文', ['Exact, BMP 中文'], missing));
record('missing-original-smp-character', false, () => checkNativeTexMetadata(text.replace('\\readmdunicode{😀}', '\\readmdunicode{}'), tokens, correct));
record('changed-original-unicode', false, () => checkNativeTexMetadata(text.replace('\\readmdunicode{😀}', '\\readmdunicode{🙂}'), tokens, correct));
record('fake-fallback-macro', false, () => checkNativeTexMetadata(text.replace('\\readmdunicode{😀}', '\\readmdunicodex{😀}'), tokens, correct));
record('raw-smp-does-not-substitute-for-controlled-macro', false, () => checkNativeTexMetadata(text.replace('\\readmdunicode{😀}', '😀'), tokens, correct));
record('missing-controlled-command-definition', false, () => checkNativeTexMetadata(text.slice(declaration.length), tokens, correct));
record('missing-actual-copied-font', false, () => checkNativeTexMetadata(text, tokens, missing));
record('missing-actual-copied-license', false, () => checkNativeTexMetadata(text, tokens, missingLicense));
record('same-length-corrupt-font', false, () => checkNativeTexMetadata(text, tokens, corrupt));
record('changed-non-smp-token-still-rejects', false, () => checkNativeTexMetadata(text.replace('Exact, BMP 中文', 'Exact BMP 中文'), tokens, correct));
const report = {schemaVersion: 1, recordedUtc: new Date().toISOString(), passed: true, scope: 'QA token/resource contract calibration only; not native/product acceptance',
  candidateExecutableExecuted: false, productRequests: 0, originalFixtureChanged: false,
  engineScriptSha256: digest(path.join(root, 'ui-tests/publication-engines-native.cjs')), selfcheckScriptSha256: digest(__filename),
  originalFontSha256: digest(path.join(root, 'assets/fonts/DejaVuSans.ttf')), originalLicenseSha256: digest(path.join(root, 'assets/fonts/DejaVuSans.LICENSE.txt')), cases};
const target = path.join(data, 'report.json'); fs.writeFileSync(target, JSON.stringify(report, null, 2) + '\n', {flag: 'wx'});
console.log(JSON.stringify({passed: true, cases: cases.length, positive: cases.filter(item => item.expectedAccepted).length,
  negative: cases.filter(item => !item.expectedAccepted).length, report: path.relative(root, target).replaceAll(path.sep, '/'), sha256: digest(target), scope: report.scope}));
