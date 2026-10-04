import fs from 'node:fs';
import path from 'node:path';
import crypto from 'node:crypto';
import {fileURLToPath} from 'node:url';
import {CAPTURE_PROFILE,recordingUiHash} from './capture-mode.mjs';
const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'../..');
const showcase=path.join(root,'showcase');
const manifest=JSON.parse(fs.readFileSync(path.join(showcase,'manifest.json'),'utf8'));
const hash=p=>crypto.createHash('sha256').update(fs.readFileSync(p)).digest('hex');
const currentUiHash=recordingUiHash(root);
for(const f of manifest.features.filter(f=>f.recording.status==='recorded')){
 if(f.recording.capture_profile!==CAPTURE_PROFILE||f.recording.ui_source_sha256!==currentUiHash)throw Error('Old UI footage must be rerecorded: '+f.id);
 const r=f.recording;if(!r.privacy?.approved||r.privacy.sha256!==hash(path.join(showcase,r.video))||r.privacy.poster_sha256!==hash(path.join(showcase,r.poster)))throw Error(`Privacy approval missing or stale: ${f.id}`);
}
const publicRoot=path.join(root,'website/public');
const target=path.join(publicRoot,'showcase');fs.mkdirSync(target,{recursive:true});
const compact={schema:manifest.schema,date:manifest.date,capture_profile:manifest.capture_profile,ui_source_sha256:manifest.ui_source_sha256,inventory_sha256:manifest.inventory_sha256,features:manifest.features.map(f=>({id:f.id,section:f.section,title:f.title,flow:f.recording.steps?.map(s=>s.text).join(' → ')||f.title,controls:f.controls,recording:f.recording.status==='recorded'?{status:'recorded',video:f.recording.video,poster:f.recording.poster,captions:f.recording.captions,duration:f.recording.duration,bytes:f.recording.bytes,sha256:f.recording.sha256,steps:f.recording.steps}: {status:f.recording.status==='failed'?'pending':f.recording.status}}))};
fs.writeFileSync(path.join(target,'catalog.json'),JSON.stringify(compact)+'\n');
// Hardlinks avoid storing a second local copy of the videos. The site build copies them into dist.
for(const dir of ['videos','posters','captions']){
 fs.mkdirSync(path.join(target,dir),{recursive:true});
 for(const f of compact.features.filter(f=>f.recording.status==='recorded')){
  const filename=f.id+(dir==='videos'?'.mp4':dir==='posters'?'.webp':'.vtt');
  const source=path.join(showcase,dir,filename),dest=path.join(target,dir,filename);
  if(fs.existsSync(dest)&&fs.statSync(dest).ino===fs.statSync(source).ino)continue;
  if(fs.existsSync(dest))fs.unlinkSync(dest);try{fs.linkSync(source,dest);}catch{fs.copyFileSync(source,dest);}
 }
}
const count=compact.features.filter(f=>f.recording.status==='recorded').length;
const html=`<!doctype html><html lang="zh-CN"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>ReadMD 功能操作演示</title><meta name="description" content="按功能清单浏览 ReadMD 的真实操作短视频。阅读、编辑、保存恢复、文件转换、导出与桌宠。"><link rel="canonical" href="https://rust.readmd.asia/showcase/"><link rel="stylesheet" href="/assets/showcase.css"><script src="/assets/showcase.js" defer></script></head><body><header><a class="brand" href="/zh-cn/">ReadMD<span>功能操作演示</span></a><nav><a href="/zh-cn/">官网</a><a href="/zh-cn/download/">下载应用</a><button id="theme" type="button" aria-label="切换深浅主题">◐</button></nav></header><main><section class="intro"><p class="eyebrow">真实界面 · 一个功能，一段演示</p><h1><span>看到操作，</span> <span>也看到结果。</span></h1><p>从阅读和编辑到转换、导出与桌宠，按最新功能清单查找你需要的操作。</p><div class="stats"><span><strong>${count}</strong> 已录制</span><span><strong>${compact.features.length}</strong> 功能流程</span><span><strong>21</strong> 功能分类</span></div></section><div class="browser"><aside aria-label="功能分类"><label class="search"><span>搜索功能</span><input id="search" type="search" placeholder="例如：保存、PDF、桌宠" autocomplete="off"></label><div id="categories"></div><p class="help">短片以中文界面录制。点击卡片播放，可使用字幕、倍速与全屏。</p></aside><section class="results" aria-label="功能演示"><div class="results-heading"><h2 id="category-title">全部功能</h2><output id="count" aria-live="polite"></output></div><div id="cards" class="cards"></div><p id="empty" hidden>没有找到匹配功能，请换一个关键词。</p></section></div></main><dialog id="player"><div class="player-heading"><div><span id="player-id"></span><h2 id="player-title"></h2></div><button id="close-player" type="button" aria-label="关闭视频">×</button></div><video id="video" controls playsinline preload="none"><track kind="captions" srclang="zh" label="中文操作说明"></video><div class="player-detail"><ol id="steps"></ol><details><summary>查看完整操作流程</summary><p id="flow"></p></details><a id="download-video" download>下载这段演示</a></div></dialog><footer>ReadMD · ${manifest.date} 功能清单 · 演示正文均为原创资料</footer></body></html>`;
fs.writeFileSync(path.join(target,'index.html'),html);
const canonical='https://rust.readmd.asia/showcase/';
const sitemapPath=path.join(publicRoot,'sitemap.xml');let sitemap=fs.readFileSync(sitemapPath,'utf8');
if(!sitemap.includes(`<loc>${canonical}</loc>`))sitemap=sitemap.replace('</urlset>',`  <url><loc>${canonical}</loc><lastmod>${manifest.date}</lastmod><changefreq>monthly</changefreq><priority>0.8</priority></url>\n</urlset>`);
fs.writeFileSync(sitemapPath,sitemap);
const feedPath=path.join(publicRoot,'feed.xml');let feed=fs.readFileSync(feedPath,'utf8');
feed=feed.replace(/<updated>[^<]+<\/updated>/,`<updated>${manifest.date}T00:00:00Z</updated>`);
if(!feed.includes(`<id>${canonical}</id>`))feed=feed.replace('</feed>',`  <entry><id>${canonical}</id><title>ReadMD 功能操作演示</title><link rel="alternate" href="${canonical}"/><updated>${manifest.date}T00:00:00Z</updated><summary>按最新清单浏览阅读、编辑、保存恢复、转换、导出和桌宠的真实操作短视频。</summary></entry>\n</feed>`);
fs.writeFileSync(feedPath,feed);
// Navigation and cards belong to the original-style home builder.
for(const prefix of ['','zh-cn','zh-tw','ja']){
 const p=path.join(publicRoot,prefix,'index.html');let s=fs.readFileSync(p,'utf8');
 s=s.replace(/<!-- inventory-showcase-start -->[\s\S]*?<!-- inventory-showcase-end -->/g,'');
 fs.writeFileSync(p,s);
}
console.log(`Gallery built with ${count}/${compact.features.length} recordings`);
