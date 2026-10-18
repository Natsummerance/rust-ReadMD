const {waitForPage}=require('./page-condition.cjs');
const {test,expect}=require('@playwright/test');
const http=require('node:http');
const answer='核对原始证据，再比较不同解释。';
const protocols=[
  {name:'chat',mode:'chat',format:'openai',endpoint:'/v1/chat/completions',json:{choices:[{message:{content:answer}}]},frames:[{choices:[{delta:{content:answer}}]}]},
  {name:'completion',mode:'completion',format:'openai',endpoint:'/v1/completions',json:{choices:[{text:answer}]},frames:[{choices:[{text:answer}]}]},
  {name:'responses',mode:'responses',format:'openai',endpoint:'/v1/responses',json:{output:[{type:'message',content:[{type:'output_text',text:answer}]}]},frames:[{type:'response.output_text.delta',delta:answer},{type:'response.completed',response:{usage:{total_tokens:12}}}]},
  {name:'anthropic',mode:'messages',format:'anthropic',endpoint:'/v1/messages',json:{content:[{type:'text',text:answer}],usage:{input_tokens:4,output_tokens:8}},frames:[{type:'message_start',message:{usage:{input_tokens:4}}},{type:'content_block_delta',delta:{type:'text_delta',text:answer}},{type:'message_stop'}]},
];
for(const protocol of protocols)for(const jsonReply of [false,true])test(`${protocol.name}: streaming request accepts ${jsonReply?'JSON fallback':'standard SSE'} without a second request`,async({page})=>{
  const requests=[];
  const server=http.createServer((req,res)=>{
    let body='';req.on('data',chunk=>body+=chunk);req.on('end',()=>{
      requests.push({path:req.url,body:JSON.parse(body)});
      res.writeHead(200,{'Content-Type':jsonReply?'application/json; charset=utf-8':'text/event-stream'});
      if(jsonReply)res.end(JSON.stringify(protocol.json));
      else res.end(protocol.frames.map(frame=>'data: '+JSON.stringify(frame)+'\n\n').join('')+'data: [DONE]\n\n');
    });
  });
  await new Promise(resolve=>server.listen(0,'127.0.0.1',resolve));
  try{
    await page.goto('/');await waitForPage(page, ()=>window.__readmdAppReady);
    const setup=await page.evaluate(async({port,protocol})=>{
      const response=await apiFetch('/api/ai/config',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({
        providers:[{id:'custom:protocol-fixture',name:'协议回归',base_url:'http://127.0.0.1:'+port+'/v1',category:'local',api_key:'synthetic-fixture-only',format:protocol.format,mode:protocol.mode,models:['protocol-fixture'],autocomplete:{mode:'current'}}],
        current:{provider_id:'custom:protocol-fixture',model:'protocol-fixture'}})});
      return response.status;
    },{port:server.address().port,protocol});
    expect(setup).toBe(200);
    const result=await page.evaluate(async()=>{
      const response=await apiFetch('/api/ai/chat',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({stream:true,model:'protocol-fixture',messages:[{role:'user',content:'继续写一句。'}]})});
      return {status:response.status,body:await response.json()};
    });
    expect(result.status).toBe(200);expect(result.body.content).toBe(answer);expect(requests).toHaveLength(1);
    const completion=await page.evaluate(async()=>{
      const response=await apiFetch('/api/ai/complete',{method:'POST',headers:{'Content-Type':'application/json',Accept:'text/event-stream'},body:JSON.stringify({model:'protocol-fixture',prefix:'# 研究札记\n\n我们应该',suffix:'\n\n## 结论'})});
      const text=await response.text();
      return {status:response.status,frames:text.split('\n').filter(line=>line.startsWith('data:')).map(line=>JSON.parse(line.slice(5)))};
    });
    expect(completion.status).toBe(200);expect(completion.frames.at(-1)).toMatchObject({type:'done',ok:true,completion:answer});
    expect(requests).toHaveLength(2);
    for(const request of requests){expect(request.path).toBe(protocol.endpoint);expect(request.body.stream).toBe(true);}
  }finally{server.closeAllConnections();await new Promise(resolve=>server.close(resolve));}
});
