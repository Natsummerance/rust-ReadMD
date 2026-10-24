'use strict';

// Literal expectations shared by Node and the real browser. No production
// parser or slug helper is used to construct these identities.
const source = [
  '---', 'title: TOC fixture', '# Metadata is not a heading', '---',
  '# A', '', '# A-1', '', '# A', '',
  '# API {#API_V1}', '', '# Again {#API_V1}', '',
  '# 中文 α &amp; β', '',
  'Setext &eacute;', '===============', '',
  '> ## Quoted &#x5B;x&#x5D;', '',
  '- ### Nested', '',
  '```md', '# Ignored', '```', '',
  '    # Indented code', '',
  '# !!!', '', '# !!!', '',
  '# Path \\ [label] \\*literal\\*', '',
  '# Code `a[b]\\c * d &amp; <i>`', '',
  '# Entities &copy; &eacute; &#128; &#x1F642;', '',
  '# HTML <em>visible &amp; nested</em>', '',
  '# Bare &copy stays literal', '',
  '# Image ![alt &amp; **bold**](/image.png)', '',
  '# [Reference][guide] {#ref}', '', '[guide]: /docs', '',
  '# Literal `\\[x\\] \\(y\\) $$z$$`', '',
  '# Genuine $x^2$', '',
  '# Target {#Case%/中文[one](x)}', '',
].join('\n');

const headings = [
  { id: 'a', text: 'A', level: 1 },
  { id: 'a-1', text: 'A-1', level: 1 },
  { id: 'a-2', text: 'A', level: 1 },
  { id: 'API_V1', text: 'API', level: 1 },
  { id: 'API_V1-1', text: 'Again', level: 1 },
  { id: '中文-α--β', text: '中文 α & β', level: 1 },
  { id: 'setext-é', text: 'Setext é', level: 1 },
  { id: 'quoted-x', text: 'Quoted [x]', level: 2 },
  { id: 'nested', text: 'Nested', level: 3 },
  { id: 'section', text: '!!!', level: 1 },
  { id: 'section-1', text: '!!!', level: 1 },
  { id: 'path--label-literal', text: 'Path \\ [label] *literal*', level: 1 },
  { id: 'code-abc--d-amp-i', text: 'Code a[b]\\c * d &amp; <i>', level: 1 },
  { id: 'entities--é--', text: 'Entities © é € 🙂', level: 1 },
  { id: 'html-visible--nested', text: 'HTML visible & nested', level: 1 },
  { id: 'bare-copy-stays-literal', text: 'Bare &copy stays literal', level: 1 },
  { id: 'image-alt--bold', text: 'Image alt & bold', level: 1 },
  { id: 'ref', text: 'Reference', level: 1 },
  { id: 'literal-x-y-z', text: 'Literal \\[x\\] \\(y\\) $$z$$', level: 1 },
  { id: 'genuine-x2', text: 'Genuine $x^2$', level: 1 },
  { id: 'Case%/中文[one](x)', text: 'Target', level: 1 },
];

module.exports = { source, headings };
