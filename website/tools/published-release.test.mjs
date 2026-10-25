import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';
import { packageNames, publishedRelease, releaseLink, updateReleaseHtml, featureStatusText } from '../public/assets/release-contract.mjs';
const snapshot = JSON.parse(fs.readFileSync(new URL('../published-release.json', import.meta.url), 'utf8'));
const fixture = (tag='V0.0.5') => ({tag_name:tag, draft:false, prerelease:false, published_at:'2026-10-08T12:00:00Z',
  assets:packageNames(tag.slice(1)).map(name=>({name,size:1024,state:'uploaded',browser_download_url:`https://github.com/Natsummerance/rust-ReadMD/releases/download/${tag}/${name}`}))});
test('exact case and every uploaded asset survive manifest round trip',()=>{
  for(const tag of ['V0.0.5','v0.0.5']){const r=publishedRelease(fixture(tag));assert.equal(r.releaseTag,tag);assert.deepEqual(publishedRelease(r),r);assert.equal(r.assets.length,11);}
});
test('draft, prerelease, incomplete, zero length, wrong owner and malformed version are rejected',()=>{
  for(const mutate of [r=>r.draft=true,r=>r.prerelease=true,r=>r.assets.pop(),r=>r.assets[0].size=0,
    r=>r.assets[0].state='new',r=>r.assets[0].browser_download_url='https://example.com/file.exe',r=>r.tag_name='V0.0.5-rc1',r=>r.version='0.0.6']){
    const r=fixture();mutate(r);assert.throws(()=>publishedRelease(r));
  }
});
test('all old downloads move to actual current packages, including versioned integrations',()=>{
  const r=publishedRelease(fixture());
  for(const name of packageNames('2.3.9')) {
    const result=releaseLink('https://github.com/Natsummerance/readMD/releases/download/v2.3.9/'+name,r);
    assert.ok(r.assets.some(a=>a.url===result),result);
  }
  assert.equal(releaseLink('https://github.com/Natsummerance/readMD/releases/download/v2.3.9/ReadMD.exe',r),r.assetsBaseUrl+'ReadMD-windows-x64.zip');
  assert.equal(releaseLink('https://github.com/Natsummerance/readMD/releases/download/v2.3.9/ReadMD-linux-arm64.AppImage',r),'https://github.com/Natsummerance/rust-ReadMD/releases/tag/V0.0.5');
  assert.equal(releaseLink('https://example.org/private',r),'https://example.org/private');
});
test('offline snapshot describes the currently published stable version, not the working VERSION',()=>{
  const config=JSON.parse(fs.readFileSync(new URL('../release.json',import.meta.url),'utf8'));
  assert.equal(publishedRelease(snapshot).version,config.stable);
});
for(const lang of ['','zh-cn/','zh-tw/','ja/']) test('localized download markup and styles survive updates: '+(lang||'en'),()=>{
  const source=fs.readFileSync(new URL('../public/'+lang+'download/index.html',import.meta.url),'utf8');
  const r=publishedRelease(fixture());const out=updateReleaseHtml(source,r,{download:true});
  assert.match(out,/<title>[^<]*V0\.0\.5/);assert.match(out,/"softwareVersion": "0\.0\.5"/);
  assert.match(out,/2026-10-08<\/time>/);assert.doesNotMatch(out,/V0\.0\.4/);
  assert.deepEqual([...out.matchAll(/class="([^"]+)"/g)].map(m=>m[1]),[...source.matchAll(/class="([^"]+)"/g)].map(m=>m[1]));
  const urls=[...out.matchAll(/href="(https:\/\/github.com\/[^\"]+\/releases\/download\/[^\"]+)"/g)].map(m=>m[1]);
  assert.deepEqual(new Set(urls),new Set(r.assets.map(a=>a.url)));
  assert.equal(updateReleaseHtml(out,r,{download:true}),out);
});
test('historical release prose is preserved while old binary links are upgraded',()=>{
  const html='<html><body>v2.3.9 <a href="https://github.com/Natsummerance/readMD/releases/download/v2.3.9/ReadMD.exe">Old</a></body></html>';
  const out=updateReleaseHtml(html,publishedRelease(fixture()),{historical:true});
  assert.match(out,/v2\.3\.9/);assert.match(out,/download\/V0\.0\.5\/ReadMD-windows-x64.zip/);
});
test('stale visible title is repaired even when structured metadata was already updated',()=>{
  const html='<html><head><title>Download ReadMD | v2.3.9</title><script type="application/ld+json">{"@type":"SoftwareApplication","softwareVersion":"0.0.4"}</script></head><body>Stable · v2.3.9</body></html>';
  const out=updateReleaseHtml(html,publishedRelease(fixture()),{download:true});
  assert.match(out,/<title>Download ReadMD \| V0\.0\.5<\/title>/);assert.doesNotMatch(out,/2\.3\.9/);
});
test('candidate pages keep their candidate title while downloads use the stable release',()=>{
  const html='<html><head><title>V0.0.5 AI autocomplete</title></head><body>V0.0.5 candidate <a href="https://github.com/Natsummerance/rust-ReadMD/releases/download/v2.3.9/ReadMD.exe">Stable</a></body></html>';
  const out=updateReleaseHtml(html,publishedRelease(snapshot),{candidate:true});
  assert.match(out,/<title>V0\.0\.5 AI autocomplete/);assert.match(out,/V0\.0\.5 candidate/);
  assert.ok(out.includes(snapshot.assetsBaseUrl+'ReadMD-windows-x64.zip'));
});

