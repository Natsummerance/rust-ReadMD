'use strict';
// Actual Windows service + independent PDF coordinates. This does not certify
// other PDF engines, Word page layout, or the future authoring controls.
const fs=require('node:fs'),path=require('node:path'),net=require('node:net'),assert=require('node:assert/strict');
const {createHarness,fileHash}=require('../tools/test/publication-batch3-harness.cjs');
const root=path.resolve(__dirname,'..');
const base={cover:{enabled:false},toc:{enabled:false},frontMatter:{mode:'none'},pdf:{engine:'rust'},
  typography:{size:'11pt',spacing:0},footer:{pageNumbers:false,text:''},header:{text:''}};
const options=mode=>({...base,typography:{...base.typography,lineSpacing:mode}});
const short='\uFEFFFIRST  \r\nSECOND  \r\nTHIRD\r\n';
const tall='FIRST  \r\nSECOND $\\frac{\\frac{a}{b}}{\\frac{c}{d}}$  \r\nTHIRD\r\n';
const local='<!-- readmd:layout v1 {"id":"local-spacing","text":{"size":16},"paragraph":{"lineSpacing":{"mode":"exact","pt":24}}} -->\r\n\r\n'+short.slice(1);
const long=Array.from({length:300},(_,i)=>'P04ROW'+String(i).padStart(4,'0')).join('  \r\n')+'\r\n';
async function freePort(){const s=net.createServer();await new Promise((r,j)=>{s.once('error',j);s.listen(0,'127.0.0.1',r);});const port=s.address().port;await new Promise(r=>s.close(r));return port;}
async function main(){
  assert.equal(process.platform,'win32');
  const h=createHarness('publication-pdf-typography-native',await freePort(),__filename);
  for(const [name,text] of Object.entries({short,tall,local,long}))fs.writeFileSync(path.join(h.docs,name+'.md'),text,{flag:'wx'});
  const inputs=h.snapshotInputs(),inspector=path.join(root,'tools/test/inspect-publication-line-spacing.py');
  const python=process.env.READMD_QA_PYTHON;
  assert(python&&path.isAbsolute(python),'Explicit independent QA Python runtime required');
  assert.match(process.env.READMD_QA_PYTHON_SHA256||'',/^[a-f0-9]{64}$/);
  assert.equal(fileHash(python),process.env.READMD_QA_PYTHON_SHA256,'QA runtime changed');
  h.report.independentRuntime={sha256:fileHash(python),productDependency:false};
  const sourceFiles=['rust/readmd-kernel/src/pdf_render.rs','rust/readmd-kernel/src/publication_pdf_paragraph.rs','rust/readmd-kernel/src/publication_typography.rs','rust/readmd-kernel/src/md_ast.rs','rust/readmd-kernel/src/mdexport.rs'];
  const hashes=Object.fromEntries(sourceFiles.map(file=>[file,fileHash(path.join(root,file))]));
  h.report.qaDependencySha256['tools/test/inspect-publication-line-spacing.py']=fileHash(inspector);
  h.report.scope='P04 Rust PDF line spacing: actual exported baseline coordinates, fixed-point local override, tall vector formula, pagination, negative preservation, and preset process restart';
  h.report.outputs=[];h.report.limitations.push('Standard-14 line extents are conservative bounds; Exact can reject a spacing that individual glyphs might fit. No arbitrary viewer replacement-font guarantee.');
  let page,requests=0;
  const responses=path.join(h.data,'responses');fs.mkdirSync(responses);
  async function boot(){page=await h.start();await page.evaluate(()=>Promise.all([__readmdSessionReady,__readmdPreferencesReady]));
    assert.equal(h.report.servedBootSha256,process.env.READMD_CANDIDATE_BOOT_SHA256);
    const csp=fs.readFileSync(path.join(process.env.READMD_ASSETS_DIR||path.join(root,'assets'),'index.html'),'utf8').match(/<meta http-equiv="Content-Security-Policy" content="([^"]+)"/)[1];
    assert.equal(await page.locator('meta[http-equiv="Content-Security-Policy"]').getAttribute('content'),csp);}
  async function api(route,body,code){const result=await h.api(route,body);fs.writeFileSync(path.join(responses,String(++requests).padStart(3,'0')+'.json'),JSON.stringify({route,request:body,response:result},null,2),{flag:'wx'});
    assert.equal(result.status,200,route);if(code){assert.equal(result.data.ok,false);assert.equal(result.data.error_code,code);}else assert.equal(result.data.ok,true,JSON.stringify(result.data));return result.data;}
  async function exported(name,content,settings,gap,flags=[]){const out=path.join(h.outputs,name+'.pdf');
    await api('/api/export',{format:'pdf',content,options:settings,baseDir:h.docs,out_path:out,suggestedName:name});
    const evidence=JSON.parse(h.command(python,[inspector,out,String(gap),...flags]));
    fs.writeFileSync(out+'.inspection.json',JSON.stringify(evidence,null,2),{flag:'wx'});
    h.report.outputs.push({name,sha256:fileHash(out),pages:evidence.pages,markers:evidence.markers,gapsPt:evidence.gapsPt,
      renderedPage:evidence.renderedPage,inspectionSha256:fileHash(out+'.inspection.json')});return evidence;}
  await h.finish(async()=>{
    await h.step('native candidate, real boot and production CSP',boot);
    await h.step('all three line modes have actual physical baselines',async()=>{
      await exported('exact-18',short,options({mode:'exact',pt:18}),18);
      await exported('multiple-2',short,options({mode:'multiple',multiple:2}),22);
      await exported('minimum-22',short,options({mode:'atLeast',pt:22}),22);
      const proof=await exported('local-exact-24',local,options({mode:'multiple',multiple:2}),24);
      assert(proof.baselines.every(line=>Math.abs(line.fontSizePt-16)<0.02));h.unchanged(inputs);
    });
    await h.step('minimum mode expands around a real vector formula',async()=>{
      await exported('minimum-tall-formula',tall,options({mode:'atLeast',pt:18}),18,['--tall']);
    });
    await h.step('one long paragraph paginates with complete text and exact baselines',async()=>{
      await exported('long-exact-18',long,options({mode:'exact',pt:18}),18,['--long']);
    });
    await h.step('impossible exact spacing and invalid declarations preserve existing target bytes',async()=>{
      const out=path.join(h.outputs,'protected.pdf');fs.writeFileSync(out,'ORIGINAL PDF',{flag:'wx'});const original=fileHash(out);
      const result=await api('/api/export',{format:'pdf',content:tall,baseDir:h.docs,out_path:out,options:options({mode:'exact',pt:6})},'publication_layout_geometry');
      assert.equal(result.details.coordinateSpace,'pdfParagraphLine');assert(result.details.requiredPt>result.details.requestedPt);
      assert.equal(fileHash(out),original);
      await api('/api/export',{format:'pdf',content:short,baseDir:h.docs,out_path:out,options:options({mode:'exact',pt:0})},'publication_local_style_invalid');
      assert.equal(fileHash(out),original);h.unchanged(inputs);
    });
    await h.step('saved typed preset survives a real process restart and remains operational',async()=>{
      const settings=options({mode:'exact',pt:18});await api('/api/export/presets',{custom:{'P04 PDF exact 18':settings}});
      const presetFile=path.join(h.nativeData,'export_presets.json');const before=fileHash(presetFile);await h.stop();await boot();
      const result=await page.evaluate(async()=>{const r=await apiFetch('/api/export/presets');return{status:r.status,data:await r.json()};});
      assert.equal(result.status,200);const saved=result.data.custom['P04 PDF exact 18'];assert.deepEqual(saved.typography.lineSpacing,settings.typography.lineSpacing);
      assert.equal(fileHash(presetFile),before);await exported('restarted-exact-18',short,saved,18);h.report.presetSha256=before;
    });
    await h.step('source bytes and frozen production dependencies remain unchanged',async()=>{
      for(const [file,hash] of Object.entries(hashes))assert.equal(fileHash(path.join(root,file)),hash);
      h.unchanged(inputs);h.report.sourceHashes=hashes;h.report.inputHashes=inputs;h.report.actualRequests=requests;
      assert.equal(fileHash(inspector),h.report.qaDependencySha256['tools/test/inspect-publication-line-spacing.py']);
    });
  });
}
main().catch(error=>{console.error(String(error.stack||error));process.exitCode=1;});
