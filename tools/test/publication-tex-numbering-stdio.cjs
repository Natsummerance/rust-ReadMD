'use strict';
// Real source-bound MCP exports and independent PDF readback; --plan is pure.
// No GUI, new dependency, engine installation or replacement compile path.
const fs=require('node:fs'),path=require('node:path'),crypto=require('node:crypto'),assert=require('node:assert/strict');
const {spawnSync}=require('node:child_process');
const {StdioClient,assertToolCallSchema,verifyBinding,systemEnvironment}=require('./batch3-semantics-stdio.cjs');
const {payload}=require('./managed-tex-process-cold.cjs');
const {alpha}=require('./publication-numbering-oracle.cjs');
const ROOT=path.resolve(__dirname,'../..'),BASE=path.join(ROOT,'.cache/capability-upgrade/batch3-20261020/tex-numbering');
const hash=value=>crypto.createHash('sha256').update(value).digest('hex'),fileHash=file=>hash(fs.readFileSync(file));
const QA_FILES=['tools/test/publication-tex-numbering-stdio.cjs','tools/test/publication-tex-numbering-stdio.test.cjs','tools/test/publication-numbering-oracle.cjs','tools/test/publication-numbering-oracle.test.cjs','tools/test/batch3-semantics-stdio.cjs','tools/test/managed-tex-process-cold.cjs','tools/test/inspect-mcp-publication-closure.py','tools/test/inspect-mcp-publication-closure.test.py','ui-tests/publication-engines-native.cjs','tools/test/publication-runtime-reuse.cjs'];
const PRODUCT_FILES=['latex_writer.rs','latex_heading_numbering_tests.rs','latex_compile.rs','mdexport.rs','md_ast.rs','cross_references.rs','publication_layout.rs','publication_sections.rs','publication_blocks.rs','publication_typography.rs','export_styles.rs'].map(name=>'rust/readmd-kernel/src/'+name);
const RULES=[{format:'upperRoman',start:4},{format:'upperLetter',start:27,restartAfter:0},{format:'decimal',start:5,restartAfter:1},{format:'lowerLetter',start:2},{format:'lowerRoman',start:3},{format:'decimal',start:7}];
// Expected vectors are a fixed fixture contract, independent of the product plan.
const HEADINGS=[
  {level:1,numbers:[4]},{level:2,numbers:[4,27]},{level:3,numbers:[4,27,5]},
  {level:4,numbers:[4,27,5,2]},{level:5,numbers:[4,27,5,2,3]},{level:6,numbers:[4,27,5,2,3,7]},
  {level:2,numbers:[4,28]},{level:3,numbers:[4,28,6]},{level:1,numbers:[5]},
  {level:2,numbers:[5,29]},{level:3,numbers:[5,29,5]}
].map((value,index)=>({...value,key:String(index+1).padStart(2,'0'),marker:'TEXHEAD'+String(index+1).padStart(2,'0'),body:'TEXBODY'+String(index+1).padStart(2,'0')}));
const IDENTITY_HEADINGS=[
  {id:'α',label:'sec:.3b1.'},{id:'u3b1',label:'sec:u3b1'},
  {id:'x:y',label:'sec:x.3a.y'},{id:'xu3ay',label:'sec:xu3ay'},
  {id:'.',label:'sec:.2e.'},{id:'u2e',label:'sec:u2e'},
  {id:'.3b1.',label:'sec:.2e.3b1.2e.'},{id:'u2e3b1u2e',label:'sec:u2e3b1u2e'}
].map((value,index)=>({...value,key:String(index+1).padStart(2,'0'),level:1,number:index+1,marker:'TEXIDHEAD'+String(index+1).padStart(2,'0'),body:'TEXIDBODY'+String(index+1).padStart(2,'0')}));
const CASES=[
  {name:'article-dot-space',class:'article',separator:'.',suffix:'space',longTitles:true},
  {name:'book-dot-space',class:'book',separator:'.',suffix:'space',longTitles:true},
  {name:'article-colon-empty',class:'article',separator:':',suffix:'nothing'},
  {name:'book-dash-empty',class:'book',separator:'-',suffix:'nothing'},
  {name:'article-partial',class:'article',separator:'.',suffix:'space',levels:2},
  {name:'book-local-font',class:'book',separator:'.',suffix:'space',local:true},
  {name:'article-depth2',class:'article',separator:'.',suffix:'space',depth:2},
  {name:'book-depth2',class:'book',separator:'.',suffix:'space',depth:2},
  {name:'article-letter-boundaries',class:'article',separator:'.',suffix:'space',letterStart:52,letterFooter:true},
  {name:'article-label-identities',class:'article',separator:'.',suffix:'space',identity:true}
];
function inside(base,file){const relative=path.relative(base,path.resolve(file));assert(relative&&relative!=='..'&&!relative.startsWith('..'+path.sep)&&!path.isAbsolute(relative),'Path escaped owned evidence scope');return path.resolve(file);}
function sourceReadArguments(file,source){
  assert(typeof source==='string','Known source text is required for a bounded full read');
  const text=source.startsWith('\uFEFF')?source.slice(1):source,lines=Math.max(1,text.split('\n').length-Number(text.endsWith('\n')));
  assert(lines<=1000,'Full source read exceeds the real MCP 1000-line request limit');
  return {file_path:file,start_line:1,end_line:lines};
}
function roman(number){let value='';for(const [size,text]of [[1000,'M'],[900,'CM'],[500,'D'],[400,'CD'],[100,'C'],[90,'XC'],[50,'L'],[40,'XL'],[10,'X'],[9,'IX'],[5,'V'],[4,'IV'],[1,'I']])while(number>=size){value+=text;number-=size;}return value;}
function formatted(number,format){switch(format){case 'upperRoman':return roman(number);case 'lowerRoman':return roman(number).toLowerCase();case 'upperLetter':return alpha(number);case 'lowerLetter':return alpha(number).toLowerCase();case 'decimal':return String(number);default:throw Error('Unknown independent fixture format');}}
function expectations(testCase){if(testCase.identity)return IDENTITY_HEADINGS.map(heading=>({...heading,numbers:[heading.number],text:String(heading.number)}));return HEADINGS.map(heading=>{const numbers=heading.numbers.map((number,index)=>index===1?number+(testCase.letterStart??27)-27:number);return {...heading,numbers:heading.level<=(testCase.levels||6)?numbers:null,text:heading.level<=(testCase.levels||6)?numbers.map((number,index)=>formatted(number,RULES[index].format)).join(testCase.separator):''};});}
function referenceKeys(testCase){return testCase.levels===2?['01','07','10']:['01','06','11'];}
function wrapEndFor(heading,testCase){return testCase.longTitles&&['01','02','06'].includes(heading.key)?'TEXWRAPEND'+heading.key:null;}
function titleFor(heading,testCase){const end=wrapEndFor(heading,testCase);return heading.marker+(end?' Long title evidence continues across the native directory width while the following line keeps its measured hanging indent and preserves the full author heading without entering the number column. '+end:'');}
function identitySource(){
  const links=IDENTITY_HEADINGS.map(heading=>'[TEXIDLINK'+heading.key+'](#'+heading.id+')\n\nTEXIDREF'+heading.key+' $\\text{\\ref{'+heading.label+'}}$ TEXIDREFEND'+heading.key+'\n\n').join('');
  const targets=IDENTITY_HEADINGS.map(heading=>'<!-- pagebreak -->\n\n# '+heading.marker+' {#'+heading.id+'}\n\n'+heading.body+' English title and body evidence keep each source identity on its own physical page.\n\n').join('');
  return '\uFEFF'+('<!-- readmd-numbering:v1 {"headings":{"levels":[{"format":"decimal","start":1}],"suffix":"space"}} -->\n\nTEXIDHUB Independent links target separate English heading pages.\n\n'+links+targets+'TEXIDFINAL End of identity evidence.\n').replaceAll('\n','\r\n');
}
function sourceFor(testCase){
  if(testCase.identity)return identitySource();
  const declaration={headings:{levels:RULES.slice(0,testCase.levels||6).map((rule,index)=>index===1&&testCase.letterStart?{...rule,start:testCase.letterStart}:rule),separator:testCase.separator,suffix:testCase.suffix}};
  const heads=HEADINGS.map(heading=>(testCase.local&&heading.key==='01'?'<!-- readmd:layout v1 {"id":"tex-numbered-local-font","text":{"font":"TeX Gyre Termes"}} -->\n\n':'')+'#'.repeat(heading.level)+' '+titleFor(heading,testCase)+' {#h'+heading.key+'}\n\n'+heading.body+' Body evidence stays complete and appears once.\n\n').join('');
  const references=referenceKeys(testCase).map(key=>'TEXREF'+key+' $\\text{\\ref{sec:h'+key+'}}$ TEXREFEND'+key+'\n\n').join('');
  const links=['01','06','11'].map(key=>'[TEXLINK'+key+'](#h'+key+')').join(' then ');
  const footerFlow=testCase.letterFooter?Array.from({length:36},(_,index)=>'TEXFOOTFLOW'+String(index).padStart(3,'0')+' '+('Automatic page flow retains the native letter footer. '.repeat(5))+'\n\n').join(''):'';
  return '\uFEFF'+('<!-- readmd-numbering:v1 '+JSON.stringify(declaration)+' -->\n\n'+heads+references+links+'.\n\n'+footerFlow+'TEXFINAL End of retained body.\n').replaceAll('\n','\r\n');
}
function optionsFor(testCase){
  const headings={};for(let level=1;level<=6;level++)headings['h'+level]={size:16-level/2,align:'left',before:4,after:4,pageBreakBefore:false,keepNext:true};
  return {cover:{enabled:false},frontMatter:{mode:'none'},toc:{enabled:true,depth:testCase.depth||6},footer:testCase.letterFooter?{pageNumbers:true,text:'PAGEALPHAMARK',numberFormat:'upperLetter',start:52,align:'center',size:9}:{pageNumbers:false},typography:{font:'TeX Gyre Termes',size:10,lineHeight:1.15,spacing:4,align:'left'},headings,tex:{docClass:testCase.class,useCtex:false},pdf:{engine:'xelatex'}};
}
function argumentsFor(argv,env=process.env){
  const options={candidate:env.READMD_MCP_CANDIDATE,binary:env.READMD_BIN,assets:env.READMD_ASSETS_DIR,profile:env.READMD_ENGINE_REUSE_PROFILE,python:env.READMD_TEST_PYTHON,plan:false};
  for(let index=0;index<argv.length;index++){
    const key=argv[index];if(key==='--plan'){options.plan=true;continue;}if(key==='--help')return {help:true};
    assert(['--candidate','--binary','--assets','--profile','--python','--output'].includes(key),'Unknown argument: '+key);
    assert(index+1<argv.length,'Missing argument: '+key);options[key.slice(2)]=argv[++index];
  }
  for(const key of ['candidate','binary','assets','profile','python','output'])if(options[key])options[key]=path.resolve(options[key]);
  if(options.output)inside(BASE,options.output);
  if(!options.plan){assert(options.candidate,'Required --candidate');assert(options.profile,'Required explicit reviewed --profile');assert(options.python,'Required existing independent --python reader');}
  return options;
}
function bindingFor(options){
  const candidate=JSON.parse(fs.readFileSync(options.candidate,'utf8')),binary=options.binary||candidate.binary,assets=options.assets||path.dirname(candidate.boot),boot=path.join(assets,'readmd.boot.js');
  const sourceFreeze=inside(ROOT,path.resolve(ROOT,candidate.sourceFreeze));assert.equal(fileHash(sourceFreeze),candidate.sourceFreezeSha256,'Source freeze changed');
  const freeze=JSON.parse(fs.readFileSync(sourceFreeze,'utf8')),boundFiles={};
  for(const name of [...QA_FILES,...PRODUCT_FILES]){
    const row=freeze.sources.find(row=>row.path===name);assert(row,'Candidate did not freeze TeX numbering source: '+name);
    boundFiles[name]={file:inside(ROOT,path.join(ROOT,name)),copy:inside(ROOT,path.join(ROOT,row.copy)),sha256:row.sha256};
  }
  if(process.env.READMD_CANDIDATE_SHA256)assert.equal(candidate.binarySha256,process.env.READMD_CANDIDATE_SHA256);
  if(process.env.READMD_BOOT_SHA256)assert.equal(candidate.bootSha256,process.env.READMD_BOOT_SHA256);
  const binding={candidate:options.candidate,candidateSha256:fileHash(options.candidate),binary,assets,boot,binarySha256:candidate.binarySha256,bootSha256:candidate.bootSha256,sourceFreeze,sourceFreezeSha256:candidate.sourceFreezeSha256,boundFiles};verifyBinding(binding);return binding;
}
function exportRequestFor(format,parameters){assert(['tex','pdf'].includes(format));assert(!Object.hasOwn(parameters,'output_format'));return {name:'readmd_export_document',arguments:{...parameters,output_format:format}};}
function inspectTex(source,testCase){
  if(testCase.identity)return inspectIdentityTex(source,testCase);
  assert(typeof source==='string'&&source.includes('\\begin{document}\n'),'Missing actual standalone TeX document');
  const [preamble,body]=source.split('\\begin{document}\n');assert(preamble.includes(']{'+testCase.class+'}'),'Wrong actual document class');
  for(let level=1;level<=6;level++)assert.equal(preamble.split('\\newcounter{readmdh'+level+'}').length-1,1,'Missing or duplicate native heading counter');
  assert(preamble.includes('\\refstepcounter{readmdh#1}')&&preamble.includes('\\int_to_Alph:n')&&preamble.includes('\\int_to_alph:n'),'Native step or alphabet formatter missing');
  assert(preamble.includes('\\prg_replicate:nn')&&preamble.includes('\\int_div_truncate:nn')&&preamble.includes('\\int_mod:nn'),'Office repeated-letter formatter missing');
  if(testCase.letterFooter){assert(body.includes('\\renewcommand{\\thepage}{\\readmdAlph{\\value{page}}}')&&body.includes('\\setcounter{page}{52}'),'Native letter page counter missing');assert(body.includes('PAGEALPHAMARK \\thepage'),'Native letter footer omitted');}
  assert(preamble.includes('\\xdef\\readmdheadinghref{\\@currentHref}')&&preamble.includes('\\let\\@currentHref\\readmdheadinghref'),'Native heading TOC destinations were detached');
  assert(preamble.includes('\\providecommand{\\l@readmdsubparagraph}')&&preamble.includes('\\providecommand{\\toclevel@readmdsubparagraph}{6}'),'Sixth-level TOC reader is unavailable before document/TOC reading');
  assert(body.includes('\\tableofcontents\n\\newpage\n'),'Actual native TOC was omitted');
  const tocSetting='\\setcounter{tocdepth}{'+(testCase.depth||6)+'}';
  assert.equal(body.split(tocSetting).length-1,1,'Requested TOC depth did not reach the native reader');
  assert(body.indexOf(tocSetting)<body.indexOf('\\tableofcontents'),'TOC depth was set after the native reader already consumed the file');
  const expected=expectations(testCase),states=[],current=[0,0,0,0,0,0];
  for(const line of body.split('\n')){
    const update=line.match(/^\\setcounter\{readmdh([1-6])\}\{(\d+)\}$/);if(update)current[Number(update[1])-1]=Number(update[2]);
    const mark=line.match(/\\readmdnumberedheadingmark\{([1-6])\}\{(TEXHEAD\d{2})(?:\}| )/);
    if(mark){const depth=Number(mark[1]);current[depth-1]++;states.push({marker:mark[2],numbers:current.slice(0,depth)});}
  }
  assert.deepEqual(states,expected.filter(value=>value.numbers).map(({marker,numbers})=>({marker,numbers})),'Actual native counter sequence ignored starts or restartAfter');
  assert.equal((body.match(/\\csname readmdheading[1-6]\\endcsname\*\{/g)||[]).length,HEADINGS.length,'A class heading could add a second number');
  assert.equal((body.match(/\\readmdnumberedheadingtoc\{/g)||[]).length,states.length);
  for(const heading of expected){
    assert.equal(body.split('\\label{sec:h'+heading.key+'}').length-1,1,'Heading label identity changed');
    assert.equal(body.split(heading.body).length-1,1,'Body source marker was lost or duplicated');
    if(heading.numbers){
      const suffix=testCase.suffix==='space'?'\\ ':'',title=titleFor(heading,testCase),visible='\\readmdnumberedheadingmark{'+heading.level+'}{'+title+'}\\label{sec:h'+heading.key+'}\\csname thereadmdh'+heading.level+'\\endcsname{}'+suffix+title;
      assert(body.includes(visible),'Author title is not attached to the native counter or suffix changed');
    }else assert(body.includes('*{\\texorpdfstring{'+heading.marker+'}{'+heading.marker+'}}\\label{sec:h'+heading.key+'}'),'Unconfigured heading level acquired a number');
  }
  for(const key of referenceKeys(testCase)){
    assert(body.includes('\\ref{sec:h'+key+'}'),'Native REF was flattened');
    assert(body.includes('TEXREF'+key+' \\(\\text{\\ref{sec:h'+key+'}}\\) TEXREFEND'+key),'Native REF must retain text semantics');
  }
  for(const key of ['01','06','11'])assert(body.includes('\\hyperref[sec:h'+key+']{TEXLINK'+key+'}'),'Native internal link was lost');
  if(testCase.local)assert(body.includes('\\begingroup\\readmdtermesblockfont'),'Local heading font removed native numbering');
  return {class:testCase.class,sharedStyle:true,configuredLevels:testCase.levels||6,nativeCounters:6,nativeSteps:states,nativeRef:true,authorTextUsesCounter:true,physicalPdfCertified:false};
}
function occurrences(value,marker){
  return value.pages.flatMap((page,pageIndex)=>page.words.filter(word=>String(word[4]).includes(marker)).map(word=>({page:pageIndex,word})));
}
function prefixFor(page,word,marker){
  const own=String(word[4]),at=own.indexOf(marker);assert(at>=0);
  const middle=(word[1]+word[3])/2,tolerance=Math.max(1,(word[3]-word[1])*.4);
  const leading=page.words.filter(other=>other!==word&&other[0]<word[0]-.5&&Math.abs((other[1]+other[3])/2-middle)<=tolerance).sort((a,b)=>a[0]-b[0]).map(other=>String(other[4])).join('');
  return (leading+own.slice(0,at)).replace(/\s/g,'');
}
function withinPage(page,word){assert(word.slice(0,4).every(Number.isFinite));assert(word[0]>=-1&&word[1]>=-1&&word[2]<=page.width+1&&word[3]<=page.height+1,'Visible evidence is clipped outside its actual page');}
function intersect(a,b){return Math.min(a[2],b[2])-Math.max(a[0],b[0])>0&&Math.min(a[3],b[3])-Math.max(a[1],b[1])>0;}
function characterRows(page){
  assert(Array.isArray(page.characters)&&page.characters.length,'Independent PDF reader lacks actual character bounds');
  const rows=new Map();for(const [order,character]of page.characters.entries()){
    assert(character&&typeof character.text==='string'&&character.text,'Missing actual PDF character text');
    assert(Array.isArray(character.bounds)&&character.bounds.length===4&&character.bounds.every(Number.isFinite),'Invalid actual PDF character bounds');
    assert(character.bounds[0]<=character.bounds[2]&&character.bounds[1]<=character.bounds[3],'Reversed actual PDF character bounds');
    assert(Array.isArray(character.origin)&&character.origin.length===2&&character.origin.every(Number.isFinite),'Invalid actual PDF character origin');
    for(const key of ['block','line','span','index'])assert(Number.isInteger(character[key])&&character[key]>=0,'Invalid actual PDF character identity');
    assert(typeof character.font==='string'&&character.font&&Number.isFinite(character.size)&&character.size>0,'Missing actual PDF character font');
    const key=character.block+':'+character.line;if(!rows.has(key))rows.set(key,[]);rows.get(key).push({...character,order});
  }return [...rows.values()];
}
function markerCharacters(rows,marker){
  const matches=[];for(const row of rows){const text=row.map(character=>character.text).join('');let at=-1;
    while((at=text.indexOf(marker,at+1))>=0){let offset=0;const characters=row.filter(character=>{const start=offset;offset+=character.text.length;return start<at+marker.length&&offset>at;});assert.equal(characters.map(character=>character.text).join(''),marker,'PDF marker does not have exact character boundaries');matches.push(characters);}
  }assert.equal(matches.length,1,'TOC character marker missing or duplicated: '+marker);return matches[0];
}
function characterUnion(characters){return [Math.min(...characters.map(character=>character.bounds[0])),Math.min(...characters.map(character=>character.bounds[1])),Math.max(...characters.map(character=>character.bounds[2])),Math.max(...characters.map(character=>character.bounds[3]))];}
function tocNumberGeometry(page,pageIndex,heading,testCase={}){
  const rows=characterRows(page),title=markerCharacters(rows,heading.marker),baseline=title[0].origin[1],first=title[0].order;
  // Reader order identifies the literal number even when it overlaps the title;
  // word boxes and guessed per-character advances cannot establish this gap.
  const number=rows.flat().filter(character=>character.order<first&&Math.abs(character.origin[1]-baseline)<.2&&!/^\s+$/.test(character.text)).sort((a,b)=>a.order-b.order);
  assert.equal(number.map(character=>character.text).join(''),heading.text,'TOC character number differs from its expected native value: '+heading.marker);
  assert(number.length,'Missing TOC number characters');for(const character of [...number,...title])withinPage(page,character.bounds);
  const numberBounds=characterUnion(number),titleBounds=characterUnion(title),gapPt=titleBounds[0]-numberBounds[2],minimumGapPt=1;
  assert(gapPt>=minimumGapPt,'TOC number overlaps or crowds title: '+heading.marker+' gap='+gapPt.toFixed(4)+'pt');
  const wrapEnd=wrapEndFor(heading,testCase),continuationLines=[];
  if(wrapEnd){
    const end=markerCharacters(rows,wrapEnd),lastBaseline=end[0].origin[1];assert(lastBaseline>baseline+1,'Long TOC title did not wrap: '+heading.marker);
    const lines=new Map();for(const character of rows.flat())if(character.origin[1]>baseline+.2&&character.origin[1]<=lastBaseline+.2&&!/^\s+$/.test(character.text)){
      withinPage(page,character.bounds);const key=character.origin[1].toFixed(2);if(!lines.has(key))lines.set(key,[]);lines.get(key).push(character);
    }
    assert(lines.size,'Missing actual wrapped TOC character lines');for(const characters of lines.values()){
      const bounds=characterUnion(characters),clearancePt=bounds[0]-numberBounds[2];assert(bounds[0]>=titleBounds[0]-1&&clearancePt>=minimumGapPt,'Wrapped TOC title entered its number column: '+heading.marker);
      continuationLines.push({baseline:characters[0].origin[1],bounds,left:bounds[0],clearancePt});
    }
  }
  return {key:heading.key,level:heading.level,page:pageIndex,marker:heading.marker,numberText:heading.text,coordinateSpace:'pdfCharacterBounds',numberBounds,titleBounds,numberRight:numberBounds[2],titleLeft:titleBounds[0],gapPt,minimumGapPt,numberFonts:[...new Set(number.map(character=>character.font))],titleFonts:[...new Set(title.map(character=>character.font))],fontSizes:[...new Set([...number,...title].map(character=>character.size))],wrapEnd,continuationLines};
}
function normalizeMathLatin(value){
  // Unicode stores a few mathematical Latin alphabet members in Letterlike
  // Symbols. Restrict compatibility folding to those members and the Latin
  // part of Mathematical Alphanumeric Symbols; never fold numbers/punctuation.
  const legacy='\u2102\u210A\u210B\u210C\u210D\u210E\u2110\u2111\u2112\u2115\u2119\u211A\u211B\u211C\u211D\u2124\u2128\u212C\u212D\u212F\u2130\u2131\u2133\u2134';
  return [...value].map(character=>{
    const point=character.codePointAt(0);if(!(point>=0x1D400&&point<=0x1D6A3)&&!legacy.includes(character))return character;
    const plain=character.normalize('NFKC');return /^[A-Za-z]$/.test(plain)?plain:character;
  }).join('');
}
function inspectLetterFooters(value){
  const footers=[];
  for(const [pageIndex,page]of value.pages.entries()){
    const markers=page.words.filter(word=>word[4]==='PAGEALPHAMARK');assert(markers.length<=1,'Duplicate physical letter-footer marker');
    for(const marker of markers){
      withinPage(page,marker);assert(marker[1]>page.height*.85,'Letter-footer marker is body text');
      const middle=(marker[1]+marker[3])/2,numbers=page.words.filter(word=>/^[A-Z]+$/.test(word[4])&&word[0]>=marker[2]-1&&word[0]-marker[2]<120&&Math.abs((word[1]+word[3])/2-middle)<4);
      assert.equal(numbers.length,1,'Physical footer lacks one independent letter number');withinPage(page,numbers[0]);footers.push({page:pageIndex,value:numbers[0][4],bounds:numbers[0].slice(0,4)});
    }
  }
  assert(footers.length>=2,'Letter footer did not cross two actual physical pages');assert.deepEqual(footers.map(row=>row.value),footers.map((_,index)=>alpha(52+index)),'Physical footer failed 52=ZZ to 53=AAA');return footers;
}
function inspectPdf(value,testCase){
  if(testCase.identity)return inspectIdentityPdf(value);
  assert.equal(value.kind,'pdf');assert.equal(value.reader?.name,'PyMuPDF');assert(Array.isArray(value.pages)&&value.pages.length>=2,'TOC and body did not occupy distinct actual pages');
  const depth=testCase.depth||6,expected=expectations(testCase),locations=[],toc=[],tocGeometry=[];
  for(const heading of expected){
    const bodies=occurrences(value,heading.body);assert.equal(bodies.length,1,'A body marker was lost or duplicated: '+heading.body);const body=bodies[0];
    withinPage(value.pages[body.page],body.word);
    const candidates=occurrences(value,heading.marker).filter(item=>item.page===body.page&&item.word[1]<body.word[1]).sort((a,b)=>b.word[1]-a.word[1]);assert(candidates.length,'No heading precedes its own unique body: '+heading.marker);
    const actual=candidates[0],page=value.pages[actual.page];withinPage(page,actual.word);
    const prefix=prefixFor(page,actual.word,heading.marker);assert.equal(prefix,heading.text,'Actual PDF heading has a missing, doubled or wrong number: '+heading.marker);
    locations.push({key:heading.key,level:heading.level,text:heading.text,page:actual.page,bounds:actual.word.slice(0,4)});
    const entries=occurrences(value,heading.marker).filter(item=>item.page!==actual.page);
    assert.equal(entries.length,heading.level<=depth?1:0,'Actual TOC ignored requested depth '+depth+': '+heading.marker);
    for(const entry of entries){assert.equal(prefixFor(value.pages[entry.page],entry.word,heading.marker),heading.text,'Actual TOC has a missing, doubled or wrong number: '+heading.marker);withinPage(value.pages[entry.page],entry.word);if(heading.numbers)tocGeometry.push(tocNumberGeometry(value.pages[entry.page],entry.page,heading,testCase));}
    toc.push({key:heading.key,level:heading.level,entries:entries.length});
  }
  const text=value.pages.map(page=>page.text).join('\n'),references=[];
  for(const key of referenceKeys(testCase)){
    const expression=new RegExp('TEXREF'+key+'([\\s\\S]*?)TEXREFEND'+key,'g'),matches=[...text.matchAll(expression)];assert.equal(matches.length,1,'Native REF evidence missing or duplicated');
    const rawValue=matches[0][1],actual=normalizeMathLatin(rawValue).replace(/\s/g,''),expectedNumber=expected.find(value=>value.key===key).text;assert.equal(actual,expectedNumber,'Native REF did not converge to the heading counter');
    references.push({key,value:actual,rawValue});
  }
  assert.equal(occurrences(value,'TEXFINAL').length,1,'Final body evidence missing');
  assert(Array.isArray(value.outline),'Independent reader did not expose typed PDF outline');
  for(const row of value.outline)assert(Array.isArray(row)&&Number.isInteger(row[0])&&row[0]>0&&row[0]<=depth&&typeof row[1]==='string'&&Number.isInteger(row[2])&&row[2]>=1&&row[2]<=value.pages.length,'Invalid actual outline destination or depth');
  for(const heading of locations){
    const marker='TEXHEAD'+heading.key,entry=value.outline.filter(row=>row[1].includes(marker));
    assert.equal(entry.length,heading.level<=depth?1:0,'Actual outline ignored requested depth '+depth+': '+marker);
    if(entry.length){assert.equal(entry[0][0],heading.level,'Native outline hierarchy changed');assert.equal(entry[0][2],heading.page+1,'Outline destination points at another physical page');}
  }
  let internal=0;for(const page of value.pages){
    assert(Array.isArray(page.links),'Independent reader did not expose typed page links');
    for(const link of page.links)if(link.kind===1){assert(Number.isInteger(link.page)&&link.page>=0&&link.page<value.pages.length,'Unresolved actual internal PDF destination');if(link.to)assert(Array.isArray(link.to)&&link.to.length===2&&link.to.every(Number.isFinite));internal++;}
  }
  const anchors=[];for(const key of ['01','06','11']){
    const words=occurrences(value,'TEXLINK'+key);assert.equal(words.length,1,'Source link evidence missing or duplicated');const source=words[0],target=locations.find(value=>value.key===key),page=value.pages[source.page];
    const links=page.links.filter(link=>Array.isArray(link.from)&&link.from.length===4&&intersect(link.from,source.word.slice(0,4)));assert(links.length,'No actual PDF annotation covers source link '+key);
    assert(links.some(link=>link.kind===1&&link.page===target.page&&(!link.to||Math.abs(link.to[1]-target.bounds[1])<=90)),'Actual source link targets the wrong heading destination');
    anchors.push({key,sourcePage:source.page,targetPage:target.page,annotations:links});
  }
  assert(internal>=3);
  const letterFooters=testCase.letterFooter?inspectLetterFooters(value):null;
  return {pages:value.pages.length,headings:locations,bodyMarkersOnce:true,physicalNumbersAndOrder:true,nativeReferences:references,outline:value.outline,internalLinks:internal,anchors,toc,tocNumberGeometry:tocGeometry,tocNumberGeometryObserved:true,letterFooters,requestedTocDepth:depth,requestedTocAndOutlineDepthObserved:true,fullSixLevelTocObserved:depth===6&&toc.every(value=>value.entries===1),wordApplicationCertified:false};
}
function inspectIdentityTex(source,testCase){
  assert(typeof source==='string'&&source.includes('\\begin{document}\n'),'Missing actual identity TeX document');
  const [preamble,body]=source.split('\\begin{document}\n');assert(preamble.includes(']{'+testCase.class+'}'));
  assert(preamble.includes('\\refstepcounter{readmdh#1}'),'Identity headings lost their native counters');
  assert(body.includes('\\setcounter{tocdepth}{6}\n\\tableofcontents'),'Identity headings lost the actual TOC');
  const labels=[...body.matchAll(/\\label\{([^}]+)\}/g)].map(match=>match[1]);assert.equal(new Set(labels).size,labels.length,'Actual TeX contains colliding native label keys');
  const states=[...body.matchAll(/\\setcounter\{readmdh1\}\{(\d+)\}/g)].map(match=>Number(match[1])+1);
  assert.deepEqual(states,IDENTITY_HEADINGS.map(heading=>heading.number),'Identity heading counter sequence changed');
  assert((body.match(/\\newpage\b/g)||[]).length>=IDENTITY_HEADINGS.length+1,'Identity targets were not separated by native page breaks');
  for(const heading of IDENTITY_HEADINGS){
    assert.equal(labels.filter(label=>label===heading.label).length,1,'Source identity mapped to the wrong native TeX key');
    assert(body.includes('\\hyperref['+heading.label+']{TEXIDLINK'+heading.key+'}'),'Heading and link use different encoded keys');
    assert(body.includes('\\ref{'+heading.label+'}'),'Native identity REF lost its encoded key');
    assert(body.includes('TEXIDREF'+heading.key+' \\(\\text{\\ref{'+heading.label+'}}\\) TEXIDREFEND'+heading.key),'Native identity REF must retain text semantics');
    assert(body.includes('\\readmdnumberedheadingmark{1}{'+heading.marker+'}\\label{'+heading.label+'}\\csname thereadmdh1\\endcsname{}\\ '+heading.marker),'Identity title or native prefix changed');
    assert.equal(body.split(heading.body).length-1,1,'Identity source body was lost or duplicated');
  }
  assert(!body.includes('α'),'The Greek source ID became visible author text or a font requirement');
  return {class:testCase.class,sharedStyle:true,nativeCounters:true,labels:IDENTITY_HEADINGS.map(({id,label})=>({id,label})),separatePageTargets:true,physicalPdfCertified:false};
}
function inspectIdentityPdf(value){
  assert.equal(value.kind,'pdf');assert.equal(value.reader?.name,'PyMuPDF');assert(Array.isArray(value.pages)&&value.pages.length>=IDENTITY_HEADINGS.length+2,'Identity targets did not occupy separate physical pages');
  assert(Array.isArray(value.outline),'Identity PDF lacks a typed outline');
  const destinations=[],references=[],tocGeometry=[];
  for(const heading of IDENTITY_HEADINGS){
    const bodies=occurrences(value,heading.body);assert.equal(bodies.length,1,'Identity body missing or duplicated');const body=bodies[0],page=value.pages[body.page];withinPage(page,body.word);
    const titles=occurrences(value,heading.marker).filter(item=>item.page===body.page&&item.word[1]<body.word[1]);assert.equal(titles.length,1,'Identity title is missing from its own body page');const title=titles[0];withinPage(page,title.word);
    assert.equal(prefixFor(page,title.word,heading.marker),String(heading.number),'Identity heading number changed');
    const toc=occurrences(value,heading.marker).filter(item=>item.page!==body.page);assert.equal(toc.length,1,'Identity TOC title missing or duplicated');assert.equal(prefixFor(value.pages[toc[0].page],toc[0].word,heading.marker),String(heading.number),'Identity TOC number changed');
    tocGeometry.push(tocNumberGeometry(value.pages[toc[0].page],toc[0].page,{...heading,text:String(heading.number)}));
    const outline=value.outline.filter(row=>Array.isArray(row)&&String(row[1]).includes(heading.marker));assert.equal(outline.length,1,'Identity outline entry missing or duplicated');assert.equal(outline[0][0],1);assert.equal(outline[0][2],body.page+1,'Identity outline targets another heading');
    const links=occurrences(value,'TEXIDLINK'+heading.key);assert.equal(links.length,1,'Identity source-link evidence missing or duplicated');const origin=links[0],originPage=value.pages[origin.page];withinPage(originPage,origin.word);assert.notEqual(origin.page,body.page,'Identity source link and target were not separated');
    assert(Array.isArray(originPage.links),'Independent reader lacks typed identity links');
    const annotations=originPage.links.filter(link=>Array.isArray(link.from)&&link.from.length===4&&intersect(link.from,origin.word.slice(0,4)));assert.equal(annotations.length,1,'Source identity does not have one actual link annotation');
    const destination=annotations[0];assert.equal(destination.kind,1,'Native identity link is unresolved');assert.equal(destination.page,body.page,'A colliding identity link targets a different physical heading page');
    assert(Array.isArray(destination.to)&&destination.to.length===2&&destination.to.every(Number.isFinite),'Identity link lacks a resolved physical point');assert(Math.abs(destination.to[1]-title.word[1])<=90,'Identity link targets a different location on its page');
    destinations.push({id:heading.id,label:heading.label,key:heading.key,sourcePage:origin.page,targetPage:body.page,to:destination.to,headingBounds:title.word.slice(0,4)});
    const text=value.pages.map(page=>page.text).join('\n'),expression=new RegExp('TEXIDREF'+heading.key+'([\\s\\S]*?)TEXIDREFEND'+heading.key,'g'),matches=[...text.matchAll(expression)];
    assert.equal(matches.length,1,'Identity REF missing or duplicated');const rawValue=matches[0][1],actual=normalizeMathLatin(rawValue).replace(/\s/g,'');assert.equal(actual,String(heading.number),'A colliding identity REF resolved to another heading number');references.push({key:heading.key,value:actual,rawValue});
  }
  assert.equal(new Set(destinations.map(row=>row.targetPage)).size,IDENTITY_HEADINGS.length,'Distinct source identities did not retain separate heading pages');
  for(const marker of ['TEXIDHUB','TEXIDFINAL'])assert.equal(occurrences(value,marker).length,1,'Identity body boundary missing or duplicated');
  return {pages:value.pages.length,bodyMarkersOnce:true,physicalNumbersAndOrder:true,identityDestinationsObserved:true,destinations,nativeReferences:references,tocNumberGeometry:tocGeometry,tocNumberGeometryObserved:true,requestedTocDepth:6,requestedTocAndOutlineDepthObserved:true,fullSixLevelTocObserved:null,wordApplicationCertified:false};
}
function markdownValue(result){assert.notEqual(result.isError,true);const value=(result.content||[]).filter(item=>item.type==='text').map(item=>item.text).join('\n');assert(value.trim(),'Actual PDF conversion returned no Markdown');return value;}
function planFor(){
  return {schemaVersion:1,scope:'Native TeX heading counters through source export and the managed XeLaTeX PDF worker',transport:'real-stdio',synthetic:true,noProcessesStarted:true,noFilesWritten:true,guiStarted:false,managedEnginesCopied:false,cases:CASES.map(testCase=>({...testCase,tocDepth:testCase.depth||6,inputSha256:hash(sourceFor(testCase)),numbers:expectations(testCase).map(({marker,text,numbers})=>({marker,text,numbers}))})),exports:['tex','pdf'],caseIsolation:{hostPerCase:true,sessionDeadlineMs:CASE_SESSION_BUDGET,perToolRequestDeadlineMs:120000,expectedHostSessions:10,expectedToolCalls:50,expectedExports:20,performanceCertification:false},actualToolSchemaPreflight:true,engine:'xelatex',independentPdfReader:'existing inspect-mcp-publication-closure.py typed PyMuPDF words, optional raw character bounds, outline and page links',nativeRefAndAnchors:true,tocCharacterGeometry:{readerOption:'--character-bounds',expectedRows:89,minimumGapPt:1,wrappedTitles:6,syntheticWordRectanglesAccepted:false},minimumBackendPasses:2,rawCompilerPassTraceObserved:false,standaloneLegacyClassLaneCertified:false,tocAcceptance:{characterGeometry:'Every numbered physical TOC row requires at least 1pt between actual number and title character bounds. Six article/book top-level and dotted titles must wrap with their hanging indent outside the number column.',depth6:'All eleven H1..H6 titles occur exactly once in the physical TOC and typed outline, with the correct native hierarchy and destination.',depth2:'Only H1/H2 titles occur in the physical TOC and typed outline; every H3..H6 title is absent from both while remaining numbered in the body.'},auxiliaryEvidence:{available:false,reason:'The managed compiler owns a temporary workspace and removes its aux/toc files before export returns; this QA does not copy runtimes or change retention.'},limitations:['The real MCP TeX path always supplies shared publication style. Bare class-default article/book behavior is covered only by Rust regressions, not this stdio lane.','The backend owns its existing multi-pass policy. Resolved REF values and native TOC/link destinations are checked in the real PDF; a raw compiler pass trace is not exposed.','Reviewed immutable runtime reuse is separate from current worker success; no cold-performance, GUI or release acceptance.'],fullBatch3Certified:false};
}
const CASE_SESSION_BUDGET=600000;
// A functional case owns one host and one unchanged 600s session budget. Tool
// calls retain their 120s deadlines; no runtime trust crosses export operations.
// The injected factory/timers are pure control-test seams, not CLI overrides.
async function runCaseSessions({cases,report,createClient,runCase,setTimer=setTimeout,clearTimer=clearTimeout,now=()=>process.hrtime.bigint()}){
  assert(cases.length>0);assert.equal(new Set(cases.map(value=>value.name)).size,cases.length);
  const pids=new Set();
  for(const testCase of cases){
    let owned,timer,failure;
    const started=now(),session={case:testCase.name,passed:false,startedUtc:new Date().toISOString(),deadlineScope:'per-case-host-session',deadlineMs:CASE_SESSION_BUDGET,perToolRequestDeadlineMs:120000};
    try{
      owned=createClient(testCase);const pid=owned.info.pid;
      assert(Number.isSafeInteger(pid)&&pid>0,'Owned MCP host has no PID');
      assert(!pids.has(pid),'Every TeX case requires a new host PID');pids.add(pid);
      timer=setTimer(()=>{
        report.workflowTimedOut=true;report.failedCase=testCase.name;
        owned.transport.fail(Error('TeX numbering case '+testCase.name+' exceeded '+CASE_SESSION_BUDGET+'ms'),'workflow_deadline');owned.forceStop();
      },CASE_SESSION_BUDGET);
      await owned.initialize(true);await runCase(owned,testCase);
      session.workflowElapsedMs=Number(now()-started)/1e6;
      if(session.workflowElapsedMs>=CASE_SESSION_BUDGET){report.workflowTimedOut=true;throw Error('TeX numbering case '+testCase.name+' exceeded '+CASE_SESSION_BUDGET+'ms');}
      session.passed=true;
    }catch(error){failure=error;session.error=String(error.stack||error);}
    finally{
      clearTimer(timer);
      if(owned){try{await owned.stop(session);}catch(error){session.passed=false;session.stopError=String(error.stack||error);failure??=error;}}
      else session.processStarted=false;
      session.finishedUtc=new Date().toISOString();session.elapsedMs=Number(now()-started)/1e6;
      report.sessions.push(session);
    }
    if(failure||!session.passed){report.failedCase=testCase.name;throw failure||Error(session.error||'TeX case host did not close successfully');}
  }
}
function assertCaseSessions(report,cases,requireRealTransport=true){
  assert.equal(report.sessions.length,cases.length,'Every case needs a completed host session');
  assert.deepEqual(report.sessions.map(session=>session.case),cases.map(testCase=>testCase.name),'Case session coverage/order changed');
  const pids=new Set();
  for(const session of report.sessions){
    assert.equal(session.passed,true,'A case session failed');assert.equal(session.processStopped,true,'Owned MCP host did not stop');
    assert.equal(session.hostStdioClosed,true,'Owned MCP stdio did not close');assert.equal(session.logDescriptorsClosed,true,'Owned MCP logs did not close');
    assert.equal(session.transportFatal,null,'Owned MCP transport recorded a fatal error');
    assert.equal(session.deadlineMs,CASE_SESSION_BUDGET);assert.equal(session.perToolRequestDeadlineMs,120000);
    const info=session.session;assert(info,'Missing real host observation');
    if(requireRealTransport)assert.equal(info.transport,'real-stdio','A simulated session cannot certify the real lane');
    assert(Number.isSafeInteger(info.pid)&&info.pid>0);assert(!pids.has(info.pid),'Host PID was reused across cases');pids.add(info.pid);
    assert.deepEqual(info.hostExit,{code:0,signal:null});assert.equal(info.forcedStop,false,'Owned MCP host was forcibly terminated');
    assert.equal(info.pendingAtClose,0,'Owned MCP request remained pending');assert.equal(info.requests,7,'Each case needs initialization and five actual tool calls');assert.equal(info.responses,7,'Every actual case request needs a response');
  }
}

async function main(){
  const options=argumentsFor(process.argv.slice(2));if(options.help){console.log('node tools/test/publication-tex-numbering-stdio.cjs --plan\nnode tools/test/publication-tex-numbering-stdio.cjs --candidate CANDIDATE --profile REVIEWED_PROFILE --python EXISTING_QA_PYTHON');return;}
  if(options.plan){console.log(JSON.stringify(planFor(),null,2));return;}
  assert.equal(process.platform,'win32');const binding=bindingFor(options);fs.mkdirSync(BASE,{recursive:true});
  const evidence=options.output?(fs.mkdirSync(options.output),options.output):fs.mkdtempSync(path.join(BASE,'stdio-'));
  for(const name of ['documents','outputs','requests','responses','inspection','wire'])fs.mkdirSync(path.join(evidence,name));
  const retain=(name,value)=>{const file=inside(evidence,path.join(evidence,name));fs.writeFileSync(file,typeof value==='string'||Buffer.isBuffer(value)?value:JSON.stringify(value,null,2)+'\n',{flag:'wx'});return {path:name,sha256:fileHash(file)};};
  const report={...planFor(),noProcessesStarted:undefined,noFilesWritten:undefined,passed:false,startedUtc:new Date().toISOString(),candidateBinding:binding,checks:[],outputs:[],sessions:[],pdfReadbacks:[],pdfReader:{path:options.python,sha256:fileHash(options.python),qaOnly:true},currentXelatexWorkerSucceeded:false};
  let client,reuse,assets=binding.assets,profile=options.profile,sequence=0;
  const stage=value=>{assert(!report.workflowTimedOut,'TeX numbering workflow deadline already expired');report.stage=value;console.log(JSON.stringify({stage:value}));};
  const request=async(name,args,label)=>{
    const call={name,arguments:args};assertToolCallSchema(call,client.tools);const id=String(++sequence).padStart(3,'0')+'-'+label;
    retain('requests/'+id+'.json',call);const result=await client.tool(name,args,120000);retain('responses/'+id+'.json',result);return result;
  };
  const exportFile=async(testCase,format,source)=>{
    stage('Actual '+format+' '+testCase.name);verifyBinding(binding);
    const file=path.join(evidence,'outputs',testCase.name+'.'+format),call=exportRequestFor(format,{markdown_content:source,output_path:file,base_dir:path.join(evidence,'documents'),title:testCase.name,confirm:true,options:optionsFor(testCase)});
    const value=payload(await request(call.name,call.arguments,'export-'+testCase.name+'-'+format));assert(fs.statSync(file).size>0);
    assert(Array.isArray(value.warnings)&&value.warnings.every(warning=>typeof warning==='string'),'Actual MCP export omitted its canonical warnings array');
    report.outputs.push({label:testCase.name,format,path:path.relative(evidence,file),sha256:fileHash(file),bytes:fs.statSync(file).size,warnings:value.warnings,warn_items:value.warn_items});return {file,value};
  };
  const independentPdf=(file,label)=>{
    const result=spawnSync(options.python,[path.join(ROOT,'tools/test/inspect-mcp-publication-closure.py'),file,'--character-bounds'],{windowsHide:true,timeout:60000,maxBuffer:32*1024*1024,env:systemEnvironment(profile,path.join(evidence,'documents'))});
    retain('inspection/'+label+'-pdf.stdout.json',result.stdout||Buffer.alloc(0));retain('inspection/'+label+'-pdf.stderr.log',result.stderr||Buffer.alloc(0));assert(!result.error,result.error?.message);assert.equal(result.status,0,'Independent PDF reader failed');
    const value=JSON.parse(result.stdout);assert.equal(value.sha256,fileHash(file),'Independent reader inspected other PDF bytes');return value;
  };
  try{
    retain('plan.json',planFor());stage('Reviewed immutable engine reuse');
    const helpers=require('../../ui-tests/publication-engines-native.cjs'),reviewed=helpers.resolveEngineReuseProfile(options.profile);let preflight=0;
    const harness={root:ROOT,data:evidence,report,command(executable,args){const result=spawnSync(executable,args,{windowsHide:true,timeout:60000,maxBuffer:16*1024*1024});const label='runtime-'+String(++preflight).padStart(2,'0');retain('inspection/'+label+'.stdout.log',result.stdout||Buffer.alloc(0));retain('inspection/'+label+'.stderr.log',result.stderr||Buffer.alloc(0));assert.equal(result.status,0,'Reviewed runtime preflight failed');return result.stdout.toString('utf8');}};
    for(const [variable,name]of [['READMD_FONTS_PACK','readmd-fonts-2026.10.10-universal.readmd-pack'],['READMD_PDFIUM_PACK','readmd-pdfium-7881-windows-x64.readmd-pack'],['READMD_TYPST_PACK','readmd-typst-0.15.1-windows-x64.readmd-pack']])process.env[variable]??=path.join(ROOT,'.cache/capability-upgrade/locked-packs',name);
    reuse=helpers.prepareEngineProfileReuse(harness,reviewed,assets);assets=reuse.assets;profile=reviewed;
    await runCaseSessions({cases:CASES,report,
      createClient(testCase){const wire=path.join(evidence,'wire',testCase.name);fs.mkdirSync(wire);return new StdioClient({binary:binding.binary,assets,profile,documents:path.join(evidence,'documents'),out:wire});},
      async runCase(caseClient,testCase){client=caseClient;
      const original=sourceFor(testCase),file=path.join(evidence,'documents',testCase.name+'.md');retain('documents/'+testCase.name+'.md',original);
      const read=payload(await request('readmd_read_document',sourceReadArguments(file,original),'read-source-'+testCase.name));assert.equal(read.content,original.slice(1));assert.equal(read.encoding,'utf-8-sig');assert.equal(read.eol,'crlf');
      const tex=await exportFile(testCase,'tex',read.content);retain('inspection/'+testCase.name+'-tex.json',inspectTex(fs.readFileSync(tex.file,'utf8'),testCase));
      const pdf=await exportFile(testCase,'pdf',read.content),inspection=inspectPdf(independentPdf(pdf.file,testCase.name),testCase);retain('inspection/'+testCase.name+'-pdf.json',inspection);
      report.pdfReadbacks.push({case:testCase.name,identity:testCase.identity===true,identityDestinationsObserved:inspection.identityDestinationsObserved,inspection:path.join('inspection',testCase.name+'-pdf.json'),tocDepth:inspection.requestedTocDepth,tocAndOutlineDepthObserved:inspection.requestedTocAndOutlineDepthObserved,fullSixLevelTocObserved:inspection.fullSixLevelTocObserved,tocNumberGeometryObserved:inspection.tocNumberGeometryObserved,tocNumberGeometryChecks:inspection.tocNumberGeometry.length,wrappedTocTitles:inspection.tocNumberGeometry.filter(row=>row.continuationLines.length).length});report.currentXelatexWorkerSucceeded=true;
      const markdown=markdownValue(await request('readmd_convert_to_markdown',{file_path:pdf.file},'pdf-readback-'+testCase.name));retain('inspection/'+testCase.name+'-readback.md',markdown);
      for(const marker of testCase.identity?[...IDENTITY_HEADINGS.flatMap(heading=>[heading.marker,heading.body]),'TEXIDHUB','TEXIDFINAL']:[...HEADINGS.flatMap(heading=>[heading.marker,heading.body]),'TEXFINAL'])assert(markdown.includes(marker),'Actual PDF conversion lost '+marker);
      assert.equal(fileHash(file),hash(original),'Export rewrote source numbering, labels or line endings');
      const reopened=payload(await request('readmd_read_document',sourceReadArguments(file,original),'read-source-reopened-'+testCase.name));assert.equal(reopened.content,read.content);assert.equal(reopened.revision,read.revision,'Source identity changed during actual output/readback');
      report.checks.push({case:testCase.name,nativeSource:true,physicalPdf:true,actualPdfConversion:true,sourceReopened:true});
      }
    });
    assertCaseSessions(report,CASES);report.startedNewHostForEveryCase=true;
    report.fullSixLevelTocCertified=report.pdfReadbacks.filter(value=>!value.identity&&value.tocDepth===6).every(value=>value.fullSixLevelTocObserved);
    report.depthTwoTocCertified=report.pdfReadbacks.filter(value=>value.tocDepth===2).every(value=>value.tocAndOutlineDepthObserved);
    report.identityDestinationsCertified=report.pdfReadbacks.filter(value=>value.identity).every(value=>value.identityDestinationsObserved);assert.equal(report.checks.length,CASES.length);assert(report.fullSixLevelTocCertified&&report.depthTwoTocCertified&&report.identityDestinationsCertified);
    report.tocCharacterGeometryChecks=report.pdfReadbacks.reduce((count,row)=>count+row.tocNumberGeometryChecks,0);report.wrappedTocTitles=report.pdfReadbacks.reduce((count,row)=>count+row.wrappedTocTitles,0);report.tocCharacterGeometryCertified=report.pdfReadbacks.every(row=>row.tocNumberGeometryObserved&&row.tocNumberGeometryChecks>0)&&report.wrappedTocTitles===6;assert.equal(report.tocCharacterGeometryChecks,89,'Actual TOC character geometry row count changed');assert(report.tocCharacterGeometryCertified,'Actual TOC character geometry or wrapped titles were not observed');report.passed=true;
  }catch(error){report.error=String(error.stack||error);}
  finally{
    // Every created host is stopped and retained by runCaseSessions before this final binding check.
    try{verifyBinding(binding);assert.equal(fileHash(options.python),report.pdfReader.sha256,'Independent PDF reader changed');reuse?.assertPreserved();report.finalCandidateBindingVerified=true;}catch(error){report.passed=false;report.bindingError=String(error.stack||error);}
    report.finishedUtc=new Date().toISOString();retain('report.json',report);
  }
  console.log(JSON.stringify({evidence:path.relative(ROOT,evidence),passed:report.passed,checks:report.checks.length,outputs:report.outputs.length,error:report.error,bindingError:report.bindingError}));if(!report.passed)process.exitCode=1;
}
if(require.main===module)main().catch(error=>{console.error(error.stack||error);process.exitCode=1;});
module.exports={ROOT,BASE,QA_FILES,PRODUCT_FILES,RULES,HEADINGS,IDENTITY_HEADINGS,CASES,inside,sourceReadArguments,alpha,roman,formatted,expectations,referenceKeys,wrapEndFor,titleFor,identitySource,sourceFor,optionsFor,argumentsFor,bindingFor,exportRequestFor,inspectTex,occurrences,prefixFor,characterRows,markerCharacters,tocNumberGeometry,normalizeMathLatin,inspectLetterFooters,inspectPdf,inspectIdentityTex,inspectIdentityPdf,markdownValue,planFor,CASE_SESSION_BUDGET,runCaseSessions,assertCaseSessions};
