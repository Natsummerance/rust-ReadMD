'use strict';
// Owned files exercise actual DOCX relationships; no Office/Python fixture tool.
const fs=require('node:fs'),path=require('node:path');
module.exports=async function(docs){
 const{zip}=await import('../tools/lib/zip.mjs'),m='http://schemas.openxmlformats.org/officeDocument/2006/math',w='http://schemas.openxmlformats.org/wordprocessingml/2006/main',a='http://schemas.openxmlformats.org/drawingml/2006/main';
 const run=(text,props='',normal=true)=>`<m:r>${normal?'<m:rPr><m:nor/></m:rPr>':''}${props?'<w:rPr>'+props+'</w:rPr>':''}<m:t>${text}</m:t></m:r>`;
 const themefont=(kind,latin,ea)=>`<a:${kind}Font><a:latin typeface="${latin}"/><a:ea typeface=""/><a:cs typeface="STIX Two Math"/><a:font script="Hans" typeface="${ea}"/><a:font script="Jpan" typeface="ReadMD Serif SC"/></a:${kind}Font>`;
 const theme=`<a:theme xmlns:a="${a}" name="Owned"><a:themeElements><a:clrScheme name="Owned"><a:dk1><a:srgbClr val="000000"/></a:dk1></a:clrScheme><a:fontScheme name="Owned Fonts">${themefont('major','Arial','ReadMD Sans SC')}${themefont('minor','Georgia','ReadMD Serif SC')}</a:fontScheme></a:themeElements></a:theme>`;
 const serif='<w:rFonts w:asciiTheme="minorHAnsi" w:eastAsiaTheme="minorEastAsia"/>',sans='<w:rFonts w:asciiTheme="majorAscii" w:eastAsiaTheme="majorEastAsia"/>';
 const equation=`<m:oMathPara><m:oMath>${run('Sans 研究 ',sans)}${run('AVMW ',sans+'<w:b/>')}${run('+','',false)}<m:f><m:num>${run('AVMW',serif)}</m:num><m:den>${run('Serif 论文',serif)}</m:den></m:f>${run(' italic ',serif+'<w:i/>')}${run('+α','',false)}</m:oMath></m:oMathPara>`;
 for(const [name,lang]of [['English','en-US'],['Japanese','ja-JP']]){
  const entries=[
   {name:'word/document.xml',data:`<w:document xmlns:w="${w}" xmlns:m="${m}"><w:body><w:p>${equation}</w:p></w:body></w:document>`},
   {name:'word/settings.xml',data:`<w:settings xmlns:w="${w}" xmlns:m="${m}"><w:themeFontLang w:val="${lang}" w:eastAsia="zh-CN"/><m:mathPr><m:mathFont m:val="STIX Two Math"/></m:mathPr></w:settings>`},
   {name:'word/styles.xml',data:`<w:styles xmlns:w="${w}"><w:docDefaults><w:rPrDefault><w:rPr><w:sz w:val="32"/></w:rPr></w:rPrDefault></w:docDefaults></w:styles>`},
   {name:'word/theme/fonts.xml',data:theme},
   {name:'word/_rels/document.xml.rels',data:`<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">${[['theme','theme/fonts.xml'],['styles','styles.xml'],['settings','settings.xml']].map(([type,target])=>`<Relationship Id="${type}" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/${type}" Target="${target}"/>`).join('')}</Relationships>`},
   {name:'_rels/.rels',data:'<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="document" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>'},
   {name:'[Content_Types].xml',data:`<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>${[['document','application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml'],['styles','application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml'],['settings','application/vnd.openxmlformats-officedocument.wordprocessingml.settings+xml'],['theme/fonts','application/vnd.openxmlformats-officedocument.theme+xml']].map(([n,type])=>`<Override PartName="/word/${n}.xml" ContentType="${type}"/>`).join('')}</Types>`}
  ];fs.writeFileSync(path.join(docs,name+'.docx'),zip(entries));
 }
};
