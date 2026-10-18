'use strict';
// Original synthetic material only. Fixed upstream INPUT/CSL/RESULT is loaded
// separately from private QA storage and must never be embedded in this file.
const {sourceFor, paperMetadata, bookMetadata} = require('./publication-batch3-fixtures.cjs');
const SECTION_OPTIONS = Object.freeze({typography:{font:'Arial',size:11,lineHeight:1.1,spacing:0,align:'left',firstLineIndent:0},
  page:{size:'A4',marginTop:20,marginRight:18,marginBottom:20,marginLeft:18},cover:{enabled:false},toc:{enabled:false},
  frontMatter:{mode:'none'},header:{align:'left',distance:8,size:8},footer:{pageNumbers:true,align:'right',distance:8,size:9}});
const DISPLAY_OPTIONS = Object.freeze({...SECTION_OPTIONS,typography:{...SECTION_OPTIONS.typography,lineHeight:1},footer:{pageNumbers:false}});
const declarations = [
  {id:'front',break:'nextPage',page:{size:'A4',orientation:'portrait',columns:1},numbering:{format:'lowerRoman',start:1},header:{linkPrevious:false,text:'FRONTHEADER'},footer:{linkPrevious:false,text:'FRONTFOOTER'}},
  {id:'main',break:'nextPage',page:{columns:2,columnGap:8},numbering:{format:'decimal',start:1},header:{linkPrevious:false,text:'MAINHEADER'},footer:{linkPrevious:false,text:'MAINFOOTER'}},
  {id:'full',break:'continuous',page:{columns:1},header:{linkPrevious:true},footer:{linkPrevious:true}},
  {id:'resume',break:'continuous',page:{columns:2},header:{linkPrevious:true},footer:{linkPrevious:true}},
  {id:'wide',break:'nextPage',page:{orientation:'landscape',columns:1},header:{linkPrevious:true},footer:{linkPrevious:true}},
  {id:'odd',break:'oddPage',page:{orientation:'portrait',columns:1},numbering:{format:'upperLetter',start:1},header:{linkPrevious:false,text:'ODDHEADER'},footer:{linkPrevious:false,text:'ODDFOOTER'}},
  {id:'even',break:'evenPage',numbering:{format:'decimal',start:7},header:{linkPrevious:false,text:''},footer:{linkPrevious:true}},
];
const directive = value => '<!-- readmd-section:v1 '+JSON.stringify(value)+' -->\n\n';
const sectionBodies = [
  '# Preface\n\nP03FRONT Roman preface. A real editable formula $E=mc^2$ and a [synthetic link](https://example.invalid/p03).\n\n[Go to main section](#readmd-section-main).\n\n',
  '# Main text\n\n'+Array.from({length:65},(_,i)=>`P03MAIN${String(i).padStart(2,'0')} Plain local proof.\n\n`).join('')+'[Back to front section](#readmd-section-front).\n\n',
  'P03FULL Full width continuous text.\n\n',
  'P03RESUME Two column continuous text.\n\n',
  'P03WIDE Full width landscape table.\n\n| Wide A | Wide B | Wide C | Wide D |\n|---|---|---|---|\n| Alpha proof | Beta proof | Gamma proof | Delta proof |\n\n',
  '# Odd appendix\n\nP03ODD Physical odd page, displayed letter A.\n\n',
  '# Even appendix\n\nP03EVEN Physical even page, displayed number 7 and an explicitly blank running header.\n\n',
];
const sectionSource = '\uFEFF'+('---\ntitle: Synthetic shared publication sections\nlanguage: en-US\nunknown-extension: "Keep: exact, bytes" # protected\n---\n\n'+declarations.map((s,i)=>directive(s)+sectionBodies[i]).join('')).replaceAll('\n','\r\n');
const sectionExpected = Object.freeze({ids:declarations.map(d=>d.id),physicalPages:{P03FRONT:1,P03MAIN00:2,P03MAIN64:2,P03FULL:2,P03RESUME:2,P03WIDE:3,P03ODD:5,P03EVEN:6},
  blankPages:[4],pageCount:6,numbering:{front:'i',main:'1',wide:'2',odd:'A',even:'7'},mainRows:65});
const words = 'alpha beta gamma delta plain local proof data text rows clean words safe scope known full body keeps each last mark intact'.split(' ');
const titles = ['A','B'].map(letter=>`BODY${letter} ${[...words,...words.slice(0,14)].join(' ')} END${letter}`);
const refs = titles.map((title,i)=>({id:['a','b'][i],type:'book',title,volume:['9','100'][i],author:[{literal:'Synthetic Author '+['A','B'][i]}],issued:{'date-parts':[[2026]]},URL:'https://example.invalid/structure/'+['a','b'][i]}));
function style(body,{line=1,entry=0}={}) {
  return '<style xmlns="http://purl.org/net/xbiblio/csl" version="1.0" class="in-text"><info><title>Original recursive display QA</title><id>urn:readmd:qa:publication-structure</id><updated>2026-10-15T00:00:00Z</updated></info>'+ 
    '<citation><layout prefix="[" suffix="]" delimiter=", "><text variable="citation-number"/></layout></citation>'+ 
    `<bibliography line-spacing="${line}" entry-spacing="${entry}"><layout>${body}</layout></bibliography></style>`;
}
const marker = (prefix,display,extra='') => `<group display="${display}"${extra}><text value="${prefix}"/><text variable="volume"/></group>`;
const text = (value,display,extra='') => `<text variable="${value}" display="${display}"${extra}/>`;
const nestedStyle = style(marker('NHEAD','block')+
  '<group display="left-margin">'+marker('NLEFT','left-margin')+marker('LEFTBODY','right-inline')+'</group>'+ 
  '<group display="right-inline">'+marker('NRIGHT','left-margin')+text('title','right-inline',' font-style="italic"')+'</group>'+marker('NTAIL','block'));
