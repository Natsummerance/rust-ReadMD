'use strict';

// Ghost text is never part of the document until Tab accepts it. Cache remains
// in memory; only bounded cursor context enters Rust, never paths or history.
const ReadMDInlineCompletion = (() => {
  const instances = new WeakMap(), cache = new Map();
  let sequence = 0;
  const t = key => window.i18n ? window.i18n.t(key) : key;
  const enabled = () => !!editorPrefs.aiCompletion;
  const identity = () => {
    const current = state.ai?.config?.current || {};
    const provider = state.ai?.providers?.find(p => p.id === (current.provider_id || current.provider));
    return JSON.stringify([current, provider?.autocomplete || {mode:'auto'}]);
  };
  function extension(CM) {
    const effect = CM.StateEffect.define();
    class Ghost extends CM.WidgetType {
      constructor(text,streaming) { super(); this.text = text; this.streaming=streaming; }
      eq(other) { return this.text === other.text && this.streaming === other.streaming; }
      toDOM() {
        const node = document.createElement('span');
        node.className = 'readmd-ai-ghost'; node.textContent = this.text;
        if(this.streaming)node.dataset.streaming='true';
        node.setAttribute('aria-hidden','true'); node.title = t('editor.aiCompletionAccept');
        return node;
      }
      ignoreEvent() { return true; }
    }
    const field = CM.StateField.define({
      create: () => CM.Decoration.none,
      update(value,tr) {
        if (tr.docChanged || tr.selection) value = CM.Decoration.none;
        for (const e of tr.effects) if (e.is(effect)) value = e.value
          ? CM.Decoration.set([CM.Decoration.widget({widget:new Ghost(e.value.text,e.value.streaming),side:1}).range(e.value.pos)])
          : CM.Decoration.none;
        return value;
      },
      provide: f => CM.EditorView.decorations.from(f),
    });
    class Controller {
      constructor(view) {
        this.view=view; this.generation=0; this.timer=0; this.pending=null;
        this.suggestion=null; this.disposed=false; this.pausedUntil=0; this.lastInput=0; this.typingInterval=200; this.latency=0;
        this.revealTimer=0; this.revealed=''; this.revealTarget=''; this.model='';
        instances.set(view,this);
      }
      status(key) {
        const node = document.getElementById('status-ai-completion');
        if (node) { node.textContent=key?t(key):''; node.dataset.state=key||''; node.title=''; }
      }
      cancel() {
        clearTimeout(this.timer); this.timer=0; this.generation++;
        clearTimeout(this.revealTimer);this.revealTimer=0;this.revealed='';this.revealTarget='';
        if (this.pending) {
          const {abort,task}=this.pending; this.pending=null; abort.abort();
          apiFetch('/api/task/cancel',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({id:task})}).catch(()=>{});
        }
      }
      clear() {
        this.cancel(); this.suggestion=null; this.status('');
        if (!this.disposed) this.view.dispatch({effects:effect.of(null)});
      }
      snapshot() {
        const view=this.view, sel=view.state.selection;
        if (!enabled() || view.composing || !view.hasFocus || sel.ranges.length!==1 || !sel.main.empty || !state.editing || document.hidden) return null;
        if (typeof slash!=='undefined' && slash.open || CM.completionStatus(view.state)) return null;
        const pos=sel.main.head, doc=view.state.doc;
        if (doc.length>2*1024*1024 || /^\s*\|/.test(doc.lineAt(pos).text)) return null;
        let prefix=doc.sliceString(Math.max(0,pos-16000),pos), suffix=doc.sliceString(pos,Math.min(doc.length,pos+4000));
        // A UTF-16 slice may bisect a pair at its outer edge.
        prefix=prefix.replace(/^[\uDC00-\uDFFF]/,''); suffix=suffix.replace(/[\uD800-\uDBFF]$/,'');
        prefix=Array.from(prefix).slice(-3000).join(''); suffix=Array.from(suffix).slice(0,750).join('');
        if (Array.from(prefix.trim()).length<4) return null;
        return {doc,pos,prefix,suffix,provider:identity(),tab:state.activeTabId};
      }
      current(s) {
        return !this.disposed && enabled() && this.view.hasFocus && !this.view.composing && !document.hidden
          && this.view.state.doc===s.doc && this.view.state.selection.main.empty
          && this.view.state.selection.main.head===s.pos && state.activeTabId===s.tab && identity()===s.provider;
      }
      show(s,text,streaming=false) {
        if (!text || !this.current(s)) return;
        this.suggestion={...s,text}; this.view.dispatch({effects:effect.of({pos:s.pos,text,streaming})}); this.status('editor.aiCompletionAccept');
        const status=document.getElementById('status-ai-completion');
        if(status && this.model)status.title=window.i18n?window.i18n.t('ai.autocompleteActiveModel',{model:this.model}):this.model;
        // A wrapped candidate can extend beyond the last visible line. Reveal
        // only the missing height; do not recenter the user's document.
        this.view.requestMeasure({key:this,
          read:view=>{const node=view.dom.querySelector('.readmd-ai-ghost');if(!node)return 0;
            return Math.max(0,node.getBoundingClientRect().bottom-view.scrollDOM.getBoundingClientRect().bottom+24);},
          write:delta=>{if(delta>0 && this.current(s))this.view.scrollDOM.scrollTop+=delta;}});
      }
      reveal(s,text,finished=false) {
        // Animate only text already received. Large proxy chunks take at most
        // 120 ms; cached suggestions and reduced-motion mode stay immediate.
        clearTimeout(this.revealTimer);this.revealTimer=0;
        if(window.matchMedia('(prefers-reduced-motion: reduce)').matches){this.revealed=text;this.show(s,text,!finished);return;}
        if(!text.startsWith(this.revealed))this.revealed='';
        this.revealTarget=text;
        const parts=typeof Intl.Segmenter==='function'?[...new Intl.Segmenter(undefined,{granularity:'grapheme'}).segment(text)].map(s=>s.segment):Array.from(text);
        let count=typeof Intl.Segmenter==='function'?[...new Intl.Segmenter(undefined,{granularity:'grapheme'}).segment(this.revealed)].length:Array.from(this.revealed).length;
        const step=Math.max(1,Math.ceil((parts.length-count)/6));
        const advance=()=>{
          if(!this.current(s))return;
          count=Math.min(parts.length,count+step);this.revealed=parts.slice(0,count).join('');
          this.show(s,this.revealed,!finished || count<parts.length);
          if(count<parts.length)this.revealTimer=setTimeout(advance,20);
          else this.revealTimer=0;
        };advance();
      }
      schedule(manual=false) {
        this.cancel(); if (Date.now()<this.pausedUntil && !manual) return;
        const s=this.snapshot(); if (!s) return;
        const generation=this.generation;
        const item=cache.get(JSON.stringify([s.provider,s.prefix,s.suffix]));
        if(item && Date.now()-item.time<60000){this.model=item.model||'';this.show(s,item.text);return;}
        // A slow provider should not add a second long wait before sending.
        const delay=Math.max(180,Math.min(650,this.typingInterval*1.8-Math.min(this.latency/12,180)));
        this.timer=setTimeout(()=>this.request(s,generation),manual?0:delay);
      }
      async request(s,generation) {
        if (generation!==this.generation || !this.current(s)) return;
        const key=JSON.stringify([s.provider,s.prefix,s.suffix]);
        if (cache.has(key)) {
          const item=cache.get(key); cache.delete(key);
          if(Date.now()-item.time<60000){cache.set(key,item);this.model=item.model||'';this.show(s,item.text);return;}
        }
        const abort=new AbortController(), task='inline-'+Date.now()+'-'+(++sequence);
        this.pending={abort,task}; this.status('editor.aiCompletionLoading');
        const began=Date.now(),deadline=setTimeout(()=>abort.abort(),35000);
        try {
          const response=await apiFetch('/api/ai/complete',{method:'POST',headers:{'Content-Type':'application/json','Accept':'text/event-stream'},
            body:JSON.stringify({prefix:s.prefix,suffix:s.suffix,task_id:task}),signal:abort.signal});
          let body;
          const streaming=response.ok && (response.headers.get('content-type')||'').includes('text/event-stream');
          if(streaming){
            const reader=response.body.getReader(),decoder=new TextDecoder();let buffer='',finished=false;
            const frame=line=>{
              if(!line.startsWith('data:'))return;
              const event=JSON.parse(line.slice(5).trim());
              if(event.type==='error' || event.ok===false)throw new Error(event.error_code||'ai_request_failed');
              if(event.type==='done'){body=event;finished=true;return;}
              if(event.type==='suggestion' && generation===this.generation && this.current(s)){
                const text=typeof event.completion==='string'?event.completion:'';
                this.model=event.model||'';
                if(text)this.reveal(s,text);
                else{clearTimeout(this.revealTimer);this.revealTimer=0;this.revealed='';this.suggestion=null;this.view.dispatch({effects:effect.of(null)});this.status('editor.aiCompletionLoading');}
              }
            };
            try{
              while(!finished){
                const {done,value}=await reader.read();
                buffer+=decoder.decode(value||new Uint8Array(),{stream:!done});
                let end;while((end=buffer.indexOf('\n'))>=0){frame(buffer.slice(0,end).trimEnd());buffer=buffer.slice(end+1);}
                if(buffer.length>32768)throw new Error('invalid_completion_stream');
                if(done){if(buffer.trim())frame(buffer.trim());break;}
              }
              if(!finished)throw new Error('incomplete_completion_stream');
            }finally{await reader.cancel().catch(()=>{});}
          }else{body=await response.json();}
          if (generation!==this.generation || !this.current(s)) return;
          if (!response.ok || !body.ok) throw new Error(body.error_code||'ai_request_failed');
          const text=typeof body.completion==='string'?body.completion:'';
          this.latency=Date.now()-began;
          cache.set(key,{text,model:body.model||'',time:Date.now()}); while (cache.size>24) cache.delete(cache.keys().next().value);
          this.model=body.model||'';
          this.status('');
          if(streaming && !body.cached && text)this.reveal(s,text,true);
          else this.show(s,text);
        } catch(error) {
          if (generation===this.generation && !this.disposed) {
            clearTimeout(this.revealTimer);this.revealTimer=0;this.revealed='';
            this.suggestion=null;this.view.dispatch({effects:effect.of(null)});
            this.status(error.name==='AbortError'?'':'editor.aiCompletionUnavailable'); this.pausedUntil=Date.now()+10000;
          }
        } finally { clearTimeout(deadline); if (this.pending?.task===task) this.pending=null; }
      }
      update(u) {
        if (u.docChanged || u.selectionSet || u.focusChanged) {
          const previous=this.suggestion;
          this.cancel(); this.suggestion=null; this.status('');
          if(u.docChanged && previous && u.transactions.some(tr=>tr.isUserEvent('input.type'))){
            const changes=[];u.changes.iterChanges((from,to,_a,_b,insert)=>changes.push({from,to,text:insert.toString()}));
            const change=changes[0];
            if(changes.length===1 && change.from===previous.pos && change.to===previous.pos && previous.text.startsWith(change.text)){
              const next=this.snapshot(),remaining=previous.text.slice(change.text.length);
              if(next && remaining)queueMicrotask(()=>this.show(next,remaining));
              return;
            }
          }
          if (u.docChanged && u.transactions.some(tr=>tr.isUserEvent('input.type')||tr.isUserEvent('delete'))) {
            const now=Date.now();if(this.lastInput && now-this.lastInput<1500)this.typingInterval=.7*this.typingInterval+.3*(now-this.lastInput);
            this.lastInput=now;this.schedule();
          }
        }
      }
      accept() {
        const s=this.suggestion; if (!s || !this.current(s)) return false;
        this.cancel(); this.suggestion=null; this.status('');
        this.view.dispatch({changes:{from:s.pos,insert:s.text},selection:{anchor:s.pos+s.text.length},effects:effect.of(null),userEvent:'input.complete'});
        return true;
      }
      destroy() { this.disposed=true; this.cancel(); this.status(''); instances.delete(this.view); }
    }
    return [field,CM.ViewPlugin.fromClass(Controller),CM.EditorView.domEventHandlers({
      compositionstart(_e,v) { instances.get(v)?.clear(); },
      compositionend(_e,v) { setTimeout(()=>instances.get(v)?.schedule(),50); },
      blur(_e,v) { instances.get(v)?.clear(); },
    })];
  }
  const reset=()=>{cache.clear(); if(cmView) instances.get(cmView)?.clear();};
  window.addEventListener('readmd:ai-config-changed',reset);
  document.addEventListener('visibilitychange',()=>{if(document.hidden) reset();});
  return {extension,reset,accept:v=>instances.get(v)?.accept()||false,
    dismiss(v) {const c=instances.get(v); if(!c?.suggestion&&!c?.pending&&!c?.timer)return false; c.clear(); return true;},
    request:v=>{instances.get(v)?.schedule(true);return true;}};
})();
