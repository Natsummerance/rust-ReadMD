'use strict';
/* Content identities shared by reader, paged outlines, preview and TOC insertion.
   Keep allocation consistent with md_ast::HeadingIds; never change the source. */
window.ReadMDHeadings=(()=>{
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
  function decode(text){
    text=String(text);
    // Most headings contain plain text. Avoid creating and parsing a DOM node
    // for every token in a book's complete outline.
    if(!/[&\r]/.test(text))return text;
    decoder||=document.createElement('textarea');
    decoder.innerHTML=text.replace(/</g,'&lt;').replace(/>/g,'&gt;');return decoder.value;
  }
  function inlineText(tokens){return (tokens||[]).map(token=>{
    if(token.type==='html')return '';
    if(token.type==='br')return ' ';
    if(token.type==='codespan'||token.type==='image')return token.text||'';
    if(token.tokens)return inlineText(token.tokens);
    return decode(token.text||'');
  }).join('');}
  function fromMarkdown(source,allocate=createIds()){
    const rows=[];
    function walk(tokens,start=1){let line=start;for(const token of tokens){
      if(token.type==='heading'){
        const attrs=attributes(token.text||''),plain=inlineText(attrs.id?marked.Lexer.lexInline(attrs.text):(token.tokens||marked.Lexer.lexInline(attrs.text)));
        rows.push({id:allocate(plain,attrs.id),text:plain,level:token.depth,sourceLine:line});
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
  return {slug,createIds,attributes,fromMarkdown,applyDom};
})();
