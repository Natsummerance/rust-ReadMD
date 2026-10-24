'use strict';
window.ReadMDMathProperties=(()=>{
  let schema;
  const tr=k=>i18n.t(k),local=n=>n.name.split(':').pop();
  const isMath=n=>!n.namespace||n.namespace==='http://schemas.openxmlformats.org/officeDocument/2006/math';
  const isSpacing=n=>n.namespace==='http://www.w3.org/1998/Math/MathML'&&local(n)==='mspace';
  function flatten(n){return[n,...n.content.filter(p=>p.type==='element').flatMap(p=>flatten(p.value))];}
  const wordNS='http://schemas.openxmlformats.org/wordprocessingml/2006/main',mathNS='http://schemas.openxmlformats.org/officeDocument/2006/math';
  const wordLabels={b:'editor.bold',i:'editor.italic',sz:'toolbar.fontSize',color:'export.codeColor'};
  const wordLabel=kind=>tr(wordLabels[kind])+(kind==='sz'?' (½ pt)':kind==='color'?' (RRGGBB)':'');
  const elements=n=>(n?.content||[]).filter(p=>p.type==='element').map(p=>p.value);
  const wordValue=(n,name)=>Object.entries(n.attributes).find(([key])=>local({name:key})===name&&n.attribute_namespaces?.[key]===wordNS)?.[1];
  function wordRunSources(root,nodes){
    const ids=new Map(),sources=new Map();for(const n of nodes)ids.set(n.id,(ids.get(n.id)||0)+1);
    function visit(n,path){
      path=[...path,n];
      if(n.namespace===wordNS&&Object.hasOwn(wordLabels,local(n))&&typeof n.id==='string'&&n.id&&ids.get(n.id)===1){
        const pr=path.at(-2),owner=path.at(-3),parents=path.slice(0,-2);
        const leaf=Object.keys(n.attributes).every(key=>key==='xmlns'||key.startsWith('xmlns:')||local({name:key})==='val'&&n.attribute_namespaces?.[key]===wordNS)
          &&Object.keys(n.attributes).filter(key=>local({name:key})==='val'&&n.attribute_namespaces?.[key]===wordNS).length<=1
          &&n.content.every(p=>p.type!=='element'&&(p.type!=='text'||!p.value.trim()));
        const rpr=pr?.namespace===wordNS&&local(pr)==='rPr'&&Object.keys(pr.attributes).every(key=>key==='xmlns'||key.startsWith('xmlns:'))
          &&elements(owner).filter(c=>c.namespace===wordNS&&local(c)==='rPr').length===1
          &&elements(pr).filter(c=>c.namespace===wordNS&&local(c)===local(n)).length===1;
        const style=path.length===5&&root.name==='#root'&&path[1].namespace===wordNS&&local(path[1])==='styles'
          &&owner?.namespace===wordNS&&local(owner)==='style'&&['paragraph','character'].includes(wordValue(owner,'type'))&&!!wordValue(owner,'styleId');
        const direct=parents.length>=3&&root.name==='#root'&&owner?.namespace===mathNS&&['r','ctrlPr'].includes(local(owner))
          &&parents.slice(1).every(c=>c.namespace===mathNS)&&parents.some(c=>c.namespace===mathNS&&local(c)==='oMath')
          &&parents.slice(2).every((c,i)=>schema.elements.find(e=>e.element===local(parents[i+1]))?.children?.includes(local(c)));
        if(leaf&&rpr&&(style||direct)&&path.every(c=>typeof c.id==='string'&&c.id&&ids.get(c.id)===1))sources.set(n.id,{kind:style?'style':'direct',styleId:style?wordValue(owner,'styleId'):null});
      }
      for(const child of elements(n))visit(child,path);
    }
    visit(root,[]);return sources;
  }
  async function mount(d,activate,change){
    const details=document.createElement('details');details.className='formula-properties';
    const summary=document.createElement('summary');summary.textContent=tr('ai.advanced');details.append(summary);d.host.append(details);
    let selection;const matrixColumns=new Map(),matrixPositions=new Map();
    async function refresh(restoreFocus){
      if(!d.rich)return;
      const nodes=flatten(d.rich.root),core=new Set(schema.coreObjects.map(n=>n.element)),wordSources=wordRunSources(d.rich.root,nodes);
      const argumentsAndParagraphs=new Set(['e','num','den','deg','sub','sup','lim','fName','oMathPara']);
      const items=nodes.filter(n=>wordSources.has(n.id)||isSpacing(n)||isMath(n)&&(core.has(local(n))||local(n)==='r'||argumentsAndParagraphs.has(local(n))));
      const oldBody=details.querySelector('.formula-property-body'),openGroups=new Set(Array.from(oldBody?.querySelectorAll('[data-math-container][open]')||[],g=>g.dataset.mathContainer));
      const scroll=oldBody?.querySelector('.formula-property-fields')?.scrollTop||0;
      const focused=restoreFocus||(oldBody?.contains(document.activeElement)?document.activeElement.dataset.mathFocus:null);
      const body=document.createElement('span');body.className='formula-property-body';
      const select=document.createElement('select');select.setAttribute('aria-label',tr('formula.category.structures'));
      select.dataset.mathFocus='structure';
      const counts=new Map();
      for(const n of items){const o=document.createElement('option'),kind=local(n),index=(counts.get(kind)||0)+1,word=wordSources.get(n.id);counts.set(kind,index);o.value=n.id;o.textContent=(word?wordLabel(kind):tr(isSpacing(n)?'formula.mathml.mspace':'formula.omml.'+kind))+(word?' · '+tr('formula.wordRun.'+word.kind):'')+' '+index;o.title=kind+' · '+(word?.styleId?word.styleId+' · ':'')+n.id;select.append(o);}
      if(items.some(n=>n.id===selection))select.value=selection;
      else select.value=(items.find(n=>core.has(local(n)))||items.find(isSpacing)||items.find(n=>local(n)==='r')||items[0])?.id||'';
      selection=select.value;
      select.onchange=()=>{selection=select.value;fields();};body.append(select);
      const fieldsBox=document.createElement('span');fieldsBox.className='formula-property-fields';body.append(fieldsBox);
      async function patch(p){const focus=body.contains(document.activeElement)?document.activeElement.dataset.mathFocus:null;await change(p);await refresh(focus);}
      function button(text,operation){const b=document.createElement('button');b.type='button';b.textContent=text;b.onclick=()=>patch(operation).catch(()=>{d.message.textContent=tr('formula.invalid');});return b;}
      function fields(){
        fieldsBox.replaceChildren();const n=items.find(n=>n.id===select.value);if(!n)return;
        const word=wordSources.get(n.id);
        if(word){
          // Edit only the author's existing Word leaf. Effective cascade values
          // and rendered glyph owners never supply an attribute or source ID.
          const kind=local(n),value=wordValue(n,'val'),boolean=kind==='b'||kind==='i';
          const row=document.createElement('label'),label=document.createElement('span'),input=document.createElement('input');
          row.dataset.mathWordRun=kind;label.textContent=wordLabel(kind);input.type='text';input.autocomplete='off';input.spellcheck=false;input.maxLength=4096;
          input.setAttribute('aria-label',tr('formula.wordRun.'+word.kind)+' · '+wordLabel(kind));input.dataset.mathFocus='word.'+n.id+'.val';input.value=value??'';
          input.onchange=()=>{
            if(input.value===(value??'')&&!input.hasAttribute('aria-invalid'))return;
            if(!boolean&&input.value===''){input.setAttribute('aria-invalid','true');d.valid=false;d.apply.disabled=true;d.message.textContent=apiMessage({error_code:'math_invalid_property'},'formula.invalid');return;}
            return patch({operation:'set_attribute',node_id:n.id,name:'w:val',value:input.value===''?null:input.value}).catch(error=>{
              input.setAttribute('aria-invalid','true');d.message.textContent=apiMessage({error_code:error.message},'formula.invalid');
            });
          };
          row.append(label,input);fieldsBox.append(row);
          if(boolean){const hint=document.createElement('small');hint.textContent=tr('formula.wordRun.'+word.kind+'Toggle');hint.dataset.mathWordHint=word.kind;fieldsBox.append(hint);}
          return;
        }
        if(isSpacing(n)){
          // Keep lexical units/whitespace and explicit zero intact. The same
          // bounded source-owned SetAttribute API validates every edit; this
          // UI neither reparses dimensions nor replaces a containing DSL slot.
          for(const [name,key] of [['width','img.width'],['height','img.height'],['depth','formula.mathml.depth']]){
            const row=document.createElement('label'),label=document.createElement('span'),input=document.createElement('input');
            row.dataset.mathSpace=name;label.textContent=tr(key);input.type='text';input.autocomplete='off';input.spellcheck=false;input.maxLength=128;
            input.setAttribute('aria-label',tr('formula.mathml.mspace')+' '+tr(key));input.dataset.mathFocus='mspace.'+name;input.value=n.attributes[name]??'';
            input.onchange=()=>patch({operation:'set_attribute',node_id:n.id,name,value:input.value===''?null:input.value}).catch(error=>{
              // Keep a rejected value visible so it can be corrected. Source,
              // revision and the last valid geometry remain in the local draft.
              input.setAttribute('aria-invalid','true');d.message.textContent=apiMessage({error_code:error.message},'formula.invalid');
            });
            row.append(label,input);fieldsBox.append(row);
          }
          return;
        }
        if(local(n)==='m'){
          const rows=n.content.filter(p=>p.type==='element'&&local(p.value)==='mr');
          const columns=rows[0]?.value.content.filter(p=>p.type==='element'&&local(p.value)==='e').length||1;
          for(const axis of ['row','column']){
            const name=axis==='row'?tr('formula.omml.mr'):tr('formula.column'),count=axis==='row'?rows.length:columns,key=n.id+':'+axis;
            const label=document.createElement('span');label.textContent=name+' ('+count+')';
            const index=document.createElement('input');index.type='number';index.min='1';index.max=String(count+1);index.value=String(Math.min(count+1,matrixPositions.get(key)||1));index.setAttribute('aria-label',name);
            index.dataset.mathFocus='matrix-axis-'+axis;
            const row=document.createElement('label');row.dataset.mathMatrixAxis=axis;row.append(label,index);
            const actions=[];
            function sync(){for(const {b,insert}of actions)b.disabled=!index.validity.valid||(insert?count>=(axis==='row'?256:64):count<=1||Number(index.value)>count);}
            index.oninput=()=>{if(index.validity.valid)matrixPositions.set(key,Number(index.value));sync();};
            for(const insert of [true,false]){const b=button(insert?'+':'−',{});b.setAttribute('aria-label',tr(insert?'editor.insert':'ai.delete')+' '+name);b.title=b.getAttribute('aria-label');actions.push({b,insert});b.onclick=()=>{if(b.disabled||!index.validity.valid)return;matrixPositions.set(key,Number(index.value));patch({operation:'matrix_axis',node_id:n.id,axis,index:Number(index.value)-1,insert}).catch(()=>{d.message.textContent=tr('formula.invalid');});};row.append(b);}sync();fieldsBox.append(row);
          }
          const row=document.createElement('label'),label=document.createElement('span');label.textContent=tr('formula.column');
          const index=document.createElement('input');index.type='number';index.min='1';index.max=String(columns);index.value=String(Math.min(columns,matrixColumns.get(n.id)||1));index.setAttribute('aria-label',tr('formula.column'));
          const align=document.createElement('select');align.setAttribute('aria-label',tr('export.align'));
          index.dataset.mathFocus='matrix-column';align.dataset.mathFocus='matrix-alignment';
          for(const [value,key]of [['left','export.alignLeft'],['center','export.alignCenter'],['right','export.alignRight']]){const option=document.createElement('option');option.value=value;option.textContent=tr(key);align.append(option);}
          const children=(node,kind)=>node?.content.filter(p=>p.type==='element'&&local(p.value)===kind).map(p=>p.value)||[];
          const groups=children(children(children(n,'mPr')[0],'mcs')[0],'mc');
          function sync(){const at=Number(index.value)-1;let start=0;align.value='center';for(const group of groups){const props=group.content.find(p=>p.type==='element'&&local(p.value)==='mcPr')?.value;const val=key=>{const item=props?.content.find(p=>p.type==='element'&&local(p.value)===key)?.value;return item?.attributes[Object.keys(item.attributes).find(k=>k.split(':').pop()==='val')];};const count=Number(val('count')||1);if(at>=start&&at<start+count){align.value=val('mcJc')||'center';break;}start+=count;}}
          index.oninput=()=>{if(index.validity.valid)matrixColumns.set(n.id,Number(index.value));sync();};sync();align.onchange=()=>patch({operation:'matrix_column_alignment',node_id:n.id,index:Number(index.value)-1,alignment:align.value}).catch(()=>{sync();d.message.textContent=tr('formula.invalid');});
          row.dataset.mathMatrixAlignment='';row.append(label,index,align);fieldsBox.append(row);
        }
        if(['sSup','sSub','sSubSup','sPre'].includes(local(n))){
          const kinds=document.createElement('select');kinds.setAttribute('aria-label',tr('formula.template.scripts'));
          kinds.dataset.mathFocus='script-kind';
          for(const kind of ['sSup','sSub','sSubSup','sPre']){const o=document.createElement('option');o.value=kind;o.textContent=tr('formula.omml.'+kind);kinds.append(o);}kinds.value=local(n);kinds.onchange=()=>patch({operation:'convert_script',node_id:n.id,kind:kinds.value}).catch(()=>{d.message.textContent=tr('formula.invalid');});fieldsBox.append(kinds);
        }
        const definition=schema.elements.find(e=>e.element===local(n));
        for(const container of (definition?.children||[]).filter(p=>p.endsWith('Pr'))){
          const props=schema.elements.find(e=>e.element===container);if(!props)continue;
          const group=document.createElement('details');group.dataset.mathContainer=container;group.open=openGroups.has(container);const title=document.createElement('summary');title.textContent=tr('formula.omml.'+container);title.title=container;group.append(title);
          const existing=n.content.find(p=>p.type==='element'&&isMath(p.value)&&local(p.value)===container)?.value;
          for(const property of props.children){
            const def=schema.elements.find(e=>e.element===property),valueDef=def?.attributes.find(a=>a.name==='m:val');if(!valueDef)continue;
            const row=document.createElement('label'),code=document.createElement('span');code.textContent=tr('formula.omml.'+property);row.dataset.mathProperty=container+'.'+property;row.title=container+'.'+property;
            const old=existing?.content.find(p=>p.type==='element'&&isMath(p.value)&&local(p.value)===property)?.value;
            const value=old?.attributes[Object.keys(old.attributes).find(k=>k.split(':').pop()==='val'&&(!old.attribute_namespaces?.[k]||old.attribute_namespaces[k]==='http://schemas.openxmlformats.org/officeDocument/2006/math'))];
            const enumName=/EnumValue<DocumentFormat.OpenXml.Math.(.+)>/.exec(valueDef.type)?.[1];
            const characterProperty=['begChr','endChr','sepChr','chr'].includes(property);
            const facets=schema.enumerations.find(e=>e.name===enumName)?.facets;
            const input=document.createElement(facets?'select':'input');input.setAttribute('aria-label',tr('formula.omml.'+container)+' '+tr('formula.omml.'+property));
            input.dataset.mathFocus=container+'.'+property;
            if(facets){const blank=document.createElement('option');blank.value='';blank.textContent='—';input.append(blank);for(const f of facets){const option=document.createElement('option');option.value=f.Value;option.textContent=tr('formula.ommlEnum.'+enumName+'.'+(f.Name||f.Value));input.append(option);}}
            else input.type=valueDef.type.includes('Int')?'number':'text';
            input.value=value??'';
            input.onchange=()=>patch({operation:'set_property',node_id:n.id,container,property,value:input.value===''&&!characterProperty?null:input.value}).catch(()=>{input.value=value??'';d.message.textContent=tr('formula.invalid');});
            row.append(code,input);
            if(characterProperty&&old){
              // Empty characters hide a fence/operator; removing the override
              // restores Office's default. These are distinct author actions.
              const reset=button('↺',{operation:'set_property',node_id:n.id,container,property,value:null});
              reset.dataset.mathDefault=container+'.'+property;
              reset.setAttribute('aria-label',tr('export.restoreDefaults')+': '+tr('formula.omml.'+property));reset.title=reset.getAttribute('aria-label');
              row.append(reset);
            }
            group.append(row);
          }
          if(group.children.length>1)fieldsBox.append(group);
        }
      }
      const old=details.querySelector('.formula-property-body');if(old)old.replaceWith(body);else details.append(body);fields();fieldsBox.scrollTop=scroll;
      if(focused)Array.from(body.querySelectorAll('[data-math-focus]')).find(e=>e.dataset.mathFocus===focused)?.focus({preventScroll:true});
    }
    details.addEventListener('toggle',async()=>{
      if(!details.open)return;
      try{if(!schema){const r=await apiFetch('/api/math/schema');if(!r.ok)throw Error('schema');schema=(await r.json()).schema;}
        await activate();await refresh();
      }catch(_){d.message.textContent=tr('formula.invalid');}
    });
    return {refresh,details};
  }
  return {mount,flatten,local,isMath,isSpacing};
})();
