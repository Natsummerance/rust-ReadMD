'use strict';
// A new candidate lane; historical C61 evidence and its negative case stay frozen.
const fs=require('node:fs'),path=require('node:path'),crypto=require('node:crypto'),assert=require('node:assert/strict');
const {spawnSync}=require('node:child_process');
const native=require('./batch3-math-native-styles-stdio.cjs');
const lifecycle=require('./batch3-math-section-stdio.cjs');
const physical=require('./batch3-math-word-paragraph-stdio.cjs');
const {StdioClient,assertToolCallSchema,verifyBinding,systemEnvironment}=require('./batch3-semantics-stdio.cjs');
const {payload}=require('./managed-tex-process-cold.cjs');
const ROOT=native.ROOT,BASE=path.join(ROOT,'.cache/capability-upgrade/batch3-m07-document-defaults-20261024/qa');
const ORACLE='tools/test/inspect-math-document-defaults.py';
const sha=value=>crypto.createHash('sha256').update(value).digest('hex'),hash=file=>sha(fs.readFileSync(file));
const PLAIN='<!-- OWNED_DEFAULTS_SOURCE 😀 -->\r\n\r\n# PLAIN_CONTROL\r\n\r\nPLAIN_BODY must remain unchanged.\r\n\r\nInline $x^2$ and duplicate $x^2$.\r\n\r\n$$\\frac{x}{y}$$\r\n\r\n$$\\frac{x}{y}$$\r\n\r\n';
const CONTROL_PR='office.mathPr(office.dispDef(val: false), office.preSp(val: 120), office.postSp(val: 240), office.interSp(val: 40), office.intraSp(val: 60))';
const controlMath=display=>'row('+CONTROL_PR+', '+(display?'office.oMathPara(office.oMath(frac(x,y)))':'office.oMath(x^2)')+')';
const CONTROL_PLAIN=PLAIN.replaceAll('$x^2$',':math['+controlMath(false)+']').replaceAll('$$\\frac{x}{y}$$','```readmd-math v1\r\n'+controlMath(true)+'\r\n```');
function args(argv){const out={};for(let i=0;i<argv.length;i++){if(argv[i]==='--plan'){out.plan=true;continue;}assert(['--candidate','--profile','--python','--output'].includes(argv[i]));out[argv[i].slice(2)]=path.resolve(argv[++i]);}return out;}
function plan(){return {schema:1,scope:'M07 ordinary LaTeX inherits checked document defaults across source-bound APIs and all five output consumers',
  nativeInputs:1,sourceOrders:['ordinary-first','native-first'],ordinaryOccurrences:4,nativeOccurrences:3,positiveOutputLimit:15,
  noGui:true,offline:true,candidateRequired:true,fullBatch3Certified:false,wordApplicationCertified:false,readerApplicationCertified:false,
  checks:['native source conversion and migration','exact CRLF/astral occurrence contexts','author/render separation','duplicate-range and stale guards',
    'ordinary patch with fresh candidate analysis','multiline raw source and repeated exact-body rebind','strict provider patch guard','save/reopen/new PID','context invalidation after restart',
    'two-order DOCX native style isolation','EPUB MathML and HTML vectors','real Rust/Typst PDF vectors','explicit conflicts preserve old targets'],
  limits:{perOutputBytes:16*1024*1024,totalOutputBytes:128*1024*1024,workflowMs:900000},
  limitations:['Bounded CLI/MCP and local engine evidence, no GUI or independent application editing.',
    'Existing reviewed engine profile reused, no cold-start performance certification.']};}
function selector(context,o){return {id:context.id,revision:context.revision,fingerprint:context.fingerprint,from:o.from,to:o.to};}
function parseRequest(context,o){return {operation:'parse',source:o.source,dialect:o.dialect,display:o.display,font_size:12,
  ...(o.display?{layout_width:300}:{}),...(o.metadata?{metadata:o.metadata}:{}),document_context:selector(context,o)};}
