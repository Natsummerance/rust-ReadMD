'use strict';
// One owned, headless production-CSP flow for reversing a shared acceptance.
const fs=require('node:fs'),path=require('node:path'),assert=require('node:assert/strict');
const {ROOT,openOwned,until,sha}=require('./owned-browser-host.cjs');
async function run(){
  const binary=process.env.READMD_BIN,playwright=process.env.READMD_PLAYWRIGHT;
  assert(binary&&path.isAbsolute(binary),'Explicit READMD_BIN required');
  const output=path.resolve(process.env.READMD_COLLABORATION_OUTPUT||path.join(ROOT,'.cache','v007-collaboration-reversal-'+Date.now()));
  assert(output.startsWith(path.join(ROOT,'.cache')+path.sep));fs.mkdirSync(path.dirname(output),{recursive:true});
  const original='# Shared\n\nOriginal 🦀\n',accepted=original.replace('Original','Revised');
  const report={passed:false,headless:true,nativeOsAcceptance:false,checks:[],binarySha256:sha(fs.readFileSync(binary))};
  let owned,file,guest;
  const record=name=>{report.checks.push(name);console.log('PASS: '+name);};
  try{
    owned=await openOwned(output,async(_api,paths)=>{file=path.join(paths.documents,'Shared.md');fs.writeFileSync(file,original);},{binary,playwright});
    const page=owned.page;
    await page.evaluate(async file=>{await loadFile(file);await toggleEdit();await openShareModal();},file);
    await page.locator('.share-collaboration > summary').click();await page.locator('#collaboration-start').click();
    await until(()=>page.evaluate(()=>ReadMDCollaborationShare.bound()),'native binding');
    const fixture=await page.evaluate(async()=>{
      const source=await ReadMDCollaborationShare.request('snapshot'),at=source.content.indexOf('Original');
      const review=await ReadMDCollaborationShare.request('review',{text:'Decision to reverse',transaction:{base_revision:source.revision,origin:'review',edits:[{from:at,to:at+8,insert:'Revised'}]}});
      await ReadMDCollaborationShare.request('resolve',{id:review.id,accept:true});await ReadMDCollaborationShare.sync();
      const before=(await ReadMDCollaborationShare.request('reviews')).reviews.find(item=>item.id===review.id);
      const credential=(await ReadMDCollaborationShare.request('invite',{role:'editor',name:'Shared editor'})).credential;
      const url=new URL(document.getElementById('collaboration-open').href);url.hostname='127.0.0.1';url.hash=new URLSearchParams({room:credential.room,token:credential.token,lang:'en'});
      return {id:review.id,before,url:url.href,member:credential.member};
    });
    await until(()=>page.evaluate(accepted=>getEditContent()===accepted,accepted),'accepted native draft');
    await page.evaluate(()=>ReadMDModal.close(document.getElementById('share-modal')));await page.locator('#edit-save').click();
    await until(()=>Promise.resolve(fs.readFileSync(file,'utf8')===accepted),'accepted state saved');await until(()=>page.evaluate(()=>!state.editing),'save finished');
    const allowed=new Set([new URL(page.url()).origin,new URL(fixture.url).origin]);await owned.context.unroute('**/*');
    await owned.context.route('**/*',route=>allowed.has(new URL(route.request().url()).origin)?route.continue():route.abort());
    guest=await owned.context.newPage();guest.on('pageerror',error=>owned.errors.push(error.message));await guest.goto(fixture.url);
    await until(()=>guest.evaluate(()=>Boolean(window.ReadMDCollaboration)),'guest ready');
    await guest.locator('aside > details').first().evaluate(el=>el.open=true);
    const row=guest.locator('#reviews article').filter({hasText:'Decision to reverse'}),undo=row.locator('[data-review-action="reverse"]');
    await undo.waitFor();assert.equal(await undo.innerText(),'Undo');await undo.click();
    await until(()=>page.evaluate(original=>getActiveTab().content===original,original),'reversed draft reaches native');
    assert.equal(fs.readFileSync(file,'utf8'),accepted);
    const history=await page.evaluate(()=>ReadMDCollaborationShare.request('reviews')),item=history.reviews.find(value=>value.id===fixture.id);
    assert.deepEqual(item.body,fixture.before.body);assert.equal(item.status,'accepted');assert.equal(item.reversal.member,fixture.member);assert.equal(item.can_reverse,false);
    record('explicit shared Undo uses the stored inverse, keeps original decision and leaves disk at the last save');
    await guest.reload();await until(()=>guest.evaluate(()=>Boolean(window.ReadMDCollaboration)),'guest reload');
    await guest.locator('aside > details').first().evaluate(el=>el.open=true);await row.locator('[data-review-event="reverse"]').waitFor();assert.equal(await undo.count(),0);
    await page.evaluate(()=>openShareModal());await page.locator('#collaboration-review-panel').evaluate(el=>el.open=true);
    await page.locator('#collaboration-reviews [data-review-event="reverse"]').waitFor();
    record('web reload and native sharing panel display the separate reversal event without another Undo action');
    const later=original+'Independent later edit.\n';await guest.locator('#editor').fill(later);await guest.evaluate(()=>ReadMDCollaboration.sync());
    await until(()=>page.evaluate(later=>getActiveTab().content===later,later),'later edit synchronized');
    const retry=()=>guest.evaluate(async body=>{const params=new URLSearchParams(location.hash.slice(1));const r=await fetch('/sessions/'+params.get('room')+'/reverse',{method:'POST',headers:{'Content-Type':'application/json',Authorization:'Bearer '+params.get('token')},body:JSON.stringify(body)});return {status:r.status,data:await r.json()};},{id:fixture.id,generation:fixture.before.body._decision.result_generation,revision:fixture.before.body._decision.result_revision});
    const replay=await retry();assert.equal(replay.status,200);assert.equal(replay.data.replayed,true);assert.deepEqual(replay.data.reversal,item.reversal);
    assert.equal(await guest.locator('#editor').inputValue(),later);assert.equal(fs.readFileSync(file,'utf8'),accepted);
    record('retry acknowledges the same durable receipt and preserves later text');
    await page.evaluate(member=>ReadMDCollaborationShare.request('permission',{member,role:'reviewer'}),fixture.member);
    assert.equal((await retry()).status,403);await guest.evaluate(()=>ReadMDCollaboration.sync());
    assert.equal(owned.errors.length,0,owned.errors.join('\n'));report.passed=true;report.bootSha256=sha(fs.readFileSync(path.join(ROOT,'assets/readmd.boot.js')));
    await page.screenshot({path:path.join(output,'native-reversal-history.png')});record('replayed reversal still requires current editor permission');
  }catch(error){report.failure=String(error.message).replaceAll(output,'<owned>').slice(0,1500);if(owned)await owned.page.screenshot({path:path.join(output,'failure.png')}).catch(()=>{});throw error;}
  finally{if(owned){await owned.close();report.pageErrors=owned.errors;}fs.writeFileSync(path.join(output,'report.json'),JSON.stringify(report,null,2));console.log(JSON.stringify({passed:report.passed,checks:report.checks.length,report:path.join(output,'report.json')}));}
}
if(require.main===module)run().catch(error=>{console.error(error.stack);process.exitCode=1;});
