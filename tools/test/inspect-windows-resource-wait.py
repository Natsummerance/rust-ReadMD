"""Independent, finite Windows resource-lock/owner observer; no product imports.

The Job accounting is cumulative (including exited descendants), not a process
name snapshot. Execution must demonstrate a positive worker after release before
using its unchanged count as evidence for the preceding blocked requests.
"""
import argparse
import ctypes
from ctypes import wintypes as W
import hashlib
import json
import os
from pathlib import Path
import queue
import re
import stat
import sys
import threading
import time

MAX_FILES = 4096
MAX_TREE_BYTES = 128 * 1024 * 1024
MAX_LINE = 1024 * 1024


def digest(value):
    return hashlib.sha256(value).hexdigest()


def ordinary(path, directory=False):
    path = Path(path).absolute()
    for part in [*reversed(path.parents), path]:
        info = part.lstat()
        assert not stat.S_ISLNK(info.st_mode) and not getattr(info, 'st_file_attributes', 0) & 0x400, 'Reparse/link path'
        if part != path or directory:
            assert stat.S_ISDIR(info.st_mode), 'Non-directory ancestor'
    if not directory:
        assert stat.S_ISREG(path.stat().st_mode), 'Not an ordinary file'
    assert path.resolve(strict=True) == path, 'Non-canonical path'
    return path


def inside(base, path):
    base, path = Path(base).absolute(), Path(path).absolute()
    assert path != base and base in path.parents, 'Path escaped owned evidence'
    return path


def journals(profile):
    # Completed owners can retire a directory between enumeration and read.
    # Restart the whole observation, never turn a partial Header into READY.
    for attempt in range(3):
        try:
            return journals_once(profile)
        except FileNotFoundError:
            if attempt == 2:
                raise


def journals_once(profile):
    root = profile / 'worker-tasks-v1'
    if not root.exists():
        return []
    ordinary(root, True)
    result = []
    for child in sorted(root.iterdir()):
        assert len(result) < MAX_FILES
        if child.name in ('allocation.lock', 'recovery.lock', 'recovery.cursor'):
            ordinary(child)
            assert child.stat().st_size <= 1024, 'Coordination file budget'
            continue
        retiring = re.fullmatch(r'retiring-([0-9a-f]{32})\.jsonl', child.name)
        if retiring:
            ordinary(child)
            ident, file = retiring.group(1), child
        else:
            ordinary(child, True)
            assert len(child.name) == 32 and all(c in '0123456789abcdef' for c in child.name), 'Unexpected journal ID'
            ident, file = child.name, child / 'record.jsonl'
        if not file.exists():
            result.append({'id': ident, 'headerPending': True})
            continue
        ordinary(file)
        assert file.stat().st_size <= 1024 * 1024, 'Journal budget'
        raw = file.read_bytes()
        # A writer may currently be appending its final line. Only complete
        # lines are observations; incomplete headers never establish readiness.
        lines = [json.loads(line) for line in raw.split(b'\n')[:-1] if line]
        headers = [item['value'] for item in lines if item.get('type') == 'Header']
        assert len(headers) <= 1, 'Duplicate journal header'
        if not headers:
            result.append({'id': ident, 'headerPending': True})
            continue
        header = headers[0]
        assert header['id'] == ident and header['schema'] == 1
        assert len(header['profile']) == 64 and all(c in '0123456789abcdef' for c in header['profile'])
        work = Path(header['root'])
        assert work.is_absolute() and work.name == ident
        assert work.parent.name == 'rmdw-' + header['profile'][:12], 'Journal workspace identity'
        result.append({'id': ident, 'purpose': header['purpose'], 'root': str(work),
                       'retiring': bool(retiring),
                       'rootExists': work.exists(), 'recordSha256': digest(raw), 'recordBytes': len(raw)})
    return result


def inventory(profile):
    base = profile / 'capabilities' / 'managed-v2'
    ordinary(base, True)
    result, total = [], 0
    for folder, dirs, files in os.walk(base, followlinks=False):
        folder = ordinary(Path(folder), True)
        dirs[:] = sorted(name for name in dirs if not (folder == base and name == '.locks'))
        for name in dirs:
            ordinary(folder / name, True)
        for name in sorted(files):
            file = ordinary(folder / name)
            total += file.stat().st_size
            assert len(result) < MAX_FILES and total <= MAX_TREE_BYTES, 'Installed resource budget'
            raw = file.read_bytes()
            result.append({'path': file.relative_to(base).as_posix(), 'bytes': len(raw), 'sha256': digest(raw)})
    return {'files': result, 'bytes': total}


