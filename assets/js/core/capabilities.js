'use strict';
/* A single contextual installer shared by feature entries and the plugin page.
   Installing adds an engine to the user's profile, never to a document folder. */
(function(){
  const t=(key,params)=>window.i18n.t(key,params);
  let active=null;
  function failure(error){const code=error?.message,key='error.'+code,result=Error(typeof code==='string'&&/^[a-z_0-9]+$/.test(code)&&t(key)!==key?code:'capability_install_failed');result.retryable=Boolean(error?.retryable||error instanceof TypeError||error instanceof SyntaxError);return result;}
  async function request(body){try{const r=await apiFetch('/api/capabilities/packs',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify(body)}),d=await r.json();if(!r.ok||!d.ok)throw Error(d.error_code||'capability_install_failed');return d;}catch(error){throw failure(error);}}
  const size=n=>new Intl.NumberFormat(window.i18n.currentLang,{maximumFractionDigits:1}).format(n/1048576)+' MiB';
  const sleep=ms=>new Promise(r=>setTimeout(r,ms));
  async function retryNetwork(action,cancelled=()=>false){
    for(let attempt=0;attempt<3;attempt++){
      if(cancelled())throw Error('cancelled');
      try{return await action();}catch(error){if(!error.retryable||attempt===2)throw error;await sleep(150*(attempt+1));}
    }
  }
  async function settle(d,onProgress=()=>{}){
    for(;;){onProgress(d);if(d.phase==='ready'||d.phase==='removed')return d;if(d.phase==='cancelled')throw Error('cancelled');if(d.phase==='error')throw Error(d.error_code||'capability_install_failed');await sleep(250);d=await retryNetwork(()=>request({action:'download_status',job:d.job}));}
  }
  async function uninstall(id,hooks={}){
    const started=await request({action:'uninstall_start',id});
    hooks.onJob?.(started.job,()=>request({action:'cancel_download',job:started.job}));
    const done=await settle(started,hooks.onProgress);
    if(done.phase!=='removed'||done.result?.installed!==false)throw Error('capability_install_failed');
    if(id==='pdfium'||id==='ocr'){state.modules[id==='pdfium'?'pdf_editor':'ocr']='error';updateModuleUi();}
    window.dispatchEvent(new CustomEvent('readmd:capability-removed',{detail:{id}}));
    return done.result;
  }
  async function upload(file,id='',hooks={}){
    if(!Number.isSafeInteger(file.size)||file.size<=0||file.size>2*1024*1024*1024)throw Error('capability_pack_invalid');
    const prefix=new Uint8Array(await file.slice(0,12).arrayBuffer()),stream=prefix.length===12&&new TextDecoder().decode(prefix.slice(0,8))==='RMDPKG2\n';
    if(stream){const length=new DataView(prefix.buffer).getUint32(8,true);if(!length||length>8*1024*1024||length+12>file.size)throw Error('capability_pack_invalid');let manifest;try{manifest=JSON.parse(await file.slice(12,12+length).text());}catch(_){throw Error('capability_pack_invalid');}if(manifest.schema!==2||!['pdfium','typst','ocr','fonts','latex'].includes(manifest.id)||(id&&manifest.id!==id))throw Error('capability_pack_invalid');id=manifest.id;}
    // Old generic JSON imports retain their existing contract; feature-entry
    // imports know their engine ID and use bounded chunks for either format.
    if(!id){if(file.size>128*1024*1024)throw Error('capability_pack_invalid');
    try{const r=await apiFetch('/api/capabilities/upload?id='+encodeURIComponent(id),{method:'POST',headers:{'Content-Type':'application/octet-stream'},body:file}),d=await r.json();
    if(!r.ok||!d.ok)throw Error(d.error_code||'capability_install_failed');ready(d.id);return d;}catch(error){throw failure(error);}}
    const controller=new AbortController(),cancelled=()=>Boolean(hooks.isCancelled?.()||controller.signal.aborted),paint=d=>hooks.onProgress?.(d);let job,token,installing=false;
    try{
      const key=crypto.randomUUID();let d=await retryNetwork(()=>request({action:'upload_start',id,total:file.size,format:stream?'stream-v2':'json-v1',request_key:key}),cancelled);job=d.job;token=d.upload_token;
      if(!job||typeof token!=='string'||!Number.isInteger(d.chunk_bytes)||d.chunk_bytes<1||d.chunk_bytes>4*1024*1024||!Number.isSafeInteger(d.received)||d.received<0||d.received>file.size)throw Error('capability_pack_invalid');hooks.onJob?.(job,()=>controller.abort());paint(d);
      for(let offset=d.received;offset<file.size;){
        if(cancelled())throw Error('cancelled');
        const bytes=await file.slice(offset,Math.min(file.size,offset+d.chunk_bytes)).arrayBuffer(),digest=[...new Uint8Array(await crypto.subtle.digest('SHA-256',bytes))].map(b=>b.toString(16).padStart(2,'0')).join('');
        const ack=await retryNetwork(async()=>{try{const r=await apiFetch('/api/capabilities/upload/chunk?job='+encodeURIComponent(job)+'&offset='+offset,{method:'POST',headers:{'Content-Type':'application/octet-stream','X-ReadMD-Upload-Token':token,'X-ReadMD-Chunk-SHA256':digest},body:bytes,signal:controller.signal}),response=await r.json();if(!r.ok||!response.ok)throw Error(response.error_code||'capability_install_failed');return response;}catch(error){throw failure(error);}},cancelled);
        if(!Number.isSafeInteger(ack.received)||ack.received<offset+bytes.byteLength||ack.received>file.size)throw Error('capability_pack_invalid');offset=ack.received;paint(ack);
      }
      if(cancelled())throw Error('cancelled');
      installing=true;d=await retryNetwork(()=>request({action:'upload_finish',job,upload_token:token}),cancelled);const result=await settle(d,paint);ready(id);return {...result,installed:true};
    }catch(error){
      let cancelledByServer=false;
      if(job){try{const result=await request({action:'cancel_download',job});cancelledByServer=['cancelled','cancelling'].includes(result.phase);if(result.phase==='ready'){ready(id);return {...result,installed:true};}}
        catch(cancelError){if(installing&&cancelError.message==='capability_install_busy'){const result=await settle(await request({action:'download_status',job}),paint);ready(id);return {...result,installed:true};}}}
      // Cleaning up a failed upload is not a user cancellation: retain the
      // actual integrity/network error instead of silently dismissing the UI.
      if(error.message==='cancelled'||(cancelled()&&(cancelledByServer||!job)))throw Error('cancelled');throw failure(error);
    }
  }
  function ready(id){if(id==='pdfium'||id==='ocr'){state.modules[id==='pdfium'?'pdf_editor':'ocr']='ready';updateModuleUi();}window.dispatchEvent(new CustomEvent('readmd:capability-installed',{detail:{id}}));}
  async function ensure(id){
    if(active)return active.id===id?active.promise:false;
    try{const status=await request({action:'status',id});if(status.ready||status.installed)return true;if(status.busy){showToast(t('error.'+(status.busy_code||'capability_install_busy')));return false;}}catch(_){/* catalog explains an unavailable platform */}
    let entry;
    try{entry=(await request({action:'catalog'})).packs.find(p=>p.id===id&&p.available);}catch(_){showToast(t('error.capability_catalog_invalid'));return false;}
    if(!entry){showToast(t('error.capability_pack_unavailable'));return false;}
    let resolve;const promise=new Promise(r=>{resolve=r;});active={id,promise};
    const modal=document.createElement('div');modal.id='capability-install-modal';modal.className='modal hidden';modal.setAttribute('role','dialog');modal.setAttribute('aria-modal','true');modal.setAttribute('aria-labelledby','capability-install-title');
    const box=document.createElement('section');box.className='capability-install-box';
    const title=document.createElement('h2');title.id='capability-install-title';title.textContent=t('capability.installTitle',{name:t(entry.labelKey)});
    const purpose=document.createElement('p');purpose.textContent=t(entry.descriptionKey);
    const summary=document.createElement('p');summary.className='capability-install-summary';summary.textContent=t('capability.sizes',{download:size(entry.downloadBytes),installed:size(entry.installedBytes)});
    const details=document.createElement('details'),about=document.createElement('summary'),license=document.createElement('p');about.textContent=t('capability.details');license.textContent=entry.version+' · '+entry.platform+' · '+entry.license;details.append(about,license);
    const hint=document.createElement('p');hint.className='capability-install-hint';hint.textContent=t('capability.installHint');
    const status=document.createElement('p');status.id='capability-install-status';status.setAttribute('role','status');status.setAttribute('aria-live','polite');
    const progress=document.createElement('progress');progress.hidden=true;progress.max=100;progress.setAttribute('aria-label',t('plugin.installing'));
    const actions=document.createElement('div');actions.className='capability-install-actions';
    const cancel=document.createElement('button'),offline=document.createElement('button'),install=document.createElement('button');for(const b of [cancel,offline,install]){b.type='button';b.className='tb-btn';}cancel.textContent=t('common.cancel');offline.textContent=t('capability.offlineImport');install.textContent=t('capability.installContinue');install.classList.add('accent');actions.append(cancel,offline,install);
    box.append(title,purpose,summary,details,hint,progress,status,actions);modal.append(box);document.body.append(modal);
    let busy=false,job=null,finished=false,cancelRequested=false,abortUpload=null;
    function finish(value){if(finished)return;finished=true;window.ReadMDModal.close(modal);window.ReadMDModal.setGuard(modal.id,null);modal.remove();active=null;if(value)ready(id);resolve(value);}
    function working(value){busy=value;install.disabled=value;offline.disabled=value;progress.hidden=!value;modal.dataset.modalEsc=value?'off':'on';if(value){cancelRequested=false;abortUpload=null;progress.removeAttribute('value');status.textContent=t('plugin.installing');}cancel.disabled=false;}
    function failed(error){if(finished)return;if(error.message==='cancelled'){finish(false);return;}working(false);job=null;status.textContent=t('error.'+failure(error).message);install.textContent=t('plugin.retry');}
    window.ReadMDModal.setGuard(modal.id,()=>{if(busy)return false;finish(false);return false;});
    cancel.onclick=async()=>{
      if(!busy){finish(false);return;}
      cancelRequested=true;abortUpload?.();cancel.disabled=true;
      if(!job){status.textContent=t('task.cancelling');return;}
      try{const d=await request({action:'cancel_download',job});if(d.phase==='cancelling'){status.textContent=t('task.cancelling');}else if(d.phase==='cancelled'){finish(false);}}
      catch(error){status.textContent=t('error.'+error.message);cancel.disabled=false;}
    };
    function paint(d){
          if(finished)return;
          if(['downloading','uploading'].includes(d.phase))progress.value=d.total?Math.min(100,100*d.received/d.total):0;
          else if(d.phase==='extracting'&&d.expanded_total)progress.value=Math.min(100,100*d.expanded_bytes/d.expanded_total);
          else progress.removeAttribute('value');
          cancel.disabled=!['uploading','downloading','extracting','probing'].includes(d.phase);
          status.textContent=d.phase==='cancelling'?t('task.cancelling'):d.phase==='uploading'?t('capability.offlineImport')+' · '+size(d.received)+' / '+size(d.total):d.phase==='downloading'?t('capability.downloading',{received:size(d.received),total:size(d.total)}):t('capability.verifying');
    }
    async function watch(d){
        job=d.job;if(cancelRequested&&job&&['downloading','extracting','probing'].includes(d.phase))d=await request({action:'cancel_download',job});
        while(!finished){
          if(d.phase==='ready'){finish(true);break;}
          if(d.phase==='cancelled'){finish(false);break;}
          if(d.phase==='error')throw Error(d.error_code||'capability_install_failed');
          paint(d);
          await new Promise(r=>setTimeout(r,250));d=await request({action:'download_status',job});
        }
    }
    install.onclick=async()=>{
      if(busy)return;working(true);
      try{await watch(await request({action:'download',id}));}catch(error){failed(error);}
    };
    async function installFile(path){if(!path)return;working(true);try{await watch(await request({action:'import',id,path}));}catch(error){failed(error);}}
    offline.onclick=async()=>{
      if(busy)return;
      if(hasPy){try{await installFile(await py.choose_any_file());}catch(error){failed(error);}return;}
      const input=document.createElement('input');input.type='file';input.accept='.readmd-pack';input.onchange=async()=>{try{const file=input.files?.[0];if(file){working(true);await upload(file,id,{onJob:(id,abort)=>{job=id;abortUpload=abort;},onProgress:paint,isCancelled:()=>cancelRequested});finish(true);}}catch(error){failed(error);}};input.click();
    };
    window.ReadMDModal.open(modal);return promise;
  }
  const needsFonts=text=>/[\u2e80-\u9fff\uac00-\ud7af\uf900-\ufaff\u{20000}-\u{323af}]/u.test(text);
  async function ensurePublication(content,options,format='pdf'){
    const engine=options?.pdf?.engine;if(engine&&!['rust','typst'].includes(engine))return true;
    const required=window.READMD_ENGINE==='rust'
      ?(await request({action:'publication_fonts_needed',format,content,options:options||{}})).required
      :needsFonts(content+JSON.stringify(options||{}));
    return !required||await ensure('fonts');
  }
  window.ReadMDCapabilities={ensure,request,size,upload,uninstall,ensurePublication};
})();
