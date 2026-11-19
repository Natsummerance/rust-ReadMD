'use strict';
// Collaboration lives in the existing sharing panel and loads its CRDT client
// only when opened. Creating a session never writes the source document.
window.ReadMDCollaborationShare=(()=>{
  let active,loading,registry;
  const tr=(k,args)=>i18n.t(k,args),current=()=>getActiveTab();
  const content=tab=>((current()===tab&&state.editing)?getEditContent():(tab?.isDirty?tab.content:tab?.original)??tab?.content??'').replace(/\r\n?/g,'\n');
  const identity=tab=>tab?.path?normalizePath(tab.path):(tab?.recoveryKey||'draft:'+tab?.id);
  function binding(operation,key,value){
    registry||=new Promise((resolve,reject)=>{const r=indexedDB.open('readmd-native-collaboration',1);r.onupgradeneeded=()=>r.result.createObjectStore('bindings');r.onsuccess=()=>resolve(r.result);r.onerror=()=>reject(r.error);});
    return registry.then(db=>new Promise((resolve,reject)=>{const tx=db.transaction('bindings',operation==='get'||operation==='all'?'readonly':'readwrite'),store=tx.objectStore('bindings');const r=operation==='get'?store.get(key):operation==='all'?store.getAll():operation==='delete'?store.delete(key):store.put(value,key);let result;r.onsuccess=()=>result=r.result;tx.oncomplete=()=>resolve(result);tx.onerror=()=>reject(tx.error);tx.onabort=()=>reject(tx.error);}));
  }
  async function remember(target,enabled=true,discarded=false){
    const tab=state.tabs.find(tab=>tab.id===target.tabId);if(!tab)return;
    const key=identity(tab),old=target.bindingKey;
    await binding('put',key,{key,credential:target.credential,baseRevision:tab.revision||'',enabled,discarded,updated:Date.now()});
    target.bindingKey=key;if(old&&old!==key)await binding('delete',old);
  }
  async function request(operation,body={},target=active){
    const r=await apiFetch(['create','resume'].includes(operation)?'/api/collaboration/'+operation:'/api/collaboration/session',{method:'POST',headers:{'Content-Type':'application/json',...(operation!=='create'&&target?{'X-ReadMD-Collaboration-Token':target.credential.token}:{})},body:JSON.stringify({...body,...(operation==='create'?{}:{room:target.credential.room,operation})}),signal:AbortSignal.timeout(10000)});
    const data=await r.json();if(!r.ok||!data.ok){const error=Error(data.error_code||'collaboration_failed');error.status=r.status;throw error;}return data;
  }
  function libraries(){
    return loading||=(async()=>{for(const[name,url]of[['ReadMDCRDT','/assets/vendor/yjs.bundle.js'],['ReadMDCollaborationSession','/assets/collaboration/session.js'],['ReadMDCollaborationReviews','/assets/collaboration/reviews.js']])if(!window[name])await new Promise((resolve,reject)=>{const script=document.createElement('script');script.src=url;script.onload=()=>window[name]?resolve():reject(Error('collaboration_failed'));script.onerror=reject;document.head.append(script);});})().catch(error=>{loading=null;throw error;});
  }
  function link(){const url=new URL(active.url);url.hash=new URLSearchParams({room:active.credential.room,token:active.credential.token,lang:i18n.currentLang||'en'});return url.href;}
  function bound(tab=current()){return !!(active?.session&&tab?.id===active.tabId);}
  function status(key){const note=document.getElementById('collaboration-note');const readOnly=key==='collaboration.reader'||key==='collaboration.reviewer';if(note)note.textContent=(readOnly?tr('collaboration.role')+': ':'')+tr(key)+(readOnly&&active?.session?.pending()?' · '+tr('storage.draftKept'):'')+' · '+tr('collaboration.nativeSaveHint');if(active&&active.status!==key&&['collaboration.revoked','collaboration.storageError','collaboration.conflict'].includes(key))showToast(tr(key));if(active)active.status=key;}
  function syncStatus(target){const session=target.session;status(session.revoked()?'collaboration.revoked':!session.canEdit()?'collaboration.'+session.role():session.pending()?'collaboration.syncing':'collaboration.synced');}
  function selection(target){
    const Y=ReadMDCRDT,s=target.session,view=bound()&&state.editing?cmView:null;
    const range=view?.state.selection.main||{anchor:0,head:0};
    return {anchor:ReadMDCollaborationSession.b64(Y.encodeRelativePosition(Y.createRelativePositionFromTypeIndex(s.text,Math.min(range.anchor,s.text.length)))),head:ReadMDCollaborationSession.b64(Y.encodeRelativePosition(Y.createRelativePositionFromTypeIndex(s.text,Math.min(range.head,s.text.length))))};
  }
  function beforeChange(target){if(active===target&&bound()&&state.editing&&cmView)target.positions=selection(target);}
  function change(before,after){let from=0,end=0;while(from<before.length&&from<after.length&&before[from]===after[from])from++;if(from>0&&/[\uD800-\uDBFF]/.test(before[from-1]))from--;while(end<before.length-from&&end<after.length-from&&before[before.length-end-1]===after[after.length-end-1])end++;if(end>0&&/[\uDC00-\uDFFF]/.test(before[before.length-end]))end--;return {from,to:before.length-end,insert:after.slice(from,after.length-end)};}
  function applyText(target){
    if(active!==target||!target.ready)return;
    const tab=state.tabs.find(tab=>tab.id===target.tabId);if(!tab){void detach(null);return;}
    const after=target.session.text.toString(),before=content(tab);if(after===before)return;
    target.applying=true;
    try{
      if(bound()&&state.editing&&cmView){
        const CM=ReadMDCodeMirror,changes=change(before,after),positions=target.positions;let selection;
        if(positions){const a=ReadMDCRDT.createAbsolutePositionFromRelativePosition(ReadMDCRDT.decodeRelativePosition(ReadMDCollaborationSession.bytes(positions.anchor)),target.session.doc),h=ReadMDCRDT.createAbsolutePositionFromRelativePosition(ReadMDCRDT.decodeRelativePosition(ReadMDCollaborationSession.bytes(positions.head)),target.session.doc);if(a&&h)selection={anchor:a.index,head:h.index};}
        cmView.dispatch({changes,...(selection?{selection}:{}),annotations:CM.Transaction.addToHistory.of(false),userEvent:'collaboration.remote'});
      }else{
        tab.content=after;tab.fixed=after;tab.isDirty=after!==(tab.original??'').replace(/\r\n?/g,'\n');tab.editGeneration=(tab.editGeneration||0)+1;
        // Cached editor state predates these remote changes and its undo log.
        tab.editorState=null;tab.editorCompartments=null;
        if(current()===tab){state.fixed=after;void renderActiveTab({restoreScroll:true});}
        renderTabsBar();window.ReadMDRecovery?.schedule();updateUnloadGuard();
      }
    }finally{target.applying=false;target.positions=null;}
  }
  async function synchronize(target=active){
    if(!target?.session||!target.ready||target.stopped||target.syncing)return;
    target.syncing=true;
    try{const data=await target.session.sync();if(active!==target||!data)return;syncStatus(target);target.reviews?.permissions();const signature=JSON.stringify(data.peers);if(signature!==target.peerSignature){target.peerSignature=signature;target.peers=data.peers;target.member=data.member;target.generation=(target.generation||0)+1;if(bound()&&state.editing&&cmView)cmView.dispatch({});}}
    catch{if(active!==target)return;if(target.session.revoked()){clearInterval(target.timer);clearInterval(target.reviewTimer);target.reviews?.permissions();target.peers=[];target.generation=(target.generation||0)+1;document.getElementById('collaboration-open').hidden=true;if(bound()&&state.editing&&cmView)cmView.dispatch({});}if(!target.session.stopped())status(target.session.revoked()?'collaboration.revoked':'collaboration.offline');}
    finally{target.syncing=false;}
  }
  async function start(candidate){
    const button=document.getElementById('collaboration-start'),tab=state.tabs.includes(candidate)?candidate:current();if(active||!tab)return false;button.disabled=true;
    const original=content(tab),valid=()=>state.tabs.includes(tab)&&content(tab)===original;let target,record,denied;
    try{
      record=await binding('get',identity(tab));await libraries();if(!valid())throw Error('transaction_conflict');
      if(record){
        if(tab.externalChanged||(tab.revision||'')!==record.baseRevision)throw Error('transaction_conflict');
        try{const resumed=await request('resume',{},record);target={...resumed,credential:record.credential,bindingKey:record.key};}
        catch(error){if(error.status!==403)throw error;denied=error;target={credential:record.credential,bindingKey:record.key};}
      }
      else target=await request('create',{title:tab.name||'ReadMD.md',content:original});target.tabId=tab.id;
      if(!valid())throw Error('transaction_conflict');
      active=target;
      target.session=await ReadMDCollaborationSession.open({Y:ReadMDCRDT,...target.credential,reset:!!record?.discarded,api:(op,body)=>request(op,body,target),presence:()=>selection(target),onBeforeChange:()=>beforeChange(target),canApplyRemote:()=>!(bound()&&state.editing&&(cmView?.composing||cmView?.compositionStarted)),onStorageFailure:()=>{target.stopped=true;status('collaboration.storageError');}});
      if(record&&tab.isDirty&&target.session.text.toString()!==original)throw Error('transaction_conflict');
      target.session.setMetadata({role:record?target.snapshot?.role:'owner',title:tab.name});
      // Bootstrap the authoritative CRDT without inserting a duplicate copy.
      if(denied)await target.session.revoke();
      else try{await target.session.sync();}catch(error){if(!record||!target.session.revoked())throw error;}
      if(!valid())throw Error('transaction_conflict');
      if(target.session.revoked()&&!target.session.hasLocalCopy())throw denied||Object.assign(Error('collaboration_forbidden'),{status:403});
      if(!record&&target.session.text.toString()!==original)throw Error('transaction_conflict');
      target.ready=true;target.session.text.observe(event=>{if(event.transaction.origin!==target.session.local)applyText(target);});
      await remember(target);applyText(target);syncStatus(target);
      const reviewPanel=document.getElementById('collaboration-review-panel');reviewPanel.hidden=false;
      target.reviews=ReadMDCollaborationReviews.mount({root:document.getElementById('collaboration-reviews'),api:(op,body)=>request(op,body,target),session:target.session,tr,flush:async()=>{if(active!==target||!target.ready)throw Error('changed');const data=await target.session.sync();if(!data||target.session.pending())throw Error('changed');syncStatus(target);},getContent:()=>content(tab),getSelection:()=>bound()&&state.editing&&cmView?{from:cmView.state.selection.main.from,to:cmView.state.selection.main.to}:null});
      if(!target.session.revoked()){const open=document.getElementById('collaboration-open');open.href=link();open.hidden=false;target.timer=setInterval(()=>synchronize(target),500);target.reviewTimer=setInterval(()=>target.reviews.refresh(),4000);}
      return true;
    }catch(error){if(target){await target.session?.dispose().catch(()=>{});target.session=null;target.ready=false;if(!record){await request('close',{},target).catch(()=>{});await binding('delete',target.bindingKey||identity(tab)).catch(()=>{});}}if(active===target)active=null;status(error.message==='transaction_conflict'?'collaboration.conflict':error.status===403?'collaboration.revoked':'collaboration.failed');button.disabled=!!active;return false;}
  }
  function onEditorUpdate(update){
    const target=active;if(!bound()||!target.ready||target.applying)return;
    if(!update.docChanged)return;
    if(target.stopped||!target.session.canEdit())return;
    const before=update.startState.doc.toString();
    if(before!==target.session.text.toString()){target.stopped=true;status('collaboration.conflict');return;}
    const changes=[];update.changes.iterChanges((from,to,_a,_b,insert)=>changes.push({from,to,insert:insert.toString()}));
    const isolated=update.transactions.some(tx=>!tx.isUserEvent('input.type')&&!tx.isUserEvent('delete.backward')&&!tx.isUserEvent('delete.forward'));
    target.session.applyChanges(changes,isolated);status('collaboration.syncing');
  }
  function history(redo=false){if(!bound())return false;const target=active;if(!target.ready||target.stopped||!target.session.canEdit())return true;redo?target.session.undo.redo():target.session.undo.undo();void synchronize();return true;}
  async function detach(tab=current(),discarded=false){
    const target=active;if(!target||(tab&&tab.id!==target.tabId))return true;
    clearInterval(target.timer);clearInterval(target.reviewTimer);target.reviews?.dispose();document.getElementById('collaboration-review-panel').hidden=true;target.ready=false;active=null;if(state.editing&&cmView)cmView.dispatch({});
    await remember(target,false,discarded).catch(()=>{});
    await target.session?.dispose().catch(()=>{});
    document.getElementById('collaboration-start').disabled=false;document.getElementById('collaboration-open').hidden=true;document.getElementById('collaboration-note').textContent='';return true;
  }
  async function end(){const target=active;if(!target)return true;await synchronize(target);const tab=state.tabs.find(tab=>tab.id===target.tabId);if(target.session?.revoked()?tab?.isDirty:target.session?.pending()){syncStatus(target);return false;}try{if(!target.session?.revoked())await request('close');await detach(tab);if(target.bindingKey)await binding('delete',target.bindingKey);return true;}catch{status('collaboration.failed');return false;}}
  async function prepareClose(){const target=active;if(!target?.session)return true;try{await target.session.checkpoint(true);await remember(target);await synchronize(target);await target.session.checkpoint(true);return true;}catch{status('collaboration.storageError');return false;}}
  async function documentSaved(tab){if(bound(tab))try{await remember(active);}catch{active.stopped=true;status('collaboration.storageError');}}
  async function restore(){try{if(active)return;const records=await binding('all'),record=records.filter(r=>r.enabled).sort((a,b)=>b.updated-a.updated).find(r=>state.tabs.some(tab=>identity(tab)===r.key));if(record){const tab=state.tabs.find(tab=>identity(tab)===record.key);await start(tab);}}catch{/* Basic reading is independent of optional collaboration storage. */}}
  function editorExtension(CM){
    class Cursor extends CM.WidgetType {
      constructor(name,color){super();this.name=name;this.color=color;}
      eq(other){return other.name===this.name&&other.color===this.color;}
      toDOM(){const marker=document.createElement('span');marker.className='collaboration-cursor';marker.dataset.name=this.name;marker.title=this.name;marker.setAttribute('aria-label',this.name);marker.style.setProperty('--peer-color',this.color);return marker;}
      ignoreEvent(){return true;}
    }
    function decorations(){
      if(!bound()||!active.ready)return CM.Decoration.none;
      const ranges=[];for(const peer of active.peers||[]){if(peer.member===active.member)continue;try{
        const position=key=>ReadMDCRDT.createAbsolutePositionFromRelativePosition(ReadMDCRDT.decodeRelativePosition(ReadMDCollaborationSession.bytes(peer.selection[key])),active.session.doc)?.index;
        const anchor=position('anchor'),head=position('head');if(!Number.isInteger(head)||head>cmView.state.doc.length)continue;
        const color='hsl('+Math.abs([...peer.member].reduce((n,c)=>n*31+c.charCodeAt(0)|0,0))%360+' 65% 48%)';
        if(Number.isInteger(anchor)&&anchor!==head&&anchor<=cmView.state.doc.length)ranges.push(CM.Decoration.mark({class:'collaboration-selection',attributes:{style:'--peer-color:'+color}}).range(Math.min(anchor,head),Math.max(anchor,head)));
        ranges.push(CM.Decoration.widget({widget:new Cursor(peer.name||tr('collaboration.'+peer.role),color),side:1}).range(head));
      }catch{/* A peer can leave between a presence snapshot and rendering. */}}
      return CM.Decoration.set(ranges,true);
    }
    return CM.ViewPlugin.fromClass(class {
      constructor(){this.generation=-1;this.decorations=CM.Decoration.none;}
      update(update){const generation=active?.generation??-1;if(update.docChanged||update.viewportChanged||generation!==this.generation){this.generation=generation;this.decorations=decorations();}}
    },{decorations:plugin=>plugin.decorations});
  }
  function bind(){
    const panel=document.querySelector('#share-modal .share-body'),details=document.createElement('details');details.className='share-collaboration';
    const summary=document.createElement('summary');summary.dataset.i18n='collaboration.title';summary.textContent=tr('collaboration.title');
    const startButton=document.createElement('button');startButton.id='collaboration-start';startButton.className='tb-btn';startButton.type='button';startButton.dataset.i18n='collaboration.start';startButton.textContent=tr('collaboration.start');startButton.onclick=start;
    const open=document.createElement('a');open.id='collaboration-open';open.className='tb-btn';open.hidden=true;open.dataset.i18n='collaboration.open';open.textContent=tr('collaboration.open');open.target='_blank';open.rel='noopener noreferrer';open.onclick=e=>{if(window.py?.open_external){e.preventDefault();py.open_external(open.href);}};
    const close=document.createElement('button');close.type='button';close.id='collaboration-end';close.className='tb-btn';close.dataset.i18n='collaboration.close';close.textContent=tr('collaboration.close');close.onclick=end;
    const note=document.createElement('p');note.id='collaboration-note';note.setAttribute('role','status');
    const reviewPanel=document.createElement('details');reviewPanel.id='collaboration-review-panel';reviewPanel.hidden=true;const reviewSummary=document.createElement('summary');reviewSummary.dataset.i18n='collaboration.review';reviewSummary.textContent=tr('collaboration.review');const reviews=document.createElement('div');reviews.id='collaboration-reviews';reviewPanel.append(reviewSummary,reviews);
    details.append(summary,startButton,open,close,note,reviewPanel);panel.append(details);
  }
  if(document.readyState==='loading')document.addEventListener('DOMContentLoaded',bind);else bind();
  addEventListener('online',()=>synchronize());
  addEventListener('readmd:language-changed',()=>{if(active){if(active.status)status(active.status);active.reviews?.language();if(active.url&&!active.session?.revoked())document.getElementById('collaboration-open').href=link();active.generation=(active.generation||0)+1;if(bound()&&state.editing&&cmView)cmView.dispatch({});}});
  addEventListener('beforeunload',event=>{if(active?.session?.pending()||active?.session?.pendingReviews()){event.preventDefault();event.returnValue='';}});
  return {start,request,end,detach,bound,onEditorUpdate,history,editorExtension,restore,documentSaved,sync:synchronize,prepareClose,canEdit:()=>!bound()||(!active.stopped&&active.session.canEdit()),pending:()=>active?.session?.pending()||0};
})();
