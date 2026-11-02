"""Archive the project, executable releases and finished presentation without build caches."""
from __future__ import annotations

import hashlib
import json
import os
from pathlib import Path
import struct
import zipfile

ROOT = Path(__file__).resolve().parent.parent
OUTPUT = ROOT / 'dist' / 'OmniView-0.5-Complete.zip'
EXCLUDED_DIRS = {'target', 'node_modules', '.git', '.codex', '__pycache__', 'verification'}


def files():
    for current, directories, names in os.walk(ROOT, followlinks=False):
        folder = Path(current)
        relative = folder.relative_to(ROOT)
        directories[:] = sorted(d for d in directories if d not in EXCLUDED_DIRS)
        if relative.as_posix() == '.':
            directories[:] = [d for d in directories if d != 'site-dist']
        if relative.as_posix() == 'site/.vitepress':
            directories[:] = [d for d in directories if d == 'theme']
        if relative.as_posix() == 'presentation/.build':
            directories[:] = [d for d in directories if d == 'assets']
        for name in sorted(names):
            source = folder / name
            if source == OUTPUT or ('Complete' in name and source.suffix == '.zip') or source == ROOT / 'dist.7z':
                continue
            if relative.as_posix() == 'presentation/.build' and source.suffix != '.mjs':
                continue
            if source.is_symlink() or source.suffix in {'.tmp', '.pyc'}:
                continue
            yield source, 'OmniView/' + source.relative_to(ROOT).as_posix()


def main():
    source_files = list(files())
    inventory = []
    with zipfile.ZipFile(OUTPUT, 'w', compression=zipfile.ZIP_DEFLATED, compresslevel=6) as archive:
        for source, name in source_files:
            archive.write(source, name)
            inventory.append({'file': name, 'bytes': source.stat().st_size,
                              'sha256': hashlib.sha256(source.read_bytes()).hexdigest()})
        archive.writestr('OmniView/СОСТАВ АРХИВА.json', json.dumps(inventory, ensure_ascii=False, indent=2))
    with zipfile.ZipFile(OUTPUT) as archive:
        assert archive.testzip() is None, 'ZIP CRC check failed'
        assert len(archive.namelist()) == len(set(archive.namelist())), 'Duplicate paths'
        for record in inventory:
            data = archive.read(record['file'])
            assert len(data) == record['bytes']
            assert hashlib.sha256(data).hexdigest() == record['sha256']
        for prefix, machine in [('OmniView/dist/', 0x8664), ('OmniView/dist/win32/', 0x14c)]:
            for name in ['OmniView.exe', 'OmniView-Student.exe', 'WebView2Loader.dll']:
                data = archive.read(prefix + name)
                pe = struct.unpack_from('<I', data, 0x3c)[0]
                assert struct.unpack_from('<H', data, pe + 4)[0] == machine
        import io
        import re
        deck = archive.read('OmniView/presentation/output/OmniView-0.3-Presentation.pptx')
        with zipfile.ZipFile(io.BytesIO(deck)) as pptx:
            assert len([n for n in pptx.namelist() if re.fullmatch(r'ppt/slides/slide\d+\.xml', n)]) == 12
            assert len([n for n in pptx.namelist() if re.fullmatch(r'ppt/notesSlides/notesSlide\d+\.xml', n)]) == 12
        assert 'OmniView/src/main.rs' in archive.namelist()
        assert 'OmniView/НАЧАТЬ ЗДЕСЬ.txt' in archive.namelist()
        assert all('/target/' not in n and '/node_modules/' not in n for n in archive.namelist())
    print(json.dumps({'archive': str(OUTPUT), 'files': len(inventory) + 1,
                      'bytes': OUTPUT.stat().st_size, 'integrity': 'passed'}, ensure_ascii=False))


if __name__ == '__main__':
    main()
