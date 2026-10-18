'use strict';
// Original, synthetic QA material. No upstream CSL fixture is redistributed.
const paperMetadata = {
  title: 'Evidence, context and reproducibility',
  subtitle: 'A synthetic bilingual paper',
  authors: [
    {id: 'linh', name: 'Nguyen, Linh', given: 'Linh', family: 'Nguyen', fileAs: 'Nguyen, Linh', orcid: '0000-0000-0000-0001', corresponding: true, affiliationIds: ['north', 'east']},
    {id: 'mo', name: '林墨', given: '墨', family: '林', orcid: '0000-0000-0000-001X', corresponding: false, affiliationIds: ['east']},
  ],
  affiliations: [
    {id: 'north', name: 'North Institute, Synthetic Research', department: 'Evidence Laboratory', address: 'Example City'},
    {id: 'east', name: '东岸示例研究院', department: '可复核研究室', address: '示例市'},
  ],
  abstracts: [
    {language: 'en-US', text: 'We preserve names, commas, Unicode and exact citation context. This synthetic abstract contains a colon: evidence first.'},
    {language: 'zh-CN', text: '本文是虚构测试材料。作者、机构、标识、基金与双语摘要必须完整保存，不能因预览或导出而改写源文。'},
  ],
  keywords: ['evidence, context', '可复核性', 'Unicode 😀'],
  funding: [{funder: 'Synthetic Research Fund', award: 'QA-000042', recipientIds: ['linh', 'mo']}],
  identifiers: [{scheme: 'doi', value: '10.5555/readmd.synthetic.paper', primary: true}],
  language: 'en-US', date: '2026-10-13', rights: 'Synthetic QA material; no real research participants.',
};
const bookMetadata = {
  title: 'The Synthetic Book of Evidence', subtitle: 'A translated edition',
  authors: [{id: 'writer', name: 'Example, Rowan', given: 'Rowan', family: 'Example', affiliationIds: []}],
  translators: [{id: 'translator', name: '译者，明', given: '明', family: '译者', fileAs: '译者，明', affiliationIds: []}],
  publisher: 'Example Publishing, Synthetic Editions', date: '2026-10',
  rights: 'Copyright 2026 Synthetic QA Authors. All material is fictional.',
  edition: 'Second, revised edition', series: {title: 'Evidence Studies', number: '0042'},
  identifiers: [
    {scheme: 'isbn', value: '978-0-00-000000-2', primary: true},
    {scheme: 'doi', value: '10.5555/readmd.synthetic.book', primary: false},
  ],
  keywords: ['translation, evidence', '书籍前置内容'], language: 'en-US',
};

