"""Synthetic independent ZIP/XML/PNG oracle controls; no product or Word host."""
import base64
import copy
import importlib.util
from pathlib import Path
import struct
import tempfile
import unittest
import zipfile
import zlib

_spec = importlib.util.spec_from_file_location("numbering_inspector", Path(__file__).with_name("inspect-publication-numbering.py"))
qa = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(qa)
W, WP, A, R = qa.W, qa.WP, qa.A, qa.R
P = "http://schemas.openxmlformats.org/package/2006/relationships"
VALID = base64.b64decode("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNgYGD4DwABBAEAgLvRWwAAAABJRU5ErkJggg==")
# Exact original candidate41 pixel.png, retained as a failed material control.
OLD_INVALID = base64.b64decode("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+a2ioAAAAASUVORK5CYII=")


def chunk(kind, data):
    return struct.pack(">I", len(data)) + kind + data + struct.pack(">I", zlib.crc32(kind + data) & 0xffffffff)


def pixel(rgba):
    return b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", 1, 1, 8, 6, 0, 0, 0)) + chunk(b"IDAT", zlib.compress(bytes([0] + rgba), 9)) + chunk(b"IEND", b"")


def parts():
    def picture(alt):
        return f'<w:p><w:r><w:drawing><wp:inline><wp:docPr id="1" descr="{alt}"/><a:graphic><a:blip r:embed="image"/></a:graphic></wp:inline></w:drawing></w:r></w:p>'
    def caption(chapter):
        return f'<w:p><w:pPr><w:pStyle w:val="Caption"/></w:pPr><w:bookmarkStart w:id="{chapter}" w:name="figure_{chapter}"/><w:r><w:t>Figure </w:t></w:r><w:fldSimple w:instr=" STYLEREF 1 \\s "><w:r><w:t>{chapter}</w:t></w:r></w:fldSimple><w:r><w:t>-</w:t></w:r><w:fldSimple w:instr=" SEQ ReadMDFigure \\s 1 "><w:r><w:t>1</w:t></w:r></w:fldSimple><w:bookmarkEnd w:id="{chapter}"/><w:r><w:t>: Figure title</w:t></w:r></w:p>'
    document=f'<w:document xmlns:w="{W}" xmlns:wp="{WP}" xmlns:a="{A}" xmlns:r="{R}"><w:body>{picture("One")}{caption(1)}{picture("Two")}{caption(2)}<w:sectPr/></w:body></w:document>'
    styles=f'<w:styles xmlns:w="{W}"><w:docDefaults><w:rPrDefault><w:rPr><w:sz w:val="24"/><w:szCs w:val="24"/></w:rPr></w:rPrDefault></w:docDefaults><w:style w:styleId="Normal" w:type="paragraph"/><w:style w:styleId="Caption" w:type="paragraph"><w:basedOn w:val="Normal"/><w:rPr><w:sz w:val="18"/></w:rPr></w:style></w:styles>'
    rels=f'<Relationships xmlns="{P}"><Relationship Id="styles" Type="{R}/styles" Target="styles.xml"/><Relationship Id="numbering" Type="{R}/numbering" Target="numbering.xml"/><Relationship Id="settings" Type="{R}/settings" Target="settings.xml"/><Relationship Id="image" Type="{R}/image" Target="media/shared.png"/></Relationships>'
    return {'_rels/.rels':f'<Relationships xmlns="{P}"><Relationship Id="main" Type="{R}/officeDocument" Target="word/document.xml"/></Relationships>'.encode(),
            'word/_rels/document.xml.rels':rels.encode(),'word/document.xml':document.encode(),'word/styles.xml':styles.encode(),
            'word/numbering.xml':f'<w:numbering xmlns:w="{W}"/>'.encode(),'word/settings.xml':f'<w:settings xmlns:w="{W}"/>'.encode(),'word/media/shared.png':VALID}


