'use strict';
// Pure QA contracts; no native/product process or expected output generation.
const assert=require('node:assert/strict'),c=require('./publication-structure-checks.cjs'),s=require('./publication-structure-spatial.cjs');
const results=[],yes=(name,fn)=>{fn();results.push({name,accepted:true,expected:true});},no=(name,fn)=>{assert.throws(fn);results.push({name,accepted:false,expected:false});};
const inline=text=>({type:'inline',runs:[{text,marks:{},href:null}]});let identity=0;
function row(text,x,y){return[...text].map((ch,i)=>({id:identity++,page:1,c:ch,size:11,origin:[x+i*3.4,y],bbox:[x+i*3.4,y-8,x+i*3.4+2.5,y+2]}));}
const box={left:0,right:100},flow=[{type:'columns',left:[inline('AB')],right:[inline(' HELLO WORLD ')]}],chars=[...row('AB',0,20),...row('HELLO WORLD',20,20)],prove=value=>s.proveFlow(flow,value,box,{fontSize:11,assertExact:c.assertExactSegments});
yes('proved gutter represents collapsible cell-edge whitespace without painted spaces',()=>prove(chars));
yes('actual copy spaces inside a cell remain valid',()=>prove([...row('AB',0,20),...row(' HELLO WORLD ',20,20)]));
yes('an explicit geometric gutter copy-space is classified, not a lost body character',()=>{const blank=row(' ',8,20)[0];blank.bbox[2]=19;prove([...chars,blank]);});
for(const[name,change]of [['interior word space cannot be omitted',x=>x.splice(x.findIndex(q=>q.c===' '),1)],['non-white original character cannot be omitted',x=>x.pop()],['extra body word cannot be swallowed',x=>x.push(...row('Q',70,20))],['reordered body cannot pass',x=>{[x[2].c,x[3].c]=[x[3].c,x[2].c];}],['whitespace outside parent is rejected',x=>x.push(...row(' ',110,20))],['whitespace outside a left cell is rejected',x=>{const w=row(' ',-3,20)[0];w.bbox[2]=1;x.push(w);}]] )no(name,()=>{const v=structuredClone(chars);change(v);prove(v);});
no('NBSP at the original cell boundary is not collapsible ASCII structure',()=>s.proveFlow([{type:'columns',left:[inline('AB')],right:[inline('\u00a0HELLO WORLD')]}],chars,box,{fontSize:11,assertExact:c.assertExactSegments}));
no('edge whitespace never replaces the original .75em gutter',()=>s.proveFlow(flow,[...row('AB',0,20),...row('HELLO WORLD',10,20)],box,{fontSize:11,assertExact:c.assertExactSegments}));
const wrapped=[{type:'columns',left:[inline('AB')],right:[inline(' ABCDEFGHIJK')]}],wrappedChars=[...row('AB',0,20),...row(' ABCDEF',20,20),...row('GHIJK',20,31)];
yes('actual leading copy-space and continuation expose the cell origin',()=>s.proveFlow(wrapped,wrappedChars,box,{fontSize:11,assertExact:c.assertExactSegments}));
yes('a continuation exposes an unpainted leading-space cell origin',()=>s.proveFlow(wrapped,[...row('AB',0,20),...row('ABCDEF',23.4,20),...row('GHIJK',20,31)],box,{fontSize:11,assertExact:c.assertExactSegments}));
no('continuation beyond the physical gutter remains invalid',()=>s.proveFlow(wrapped,[...row('AB',0,20),...row('ABCDEF',23.4,20),...row('GHIJK',8,31)],box,{fontSize:11,assertExact:c.assertExactSegments}));
yes('a complete entry can continue on the next page before its indent',()=>c.assertMarkerOrder({page:1,y:600},{page:2,y:100},'control'));
no('a previous-page indent is still rejected',()=>c.assertMarkerOrder({page:2,y:100},{page:1,y:600},'control'));
no('a reversed same-page indent is still rejected',()=>c.assertMarkerOrder({page:1,y:600},{page:1,y:100},'control'));
const end='READMDQABIBENDABCDEF123456',fixture={id:'independent-own-notes',boundaryMarker:end,expectedNotes:[{number:1,segments:['WORD ONE']}],expectedLayout:[{type:'block',indent:false,children:[inline('BODY')]}]};
function synchronizeCharacters(value){let next=identity;value.pages[0].characters=value.pages[0].spans.flatMap(s=>[...s.text].map((ch,i)=>({id:next++,page:1,c:ch,size:s.size,origin:[s.origin[0]+i*3.4,s.origin[1]],bbox:[s.origin[0]+i*3.4,s.origin[1]-8,s.origin[0]+i*3.4+2.5,s.origin[1]+2]})));identity=next;return value;}
function actual(){const left=18*72/25.4,parts=[['References',40,11,left],['BODY',80,11,left],[end,100,11,left],['1',716,5.4,left],['WORD',720,8.8,left+10],[' ',720,8.8,left+23.6],['ONE',720,8.8,left+27]],spans=parts.map(([text,y,size,x])=>({text,size,origin:[x,y],bbox:[x,y-8,x+text.length*3.4,y+2]}));const value=synchronizeCharacters({kind:'pdf',text:'References\nBODY\n'+end+'\n1\nWORD ONE',pypdfText:'References\nBODY\n'+end+'\n1\nWORD ONE',pages:[{width:595,height:842,spans,drawings:[{width:.5,bbox:[left,700,left+100,700]}]}]});value.pages[0].glyphTraces=[{size:11,chars:value.pages[0].characters.map(ch=>({...ch,unicode:ch.c.codePointAt(0),glyphId:ch.id+1}))}];return value;}
yes('a real space-only footnote span is preserved between differently styled words',()=>c.displayPdfRegions(actual(),fixture,{engine:'xelatex'}));
no('lost actual footnote word space is rejected',()=>{const v=actual();v.pages[0].spans=v.pages[0].spans.filter(x=>x.text!==' ');synchronizeCharacters(v);c.displayPdfRegions(v,fixture,{engine:'xelatex'});});
no('additional note word is not accepted as whitespace repair',()=>{const v=actual();v.pages[0].spans.at(-1).text+=' EXTRA';synchronizeCharacters(v);c.displayPdfRegions(v,fixture,{engine:'xelatex'});});
no('changed independent label is not accepted as whitespace repair',()=>{const v=actual();v.pages[0].spans[3].text='2';synchronizeCharacters(v);c.displayPdfRegions(v,fixture,{engine:'xelatex'});});
yes('pypdf inserted ASCII space inside exact actual glyph boundary is recognized',()=>{const v=actual();v.pypdfText=v.pypdfText.replace(end,end.slice(0,19)+' '+end.slice(19));c.displayPdfRegions(v,fixture,{engine:'xelatex'});});
no('MuPDF boundary remains exact',()=>{const v=actual();v.text=v.text.replace(end,end.slice(0,19)+' '+end.slice(19));c.displayPdfRegions(v,fixture,{engine:'xelatex'});});
no('pypdf changed boundary character is rejected',()=>{const v=actual();v.pypdfText=v.pypdfText.replace(end,end.slice(0,-1)+'0');c.displayPdfRegions(v,fixture,{engine:'xelatex'});});
no('pypdf missing boundary character is rejected',()=>{const v=actual();v.pypdfText=v.pypdfText.replace(end,end.slice(0,-1));c.displayPdfRegions(v,fixture,{engine:'xelatex'});});
no('duplicate pypdf boundary is rejected',()=>{const v=actual();v.pypdfText+='\n'+end;c.displayPdfRegions(v,fixture,{engine:'xelatex'});});
no('missing actual glyph boundary cannot be repaired by pypdf text',()=>{const v=actual();for(const t of v.pages[0].glyphTraces)t.chars=t.chars.filter(ch=>Math.abs(ch.origin[1]-100)>.25);c.displayPdfRegions(v,fixture,{engine:'xelatex'});});
console.log(JSON.stringify({passed:true,productInvocations:0,nativeAcceptance:false,controls:results.length,results}));
