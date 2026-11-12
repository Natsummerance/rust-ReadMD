const {test,expect}=require('@playwright/test');
const {waitForPage}=require('./page-condition.cjs');
test.use({bypassCSP:false});
test.beforeEach(async({page})=>{
  await page.addInitScript(()=>localStorage.setItem('readmd_language','zh-CN'));
  await page.goto('/');await waitForPage(page,()=>window.__readmdAppReady);
  await page.evaluate(()=>window.__readmdPreferencesReady);
});

test('late startup dictionary cannot overwrite an explicit language choice',async({page})=>{
  const result=await page.evaluate(async()=>{
    const original=i18n.fetchDict;let release;
    const gate=new Promise(resolve=>release=resolve);
    i18n.fetchDict=async function(code){if(code==='zh-CN')await gate;return original.call(this,code);};
    const startup=i18n.init();await i18n.setLanguage('en',false);release();await startup;
    i18n.fetchDict=original;
    return {language:i18n.currentLang,html:document.documentElement.lang,apply:i18n.t('formula.apply')};
  });
  expect(result).toEqual({language:'en',html:'en',apply:'Apply equation'});
});

test('the latest language wins when real dictionary responses arrive out of order',async({page})=>{
  const result=await page.evaluate(async()=>{
    const original=i18n.fetchDict;let release;
    const gate=new Promise(resolve=>release=resolve);
    i18n.fetchDict=async function(code){if(code==='en')await gate;return original.call(this,code);};
    const first=i18n.setLanguage('en');await i18n.setLanguage('ar');release();const obsolete=await first;
    i18n.fetchDict=original;
    return {obsolete,language:i18n.currentLang,html:document.documentElement.lang,dir:document.documentElement.dir,saved:localStorage.getItem('readmd_language')};
  });
  expect(result).toEqual({obsolete:false,language:'ar',html:'ar',dir:'rtl',saved:'ar'});
});

test('a missing dictionary leaves the selected language and icon markup intact',async({page})=>{
  const result=await page.evaluate(async()=>{
    await i18n.setLanguage('ar');
    const original=i18n.fetchDict;
    i18n.fetchDict=async code=>code==='zh-CN'?null:original.call(i18n,code);
    const button=document.createElement('button');
    button.innerHTML='<svg aria-hidden="true"></svg>';
    const icon=button.firstElementChild;
    button.dataset.i18n='toolbar.open';
    button.dataset.i18nTitle='test.missing';
    button.title='Open';
    document.body.appendChild(button);
    const changed=await i18n.setLanguage('zh-CN');
    i18n.translateDOM();
    const name='$& / {name}';
    const outcome={changed,language:document.documentElement.lang,dir:document.documentElement.dir,
      saved:localStorage.getItem('readmd_language'),icon:icon===button.firstElementChild,
      title:button.title,text:button.textContent,open:i18n.t('toolbar.open'),
      literal:i18n.t('lang.autoDetected',{name}).includes(name),taiwan:i18n.matchLanguage('zh-Hant-TW')};
    button.remove();
    i18n.fetchDict=original;
    return outcome;
  });
  expect(result).toEqual({changed:false,language:'ar',dir:'rtl',saved:'ar',icon:true,title:'Open',
    text:result.open,open:result.open,literal:true,taiwan:'zh-TW'});
  expect(result.text).not.toBe('toolbar.open');
});
