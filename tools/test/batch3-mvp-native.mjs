#!/usr/bin/env node
// Reusable Windows MVP audit: real hosts, production CSP, pinned local inputs.
// Independent workflows continue after failure; there are no automatic retries.
import fs from 'node:fs';
import path from 'node:path';
import crypto from 'node:crypto';
import {spawnSync} from 'node:child_process';
import {fileURLToPath} from 'node:url';
const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'../..');
const sha=file=>crypto.createHash('sha256').update(fs.readFileSync(file)).digest('hex');
const cases=[
 {id:'math',requirements:['M3-01','M3-02','M3-08'],script:'ui-tests/math-dsl-native.cjs'},
 {id:'drafts',requirements:['M3-01','M3-08'],script:'ui-tests/math-drafts-native.cjs'},
 {id:'migration',requirements:['M3-02','M3-08'],script:'ui-tests/math-migration-native.cjs'},
 {id:'fonts-pages',requirements:['M3-05','M3-08'],script:'ui-tests/publication-fonts-native.cjs'},
 {id:'pagination',requirements:['M3-03','M3-05','M3-08'],script:'ui-tests/publication-pagination-native.cjs'},
 {id:'references',requirements:['M3-04','M3-08'],script:'ui-tests/publication-references-native.cjs'},
 {id:'books',requirements:['M3-03','M3-06'],script:'ui-tests/epub-publication-native.cjs'},
 {id:'notes',requirements:['M3-04','M3-06','M3-08'],script:'ui-tests/publication-notes-native.cjs'},
 {id:'note-content',requirements:['M3-04','M3-06','M3-08'],script:'ui-tests/publication-note-content-native.cjs'},
 {id:'endnotes',requirements:['M3-03','M3-04','M3-06','M3-08'],script:'ui-tests/publication-endnotes-native.cjs'},
 {id:'figures',requirements:['M3-07','M3-08'],script:'ui-tests/scientific-no-system-runtime-native.cjs'},
];
if(process.argv.includes('--list')){console.log(JSON.stringify(cases,null,2));process.exit(0);}
if(process.platform!=='win32')throw Error('This audit requires the real Windows host; other platforms need separate native evidence.');
const bin=process.env.READMD_BIN;if(!bin||!fs.statSync(bin).isFile())throw Error('READMD_BIN must explicitly select a prebuilt native binary.');
const packs=path.join(root,'.cache/capability-upgrade/locked-packs');
const env={...process.env,READMD_BIN:path.resolve(bin),READMD_ASSETS_DIR:path.join(root,'assets'),READMD_TEST_ONLINE_PACKS:'0',
 READMD_FONTS_PACK:process.env.READMD_FONTS_PACK||path.join(packs,'readmd-fonts-2026.10.10-universal.readmd-pack'),
 READMD_TYPST_PACK:process.env.READMD_TYPST_PACK||path.join(packs,'readmd-typst-0.15.1-windows-x64.readmd-pack'),
 READMD_PDF_PACK:process.env.READMD_PDF_PACK||path.join(packs,'readmd-pdfium-7881-windows-x64.readmd-pack'),
 READMD_EPUBCHECK_JAR:process.env.READMD_EPUBCHECK_JAR||path.join(root,'.cache/community-upgrade/qa-tools/epubcheck-v5.4.0/epubcheck-5.4.0/epubcheck.jar')};
for(const key of ['READMD_FONTS_PACK','READMD_TYPST_PACK','READMD_PDF_PACK','READMD_EPUBCHECK_JAR'])if(!fs.statSync(env[key]).isFile())throw Error('Missing pinned offline input: '+key);
const base=process.env.READMD_MVP_OUTPUT||path.join(root,'.cache/community-upgrade/verification/batch3-mvp');fs.mkdirSync(base,{recursive:true});const out=fs.mkdtempSync(path.join(base,'native-'));
const only=process.argv.find(a=>a.startsWith('--only='))?.slice(7).split(',');if(only?.some(id=>!cases.some(c=>c.id===id)))throw Error('Unknown workflow id.');
const selected=only?cases.filter(c=>only.includes(c.id)):cases;
const result={schema:1,platform:'windows-x64',scope:'Third-batch MVP native workflows; not full B3-51–57, platform, publication or release certification.',binarySha256:sha(bin),bootSha256:sha(path.join(root,'assets/readmd.boot.js')),runnerSha256:sha(fileURLToPath(import.meta.url)),automaticRetries:0,startedAt:new Date().toISOString(),cases:[],passed:false,fullSelection:selected.length===cases.length};
const relative=file=>path.relative(root,file).replaceAll('\\','/');
for(const item of selected){
 const log=path.join(out,item.id+'.log'),started=Date.now();console.log('Running '+item.id+' ...');
 const run=spawnSync(process.execPath,[path.join(root,item.script)],{cwd:root,env,windowsHide:true,encoding:'utf8',maxBuffer:2*1024*1024});
 const output=(run.stdout||'')+(run.stderr||'');fs.writeFileSync(log,output);
 const summary=output.split(/\r?\n/).map(line=>{try{return JSON.parse(line);}catch{return null;}}).findLast(value=>value?.report);
 const row={...item,scriptSha256:sha(path.join(root,item.script)),seconds:Math.round((Date.now()-started)/10)/100,exitCode:run.status,log:relative(log),passed:false};
 if(summary){
  const reportFile=path.resolve(root,summary.report),within=path.relative(path.join(root,'.cache/capability-upgrade'),reportFile);
  if(within.startsWith('..')||path.isAbsolute(within))throw Error('Unexpected native report location.');
  const report=JSON.parse(fs.readFileSync(reportFile,'utf8'));row.report=relative(reportFile);row.reportSha256=sha(reportFile);row.checks=report.checks.length;row.failedStage=report.failedStage;
  row.passed=run.status===0&&report.passed===true&&report.binarySha256===result.binarySha256;
  const boot=report.bootSha256||report.assets?.['assets/readmd.boot.js'];if(boot&&boot!==result.bootSha256)row.passed=false;
 }else row.error=run.error?.message||'Native audit produced no report.';
 result.cases.push(row);fs.writeFileSync(path.join(out,'report.json'),JSON.stringify(result,null,2)+'\n');console.log(JSON.stringify({id:row.id,passed:row.passed,checks:row.checks,seconds:row.seconds,failedStage:row.failedStage,report:row.report}));
}
result.passed=result.cases.length===selected.length&&result.cases.every(c=>c.passed);result.finishedAt=new Date().toISOString();result.seconds=Math.round((Date.parse(result.finishedAt)-Date.parse(result.startedAt))/10)/100;
fs.writeFileSync(path.join(out,'report.json'),JSON.stringify(result,null,2)+'\n');console.log(JSON.stringify({passed:result.passed,fullSelection:result.fullSelection,workflows:result.cases.length,checks:result.cases.reduce((n,c)=>n+(c.checks||0),0),report:relative(path.join(out,'report.json'))}));if(!result.passed)process.exitCode=1;
