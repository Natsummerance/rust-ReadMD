'use strict';
// A real native process with developer Node/Office/TeX removed from PATH.
const fs=require('node:fs'),path=require('node:path'),assert=require('node:assert/strict'),crypto=require('node:crypto');
const {chromium}=require('playwright'),{startNative}=require('./native-session.cjs'),zipParts=require('./zip-parts.cjs');
const root=path.resolve(__dirname,'..'),data=fs.mkdtempSync(path.join(root,'.cache/capability-upgrade/no-runtime-native-')),catalog=JSON.parse(fs.readFileSync('assets/data/scientific-figures.json'));
const checks=[];let native;const hash=f=>crypto.createHash('sha256').update(fs.readFileSync(f)).digest('hex');const report={platform:'windows-x64',nativeCsp:true,systemNode:false,systemOffice:false,binarySha256:hash(process.env.READMD_BIN),bootSha256:hash('assets/readmd.boot.js'),checks};
(async()=>{try{
 const originalPath=process.env.PATH;process.env.PATH=process.env.SystemRoot+'/System32;'+process.env.SystemRoot;
 try{native=await startNative(root,data,chromium,28978);}finally{process.env.PATH=originalPath;}
 const page=native.page;await page.evaluate(()=>i18n.setLanguage('zh-CN',false));
 const capabilities=await page.evaluate(async()=>{const r=await apiFetch('/api/diagram/capabilities');return r.json();});
 assert.equal(capabilities.engines['vega-lite'].available,false);assert.equal(capabilities.engines['vega-lite'].client.available,true);
 const content='# Research\n\n'+catalog.templates.map(t=>'```vega-lite\n'+JSON.stringify(t.spec)+'\n```\n').join('\n');
 const result=await page.evaluate(async({content,data})=>{
   const options=await prepareExportFigures(content,{figures:{style:'research',widthMm:89,fontSizePt:7}});
   const r=await apiFetch('/api/export',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({format:'docx',content,out_path:data+'/Research.docx',baseDir:data,options})});return {result:await r.json(),snapshots:options.figures.rendered};
 },{content,data});assert.equal(result.result.ok,true,JSON.stringify(result.result));assert.equal(result.snapshots.length,5);for(const snapshot of result.snapshots){assert.ok(snapshot.svg.includes('<text'));assert.ok(!snapshot.svg.includes('<image'));}
 const parts=zipParts(fs.readFileSync(path.join(data,'Research.docx')));assert.equal([...parts.keys()].filter(n=>/word\/media\/.*\.svg$/.test(n)).length,5);
 checks.push({name:'five scientific templates render as editable vectors and export to real Word without system Node, Office or TeX',passed:true});
 const failed=await page.evaluate(async()=>{const code=JSON.stringify({data:{url:'https://example.invalid/measurements.csv'},mark:'point'});try{await prepareExportFigures('```vega-lite\n'+code+'\n```',{figures:{style:'research'}});return false;}catch(_){return true;}});assert.equal(failed,true);
 checks.push({name:'portable browser rendering still blocks external data instead of producing a blank successful figure',passed:true});
 assert.ok(!fs.existsSync(path.join(data,'readmd-assets')));report.passed=true;
}catch(error){report.passed=false;report.error=String(error.message).replaceAll(data,'<profile>').replaceAll(root,'<workspace>');throw error;
}finally{if(native){native.server.child.kill();await native.browser.close().catch(()=>{});}report.recordedAt=new Date().toISOString();fs.writeFileSync(path.join(data,'report.json'),JSON.stringify(report,null,2));console.log(JSON.stringify({passed:report.passed,checks:checks.length,report:path.relative(root,path.join(data,'report.json'))}));}
})().catch(()=>process.exitCode=1);
