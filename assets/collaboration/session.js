'use strict';
// Shared CRDT transport and durable delta journal for the web and native editor.
// UI adapters own selection/rendering; the Rust service owns roles and commits.
window.ReadMDCollaborationSession=(()=>{
  const b64=bytes=>{let value='';for(let i=0;i<bytes.length;i+=8192)value+=String.fromCharCode(...bytes.subarray(i,i+8192));return btoa(value);};
  const bytes=value=>Uint8Array.from(atob(value),c=>c.charCodeAt(0));
  async function open({Y,room,token,api,presence,onStorageFailure,onBeforeChange,canApplyRemote,reset=false}){
    const storage=await new Promise((resolve,reject)=>{const r=indexedDB.open('readmd-collaboration',2);r.onupgradeneeded=()=>{if(!r.result.objectStoreNames.contains('sessions'))r.result.createObjectStore('sessions');if(!r.result.objectStoreNames.contains('events')){const events=r.result.createObjectStore('events',{keyPath:'id',autoIncrement:true});events.createIndex('session','session');}};r.onsuccess=()=>resolve(r.result);r.onerror=()=>reject(r.error);});
    const key=room+':'+token.slice(0,12);
    if(reset)await new Promise((resolve,reject)=>{const tx=storage.transaction(['sessions','events'],'readwrite');tx.objectStore('sessions').delete(key);const cursor=tx.objectStore('events').index('session').openCursor(IDBKeyRange.only(key));cursor.onsuccess=()=>{const item=cursor.result;if(item){item.delete();item.continue();}};tx.oncomplete=resolve;tx.onerror=()=>reject(tx.error);tx.onabort=()=>reject(tx.error);});
    const read=(store,request)=>new Promise((resolve,reject)=>{const r=request(storage.transaction(store).objectStore(store));r.onsuccess=()=>resolve(r.result);r.onerror=()=>reject(r.error);});
    const saved=await read('sessions',store=>store.get(key)),events=await read('events',store=>store.index('session').getAll(key));
    const doc=new Y.Doc(),text=doc.getText('content'),local=Symbol('local'),remote=Symbol('remote');
    if(saved?.document)Y.applyUpdate(doc,saved.document,remote);
    for(const event of events)Y.applyUpdate(doc,event.update,remote);
    let queue=[...(saved?.pendingUpdates||[...(saved?.pending?[saved.pending]:[])]).map(update=>({update,checkpointed:true})),...events.filter(e=>e.pending)];
    let role=saved?.role||'reader',title=saved?.title||'ReadMD.md',persist=Promise.resolve(),journalCount=events.length,stopped=false,inFlight=null,disposed=false;
    const undo=new Y.UndoManager(text,{trackedOrigins:new Set([local]),captureTimeout:450});
    function storageFailure(){stopped=true;onStorageFailure?.();}
    function checkpoint(force=false){
      if(!force&&journalCount<128)return persist;
      const pending=queue.slice(),value={document:Y.encodeStateAsUpdate(doc),pendingUpdates:pending.map(e=>e.update),role,title,updated:Date.now()};journalCount=0;
      persist=persist.then(()=>new Promise((resolve,reject)=>{const tx=storage.transaction(['sessions','events'],'readwrite');tx.objectStore('sessions').put(value,key);const cursor=tx.objectStore('events').index('session').openCursor(IDBKeyRange.only(key));cursor.onsuccess=()=>{const item=cursor.result;if(item){item.delete();item.continue();}};tx.oncomplete=()=>{pending.forEach(e=>e.checkpointed=true);resolve();};tx.onerror=()=>reject(tx.error);tx.onabort=()=>reject(tx.error);}));
      persist.catch(storageFailure);return persist;
    }
    function append(update,pending){
      const event={session:key,update,pending};if(pending)queue.push(event);journalCount++;
      persist=persist.then(()=>new Promise((resolve,reject)=>{const tx=storage.transaction('events','readwrite'),r=tx.objectStore('events').add(event);r.onsuccess=()=>event.id=r.result;tx.oncomplete=resolve;tx.onerror=()=>reject(tx.error);tx.onabort=()=>reject(tx.error);}));persist.catch(storageFailure);checkpoint();
    }
    function acknowledge(sent){
      const accepted=queue.splice(0,sent),remaining=queue.slice();
      persist=persist.then(()=>new Promise((resolve,reject)=>{const tx=storage.transaction(['sessions','events'],'readwrite'),session=tx.objectStore('sessions').get(key);session.onsuccess=()=>{const value=session.result||{};delete value.pending;value.pendingUpdates=remaining.filter(e=>e.checkpointed).map(e=>e.update);value.role=role;value.title=title;value.updated=Date.now();tx.objectStore('sessions').put(value,key);};for(const event of accepted){if(event.id&&!event.checkpointed){const r=tx.objectStore('events').get(event.id);r.onsuccess=()=>{if(r.result){r.result.pending=false;tx.objectStore('events').put(r.result);}};}}tx.oncomplete=resolve;tx.onerror=()=>reject(tx.error);tx.onabort=()=>reject(tx.error);}));persist.catch(storageFailure);return persist;
    }
    const updated=(update,origin)=>append(update,origin!==remote);
    const before=transaction=>{if(transaction.origin!==local)onBeforeChange?.(transaction);};
    doc.on('update',updated);doc.on('beforeTransaction',before);
    function sync(){
      if(disposed||stopped)return Promise.resolve(null);if(inFlight)return inFlight;
      inFlight=(async()=>{
        let sent=0,size=0;for(const event of queue){if(sent&&size+event.update.length>512*1024)break;size+=event.update.length;sent++;}
        await persist;
        try{
          const data=await api('sync',{vector:b64(Y.encodeStateVector(doc)),...(sent?{update:b64(Y.mergeUpdates(queue.slice(0,sent).map(e=>e.update)))}:{}),...(presence?{presence:presence()}:{} )});
          if(disposed)return null;
          role=data.role;if(sent)await acknowledge(sent);if(disposed)return null;
          // An in-flight response can arrive after compositionstart. Keep the
          // local vector unchanged; the durable server resends it after IME.
          if(!canApplyRemote||canApplyRemote())Y.applyUpdate(doc,bytes(data.update),remote);
          await checkpoint();return data;
        }catch(error){if(error.status===403)stopped=true;throw error;}
      })().finally(()=>inFlight=null);return inFlight;
    }
    // UTF-16 ChangeSet coordinates are applied from right to left. Large paste
    // chunks remain one personal undo group while network updates stay bounded.
    function applyChanges(changes,isolated=false){
      if(stopped||disposed||!['owner','editor'].includes(role))return false;
      if(isolated)undo.stopCapturing();
      for(const change of [...changes].reverse()){
        if(change.to>change.from)doc.transact(()=>text.delete(change.from,change.to-change.from),local);
        for(let i=0;i<change.insert.length;){let end=Math.min(i+65536,change.insert.length);if(end<change.insert.length&&/[\uD800-\uDBFF]/.test(change.insert[end-1]))end--;const chunk=change.insert.slice(i,end),at=change.from+i;doc.transact(()=>text.insert(at,chunk),local);i=end;}
      }
      if(isolated)undo.stopCapturing();return true;
    }
    async function dispose(){if(disposed)return;await checkpoint(true);disposed=true;doc.off('update',updated);doc.off('beforeTransaction',before);undo.destroy();doc.destroy();storage.close();}
    return {doc,text,undo,local,remote,sync,checkpoint,applyChanges,dispose,pending:()=>queue.length,role:()=>role,stopped:()=>stopped,setMetadata:value=>{if(value.role)role=value.role;if(value.title)title=value.title;}};
  }
  return {open,b64,bytes};
})();
