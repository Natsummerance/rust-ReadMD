'use strict';
const {test}=require('node:test'),assert=require('node:assert/strict'),path=require('node:path');
const qa=require('./publication-tex-numbering-stdio.cjs'),{assertToolCallSchema}=require('./batch3-semantics-stdio.cjs');

test('TeX numbering plan stays pure and records the real MCP and auxiliary-file boundaries',()=>{
  const plan=qa.planFor();assert.equal(plan.noProcessesStarted,true);assert.equal(plan.noFilesWritten,true);assert.equal(plan.transport,'real-stdio');
  for(const file of [...qa.QA_FILES,...qa.PRODUCT_FILES])assert(require('node:fs').statSync(path.join(qa.ROOT,file)).isFile(),'Frozen QA/product file is missing: '+file);
  assert.equal(plan.cases.length,10);assert.equal(qa.CASES.filter(value=>!value.identity).length,9);assert.deepEqual(plan.exports,['tex','pdf']);assert.equal(plan.engine,'xelatex');
  assert(plan.tocAcceptance.depth6.includes('H1..H6'));assert(plan.tocAcceptance.depth2.includes('H3..H6'));
  assert.deepEqual(plan.tocCharacterGeometry,{readerOption:'--character-bounds',expectedRows:89,minimumGapPt:1,wrappedTitles:6,syntheticWordRectanglesAccepted:false});
  assert.equal(qa.CASES.reduce((count,testCase)=>count+qa.expectations(testCase).filter(heading=>heading.numbers&&heading.level<=(testCase.depth||6)).length,0),89);
  assert.equal(plan.standaloneLegacyClassLaneCertified,false);assert.equal(plan.rawCompilerPassTraceObserved,false);assert.equal(plan.auxiliaryEvidence.available,false);assert.equal(plan.fullBatch3Certified,false);
  for(const testCase of qa.CASES){
    const source=qa.sourceFor(testCase);assert(source.startsWith('\uFEFF'));assert(!source.replaceAll('\r\n','').includes('\n'));
    if(testCase.identity){
      assert.equal((source.match(/^# TEXIDHEAD\d{2}/gm)||[]).length,8);assert.equal((source.match(/TEXIDBODY\d{2}/g)||[]).length,8);assert.equal((source.match(/<!-- pagebreak -->/g)||[]).length,8);
      for(const line of source.split('\r\n').filter(line=>line.startsWith('# ')))assert(/^[\x00-\x7f]+$/.test(line.split(' {#')[0]),'Only the source ID needs Greek, never the visible title');
    }else{assert.equal((source.match(/^#+ TEXHEAD\d{2}/gm)||[]).length,11);assert.equal((source.match(/TEXBODY\d{2}/g)||[]).length,11);}
    assert.equal(qa.optionsFor(testCase).pdf.engine,'xelatex');assert.equal(qa.optionsFor(testCase).tex.docClass,testCase.class);
    assert.equal(qa.optionsFor(testCase).toc.depth,testCase.depth||6);
    if(testCase.local)assert(source.includes('-->\r\n\r\n# TEXHEAD01 {#h01}\r\n'),'Local style must preserve terminal heading identity syntax');
    if(!testCase.identity){const expected=qa.expectations(testCase);assert.equal(expected[1].text,'IV'+testCase.separator+qa.alpha(testCase.letterStart??27));assert.equal(expected[9].text,'V'+testCase.separator+qa.alpha((testCase.letterStart??27)+2));}
  }
  assert.equal(qa.alpha(27),'AA');assert.equal(qa.alpha(28),'BB');assert.equal(qa.alpha(29),'CC');assert.equal(qa.alpha(52),'ZZ');assert.equal(qa.alpha(53),'AAA');assert.equal(qa.formatted(5,'lowerRoman'),'v');assert.throws(()=>qa.formatted(1,'invented'));
});

test('execution requires explicit candidate, reviewed runtime and independent reader',()=>{
  assert.throws(()=>qa.argumentsFor([],{}),/candidate/);assert.throws(()=>qa.argumentsFor(['--candidate','candidate'],{}),/profile/);
  assert.throws(()=>qa.argumentsFor(['--candidate','candidate','--profile','profile'],{}),/python/);
  assert.throws(()=>qa.argumentsFor(['--plan','--output',path.join(qa.ROOT,'outside')],{}),/escaped/);
  assert.throws(()=>qa.argumentsFor(['--plan','--unknown'],{}),/Unknown/);assert.throws(()=>qa.argumentsFor(['--plan','--candidate'],{}),/Missing/);
  assert.equal(qa.argumentsFor(['--help'],{}).help,true);assert.equal(qa.argumentsFor(['--plan'],{}).plan,true);
});

test('source fixtures request text-mode native REF values with unchanged identities',()=>{
  for(const testCase of qa.CASES){
    const source=qa.sourceFor(testCase),references=testCase.identity?qa.IDENTITY_HEADINGS.map(heading=>({key:heading.key,label:heading.label,marker:'TEXIDREF'})):qa.referenceKeys(testCase).map(key=>({key,label:'sec:h'+key,marker:'TEXREF'}));
    const lines=source.split('\r\n').filter(line=>/^TEX(?:ID)?REF\d{2} /.test(line));assert.equal(lines.length,references.length);
    for(const [index,reference]of references.entries()){
      const end=reference.marker==='TEXIDREF'?'TEXIDREFEND':'TEXREFEND';
      assert.equal(lines[index],reference.marker+reference.key+' $\\text{\\ref{'+reference.label+'}}$ '+end+reference.key);
    }
    assert(!source.includes('$\\ref{'),'A bare math REF changes alphabet/punctuation semantics');
  }
});

test('every TeX fixture read has exact one-based inclusive bounds and no synthetic EOF overrun',()=>{
  for(const [source,lines]of [['',1],['\uFEFF',1],['one',1],['one\n',1],['one\ntwo',2],['\uFEFFone\r\n\r\n',2],['one\r\ntwo\r\n',2],['one\n'.repeat(1000),1000]]){
    const args=qa.sourceReadArguments('owned.md',source);assert.deepEqual(args,{file_path:'owned.md',start_line:1,end_line:lines});
    assert(args.start_line>0&&args.end_line-args.start_line<1000);
  }
  assert.throws(()=>qa.sourceReadArguments('owned.md','one\n'.repeat(1001)),/1000-line/);
  assert.throws(()=>qa.sourceReadArguments('owned.md',undefined),/Known source/);
  for(const testCase of qa.CASES){
    const source=qa.sourceFor(testCase).slice(1),args=qa.sourceReadArguments('owned.md',source);
    assert.equal(args.end_line,(source.match(/\n/g)||[]).length+Number(!source.endsWith('\n')));
  }
});

test('actual export request builder owns format and preflights the real tool schema',()=>{
  const parameters={markdown_content:'source',output_path:'target.tex',base_dir:'documents',confirm:true,options:{tex:{docClass:'book'}}};
  const request=qa.exportRequestFor('tex',parameters);
  const schema={name:'readmd_export_document',inputSchema:{type:'object',additionalProperties:false,required:['markdown_content','output_path','output_format'],properties:{markdown_content:{type:'string'},output_path:{type:'string'},output_format:{type:'string',enum:['tex','pdf']},base_dir:{type:'string'},confirm:{type:'boolean'},options:{type:'object'}}}};
  assert.equal(assertToolCallSchema(request,[schema]),request);assert.equal(request.arguments.output_format,'tex');assert(!Object.hasOwn(parameters,'output_format'));
  assert.throws(()=>qa.exportRequestFor('docx',parameters));assert.throws(()=>qa.exportRequestFor('pdf',{...parameters,output_format:'tex'}));
  assert.throws(()=>assertToolCallSchema({...request,arguments:{...request.arguments,unexpected:true}},[schema]),/inputSchema/);
});

// These synthetic reader records test rejection logic only, never the product.
function texFixture(testCase){
  const preamble='\\documentclass[10pt]{'+testCase.class+'}\n'+Array.from({length:6},(_,index)=>'\\newcounter{readmdh'+(index+1)+'}\n').join('')+'\\refstepcounter{readmdh#1}\n\\int_to_Alph:n\n\\int_to_alph:n\n\\prg_replicate:nn\n\\int_div_truncate:nn\n\\int_mod:nn\n\\xdef\\readmdheadinghref{\\@currentHref}\n\\let\\@currentHref\\readmdheadinghref\n\\providecommand{\\l@readmdsubparagraph}{reader}\n\\providecommand{\\toclevel@readmdsubparagraph}{6}\n';
  const body=qa.expectations(testCase).map(heading=>{
    let text='';
    if(heading.numbers){
      text+=heading.numbers.map((number,index)=>'\\setcounter{readmdh'+(index+1)+'}{'+(number-(index+1===heading.level?1:0))+'}\n').join('');
      const title=qa.titleFor(heading,testCase);text+='\\csname readmdheading'+heading.level+'\\endcsname*{\\texorpdfstring{\\readmdnumberedheadingmark{'+heading.level+'}{'+title+'}\\label{sec:h'+heading.key+'}\\csname thereadmdh'+heading.level+'\\endcsname{}'+(testCase.suffix==='space'?'\\ ':'')+title+'}{'+heading.text+' '+title+'}}\n';
      text+='\\readmdnumberedheadingtoc{section}{'+heading.level+'}{'+heading.marker+'}\n';
    }else text+='\\csname readmdheading'+heading.level+'\\endcsname*{\\texorpdfstring{'+heading.marker+'}{'+heading.marker+'}}\\label{sec:h'+heading.key+'}\n\\addcontentsline{toc}{subsection}{'+heading.marker+'}\n';
    return text+heading.body+'\n';
  }).join('');
  return preamble+'\\begin{document}\n\\setcounter{tocdepth}{'+(testCase.depth||6)+'}\n\\tableofcontents\n\\newpage\n'+(testCase.letterFooter?'\\renewcommand{\\thepage}{\\readmdAlph{\\value{page}}}\n\\setcounter{page}{52}\nPAGEALPHAMARK \\thepage\n':'')+(testCase.local?'\\begingroup\\readmdtermesblockfont\n':'')+body+qa.referenceKeys(testCase).map(key=>'TEXREF'+key+' \\(\\text{\\ref{sec:h'+key+'}}\\) TEXREFEND'+key).join('\n')+'\n'+['01','06','11'].map(key=>'\\hyperref[sec:h'+key+']{TEXLINK'+key+'}').join('\n')+'\n\\end{document}\n';
}
test('source inspector rejects non-native prefixes, lost identity and wrong counter restarts',()=>{
  for(const testCase of qa.CASES.filter(value=>!value.identity)){
    const source=texFixture(testCase),value=qa.inspectTex(source,testCase);assert.equal(value.nativeCounters,6);assert.equal(value.physicalPdfCertified,false);
    assert.throws(()=>qa.inspectTex(source.replace('\\refstepcounter{readmdh#1}','\\stepcounter{readmdh#1}'),testCase),/Native step/);
    assert.throws(()=>qa.inspectTex(source.replace('\\label{sec:h01}','\\label{sec:changed}'),testCase),/identity/);
    const start=testCase.letterStart??27;assert.throws(()=>qa.inspectTex(source.replace('\\setcounter{readmdh2}{'+(start-1)+'}','\\setcounter{readmdh2}{'+start+'}'),testCase),/counter sequence/);
    assert.throws(()=>qa.inspectTex(source.replace('\\csname readmdheading1\\endcsname*{','\\section{'),testCase),/second number/);
    assert.throws(()=>qa.inspectTex(source.replace('\\csname thereadmdh1\\endcsname{}','IV '),testCase),/native counter/);
    assert.throws(()=>qa.inspectTex(source.replace('\\setcounter{tocdepth}{'+(testCase.depth||6)+'}','\\setcounter{tocdepth}{3}'),testCase),/TOC depth/);
    assert.throws(()=>qa.inspectTex(source.replace('\\text{\\ref{sec:h01}}','\\ref{sec:h01}'),testCase),/REF must retain text semantics/);
  }
});

function syntheticTocCharacters(page,y,heading,testCase={}){
  // Explicit synthetic glyph records exercise the oracle. Actual PDF runs use
  // rawdict character boxes; no actual word rectangle is divided into glyphs.
  page.characters??=[];const block=page.characters.length?Math.max(...page.characters.map(character=>character.block))+1:0;
  const add=(text,x,line,span)=>{for(const [index,value]of [...text].entries())page.characters.push({text:value,bounds:[x+index*5,y+line*12,x+(index+1)*5,y+line*12+10],origin:[x+index*5,y+line*12+8],block,line,span,index,font:'Synthetic QA font',size:10});};
  add(heading.text,45,0,0);const words=qa.titleFor(heading,testCase).split(' '),lines=[''];for(const word of words){if((lines.at(-1)+' '+word).trim().length>64)lines.push('');lines[lines.length-1]+=(lines.at(-1)?' ':'')+word;}
  for(const [line,text]of lines.entries())add(text,140,line,1);
}
function pdfFixture(testCase,{tocDepth=testCase.depth||6}={}){
  const page=()=>({width:595,height:842,text:'',words:[],links:[],characters:[]}),toc=page(),body=page(),expected=qa.expectations(testCase),positions=new Map();
  const add=(target,y,words)=>{
    const row=target.words.length;for(const [x,text,width=70]of words)target.words.push([x,y,x+width,y+10,text,0,row,0]);target.text+=words.map(word=>word[1]).join(' ')+'\n';
  };
  for(const [index,heading]of expected.entries()){
    const y=40+index*35,words=heading.text?(testCase.suffix==='nothing'?[[45,heading.text+heading.marker,200]]:[[45,heading.text,90],[140,heading.marker,100]]):[[140,heading.marker,100]];
    add(body,y,words);add(body,y+14,[[45,heading.body,90],[140,'Body',30]]);positions.set(heading.key,y);
    if(heading.level<=tocDepth){const tocY=40+index*60;add(toc,tocY,heading.text?[[45,heading.text,90],[140,heading.marker,100],[480,'2',10]]:[[140,heading.marker,100],[480,'2',10]]);if(heading.numbers)syntheticTocCharacters(toc,tocY,heading,testCase);toc.links.push({kind:1,page:1,from:[140,tocY-1,240,tocY+11],to:[45,y]});}
  }
  for(const [index,key]of qa.referenceKeys(testCase).entries())add(body,500+index*18,[[45,'TEXREF'+key,80],[130,expected.find(value=>value.key===key).text,120],[260,'TEXREFEND'+key,100]]);
  for(const [index,key]of ['01','06','11'].entries()){
    const x=45+index*150;add(body,610,[[x,'TEXLINK'+key,90]]);body.links.push({kind:1,page:1,from:[x,609,x+90,621],to:[45,positions.get(key)]});
  }
  add(body,650,[[45,'TEXFINAL',90]]);
  const pages=[toc,body];if(testCase.letterFooter){const next=page();add(body,810,[[230,'PAGEALPHAMARK',100],[340,'ZZ',20]]);add(next,810,[[230,'PAGEALPHAMARK',100],[340,'AAA',30]]);pages.push(next);}
  return {kind:'pdf',reader:{name:'PyMuPDF',version:'selftest only'},outline:expected.filter(heading=>heading.level<=tocDepth).map(heading=>[heading.level,heading.marker,2]),pages};
}
test('independent PDF proof accepts mixed six-level numbers, AA and merged empty suffixes',()=>{
  for(const testCase of qa.CASES.filter(value=>!value.identity)){
    const result=qa.inspectPdf(pdfFixture(testCase),testCase);assert.equal(result.bodyMarkersOnce,true);assert.equal(result.physicalNumbersAndOrder,true);
    assert.equal(result.anchors.length,3);assert.equal(result.nativeReferences.length,3);assert.equal(result.fullSixLevelTocObserved,(testCase.depth||6)===6);
    assert.equal(result.requestedTocAndOutlineDepthObserved,true);
    assert.equal(result.tocNumberGeometryObserved,true);assert.equal(result.tocNumberGeometry.length,qa.expectations(testCase).filter(heading=>heading.numbers&&heading.level<=(testCase.depth||6)).length);
    assert(result.tocNumberGeometry.every(row=>row.coordinateSpace==='pdfCharacterBounds'&&row.gapPt>=row.minimumGapPt));
    assert.equal(result.tocNumberGeometry.filter(row=>row.continuationLines.length).length,testCase.longTitles?3:0);
    assert.equal(result.wordApplicationCertified,false);assert.equal(result.headings[9].text,'V'+testCase.separator+qa.alpha((testCase.letterStart??27)+2));
  }
  assert.throws(()=>qa.inspectPdf(pdfFixture(qa.CASES[0],{tocDepth:2}),qa.CASES[0]),/TOC ignored requested depth/);
});

test('TOC geometry rejects overlapping glyphs at every level even when words still match',()=>{
  const testCase={...qa.CASES[0],longTitles:false},expected=qa.expectations(testCase);
  for(const heading of expected){
    const value=pdfFixture(testCase),page=value.pages[0],numberRight=qa.tocNumberGeometry(page,0,heading).numberRight;
    const marker=qa.markerCharacters(qa.characterRows(page),heading.marker),block=marker[0].block,shift=numberRight-marker[0].bounds[0]-2;
    for(const character of page.characters.filter(character=>character.block===block&&character.span===1)){character.bounds[0]+=shift;character.bounds[2]+=shift;character.origin[0]+=shift;}
    assert.throws(()=>qa.inspectPdf(value,testCase),/TOC number overlaps or crowds title/);
  }
  const merged=pdfFixture(testCase),first=expected[0];merged.pages[0].words=merged.pages[0].words.filter(word=>![first.text,first.marker].includes(word[4]));merged.pages[0].words.push([45,40,240,50,first.text+first.marker]);
  assert.equal(qa.inspectPdf(merged,testCase).tocNumberGeometry.length,11);
  const missing=pdfFixture(testCase);delete missing.pages[0].characters;assert.throws(()=>qa.inspectPdf(missing,testCase),/lacks actual character bounds/);
  const bad=pdfFixture(testCase);bad.pages[0].characters[0].bounds[2]=NaN;assert.throws(()=>qa.inspectPdf(bad,testCase),/Invalid actual PDF character bounds/);
  const wrong=pdfFixture(testCase);wrong.pages[0].characters[0].text='X';assert.throws(()=>qa.inspectPdf(wrong,testCase),/TOC character number differs/);
});

test('TOC geometry measures dynamic long numbers and requires native hanging continuation',()=>{
  const heading={key:'06',level:6,marker:'TEXHEAD06',text:'MMMDCCCLXXXVIII.AAA.10000.b.iii.777'},testCase={longTitles:true};
  const page={width:900,height:842,characters:[]};syntheticTocCharacters(page,40,heading,testCase);
  for(const character of page.characters.filter(character=>character.span===1)){character.bounds[0]+=150;character.bounds[2]+=150;character.origin[0]+=150;}
  const positive=qa.tocNumberGeometry(page,0,heading,testCase);assert.equal(positive.numberText,heading.text);assert(positive.continuationLines.length>=2);assert(positive.gapPt>1);
  const wrong=structuredClone(page);for(const character of wrong.characters.filter(character=>character.line>0)){character.bounds[0]-=100;character.bounds[2]-=100;character.origin[0]-=100;}assert.throws(()=>qa.tocNumberGeometry(wrong,0,heading,testCase),/entered its number column/);
  const flat=structuredClone(page);for(const character of flat.characters){character.origin[1]=48;character.bounds[1]=40;character.bounds[3]=50;}assert.throws(()=>qa.tocNumberGeometry(flat,0,heading,testCase),/did not wrap/);
  for(const name of ['article-dot-space','book-dot-space']){
    const fixture=qa.CASES.find(value=>value.name===name),source=qa.sourceFor(fixture);
    for(const key of ['01','02','06'])assert(source.includes(qa.titleFor(qa.HEADINGS.find(heading=>heading.key===key),fixture)+' {#h'+key+'}'));
    assert.equal(qa.inspectPdf(pdfFixture(fixture),fixture).tocNumberGeometry.filter(row=>row.continuationLines.length).length,3);
  }
});

test('depth six and depth two require exact actual TOC and outline membership',()=>{
  const deep=qa.CASES[0],shallow=qa.CASES.find(value=>value.name==='book-depth2');
  const shallowProof=qa.inspectPdf(pdfFixture(shallow),shallow);assert.equal(shallowProof.requestedTocDepth,2);
  assert(shallowProof.toc.every(value=>value.entries===(value.level<=2?1:0)));assert(shallowProof.outline.every(row=>row[0]<=2));
  const missingDeepOutline=pdfFixture(deep);missingDeepOutline.outline=missingDeepOutline.outline.filter(row=>row[0]!==6);assert.throws(()=>qa.inspectPdf(missingDeepOutline,deep),/outline ignored requested depth/);
  const extraShallowToc=pdfFixture(shallow,{tocDepth:6});assert.throws(()=>qa.inspectPdf(extraShallowToc,shallow),/TOC ignored requested depth/);
  const extraShallowOutline=pdfFixture(shallow);extraShallowOutline.outline.push([6,'TEXHEAD06',2]);assert.throws(()=>qa.inspectPdf(extraShallowOutline,shallow),/outline destination or depth/);
});

test('independent PDF proof rejects doubled numbers, wrong restart and missing or clipped body',()=>{
  const testCase=qa.CASES[0],value=pdfFixture(testCase);
  const doubled=structuredClone(value);doubled.pages[1].words.push([20,40,35,50,'1']);assert.throws(()=>qa.inspectPdf(doubled,testCase),/doubled or wrong/);
  const wrong=structuredClone(value),last=wrong.pages[1].words.find(word=>word[4]==='V.CC.5');last[4]='V.CC.7';assert.throws(()=>qa.inspectPdf(wrong,testCase),/wrong number/);
  const missing=structuredClone(value);missing.pages[1].words=missing.pages[1].words.filter(word=>word[4]!=='TEXBODY06');assert.throws(()=>qa.inspectPdf(missing,testCase),/lost or duplicated/);
  const duplicate=structuredClone(value);duplicate.pages[1].words.push([...duplicate.pages[1].words.find(word=>word[4]==='TEXBODY06')]);assert.throws(()=>qa.inspectPdf(duplicate,testCase),/lost or duplicated/);
  const clipped=structuredClone(value);clipped.pages[1].words.find(word=>word[4]==='TEXBODY11')[3]=900;assert.throws(()=>qa.inspectPdf(clipped,testCase),/clipped/);
  const badToc=structuredClone(value);badToc.pages[0].words.find(word=>word[4]==='IV.AA')[4]='IV.Z';assert.throws(()=>qa.inspectPdf(badToc,testCase),/Actual TOC/);
});

test('PDF REF, outline and link assertions require resolved native values and physical targets',()=>{
  const testCase=qa.CASES[0],value=pdfFixture(testCase);
  const unresolved=structuredClone(value);unresolved.pages[1].text=unresolved.pages[1].text.replace('TEXREF06 IV.AA.5.b.iii.7','TEXREF06 ??');assert.throws(()=>qa.inspectPdf(unresolved,testCase),/REF did not converge/);
  const badOutline=structuredClone(value);badOutline.outline[0][2]=1;assert.throws(()=>qa.inspectPdf(badOutline,testCase),/Outline destination/);
  const noOutline=structuredClone(value);delete noOutline.outline;assert.throws(()=>qa.inspectPdf(noOutline,testCase),/typed PDF outline/);
  const badTarget=structuredClone(value);badTarget.pages[1].links[0].page=5;assert.throws(()=>qa.inspectPdf(badTarget,testCase),/Unresolved actual/);
  const wrongPage=structuredClone(value);wrongPage.pages[1].links[0].page=0;assert.throws(()=>qa.inspectPdf(wrongPage,testCase),/wrong heading destination/);
  const wrongAnchor=structuredClone(value);wrongAnchor.pages[1].links[0].to=[45,800];assert.throws(()=>qa.inspectPdf(wrongAnchor,testCase),/wrong heading destination/);
  const missingLink=structuredClone(value);missingLink.pages[1].links.shift();assert.throws(()=>qa.inspectPdf(missingLink,testCase),/annotation covers/);
  const unnamed=structuredClone(value);unnamed.pages[1].links[0]={...unnamed.pages[1].links[0],kind:4,name:'unresolved',page:-1};assert.throws(()=>qa.inspectPdf(unnamed,testCase),/wrong heading destination/);
  const unresolvedNamed=structuredClone(value);unresolvedNamed.pages[1].links[0]={...unresolvedNamed.pages[1].links[0],kind:4,nameddest:'readmdh1.1'};assert.throws(()=>qa.inspectPdf(unresolvedNamed,testCase),/wrong heading destination/);
});

test('REF comparison folds only standard mathematical Latin alphabets and retains raw evidence',()=>{
  const testCase=qa.CASES[0],value=pdfFixture(testCase),references=[['01','IV','𝐼𝑉'],['06','IV.AA.5.b.iii.7','𝐼𝑉.𝐴𝐴.5.𝑏.𝑖𝑖𝑖.7'],['11','V.CC.5','𝑉.𝐶𝐶.5']];
  for(const [key,plain,styled]of references)value.pages[1].text=value.pages[1].text.replace('TEXREF'+key+' '+plain+' TEXREFEND'+key,'TEXREF'+key+' '+styled+' TEXREFEND'+key);
  const proof=qa.inspectPdf(value,testCase);assert.deepEqual(proof.nativeReferences.map(row=>row.value),references.map(row=>row[1]));
  for(const [index,row]of proof.nativeReferences.entries())assert.equal(row.rawValue.trim(),references[index][2]);
  assert.equal(qa.normalizeMathLatin('𝐼𝑉.𝐴𝐴.5.𝑏.𝑖𝑖𝑖.7'),'IV.AA.5.b.iii.7');assert.equal(qa.normalizeMathLatin('ℎℂℤ𝔞'),'hCZa');
  const authored='IV:AA-5.b/iii 7 ?? ⅠⅤ ＩＶ ⒾⓋ KÅ 𝟝 ⁵ Ι−';assert.equal(qa.normalizeMathLatin(authored),authored);
  for(const wrong of ['??','𝐼𝑋','𝑖𝑣','𝐼𝑉.𝐴𝐵.5.𝑏.𝑖𝑖𝑖.7','𝐼𝑉:𝐴𝐴.5.𝑏.𝑖𝑖𝑖.7','𝐼𝑉.𝐴𝐴.6.𝑏.𝑖𝑖𝑖.7','𝐼𝑉.𝐴𝐴.5.𝐵.𝑖𝑖𝑖.7','ⅠⅤ','ＩＶ']){
    const broken=structuredClone(value);broken.pages[1].text=broken.pages[1].text.replace('TEXREF06 𝐼𝑉.𝐴𝐴.5.𝑏.𝑖𝑖𝑖.7 TEXREFEND06','TEXREF06 '+wrong+' TEXREFEND06');assert.throws(()=>qa.inspectPdf(broken,testCase),/REF did not converge/);
  }
  const duplicate=structuredClone(value);duplicate.pages[1].text+='TEXREF01 𝐼𝑉 TEXREFEND01\n';assert.throws(()=>qa.inspectPdf(duplicate,testCase),/missing or duplicated/);
  const missing=structuredClone(value);missing.pages[1].text=missing.pages[1].text.replace('TEXREFEND06','BROKENEND06');assert.throws(()=>qa.inspectPdf(missing,testCase),/missing or duplicated/);
});

test('exact REF punctuation rejects mathematical colon and minus substitutions',()=>{
  for(const [name,plain,math]of [['article-colon-empty',':','∶'],['book-dash-empty','-','−']]){
    const testCase=qa.CASES.find(value=>value.name===name),value=pdfFixture(testCase),expected=qa.expectations(testCase).find(heading=>heading.key==='06').text;
    assert.equal(qa.inspectPdf(value,testCase).nativeReferences.find(row=>row.key==='06').value,expected);
    const altered=expected.replaceAll(plain,math);assert.notEqual(altered,expected);assert.equal(qa.normalizeMathLatin(altered),altered);
    value.pages[1].text=value.pages[1].text.replace('TEXREF06 '+expected+' TEXREFEND06','TEXREF06 '+altered+' TEXREFEND06');assert.throws(()=>qa.inspectPdf(value,testCase),/REF did not converge/);
  }
});

test('letter-page proof uses the physical footer beside its marker, independently of heading ZZ',()=>{
  const testCase=qa.CASES.find(value=>value.letterFooter),value=pdfFixture(testCase),proof=qa.inspectPdf(value,testCase);
  assert.deepEqual(proof.letterFooters.map(row=>row.value),['ZZ','AAA']);assert(proof.headings.some(row=>row.text.includes('ZZ')));
  const missing=structuredClone(value);for(const page of missing.pages)page.words=page.words.filter(word=>word[1]<800);assert.throws(()=>qa.inspectPdf(missing,testCase),/two actual physical pages/);
  const wrong=structuredClone(value);wrong.pages[2].words.find(word=>word[4]==='AAA')[4]='BA';assert.throws(()=>qa.inspectPdf(wrong,testCase),/52=ZZ to 53=AAA/);
  const inBody=structuredClone(value);inBody.pages[1].words.find(word=>word[4]==='PAGEALPHAMARK')[1]=400;assert.throws(()=>qa.inspectPdf(inBody,testCase),/body text/);
});

function identityTexFixture(){
  const source='\\documentclass[10pt]{article}\n\\refstepcounter{readmdh#1}\n\\begin{document}\n\\setcounter{tocdepth}{6}\n\\tableofcontents\n\\newpage\n'
    +qa.IDENTITY_HEADINGS.map(heading=>'\\hyperref['+heading.label+']{TEXIDLINK'+heading.key+'}\nTEXIDREF'+heading.key+' \\(\\text{\\ref{'+heading.label+'}}\\) TEXIDREFEND'+heading.key+'\n').join('')
    +qa.IDENTITY_HEADINGS.map(heading=>'\\newpage\n\\setcounter{readmdh1}{'+(heading.number-1)+'}\n\\csname readmdheading1\\endcsname*{\\texorpdfstring{\\readmdnumberedheadingmark{1}{'+heading.marker+'}\\label{'+heading.label+'}\\csname thereadmdh1\\endcsname{}\\ '+heading.marker+'}{'+heading.marker+'}}\n'+heading.body+'\n').join('')+'\\end{document}\n';
  return source;
}
test('identity source proof requires eight distinct exact keys and matching native links',()=>{
  const testCase=qa.CASES.find(value=>value.identity),source=identityTexFixture(),proof=qa.inspectTex(source,testCase);
  assert.equal(proof.labels.length,8);assert.equal(proof.physicalPdfCertified,false);assert.equal(new Set(proof.labels.map(row=>row.label)).size,8);
  assert.throws(()=>qa.inspectTex(source.replaceAll('sec:.3b1.','sec:u3b1'),testCase),/colliding native label/);
  assert.throws(()=>qa.inspectTex(source.replaceAll('sec:.3b1.','sec:.3b2.'),testCase),/wrong native TeX key/);
  assert.throws(()=>qa.inspectTex(source.replace('\\hyperref[sec:.3b1.]','\\hyperref[sec:u3b1]'),testCase),/different encoded keys/);
  assert.throws(()=>qa.inspectTex(source.replace('TEXIDBODY01','TEXIDBODY01 α'),testCase),/Greek source ID became visible/);
  assert.throws(()=>qa.inspectTex(source.replace('\\text{\\ref{sec:.3b1.}}','\\ref{sec:.3b1.}'),testCase),/identity REF must retain text semantics/);
});

function identityPdfFixture(){
  const page=()=>({width:595,height:842,text:'',words:[],links:[],characters:[]}),toc=page(),hub=page(),targets=qa.IDENTITY_HEADINGS.map(()=>page());
  const add=(target,y,words)=>{for(const [x,text,width=100]of words)target.words.push([x,y,x+width,y+10,text]);target.text+=words.map(word=>word[1]).join(' ')+'\n';};
  add(hub,20,[[45,'TEXIDHUB']]);
  for(const [index,heading]of qa.IDENTITY_HEADINGS.entries()){
    const target=targets[index],y=50+index*65;add(toc,40+index*22,[[45,String(heading.number),20],[100,heading.marker,130]]);
    syntheticTocCharacters(toc,40+index*22,{...heading,text:String(heading.number)});
    add(target,50,[[45,String(heading.number),20],[100,heading.marker,130]]);add(target,80,[[45,heading.body,130]]);
    add(hub,y,[[45,'TEXIDLINK'+heading.key,100]]);add(hub,y+15,[[45,'TEXIDREF'+heading.key,100],[150,String(heading.number),15],[170,'TEXIDREFEND'+heading.key,120]]);
    hub.links.push({kind:1,page:index+2,from:[45,y-1,145,y+11],to:[45,50]});
  }
  add(targets.at(-1),120,[[45,'TEXIDFINAL',110]]);
  return {kind:'pdf',reader:{name:'PyMuPDF',version:'selftest only'},outline:qa.IDENTITY_HEADINGS.map((heading,index)=>[1,heading.marker,index+3]),pages:[toc,hub,...targets]};
}
test('identity PDF proof requires eight separately resolved pages and rejects the old collisions',()=>{
  const testCase=qa.CASES.find(value=>value.identity),value=identityPdfFixture(),proof=qa.inspectPdf(value,testCase);
  assert.equal(proof.identityDestinationsObserved,true);assert.equal(new Set(proof.destinations.map(row=>row.targetPage)).size,8);assert.equal(proof.nativeReferences.length,8);
  assert.equal(proof.tocNumberGeometry.length,8);assert.equal(proof.tocNumberGeometryObserved,true);
  const oldCollision=structuredClone(value);oldCollision.pages[1].links[0].page=3;assert.throws(()=>qa.inspectPdf(oldCollision,testCase),/different physical heading page/);
  const wrongPoint=structuredClone(value);wrongPoint.pages[1].links[0].to=[45,800];assert.throws(()=>qa.inspectPdf(wrongPoint,testCase),/different location/);
  const unresolved=structuredClone(value);unresolved.pages[1].links[0].kind=4;assert.throws(()=>qa.inspectPdf(unresolved,testCase),/unresolved/);
  const missing=structuredClone(value);missing.pages[1].links.shift();assert.throws(()=>qa.inspectPdf(missing,testCase),/one actual link annotation/);
  const wrongRef=structuredClone(value);wrongRef.pages[1].text=wrongRef.pages[1].text.replace('TEXIDREF01 1','TEXIDREF01 2');assert.throws(()=>qa.inspectPdf(wrongRef,testCase),/another heading number/);
  const missingBody=structuredClone(value);missingBody.pages[2].words=missingBody.pages[2].words.filter(word=>word[4]!=='TEXIDBODY01');assert.throws(()=>qa.inspectPdf(missingBody,testCase),/Identity body missing/);
});

// Focused controls use fake owned clients only. They never start a host/compiler.
function fakeCaseClient(pid,events,behavior={}){
  const info={pid,transport:'simulated-stdio-selftest',forcedStop:false,requests:7,responses:7,pendingAtClose:0,hostExit:{code:0,signal:null}};
  let fatal=null;
  return {info,transport:{fail(error,kind){fatal={kind,message:error.message};behavior.onFatal?.(error);}},
    forceStop(){events.push(['force',pid]);info.forcedStop=true;info.hostExit={code:null,signal:'SIGTERM'};},
    async initialize(exportsEnabled){events.push(['initialize',pid,exportsEnabled]);if(behavior.initializeError)throw Error(behavior.initializeError);},
    async stop(report){events.push(['stop',pid]);Object.assign(report,{session:info,processStopped:true,hostStdioClosed:true,logDescriptorsClosed:true,transportFatal:fatal});if(fatal)report.passed=false;
      if(behavior.stopFatal){report.transportFatal={kind:'invalid_json',message:behavior.stopFatal};report.passed=false;}
      if(behavior.unclosed){report.hostStdioClosed=false;report.passed=false;}
      if(behavior.stopError)throw Error(behavior.stopError);
    }};
}
const fakeCases=qa.CASES.map(value=>({name:value.name}));

test('each functional TeX case owns a distinct bounded host and complete closure',async()=>{
  const events=[],report={sessions:[]},timers=[];
  await qa.runCaseSessions({cases:fakeCases,report,createClient(testCase){const pid=900+fakeCases.indexOf(testCase);events.push(['create',pid]);return fakeCaseClient(pid,events);},async runCase(client,testCase){events.push(['case',client.info.pid,testCase.name]);},setTimer(callback,ms){timers.push(ms);return callback;},clearTimer(){}});
  qa.assertCaseSessions(report,fakeCases,false);assert.equal(report.sessions.length,10);assert.deepEqual(timers,Array(10).fill(600000));
  assert.equal(events.filter(e=>e[0]==='initialize').length,10);assert.equal(events.filter(e=>e[0]==='case').length,10);assert.equal(events.filter(e=>e[0]==='stop').length,10);
  for(let i=1;i<10;i++)assert(events.findIndex(e=>e[0]==='stop'&&e[1]===899+i)<events.findIndex(e=>e[0]==='create'&&e[1]===900+i),'The next host started before its predecessor closed');
  assert.throws(()=>qa.assertCaseSessions(report,fakeCases),/simulated|real-stdio/,'A fake control must never certify real stdio');
});

test('the first failing case is retained and later cases never start',async()=>{
  const events=[],report={sessions:[]};
  await assert.rejects(qa.runCaseSessions({cases:fakeCases,report,createClient(testCase){return fakeCaseClient(1000+fakeCases.indexOf(testCase),events);},async runCase(client,testCase){if(testCase.name===fakeCases[2].name)throw Error('retained original PDF failure');}}),/retained original PDF failure/);
  assert.equal(report.sessions.length,3);assert.equal(report.failedCase,fakeCases[2].name);assert.equal(report.sessions[2].passed,false);assert(report.sessions[2].error.includes('retained original PDF failure'));assert.equal(events.filter(e=>e[0]==='stop').length,3);assert.throws(()=>qa.assertCaseSessions(report,fakeCases,false));
});

test('a fatal observed after case work resolves prevents the next host',async()=>{
  const events=[],report={sessions:[]};await assert.rejects(qa.runCaseSessions({cases:fakeCases,report,createClient(){return fakeCaseClient(1100,events,{stopFatal:'late malformed stdout'});},async runCase(){}}),/host did not close/);
  assert.equal(report.sessions.length,1);assert.equal(report.sessions[0].transportFatal.message,'late malformed stdout');assert.equal(report.sessions[0].passed,false);
});

test('unclosed EOF is a failed case and cannot be hidden by successful outputs',async()=>{
  const report={sessions:[]};await assert.rejects(qa.runCaseSessions({cases:fakeCases,report,createClient(){return fakeCaseClient(1200,[],{unclosed:true});},async runCase(){}}),/host did not close/);assert.equal(report.sessions.length,1);assert.equal(report.sessions[0].hostStdioClosed,false);assert.throws(()=>qa.assertCaseSessions(report,fakeCases,false));
});

test('a repeated host PID is rejected, stopped and never initialized twice',async()=>{
  const report={sessions:[]},events=[];await assert.rejects(qa.runCaseSessions({cases:fakeCases,report,createClient(){return fakeCaseClient(1300,events);},async runCase(){}}),/new host PID/);assert.equal(report.sessions.length,2);assert.equal(events.filter(e=>e[0]==='initialize').length,1);assert.equal(events.filter(e=>e[0]==='stop').length,2);assert.equal(report.sessions[1].passed,false);
});

test('the unchanged 600000ms timer fails its owning case and forces only that host',async()=>{
  const report={sessions:[]},events=[];let fire,rejectPending;
  const work=new Promise((resolve,reject)=>{rejectPending=reject;});
  await assert.rejects(qa.runCaseSessions({cases:fakeCases,report,createClient(){return fakeCaseClient(1400,events,{onFatal:rejectPending});},runCase(){fire();return work;},setTimer(callback,ms){assert.equal(ms,600000);fire=callback;return callback;},clearTimer(){}}),/exceeded 600000ms/);
  assert.equal(report.workflowTimedOut,true);assert.equal(report.failedCase,fakeCases[0].name);assert.equal(report.sessions.length,1);assert.equal(report.sessions[0].transportFatal.kind,'workflow_deadline');assert.equal(report.sessions[0].session.forcedStop,true);assert.equal(events.filter(e=>e[0]==='force').length,1);
});

test('cleanup errors retain the first case error and its failed session',async()=>{
  const report={sessions:[]};await assert.rejects(qa.runCaseSessions({cases:fakeCases,report,createClient(){return fakeCaseClient(1500,[],{stopError:'secondary close failure'});},async runCase(){throw Error('first source assertion failed');}}),/first source assertion failed/);assert.equal(report.sessions.length,1);assert(report.sessions[0].error.includes('first source assertion failed'));assert(report.sessions[0].stopError.includes('secondary close failure'));assert.equal(report.sessions[0].passed,false);
});

test('monotonic completion rejects a budget overrun before a delayed timer callback',async()=>{
  const report={sessions:[]};let clock=0n;
  await assert.rejects(qa.runCaseSessions({cases:fakeCases,report,createClient(){return fakeCaseClient(1550,[]);},async runCase(){clock=600000000001n;},now(){return clock;},setTimer(callback,ms){assert.equal(ms,600000);return callback;},clearTimer(){}}),/exceeded 600000ms/);
  assert.equal(report.workflowTimedOut,true);assert.equal(report.sessions.length,1);assert.equal(report.sessions[0].passed,false);assert(report.sessions[0].workflowElapsedMs>600000);assert.equal(report.sessions[0].session.forcedStop,false);assert.deepEqual(report.sessions[0].session.hostExit,{code:0,signal:null});
});

test('session aggregation rejects lost responses, fatal transport, bad exits and wrong coverage',async()=>{
  const report={sessions:[]};await qa.runCaseSessions({cases:fakeCases,report,createClient(testCase){return fakeCaseClient(1600+fakeCases.indexOf(testCase),[]);},async runCase(){}});qa.assertCaseSessions(report,fakeCases,false);
  const changes=[r=>{r.sessions.pop();},r=>{r.sessions[0].case='unknown';},r=>{r.sessions[0].session.responses=6;},r=>{r.sessions[0].session.pendingAtClose=1;},r=>{r.sessions[0].transportFatal={kind:'stdout_budget',message:'too much'};},r=>{r.sessions[0].session.hostExit={code:1,signal:null};},r=>{r.sessions[0].session.forcedStop=true;},r=>{r.sessions[0].logDescriptorsClosed=false;},r=>{r.sessions[0].session.pid=r.sessions[1].session.pid;}];
  for(const change of changes){const broken=structuredClone(report);change(broken);assert.throws(()=>qa.assertCaseSessions(broken,fakeCases,false));}
});

test('case isolation preserves the ten-case twenty-export functional matrix and excludes D06 claims',()=>{
  const plan=qa.planFor();assert.deepEqual(plan.caseIsolation,{hostPerCase:true,sessionDeadlineMs:600000,perToolRequestDeadlineMs:120000,expectedHostSessions:10,expectedToolCalls:50,expectedExports:20,performanceCertification:false});assert.equal(plan.cases.length,10);assert.equal(plan.tocCharacterGeometry.expectedRows,89);assert.equal(plan.tocCharacterGeometry.wrappedTitles,6);assert.equal(plan.fullBatch3Certified,false);assert.deepEqual(plan.exports,['tex','pdf']);
});
