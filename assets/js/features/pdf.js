'use strict';
/* Contextual PDF editor. The reader's toolbar stays unchanged. */
(function () {
  const t=(key,params)=>window.i18n.t(key,params);
  let modal,session=null,selected=null,working=false,task=null,opener=null,openedPath=null,listKind=null;
  let navigation=[];
  const $pdf=id=>document.getElementById('pdf-'+id);
  function button(id,key,action){const b=document.createElement('button');b.id='pdf-'+id;b.type='button';b.className='tb-btn';b.dataset.i18n=key;b.textContent=t(key);b.onclick=()=>Promise.resolve(action()).catch(()=>{$pdf('status').textContent=t('error.pdf_operation_failed');});return b;}
  function label(key,input){const el=document.createElement('label'),text=document.createElement('span');text.dataset.i18n=key;text.textContent=t(key);el.append(input,text);return el;}
  function labelLink(hit,target){const label=target.kind==='internal'?t('pdf.goToPage',{page:target.page+1}):target.uri;hit.setAttribute('aria-label',label);hit.title=label+' · '+t('pdf.linkEditHint');}
  addEventListener('readmd:language-changed',()=>{for(const hit of modal?.querySelectorAll('.pdf-link')||[])labelLink(hit,hit.pdfTarget);});
  function build(){
    if(modal)return;
    modal=document.createElement('div');modal.id='pdf-modal';modal.className='modal hidden';modal.setAttribute('role','dialog');modal.setAttribute('aria-modal','true');modal.setAttribute('aria-labelledby','pdf-title');
    const box=document.createElement('section');box.className='pdf-workspace';
    const header=document.createElement('header');header.className='pdf-toolbar';
    const title=document.createElement('strong');title.id='pdf-title';title.dataset.i18n='pdf.title';title.textContent=t('pdf.title');header.append(title);
    header.append(button('previous','pdf.previous',()=>page(-1)));
    const number=document.createElement('input');number.id='pdf-page';number.type='number';number.min='1';number.dataset.i18nAria='pdf.page';number.setAttribute('aria-label',t('pdf.page'));number.onchange=()=>refresh(Math.max(0,Number(number.value)-1));header.append(number);
    const total=document.createElement('span');total.id='pdf-pages';header.append(total,button('next','pdf.next',()=>page(1)),button('undo','editor.undo',()=>run('undo')),button('redo','editor.redo',()=>run('redo')));
    for(const[id,symbol]of[['previous','‹'],['next','›'],['undo','↶'],['redo','↷']]){const b=$pdf(id)||header.querySelector('#pdf-'+id);b.dataset.i18nTitle=b.dataset.i18n;b.dataset.i18nAria=b.dataset.i18n;delete b.dataset.i18n;b.title=b.textContent;b.setAttribute('aria-label',b.textContent);b.textContent=symbol;}
    const more=document.createElement('details');more.className='pdf-more';const summary=document.createElement('summary');summary.textContent='…';summary.dataset.i18nAria='toolbar.more';summary.dataset.i18nTitle='toolbar.more';summary.setAttribute('aria-label',t('toolbar.more'));summary.title=t('toolbar.more');more.append(summary);
    const options=document.createElement('div');options.className='pdf-more-items';options.append(button('rotate','pdf.rotate',()=>session&&edit({kind:'rotate',page:session.page,turns:(session.rotation+1)%4})),button('delete-page','pdf.deletePage',async()=>{if(session&&await confirmAction({title:t('pdf.deletePage'),message:t('pdf.deletePageConfirm'),confirmText:t('ai.delete'),cancelText:t('common.cancel'),danger:true}))await edit({kind:'delete_page',page:session.page});}));
    const signed=document.createElement('input');signed.type='checkbox';signed.id='pdf-signatures';options.append(label('pdf.allowSignatures',signed));
    const password=document.createElement('input');password.id='pdf-password';password.type='password';password.autocomplete='off';password.dataset.i18nAria='pdf.password';password.setAttribute('aria-label',t('pdf.password'));options.append(label('pdf.password',password),button('retry','audit.retryLoad',()=>openedPath&&open(openedPath)));
    options.append(button('back','pdf.back',navigateBack),button('drafts','pdf.drafts',loadDrafts),button('history','storage.history',loadHistory));const drafts=document.createElement('div');drafts.id='pdf-drafts';options.append(drafts);more.append(options);header.append(more);
    const save=button('save','editor.save',()=>run('save'));save.classList.add('accent');header.append(save,button('close','toolbar.close',close));
    const status=document.createElement('p');status.id='pdf-status';status.className='pdf-status';status.setAttribute('role','status');
    const viewport=document.createElement('div');viewport.className='pdf-viewport';viewport.id='pdf-viewport';
    const loading=document.createElement('div');loading.id='pdf-loading';loading.hidden=true;viewport.append(loading);
    const stage=document.createElement('div');stage.className='pdf-page';stage.id='pdf-stage';
    const image=document.createElement('img');image.id='pdf-image';image.alt=t('pdf.page');image.draggable=false;const hits=document.createElement('div');hits.id='pdf-hits';stage.append(image,hits);viewport.append(stage);
    const context=document.createElement('form');context.id='pdf-context';context.className='pdf-context';context.hidden=true;
    const input=document.createElement('textarea');input.id='pdf-text';input.rows=2;input.dataset.i18nAria='pdf.selectedText';input.setAttribute('aria-label',t('pdf.selectedText'));context.append(input);
    const actions=document.createElement('div');actions.className='pdf-context-actions';const overflow=document.createElement('input');overflow.type='checkbox';overflow.id='pdf-overflow';actions.append(label('pdf.allowOverflow',overflow));
    const font=document.createElement('select');font.id='pdf-font';font.dataset.i18nAria='pdf.font';font.setAttribute('aria-label',t('pdf.font'));actions.append(font,button('replace-image','pdf.replaceImage',replaceImage));
    actions.append(button('apply','pdf.apply',apply),button('note','pdf.note',async()=>{if(!selected)return;const p=selected.point||session.coordinates.origin;await edit({kind:'note',page:session.page,x:p[0],y:p[1],text:$pdf('text').value});}),button('remove','ai.delete',()=>selected&&edit({kind:selected.kind==='note'?'remove_annotation':'remove',page:session.page,object:selected.id,annotation:selected.annotation})),button('deselect','common.cancel',()=>select(null)));context.append(actions);context.onsubmit=e=>{e.preventDefault();apply();};
    box.append(header,status,viewport,context);modal.append(box);document.body.append(modal);
    modal.addEventListener('keydown',e=>{const mod=e.ctrlKey||e.metaKey;if(mod&&e.key.toLowerCase()==='s'){e.preventDefault();e.stopPropagation();run('save');}else if(mod&&e.key.toLowerCase()==='z'&&e.target.tagName!=='TEXTAREA'){e.preventDefault();e.stopPropagation();run(e.shiftKey?'redo':'undo');}else if(e.key==='Escape'){e.preventDefault();e.stopPropagation();if(selected)select(null);else close();}else if(mod||/^F\d{1,2}$/.test(e.key)){e.stopPropagation();if(['r','w','n','p','o','f','e','g','d','k'].includes(e.key.toLowerCase())||/^F\d{1,2}$/.test(e.key))e.preventDefault();}});
    modal.addEventListener('keydown',e=>{if(e.altKey&&e.key==='ArrowLeft'){e.preventDefault();e.stopPropagation();navigateBack();}});
    window.ReadMDModal?.setGuard?.('pdf-modal',()=>{if(selected)select(null);else close();return false;});
  }
  function select(item){
    selected=item;$pdf('context').hidden=!item;
    if(!item)return;
    $pdf('text').value=item.text||'';$pdf('text').readOnly=!!item.readonly||item.kind==='image'||item.kind==='checkbox';
    const allowed=session.permissions||{content:session.can_edit,annotations:session.can_edit,forms:session.can_edit,pages:session.can_edit};
    $pdf('apply').disabled=!['text','field','checkbox','note'].includes(item.kind)||item.readonly||!(item.kind==='note'?allowed.annotations:item.annotation!==undefined?allowed.forms:allowed.content);
    $pdf('remove').disabled=!!item.readonly||(item.annotation!==undefined&&item.kind!=='note')||!(item.kind==='note'?allowed.annotations:allowed.content);$pdf('note').disabled=!allowed.annotations;$pdf('replace-image').disabled=!allowed.content;
    $pdf('replace-image').hidden=item.kind!=='image';$pdf('font').hidden=item.kind!=='text'||!!item.readonly;
    $pdf('overflow').closest('label').hidden=item.kind!=='text'||!!item.readonly;
    $pdf('apply').hidden=!!item.readonly;$pdf('remove').hidden=!!item.readonly;
    $pdf('text').focus();
  }
  function setLoading(value){
    modal.setAttribute('aria-busy',String(value));$pdf('viewport').setAttribute('aria-busy',String(value));
    $pdf('loading').hidden=!value;
    if(value){window.ReadMDTask.loading($pdf('loading'));$pdf('stage').hidden=true;select(null);}
    for(const control of modal.querySelectorAll('.pdf-toolbar button,.pdf-toolbar input'))if(control.id!=='pdf-close')control.inert=value;
  }
  async function paint(data){
    if(session?.session!==data.session){$pdf('drafts').replaceChildren();listKind=null;navigation=[];}
    session=data;openedPath=data.path||openedPath;updateUnloadGuard();select(null);$pdf('image').src='data:image/bmp;base64,'+data.image;$pdf('stage').style.aspectRatio=data.width+'/'+data.height;
    $pdf('page').value=String(data.page+1);$pdf('page').max=String(data.pages);$pdf('pages').textContent='/ '+data.pages;
    $pdf('previous').disabled=data.page===0;$pdf('next').disabled=data.page+1>=data.pages;$pdf('undo').disabled=!data.can_undo;$pdf('redo').disabled=!data.can_redo;$pdf('save').disabled=!data.dirty;
    $pdf('rotate').disabled=data.permissions?.pages===false;$pdf('delete-page').disabled=data.pages===1||data.permissions?.pages===false;
    $pdf('status').textContent=t(data.can_edit?'pdf.hint':'error.pdf_read_only')+(data.dirty?' · '+t('pdf.unsaved'):'');
    $pdf('font').replaceChildren();for(const name of ['',...(data.fonts||[])]){const option=document.createElement('option');option.value=name;option.textContent=name||t('pdf.originalFont');$pdf('font').append(option);}
    const hits=$pdf('hits');hits.replaceChildren();
    for(const item of [...data.objects,...data.annotations.map(a=>({...a,annotation:a.id}))]){
      if(!['text','image','field','checkbox','note'].includes(item.kind))continue;
      const r=item.bounds;if(!r.width||!r.height)continue;
      const hit=document.createElement('button');hit.type='button';hit.className='pdf-hit';hit.setAttribute('aria-label',item.text||item.name||t(item.kind==='image'?'pdf.image':'pdf.selectedText'));
      hit.style.left=(100*r.x/data.width)+'%';hit.style.top=(100*r.y/data.height)+'%';hit.style.width=(100*r.width/data.width)+'%';hit.style.height=(100*r.height/data.height)+'%';
      hit.onclick=()=>{
        const c=data.coordinates,x=r.x,y=r.y;item.point=[c.origin[0]+x*c.x[0]+y*c.y[0],c.origin[1]+x*c.x[1]+y*c.y[1]];select(item);
      };hits.append(hit);
    }
    for(const link of data.links||[]){
      const r=link.bounds;if(!r.width||!r.height)continue;
      const target=link.target,hit=document.createElement('button');hit.type='button';hit.className='pdf-hit pdf-link';hit.dataset.link=String(link.id);
      hit.pdfTarget=target;labelLink(hit,target);
      hit.style.left=(100*r.x/data.width)+'%';hit.style.top=(100*r.y/data.height)+'%';hit.style.width=(100*r.width/data.width)+'%';hit.style.height=(100*r.height/data.height)+'%';
      hit.onclick=e=>{if(e.altKey){const x=r.x+r.width/2,y=r.y+r.height/2,item=data.objects.find(o=>o.kind==='text'&&o.bounds.x<=x&&o.bounds.y<=y&&o.bounds.x+o.bounds.width>=x&&o.bounds.y+o.bounds.height>=y);if(item)select(item);return;}navigateLink(target).catch(()=>{$pdf('status').textContent=t('error.pdf_operation_failed');});};
      hits.append(hit);
    }
    $pdf('back').disabled=!navigation.length;
    await $pdf('image').decode();
    $pdf('stage').hidden=false;
  }
  function scrollToTarget(target){
    const viewport=$pdf('viewport'),stage=$pdf('stage');let y=0;
    if(Number.isFinite(target.y)){
      const c=session.coordinates,dx=Number.isFinite(target.x)?target.x-c.origin[0]:0,dy=target.y-c.origin[1],det=c.x[0]*c.y[1]-c.x[1]*c.y[0];
      if(Math.abs(det)>1e-12)y=(dy*c.x[0]-dx*c.x[1])/det;
    }
    const top=stage.getBoundingClientRect().top-viewport.getBoundingClientRect().top+viewport.scrollTop;
    viewport.scrollTo({top:Math.max(0,top+y*stage.clientHeight/session.height-16),behavior:'instant'});
  }
  async function navigateLink(target){
    if(working||!session)return;
    if(target.kind==='external'){
      // A PDF action cannot launch programs, access local files, or execute script.
      if(!/^(https?:\/\/|mailto:)/i.test(target.uri)||/[\u0000-\u001f\u007f]/.test(target.uri))return;
      if(hasPy)await py.open_external(target.uri);else window.open(target.uri,'_blank','noopener');return;
    }
    if(!Number.isInteger(target.page)||target.page<0||target.page>=session.pages)return;
    const previous={page:session.page,top:$pdf('viewport').scrollTop};
    if(target.page!==session.page&&!await refresh(target.page))return;
    await $pdf('image').decode().catch(()=>{});
    navigation.push(previous);if(navigation.length>100)navigation.shift();$pdf('back').disabled=false;select(null);scrollToTarget(target);
  }
  async function navigateBack(){
    if(working||!session||!navigation.length)return;
    const previous=navigation[navigation.length-1];
    if(previous.page!==session.page&&!await refresh(previous.page))return;
    await $pdf('image').decode().catch(()=>{});
    navigation.pop();$pdf('back').disabled=!navigation.length;select(null);$pdf('viewport').scrollTop=previous.top;
  }
  async function request(payload){
    task=crypto.randomUUID();
    const response=await apiFetch('/api/pdf/workspace',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({...payload,task_id:task,allow_signatures:!!$pdf('signatures')?.checked})});
    const data=await response.json();if(!response.ok||!data.ok)throw Error(data.error_code||'pdf_operation_failed');return data;
  }
  async function run(action,extra={}){
    if(working||!session)return false;
    const old=session,selection=selected,draft=selection?{text:$pdf('text').value,font:$pdf('font').value,overflow:$pdf('overflow').checked}:null;
    working=true;if(action!=='save')setLoading(true);
    try{
      const data=await request({action,session:old.session,revision:old.revision,page:old.page,width:1600,...extra});
      if(action==='save'){session={...old,...data};updateUnloadGuard();$pdf('save').disabled=true;$pdf('status').textContent=t('pdf.saved');}
      else await paint(data);return true;
    }catch(error){
      // A rejected edit leaves the old page valid. Restore its selection and
      // uncommitted input so overflow/font errors can be corrected in place.
      if(action!=='save'&&session===old){
        $pdf('stage').hidden=false;select(selection);
        if(draft){$pdf('text').value=draft.text;$pdf('font').value=draft.font;$pdf('overflow').checked=draft.overflow;}
      }
      $pdf('status').textContent=t('error.'+error.message);return false;
    }
    finally{working=false;task=null;setLoading(false);}
  }
  const refresh=number=>run('render',{page:number});const page=delta=>session&&refresh(Math.max(0,Math.min(session.pages-1,session.page+delta)));
  const edit=operation=>run('edit',{operation});
  async function apply(){
    if(!selected||working)return;
    const current=session,target=selected;
    const op={kind:target.kind==='note'?'annotation':target.annotation!==undefined?'field':'text',page:current.page,object:target.id,annotation:target.annotation,text:$pdf('text').value,font:$pdf('font').value,allow_overflow:$pdf('overflow').checked};
    if(target.kind==='checkbox')op.checked=!target.checked;
    if(op.kind==='text'&&op.font&&!await window.ReadMDCapabilities.ensure('fonts'))return;
    // Installation can take time. Never apply the old selection to a newer
    // page or revision after the user has continued working.
    if(session!==current||selected!==target)return;
    await edit(op);
  }
  async function replaceImage(){
    if(!selected||selected.kind!=='image')return;const target={id:selected.id,page:session.page};
    const input=document.createElement('input');input.type='file';input.accept='image/png,image/jpeg,image/webp';
    input.onchange=async()=>{const file=input.files?.[0];if(!file)return;try{
      const bitmap=await createImageBitmap(file);try{if(bitmap.width*bitmap.height>4000000)throw Error('pdf_limit');const canvas=document.createElement('canvas');canvas.width=bitmap.width;canvas.height=bitmap.height;const context=canvas.getContext('2d');context.drawImage(bitmap,0,0);const bytes=context.getImageData(0,0,bitmap.width,bitmap.height).data;
      let raw='';for(let i=0;i<bytes.length;i+=32768)raw+=String.fromCharCode(...bytes.subarray(i,i+32768));await edit({kind:'image',page:target.page,object:target.id,width:bitmap.width,height:bitmap.height,rgba:btoa(raw)});
      }finally{bitmap.close();}
    }catch(error){$pdf('status').textContent=t('error.'+(error.message==='pdf_limit'?'pdf_limit':'pdf_operation_failed'));}};input.click();
  }
  async function close(){
    if(working){$pdf('status').textContent=t('task.cancelling');if(task)await apiFetch('/api/task/cancel',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({id:task})});return false;}
    if(session?.dirty){const answer=await promptDirtyClose((openedPath||t('pdf.title')).split(/[\\/]/).pop());if(answer==='cancel')return false;if(answer==='save'&&!await run('save'))return false;}
    if(session){try{await request({action:'close',session:session.session,discard:true});}catch(error){$pdf('status').textContent=t('error.'+error.message);return false;}session=null;updateUnloadGuard();}
    if(modal){modal.classList.add('hidden');$pdf('password').value='';}opener?.focus?.();return true;
  }
  async function open(path){
    if(!await window.ReadMDCapabilities.ensure('pdfium'))return false;
    build();const password=$pdf('password').value;if(session){if(!await close())return false;}
    opener=document.activeElement;window.ReadMDModal.open(modal);$pdf('status').textContent=t('app.loading');working=true;setLoading(true);
    openedPath=path;
    try{await paint(await request({action:'open',path,password,width:1600}));return true;}
    catch(error){$pdf('status').textContent=t('error.'+error.message);return false;}
    finally{working=false;task=null;setLoading(false);}
  }
  async function choose(){
    if(hasPy){const path=await py.choose_any_file();if(path)await open(path);return;}
    const input=document.createElement('input');input.type='file';input.accept='.pdf';input.onchange=async()=>{const file=input.files?.[0];if(file){const path=await uploadFile(file);if(path)await open(path);}};input.click();
  }
  async function loadDrafts(){
    if(working)return;listKind='drafts';const result=await request({action:'drafts'}),list=$pdf('drafts');list.replaceChildren();
    if(!result.drafts.length){list.textContent=t('pdf.noDrafts');return;}
    for(const draft of result.drafts){const b=button('resume-'+draft.id,'pdf.drafts',async()=>{const password=$pdf('password').value;if(!await close())return;window.ReadMDModal.open(modal);working=true;setLoading(true);try{await paint(await request({action:'resume',draft:draft.id,password,width:1600}));}catch(error){$pdf('status').textContent=t('error.'+error.message);}finally{working=false;setLoading(false);}});delete b.dataset.i18n;b.textContent=draft.name+' · '+new Date(draft.created).toLocaleDateString(window.i18n.currentLang);list.append(b);}
  }
  async function loadHistory(){
    if(working||!session)return;listKind='history';const result=await request({action:'history',path:session.path}),list=$pdf('drafts');list.replaceChildren();if(!result.history.length){list.textContent=t('storage.empty');return;}
    const path=session.path;
    for(const entry of result.history){const date=new Date(entry.created).toLocaleString(window.i18n.currentLang),b=button('restore-'+entry.id,'storage.history',async()=>{
      if(!await confirmAction({title:t('pdf.restoreHistory'),message:t('pdf.restoreHistoryConfirm',{date}),confirmText:t('pdf.restoreHistory'),cancelText:t('common.cancel')}))return;
      const password=$pdf('password').value;if(!await close())return;
      let failure;try{await request({action:'restore_history',path,checkpoint:entry.id,expected_source_revision:result.source_revision});}catch(error){failure=error;}
      $pdf('password').value=password;await open(path);if(failure)$pdf('status').textContent=t('error.'+failure.message);
    });delete b.dataset.i18n;b.textContent=date;list.append(b);}
  }
  window.ReadMDPdf={open,choose,close,prepareClose:close,get dirty(){return !!session?.dirty;}};
  async function packRequest(payload){
    const d=await window.ReadMDCapabilities.request(payload);
    if(payload.action!=='status'&&(d.id==='pdfium'||d.id==='ocr')){
      // Installation has already run the engine probe. Update the same module
      // state used by the rest of the reader without starting another poll loop.
      state.modules[d.id==='pdfium'?'pdf_editor':'ocr']=d.installed?'ready':'error';updateModuleUi();
    }
    return d;
  }
  window.refreshOfflineCapabilityPack=async()=>{
    const row=document.getElementById('plugin-capability-pack');if(!row)return;
    const catalog=await window.ReadMDCapabilities.request({action:'catalog'}),entries=catalog.packs.filter(p=>p.available);
    const results=await Promise.allSettled(entries.map(p=>packRequest({action:'status',id:p.id})));row.replaceChildren();row.classList.remove('hidden');
    for(let i=0;i<results.length;i++){
      const result=results[i],item=document.createElement('div');item.className='plugin-capability-item';row.append(item);
      if(result.status!=='fulfilled'){item.textContent=t('error.'+result.reason.message);continue;}
      const data=result.value,entry=entries[i],engine=t(entry.labelKey),name=document.createElement('div');name.className='plugin-capability-details';const title=document.createElement('strong');title.textContent=engine+' · '+t(data.installed?'plugin.installed':'plugin.notInstalled');const purpose=document.createElement('p');purpose.textContent=t(entry.descriptionKey);const sizes=document.createElement('p');sizes.textContent=t('capability.sizes',{download:window.ReadMDCapabilities.size(entry.downloadBytes),installed:window.ReadMDCapabilities.size(entry.installedBytes)});name.append(title,purpose,sizes);item.append(name);
      if(!data.installed){const install=document.createElement('button');install.type='button';install.className='tb-btn';install.textContent=t('plugin.install');install.id='plugin-capability-install-'+data.id;install.onclick=async()=>{if(await window.ReadMDCapabilities.ensure(data.id))await window.refreshOfflineCapabilityPack();};item.append(install);}
      if(data.present){const remove=document.createElement('button');remove.type='button';remove.className='tb-btn';remove.id=data.id==='pdfium'?'plugin-capability-uninstall':'plugin-capability-uninstall-'+data.id;remove.textContent=t('plugin.uninstall');remove.onclick=async()=>{
        if(!await confirmAction({title:t('plugin.uninstall'),message:t('capability.uninstallConfirm',{name:engine}),confirmText:t('plugin.uninstall'),cancelText:t('common.cancel'),danger:true}))return;
        remove.disabled=true;
        const operation=document.createElement('div');operation.className='plugin-capability-operation';
        const progress=document.createElement('progress');progress.max=100;progress.setAttribute('aria-label',t('plugin.uninstall'));
        const status=document.createElement('span');status.setAttribute('role','status');status.setAttribute('aria-live','polite');status.textContent=t('plugin.uninstall')+'…';
        const cancel=document.createElement('button');cancel.type='button';cancel.className='tb-btn';cancel.id='plugin-capability-cancel-'+data.id;cancel.textContent=t('common.cancel');
        operation.append(progress,status,cancel);item.append(operation);
        let cancelTask=null,cancelRequested=false;
        const paint=d=>{cancel.disabled=!['removing'].includes(d.phase);if(d.files_total){progress.value=100*d.files_done/d.files_total;}else{progress.removeAttribute('value');}status.textContent=d.phase==='cancelling'?t('task.cancelling'):t('plugin.uninstall')+'…'+(d.files_total?' '+new Intl.NumberFormat(window.i18n.currentLang).format(d.files_done)+' / '+new Intl.NumberFormat(window.i18n.currentLang).format(d.files_total):'');};
        const requestCancel=async()=>{cancelRequested=true;cancel.disabled=true;status.textContent=t('task.cancelling');if(!cancelTask)return;try{paint(await cancelTask());}catch(error){status.textContent=t('error.'+error.message);}};
        cancel.onclick=requestCancel;
        try{const result=await window.ReadMDCapabilities.uninstall(data.id,{onJob:(_job,action)=>{cancelTask=action;if(cancelRequested)requestCancel();},onProgress:paint});await window.refreshOfflineCapabilityPack();showToast(t(result.cleanup_pending?'capability.cleanupPending':'plugin.uninstallSuccess',{name:engine}));}
        catch(error){if(error.message!=='cancelled')showToast(t('error.'+error.message));await window.refreshOfflineCapabilityPack();}
        finally{operation.remove();remove.disabled=false;}
      };item.append(remove);}
    }
  };
  window.installOfflineCapabilityPack=async()=>{
    const install=async picked=>{if(!picked)return;const b=document.getElementById('plugin-offline-pack');b.disabled=true;try{const installed=await packRequest({action:'install',path:picked});showToast(t('capability.packInstalled',{name:t('capability.'+installed.id+'.name')}));}catch(error){showToast(t('error.'+error.message));}finally{b.disabled=false;await window.refreshOfflineCapabilityPack();}};
    if(hasPy){await install(await py.choose_any_file());return;}
    const input=document.createElement('input');input.type='file';input.accept='.readmd-pack';input.onchange=async()=>{const file=input.files?.[0];if(!file)return;const button=document.getElementById('plugin-offline-pack');button.disabled=true;try{const installed=await window.ReadMDCapabilities.upload(file);showToast(t('capability.packInstalled',{name:t('capability.'+installed.id+'.name')}));}catch(error){showToast(t('error.'+error.message));}finally{button.disabled=false;await window.refreshOfflineCapabilityPack();}};input.click();
  };
  document.getElementById('plugin-offline-pack')?.addEventListener('click',()=>window.installOfflineCapabilityPack().catch(()=>showToast(t('error.capability_pack_invalid'))));
  window.addEventListener('readmd:language-changed',()=>{if(!document.getElementById('plugin-modal')?.classList.contains('hidden'))window.refreshOfflineCapabilityPack();if(session){$pdf('status').textContent=t(session.can_edit?'pdf.hint':'error.pdf_read_only')+(session.dirty?' · '+t('pdf.unsaved'):'');const original=$pdf('font').querySelector('option[value=""]');if(original)original.textContent=t('pdf.originalFont');}});
  window.openPdfEditor=choose;
  document.getElementById('btn-pdf-edit')?.addEventListener('click',choose);
})();
