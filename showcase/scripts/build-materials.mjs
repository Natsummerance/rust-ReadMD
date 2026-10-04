/** Original offline demo materials. Node built-ins only; never reads a library. */
import fs from 'node:fs';import path from 'node:path';import{deflateRawSync}from'node:zlib';import{fileURLToPath}from'node:url';
const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'../materials');
const documents={
  "references.bib": "@book{reading2026, title={Evidence and Reasoning}, author={ReadMD Reading Group}, year={2026}, publisher={Original demonstration material}}\r\n",
  "哲学阅读研究手册.md": "---\r\ntitle: 哲学阅读研究手册\r\nauthor: ReadMD 阅读小组\r\nbibliography: references.bib\r\n---\r\n\r\n# 哲学阅读研究手册\r\n\r\n把问题写清楚，把论证留下来。本文是为 ReadMD 操作演示编写的原创研究资料。\r\n\r\n## 一 认识与经验\r\n\r\n一个判断的可靠性来自什么？我们用**观察记录**与*推理过程*区分证据和结论。\r\n\r\n> [!NOTE]\r\n> 这里的例子用于练习论证分析，不代表对某位哲学家的原文转述。\r\n\r\n阅读 [[认识与经验]]，并与 [[自由与责任]] 对照。\r\n\r\n| 研究问题 | 可检查的材料 | 下一步 |\r\n| :--- | :--- | :--- |\r\n| 经验是否足够 | 观察笔记 | 寻找反例 |\r\n| 行动是否自由 | 选择情境 | 区分限制 |\r\n| 判断如何成立 | 前提与结论 | 检查推理 |\r\n\r\n## 二 论证与反例\r\n\r\n如果每个观察样本都符合命题，命题是否一定成立？[^sample]\r\n\r\n$P(H \\mid E)=\\frac{P(E \\mid H)P(H)}{P(E)}$\r\n\r\n- [x] 明确研究问题\r\n- [ ] 比较不同解释\r\n- [ ] 整理参考文献\r\n\r\n```javascript\r\nconst observations = [3, 5, 8, 13];\r\nconsole.log(observations.reduce((a, b) => a + b, 0));\r\n```\r\n\r\n## 三 讨论记录\r\n\r\n![阅读路线](reading-map.png)\r\n\r\n我们先保留原稿，再尝试改写，最后核对修改内容。参见 [研究方法](notes/研究方法.md)。\r\n\r\n[^sample]: 有限样本支持推断，但需要说明适用范围。\r\n",
  "研究摘要.tex": "\\documentclass{article}\r\n\\title{Evidence and Reasoning}\r\n\\author{ReadMD Reading Group}\r\n\\begin{document}\r\n\\maketitle\r\n\\section{Research question}\r\nHow do observations support a conclusion?\r\n\\begin{equation}P(H\\mid E)=\\frac{P(E\\mid H)P(H)}{P(E)}\\end{equation}\r\n\\section{Method}\r\nWe distinguish observations, premises, and conclusions.\r\n\\end{document}\r\n",
  "研究数据.csv": "主题,阅读分钟,笔记数\r\n认识与经验,35,6\r\n自由与责任,28,5\r\n论证与反例,42,8\r\n研究方法,25,4\r\n",
  "研究数据.json": "{\r\n  \"title\": \"哲学阅读记录\",\r\n  \"sessions\": [\r\n    {\r\n      \"topic\": \"认识与经验\",\r\n      \"minutes\": 35\r\n    },\r\n    {\r\n      \"topic\": \"自由与责任\",\r\n      \"minutes\": 28\r\n    }\r\n  ]\r\n}",
  "讨论邀请.eml": "From: reading@example.test\r\nTo: colleague@example.test\r\nSubject: Philosophy reading discussion\r\nMIME-Version: 1.0\r\nContent-Type: text/plain; charset=UTF-8\r\n\r\n下一次阅读讨论关注证据与论证。请准备一份观察笔记和一个反例。\r\n",
  "论证示例.js": "// 哲学阅读记录的统计示例\r\nconst minutes = [35, 28, 42, 25];\r\nconsole.log(\"阅读总时长\", minutes.reduce((a, b) => a + b, 0));\r\n",
  "证据与论证.html": "<!doctype html><meta charset=\"utf-8\"><title>证据与论证</title><style>body{margin:0;background:#f6f2e9;color:#26342f;font:20px/1.8 \"Microsoft YaHei\",sans-serif}main{max-width:760px;margin:64px auto;padding:56px;background:#fffdf8}small{color:#728879;letter-spacing:3px}h1{font-size:42px;line-height:1.25}h2{font-size:26px;margin-top:40px}table{width:100%;border-collapse:collapse}td,th{padding:12px;text-align:left;border-bottom:1px solid #d9ded5}footer{margin-top:48px;font-size:15px;color:#728879}</style><main><small>READMD READING GROUP · 2026</small><h1>证据与论证<br>哲学阅读工作坊</h1><p>从观察到判断，记录每一步推理。这是一份原创演示资料。</p><h2>阅读问题</h2><p>我们如何区分一个可信的结论和一个看似合理的解释？先列前提，再讨论证据。</p><table><tr><th>阶段</th><th>研究记录</th></tr><tr><td>观察</td><td>描述可以检查的现象</td></tr><tr><td>分析</td><td>区分证据与解释</td></tr><tr><td>修订</td><td>用反例检验结论</td></tr></table><h2>讨论笔记</h2><p>保留原稿，建立修改副本，在保存前比较不同解释。</p><footer>原创操作演示资料 · 可自由用于 ReadMD 演示</footer></main>",
  "阅读讨论演示.md": "# 哲学阅读讨论\r\n\r\n阅读小组 · 2026\r\n\r\n---\r\n\r\n## 经验与判断\r\n\r\n观察是起点，论证需要说明前提。\r\n\r\n---\r\n\r\n## 反例与修订\r\n\r\n- 保留原稿\r\n- 比较解释\r\n- 记录修改理由\r\n\r\n---\r\n\r\n## 下一次讨论\r\n\r\n自由、选择与责任。\r\n",
  "notes/研究方法.md": "# 研究方法\r\n\r\n先界定术语，再检查前提。\r\n\r\n[[认识与经验]] 与 [[自由与责任]] 构成这份阅读笔记的关联网络。\r\n",
  "notes/自由与责任.md": "# 自由与责任\r\n\r\n行动的理由如何成为可以解释的选择？参见 [[认识与经验]]。\r\n\r\n## 讨论要点\r\n\r\n描述情境，再评价责任。\r\n",
  "notes/认识与经验.md": "# 认识与经验\r\n\r\n[[自由与责任]] 与 [[研究方法]] 提供不同的分析路径。\r\n\r\n## 观察笔记\r\n\r\n区分直接观察、解释与推论。\r\n"
};
const archives={
  "哲学阅读工作坊.epub": {
    "mimetype": "application/epub+zip",
    "META-INF/container.xml": "<?xml version=\"1.0\"?><container version=\"1.0\" xmlns=\"urn:oasis:names:tc:opendocument:xmlns:container\"><rootfiles><rootfile full-path=\"OEBPS/content.opf\" media-type=\"application/oebps-package+xml\"/></rootfiles></container>",
    "OEBPS/content.opf": "<?xml version=\"1.0\"?><package xmlns=\"http://www.idpf.org/2007/opf\" version=\"3.0\" unique-identifier=\"id\" xml:lang=\"zh-CN\"><metadata xmlns:dc=\"http://purl.org/dc/elements/1.1/\"><dc:identifier id=\"id\">readmd-original-philosophy-2026</dc:identifier><dc:title>哲学阅读工作坊</dc:title><dc:language>zh-CN</dc:language><dc:creator>ReadMD 阅读小组</dc:creator><meta property=\"dcterms:modified\">2026-10-03T00:00:00Z</meta></metadata><manifest><item id=\"nav\" href=\"nav.xhtml\" media-type=\"application/xhtml+xml\" properties=\"nav\"/><item id=\"body\" href=\"chapter.xhtml\" media-type=\"application/xhtml+xml\"/></manifest><spine><itemref idref=\"body\"/></spine></package>",
    "OEBPS/nav.xhtml": "<html xmlns=\"http://www.w3.org/1999/xhtml\" xmlns:epub=\"http://www.idpf.org/2007/ops\"><head><title>目录</title></head><body><nav epub:type=\"toc\"><ol><li><a href=\"chapter.xhtml\">证据与论证</a></li></ol></nav></body></html>",
    "OEBPS/chapter.xhtml": "<html xmlns=\"http://www.w3.org/1999/xhtml\"><head><title>证据与论证</title><style>body{margin:0;background:#f6f2e9;color:#26342f;font:20px/1.8 \"Microsoft YaHei\",sans-serif}main{max-width:760px;margin:64px auto;padding:56px;background:#fffdf8}small{color:#728879;letter-spacing:3px}h1{font-size:42px;line-height:1.25}h2{font-size:26px;margin-top:40px}table{width:100%;border-collapse:collapse}td,th{padding:12px;text-align:left;border-bottom:1px solid #d9ded5}footer{margin-top:48px;font-size:15px;color:#728879}</style></head><body><main><h1>证据与论证</h1><p>哲学阅读工作坊的原创操作演示资料。</p><h2>认识与经验</h2><p>观察是研究的起点。我们记录现象，区分解释与推论。</p><h2>反例与修订</h2><p>用反例检验判断，保留原稿，在副本中尝试新的表达。</p></main></body></html>"
  },
  "阅读笔记.zip": {
    "研究方法.md": "# 研究方法\r\n\r\n先界定术语，再检查前提。\r\n\r\n[[认识与经验]] 与 [[自由与责任]] 构成这份阅读笔记的关联网络。\r\n",
    "自由与责任.md": "# 自由与责任\r\n\r\n行动的理由如何成为可以解释的选择？参见 [[认识与经验]]。\r\n\r\n## 讨论要点\r\n\r\n描述情境，再评价责任。\r\n",
    "认识与经验.md": "# 认识与经验\r\n\r\n[[自由与责任]] 与 [[研究方法]] 提供不同的分析路径。\r\n\r\n## 观察笔记\r\n\r\n区分直接观察、解释与推论。\r\n"
  },
  "原创研究Skill.zip": {
    "demo-philosophy/SKILL.md": "---\nname: demo-philosophy\ndescription: Analyze observations and premises in a philosophy reading note\n---\n\n# Philosophy reading\n\nSeparate observations, premises, and conclusions. Preserve uncertainty.\n",
    "demo-philosophy/LICENSE": "MIT License\n\nCopyright (c) 2026 ReadMD Demo Authors\n\nPermission is hereby granted, free of charge, to any person obtaining a copy of this software and associated documentation files, to deal in the Software without restriction, including without limitation the rights to use, copy, modify, merge, publish, distribute, sublicense, and/or sell copies of the Software, and to permit persons to whom the Software is furnished to do so, subject to the following conditions:\n\nThe above copyright notice and this permission notice shall be included in all copies or substantial portions of the Software.\n\nTHE SOFTWARE IS PROVIDED \"AS IS\", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.\n"
  }
};
fs.mkdirSync(root,{recursive:true});
for(const [name,text]of Object.entries(documents)){const file=path.join(root,name);fs.mkdirSync(path.dirname(file),{recursive:true});fs.writeFileSync(file,text);}
function crc32(bytes){let crc=0xffffffff;for(const byte of bytes){crc^=byte;for(let i=0;i<8;i++)crc=(crc>>>1)^((crc&1)?0xedb88320:0);}return(crc^0xffffffff)>>>0;}
function zip(entries){const locals=[],central=[];let offset=0;for(const[name,text]of Object.entries(entries)){
 const filename=Buffer.from(name),body=Buffer.from(text),method=name==='mimetype'?0:8,data=method?deflateRawSync(body):body,crc=crc32(body);
 const header=Buffer.alloc(30);header.writeUInt32LE(0x04034b50);header.writeUInt16LE(20,4);header.writeUInt16LE(0x800,6);header.writeUInt16LE(method,8);header.writeUInt16LE(0x5d43,12);header.writeUInt32LE(crc,14);header.writeUInt32LE(data.length,18);header.writeUInt32LE(body.length,22);header.writeUInt16LE(filename.length,26);
 const index=Buffer.alloc(46);index.writeUInt32LE(0x02014b50);index.writeUInt16LE(20,4);header.copy(index,6,4,30);index.writeUInt32LE(offset,42);locals.push(header,filename,data);central.push(index,filename);offset+=header.length+filename.length+data.length;
 }const table=Buffer.concat(central),end=Buffer.alloc(22);end.writeUInt32LE(0x06054b50);end.writeUInt16LE(Object.keys(entries).length,8);end.writeUInt16LE(Object.keys(entries).length,10);end.writeUInt32LE(table.length,12);end.writeUInt32LE(offset,16);return Buffer.concat([...locals,table,end]);}
