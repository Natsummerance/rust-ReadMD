"""Maintainer-only, offline input composition; Python is never shipped/required.

No TeX executable, shell, installer hook, package updater or host font is used.
Prepared inputs remain unapproved until the separate distribution review.
"""
import argparse
import hashlib
import json
import os
import tempfile
import stat
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path
from lib.tex_archives import digest_file, extract_container, safe_name

ROOT = Path(__file__).resolve().parent.parent
BASE = ROOT / '.cache/capability-upgrade/tex-inputs'


def audit_tree(target, records):
    expected = {r['path'].casefold(): r for r in records}
    found = set()
    entries = []
    for folder, dirs, files in os.walk(target, followlinks=False):
        for child in dirs:
            if (Path(folder) / child).is_symlink():
                raise ValueError('Linked prepared TeX directory')
        for child in files:
            file = Path(folder) / child
            relative = file.relative_to(target).as_posix()
            record = expected.get(relative.casefold())
            if not record or record['path'] != relative:
                raise ValueError('Prepared TeX tree differs: ' + relative)
            entries.append((file, record))
            found.add(relative.casefold())
    if found != expected.keys():
        raise ValueError('Prepared TeX files missing')
    def verify(entry):
        file, record = entry
        info = file.lstat()
        if not stat.S_ISREG(info.st_mode) or info.st_size != record['size'] or digest_file(file) != record['sha256']:
            raise ValueError('Prepared TeX tree differs: ' + record['path'])
    # Eight fixed workers amortize small-file filesystem latency while keeping
    # reads bounded to 8 x 64 KiB. No unbounded extraction or process pool.
    print(json.dumps({'verificationStarted': True, 'files': len(entries)}), flush=True)
    with ThreadPoolExecutor(max_workers=8) as workers:
        for i, _ in enumerate(workers.map(verify, entries), 1):
            if i % 5000 == 0 or i == len(entries):
                print(json.dumps({'verifiedFiles': i, 'totalFiles': len(entries)}), flush=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--materials', action='store_true', help='Compose corresponding package sources/docs separately from execution resources')
    args = parser.parse_args()
    lock_file = ROOT / 'tools/tex-publication-inputs.lock.json'
    lock = json.loads(lock_file.read_text(encoding='utf8'))
    base_pin = json.loads((ROOT / 'tools/tex-runtime-inputs.lock.json').read_text(encoding='utf8'))
    inventory = json.loads((BASE / 'inventory.json').read_text(encoding='utf8'))
    if lock['schema'] != 1 or lock['redistributionApproved'] is not False or lock['baseArchiveSha256'] != base_pin['archive']['sha256'] or inventory['archive'] != base_pin['archive']:
        raise ValueError('Unexpected TeX input identity')
    kind = 'publication-materials' if args.materials else 'publication-runtime'
    target = BASE / kind
    receipt = BASE / (kind + '.json')
    lock_sha = digest_file(lock_file)
    if target.exists():
        if target.is_symlink():
            raise ValueError('Real prepared TeX directory required')
        report = json.loads(receipt.read_text(encoding='utf8'))
        if report['lockSha256'] != lock_sha:
            raise ValueError('Existing TeX composition uses another lock; retain it for review')
        audit_tree(target, report['files'])
        print(json.dumps({'audited': True, 'offline': True, 'files': len(report['files']), 'redistributionApproved': False}))
        return
    # Check every input before allocating a new tree. No scripts or processes
    # are run from these archives, including tlpobj execute directives.
    selected = [(p, k, c) for p in lock['components'] for k, c in p['containers'].items() if (k != 'run') == args.materials]
    for _, _, container in selected:
        file = BASE / 'containers' / safe_name(container['name'])
        if file.is_symlink() or file.stat().st_size != container['size'] or digest_file(file, 'sha512') != container['sha512']:
            raise ValueError('Unprepared TeX input: ' + container['name'])
    records = {}
    if not args.materials:
        audit_tree(BASE / 'TinyTeX', inventory['files'])
    staging = Path(tempfile.mkdtemp(prefix=kind + '-staging-', dir=BASE))
    if not args.materials:
        for record in inventory['files']:
            relative = safe_name(record['path'])
            file = staging.joinpath(*relative.split('/'))
            file.parent.mkdir(parents=True, exist_ok=True)
            os.link(BASE / 'TinyTeX' / relative, file)
            records[relative.casefold()] = record
    for component, container_kind, container in selected:
        extract_container(BASE / 'containers' / container['name'], component, container_kind, staging, records)
    if not args.materials:
        # Generate a relative TDS filename database in Python; do not run
        # mktexlsr/tlmgr or alter the immutable base snapshot/hard links.
        tree = staging / 'texmf-dist'
        lines = ['% ls-R -- filename database generated by ReadMD input preparation.', '']
        for folder, dirs, files in os.walk(tree):
            dirs.sort(); files.sort()
            relative = Path(folder).relative_to(tree).as_posix()
            lines += [('./' if relative == '.' else './' + relative) + ':', *sorted(dirs + files), '']
        database = tree / 'ls-R'
        replacement = tree / 'readmd-ls-R.prepared'
        replacement.write_text('\n'.join(lines) + '\n', encoding='utf8', newline='\n')
        os.replace(replacement, database)
        records['texmf-dist/ls-r'] = {'path': 'texmf-dist/ls-R', 'size': database.stat().st_size, 'sha256': digest_file(database)}
    files = sorted(records.values(), key=lambda r: r['path'])
    audit_tree(staging, files)
    report = {'schema': 1, 'offline': True, 'kind': kind, 'baseArchiveSha256': lock['baseArchiveSha256'], 'lockSha256': lock_sha,
              'additionalPackages': lock['additionalPackages'], 'files': files, 'totalBytes': sum(r['size'] for r in files),
              'redistributionApproved': False,
              'scope': 'Pinned maintainer input only. No application catalog entry, install, publication or redistribution certification.'}
    # A crash retains staging for inspection; no recursive deletion or silent
    # overwrite of an existing preparation is performed.
    os.rename(staging, target)
    with open(receipt, 'x', encoding='utf8', newline='\n') as output:
        json.dump(report, output, ensure_ascii=False, indent=2)
        output.write('\n')
    print(json.dumps({'prepared': True, 'offline': True, 'kind': kind, 'files': len(files), 'bytes': report['totalBytes'], 'redistributionApproved': False}))


if __name__ == '__main__':
    main()
