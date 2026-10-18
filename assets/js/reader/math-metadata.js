'use strict';
// Render the stored mathematical properties after normal TeX typesetting.
// The MathJax item/root identity is retained so identical occurrences remain
// independently editable. A stale metadata hash is never applied to new text.
window.ReadMDMathMetadata=(()=>{
  const cache=new Map();let timer,epoch=0;
  function source(){return state.editing&&cmView?cmView.state.doc.toString():(state.fixed??state.original??'');}
  async function run(){
    if(!window.ReadMDFormula||typeof renderMathDocument!=='function')return;
    const serial=++epoch,text=source();if(!text.includes('<!--readmd:math:v1:'))return;
    const ranges=ReadMDFormula.ranges(text).filter(range=>range.dialect!=='dsl');
    for(const pane of [document.getElementById('content'),document.getElementById('preview-pane')]){
      if(!pane?.isConnected)continue;
      const items=renderedMathItemsWithin(pane);let cursor=0;
      for(const item of items){
        const index=ranges.findIndex((r,i)=>i>=cursor&&repairLatex(r.tex).trim()===item.math.trim());if(index<0)continue;cursor=index+1;
        const range=ranges[index],match=/^\s*(<!--readmd:math:v1:[A-Za-z0-9+/=]+-->)/.exec(text.slice(range.to));if(!match)continue;
        const root=item.typesetRoot;if(!root?.isConnected)continue;
        const display=range.mode==='block',metrics=mathLayoutMetrics(root,display);
        const key=range.tex+'\0'+match[1]+'\0'+display+'\0'+JSON.stringify(metrics);if(root.dataset.readmdMetadata===key)continue;
        try{
          let value=cache.get(key);
          if(!value){const r=await apiFetch('/api/math/parse',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({source:range.tex,dialect:'latex',metadata:match[1],display,...metrics})});
            if(!r.ok)continue;value=await r.json();if(value.metadata_status!=='current')continue;
            if(serial!==epoch||text!==source()||!root.isConnected)return;
            if(cache.size>=16)cache.delete(cache.keys().next().value);cache.set(key,value);
          }
          const rendered=await renderMathDocument(value,display);
          if(serial!==epoch||text!==source()||!root.isConnected)return;
          if(rendered.querySelector('[data-mml-node="merror"]'))continue;
          root.replaceChildren(rendered);root.dataset.readmdMetadata=key;
        }catch(_){/* Source rendering remains usable; metadata stays recoverable. */}
      }
    }
  }
  function schedule(){clearTimeout(timer);timer=setTimeout(run,80);}
  function invalidateFonts(){
    epoch++;cache.clear();
    document.querySelectorAll('[data-readmd-metadata]').forEach(e=>delete e.dataset.readmdMetadata);
    schedule();
  }
  return {schedule,run,invalidateFonts};
})();
