'use strict';
// Explicit development ABI probe. The selected external library is read only;
// it is not installed, shipped, or enrolled into application trust/catalogs.
const fs=require('node:fs'),path=require('node:path'),crypto=require('node:crypto'),assert=require('node:assert/strict');
const {spawnSync}=require('node:child_process');
const {startKernel}=require('./v006-table-clipboard-kernel.cjs');
const {fixture}=require('../../ui-tests/pdf-fixture.cjs');
const sha=b=>crypto.createHash('sha256').update(b).digest('hex');
const root=path.resolve(__dirname,'../..');
function pixels(before,after,rect){
  const a=Buffer.from(before,'base64'),b=Buffer.from(after,'base64');assert.equal(a.length,b.length);const w=a.readInt32LE(18),h=-a.readInt32LE(22);let changed=0;
  for(let y=0;y<h;y++)for(let x=0;x<w;x++){const at=54+(y*w+x)*4;if(!a.subarray(at,at+4).equals(b.subarray(at,at+4))){changed++;assert(x>=rect.x-4&&x<=rect.x+rect.width+4&&y>=rect.y-4&&y<=rect.y+rect.height+4,'Native ink changed pixels outside its annotation bounds');}}
  assert(changed>10,'Native InkList must paint visible vector ink');return changed;
}
async function main(){
  const args={};let multi=false;for(let i=2;i<process.argv.length;){if(process.argv[i]==='--multi'){multi=true;i++;continue;}if(!['--kernel','--worker','--library','--output'].includes(process.argv[i])||!process.argv[i+1])throw Error('Expected explicit --kernel, --worker, --library and --output, optionally --multi');args[process.argv[i].slice(2)]=path.resolve(process.argv[i+1]);i+=2;}
  assert(Object.keys(args).length===4);for(const name of ['kernel','worker','library'])assert(fs.statSync(args[name]).isFile());assert(!fs.existsSync(args.output));fs.mkdirSync(args.output);
  const docs=path.join(args.output,'documents');fs.mkdirSync(docs);const original=path.join(docs,'owned.pdf'),signed=path.join(docs,'synthetic-signed.pdf');fs.writeFileSync(original,fixture());fs.writeFileSync(signed,fixture({signature:true}));
  const originalHash=sha(fs.readFileSync(original)),libraryHash=sha(fs.readFileSync(args.library));
  const report={schema:1,scope:'development ABI only; external selected library, no managed-pack certification',platform:process.platform+'-'+process.arch,syntheticOnly:true,workerSha256:sha(fs.readFileSync(args.worker)),librarySha256:libraryHash,sourceSha256:sha(fs.readFileSync(path.join(root,'rust/readmd-kernel/src/pdfium_worker.rs'))),checks:[]};
  let kernel,counter=0;
  function call(payload,label){
    const request=path.join(args.output,`${++counter}-${label}.request.json`),output=path.join(args.output,`${counter}-${label}.result.json`);
    fs.writeFileSync(request,JSON.stringify({library:args.library,pdf:original,password:'',page:0,width:400,operations:[],...payload}),{flag:'wx'});
    const child=spawnSync(args.worker,[request,output],{input:'',encoding:'utf8',maxBuffer:65536,timeout:30000,windowsHide:true});
    assert.equal(child.status,0,'Native worker must exit successfully: '+(child.stderr||''));return JSON.parse(fs.readFileSync(output));
  }
  try{
    kernel=await startKernel(args.kernel,path.join(args.output,'profile'),docs);
    const document={schema:1,width:120,height:80,layers:[{name:'Uniform vector stroke',visible:true,opacity:100,strokes:[{color:'#1267fe',width:8,points:[[10,20,1],[50,55,1],[100,30,1]]}]}]};
    if(multi)document.layers[0].strokes.push({color:'#1267fe',width:8,points:[[20,8,1],[65,8,1]]});
    const response=await kernel.api('/api/drawing',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({action:'render',document})});const shared=await response.json();assert(response.ok&&shared.ok&&shared.svg);
    fs.writeFileSync(path.join(args.output,'owned-stroke.svg'),shared.svg,{flag:'wx'});
    const stroke={width:120,height:80,color:'#1267fe',strokeWidth:8,points:[[10,20],[50,55],[100,30]]};
    if(multi){stroke.strokes=[stroke.points,[[20,8],[65,8]]];delete stroke.points;}
    let plain,inkOperation,painted;
    for(const turns of multi?[0]:[0,1,2,3]){
      const rotation=turns?[{kind:'rotate',page:0,turns}]:[],before=call({operations:rotation},'before-'+turns);assert.equal(before.ok,true);
      const c=before.coordinates,scale=Math.min(before.width*.4/stroke.width,before.height*.4/stroke.height),x=before.width*.25,y=before.height*.4;
      const sx=Math.hypot(...c.x),sy=Math.hypot(...c.y),unit=Math.min(sx,sy),transform=[c.origin[0]+c.x[0]*x+c.y[0]*y,c.origin[1]+c.x[1]*x+c.y[1]*y,c.x[0]*scale*unit/sx,c.x[1]*scale*unit/sx,c.y[0]*scale*unit/sy,c.y[1]*scale*unit/sy];
      const operation={kind:'ink',page:0,svg:shared.svg,stroke,transform},after=call({operations:[...rotation,operation]},'ink-'+turns);assert.equal(after.ok,true,after.error_code);
      const annotation=after.annotations.find(a=>a.kind==='ink');assert(annotation);assert.equal(after.annotations.length,before.annotations.length+1);assert.deepEqual(after.objects,before.objects);
      const changed=pixels(before.image,after.image,annotation.bounds);
      if(!turns){plain=before;inkOperation=operation;painted=after;fs.writeFileSync(path.join(args.output,'before.bmp'),Buffer.from(before.image,'base64'));fs.writeFileSync(path.join(args.output,'ink.bmp'),Buffer.from(after.image,'base64'));}
      report.checks.push({name:'native InkList visible within bounds, original objects retained',rotation:turns*90,changedPixels:changed});
    }
    if(multi){
      const m=inkOperation.transform,c=plain.coordinates,px=m[0]+m[2]*60+m[4]*19,py=m[1]+m[3]*60+m[5]*19,dx=px-c.origin[0],dy=py-c.origin[1],det=c.x[0]*c.y[1]-c.x[1]*c.y[0];
      const x=Math.round((dx*c.y[1]-dy*c.y[0])/det),y=Math.round((dy*c.x[0]-dx*c.x[1])/det),a=Buffer.from(plain.image,'base64'),b=Buffer.from(painted.image,'base64'),width=a.readInt32LE(18);
      for(let row=y-1;row<=y+1;row++)for(let col=x-1;col<=x+1;col++){const at=54+(row*width+col)*4;assert(a.subarray(at,at+4).equals(b.subarray(at,at+4)),'Separate strokes must not acquire a connecting segment');}
      assert.equal(call({},'undo-whole-group').image,plain.image);assert.equal(call({operations:[inkOperation]},'redo-whole-group').image,painted.image);report.checks.push({name:'separate strokes retain their blank gap and whole-operation replay restores both together'});
      const oldDocument=structuredClone(document);oldDocument.layers[0].strokes.splice(1);const response=await kernel.api('/api/drawing',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({action:'render',document:oldDocument})}),legacySvg=(await response.json()).svg;assert(response.ok&&legacySvg);
      const legacy={...inkOperation,svg:legacySvg,stroke:{width:120,height:80,color:'#1267fe',strokeWidth:8,points:stroke.strokes[0]}};assert.equal(call({operations:[legacy]},'legacy-points-replay').ok,true);report.checks.push({name:'legacy single-stroke points journal remains replayable'});
    }
    const secondBefore=call({page:1},'second-before'),secondAfter=call({page:1,operations:[inkOperation]},'second-after');assert.equal(secondBefore.image,secondAfter.image);report.checks.push({name:'unmodified second page pixel-identical'});
    const saved=path.join(docs,'ink-saved.pdf');assert.equal(call({action:'save',operations:[inkOperation],output:saved},'save').ok,true);
    const reopened=call({pdf:saved},'reopen');assert.equal(reopened.image,painted.image);assert(reopened.annotations.some(a=>a.kind==='ink'));assert.deepEqual(reopened.objects,plain.objects);
    const bytes=fs.readFileSync(saved);assert.match(bytes.toString('latin1'),/\/Subtype\s*\/Ink/);assert(bytes.includes(Buffer.from('/ReadMDInk')));
    if(multi){const inspection=spawnSync('python3',[path.join(__dirname,'pdf-ink-structure.py'),args.library,saved,path.join(args.output,'owned-stroke.svg'),JSON.stringify([3,2])],{encoding:'utf8',maxBuffer:65536,timeout:15000});assert.equal(inspection.status,0,inspection.stderr);const structure=JSON.parse(inspection.stdout);fs.writeFileSync(path.join(args.output,'annotation-structure.json'),JSON.stringify(structure,null,2)+'\n');report.checks.push({name:'native saved annotation retains two independent InkList entries and round vector paths; embedded SVG is exact',...structure});}
    const inkId=reopened.annotations.find(a=>a.kind==='ink').id,removed=call({pdf:saved,operations:[{kind:'remove_annotation',page:0,annotation:inkId}]},'remove');assert.equal(removed.ok,true);assert.equal(removed.image,plain.image);report.checks.push({name:'native save/reopen retains Ink annotation and editable SVG metadata; explicit removal restores original pixels'});
    const signedPlain=call({pdf:signed},'signed');assert.equal(signedPlain.ok,true);assert(signedPlain.signatures>0);const rejected=call({pdf:signed,operations:[inkOperation]},'signed-reject');assert.equal(rejected.error_code,'pdf_signed');assert.equal(call({pdf:signed,operations:[inkOperation],allow_signatures:true},'signed-explicit').ok,true);report.checks.push({name:'synthetic signature dictionary gates ink until explicit permission; no cryptographic signature certification'});
    assert.equal(sha(fs.readFileSync(original)),originalHash);assert.equal(sha(fs.readFileSync(args.library)),libraryHash);report.checks.push({name:'input PDF and borrowed library byte-identical; no engine installation or trust change'});report.passed=true;
  }catch(error){report.passed=false;report.error=String(error.message).replaceAll(args.output,'<evidence>').replaceAll(root,'<workspace>');throw error;}
  finally{await kernel?.stop();fs.writeFileSync(path.join(args.output,'report.json'),JSON.stringify(report,null,2)+'\n');console.log(JSON.stringify({passed:report.passed,checks:report.checks.length,report:path.relative(root,path.join(args.output,'report.json'))}));}
}
main().catch(error=>{console.error(String(error.message).slice(0,600));process.exitCode=1;});
