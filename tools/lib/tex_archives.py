"""Offline maintainer extraction. This module is not a ReadMD runtime dependency."""
import hashlib
import io
import lzma
import re
import tarfile
from pathlib import PurePosixPath

MAX_FILE = 256 * 1024 * 1024
MAX_EXPANDED = 4 * 1024 * 1024 * 1024


def safe_name(name):
    if not isinstance(name, str) or not name or re.search(r'[\\:\x00-\x1f\x7f"<>|?*]', name):
        raise ValueError('Unsafe TeX archive path')
    parts = name.split('/')
    if any(not p or p in ('.', '..') or p.endswith((' ', '.')) or
           re.match(r'^(CON|PRN|AUX|NUL|COM[1-9]|LPT[1-9])(?:\.|$)', p, re.I) for p in parts):
        raise ValueError('Unsafe TeX archive path')
    return str(PurePosixPath(name))


def digest_file(file, algorithm='sha256'):
    digest = hashlib.new(algorithm)
    with open(file, 'rb') as stream:
        for block in iter(lambda: stream.read(65536), b''):
            digest.update(block)
    return digest.hexdigest()


class BoundedXZ(io.RawIOBase):
    """Bound dictionary allocation, streaming output and compressed framing."""
    def __init__(self, file):
        super().__init__()
        self.source = open(file, 'rb')
        self.decoder = lzma.LZMADecompressor(memlimit=128 * 1024 * 1024)
        self.total = 0
        self.finished = False

    def readable(self):
        return True

    def readinto(self, buffer):
        if self.finished:
            return 0
        while True:
            data = self.source.read(65536) if self.decoder.needs_input else b''
            if self.decoder.needs_input and not data:
                raise ValueError('Truncated TeX XZ container')
            output = self.decoder.decompress(data, max_length=min(len(buffer), 65536))
            self.total += len(output)
            if self.total > MAX_EXPANDED:
                raise ValueError('TeX expanded archive budget')
            if self.decoder.eof:
                if self.decoder.unused_data or self.source.read(1):
                    raise ValueError('Trailing or concatenated TeX XZ container')
                self.finished = True
            if output:
                buffer[:len(output)] = output
                return len(output)
            if self.finished:
                return 0

    def close(self):
        self.source.close()
        super().close()


def extract_container(file, component, kind, destination, existing=None):
    """Strict declared file set; never call extractall or execute package hooks."""
    container = component['containers'][kind]
    if file.is_symlink() or file.stat().st_size != container['size'] or digest_file(file, 'sha512') != container['sha512']:
        raise ValueError('Pinned TeX container integrity differs: ' + container['name'])
    expected, expected_folded = set(), set()
    for name in container['files']:
        if name.startswith('RELOC/'):
            if not component['relocated']:
                raise ValueError('Unexpected relocated TeX file')
            name = name[6:]
        name = safe_name(name)
        if name.casefold() in expected_folded:
            raise ValueError('Duplicate TeX file declaration')
        expected.add(name)
        expected_folded.add(name.casefold())
    suffix = '' if kind == 'run' else '.' + kind
    control = 'tlpkg/tlpobj/' + component['name'] + suffix + '.tlpobj'
    expected.add(control)
    found, folded, records = set(), set(), []
    destination.mkdir(parents=True, exist_ok=True)
    if destination.is_symlink():
        raise ValueError('Real TeX preparation directory required')
    with BoundedXZ(file) as raw, io.BufferedReader(raw) as stream:
        with tarfile.open(fileobj=stream, mode='r|') as archive:
            for member in archive:
                name = safe_name(member.name.rstrip('/') if member.isdir() else member.name)
                if member.isdir():
                    if member.linkname:
                        raise ValueError('Linked TeX directory')
                    continue
                if not member.isfile() or member.linkname or member.size < 0 or member.size > MAX_FILE:
                    raise ValueError('Unsupported TeX archive entry')
                if name not in expected or name.casefold() in folded:
                    raise ValueError('Undeclared or duplicate TeX entry: ' + name)
                folded.add(name.casefold())
                found.add(name)
                relative = name if name.startswith('tlpkg/') or not component['relocated'] else 'texmf-dist/' + name
                target = destination.joinpath(*safe_name(relative).split('/'))
                if existing is not None and relative.casefold() in existing and existing[relative.casefold()]['path'] != relative:
                    raise ValueError('Case alias in TeX preparation tree')
                parent = target.parent
                while parent != destination:
                    if parent.is_symlink():
                        raise ValueError('Linked TeX extraction parent')
                    parent = parent.parent
                target.parent.mkdir(parents=True, exist_ok=True)
                if target.is_symlink():
                    raise ValueError('Linked TeX extraction file')
                source = archive.extractfile(member)
                digest = hashlib.sha256()
                count = 0
                # Shared paths may occur across dependency containers, but
                # neither changed bytes nor case aliases may overwrite them.
                duplicate = target.exists()
                if name == control and member.size > 512 * 1024:
                    raise ValueError('TeX control metadata budget')
                output = io.BytesIO() if name == control else open(target, 'rb' if duplicate else 'xb')
                try:
                    while True:
                        data = source.read(65536)
                        if not data:
                            break
                        digest.update(data)
                        if duplicate and name != control:
                            if output.read(len(data)) != data:
                                raise ValueError('Conflicting TeX dependency file: ' + relative)
                        else:
                            output.write(data)
                        count += len(data)
                        if count > member.size:
                            raise ValueError('TeX member size differs')
                    if count != member.size:
                        raise ValueError('Truncated TeX member')
                    sha = digest.hexdigest()
                    if duplicate and (target.stat().st_size != count or name == control and digest_file(target) != sha):
                        raise ValueError('Conflicting TeX dependency file: ' + relative)
                    if name == control:
                        text = output.getvalue().decode('utf8')
                        if not re.search(r'^name ' + re.escape(component['name'] + suffix) + r'$', text, re.M) or not re.search(r'^revision ' + re.escape(component['revision']) + r'$', text, re.M):
                            raise ValueError('TeX package control identity differs')
                        if not duplicate:
                            with open(target, 'xb') as saved:
                                saved.write(output.getvalue())
                    if not duplicate:
                        record = {'path': relative, 'size': count, 'sha256': sha}
                        if existing is not None:
                            key = relative.casefold()
                            if key in existing:
                                raise ValueError('Case alias in TeX preparation tree')
                            existing[key] = record
                        records.append(record)
                finally:
                    output.close()
                    source.close()
        # tarfile stops at end records. Consume and validate all XZ framing;
        # unknown bytes after the tar end cannot be silently ignored.
        for tail in iter(lambda: stream.read(65536), b''):
            if tail.strip(b'\0'):
                raise ValueError('Trailing TeX tar data')
    if found != expected:
        raise ValueError('Incomplete TeX container file set: ' + container['name'])
    return records
