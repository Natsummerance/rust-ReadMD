'use strict';
// QA-control tests only: no product, browser, runtime or generated candidate.
const assert=require('node:assert/strict');
const c=require('./publication-structure-checks.cjs');
const fixtures=require('./publication-structure-fixtures.cjs');
function run(){let count=0;const positive=fn=>{fn();count++;},negative=fn=>{assert.throws(fn);count++;};
  positive(()=>c.assertExactSegments('alpha\nbeta gamma',['alpha beta','gamma'],'physical wrap'));
  positive(()=>c.assertExactSegments('reproduc-\nible proof',['reproducible proof'],'discretionary wrap'));
  negative(()=>c.assertExactSegments('alpha gamma',['alpha beta','gamma'],'missing word'));
  negative(()=>c.assertExactSegments('alphabeta gamma',['alpha beta','gamma'],'joined words'));
  negative(()=>c.assertExactSegments('alpha beta EXTRA gamma',['alpha beta','gamma'],'extra text'));
  negative(()=>c.assertExactSegments('reproduc\nible proof',['reproducible proof'],'unapproved character split'));
  positive(()=>c.assertExactSegments('LONGUNSPACED\nLABEL9',[{text:'LONGUNSPACEDLABEL9',characterWrap:true}],'explicit label wrap'));
  negative(()=>c.assertExactSegments('LONGUNSPACED\nLABLE9',[{text:'LONGUNSPACEDLABEL9',characterWrap:true}],'missing label glyph'));
  const html='<div class="csl-bib-body">\n<div class="csl-entry"><div class="csl-left-margin">[9]</div><div class="csl-right-inline"><i>Original body</i></div><div class="csl-indent">Tail</div></div>\n</div>';
  const expected=c.parseExpectedHTML(html);positive(()=>{assert.equal(expected.length,1);assert.deepEqual(expected[0].segments,['[9]','Original body','Tail']);});
  negative(()=>c.parseExpectedHTML('<div class="csl-bib-body"><script>bad</script></div>'));
  negative(()=>c.parseExpectedHTML('<div class="csl-entry">broken'));
  negative(()=>c.parseExpectedHTML('<div class="csl-entry" onclick="bad()">bad</div>'));
  const rich=(text,format={})=>({type:'text',text,format});
  const item={key:'a',first_field_rich:[],rich:[{type:'group',display:'left-margin',children:[rich('[9]')]},{type:'group',display:'right-inline',children:[rich('Original body',{font_style:'italic'})]},{type:'group',display:'indent',children:[rich('Tail')]}],layout:[{type:'columns',left:[{type:'inline',children:[rich('[9]')]}],right:[{type:'inline',children:[rich('Original body',{font_style:'italic'})]}]},{type:'block',indent:true,children:[{type:'inline',children:[rich('Tail')]}]}]};
  const fixture={id:'control',expected,minimumColumns:1};const api={bibliography:{items:[item]},projection:{bibliography:{items:[{key:'a',first_field:item.first_field_rich,rich:item.rich}]}}};
  positive(()=>c.checkBibliography(api,fixture));
  negative(()=>{const value=structuredClone(api);value.projection.bibliography.items[0]={key:'a',first_field:[rich('[9]')],rich:[rich('Original body',{font_style:'italic'}),{type:'group',display:'indent',children:[rich('Tail')]}]};c.checkBibliography(value,fixture);}); // A split owns the whole right cell; it cannot invent a full-width Tail.

  for(const mutate of [x=>x.bibliography.items[0].rich[1].children[0].text='Original bod',x=>x.bibliography.items[0].rich[1].children[0].format={},x=>x.bibliography.items[0].layout=[],x=>x.projection.bibliography.items[0].key='other'])negative(()=>{const value=structuredClone(api);mutate(value);c.checkBibliography(value,fixture);});
  const readerFixture={id:'control',line:3,entry:2,expected:[{segments:['A','BODY']},{segments:['B','BODY']}],minimumColumns:1};
  const reader={overflow:false,viewportOverflow:false,sections:[{ready:'ready',preferred:'100px',complex:true,entries:[{text:'ABODY',margin:96,font:16},{text:'BBODY',margin:0,font:16}],columns:[{box:{width:500},left:{width:100,right:100},right:{x:112},gap:12,font:16,leftOverflow:false,rightOverflow:false,intrinsic:100,nested:false}],indents:[{padding:64,font:16}]}]};
  positive(()=>c.checkReader(reader,readerFixture));
  positive(()=>{const value=structuredClone(reader);value.sections[0].columns=[];value.sections[0].preferred='0px';c.checkReader(value,{...readerFixture,minimumColumns:0});});
  for(const mutate of [x=>x.sections[0].ready='pending',x=>x.sections[0].preferred='auto',x=>x.sections[0].preferred='0px',x=>x.sections[0].preferred='80px',x=>x.sections[0].columns[0].left.width=101.01,x=>x.sections[0].columns[0].right.x=97,x=>x.sections[0].columns[0].gap=24,x=>x.sections[0].entries[0].margin=32,x=>x.sections[0].entries[1].margin=96,x=>x.sections[0].entries[0].text='AMISSING',x=>x.sections[0].indents[0].padding=96])negative(()=>{const value=structuredClone(reader);mutate(value);c.checkReader(value,readerFixture);});
  const glyphs=[...'BODYA'].map((ch,i)=>({unicode:ch.codePointAt(0),glyphId:i+1,origin:[20+i*6,40],bbox:[20+i*6,30,25+i*6,42]}));
  const pdf={pages:[{width:200,height:200,glyphTraces:[{font:'Synthetic control only',size:11,chars:glyphs}]}]};
  positive(()=>{assert.equal(c.marker(pdf,'BODYA').x,20);c.checkPdfBounds(pdf);});
  negative(()=>{const x=structuredClone(pdf);x.pages[0].glyphTraces[0].chars[2].glyphId=0;c.marker(x,'BODYA');});
  negative(()=>{const x=structuredClone(pdf);x.pages.push(structuredClone(x.pages[0]));c.marker(x,'BODYA');});
  negative(()=>{const x=structuredClone(pdf);x.pages[0].glyphTraces[0].chars[0].bbox[0]=-.76;c.checkPdfBounds(x);});
  positive(()=>{assert.equal(fixtures.declarations.length,7);assert.equal(fixtures.sectionExpected.pageCount,6);assert.equal(fixtures.originalCases.length,3);assert.equal(fixtures.metadataCases.length,2);assert(fixtures.sectionSource.startsWith('\uFEFF'));assert(!fixtures.sectionSource.replace(/\r\n/gu,'').includes('\n'));assert(fixtures.sectionSource.includes('](#readmd-section-main)'));});
  // Fixed originals are read and hash-checked; raw values are never printed or
  // copied into this source. This does not invoke the product/CSL processor.
  positive(()=>{const official=require('../../ui-tests/publication-structure-native.cjs').loadOfficial();assert.equal(official.length,5);assert.deepEqual(official.map(f=>f.expected.length),[5,2,1,2,2]);assert(official.slice(0,4).every(f=>f.line===1&&f.entry===1));assert.equal(official[4].entry,0);});
  const runner=require('../../ui-tests/publication-structure-native.cjs'),{fileHash}=require('./publication-batch3-harness.cjs'),path=require('node:path');
  const {checkSpans}=require('./publication-batch3-checks.cjs');
  const spanSource='\uFEFF😀 中文\r\n\r\n[@a]\r\n\r\n[@a]\r\n\r\n[@a; @b]\r\n';
  const sourceSpan=(from,to)=>({from,to,source:spanSource.slice(from,to),byte_from:Buffer.byteLength(spanSource.slice(0,from)),byte_to:Buffer.byteLength(spanSource.slice(0,to))});
  const spanResult={revision:'a'.repeat(64),source_citations:[...spanSource.matchAll(/\[@[^\]]+\]/gu)].map(m=>({span:sourceSpan(m.index,m.index+m[0].length),items:[...m[0].matchAll(/@[a-z]/gu)].map(item=>({span:sourceSpan(m.index+item.index,m.index+item.index+item[0].length),fields:{}}))}))};
  positive(()=>runner.checkDisplaySourceSpans(spanSource,spanResult,3));
  // Fixture-independent checks reject omitted, duplicated, displaced, and
  // incorrectly encoded ranges even when every returned text slice looks valid.
  for(const mutate of [x=>x.source_citations.pop(),x=>x.source_citations[1]=structuredClone(x.source_citations[0]),x=>x.source_citations.reverse(),x=>x.source_citations[0].span.byte_from--,x=>x.source_citations[0].span.to++,x=>x.source_citations[0].items[0].span.byte_to--])negative(()=>{const value=structuredClone(spanResult);mutate(value);runner.checkDisplaySourceSpans(spanSource,value,3);});
  negative(()=>runner.checkDisplaySourceSpans(spanSource,spanResult,4));
  negative(()=>checkSpans(spanSource,spanResult)); // Original @study fixture guard remains strict.
  positive(()=>{const inputs=runner.independentReaderInputs();assert.deepEqual(inputs.map(i=>i.name),['Independent-reader-book.md','references.json','cover.svg']);assert(inputs.every(i=>i.bytes>0&&/^[a-f0-9]{64}$/u.test(i.sha256)));});
  const epub={passed:true,files:[{name:path.basename(__filename),sha256:fileHash(__filename),passed:true,exitCode:0,counts:{fatal:0,error:0,warning:0}}]};
  positive(()=>runner.checkEpubResult(epub,[__filename]));
  for(const mutate of [x=>x.files[0].counts.warning=1,x=>x.files[0].sha256='0'.repeat(64),x=>x.files[0].counts={nFatal:0,nError:0,nWarning:0},x=>x.files[0].exitCode=1])negative(()=>{const value=structuredClone(epub);mutate(value);runner.checkEpubResult(value,[__filename]);});
  console.log(JSON.stringify({passed:true,selfTestOnly:true,productInvocations:0,controls:count,officialBytesHashChecked:5,syntaxOnlyIsNotProductAcceptance:true}));return{passed:true,controls:count};
}
module.exports={run};if(require.main===module)run();
