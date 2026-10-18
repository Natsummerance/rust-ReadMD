"""Offline resource preparation only; never shipped or invoked by ReadMD.

Pin variable fonts to real 400/700 outlines so the Rust TrueType subsetter and
Typst use identical glyphs. No font is registered with the operating system.
"""
import argparse
import hashlib
import json
from pathlib import Path
import fontTools
from fontTools.ttLib import TTFont
from fontTools.varLib.instancer import instantiateVariableFont

parser = argparse.ArgumentParser()
parser.add_argument("--inputs", type=Path, required=True)
parser.add_argument("--output", type=Path, required=True)
parser.add_argument("--write-lock", action="store_true")
args = parser.parse_args()
if fontTools.__version__ != "4.63.0":
    raise SystemExit("Reviewed fontTools 4.63.0 is required for resource preparation")
root = Path(__file__).resolve().parents[2]
inputs = json.loads((root / "tools/publication-font-inputs.lock.json").read_text())
pins = {item["name"]: item for item in inputs["inputs"]}
args.output.mkdir(parents=True, exist_ok=True)
outputs = []
for source, family in [("NotoSansSC.ttf", "ReadMD Sans SC"), ("NotoSerifSC.ttf", "ReadMD Serif SC")]:
    raw = (args.inputs / source).read_bytes()
    if hashlib.sha256(raw).hexdigest() != pins[source]["sha256"]:
        raise SystemExit("Source font differs from reviewed input")
    for weight, style in [(400, "Regular"), (700, "Bold")]:
        font = TTFont(args.inputs / source, recalcTimestamp=False)
        if [axis.axisTag for axis in font["fvar"].axes] != ["wght"]:
            raise SystemExit("Unexpected variable axes")
        font = instantiateVariableFont(font, {"wght": weight}, inplace=True)
        ps = family.replace(" ", "") + "-" + style
        values = {1: family, 2: style, 3: ps + "; ReadMD 2026.10.10", 4: family + " " + style, 6: ps, 16: family, 17: style}
        for record in font["name"].names:
            if record.nameID in values:
                record.string = values[record.nameID].encode(record.getEncoding())
        for key, value in values.items():
            font["name"].setName(value, key, 3, 1, 0x409)
        font["OS/2"].usWeightClass = weight
        font["OS/2"].fsSelection = (font["OS/2"].fsSelection & ~0x61) | (0x20 if weight == 700 else 0x40)
        font["head"].macStyle = 1 if weight == 700 else 0
        if "fvar" in font or "gvar" in font:
            raise SystemExit("Static font retains variation tables")
        name = ps + ".ttf"
        font.save(args.output / name)
        data = (args.output / name).read_bytes()
        outputs.append({"name": name, "source": source, "weight": weight, "size": len(data), "sha256": hashlib.sha256(data).hexdigest()})
        font.close()
lock = {"schema": 1, "tool": "fontTools 4.63.0", "modified": "Pinned weight, renamed derivative families; original OFL copyright and license retained", "files": outputs}
lockfile = root / "tools/publication-static-fonts.lock.json"
if args.write_lock:
    lockfile.write_text(json.dumps(lock, indent=2) + "\n", encoding="utf-8")
elif json.loads(lockfile.read_text()) != lock:
    raise SystemExit("Prepared static fonts differ from reviewed output")
print(json.dumps({"offline": True, "files": outputs}))
