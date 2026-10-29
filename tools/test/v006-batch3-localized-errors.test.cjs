'use strict';
const test=require('node:test'),assert=require('node:assert/strict'),fs=require('node:fs'),path=require('node:path');
const root=path.resolve(__dirname,'../..'),dir=path.join(root,'assets/i18n');
test('stale OpenAPI and export messages use actual text in every supported locale',()=>{
  const names=fs.readdirSync(dir).filter(name=>name.endsWith('.json')&&name!=='meta.json');assert.equal(names.length,46);
  const english=JSON.parse(fs.readFileSync(path.join(dir,'en.json'),'utf8'));
  for(const name of names){const data=JSON.parse(fs.readFileSync(path.join(dir,name),'utf8'));
    for(const key of ['error.review_stale_generation','export.inputsChanged','toast.unknownError']){
      assert.ok(data[key]&&data[key]!==key,`${name}: ${key}`);
      if(name!=='en.json')assert.notEqual(data[key],english[key],`${name}: ${key}`);
    }
  }
  const source=fs.readFileSync(path.join(root,'assets/js/features/openapi.js'),'utf8');
  assert.match(source,/error\.review_stale_generation/);
  const exportSource=fs.readFileSync(path.join(root,'assets/js/features/export.js'),'utf8');
  assert.ok(!exportSource.includes('error.transaction_conflict'));
});
