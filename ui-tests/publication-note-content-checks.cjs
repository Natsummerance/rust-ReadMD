'use strict';
const assert=require('node:assert/strict');
const source='# Footnote study\n\nBody cites the first note[^a], the second[^b], and repeats the first[^a].\n\n[^a]: First note paragraph.\n\n    > Quoted evidence **strong**.\n\n    3. Ordered evidence.\n    4. Second item.\n\n    | Measure | Value |\n    |---|---:|\n    | Accuracy | 98 |\n\n    ```rust\n    let evidence = 98;\n    ```\n\n    $x^2$\n\n[^b]: Repeated evidence.\n\n    Repeated evidence.\n';
async function applyStyle(page,text,{last=false,size,align='right'}={}){
 await page.evaluate(({text,last})=>{const s=cmView.state.doc.toString(),at=last?s.lastIndexOf(text):s.indexOf(text);if(at<0)throw Error('Missing source target');cmView.dispatch({selection:{anchor:at}});cmView.focus();},{text,last});
 await page.locator('[data-menu="md-text-menu"]').click();await page.locator('#md-text-menu [data-md="publication-style"]').click();await page.locator('#publication-style-modal').waitFor({state:'visible'});
 if(size!==undefined)await page.locator('[data-local="text.size"]').fill(String(size));await page.locator('[data-local="paragraph.align"]').selectOption(align);
 await page.locator('#publication-style-modal button[type=submit]').click();await page.locator('#publication-style-modal').waitFor({state:'detached'});
}
async function inspectWord(page,parts){
 const body=parts.get('word/document.xml'),notes=parts.get('word/footnotes.xml');assert.ok(body&&notes);
 const xml=await page.evaluate(({body,notes})=>{
  const parser=new DOMParser(),b=parser.parseFromString(body,'application/xml'),n=parser.parseFromString(notes,'application/xml'),ns='http://schemas.openxmlformats.org/wordprocessingml/2006/main';
  const elements=(root,name)=>[...root.getElementsByTagNameNS(ns,name)];
  return {valid:[b,n].every(d=>!d.querySelector('parsererror')),body:elements(b,'t').map(e=>e.textContent).join(''),nested:elements(n,'footnoteReference').length,notes:elements(n,'footnote').filter(e=>/^[1-9]\d*$/.test(e.getAttributeNS(ns,'id'))).map(e=>({id:e.getAttributeNS(ns,'id'),text:elements(e,'t').map(t=>t.textContent).join(''),markers:elements(e,'footnoteRef').length,tables:elements(e,'tbl').length,numbered:elements(e,'numPr').length,xml:new XMLSerializer().serializeToString(e)}))};
 },{body,notes});
 assert.equal(xml.valid,true);assert.equal(xml.nested,0);assert.equal(xml.notes.length,2);assert.ok(xml.notes.every(n=>n.markers===1));
 const first=xml.notes.find(n=>n.id==='1');for(const text of ['First note paragraph.','Quoted evidence strong.','Ordered evidence.','Second item.','Measure','Accuracy','evidence']){assert.ok(first.text.includes(text),'Lost note content: '+text);assert.ok(!xml.body.includes(text),'Note leaked into main story: '+text);}
 assert.equal(first.tables,1);assert.equal(first.numbered,2);assert.ok(first.xml.includes('oMath'));assert.ok(parts.get('word/numbering.xml').includes('w:startOverride w:val="3"'));return xml;
}
function markerOnFirstLine(objects){
 const text=objects.find(o=>o.kind==='text'&&o.text.includes('First note paragraph.'));assert.ok(text,'First note must be present');
 const markers=objects.filter(o=>o.kind==='text'&&o.text.trim()==='1'&&o.bounds.x<text.bounds.x);
 assert.ok(markers.some(({bounds:b})=>Math.min(b.y+b.height,text.bounds.y+text.bounds.height)>Math.max(b.y,text.bounds.y)),'Footnote marker is orphaned above its first paragraph');
}
module.exports={source,applyStyle,inspectWord,markerOnFirstLine};
