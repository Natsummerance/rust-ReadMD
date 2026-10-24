"""Pure helper controls; no ReadMD/managed engine or Win32 observer is started."""
import importlib.util
import io
import json
from pathlib import Path
import tempfile
import types
import unittest
from unittest import mock

HERE = Path(__file__).resolve().parent
spec = importlib.util.spec_from_file_location('resource_wait', HERE / 'inspect-windows-resource-wait.py')
qa = importlib.util.module_from_spec(spec)
spec.loader.exec_module(qa)


class Controls(unittest.TestCase):
    def test_00_finish_stops_and_joins_reader_before_final_receipt(self):
        class NoReadAfterFinish:
            reads = 0
            def readline(self, size):
                self.reads += 1
                assert self.reads == 1, 'A second stdin read could block during interpreter shutdown'
                return b'{"seq":1,"action":"finish"}\n'
        stream, threads = NoReadAfterFinish(), []
        constructor = qa.threading.Thread
        def record_thread(*args, **kwargs):
            value = constructor(*args, **kwargs)
            threads.append(value)
            return value
        with mock.patch.object(qa.threading, 'Thread', side_effect=record_thread):
            generator = qa.commands(1, stream)
            self.assertEqual(next(generator), {'seq': 1, 'action': 'finish'})
            self.assertEqual(len(threads), 1)
            self.assertFalse(threads[0].is_alive(), 'Reader must end before finish is handed to observer')
            with self.assertRaises(StopIteration):
                next(generator)
        self.assertEqual(stream.reads, 1)

    def test_00b_bad_command_and_eof_are_not_synthetic_finish(self):
        self.assertEqual(list(qa.commands(1, io.BytesIO(b''))), [])
        values = list(qa.commands(1, io.BytesIO(b'{"seq":1,"action":"snapshot"}\n')))
        self.assertEqual(values, [{'seq': 1, 'action': 'snapshot'}])
        self.assertFalse(any(row.get('action') == 'finish' for row in values))
        for raw in (b'{malformed}\n', b'{"action":"finish"}', b' ' * (qa.MAX_LINE + 1)):
            with self.subTest(raw=raw[:40]), self.assertRaises((AssertionError, json.JSONDecodeError)):
                list(qa.commands(1, io.BytesIO(raw)))

    def test_01_journal_header_is_a_real_complete_record_not_sleep(self):
        with tempfile.TemporaryDirectory(prefix='readmd-resource-pure-') as raw:
            profile = Path(raw)
            folder = profile / 'worker-tasks-v1' / ('a' * 32)
            folder.mkdir(parents=True)
            record = folder / 'record.jsonl'
            header = {'type': 'Header', 'value': {'id': folder.name, 'schema': 1, 'profile': 'b' * 64,
                      'purpose': 'mcp-export', 'root': str(profile / ('rmdw-' + 'b' * 12) / folder.name)}}
            record.write_text(json.dumps(header), encoding='utf-8')
            self.assertTrue(qa.journals(profile)[0]['headerPending'])
            record.write_text(json.dumps(header) + '\n', encoding='utf-8')
            self.assertEqual(qa.journals(profile)[0]['purpose'], 'mcp-export')
            record.write_text((json.dumps(header) + '\n') * 2, encoding='utf-8')
            with self.assertRaises(AssertionError):
                qa.journals(profile)

    def test_02_journal_root_identity_is_not_trusted(self):
        with tempfile.TemporaryDirectory() as raw:
            profile = Path(raw)
            folder = profile / 'worker-tasks-v1' / ('a' * 32)
            folder.mkdir(parents=True)
            header = {'type': 'Header', 'value': {'id': folder.name, 'schema': 1, 'profile': 'b' * 64,
                      'purpose': 'mcp-export', 'root': str(profile / 'outside' / folder.name)}}
            (folder / 'record.jsonl').write_text(json.dumps(header) + '\n', encoding='utf-8')
            with self.assertRaises(AssertionError):
                qa.journals(profile)

    def test_03_inventory_preserves_all_resource_bytes_excludes_only_coordination_locks(self):
        with tempfile.TemporaryDirectory() as raw:
            profile = Path(raw)
            managed = profile / 'capabilities' / 'managed-v2'
            (managed / '.locks').mkdir(parents=True)
            (managed / 'typst').mkdir()
            file = managed / 'typst' / 'program'
            file.write_bytes(b'original')
            (managed / '.locks' / 'typst.lock').write_bytes(b'')
            before = qa.inventory(profile)
            self.assertEqual(before['bytes'], 8)
            self.assertEqual(len(before['files']), 1)
            file.write_bytes(b'changed!')
            self.assertNotEqual(qa.inventory(profile), before)

    def test_04_nonordinary_and_escape_reject(self):
        with tempfile.TemporaryDirectory() as raw:
            profile = Path(raw)
            with self.assertRaises(AssertionError):
                qa.ordinary(profile)
            with self.assertRaises(AssertionError):
                qa.inside(profile, profile)
            with self.assertRaises(AssertionError):
                qa.inside(profile, profile.parent / 'foreign')

    def test_04b_real_registry_coordination_is_not_an_active_task(self):
        with tempfile.TemporaryDirectory() as raw:
            profile = Path(raw)
            root = profile / 'worker-tasks-v1'
            root.mkdir()
            for name in ('allocation.lock', 'recovery.lock'):
                (root / name).write_bytes(b'')
            (root / 'recovery.cursor').write_bytes(b'fixed-size scheduling state')
            self.assertEqual(qa.journals(profile), [])
            (root / 'unknown.lock').write_bytes(b'unknown, do not delete')
            with self.assertRaises(AssertionError):
                qa.journals(profile)
            self.assertEqual((root / 'unknown.lock').read_bytes(), b'unknown, do not delete')

    def test_04c_retiring_witness_remains_a_task_until_reclaimed(self):
        with tempfile.TemporaryDirectory() as raw:
            profile = Path(raw)
            root = profile / 'worker-tasks-v1'
            root.mkdir()
            ident, binding = 'a' * 32, 'b' * 64
            header = {'type': 'Header', 'value': {'id': ident, 'schema': 1, 'profile': binding,
                      'purpose': 'mcp-export', 'root': str(profile / ('rmdw-' + binding[:12]) / ident)}}
            (root / ('retiring-' + ident + '.jsonl')).write_text(json.dumps(header) + '\n', encoding='utf-8')
            rows = qa.journals(profile)
            self.assertEqual(len(rows), 1)
            self.assertTrue(rows[0]['retiring'])

    def make_pdf(self, base, marker='RESOURCEWAITPURE', visible=True, link=True, paths=True, metadata=True, embedded=True):
        import pymupdf
        file = base / 'control.pdf'
        doc = pymupdf.open()
        page = doc.new_page()
        if embedded:
            page.insert_font(fontname='Qa', fontfile=str(HERE.parent.parent / 'assets/fonts/DejaVuSans.ttf'))
        page.insert_text((72, 72), marker, fontname='Qa' if embedded else 'helv',
                         fontsize=12, render_mode=0 if visible else 3)
        if paths:
            page.draw_line((72, 100), (144, 100))
        if link:
            page.insert_link({'kind': pymupdf.LINK_URI, 'from': pymupdf.Rect(72, 60, 140, 74),
                              'uri': 'https://example.invalid/resource-wait'})
        if metadata:
            # Synthetic control of admission only; this is not an engine run.
            doc.set_metadata({'creator': 'Typst 0.15.1'})
        doc.save(file)
        doc.close()
        return file

    def test_05_actual_file_reader_positive(self):
        with tempfile.TemporaryDirectory() as raw:
            file = self.make_pdf(Path(raw))
            proof = qa.pdf(types.SimpleNamespace(input=file, marker='RESOURCEWAITPURE'))
            self.assertTrue(proof['passed'])
            self.assertGreater(proof['visibleGlyphs'], 0)
            self.assertGreater(proof['drawPaths'], 0)
            self.assertTrue(proof['fonts'])
            self.assertEqual(proof['sha256'], qa.digest(file.read_bytes()))

    def test_06_invisible_stale_missing_link_path_or_font_cannot_fake_recovery(self):
        for changes in [{'visible': False}, {'marker': 'OTHERCONTENT'}, {'link': False},
                        {'paths': False}, {'metadata': False}, {'embedded': False}]:
            with self.subTest(changes=changes), tempfile.TemporaryDirectory() as raw:
                file = self.make_pdf(Path(raw), **changes)
                with self.assertRaises(AssertionError):
                    qa.pdf(types.SimpleNamespace(input=file, marker='RESOURCEWAITPURE'))

    def test_07_pdf_image_fallback_reject(self):
        import pymupdf
        with tempfile.TemporaryDirectory() as raw:
            base = Path(raw)
            file = self.make_pdf(base)
            doc = pymupdf.open(file)
            pix = pymupdf.Pixmap(pymupdf.csRGB, pymupdf.IRect(0, 0, 4, 4), False)
            pix.clear_with(255)
            doc[0].insert_image(pymupdf.Rect(200, 200, 204, 204), pixmap=pix)
            alternate = base / 'image.pdf'
            doc.save(alternate)
            doc.close()
            with self.assertRaises(AssertionError):
                qa.pdf(types.SimpleNamespace(input=alternate, marker='RESOURCEWAITPURE'))


if __name__ == '__main__':
    unittest.main()
