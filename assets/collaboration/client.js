'use strict';
(async()=>{
  const theme=matchMedia('(prefers-color-scheme: dark)'),applyTheme=()=>document.body.dataset.theme=theme.matches?'dark':'light';applyTheme();theme.addEventListener('change',applyTheme);
  const Y=ReadMDCRDT,$=id=>document.getElementById(id),fragment=new URLSearchParams(location.hash.slice(1));
  const room=fragment.get('room'),token=fragment.get('token');let locale=fragment.get('lang')||'en';
  // Credentials stay in the URL fragment (never HTTP logs/Referer) and local
  // session storage. Neither tokens nor document contents enter console logs.
  if(!/^[0-9a-f-]{36}$/.test(room||'')||!/^[0-9a-f]{64}$/.test(token||''))return;
  if(!/^[a-z]{2}(?:-[A-Z]{2})?$/.test(locale))locale='en';
  let response=await fetch('/i18n/'+encodeURIComponent(locale)+'.json');if(!response.ok){locale='en';response=await fetch('/i18n/en.json');}const labels=await response.json();
  const tr=(key,args={})=>(labels[key]||key).replace(/\{(\w+)\}/g,(_,k)=>args[k]??'{'+k+'}');
  document.documentElement.lang=locale;if(['ar','he','ug'].includes(locale))document.documentElement.dir='rtl';
  document.querySelectorAll('[data-i18n]').forEach(el=>el.textContent=tr(el.dataset.i18n));
  document.querySelectorAll('[data-i18n-placeholder]').forEach(el=>el.placeholder=tr(el.dataset.i18nPlaceholder));
  document.querySelectorAll('[data-i18n-aria]').forEach(el=>el.setAttribute('aria-label',tr(el.dataset.i18nAria)));
  const b64=bytes=>{let s='';for(let i=0;i<bytes.length;i+=8192)s+=String.fromCharCode(...bytes.subarray(i,i+8192));return btoa(s);};
  const bytes=s=>Uint8Array.from(atob(s),c=>c.charCodeAt(0));
  const api=async(operation,body={})=>{
    const r=await fetch('/sessions/'+room+'/'+operation,{method:'POST',headers:{'Content-Type':'application/json',Authorization:'Bearer '+token},body:JSON.stringify(body),signal:AbortSignal.timeout(10000)});
    const data=await r.json();if(!r.ok||!data.ok){const error=Error(data.error_code||'collaboration_failed');error.status=r.status;throw error;}return data;
  };
  let role='reader',title='ReadMD.md',member,composing=false,stopped=false,peers=[];
  function storageFailure(){stopped=true;$('status').textContent=tr('collaboration.storageError');$('editor').disabled=true;}
  const session=await ReadMDCollaborationSession.open({Y,room,token,api,presence:()=>selection(),onStorageFailure:storageFailure,canApplyRemote:()=>!composing});
  const {doc,text,local,remote,undo}=session;role=session.role();
  const checkpoint=force=>session.checkpoint(force);
  function editable(){return !stopped&&['owner','editor'].includes(role);}
  const editor=$('editor');editor.value=text.toString();editor.disabled=false;editor.readOnly=!editable();
  function selection(){return{anchor:b64(Y.encodeRelativePosition(Y.createRelativePositionFromTypeIndex(text,editor.selectionStart))),head:b64(Y.encodeRelativePosition(Y.createRelativePositionFromTypeIndex(text,editor.selectionEnd)))};}
  function restoreSelection(position){try{const a=Y.createAbsolutePositionFromRelativePosition(Y.decodeRelativePosition(bytes(position.anchor)),doc),h=Y.createAbsolutePositionFromRelativePosition(Y.decodeRelativePosition(bytes(position.head)),doc);if(a&&h)editor.setSelectionRange(a.index,h.index);}catch{}}
  let positions=selection(),handlingInput=false;
  text.observe(()=>{if(handlingInput)return;const value=text.toString();if(editor.value!==value){editor.value=value;restoreSelection(positions);}});
  function input(){
    if(composing||!editable())return;
    const before=text.toString(),after=editor.value,selectionStart=editor.selectionStart,selectionEnd=editor.selectionEnd;let start=0,end=0;
    while(start<before.length&&start<after.length&&before[start]===after[start])start++;
    // UTF-16 boundaries cannot split a surrogate pair.
    if(start>0&&/[\uD800-\uDBFF]/.test(before[start-1]))start--;
    while(end<before.length-start&&end<after.length-start&&before[before.length-end-1]===after[after.length-end-1])end++;
    if(end>0&&/[\uDC00-\uDFFF]/.test(before[before.length-end]))end--;
    const removed=before.length-start-end,added=after.slice(start,after.length-end);
    if(added.length>65536)undo.stopCapturing();handlingInput=true;
    try{
    if(removed)doc.transact(()=>text.delete(start,removed),local);
    // Bound each CRDT delta, including a large paste made while offline. Keep
    // one personal undo group; checkpoints retain separate unsent deltas.
    for(let i=0;i<added.length;){let next=Math.min(i+65536,added.length);if(next<added.length&&/[\uD800-\uDBFF]/.test(added[next-1]))next--;const chunk=added.slice(i,next),at=start+i;doc.transact(()=>text.insert(at,chunk),local);i=next;}
    }finally{handlingInput=false;}
    if(added.length>65536)undo.stopCapturing();
    editor.setSelectionRange(selectionStart,selectionEnd);positions=selection();
  }
  editor.addEventListener('input',input);editor.addEventListener('compositionstart',()=>composing=true);editor.addEventListener('compositionend',()=>{composing=false;input();});
  editor.addEventListener('select',()=>positions=selection());
  editor.addEventListener('keydown',e=>{
    if(e.isComposing||composing||!editable())return;
    if((e.ctrlKey||e.metaKey)&&['z','y'].includes(e.key.toLowerCase())){e.preventDefault();e.key.toLowerCase()==='y'||e.shiftKey?undo.redo():undo.undo();}
    if((e.ctrlKey||e.metaKey)&&e.key.toLowerCase()==='s'){e.preventDefault();download();}
  });
  async function sync(){
    if(stopped||composing)return;
    try{
      const data=await session.sync();if(!data)return;
      role=data.role;member=data.member;
      editor.disabled=stopped;editor.readOnly=!editable();$('status').textContent=tr(session.pending()?'collaboration.syncing':'collaboration.synced');$('sharing').hidden=role!=='owner';
      $('post').disabled=role==='reader';$('propose').disabled=role==='reader';$('add-file').hidden=!editable();
      const list=$('peers');list.replaceChildren();peers=data.peers;for(const peer of peers){const row=document.createElement('div');row.textContent=(peer.name||tr('collaboration.'+peer.role))+(peer.member===member?' · '+tr('collaboration.you'):'');
        try{const p=Y.createAbsolutePositionFromRelativePosition(Y.decodeRelativePosition(bytes(peer.selection.head)),doc);if(p)row.textContent+=' · '+tr('editor.lineCol',{line:1+(text.toString().slice(0,p.index).match(/\n/g)||[]).length,col:1+p.index-(text.toString().lastIndexOf('\n',Math.max(0,p.index-1))+1)});}catch{}list.append(row);}drawCursors();
    }catch(error){$('status').textContent=tr(error.status===403?'collaboration.revoked':'collaboration.offline');if(error.status===403){stopped=true;editor.disabled=true;}}
  }
  function download(){const url=URL.createObjectURL(new Blob([text.toString()],{type:'text/markdown;charset=utf-8'})),a=document.createElement('a');a.href=url;a.download=(title.replace(/[\\/:*?"<>|]/g,'_').slice(0,120)||'ReadMD')+(title.endsWith('.md')?'':'.md');a.click();setTimeout(()=>URL.revokeObjectURL(url),1000);}
  $('download').onclick=download;
  async function reviews(){try{const data=await api('reviews');$('reviews').replaceChildren();for(const item of data.reviews){const row=document.createElement('article'),by=document.createElement('small');row.textContent=item.body.text;by.textContent=item.name+' · '+tr('collaboration.'+(item.status==='open'?'openReview':item.status));row.append(by);
    if(item.body.transaction){const diff=document.createElement('div');diff.className='review-diff';item.body.transaction.edits.forEach((edit,i)=>{const before=document.createElement('del'),after=document.createElement('ins');before.textContent=item.body._anchors?.[i]?.original||'';after.textContent=edit.insert;diff.append(before,after);});row.append(diff);}
    if(item.status==='open'&&editable())for(const accept of [true,false]){const b=document.createElement('button');b.textContent=tr(accept?'collaboration.accept':'collaboration.reject');b.onclick=async()=>{try{await api('resolve',{id:item.id,accept});await sync();await reviews();}catch{$('status').textContent=tr('collaboration.conflict');}};row.append(b);}$('reviews').append(row);}}catch{}}
  $('post').onclick=async()=>{if(!$('comment').value.trim())return;try{await api('review',{text:$('comment').value,selection:selection()});$('comment').value='';await reviews();}catch{$('status').textContent=tr('collaboration.failed');}};
  $('propose').onclick=async()=>{const from=editor.selectionStart,to=editor.selectionEnd,source=text.toString(),insert=$('replacement').value;if(from===to&&!insert)return;
    try{await sync();const snapshot=await api('snapshot');if(source!==text.toString()||snapshot.content!==source)throw Error('changed');await api('review',{text:$('comment').value.trim()||tr('collaboration.suggest'),transaction:{base_revision:snapshot.revision,origin:'review',edits:[{from,to,insert}]}});$('replacement').value='';await reviews();}catch{$('status').textContent=tr('collaboration.conflict');}};
  async function attachments(){try{const data=await api('attachments');$('attachments').replaceChildren();for(const item of data.attachments){const b=document.createElement('button');b.textContent=item.name+' · '+Math.ceil(item.size/1024)+' KB';b.onclick=async()=>{try{const r=await fetch('/sessions/'+room+'/download',{method:'POST',headers:{'Content-Type':'application/json',Authorization:'Bearer '+token},body:JSON.stringify({id:item.id})});if(!r.ok)throw Error('download');const url=URL.createObjectURL(await r.blob()),a=document.createElement('a');a.href=url;a.download=item.name.replace(/[\\/:*?"<>|]/g,'_');a.click();setTimeout(()=>URL.revokeObjectURL(url),1000);}catch{$('status').textContent=tr('collaboration.failed');}};$('attachments').append(b);}}catch{}}
  $('add-file').onclick=()=>$('upload').click();$('upload').onchange=async()=>{const file=$('upload').files[0];if(!file)return;$('add-file').disabled=true;try{if(file.size>16*1024*1024)throw Error('size');const r=await fetch('/sessions/'+room+'/attachment',{method:'POST',headers:{'Content-Type':file.type||'application/octet-stream',Authorization:'Bearer '+token,'X-Asset-Name-Base64':b64(new TextEncoder().encode(file.name))},body:file});if(!r.ok)throw Error('upload');await attachments();}catch{$('status').textContent=tr('collaboration.failed');}finally{$('upload').value='';$('add-file').disabled=false;}};
  async function members(){if(role!=='owner')return;try{const data=await api('members');$('members').replaceChildren();for(const item of data.members){const row=document.createElement('article'),name=document.createElement('span');name.textContent=item.name||tr('collaboration.'+item.role);row.append(name);if(item.role!=='owner'){const select=document.createElement('select');select.setAttribute('aria-label',tr('collaboration.role'));for(const value of ['editor','reviewer','reader'])select.add(new Option(tr('collaboration.'+value),value));select.value=item.role;select.onchange=async()=>{try{await api('permission',{member:item.member,role:select.value});}catch{$('status').textContent=tr('collaboration.failed');}await members();};const remove=document.createElement('button');remove.textContent=tr('collaboration.revoke');remove.onclick=async()=>{try{await api('permission',{member:item.member,role:null});await members();}catch{$('status').textContent=tr('collaboration.failed');}};row.append(select,remove);}$('members').append(row);}}catch{}}
  $('invite').onclick=async()=>{try{const data=await api('invite',{role:$('role').value,name:$('name').value});const link=new URL(location.href);link.hash=new URLSearchParams({room,token:data.credential.token,lang:locale});$('link').value=link.href;$('link').hidden=false;$('link').select();await members();try{await navigator.clipboard.writeText(link.href);}catch{document.execCommand('copy');}}catch{$('status').textContent=tr('collaboration.failed');}};
  $('close').onclick=async()=>{try{await api('close');stopped=true;editor.disabled=true;$('status').textContent=tr('collaboration.revoked');}catch{$('status').textContent=tr('collaboration.failed');}};
  try{const data=await api('snapshot');title=data.title;role=data.role;session.setMetadata(data);$('title').textContent=title;await checkpoint(true);}catch{$('status').textContent=tr('collaboration.offline');}
  function drawCursors(){const mirror=$('cursors'),value=text.toString(),points=[];mirror.style.width=editor.clientWidth+'px';for(const peer of peers){if(peer.member===member)continue;try{const p=Y.createAbsolutePositionFromRelativePosition(Y.decodeRelativePosition(bytes(peer.selection.head)),doc);if(p&&p.index<=value.length)points.push({at:p.index,name:peer.name||tr('collaboration.'+peer.role),color:Math.abs([...peer.member].reduce((n,c)=>n*31+c.charCodeAt(0)|0,0))%360});}catch{}}points.sort((a,b)=>a.at-b.at);mirror.replaceChildren();let at=0;for(const p of points){mirror.append(document.createTextNode(value.slice(at,p.at)));const marker=document.createElement('span');marker.className='remote-cursor';marker.dataset.name=p.name;marker.style.setProperty('--peer-color','hsl('+p.color+' 65% 48%)');mirror.append(marker);at=p.at;}mirror.append(document.createTextNode(value.slice(at)||' '));mirror.scrollTop=editor.scrollTop;mirror.scrollLeft=editor.scrollLeft;}
  new ResizeObserver(drawCursors).observe(editor);
  editor.addEventListener('scroll',()=>{$('cursors').scrollTop=editor.scrollTop;$('cursors').scrollLeft=editor.scrollLeft;});
  await sync();await reviews();await attachments();await members();setInterval(sync,500);setInterval(()=>{reviews();attachments();members();},4000);
  addEventListener('online',sync);addEventListener('beforeunload',e=>{if(session.pending()){e.preventDefault();e.returnValue='';}});
  // Diagnostics expose state, never credentials; used by native browser tests.
  window.ReadMDCollaboration={doc,text,undo,sync,checkpoint,pending:session.pending,role:()=>role};
})().catch(()=>{document.getElementById('status').textContent='ReadMD';});
