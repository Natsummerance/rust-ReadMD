'use strict';
function fixture(){
  const stream=text=>'<< /Length '+Buffer.byteLength(text)+' >>\nstream\n'+text+'\nendstream';
  const objects=[
    '<< /Type /Catalog /Pages 2 0 R /AcroForm << /Fields [9 0 R] /DR << /Font << /Helv 4 0 R >> >> /DA (/Helv 12 Tf 0 g) >> >>',
    '<< /Type /Pages /Kids [3 0 R 6 0 R] /Count 2 >>',
    '<< /Type /Page /Parent 2 0 R /MediaBox [0 0 595 842] /Resources << /Font << /F1 4 0 R >> /XObject << /Im1 8 0 R >> >> /Contents 5 0 R /Annots [9 0 R] >>',
    '<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>',
    stream('BT /F1 24 Tf 50 760 Td (ReadMD PDF original) Tj ET\nBT /F1 14 Tf 50 710 Td (Evidence remains unchanged.) Tj ET\n0.1 0.2 0.8 rg 50 520 200 100 re f\nq 80 0 0 60 350 530 cm /Im1 Do Q'),
    '<< /Type /Page /Parent 2 0 R /MediaBox [0 0 595 842] /Resources << /Font << /F1 4 0 R >> >> /Contents 7 0 R >>',
    stream('BT /F1 24 Tf 50 760 Td (Second page remains unchanged.) Tj ET\n0.2 0.6 0.3 rg 50 520 200 100 re f'),
    '<< /Type /XObject /Subtype /Image /Width 1 /Height 1 /ColorSpace /DeviceRGB /BitsPerComponent 8 /Length 3 >>\nstream\n'+String.fromCharCode(230,170,90)+'\nendstream',
    '<< /Type /Annot /Subtype /Widget /FT /Tx /T (reviewer) /V (ReadMD) /Rect [50 420 250 452] /P 3 0 R /F 4 /DA (/Helv 12 Tf 0 g) >>'
  ];
  let text='%PDF-1.7\n',offsets=[0];for(let i=0;i<objects.length;i++){offsets.push(Buffer.byteLength(text,'latin1'));text+=(i+1)+' 0 obj\n'+objects[i]+'\nendobj\n';}
  const xref=Buffer.byteLength(text,'latin1');text+='xref\n0 '+(objects.length+1)+'\n0000000000 65535 f \n'+offsets.slice(1).map(n=>String(n).padStart(10,'0')+' 00000 n \n').join('')+'trailer\n<< /Size '+(objects.length+1)+' /Root 1 0 R >>\nstartxref\n'+xref+'\n%%EOF\n';return Buffer.from(text,'latin1');
}
module.exports={fixture};
