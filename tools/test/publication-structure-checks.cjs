'use strict';
const assert=require('node:assert/strict');
const spatial=require('./publication-structure-spatial.cjs');
const TOLERANCE=Object.freeze({coordinatePt:.75,intervalPt:1,baselinePt:.25,cssPx:1,mmPt:72/25.4});
const plain=nodes=>(nodes||[]).map(n=>n.text??plain(n.children)).join('');
const compact=value=>value.replace(/\s+/gu,' ').trim();
const near=(a,b,t=TOLERANCE.coordinatePt,label='coordinate')=>assert(Number.isFinite(a)&&Number.isFinite(b)&&Math.abs(a-b)<=t,`${label}: ${a} != ${b} ±${t}`);
const escape=value=>value.replace(/[.*+?^${}()|[\]\\]/g,'\\$&');
function textPattern(segment){
  const value=typeof segment==='string'?segment:segment.text,characterWrap=typeof segment==='object'&&segment.characterWrap===true;
  // Only physical discretionary hyphen+newline inside an original unhyphenated
  // word is equivalent. Required word spaces remain required: missing letters
  // and joined words cannot pass this boundary normalization.
  return value.split(/(\s+)/u).map(piece=>/^\s+$/u.test(piece)?'\\s+':[...piece].map((c,i,chars)=>escape(c)+(i+1<chars.length?(characterWrap?'(?:-?\\r?\\n)?':/\p{L}/u.test(c)&&/\p{L}/u.test(chars[i+1])?'(?:-\\r?\\n)?':''):'')).join('')).join('');
}
function assertSegments(actual,segments,label){
  let cursor=0;const matches=[];
  for(const expected of segments){if(!expected)continue;const regex=new RegExp(textPattern(expected),'u');const match=regex.exec(actual.slice(cursor));
    const value=typeof expected==='string'?expected:expected.text;assert(match,`${label}: missing or changed original segment ${value.slice(0,90)}`);
    const from=cursor+match.index,to=from+match[0].length;matches.push({from,to,expectedLength:value.length});cursor=to;}
  return matches;
}
function assertExactSegments(actual,segments,label){
  // Consume one anchored original segment at a time. A 90-paragraph entry
  // must not become one enormous V8 regular expression. No substring search,
  // expected-length truncation, or non-whitespace tail is permitted.
  let cursor=0;const matches=[];
  for(const segment of segments.filter(Boolean)){
    const value=typeof segment==='string'?segment:segment.text;
    const match=new RegExp('^\\s*('+textPattern(segment)+')','u').exec(actual.slice(cursor));
    assert(match,label+': original segment missing, changed or reordered');
    const leading=match[0].length-match[1].length;
    matches.push({from:cursor+leading,to:cursor+match[0].length,expectedLength:value.length});cursor+=match[0].length;
  }
  assert(/^\s*$/u.test(actual.slice(cursor)),label+': extra or duplicated trailing text');return matches;
}
function decode(value){return value.replace(/&(#x[\da-f]+|#\d+|amp|lt|gt|quot|apos|nbsp);/giu,(_,n)=>n[0]==='#'?String.fromCodePoint(n[1].toLowerCase()==='x'?parseInt(n.slice(2),16):Number(n.slice(1))):({amp:'&',lt:'<',gt:'>',quot:'"',apos:"'",nbsp:'\u00a0'})[n.toLowerCase()]);}
function parseExpectedHTML(source){
  assert(source.length<1024*1024,'Bounded original RESULT');
  const root={kind:'root',children:[]},stack=[root],parts=source.match(/<[^>]*>|[^<]+/gu)||[];assert.equal(parts.join(''),source);
  for(const token of parts){if(!token.startsWith('<')){const value=decode(token),top=stack.at(-1);if(/^\s*$/u.test(value)&&value.includes('\n')&&['root','body','entry'].includes(top.kind))continue;
      if(value)top.children.push({kind:'text',text:value,marks:{...(top.marks||{})},href:top.href||null});continue;}
    const m=/^<(\/)?([a-z][\w-]*)([^>]*)>$/iu.exec(token);assert(m,'Restricted original RESULT token');const tag=m[2].toLowerCase();
    if(m[1]){assert.equal(stack.pop().tag,tag);assert(stack.length);continue;}
    assert(['div','span','i','em','b','strong','u','sup','sub','a','br'].includes(tag),'Unreviewed original RESULT element');
    const attrs={};let rest=m[3];while(rest.trim()&&rest.trim()!=='/'){const a=/^\s+([\w:-]+)\s*=\s*(?:"([^"]*)"|'([^']*)')/u.exec(rest);assert(a,'Restricted original RESULT attributes');assert(!Object.hasOwn(attrs,a[1]));attrs[a[1]]=decode(a[2]??a[3]);rest=rest.slice(a[0].length);}
    for(const key of Object.keys(attrs))assert(['class','style','href'].includes(key),'Unreviewed original RESULT attribute');
    if(tag==='br'){stack.at(-1).children.push({kind:'text',text:'\n',marks:{...(stack.at(-1).marks||{})},href:null});continue;}
    const classes={'csl-bib-body':'body','csl-entry':'entry','csl-block':'block','csl-indent':'indent','csl-left-margin':'left','csl-right-inline':'right'};
    const node={tag,kind:tag==='div'?classes[attrs.class]:'inline',children:[],marks:{...(stack.at(-1).marks||{})},href:attrs.href??stack.at(-1).href??null};
    assert(node.kind,'Unreviewed original RESULT class');
    for(const [tags,key,value] of [[['i','em'],'font_style','italic'],[['b','strong'],'font_weight','bold'],[['u'],'text_decoration','underline'],[['sup'],'vertical_align','sup'],[['sub'],'vertical_align','sub']])if(tags.includes(tag))node.marks[key]=value;
    const axes={'font-style':'font_style','font-weight':'font_weight','font-variant':'font_variant','text-decoration':'text_decoration','vertical-align':'vertical_align'};
    for(const declaration of (attrs.style||'').split(';').filter(s=>s.trim())){const [key,value,...extra]=declaration.split(':').map(s=>s.trim());assert(!extra.length&&axes[key],'Restricted original RESULT CSS');node.marks[axes[key]]=value==='small-caps'?'small-caps':value==='super'?'sup':value;}
    stack.at(-1).children.push(node);stack.push(node);assert(stack.length<=32,'Bounded original RESULT depth');
  }assert.equal(stack.length,1);const entries=[];
  function collect(node){if(node.kind==='entry')entries.push(node);else for(const child of node.children||[])collect(child);}collect(root);assert(entries.length);
  const textOf=node=>node.text??(node.children||[]).map(textOf).join('');
  function segments(node,out,pending){for(const child of node.children||[]){if(child.kind==='text')pending.value+=child.text;else if(['block','indent','left','right'].includes(child.kind)){if(pending.value){out.push(pending.value);pending.value='';}segments(child,out,pending);if(pending.value){out.push(pending.value);pending.value='';}}else segments(child,out,pending);}}
  return entries.map(node=>{const out=[],pending={value:''};segments(node,out,pending);if(pending.value)out.push(pending.value);return{node,segments:out,text:textOf(node)};});
}
function canonicalRich(nodes,inheritedHref=null){const output=[];function append(text,marks,href){const clean=Object.fromEntries(Object.entries(marks).filter(([,v])=>!['normal','none','baseline'].includes(v)).sort());const previous=output.at(-1);
    if(previous&&JSON.stringify(previous.marks)===JSON.stringify(clean)&&previous.href===href)previous.text+=text;else if(text)output.push({text,marks:clean,href});}
  function walk(nodes,href=null){for(const n of nodes||[]){if(n.type==='text')append(n.text,n.format||{},href);else walk(n.children,n.type==='link'?n.href:href);}}walk(nodes,inheritedHref);return output;}
function expectedRich(node){const output=[];function walk(n){if(n.kind==='text'){const marks=Object.fromEntries(Object.entries(n.marks).filter(([,v])=>!['normal','none','baseline'].includes(v)).sort()),p=output.at(-1);if(p&&JSON.stringify(p.marks)===JSON.stringify(marks)&&p.href===n.href)p.text+=n.text;else output.push({text:n.text,marks,href:n.href});}else for(const child of n.children||[])walk(child);}walk(node);return output;}
function mergeRuns(runs){const out=[];for(const run of runs){if(!run.text)continue;const p=out.at(-1);if(p&&JSON.stringify(p.marks)===JSON.stringify(run.marks)&&p.href===run.href)p.text+=run.text;else out.push({...run});}return out;}
function cellFlow(flow){let nodes=packFlow(flow);while(nodes.length===1&&nodes[0].type==='block'&&!nodes[0].indent)nodes=nodes[0].children;return nodes;}
function packFlow(flow){const out=[];for(const n of flow){
  if(n.type==='inline'){const runs=mergeRuns(n.runs);if(!runs.length)continue;const last=out.at(-1);if(last?.type==='inline')last.runs=mergeRuns([...last.runs,...runs]);else out.push({type:'inline',runs});}
  else if(n.type==='block'){const children=packFlow(n.children);if(!children.length&&!n.indent)continue;out.push({type:'block',indent:n.indent,children});}
  else if(n.type==='columns')out.push({type:'columns',left:cellFlow(n.left),right:cellFlow(n.right)});
  else assert.fail('Unreviewed recursive layout node');
}return out;}
function expectedFlow(children){
  // Transparent HTML inline wrappers carry marks/hrefs already attached to
  // their text leaves. Only adjacent left/right divs form a column row.
  const parts=[];function expand(nodes){for(const n of nodes||[])if(n.kind==='inline')expand(n.children);else parts.push(n);}expand(children);
  const out=[],pending=[];const flush=()=>{if(pending.length){out.push({type:'inline',runs:expectedRich({children:pending.splice(0)})});}};
  for(let i=0;i<parts.length;i++){const n=parts[i];if(n.kind==='text'){pending.push(n);continue;}flush();
    if(n.kind==='left'&&parts[i+1]?.kind==='right'){const right=[];while(parts[i+1]?.kind==='right')right.push({type:'block',indent:false,children:expectedFlow(parts[++i].children)});out.push({type:'columns',left:expectedFlow(n.children),right});}
    else if(['left','right','block','indent'].includes(n.kind))out.push({type:'block',indent:n.kind==='indent',children:expectedFlow(n.children)});
    else assert.fail('Unexpected original RESULT layout container');
  }flush();return packFlow(out);
}
function actualFlow(nodes,href=null){return packFlow((nodes||[]).flatMap(n=>{
  if(n.type==='inline')return[{type:'inline',runs:canonicalRich(n.children,href)}];
  if(n.type==='block'){assert.equal(typeof n.indent,'boolean');return[{type:'block',indent:n.indent,children:actualFlow(n.children,href)}];}
  if(n.type==='columns')return[{type:'columns',left:actualFlow(n.left,href),right:actualFlow(n.right,href)}];
  if(n.type==='group')return actualFlow(n.children,href);
  if(n.type==='link'){assert.equal(typeof n.href,'string');return actualFlow(n.children,n.href);}
  assert.fail('Unreviewed actual typed recursive layout');
}));}
function projectedFlow(item){
  const convert=(nodes,href=null,omit=null)=>(nodes||[]).map(n=>{
    if(n.type==='text')return{kind:'text',text:n.text,marks:n.format||{},href};
    if(n.type==='link')return{kind:'inline',children:convert(n.children,n.href,omit)};
    assert.equal(n.type,'group');const display=n.display||null,kind=display===omit||!display?'inline':({'left-margin':'left','right-inline':'right',block:'block',indent:'indent'})[display];assert(kind,'Unreviewed projected display');
    return{kind,children:convert(n.children,href,kind==='inline'?omit:null)};
  });
  const first=item.first_field||[];
  return first.length?expectedFlow([{kind:'left',children:convert(first,null,'left-margin')},{kind:'right',children:convert(item.rich,null,'right-inline')}]):expectedFlow(convert(item.rich));
}
function layoutCounts(nodes){const counts={columns:0,block:0,indent:0};for(const n of nodes||[]){if(n.type==='columns'){counts.columns++;for(const k of ['left','right']){const c=layoutCounts(n[k]);for(const key in c)counts[key]+=c[key];}}else if(n.children){if(n.type==='block'){counts.block++;if(n.indent)counts.indent++;}const c=layoutCounts(n.children);for(const key in c)counts[key]+=c[key];}}return counts;}
function checkBibliography(data,fixture){const bib=data.bibliography||data.projection?.bibliography;assert(bib&&Array.isArray(bib.items),'Shared bibliography missing');assert.equal(bib.items.length,fixture.expected.length);const totals={columns:0,block:0,indent:0};
  for(const [i,item]of bib.items.entries()){const expected=fixture.expected[i];if(expected.key)assert.equal(item.key,expected.key);
    const first=Array.isArray(item.first_field_rich)?item.first_field_rich:Array.isArray(item.first_field)?item.first_field:[];
    if(expected.node)assert.deepEqual(canonicalRich([...first,...item.rich]),expectedRich(expected.node),'Fixed independent RESULT text/format/order changed');
    else assertExactSegments(plain(first)+plain(item.rich),expected.segments,fixture.id+' typed entry '+i);
    if(data.projection?.bibliography){const projected=data.projection.bibliography.items[i];assert.equal(projected.key,item.key);
      assert.deepEqual(canonicalRich([...(projected.first_field||[]),...projected.rich]),canonicalRich([...first,...item.rich]),'Source projection loses text, marks, href or reading order');
      assert.deepEqual(projectedFlow(projected),actualFlow(item.layout),'Source projection loses recursive display layout');}
    assert(Array.isArray(item.layout),'Actual typed recursive layout is missing');const c=layoutCounts(item.layout);for(const key in c)totals[key]+=c[key];
    if(expected.node)assert.deepEqual(actualFlow(item.layout),expectedFlow(expected.node.children),'Original RESULT recursive block/indent/column placement changed');
    if(fixture.expectedLayout)assert.deepEqual(actualFlow(item.layout),packFlow(fixture.expectedLayout[i].children),'Original self-authored recursive layout changed');
  }
  if(fixture.minimumColumns)assert(totals.columns>=fixture.minimumColumns,'Columns were flattened');if(fixture.minimumBlocks)assert(totals.block>=fixture.minimumBlocks,'Blocks were flattened');
  return{items:bib.items.length,counts:totals};}
function glyphLines(result){return result.pages.flatMap((page,index)=>{const lines=[];for(const [traceIndex,trace]of(page.glyphTraces||[]).entries())for(const [order,c]of trace.chars.entries()){
      if(c.unicode<32||c.unicode===0x7f)continue;let line=lines.find(l=>Math.abs(l.y-c.origin[1])<=TOLERANCE.baselinePt);if(!line){line={page:index+1,y:c.origin[1],chars:[]};lines.push(line);}line.chars.push({...c,c:String.fromCodePoint(c.unicode),size:trace.size,trace:traceIndex,order});}
    for(const l of lines){l.chars.sort((a,b)=>a.origin[0]-b.origin[0]||a.trace-b.trace||a.order-b.order);l.text=l.chars.map(c=>c.c).join('');}return lines.sort((a,b)=>a.y-b.y);});}
function marker(result,value){const found=[];for(const line of glyphLines(result)){const at=line.text.indexOf(value);if(at<0)continue;assert.equal(line.text.indexOf(value,at+value.length),-1,'Ambiguous marker on one actual PDF line');let cursor=0;const chars=line.chars.filter(c=>{const start=cursor;cursor+=c.c.length;return start>=at&&start<at+value.length;});
    assert.equal(chars.map(c=>c.c).join(''),value);found.push({page:line.page,x:chars[0].origin[0],y:line.y,bbox:[Math.min(...chars.map(c=>c.bbox[0])),Math.min(...chars.map(c=>c.bbox[1])),Math.max(...chars.map(c=>c.bbox[2])),Math.max(...chars.map(c=>c.bbox[3]))],chars});}
  assert.equal(found.length,1,'Actual marker missing/ambiguous: '+value);assert(found[0].chars.every(c=>c.glyphId>0),'Actual marker contains .notdef');return found[0];}
function checkPdfBounds(result){const bad=[];for(const [i,p]of result.pages.entries())for(const t of p.glyphTraces||[])for(const c of t.chars){if(c.unicode<=32)continue;
    if(c.glyphId===0||c.bbox[0]<-TOLERANCE.coordinatePt||c.bbox[1]<-TOLERANCE.coordinatePt||c.bbox[2]>p.width+TOLERANCE.coordinatePt||c.bbox[3]>p.height+TOLERANCE.coordinatePt)bad.push({page:i+1,unicode:c.unicode,glyphId:c.glyphId,bbox:c.bbox});}assert.deepEqual(bad,[],'Visible PDF glyph lost or outside actual page');}
function checkSectionPdf(result,expected){assert.equal(result.pages.length,expected.pageCount);checkPdfBounds(result);const measured={};for(const [name,page]of Object.entries(expected.physicalPages)){measured[name]=marker(result,name);assert.equal(measured[name].page,page,'Section starts on wrong physical page: '+name);}
  for(const [i,page]of result.pages.entries()){const landscape=[3,4].includes(i+1);near(page.width,(landscape?297:210)*TOLERANCE.mmPt,.75,'Actual paper width');near(page.height,(landscape?210:297)*TOLERANCE.mmPt,.75,'Actual paper height');}
  const rows=Array.from({length:65},(_,i)=>marker(result,'P03MAIN'+String(i).padStart(2,'0')));const left=18*TOLERANCE.mmPt,right=left+(210-36-8)/2*TOLERANCE.mmPt+8*TOLERANCE.mmPt;
  assert(rows.some(r=>Math.abs(r.x-left)<=.75)&&rows.some(r=>Math.abs(r.x-right)<=.75),'Two real text columns are required');for(const r of rows)assert(Math.min(Math.abs(r.x-left),Math.abs(r.x-right))<=.75,'Body origin is outside both declared columns');
  near(measured.P03FULL.x,left);near(measured.P03RESUME.x,left);assert(measured.P03FULL.y>rows[0].y);assert(measured.P03RESUME.y>measured.P03FULL.y);
  for(const page of expected.blankPages)assert.equal(result.pages[page-1].text.trim(),'','Physical parity padding page must be blank');
  for(const [page,header,footer,num]of [[1,'FRONTHEADER','FRONTFOOTER','i'],[2,'MAINHEADER','MAINFOOTER','1'],[3,'MAINHEADER','MAINFOOTER','2'],[5,'ODDHEADER','ODDFOOTER','A'],[6,'','ODDFOOTER','7']]){
    const p=result.pages[page-1],top=p.spans.filter(s=>s.bbox[1]<50).map(s=>s.text).join(' '),bottom=p.spans.filter(s=>s.bbox[1]>p.height-70).map(s=>s.text).join(' ');
    if(header)assert(top.includes(header),'Actual running header missing');else assert(!/FRONTHEADER|MAINHEADER|ODDHEADER/u.test(top),'Explicit blank header inherited a prior section');
    assert(new RegExp(escape(footer)+'\\s*'+escape(num)+'(?:\\s|$)','u').test(bottom),'Actual page numbering did not reset/continue independently of physical parity: '+bottom);
  }return{markers:measured,rows,physicalPages:result.pages.map(p=>({width:p.width,height:p.height,textShaOnly:true}))};}
function regionText(spans){let out='',previous=null;for(const span of spans){if(previous&&(previous.page!==span.page||Math.abs(previous.origin[1]-span.origin[1])>TOLERANCE.baselinePt))out+='\n';out+=span.text;previous=span;}return out;}
function displayPdfRegions(result,fixture,{engine}={}){
  assert(['rust','typst','xelatex'].includes(engine),'Explicit matrix engine identity required');
  assert.match(fixture.boundaryMarker,/^READMDQABIBEND[A-F0-9]{12}$/u,'Owned source END boundary required');
  assert(Array.isArray(fixture.expectedNotes),'Explicit independent note expectation required, including an empty list');
  const heading=marker(result,'References'),end=marker(result,fixture.boundaryMarker);
  assert(end.page>heading.page||end.page===heading.page&&end.y>heading.y,'END boundary precedes bibliography heading');
  const body=[],notes=[],characters=[],noteCharacters=[],requiredWhitespaceIds=[],rules=[],afterHeading=(page,y)=>page>heading.page||page===heading.page&&y>heading.y+.25;
  const beforeEnd=(page,y)=>page<end.page||page===end.page&&y<end.y-.25;
  for(const [index,page]of result.pages.entries()){
    const p=index+1,candidates=engine==='rust'||!fixture.expectedNotes.length?[]:(page.drawings||[]).filter(d=>d.width>0&&d.width<=1&&Math.abs(d.bbox[3]-d.bbox[1])<=.25&&d.bbox[2]-d.bbox[0]>=20&&d.bbox[1]>page.height*.70);
    assert(candidates.length<=1,'Ambiguous actual footnote separator');const rule=candidates[0];if(rule)rules.push({page:p,bbox:rule.bbox,width:rule.width});
    for(const span of page.spans){if(!span.text)continue;const y=span.origin[1];assert(Number.isFinite(y),'Actual text baseline missing');
      if(span.text===fixture.boundaryMarker){assert.equal(p,end.page);assert(Math.abs(y-end.y)<=.25);continue;}
      if(span.text==='References'&&p===heading.page&&Math.abs(y-heading.y)<=.25)continue;
      if(!afterHeading(p,y))continue;
      if(rule&&span.bbox[1]>rule.bbox[1]){notes.push({...span,page:p});continue;}
      if(beforeEnd(p,y)){body.push({...span,page:p});continue;}
      if(engine==='rust'&&(p>end.page||p===end.page&&y>end.y+.25)){notes.push({...span,page:p});continue;}
      assert.fail('Unclassified text after References, outside the bounded bibliography and actual note area');
    }
    assert(Array.isArray(page.characters),'Actual copy-character geometry is required');
    for(const char of page.characters){const y=char.origin[1];if(!afterHeading(p,y))continue;
      if(rule&&char.bbox[1]>rule.bbox[1]||engine==='rust'&&(p>end.page||p===end.page&&y>end.y+.25)){noteCharacters.push(char);continue;}
      if(/\S/u.test(char.c)&&beforeEnd(p,y))characters.push(char);}
    for(const char of page.spaceOutlineCharacters||[]){const y=char.origin[1];if(afterHeading(p,y)&&beforeEnd(p,y)&&!(rule&&char.bbox[1]>rule.bbox[1]))requiredWhitespaceIds.push(char.id);}
    if(rule&&p===end.page)assert(end.bbox[3]<rule.bbox[1],'END boundary overlaps the native footnote area');
  }
  if(engine!=='rust'&&fixture.expectedNotes.length)assert(rules.length,'Independent native notes have no actual physical separator');
  // Retain actual inter-word whitespace inside cells. Whitespace in a proven
  // gutter is structural; never discard a non-whitespace glyph by its text.
  for(const [index,page]of result.pages.entries())for(const char of page.characters){if(/\S/u.test(char.c))continue;const p=index+1,y=char.origin[1],rule=rules.find(r=>r.page===p);if(afterHeading(p,y)&&beforeEnd(p,y)&&!(rule&&char.bbox[1]>rule.bbox[1]))characters.push(char);}
  const flow=fixture.expectedLayout||fixture.expected.map(e=>({type:'block',indent:false,children:expectedFlow(e.node.children)}));
  const pad=engine==='rust'?6:0,left=18*TOLERANCE.mmPt+pad,right=210*TOLERANCE.mmPt-18*TOLERANCE.mmPt-pad;
  const physical=spatial.proveFlow(flow,characters,{left,right},{assertExact:assertExactSegments,fontSize:11,requiredWhitespaceIds});
  // A copy span can inherit a superscript marker's origin while its actual
  // body glyphs have another baseline. Keep real character baselines: the
  // superscript label remains separate and true wrapped lines stay separate.
  assert.equal(noteCharacters.map(c=>c.c).join(''),notes.map(s=>s.text).join(''),'Actual note span/character inventory differs');
  const bibliographyText=regionText(body),noteText=spatial.physicalText(noteCharacters),noteSegments=fixture.expectedNotes.flatMap(n=>[engine==='rust'?'[readmd-citation-'+n.number+']':String(n.number),...n.segments]);
  if(engine!=='rust'&&notes.length){const sizes=new Map();for(const span of body)sizes.set(span.size,(sizes.get(span.size)||0)+span.text.length);const dominant=[...sizes].sort((a,b)=>b[1]-a[1])[0][0];assert(notes.every(span=>span.size<=dominant*.90+.05),'Body-sized content cannot be reclassified into the physical footnote area');}
  assertExactSegments(noteText,noteSegments,fixture.id+' independent note text and labels');
  // Keep both full independent extraction streams strict. The END marker is
  // real source text and actual glyph geometry, never expected-length slicing.
  // Interleaved multi-page note streams are deliberately rejected by this
  // selected-fixture grammar; this does not certify arbitrary long notes.
  const extractionProof={};for(const [name,text]of [['MuPDF',result.text],['pypdf',result.pypdfText]]){
    const occurrences=[...text.matchAll(/References/gu)];assert.equal(occurrences.length,1,'Ambiguous '+name+' bibliography heading');
    // The actual glyph marker above is exact and unique. pypdf may insert
    // ASCII whitespace inside its kerned glyph run; only this owned boundary
    // accepts that extraction artifact, never bibliography or note text.
    const after=text.slice(occurrences[0].index+10),pattern=name==='pypdf'?[...fixture.boundaryMarker].map(escape).join('[ \t\r\n\f]*'):escape(fixture.boundaryMarker),boundary=[...after.matchAll(new RegExp(pattern,'gu'))];assert.equal(boundary.length,1,'Missing/duplicate actual '+name+' END boundary');
    extractionProof[name]=spatial.assertFlowShuffle(after.slice(0,boundary[0].index),flow);
    assertExactSegments(after.slice(boundary[0].index+boundary[0][0].length),noteSegments,fixture.id+' full '+name+' independent notes after actual END boundary');
  }
  return{bibliographyText,noteText,physical,characters,evidence:{heading:{page:heading.page,y:heading.y},end:{page:end.page,y:end.y,marker:fixture.boundaryMarker},rules,bodySpans:body.length,noteSpans:notes.length,independentNotes:fixture.expectedNotes.length,engine,physical,extractionProof}};
}
function assertMarkerOrder(first,last,label){assert(first.page<last.page||first.page===last.page&&first.y<last.y,label+' reordered across actual pages');}
function checkDisplayPdf(result,fixture,options){assert.equal(result.kind,'pdf');checkPdfBounds(result);const regions=displayPdfRegions(result,fixture,options),bibliographyText=regions.bibliographyText,evidence={textMatches:[],markers:[],regions:regions.evidence};
  // The independent pypdf stream was already checked in full with its actual
  // boundary and separately generated note oracle, without a prefix-only pass.
  if(fixture.minimumPdfPages)assert(result.pages.length>=fixture.minimumPdfPages,'Long native bibliography did not span pages');
  if(result.outlineInk){assert.equal(result.outlineInk.verified,true);assert.equal(result.outlineInk.helperSha256,'667fa0a83b29ce38b0ec2d432441ccd8a149b60e24d1d6d4045772b43b815ff9');for(const p of result.pages)for(const t of p.glyphTraces)for(const ch of t.chars)if(ch.unicode>32&&ch.unicode!==127)assert(Array.isArray(ch.inkBBox)&&ch.inkBBox.length===4&&ch.inkBBox.every(Number.isFinite),'An actual glyph has no verified embedded outline bounds');}
  function wrapped(value){const leaves=[];function visit(n){if(n.kind==='inline')leaves.push(n);else for(const child of n.children||[])visit(child);}visit(regions.physical.proof);const matches=[];for(const leaf of leaves){const chars=leaf.ids.map(id=>regions.characters.find(c=>c.id===id));try{const found=spatial.markerInCell(chars,value,{siblingLeft:leaf.bounds.right,lineHeight:fixture.line*11}),used=found.ids.map(id=>regions.characters.find(c=>c.id===id));matches.push({page:found.page,x:used[0].origin[0],y:found.firstBaseline,bbox:[Math.min(...used.map(c=>c.bbox[0])),Math.min(...used.map(c=>c.bbox[1])),Math.max(...used.map(c=>c.bbox[2])),Math.max(...used.map(c=>c.bbox[3]))],chars:used,rows:found.rows});}catch{}}
    assert.equal(matches.length,1,'Complete same-cell marker missing/ambiguous');return matches[0];}
  for(const entry of fixture.expected){if(!entry.markers)continue;const m=Object.fromEntries(Object.entries(entry.markers).filter(([k])=>k!=='label'||!fixture.longLabel).map(([key,value])=>[key,['left','leftBody','rightLabel','label'].includes(key)?wrapped(value):marker(result,value)]));evidence.markers.push(m);
    if(m.head&&m.tail)assert(m.head.page<m.tail.page||m.head.page===m.tail.page&&m.head.y<m.tail.y,'Full-width head/tail reordered');
    if(m.body&&m.end){const bodyLines=glyphLines(result).filter(l=>(l.page>m.body.page||l.page===m.body.page&&l.y>=m.body.y-.25)&&(l.page<m.end.page||l.page===m.end.page&&l.y<=m.end.y+.25)).map(l=>({...l,chars:l.chars.filter(c=>/\S/u.test(c.c)&&c.origin[0]>=m.body.x-.75)})).filter(l=>l.chars.length);
      assert(bodyLines.length>1,'The actual long body must wrap');for(const l of bodyLines){near(l.chars[0].origin[0],m.body.x,.75,'Every continuation stays in the body column');}
      for(let i=1;i<bodyLines.length;i++)if(bodyLines[i].page===bodyLines[i-1].page){const before=Math.max(...bodyLines[i-1].chars.map(c=>(c.inkBBox||c.bbox)[3])),after=Math.min(...bodyLines[i].chars.map(c=>(c.inkBBox||c.bbox)[1]));assert(before<=after+.75,'Consecutive actual body lines overlap ink');}
      m.bodyLines=bodyLines.map(l=>({page:l.page,y:l.y,x:l.chars[0].origin[0],inkRight:Math.max(...l.chars.map(c=>c.bbox[2])),glyphCount:l.chars.length}));}
    if(fixture.id==='Nested'){assert.equal(m.left.page,m.leftBody.page);near(m.left.y,m.leftBody.y);assert(m.leftBody.bbox[0]>=m.left.bbox[2]-.75,'Nested left-cell ink overlap');assert.equal(m.rightLabel.page,m.body.page);near(m.rightLabel.y,m.body.y);assert(m.body.bbox[0]>=m.rightLabel.bbox[2]-.75,'Nested body overlaps its label');}
    if(m.indent){assertMarkerOrder(m.head,m.indent,'Full-width head/indent');assertMarkerOrder(m.indent,m.deep,'Recursive indent');near(m.indent.x-m.head.x,44,.75,'4em indentation at 11pt');near(m.deep.x-m.head.x,88,.75,'Nested 8em indentation at 11pt');}
    if(fixture.longLabel){const expectedLabel=typeof entry.segments[1]==='string'?entry.segments[1]:entry.segments[1].text,columns=[];function visit(n){if(n.kind==='columns')columns.push(n);for(const child of n.children||[])visit(child);}visit(regions.physical.proof);const matches=[];for(const col of columns){const chars=col.children[0].ids.map(id=>regions.characters.find(c=>c.id===id));try{assertExactSegments(spatial.physicalText(chars),[{text:expectedLabel,characterWrap:true}],'Complete original long label in its proved physical cell');matches.push({col,chars});}catch{}}assert.equal(matches.length,1,'Complete same-cell long label missing or duplicated');const {col,chars}=matches[0],lines=spatial.lines(chars);assert(lines.length>1,'Long label must actually wrap');for(const ch of chars)assert(ch.bbox[2]<=col.leftBounds.right+.75,'Wrapped label ink exceeds its proved cell or gutter');m.labelLines=lines.map(l=>({page:l.page,y:l.y,glyphs:l.chars.length,inkRight:Math.max(...l.chars.map(c=>c.bbox[2]))}));}
  }if(evidence.markers.length>1){const origin=evidence.markers[0].body?.x;for(const m of evidence.markers)if(m.body)near(m.body.x,origin,.75,'Shared same-container body origins across entries');}return evidence;}

// Functions passed to page.evaluate are self-contained; no string eval or CSP
// bypass, and all DOMParser documents stay detached from the product page.
function inspectDocxDom(parts){const W='http://schemas.openxmlformats.org/wordprocessingml/2006/main',R='http://schemas.openxmlformats.org/officeDocument/2006/relationships',parse=s=>new DOMParser().parseFromString(s,'application/xml');
  const xml=Object.fromEntries(Object.entries(parts).filter(([k])=>/\.(?:xml|rels)$/u.test(k)).map(([k,v])=>[k,parse(v)]));for(const doc of Object.values(xml))if(doc.querySelector('parsererror'))throw Error('Actual OPC XML is malformed');
  const attrs=n=>n?Object.fromEntries([...n.attributes].map(a=>[a.localName,a.value])):{},direct=(n,name)=>[...n.children].filter(c=>c.namespaceURI===W&&c.localName===name),desc=(n,name)=>[...n.getElementsByTagNameNS(W,name)],text=n=>desc(n,'t').map(x=>x.textContent).join('');
  const doc=xml['word/document.xml'],body=desc(doc,'body')[0],rels=xml['word/_rels/document.xml.rels'];const relationships=Object.fromEntries([...rels.documentElement.children].map(n=>[n.getAttribute('Id'),attrs(n)]));
  const headerPart=ref=>{const rel=relationships[ref.getAttributeNS(R,'id')];if(!rel)throw Error('Broken running-part relationship');const part='word/'+rel.Target;if(!xml[part])throw Error('Missing running part');return{...attrs(ref),target:part,text:text(xml[part])};};
  const sections=[];let ownedText='',ownedBookmarks=[];for(const n of body.children){ownedText+=text(n);ownedBookmarks.push(...desc(n,'bookmarkStart').map(attrs).filter(b=>b.name?.startsWith('_s')));const sect=n.namespaceURI===W&&n.localName==='sectPr'?n:desc(n,'sectPr')[0];if(sect){sections.push({text:ownedText,bookmarks:ownedBookmarks,parent:sect.parentElement.localName,type:attrs(direct(sect,'type')[0]),page:attrs(direct(sect,'pgSz')[0]),columns:attrs(direct(sect,'cols')[0]),numbering:attrs(direct(sect,'pgNumType')[0]),headers:direct(sect,'headerReference').map(headerPart),footers:direct(sect,'footerReference').map(headerPart)});ownedText='';ownedBookmarks=[];}}
  const paragraphs=desc(doc,'p').map(n=>({text:text(n),spacing:attrs(direct(direct(n,'pPr')[0]||n,'spacing')[0]),indent:attrs(direct(direct(n,'pPr')[0]||n,'ind')[0]),keepNext:attrs(direct(direct(n,'pPr')[0]||n,'keepNext')[0]),keepLines:attrs(direct(direct(n,'pPr')[0]||n,'keepLines')[0])}));
  const tables=desc(doc,'tbl').map(n=>({text:text(n),grid:direct(n,'tblGrid').flatMap(g=>direct(g,'gridCol').map(attrs)),width:attrs(desc(direct(n,'tblPr')[0],'tblW')[0]),cells:direct(n,'tr').flatMap(r=>direct(r,'tc').map(c=>({text:text(c),last:c.lastElementChild?.localName,paragraphs:desc(c,'p').length}))),cantSplit:desc(n,'cantSplit').length,heights:desc(n,'trHeight').length}));
  return{sections,paragraphs,tables,bookmarks:desc(doc,'bookmarkStart').map(attrs),controls:desc(doc,'tag').map(attrs),externalLinks:Object.values(relationships).filter(r=>r.TargetMode==='External'),internalLinks:desc(doc,'hyperlink').filter(n=>n.hasAttributeNS(W,'anchor')).map(n=>({text:text(n),anchor:n.getAttributeNS(W,'anchor')})),text:text(doc)};}
function checkSectionDocx(actual,declarations){assert.equal(actual.sections.length,declarations.length);const markers=['P03FRONT','P03MAIN00','P03FULL','P03RESUME','P03WIDE','P03ODD','P03EVEN'];const cols=[1,2,1,2,1,1,1],formats=['lowerRoman','decimal','decimal','decimal','decimal','upperLetter','decimal'];
  for(const [i,s]of actual.sections.entries()){assert(s.text.includes(markers[i]));for(const [j,m]of markers.entries())if(j!==i)assert(!s.text.includes(m),'Native sectPr attributed content to a neighbouring section');assert.equal(s.type.val,declarations[i].break);assert.equal(s.parent,i+1===declarations.length?'body':'pPr');assert.equal(Number(s.columns.num||1),cols[i]);assert.equal(s.numbering.fmt,formats[i]);
    assert.equal(s.numbering.start,declarations[i].numbering?.start?.toString());near(Number(s.page.w),Math.round((i===4?297:210)*56.692913),1,'DOCX paper width twips');near(Number(s.page.h),Math.round((i===4?210:297)*56.692913),1,'DOCX paper height twips');
    if(declarations[i].header.linkPrevious){assert.equal(s.headers.length,0);}else{assert.equal(s.headers.length,1);assert.equal(s.headers[0].text,declarations[i].header.text);}
    if(declarations[i].footer.linkPrevious)assert.equal(s.footers.length,0);else assert(s.footers.length>0);
  }assert.equal(actual.bookmarks.filter(b=>b.name.startsWith('_s')).length,7);assert(actual.externalLinks.some(l=>l.Target==='https://example.invalid/p03'));
  for(const [label,section]of [['Go to main section',1],['Back to front section',0]]){const link=actual.internalLinks.find(l=>l.text===label);assert(link,'Actual native section link missing');assert.equal(actual.sections[section].bookmarks.length,1);assert.equal(link.anchor,actual.sections[section].bookmarks[0].name,'Section link points at a heading or wrong section');}return actual.sections;}
function checkDisplayDocx(actual,fixture){const heading=actual.paragraphs.findIndex(p=>p.text==='References');assert(heading>=0,'Native bibliography heading missing');assert.match(fixture.boundaryMarker,/^READMDQABIBEND[A-F0-9]{12}$/u,'Owned DOCX END boundary required');const boundaries=actual.paragraphs.flatMap((p,i)=>p.text===fixture.boundaryMarker?[i]:[]);assert.equal(boundaries.length,1,'Missing/duplicate actual DOCX END boundary');assert(boundaries[0]>heading,'DOCX END precedes bibliography');assert(actual.paragraphs.slice(boundaries[0]+1).every(p=>!p.text.trim()),'Unexpected DOCX body text after actual END boundary');const paragraphs=actual.paragraphs.slice(heading+1,boundaries[0]),text=paragraphs.map(p=>p.text).join('\n');let cursor=0;for(const e of fixture.expected){const matches=assertSegments(text.slice(cursor),e.segments,fixture.id+' DOCX original text');cursor+=matches.at(-1)?.to||0;}
  assertExactSegments(text,fixture.expected.flatMap(e=>e.segments),fixture.id+' full DOCX bibliography');
  if(fixture.minimumColumns)assert(actual.tables.length>=fixture.minimumColumns,'Native complex column tables missing');
  for(const table of actual.tables){assert.equal(table.grid.length,2);const width=Number(table.width.w);assert.equal(table.grid.reduce((n,c)=>n+Number(c.w),0),width);assert(Number(table.grid[0].w)<=Math.floor(width*.35));assert.equal(table.cantSplit,0);assert.equal(table.heights,0);assert(table.cells.every(c=>c.last==='p'));}
  if(fixture.minimumBlocks){assert(paragraphs.filter(p=>p.text).length>=fixture.minimumBlocks);assert(actual.controls.some(c=>c.val.startsWith('readmd-csl-entry:')));}
  for(const p of paragraphs.filter(p=>p.text)){if(p.keepNext.val)assert.equal(p.keepNext.val,'0');if(p.keepLines.val)assert.equal(p.keepLines.val,'0');}
  return{tables:actual.tables.length,paragraphs:paragraphs.length};}
function inspectReaderDom(selector){const host=document.querySelector(selector);if(!host)throw Error('Actual reader container missing');const sections=[...host.querySelectorAll('.readmd-bibliography')];
  const rect=n=>{const r=n.getBoundingClientRect();return{x:r.x,y:r.y,width:r.width,height:r.height,right:r.right,bottom:r.bottom};};return{source:state.editing?getEditContent():state.original,text:host.innerText,overflow:host.scrollWidth>host.clientWidth+1,viewportOverflow:document.documentElement.scrollWidth>innerWidth+1,
    sections:sections.map(s=>({ready:s.dataset.cslMeasurement,preferred:getComputedStyle(s).getPropertyValue('--readmd-csl-label-preferred-width').trim(),complex:s.classList.contains('readmd-bibliography-complex'),entries:[...s.querySelectorAll('.readmd-bibliography-entry')].map(n=>({text:n.textContent,margin:parseFloat(getComputedStyle(n).marginBottom),font:parseFloat(getComputedStyle(n).fontSize)})),
      columns:[...s.querySelectorAll('.readmd-csl-layout-columns')].map(n=>{const l=n.querySelector(':scope>.readmd-csl-layout-left'),r=n.querySelector(':scope>.readmd-csl-layout-right'),style=getComputedStyle(n),font=getComputedStyle(l),nested=Boolean(l.querySelector('.readmd-csl-layout-columns')),canvas=document.createElement('canvas'),ctx=canvas.getContext('2d');ctx.font=font.font||`${font.fontStyle} ${font.fontWeight} ${font.fontSize} ${font.fontFamily}`;const intrinsic=nested?null:Math.max(...l.textContent.split('\n').map(text=>ctx.measureText(text).width+Math.max(0,[...text].length-1)*(parseFloat(font.letterSpacing)||0)));return{box:rect(n),left:rect(l),right:rect(r),gap:parseFloat(style.columnGap),font:parseFloat(style.fontSize),leftText:l.textContent,intrinsic,nested,leftOverflow:l.scrollWidth>l.clientWidth+1,rightOverflow:r.scrollWidth>r.clientWidth+1};}),
      indents:[...s.querySelectorAll('.readmd-csl-layout-indent')].map(n=>({text:n.textContent,padding:parseFloat(getComputedStyle(n).paddingInlineStart),font:parseFloat(getComputedStyle(n).fontSize)}))}))};}
function checkReader(actual,fixture){assert(!actual.overflow&&!actual.viewportOverflow,'Actual reader horizontally clips');assert.equal(actual.sections.length,1);const s=actual.sections[0];assert.equal(s.entries.length,fixture.expected.length);
  for(const [i,e]of s.entries.entries()){assertExactSegments(e.text,fixture.expected[i].segments,fixture.id+' reader entry');near(e.margin,i+1<s.entries.length?fixture.line*fixture.entry*e.font:0,1,'CSL complete-entry margin');}
  if(s.complex){assert.equal(s.ready,'ready');assert.match(s.preferred,/^\d+(?:\.\d+)?px$/u);const preferred=parseFloat(s.preferred);assert(preferred>=0);if(s.columns.length)assert(preferred>0);for(const c of s.columns){near(c.left.width,Math.min(preferred,c.box.width*.35),1,'Numeric measured preferred/actual label width');near(c.gap,.75*c.font,1,'Actual 0.75em gutter');assert(c.left.right<=c.right.x+1&&!c.leftOverflow&&!c.rightOverflow,'Actual label/body overlap');if(c.intrinsic!==null)assert(preferred>=c.intrinsic-1,'Numeric preferred width is smaller than independently measured label text');}
    if(s.columns.length&&s.columns.every(c=>!c.nested))near(preferred,Math.max(...s.columns.map(c=>c.intrinsic)),1,'Preferred width equals independent canvas metric for flat labels');}
  if(fixture.minimumColumns)assert(s.columns.length>=fixture.minimumColumns);for(const i of s.indents)near(i.padding,4*i.font,1,'Actual 4em indent');return actual;}
module.exports={assertMarkerOrder,TOLERANCE,plain,compact,near,textPattern,assertSegments,assertExactSegments,parseExpectedHTML,canonicalRich,expectedRich,expectedFlow,actualFlow,projectedFlow,checkBibliography,glyphLines,marker,checkPdfBounds,checkSectionPdf,displayPdfRegions,checkDisplayPdf,inspectDocxDom,checkSectionDocx,checkDisplayDocx,inspectReaderDom,checkReader};