const featureLabels = [
  { lang: 'en', prefix: '', pending: 'Release candidate · not yet published', available: 'Available in the stable release' },
  { lang: 'zh-CN', prefix: 'zh-cn/', pending: '发布候选 · 尚未正式发布', available: '已在正式版提供' },
  { lang: 'zh-TW', htmlLang: 'zh-Hant-TW', prefix: 'zh-tw/', pending: '發布候選 · 尚未正式發布', available: '已於正式版提供' },
  { lang: 'ja', prefix: 'ja/', pending: 'リリース候補 · 未公開', available: '安定版で利用できます' },
];
const featureHtml = (copy, version = '0.0.5') => `<html lang="${copy.htmlLang || copy.lang}"><head><title>ReadMD V0.0.4</title></head><body><section id="feature"><p>V${version} · ${copy.pending}</p><h2>V${version} AI autocomplete</h2><button data-ai-action="accept">Tab</button></section><p id="unrelated">Keep these instructions.</p><p>${copy.pending}</p></body></html>`;

for (const copy of featureLabels) test('explicit feature publication status follows complete stable releases: '+copy.lang, () => {
  const before = publishedRelease(fixture('V0.0.4')), current = publishedRelease(fixture()), later = publishedRelease(fixture('V0.0.6'));
  const source = featureHtml(copy);
  const pending = updateReleaseHtml(source, before);
  assert.ok(pending.includes('V0.0.5 · '+copy.pending));
  const released = updateReleaseHtml(source, current);
  assert.ok(released.includes('V0.0.5 · '+copy.available));
  assert.ok(released.includes('<h2>V0.0.5 AI autocomplete</h2>'));
  assert.ok(released.includes('<p id="unrelated">Keep these instructions.</p>'));
  assert.ok(released.includes('<p>'+copy.pending+'</p>'), 'a status without a feature version is ordinary prose');
  assert.equal(updateReleaseHtml(released, current), released);
  const upgraded = updateReleaseHtml(released, later);
  assert.ok(upgraded.includes('V0.0.5 · '+copy.available));
  assert.ok(upgraded.includes('<h2>V0.0.5 AI autocomplete</h2>'));
  assert.ok(upgraded.includes('<title>ReadMD V0.0.6</title>'));
  const future = updateReleaseHtml(featureHtml(copy, '0.0.6'), current);
  assert.ok(future.includes('V0.0.6 · '+copy.pending));
  assert.ok(updateReleaseHtml(future, later).includes('V0.0.6 · '+copy.available));
  const historical = updateReleaseHtml(source, later, { historical: true });
  assert.ok(historical.includes('V0.0.5 · '+copy.pending));
  assert.ok(historical.includes('<title>ReadMD V0.0.4</title>'));
  for (const slug of ['', 'integrations/']) {
    const actual = fs.readFileSync(new URL('../public/'+copy.prefix+slug+'index.html', import.meta.url), 'utf8');
    const result = updateReleaseHtml(actual, current, { candidate: !!slug });
    assert.ok(!result.includes(copy.pending), slug || 'homepage');
    assert.ok(result.includes(copy.available), slug || 'homepage');
    assert.equal(updateReleaseHtml(result, current, { candidate: !!slug }), result);
  }
});

test('feature versions use numeric comparison and malformed releases cannot change build output', () => {
  assert.equal(featureStatusText('V0.0.9 · Release candidate · not published yet', publishedRelease(fixture('V0.0.10'))), 'V0.0.9 · Available in the stable release');
  assert.equal(featureStatusText('V0.1.0 · Release candidate · not published yet', publishedRelease(fixture('V0.0.99'))), 'V0.1.0 · Release candidate · not published yet');
  const incomplete = fixture(); incomplete.assets.pop();
  assert.throws(() => updateReleaseHtml(featureHtml(featureLabels[0]), incomplete), /Missing or invalid published package/);
});

