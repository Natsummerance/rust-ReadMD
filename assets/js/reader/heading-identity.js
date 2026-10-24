'use strict';
/* Content identities shared by reader, paged outlines, preview and TOC insertion.
   Keep allocation consistent with md_ast::HeadingIds; never change the source. */
(function(root,factory){
  if(typeof module!=='undefined'&&module.exports){
    module.exports=factory(require('../../vendor/marked.min.js'),null,require('../../vendor/html5-character-references.json'));
  }else root.ReadMDHeadings=factory(root.marked,root.document);
})(typeof window!=='undefined'?window:null,(marked,document,entities)=>{
  function slug(text){
    let value='';
    for(const char of String(text).trim()){
      if(/[\p{L}\p{N}_-]/u.test(char))value+=char.toLowerCase();
      else if(/\p{White_Space}/u.test(char))value+='-';
    }
    return value||'section';
  }
  function createIds(){
    const used=new Set(),next=new Map();
    return (text,explicit)=>{
      const base=explicit||slug(text);if(!used.has(base)){used.add(base);return base;}
      let n=next.get(base)||1,id;do{id=base+'-'+n++;}while(used.has(id));
      next.set(base,n);used.add(id);return id;
    };
  }
  function attributes(text,stripClosers=true){
    text=String(text);
    const match=text.match(/[ \t]*\{#([^\s{}]+)(?:[ \t]+\.[\w-]+)*\}[ \t]*$/u);
    if(!match)return {text,id:null,suffix:''};
    // Explicit attributes follow optional ATX closing hashes. Marked does
    // not recognize that extension, so remove both from the display title.
    const before=text.slice(0,match.index),title=stripClosers?before.replace(/[ \t]+#+[ \t]*$/,''):before;
    return {text:title,id:match[1],suffix:text.slice(title.length)};
  }
  let decoder;
  // HTML numeric references use the Windows-1252 replacements in this range.
  // The Node path uses the same HTML5 names as a browser textarea, including
  // legacy names without semicolons; no DOM shim or runtime dependency needed.
  const numericReplacements={128:8364,130:8218,131:402,132:8222,133:8230,134:8224,135:8225,136:710,137:8240,138:352,139:8249,140:338,142:381,145:8216,146:8217,147:8220,148:8221,149:8226,150:8211,151:8212,152:732,153:8482,154:353,155:8250,156:339,158:382,159:376};
  function decodeWithoutDom(text){
    return text.replace(/\r\n?/g,'\n').replace(/&(#(?:[xX][\da-fA-F]+|\d+);?|[A-Za-z][\dA-Za-z]{0,31};?)/g,(reference,name)=>{
      if(name[0]==='#'){
        const hex=/^#[xX]/.test(name);
        let cp=parseInt(name.slice(hex?2:1),hex?16:10);
        if(!cp||cp>0x10ffff||cp>=0xd800&&cp<=0xdfff)cp=0xfffd;
        return String.fromCodePoint(numericReplacements[cp]||cp);
      }
      for(let end=name.length;end>0;end--){
        const key=name.slice(0,end);
        if(Object.prototype.hasOwnProperty.call(entities,key))return entities[key]+name.slice(end);
      }
      return reference;
    });
  }
  function decode(text){
    text=String(text);
    // Most headings contain plain text. Avoid creating and parsing a DOM node
    // for every token in a book's complete outline.
    if(!/[&\r]/.test(text))return text;
    // Marked escapes an ampersand unless it starts a complete reference.
    // Protect the same literals before HTML decoding (e.g. bare "&copy").
    text=text.replace(/&(?!(#\d{1,7}|#[Xx][a-fA-F0-9]{1,6}|\w+);)/g,'&amp;');
    if(!document)return decodeWithoutDom(text);
    decoder||=document.createElement('textarea');
    // textContent preserves numeric CR references exactly as heading DOM text;
    // textarea.value would silently normalize them to LF.
    decoder.innerHTML=text.replace(/</g,'&lt;').replace(/>/g,'&gt;');return decoder.textContent;
  }
  function inlineText(tokens){return (tokens||[]).map(token=>{
    if(token.type==='html')return '';
    if(token.type==='br')return ' ';
    if(token.type==='codespan'||token.type==='escape')return token.text||'';
    if(token.tokens)return inlineText(token.tokens);
    return decode(token.text||'');
  }).join('');}
  function heading(token){
    const attrs=attributes(token.text||'');
    let tokens=token.tokens||marked.Lexer.lexInline(token.text||'');
    if(attrs.id){
      // Retain already-resolved reference links and inline formatting in the
      // title. Re-lexing the whole title would lose the document definitions.
      const title=[];let remaining=attrs.text.length;
      for(const part of tokens){
        if(remaining<=0)break;
        const raw=part.raw||'';
        if(raw.length<=remaining){title.push(part);remaining-=raw.length;}
        else {title.push(...marked.Lexer.lexInline(raw.slice(0,remaining)));break;}
      }
      tokens=title;
    }
    return {id:attrs.id,text:inlineText(tokens),tokens};
  }
  function fromMarkdown(source,allocate=createIds()){
    const rows=[];
    function walk(tokens,start=1){let line=start;for(const token of tokens){
      if(token.type==='heading'){
        const info=heading(token);
        rows.push({id:allocate(info.text,info.id),text:info.text,level:token.depth,sourceLine:line});
      }else if(token.type==='blockquote')walk(token.tokens||[],line);
      else if(token.type==='list')for(const item of token.items||[])walk(item.tokens||[],line);
      line+=(token.raw?.match(/\n/g)||[]).length;
    }}
    source=String(source).replace(/^\uFEFF?---[ \t]*\r?\n[\s\S]*?\r?\n(?:---|\.\.\.)[ \t]*(?:\r?\n|$)/,fm=>'\n'.repeat((fm.match(/\n/g)||[]).length));
    walk(marked.lexer(source,{gfm:true}));return rows;
  }
  function applyDom(body,allocate=createIds()){
    for(const heading of body.querySelectorAll('h1,h2,h3,h4,h5,h6')){
      let explicit=heading.dataset.readmdExplicitId||(!heading.dataset.readmdAutoId?heading.id:'');
      const attrs=attributes(heading.textContent,false);
      if(attrs.id){
        // Attributes are ordinary trailing text; only remove text we can map
        // precisely. Never replace heading.innerHTML or drop nested formatting.
        const walker=document.createTreeWalker(heading,NodeFilter.SHOW_TEXT);let last,node;
        while((node=walker.nextNode()))last=node;
        if(last?.nodeValue.endsWith(attrs.suffix)){last.nodeValue=last.nodeValue.slice(0,-attrs.suffix.length).replace(/[ \t]+#+[ \t]*$/,'');explicit=attrs.id;}
      }
      if(explicit)heading.dataset.readmdExplicitId=explicit;
      const title=heading.dataset.readmdHeadingText??Array.from(heading.childNodes).filter(n=>!(n.nodeType===1&&n.classList.contains('rd-anchor'))).map(n=>n.textContent).join('');
      heading.dataset.readmdHeadingText=title;
      heading.id=allocate(title,heading.dataset.readmdGlobalId||explicit);
      if(!explicit)heading.dataset.readmdAutoId='true';
    }
  }
  return {slug,createIds,attributes,heading,fromMarkdown,applyDom};
});