function expectedError(result,code){assert.equal(lifecycle.errorBody(result).error_code,code);}
function inside(file){const rel=path.relative(BASE,path.resolve(file));assert(rel&&rel!=='..'&&!rel.startsWith('..'+path.sep)&&!path.isAbsolute(rel));return path.resolve(file);}
function htmlVectorCases(selected,previews){
  assert.equal(previews.length,7);assert.equal((selected.match(/class="readmd-export-math(?: readmd-export-math-block)?"/g)||[]).length,7);
  const matches=[...selected.matchAll(/<(span|div) class="readmd-export-math(?: readmd-export-math-block)?"[^>]*>([\s\S]*?)<\/\1>/g)];
  assert.equal(matches.length,7);assert.equal((selected.match(/<svg\b/g)||[]).length,7);assert.equal((selected.match(/<\/svg>/g)||[]).length,7);
  return matches.map((match,index)=>{
    const occurrence=previews[index].occurrence,ordinary=occurrence.dialect==='latex',tag=occurrence.display?'div':'span';
    assert.equal(occurrence.ordinal,index);assert.equal(match[1],tag);
    const label=ordinary?(occurrence.display?'\\frac{x}{y}':'{x}^{2}'):'\\frac{a}{b}';
    const opening='<'+tag+' class="readmd-export-math'+(occurrence.display?' readmd-export-math-block':'')+'"'+(!ordinary?' style="text-align:left"':'')+' role="math" aria-label="'+label+'">';
    assert.equal(match[0].slice(0,match[0].indexOf('>')+1),opening,'Unexpected or hidden math wrapper');
    const svg=match[2];assert(/^<svg\s[^>]*>[\s\S]*<\/svg>$/.test(svg),'Missing complete SVG');
    assert.equal((svg.match(/<svg\b/g)||[]).length,1);assert.equal((svg.match(/<\/svg>/g)||[]).length,1);
    assert(svg.includes('data-math-ink="true"')&&/<path\b[^>]*\bd="[^"]+"/.test(svg),'Missing vector ink');
    return {ordinal:index,svg};
  });
}

