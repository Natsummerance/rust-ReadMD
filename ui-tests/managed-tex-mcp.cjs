'use strict';
// Actual stdio transport and the same native export service as the desktop.
const {spawn}=require('node:child_process'),assert=require('node:assert/strict'),path=require('node:path');
module.exports=async function exportWithMcp(binary,data,assets,workspace,output){
  const child=spawn(binary,['--mcp','--data-dir',data,'--workspace',workspace,'--assets',assets],{
    windowsHide:true,stdio:['pipe','pipe','ignore'],env:{...process.env,PATH:path.join(process.env.WINDIR,'System32')}
  });
  let next=1,buffer='';const pending=new Map();
  child.stdout.setEncoding('utf8');child.stdout.on('data',chunk=>{
    buffer+=chunk;if(buffer.length>2*1024*1024){for(const call of pending.values())call.reject(Error('MCP output budget'));child.kill();return;}
    for(;;){const at=buffer.indexOf('\n');if(at<0)break;const line=buffer.slice(0,at);buffer=buffer.slice(at+1);if(!line.trim())continue;
      const message=JSON.parse(line),call=pending.get(message.id);if(call){clearTimeout(call.timer);pending.delete(message.id);message.error?call.reject(Error('MCP protocol error')):call.resolve(message.result);}
    }
  });
  child.on('error',()=>{for(const call of pending.values())call.reject(Error('MCP launch failed'));});
  function request(method,params){return new Promise((resolve,reject)=>{const id=next++,timer=setTimeout(()=>{pending.delete(id);reject(Error('MCP timeout'));},120000);pending.set(id,{resolve,reject,timer});child.stdin.write(JSON.stringify({jsonrpc:'2.0',id,method,params})+'\n');});}
  try{
    await request('initialize',{protocolVersion:'2024-11-05',capabilities:{},clientInfo:{name:'readmd-managed-tex-qa',version:'1'}});
    child.stdin.write(JSON.stringify({jsonrpc:'2.0',method:'notifications/initialized'})+'\n');
    const listed=await request('tools/list',{});assert(listed.tools.some(t=>t.name==='readmd_export_document'));
    const result=await request('tools/call',{name:'readmd_export_document',arguments:{
      markdown_content:'# Offline publication\n\nReadMD portable output. A formula $x^2$.\n',
      output_path:output,output_format:'pdf',base_dir:workspace,title:'Offline publication',
      options:{pdf:{engine:'xelatex'},tex:{docClass:'article',useCtex:false}},confirm:true
    }});
    assert(!result.isError);const value=result.structuredContent||JSON.parse(result.content[0].text);
    assert.equal(value.ok,true,JSON.stringify(value));return {ok:true,transport:'stdio',engine:'xelatex',sameManagedService:true,system32OnlyPath:true};
  }finally{for(const call of pending.values())clearTimeout(call.timer);child.stdin.end();if(child.exitCode===null)child.kill();}
};
