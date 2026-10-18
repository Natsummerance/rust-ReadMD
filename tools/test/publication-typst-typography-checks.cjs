'use strict';
// Independent checks of actual PDF/parser coordinates and service contracts.
const assert=require('node:assert/strict'),crypto=require('node:crypto');
const TOLERANCE_PT=0.004;
const near=(a,b,label)=>assert(Number.isFinite(a)&&Math.abs(a-b)<=TOLERANCE_PT,`${label}: ${a} != ${b}`);
const count=(text,token)=>(text.match(new RegExp('(?<![A-Z0-9_])'+token+'(?![A-Z0-9_])','gu'))||[]).length;
function checkPdf(value,{markers=['FIRST','SECOND','THIRD'],gap,minimum=false,fontSize=11,long=false,formula}={}){
  assert.equal(value.kind,'pdf');assert(value.pages.length);assert.equal(value.leakedCitationSlot,false);
  for(const text of [value.text,value.pypdfText]){
    assert.equal(typeof text,'string');for(const marker of markers)assert.equal(count(text,marker),1,'Missing/duplicated original token '+marker);
    assert(!/readmd:layout|readmd:page-source|readmd-internal:publication/u.test(text),'Private/source directives became copied text');
  }
  const found=[];
  for(const [i,page]of value.pages.entries())for(const span of page.spans){
    const marker=span.text.trim();if(!markers.includes(marker))continue;
    const [x,y]=span.origin;assert(x>=0&&x<page.width&&y>0&&y<page.height,'Actual marker leaves physical page');
    near(span.size,fontSize,'Actual paragraph font size');found.push({marker,page:i+1,x,baseline:y,fontSize:span.size,bbox:span.bbox});
  }
  assert.deepEqual(found.map(m=>m.marker),markers,'Actual baseline inventory lost, duplicated or reordered a line');
  const gaps=[];for(let i=1;i<found.length;i++)if(found[i].page===found[i-1].page){const actual=found[i].baseline-found[i-1].baseline;
    if(minimum)assert(actual>=gap-TOLERANCE_PT,'Minimum/Multiple formula line did not retain its requested minimum');else near(actual,gap,'Actual baseline interval');
    gaps.push(actual);
  }
  assert(gaps.length,'No actual same-page baseline interval was measured');
  if(minimum)assert(gaps.some(n=>n>gap+1),'Tall original formula did not enlarge its native line');
  if(long)assert(value.pages.length>1&&gaps.length>=markers.length-value.pages.length,'Single original paragraph did not genuinely paginate');
  let vector;
  if(formula){const drawings=value.pages.flatMap((p,i)=>p.drawings.map(d=>({...d,page:i+1})));const expectedWidth=(formula.inkBounds[2]-formula.inkBounds[0])*formula.size/formula.unitsPerEm,
      expectedHeight=(formula.inkBounds[3]-formula.inkBounds[1])*formula.size/formula.unitsPerEm;
    const matches=drawings.filter(d=>Array.isArray(d.bbox)&&Math.abs(d.bbox[2]-d.bbox[0]-expectedWidth)<=TOLERANCE_PT&&Math.abs(d.bbox[3]-d.bbox[1]-expectedHeight)<=TOLERANCE_PT);
    assert.equal(matches.length,1,'Original fixed-font mathematical glyph has been lost, duplicated or scaled');const actual=matches[0];
    assert.equal(found.length,3);assert(found.every(line=>line.page===actual.page),'Short mathematical proof unexpectedly split pages');
    assert(actual.bbox[1]>=found[0].bbox[3]-TOLERANCE_PT&&actual.bbox[3]<=found[2].bbox[1]+TOLERANCE_PT,'Unchanged mathematical ink overlaps an adjacent original text line');
    vector={expectedWidth,expectedHeight,actual,adjacentTextNoOverlap:true};
  }
  return {pages:value.pages.length,markers:found,gapsPt:gaps,tolerancePt:TOLERANCE_PT,...(vector?{vector}:{} )};
}
function checkGeometry(value,{requested,id,index=1,width,height}={}){
  assert.equal(value.ok,false);assert.equal(value.error_code,'publication_layout_geometry');const d=value.details;assert(d&&typeof d==='object'&&!Array.isArray(d));
  const allowed=['coordinateSpace','blockIndex','blockId','line','mode','requestedPt','requiredPt','availableWidthPt','availableHeightPt'];assert(Object.keys(d).every(k=>allowed.includes(k)));
  assert.equal(d.coordinateSpace,'pdfParagraphLine');assert.equal(d.mode,'exact');assert.equal(d.blockIndex,index);assert.equal(d.line,1);
  assert.equal(d.blockId,id);near(d.requestedPt,requested,'Exact requested physical pt');assert(Number.isFinite(d.requiredPt)&&d.requiredPt>d.requestedPt);
  for(const k of ['requiredPt','availableWidthPt','availableHeightPt'])assert(d[k]>0&&d[k]<=20000,'Malformed bounded geometry '+k);
  if(width!==undefined)near(d.availableWidthPt,width,'Current container width');if(height!==undefined)near(d.availableHeightPt,height,'Current section full body height');
  return d;
}
function checkMap(source,value,inspection,{duplicate}={}){
  assert.equal(value.ok,true);assert.equal(value.mode,'artifact');assert.equal(value.previewEngine,'typst');const map=value.sourceMap;
  assert(map&&/^[a-f0-9]{64}$/u.test(map.revision));assert.equal(map.revision,crypto.createHash('sha256').update(source).digest('hex'),'Preview is bound to another source revision');assert(Array.isArray(map.nodes)&&map.nodes.length&&Array.isArray(map.hits)&&map.hits.length);
  assert.equal(map.pages.length,inspection.pages.length);assert.equal(new Set(map.nodes.map(n=>n.id)).size,map.nodes.length);
  const nodes=new Map(map.nodes.map(n=>[n.id,n]));for(const node of map.nodes){assert(Number.isInteger(node.from)&&Number.isInteger(node.to)&&node.from>=0&&node.to>node.from&&node.to<=source.length);}
  for(const hit of map.hits){assert(nodes.has(hit.id));assert(Number.isInteger(hit.page)&&hit.page>=1&&hit.page<=map.pages.length);const [x,y,w,h]=hit.rect,[width,height]=map.pages[hit.page-1];
    assert([x,y,w,h].every(Number.isFinite)&&w>0&&h>0&&x>=-TOLERANCE_PT&&y>=-TOLERANCE_PT&&x+w<=width+TOLERANCE_PT&&y+h<=height+TOLERANCE_PT,'Source target escaped actual PDF page');}
  if(duplicate){const starts=[...source.matchAll(new RegExp(duplicate.replace(/[.*+?^${}()|[\]\\]/gu,'\\$&'),'gu'))].map(m=>m.index);assert.equal(starts.length,2);
    for(const text of [inspection.text,inspection.pypdfText])assert.equal(text.split(duplicate).length-1,2,'Actual duplicate body text was lost or repeated in a parser');
    const matched=starts.map(start=>map.nodes.filter(n=>n.editable&&n.from<=start&&n.to>=start+duplicate.length&&source.slice(n.from,n.to).trim()===duplicate));
    assert(matched.every(n=>n.length===1));assert.notEqual(matched[0][0].id,matched[1][0].id);for(const [node]of matched)assert(map.hits.some(h=>h.id===node.id),'Duplicate lost its own physical source target');
  }
  for(const page of inspection.pageObjects)assert(!String(page.annotations).includes('readmd-internal:publication/'),'Private mapping annotation escaped cleaned PDF');
  return {revision:map.revision,nodes:map.nodes.length,hits:map.hits.length,pages:map.pages.length,duplicateTargets:duplicate?2:0};
}
function checkLegacyPair(a,b){assert.equal(a.text,b.text);assert.equal(a.pypdfText,b.pypdfText);assert.deepEqual(a.pageObjects,b.pageObjects,'Absent/null typed mode changed legacy page objects');
  assert.deepEqual(a.pages.map(p=>[p.pixelsSha256,p.spans]),b.pages.map(p=>[p.pixelsSha256,p.spans]),'Absent/null typed mode changed legacy pixels/geometry');return {pages:a.pages.length,pageObjectsUnchanged:true,pixelsAndBaselinesUnchanged:true};}
module.exports={TOLERANCE_PT,checkPdf,checkGeometry,checkMap,checkLegacyPair};