class Win32:
    def __init__(self):
        assert os.name == 'nt', 'Windows-only observer'
        self.k = ctypes.WinDLL('kernel32', use_last_error=True)
        signatures = {
            'CreateFileW': ([W.LPCWSTR, W.DWORD, W.DWORD, W.LPVOID, W.DWORD, W.DWORD, W.HANDLE], W.HANDLE),
            'CloseHandle': ([W.HANDLE], W.BOOL),
            'GetFileInformationByHandle': ([W.HANDLE, W.LPVOID], W.BOOL),
            'OpenProcess': ([W.DWORD, W.BOOL, W.DWORD], W.HANDLE),
            'QueryFullProcessImageNameW': ([W.HANDLE, W.DWORD, W.LPWSTR, ctypes.POINTER(W.DWORD)], W.BOOL),
            'GetProcessTimes': ([W.HANDLE, W.LPVOID, W.LPVOID, W.LPVOID, W.LPVOID], W.BOOL),
            'CreateJobObjectW': ([W.LPVOID, W.LPCWSTR], W.HANDLE),
            'AssignProcessToJobObject': ([W.HANDLE, W.HANDLE], W.BOOL),
            'QueryInformationJobObject': ([W.HANDLE, ctypes.c_int, W.LPVOID, W.DWORD, W.LPVOID], W.BOOL),
            'WaitForSingleObject': ([W.HANDLE, W.DWORD], W.DWORD),
        }
        for name, (args, restype) in signatures.items():
            function = getattr(self.k, name)
            function.argtypes, function.restype = args, restype

    def check(self, ok):
        if not ok:
            raise ctypes.WinError(ctypes.get_last_error())
        return ok

    def close(self, handle):
        if handle:
            self.check(self.k.CloseHandle(handle))

    def file(self, file, exclusive=False):
        handle = self.k.CreateFileW(str(file), 0xC0000000 if exclusive else 0x80000000,
                                   0 if exclusive else 7, None, 4 if exclusive else 3, 0x00200000, None)
        if handle == ctypes.c_void_p(-1).value:
            raise ctypes.WinError(ctypes.get_last_error())
        return handle

    def file_id(self, handle):
        class Info(ctypes.Structure):
            _fields_ = [('attrs', W.DWORD), ('created', W.FILETIME), ('accessed', W.FILETIME),
                        ('written', W.FILETIME), ('volume', W.DWORD), ('high', W.DWORD),
                        ('low', W.DWORD), ('links', W.DWORD), ('indexHigh', W.DWORD), ('indexLow', W.DWORD)]
        info = Info()
        self.check(self.k.GetFileInformationByHandle(handle, ctypes.byref(info)))
        assert not info.attrs & (0x400 | 0x10) and info.links == 1, 'Non-ordinary file handle'
        return {'volume': f'{int(info.volume):08x}', 'index': f'{((int(info.indexHigh) << 32) | int(info.indexLow)):016x}',
                'created': f'{((int(info.created.dwHighDateTime) << 32) | int(info.created.dwLowDateTime)):016x}',
                'bytes': (int(info.high) << 32) | int(info.low)}

    def identity(self, file):
        handle = self.file(ordinary(file))
        try:
            result = self.file_id(handle)
            raw = Path(file).read_bytes()
            assert len(raw) == result['bytes'] <= 8 * 1024 * 1024
            return {**result, 'sha256': digest(raw)}
        finally:
            self.close(handle)

    def host(self, pid, binary):
        assert 0 < pid < 2**32 and pid != os.getpid()
        # Query + set quota + terminate are the documented rights required by
        # AssignProcessToJobObject. No terminate operation is ever issued.
        handle = self.k.OpenProcess(0x1000 | 0x0100 | 0x0001 | 0x00100000, False, pid)
        self.check(handle)
        job = None
        try:
            buffer, length = ctypes.create_unicode_buffer(32768), W.DWORD(32768)
            self.check(self.k.QueryFullProcessImageNameW(handle, 0, buffer, ctypes.byref(length)))
            assert Path(buffer.value).resolve(strict=True) == ordinary(binary), 'Wrong process image'
            times = [W.FILETIME() for _ in range(4)]
            self.check(self.k.GetProcessTimes(handle, *[ctypes.byref(value) for value in times]))
            created = f'{((int(times[0].dwHighDateTime) << 32) | int(times[0].dwLowDateTime)):016x}'
            job = self.k.CreateJobObjectW(None, None)
            self.check(job)
            self.check(self.k.AssignProcessToJobObject(job, handle))
            return handle, job, {'pid': pid, 'image': str(binary), 'created': created}
        except BaseException:
            self.close(job)
            self.close(handle)
            raise

    def accounting(self, job, host):
        class Accounting(ctypes.Structure):
            _fields_ = [('user', ctypes.c_longlong), ('kernel', ctypes.c_longlong),
                        ('periodUser', ctypes.c_longlong), ('periodKernel', ctypes.c_longlong),
                        ('faults', W.DWORD), ('total', W.DWORD), ('active', W.DWORD), ('terminated', W.DWORD)]
        value = Accounting()
        self.check(self.k.QueryInformationJobObject(job, 1, ctypes.byref(value), ctypes.sizeof(value), None))
        assert value.total >= value.active
        state = self.k.WaitForSingleObject(host, 0)
        assert state in (0, 258), 'Cannot observe host lifetime'
        return {'totalProcesses': int(value.total), 'activeProcesses': int(value.active),
                'terminatedProcesses': int(value.terminated), 'hostExited': state == 0}