const longLabel = 'LONGUNSPACEDLABEL'.repeat(8);
const multiStyle = style(marker('MHEAD','block')+marker(longLabel,'left-margin')+text('title','right-inline')+
  marker('SECONDROW','right-inline')+marker('MINDENT','indent')+
  '<group display="indent">'+marker('MDEEP','indent')+'</group>'+ 
  marker('MSECOND','left-margin')+text('URL','right-inline',' font-style="italic"')+marker('MTAIL','block'),{line:3,entry:2});
const longParagraphs = Array.from({length:90},(_,i)=>`LONGPARA${String(i).padStart(3,'0')} ${words.join(' ')} ENDLONG${String(i).padStart(3,'0')}`);
const longStyle = style(marker('LHEAD','block')+'<group display="left-margin"><text value="["/><text variable="volume"/><text value="]"/></group>'+ 
  '<group display="right-inline">'+longParagraphs.map((value,i)=>`<text value="${value}" display="block"${i%2?' font-style="italic"':' font-weight="bold"'}/>`).join('')+'</group>'+marker('LTAIL','block'));
const source = (name,ids=['a','b']) => '\uFEFF'+(`---\ntitle: Synthetic ${name} publication\nlanguage: en-US\nbibliography: ${name}.json\ncsl: ${name}.csl\nunknown-extension: "Keep: exact, bytes" # protected\n---\n\n😀 中文 original source.\n\nFirst [@${ids[0]}] here.\n\nRepeat [@${ids[0]}] here.\n\nRepeat [@${ids[0]}] here.\n\nCited [${ids.map(id=>'@'+id).join('; ')}].\n\n<!--readmd:bibliography-->\n`).replaceAll('\n','\r\n');
const originalCases = [
  {id:'Nested',csl:nestedStyle,refs,source:source('Nested'),line:1,entry:0,expected:refs.map((r,i)=>({key:r.id,segments:['NHEAD'+r.volume,'NLEFT'+r.volume,'LEFTBODY'+r.volume,'NRIGHT'+r.volume,titles[i],'NTAIL'+r.volume],
    markers:{head:'NHEAD'+r.volume,left:'NLEFT'+r.volume,leftBody:'LEFTBODY'+r.volume,rightLabel:'NRIGHT'+r.volume,body:'BODY'+['A','B'][i],end:'END'+['A','B'][i],tail:'NTAIL'+r.volume}})),minimumColumns:6,minimumBlocks:4},
  {id:'Multipair',csl:multiStyle,refs,source:source('Multipair'),line:3,entry:2,expected:refs.map((r,i)=>({key:r.id,segments:['MHEAD'+r.volume,{text:longLabel+r.volume,characterWrap:true},titles[i],'SECONDROW'+r.volume,'MINDENT'+r.volume,'MDEEP'+r.volume,'MSECOND'+r.volume,r.URL,'MTAIL'+r.volume],
    markers:{head:'MHEAD'+r.volume,label:longLabel+r.volume,body:'BODY'+['A','B'][i],end:'END'+['A','B'][i],indent:'MINDENT'+r.volume,deep:'MDEEP'+r.volume,tail:'MTAIL'+r.volume}})),minimumColumns:4,minimumBlocks:8,longLabel},
  {id:'Long',csl:longStyle,refs:[refs[0]],source:source('Long',['a']),line:1,entry:0,expected:[{key:'a',segments:['LHEAD9','[9]',...longParagraphs,'LTAIL9'],markers:{head:'LHEAD9',label:'[9]',body:'LONGPARA000',end:'ENDLONG089',tail:'LTAIL9'}}],minimumColumns:1,minimumBlocks:90,minimumPdfPages:2,longParagraphs},
];
const metadataCases = [
  {id:'Paper',metadata:paperMetadata,mode:'paper',source:sourceFor(paperMetadata,'# P01 paper\n\nP01PAPER Body stays exact.\n',{bibliography:false})},
  {id:'Book',metadata:bookMetadata,mode:'book',source:sourceFor(bookMetadata,'# P01 book\n\nP01BOOK Body stays exact.\n',{bibliography:false})},
];
const invalidSections = [
  {id:'duplicate',code:'publication_section_invalid',source:directive({id:'dup'})+'A\n\n'+directive({id:'dup'})+'B\n'},
  {id:'unknown',code:'publication_section_invalid',source:directive({id:'bad',unknown:true})+'A\n'},
  {id:'continuous-page',code:'publication_section_continuous_page',source:directive({id:'a',page:{size:'A4'}})+'A\n\n'+directive({id:'b',break:'continuous',page:{orientation:'landscape'}})+'B\n'},
  {id:'header-link-conflict',code:'publication_section_invalid',source:directive({id:'bad',header:{linkPrevious:true,text:'Conflicting'}})+'A\n'},
  {id:'budget',code:'publication_section_budget',source:Array.from({length:129},(_,i)=>directive({id:'s'+i})+'A\n\n').join('')},
  {id:'geometry',code:'publication_section_geometry',source:directive({id:'bad',page:{size:'A5',columns:3,columnGap:30,gutter:40}})+'A\n'},
];
module.exports={SECTION_OPTIONS,DISPLAY_OPTIONS,declarations,directive,sectionSource,sectionExpected,originalCases,metadataCases,invalidSections,titles};
