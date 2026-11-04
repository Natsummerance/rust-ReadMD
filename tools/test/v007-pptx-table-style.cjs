'use strict';
// Real authenticated export/preview against an explicit prebuilt kernel.
// OPC/XML semantic checks are independent of preview and are not Office pixels.
const fs=require('node:fs'),path=require('node:path'),crypto=require('node:crypto'),assert=require('node:assert/strict');
const {options}=require('./v006-table-clipboard-kernel.cjs'),{startKernel}=require('./v006-refactor-host.cjs');
const zipParts=require('../../ui-tests/zip-parts.cjs');
const ROOT=path.resolve(__dirname,'../..'),sha=bytes=>crypto.createHash('sha256').update(bytes).digest('hex');
const source='# Owned PPTX style\n\n| Label | Group | Amount |\n| :--- | :---: | ---: |\n| Alpha | X | 12.40 |\n| Beta | Y | 7.20 |\n';
const longSource='# Owned PPTX continuation\n\n| Label | Group | Amount |\n| :--- | :---: | ---: |\n'+Array.from({length:60},(_,n)=>'| row-'+String(n).padStart(3,'0')+' | G | '+n+'.40 |\n').join('');
const fallback='# Owned alignment fallback\n\n| Unspecified | Amount |\n| --- | ---: |\n| fallback-marker | 8.40 |\n';
const encoded=value=>Buffer.from('\ufeff'+value.replaceAll('\n','\r\n'));
const attr=(xml,name)=>xml.match(new RegExp('\\b'+name+'="([^"]*)"'))?.[1];
function tableParts(xml){
 return [...xml.matchAll(/<a:tbl\b[^>]*>([\s\S]*?)<\/a:tbl>/g)].map(table=>
  [...table[1].matchAll(/<a:tr\b[^>]*>([\s\S]*?)<\/a:tr>/g)].map(row=>
   [...row[1].matchAll(/<a:tc\b[^>]*>([\s\S]*?)<\/a:tc>/g)].map(cell=>cell[1])));
}
async function main(){
 const args=options(process.argv.slice(2));assert(args.bin&&!args.base);const out=args.evidence;
 const paths=Object.fromEntries(['profile','documents','tmp','home'].map(name=>[name,path.join(out,name)]));for(const directory of Object.values(paths))fs.mkdirSync(directory,{mode:0o700});
 const scratch=['tmp','home'].map(name=>({path:paths[name],stat:fs.lstatSync(paths[name])}));Object.assign(process.env,{TMPDIR:paths.tmp,TMP:paths.tmp,TEMP:paths.tmp,CFFIXED_USER_HOME:paths.home,READMD_EXTRA_ROOTS:''});
 const cases=[
  {id:'grid',source,style:'grid',aligns:['l','ctr','r'],color:'112233',base:22225},
  {id:'three-line',source,style:'three-line',aligns:['l','ctr','r'],color:'112233',base:22225},
  {id:'continued',source:longSource,style:'three-line',aligns:['l','ctr','r'],color:'112233',base:22225,long:true},
  {id:'fallback',source:fallback,style:'three-line',aligns:['ctr','r'],color:'112233',base:22225,fallback:true},
  {id:'default-grid',source,style:'grid',aligns:['l','ctr','r'],color:'C8CDD4',base:9525,defaults:true},
 ];
 const expected={declaredBeforeExecution:true,nativeEditableTable:true,columns:['left','center','right'],threeLine:{topAndBottomPt:3.5,topAndBottomEmu:44450,headerSeparatorPt:1.75,headerSeparatorEmu:22225,leftAndRight:'explicit a:noFill',internalHorizontal:'explicit a:noFill'},continuation:'Every real table fragment repeats the header, outer top and last-row bottom; all source rows appear once',grid:'All four sides retain requested base width/color, independently of prior three-line exports',fallback:'GFM unspecified alignment consumes sanitized table.align; explicit GFM alignment wins',officePixelsCertified:false};
 const report={passed:false,binarySha256:sha(fs.readFileSync(args.bin)),driverSha256:sha(fs.readFileSync(__filename)),sourceBefore:sha(fs.readFileSync(ROOT+'/rust/readmd-kernel/src/pptx_export.rs')),expected,modelsRequested:false,guiUsed:false,officePixelsCertified:false,checks:[],exports:[]};
 fs.writeFileSync(path.join(out,'fixed-inputs.json'),JSON.stringify({expected,cases},null,2)+'\n',{flag:'wx'});
 const check=(name,pass,extra={})=>report.checks.push({name,pass:Boolean(pass),...extra});let host,phase='start';
 async function api(route,body){const response=await host.api(route,{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify(body)});return{status:response.status,body:await response.json()};}
 try{
  host=await startKernel(args.bin,paths.profile,paths.documents);report.pid=host.pid;
  for(const row of cases){
   phase=row.id;const file=path.join(paths.documents,row.id+'.md'),target=path.join(paths.documents,row.id+'.pptx');fs.writeFileSync(file,encoded(row.source),{flag:'wx'});
   const config=row.defaults?{theme:'white',transition:'none'}:{theme:'white',transition:'none',table:{style:row.style,borderColor:'#112233',borderWidth:1.75,...(row.fallback?{align:'center'}:{})}};
   const result=await api('/api/export',{format:'pptx',content:row.source,baseDir:paths.documents,suggestedName:'Owned PPTX table',out_path:target,options:config});
   const receipt={id:row.id,status:result.status,result:result.body,inputSha256:sha(Buffer.from(row.source))};report.exports.push(receipt);check(row.id+'/export-success',result.status===200&&result.body.ok===true);
   if(result.body.ok&&fs.existsSync(target)){
    const bytes=fs.readFileSync(target),parts=zipParts(bytes),slides=[...parts].filter(([name])=>/^ppt\/slides\/slide\d+\.xml$/.test(name)),tables=slides.flatMap(([,xml])=>tableParts(xml));receipt.sha256=sha(bytes);receipt.tableFragments=tables.length;
    fs.writeFileSync(path.join(out,row.id+'-slides.xml'),slides.map(([,xml])=>xml).join('\n'),{flag:'wx'});
    check(row.id+'/editable-a-tbl',tables.length>0&&slides.every(([,xml])=>!xml.includes('<p:pic>'))&&![...parts.keys()].some(name=>name.startsWith('ppt/media/')));
    if(row.long)check(row.id+'/actual-continuations',tables.length>1);
    for(const [fragment,rows] of tables.entries()){
     check(row.id+'/'+fragment+'/rectangular-columns',rows.every(cells=>cells.length===row.aligns.length));
     const header=row.fallback?['Unspecified','Amount']:['Label','Group','Amount'];check(row.id+'/'+fragment+'/repeated-header',rows[0].every((cell,col)=>cell.includes('>'+header[col]+'</a:t>')));
     for(const [ri,cells] of rows.entries())for(const [ci,cell] of cells.entries()){
      const prefix=row.id+'/'+fragment+'/'+ri+'/'+ci,paragraphs=[...cell.matchAll(/<a:pPr\b([^>]*)>/g)];check(prefix+'/column-align',paragraphs.length>0&&paragraphs.every(match=>attr(match[1],'algn')===row.aligns[ci]));
      for(const side of ['L','R','T','B']){
       const line=cell.match(new RegExp('<a:ln'+side+'\\b([^>]*)>([\\s\\S]*?)<\\/a:ln'+side+'>'));
       const wanted=row.style==='grid'?row.base:side==='T'&&ri===0||side==='B'&&ri===rows.length-1?row.base*2:side==='B'&&ri===0?row.base:0;
       check(prefix+'/'+side,line&& (wanted===0 ? line[2].trim()==='<a:noFill/>' : Number(attr(line[1],'w'))===wanted&&line[2].includes('val="'+row.color+'"')&&!line[2].includes('<a:noFill')),{expectedWidthEmu:wanted,actualWidthEmu:line?Number(attr(line[1],'w'))||0:null});
      }
     }
    }
    const combined=slides.map(([,xml])=>xml).join('\n');if(row.long)for(let n=0;n<60;n++)check(row.id+'/row-'+n,(combined.match(new RegExp('>row-'+String(n).padStart(3,'0')+'</a:t>','g'))||[]).length===1);
   }
   // The production PPTX UI previews its exact frozen review snapshot. The
   // legacy content-only preview route accepts PDF/Word, not a PPTX deck.
   const openedFile=await host.api('/api/file?p='+encodeURIComponent(file));assert.equal(openedFile.status,200);
   const review=await api('/api/review',{action:'open',path:file});assert.equal(review.status,200);assert.equal(typeof review.body.document_id,'string');
   const prepared=await api('/api/export/snapshot',{action:'prepare',document_id:review.body.document_id,generation:review.body.generation,review_revision:review.body.review_revision,call_id:crypto.randomUUID(),format:'pptx',suggestedName:'Owned PPTX table',options:config});
   assert.equal(prepared.status,200,prepared.body.error_code);assert.equal(typeof prepared.body.snapshot_id,'string');
   const runtime=await api('/api/export/snapshot',{action:'runtime',snapshot_id:prepared.body.snapshot_id});assert.equal(runtime.status,200,runtime.body.error_code);assert.equal(runtime.body.runtime_ready,true);
   const preview=await api('/api/export/preview',{snapshot_id:prepared.body.snapshot_id,task_id:crypto.randomUUID()});check(row.id+'/preview-success',preview.status===200&&preview.body.ok===true&&preview.body.format==='pptx'&&typeof preview.body.html==='string');
   receipt.previewSha256=sha(Buffer.from(preview.body.html||''));receipt.previewUsesFrozenSnapshot=true;fs.writeFileSync(path.join(out,row.id+'-preview.html'),preview.body.html||JSON.stringify(preview.body),{flag:'wx'});
   const discarded=await api('/api/export/snapshot',{action:'discard',snapshot_id:prepared.body.snapshot_id});assert.equal(discarded.status,200);
   check(row.id+'/source-bom-crlf-untouched',fs.readFileSync(file).equals(encoded(row.source)));
  }
  const grid=report.exports.find(row=>row.id==='grid'),three=report.exports.find(row=>row.id==='three-line');check('preview-style-differential',grid.previewSha256!==three.previewSha256);const html=fs.readFileSync(path.join(out,'three-line-preview.html'),'utf8');check('preview-three-line-no-vertical-rules',html.includes('border-left:0')&&html.includes('border-right:0'));check('preview-preserves-right-column',html.includes('text-align:right'));
  const modules=await(await host.api('/api/modules')).json();check('ocr-transcribe-stay-idle',modules.modules.ocr==='idle'&&modules.modules.transcribe==='idle');report.passed=report.checks.every(row=>row.pass);if(!report.passed)process.exitCode=1;
 }catch(error){report.harnessFailure={phase,name:error.name,message:String(error.message).replaceAll(out,'<owned>').replaceAll(ROOT,'<repo>').slice(0,700)};process.exitCode=2;}
 finally{
  if(host)await host.stop();for(const item of scratch){const current=fs.lstatSync(item.path);assert(current.isDirectory()&&!current.isSymbolicLink()&&current.ino===item.stat.ino&&current.dev===item.stat.dev);fs.rmSync(item.path,{recursive:true});}
  report.scratchRemoved=true;report.binaryUnchanged=sha(fs.readFileSync(args.bin))===report.binarySha256;report.sourceAfter=sha(fs.readFileSync(ROOT+'/rust/readmd-kernel/src/pptx_export.rs'));report.sourceUnchanged=report.sourceBefore===report.sourceAfter;
  const failures=report.checks.filter(row=>!row.pass);fs.writeFileSync(path.join(out,'report.json'),JSON.stringify(report,null,2)+'\n',{flag:'wx'});if(failures.length||report.harnessFailure)fs.writeFileSync(path.join(out,'first-failure.json'),JSON.stringify({failures,harnessFailure:report.harnessFailure},null,2)+'\n',{flag:'wx'});
  console.log(JSON.stringify({passed:report.passed,checks:report.checks.length,failures:failures.length,harnessFailure:report.harnessFailure,binaryUnchanged:report.binaryUnchanged,sourceUnchanged:report.sourceUnchanged}));
 }
}
if(require.main===module)main().catch(error=>{console.error('Owned PPTX table probe failed: '+error.message);process.exitCode=2;});
module.exports={main};
