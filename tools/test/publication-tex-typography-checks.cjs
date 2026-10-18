'use strict';
// Independent actual-coordinate checks. TeX's missing source-map/outline
// capabilities are explicit; neither is inferred from source strings.
const assert=require('node:assert/strict');
const shared=require('./publication-typst-typography-checks.cjs');
const TOLERANCE_PT=shared.TOLERANCE_PT;
function near(a,b,label){assert(Number.isFinite(a)&&Math.abs(a-b)<=TOLERANCE_PT,`${label}: ${a} != ${b}`);}
function checkPdf(value,expected){return expected?.nativeMathFixture?checkMathMinimum(value,expected.engine,expected):shared.checkPdf(value,expected);}
function checkPreview(value,engine){
  assert.equal(value.ok,true);assert.equal(value.mode,'artifact');assert.equal(value.previewEngine,engine);
  assert.equal(value.sourceMap,null,'TeX preview has no independently validated source map in this slice');
  assert(typeof value.pdf==='string'&&value.pdf.length>100);assert(Number.isInteger(value.pages)&&value.pages>0);
  return {engine,pages:value.pages,actualArtifact:true,sourceMapAvailable:false};
}
function checkMathMinimum(value,engine,expected={gap:18}){
  // Only this fixed source fixture permits a parser's final denominator Z to
  // touch THIRD in extracted text. Match the complete original token sequence
  // and actual painted body glyph inventory before inserting that boundary.
  for(const text of [value.text,value.pypdfText])assert(/^\s*FIRST\s+SECOND\s+[X𝑋]\s+[Y𝑌]\s+[Z𝑍]\s*THIRD\s*$/u.test(text),'Native math original token inventory changed');
  const body=[];for(const page of value.pages)for(const trace of page.glyphTraces||[])if(!/math|lmmi/iu.test(trace.font))for(const char of trace.chars){
    if(/^\s$/u.test(String.fromCodePoint(char.unicode)))continue;assert(Number.isInteger(char.glyphId)&&char.glyphId>0,'Original body glyph is missing');body.push(String.fromCodePoint(char.unicode));}
  assert.equal(body.join(''),'FIRSTSECONDTHIRD','Actual painted body glyph inventory changed');
  const pypdfText=value.pypdfText.replace(/([Z𝑍])(?=THIRD\s*$)/u,'$1\n');
  const baseline=shared.checkPdf({...value,pypdfText},{...expected,minimum:true});
  const candidates=[];
  for(const [page,p]of value.pages.entries())for(const trace of p.glyphTraces||[]){
    if(!/math|lmmi/iu.test(trace.font))continue;
    for(const char of trace.chars){if(/^\s$/u.test(String.fromCodePoint(char.unicode)))continue;const name={88:'X',89:'Y',90:'Z',119883:'X',119884:'Y',119885:'Z'}[char.unicode];assert(name,'Unexpected original native math glyph');candidates.push({...char,font:trace.font,size:trace.size,page:page+1,name});}
  }
  for(const name of ['X','Y','Z']){const list=candidates.filter(c=>c.name===name);assert.equal(list.length,1,'Native math glyph missing/duplicated: '+name);
    assert(Number.isInteger(list[0].glyphId)&&list[0].glyphId>0);near(list[0].size,11,'Native display-fraction glyph retains physical body size: '+name);}
  assert(candidates.every(c=>c.page===baseline.markers[1].page),'Short native math paragraph unexpectedly changed pages');
  return {engine,baseline,nativeMathGlyphs:candidates,physicalNumeratorSizePt:11,
    parserSegmentation:{scope:'Fixed FIRST/SECOND/dfrac(X,dfrac(Y,Z))/THIRD source only',rawPypdfText:value.pypdfText,finalDenominatorBoundaryInserted:pypdfText!==value.pypdfText,actualBodyGlyphInventory:body.join('')},
    actualOutlineInkVerified:false,outlineProofStatus:'pending-separate-Type1/OpenType-proof',
    limitation:'Baseline/font-size/real native glyph evidence only; this check does not certify the unmeasured ink envelope.'};
}
function checkGeometry(value,expected){return shared.checkGeometry(value,expected);}
function checkLegacyPair(a,b){return shared.checkLegacyPair(a,b);}
function checkUnicode(value){for(const text of [value.text,value.pypdfText])for(const token of ['中文','😀'])assert.equal(text.split(token).length-1,1,'Original Unicode text was lost/duplicated: '+token);
  const actual=value.pages.flatMap(p=>p.physicalAstralGlyphs||[]),emoji=actual.filter(g=>g.unicode===0x1F600);assert.equal(emoji.length,1,'Actual embedded emoji glyph proof is missing/duplicated');
  const g=emoji[0];assert(Number.isInteger(g.glyphId)&&g.glyphId>0);assert.equal(g.unitsPerEm,2048);assert.equal(g.advanceUnits,2135);assert.deepEqual(g.inkBounds,[170,-150,1965,1646]);assert(g.fontBytes>0&&g.darkPixels>8,'Physical original emoji outline is missing');
  return {bothParsersPreserveOriginalUnicode:true,physicalAstralGlyphs:actual};}
module.exports={TOLERANCE_PT,checkPdf,checkPreview,checkMathMinimum,checkGeometry,checkLegacyPair,checkUnicode};
