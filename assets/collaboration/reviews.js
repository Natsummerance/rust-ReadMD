'use strict';
// One review adapter for the LAN editor and the native sharing panel. Text
// edits, file saving and personal undo remain owned by their existing adapters.
window.ReadMDCollaborationReviews=(()=>{
  const stylesheet=new URL('reviews.css',document.currentScript.src);
  const link=document.createElement('link');link.rel='stylesheet';link.href=stylesheet;document.head.append(link);
  function mount({root,api,session,tr,flush,getContent,getSelection}){
    let disposed=false,refreshing,items=[],editable=false,cacheSignature;
    const rows=new Map(),toolbar=document.createElement('div'),note=document.createElement('p'),list=document.createElement('div');
    root.classList.add('collaboration-reviews');note.setAttribute('role','status');note.setAttribute('aria-live','polite');
    function button(label,action){const b=document.createElement('button');b.type='button';b.textContent=tr(label);b.onclick=action;return b;}
    // The keyboard shortcut still belongs to personal undo. Reuse the native
    // localized action name without advertising that shortcut on this button.
    const undoLabel=()=>tr('editor.undo').replace(/\s*[（(][^（）()]*[）)]\s*$/u,'').trim();
    const reload=button('collaboration.resolution.refresh',()=>refresh());toolbar.append(reload);root.replaceChildren(toolbar,note,list);
    const message=key=>{if(!disposed){note.textContent=tr(key);note.dataset.status=key;}};
    const failure=error=>message(error?.status===403?'collaboration.resolution.denied':error?.status===413||error?.status===429||error?.message==='collaboration_budget'?'review.limit':error?.status===409||error?.message==='changed'?'collaboration.resolution.changed':'collaboration.offline');
    function excerpt(parent,label,value,tag='pre'){
      const section=document.createElement('div'),caption=document.createElement('small'),text=document.createElement(tag);
      caption.textContent=tr(label);text.textContent=value??tr('collaboration.resolution.unlocated');section.append(caption,text);parent.append(section);return text;
    }
    async function form(row,item,saved){
      if(disposed||!row.isConnected)return;
      let context=saved,fields=[],writing=false,formNode=row.querySelector('.collaboration-merge');
      if(formNode)return;
      formNode=document.createElement('div');formNode.className='collaboration-merge';row.append(formNode);
      const hint=document.createElement('p'),body=document.createElement('div'),actions=document.createElement('div');hint.textContent=tr('collaboration.resolution.hint');formNode.append(hint,body,actions);
      const draft=()=>{const limit=Math.max(1,Math.floor(32768/Math.max(1,fields.length))),excerpt=value=>typeof value==='string'?value.slice(0,limit)+(value.length>limit?'…':''):value;return {revision:context.revision,generation:context.generation,edits:fields.map(field=>({from:field.from,to:field.to,original:excerpt(field.original),current:excerpt(field.current),insert:field.input.value,selected:field.selected}))};};
      async function save(){if(context)try{await session.reviewRecord('draft:'+item.id,draft());message('collaboration.resolution.draftKept');}catch(error){failure(error);}}
      function draw(){
        body.replaceChildren();fields=[];
        for(const edit of context.edits){
          const section=document.createElement('section');section.className='collaboration-merge-edit';body.append(section);
          excerpt(section,'collaboration.resolution.original',edit.original);
          const current=excerpt(section,'collaboration.resolution.current',edit.current);
          const label=document.createElement('label'),input=document.createElement('textarea');label.textContent=tr('collaboration.resolution.merged');input.value=edit.insert;input.rows=4;label.append(input);section.append(label);
          const range=document.createElement('small'),field={...edit,input};fields.push(field);
          const showRange=()=>range.textContent=Number.isInteger(field.from)&&Number.isInteger(field.to)?tr('collaboration.resolution.range',{from:field.from,to:field.to}):tr('collaboration.resolution.selectRange');showRange();section.append(range);
          const selected=button('collaboration.resolution.useSelection',async()=>{
            const selection=getSelection();
            if(typeof context.content!=='string'||getContent()!==context.content){message('collaboration.resolution.changed');return;}
            if(!selection||!Number.isInteger(selection.from)||!Number.isInteger(selection.to)){message('collaboration.resolution.selectRange');return;}
            field.from=Math.min(selection.from,selection.to);field.to=Math.max(selection.from,selection.to);field.selected=true;
            field.current=context.content.slice(field.from,field.to);current.textContent=field.current;showRange();await save();
          });section.append(selected);input.addEventListener('input',()=>void save());
        }
      }
      async function compare(){
        if(writing)return;writing=true;refreshButton.disabled=true;
        try{
          const previous=context?.edits?.map((edit,index)=>fields[index]?.input.value??edit.insert);
          await flush();const data=await api('review-preview',{id:item.id});
          if(disposed||!row.isConnected)return;
          if(getContent()!==data.content)throw Error('changed');
          context={...data,edits:data.edits.map((edit,index)=>({...edit,insert:previous?.[index]??edit.insert}))};draw();await save();
          hint.textContent=tr(data.conflict?'collaboration.resolution.conflictHint':'collaboration.resolution.hint');
        }catch(error){failure(error);}finally{writing=false;refreshButton.disabled=false;}
      }
      const refreshButton=button('collaboration.resolution.compare',compare);
      const apply=button('collaboration.resolution.apply',async()=>{
        if(writing||!session.canEdit())return;writing=true;apply.disabled=true;
        try{
          await save();await flush();
          if(typeof context?.content!=='string'||context.content!==getContent()||fields.some(field=>!Number.isInteger(field.from)||!Number.isInteger(field.to)))throw Error('changed');
          await api('resolve',{id:item.id,accept:true,generation:context.generation,transaction:{base_revision:context.revision,origin:'review',edits:fields.map(field=>({from:field.from,to:field.to,insert:field.input.value}))}});
          await session.reviewRecord('draft:'+item.id,null);await flush();await refresh();message('collaboration.resolution.applied');
        }catch(error){failure(error);}finally{writing=false;apply.disabled=!session.canEdit();}
      });apply.disabled=!session.canEdit()||item.status!=='open';
      const discard=button('collaboration.resolution.discardDraft',async()=>{try{await session.reviewRecord('draft:'+item.id,null);formNode.remove();message('collaboration.resolution.draftRemoved');}catch(error){failure(error);}});
      actions.append(refreshButton,apply,discard);
      if(context){draw();hint.textContent=tr('collaboration.resolution.draftRestored');}else await compare();
    }
    function render(){
      if(disposed)return;
      const canEdit=session.canEdit();
      const known=new Set(items.map(item=>item.id));for(const [id,row]of rows)if(!known.has(id)){row.remove();rows.delete(id);}
      if(!items.length){list.textContent=tr('collaboration.resolution.empty');return;}
      if(!rows.size)list.replaceChildren();
      for(const item of items){
        const signature=JSON.stringify([item,canEdit]);let row=rows.get(item.id);if(row?.dataset.signature===signature)continue;
        const replacement=document.createElement('article');replacement.dataset.signature=signature;replacement.dataset.reviewId=item.id;
        const text=document.createElement('p'),by=document.createElement('small');text.textContent=item.body.text;by.textContent=item.name+' · '+tr('collaboration.'+(item.status==='open'?'openReview':item.status));replacement.append(text,by);
        if(item.body.transaction){
          const diff=document.createElement('div');diff.className='review-diff';item.body.transaction.edits.forEach((edit,index)=>{const before=document.createElement('del'),after=document.createElement('ins');before.textContent=item.body._anchors?.[index]?.original||'';after.textContent=edit.insert;diff.append(before,after);});replacement.append(diff);
        }
        if(item.body._decision){
          const decision=item.body._decision,history=document.createElement('details'),summary=document.createElement('summary');summary.textContent=tr('collaboration.resolution.history');history.append(summary);
          const actor=document.createElement('small');actor.textContent=(decision.name||decision.member)+' · '+new Date(decision.created*1000).toLocaleString();history.append(actor);
          (decision.transaction?.edits||[]).forEach(edit=>excerpt(history,'collaboration.resolution.merged',edit.insert));replacement.append(history);
          if(item.reversal){const receipt=item.reversal,event=document.createElement('p');event.dataset.reviewEvent='reverse';event.textContent=undoLabel()+' · '+(receipt.name||receipt.member)+' · '+new Date(receipt.created*1000).toLocaleString();history.append(event);history.open=true;}
        }
        if(item.can_reverse&&canEdit){
          const undo=button('editor.undo',async()=>{
            if(!session.canEdit()||undo.disabled)return;undo.disabled=true;
            try{await flush();await api('reverse',{id:item.id,generation:item.body._decision.result_generation,revision:item.body._decision.result_revision});await flush();await refresh();message('collaboration.synced');}
            catch(error){if(error?.status===403)message('review.notAuthorized');else if(error?.status===409)message('error.review_undo_unavailable');else failure(error);}
            finally{undo.disabled=!session.canEdit();}
          });undo.textContent=undoLabel();undo.dataset.reviewAction='reverse';replacement.append(undo);
        }
        if(item.status==='open'&&canEdit)for(const accept of [true,false])replacement.append(button(accept?'collaboration.accept':'collaboration.reject',async()=>{
          try{await flush();await api('resolve',{id:item.id,accept});await flush();await refresh();}
          catch(error){failure(error);if(accept&&item.body.transaction&&error?.status===409)await form(replacement,item);}
        }));
        if(item.body.transaction&&item.status==='open')replacement.append(button('collaboration.resolution.compare',()=>form(replacement,item)));
        if(row)row.replaceWith(replacement);else list.append(replacement);row=replacement;rows.set(item.id,row);
        void session.reviewRecord('draft:'+item.id).then(saved=>{if(saved&&!disposed&&rows.get(item.id)===row)return form(row,item,saved);}).catch(failure);
      }
      editable=canEdit;
    }
    function refresh(){
      if(disposed)return Promise.resolve();if(refreshing)return refreshing;
      refreshing=(async()=>{
        try{await restored;const data=await api('reviews');if(disposed)return;const signature=JSON.stringify(data.reviews);if(signature!==cacheSignature){await session.reviewRecord('list',data.reviews);cacheSignature=signature;items=data.reviews;render();}else if(editable!==session.canEdit())render();if(note.dataset.status==='collaboration.offline')note.textContent='';}
        catch(error){if(!items.length){try{items=await session.reviewRecord('list')||[];render();}catch(storage){failure(storage);return;}}failure(error);}
      })().finally(()=>refreshing=null);return refreshing;
    }
    const restored=(async()=>{try{items=await session.reviewRecord('list')||[];cacheSignature=JSON.stringify(items);render();}catch(error){failure(error);}})();
    void refresh();
    return {refresh,permissions:()=>{if(editable!==session.canEdit())render();},language:()=>{reload.textContent=tr('collaboration.resolution.refresh');if(note.dataset.status)message(note.dataset.status);for(const row of rows.values())row.dataset.signature='';render();},dispose:()=>{disposed=true;rows.clear();root.replaceChildren();}};
  }
  return {mount};
})();
