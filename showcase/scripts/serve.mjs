/** Local, read-only preview of the built website, including MP4 byte ranges. */
import http from 'node:http';import fs from 'node:fs';import path from 'node:path';import{fileURLToPath}from'node:url';
const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'../../website/dist');
if(!fs.existsSync(path.join(root,'showcase/index.html')))throw Error('Build the website before previewing it');
const types={'.html':'text/html; charset=utf-8','.js':'text/javascript; charset=utf-8','.css':'text/css; charset=utf-8','.json':'application/json','.mp4':'video/mp4','.webp':'image/webp','.png':'image/png','.vtt':'text/vtt; charset=utf-8','.svg':'image/svg+xml','.woff2':'font/woff2','.ico':'image/x-icon','.xml':'application/xml','.txt':'text/plain; charset=utf-8'};
const server=http.createServer((req,res)=>{
 if(!['GET','HEAD'].includes(req.method)){res.writeHead(405).end();return;}
 let file;try{const name=decodeURIComponent(new URL(req.url,'http://localhost').pathname);file=path.resolve(root,'.'+name);if(name.endsWith('/'))file=path.join(file,'index.html');}catch{res.writeHead(400).end();return;}
 if(!file.startsWith(root+path.sep)){res.writeHead(403).end();return;}
 let size;try{const stat=fs.statSync(file);if(!stat.isFile())throw Error();size=stat.size;}catch{res.writeHead(404).end();return;}
 const headers={'Content-Type':types[path.extname(file)]||'application/octet-stream','Accept-Ranges':'bytes','X-Content-Type-Options':'nosniff'};
 const range=req.headers.range?.match(/^bytes=(\d+)-(\d*)$/);let start=0,end=size-1,status=200;
 if(range){start=Number(range[1]);end=Math.min(Number(range[2]||end),end);if(start>end||start>=size){res.writeHead(416,{'Content-Range':`bytes */${size}`}).end();return;}status=206;headers['Content-Range']=`bytes ${start}-${end}/${size}`;}
 res.writeHead(status,{...headers,'Content-Length':end-start+1});if(req.method==='HEAD')res.end();else fs.createReadStream(file,{start,end}).pipe(res);
});
server.listen(Number(process.argv[2]||4173),'127.0.0.1',()=>console.log(`ReadMD preview: http://127.0.0.1:${server.address().port}/zh-cn/ · /showcase/`));
for(const signal of ['SIGINT','SIGTERM'])process.on(signal,()=>server.close(()=>process.exit(0)));
