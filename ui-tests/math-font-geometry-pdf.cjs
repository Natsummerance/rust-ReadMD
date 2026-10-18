'use strict';
// Independent inspection of owned Rust-generated PDF content streams. PDFium
// separately validates/reopens the actual document; this is not a general PDF
// parser and does not inspect font programs or infer vectors from filenames.
const assert=require('node:assert/strict'),zlib=require('node:zlib');
const EPS=0.004; // PDF path coordinates are serialized to 0.001 pt.
function multiply(a,b){return[a[0]*b[0]+a[2]*b[1],a[1]*b[0]+a[3]*b[1],a[0]*b[2]+a[2]*b[3],a[1]*b[2]+a[3]*b[3],a[0]*b[4]+a[2]*b[5]+a[4],a[1]*b[4]+a[3]*b[5]+a[5]];}
function point(m,x,y){return[m[0]*x+m[2]*y+m[4],m[1]*x+m[3]*y+m[5]];}
function extrema(p0,p1,p2,p3){
 const a=-p0+3*p1-3*p2+p3,b=2*(p0-2*p1+p2),c=p1-p0,roots=[];
 if(Math.abs(a)<1e-12){if(Math.abs(b)>1e-12)roots.push(-c/b);}else{const d=b*b-4*a*c;if(d>=0)roots.push((-b+Math.sqrt(d))/(2*a),(-b-Math.sqrt(d))/(2*a));}
 const values=[p0,p3];for(const t of roots)if(t>0&&t<1)values.push((1-t)**3*p0+3*(1-t)**2*t*p1+3*(1-t)*t*t*p2+t**3*p3);return[Math.min(...values),Math.max(...values)];
}
function contentFills(content){
 // Strip strings and hex strings before tokenization so copied formula text
 // cannot be mistaken for drawing operators. Owned exports use no inline images.
 let clean='',depth=0,hex=false;
 for(let i=0;i<content.length;i++){const c=content[i];if(depth){if(c==='\\'){i++;continue;}if(c==='(')depth++;if(c===')')depth--;clean+=' ';continue;}if(hex){if(c==='>')hex=false;clean+=' ';continue;}if(c==='('){depth=1;clean+=' ';continue;}if(c==='<'&&content[i+1]!=='<'&&content[i-1]!=='<'){hex=true;clean+=' ';continue;}if(c==='%'){while(i<content.length&&content[i]!=='\n')i++;clean+='\n';continue;}clean+=c;}
 const tokens=clean.match(/[-+]?(?:\d*\.\d+|\d+\.?\d*)|\/[A-Za-z0-9_.+-]+|[A-Za-z][A-Za-z0-9*]*|<<|>>/g)||[];
 const fills=[],numbers=[],states=[];let matrix=[1,0,0,1,0,0],bounds=null,current=null,start=null;
 const add=p=>{bounds=bounds?[Math.min(bounds[0],p[0]),Math.min(bounds[1],p[1]),Math.max(bounds[2],p[0]),Math.max(bounds[3],p[1])]:[p[0],p[1],p[0],p[1]];};
 const args=n=>{assert(numbers.length>=n,'Missing PDF operator arguments');return numbers.slice(-n);};
 for(const token of tokens){if(/^[-+]?(?:\d*\.\d+|\d+\.?\d*)$/.test(token)){numbers.push(Number(token));continue;}
  if(token==='q')states.push([...matrix]);else if(token==='Q'){assert(states.length,'Unbalanced PDF graphics state');matrix=states.pop();}
  else if(token==='cm')matrix=multiply(matrix,args(6));
  else if(token==='m'){current=point(matrix,...args(2));start=current;add(current);}
  else if(token==='l'){current=point(matrix,...args(2));add(current);}
  else if(token==='c'){assert(current);const a=args(6),p1=point(matrix,a[0],a[1]),p2=point(matrix,a[2],a[3]),p3=point(matrix,a[4],a[5]);const x=extrema(current[0],p1[0],p2[0],p3[0]),y=extrema(current[1],p1[1],p2[1],p3[1]);add([x[0],y[0]]);add([x[1],y[1]]);current=p3;}
  else if(token==='h'){current=start;}
  else if(token==='re'){const[x,y,w,h]=args(4);for(const p of [[x,y],[x+w,y],[x+w,y+h],[x,y+h]])add(point(matrix,...p));}
  else if(['f','f*','B','B*','b','b*'].includes(token)){if(bounds)fills.push(bounds);bounds=null;current=start=null;}
  else if(['n','S','s'].includes(token)){bounds=null;current=start=null;}
  numbers.length=0;
 }
 assert.equal(states.length,0,'Unbalanced final PDF graphics state');return fills;
}
function inspectPdf(bytes){
 assert(Buffer.isBuffer(bytes)&&bytes.length<=32*1048576,'Owned PDF exceeds QA bound');assert(bytes.subarray(0,5).equals(Buffer.from('%PDF-')));
 const text=bytes.toString('latin1'),objects=new Map(),pattern=/(\d+)\s+\d+\s+obj\b/g;let match;
 while((match=pattern.exec(text))){const end=text.indexOf('endobj',pattern.lastIndex);assert(end>=0);objects.set(Number(match[1]),{start:pattern.lastIndex,end,body:text.slice(pattern.lastIndex,end)});pattern.lastIndex=end+6;}
 const images=[...objects].filter(([,o])=>/\/Subtype\s*\/Image\b/.test(o.body)).length,pages=[];
 for(const[id,page]of objects)if(/\/Type\s*\/Page\b/.test(page.body)){
  const entry=page.body.match(/\/Contents\s*(\[[^\]]*\]|\d+\s+\d+\s+R)/);assert(entry,'Owned PDF page has no indirect content stream');const refs=[...entry[1].matchAll(/(\d+)\s+\d+\s+R/g)].map(m=>Number(m[1])),fills=[];
  for(const ref of refs){const object=objects.get(ref);assert(object,'Missing PDF content object');const marker=object.body.match(/stream\r?\n/);assert(marker);const dictionary=object.body.slice(0,marker.index),length=dictionary.match(/\/Length\s+(\d+)\b(?!\s+\d+\s+R)/);assert(length,'Owned content stream needs a direct length');const start=object.start+marker.index+marker[0].length;let body=bytes.subarray(start,start+Number(length[1]));if(/\/Filter\s*\/FlateDecode\b/.test(dictionary))body=zlib.inflateSync(body,{maxOutputLength:8*1048576});else assert(!/\/Filter\b/.test(dictionary),'Unsupported owned content encoding');assert(body.length<=8*1048576);fills.push(...contentFills(body.toString('latin1')));}
  pages.push({object:id,fills});
 }
 assert(pages.length>0,'No actual PDF pages');return{pages,images,vectorFillCount:pages.reduce((n,p)=>n+p.fills.length,0),scope:'Owned indirect Rust content streams, actual filled Bezier/rectangle paths, no font-stream inference'};
}
function matchSvgInk(pdf,primitives,points=12){
 const expected=primitives.map(p=>[p.box.left,p.box.bottom,p.box.right,p.box.top]);assert(expected.length>=2);let best=Infinity,matched=null;
 for(let page=0;page<pdf.pages.length;page++)for(let start=0;start+expected.length<=pdf.pages[page].fills.length;start++){
  const actual=pdf.pages[page].fills.slice(start,start+expected.length),dx=actual[0][0]-expected[0][0]*points,dy=actual[0][1]-expected[0][1]*points;let error=0;
  for(let i=0;i<expected.length;i++)for(let axis=0;axis<4;axis++)error=Math.max(error,Math.abs(actual[i][axis]-(expected[i][axis]*points+(axis%2?dy:dx))));
  best=Math.min(best,error);if(error<=EPS){matched={page,start,paths:expected.length,points,maxErrorPoints:error,translation:[dx,dy]};break;}
 }
 assert(matched,'Actual PDF filled paths do not match SVG relative ink at '+points+' pt; best error='+best);assert.equal(pdf.images,0,'Owned formula PDF unexpectedly rasterizes images');return matched;
}
module.exports={inspectPdf,matchSvgInk,contentFills};
if(require.main===module){
 assert.equal(process.argv[2],'--self-test');
 assert.deepEqual(contentFills('q 2 0 0 2 10 20 cm 0 0 2 3 re f Q'),[[10,20,14,26]]);
 assert.deepEqual(contentFills('(0 0 99 99 re f) Tj <302030206d> Tj 0 0 m 1 0 l 1 2 l h f'),[[0,0,1,2]]);
 const curve=contentFills('0 0 m 0 4 4 4 4 0 c f')[0];assert(Math.abs(curve[3]-3)<1e-12);
 assert.throws(()=>contentFills('Q'),/Unbalanced/);
 const stream=zlib.deflateSync(Buffer.from('10 20 12 24 re f 34 56 24 24 re f'));
 const fixture=Buffer.concat([Buffer.from('%PDF-1.7\n1 0 obj\n<< /Type /Page /Contents 2 0 R >>\nendobj\n2 0 obj\n<< /Length '+stream.length+' /Filter /FlateDecode >>\nstream\n'),stream,Buffer.from('\nendstream\nendobj\n%%EOF')]);
 const pdf=inspectPdf(fixture);assert.equal(pdf.vectorFillCount,2);
 const shapes=[{box:{left:0,bottom:0,right:1,top:2}},{box:{left:2,bottom:3,right:4,top:5}}];
 assert.equal(matchSvgInk(pdf,shapes,12).maxErrorPoints,0);
 assert.throws(()=>matchSvgInk({...pdf,images:1},shapes,12),/rasterizes/);
 console.log(JSON.stringify({passed:true,checks:7,scope:'Synthetic independent PDF path parser checks, not native export verification'}));
}
