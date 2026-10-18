'use strict';
// Real Yjs clients against the standalone Rust HTTP service. This exercises
// transport, authorization and durable ACKs, not fifty native editor windows.
const fs=require('node:fs'),path=require('node:path'),crypto=require('node:crypto'),assert=require('node:assert/strict');
const {spawn}=require('node:child_process');
const Y=require('../tools/collaboration-bundle/node_modules/yjs');
const root=path.resolve(__dirname,'..'),binary=process.env.READMD_COLLAB_BIN||path.join(root,'.cache/community-upgrade/build/debug/readmd-collaboration.exe');
const base=fs.mkdtempSync(path.join(root,'.cache/capability-upgrade/collab-http-')),data=path.join(base,'service');
const port=Number(process.env.READMD_COLLAB_PORT||29072),url='http://127.0.0.1:'+port,admin=crypto.randomBytes(32).toString('hex');
const report={platform:process.platform,binarySha256:crypto.createHash('sha256').update(fs.readFileSync(binary)).digest('hex'),scope:'real HTTP service with 3/10/50 independent Yjs documents; no native editor or 8-hour certification',competingWorkload:process.env.READMD_COMPETING_TESTS||'unspecified',scenarios:[],passed:false};
let child,phase='startup';const rooms=[];
const wait=ms=>new Promise(resolve=>setTimeout(resolve,ms));
const bytes=value=>Buffer.from(value).toString('base64');
async function stop(){if(!child)return;const current=child;child=null;if(current.exitCode!==null)return;const closed=new Promise(resolve=>current.once('exit',resolve));current.kill();await Promise.race([closed,wait(5000).then(()=>{if(current.exitCode===null)throw Error('Service did not stop');})]);}
async function start(){
  child=spawn(binary,['--data-dir',data,'--assets',path.join(root,'assets'),'--listen','127.0.0.1:'+port],{windowsHide:true,env:{...process.env,READMD_COLLAB_ADMIN_TOKEN:admin},stdio:'ignore'});
  for(let i=0;i<100;i++){
    if(child.exitCode!==null)throw Error('Service startup failed');
    try{if((await fetch(url+'/',{signal:AbortSignal.timeout(500)})).ok)return;}catch{}
    await wait(100);
  }throw Error('Service startup timed out');
}
async function api(route,body,credential){
  const started=performance.now();
  const response=await fetch(url+route,{method:'POST',headers:{'Content-Type':'application/json',Authorization:'Bearer '+(credential?.token||admin)},body:JSON.stringify(body),signal:AbortSignal.timeout(30000)});
  return {status:response.status,data:await response.json(),ms:performance.now()-started};
}
async function sync(client,update,vector=Y.encodeStateVector(client.doc)){
  const response=await api('/sessions/'+client.credential.room+'/sync',{vector:bytes(vector),...(update?{update:bytes(update)}:{})},client.credential);
  assert.equal(response.status,200,'Sync must acknowledge an accepted update');
  Y.applyUpdate(client.doc,Buffer.from(response.data.update,'base64'),'remote');return response.ms;
}
(async()=>{try{
  await start();
  for(const count of [3,10,50]){
    phase=count+' clients';const started=performance.now(),timings=[];
    const created=await api('/sessions',{title:'Synthetic concurrency '+count,content:'# Shared evidence\n',name:'Host'});assert.equal(created.status,200);
    const owner=created.data.credential,route='/sessions/'+owner.room+'/',clients=[];
    for(let i=0;i<count;i++){
      const invitation=await api(route+'invite',{role:'editor',name:'Peer '+i},owner);assert.equal(invitation.status,200);
      const client={credential:invitation.data.credential,doc:new Y.Doc(),updates:[]};
      client.undo=new Y.UndoManager(client.doc.getText('content'),{trackedOrigins:new Set(['local']),captureTimeout:0});await sync(client);clients.push(client);
    }
    for(let round=0;round<4;round++){
      await Promise.all(clients.map(async(client,index)=>{
        const vector=Y.encodeStateVector(client.doc),text=client.doc.getText('content');
        client.doc.transact(()=>text.insert(text.length,`[peer-${index}/round-${round}] 🦀\n`),'local');
        const update=Y.encodeStateAsUpdate(client.doc,vector);client.updates.push(update);timings.push(await sync(client,update,vector));
      }));
    }
    const snapshot=(await api(route+'snapshot',{},owner)).data.content;
    for(let peer=0;peer<count;peer++)for(let round=0;round<4;round++)assert.equal(snapshot.split(`[peer-${peer}/round-${round}]`).length-1,1,'Every acknowledged edit occurs exactly once');
    await Promise.all(clients.map(client=>sync(client)));for(const client of clients)assert.equal(client.doc.getText('content').toString(),snapshot);
    // Replaying an acknowledged update must neither duplicate nor lose text.
    await Promise.all(clients.map(client=>sync(client,client.updates[0])));
    assert.equal((await api(route+'snapshot',{},owner)).data.content,snapshot);
    const first=clients[0],vector=Y.encodeStateVector(first.doc);first.undo.undo();await sync(first,Y.encodeStateAsUpdate(first.doc,vector),vector);
    const afterUndo=(await api(route+'snapshot',{},owner)).data.content;
    assert.ok(!afterUndo.includes('[peer-0/round-3]'));for(let peer=1;peer<count;peer++)assert.ok(afterUndo.includes(`[peer-${peer}/round-3]`),'Personal undo preserves every other author');
    // A reader can receive state but cannot submit even a valid CRDT delta.
    const reader=(await api(route+'invite',{role:'reader',name:'Reader'},owner)).data.credential;
    const beforeReader=afterUndo;assert.equal((await api(route+'sync',{vector:bytes(Y.encodeStateVector(new Y.Doc())),update:bytes(first.updates[0])},reader)).status,403);
    assert.equal((await api(route+'snapshot',{},owner)).data.content,beforeReader);
    assert.equal((await api(route+'permission',{member:first.credential.member,role:null},owner)).status,200);
    assert.equal((await api(route+'snapshot',{},first.credential)).status,403);
    rooms.push({owner,expected:afterUndo,revoked:first.credential});
    timings.sort((a,b)=>a-b);report.scenarios.push({clients:count,rounds:4,acknowledgedEdits:count*4,duplicateUpdates:count,personalUndo:true,readerWriteDenied:true,revokedAccessDenied:true,syncP95Ms:Math.round(timings[Math.floor((timings.length-1)*.95)]),elapsedMs:Math.round(performance.now()-started)});
    for(const client of clients){client.undo.destroy();client.doc.destroy();}
  }
  phase='out-of-order durable acknowledgement';
  for(const room of rooms){
    const offline={credential:room.owner,doc:new Y.Doc()};await sync(offline);
    const initial=Y.encodeStateVector(offline.doc),text=offline.doc.getText('content');text.insert(text.length,'[offline-first]');
    const first=Y.encodeStateAsUpdate(offline.doc,initial),afterFirst=Y.encodeStateVector(offline.doc);text.insert(text.length,'[offline-second]');
    const second=Y.encodeStateAsUpdate(offline.doc,afterFirst),route='/sessions/'+room.owner.room+'/';
    // Second depends on first: acknowledge it, then cross the real journal's
    // 128-update compaction threshold while the dependency is still missing.
    assert.equal((await api(route+'sync',{vector:bytes(initial),update:bytes(second)},room.owner)).status,200);
    for(let i=0;i<128;i++)assert.equal((await api(route+'sync',{vector:bytes(initial),update:bytes(second)},room.owner)).status,200);
    assert.equal((await api(route+'snapshot',{},room.owner)).data.content,room.expected);
    room.missing={vector:initial,update:first};offline.doc.destroy();
  }
  phase='restart durability';await stop();await start();
  for(const {owner,expected,revoked}of rooms){assert.equal((await api('/sessions/'+owner.room+'/snapshot',{},owner)).data.content,expected);assert.equal((await api('/sessions/'+owner.room+'/snapshot',{},revoked)).status,403);}
  phase='missing dependency after restart';
  for(const {owner,expected,missing}of rooms){
    const route='/sessions/'+owner.room+'/';assert.equal((await api(route+'sync',{vector:bytes(missing.vector),update:bytes(missing.update)},owner)).status,200);
    assert.equal((await api(route+'snapshot',{},owner)).data.content,expected+'[offline-first][offline-second]');
  }
  report.restartRetainsAcknowledgedContentAndRevocation=true;report.outOfOrderUpdatesSurviveCompactionAndRestart=true;report.pendingDuplicateDeliveriesPerRoom=128;report.passed=true;
}catch(error){report.failedStage=phase;report.error=error.name;report.message=String(error.message).slice(0,300);process.exitCode=1;}
finally{await stop();report.recordedAt=new Date().toISOString();fs.writeFileSync(path.join(base,'report.json'),JSON.stringify(report,null,2)+'\n');console.log(JSON.stringify({passed:report.passed,scenarios:report.scenarios.length,failedStage:report.failedStage,report:path.relative(root,path.join(base,'report.json'))}));}
})().catch(()=>{console.error('Concurrency check stopped; isolated evidence retained without credentials.');process.exitCode=1;});
