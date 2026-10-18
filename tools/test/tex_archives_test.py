"""Adversarial extraction tests use only synthetic archives and owned temp roots."""
import hashlib
import io
import lzma
import sys
import tarfile
import tempfile
import unittest
from pathlib import Path
sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from lib.tex_archives import extract_container, safe_name


class TeXArchives(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix='readmd-tex-extraction-test-')
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)

    def fixture(self, entries=None, tail=b''):
        entries = entries or [('tex/fixture.sty', b'Synthetic style'), ('tlpkg/tlpobj/fixture.tlpobj', b'name fixture\nrevision 1\n')]
        raw = io.BytesIO()
        with tarfile.open(fileobj=raw, mode='w', format=tarfile.USTAR_FORMAT) as archive:
            for name, content in entries:
                info = tarfile.TarInfo(name)
                if content is None:
                    info.type = tarfile.SYMTYPE
                    info.linkname = '../outside'
                    archive.addfile(info)
                else:
                    info.size = len(content)
                    archive.addfile(info, io.BytesIO(content))
        payload = lzma.compress(raw.getvalue()) + tail
        file = self.root / ('input-' + str(len(list(self.root.glob('input-*')))) + '.tar.xz')
        file.write_bytes(payload)
        component = {'name': 'fixture', 'revision': '1', 'relocated': True, 'containers': {'run': {'name': 'fixture.tar.xz', 'size': len(payload), 'sha512': hashlib.sha512(payload).hexdigest(), 'files': ['RELOC/tex/fixture.sty']}}}
        return file, component

    def test_valid_archive_relocates_exact_declared_files_and_verifies_control(self):
        file, component = self.fixture()
        records = extract_container(file, component, 'run', self.root / 'output', {})
        self.assertEqual(len(records), 2)
        self.assertEqual((self.root / 'output/texmf-dist/tex/fixture.sty').read_bytes(), b'Synthetic style')

    def test_archive_digest_or_size_substitution_is_refused_before_extraction(self):
        file, component = self.fixture()
        file.write_bytes(file.read_bytes()[:-1] + b'x')
        with self.assertRaisesRegex(ValueError, 'integrity'):
            extract_container(file, component, 'run', self.root / 'output')
        self.assertFalse((self.root / 'output').exists())

    def test_paths_links_duplicates_unknown_and_missing_files_are_refused(self):
        for bad in ['../outside', '/absolute', 'C:/outside', 'tex/CON.txt', 'tex/name.', 'tex\\escape']:
            with self.subTest(path=bad), self.assertRaises(ValueError):
                safe_name(bad)
        cases = [ [('tex/fixture.sty', None)], [('tex/fixture.sty', b'x'), ('tex/fixture.sty', b'x')],
                  [('tex/unknown.sty', b'x')], [('tlpkg/tlpobj/fixture.tlpobj', b'name fixture\nrevision 1\n')],
                  [('tex/fixture.sty', b'x'), ('tlpkg/tlpobj/fixture.tlpobj', b'name fixture\nrevision 2\n')] ]
        for i, entries in enumerate(cases):
            with self.subTest(case=i):
                file, component = self.fixture(entries)
                with self.assertRaises(ValueError):
                    extract_container(file, component, 'run', self.root / ('case-' + str(i)))

    def test_extra_xz_stream_or_trailing_bytes_are_refused_even_with_matching_hash(self):
        for i, tail in enumerate([b'unrecognized', lzma.compress(b'concatenated')]):
            file, component = self.fixture(tail=tail)
            with self.assertRaisesRegex(ValueError, 'Trailing|concatenated'):
                extract_container(file, component, 'run', self.root / ('tail-' + str(i)))

    def test_shared_dependency_files_require_identical_bytes_and_no_case_alias(self):
        file, component = self.fixture()
        output, records = self.root / 'shared', {}
        extract_container(file, component, 'run', output, records)
        self.assertEqual(extract_container(file, component, 'run', output, records), [])
        (output / 'texmf-dist/tex/fixture.sty').write_bytes(b'Changed existing content')
        with self.assertRaisesRegex(ValueError, 'Conflicting'):
            extract_container(file, component, 'run', output, records)
        (output / 'texmf-dist/tex/fixture.sty').write_bytes(b'Synthetic style')
        records['texmf-dist/tex/fixture.sty']['path'] = 'texmf-dist/tex/FIXTURE.sty'
        with self.assertRaisesRegex(ValueError, 'Case alias'):
            extract_container(file, component, 'run', output, records)


if __name__ == '__main__':
    unittest.main()
