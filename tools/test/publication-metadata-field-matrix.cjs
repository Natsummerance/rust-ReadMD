'use strict';
// Independent P01 QA input and actual artifact assertions, shared by full UI
// acceptance and the explicitly separate API/output diagnostic. No writer code.
const fs = require('node:fs'), path = require('node:path'), assert = require('node:assert/strict');
const {paperMetadata, bookMetadata, sourceFor} = require('./publication-batch3-fixtures.cjs');
const {assertMetadata, inspectArtifact, checkMetadataArtifact} = require('./publication-batch3-checks.cjs');
const {fileHash, sha256} = require('./publication-batch3-harness.cjs');
const contentLabelsPath = path.resolve(__dirname, '../../assets/data/publication-metadata-i18n.json');
const contentLabelInputs = JSON.parse(fs.readFileSync(contentLabelsPath, 'utf8')).contentLabels;

function metadataFieldCases() {
  const cases = [];
  const add = (id, source, fields, change, explicitMeta) => {
    const expected = structuredClone(source === 'paper' ? paperMetadata : bookMetadata);
    change(expected);
    const meta = explicitMeta || Object.fromEntries([...new Set(fields.map(field => field.split(/[.[]/)[0]))].map(field => [field, structuredClone(expected[field])]));
    cases.push({id, source, fields, expected, meta});
  };
  add('01-title', 'paper', ['title'], m => {m.title = 'Matrix evidence: title, intact';});
  add('02-subtitle', 'book', ['subtitle'], m => {m.subtitle = 'Independent subtitle, edition B';});
  add('03-author-name', 'paper', ['authors[0].name'], m => {m.authors[0].name = 'QA, Linh Matrix';});
  add('04-author-given', 'paper', ['authors[0].given'], m => {m.authors[0].given = 'Linh Matrix Given';});
  add('05-author-family', 'paper', ['authors[0].family'], m => {m.authors[0].family = 'Nguyen Matrix Family';});
  add('06-author-file-as', 'paper', ['authors[0].fileAs'], m => {m.authors[0].fileAs = 'Matrix, Nguyen Linh';});
  add('07-author-orcid', 'paper', ['authors[0].orcid'], m => {m.authors[0].orcid = '0000-0000-0000-0028';});
  add('08-author-corresponding', 'paper', ['authors[0].corresponding'], m => {m.authors[0].corresponding = false;});
  add('09-author-order', 'paper', ['authors'], m => {m.authors.reverse();});
  add('10-author-affiliations', 'paper', ['authors[0].affiliationIds'], m => {m.authors[0].affiliationIds = ['east'];});
  add('11-institution-name', 'paper', ['affiliations[0].name'], m => {m.affiliations[0].name = 'Matrix Institute, Evidence';});
  add('12-institution-department', 'paper', ['affiliations[0].department'], m => {m.affiliations[0].department = 'Independent Department, B';});
  add('13-institution-address', 'paper', ['affiliations[0].address'], m => {m.affiliations[0].address = '42 Example Avenue, Matrix City';});
  add('14-abstract-text', 'paper', ['abstracts[0].text'], m => {m.abstracts[0].text = 'Independent abstract B: commas, Unicode and exact metadata survive.';});
  add('15-abstract-language', 'paper', ['abstracts[0].language'], m => {m.abstracts[0].language = 'en-GB';});
  add('16-keywords', 'paper', ['keywords'], m => {m.keywords = ['matrix, one intact keyword', '独立关键词', 'Unicode 😀'];});
  add('17-funding-funder', 'paper', ['funding[0].funder'], m => {m.funding[0].funder = 'Matrix Fund, Synthetic Programme';});
  add('18-funding-award', 'paper', ['funding[0].award'], m => {m.funding[0].award = 'QA-000043';});
  add('19-funding-recipients', 'paper', ['funding[0].recipientIds'], m => {m.funding[0].recipientIds = ['mo'];});
  add('20-translator', 'book', ['translators[0].name'], m => {m.translators[0].name = '译者，明（矩阵）';});
  add('21-publisher', 'book', ['publisher'], m => {m.publisher = 'Matrix Publishing, Independent Editions';});
  add('22-date-precision', 'book', ['date'], m => {m.date = '2027';});
  add('23-rights', 'book', ['rights'], m => {m.rights = 'Synthetic matrix B rights; exact commas, no participants.';});
  add('24-edition', 'book', ['edition'], m => {m.edition = 'Third, independent edition';});
  add('25-series-title', 'book', ['series.title'], m => {m.series.title = 'Independent Series, B';});
  add('26-series-number', 'book', ['series.number'], m => {m.series.number = '0042.2.1';});
  add('27-identifier-scheme', 'book', ['identifiers[0].scheme'], m => {m.identifiers[0].scheme = 'ISBN';});
  add('28-identifier-value', 'book', ['identifiers[0].value'], m => {m.identifiers[0].value = '978-0-00-000001-9';});
  add('29-identifier-primary', 'book', ['identifiers[0].primary', 'identifiers[1].primary'], m => {m.identifiers[0].primary = false; m.identifiers[1].primary = true;});
  add('30-language', 'book', ['language'], m => {m.language = 'de-DE';});
  add('31-null-inherit', 'paper', ['authors', 'subtitle'], () => {}, {authors: null, subtitle: null});
  add('32-scalar-clear', 'book', ['subtitle'], m => {m.subtitle = null;}, {clear: ['subtitle']});
  add('33-list-clear', 'paper', ['keywords'], m => {m.keywords = [];}, {keywords: []});
  add('34-person-id-linked', 'paper', ['authors[0].id', 'funding[0].recipientIds'], m => {m.authors[0].id = 'linh-matrix'; m.funding[0].recipientIds = ['linh-matrix', 'mo'];});
  add('35-institution-id-linked', 'paper', ['affiliations[1].id', 'authors[0].affiliationIds', 'authors[1].affiliationIds'], m => {m.affiliations[1].id = 'east-matrix'; for (const person of m.authors) person.affiliationIds = person.affiliationIds.map(id => id === 'east' ? 'east-matrix' : id);});
  assert.equal(cases.length, 35);
  return cases;
}


function nativeMetadataDetails(inspected, expected, mode) {
  const values = inspected.kind === 'docx' ? Object.values(inspected.custom) : inspected.kind === 'pdf' ? inspected.extensionText.split('\n') : inspected.metadata.map(item => item.text);
  const models = values.flatMap(value => {try {const model = typeof value === 'string' ? JSON.parse(value) : value; return model && Array.isArray(model.authors) ? [model] : [];} catch {return [];}});
  assert.equal(models.length, 1, 'Every format must carry one inspectable complete publication model');
  assertMetadata(models[0], expected);
  if (inspected.kind === 'pdf') {
    const xml = value => value.replaceAll('&', '&amp;').replaceAll('<', '&lt;').replaceAll('>', '&gt;').replaceAll('"', '&quot;').replaceAll("'", '&apos;');
    for (const item of expected.abstracts || []) if (item.language) assert.equal(inspected.dcDescription[item.language], item.text, 'XMP abstract language changed');
    if (expected.date) assert(inspected.xmp.includes('<dc:date><rdf:Seq><rdf:li>' + expected.date + '</rdf:li></rdf:Seq></dc:date>'), 'XMP publication date lost its exact precision');
    if (expected.publisher) assert(inspected.xmp.includes('<dc:publisher><rdf:Bag><rdf:li>' + xml(expected.publisher) + '</rdf:li></rdf:Bag></dc:publisher>'), 'Publisher did not reach its standard XMP array');
    if (expected.translators?.length) assert(inspected.xmp.includes('<dc:contributor><rdf:Bag>' + expected.translators.map(person => '<rdf:li>' + xml(person.name) + '</rdf:li>').join('') + '</rdf:Bag></dc:contributor>'), 'Translators did not reach the standard XMP contributor array');
  }
  if (inspected.kind === 'epub') {
    for (const [index, person] of (expected.authors || []).entries()) if (person.fileAs) assert(inspected.metadata.some(item => item.attributes.refines === '#' + inspected.creators[index].attributes.id && item.attributes.property === 'file-as' && item.text === person.fileAs), 'EPUB author sort name changed');
    for (const item of expected.abstracts || []) if (item.language) assert(inspected.metadata.some(actual => actual.tag === 'description' && actual.text === item.text && actual.attributes['{http://www.w3.org/XML/1998/namespace}lang'] === item.language), 'EPUB abstract language changed');
    if (expected.edition) {
      const title = inspected.metadata.find(item => item.tag === 'title' && item.text === expected.edition);
      assert(title && inspected.metadata.some(item => item.attributes.refines === '#' + title.attributes.id && item.attributes.property === 'title-type' && item.text === 'edition'), 'Edition did not reach the native EPUB title refinement');
    }
    for (const identifier of expected.identifiers || []) if (identifier.scheme.toLowerCase() === 'isbn') {
      const actual = inspected.identifiers.find(item => item.text === identifier.value);
      const onix = identifier.value.replace(/[^0-9]/g, '').length === 13 ? '15' : '02';
      assert(actual && inspected.metadata.some(item => item.attributes.refines === '#' + actual.attributes.id && item.attributes.property === 'identifier-type' && item.attributes.scheme === 'onix:codelist5' && item.text === onix), 'ISBN native refinement was lost or points to a duplicate primary identifier');
    }
  }
  if (mode === 'none') return;
  const text = inspected.text.replace(/\s+/gu, ' ');
  const compact = value => value.replace(/\s+/gu, '');
  const label = (key, language = expected.language) => {
    const values = contentLabelInputs[language] || contentLabelInputs[language?.split('-')[0]];
    assert(values && typeof values[key] === 'string', 'Independent content-label input is missing: ' + language + '.' + key);
    return values[key];
  };
  const corresponding = label('corresponding');
  assert.equal(text.includes(corresponding), (expected.authors || []).some(person => person.corresponding), 'Corresponding-author flag did not control visible front matter');
  for (const author of expected.authors || []) {
    const marks = (author.affiliationIds || []).map(id => (expected.affiliations || []).findIndex(item => item.id === id) + 1);
    assert(marks.every(mark => mark > 0));
    const prefix = author.name + (marks.length ? ' [' + marks.join(', ') + ']' : '');
    assert(text.includes(prefix), 'Visible author-to-institution marks changed: ' + prefix);
  }
  const authorPositions = (expected.authors || []).map(person => text.indexOf(person.name));
  assert(authorPositions.every((position, index) => position >= 0 && (index === 0 || position > authorPositions[index - 1])), 'Visible author order changed');
  for (const [index, institution] of (expected.affiliations || []).entries()) {
    const line = '[' + (index + 1) + '] ' + institution.name + [institution.department, institution.address].filter(Boolean).map(value => ' · ' + value).join('');
    assert(compact(text).includes(compact(line)), 'Visible institution name/department/address relationship changed');
  }
  if (mode === 'paper') for (const funding of expected.funding || []) {
    const recipients = (funding.recipientIds || []).map(id => expected.authors.find(person => person.id === id)?.name);
    assert(recipients.every(Boolean));
    const line = funding.funder + (funding.award ? ' (' + funding.award + ')' : '') + (recipients.length ? ' — ' + recipients.join('; ') : '');
    assert(text.includes(line), 'Visible funding-to-author relationship changed: ' + line);
  }
  if (mode === 'paper') for (const item of expected.abstracts || []) assert(compact(text).includes(compact(label('abstract', item.language || expected.language) + item.text)), 'Abstract heading did not use its declared content language');
  if (mode === 'book') for (const key of ['publisher', 'date', 'rights', 'edition']) if (expected[key]) assert(compact(text).includes(compact(label(key) + ': ' + expected[key])), 'Book field heading did not use the document language: ' + key);
  if (mode === 'book') for (const identifier of expected.identifiers || []) assert(compact(text).includes(compact(identifier.scheme + ': ' + identifier.value)), 'Visible identifier scheme/value was changed');
}


async function exportMetadataArtifact(h, baseOptions, name, format, content, expected, mode, extra = {}, qa = {}) {
  const file = path.join(h.outputs, name + '.' + format);
  const result = await h.api('/api/export', {format, content, baseDir: h.docs, out_path: file, options: {...baseOptions, frontMatter: {mode}, ...extra}});
  qa.recordResponse?.(result);
  assert.equal(result.status, 200, JSON.stringify(result.data)); assert.equal(result.data.ok, true, JSON.stringify(result.data));
  const evidence = {file: path.basename(file), sha256: fileHash(file), bytes: fs.statSync(file).size, validated: false};
  h.report.outputs ||= []; h.report.outputs.push(evidence);
  const inspected = inspectArtifact(h, file, format === 'pdf' ? {glyphs: true, ...(qa.renderPdf !== false ? {render: path.join(h.data, name + '-pages')} : {})} : {});
  checkMetadataArtifact(inspected, expected, mode);
  if (format === 'pdf' && mode === 'paper' && (expected.keywords || []).some(value => value.includes('😀'))) {
    assert(inspected.pypdfText.includes('😀'), 'pypdf lost U+1F600 from actual PDF page text');
    assert(inspected.text.includes('😀'), 'MuPDF lost U+1F600 from actual PDF page text');
    const glyphs = inspected.pages.flatMap(page => page.physicalAstralGlyphs);
    assert(glyphs.length, 'U+1F600 has no physical embedded DejaVu glyph');
    for (const glyph of glyphs) {
      assert.equal(glyph.unicode, 0x1f600); assert(glyph.glyphId > 0);
      assert.match(glyph.font, /DejaVuSans/); assert(glyph.fontBytes > 0);
      assert.equal(glyph.unitsPerEm, 2048); assert.equal(glyph.advanceUnits, 2135);
      assert.deepEqual(glyph.inkBounds, [170, -150, 1965, 1646]);
      assert(glyph.darkPixels > 40, 'Embedded emoji outline did not paint real pixels');
    }
    evidence.astralGlyph = {codepoint: 'U+1F600', pypdf: true, mupdf: true, glyphs};
  }
  const primary = expected.identifiers?.find(item => item.primary) || expected.identifiers?.[0];
  if (format === 'pdf' && primary) assert.deepEqual(inspected.dcIdentifierText, [primary.value], 'PDF primary dc:identifier must remain one text property');
  if (format === 'epub') {
    assert.equal(inspected.identifiers.length, expected.identifiers?.length || 1, 'EPUB duplicated the primary identifier');
    for (const person of inspected.creators) assert(inspected.metadata.some(item =>
      item.attributes.refines === '#' + person.attributes.id && item.attributes.property === 'role' && item.attributes.scheme === 'marc:relators' && item.text === 'aut'), 'EPUB creator is missing its native MARC author role');
  }
  evidence.validated = true;
  return {file, inspected};
}

async function runMetadataFieldMatrix(h, baseOptions, expectedInputs, {collectAll = false} = {}) {
  const cases = metadataFieldCases(), formats = ['docx', 'pdf', 'epub'];
  const file = path.join(h.data, 'metadata-field-matrix.json');
  const scope = {
    givenFamily: 'Complete custom/extension model; explicit full name remains the display name.',
    fileAs: 'EPUB file-as refinement plus complete DOCX/PDF custom/extension values.',
    identitiesAndReferences: 'Complete stable IDs; visible institution marks and funding recipient names. No native Office or reader institution UI is certified.',
    orcidCorresponding: 'Complete model and visible front matter; no external ORCID lookup.',
    identifierScheme: 'Exact scheme spelling, ISBN refinement and visible book label; identifiers are never numerically rewritten.',
    applications: 'Word and independent EPUB desktop application opening/editing remain separately unverified.',
  };
  const outputSlots = prefix => Object.fromEntries(formats.map(format => [format, {file: prefix + '.' + format, status: 'not-run', validated: false}]));
  const sources = {}, baselines = {};
  for (const source of ['paper', 'book']) {
    const expected = source === 'paper' ? paperMetadata : bookMetadata;
    sources[source] = sourceFor(expected, '# Matrix body\n\nSynthetic matrix source sentinel remains unchanged.\n', {bibliography: false});
    const input = path.join(h.data, 'matrix-' + source + '-input.md'); fs.writeFileSync(input, sources[source]);
    baselines[source] = {id: 'A-' + source, source, input: path.basename(input), inputSha256: sha256(sources[source]), expected, options: {...baseOptions, frontMatter: {mode: source}}, outputs: outputSlots('Matrix-A-' + source), passed: false};
  }
  const index = {version: 2, passed: false, scope: h.report.scope, collectAllDistinctCases: collectAll, requestsPerCaseFormat: 1, binarySha256: h.report.binarySha256, bootSha256: h.report.bootSha256, contentLabelsSha256: fileHash(contentLabelsPath), plannedCaseCount: cases.length, plannedOutputCount: (cases.length + 2) * formats.length, formats, capabilityScope: scope, baselines,
    cases: cases.map(item => ({id: item.id, source: item.source, fields: item.fields, baseline: 'baselines.' + item.source, inputSha256: baselines[item.source].inputSha256, options: {...baseOptions, frontMatter: {mode: item.source}, meta: item.meta}, expected: item.expected, outputs: outputSlots('Matrix-B-' + item.id), passed: false})), failures: []};
  const records = () => [...Object.values(index.baselines), ...index.cases];
  const slots = () => records().flatMap(record => Object.values(record.outputs));
  const save = () => {
    const entries = slots();
    index.counts = {attempted: entries.filter(entry => entry.status !== 'not-run').length, completed: entries.filter(entry => ['passed', 'failed'].includes(entry.status)).length,
      exported: entries.filter(entry => entry.response?.status === 200 && entry.response?.data?.ok === true).length,
      artifacts: entries.filter(entry => entry.sha256).length, validated: entries.filter(entry => entry.validated).length,
      failed: entries.filter(entry => entry.status === 'failed').length, notRun: entries.filter(entry => entry.status === 'not-run').length};
    index.passedCaseCount = index.cases.filter(record => record.passed).length;
    h.report.fieldMatrix = {path: path.basename(file), scope: index.scope, plannedCaseCount: cases.length, passedCaseCount: index.passedCaseCount, plannedOutputCount: index.plannedOutputCount, ...index.counts, capabilityScope: scope};
    fs.writeFileSync(file, JSON.stringify(index, null, 2) + '\n');
  };
  save();
  const inputsUnchanged = () => {
    h.unchanged(expectedInputs);
    for (const record of Object.values(baselines)) assert.equal(fileHash(path.join(h.data, record.input)), record.inputSha256, 'Matrix source input was changed by export');
  };
  async function execute(record, format) {
    const entry = record.outputs[format], artifact = path.join(h.outputs, entry.file);
    entry.status = 'running'; entry.startedAt = new Date().toISOString(); save();
    let failure;
    try {
      const extra = record.options.meta === undefined ? {} : {meta: record.options.meta};
      const result = await exportMetadataArtifact(h, baseOptions, path.basename(entry.file, '.' + format), format, sources[record.source], record.expected, record.source, extra,
        {renderPdf: false, recordResponse: response => {entry.response = response; save();}});
      nativeMetadataDetails(result.inspected, record.expected, record.source);
      const text = result.inspected.text.replace(/\s+/gu, ' ');
      if (record.id === '08-author-corresponding') assert(!text.includes('Corresponding author'), 'A cleared corresponding-author flag remained visible');
      if (record.id === '32-scalar-clear') assert(!text.includes(bookMetadata.subtitle), 'Cleared subtitle remained in front matter');
      if (record.id === '33-list-clear') {
        assert(!text.includes('evidence, context'), 'Cleared keyword list remained visible');
        if (format === 'docx') assert.equal(result.inspected.core.keywords, undefined);
        if (format === 'pdf') assert.equal(result.inspected.info['/Keywords'], undefined);
      }
      inputsUnchanged(); entry.inputsUnchanged = true; entry.validated = true; entry.status = 'passed';
    } catch (error) {
      failure = error; entry.status = 'failed';
      entry.error = {message: String(error.message).slice(0, 2400), stack: String(error.stack || error).replaceAll(h.data, '<owned-profile>').replaceAll(h.root, '<workspace>').slice(0, 4000)};
      const detail = {case: record.id, format, file: entry.file, responseStatus: entry.response?.status, errorCode: entry.response?.data?.error_code, error: entry.error.message};
      index.failures.push(detail); h.report.firstOutputFailure ||= detail;
    } finally {
      if (fs.existsSync(artifact)) {entry.sha256 = fileHash(artifact); entry.bytes = fs.statSync(artifact).size;}
      entry.finishedAt = new Date().toISOString(); record.passed = formats.every(kind => record.outputs[kind].status === 'passed'); save();
      console.log(JSON.stringify({scope: index.scope, stage: 'metadata-field-matrix', case: record.id, format, passed: entry.validated, artifact: entry.file, responseStatus: entry.response?.status, errorCode: entry.response?.data?.error_code, error: entry.error?.message.slice(0, 600), completed: index.counts.completed, total: index.plannedOutputCount}));
    }
    if (failure && !collectAll) throw failure;
  }
  for (const record of records()) for (const format of formats) await execute(record, format);
  inputsUnchanged(); index.inputsUnchanged = true;
  index.passed = index.counts.completed === index.plannedOutputCount && index.counts.validated === index.plannedOutputCount && index.failures.length === 0;
  index.outputCount = index.counts.artifacts; save();
  h.report.fieldMatrix.outputCount = index.outputCount; h.report.fieldMatrix.passed = index.passed; h.report.fieldMatrix.sha256 = fileHash(file);
  return index;
}

module.exports = {metadataFieldCases, nativeMetadataDetails, exportMetadataArtifact, runMetadataFieldMatrix, contentLabelsPath};