// Execute the real online apply body with a small DOM. No browser, requests or
// innerHTML parsing is involved; existing widget objects retain their identity.
function onlineFixture(copy, { version = '0.0.5', pathname = '/' } = {}) {
  const element = (tagName, text, parentElement = null) => {
    const node = { nodeType: 3, nodeValue: text, parentElement: null };
    const item = { tagName, parentElement, childNodes: [node], firstChild: node, dataset: {},
      get textContent() { return node.nodeValue; }, set textContent(value) { node.nodeValue = value; },
      closest(selector) { const names = selector.split(',').map(name => name.toUpperCase()); for (let p = this; p; p = p.parentElement) if (names.includes(p.tagName)) return p; return null; },
    }; node.parentElement = item; return item;
  };
  const section = element('SECTION', ''), label = element('P', `V${version} · ${copy.pending}`, section);
  const heading = element('H2', `V${version} AI autocomplete`, section), widget = element('BUTTON', 'Tab', section);
  const listener = () => 'still attached'; widget.listener = listener;
  const unrelated = element('P', 'Keep these instructions.'), unversioned = element('P', copy.pending);
  const link = { href: 'https://github.com/Natsummerance/rust-ReadMD/releases/download/V0.0.4/ReadMD-windows-x64.zip', dataset: {} };
  const metadata = element('SCRIPT', JSON.stringify({ '@type': 'SoftwareApplication', softwareVersion: '0.0.4' }));
  const bodyNodes = [label, heading, widget, unrelated, unversioned].map(item => item.firstChild);
  const document = {
    title: 'ReadMD V0.0.4', documentElement: { lang: copy.htmlLang || copy.lang, dataset: {} }, body: {},
    querySelectorAll(selector) {
      if (selector === 'p,span') return [label, unrelated, unversioned];
      if (selector === 'script[type="application/ld+json"]') return [metadata];
      if (selector === 'a[href]') return [link];
      return [];
    },
    createTreeWalker() { let index = -1; return { nextNode() { index++; this.currentNode = bodyNodes[index]; return index < bodyNodes.length; } }; },
    dispatchEvent() {},
  };
  const source = fs.readFileSync(new URL('../public/assets/release.js', import.meta.url), 'utf8');
  assert.doesNotMatch(source, /\.innerHTML\s*=/);
  const body = source.slice(source.indexOf('let applied;'), source.indexOf('async function refresh()'));
  const context = { document, location: { pathname }, publishedRelease, releaseLink, featureStatusText,
    Node: { TEXT_NODE: 3 }, NodeFilter: { SHOW_TEXT: 4 }, CustomEvent: class { constructor(type, data) { this.type = type; this.detail = data.detail; } } };
  vm.runInNewContext(body+'\nglobalThis.applyRelease = apply;', context, { timeout: 1000 });
  return { apply: context.applyRelease, document, label, heading, widget, listener, unrelated, unversioned, link, metadata };
}

for (const copy of featureLabels) test('online status update preserves interactive nodes and feature versions: '+copy.lang, () => {
  const page = onlineFixture(copy);
  page.apply(fixture('V0.0.4')); assert.equal(page.label.textContent, 'V0.0.5 · '+copy.pending);
  const incomplete = fixture(); incomplete.assets.pop();
  assert.throws(() => page.apply(incomplete)); assert.equal(page.label.textContent, 'V0.0.5 · '+copy.pending);
  page.apply(fixture()); assert.equal(page.label.textContent, 'V0.0.5 · '+copy.available);
  const current = page.label.textContent; page.apply(fixture()); assert.equal(page.label.textContent, current);
  page.apply(fixture('V0.0.6'));
  assert.equal(page.label.textContent, 'V0.0.5 · '+copy.available);
  assert.equal(page.heading.textContent, 'V0.0.5 AI autocomplete');
  assert.equal(page.widget.listener, page.listener);
  assert.equal(page.unrelated.textContent, 'Keep these instructions.');
  assert.equal(page.unversioned.textContent, copy.pending);
  assert.equal(page.document.title, 'ReadMD V0.0.6');
  assert.match(page.link.href, /download\/V0\.0\.6\//);
  page.apply(fixture('V0.0.4')); assert.equal(page.label.textContent, current, 'an older API response must not downgrade the page');
  const future = onlineFixture(copy, { version: '0.0.6', pathname: '/'+copy.prefix+'ai-autocomplete/' });
  future.apply(fixture()); assert.equal(future.label.textContent, 'V0.0.6 · '+copy.pending);
  future.apply(fixture('V0.0.6')); assert.equal(future.label.textContent, 'V0.0.6 · '+copy.available);
  assert.equal(future.document.title, 'ReadMD V0.0.4', 'a feature-page title is not a current-release badge');
  const history = onlineFixture(copy, { pathname: '/'+copy.prefix+'release-notes/' });
  history.apply(fixture('V0.0.6'));
  assert.equal(history.label.textContent, 'V0.0.5 · '+copy.pending);
  assert.equal(history.heading.textContent, 'V0.0.5 AI autocomplete');
  assert.equal(JSON.parse(history.metadata.textContent).softwareVersion, '0.0.4');
});
