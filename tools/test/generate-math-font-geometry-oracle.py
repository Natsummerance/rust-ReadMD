"""Independent, offline FontTools oracle for M01/M02 QA; never an app dependency.

Only the pinned bundled STIX font is read. No ReadMD binary, Rust output or
rendered SVG is consulted when producing expectations. --check is read-only.
"""
import argparse
import hashlib
import json
from pathlib import Path

import fontTools
from fontTools.pens.boundsPen import BoundsPen
from fontTools.ttLib import TTFont

ROOT = Path(__file__).resolve().parents[2]
FONT = ROOT / "assets/fonts/STIXTwoMath-Regular.ttf"
FIXTURE = ROOT / "rust/readmd-kernel/tests/fixtures/math-font-geometry-stix.json"
FONT_SHA256 = "49b22767985171bce579307b21d98f4d35012fa39e316acc4ffcab8644dfb69b"
CHARS = "abfgjpnxyA𝐴+0123/⁄√\u0302\u0303\u0304\u0307\u0308\u20d7𝑎𝑏𝑓𝑔𝑗𝑝𝑛𝑥𝑦"
PERCENT = ["ScriptPercentScaleDown", "ScriptScriptPercentScaleDown", "RadicalDegreeBottomRaisePercent"]


def numeric(value):
    return int(value) if value == int(value) else round(float(value), 10)


def oracle():
    if fontTools.__version__ != "4.63.0":
        raise ValueError("This fixture generator requires the already installed FontTools 4.63.0")
    data = FONT.read_bytes()
    digest = hashlib.sha256(data).hexdigest()
    if digest != FONT_SHA256:
        raise ValueError("Bundled font identity differs; do not silently update the fixed oracle")
    font = TTFont(FONT, lazy=False)
    math = font["MATH"].table
    constants = {
        name: int(value.Value if hasattr(value, "Value") else value)
        for name, value in vars(math.MathConstants).items()
        if not name.startswith("_")
    }
    if len(constants) != 56:
        raise ValueError("Unexpected complete MATH constants table")
    cmap = font.getBestCmap()
    characters = {char: cmap[ord(char)] for char in CHARS}
    info = math.MathGlyphInfo
    top = info.MathTopAccentAttachment
    attachments = dict(zip(top.TopAccentCoverage.glyphs, (v.Value for v in top.TopAccentAttachment)))
    italic = info.MathItalicsCorrectionInfo
    corrections = dict(zip(italic.Coverage.glyphs, (v.Value for v in italic.ItalicsCorrection)))
    variants = {"vertical": {}, "horizontal": {}}
    names = set(characters.values())
    for axis, coverage, constructions in [
        ("vertical", math.MathVariants.VertGlyphCoverage, math.MathVariants.VertGlyphConstruction),
        ("horizontal", math.MathVariants.HorizGlyphCoverage, math.MathVariants.HorizGlyphConstruction),
    ]:
        for char in "/⁄√\u0302\u0303\u0304\u0307\u0308\u20d7":
            name = cmap[ord(char)]
            if coverage is None or name not in coverage.glyphs:
                variants[axis][char] = None
                continue
            construction = constructions[coverage.glyphs.index(name)]
            records = [{"glyph": v.VariantGlyph, "advanceMeasurement": v.AdvanceMeasurement}
                       for v in construction.MathGlyphVariantRecord]
            if len(records) > 32:
                raise ValueError("Unexpectedly large construction")
            names.update(record["glyph"] for record in records)
            assembly = None
            if construction.GlyphAssembly is not None:
                original = construction.GlyphAssembly
                parts = [{"glyph": p.glyph, "startConnectorLength": p.StartConnectorLength,
                          "endConnectorLength": p.EndConnectorLength, "fullAdvance": p.FullAdvance,
                          "flags": p.PartFlags} for p in original.PartRecords]
                names.update(part["glyph"] for part in parts)
                assembly = {"italicCorrection": original.ItalicsCorrection.Value, "parts": parts}
            variants[axis][char] = {"baseGlyph": name, "variants": records, "assembly": assembly}
    glyph_set = font.getGlyphSet()
    glyphs = {}
    for name in sorted(names):
        pen = BoundsPen(glyph_set)
        glyph_set[name].draw(pen)
        bounds = None if pen.bounds is None else [numeric(v) for v in pen.bounds]
        source = font["glyf"][name]
        font_bounds = ([source.xMin, source.yMin, source.xMax, source.yMax]
                       if hasattr(source, "xMin") else None)
        advance, bearing = font["hmtx"][name]
        glyphs[name] = {"gid": font.getGlyphID(name), "advance": advance, "leftSideBearing": bearing,
                        "fontBounds": font_bounds, "inkBounds": bounds,
                        "topAccentAttachment": attachments.get(name),
                        "italicCorrection": corrections.get(name, 0)}
    return {
        "schema": 1,
        "fontPath": FONT.relative_to(ROOT).as_posix(),
        "fontSha256": digest,
        "fontBytes": len(data),
        "unitsPerEm": font["head"].unitsPerEm,
        "fontToolsVersion": fontTools.__version__,
        "origin": "Direct FontTools MATH/hmtx/glyf/BoundsPen extraction; no product rendering or inferred expectations",
        "specification": "https://learn.microsoft.com/en-us/typography/opentype/spec/math",
        "units": {"constants": "font design units except percentageConstants", "percentageConstants": PERCENT,
                  "glyphBounds": "font design units, y up, signed; positive yMin is retained",
                  "inkBounds": "BoundsPen curve extrema; fontBounds retains the stored glyf bounds",
                  "attachmentFallback": "An absent attachment stays null; consumers may use advance/2 under the OpenType rule"},
        "constants": constants,
        "characters": characters,
        "glyphs": glyphs,
        "minConnectorOverlap": math.MathVariants.MinConnectorOverlap,
        "variants": variants,
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    mode = parser.add_mutually_exclusive_group(required=True)
    mode.add_argument("--out", type=Path, help="Explicit new or intentionally regenerated workspace JSON")
    mode.add_argument("--check", action="store_true", help="Compare to the fixed fixture without writing")
    args = parser.parse_args()
    result = oracle()
    encoded = (json.dumps(result, ensure_ascii=False, sort_keys=True, indent=2) + "\n").encode("utf-8")
    if len(encoded) > 64 * 1024:
        raise ValueError("Oracle exceeds the small fixture budget")
    if args.check:
        if FIXTURE.read_bytes() != encoded:
            raise ValueError("Fixed oracle differs; inspect the independent input before explicitly regenerating")
        target = FIXTURE
    else:
        target = args.out if args.out.is_absolute() else ROOT / args.out
        target = target.resolve()
        if ROOT not in target.parents:
            raise ValueError("Output must remain inside the workspace")
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_bytes(encoded)
    print(json.dumps({"checked": bool(args.check), "path": target.relative_to(ROOT).as_posix(),
                      "fontSha256": result["fontSha256"], "unitsPerEm": result["unitsPerEm"],
                      "constants": len(result["constants"]), "glyphs": len(result["glyphs"]),
                      "bytes": len(encoded), "sha256": hashlib.sha256(encoded).hexdigest()}))


if __name__ == "__main__":
    main()