for(const[name,entries]of Object.entries(archives))fs.writeFileSync(path.join(root,name),zip(entries));
// Two original A4 pages, using the standard GB1 CID font supported by PDF readers.
const objects=[];const add=s=>{objects.push(Buffer.from(s,'binary'));return objects.length;};
add('<< /Type /Catalog /Pages 2 0 R >>');add('');
const cjk=add('<< /Type /Font /Subtype /Type0 /BaseFont /STSong-Light /Encoding /UniGB-UCS2-H /DescendantFonts [5 0 R] >>');
const latin=add('<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>');
add('<< /Type /Font /Subtype /CIDFontType0 /BaseFont /STSong-Light /CIDSystemInfo << /Registry (Adobe) /Ordering (GB1) /Supplement 4 >> /FontDescriptor 6 0 R /DW 1000 >>');
add('<< /Type /FontDescriptor /FontName /STSong-Light /Flags 6 /FontBBox [-25 -254 1000 880] /Ascent 880 /Descent -120 /CapHeight 880 /StemV 80 >>');
const pages=[];
for(let page=0;page<2;page++){
 const lines=['0.965 0.949 0.914 rg 0 0 595.28 841.89 re f'];
 const draw=(text,size,x,y,font='F1',color='0.149 0.204 0.184')=>{const encoded=font==='F1'?'<'+Buffer.from(text,'utf16le').swap16().toString('hex')+'>':'('+text.replace(/[()\\]/g,'\\$&')+')';lines.push(`${color} rg BT /${font} ${size} Tf 1 0 0 1 ${x} ${y} Tm ${encoded} Tj ET`);};
 draw('READMD READING GROUP / ORIGINAL DEMO MATERIAL',10,58,783,'F2','0.447 0.533 0.475');
 draw(page?'阅读与修订记录':'证据与论证',32,58,706);draw(page?'保留原稿  对照解释  检查推理':'哲学阅读工作坊 2026',15,58,665);
 const sections=page?[
 ['阅读记录','主题：认识与经验  阅读时间：35 分钟','笔记：区分直接观察、解释与推论。'],
 ['修订计划','第一步：明确术语与研究问题。','第二步：比较论证，标记需要核查的前提。'],
 ['保存原则','保存确认后的版本，并保留可恢复的修改记录。','本资料为原创演示文本，不引用私人藏书正文。']]:[
 ['阅读问题','我们如何区分可信的结论与看似合理的解释？','先列出前提，再检查每一项证据。'],
 ['观察与解释','描述可以检查的现象，避免把推断写成事实。','不同的解释需要各自说明适用范围。'],
 ['讨论方法','用反例检验结论，并记录修改的原因。','保留原稿，在副本中尝试新的表达。']];
 sections.forEach(([title,a,b],i)=>{const y=590-i*145;draw(title,20,58,y);draw(a,12,58,y-38);draw(b,12,58,y-62);});
 lines.push('0.827 0.855 0.812 RG 58 90 m 537 90 l S');draw('ReadMD 原创操作演示资料',10,58,66);draw(`0${page+1} / 02`,10,500,66,'F2');
 const stream=Buffer.from(lines.join('\n')+'\n'),content=add(`<< /Length ${stream.length} >>\nstream\n${stream.toString('binary')}endstream`);
 pages.push(add(`<< /Type /Page /Parent 2 0 R /MediaBox [0 0 595.28 841.89] /Resources << /Font << /F1 ${cjk} 0 R /F2 ${latin} 0 R >> >> /Contents ${content} 0 R >>`));
}
objects[1]=Buffer.from(`<< /Type /Pages /Kids [${pages.map(p=>p+' 0 R').join(' ')}] /Count 2 >>`);
const chunks=[Buffer.from('%PDF-1.4\n%\xe2\xe3\xcf\xd3\n','binary')],offsets=[0];let length=chunks[0].length;
objects.forEach((object,i)=>{offsets.push(length);const chunk=Buffer.concat([Buffer.from(`${i+1} 0 obj\n`),object,Buffer.from('\nendobj\n')]);chunks.push(chunk);length+=chunk.length;});
chunks.push(Buffer.from(`xref\n0 ${objects.length+1}\n0000000000 65535 f \n${offsets.slice(1).map(x=>String(x).padStart(10,'0')+' 00000 n \n').join('')}trailer\n<< /Size ${objects.length+1} /Root 1 0 R >>\nstartxref\n${length}\n%%EOF\n`));
fs.writeFileSync(path.join(root,'证据与论证.pdf'),Buffer.concat(chunks));
fs.writeFileSync(path.join(root,'README.md'),'# 演示资料\n\n全部正文为 ReadMD 原创哲学阅读、论证分析和笔记修订资料。Markdown 用于阅读、编辑、图谱与保存；PDF、EPUB、HTML、TeX、邮件、代码、数据及 ZIP 用于转换；演示稿用于放映。\n\n离线重建：运行 showcase/scripts/build-materials.mjs、build-material-images.mjs 与 build-material-audio.ps1。只使用既有 Node、浏览器与 Windows 语音环境，无新增依赖。录制使用隔离副本。Hermes 私人书籍不放入此目录或网站。\n');
console.log('Built original text, ZIP, EPUB and two-page PDF materials offline');
