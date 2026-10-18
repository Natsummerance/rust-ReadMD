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
