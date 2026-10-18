'use strict';
// Actual PDF checks for numbering formats and physical-page parity, including
// the case where the first printed number is even. No mocked engine output.
const fs=require('node:fs'),path=require('node:path'),assert=require('node:assert/strict');
module.exports=async({api,docs,options,boxes})=>{
  const reports=[];
  const content='# Page one\n\nPARITY_BODY_MARKER\n\n<!-- pagebreak -->\n\n# Page two\n\nPARITY_BODY_MARKER\n\n<!-- pagebreak -->\n\n# Page three\n\nPARITY_BODY_MARKER\n';
  for(const[format,expected]of [['decimal','8'],['lowerRoman','viii'],['upperRoman','VIII'],['lowerLetter','h'],['upperLetter','H']]){
    const output=path.join(docs,`Numbering-${format}.pdf`),style={...options,page:{...options.page,columns:1,differentFirstPage:false},cover:{enabled:false},toc:{enabled:false},footer:{...options.footer,start:8,numberFormat:format},footnotes:{...options.footnotes,numberFormat:format}};
    const result=await api('/api/export',{format:'pdf',content,baseDir:docs,out_path:output,options:style});assert.equal(result.ok,true,JSON.stringify(result));
    const pages=boxes(output,'numbering-'+format);assert.equal(pages.length,3);assert.ok(pages[0].some(b=>b.text===expected&&b.y>780));
    assert.ok(pages[0].some(b=>b.text==='3'&&b.y>780),'Total is physical page count');
    const positions=pages.map(p=>p.find(b=>b.text==='PARITY_BODY_MARKER'));
    for(let i=0;i<3;i++)assert.ok(Math.abs(positions[i].x-(i%2===0?26:18)*72/25.4)<2,'Mirrored body uses physical parity, independent of displayed number');
    reports.push({format,firstNumber:expected,pages:3,bodyPositions:positions.map(b=>({x:b.x,y:b.y})),bytes:fs.statSync(output).size});
  }
  return reports;
};
