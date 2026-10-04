import http from 'node:http';
import fs from 'node:fs';
import path from 'node:path';
export async function webFixture(materials){
 const page=fs.readFileSync(path.join(materials,'证据与论证.html'),'utf8');
 const server=http.createServer((req,res)=>{
  res.setHeader('Content-Type','text/html; charset=utf-8');
  const dynamic=`<section style="padding:22px;border:1px solid #a5bca6;border-radius:12px;background:#e3eadd"><h2>阅读小组 · 演示会话</h2><p>这是一份专为网页提取制作的本地资料。</p><button id="demo-login" style="padding:12px 20px;border:0;border-radius:8px;background:#203c32;color:white;font-size:16px;cursor:pointer">进入演示会话</button></section><script>document.querySelector('#demo-login').onclick=()=>{document.cookie='readmd_demo=joined;path=/';document.querySelector('#demo-story').textContent+=' 已进入演示会话：讨论观察与解释的边界。';};setTimeout(()=>{document.querySelector('#demo-story').textContent='动态阅读记录：可靠的论证允许结论被新证据修订。'+(document.cookie.includes('readmd_demo=joined')?' 已进入演示会话：讨论观察与解释的边界。':'');},350);</script>`;
  res.end(req.url==='/dynamic'?page.replace('<p>','<p id="demo-story">').replace('</main>',dynamic+'</main>'):page);
 });
 await new Promise(r=>server.listen(0,'127.0.0.1',r));
 return {url:'http://127.0.0.1:'+server.address().port,close:()=>new Promise(r=>{server.closeAllConnections();server.close(r);})};
}