class IndependentNumberedImages(unittest.TestCase):
    def inspect(self, values):
        with tempfile.TemporaryDirectory(prefix="readmd-numbered-oracle-") as directory:
            file=Path(directory)/'owned.docx'
            with zipfile.ZipFile(file,'w',zipfile.ZIP_STORED) as archive:
                for name, data in values.items(): archive.writestr(name,data)
            return qa.inspect(file)

    def test_new_png_is_exact_crc_valid_and_has_real_black_rgba_pixels(self):
        self.assertEqual(len(VALID),70)
        self.assertEqual(qa.hashlib.sha256(VALID).hexdigest(),'dc5687b50f70cb95379bfced5a0eae768dd4382cd6b393ee77d65bbdd6373fbf')
        self.assertEqual(VALID,pixel([0,0,0,255]))
        actual=qa.png_pixels(VALID)
        self.assertTrue(actual['allChunkCrcValid'])
        self.assertEqual(actual['dimensions'],[1,1]);self.assertEqual(actual['rgba'],[0,0,0,255])
        self.assertEqual([row['crc'] for row in actual['chunks']],['1f15c489','80bbd15b','ae426082'])

    def test_old_candidate41_invalid_idat_crc_is_a_real_negative(self):
        with self.assertRaisesRegex(AssertionError,'PNG chunk CRC: IDAT'): qa.png_pixels(OLD_INVALID)

    def test_changed_valid_pixel_is_decoded_instead_of_trusting_sut_or_name(self):
        actual=qa.png_pixels(pixel([255,255,255,255]))
        self.assertEqual(actual['rgba'],[255,255,255,255]);self.assertNotEqual(actual['rgba'],qa.png_pixels(VALID)['rgba'])

    def test_native_drawing_relationships_alt_media_and_caption_axes_are_independent(self):
        actual=self.inspect(parts())
        self.assertEqual([row['description'] for row in actual['images']],['One','Two'])
        self.assertEqual(len({row['member'] for row in actual['images']}),1)
        self.assertTrue(all(row['sha256']==qa.hashlib.sha256(VALID).hexdigest() for row in actual['images']))
        captions=[row for row in actual['paragraphs'] if row['style']=='Caption']
        self.assertEqual([row['bodyIndex'] for row in captions],[1,3])
        self.assertEqual([row['text'] for row in captions],['Figure 1-1: Figure title','Figure 2-1: Figure title'])
        self.assertTrue(all(run['properties']['sz']=='18' and run['properties']['szCs']=='24' for row in captions for run in row['visibleRuns']))
        self.assertEqual(captions[0]['bookmarkRanges'][0]['text'],'Figure 1-1')
        self.assertEqual([field['cached'] for field in captions[1]['fields']],['2','1'])

    def test_cs_axis_is_not_copied_from_western_size(self):
        data=parts();data['word/styles.xml']=data['word/styles.xml'].replace(b'<w:sz w:val="18"/>',b'<w:sz w:val="18"/><w:szCs w:val="22"/>')
        caption=self.inspect(data)['paragraphs'][1]
        self.assertTrue(all(run['properties']['sz']=='18' and run['properties']['szCs']=='22' for run in caption['visibleRuns']))

    def test_alias_prefixes_do_not_change_real_namespaces_or_description(self):
        data=parts();data['word/document.xml']=data['word/document.xml'].replace(b'wp:',b'wpAlias:').replace(b'xmlns:wp=',b'xmlns:wpAlias=')
        self.assertEqual([row['description'] for row in self.inspect(data)['images']],['One','Two'])

    def test_foreign_docpr_cannot_supply_a_real_drawing_description(self):
        data=parts();data['word/document.xml']=data['word/document.xml'].replace(WP.encode(),b'urn:foreign-docpr')
        with self.assertRaisesRegex(AssertionError,'description and blip'): self.inspect(data)

    def test_media_is_not_assumed_to_live_at_a_named_word_media_file(self):
        data=parts();data['payload/actual.png']=data.pop('word/media/shared.png');data['word/_rels/document.xml.rels']=data['word/_rels/document.xml.rels'].replace(b'media/shared.png',b'../payload/actual.png')
        self.assertTrue(all(row['member']=='payload/actual.png' for row in self.inspect(data)['images']))

    def test_bad_media_crc_refuses_without_reading_any_host_path(self):
        data=parts();data['word/media/shared.png']=OLD_INVALID
        with self.assertRaisesRegex(AssertionError,'PNG chunk CRC: IDAT'): self.inspect(data)

    def test_missing_image_payload_is_not_a_successful_native_drawing(self):
        data=parts();del data['word/media/shared.png']
        with self.assertRaisesRegex(AssertionError,'Missing image payload'): self.inspect(data)

    def test_external_or_foreign_image_relationships_do_not_grant_a_payload(self):
        for old,new in [(b'Target="media/shared.png"',b'Target="https://outside.invalid/x.png" TargetMode="External"'),(b'Type="'+R.encode()+b'/image"',b'Type="urn:foreign/image"')]:
            data=parts();data['word/_rels/document.xml.rels']=data['word/_rels/document.xml.rels'].replace(old,new)
            with self.assertRaisesRegex(AssertionError,'Non-image/external'): self.inspect(data)

    def test_escaping_and_duplicate_relationships_are_explicit_failures(self):
        data=parts();data['word/_rels/document.xml.rels']=data['word/_rels/document.xml.rels'].replace(b'media/shared.png',b'../../private.png')
        with self.assertRaisesRegex(AssertionError,'escapes OPC root'): self.inspect(data)
        data=parts();node=f'<Relationship Id="image" Type="{R}/image" Target="media/shared.png"/>'.encode();data['word/_rels/document.xml.rels']=data['word/_rels/document.xml.rels'].replace(b'</Relationships>',node+b'</Relationships>')
        with self.assertRaisesRegex(AssertionError,'Duplicate/missing relationship'): self.inspect(data)


if __name__=='__main__': unittest.main()
