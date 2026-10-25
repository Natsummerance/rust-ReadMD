'use strict';
// One read-only CSL projection is used by reader and editor preview. The editor
// always retains the original citation keys and author-controlled metadata.
window.ReadMDPublication=(()=>{
  let cached=null,pending=null,styleSession=null,rangeCache=null,paged=null,epoch=0,cacheGeneration=0,citationNavigation=0;
  const projections=[];
  const citationBindings=new WeakMap(),citationContainers=new WeakSet();
  const bibliographyMeasurements=new Map();
  const authorSurfaces=new Map();
  const trackingBindings=new WeakMap();
  const baselineBindings=new WeakMap();
  const decorationBindings=new Map();
  const decorationSides=['top','left','bottom','right'];
  const decorationProperties=new Set(['background-color',...decorationSides.flatMap(side=>[`border-${side}-color`,`border-${side}-style`,`border-${side}-width`,`padding-${side}`])]);
  const headingNumberBindings=new WeakMap();
  const pagedHeadingBindings=new WeakMap();
  const untrackedSelector='code,pre,.readmd-citation,.rd-fnref,mjx-container,.readmd-dsl-math,[data-readmd-math-renderer],math';
  const untrackedProperty='--readmd-untracked-letter-spacing';
  const scheduleMeasurement=callback=>typeof requestIdleCallback==='function'?requestIdleCallback(callback,{timeout:200}):requestAnimationFrame(()=>callback({timeRemaining:()=>4}));
  const cancelMeasurement=id=>{if(typeof cancelIdleCallback==='function')cancelIdleCallback(id);else cancelAnimationFrame(id);};
  const editorView=()=>typeof cmView==='undefined'?null:cmView;
  const activeTab=()=>typeof getActiveTab==='function'?getActiveTab():null;
  const normalizeSource=source=>source.replace(/\r\n?/g,'\n');
  function currentSource(){
    if(state.editing&&typeof getEditContent==='function')return getEditContent();
    return activeTab()?.content??state.fixed??state.original??'';
  }
  function origin(){return {tab:state.activeTabId,document:activeTab(),file:state.file||'',view:state.editing?editorView():null,editing:Boolean(state.editing)};}
  function sameDocument(session){return state.activeTabId===session.tab&&activeTab()===session.document&&(state.file||'')===session.file;}
  function currentOrigin(session,source){return sameDocument(session)&&Boolean(state.editing)===session.editing&&(!session.editing||editorView()===session.view)&&currentSource()===source;}
  function projectionOrigin(session,source){return sameDocument(session)&&Boolean(state.editing)===session.editing&&(!session.editing||editorView()===session.view)&&normalizeSource(currentSource())===normalizeSource(source);}
  function discardPrepared(){
    ++cacheGeneration;++citationNavigation;
    for(const record of bibliographyMeasurements.values())record.stop();
    for(const container of authorSurfaces.keys())window.ReadMDAuthorText?.apply(container,null);
    authorSurfaces.clear();
    for(const target of decorationBindings.keys())restoreDecoration(target);
    cached=null;pending=null;projections.length=0;rangeCache=null;
  }
  function editableSource(owner){return owner.document?.isDirty?owner.document.content:state.original;}
  function readerOriginal(source,owner){
    if(owner.editing||source!==currentSource())return null;
    const original=editableSource(owner);return typeof original==='string'&&original!==source?original:null;
  }
  async function preparedDocument(source,base){
    const r=await apiFetch('/api/academic/document',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({content:source,baseDir:base,view:true}),signal:AbortSignal.timeout(15000)}),data=await r.json();
    if(!r.ok||!data.ok){const error=new Error('publication');error.response=data;throw error;}return data;
  }
  function projectionRecord(source,data,request){
    return {source,epoch:request.epoch,generation:request.generation,owner:request.owner,source_citations:data.source_citations||[],projection:data.projection||null,revision:data.projection?.revision||data.revision||'',numbering:data.source_numbering||null};
  }
  async function prepare(source,file=''){
    const base=String(file||'').replace(/[\\/][^\\/]*$/,'');
    const owner=origin(),navigationSource=readerOriginal(source,owner),previous=pending||cached||projections.at(-1);
    // Preserved CRLF preview and LF completion may share one document. Every
    // other source/view change invalidates its previous projection authority.
    if(previous&&(!projectionOrigin(previous.owner,source)||normalizeSource(previous.source)!==normalizeSource(source)))discardPrepared();
    const front=/^\uFEFF?---[ \t]*\r?\n([\s\S]*?)\r?\n(?:---|\.\.\.)[ \t]*(?:\r?\n|$)/.exec(source)?.[1]||'';
    const hasProfile=/(?:^|[\s{,])(?:readmd|"readmd"|'readmd')\s*:/.test(front);
    if(!hasProfile&&!/:label\[|:ref\[|readmd-math|data-readmd-text|<!-- readmd:layout |<!-- readmd-section:|<!-- readmd-numbering:|<!--readmd-numbering-import:|<!--readmd:citation-(?:resources|import):/i.test(source)&&!/^\uFEFF?---[ \t]*\r?\n[\s\S]*?^(?:bibliography|citation-resources)\s*:/m.test(source)){++epoch;discardPrepared();return source;}
    if(cached?.source===source&&cached.base===base&&cached.navigationSource===navigationSource&&cached.epoch===epoch&&Date.now()-cached.time<2000){cached.owner=owner;return cached.content;}
    if(pending?.source===source&&pending.base===base&&pending.navigationSource===navigationSource){pending.owner=owner;return pending.promise;}
    const request={source,base,owner,navigationSource,epoch:++epoch,generation:cacheGeneration};request.promise=(async()=>{
      try{
        // File loading may repair tables and normalize CRLF for display. Keep
        // that Rust-rendered display, and obtain a second authoritative Rust
        // projection for the exact original opened by the editor.
        const [data,navigationData]=await Promise.all([preparedDocument(source,base),navigationSource===null?null:preparedDocument(navigationSource,base)]);
        if(request.generation!==cacheGeneration||!projectionOrigin(request.owner,source)){
          if(request.generation===cacheGeneration&&request.epoch===epoch)discardPrepared();
          return source;
        }
        const content=typeof data.reader_content==='string'?data.reader_content:data.content;
        if(typeof content!=='string')throw new Error('publication');
        const record={...projectionRecord(source,data,request),base,content,time:Date.now(),navigationSource,navigation:null,diagnostics:data.diagnostics||[],labels:data.source_labels||[],layout:data.layout||{},authorText:data.author_text||null};
        if(navigationData){const original=projectionRecord(navigationSource,navigationData,request);if(sameCitations(record,original))record.navigation=original;}
        projections.push(record);if(projections.length>4)projections.shift();
        if(request.epoch===epoch){
          cached=record;
          const warns=(data.warnings||[]).map(text=>{const at=text.indexOf(': '),code=at<0?text:text.slice(0,at),key=at<0?'':text.slice(at+2),label=i18n.t('warn.'+code,{key});if(label!=='warn.'+code)return label;const error=i18n.t('error.'+code,{key});return error==='error.'+code?i18n.t('academic.prepareFailed'):error;});
          if(warns.length&&currentOrigin(request.owner,source))showToast(warns.join('\n'),6000);
        }
        return content;
      }catch(error){if(request.generation===cacheGeneration&&request.epoch===epoch){discardPrepared();if(currentOrigin(request.owner,source))showToast(apiMessage(error.response,'academic.prepareFailed'));}return source;}
      finally{if(pending===request)pending=null;}
    })();pending=request;return request.promise;
  }
  async function labels(source,file=''){
    await prepare(source,file);
    const base=String(file||'').replace(/[\\/][^\\/]*$/,'');
    return cached?.source===source&&cached.base===base?cached.labels:[];
  }
  function decorationValues(rule){
    if(!rule)return null;
    const values={},fill=rule['background-color'];
    if(fill!==undefined){if(typeof fill!=='string'||!/^#[a-f0-9]{6}$/i.test(fill))return null;values['background-color']=fill;}
    for(const side of decorationSides){
      const color=rule[`border-${side}-color`],style=rule[`border-${side}-style`],width=rule[`border-${side}-width`],space=rule[`padding-${side}`];
      if([color,style,width,space].every(value=>value===undefined))continue;
      if(typeof color!=='string'||!/^#[a-f0-9]{6}$/i.test(color)||style!=='solid'||typeof width!=='string'||!/^(?:\d+(?:\.\d*)?|\.\d+)pt$/.test(width)||typeof space!=='string'||!/^(?:0|[1-9]\d*)pt$/.test(space))return null;
      const pt=Number(width.slice(0,-2)),gap=Number(space.slice(0,-2));
      if(!Number.isFinite(pt)||pt<0.25||pt>12||!Number.isSafeInteger(gap)||gap>31)return null;
      values[`border-${side}-color`]=color;values[`border-${side}-style`]=style;values[`border-${side}-width`]=width;values[`padding-${side}`]=space;
    }
    return Object.keys(values).length?values:null;
  }
  function restoreDecoration(target){
    const binding=decorationBindings.get(target);if(!binding)return;
    for(const [property,value,priority]of binding.saved){if(value)target.style.setProperty(property,value,priority);else target.style.removeProperty(property);}
    delete target.dataset.readmdDecoration;decorationBindings.delete(target);
  }
  function applyDecoration(target,key,rule){
    if(target?.tagName!=='P'||target.closest?.('blockquote,li,td,th,pre,figcaption,.rd-footnotes,.footnotes,.readmd-bibliography,[data-readmd-footnote],[role="doc-endnotes"]'))return;
    const values=decorationValues(rule);if(!values)return;
    const signature=JSON.stringify(values),old=decorationBindings.get(target);
    if(old?.key===key&&old.signature===signature)return;if(old)restoreDecoration(target);
    const saved=[];
    for(const [property,value]of Object.entries(values)){
      saved.push([property,target.style.getPropertyValue(property),target.style.getPropertyPriority(property)]);
      target.style.setProperty(property,value);
    }
    target.dataset.readmdDecoration=key;decorationBindings.set(target,{key,signature,saved});
  }
  function restoreTracking(target){
    const binding=trackingBindings.get(target);if(!binding)return;
    for(const [element,property,value,priority]of binding.saved){if(value)element.style.setProperty(property,value,priority);else element.style.removeProperty(property);}
    delete target.dataset.readmdTracking;trackingBindings.delete(target);
  }
  function applyTracking(target,key,value){
    // Only the current Rust Reader map may request bounded native CSS. Do not
    // split text nodes or replace any code/citation/math subtree.
    if(target.tagName!=='P'||typeof value!=='string'||!/^(-?(?:\d+(?:\.\d*)?|\.\d+)(?:e[+-]?\d+)?)pt$/i.test(value))return;
    const pt=Number(value.slice(0,-2));if(!Number.isFinite(pt)||pt< -5||pt>20)return;
    const old=trackingBindings.get(target);if(old?.key===key)return;if(old)restoreTracking(target);
    const saved=[],remember=(element,property)=>saved.push([element,property,element.style.getPropertyValue(property),element.style.getPropertyPriority(property)]);
    remember(target,'letter-spacing');remember(target,untrackedProperty);
    const inherited=getComputedStyle(target).letterSpacing;
    const protectedValues=[...target.querySelectorAll(untrackedSelector)].map(element=>[element,getComputedStyle(element).letterSpacing]);
    target.style.setProperty(untrackedProperty,inherited);
    for(const [element,spacing]of protectedValues){remember(element,untrackedProperty);element.style.setProperty(untrackedProperty,spacing);}
    target.style.setProperty('letter-spacing',value);target.dataset.readmdTracking=key;trackingBindings.set(target,{key,saved});
  }
  function restoreBaseline(target){
    const binding=baselineBindings.get(target);if(!binding)return;
    for(const span of binding.spans){if(span.parentNode)span.replaceWith(...span.childNodes);}
    delete target.dataset.readmdBaseline;baselineBindings.delete(target);
  }
  function applyBaseline(target,key,value){
    // Offset complete text nodes only. Code, CSL, notes and math retain their
    // own font/placement and DOM identity; links and semantic sup/sub survive.
    if(!/^(?:P|H[1-6])$/.test(target.tagName)||typeof value!=='string'||!/^(-?(?:\d+(?:\.\d*)?|\.\d+)(?:e[+-]?\d+)?)pt$/i.test(value))return;
    const pt=Number(value.slice(0,-2));if(!Number.isFinite(pt)||pt< -72||pt>72)return;
    const old=baselineBindings.get(target);if(old?.key===key&&old.value===value)return;if(old)restoreBaseline(target);
    const protectedNodes=new Set(target.querySelectorAll(untrackedSelector)),nodes=[];
    const walk=node=>{if(protectedNodes.has(node))return;if(node.nodeType===3){if(node.nodeValue)nodes.push(node);return;}for(const child of [...node.childNodes||[]])walk(child);};walk(target);
    const spans=[];
    for(const node of nodes){const span=(target.ownerDocument||document).createElement('span');span.dataset.readmdBaselineRun=key;span.style.setProperty('position','relative');span.style.setProperty('top',(-pt)+'pt');node.replaceWith(span);span.appendChild(node);spans.push(span);}
    target.dataset.readmdBaseline=key;baselineBindings.set(target,{key,value,spans});
  }
  function currentLayout(){
    const source=currentSource(),record=[cached,...[...projections].reverse()].find(record=>record?.generation===cacheGeneration&&sameDocument(record.owner)&&record.source===source);
    return record?.layout;
  }
  function refreshBaseline(container){
    if(!container?.querySelectorAll||container.isConnected===false)return;
    const activeLayout=currentLayout(),targets=[...(container.matches?.('[data-readmd-baseline]')?[container]:[]),...container.querySelectorAll('[data-readmd-baseline]')];
    for(const target of targets){
      const binding=baselineBindings.get(target);if(!binding)continue;
      // Cold TeX is still a text node when layout first runs. MathJax may
      // replace it inside that shifted span; unwrap before protecting its new
      // container. Preserve every generated node and all unrelated CSS.
      const {key,value}=binding;restoreBaseline(target);
      if(activeLayout?.[key]?.['--readmd-baseline-shift']===value)applyBaseline(target,key,value);
    }
  }
  function currentNumberingRecord(){
    const source=currentSource();
    return [cached,...[...projections].reverse()].find(record=>record?.numbering&&record.generation===cacheGeneration&&currentOrigin(record.owner,source)&&record.source===source)||null;
  }
  function projectedHeadingNumbers(record){
    if(record.numberingIds)return record.numberingIds;
    const mapping=new Map(),plan=record.numbering?.headings;
    if(!Array.isArray(plan)||typeof marked==='undefined'||typeof ReadMDHeadings==='undefined')return mapping;
    // Reader pages carry globally allocated identities from the projected
    // Markdown. Match those real top-level rows to the authoritative source
    // plan, so a page never restarts its heading counter at one.
    const source=record.content.replace(/^\uFEFF?---[ \t]*\r?\n[\s\S]*?\r?\n(?:---|\.\.\.)[ \t]*(?:\r?\n|$)/,front=>'\n'.repeat((front.match(/\n/g)||[]).length));
    const lines=new Set();let line=1;
    for(const token of marked.lexer(source,{gfm:true})){if(token.type==='heading')lines.add(line);line+=(token.raw?.match(/\n/g)||[]).length;}
    const rows=ReadMDHeadings.fromMarkdown(record.content).filter(row=>lines.has(row.sourceLine));
    if(rows.length===plan.length&&rows.every((row,index)=>row.level===plan[index].level))for(let index=0;index<rows.length;index++)mapping.set(rows[index].id,plan[index]);
    record.numberingIds=mapping;return mapping;
  }
  function syncPagedHeadingIds(record,targetId=null,pageIndex=state.pagination?.currentPage){
    if(state.pagination?.rawContent!==record.content)return targetId;
    const mapping=projectedHeadingNumbers(record),changes=new Map(),rows=state.pagination.allHeadings||[];
    for(const row of rows){
      let binding=pagedHeadingBindings.get(row);
      if(!binding||binding.content!==record.content){binding={content:record.content,projectedId:row.id};pagedHeadingBindings.set(row,binding);}
      const entry=mapping.get(binding.projectedId);
      if(entry&&row.level===entry.level&&row.id!==entry.id){changes.set(row.id,entry.id);row.id=entry.id;}
    }
    // buildToc may already have materialized the active page's links. Lazy
    // groups retain these same row objects and use the resolved IDs later.
    if(changes.size)for(const link of document.getElementById('toc-list')?.querySelectorAll('[data-heading-id]')||[]){
      const id=changes.get(link.dataset.headingId);if(id){link.dataset.headingId=id;link.setAttribute('href','#'+id);}
    }
    if(rows.some(row=>row.pageIndex===pageIndex&&row.id===targetId))return targetId;
    return changes.get(targetId)||targetId;
  }
  function preparePagedHeadings(buildOutline,targetId,pageIndex){
    const record=currentNumberingRecord();
    if(!record||state.pagination?.rawContent!==record.content)return targetId;
    // The first page has no global IDs yet. Build its existing outline before
    // binding DOM headings, then resolve both outline and links in one pass.
    buildOutline();return syncPagedHeadingIds(record,targetId,pageIndex);
  }
  function applyHeadingNumbering(container){
    if(container?.isConnected===false)return;
    const headings=[...container.querySelectorAll('h1,h2,h3,h4,h5,h6')];
    for(const heading of headings){
      const previous=headingNumberBindings.get(heading);if(!previous)continue;
      previous.span?.remove();
      if(previous.global===undefined)delete heading.dataset.readmdGlobalId;else heading.dataset.readmdGlobalId=previous.global;
      headingNumberBindings.delete(heading);
    }
    const record=currentNumberingRecord(),plan=record?.numbering?.headings;if(!Array.isArray(plan))return;
    const real=headings.filter(heading=>!heading.closest?.('blockquote,li,pre,code,figcaption,.rd-footnotes,.readmd-bibliography,[data-readmd-footnote],[role="doc-endnotes"]'));
    const complete=real.length===plan.length&&real.every((heading,index)=>Number(heading.tagName.slice(1))===plan[index].level);
    const mapped=projectedHeadingNumbers(record),bySourceId=new Map(plan.map(entry=>[entry.id,entry]));
    syncPagedHeadingIds(record);
    const pageIds=new Set(state.pagination?.rawContent===record.content?(state.pagination.allHeadings||[]).filter(row=>pagedHeadingBindings.get(row)?.content===record.content).map(row=>row.id):[]);
    for(let index=0;index<real.length;index++){
      const heading=real[index],known=heading.dataset.readmdGlobalId||heading.id||heading.dataset.readmdExplicitId;
      const entry=complete?plan[index]:pageIds.has(known)?bySourceId.get(known):mapped.get(known)||bySourceId.get(known);
      if(!entry||Number(heading.tagName.slice(1))!==entry.level||typeof entry.id!=='string')continue;
      if(heading.dataset.readmdHeadingText===undefined){
        const authorTitle=[...heading.childNodes||[]].filter(node=>!(node.nodeType===1&&node.classList?.contains('rd-anchor'))).map(node=>node.textContent).join('');
        heading.dataset.readmdHeadingText=typeof ReadMDHeadings==='undefined'?authorTitle:ReadMDHeadings.attributes(authorTitle).text;
      }
      const global=heading.dataset.readmdGlobalId;heading.dataset.readmdGlobalId=entry.id;
      let span=null;
      if(entry.numbering&&typeof entry.numbering.text==='string'&&[' ','\t',''].includes(entry.numbering.suffix)){
        span=(heading.ownerDocument||document).createElement('span');span.className='readmd-heading-number';span.dataset.readmdHeadingNumber='';span.textContent=entry.numbering.text+entry.numbering.suffix;heading.prepend(span);
      }
      headingNumberBindings.set(heading,{global,span});
    }
  }
  function applyLayout(container){
    applyHeadingNumbering(container);
    const activeLayout=currentLayout();
    // A sparse source edit can remove just the marker while retaining its
    // unchanged paragraph DOM. Restore that paragraph's exact prior CSS.
    for(const target of container.querySelectorAll('[data-readmd-tracking]')){const binding=trackingBindings.get(target);if(binding&&(!activeLayout||!Object.hasOwn(activeLayout,binding.key)))restoreTracking(target);}
    for(const target of container.querySelectorAll('[data-readmd-baseline]')){const binding=baselineBindings.get(target);if(binding&&activeLayout?.[binding.key]?.['--readmd-baseline-shift']!==binding.value)restoreBaseline(target);}
    for(const [target,binding]of decorationBindings){if(target.isConnected===false||container.contains?.(target)&&JSON.stringify(decorationValues(activeLayout?.[binding.key]))!==binding.signature)restoreDecoration(target);}
    for(const marker of container.querySelectorAll('[data-readmd-layout]')){
      const inline=marker.dataset.readmdLayoutInline==='true';
      // A marker can consume only the current source map, including ordinary
      // local CSS; a map retained from a previous document is never a fallback.
      const rule=activeLayout?.[marker.dataset.readmdLayout],target=inline?marker:marker.nextElementSibling;
      if(!rule||!target)continue;
      for(const [key,value]of Object.entries(rule)){
        if(key==='letter-spacing'){if(activeLayout?.[marker.dataset.readmdLayout]?.['letter-spacing']===value)applyTracking(target,marker.dataset.readmdLayout,value);}
        else if(key==='--readmd-baseline-shift'){if(activeLayout?.[marker.dataset.readmdLayout]?.['--readmd-baseline-shift']===value)applyBaseline(target,marker.dataset.readmdLayout,value);}
        else if(!decorationProperties.has(key))target.style.setProperty(key,value);
      }
      applyDecoration(inline?marker.closest?.('p'):target,marker.dataset.readmdLayout,rule);
      if(inline)target.style.display='block';else marker.remove();
    }
    // The sanitizer drops style strings. Restore only bounded numeric CSL
    // spacing supplied in typed attributes; values above ten remain valid.
    const spacing=(value,min)=>/^(0|[1-9]\d*)$/.test(value||'')&&Number.isSafeInteger(Number(value))&&Number(value)>=min&&Number(value)<=10000?Number(value):null;
    for(const section of container.querySelectorAll('.readmd-bibliography')){
      const line=spacing(section.dataset.cslLineSpacing,1),entry=spacing(section.dataset.cslEntrySpacing,0);
      if(line!==null)section.style.lineHeight=String(line);
      if(entry!==null&&line!==null){
        const nodes=[...section.querySelectorAll('.readmd-bibliography-entry')];
        for(const [index,node]of nodes.entries())node.style.marginBottom=(index<nodes.length-1?entry*line:0)+'em';
      }
    }
    const authorRecord=[cached,...[...projections].reverse()].find(record=>record?.generation===cacheGeneration&&sameDocument(record.owner)&&record.source===currentSource());
    window.ReadMDAuthorText?.apply(container,authorRecord?.authorText||null);
    if(authorRecord?.authorText?.runs?.length)authorSurfaces.set(container,authorRecord);else authorSurfaces.delete(container);
    measureBibliographies(container);
    bindCitations(container);
  }
  function measureBibliographies(container){
    const old=bibliographyMeasurements.get(container);old?.stop();
    const sections=[...container.querySelectorAll('.readmd-bibliography-complex')];
    if(!sections.length)return;
    const record={generation:cacheGeneration,disposed:false,job:0,serial:0,observer:null};
    const current=()=>!record.disposed&&record.generation===cacheGeneration&&container.isConnected;
    record.stop=()=>{record.disposed=true;record.serial++;if(record.job)cancelMeasurement(record.job);record.observer?.disconnect();if(bibliographyMeasurements.get(container)===record)bibliographyMeasurements.delete(container);};
    bibliographyMeasurements.set(container,record);
    function start(){
      if(!current())return record.stop();
      const serial=++record.serial;if(record.job)cancelMeasurement(record.job);
      const jobs=sections.filter(section=>section.isConnected&&container.contains(section)).map(section=>({section,left:[...section.querySelectorAll('.readmd-csl-layout-left')],index:0,width:0}));
      for(const job of jobs)job.section.dataset.cslMeasurement='pending';
      function step(deadline){
        record.job=0;if(!current()||record.serial!==serial)return;
        const began=performance.now();let processed=0;
        while(jobs.length&&(processed===0||(performance.now()-began<4&&deadline.timeRemaining()>0))){
          const job=jobs[0];if(!job.section.isConnected||!container.contains(job.section)){jobs.shift();continue;}
          if(job.index>=job.left.length){
            if(Number.isFinite(job.width)&&job.width>=0){job.section.style.setProperty('--readmd-csl-label-preferred-width',job.width+'px');job.section.dataset.cslMeasurement='ready';}
            else job.section.dataset.cslMeasurement='failed';
            jobs.shift();continue;
          }
          const left=job.left[job.index++];if(!left.isConnected||!job.section.contains(left))continue;
          let clone;
          try{
            const font=getComputedStyle(left);clone=left.cloneNode(true);clone.inert=true;clone.setAttribute('aria-hidden','true');clone.removeAttribute('id');
            for(const node of clone.querySelectorAll('[id]'))node.removeAttribute('id');
            Object.assign(clone.style,{position:'absolute',visibility:'hidden',pointerEvents:'none',inset:'0 auto auto 0',width:'max-content',maxWidth:'none',minWidth:'0',gridColumn:'auto',fontFamily:font.fontFamily,fontSize:font.fontSize,fontStyle:font.fontStyle,fontWeight:font.fontWeight,fontVariant:font.fontVariant,fontStretch:font.fontStretch,lineHeight:font.lineHeight});
            for(const column of clone.querySelectorAll('.readmd-csl-layout-columns')){column.style.gridTemplateColumns='max-content max-content';column.style.width='max-content';}
            job.section.append(clone);job.width=Math.max(job.width,clone.getBoundingClientRect().width);
          }catch(_){job.width=NaN;}
          finally{clone?.remove();}
          processed++;
        }
        if(jobs.length)record.job=scheduleMeasurement(step);
      }
      record.job=scheduleMeasurement(step);
    }
    record.refresh=start;
    let lastWidth=container.getBoundingClientRect().width;
    if(typeof ResizeObserver==='function'){record.observer=new ResizeObserver(()=>{if(!current())return record.stop();const width=container.getBoundingClientRect().width;if(Math.abs(width-lastWidth)>.5){lastWidth=width;start();}});record.observer.observe(container);}
    document.fonts?.ready.then(()=>{if(current())start();});start();
  }
  function checkedSpan(record,index){
    const projection=record.projection,span=projection?.citations?.[index]?.source?.span,legacy=record.source_citations?.[index];
    if(!span||!legacy||!record.revision||projection.revision!==record.revision||projection.source_utf16!==record.source.length)return null;
    if(!Number.isSafeInteger(span.from)||!Number.isSafeInteger(span.to)||span.from<0||span.to<=span.from||span.to>record.source.length)return null;
    if(record.source.slice(span.from,span.to)!==span.source||legacy.from!==span.from||legacy.to!==span.to||legacy.source!==span.source||legacy.revision!==record.revision)return null;
    // A UTF-16 boundary cannot bisect a CRLF or a supplementary character.
    for(const at of [span.from,span.to]){
      if(at&&record.source[at-1]==='\r'&&record.source[at]==='\n')return null;
      if(at&&/[\uD800-\uDBFF]/.test(record.source[at-1])&&/[\uDC00-\uDFFF]/.test(record.source[at]||''))return null;
    }
    return {index,from:span.from,to:span.to};
  }
  function sourceSpan(record,node){
    const indexText=node.dataset.readmdCitationIndex;if(!/^(0|[1-9]\d*)$/.test(indexText||''))return null;
    const span=checkedSpan(record,Number(indexText));
    return span&&node.dataset.readmdSourceRevision===record.revision&&node.dataset.readmdSourceFrom===String(span.from)&&node.dataset.readmdSourceTo===String(span.to)?span:null;
  }
  function sameCitations(display,original){
    const left=display.projection?.citations,right=original.projection?.citations;
    if(!Array.isArray(left)||!Array.isArray(right)||left.length!==right.length||display.source_citations.length!==left.length||original.source_citations.length!==right.length)return false;
    let displayEnd=0,originalEnd=0;
    // Reading-order identities come from the two typed models. Never search
    // citation text: identical citations must retain their ordinal occurrence.
    for(let index=0;index<left.length;index++){
      const a=left[index],b=right[index],from=checkedSpan(display,index),to=checkedSpan(original,index);
      if(!from||!to||from.from<displayEnd||to.from<originalEnd||normalizeSource(a.source.span.source)!==normalizeSource(b.source.span.source))return false;
      if(!Array.isArray(a.source.items)||!Array.isArray(b.source.items)||!a.source.items.length||!b.source.items.length)return false;
      const identity=citation=>JSON.stringify([citation.source.items.map(item=>item.cite),citation.source.context,citation.rich,citation.inline_rich,citation.note_rich,citation.note_label,citation.rendered]);
      if(identity(a)!==identity(b))return false;
      displayEnd=from.to;originalEnd=to.to;
    }
    return true;
  }
  function editorRange(source,span,view){
    if(view.state.doc.toString()!==normalizeSource(source))return null;
    let before=0,within=0;
    for(let at=0;at<span.to;at++)if(source[at]==='\r'&&source[at+1]==='\n'){if(at<span.from)before++;else within++;at++;}
    return {anchor:span.from-before,head:span.to-before-within};
  }
  function currentCitation(session,node){
    const {record,displayRecord,owner,container}=session;
    return record.generation===cacheGeneration&&displayRecord.generation===cacheGeneration&&node.isConnected&&container.contains(node)&&sameDocument(owner)&&currentSource()===displayRecord.source&&(owner.editing||editableSource(owner)===record.source)&&sourceSpan(record,node);
  }
  async function locateCitation(node){
    const session=citationBindings.get(node);if(!session||!currentCitation(session,node))return false;
    const {record,owner}=session,serial=++citationNavigation;
    let view=editorView();
    if(owner.editing){
      if(!state.editing||view!==owner.view)return false;
    }else{
      if(state.editing||typeof toggleEdit!=='function')return false;
      const editable=editableSource(owner);
      if(editable!==record.source)return false;
      const opening=toggleEdit(),openEpoch=typeof editorOpenEpoch==='undefined'?null:editorOpenEpoch;await opening;
      if(openEpoch!==null&&editorOpenEpoch!==openEpoch)return false;
      if(serial!==citationNavigation||!sameDocument(owner)||!state.editing||currentSource()!==record.source)return false;
      view=editorView();
    }
    if(!view||serial!==citationNavigation||!sameDocument(owner)||currentSource()!==record.source)return false;
    const span=sourceSpan(record,node),selection=span&&editorRange(record.source,span,view);if(!selection)return false;
    view.dispatch({selection,scrollIntoView:true});view.focus();return true;
  }
  function bindCitations(container){
    // Completion requests may use LF while preview uses the preserved CRLF.
    // Bind the exact committed projection, never whichever request finished last.
    const source=currentSource(),records=[cached,...[...projections].reverse()].filter(record=>record?.projection&&record.generation===cacheGeneration&&sameDocument(record.owner)&&record.source===source);
    if(!records.length)return;const owner=origin();
    for(const node of container.querySelectorAll('.readmd-citation[data-readmd-citation-index]')){
      const old=citationBindings.get(node),displayRecord=records.find(record=>sourceSpan(record,node)||(old?.displayRecord===record&&sourceSpan(old.record,node)));if(!displayRecord)continue;
      const record=displayRecord.navigationSource===null?displayRecord:displayRecord.navigation;
      if(!record||(!owner.editing&&editableSource(owner)!==record.source))continue;
      const index=Number(node.dataset.readmdCitationIndex),span=checkedSpan(record,index);if(!span)continue;
      node.dataset.readmdSourceFrom=String(span.from);node.dataset.readmdSourceTo=String(span.to);node.dataset.readmdSourceRevision=record.revision;
      citationBindings.set(node,{record,displayRecord,owner,container});node.tabIndex=0;node.setAttribute('role','button');
      node.setAttribute('aria-label',i18n.t('codebar.editTip')+': '+node.textContent);node.title=i18n.t('codebar.editTip');
    }
    if(citationContainers.has(container))return;citationContainers.add(container);
    const target=event=>{const node=event.target?.closest?.('.readmd-citation[data-readmd-citation-index]');return node&&citationBindings.get(node)?.container===container?node:null;};
    const activate=(event,node)=>{event.preventDefault();event.stopPropagation();locateCitation(node).catch(()=>{});};
    container.addEventListener('click',event=>{if(event.button!==0||event.ctrlKey||event.metaKey||event.altKey||event.shiftKey)return;const node=target(event);if(node)activate(event,node);},true);
    container.addEventListener('keydown',event=>{if(event.repeat||event.ctrlKey||event.metaKey||event.altKey||event.shiftKey||!['Enter',' '].includes(event.key))return;const node=target(event);if(node&&event.target===node)activate(event,node);},true);
  }
  // Shared with the reader pre-pass. Preserve a line-level source map while
  // removing only the four continuation columns of a note definition.
  function noteDefinitions(source){
    const lines=String(source).split('\n'),starts=[];let offset=0,fence=null,comment=false;const notes=[];
    for(const line of lines){starts.push(offset);offset+=line.length+1;}
    for(let i=0;i<lines.length;i++){
      const line=lines[i];
      if(fence){if(new RegExp('^ {0,3}'+fence[0]+'{'+fence.length+',}[ \\t\\r]*$').test(line))fence=null;continue;}
      if(comment){if(line.includes('-->'))comment=false;continue;}
      if(/^ {0,3}<!--/.test(line)){comment=!line.includes('-->');continue;}
      const opening=/^ {0,3}(`{3,}|~{3,})/.exec(line);if(opening){fence=opening[1];continue;}
      const definition=/^ {0,3}\[\^([^\]\s]+)\]:[ \t]*(.*)/.exec(line);if(!definition)continue;
      const content=[definition[2]],map=[{from:0,sourceFrom:starts[i]+definition[0].length-definition[2].length}];let end=i+1,bodyOffset=definition[2].length+1;
      while(end<lines.length){
        const continuation=lines[end];let at=0,column=0;
        if(continuation.trim()){
          while(at<continuation.length&&column<4){if(continuation[at]===' ')column++;else if(continuation[at]==='\t')column+=4-column%4;else break;at++;}
          if(column<4)break;
        }
        const text=continuation.slice(at);map.push({from:bodyOffset,sourceFrom:starts[end]+at});content.push(text);bodyOffset+=text.length+1;end++;
      }
      notes.push({label:definition[1],startLine:i,endLine:end,content:content.join('\n'),map});i=end-1;
    }
    return notes;
  }
  function metadataRanges(source,editorState=null){
    if(!source.includes('<!-- readmd:layout v1 '))return [];
    const normalized=source.replace(/\r\n?/g,'\n'),CM=window.ReadMDCodeMirror;
    editorState ||= window.cmView?.state;
    if(editorState?.doc.toString()!==normalized)editorState=null;
    if(rangeCache?.source===source&&(!editorState||rangeCache.syntax))return rangeCache.ranges;
    const ranges=[];let from=0;
    for(const token of marked.lexer(normalized,{gfm:true})){
      if(token.type==='html'){
        const match=/^( {0,3})(<!-- readmd:layout v1 (\{[^\r\n]*\}) -->)/.exec(token.raw);
        if(match)try{const value=JSON.parse(match[3]);if(typeof value.id==='string'&&/^block-[a-f0-9]{24}$/.test(value.id))ranges.push({from:from+match[1].length,to:from+match[1].length+match[2].length,layout:true});}catch(_){}
      }from+=token.raw.length;
    }
    // The editor's Markdown syntax tree supplies exact nested comment ranges;
    // searching for their text would fold identical examples in code blocks.
    if(editorState&&CM?.syntaxTree){
      CM.syntaxTree(editorState).iterate({enter(node){
        if(!['HTMLBlock','HTMLTag','Comment'].includes(node.name))return;
        const raw=normalized.slice(node.from,node.to),match=/^(<!-- readmd:layout v1 (\{[^\n]*\}) -->)/.exec(raw);if(!match)return;
        try{const value=JSON.parse(match[2]);if(typeof value.id==='string'&&/^block-[a-f0-9]{24}$/.test(value.id)&&!ranges.some(r=>r.from===node.from))ranges.push({from:node.from,to:node.from+match[1].length,layout:true,inline:node.name!=='HTMLBlock'});}catch(_){}
      }});
      // CodeMirror does not implement footnote continuation syntax: it sees
      // later paragraphs as indented code. Parse the dedented note body using
      // the same grammar, so code examples stay literal and real comments
      // map back to their exact original position.
      for(const note of noteDefinitions(normalized)){
        if(!note.content.includes('<!-- readmd:layout v1 '))continue;
        CM.markdownLanguage.parser.parse(note.content).iterate({enter(node){
          if(!['HTMLBlock','HTMLTag','Comment'].includes(node.name))return;
          const raw=note.content.slice(node.from,node.to),match=/^(<!-- readmd:layout v1 (\{[^\n]*\}) -->)/.exec(raw);if(!match)return;
          try{
            const value=JSON.parse(match[2]);if(typeof value.id!=='string'||!/^block-[a-f0-9]{24}$/.test(value.id))return;
            const line=note.map.findLast(row=>row.from<=node.from),start=line.sourceFrom+node.from-line.from;
            if(!ranges.some(r=>r.from===start))ranges.push({from:start,to:start+match[1].length,layout:true,inline:node.name!=='HTMLBlock'});
          }catch(_){}
        }});
      }
    }
    ranges.sort((a,b)=>a.from-b.from);
    if(source.includes('\r')){
      const boundaries=new Set(ranges.flatMap(r=>[r.from,r.to])),offsets=new Map();let normalized=0;
      for(let raw=0;raw<=source.length;raw++,normalized++){
        if(boundaries.has(normalized))offsets.set(normalized,raw);
        if(source[raw]==='\r'&&source[raw+1]==='\n')raw++;
      }
      for(const range of ranges){range.from=offsets.get(range.from);range.to=offsets.get(range.to);}
    }
    rangeCache={source,ranges,syntax:Boolean(editorState)};return ranges;
  }
  // One sparse line-spacing control for export settings and source blocks.
  // A passive read never migrates an old preset or materializes typed fields.
  const lineSpacingControls=(()=>{
    const controls=new WeakMap(),has=(o,k)=>Object.prototype.hasOwnProperty.call(o||{},k),copy=v=>JSON.parse(JSON.stringify(v));let serial=0;
    const modes=['multiple','exact','atLeast'],modeKey={multiple:'export.lineSpacingMultiple',exact:'export.lineSpacingExact',atLeast:'export.lineSpacingAtLeast'};
    function describe(style={},scope='global'){
      const maximum=scope==='local'?3:2.5,typed=has(style,'lineSpacing')&&style.lineSpacing!=null,legacy=has(style,'lineHeight')&&style.lineHeight!=null;
      if(!typed&&!legacy)return{mode:'',value:'',family:'inherit',valid:true};
      const raw=typed?style.lineSpacing:null,mode=typed?raw?.mode:'multiple',field=mode==='multiple'?'multiple':'pt',value=typed?raw?.[field]:style.lineHeight;
      const shape=!typed||raw&&typeof raw==='object'&&!Array.isArray(raw)&&Object.keys(raw).length===2&&has(raw,'mode')&&has(raw,field);
      const valid=Boolean(shape&&modes.includes(mode)&&typeof value==='number'&&Number.isFinite(value)&&value>=(mode==='multiple'?1:6)&&value<=(mode==='multiple'?maximum:144)&&!(typed&&legacy));
      return{mode:modes.includes(mode)?mode:'invalid',rawMode:mode,value,family:typed?'typed':'legacy',valid};
    }
    function inheritGlobal(base,over,out){
      const explicit=over?.typography;if(!explicit||typeof explicit!=='object'||!out.typography)return out;
      if(has(explicit,'lineSpacing')){
        if(explicit.lineSpacing==null)delete out.typography.lineSpacing;
        else{out.typography.lineSpacing=copy(explicit.lineSpacing);if(!has(explicit,'lineHeight'))delete out.typography.lineHeight;}
      }else if(has(explicit,'lineHeight'))delete out.typography.lineSpacing;
      return out;
    }
    function supported(format,engine){return ['docx','tex','latex'].includes(format)||format==='pdf'&&['rust','typst','xelatex','pdflatex'].includes(engine||'rust');}
    function issue(options,format,engine){const style=options?.typography||{},description=describe(style);if(!description.valid)return'error.publication_local_style_invalid';return description.family==='typed'&&!supported(format,engine)?'error.publication_line_spacing_unsupported':'';}
    function mount(parent,{scope='global',style={},t=key=>i18n.t(key),onChange,getFormat}={}){
      const wrapper=document.createElement('fieldset');wrapper.className='readmd-line-spacing-control';wrapper.dataset.lineSpacingControl=scope;
      const legend=document.createElement('legend');legend.textContent=t('export.lineHeight');legend.id='readmd-line-spacing-'+(++serial);wrapper.append(legend);
      const row=document.createElement('div');row.className='readmd-line-spacing-row';wrapper.append(row);
      const mode=document.createElement('select');mode.dataset.lineSpacing='mode';mode.setAttribute('aria-labelledby',legend.id);row.append(mode);
      const valueWrap=document.createElement('div');valueWrap.className='readmd-line-spacing-value';row.append(valueWrap);
      const value=document.createElement('input');value.dataset.lineSpacing='value';value.step='any';valueWrap.append(value);
      const unit=document.createElement('span');unit.dataset.lineSpacing='unit';unit.setAttribute('aria-hidden','true');valueWrap.append(unit);
      const hint=document.createElement('p');hint.className='readmd-line-spacing-hint';hint.id=legend.id+'-hint';hint.setAttribute('role','status');wrapper.append(hint);value.setAttribute('aria-describedby',hint.id);
      let original,dirty=false,previous='',values={};
      function sync(){const multiple=mode.value==='multiple';value.min=multiple?'1':'6';value.max=multiple?(scope==='local'?'3':'2.5'):'144';value.disabled=mode.value==='';unit.textContent=multiple?'×':'pt';unit.hidden=value.disabled;value.setAttribute('aria-label',t('export.lineHeight')+' ('+unit.textContent+')');validate(false);}
      function read(){const number=Number(value.value),valid=mode.value===''||modes.includes(mode.value)&&value.value.trim()!==''&&Number.isFinite(number)&&number>=Number(value.min)&&number<=Number(value.max);return{mode:mode.value,value:number,valid:valid&&(dirty||original.valid),changed:dirty,family:dirty?(mode.value===''?'inherit':mode.value==='multiple'?'legacy':'typed'):original.family};}
      function validate(report=false){const current=read(),message=current.valid?'':t('error.publication_local_style_invalid');mode.setCustomValidity(mode.value==='invalid'?message:'');value.setCustomValidity(message);const context=getFormat?.();hint.textContent=!current.valid?message:context&&current.family==='typed'&&!supported(context.format,context.engine)?t('error.publication_line_spacing_unsupported'):'';hint.hidden=!hint.textContent;if(report&&!current.valid){for(let parent=wrapper.parentElement;parent;parent=parent.parentElement)if(parent.tagName==='DETAILS')parent.open=true;wrapper.closest('.exp-sec')?.classList.add('open');if(!mode.reportValidity())return false;value.reportValidity();}return current.valid;}
      function patch(){const current=read();if(!current.valid)return null;if(!dirty)return{};if(current.mode==='')return{lineSpacing:null,lineHeight:null};if(current.mode==='multiple')return{lineSpacing:null,lineHeight:current.value};return{lineHeight:null,lineSpacing:{mode:current.mode,pt:current.value}};}
      // Keep an invalid UI draft visible across engine/format rerenders. This
      // does not authorize export/preset save; both validate before requests.
      function globalPatch(){const valid=patch();if(valid)return valid;if(!dirty||!modes.includes(mode.value))return{};const n=Number(value.value),raw=value.value.trim()!==''&&Number.isFinite(n)?n:value.value;return mode.value==='multiple'?{lineSpacing:null,lineHeight:raw}:{lineHeight:null,lineSpacing:{mode:mode.value,pt:raw}};}
      function reset(next={}){
        original=describe(next,scope);dirty=false;previous=original.mode;values={multiple:typeof next.lineHeight==='number'?next.lineHeight:1.6,exact:18,atLeast:18,[original.mode]:original.value};
        const choices=[['','publication.local.inherit'],...modes.map(m=>[m,modeKey[m]])];if(original.mode==='invalid')choices.push(['invalid',null]);mode.replaceChildren();
        for(const[name,key]of choices){const option=document.createElement('option');option.value=name;option.textContent=key?t(key):String(original.rawMode??t('error.publication_local_style_invalid'));mode.append(option);}
        mode.value=original.mode;value.type=typeof original.value==='number'||original.value===''?'number':'text';value.value=original.value==null?'':String(original.value);sync();
      }
      const api={scope,wrapper,mode,value,read,validate,patch,globalPatch,reset};controls.set(wrapper,api);
      mode.addEventListener('change',()=>{values[previous]=value.value;previous=mode.value;dirty=true;const next=values[mode.value]??'';value.type=typeof next==='number'||next===''||typeof next==='string'&&next.trim()!==''&&Number.isFinite(Number(next))?'number':'text';value.value=String(next);sync();onChange?.(api);});
      const changed=()=>{dirty=true;values[mode.value]=value.value;sync();onChange?.(api);};value.addEventListener('input',changed);value.addEventListener('change',changed);
      reset(style);parent.append(wrapper);return api;
    }
    function each(host,work){for(const wrapper of host?.querySelectorAll('[data-line-spacing-control]')||[]){const control=controls.get(wrapper);if(control)work(control);}}
    function validateExport(host,report=false){let valid=true;each(host,c=>{if(!c.validate(report))valid=false;});return valid;}
    function collectExport(options,host){each(host,c=>{if(c.scope!=='global')return;const patch=c.globalPatch();if(!Object.keys(patch).length)return;options.typography={...(options.typography||{})};for(const[k,v]of Object.entries(patch)){if(v===null)delete options.typography[k];else options.typography[k]=copy(v);}});return options;}
    function refreshExport(host,style){each(host,c=>{if(c.scope==='global')c.reset(style||{});});}
    return{describe,inheritGlobal,supported,issue,mount,validateExport,collectExport,refreshExport};
  })();

  // Optional physical tracking stays absent until explicitly edited. Empty
  // input removes this override; zero is a real request and must survive.
  function physicalTextControls({key,labelKey,marker,min,max,supported,unsupported}){
    const controls=new WeakMap();let serial=0;
    const valid=v=>typeof v==='number'&&Number.isFinite(v)&&v>=min&&v<=max;
    function inheritGlobal(base,over,out){if(over?.typography?.[key]===null&&out.typography){const inherited=base?.typography?.[key];if(inherited==null)delete out.typography[key];else out.typography[key]=inherited;}return out;}
    function issue(options,format,engine){const v=options?.typography?.[key];return v==null?'':!valid(v)?'error.publication_local_style_invalid':!supported(format,engine)?unsupported:'';}
    function mount(parent,{style={},t=key=>i18n.t(key),getFormat,onChange}={}){
      const wrapper=document.createElement('div');wrapper.className='readmd-tracking-control';wrapper.dataset[marker+'Control']='global';
      const label=document.createElement('label'),input=document.createElement('input'),hint=document.createElement('p');
      input.id='readmd-'+marker+'-'+(++serial);input.type='text';input.inputMode='decimal';input.maxLength=64;input.placeholder=t('publication.local.inherit');input.dataset[marker]='value';
      label.setAttribute('for',input.id);label.textContent=t(labelKey);
      hint.id=input.id+'-hint';hint.className='readmd-line-spacing-hint';hint.setAttribute('role','status');input.setAttribute('aria-describedby',hint.id);
      wrapper.append(label,input,hint);let original,dirty=false;
      function read(){const raw=input.value.trim(),empty=raw==='',number=/^[+-]?(?:\d+\.?\d*|\.\d+)(?:e[+-]?\d+)?$/i.test(raw)?Number(raw):NaN;return{empty,raw,value:number,valid:(empty||valid(number))&&(dirty||original==null||valid(original))};}
      function validate(report=false){const current=read(),format=getFormat?.(),message=current.valid?'':t('error.publication_local_style_invalid');input.setCustomValidity(message);input.setAttribute('aria-invalid',String(!current.valid));hint.textContent=message||(!current.empty&&!supported(typeof format==='string'?format:format?.format,format?.engine)?t(unsupported):'');hint.hidden=!hint.textContent;if(report&&!current.valid){wrapper.closest('.exp-sec')?.classList.add('open');input.reportValidity();}return current.valid;}
      function collect(options){if(!dirty)return options;const current=read();options.typography={...(options.typography||{})};if(current.empty)delete options.typography[key];else options.typography[key]=Number.isFinite(current.value)?current.value:current.raw;return options;}
      function reset(next={}){original=next[key];dirty=false;input.value=original==null?'':String(original);validate(false);}
      const api={wrapper,input,read,validate,collect,reset};controls.set(wrapper,api);input.addEventListener('input',()=>{dirty=true;validate(false);onChange?.(api);});
      reset(style);parent.append(wrapper);return api;
    }
    function each(host,fn){for(const element of host?.querySelectorAll('[data-'+marker+'-control]')||[]){const c=controls.get(element);if(c)fn(c);}}
    function validateExport(host,report=false){let result=true;each(host,c=>{if(!c.validate(report))result=false;});return result;}
    function collectExport(options,host){each(host,c=>c.collect(options));return options;}
    function refreshExport(host,style){each(host,c=>c.reset(style));}
    return{issue,inheritGlobal,mount,validateExport,collectExport,refreshExport};
  }
  const trackingControls=physicalTextControls({key:'trackingPt',labelKey:'export.trackingPt',marker:'tracking',min:-5,max:20,supported:format=>format==='docx',unsupported:'error.publication_tracking_unsupported'});
  const baselineControls=physicalTextControls({key:'baselineShiftPt',labelKey:'export.baselineShiftPt',marker:'baseline',min:-72,max:72,supported:(format,engine)=>format==='docx'||format==='pdf'&&(!engine||['rust','typst'].includes(engine)),unsupported:'error.publication_baseline_unsupported'});

  async function blocks(body,signal){
    const response=await apiFetch('/api/publication/blocks',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify(body),signal});
    const value=await response.json();if(!response.ok||!value.ok){const error=new Error('publication');error.response=value;throw error;}return value;
  }
  function closeStyle(){
    const session=styleSession;styleSession=null;session?.numbering?.destroy();session?.inlineText?.destroy();session?.controller.abort();if(session?.modal){ReadMDModal.close(session.modal);session.modal.remove();if(editorView()===session.view&&state.activeTabId===session.tab&&state.editing)session.view.focus();}
  }
  const current=s=>styleSession===s&&state.editing&&editorView()===s.view&&state.activeTabId===s.tab&&s.view.state.doc.toString()===s.source;
  async function openStyle(target=null){
    closeStyle();const view=editorView();if(!state.editing||!view)return;
    const session={view,source:view.state.doc.toString(),tab:state.activeTabId,controller:new AbortController(),modal:null};styleSession=session;
    const t=(key)=>i18n.t(key);
    try{
      const data=await blocks({content:session.source},session.controller.signal);if(!current(session)){if(styleSession===session)closeStyle();return;}
      const pos=view.state.selection.main.head,model=data.document;
      if(target&&(target.source!==session.source||target.revision!==model.revision||target.view!==view||target.tab!==session.tab)){showToast(t('error.publication_local_style_target'));closeStyle();return;}
      if(model.diagnostics.length){showToast(t('error.'+model.diagnostics[0].code));closeStyle();return;}
      const node=model.nodes.find(n=>n.editable&&(target?n.id===target.id&&n.from===target.from&&n.to===target.to:pos>=n.from&&pos<=n.to));
      if(!node){showToast(t('publication.local.pick'));closeStyle();return;}
      const modal=document.createElement('div');modal.id='publication-style-modal';modal.role='dialog';modal.setAttribute('aria-labelledby','publication-style-title');modal.className='hidden modal-overlay';modal.dataset.backdrop='static';session.modal=modal;
      const box=document.createElement('form');box.className='publication-style-box';modal.append(box);
      const title=document.createElement('h2');title.id='publication-style-title';title.textContent=t('publication.local.title');box.append(title);
      const hint=document.createElement('p');hint.textContent=t('publication.local.hint');box.append(hint);
      const preview=document.createElement('p');preview.className='publication-style-target';preview.textContent=node.title;box.append(preview);
      const content=document.createElement('div');content.className='publication-style-content';box.append(content);
      const fields=document.createElement('div');fields.className='publication-style-fields';content.append(fields);
      const advanced=document.createElement('details'),summary=document.createElement('summary');summary.textContent=t('ai.advanced');advanced.append(summary);const extra=document.createElement('div');extra.className='publication-style-fields';advanced.append(extra);content.append(advanced);
      const inputs=[];
      function field(section,key,label,type,min,max,parent=fields){
        const wrapper=document.createElement('label'),caption=document.createElement('span');caption.textContent=t(label);wrapper.append(caption);parent.append(wrapper);
        let input;if(type==='boolean'||type==='align'){
          input=document.createElement('select');const options=type==='align'?[['left','export.alignLeft'],['center','export.alignCenter'],['right','export.alignRight'],['justify','export.alignJustify']]:[['true','app.enabled'],['false','app.disabled']];
          for(const [value,key]of [['','publication.local.inherit'],...options]){const option=document.createElement('option');option.value=value;option.textContent=t(key);input.append(option);}
        }else{input=document.createElement('input');input.type=type;if(type==='number'){input.min=min;input.max=max;input.step='any';}else input.maxLength=128;input.placeholder=t('publication.local.inherit');}
        input.dataset.local=section+'.'+key;input.value=node.style[section]?.[key]??'';wrapper.append(input);inputs.push({input,section,key,type});
      }
      field('text','size','export.fontSize','number',6,72);
      field('paragraph','align','export.align','align');
      if(node.kind!=='math')field('text','font','export.fontFamily','text');
      field('text','color','export.bodyColor','text');inputs.at(-1).input.pattern='#[0-9a-fA-F]{6}';inputs.at(-1).input.maxLength=7;
      if(node.kind!=='math'){field('text','bold','export.bold','boolean');field('text','italic','editor.italic','boolean');}
      for(const [key,label,min,max]of [['before','publication.local.before',0,100],['after','publication.local.after',0,100],['left','publication.local.left',0,60],['right','publication.local.right',0,60]])field('paragraph',key,label,'number',min,max,extra);
      if(node.kind!=='math')for(const [key,label,min,max]of [['firstLine','export.firstLineIndent',0,60],['hanging','publication.local.hanging',0,60]])field('paragraph',key,label,'number',min,max,extra);
      for(const[key,label]of [['keepNext','export.keepNext'],['keepLines','export.keepLines'],['pageBreakBefore','publication.local.pageBreak'],...(node.kind==='math'?[]:[['widowControl','export.widowControl']])])field('paragraph',key,label,'boolean',null,null,extra);
      if(node.kind==='paragraph')field('text','trackingPt','export.trackingPt','number',-5,20,extra);
      if(['paragraph','heading','caption'].includes(node.kind))field('text','baselineShiftPt','export.baselineShiftPt','number',-72,72,extra);
      const lineSpacing=node.kind==='math'?null:lineSpacingControls.mount(extra,{scope:'local',style:node.style.paragraph||{},t});
      session.numbering=window.ReadMDPublicationNumbering?.mount(extra,{source:session.source,getSource:()=>session.view.state.doc.toString(),isCurrent:()=>current(session),view:session.view,signal:session.controller.signal,onApplied:closeStyle});
      session.inlineText=window.ReadMDPublicationInlineText?.mount(extra,{source:session.source,isCurrent:()=>current(session),view:session.view,signal:session.controller.signal,onApplied:closeStyle,t});
      const error=document.createElement('p');error.className='publication-style-error';error.role='alert';box.append(error);
      const actions=document.createElement('div');actions.className='publication-style-actions';box.append(actions);
      const reset=document.createElement('button');reset.type='button';reset.dataset.localAction='reset';reset.className='tb-btn';reset.textContent=t('publication.local.reset');actions.append(reset);
      const cancel=document.createElement('button');cancel.type='button';cancel.className='tb-btn';cancel.dataset.modalClose='';cancel.textContent=t('common.cancel');cancel.onclick=closeStyle;actions.append(cancel);
      const apply=document.createElement('button');apply.type='submit';apply.className='tb-btn accent';apply.textContent=t('publication.local.apply');actions.append(apply);
      async function commit(patch){
        if(!current(session)){error.textContent=t('error.publication_local_style_target');return;}
        reset.disabled=apply.disabled=true;
        try{
          const result=await blocks({action:'apply',content:session.source,base_revision:model.revision,id:node.id,patch},session.controller.signal);
          if(!current(session)){error.textContent=t('error.publication_local_style_target');return;}
          if(result.result.changed)session.view.dispatch({changes:result.transaction.edits,userEvent:'input.publication'});
          closeStyle();if(editorView()===session.view&&state.activeTabId===session.tab)session.view.focus();
        }catch(e){if(styleSession===session)error.textContent=apiMessage(e.response,'error.publication_local_style_invalid');}
        finally{reset.disabled=apply.disabled=false;}
      }
      box.onsubmit=event=>{event.preventDefault();if(lineSpacing&&!lineSpacing.validate(true)||!box.reportValidity())return;const patch={text:{},paragraph:{}};for(const {input,section,key,type}of inputs){patch[section][key]=input.value===''?null:type==='number'?Number(input.value):type==='boolean'?input.value==='true':input.value;}if(lineSpacing)Object.assign(patch.paragraph,lineSpacing.patch());commit(patch);};
      reset.onclick=()=>commit({text:null,paragraph:null});document.body.append(modal);ReadMDModal.open(modal);
    }catch(e){if(styleSession===session){if(!session.controller.signal.aborted)showToast(apiMessage(e.response,'error.publication_local_style_invalid'));closeStyle();}}
  }
  function stopPaged(){
    const session=paged;paged=null;if(!session)return;session.controller.abort();session.pageController?.abort();
    if(session.task)apiFetch('/api/task/cancel',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({id:session.task})}).catch(()=>{});
  }
  function showPaged(session){
    if(paged!==session||!session.current())return;
    const t=(key,p)=>i18n.t(key,p),{pane,data}=session,map=data.sourceMap;
    pane.setAttribute('aria-busy','false');pane.replaceChildren();
    const controls=document.createElement('div');controls.className='publication-page-controls';pane.append(controls);
    const previous=document.createElement('button'),next=document.createElement('button'),label=document.createElement('span');
    for(const [button,delta,symbol,key]of [[previous,-1,'←','pagination.prevPage'],[next,1,'→','pagination.nextPage']]){
      button.type='button';button.className='tb-btn';button.textContent=symbol;button.title=button.ariaLabel=t(key);button.disabled=session.pageLoading||(delta<0?session.page===1:session.page===data.pages);button.onclick=()=>changePagedPage(session,session.page+delta);
    }
    label.textContent=`Typst · ${session.page} / ${data.pages}`;controls.append(previous,label,next);
    const sheet=document.createElement('div');sheet.className='publication-page-sheet';sheet.dataset.publicationRevision=map.revision;const dimensions=map.pages[session.page-1];sheet.style.aspectRatio=dimensions[0]+'/'+dimensions[1];pane.append(sheet);
    const image=document.createElement('img');image.alt=t('export.actualPreview');image.src='data:image/png;base64,'+session.images.get(session.page);sheet.append(image);
    const layer=document.createElement('div');layer.className='publication-page-hits';sheet.append(layer);
    const hits=map.hits.filter(hit=>hit.page===session.page),nodes=new Map(map.nodes.map(node=>[node.id,node]));let active='';
    const position=(element,hit)=>{const [x,y,w,h]=hit.rect;element.style.left=x/dimensions[0]*100+'%';element.style.top=y/dimensions[1]*100+'%';element.style.width=w/dimensions[0]*100+'%';element.style.height=h/dimensions[1]*100+'%';};
    const highlight=id=>{if(active===id)return;active=id;layer.querySelectorAll('.publication-page-highlight').forEach(el=>el.remove());for(const hit of hits.filter(hit=>hit.id===id)){const el=document.createElement('div');el.className='publication-page-highlight';position(el,hit);layer.append(el);}sheet.style.cursor=id?'pointer':'';};
    const select=id=>{if(paged!==session||!session.current())return;const node=nodes.get(id);if(!node?.editable)return;session.view.dispatch({selection:{anchor:node.from,head:node.to},scrollIntoView:true});openStyle({...node,source:session.source,revision:map.revision,view:session.view,tab:session.tab});};
    const hitAt=event=>{const bounds=sheet.getBoundingClientRect(),x=(event.clientX-bounds.left)/bounds.width*dimensions[0],y=(event.clientY-bounds.top)/bounds.height*dimensions[1];return hits.find(hit=>{const [left,top,w,h]=hit.rect;return x>=left&&x<=left+w&&y>=top&&y<=top+h;});};
    sheet.onpointermove=event=>highlight(hitAt(event)?.id||'');sheet.onpointerleave=()=>highlight('');sheet.onclick=event=>{const hit=hitAt(event);if(hit)select(hit.id);};
    const seen=new Set();for(const hit of hits){if(seen.has(hit.id))continue;seen.add(hit.id);const node=nodes.get(hit.id);if(!node)continue;
      const button=document.createElement('button');button.type='button';button.className='publication-page-target';button.dataset.publicationNode=hit.id;button.ariaLabel=t('publication.local.title')+': '+node.title;position(button,hit);button.onfocus=()=>highlight(hit.id);button.onblur=()=>highlight('');button.onclick=event=>{event.stopPropagation();select(hit.id);};layer.append(button);
    }
  }
  async function changePagedPage(session,page){
    if(session.pageLoading||paged!==session||!session.current()||page<1||page>session.data.pages)return;
    session.pageLoading=true;showPaged(session);const controller=new AbortController();session.pageController=controller;const timeout=setTimeout(()=>controller.abort(),15000);
    try{
      if(!session.images.has(page)){
        window.ReadMDTask.loading(session.pane,'export.previewGenerating');
        const response=await apiFetch('/api/export/preview',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({key:session.data.key,page}),signal:controller.signal}),data=await response.json();
        if(!response.ok||!data.ok||!data.png)throw Error('export.previewFailed');session.images.set(page,data.png);while(session.images.size>3)session.images.delete(session.images.keys().next().value);
      }
      if(paged===session&&session.current())session.page=page;
    }catch(error){if(paged===session&&session.current())showToast(i18n.t('export.previewFailed'));}
    finally{clearTimeout(timeout);session.pageLoading=false;if(paged===session&&session.current())showPaged(session);}
  }
  async function renderPaged(pane,{view,tab,current}){
    const sourceDocument=view.state.doc,source=sourceDocument.toString(),baseDir=state.dir||'',suggestedName=state.sourceName||'document',fallback=JSON.parse(JSON.stringify(state.export?.options||{}));fallback.pdf={...(fallback.pdf||{}),engine:'typst'};
    const signature=JSON.stringify([source,baseDir,suggestedName,fallback]);
    pane.classList.add('publication-paged');
    if(paged?.signature===signature&&paged.view===view&&paged.document===sourceDocument&&paged.tab===tab&&sameDocument(paged.owner)){paged.current=current;return;}
    stopPaged();const session={pane,source,signature,view,document:sourceDocument,owner:origin(),tab,current,controller:new AbortController(),task:crypto.randomUUID(),images:new Map(),page:1,pageLoading:false};paged=session;
    const valid=()=>paged===session&&!session.controller.signal.aborted&&session.current()&&view.state.doc===sourceDocument&&sameDocument(session.owner)&&editorView()===view;
    window.ReadMDTask.loading(pane,'export.previewGenerating');
    try{
      if(!valid())return;
      const profileResponse=await apiFetch('/api/publication/profile',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({content:source}),signal:session.controller.signal}),profile=await profileResponse.json();
      if(!valid())return;
      if(!profileResponse.ok||profile.ok!==true||profile.diagnostics?.length)throw Object.assign(Error('publication'),{response:profile.error_code?profile:{ok:false,error_code:profile.diagnostics?.[0]?.code||'publication_metadata_type',...profile.diagnostics?.[0]}});
      if(!window.ReadMDPublicationProfile)throw Object.assign(Error('publication'),{response:{ok:false,error_code:'publication_metadata_type'}});
      const options=window.ReadMDPublicationProfile.create(profile.options,fallback).request(fallback);
      const prepared=await prepareExportFigures(source,options,session.controller.signal);if(!valid())return;
      const response=await apiFetch('/api/export/preview',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({format:'pdf',content:source,baseDir,suggestedName,options:prepared,task_id:session.task}),signal:session.controller.signal}),data=await response.json();
      if(!valid())return;
      if(response.ok&&data.ok&&data.sourceMap&&!data.firstPage){
        if(await ReadMDCapabilities.ensure('pdfium')){
          if(!valid())return;
          const rendered=await apiFetch('/api/export/preview',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({key:data.key,page:1}),signal:session.controller.signal}),page=await rendered.json();
          if(!valid())return;
          if(rendered.ok&&page.ok)data.firstPage=page.png;
        }
      }
      if(!response.ok||!data.ok||!data.sourceMap||!data.firstPage)throw Object.assign(Error('publication'),{response:data});
      session.task='';session.data=data;session.images.set(1,data.firstPage);showPaged(session);
    }catch(error){
      if(!valid())return;
      pane.setAttribute('aria-busy','false');pane.replaceChildren();const message=document.createElement('p');message.role='alert';message.textContent=apiMessage(error.response,'export.previewFailed');pane.append(message);
      const retry=document.createElement('button');retry.type='button';retry.className='tb-btn';retry.textContent=i18n.t('plugin.retry');retry.onclick=()=>{stopPaged();renderPaged(pane,{view,tab,current:session.current});};pane.append(retry);
    }
  }
  function refreshLayout(){
    for(const record of bibliographyMeasurements.values())record.refresh?.();
    for(const [container,record]of authorSurfaces){
      const current=container.isConnected!==false&&record.generation===cacheGeneration&&sameDocument(record.owner)&&record.source===currentSource();
      window.ReadMDAuthorText?.apply(container,current?record.authorText:null);
      if(!current)authorSurfaces.delete(container);
    }
  }
  return {lineSpacing:lineSpacingControls,tracking:trackingControls,baseline:baselineControls,prepare,labels,preparePagedHeadings,applyLayout,refreshBaseline,refreshLayout,bindCitations,locateCitation,openStyle,metadataRanges,noteDefinitions,renderPaged,stopPaged,diagnostics:()=>cached?.diagnostics||[],projection:()=>cached?.projection||null,sourceCitations:()=>cached?.source_citations||[],revision:()=>cached?.revision||'',clear:()=>{++epoch;discardPrepared();stopPaged();}};
})();
