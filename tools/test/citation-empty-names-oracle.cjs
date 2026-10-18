'use strict';
// Independent processor, original synthetic fixtures only. The privately
// pinned citeproc runtime is a QA tool and is never bundled with ReadMD.
const fs=require('node:fs'),path=require('node:path'),crypto=require('node:crypto'),assert=require('node:assert/strict');
const root=path.resolve(__dirname,'../..'),sha=value=>crypto.createHash('sha256').update(value).digest('hex');
const engineFile=path.join(root,'.cache/private/citeproc-js/citeproc_commonjs.cc9153c45293af878de08cafddbefe6ea150c380.root-second.js');
const localeFile=path.join(root,'.cache/private/publication-context-inputs/locale-history/locales-en-US.195bef843f87fc917be45ab3eca60536e4ef4626.xml');
assert.equal(sha(fs.readFileSync(engineFile)),'2c659319b3b133c0204d60eba3e5daa566e1457e36b19cb6bfe4c8b1b4662650');
assert.equal(sha(fs.readFileSync(localeFile)),'55555f07f931a4b53af2f7eda33284e0f2c4063a6d3f4d930fb1bff9d417770e');
const CSL=require(engineFile),locale=fs.readFileSync(localeFile,'utf8');
const names='<names variable="editor"><name form="short"/></names>';
const cases=[
  {id:'missing',names,expected:'Atlas'},
  {id:'present',names,editor:[{given:'Mira',family:'Reed'}],expected:'Atlas, in Reed'},
  {id:'empty-substitute',names:'<names variable="editor"><name form="short"/><substitute><text variable="publisher"/></substitute></names>',expected:'Atlas'},
  {id:'literal-substitute',names:'<names variable="editor"><name form="short"/><substitute><text value="Anonymous"/></substitute></names>',expected:'Atlas'},
  {id:'variable-substitute',names:'<names variable="editor"><name form="short"/><substitute><text variable="publisher"/></substitute></names>',publisher:'Orchid Press',expected:'Atlas, in Orchid Press'},
  {id:'nested-empty',names:'<group>'+names+'</group>',expected:'Atlas'},
  {id:'nested-constant',names:names+'<group><text value="literal"/></group>',expected:'Atlas, in literal'},
  {id:'term-only',names:'<text value="constant"/>',expected:'Atlas, in constant'},
];
const output=[];
for(const c of cases){
 const item={id:'atlas',type:'chapter',title:'Atlas',...(c.editor?{editor:c.editor}:{}),...(c.publisher?{publisher:c.publisher}:{})};
 const csl='<style xmlns="http://purl.org/net/xbiblio/csl" version="1.0" class="note"><info><title>ReadMD original empty-name regression</title><id>https://readmd.test/empty-names</id></info><macro name="container"><group delimiter=" "><text term="in"/>'+c.names+'</group></macro><citation><layout><group delimiter=", "><text variable="title"/><text macro="container"/></group></layout></citation><bibliography><layout><text variable="title"/></layout></bibliography></style>';
 const processor=new CSL.Engine({retrieveItem:id=>{assert.equal(id,'atlas');return structuredClone(item);},retrieveLocale:lang=>{assert(['en','en-US'].includes(lang));return locale;}},csl,'en-US');
 processor.setOutputFormat('text');processor.updateItems(['atlas']);
 const actual=processor.processCitationCluster({citationID:'case-'+c.id,citationItems:[{id:'atlas'}],properties:{noteIndex:1}},[],[])[1][0][1];
 assert.equal(actual,c.expected,c.id);
 output.push({id:c.id,csl,items:[item],expected:c.expected});
}
const destination=path.resolve(process.argv[2]||path.join(root,'.cache/capability-upgrade/citation-empty-names-oracle-first.json'));
fs.writeFileSync(destination,JSON.stringify({schema:1,synthetic:true,independentEngineSha256:sha(fs.readFileSync(engineFile)),localeSha256:sha(locale),cases:output},null,2)+'\n',{flag:'wx'});
console.log(JSON.stringify({passed:output.length,report:path.relative(root,destination).replaceAll(path.sep,'/'),sha256:sha(fs.readFileSync(destination))}));
