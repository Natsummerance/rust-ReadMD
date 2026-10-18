'use strict';
// Independent physical partition proof. No expected-length slicing: candidate
// divisions are actual baseline gaps / actual >=0.75em column gutters, and
// every visible character must belong to exactly one validated leaf.
const assert=require('node:assert/strict');
const nonblank=c=>/\S/u.test(c.c),pos=c=>[c.page,c.origin[1]];
function lines(chars){const out=[],buckets=new Map();for(const c of chars){const bucket=Math.floor(c.origin[1]*4);let row=out.at(-1);if(!row||row.page!==c.page||Math.abs(row.y-c.origin[1])>.25){row=null;for(const offset of[-1,0,1]){const candidates=buckets.get(c.page+':'+(bucket+offset))||[];row=candidates.find(r=>Math.abs(r.y-c.origin[1])<=.25);if(row)break;}}
  if(!row){row={page:c.page,y:c.origin[1],chars:[]};out.push(row);const key=c.page+':'+bucket;if(!buckets.has(key))buckets.set(key,[]);buckets.get(key).push(row);}row.chars.push(c);}for(const row of out)row.chars.sort((a,b)=>a.origin[0]-b.origin[0]||a.id-b.id);return out.sort((a,b)=>a.page-b.page||a.y-b.y);}