def commands(timeout, stream=None):
    inbox = queue.Queue(maxsize=8)
    source = sys.stdin.buffer if stream is None else stream
    stopped = threading.Event()
    def read():
        try:
            while True:
                line = source.readline(MAX_LINE + 1)
                if not line:
                    inbox.put(None)
                    return
                assert len(line) <= MAX_LINE and line.endswith(b'\n'), 'Command budget'
                value = json.loads(line)
                inbox.put(value)
                # finish is the final protocol command. Do not begin another
                # buffered read which could outlive Python finalization.
                if isinstance(value, dict) and value.get('action') == 'finish':
                    return
        except BaseException as error:
            inbox.put(error)
        finally:
            stopped.set()
    reader = threading.Thread(target=read, daemon=True)
    reader.start()

    def join_reader():
        assert stopped.wait(1), 'Observer stdin reader did not finish'
        reader.join(timeout=1)
        assert not reader.is_alive(), 'Observer stdin reader remains alive'

    end = time.monotonic() + timeout
    while True:
        value = inbox.get(timeout=max(.001, end - time.monotonic()))
        if isinstance(value, BaseException):
            join_reader()
            raise value
        if value is None:
            join_reader()
            return
        assert time.monotonic() < end and isinstance(value, dict)
        finished = value.get('action') == 'finish'
        if finished:
            # The consumer may return immediately after its final receipt.
            # Prove the input thread has already stopped before yielding it.
            join_reader()
        yield value
        if finished:
            return


def emit(value):
    print(json.dumps({'monotonicNs': time.monotonic_ns(), **value}, separators=(',', ':')), flush=True)


