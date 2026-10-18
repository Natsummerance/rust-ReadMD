'use strict';
// Independently authored from the unchanged three synthetic CSL templates.
// citeproc-js is still used for original text/marks, not as an oracle for the
// explicitly requested nested-display extension which that runtime flattens.
const inline=(text,marks={},href=null)=>({type:'inline',runs:[{text,marks,href}]}),block=(children,indent=false)=>({type:'block',indent,children}),columns=(left,right)=>({type:'columns',left,right});
function entries(fixture){return fixture.refs.map((r,i)=>{
  const n=r.volume,title=inline(r.title,fixture.id==='Nested'?{font_style:'italic'}:{});
  if(fixture.id==='Nested')return block([block([inline('NHEAD'+n)]),columns([columns([inline('NLEFT'+n)],[inline('LEFTBODY'+n)])],[columns([inline('NRIGHT'+n)],[title])]),block([inline('NTAIL'+n)])]);
  if(fixture.id==='Multipair')return block([block([inline('MHEAD'+n)]),columns([inline('LONGUNSPACEDLABEL'.repeat(8)+n)],[block([title]),block([inline('SECONDROW'+n)])]),block([inline('MINDENT'+n)],true),block([block([inline('MDEEP'+n)],true)],true),columns([inline('MSECOND'+n)],[inline(r.URL,{font_style:'italic'},r.URL)]),block([inline('MTAIL'+n)])]);
  if(fixture.id==='Long')return block([block([inline('LHEAD'+n)]),columns([inline('['+n+']')],fixture.longParagraphs.map((value,j)=>block([inline(value,j%2?{font_style:'italic'}:{font_weight:'bold'})]))),block([inline('LTAIL'+n)])]);
  throw Error('No independent original recursive layout for this fixture');
});}
module.exports={entries};
