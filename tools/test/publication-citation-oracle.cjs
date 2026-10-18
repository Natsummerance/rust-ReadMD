'use strict';
// Independent RESULT files are loaded from private, offline QA evidence only.
// This file contains no upstream fixture/style/processor source or golden text.
const fs = require('node:fs');
const path = require('node:path');
const assert = require('node:assert/strict');
const crypto = require('node:crypto');
const {root, sha256} = require('./publication-batch3-harness.cjs');
const privateRoot = path.join(root, '.cache', 'private', 'publication-context-inputs');
const defaultIndex = path.join(privateRoot, 'oracle-index.json');
const realpath = fs.realpathSync.native || fs.realpathSync;

function sections(source) {
  const blocks = {};
  for (const match of source.matchAll(/>>=+ ([A-Z-]+) =+>>\r?\n([\s\S]*?)\r?\n<<=+ \1 =+<</g)) {
    assert(!Object.hasOwn(blocks, match[1]), 'Repeated fixture section: ' + match[1]);
    blocks[match[1]] = match[2];
  }
  for (const key of ['MODE', 'RESULT', 'CSL', 'INPUT']) assert(Object.hasOwn(blocks, key), 'Missing original fixture section: ' + key);
  return blocks;
}
function decode(text) {
  const entities = {amp: '&', lt: '<', gt: '>', quot: '"', apos: "'", nbsp: '\u00a0', lsquo: '‘', rsquo: '’', ldquo: '“', rdquo: '”', ndash: '–', mdash: '—'};
  return text.replace(/&(#x[\da-f]+|#\d+|[a-z]+);/gi, (raw, code) => {
    if (code.startsWith('#x')) return String.fromCodePoint(parseInt(code.slice(2), 16));
    if (code.startsWith('#')) return String.fromCodePoint(Number(code.slice(1)));
    assert(Object.hasOwn(entities, code), 'Unsupported HTML entity in independent oracle: ' + raw);
    return entities[code];
  });
}
// Declared equivalence: i/em/span font-style, b/strong/span font-weight,
// u/span underline, small-caps, and sup/sub/span vertical alignment. Only the
// eight fixed rich-format classes are equivalent to their explicit CSS marks;
// a class that disagrees with inline CSS is an error. Attribute
// order and structural indentation between csl-bib-body/csl-entry divs do not
// matter. All names, punctuation and text whitespace remain exact.
function canonicalHTML(source) {
  const result = [], stack = [{tag: '', marks: {}, href: null}], tokens = source.match(/<[^>]*>|[^<]+/g) || [];
  assert.equal(tokens.join(''), source, 'Unconsumed independent oracle HTML text');
  for (const [index, token] of tokens.entries()) {
    if (!token.startsWith('<')) {
      const text = decode(token);
      if (/^\s*$/.test(text) && /\n/.test(text) && /^<\/?div\b/.test(tokens[index - 1] || '') && /^<\/?div\b/.test(tokens[index + 1] || '')) continue;
      const top = stack.at(-1), item = {text, marks: Object.fromEntries(Object.entries(top.marks).sort()), href: top.href};
      const previous = result.at(-1);
      if (previous?.text !== undefined && JSON.stringify(previous.marks) === JSON.stringify(item.marks) && previous.href === item.href) previous.text += text;
      else if (text) result.push(item);
      continue;
    }
    const match = /^<(\/)?([a-z][\w-]*)([^>]*)>$/i.exec(token);
    assert(match, 'Unsupported independent oracle HTML token');
    const closing = Boolean(match[1]), tag = match[2].toLowerCase(), tail = match[3];
    assert(['span', 'i', 'em', 'b', 'strong', 'u', 'sup', 'sub', 'a', 'div', 'br'].includes(tag), 'Unsupported independent oracle HTML element: ' + tag);
    if (closing) {
      assert(/^[ \t\r\n\f]*$/.test(tail), 'Unexpected independent oracle closing-tag content');
      const closed = stack.pop(); assert(closed && closed.tag === tag && stack.length, 'Unbalanced independent oracle HTML');
      if (tag === 'div') result.push({blockEnd: closed.block});
      continue;
    }
    const attributes = {}; let remaining = tail;
    while (!/^[ \t\r\n\f]*$/.test(remaining)) {
      if (tag === 'br' && /^[ \t\r\n\f]*\/[ \t\r\n\f]*$/.test(remaining)) { remaining = ''; break; }
      const field = /^[ \t\r\n\f]+([\w:-]+)[ \t\r\n\f]*=[ \t\r\n\f]*(?:"([^"]*)"|'([^']*)')/.exec(remaining);
      assert(field, 'Unconsumed independent oracle HTML attribute');
      const key = field[1].toLowerCase(); assert(!Object.hasOwn(attributes, key), 'Duplicate independent oracle HTML attribute: ' + key);
      attributes[key] = decode(field[2] ?? field[3]); remaining = remaining.slice(field[0].length);
    }
    const allowedAttributes = tag === 'span' ? ['style', 'class'] : tag === 'a' ? ['href'] : tag === 'div' ? ['class'] : [];
    for (const key of Object.keys(attributes)) assert(allowedAttributes.includes(key), 'Unreviewed oracle HTML attribute: ' + key);
    if (tag === 'br') { result.push({text: '\n', marks: {}, href: null}); continue; }
    const current = stack.at(-1), marks = {...current.marks}, declared = {};
    if (tag === 'i' || tag === 'em') marks.style = 'italic';
    if (tag === 'b' || tag === 'strong') marks.weight = 'bold';
    if (tag === 'u') marks.decoration = 'underline';
    if (tag === 'sup' || tag === 'sub') marks.vertical = tag;
    for (const declaration of (attributes.style || '').split(';').filter(v => v.trim())) {
      const pair = /^([a-z-]+)\s*:\s*([^:]+)$/.exec(declaration.trim());
      assert(pair && pair[2].trim(), 'Unreviewed oracle CSS declaration');
      const key = pair[1], value = pair[2].trim();
      assert(['font-style', 'font-weight', 'font-variant', 'text-decoration', 'vertical-align', 'font-size'].includes(key), 'Unreviewed oracle CSS property: ' + key);
      if (key === 'font-style') declared.style = value;
      if (key === 'font-weight') declared.weight = value === '700' ? 'bold' : value;
      if (key === 'font-variant') declared.variant = value;
      if (key === 'text-decoration') declared.decoration = value;
      if (key === 'vertical-align') declared.vertical = value === 'super' ? 'sup' : value;
      if (key === 'font-size') declared.size = value;
    }
    const classMarks = {
      'readmd-csl-italic': ['style', 'italic'], 'readmd-csl-oblique': ['style', 'oblique'],
      'readmd-csl-bold': ['weight', 'bold'], 'readmd-csl-light': ['weight', '300'],
      'readmd-csl-smallcaps': ['variant', 'small-caps'], 'readmd-csl-underline': ['decoration', 'underline'],
      'readmd-csl-sup': ['vertical', 'sup'], 'readmd-csl-sub': ['vertical', 'sub'],
    };
    for (const name of (tag === 'span' ? attributes.class || '' : '').split(/\s+/).filter(Boolean)) {
      assert(Object.hasOwn(classMarks, name), 'Unreviewed oracle rich class: ' + name);
      const [key, value] = classMarks[name];
      assert(!Object.hasOwn(declared, key) || declared[key] === value, 'Oracle rich class disagrees with style: ' + name);
      declared[key] = value;
    }
    Object.assign(marks, declared);
    if (['sup', 'sub'].includes(marks.vertical) && marks.size === 'smaller') delete marks.size;
    for (const [key, value] of Object.entries(marks)) if (['normal', 'baseline', 'none'].includes(value)) delete marks[key];
    const block = tag === 'div' ? attributes.class || '' : undefined;
    if (tag === 'div') { assert(['csl-bib-body', 'csl-entry'].includes(block), 'Unreviewed oracle block'); result.push({blockStart: block}); }
    stack.push({tag, marks, href: tag === 'a' ? attributes.href : current.href, block});
  }
  assert.equal(stack.length, 1, 'Unclosed independent oracle HTML');
  return result;
}
function affixNodes(source) {
  // Processor fixture markup is an explicitly typed affix; API strings remain
  // literal text. No arbitrary source HTML is injected into the native DOM.
  return canonicalHTML(source.replaceAll('<sc>', '<span style="font-variant:small-caps;">').replaceAll('</sc>', '</span>')).map(item => {
    assert(item.text !== undefined && !item.href, 'Unreviewed rich fixture affix');
    const format = {};
    for (const [key, value] of Object.entries(item.marks)) {
      const field = {style: 'font_style', weight: 'font_weight', variant: 'font_variant', decoration: 'text_decoration', vertical: 'vertical_align'}[key];
      assert(field, 'Unreviewed affix formatting axis'); format[field] = value;
    }
    return {type: 'text', text: item.text, format};
  });
}
function citeItem(item, mode) {
  const result = {key: String(item.id)};
  for (const field of ['locator', 'prefix', 'suffix']) if (item[field] !== undefined) result[field] = String(item[field]);
  if (item.label !== undefined) result.kind = item.label;
  for (const field of ['author-only', 'suppress-author']) if (item[field] !== undefined) result[field] = Boolean(item[field]);
  if (mode === 'composite') result.mode = 'prose';
  for (const field of ['prefix', 'suffix']) if (result[field]?.includes('<')) result[field + '_rich'] = affixNodes(result[field]);
  const supported = new Set(['id', 'locator', 'label', 'prefix', 'suffix', 'author-only', 'suppress-author', 'position']);
  for (const key of Object.keys(item)) assert(supported.has(key), 'Unsupported original citation-item field: ' + key);
  // A fixture's position is checked through the sequence's derived state; the
  // public API does not expose a caller-controlled processor position enum.
  return result;
}
function adapt(blocks, {localeXml} = {}) {
  const input = JSON.parse(blocks.INPUT), request = {format: 'csl-json', data: JSON.stringify(input), csl: blocks.CSL, locale: 'en-US', style: 'apa', clusters: []};
  let expected = blocks.RESULT, adapter = 'ordered clusters';
  if (blocks.CITATIONS) {
    const calls = JSON.parse(blocks.CITATIONS), citations = new Map(); let final = [];
    for (const [citation, before, after] of calls) {
      citations.set(citation.citationID, citation);
      final = [...before, [citation.citationID, citation.properties?.noteIndex ?? 0], ...after].map(([id, note]) => {
        assert(citations.has(id), 'Fixture cites an unavailable citationID');
        return {...citations.get(id), noteIndex: Number(note)};
      });
    }
    request.clusters = final.map(citation => citation.citationItems.map(item => citeItem(item, citation.properties?.mode)));
    const inText = /<style\b[^>]*\bclass\s*=\s*["']in-text["']/.test(blocks.CSL);
    request.contexts = final.map(citation => {
      assert(Number.isSafeInteger(citation.noteIndex) && citation.noteIndex >= 0, 'Invalid original CITATIONS noteIndex');
      assert(citation.noteIndex !== 0 || inText, 'Note-style CITATIONS noteIndex 0 needs a separate semantic review');
      return {note_number: citation.noteIndex === 0 ? null : citation.noteIndex, in_note: false, note_label: null};
    });
    expected = expected.split('\n').map((line, index) => {
      const match = /^(?:\.\.|>>)\[(\d+)\] (.*)$/.exec(line);
      assert(match && Number(match[1]) === index, 'Unexpected original CITATIONS result framing'); return match[2];
    }).join('\n');
    adapter = 'final explicit CITATIONS order/noteIndex; in-text protocol noteIndex 0 means no note and maps to null; fixture update-notification markers are protocol framing only';
  } else if (blocks['CITATION-ITEMS']) request.clusters = JSON.parse(blocks['CITATION-ITEMS']).map(cluster => cluster.map(item => citeItem(item)));
  else request.clusters = [input.map(item => ({key: String(item.id)}))];
  if (localeXml !== undefined) {
    assert.equal(typeof localeXml, 'string'); request.locale_xml = localeXml;
    adapter += '; explicit original runtime locale XML, preserving the original CSL and RESULT bytes';
  }
  const mode = blocks.MODE.trim(); assert(['citation', 'bibliography'].includes(mode), 'Unsupported fixture MODE');
  return {request, expected, adapter, mode};
}
function loadHistoricalLocale() {
  const commit = '195bef843f87fc917be45ab3eca60536e4ef4626';
  const target = realpath(path.join(privateRoot, 'locale-history', 'locales-en-US.' + commit + '.xml'));
  assert(target.startsWith(realpath(privateRoot) + path.sep), 'Historical locale must remain inside private QA evidence');
  const bytes = fs.readFileSync(target), digest = sha256(bytes);
  assert.equal(digest, '55555f07f931a4b53af2f7eda33284e0f2c4063a6d3f4d930fb1bff9d417770e', 'Pinned original runtime locale SHA changed');
  const blob = crypto.createHash('sha1').update(Buffer.from(`blob ${bytes.length}\0`)).update(bytes).digest('hex');
  assert.equal(blob, '121781576ec9c385e78cd3384b4418697dbe8d7e', 'Pinned original runtime locale Git blob changed');
  return {xml: bytes.toString('utf8'), provenance: {
    repository: 'citation-style-language/locales', commit, path: 'locales-en-US.xml', sha256: digest, gitBlobSha1: blob,
    licenseStatus: 'CC-BY-SA-3.0 explicitly declared by the original locale file; private QA input only',
    runtimeRepository: 'Juris-M/citeproc-js', runtimeCommit: 'cc9153c45293af878de08cafddbefe6ea150c380',
    runtimeGitmodulesBlob: 'f22ddc19fca7f3317a37de1d140e593f98eb8970', runtimeTestRunnerBlob: '9cf97aed84c7997f3711d840ad5b9d760768e2da',
    reason: 'The fixed runtime locale gitlink selects this official locale commit, and retrieveLocale reads locale/locales-<lang>.xml. This supplies its original singular translator term without modifying the fixture style/RESULT or current product defaults.',
  }};
}
function loadPrivateFixtures(indexFile = process.env.READMD_PRIVATE_CSL_ORACLE_INDEX || defaultIndex) {
  const absolute = realpath(indexFile), allowed = realpath(privateRoot);
  assert(absolute.startsWith(allowed + path.sep), 'Original fixture index must remain inside the private QA cache');
  const index = JSON.parse(fs.readFileSync(absolute, 'utf8'));
  return index.fixtures.filter(item => item.path.endsWith('.txt')).map(item => {
    const target = realpath(path.join(root, item.localPath));
    assert(target.startsWith(allowed + path.sep), 'Original fixture escaped the private QA cache');
    const bytes = fs.readFileSync(target); assert.equal(sha256(bytes), item.sha256, 'Original fixture SHA changed: ' + item.path);
    const blob = crypto.createHash('sha1').update(Buffer.from(`blob ${bytes.length}\0`)).update(bytes).digest('hex');
    assert.equal(blob, item.gitBlobSha1, 'Original fixture no longer matches GitHub’s fixed blob');
    const blocks = sections(bytes.toString('utf8')); assert.equal(sha256(blocks.RESULT), item.resultSha256, 'Original RESULT changed');
    return {provenance: item, blocks};
  });
}
async function runCitationOracle(call, {indexFile, summaryDirectory, runName = 'oracle'} = {}) {
  const fixtures = loadPrivateFixtures(indexFile), results = [];
  const detailDirectory = fs.mkdtempSync(path.join(privateRoot, 'run-'));
  for (const fixture of fixtures) {
    const p = fixture.provenance;
    const optionalIntext = p.repository === 'Juris-M/citeproc-js' && p.path === 'fixtures/local/discretionary_AuthorDateAuthorOnlyThenSuppressAuthor.txt';
    const record = {repository: p.repository, commit: p.commit, path: p.path, fixtureSha256: p.sha256, resultSha256: p.resultSha256, licenseStatus: p.licenseStatus,
      mandatory: !optionalIntext, classification: optionalIntext ? 'optional-citeproc-js-cs-intext-compatibility' : 'required-citation-context', passed: false};
    let actual, adapted, response;
    try {
      const historic = p.repository === 'Juris-M/citeproc-js' && p.path === 'fixtures/local/discretionary_SuppressAuthor.txt' ? loadHistoricalLocale() : null;
      adapted = adapt(fixture.blocks, historic ? {localeXml: historic.xml} : {}); record.adapter = adapted.adapter;
      if (historic) record.locale = historic.provenance;
      record.styleSha256 = sha256(fixture.blocks.CSL); record.requestStyleSha256 = sha256(adapted.request.csl);
      assert.equal(record.requestStyleSha256, record.styleSha256, 'The independent fixture style was altered');
      record.requestSha256 = sha256(JSON.stringify(adapted.request));
      response = await call('/api/academic/citations', adapted.request);
      assert.equal(response.status, 200, 'Actual citation API status: ' + response.status);
      assert.equal(response.data.ok, true, 'Actual citation API rejected the independent fixture');
      actual = adapted.mode === 'citation' ? response.data.citations.map(item => item.html).join('\n') :
        '<div class="csl-bib-body">' + (response.data.bibliography?.items || []).map(item => '<div class="csl-entry">' + item.html + '</div>').join('') + '</div>';
      record.actualSha256 = sha256(actual);
      assert.deepEqual(canonicalHTML(actual), canonicalHTML(adapted.expected));
      record.passed = true;
    } catch (error) {
      // Each fixture runs once. Continue to other independent cases and retain
      // every original mismatch privately; no retry or replacement golden.
      const detail = {...record, expectedOriginalResult: fixture.blocks.RESULT, adaptedExpected: adapted?.expected, actualHtml: actual, actualResponse: response, error: String(error.stack || error.message)};
      fs.writeFileSync(path.join(detailDirectory, String(results.length).padStart(3, '0') + '.json'), JSON.stringify(detail, null, 2) + '\n');
      record.failureKind = error.name || 'Error';
    }
    results.push(record);
  }
  const mandatory = results.filter(item => item.mandatory), optional = results.filter(item => !item.mandatory);
  const summary = {passed: results.every(item => item.passed), fixtures: results.length, failures: results.filter(item => !item.passed).length,
    mandatoryFixtures: mandatory.length, mandatoryFailures: mandatory.filter(item => !item.passed).length, mandatoryPassed: mandatory.every(item => item.passed),
    optionalCompatibility: optional, optionalUnsupported: optional.filter(item => !item.passed).map(item => ({...item, reason: 'Optional citeproc-js cs:intext lies outside the CSL 1.0.2 required contract; the actual original failure remains privately recorded.'})),
    noAutomaticRetries: true,
    normalization: 'Only documented equivalent inline tags/CSS, attribute order, and CSL bibliography wrapper indentation. Citation text/punctuation/whitespace are exact.',
    scope: 'Independent upstream final rendering; does not certify citeproc-js incremental update notifications or redistribute unknown-license originals.', results};
  fs.writeFileSync(path.join(detailDirectory, 'summary.json'), JSON.stringify(summary, null, 2) + '\n');
  if (summaryDirectory) fs.writeFileSync(path.join(summaryDirectory, runName + '-summary.json'), JSON.stringify(summary, null, 2) + '\n');
  return summary;
}
module.exports = {sections, canonicalHTML, adapt, loadPrivateFixtures, loadHistoricalLocale, runCitationOracle};
if (require.main === module) {
  const {createHarness} = require('./publication-batch3-harness.cjs');
  const h = createHarness('publication-citation-oracle', 29127, __filename);
  h.finish(async () => {
    await h.start();
    await h.step('fixed independent upstream RESULT oracle over the real native citation API', async () => {
      h.report.oracle = await runCitationOracle(h.api, {summaryDirectory: h.data});
      assert.equal(h.report.oracle.mandatoryFixtures, 20);
      assert.equal(h.report.oracle.optionalCompatibility.length, 1);
      assert.equal(h.report.oracle.mandatoryFailures, 0, 'Mandatory independent oracle mismatches retained; no fixture was retried');
    });
  });
}