def observer(args):
    profile = ordinary(args.profile, True)
    assert profile.name == 'profile' and profile.parent.name.startswith('resource-wait-'), 'Not a fresh owned profile'
    win = Win32()
    handle, job, identity = win.host(args.pid, ordinary(args.binary))
    lock, locked_id, locked_resource = None, None, None
    try:
        initial = win.accounting(job, handle)
        assert initial['totalProcesses'] == initial['activeProcesses'] == 1 and not initial['hostExited']
        emit({'event': 'ready', 'host': identity, 'accounting': initial, 'journals': journals(profile),
              'cumulativeJobObservation': True, 'killOnJobClose': False})
        seen = set()
        for command in commands(480):
            seq = command.get('seq')
            assert type(seq) is int and seq > 0 and seq not in seen
            assert len(seen) < 4096, 'Observer command budget'
            seen.add(seq)
            action = command.get('action')
            if action == 'lock':
                assert lock is None and command.get('resource') in ('fonts', 'typst')
                parent = ordinary(profile / 'capabilities' / 'managed-v2' / '.locks', True)
                file = parent / (command['resource'] + '.lock')
                if file.exists():
                    ordinary(file)
                lock = win.file(file, True)
                locked_resource = command['resource']
                locked_id = win.file_id(lock)
                assert locked_id['bytes'] == 0
                # Match production's Shared probe exactly, rather than using
                # the more permissive share flags used for identity reads.
                other = win.k.CreateFileW(str(file), 0x80000000, 1, None, 3, 0x00200000, None)
                if other != ctypes.c_void_p(-1).value:
                    win.close(other)
                    raise AssertionError('Exclusive lock did not block the production Shared open')
                assert ctypes.get_last_error() == 32, 'Not a sharing violation'
                emit({'seq': seq, 'event': 'locked', 'path': str(file), 'resource': command['resource'],
                      'identity': locked_id, 'sharedOpenWinError': 32, 'accounting': win.accounting(job, handle)})
            elif action == 'release':
                assert lock is not None and command.get('resource') == locked_resource, 'Release is not for the held resource'
                win.close(lock)
                lock = None
                file = profile / 'capabilities' / 'managed-v2' / '.locks' / (command['resource'] + '.lock')
                assert win.identity(file) == {**locked_id, 'sha256': digest(b'')}
                emit({'seq': seq, 'event': 'released', 'identity': locked_id})
            elif action == 'snapshot':
                targets = command.get('targets', [])
                assert isinstance(targets, list) and len(targets) <= 8
                values = {str(file): win.identity(inside(profile.parent / 'outputs', file)) for file in targets}
                emit({'seq': seq, 'event': 'snapshot', 'accounting': win.accounting(job, handle),
                      'journals': journals(profile), 'targets': values, 'lockHeld': lock is not None})
            elif action == 'inventory':
                emit({'seq': seq, 'event': 'inventory', 'inventory': inventory(profile)})
            elif action == 'finish':
                assert lock is None, 'Lock not explicitly released'
                final = win.accounting(job, handle)
                assert final['hostExited'] and final['activeProcesses'] == 0, 'Owner/worker still alive'
                emit({'seq': seq, 'event': 'finished', 'accounting': final})
                return
            else:
                raise AssertionError('Unknown observer command')
        raise AssertionError('Missing explicit finish receipt')
    finally:
        win.close(lock)
        win.close(job)
        win.close(handle)


def pdf(args):
    import pymupdf
    file = ordinary(args.input)
    assert file.stat().st_size <= 8 * 1024 * 1024
    assert args.marker and all(c.isalnum() for c in args.marker)
    with pymupdf.open(file) as doc:
        assert 0 < len(doc) <= 4
        assert any(re.fullmatch(r'Typst 0\.15\.1(?: \([^\r\n]*\))?', doc.metadata.get(key, '') or '')
                   for key in ('creator', 'producer')), 'Wrong PDF engine'
        text, visible, paths, links, fonts = '', 0, 0, [], {}
        for page in doc:
            assert not page.get_images(full=True), 'Image fallback'
            for trace in page.get_texttrace():
                if trace['type'] == 3:
                    continue
                assert trace['opacity'] == 1 and trace['type'] == 0, 'Unexpected invisible/stroked text'
                text += ''.join(chr(char[0]) for char in trace['chars'])
                visible += len(trace['chars'])
                assert all(char[1] > 0 for char in trace['chars']), 'Missing glyph'
            paths += len(page.get_drawings())
            links.extend(page.get_links())
            for row in page.get_fonts(full=True):
                xref = row[0]
                program = doc.extract_font(xref)[3]
                assert program and len(program) <= 16 * 1024 * 1024, 'Unembedded font'
                fonts[str(xref)] = {'baseFont': row[3], 'sha256': digest(program), 'bytes': len(program)}
        assert args.marker in ''.join(text.split()), 'Actual visible marker is missing'
        assert paths > 0 and visible > 0 and fonts
        assert any(link.get('uri') == 'https://example.invalid/resource-wait' and
                   link['from'].width > 0 and link['from'].height > 0 for link in links), 'Actual link is missing'
        return {'passed': True, 'kind': 'actual-typst-pdf', 'sha256': digest(file.read_bytes()),
                'bytes': file.stat().st_size, 'pages': len(doc), 'visibleGlyphs': visible,
                'drawPaths': paths, 'fonts': fonts, 'marker': args.marker, 'externalLinkVerified': True}


def main():
    parser = argparse.ArgumentParser()
    sub = parser.add_subparsers(dest='command', required=True)
    watch = sub.add_parser('observe')
    for flag in ('profile', 'binary'):
        watch.add_argument('--' + flag, type=Path, required=True)
    watch.add_argument('--pid', type=int, required=True)
    check = sub.add_parser('pdf')
    check.add_argument('--input', type=Path, required=True)
    check.add_argument('--marker', required=True)
    args = parser.parse_args()
    if args.command == 'observe':
        observer(args)
    else:
        print(json.dumps(pdf(args), indent=2))


if __name__ == '__main__':
    main()
