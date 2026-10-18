// Complete translations for the new structural slot controls. This source is
// also checked against locale metadata so a future locale cannot silently fall
// back to English. Run through the existing locale insertion tool.
import fs from 'node:fs';
import {spawnSync} from 'node:child_process';
const rows={
  en:['Numerator','Denominator','Expression','Root index','Limit'],
  'zh-CN':['分子','分母','表达式','根指数','极限'],
  'zh-TW':['分子','分母','運算式','根指數','極限'],
  'zh-HK':['分子','分母','運算式','根指數','極限'],
  ja:['分子','分母','式','根指数','極限'],ko:['분자','분모','수식','근 지수','극한'],
  fr:['Numérateur','Dénominateur','Expression','Indice de racine','Limite'],
  de:['Zähler','Nenner','Ausdruck','Wurzelexponent','Grenzwert'],
  es:['Numerador','Denominador','Expresión','Índice de raíz','Límite'],
  pt:['Numerador','Denominador','Expressão','Índice da raiz','Limite'],
  ru:['Числитель','Знаменатель','Выражение','Показатель корня','Предел'],
  it:['Numeratore','Denominatore','Espressione','Indice della radice','Limite'],
  ar:['البسط','المقام','التعبير','درجة الجذر','النهاية'],he:['מונה','מכנה','ביטוי','דרגת השורש','גבול'],
  ug:['سۈرەت','مەخرەج','ئىپادە','يىلتىز دەرىجىسى','لىمىت'],
  bo:['ཆ་གྲངས།','མ་གྲངས།','མཚོན་ཚུལ།','རྩ་བའི་སྟོན་གྲངས།','ཚད་མཐའ།'],
  mn:['Хүртвэр','Хуваарь','Илэрхийлэл','Язгуурын зэрэг','Хязгаар'],
  th:['ตัวเศษ','ตัวส่วน','นิพจน์','ดัชนีราก','ลิมิต'],vi:['Tử số','Mẫu số','Biểu thức','Chỉ số căn','Giới hạn'],
  id:['Pembilang','Penyebut','Ekspresi','Indeks akar','Limit'],ms:['Pengangka','Penyebut','Ungkapan','Indeks punca','Had'],
  tl:['Numerador','Denominador','Ekspresyon','Indeks ng ugat','Hangganan'],
  hi:['अंश','हर','व्यंजक','मूल घातांक','सीमा'],bn:['লব','হর','রাশি','মূলের সূচক','সীমা'],
  ne:['अंश','हर','व्यञ्जक','मूलको घाताङ्क','सीमा'],
  km:['ភាគយក','ភាគបែង','កន្សោម','សន្ទស្សន៍ឫស','លីមីត'],
  lo:['ຕົວເສດ','ຕົວສ່ວນ','ນິພົດ','ດັດຊະນີຮາກ','ລິມິດ'],
  my:['ပိုင်းဝေ','ပိုင်းခြေ','ကိန်းရပ်','အမြစ်ညွှန်းကိန်း','လီမစ်'],
  tr:['Pay','Payda','İfade','Kök derecesi','Limit'],nl:['Teller','Noemer','Uitdrukking','Wortelexponent','Limiet'],
  sv:['Täljare','Nämnare','Uttryck','Rotindex','Gränsvärde'],da:['Tæller','Nævner','Udtryk','Rodindeks','Grænseværdi'],
  no:['Teller','Nevner','Uttrykk','Rotindeks','Grenseverdi'],fi:['Osoittaja','Nimittäjä','Lauseke','Juuren aste','Raja-arvo'],
  el:['Αριθμητής','Παρονομαστής','Παράσταση','Δείκτης ρίζας','Όριο'],
  uk:['Чисельник','Знаменник','Вираз','Показник кореня','Границя'],
  ro:['Numărător','Numitor','Expresie','Indicele radicalului','Limită'],
  hu:['Számláló','Nevező','Kifejezés','Gyökkitevő','Határérték'],
  hr:['Brojnik','Nazivnik','Izraz','Stupanj korijena','Limes'],
  sl:['Števec','Imenovalec','Izraz','Stopnja korena','Limita'],
  eo:['Numeratoro','Denominatoro','Esprimo','Radikindico','Limeso'],
  ga:['Uimhreoir','Ainmneoir','Slonn','Innéacs fréimhe','Teorainn'],
  mt:['Numeratur','Denominatur','Espressjoni','Indiċi tal-għerq','Limitu'],
  kl:['Avitap qulaaniittoq','Avitap ataaniittoq','Kisitsisitut oqaatigisaq','Sorlaata indeksia','Killissaq'],
  kg:['Ntalu ya zulu','Ntalu ya nsi','Mpova ya ntalu','Ntalu ya muanzi','Ndilu'],
  rw:['Umubare wo hejuru','Umubare wo hasi','Imvugo y’imibare','Urwego rw’umuzi','Imbibi']
};
const meta=JSON.parse(fs.readFileSync('assets/i18n/meta.json','utf8'));
for(const locale of Object.keys(meta))if(!rows[locale])throw Error('Missing slot translation: '+locale);
const spec={};
spec['common.cancel']=Object.fromEntries(Object.keys(meta).map(locale=>{
  const existing=JSON.parse(fs.readFileSync('assets/i18n/'+locale+'.json','utf8'));
  if(!existing['editor.cancel'])throw Error('Missing native cancel label: '+locale);
  return[locale,existing['editor.cancel']];
}));
['num','den','e','deg','lim'].forEach((role,i)=>spec['formula.slot.'+role]=Object.fromEntries(Object.entries(rows).map(([locale,text])=>[locale,text[i]])));
fs.mkdirSync('.cache/capability-upgrade',{recursive:true});
const file='.cache/capability-upgrade/slot-labels.json';fs.writeFileSync(file,JSON.stringify(spec));
const result=spawnSync(process.execPath,['tools/i18n-add.mjs',file],{stdio:'inherit'});process.exitCode=result.status??1;
