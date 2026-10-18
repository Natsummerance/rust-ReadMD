'use strict';
// Development source-projection diagnosis only. No ReadMD host/API or native
// acceptance is run here. Use the original immutable Sections.md and checks.
const fs=require('node:fs'),path=require('node:path'),cp=require('node:child_process');
const crypto=require('node:crypto'),assert=require('node:assert/strict');
const repo=path.resolve(__dirname,'../..');
const original=path.join(repo,'.cache/capability-upgrade/publication-structure-native-yypBMF');
const profile=path.join(repo,'.cache/capability-upgrade/publication-engines-native-n2e7Ug');
const input=path.join(original,'documents/Sections.md');
const options=path.join(repo,'.cache/capability-upgrade/publication-structure-inputs-20261015/sections-probe-options-first.json');
const engine=path.join(profile,'capabilities/managed-v2/typst/0.15.1/bin/typst.exe');
const managedFonts=path.join(profile,'capabilities/managed-v2/fonts/2026.10.10/fonts');
const inspectionBoundary=path.join(repo,'tools/test/publication-batch3-checks.cjs');
const checksFile=path.join(repo,'tools/test/publication-structure-checks.cjs');
const fixturesFile=path.join(repo,'tools/test/publication-structure-fixtures.cjs');
const hash=file=>crypto.createHash('sha256').update(fs.readFileSync(file)).digest('hex');
const relative=file=>path.relative(repo,file).replaceAll('\\','/');
const read=file=>JSON.parse(fs.readFileSync(file,'utf8'));
function originalInputs(){
  assert.equal(hash(input),'d6155e59de41a1f48dc51f82087d7b89283e51e37589698d6068eb14dcdeee64');
  assert.equal(hash(checksFile),'f19d42c755587e187e5f2ecb86f493c66858edf10fe66b8a84f13e2e4b8f9895');
  assert.equal(hash(fixturesFile),'c1e3082c5690093a6fcaa85c4987aac8f677494e1905258bb54a8042f47b787d');
  const fixtures=require(fixturesFile),actual=read(options);
  assert.equal(fs.readFileSync(input,'utf8'),fixtures.sectionSource,'Original fixture bytes changed');
  assert.deepEqual(actual,fixtures.SECTION_OPTIONS,'Original section options changed');
  const request=read(path.join(original,'Sections-typst.pdf.response.json')).request;
  assert.equal(request.content,fixtures.sectionSource);
  assert.deepEqual(request.options,{...actual,pdf:{engine:'typst'}});
  return {sourceSha256:hash(input),optionsSha256:hash(options),checksSha256:hash(checksFile),fixturesSha256:hash(fixturesFile)};
}
function collectGeometry(data,checks){
  const markers={};for(const name of [...Array.from({length:65},(_,i)=>'P03MAIN'+String(i).padStart(2,'0')),'P03FULL','P03RESUME']){
    try{const m=checks.marker(data,name);markers[name]={page:m.page,x:m.x,y:m.y,bbox:m.bbox};}catch(error){markers[name]={error:error.message};}
  }
  return {pageCount:data.pages.length,markers,pages:data.pages.map(p=>({width:p.width,height:p.height,text:p.text,
    visibleGlyphs:(p.glyphTraces||[]).reduce((n,t)=>n+t.chars.filter(c=>c.unicode>32).length,0)}))};
}
function verifyGeometry(data,checks,expected){
  const originalMeasurements=checks.checkSectionPdf(data,expected); // unchanged original assertion/tolerances
  const rows=originalMeasurements.rows,full=originalMeasurements.markers.P03FULL,resume=originalMeasurements.markers.P03RESUME;
  const tokens=Array.from({length:65},(_,i)=>'P03MAIN'+String(i).padStart(2,'0'));
  for(const [parser,text]of [['MuPDF',data.text],['pypdf',data.pypdfText]]){
    assert.deepEqual(text.match(/P03MAIN\d{2}/gu)||[],tokens,parser+' lost, duplicated or reordered original MAIN tokens');
    for(const name of ['P03FRONT','P03FULL','P03RESUME','P03WIDE','P03ODD','P03EVEN'])assert.equal(text.split(name).length-1,1,parser+' original section token: '+name);
  }
  // Each original MAIN paragraph fits one line. Compare real ink and origins,
  // never the declared lineHeight or a rebuilt approximation of engine layout.
  const columns=[];
  for(const row of rows){let column=columns.find(c=>Math.abs(c.x-row.x)<=checks.TOLERANCE.coordinatePt);if(!column){column={x:row.x,rows:[]};columns.push(column);}column.rows.push(row);}
  assert.equal(columns.length,2);for(const column of columns){column.rows.sort((a,b)=>a.y-b.y);for(let i=1;i<column.rows.length;i++){
    const previous=column.rows[i-1],current=column.rows[i];assert(current.bbox[1]>=previous.bbox[3]-checks.TOLERANCE.coordinatePt,'Actual MAIN lines overlap ink');
    checks.near(current.y-previous.y,12.1,checks.TOLERANCE.baselinePt,'Actual 11pt × 1.1 MAIN baseline');
  }}
  assert(full.bbox[1]>=Math.max(...rows.map(r=>r.bbox[3]))-checks.TOLERANCE.coordinatePt,'FULL overlaps an earlier column tail');
  assert(resume.bbox[1]>=full.bbox[3]-checks.TOLERANCE.coordinatePt,'RESUME overlaps FULL');
  assert.equal(data.pages[3].glyphTraces.reduce((n,t)=>n+t.chars.filter(c=>c.unicode>32).length,0),0,'Parity page4 must have no visible header/footer/body glyph');
  return {originalMeasurements,columns:columns.map(c=>({x:c.x,rows:c.rows.length,first:c.rows[0],last:c.rows.at(-1)})),full,resume};
}
function run(projection,out){
  assert(path.isAbsolute(projection)&&path.isAbsolute(out),'Use explicit absolute projection and NEW diagnostic directories');
  projection=fs.realpathSync.native(projection);out=path.resolve(out);assert(!fs.existsSync(out),'Never overwrite an earlier diagnosis');
  const within=path.relative(repo,out);assert(within&&!within.startsWith('..')&&!path.isAbsolute(within),'Diagnosis must be owned within workspace');
  const projectionWithin=path.relative(repo,projection);assert(projectionWithin&&!projectionWithin.startsWith('..')&&!path.isAbsolute(projectionWithin),'Projection must be owned within workspace');
  const source=path.join(projection,'document.typ');assert(fs.statSync(source).isFile());
  const report={passed:false,scope:'original-Sections source-projection diagnostic, NOT ReadMD native or full P03 acceptance',
    productHostInvocations:0,automaticRetries:0,nativeAcceptance:false,scriptSha256:hash(__filename),commands:[],checks:[],limitations:[
      'Direct pinned Typst CLI execution of an actual writer projection; no production API, WebView/CSP, atomic-export or PDFium certification.',
      'The original native50 matrix and its first failure remain separate and immutable.']};
  fs.mkdirSync(out);const save=()=>fs.writeFileSync(path.join(out,'report.json'),JSON.stringify(report,null,2)+'\n');
  function command(exe,args,label){
    const start=Date.now(),r=cp.spawnSync(exe,args,{cwd:out,encoding:'utf8',windowsHide:true,timeout:120000,maxBuffer:32*1024*1024});
    fs.writeFileSync(path.join(out,label+'.stdout'),r.stdout||'');fs.writeFileSync(path.join(out,label+'.stderr'),r.stderr||'');
    report.commands.push({label,exe,args,elapsedMs:Date.now()-start,exitCode:r.status,error:r.error?.message||null,
      stdoutSha256:hash(path.join(out,label+'.stdout')),stderrSha256:hash(path.join(out,label+'.stderr'))});save();
    assert.equal(r.status,0,label+': '+(r.stderr||r.error?.message||'process failed'));return r.stdout;
  }
  let preserved;const pinned=new Map();
  try{
    report.inputs=originalInputs();assert.equal(hash(engine),'081217a463adb006f8894b44227fe4b9c9e91fc85f5463d5948e8370db9bb31e');
    const checks=require(checksFile),fixtures=require(fixturesFile),helper=path.join(repo,'rust/readmd-kernel/src/publication_typst_columns.typ');
    assert(fs.readFileSync(source,'utf8').includes(fs.readFileSync(helper,'utf8')),'Actual writer source must contain frozen helper verbatim');
    const fontFiles=['ReadMDSansSC-Regular.ttf','ReadMDSansSC-Bold.ttf','ReadMDSerifSC-Regular.ttf','ReadMDSerifSC-Bold.ttf'].map(name=>path.join(managedFonts,name));
    const projectedFiles=fs.readdirSync(projection,{withFileTypes:true}).map(entry=>{assert(entry.isFile()&&!entry.isSymbolicLink(),'Only source-probe ordinary output files are expected');return path.join(projection,entry.name);});
    assert(projectedFiles.length<=4096);assert(projectedFiles.reduce((n,file)=>n+fs.statSync(file).size,0)<=128*1024*1024,'Bounded actual source/resources');
    for(const file of [input,options,...projectedFiles,engine,checksFile,fixturesFile,inspectionBoundary,helper,path.join(repo,'rust/readmd-kernel/src/typst_writer.rs'),...fontFiles,
      path.join(repo,'assets/fonts/STIXTwoMath-Regular.ttf'),path.join(repo,'assets/fonts/DejaVuSans.ttf')])pinned.set(file,hash(file));
    report.boundFiles=[...pinned].map(([file,sha256])=>({path:relative(file),sha256,bytes:fs.statSync(file).size}));
    report.actualGlobalFont=fs.readFileSync(source,'utf8').split('\n').find(line=>line.startsWith('#set text(font:'))||null;
    assert(/^#set text\(font:(?:"ReadMD Sans SC"|\("ReadMD Sans SC",)/u.test(report.actualGlobalFont||''),'Original Arial option must have the actual managed Sans projection, without a silent Serif fallback');
    const reuse=require(path.join(repo,'ui-tests/publication-engines-native.cjs'));
    preserved=reuse.verifyEngineProfileReuse({report,command:(exe,args)=>command(exe,args,'profile-preflight')},profile);
    report.engineProfileReuse.freshQaDocumentsAndOutputs=false;
    report.engineProfileReuse.freshDiagnosticOutputs=true;
    report.engineProfileReuse.sourceDocumentReusedVerbatim=true;
    fs.mkdirSync(path.join(out,'packages'));
    const common=['--ignore-system-fonts','--font-path',path.join(repo,'assets/fonts'),'--font-path',managedFonts,'--root',projection,
      '--package-path',path.join(out,'packages'),'--package-cache-path',path.join(out,'packages')];
    const pdf=path.join(out,'Sections-source-projection.pdf');
    command(engine,['--color','never','compile',...common,'--creation-timestamp','0','--jobs','2','--diagnostic-format','short',source,pdf],'compile');
    report.pdf={path:relative(pdf),bytes:fs.statSync(pdf).size,sha256:hash(pdf)};
    // The existing reviewed QA-only boundary owns interpreter/tool selection.
    // Record the actual inspector it supplies without adding a second launcher.
    const inspection=require(inspectionBoundary).inspectArtifact({root:repo,data:out,command:(exe,args)=>{
      const file=args[0];pinned.set(file,hash(file));report.boundFiles.push({path:relative(file),sha256:hash(file),bytes:fs.statSync(file).size});
      return command(exe,args,'inspection');
    }},pdf,{glyphs:true,render:path.join(out,'pages')});
    report.geometry=collectGeometry(inspection,checks);save();
    report.columnDiagnostics=JSON.parse(command(engine,['query',...common,source,'<readmd-section-columns-diagnostic>','--field','value'],'query'));
    for(const diagnostic of report.columnDiagnostics)assert(diagnostic.measurements<=20,'Bounded balancing search');
    report.measurements=verifyGeometry(inspection,checks,fixtures.sectionExpected);
    report.checks.push({name:'unchanged original section geometry plus both-parser tokens, ink clearance and 12.1pt real baselines',passed:true});
    report.passed=true;
  }catch(error){report.failure={message:error.message,stack:error.stack};report.passed=false;}
  finally{
    try{for(const [file,digest]of pinned)assert.equal(hash(file),digest,'Bound file changed: '+relative(file));if(preserved)preserved.assertPreserved();report.boundFilesPreserved=true;}
    catch(error){report.passed=false;report.preservationFailure={message:error.message,stack:error.stack};}
    save();
  }
  console.log(JSON.stringify({passed:report.passed,scope:report.scope,report:relative(path.join(out,'report.json')),sha256:hash(path.join(out,'report.json')),failure:report.failure?.message||report.preservationFailure?.message||null}));
  if(!report.passed)process.exitCode=1;return report;
}
function selfCheck(){
  const inputs=originalInputs(),checks=require(checksFile),fixtures=require(fixturesFile),failed=path.join(original,'Sections-typst.pdf.inspection.json');
  assert.throws(()=>checks.checkSectionPdf(read(failed),fixtures.sectionExpected),/Two real text columns are required/u,'Retained first failure must still fail unchanged original geometry');
  console.log(JSON.stringify({passed:true,selfCheckOnly:true,productInvocations:0,engineInvocations:0,originalInputs:inputs,oldFailureStillRejected:true}));
}
module.exports={originalInputs,collectGeometry,verifyGeometry,run};
if(require.main===module){if(process.argv.length===3&&process.argv[2]==='--self-check')selfCheck();else{assert.equal(process.argv.length,4,'Expected projection-directory NEW-diagnostic-directory, or --self-check');run(process.argv[2],process.argv[3]);}}
