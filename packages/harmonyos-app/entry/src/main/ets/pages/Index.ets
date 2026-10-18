import webview from '@ohos.web.webview';
import fs from '@ohos.file.fs';
import common from '@ohos.app.ability.common';
import * as kernel from 'libreadmd_bridge.so';
import {ReadMDBridge} from '../bridge/ReadMDBridge';
interface KernelManifest {version:string;files:string[];}
interface KernelResult {ok:boolean;url?:string;error_code?:string;}
@Entry
@Component
struct Index {
 private controller:webview.WebviewController=new webview.WebviewController();
 private context:common.UIAbilityContext=getContext(this) as common.UIAbilityContext;
 private bridge:ReadMDBridge=new ReadMDBridge(this.context,this.controller);
 @State private url:string='';
 @State private failed:boolean=false;
 private exists(path:string):boolean{try{return fs.accessSync(path);}catch{return false;}}
 private directory(path:string):void{if(!this.exists(path))fs.mkdirSync(path,true);}
 async aboutToAppear():Promise<void> {
  try{
   const resource=this.context.resourceManager;
   const bytes=await resource.getRawFileContent('kernel-manifest.json');
   const manifest:KernelManifest=JSON.parse(this.bridge.decode(bytes)) as KernelManifest;
   const assets=this.context.filesDir+'/readmd-assets/'+manifest.version;
   this.directory(assets);
   for(const relative of manifest.files){
    if(relative.includes('..')||relative.startsWith('/'))throw new Error('invalid_asset');
    const dest=assets+'/'+relative;
    if(this.exists(dest))continue;
    this.directory(dest.substring(0,dest.lastIndexOf('/')));
    const contents=await resource.getRawFileContent('kernel-assets/'+relative);
    const file=fs.openSync(dest,fs.OpenMode.CREATE|fs.OpenMode.WRITE_ONLY|fs.OpenMode.TRUNC);
    try{fs.writeSync(file.fd,contents.buffer);}finally{fs.closeSync(file);}
   }
   const result:KernelResult=JSON.parse(kernel.start(this.context.filesDir+'/readmd',assets)) as KernelResult;
   if(!result.ok||!result.url)throw new Error('kernel_unavailable');
   this.bridge.origin=result.url;this.url=result.url;
  }catch{this.failed=true;}
 }
 aboutToDisappear():void{kernel.stop();}
 build(){
  Column(){
   if(this.failed){Text('ReadMD 内核启动失败 / Kernel unavailable').padding(24);}
   else if(this.url===''){LoadingProgress().width(32).height(32);}
   else{
    Web({src:this.url,controller:this.controller})
     .javaScriptAccess(true).domStorageAccess(true).fileAccess(false).onlineImageAccess(false)
     .javaScriptProxy({object:this.bridge,name:'ReadMDHarmony',methodList:['call'],controller:this.controller})
     .onPageEnd(()=>{this.controller.runJavaScript('window.installReadMDHarmony && window.installReadMDHarmony()');})
     .onLoadIntercept(event=>{const u=event?.data.getRequestUrl()||'';return !u.startsWith(this.url)&&u!=='about:blank';})
     .width('100%').height('100%')
   }
  }.width('100%').height('100%').justifyContent(FlexAlign.Center)
 }
}
