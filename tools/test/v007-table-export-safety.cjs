'use strict';
// Explicit prebuilt kernel, real authenticated HTTP and independent OPC/PDF
// text checks. All inputs, outputs and profiles are owned synthetic fixtures.
// No compiler, GUI, font installation, model or external service is used.
const fs=require('node:fs'),path=require('node:path'),crypto=require('node:crypto'),assert=require('node:assert/strict');
const {spawnSync,execFileSync}=require('node:child_process');
const {options}=require('./v006-table-clipboard-kernel.cjs'),{startKernel}=require('./v006-refactor-host.cjs');
const zipParts=require('../../ui-tests/zip-parts.cjs');
const ROOT=path.resolve(__dirname,'../..'),sha=bytes=>crypto.createHash('sha256').update(bytes).digest('hex');
const encoded=text=>Buffer.from('\ufeff'+text.replaceAll('\n','\r\n'));
const UNSUPPORTED='publication_table_unsupported';
async function main(){
 const args=options(process.argv.slice(2));assert(args.bin&&!args.base);
 const python=process.env.READMD_TEST_PDF_PYTHON;assert(python&&path.isAbsolute(python)&&fs.statSync(python).isFile(),'Explicit bundled Python with pypdf required');
 const out=args.evidence,fixtureRoot=path.join(ROOT,'tests/fixtures/table-safety');
 const paths=Object.fromEntries(['profile','documents','tmp','home'].map(name=>[name,path.join(out,name)]));for(const directory of Object.values(paths))fs.mkdirSync(directory,{mode:0o700});
 const scratch=['tmp','home'].map(name=>({path:paths[name],stat:fs.lstatSync(paths[name])}));Object.assign(process.env,{TMPDIR:paths.tmp,TMP:paths.tmp,TEMP:paths.tmp,CFFIXED_USER_HOME:paths.home,READMD_EXTRA_ROOTS:''});
 const text=name=>fs.readFileSync(path.join(fixtureRoot,name),'utf8'),simple=text('simple.html.md'),merged=text('merged.html.md');
 const cases=[
  {id:'gfm',source:text('gfm.md'),tables:1,markers:['GFM_HEAD_ALPHA','GFM_CELL_ALPHA','GFM_CELL_BETA']},
  {id:'simple-html',source:simple,tables:1,markers:['SIMPLE_HEAD_ALPHA','SIMPLE_HEAD_BETA','SIMPLE_CELL_ALPHA','SIMPLE_CELL_BETA','EM_ALPHA','BR_ALPHA','LINK_ALPHA']},
  {id:'duplicate-html',source:simple+'\n\n'+simple,tables:2,markers:['SIMPLE_CELL_ALPHA','SIMPLE_CELL_BETA'],copies:2},
  {id:'unit-spans',source:simple.replace('<td align="left">SIMPLE_CELL_BETA','<td align="left" colspan="1" rowspan="1">SIMPLE_CELL_BETA'),tables:1,markers:['SIMPLE_CELL_ALPHA','SIMPLE_CELL_BETA']},
  {id:'literal',source:text('literal.md'),tables:0,markers:['LITERAL_FENCE_ALPHA','LITERAL_FENCE_BETA','LITERAL_INLINE_ALPHA']},
  {id:'merged',source:merged,reject:true},
  {id:'nested',source:simple.replace('SIMPLE_CELL_BETA','<table><tr><th>NESTED_ALPHA</th></tr></table>'),reject:true},
  {id:'cell-style',source:simple.replace('<td align="left">SIMPLE_CELL_BETA','<td align="left" style="color:red">SIMPLE_CELL_BETA'),reject:true},
  {id:'ragged',source:simple.replace('<td align="right"></td>',''),reject:true},
 ];
 const binding=()=>['md_ast.rs','docx_writer.rs','mdexport.rs','publication_profile.rs','api_codes.rs','md_ast_html_table_tests.rs'].map(file=>({file:'rust/readmd-kernel/src/'+file,sha256:sha(fs.readFileSync(path.join(ROOT,'rust/readmd-kernel/src',file)))}));
 const report={passed:false,head:execFileSync('git',['rev-parse','HEAD'],{cwd:ROOT,encoding:'utf8'}).trim(),sourceBinding:binding(),binarySha256:sha(fs.readFileSync(args.bin)),binaryOrigin:'Explicit frozen prebuilt candidate; final source/compiled binding requires root build receipts.',driverSha256:sha(fs.readFileSync(__filename)),nativeUiCertified:false,modelsRequested:false,renderedPixelsCertified:false,pdfInspector:'bundled pypdf text only',expectations:{declaredBeforeExecution:true,ordinaryHtmlUsesNativeTable:true,spanOneSupported:true,mergedAndUnknownStructuresRefuseBeforeOutput:true,codeAndCommentsDoNotCauseTableRefusal:true,oldTargetsAndSourceBytesPreserved:true},inputs:cases.map(row=>({id:row.id,sha256:sha(Buffer.from(row.source)),reject:Boolean(row.reject)})),checks:[],exports:[]};
 fs.writeFileSync(path.join(out,'independent-expectations.json'),JSON.stringify(report.expectations,null,2)+'\n',{flag:'wx'});
 let host,phase='start';const check=(name,pass,extra={})=>report.checks.push({name,pass:Boolean(pass),...extra});
 async function api(route,body){const response=await host.api(route,{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify(body)});return{status:response.status,body:await response.json()};}
 function pdfText(file){const code='import json,sys,pypdf; r=pypdf.PdfReader(sys.argv[1]); print(json.dumps({"pages":len(r.pages),"text":"\\n".join(p.extract_text() or "" for p in r.pages)}))';const child=spawnSync(python,['-c',code,file],{cwd:paths.tmp,env:{PATH:process.env.PATH,LANG:'en_US.UTF-8',TMPDIR:paths.tmp,TMP:paths.tmp,TEMP:paths.tmp,PYTHONDONTWRITEBYTECODE:'1',CFFIXED_USER_HOME:paths.home},encoding:'utf8',maxBuffer:1024*1024,timeout:15000});assert.equal(child.status,0,'Bundled PDF reader failed');return JSON.parse(child.stdout);}
 try{
  host=await startKernel(args.bin,paths.profile,paths.documents);report.pid=host.pid;
  for(const row of cases){
   const sourceFile=path.join(paths.documents,row.id+'.md');fs.writeFileSync(sourceFile,encoded(row.source),{flag:'wx'});
   for(const format of ['docx','pdf']){
    phase=row.id+'/'+format;
    const target=path.join(paths.documents,row.id+'.'+format),old=Buffer.from('OWNED_OLD_TARGET_'+row.id+'_'+format);if(row.reject)fs.writeFileSync(target,old,{flag:'wx'});
    const response=await api('/api/export',{format,content:row.source,baseDir:paths.documents,suggestedName:'Owned table safety',out_path:target,options:{pdf:{engine:'rust'},typography:{font:'Arial',size:11},frontMatter:{mode:'none'}}});
    const receipt={case:row.id,format,status:response.status,body:response.body};report.exports.push(receipt);
    if(row.reject){
     check(phase+'/specific-refusal',response.body.ok===false&&response.body.error_code===UNSUPPORTED,{actualCode:response.body.error_code});
     check(phase+'/old-output-protected',fs.readFileSync(target).equals(old));
     const newTarget=path.join(paths.documents,row.id+'-new.'+format),again=await api('/api/export',{format,content:row.source,baseDir:paths.documents,out_path:newTarget,options:{pdf:{engine:'rust'},frontMatter:{mode:'none'}}});
     check(phase+'/no-new-output',again.body.ok===false&&again.body.error_code===UNSUPPORTED&&!fs.existsSync(newTarget),{actualCode:again.body.error_code});
     const preview=await api('/api/export/preview',{format,content:row.source,baseDir:paths.documents,options:{pdf:{engine:'rust'}}});
     check(phase+'/preview-refuses',preview.body.ok===false&&preview.body.error_code===UNSUPPORTED,{actualCode:preview.body.error_code});
    }else{
     check(phase+'/succeeds',response.body.ok===true,{errorCode:response.body.error_code,environmentBlocked:response.body.error_code==='publication_fonts_unavailable'});
     if(response.body.ok&&fs.existsSync(target)){
      const bytes=fs.readFileSync(target);receipt.bytes=bytes.length;receipt.sha256=sha(bytes);
      let actual;
      if(format==='docx'){
       const xml=zipParts(bytes).get('word/document.xml');assert.equal(typeof xml,'string');actual=xml;
       check(phase+'/native-table-count',(xml.match(/<w:tbl>/g)||[]).length===row.tables);
       fs.writeFileSync(path.join(out,row.id+'-docx.xml'),xml,{flag:'wx'});
      }else{const parsed=pdfText(target);actual=parsed.text.replace(/\s+/g,'');receipt.pages=parsed.pages;receipt.pdfTextWhitespaceNormalizedForAtomicMarkers=true;fs.writeFileSync(path.join(out,row.id+'-pdf-text.json'),JSON.stringify(parsed,null,2)+'\n',{flag:'wx'});}
      for(const marker of row.markers)check(phase+'/cell/'+marker,(actual.match(new RegExp(marker,'g'))||[]).length===(row.copies||1));
     }
    }
   }
   if(row.id==='merged'){
    await host.api('/api/file?p='+encodeURIComponent(sourceFile));
    const openedResponse=await api('/api/review',{action:'open',path:sourceFile});assert.equal(openedResponse.status,200);
    const opened=openedResponse.body;assert.equal(typeof opened.document_id,'string');assert.equal(opened.content,row.source);
    const pendingResponse=await api('/api/review',{action:'propose',document_id:opened.document_id,generation:opened.generation,review_revision:opened.review_revision,call_id:crypto.randomUUID(),origin:'ai',edits:[{from:opened.content.length,to:opened.content.length,insert:'\nPENDING_REVIEW_PROTECTED\n'}]});
    assert.equal(pendingResponse.status,200);const pending=pendingResponse.body;assert(pending.changes.some(change=>change.decision==='pending'));
    for(const format of ['docx','pdf']){
     const prepared=await api('/api/export/snapshot',{action:'prepare',document_id:pending.document_id,generation:pending.generation,review_revision:pending.review_revision,call_id:crypto.randomUUID(),format,options:{pdf:{engine:'rust'}}});
     check('snapshot/'+format+'/refuses-before-preparation',prepared.body.ok===false&&prepared.body.error_code===UNSUPPORTED&&!prepared.body.snapshot_id,{actualCode:prepared.body.error_code});
    }
    const after=(await api('/api/review',{action:'read',document_id:pending.document_id})).body;
    check('snapshot/pending-review-protected',after.content===pending.content&&after.generation===pending.generation&&after.review_revision===pending.review_revision&&JSON.stringify(after.changes)===JSON.stringify(pending.changes));
   }
   check(row.id+'/source-bom-crlf-preserved',fs.readFileSync(sourceFile).equals(encoded(row.source)));
  }
  // The same unsupported native shape must remain available to the existing
  // HTML/EPUB consumers, which retain HTML markup rather than dropping cells.
  for(const format of ['html','epub']){const target=path.join(paths.documents,'merged-control.'+format),result=await api('/api/export',{format,content:merged,baseDir:paths.documents,out_path:target,options:{frontMatter:{mode:'none'}}});check(format+'/not-blocked-by-native-table-guard',result.body.error_code!==UNSUPPORTED,{ok:result.body.ok,errorCode:result.body.error_code});}
  const modules=await(await host.api('/api/modules')).json();check('ocr-transcribe-idle',modules.modules.ocr==='idle'&&modules.modules.transcribe==='idle');
  report.passed=report.checks.every(row=>row.pass);if(!report.passed)process.exitCode=1;
 }catch(error){report.harnessFailure={phase,name:error.name,message:String(error.message).replaceAll(out,'<owned>').replaceAll(ROOT,'<repo>').slice(0,1000)};process.exitCode=2;}
 finally{
  if(host)await host.stop();for(const item of scratch){const now=fs.lstatSync(item.path);assert(now.isDirectory()&&!now.isSymbolicLink()&&now.ino===item.stat.ino&&now.dev===item.stat.dev);fs.rmSync(item.path,{recursive:true});}
  report.scratchRemoved=true;report.binaryUnchanged=sha(fs.readFileSync(args.bin))===report.binarySha256;assert(report.binaryUnchanged);report.sourceBindingAfter=binding();report.sourceUnchanged=JSON.stringify(report.sourceBinding)===JSON.stringify(report.sourceBindingAfter);
  const failures=report.checks.filter(row=>!row.pass);fs.writeFileSync(path.join(out,'report.json'),JSON.stringify(report,null,2)+'\n',{flag:'wx'});if(failures.length||report.harnessFailure)fs.writeFileSync(path.join(out,'first-failure.json'),JSON.stringify({failures,harnessFailure:report.harnessFailure},null,2)+'\n',{flag:'wx'});
  console.log(JSON.stringify({passed:report.passed,checks:report.checks.length,failures:failures.length,harnessFailure:report.harnessFailure,binarySha256:report.binarySha256,binaryUnchanged:report.binaryUnchanged}));
 }
}
if(require.main===module)main().catch(error=>{console.error('Owned table safety probe failed: '+error.message);process.exitCode=2;});
module.exports={main};
