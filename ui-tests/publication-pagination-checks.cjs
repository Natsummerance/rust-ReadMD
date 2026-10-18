'use strict';
// Independent actual-page checks: toggles must change pagination, rather than
// merely appear in generated source or saved presets. Small pages force breaks.
const fs=require('node:fs'),path=require('node:path'),assert=require('node:assert/strict');
module.exports=async({api,docs,boxes})=>{
  const results=[];
  // The pinned managed sans font advances these 10pt hard-break lines by
  // 7.33pt. 29 fillers put exactly one body line at the page boundary;
  // older fixtures used the former host-font metrics (28 fillers).
  for(const[kind,count,withHeading]of [['heading',29,true],['paragraph',27,true],['widow',29,false]])for(const enabled of [false,true]){
    const content=Array(count).fill('FILLER').join('  \n')+(withHeading?'\n\n## HEADING_SENTINEL':'')+'\n\n'+Array.from({length:5},(_,i)=>'BODY_LINE_'+(i+1)).join('  \n');
    const options={pdf:{engine:'typst'},page:{size:'Custom',width:80,height:100,marginTop:10,marginRight:10,marginBottom:10,marginLeft:10},cover:{enabled:false},toc:{enabled:false},header:{text:''},footer:{text:'',pageNumbers:false},typography:{size:10,lineHeight:1,spacing:0,widowControl:kind==='widow'&&enabled,keepLines:kind==='paragraph'&&enabled},headings:{h2:{size:12,before:0,after:0,keepNext:kind==='heading'&&enabled,keepLines:true}}};
    const output=path.join(docs,`Pagination-${kind}-${enabled}.pdf`),result=await api('/api/export',{format:'pdf',content,baseDir:docs,out_path:output,options});assert.equal(result.ok,true,JSON.stringify(result));
    const pages=boxes(output,`pagination-${kind}-${enabled}`);assert.equal(pages.length,2);
    const heading=pages.findIndex(p=>p.some(w=>w.text==='HEADING_SENTINEL')),body=Array.from({length:5},(_,i)=>pages.findIndex(p=>p.some(w=>w.text==='BODY_LINE_'+(i+1))));assert.ok(body.every(p=>p>=0));
    results.push({kind,enabled,pageCount:pages.length,headingPage:heading<0?null:heading+1,bodyPages:body.map(p=>p+1),bytes:fs.statSync(output).size});
  }
  const pair=kind=>results.filter(r=>r.kind===kind),[headingOff,headingOn]=pair('heading'),[paragraphOff,paragraphOn]=pair('paragraph'),[widowOff,widowOn]=pair('widow');
  assert.equal(headingOff.headingPage,1);assert.equal(headingOff.bodyPages[0],2);assert.equal(headingOn.headingPage,2);assert.equal(headingOn.bodyPages[0],2);
  assert.equal(paragraphOff.bodyPages[0],1);assert.equal(paragraphOff.bodyPages.at(-1),2);assert.ok(paragraphOn.bodyPages.every(p=>p===2),'Keep-lines moves the whole paragraph');
  assert.equal(widowOff.bodyPages.filter(p=>p===1).length,1,'Disabled widow/orphan control allows one leading line');assert.ok(widowOn.bodyPages.every(p=>p===2),'Enabled control prevents the single leading line');
  return results;
};
