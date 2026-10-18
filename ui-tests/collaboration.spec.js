const {waitForPage}=require('./page-condition.cjs');
const {test,expect}=require('@playwright/test');
test.use({bypassCSP:false});
test('real Yjs/Yrs clients sync offline edits, personal undo, roles and durable reviews',async({page,browser})=>{
  await page.goto('/');await waitForPage(page, ()=>window.__readmdAppReady);
  const api=async(operation,body,credential)=>page.evaluate(async({operation,body,credential})=>{
    const r=await apiFetch(operation==='create'?'/api/collaboration/create':'/api/collaboration/session',{method:'POST',headers:{'Content-Type':'application/json',...(credential?{'X-ReadMD-Collaboration-Token':credential.token}:{})},body:JSON.stringify({...body,...(credential?{room:credential.room,operation}:{})})});return{status:r.status,data:await r.json()};
  },{operation,body,credential});
  const created=await api('create',{title:'Research.md',content:'# Evidence\n\nOriginal 🦀\n',name:'Owner'});expect(created.status).toBe(200);
  const owner=created.data.credential;
  const editor=(await api('invite',{role:'editor',name:'Researcher'},owner)).data.credential;
  const reader=(await api('invite',{role:'reader',name:'Reader'},owner)).data.credential;
  const reviewer=(await api('invite',{role:'reviewer',name:'Reviewer'},owner)).data.credential;
  const contexts=[];
  async function client(credential){const context=await browser.newContext({bypassCSP:false});contexts.push(context);const p=await context.newPage();
    await p.goto('http://127.0.0.1:'+created.data.port+'/#'+new URLSearchParams({room:credential.room,token:credential.token,lang:'en'}));
    await expect.poll(()=>p.evaluate(()=>!!window.ReadMDCollaboration)).toBe(true);await expect(p.locator('#editor')).toHaveValue('# Evidence\n\nOriginal 🦀\n');return p;}
  try{
    const host=await client(owner),guest=await client(editor),viewer=await client(reader),review=await client(reviewer);
    await expect(host.locator('#editor')).toBeEnabled();await expect(viewer.locator('#editor')).toHaveJSProperty('readOnly',true);
    await host.locator('#editor').press('Control+End');await host.locator('#editor').pressSequentially('Host line\n');
    await expect(guest.locator('#editor')).toHaveValue('# Evidence\n\nOriginal 🦀\nHost line\n');
    await guest.locator('#editor').press('Control+End');await guest.locator('#editor').pressSequentially('Guest line\n');
    await expect(host.locator('#editor')).toHaveValue('# Evidence\n\nOriginal 🦀\nHost line\nGuest line\n');
    await host.locator('#editor').press('Control+z');
    await expect(guest.locator('#editor')).toHaveValue('# Evidence\n\nOriginal 🦀\nGuest line\n');
    await guest.context().setOffline(true);await guest.locator('#editor').press('Control+End');await guest.locator('#editor').pressSequentially('Offline edit\n');
    await expect.poll(()=>guest.evaluate(()=>ReadMDCollaboration.pending())).toBeGreaterThan(0);await guest.evaluate(()=>ReadMDCollaboration.checkpoint());
    await guest.context().setOffline(false);await guest.evaluate(()=>ReadMDCollaboration.sync());
    await expect(host.locator('#editor')).toHaveValue('# Evidence\n\nOriginal 🦀\nGuest line\nOffline edit\n');
    await host.locator('aside details').first().locator('summary').first().click();await host.locator('#comment').fill('Check the sources.');await host.locator('#post').click();
    await expect(host.locator('#reviews')).toContainText('Check the sources.');
    await review.locator('aside details').first().locator('summary').first().click();await review.locator('#suggestion > summary').click();
    await review.locator('#editor').evaluate(el=>{const at=el.value.indexOf('Original');el.setSelectionRange(at,at+8);});await review.locator('#replacement').fill('Revised');await review.locator('#propose').click();
    await expect(review.locator('#reviews ins')).toHaveText('Revised');
    // A change elsewhere moves the relative suggestion without invalidating it.
    await host.locator('#editor').press('Control+Home');await host.locator('#editor').pressSequentially('Intro\n');
    const proposal=host.locator('#reviews article').filter({has:host.locator('ins')});await proposal.getByRole('button',{name:'Accept',exact:true}).click();
    await expect(viewer.locator('#editor')).toHaveValue('Intro\n# Evidence\n\nRevised 🦀\nGuest line\nOffline edit\n');
    await expect(host.locator('.remote-cursor')).not.toHaveCount(0);
    await host.locator('#files > summary').click();await viewer.locator('#files > summary').click();
    const attachment=Buffer.from('<svg xmlns="http://www.w3.org/2000/svg"><text>Evidence</text></svg>');
    await host.locator('#upload').setInputFiles({name:'专用图.svg',mimeType:'image/svg+xml',buffer:attachment});
    await expect(host.locator('#attachments')).toContainText('专用图.svg');await expect(viewer.locator('#attachments')).toContainText('专用图.svg');
    const downloadPromise=viewer.waitForEvent('download');await viewer.locator('#attachments button').click();const download=await downloadPromise;expect(download.suggestedFilename()).toBe('专用图.svg');expect(require('node:fs').readFileSync(await download.path())).toEqual(attachment);
    const snapshot=await api('snapshot',{},owner);expect(snapshot.data.content).toContain('Offline edit');
    expect((await api('permission',{member:editor.member,role:null},owner)).status).toBe(200);
    await guest.evaluate(()=>ReadMDCollaboration.sync());await expect(guest.locator('#editor')).toBeDisabled();
    expect((await api('snapshot',{},editor)).status).toBe(403);
    // The dedicated listener cannot read host files or use the desktop routes.
    expect((await viewer.request.get('http://127.0.0.1:'+created.data.port+'/api/file')).status()).toBe(404);
    expect((await viewer.request.get('http://127.0.0.1:'+created.data.port+'/../Cargo.toml')).status()).toBe(404);
  }finally{await api('close',{},owner);for(const c of contexts)await c.close();}
});