const physicalText=chars=>lines(chars).map(row=>row.chars.map(c=>c.c).join('')).join('\n');
const expectedText=node=>node.type==='inline'?node.runs.map(r=>r.text).join(''):node.type==='columns'?[...node.left,...node.right].map(expectedText).join(''):node.children.map(expectedText).join('');
const extent=chars=>({left:Math.min(...chars.filter(nonblank).map(c=>c.bbox[0])),right:Math.max(...chars.filter(nonblank).map(c=>c.bbox[2]))});
function proveFlow(flow,characters,bounds,{assertExact,fontSize=11,maxAttempts=20000,requiredWhitespaceIds=[]}={}){
  assert.equal(typeof assertExact,'function');assert(bounds.right>bounds.left);assert(Number.isFinite(fontSize)&&fontSize>0);let attempts=0;
  const chars=characters.filter(c=>c.c&&!/[\r\n]/u.test(c.c));assert(chars.every(c=>Number.isInteger(c.id)&&Number.isInteger(c.page)&&c.origin.every(Number.isFinite)&&c.bbox.every(Number.isFinite)));
  assert.equal(new Set(chars.map(c=>c.id)).size,chars.length,'Actual character identity duplicated');
  const actualWhitespace=new Set(chars.filter(c=>!nonblank(c)).map(c=>c.id));
  assert(requiredWhitespaceIds.every(id=>actualWhitespace.has(id)),'Actual trace whitespace identity omitted');
  const checkBudget=()=>assert(++attempts<=maxAttempts,'Bounded physical partition proof exhausted; no acceptance');
  function firstOffset(nodes){let offset=0,n=nodes[0];while(n?.type==='block'){if(n.indent)offset+=4*fontSize;n=n.children[0];}return offset;}
  function sequence(nodes,input,box,characterWrap=false){
    if(!nodes.length){assert(!input.some(nonblank),'Unconsumed actual text');return{kind:'empty',ids:[],whitespaceIds:input.map(c=>c.id),children:[]};}
    if(nodes.length===1)return node(nodes[0],input,box,characterWrap);
    const rows=lines(input),matches=[],n=[...expectedText(nodes[0]).replace(/\s/gu,'')].length;
    for(let end=1;end<=rows.length;end++){
      const first=rows.slice(0,end).flatMap(r=>r.chars),count=first.filter(nonblank).reduce((sum,c)=>sum+[...c.c].length,0);if(count<n)continue;if(count>n+end)break;
      try{const left=node(nodes[0],first,box,characterWrap),right=sequence(nodes.slice(1),rows.slice(end).flatMap(r=>r.chars),box,characterWrap);matches.push({kind:'sequence',ids:[...left.ids,...right.ids],whitespaceIds:[...left.whitespaceIds,...right.whitespaceIds],children:[left,right]});}catch(error){if(error.message.includes('exhausted'))throw error;}
    }
    assert.equal(matches.length,1,'Missing or ambiguous physical vertical partition');return matches[0];
  }
  function node(expected,input,box,characterWrap){checkBudget();
    if(expected.type==='inline'){
      const original=expected.runs.map(r=>r.text).join('');
      // HTML-collapsible whitespace at a proved column-cell boundary is
      // structural. Preserve NBSP and every original interior word space.
      const text=characterWrap?original.replace(/^[ \t\r\n\f]+|[ \t\r\n\f]+$/gu,''):original;
      assertExact(physicalText(input),[{text,characterWrap}],'Actual physical cell original sequence');
      const actualRows=lines(input);
      function unpaintedTail(c){
        const p=c.emptySpaceOutline,row=actualRows.find(r=>r.chars.some(q=>q.id===c.id));
        return c.c===' '&&p?.verified===true&&p.helperSha256==='667fa0a83b29ce38b0ec2d432441ccd8a149b60e24d1d6d4045772b43b815ff9'
          &&Number.isInteger(p.fontXref)&&p.fontXref>0&&Number.isInteger(p.glyphId)&&p.glyphId>0
          &&/^[a-f0-9]{64}$/u.test(p.fontSha256)&&/^[a-f0-9]{64}$/u.test(p.outlineSha256)
          &&Number.isFinite(p.advanceEm)&&p.advanceEm>0&&p.advanceEm<=4&&Number.isFinite(p.size)&&p.size>0
          &&p.origin.every((value,i)=>Math.abs(value-c.origin[i])<=.0001)&&Math.abs(p.size-c.size)<=.0001
          &&row?.chars.at(-1).id===c.id&&c.origin[0]>=box.left-.75&&c.origin[0]<=box.right+.75
          &&c.bbox[0]>=box.left-.75&&c.bbox[2]>=c.origin[0]
          &&Math.abs(c.bbox[2]-c.origin[0]-p.advanceEm*p.size)<=.75;
      }
      assert(input.filter(c=>!nonblank(c)).every(c=>c.bbox[0]>=box.left-.75&&(c.bbox[2]<=box.right+.75||unpaintedTail(c))),'Actual whitespace lies outside its proved cell');
      const ink=extent(input);assert(ink.left>=box.left-.75&&ink.right<=box.right+.75,'Actual cell ink exceeds its physical parent');
      return{kind:'inline',ids:input.filter(nonblank).map(c=>c.id),whitespaceIds:input.filter(c=>!nonblank(c)).map(c=>c.id),bounds:box,rows:actualRows.map(r=>({page:r.page,baseline:r.y,ink:extent(r.chars)})),textCharacters:text.length};
    }
    if(expected.type==='block'){
      const inner={...box,left:box.left+(expected.indent?4*fontSize:0)};assert(inner.right>inner.left,'Declared indent consumes its parent');
      if(input.some(nonblank)){const first=lines(input)[0].chars.find(nonblank);assert(Math.abs(first.origin[0]-inner.left-firstOffset(expected.children))<=.75,'Actual block/indent first origin changed');}
      const proof=sequence(expected.children,input,inner,characterWrap);return{kind:'block',indent:expected.indent,ids:proof.ids,whitespaceIds:proof.whitespaceIds,children:[proof]};
    }
    assert.equal(expected.type,'columns');assert(expected.left.length&&expected.right.length,'Empty cell needs an explicit independent contract');
    const top=lines(input)[0];assert(top,'Actual column row is empty');const visible=top.chars.filter(nonblank),candidates=[];
    for(let i=1;i<visible.length;i++){
      const a=visible[i-1],b=visible[i];if(b.bbox[0]-a.bbox[2]<.75*fontSize-.75)continue;const cut=(a.bbox[2]+b.bbox[0])/2;
      let left=input.filter(c=>nonblank(c)?c.origin[0]<cut:c.bbox[2]<=cut),right=input.filter(c=>nonblank(c)?c.origin[0]>=cut:c.bbox[0]>=cut);
      if(!left.some(nonblank)||!right.some(nonblank))continue;
      // A real copy-space or a later unindented continuation can expose the
      // cell origin that the first non-space glyph does not paint.
      const rightOrigin=Math.min(...right.map(c=>c.origin[0]));
      const allocation=rightOrigin-box.left-.75*fontSize;if(allocation<=0||allocation>(box.right-box.left)*.35+.75)continue;
      const leftBox={left:box.left,right:rightOrigin-.75*fontSize},rightBox={left:rightOrigin,right:box.right};
      try{
        const leftInk=extent(left).right;
        const structural=input.filter(c=>!nonblank(c)&&c.bbox[0]>=leftInk-.75&&c.bbox[2]<=rightOrigin+.75);
        const structuralIds=new Set(structural.map(c=>c.id));
        left=left.filter(c=>!structuralIds.has(c.id));right=right.filter(c=>!structuralIds.has(c.id));
        const assigned=new Set([...left,...right,...structural].map(c=>c.id));
        assert(input.every(c=>assigned.has(c.id)),'Actual whitespace crosses an unproved column boundary');
        const l=sequence(expected.left,left,leftBox,true),r=sequence(expected.right,right,rightBox,true);
        const ids=[...l.ids,...r.ids],key=[...l.ids].sort((x,y)=>x-y).join(',')+'|'+[...r.ids].sort((x,y)=>x-y).join(',');
        if(!candidates.some(c=>c.key===key))candidates.push({key,kind:'columns',ids,whitespaceIds:[...l.whitespaceIds,...r.whitespaceIds,...structural.map(c=>c.id)],cut,rightOrigin,leftAllocation:allocation,parentWidth:box.right-box.left,leftBounds:leftBox,rightBounds:rightBox,structuralWhitespaceIds:structural.map(c=>c.id),children:[l,r]});
      }catch(error){if(error.message.includes('exhausted'))throw error;}
    }
    assert.equal(candidates.length,1,'Missing or ambiguous actual >=0.75em column partition');const proof=candidates[0];delete proof.key;return proof;
  }
  const proof=sequence(flow,chars,bounds),actual=chars.filter(nonblank).map(c=>c.id).sort((a,b)=>a-b),assigned=proof.ids.toSorted((a,b)=>a-b);
  assert.deepEqual(assigned,actual,'Visible actual character omitted, duplicated or stolen by another cell');
  assert.deepEqual(proof.whitespaceIds.toSorted((a,b)=>a-b),chars.filter(c=>!nonblank(c)).map(c=>c.id).toSorted((a,b)=>a-b),'Actual whitespace omitted, duplicated or stolen by another cell');
  return{passed:true,attempts,visibleCharacters:actual.length,structuralWhitespace:chars.length-actual.length,proof};
}
// Marker verification uses the already independently proven cell assignment.
// Its complete characters may wrap over adjacent physical baselines; no search
// into a sibling or another body cell is permitted to complete the marker.
function markerInCell(characters,word,{siblingLeft=Infinity,lineHeight=11}={}){
  assert(word&&!/\s/u.test(word));const rows=lines(characters),matches=[];
  for(let start=0;start<rows.length;start++){
    let combined='';const used=[];for(let end=start;end<rows.length;end++){
      if(end>start&&(rows[end].page!==rows[end-1].page||rows[end].y-rows[end-1].y<=.25||rows[end].y-rows[end-1].y>lineHeight+.75))break;
      const chars=rows[end].chars.filter(nonblank);combined+=chars.map(c=>c.c).join('');used.push(...chars);
      if(combined===word){assert(used.every(c=>c.bbox[2]<=siblingLeft+.75),'Wrapped marker crosses sibling ink');matches.push({firstBaseline:rows[start].y,page:rows[start].page,rows:end-start+1,ids:used.map(c=>c.id)});break;}
      if(!word.startsWith(combined))break;
    }
  }
  assert.equal(matches.length,1,'Complete same-cell wrapped marker missing or ambiguous');return matches[0];
}
function assertFlowShuffle(actual,flow,{maxStates=4096,maxTransitions=2000000}={}){
  // A PDF extractor may interleave declared column cells. Preserve sequential
  // order inside each cell/block while allowing ONLY declared columns to run
  // in parallel. Every non-whitespace code point is consumed exactly once;
  // physical word spaces and unique per-cell order are checked by proveFlow.
  const pool=[];const put=node=>{pool.push(node);return pool.length-1;};
  function seq(nodes){const ids=nodes.map(compile).filter(x=>x!==null);return ids.length===0?null:ids.length===1?ids[0]:put({k:'s',ids});}
  function compile(node){if(node.type==='inline'){const value=[...node.runs.map(r=>r.text).join('').replace(/\s/gu,'')];return value.length?put({k:'t',value}):null;}
    if(node.type==='block')return seq(node.children);assert.equal(node.type,'columns');const left=seq(node.left),right=seq(node.right);return left===null?right:right===null?left:put({k:'p',left,right});}
  function init(id){if(id===null)return null;const n=pool[id];return n.k==='t'?['t',id,0]:n.k==='s'?['s',id,0,init(n.ids[0])]:['p',id,init(n.left),init(n.right)];}
  let transitions=0;
  function step(state,ch){assert(++transitions<=maxTransitions,'Bounded independent extraction shuffle exhausted');if(state===null)return[];const[k,id,a,b]=state,n=pool[id];
    if(k==='t')return n.value[a]!==ch?[]:[a+1===n.value.length?null:['t',id,a+1]];
    if(k==='s')return step(b,ch).map(next=>next!==null?['s',id,a,next]:a+1===n.ids.length?null:['s',id,a+1,init(n.ids[a+1])]);
    return[...step(a,ch).map(next=>next===null?b:['p',id,next,b]),...step(b,ch).map(next=>next===null?a:['p',id,a,next])];
  }
  const tokens=[];for(const match of actual.matchAll(/\S/gu)){const at=match.index;tokens.push({c:match[0],optionalHyphen:match[0]==='-'&&/\p{L}$/u.test(actual.slice(0,at))&&/^\r?\n\p{L}/u.test(actual.slice(at+1))});}
  const rootId=seq(flow);let states=new Map([[JSON.stringify(init(rootId)),init(rootId)]]);
  for(const token of tokens){const next=new Map();for(const state of states.values()){for(const candidate of step(state,token.c))next.set(JSON.stringify(candidate),candidate);if(token.optionalHyphen)next.set(JSON.stringify(state),state);}assert(next.size,'Independent extraction changes, omits, duplicates or reorders a cell code point');assert(next.size<=maxStates,'Ambiguous independent extraction exceeds bounded states');states=next;}
  assert(states.has('null'),'Independent extraction leaves expected cell text unconsumed');return{passed:true,codePoints:tokens.length,transitions};
}
module.exports={lines,physicalText,proveFlow,markerInCell,assertFlowShuffle};
