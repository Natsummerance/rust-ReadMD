'use strict';
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');

function assertMetadata(actual, expected) {
  assert(actual && typeof actual === 'object', 'Missing shared metadata model');
  for (const [field, value] of Object.entries(expected)) {
    if (Array.isArray(value)) {
      assert(Array.isArray(actual[field]), 'Structured metadata became a string: ' + field);
      assert.equal(actual[field].length, value.length, 'Metadata array length changed: ' + field);
      for (const [index, item] of value.entries()) {
        if (item && typeof item === 'object') for (const [key, target] of Object.entries(item)) assert.deepEqual(actual[field][index][key], target, `${field}[${index}].${key}`);
        else assert.deepEqual(actual[field][index], item, `${field}[${index}]`);
      }
    } else if (value && typeof value === 'object') {
      for (const [key, target] of Object.entries(value)) assert.deepEqual(actual[field]?.[key], target, field + '.' + key);
    } else assert.equal(actual[field], value, field);
  }
}
function meaningfulStrings(value) {
  if (typeof value === 'string') return [value];
  if (Array.isArray(value)) return value.flatMap(meaningfulStrings);
  if (value && typeof value === 'object') return Object.entries(value).filter(([key]) => !['id', 'affiliationIds', 'recipientIds', 'scheme', 'language'].includes(key)).flatMap(([, item]) => meaningfulStrings(item));
  return [];
}
function hasAll(text, values, label) {
  for (const value of values.filter(Boolean)) assert(text.includes(value), label + ' dropped expected text: ' + value);
}
function visibleText(text, kind) {
  // PDF extractors add a physical line ending between adjacent Han characters.
  // Preserve actual spaces and Latin word boundaries; never remove characters.
  const unwrapped = kind === 'pdf' ? text.replace(/(\p{Script=Han})\r?\n(?=\p{Script=Han})/gu, '$1') : text;
  return unwrapped.replace(/\s+/gu, ' ');
}
function storedModels(value, depth = 0) {
  if (depth > 4 || !value || typeof value !== 'object') return [];
  return [...(Array.isArray(value.authors) ? [value] : []), ...Object.values(value).flatMap(item => storedModels(item, depth + 1))];
}
function parsedModels(text) {
  try {return storedModels(JSON.parse(text));} catch {return [];}
}
function visibleTokens(metadata, mode) {
  const common = [metadata.title, metadata.subtitle, ...(metadata.authors || []).map(item => item.name)];
  return mode === 'paper' ? [...common, ...(metadata.affiliations || []).flatMap(item => [item.name, item.department]),
    ...(metadata.authors || []).map(item => item.orcid), ...(metadata.abstracts || []).map(item => item.text),
    ...(metadata.keywords || []), ...(metadata.funding || []).flatMap(item => [item.funder, item.award])] :
    [...common, ...(metadata.translators || []).map(item => item.name), metadata.publisher, metadata.date, metadata.rights,
      metadata.edition, metadata.series?.title, metadata.series?.number, ...(metadata.identifiers || []).map(item => item.value)];
}
function inspectArtifact(h, file, {render, baseline, glyphs = false} = {}) {
  const args = [path.join(h.root, 'tools/test/inspect-publication-batch3.py'), file];
  if (render) args.push('--render', render);
  if (glyphs) args.push('--glyphs');
  if (baseline) args.push('--baseline', baseline);
  const result = JSON.parse(h.command('python', args));
  fs.writeFileSync(path.join(h.data, path.basename(file) + '.inspection.json'), JSON.stringify(result, null, 2) + '\n');
  assert.equal(Boolean(result.leakedCitationSlot), false, 'An internal CSL nonce/slot escaped into a real file');
  return result;
}
function checkMetadataArtifact(result, metadata, mode = 'none') {
  const authors = (metadata.authors || []).map(item => item.name);
  if (result.kind === 'docx') {
    assert.equal(result.core.title, metadata.title);
    assert.equal(result.core.creator ?? '', authors.join('; '), 'DOCX creator split comma-containing names, reordered people or resurrected a cleared list');
    assert.equal(result.core.language, metadata.language);
    if (metadata.identifiers?.length) assert.equal(result.core.identifier, (metadata.identifiers.find(item => item.primary) || metadata.identifiers[0]).value);
    if (metadata.abstracts?.length) assert(result.core.description?.includes(metadata.abstracts[0].text), 'DOCX core description dropped the abstract');
    hasAll(result.core.keywords || '', metadata.keywords || [], 'DOCX core keywords');
    assert.equal(result.customRelationship.length, 1, 'Missing native custom-properties relationship');
    assert.equal(result.customContentType.length, 1, 'Missing native custom-properties content type');
    const properties = Object.entries(result.custom).filter(([name, value]) => /readmd/i.test(name) || value && typeof value === 'object' && (value.authors || value.metadata));
    assert(properties.length, 'Complete structured publication metadata is absent from DOCX custom properties');
    hasAll(JSON.stringify(properties), meaningfulStrings(metadata), 'DOCX custom properties');
    const model = storedModels(Object.fromEntries(properties))[0];
    assert(model, 'DOCX custom metadata does not expose inspectable structured author/affiliation relationships');
    assertMetadata(model, metadata);
    assert.equal(result.nestedNoteReferences.length, 0, 'A citation created nested Word note references');
  } else if (result.kind === 'pdf') {
    assert.equal(result.info['/Title'], metadata.title);
    assert.equal(result.info['/Author'], authors.join('; '));
    assert.deepEqual(result.dcCreators, authors, 'PDF XMP dc:creator is not an ordered person array');
    assert.equal(result.dcCreatorContainer, 'Seq', 'PDF authors require an ordered RDF Seq');
    assert.deepEqual(result.dcSubjects, metadata.keywords || [], 'PDF XMP keywords are not individual subjects');
    assert(Object.values(result.dcTitle).includes(metadata.title), 'PDF XMP title language alternative is missing');
    assert.equal(result.language, metadata.language);
    assert.equal(result.dcIdentifierIsText, true, 'dc:identifier was incorrectly serialized as an RDF collection');
    for (const item of metadata.identifiers || []) assert([...result.dcIdentifierText, ...result.xmpIdentifiers].includes(item.value), 'PDF standard XMP identifier was dropped');
    for (const item of metadata.abstracts || []) assert(Object.values(result.dcDescription).includes(item.text), 'PDF description lost a bilingual language alternative');
    if (metadata.rights) assert(Object.values(result.dcRights).includes(metadata.rights), 'PDF rights language alternative was dropped');
    hasAll(result.extensionText + '\n' + result.xmp, meaningfulStrings(metadata), 'PDF XMP and declared extension');
    for (const model of result.extensionText.split('\n').flatMap(parsedModels)) assertMetadata(model, metadata);
  } else {
    assert.deepEqual(result.creators.map(item => item.text), authors, 'EPUB must contain independent ordered dc:creator elements');
    assert.deepEqual(result.contributors.map(item => item.text), (metadata.translators || []).map(item => item.name));
    assert.deepEqual(result.metadata.filter(item => item.tag === 'language').map(item => item.text), [metadata.language]);
    assert.deepEqual(result.metadata.filter(item => item.tag === 'subject').map(item => item.text), metadata.keywords || [], 'EPUB subjects split comma-containing keywords or changed order');
    for (const field of ['publisher', 'date', 'rights']) if (metadata[field]) assert(result.metadata.some(item => item.tag === field && item.text === metadata[field]), 'EPUB standard field was dropped: ' + field);
    const refine = (id, property) => result.metadata.filter(item => item.attributes.refines === '#' + id && item.attributes.property === property).map(item => item.text);
    for (const [index, person] of (metadata.translators || []).entries()) assert(refine(result.contributors[index].attributes.id, 'role').includes('trl'), 'EPUB translator MARC role is missing');
    const titles = result.metadata.filter(item => item.tag === 'title');
    const main = titles.find(item => item.text === metadata.title); assert(main, 'EPUB main title is missing');
    if (metadata.subtitle) {
      const subtitle = titles.find(item => item.text === metadata.subtitle); assert(subtitle, 'EPUB subtitle is missing');
      assert(refine(subtitle.attributes.id, 'title-type').includes('subtitle'), 'EPUB subtitle refinement is missing');
    }
    if (metadata.series) {
      const collection = result.metadata.find(item => item.attributes.property === 'belongs-to-collection' && item.text === metadata.series.title);
      assert(collection, 'EPUB series title is missing');
      assert(refine(collection.attributes.id, 'collection-type').includes('series'), 'EPUB collection must retain its series role');
      assert(refine(collection.attributes.id, 'group-position').includes(metadata.series.number), 'EPUB series number was dropped or numerically rewritten');
    }
    for (const item of metadata.identifiers || []) assert(result.identifiers.some(actual => actual.text === item.value), 'EPUB identifier changed: ' + item.value);
    if (metadata.identifiers?.length) {
      const primary = metadata.identifiers.find(item => item.primary) || metadata.identifiers[0];
      const actualPrimary = result.identifiers.find(item => item.attributes.id === result.packageAttributes['unique-identifier']);
      assert.equal(actualPrimary?.text, primary.value, 'EPUB package primary identifier changed');
    }
    for (const abstract of metadata.abstracts || []) assert(result.metadata.some(item => item.tag === 'description' && item.text === abstract.text), 'EPUB abstract is incomplete');
    hasAll(JSON.stringify(result.metadata), meaningfulStrings(metadata), 'EPUB metadata/declared extensions');
    for (const model of result.metadata.flatMap(item => parsedModels(item.text))) assertMetadata(model, metadata);
  }
  if (mode !== 'none') {
    const text = visibleText(result.text, result.kind);
    hasAll(text, visibleTokens(metadata, mode).filter(Boolean).map(value => value.replace(/\s+/gu, ' ')), 'Actual ' + result.kind + ' front matter');
  }
}
function checkRichArtifact(result) {
  hasAll(result.text, ['UNDERLINE', 'SUP', 'SUB', 'Synthetic Evidence'], 'Actual rich ' + result.kind);
  if (result.kind === 'docx') {
    const enabled = property => property && !['0', 'false', 'off', 'none'].includes(property.val);
    assert(result.runs.some(run => enabled(run.properties.smallCaps)), 'Word small-caps must be an enabled native run property');
    assert(result.runs.some(run => enabled(run.properties.i)), 'Word italics must be native');
    assert(result.runs.some(run => run.text.includes('UNDERLINE') && enabled(run.properties.u)), 'Word underline must be native on the expected run');
    assert(result.runs.some(run => run.text.includes('SUP') && run.properties.vertAlign?.val === 'superscript'), 'Word superscript must be native');
    assert(result.runs.some(run => run.text.includes('SUB') && run.properties.vertAlign?.val === 'subscript'), 'Word subscript must be native');
    assert(result.links.some(link => link.Target === 'https://example.invalid/study'), 'Word CSL link relationship is missing');
    assert.equal(result.nestedNoteReferences.length, 0);
  } else if (result.kind === 'epub') {
    const xml = result.content.map(item => item.xml).join('\n') + result.css;
    for (const expression of [/font-variant\s*:\s*small-caps/, /font-style\s*:\s*italic|<i\b|<em\b/, /text-decoration\s*:\s*underline|<u\b/, /vertical-align\s*:\s*super|<sup\b/, /vertical-align\s*:\s*sub|<sub\b/, /href="https:\/\/example\.invalid\/study"/]) assert(expression.test(xml), 'EPUB lost a typed citation formatting axis: ' + expression);
  } else {
    const spans = result.pages.flatMap(page => page.spans);
    const superscript = spans.filter(span => span.text.includes('SUP'));
    const subscript = spans.filter(span => span.text.includes('SUB'));
    assert(superscript.length && subscript.length, 'PDF has no actual superscript/subscript glyph runs');
    assert(superscript.some(a => subscript.some(b => Math.abs(a.bbox[1] - b.bbox[1]) > 1)), 'PDF sup/sub runs share a baseline');
    assert(spans.some(span => /Italic|Oblique/i.test(span.font)), 'PDF citation italics did not reach the physical font');
    const target = [...'UNDERLINE'].map(character => character.codePointAt(0));
    assert(result.pages.some(page => (page.glyphTraces || []).some(trace => trace.chars.some((character, index) => {
      const letters = trace.chars.slice(index, index + target.length);
      if (letters.length !== target.length || letters.some((letter, offset) => letter.unicode !== target[offset])) return false;
      // MuPDF's display spans can merge adjacent unformatted spaces into the
      // same run. Use the actual target glyphs, retaining the original line
      // coverage and vertical tolerances without requiring underlined spaces.
      const baseline = letters[0].origin[1];
      if (letters.some(letter => Math.abs(letter.origin[1] - baseline) > 0.25)) return false;
      const left = Math.min(...letters.map(letter => letter.bbox[0]));
      const right = Math.max(...letters.map(letter => letter.bbox[2]));
      const bottom = Math.max(...letters.map(letter => letter.bbox[3]));
      return page.drawings.some(line => line.width > 0 && line.bbox[2] > line.bbox[0] &&
        line.bbox[3] >= line.bbox[1] && line.bbox[3] - line.bbox[1] < 2.5 &&
        line.bbox[1] >= baseline && line.bbox[1] <= bottom + 3 &&
        line.bbox[0] <= left + 2 && line.bbox[2] >= right - 2);
    }))), 'PDF underline has no physical line below the expected glyphs; rich PDF inspection requires --glyphs');
  }
}
function checkSourceSpans(source, result, expected) {
  assert.match(result.revision || result.projection?.revision || '', /^[a-f0-9]{64}$/i, 'Source context lacks the document revision');
  assert(Array.isArray(result.source_citations) && result.source_citations.length, 'No citation source context');
  const raw = Buffer.from(source);
  function span(value) {
    assert(Number.isInteger(value.from) && Number.isInteger(value.to));
    assert(value.from >= 0 && value.to > value.from && value.to <= source.length, 'Invalid UTF-16 source bounds');
    assert(Number.isInteger(value.byte_from) && Number.isInteger(value.byte_to));
    assert.equal(value.byte_from, Buffer.byteLength(source.slice(0, value.from)), 'UTF-8 source start differs from UTF-16 location');
    assert.equal(value.byte_to, Buffer.byteLength(source.slice(0, value.to)), 'UTF-8 source end differs from UTF-16 location');
    assert.equal(source.slice(value.from, value.to), value.source, 'UTF-16 source slice is not exact');
    assert.equal(raw.subarray(value.byte_from, value.byte_to).toString('utf8'), value.source, 'UTF-8 source slice is not exact');
  }
  for (const [index, citation] of result.source_citations.entries()) {
    const sourceSpan = citation.span || result.projection?.citations[index]?.source?.span || citation;
    span(sourceSpan);
    if (!citation.span) {
      assert.equal(citation.from, sourceSpan.from); assert.equal(citation.to, sourceSpan.to);
      assert.equal(citation.source, sourceSpan.source, 'Legacy source context diverged from the typed span');
    }
    assert(Array.isArray(citation.items) && citation.items.length, 'Missing per-item citation context');
    for (const item of citation.items) { span(item.span); for (const field of Object.values(item.fields || {})) span(field); }
  }
  const ranges = result.source_citations.map((citation, index) => {
    const value = citation.span || result.projection?.citations[index]?.source?.span || citation;
    return {from:value.from, to:value.to, source:value.source};
  });
  assert.equal(new Set(ranges.map(value => `${value.from}:${value.to}`)).size, ranges.length, 'Duplicate citation source range');
  if (expected) assert.deepEqual(ranges, expected, 'Source citations differ from independently enumerated fixture occurrences');
}
// The original context fixture deliberately repeats @study and contains
// protected @not-* strings. These requirements must not be applied to the
// independently authored upstream display fixtures, which have other keys.
function checkSpans(source, result) {
  checkSourceSpans(source, result);
  const repeated = result.source_citations.filter(citation => (citation.span?.source || citation.source) === '[@study]');
  assert(repeated.length >= 3, 'Repeated same-text citations were deduplicated');
  assert.equal(new Set(repeated.map(citation => (citation.span || citation).from)).size, repeated.length);
  assert(!result.source_citations.some(citation => JSON.stringify(citation).includes('not-')), 'Protected code/math/link/comment content became a citation');
}
module.exports = {assertMetadata, meaningfulStrings, hasAll, visibleText, visibleTokens, inspectArtifact, checkMetadataArtifact, checkRichArtifact, checkSourceSpans, checkSpans};