test('large offline input persists as bounded deltas and resumes after a browser reload',async({page,browser})=>{
  test.setTimeout(90000);await page.goto('/');await waitForPage(page, ()=>window.__readmdAppReady);
  const created=await page.evaluate(async()=>{const r=await apiFetch('/api/collaboration/create',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({title:'Long manuscript.md',content:'# Manuscript\n',name:'Author'})});return r.json();});
  const context=await browser.newContext({bypassCSP:false}),client=await context.newPage(),credential=created.credential;
  const url='http://127.0.0.1:'+created.port+'/#'+new URLSearchParams({room:credential.room,token:credential.token,lang:'en'});
  try{
    await client.goto(url);await expect.poll(()=>client.evaluate(()=>!!window.ReadMDCollaboration)).toBe(true);await expect(client.locator('#editor')).toHaveValue('# Manuscript\n');await expect(client.locator('#editor')).toBeEditable();
    await context.setOffline(true);const material='# Manuscript\n\n'+('A carefully researched paragraph with original citations and evidence.\n'.repeat(18000));
    // Exercise the actual input handler and persistence with a >1 MiB input.
    // This does not certify OS clipboard insertion performance: Chromium's CDP
    // Input.insertText itself stalls on one message of this size on this host.
    const input=await client.evaluate(material=>{const el=document.getElementById('editor'),start=performance.now();el.value=material;el.dispatchEvent(new InputEvent('input',{bubbles:true,inputType:'insertFromPaste'}));return{elapsed:performance.now()-start};},material);expect(input.elapsed).toBeLessThan(2000);await client.evaluate(()=>ReadMDCollaboration.checkpoint(true));
    await expect.poll(()=>client.evaluate(()=>ReadMDCollaboration.pending())).toBeGreaterThan(10);
    // Reload needs the client shell, so restore connectivity while suppressing
    // session requests until IndexedDB recovery has been inspected.
    await context.setOffline(false);await client.route('**/sessions/**',route=>route.abort());await client.reload();
    await expect.poll(()=>client.evaluate(()=>!!window.ReadMDCollaboration)).toBe(true);expect(await client.locator('#editor').inputValue()).toBe(material);
    await client.unroute('**/sessions/**');await expect.poll(()=>client.evaluate(async()=>{await ReadMDCollaboration.sync();return ReadMDCollaboration.pending();}),{timeout:30000}).toBe(0);
    const snapshot=await page.evaluate(async c=>{const r=await apiFetch('/api/collaboration/session',{method:'POST',headers:{'Content-Type':'application/json','X-ReadMD-Collaboration-Token':c.token},body:JSON.stringify({room:c.room,operation:'snapshot'})});return r.json();},credential);expect(snapshot.content).toBe(material);
    await client.reload();await expect.poll(()=>client.evaluate(()=>!!window.ReadMDCollaboration)).toBe(true);expect(await client.locator('#editor').inputValue()).toBe(material);expect(await client.evaluate(()=>ReadMDCollaboration.pending())).toBe(0);
  }finally{await context.close().catch(()=>{});if(!page.isClosed())await page.evaluate(async c=>apiFetch('/api/collaboration/session',{method:'POST',headers:{'Content-Type':'application/json','X-ReadMD-Collaboration-Token':c.token},body:JSON.stringify({room:c.room,operation:'close'})}),credential);}
});