async function main(argv=process.argv.slice(2)){
  const options=args(argv);if(options.plan){console.log(JSON.stringify(plan(),null,2));return;}
  for(const key of ['candidate','profile','python','output'])assert(options[key],'Required --'+key);
  assert.equal(process.platform,'win32');inside(options.output);fs.mkdirSync(options.output,{recursive:false});
  const evidence=options.output;for(const dir of ['documents','fixtures','outputs','inspection','requests','responses','wire-first','wire-reopened'])fs.mkdirSync(path.join(evidence,dir));
  const retain=(name,value)=>{const file=path.join(evidence,name);assert(path.relative(evidence,file)&&!path.relative(evidence,file).startsWith('..'));fs.writeFileSync(file,typeof value==='string'||Buffer.isBuffer(value)?value:JSON.stringify(value,null,2)+'\n',{flag:'wx'});return file;};
  const descriptor=file=>{const relative=path.relative(evidence,file);assert(relative&&!relative.startsWith('..')&&!path.isAbsolute(relative));const st=fs.lstatSync(file);assert(st.isFile()&&!st.isSymbolicLink());return {path:relative.replaceAll('\\','/'),sha256:hash(file),bytes:st.size};};
  const binding=native.bindingFor(options);require('./freeze-longtail-candidate.cjs').verify(options.candidate);
  const report={...plan(),startedUtc:new Date().toISOString(),candidate:options.candidate,candidateSha256:hash(options.candidate),binarySha256:binding.binarySha256,
    bootSha256:binding.bootSha256,passed:false,checks:[],calls:[],outputs:[],sessions:[],oracles:[],errors:[],automaticRetries:0};
  let client,reuse,presetsBefore,presetsFile,installedHash,timer,stage='setup',serial=0,oracleSerial=0,assets=binding.assets,profile=options.profile;
  const documents=path.join(evidence,'documents');
  const check=(name,details={})=>{report.checks.push({name,passed:true,...details});console.log(JSON.stringify({stage:name,passed:true}));};
  const call=async(name,arguments_,timeout=120000)=>{
    verifyBinding(binding);assertToolCallSchema({name,arguments:arguments_},client.tools);
    const label=String(++serial).padStart(3,'0')+'-'+name,request=retain('requests/'+label+'.json',{name,arguments:arguments_});
    const result=await client.tool(name,arguments_,timeout),response=retain('responses/'+label+'.json',result);report.calls.push({index:serial,name,pid:client.info.pid,request:descriptor(request),response:descriptor(response)});verifyBinding(binding);return result;
  };
  const oracle=(label,arguments_,script=ORACLE)=>{
    const run=spawnSync(options.python,['-I','-B',path.join(ROOT,script),...arguments_],{windowsHide:true,timeout:60000,maxBuffer:32*1024*1024,env:systemEnvironment(profile,documents)});
    const key=String(++oracleSerial).padStart(3,'0')+'-'+label,stdout=retain('inspection/'+key+'.stdout.json',run.stdout||Buffer.alloc(0)),stderr=retain('inspection/'+key+'.stderr.log',run.stderr||Buffer.alloc(0));
    report.oracles.push({label,script,scriptSha256:hash(path.join(ROOT,script)),arguments:arguments_,stdout:descriptor(stdout),stderr:descriptor(stderr),pid:run.pid,exitCode:run.status,signal:run.signal,error:run.error?String(run.error):null});
    assert(!run.error,run.error?.message);assert.equal(run.signal,null);assert.equal(run.status,0,label);return JSON.parse(run.stdout);
  };
  const start=async(name)=>{
    client=new StdioClient({binary:binding.binary,assets,profile,documents,out:path.join(evidence,name)});await client.initialize(true);
    retain(name+'/tools.json',client.tools);timer=setTimeout(()=>{report.timedOut=true;client?.forceStop();},plan().limits.workflowMs);
  };
  const stop=async()=>{clearTimeout(timer);const result={passed:true};await client.stop(result);report.sessions.push(result);client=null;assert(result.passed);assert.equal(result.session.forcedStop,false);assert.equal(result.session.hostExit.code,0);};
  const read=async(file,text)=>{const result=payload(await call('readmd_read_document',lifecycle.sourceReadArguments(file,text)));assert.equal(result.content,text);assert.equal(result.revision,sha(text));return result;};
  const save=async(file,before,after)=>{const current=await read(file,before);lifecycle.savedEdit(payload(await call('readmd_edit_document',{file_path:file,expected_revision:current.revision,old_text:before,new_text:after,dry_run:false,confirm:true})),before,after);assert.equal(fs.readFileSync(file,'utf8'),after);};
  const register=async(content)=>payload(await call('readmd_math',{operation:'context',content,expected_revision:sha(content)})).context;
  const registerReader=async(content)=>{
    const prepared=payload(await call('readmd_academic_document',{content,baseDir:documents,view:true}));
    const view=typeof prepared.reader_content==='string'?prepared.reader_content:prepared.content;assert.equal(typeof view,'string');
    const registered=payload(await call('readmd_math',{operation:'context',content,expected_revision:sha(content),view_content:view}));
    assert.equal(typeof registered.view_content,'string');return{...registered,prepared:view};
  };
  const exportActual=async(label,source,format,engine='rust')=>{
    assert(report.outputs.length<plan().positiveOutputLimit);const file=path.join(evidence,'outputs',label+'.'+format),request=native.exportRequestFor(source,format,engine,documents,file);
    const response=payload(await call(request.name,request.arguments));const bytes=fs.statSync(file).size;
    assert(bytes>0&&bytes<=plan().limits.perOutputBytes);assert.equal(response.file_size,bytes);
    assert(report.outputs.reduce((sum,row)=>sum+row.bytes,0)+bytes<=plan().limits.totalOutputBytes);
    assert(!/glyph_missing|missing character:|missing.{0,30}glyph|glyph.{0,30}missing/iu.test(JSON.stringify(response.warnings)));
    report.outputs.push({label,format,engine:format==='pdf'?engine:undefined,path:path.relative(evidence,file).replaceAll('\\','/'),sha256:hash(file),bytes,sourceSha256:sha(source),callIndex:serial,warnings:response.warnings});return file;
  };
  try{
    retain('plan.json',plan());stage='reviewed engine profile';
    const helpers=require('../../ui-tests/publication-engines-native.cjs');profile=helpers.resolveEngineReuseProfile(profile);
    const harness={root:ROOT,data:evidence,report,command(binary,arguments_){const r=spawnSync(binary,arguments_,{windowsHide:true,timeout:60000,maxBuffer:24*1024*1024});assert(!r.error);assert.equal(r.signal,null);assert.equal(r.status,0);return r.stdout.toString('utf8');}};
    for(const [key,name]of [['READMD_FONTS_PACK','readmd-fonts-2026.10.10-universal.readmd-pack'],['READMD_PDFIUM_PACK','readmd-pdfium-7881-windows-x64.readmd-pack'],['READMD_TYPST_PACK','readmd-typst-0.15.1-windows-x64.readmd-pack']])process.env[key]??=path.join(ROOT,'.cache/capability-upgrade/locked-packs',name);
    reuse=helpers.prepareEngineProfileReuse(harness,profile,assets);assets=reuse.assets;
    presetsFile=path.join(profile,'export_presets.json');presetsBefore=fs.readFileSync(presetsFile);retain('inspection/presets-before.json',presetsBefore);
    const installed=JSON.stringify(native.presetOverlay(presetsBefore.toString('utf8')),null,2)+'\n';fs.writeFileSync(presetsFile,installed);installedHash=sha(installed);
    await start('wire-first');const firstPid=client.info.pid;
    stage='native conversion';const nativeFile=path.join(evidence,'fixtures/native.docx');const input=oracle('native-fixture',['fixture','--kind','a','--output',nativeFile],native.ORACLE);
    const converted=await call('readmd_convert_to_markdown',{file_path:nativeFile});assert.notEqual(converted.isError,true);const raw=converted.content.filter(item=>item.type==='text').map(item=>item.text).join('\n');retain('inspection/converted.md',raw);
    assert.equal(hash(nativeFile),input.sha256);const migration=physical.migrationPlan(payload(await call('readmd_math',{operation:'migrate',content:raw,expected_revision:sha(raw)})),raw);
    const readable=migration.content;assert.equal(typeof readable,'string');assert(!readable.includes('<!--readmd:math:v1:'));const nativeSource=readable.replace(/\r\n?|\n/g,'\r\n');retain('inspection/native-readable.md',nativeSource);
    const sources=[{name:'ordinary-first',source:PLAIN+nativeSource},{name:'native-first',source:nativeSource+'\r\n\r\n'+PLAIN}];
    check('Native current OPC converted and migrated to readable CRLF author source');
    const baselineDocx=await exportActual('ordinary-baseline',PLAIN,'docx');
    let oldBinding;
    for(const entry of sources){
      stage=entry.name+' source-bound API';const file=retain('documents/'+entry.name+'.md','OWNED_SENTINEL\r\n');await save(file,'OWNED_SENTINEL\r\n',entry.source);entry.file=file;
      const reader=await registerReader(entry.source),context=reader.context;entry.context=context;assert.equal(context.revision,sha(entry.source));assert.equal(context.active,true);assert.equal(context.occurrences.length,7);
      retain('inspection/'+entry.name+'-actual-reader-view.md',reader.prepared);retain('inspection/'+entry.name+'-bound-reader-view.md',reader.view_content);
      assert.equal((reader.view_content.match(/<!--readmd:math-slot:v1:/g)||[]).length,7);
      assert.equal(reader.view_content.replace(/[^\r\n]/g,''),reader.prepared.replace(/[^\r\n]/g,''));
      expectedError(await call('readmd_math',{operation:'context',content:entry.source,expected_revision:sha(entry.source),view_content:reader.prepared+'\r\n$z$'}),'math_unsupported_extension');
      const ordinary=context.occurrences.filter(o=>o.dialect==='latex');assert.equal(ordinary.length,4);assert.equal(new Set(ordinary.map(o=>o.from)).size,4);
      const previews=[];
      for(const occurrence of context.occurrences){
        assert.equal(entry.source.slice(occurrence.body_from,occurrence.body_to).trimEnd(),occurrence.source.trimEnd());
        const request=parseRequest(context,occurrence),parsed=payload(await call('readmd_math',request));assert.equal(typeof parsed.vector_preview,'string');assert.equal(parsed.document.source,occurrence.source);
        assert.equal(parsed.document.revision,sha(occurrence.source));assert.equal(parsed.document_context.from,occurrence.from);
        if(occurrence.dialect==='latex')assert(!JSON.stringify(parsed.document.root).includes('rd-document-defaults'),'Derived defaults leaked into author document');
        retain('inspection/'+entry.name+'-'+occurrence.ordinal+'.svg',parsed.vector_preview);previews.push({occurrence,vector:parsed.vector_preview});
        if(occurrence.dialect==='latex'){
          const control=payload(await call('readmd_math',{operation:'parse',source:controlMath(occurrence.display),dialect:'dsl',display:occurrence.display,font_size:12,...(occurrence.display?{layout_width:300}:{})}));
          const actualFile=path.join(evidence,'inspection',entry.name+'-'+occurrence.ordinal+'.svg'),controlFile=retain('inspection/'+entry.name+'-control-'+occurrence.ordinal+'.svg',control.vector_preview);
          oracle(entry.name+'-ordinary-svg-'+occurrence.ordinal,['compare-svg','--actual',actualFile,'--control',controlFile]);
        }
      }
      const first=ordinary[0],second=ordinary[1],request=parseRequest(context,first);oldBinding=request;
      expectedError(await call('readmd_math',{...request,document_context:{...request.document_context,to:request.document_context.to+1}}),'transaction_invalid_range');
      expectedError(await call('readmd_math',{...request,source:'different'}),'transaction_conflict');
      expectedError(await call('readmd_math',{...request,document_context:{...request.document_context,revision:sha('stale')}}),'transaction_conflict');
      const parsed=payload(await call('readmd_math',parseRequest(context,second)));const target=parsed.document.nodes.find(node=>node.tex==='x');assert(target,'Editable source node unavailable');
      const changed=payload(await call('readmd_math',{...parseRequest(context,second),operation:'patch',expected_revision:parsed.document.revision,patch:{operation:'replace_source',node_id:target.id,text:'z'}}));
      const candidateSource=entry.source.slice(0,second.body_from)+'z^2'+entry.source.slice(second.body_to);
      assert.equal(changed.document.source,'z^2');assert.equal(changed.document.revision,sha('z^2'));assert.equal(changed.document_context.revision,sha(candidateSource));
      const changedRequest={...parseRequest(context,second),source:'z^2',document_context:changed.document_context};
      const changedParse=payload(await call('readmd_math',changedRequest));assert.equal(changedParse.document.source,'z^2');assert.equal(changedParse.vector_preview,changed.vector_preview);
      expectedError(await call('readmd_math',{...changedRequest,operation:'patch',expected_revision:parsed.document.revision,patch:{operation:'replace_source',node_id:target.id,text:'q'}}),'transaction_conflict');
      expectedError(await call('readmd_math',{...parseRequest(context,second),source:'z^2'}),'transaction_conflict');
      expectedError(await call('readmd_math',{...changedRequest,source:second.source}),'transaction_conflict');
      await save(file,entry.source,candidateSource);await read(file,candidateSource);await save(file,candidateSource,entry.source);
      const serialized=payload(await call('readmd_math',{...request,operation:'serialize',target:'dsl'}));assert(!serialized.output.includes('dispDef'),'Serialization wrote inherited settings into author source');
      const provider=context.occurrences.find(o=>o.dialect==='dsl'),providerParsed=payload(await call('readmd_math',parseRequest(context,provider)));
      const setting=providerParsed.document.nodes.find(node=>node.kind==='office.preSp'||node.tex?.startsWith('office.preSp('));assert(setting,'Source-bound mathPr setting missing');
      expectedError(await call('readmd_math',{...parseRequest(context,provider),operation:'patch',expected_revision:providerParsed.document.revision,patch:{operation:'set_attribute',node_id:setting.id,name:'m:val',value:'140'}}),'transaction_conflict');
      await read(file,entry.source);entry.previews=previews;check(entry.name+': exact source, duplicate/stale guards, author separation and fresh patch');
    }
    stage='multiline exact-body editing';
    const multilineSource=nativeSource+'\r\n\r\n$$\r\n  x^2 +\r\n y^2 \r\n$$\r\n',multilineContext=await register(multilineSource);
    assert.equal(multilineContext.active,true);assert.equal(multilineContext.occurrences.length,4);
    const multiline=multilineContext.occurrences[3];assert.equal(multiline.dialect,'latex');assert.equal(multiline.display,true);assert(multiline.patchable);
    const multilineFile=retain('documents/multiline.md',multilineSource);await read(multilineFile,multilineSource);
    for(const [index,body]of ['\r\n  z^2 +\r\n y^2 \r\n','\r\n  q^2 +\r\n y^2 \r\n'].entries()){
      const candidate=multilineSource.slice(0,multiline.body_from)+body+multilineSource.slice(multiline.body_to),context=await register(candidate);
      assert.equal(context.revision,sha(candidate));assert.equal(context.occurrences.length,4);
      for(let i=0;i<3;i++)assert.deepEqual(context.occurrences[i],multilineContext.occurrences[i]);
      const occurrence=context.occurrences[3];assert.equal(occurrence.from,multiline.from);assert.equal(occurrence.body_from,multiline.body_from);assert.equal(occurrence.body_to,multiline.body_from+body.length);
      assert.equal(candidate.slice(occurrence.body_from,occurrence.body_to),body);assert.equal(occurrence.source_revision,sha(occurrence.source));
      const parsed=payload(await call('readmd_math',parseRequest(context,occurrence)));assert.equal(parsed.document.source,occurrence.source);assert.equal(typeof parsed.vector_preview,'string');
      await save(multilineFile,multilineSource,candidate);await read(multilineFile,candidate);await save(multilineFile,candidate,multilineSource);
      retain('inspection/multiline-candidate-'+index+'.md',candidate);
    }
    check('Multiline CRLF raw body, successive original-range rebinds and actual save/restore');
    stage='actual CSL reader projection';retain('documents/defaults-refs.json',[{id:'qa-defaults',type:'book',title:'Structured notes',author:[{family:'ReadMD',given:'QA'}],issued:{'date-parts':[[2026]]}}]);
    const citationSource='---\r\nbibliography: defaults-refs.json\r\n---\r\n\r\n# Cited mathematics\r\n\r\nEvidence [@qa-defaults, p. 12].\r\n\r\n$$x^2$$\r\n\r\n```readmd-math v1\r\n'+controlMath(true)+'\r\n```\r\n';
    const citationView=await registerReader(citationSource);assert(citationView.prepared.includes('readmd-citation'));assert(!citationView.prepared.includes('[@qa-defaults'));
    assert.equal(citationView.context.occurrences.length,2);assert.equal((citationView.view_content.match(/<!--readmd:math-slot:v1:/g)||[]).length,2);
    retain('inspection/actual-csl-reader-view.md',citationView.prepared);retain('inspection/actual-csl-bound-view.md',citationView.view_content);
    for(const occurrence of citationView.context.occurrences){const parsed=payload(await call('readmd_math',parseRequest(citationView.context,occurrence)));assert.equal(typeof parsed.vector_preview,'string');}
    check('Real CSL rich reader projection binds author ranges after citation replacement');
    await stop();await start('wire-reopened');assert.notEqual(client.info.pid,firstPid);expectedError(await call('readmd_math',oldBinding),'transaction_conflict');
    for(const entry of sources){
      stage=entry.name+' actual five consumers';await read(entry.file,entry.source);const context=await register(entry.source);assert.equal(context.revision,entry.context.revision);assert.equal(context.fingerprint,entry.context.fingerprint);assert.notEqual(context.id,entry.context.id);
      for(const occurrence of context.occurrences){const parsed=payload(await call('readmd_math',parseRequest(context,occurrence)));assert.equal(parsed.vector_preview,entry.previews[occurrence.ordinal].vector);}
      const docx=await exportActual(entry.name,entry.source,'docx');retain('inspection/'+entry.name+'-docx-proof.json',oracle(entry.name+'-docx',['docx','--source',nativeFile,'--result',docx,'--baseline',baselineDocx]));
      const html=await exportActual(entry.name,entry.source,'html'),text=fs.readFileSync(html,'utf8'),selected=physical.htmlSelectedMarkdown(text);
      retain('inspection/'+entry.name+'-selected-html-source.md',selected);
      assert.equal((selected.match(/class="readmd-export-math(?: readmd-export-math-block)?"/g)||[]).length,7);assert(!selected.includes('$$\\frac{x}{y}$$'));assert(selected.includes('PLAIN_CONTROL'));
      for(const row of htmlVectorCases(selected,entry.previews)){
        const occurrence=context.occurrences[row.ordinal],request=parseRequest(context,occurrence);delete request.layout_width;
        const rendered=payload(await call('readmd_math',request));assert.equal(rendered.document.source,occurrence.source);assert.equal(rendered.document_context.from,occurrence.from);assert.equal(rendered.document_context.to,occurrence.to);
        const actual=retain('inspection/'+entry.name+'-html-'+row.ordinal+'.svg',row.svg),control=retain('inspection/'+entry.name+'-html-control-'+row.ordinal+'.svg',rendered.vector_preview);
        oracle(entry.name+'-html-svg-'+row.ordinal,['compare-svg','--actual',actual,'--control',control]);
      }
      const epub=await exportActual(entry.name,entry.source,'epub');oracle(entry.name+'-epub',['epub',epub,'--order',entry.name]);
      for(const engine of ['rust','typst']){
        const pdf=await exportActual(entry.name+'-'+engine,entry.source,'pdf',engine),inspected=oracle(entry.name+'-'+engine+'-pdf',['pdf',pdf]);
        assert.equal(typeof inspected.visibleText,'string');
        for(const marker of ['PLAIN_CONTROL','PLAIN_BODY must remain unchanged.','Inline ',' and duplicate ','M07_PREFIX_','\u{1d6c2}','KEEP_NEIGHBOR','M07_BEFORE_1','M07_BEFORE_2','M07_BEFORE_3','M07_SUFFIX_KEEP'])assert(inspected.visibleText.includes(marker),'Actual PDF omitted or hid prose '+marker);
        const controlSource=entry.name==='ordinary-first'?CONTROL_PLAIN+nativeSource:nativeSource+'\r\n\r\n'+CONTROL_PLAIN;
        const control=await exportActual(entry.name+'-explicit-control-'+engine,controlSource,'pdf',engine);
        oracle(entry.name+'-'+engine+'-complete-pdf-equality',['compare-pdf','--actual',pdf,'--control',control,'--math-order',entry.name,'--engine',engine]);
      }
      check(entry.name+': saved/new-PID source and actual DOCX/HTML/EPUB/Rust PDF/Typst PDF');
    }
    stage='explicit conflict old target';
    const protectedFile=retain('outputs/protected.docx',fs.readFileSync(baselineDocx)),before=hash(protectedFile),names=fs.readdirSync(path.dirname(protectedFile)).sort();
    let conflicts=0;
    for(const entry of sources)for(const setting of ['office.mathPr(office.dispDef())','office.mathPr(office.dispDef(val: true))','office.mathPr()']){
      // A fenced block can legally interrupt a paragraph without a blank line.
      // Preserve this boundary: the C65 failure must never be hidden by the QA.
      const conflict=entry.source.trimEnd()+'\r\n```readmd-math v1\r\nrow('+setting+', office.oMathPara(office.oMath(x)))\r\n```\r\n';
      const request=native.exportRequestFor(conflict,'docx','rust',documents,protectedFile);request.arguments.overwrite=true;
      expectedError(await call(request.name,request.arguments),'math_document_defaults_conflict');assert.equal(hash(protectedFile),before);assert.deepEqual(fs.readdirSync(path.dirname(protectedFile)).sort(),names);conflicts++;
    }
    check('Explicit on/off conflict preserves target bytes and creates no extra output',{cases:conflicts,target:descriptor(protectedFile),entriesUnchanged:true});
    await stop();assert.equal(report.outputs.length,15);assert.equal(report.sessions.length,2);report.passed=true;
  }catch(error){report.firstFailure={stage,error:String(error.stack||error)};report.errors.push(report.firstFailure);process.exitCode=1;
  }finally{
    if(client)try{await stop();}catch(error){report.passed=false;report.errors.push({stage:'stop',error:String(error.stack||error)});process.exitCode=1;}
    if(presetsBefore)try{assert.equal(hash(presetsFile),installedHash);fs.writeFileSync(presetsFile,presetsBefore);assert.equal(hash(presetsFile),sha(presetsBefore));report.presetsRestored=true;}catch(error){report.passed=false;report.errors.push({stage:'presets',error:String(error)});process.exitCode=1;}
    if(reuse)try{reuse.assertPreserved();report.runtimePreserved=true;}catch(error){report.passed=false;report.errors.push({stage:'runtime',error:String(error)});process.exitCode=1;}
    try{verifyBinding(binding);report.finalBinding=require('./freeze-longtail-candidate.cjs').verify(options.candidate);}catch(error){report.passed=false;report.errors.push({stage:'candidate',error:String(error)});process.exitCode=1;}
    try{
      const artifacts=[];
      const visit=dir=>{for(const entry of fs.readdirSync(dir,{withFileTypes:true}).sort((a,b)=>a.name.localeCompare(b.name))){assert(!entry.isSymbolicLink());const file=path.join(dir,entry.name);if(entry.isDirectory())visit(file);else artifacts.push(descriptor(file));}};
      for(const dir of ['documents','fixtures','outputs','inspection','requests','responses','wire-first','wire-reopened'])visit(path.join(evidence,dir));
      artifacts.push(descriptor(path.join(evidence,'plan.json')));report.evidenceArtifacts=artifacts.sort((a,b)=>a.path.localeCompare(b.path));
    }catch(error){report.passed=false;report.errors.push({stage:'evidence sealing',error:String(error.stack||error)});process.exitCode=1;}
    report.finishedUtc=new Date().toISOString();report.stage=stage;retain('report.json',report);console.log(JSON.stringify({evidence,passed:report.passed,checks:report.checks.length,outputs:report.outputs.length,errors:report.errors.length}));
  }
}
if(require.main===module)main().catch(error=>{console.error(error.stack||error);process.exitCode=1;});
module.exports={args,plan,selector,parseRequest,htmlVectorCases,PLAIN,CONTROL_PLAIN,controlMath,BASE,ORACLE,main};
