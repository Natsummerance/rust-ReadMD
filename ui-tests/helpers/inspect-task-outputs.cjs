'use strict';
// Independent ZIP integrity and browser XML inspection of actual engine output.
const fs=require('node:fs'),path=require('node:path'),assert=require('node:assert/strict');
const {chromium}=require('playwright'),zipParts=require('../zip-parts.cjs');
module.exports=async function inspectTaskOutputs(docs){
 const results={},inputs=['Replacement.docx','Recovered.docx'].map(name=>({name,xml:zipParts(fs.readFileSync(path.join(docs,name))).get('word/document.xml')}));
 assert(inputs.every(input=>typeof input.xml==='string'),'Actual DOCX main parts exist');
 const browser=await chromium.launch({channel:'msedge',headless:true});
 try{
  const page=await browser.newPage();
  for(const input of inputs){
   const inspected=await page.evaluate(xml=>{const document=new DOMParser().parseFromString(xml,'application/xml');if(document.querySelector('parsererror'))throw Error('Malformed actual DOCX XML');return{texts:Array.from(document.getElementsByTagNameNS('http://schemas.openxmlformats.org/wordprocessingml/2006/main','t'),e=>e.textContent).join(''),fractions:document.getElementsByTagNameNS('http://schemas.openxmlformats.org/officeDocument/2006/math','f').length};},input.xml);
   assert(inspected.texts.includes('研究与证据')&&inspected.texts.includes('原子保存'));assert.equal(inspected.fractions,1);results[input.name]={validZip:true,searchableChinese:true,editableOmmlFractions:inspected.fractions};
  }
 }finally{await browser.close();}
 const html=fs.readFileSync(path.join(docs,'Recovered.html'),'utf8');assert(html.includes('研究与证据')&&html.includes('<table'));results['Recovered.html']={containsChineseAndTable:true};return results;
};
