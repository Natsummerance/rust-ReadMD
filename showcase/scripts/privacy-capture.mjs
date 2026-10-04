/** Opaque privacy overlays affect captured pixels, never application state. */
export async function protectCapture(page){
 await page.evaluate(()=>{
  let layer=document.querySelector('#showcase-privacy-layer');
  if(!layer){layer=document.createElement('div');layer.id='showcase-privacy-layer';layer.setAttribute('aria-hidden','true');layer.style.cssText='position:fixed;inset:0;z-index:2147483646;pointer-events:none';document.body.append(layer);}
  layer.replaceChildren();
  const rectangles=[];const pattern=/(?:\b[A-Za-z]:[\\/]|\/(?:Users|home)\/|Natsumer|\.codex)[^\r\n<>]*/gi;
  const styles=new WeakMap();
  const styleFor=element=>{if(!styles.has(element))styles.set(element,getComputedStyle(element));return styles.get(element);};
  function visibleRectangle(rect,element){
   if(!rect.width||!rect.height)return null;
   let left=Math.max(0,rect.left),top=Math.max(0,rect.top),right=Math.min(innerWidth,rect.right),bottom=Math.min(innerHeight,rect.bottom);
   for(let ancestor=element;ancestor;ancestor=ancestor.parentElement){
    const style=styleFor(ancestor);
    if(style.display==='none'||style.visibility==='hidden'||style.visibility==='collapse'||Number(style.opacity)===0)return null;
    if(style.overflowX!=='visible'||style.overflowY!=='visible'){
     const bounds=ancestor.getBoundingClientRect();
     if(style.overflowX!=='visible'){left=Math.max(left,bounds.left);right=Math.min(right,bounds.right);}
     if(style.overflowY!=='visible'){top=Math.max(top,bounds.top);bottom=Math.min(bottom,bounds.bottom);}
    }
   }
   return right>left&&bottom>top?{left,top,width:right-left,height:bottom-top}:null;
  }
  const walker=document.createTreeWalker(document.body,NodeFilter.SHOW_TEXT);
  while(walker.nextNode()){
   const text=walker.currentNode,parent=text.parentElement;
   if(!parent||layer.contains(parent)||parent.closest('script,style,textarea,input'))continue;
   for(const match of text.nodeValue.matchAll(pattern)){
    const range=document.createRange();range.setStart(text,match.index);range.setEnd(text,match.index+match[0].length);
    for(const rect of range.getClientRects()){const visible=visibleRectangle(rect,parent);if(visible)rectangles.push(visible);}
   }
  }
  for(const input of document.querySelectorAll('input:not([type=password]),textarea')){
   if(pattern.test(input.value||'')){const visible=visibleRectangle(input.getBoundingClientRect(),input);if(visible)rectangles.push(visible);}pattern.lastIndex=0;
  }
  for(const rect of rectangles){const box=document.createElement('div');box.style.cssText=`position:absolute;left:${Math.max(0,rect.left-2)}px;top:${Math.max(0,rect.top-2)}px;width:${rect.width+4}px;height:${rect.height+4}px;background:#e8e8ed;border-radius:3px`;layer.append(box);}
 });
}
