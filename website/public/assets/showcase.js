(() => {
 'use strict';
 const HISTORY_COPY={
  en:{loading:'Historical recordings · source details are loading.',scope:'These clips show a historical recording snapshot. Current UI and later feature changes require separate verification.',hint:date=>`${date} historical recording · software version not verified. These clips only verify their recorded inventory.`,changed:' Later changes to this feature are not verified by this recording.',all:'Recorded inventory'},
  'zh-CN':{loading:'历史录制 · 正在加载来源说明。',scope:'这些短片展示历史录制快照，当前界面和后续功能变更需另行验证。',hint:date=>`${date} 历史录制 · 软件版本未验证。短片仅验证当时的录制清单。`,changed:' 本功能后续变更未由这段录制验证。',all:'录制清单'},
  'zh-TW':{loading:'歷史錄製 · 正在載入來源說明。',scope:'這些短片展示歷史錄製快照，目前介面及後續功能變更需另行驗證。',hint:date=>`${date} 歷史錄製 · 軟體版本未驗證。短片僅驗證當時的錄製清單。`,changed:' 本功能後續變更未由這段錄製驗證。',all:'錄製清單'},
  ja:{loading:'過去の録画 · 出典情報を読み込み中です。',scope:'これらの動画は過去の録画時点の画面です。現在の画面やその後の機能変更は、別途検証が必要です。',hint:date=>`${date} の録画 · ソフトウェアのバージョンは未確認です。動画が検証するのは録画時点の機能一覧のみです。`,changed:' この機能の後の変更は、この録画では検証されていません。',all:'録画時点の機能一覧'},
 };
 function historicalCopy(lang){const language=lang.toLowerCase();return HISTORY_COPY[/^zh(?:-|$)/.test(language)?(/^zh-(?:hant|tw|hk)(?:-|$)/.test(language)?'zh-TW':'zh-CN'):language.split('-')[0]]||HISTORY_COPY.en;}
 function historyMessage(history,lang,id){const copy=historicalCopy(lang);return copy.hint(history.recorded_date)+(id&&history.unrecorded_feature_ids.includes(id)?copy.changed:'');}
 const $=id=>document.getElementById(id);
 const lang=document.documentElement.lang,copy=historicalCopy(lang);
 const historyNote=document.createElement('p');historyNote.className='help';historyNote.id='history-disclosure';historyNote.textContent=copy.loading;
 const intro=document.querySelector('.intro'),introText=intro?.querySelector('p:not(.eyebrow)');if(introText)introText.textContent=copy.scope;intro?.append(historyNote);
 const playerHistory=document.createElement('p');playerHistory.className='help';$('steps').before(playerHistory);
 let features=[],category='',lastTrigger=null,historyInfo=null;
 let saved;try{saved=localStorage.getItem('readmd.site.theme');}catch{}
 document.documentElement.classList.toggle('dark',saved?saved==='dark':matchMedia('(prefers-color-scheme: dark)').matches);
 $('theme').addEventListener('click',()=>{const dark=document.documentElement.classList.toggle('dark');try{localStorage.setItem('readmd.site.theme',dark?'dark':'light');}catch{}});
 const close=()=>{$('video').pause();$('video').removeAttribute('src');$('video').load();$('player').close();lastTrigger?.focus();};
 $('close-player').addEventListener('click',close);
 $('player').addEventListener('cancel',e=>{e.preventDefault();close();});
 $('player').addEventListener('click',e=>{if(e.target===$('player')){const r=$('player').getBoundingClientRect();if(e.clientX<r.left||e.clientX>r.right||e.clientY<r.top||e.clientY>r.bottom)close();}});
 const open=(f,trigger)=>{
  lastTrigger=trigger;$('player-id').textContent=f.id+' · '+f.recording.duration.toFixed(1)+' 秒';$('player-title').textContent=f.title;$('flow').textContent=f.flow;
  playerHistory.textContent=historyMessage(historyInfo,lang,f.id);
  $('steps').replaceChildren(...f.recording.steps.map(s=>{const li=document.createElement('li');li.textContent=s.text;return li;}));
  const video=$('video');video.poster='/showcase/'+f.recording.poster;video.src='/showcase/'+f.recording.video;video.querySelector('track').src='/showcase/'+f.recording.captions;
  // Direct links open without a user gesture. Load the selected clip even
  // when the browser declines autoplay; the controls must still be ready.
  video.preload='auto';video.querySelector('track').track.mode='hidden';video.load();
  $('download-video').href=video.src;$('player').showModal();video.play().catch(()=>{});
  history.replaceState(null,'','#'+f.id);
 };
 const render=()=>{
  const query=$('search').value.trim().toLocaleLowerCase();
  const shown=features.filter(f=>(!category||f.section===category)&&(!query||[f.id,f.title,f.section,f.flow].join(' ').toLocaleLowerCase().includes(query)));
  $('category-title').textContent=category?category.replace(/^\d+\s*/,''):copy.all;$('count').textContent=shown.length+' 项功能';$('empty').hidden=!!shown.length;
  $('cards').replaceChildren(...shown.map(f=>{
   if(f.recording.status!=='recorded'){const article=document.createElement('article');article.className='card pending';const id=document.createElement('small');id.textContent=f.id;const h=document.createElement('h3');h.textContent=f.title;const p=document.createElement('p');p.textContent='演示录制中';article.append(id,h,p);return article;}
   const b=document.createElement('button');b.type='button';b.className='card';b.setAttribute('aria-label','播放 '+f.title);const image=document.createElement('img');image.src='/showcase/'+f.recording.poster;image.alt='';image.loading='lazy';image.width=1280;image.height=800;const play=document.createElement('span');play.className='play';play.textContent='▶';play.setAttribute('aria-hidden','true');const body=document.createElement('div');body.className='card-body';const meta=document.createElement('div');meta.className='card-meta';const id=document.createElement('span');id.textContent=f.id;const time=document.createElement('span');time.textContent=f.recording.duration.toFixed(1)+' 秒';meta.append(id,time);const h=document.createElement('h3');h.textContent=f.title;body.append(meta,h);b.append(image,play,body);b.addEventListener('click',()=>open(f,b));return b;
  }));
  for(const b of $('categories').children)b.setAttribute('aria-pressed',String(b.dataset.category===category));
 };
 $('search').addEventListener('input',render);
 const load=url=>fetch(url).then(r=>{if(!r.ok)throw Error('功能清单加载失败');return r.json();});
 Promise.all([load('/showcase/catalog.json'),load('/showcase/history.json')]).then(([c,h])=>{
  if(h.schema!==1||h.kind!=='historical-recordings'||h.software_version?.status!=='unverified'||h.software_version.value!==null||h.claims?.current_ui_recorded!==false||h.claims?.current_semantics_recorded!==false||!Array.isArray(h.unrecorded_feature_ids))throw Error('历史录制来源未验证');
  historyInfo=h;historyNote.textContent=historyMessage(h,lang);
  features=c.features;const categories=['',...new Set(features.map(f=>f.section))];
  $('categories').replaceChildren(...categories.map(name=>{const b=document.createElement('button');b.className='category';b.type='button';b.dataset.category=name;const text=document.createElement('div');text.textContent=name?name.replace(/^\d+\s*/,''):copy.all;const count=document.createElement('span');count.textContent=features.filter(f=>!name||f.section===name).length;b.append(text,count);b.addEventListener('click',()=>{category=name;render();});return b;}));render();
  const f=features.find(f=>'#'+f.id===location.hash&&f.recording.status==='recorded');if(f)open(f,null);
 }).catch(e=>{$('empty').hidden=false;$('empty').textContent=e.message+'，请刷新重试。';});
})();
