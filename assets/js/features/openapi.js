'use strict';
// A contextual import/review flow; no extra toolbar, files, or execution on open.
(function () {
  const t=(key,params)=>window.i18n.t(key,params);
  let review=null,epoch=0,modal,pending=null;
  const activeContent=()=>{const tab=getActiveTab();return state.editing&&cmView?cmView.state.doc.toString():((tab?.isDirty?tab.content:tab?.original)??state.original??'').replace(/\r\n?/g,'\n');};
  const revision=async text=>Array.from(new Uint8Array(await crypto.subtle.digest('SHA-256',new TextEncoder().encode(text))),b=>b.toString(16).padStart(2,'0')).join('');
  async function request(route,payload){const response=await apiFetch(route,{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify(payload)}),data=await response.json();if(!response.ok||!data.ok)throw Error(data.error_code||data.error||'openapi_invalid');return data;}
  function element(tag,className,text){const el=document.createElement(tag);if(className)el.className=className;if(text!==undefined)el.textContent=text;return el;}
  function excerpt(text){if(text.length<=250000)return text;return text.slice(0,125000)+'\n…\n'+text.slice(-125000);}
  function close(){++epoch;review=null;if(pending)apiFetch('/api/task/cancel',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({id:pending.id})}).catch(()=>{});if(modal)ReadMDModal.close(modal);}
  function ensure(){
    if(modal)return;modal=element('div','modal-overlay hidden');modal.id='openapi-review-modal';modal.setAttribute('role','dialog');modal.setAttribute('aria-modal','true');modal.setAttribute('aria-labelledby','openapi-review-title');
    const box=element('div','modal-box openapi-review-box'),header=element('div','modal-header'),title=element('h2');title.id='openapi-review-title';
    const cancel=element('button','tb-btn');cancel.id='openapi-review-cancel';cancel.type='button';cancel.onclick=close;
    const apply=element('button','tb-btn accent');apply.id='openapi-review-apply';apply.type='button';apply.onclick=accept;
    const status=element('p','hint');status.id='openapi-review-status';status.setAttribute('role','status');status.setAttribute('aria-live','polite');
    const changes=element('div','openapi-review-changes');changes.id='openapi-review-changes';
    const diff=element('div','openapi-review-diff');for(const side of['before','after']){const section=element('section'),heading=element('h3'),pre=element('pre');heading.dataset.i18n='openapi.'+side;heading.textContent=t('openapi.'+side);pre.id='openapi-review-'+side;section.append(heading,pre);diff.append(section);}
    const actions=element('div','modal-actions');actions.append(cancel,apply);header.append(title);box.append(header,status,changes,diff,actions);modal.append(box);document.body.append(modal);
    ReadMDModal.setGuard(modal.id,()=>{close();return false;});modal.addEventListener('click',event=>{if(event.target===modal)close();});
    modal.addEventListener('keydown',event=>{if(event.ctrlKey||event.metaKey){event.stopPropagation();if(['s','r','w','n','p','o','f','e','g','d','k'].includes(event.key.toLowerCase()))event.preventDefault();}});
  }
  function paintDifference(before,after){
    const a=excerpt(before).split('\n'),b=excerpt(after).split('\n');let prefix=0,suffix=0;
    while(prefix<Math.min(a.length,b.length)&&a[prefix]===b[prefix])prefix++;
    while(suffix<Math.min(a.length,b.length)-prefix&&a[a.length-1-suffix]===b[b.length-1-suffix])suffix++;
    for(const[side,lines]of[['before',a],['after',b]]){
      const host=$('openapi-review-'+side);host.replaceChildren();if(before===after){host.textContent=t('openapi.upToDate');continue;}
      if(lines.length===1&&!lines[0]){host.textContent=t('openapi.empty');continue;}
      const from=Math.max(0,prefix-4),to=Math.min(lines.length,lines.length-suffix+4);
      if(from)host.append(element('span','openapi-line','…'));
      for(let index=from;index<to;index++)host.append(element('span','openapi-line'+(index>=prefix&&index<lines.length-suffix?' openapi-line-'+side:''),lines[index]||' '));
      if(to<lines.length)host.append(element('span','openapi-line','…'));
    }
  }
  function paint(){
    if(!review)return;const {data,content}=review,edit=data.transaction.edits[0];
    $('openapi-review-title').textContent=t('openapi.title');$('openapi-review-cancel').textContent=t('common.cancel');$('openapi-review-apply').textContent=t('openapi.accept');$('openapi-review-apply').disabled=!data.result.changed;
    $('openapi-review-status').textContent=data.source_name+' · '+t(data.result.changed?'openapi.reviewHint':'openapi.upToDate');
    const list=$('openapi-review-changes');list.replaceChildren();for(const change of data.changes){list.append(element('p','',t('openapi.'+change.kind)+' · '+change.pointer));}
    for(const warning of data.warnings){list.append(element('p','hint',t('openapi.warning.'+warning.code)+' · '+warning.pointer));}
    // Strip provenance from the visible review, while preserving it in the edit.
    const visible=text=>text.replace(/^<!-- readmd:openapi:(?:start|end) [^\n]*-->\r?\n?/gm,'');
    paintDifference(visible(content.slice(edit.from,edit.to)),visible(edit.insert));
  }
  async function generate(file){
    if(!file)return false;
    ensure();if(pending)close();const token=++epoch,content=state.mode==='welcome'?'':activeContent(),tabId=state.activeTabId,job=pending={id:crypto.randomUUID()};review=null;
    $('openapi-review-title').textContent=t('openapi.title');$('openapi-review-status').textContent=t('task.running',{seconds:0});$('openapi-review-cancel').textContent=t('common.cancel');$('openapi-review-apply').textContent=t('openapi.accept');$('openapi-review-apply').disabled=true;
    for(const id of['changes','before','after'])$('openapi-review-'+id).replaceChildren();ReadMDModal.open(modal);
    try{
      const base_revision=await revision(content);if(token!==epoch)return false;
      const data=await request('/api/developer/openapi',{file_path:file,content,base_revision,locale:i18n.currentLang,task_id:job.id});
      if(token!==epoch||state.activeTabId!==tabId||content!==(state.mode==='welcome'?'':activeContent()))throw Error('transaction_conflict');
      review={data,content,tabId,file,locale:i18n.currentLang};paint();ReadMDModal.open(modal);return true;
    }catch(error){if(token===epoch)$('openapi-review-status').textContent=t('error.'+error.message);return false;}
    finally{if(pending===job)pending=null;}
  }
  async function accept(){
    const current=review;if(!current)return false;const button=$('openapi-review-apply');button.disabled=true;
    const valid=()=>review===current&&state.activeTabId===current.tabId&&current.content===(state.mode==='welcome'?'':activeContent());
    try{
      if(!valid())throw Error('transaction_conflict');
      await request('/api/developer/openapi',{file_path:current.file,content:current.content,base_revision:current.data.transaction.base_revision,locale:current.locale,expected_source_revision:current.data.source_revision});
      if(!valid())throw Error('transaction_conflict');
      const checked=await request('/api/documents/transaction',{content:current.content,transaction:current.data.transaction});
      if(!valid())throw Error('transaction_conflict');
      if(!current.tabId||state.mode==='welcome'){
        const name=current.data.source_name.replace(/\.(json|ya?ml)$/i,'')+'.md';close();
        await renderVirtual('openapi',name,state.dir||'',checked.result.content,[],{});await toggleEdit();return true;
      }
      if(!await ReadMDRecovery.checkpoint('openapi'))return false;
      if(!valid())throw Error('transaction_conflict');
      if(!state.editing)await toggleEdit();
      if(!valid()||!cmView)throw Error('transaction_conflict');
      const changes=current.data.transaction.edits.map(edit=>({from:edit.from,to:edit.to,insert:edit.insert}));
      cmView.dispatch({changes,userEvent:'input.openapi'});close();cmView.focus();return true;
    }catch(error){$('openapi-review-status').textContent=t('error.'+error.message);return false;}
    finally{if(review===current)button.disabled=false;}
  }
  async function choose(){
    if(hasPy)return generate(await py.choose_any_file());
    const input=document.createElement('input');input.type='file';input.accept='.json,.yaml,.yml';input.onchange=async()=>{const file=input.files?.[0];if(file)await generate(await uploadFile(file));};input.click();
  }
  window.ReadMDOpenAPI={generate,accept,close};window.openOpenAPIDocument=choose;
  window.addEventListener('readmd:language-changed',()=>{if(review){paint();modal.querySelectorAll('[data-i18n]').forEach(el=>el.textContent=t(el.dataset.i18n));}});
})();