// This serializer belongs only to the test input. Expectations are the objects
// above, written by QA, never parsed back out of the product's normalized model.
function yaml(value, depth = 0) {
  const pad = '  '.repeat(depth);
  if (Array.isArray(value)) {
    if (!value.length) return '[]';
    return value.map(item => {
      if (item && typeof item === 'object') {
        const lines = yaml(item, depth + 1).split('\n');
        return pad + '- ' + lines[0].trimStart() + (lines.length > 1 ? '\n' + lines.slice(1).join('\n') : '');
      }
      return pad + '- ' + JSON.stringify(item);
    }).join('\n');
  }
  if (value && typeof value === 'object') return Object.entries(value).map(([key, item]) => {
    const nested = item && typeof item === 'object' && Object.keys(item).length;
    return pad + key + ':' + (nested ? '\n' + yaml(item, depth + 1) : ' ' + yaml(item, depth + 1));
  }).join('\n');
  return JSON.stringify(value);
}
const protectedTrivia = '# Keep this exact comment, punctuation: [@not-a-citation].\nunknown-extension: {note: "a,b: unchanged", enabled: true}\n';
const paperBody = '# Evidence\n\nA synthetic result retains its author source. Compare [@study, p. 42].\n\n$$E=mc^2$$\n\nBody sentinel 😀 中文 remains unchanged.\n';
const bookBody = '# Chapter One\n\nThe translated book keeps its contributor roles and series number.\n\nBody sentinel 😀 中文 remains unchanged.\n';
function sourceFor(metadata, body, {bom = false, crlf = false, bibliography = true} = {}) {
  let value = '---\n' + protectedTrivia + yaml(metadata) + '\n' + (bibliography ? 'bibliography: references.json\ncsl: apa\n' : '') + '---\n\n' + body;
  if (metadata.authors?.some(person => person.id === 'mo')) value = value.replace('  - id: "mo"', '  # This comment travels with the second author.\n  - id: "mo"');
  if (crlf) value = value.replaceAll('\n', '\r\n');
  return (bom ? '\uFEFF' : '') + value;
}
const references = [
  {id: 'study', type: 'book', title: 'Synthetic Evidence', author: [{family: 'Example', given: 'Ada'}], issued: {'date-parts': [[2020]]}, publisher: 'Example Press', URL: 'https://example.invalid/study'},
  {id: 'study2', type: 'book', title: 'Second Evidence', author: [{family: 'Example', given: 'Ada'}], issued: {'date-parts': [[2020]]}, publisher: 'Example Press'},
  {id: 'study3', type: 'book', title: 'Third Evidence', author: [{family: 'Other', given: 'Bea'}], issued: {'date-parts': [[2021]]}, publisher: 'Example Press'},
];
const styles = ['apa', 'ieee', 'china-national-standard-gb-t-7714-2015-numeric', 'china-national-standard-gb-t-7714-2015-author-date', 'chicago-notes-bibliography'];
const richStyle = '<style xmlns="http://purl.org/net/xbiblio/csl" version="1.0" class="in-text"><info><title>ReadMD original rich QA</title><id>urn:readmd:qa:rich</id><updated>2026-10-13T00:00:00Z</updated></info><citation><layout prefix="(" suffix=")" delimiter="; "><group delimiter=" "><names variable="author" font-variant="small-caps"><name form="short"/></names><text variable="title" font-style="italic"/><text value="UNDERLINE" text-decoration="underline"/><text value="SUP" vertical-align="sup"/><text value="SUB" vertical-align="sub"/><text variable="URL"/></group></layout></citation><bibliography hanging-indent="true"><layout><group delimiter=" "><names variable="author" font-variant="small-caps"><name/></names><text variable="title" font-style="italic"/><text value="UNDERLINE" text-decoration="underline"/><text value="SUP" vertical-align="sup"/><text value="SUB" vertical-align="sub"/><text variable="URL"/></group></layout></bibliography></style>';
const noteStyle = '<style xmlns="http://purl.org/net/xbiblio/csl" version="1.0" class="note"><info><title>ReadMD original note QA</title><id>urn:readmd:qa:notes</id><updated>2026-10-13T00:00:00Z</updated></info><citation near-note-distance="5"><layout delimiter="; "><names variable="author"><name form="short"/></names><text value="|"/><choose><if position="ibid-with-locator"><text value="IBID-LOC"/></if><else-if position="ibid"><text value="IBID"/></else-if><else-if position="near-note"><text value="NEAR"/></else-if><else-if position="subsequent"><text value="DISTANT"/></else-if><else><text value="FIRST"/></else></choose><text variable="title" prefix=":"/><text variable="locator" prefix=":"/></layout></citation></style>';
const contextBody = '😀 中文 source mapping.\n\n[see @study, pp. 33-35 and *passim*; cf. @study2, chap. 1]\n\n[-@study, p. 42]\n\n@study [p. 43] explains the claim.\n\n:cite[author-only @study]\n\nRepeated [@study] here.\n\nRepeated [@study] here.\n\nRepeated [@study] here.\n\n[@study{}, 99 years later]\n\n> Quoted [@study3].\n\n| Source | Claim |\n|---|---|\n| [@study] | original |\n\nA manual note[^owned].\n\n[^owned]: Inside the author note [@study, p. 7].\n\nProtected: `[@not-code]`, $[@not-math]$, person@example.invalid, \\@escaped.\n\n[link](https://example.invalid/@not-link) ![image](https://example.invalid/@not-image)\n\n<!-- [@not-comment] -->\n\n```text\n[@not-fenced]\n```\n';
module.exports = {paperMetadata, bookMetadata, sourceFor, paperBody, bookBody, references, styles, richStyle, noteStyle, contextBody, protectedTrivia};
