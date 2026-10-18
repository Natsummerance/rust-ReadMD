// Explicit input preparation may use the network. All assembly is offline.
import fs from 'node:fs';
import path from 'node:path';
import crypto from 'node:crypto';
import zlib from 'node:zlib';
const args=process.argv.slice(2),value=k=>args[args.indexOf(k)+1];
if(!args.includes('--inputs')||!args.includes('--output')||args.some(a=>a.startsWith('--')&&!['--inputs','--output','--prepare','--write-lock'].includes(a)))throw Error('Usage: node tools/build-publication-fonts.mjs --inputs DIR --output PACK [--prepare] [--write-lock]');
const inputs=path.resolve(value('--inputs')),output=path.resolve(value('--output'));
const lock=JSON.parse(fs.readFileSync('tools/publication-font-inputs.lock.json'));
const sha=b=>crypto.createHash('sha256').update(b).digest('hex');
fs.mkdirSync(inputs,{recursive:true});
const reviewed=new Map();
for(const input of lock.inputs){
  const file=path.join(inputs,input.name);
  if(!fs.existsSync(file)&&args.includes('--prepare')){
    if(!input.url.startsWith('https://raw.githubusercontent.com/')||!input.url.includes('/'+input.commit+'/'))throw Error('Unpinned source URL');
    const response=await fetch(input.url,{signal:AbortSignal.timeout(120000)});if(!response.ok)throw Error('Official resource download failed');
    const bytes=Buffer.from(await response.arrayBuffer());
    if(bytes.length!==input.size||sha(bytes)!==input.sha256)throw Error('Official resource differs from reviewed input');
    fs.writeFileSync(file,bytes);
  }
  const bytes=fs.readFileSync(file);
  if(bytes.length!==input.size||sha(bytes)!==input.sha256)throw Error('Local resource differs from reviewed input: '+input.name);
  reviewed.set(input.name,bytes);
}
if(sha(fs.readFileSync('assets/fonts/STIXTwoMath-Regular.ttf'))!==sha(reviewed.get('STIXTwoMath-Regular.ttf'))||sha(fs.readFileSync('assets/licenses/STIX-OFL-1.1.txt'))!==sha(reviewed.get('STIX-OFL.txt')))throw Error('Core math font or notice differs from reviewed input');
const instances=JSON.parse(fs.readFileSync('tools/publication-static-fonts.lock.json'));
const prepared=instances.files.map(file=>{const bytes=fs.readFileSync(path.join(inputs,'static',file.name));if(bytes.length!==file.size||sha(bytes)!==file.sha256)throw Error('Static font differs from reviewed output');return ['fonts/'+file.name,bytes];});
const data=new Map([
  ...prepared,
  ['licenses/NotoSansSC-OFL.txt',reviewed.get('NotoSansSC-OFL.txt')],['licenses/NotoSerifSC-OFL.txt',reviewed.get('NotoSerifSC-OFL.txt')],
  ['NOTICE',Buffer.from('ReadMD Sans SC and ReadMD Serif SC are renamed static-weight derivatives of Noto Sans SC and Noto Serif SC. Weight 400 and 700 instances prepared with fontTools 4.63.0; no glyph subset or outline simplification. Copyright and SIL OFL 1.1 licenses are included in licenses/. Original sources and immutable revisions: tools/publication-font-inputs.lock.json in the matching ReadMD source release. Fonts stay private to ReadMD; no OS font installation. Simplified Chinese typography, Latin and available shared CJK glyphs; this package does not claim complete coverage of every language or Unicode code point.\n')]
]);
const files=[...data].map(([path,b])=>({path,size:b.length,sha256:sha(b)}));
const manifest={schema:1,id:'fonts',version:'2026.10.10',platform:'universal',files,review:{license:'OFL-1.1; renamed static derivatives; original copyright notices retained',source:'https://github.com/google/fonts/tree/a85815a42757630ce188fdad368c2dfc444d4773',delivery:'On-demand private publication fonts; no executables, scripts or global font registration',coverage:'Simplified Chinese sans/serif typography; glyph coverage is checked at use, not inferred from UI language count'}};
const target='assets/capabilities/fonts-2026.10.10-universal.lock.json';
if(args.includes('--write-lock'))fs.writeFileSync(target,JSON.stringify(manifest,null,2)+'\n');
else if(JSON.stringify(JSON.parse(fs.readFileSync(target)))!==JSON.stringify(manifest))throw Error('Fonts differ from application lock');
const payload=Object.fromEntries([...data].map(([p,b])=>[p,{compression:'gzip',data:zlib.gzipSync(b,{level:9}).toString('base64')}]));
const pack=Buffer.from(JSON.stringify({manifest,payload}));
fs.mkdirSync(path.dirname(output),{recursive:true});fs.writeFileSync(output,pack);
console.log(JSON.stringify({offline:!args.includes('--prepare'),bytes:pack.length,installedBytes:files.reduce((n,f)=>n+f.size,0),sha256:sha(pack)}));
