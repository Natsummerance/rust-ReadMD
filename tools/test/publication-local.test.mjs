import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';
const sandbox={window:{},console};vm.createContext(sandbox);
vm.runInContext(fs.readFileSync(new URL('../../assets/vendor/marked.min.js',import.meta.url),'utf8'),sandbox);
vm.runInContext(fs.readFileSync(new URL('../../assets/js/reader/publication.js',import.meta.url),'utf8'),sandbox);
const ranges=source=>JSON.parse(JSON.stringify(sandbox.window.ReadMDPublication.metadataRanges(source)));
const marker='<!-- readmd:layout v1 {"id":"block-0123456789abcdef01234567","text":{"size":16}} -->';
test('only actual top-level layout metadata is folded; examples remain editable',()=>{
  const source='```html\n'+marker+'\n```\n\n> '+marker+'\n\n'+marker+'\n\nParagraph\n';
  const found=ranges(source);assert.equal(found.length,1);assert.equal(source.slice(found[0].from,found[0].to),marker);assert.equal(found[0].from,source.lastIndexOf(marker));
});
test('CRLF, BOM and astral text keep exact editor offsets',()=>{
  const source='\uFEFF# 𝛂 heading\r\n\r\nParagraph 🙂\r\n\r\n'+marker+'\r\n\r\nOther\r\n';
  const found=ranges(source);assert.equal(found.length,1);assert.equal(source.slice(found[0].from,found[0].to),marker);
});
test('unknown versions and malformed metadata stay visible for correction',()=>{
  for(const source of [marker.replace('v1','v9'),marker.replace('16','oops'),marker.replace('block-0123456789abcdef01234567','not-an-editor-id')])assert.equal(ranges(source+'\n\nText\n').length,0);
});
