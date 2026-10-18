import pasteboard from '@ohos.pasteboard';
import picker from '@ohos.file.picker';
import fs from '@ohos.file.fs';
import util from '@ohos.util';
import common from '@ohos.app.ability.common';
import webview from '@ohos.web.webview';
interface Call {id:number;op:string;text?:string;name?:string;}
export class ReadMDBridge {
 public origin:string='';
 constructor(private context:common.UIAbilityContext,private controller:webview.WebviewController){}
 public decode(bytes:Uint8Array):string{return util.TextDecoder.create('utf-8').decodeWithStream(bytes);}
 private directory(path:string):void{let exists=false;try{exists=fs.accessSync(path);}catch{}if(!exists)fs.mkdirSync(path,true);}
 public call(raw:string):void{
  if(!this.origin||!this.controller.getUrl().startsWith(this.origin)||raw.length>4*1024*1024)return;
  this.dispatch(raw);
 }
 private async dispatch(raw:string):Promise<void>{
  let id=0;
  try{
   const request:Call=JSON.parse(raw) as Call;id=request.id;
   let value:string='';
   if(request.op==='choose_file'){
    const results=await new picker.DocumentViewPicker(this.context).select({maxSelectNumber:1});
    if(results.length){
     const source=fs.openSync(results[0],fs.OpenMode.READ_ONLY);
     const directory=this.context.filesDir+'/readmd/documents';this.directory(directory);
     const name=decodeURIComponent(results[0].substring(results[0].lastIndexOf('/')+1)).replace(/[\\/\x00]/g,'_');
     const target=directory+'/'+Date.now()+'-'+name;
     try{fs.copyFileSync(source.fd,target);}finally{fs.closeSync(source);}
     value=target;
    }
   }else if(request.op==='save_as'){
    const name=(request.name||'Document.md').replace(/[\\/\x00]/g,'_');
    const uris=await new picker.DocumentViewPicker(this.context).save({newFileNames:[name]});
    if(uris.length){
     const file=fs.openSync(uris[0],fs.OpenMode.READ_WRITE|fs.OpenMode.CREATE|fs.OpenMode.TRUNC);
     try{fs.writeSync(file.fd,request.text||'');}finally{fs.closeSync(file);}
     const directory=this.context.filesDir+'/readmd/documents';this.directory(directory);
     const working=directory+'/'+Date.now()+'-'+name;
     const copy=fs.openSync(working,fs.OpenMode.CREATE|fs.OpenMode.WRITE_ONLY|fs.OpenMode.TRUNC);
     try{fs.writeSync(copy.fd,request.text||'');}finally{fs.closeSync(copy);}
     value=JSON.stringify({ok:true,path:working});
    }
   }else if(request.op==='clipboard'){
    const data=await pasteboard.getSystemPasteboard().getData();value=data.getPrimaryText();
   }else{throw new Error('unsupported_native_operation');}
   this.controller.runJavaScript('window.__readmdHarmonyReply('+id+','+JSON.stringify(value)+',null)');
  }catch{this.controller.runJavaScript('window.__readmdHarmonyReply('+id+',null,"native_operation_failed")');}
 }
}
