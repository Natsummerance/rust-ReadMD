'use strict';
const assert=require('node:assert/strict');
const tolerance=.25;
const fontKeys=['fontFamily','fontSize','fontWeight','fontStyle','fontVariant','fontVariantCaps','lineHeight','letterSpacing','whiteSpace','wordBreak','overflowWrap','display'];
function validRect(r){assert(r&&['left','top','right','bottom','width','height'].every(k=>Number.isFinite(r[k])));assert(r.width>0&&r.height>0);assert(Math.abs(r.right-r.left-r.width)<=tolerance&&Math.abs(r.bottom-r.top-r.height)<=tolerance);}
function checkCitationFlow(first,second,text){
  assert(/^[\x21-\x7e]+$/u.test(text),'This independent probe is the exact authored ASCII CSL guard');
  const inspect=flow=>{
    assert(flow&&Array.isArray(flow.characters)&&Array.isArray(flow.elementRects)&&Array.isArray(flow.contentRangeRects),'Missing actual citation fragment proof');
    validRect(flow.parentBounds);assert.equal(flow.characters.length,text.length);assert.equal(flow.characters.map(c=>c.text).join(''),text);
    for(const key of fontKeys)assert.equal(typeof flow.font[key],'string','Missing computed citation font '+key);
    const pitch=Number.parseFloat(flow.font.lineHeight);assert(Number.isFinite(pitch)&&pitch>0);
    const groups=[];
    for(const [index,char]of flow.characters.entries()){
      assert.equal(char.unicode,text.charCodeAt(index));assert.equal(char.text,text[index]);assert.equal(char.rects.length,1,'A probe character needs one actual Range rectangle');
      const r=char.rects[0];validRect(r);for(const key of fontKeys)assert.equal(typeof char.font[key],'string','Missing computed glyph font '+key);
      assert(r.left>=flow.parentBounds.left-tolerance&&r.right<=flow.parentBounds.right+tolerance&&r.top>=flow.parentBounds.top-tolerance&&r.bottom<=flow.parentBounds.bottom+tolerance,'Citation glyph escapes its real paragraph');
      const last=groups.at(-1);
      if(last&&Math.abs(r.top-last.top)<=tolerance){assert(Math.abs(r.left-last.right)<=tolerance,'Unexplained citation character gap/overlap');assert(Math.abs(r.bottom-last.bottom)<=tolerance,'Citation character baseline/height changed');last.right=r.right;last.width=last.right-last.left;}
      else{if(last){assert(Math.abs(r.top-last.top-pitch)<=tolerance,'Citation continuation has the wrong line pitch');assert(Math.abs(r.left-flow.parentBounds.left)<=tolerance,'Citation continuation does not start at its paragraph edge');}groups.push({...r});}
    }
    assert.equal(flow.elementRects.length,groups.length,'Citation physical fragment count is unexplained');
    for(let i=0;i<groups.length;i++){validRect(flow.elementRects[i]);for(const key of ['left','top','right','bottom','width','height'])assert(Math.abs(flow.elementRects[i][key]-groups[i][key])<=tolerance,'Citation fragment does not cover its exact characters');}
    assert(flow.contentRangeRects.length>=groups.length);for(const r of flow.contentRangeRects){validRect(r);assert(groups.some(g=>['left','top','right','bottom','width','height'].every(k=>Math.abs(r[k]-g[k])<=tolerance)),'Content Range has an unexplained rectangle');}
    return groups;
  };
  const a=inspect(first),b=inspect(second);assert.deepEqual(second.font,first.font,'Citation computed font/spacing changed');
  for(let i=0;i<first.characters.length;i++){
    const original=first.characters[i],tracked=second.characters[i];assert.deepEqual(tracked.font,original.font,'Citation glyph font/spacing changed');
    for(const key of ['width','height'])assert(Math.abs(original.rects[0][key]-tracked.rects[0][key])<=tolerance,'Citation actual glyph metric changed');
  }
  assert(Math.abs(a.reduce((n,r)=>n+r.width,0)-b.reduce((n,r)=>n+r.width,0))<=tolerance,'Citation cumulative actual advance changed');
  return{characters:text.length,firstFragments:a.length,secondFragments:b.length,tolerancePx:tolerance,
    firstInlineAdvance:a.reduce((n,r)=>n+r.width,0),secondInlineAdvance:b.reduce((n,r)=>n+r.width,0),sameGlyphMetrics:true,legalReflow:true};
}
// Direct page.evaluate(function), with no eval strings or mutation of any subtree.
function captureReaderGeometry(){
  const source=getEditContent(),rows=[...document.querySelectorAll('#preview-pane p')].filter(p=>p.textContent.startsWith('Repeated source —'));
  const rect=r=>({left:r.left,top:r.top,right:r.right,bottom:r.bottom,width:r.width,height:r.height});
  const font=element=>{const s=getComputedStyle(element);return Object.fromEntries(['fontFamily','fontSize','fontWeight','fontStyle','fontVariant','fontVariantCaps','lineHeight','letterSpacing','whiteSpace','wordBreak','overflowWrap','display'].map(k=>[k,s[k]]));};
  const flow=element=>{const range=document.createRange();range.selectNodeContents(element);const walker=document.createTreeWalker(element,NodeFilter.SHOW_TEXT),characters=[];let node;
    while((node=walker.nextNode()))for(let i=0;i<node.length;i++){const r=document.createRange();r.setStart(node,i);r.setEnd(node,i+1);characters.push({unicode:node.data.charCodeAt(i),text:node.data.slice(i,i+1),rects:[...r.getClientRects()].map(rect),font:font(node.parentElement)});}
    return{parentBounds:rect(element.closest('p').getBoundingClientRect()),font:font(element),characters,elementRects:[...element.getClientRects()].map(rect),contentRangeRects:[...range.getClientRects()].map(rect)};};
  const read=element=>{const box=element.getBoundingClientRect();return{text:element.textContent,width:box.width,height:box.height,spacing:getComputedStyle(element).letterSpacing};};
  return{source,revision:rows[0].querySelector('.readmd-citation').dataset.readmdSourceRevision,rows:rows.map(p=>{
    const text=[...p.childNodes].find(n=>n.nodeType===3&&n.nodeValue.startsWith('Repeated source —')),range=document.createRange();range.setStart(text,0);range.setEnd(text,'Repeated source'.length);
    const cite=p.querySelector('.readmd-citation'),math=p.querySelector('mjx-container');
    return{prefix:range.toString(),prefixWidth:range.getBoundingClientRect().width,spacing:getComputedStyle(p).letterSpacing,tracked:p.hasAttribute('data-readmd-tracking'),code:read(p.querySelector('code')),citation:{...read(cite),flow:flow(cite),from:Number(cite.dataset.readmdSourceFrom),to:Number(cite.dataset.readmdSourceTo),revision:cite.dataset.readmdSourceRevision,format:[...cite.querySelectorAll('[class]')].map(e=>e.className).join('|')},math:{...read(math),shapes:[...math.querySelectorAll('path,use')].map(e=>[e.tagName,e.getAttribute('d'),(e.getAttribute('href')||e.getAttribute('xlink:href')||'').replace(/MJX-?\d+-/g,'MJX-')])}};
  })};
}
module.exports={checkCitationFlow,captureReaderGeometry};
