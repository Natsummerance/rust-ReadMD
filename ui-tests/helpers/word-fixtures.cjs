'use strict';
// XML content and relationship locations match the original owned fixtures.
const fs=require('node:fs'),path=require('node:path'),{zip}=require('../zip-fixtures.cjs');
const m='http://schemas.openxmlformats.org/officeDocument/2006/math',w='http://schemas.openxmlformats.org/wordprocessingml/2006/main';
const rel=(kind,target)=>`<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="${kind}" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/${kind}" Target="${target}"/></Relationships>`;
function write(dir,name,document,config,kind='settings',target='config/mathematics.xml'){
 fs.writeFileSync(path.join(dir,name+'.docx'),zip([['word/document.xml',document],['word/'+target,config],['word/_rels/document.xml.rels',rel(kind,target)]]));
}
function defaults(dir){
 for(const[name,def,explicit]of [['Side','subSup','undOvr'],['Above','undOvr','subSup']]){
  const equation=local=>'<m:oMath><m:nary><m:naryPr><m:chr m:val="∑"/>'+local+'</m:naryPr><m:sub><m:r><m:t>0</m:t></m:r></m:sub><m:sup><m:r><m:t>1</m:t></m:r></m:sup><m:e><m:r><m:t>x</m:t></m:r></m:e></m:nary></m:oMath>';
  const document=`<w:document xmlns:w="${w}" xmlns:m="${m}"><w:body><w:p>`+equation('')+'</w:p><w:p>'+equation(`<m:limLoc m:val="${explicit}"/>`)+'</w:p></w:body></w:document>';
  write(dir,name,document,`<v:settings xmlns:v="${w}" xmlns:q="${m}"><q:mathPr><q:intLim q:val="undOvr"/><q:naryLim q:val="${def}"/></q:mathPr></v:settings>`);
 }
}
function paragraphs(dir){
 const run=(text,properties='')=>`<m:r>${properties}<m:t>${text}</m:t></m:r>`,prefix=['a','+','b','=','c'].map(t=>run(t)).join('');
 for(const name of ['left','centerGroup','center','right','manual-0','manual-1','manual-2']){
  let equation,align;
  if(name.startsWith('manual-')){const index=Number(name.at(-1)),brk=index===0?'<m:brk/>':`<m:brk m:alnAt="${index}"/>`;equation='<m:oMath>'+prefix+run('=',`<m:rPr>${brk}</m:rPr>`)+run('d')+'</m:oMath>';align='left';}
  else{equation='<m:oMath>'+prefix+'</m:oMath><m:oMath>'+run('x')+'</m:oMath>';align=name;}
  const document=`<w:document xmlns:w="${w}" xmlns:m="${m}"><w:body><w:p><m:oMathPara><m:oMathParaPr><m:jc m:val="${align}"/></m:oMathParaPr>${equation}</m:oMathPara></w:p></w:body></w:document>`;
  write(dir,name,document,`<v:settings xmlns:v="${w}" xmlns:q="${m}"><q:mathPr><q:defJc q:val="right"/><q:preSp q:val="120"/><q:postSp q:val="240"/><q:interSp q:val="180"/><q:intraSp q:val="120"/></q:mathPr></v:settings>`);
 }
}
function smallFractions(dir){
 for(const[name,prop]of [['Default',''],['Small','<q:smallFrac/>'],['Normal','<q:smallFrac q:val="0"/>']]){
  const document=`<w:document xmlns:w="${w}" xmlns:m="${m}"><w:body><w:p><m:oMathPara><m:oMath><m:f><m:num><m:r><m:t>a</m:t></m:r></m:num><m:den><m:r><m:t>b</m:t></m:r></m:den></m:f></m:oMath></m:oMathPara></w:p></w:body></w:document>`;
  write(dir,name,document,`<v:settings xmlns:v="${w}" xmlns:q="${m}"><q:mathPr>${prop}</q:mathPr></v:settings>`);
 }
}
function styles(dir){
 const equation='<m:oMathPara><m:oMath><m:r><m:t>x</m:t></m:r><m:r><m:t>+</m:t></m:r><m:f><m:num><m:r><m:t>α</m:t></m:r></m:num><m:den><m:r><m:t>β</m:t></m:r></m:den></m:f><m:r><m:rPr><m:nor/></m:rPr><w:rPr><w:rStyle w:val="Accent"/></w:rPr><m:t>note</m:t></m:r></m:oMath></m:oMathPara>';
 for(const name of ['Styled','Plain','Cyclic']){
  const document=`<w:document xmlns:w="${w}" xmlns:m="${m}"><w:body><w:p><w:pPr><w:pStyle w:val="Equation"/><w:rPr><w:sz w:val="90"/></w:rPr></w:pPr>${equation}</w:p></w:body></w:document>`;
  const prop=name==='Styled'?'<v:b/><v:color v:val="336699"/><v:sz v:val="32"/>':'<v:sz v:val="24"/>',base=name==='Cyclic'?'Equation':'Normal';
  const config=`<v:styles xmlns:v="${w}"><v:docDefaults><v:rPrDefault><v:rPr><v:sz v:val="24"/></v:rPr></v:rPrDefault></v:docDefaults><v:style v:type="paragraph" v:styleId="Normal" v:default="1"><v:rPr><v:b v:val="0"/></v:rPr></v:style><v:style v:type="paragraph" v:styleId="Equation"><v:basedOn v:val="${base}"/><v:rPr>${prop}</v:rPr></v:style><v:style v:type="character" v:styleId="Accent"><v:rPr><v:i/></v:rPr></v:style><v:style v:type="character" v:styleId="Unused"><v:rPr><v:sz v:val="120"/></v:rPr></v:style></v:styles>`;
  write(dir,name,document,config,'styles','config/academic.xml');
 }
}
module.exports={defaults,paragraphs,smallFractions,styles};
