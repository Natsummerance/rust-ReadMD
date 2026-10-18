"""Independent PDF counterexamples for the QA checker, never native acceptance.

Use existing private font evidence. Controlled PDFs deliberately draw correct
glyphs, full capitals, or a smaller point size; no ReadMD engine is executed.
"""
import argparse
from io import BytesIO
import importlib.util
import json
from pathlib import Path
import traceback

import fitz
from fontTools.ttLib import TTFont


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("audit", type=Path)
    parser.add_argument("destination", type=Path)
    parser.add_argument("--family", default="Libertinus Serif")
    args = parser.parse_args()
    assert not args.destination.exists(), "Preserve the first checker calibration evidence"
    args.destination.mkdir(parents=True)
    spec = importlib.util.spec_from_file_location("smallcaps", Path(__file__).with_name("publication-smallcaps-proof.py"))
    checker = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(checker)
    sources = checker.source_fonts(args.audit, args.family)
    fonts = {}
    for face, item in sources.items():
        raw = (checker.ROOT / item["path"]).read_bytes()
        fonts[(face, "NORMAL")] = fitz.Font(fontbuffer=raw)
        for mode in ("SMALL", "UPPERCASE"):
            font = TTFont(BytesIO(raw))
            for table in font["cmap"].tables:
                if table.isUnicode():
                    for char in checker.WORD[1:]:
                        source_glyph = item["cmap"][ord(char)]
                        table.cmap[ord(char)] = item["smcp"][source_glyph] if mode == "SMALL" else item["cmap"][ord(char.upper())]
            suffix = "-" + mode + "QA"
            for name in font["name"].names:
                if name.nameID in (1, 4, 6, 16):
                    name.string = (name.toUnicode() + suffix).encode(name.getEncoding())
            if "CFF " in font:
                cff = font["CFF "].cff
                cff.fontNames = [str(name) + suffix for name in cff.fontNames]
                cff.topDictIndex[0].FullName += suffix
                cff.topDictIndex[0].FamilyName += suffix
            buffer = BytesIO()
            font.save(buffer)
            fonts[(face, mode)] = fitz.Font(fontbuffer=buffer.getvalue())
    cases = []
    for fault in ("correct", "full-uppercase", "smaller-point-size"):
        document = fitz.open()
        page = document.new_page(width=500, height=400)
        y = 35
        for face in checker.FACES:
            normal = fonts[(face, "NORMAL")]
            for mode in ("NORMAL", "SMALL", "UPPER"):
                marker = face + "_" + mode + " "
                writer = fitz.TextWriter(page.rect)
                writer.append((30, y), marker, font=normal, fontsize=11)
                font = fonts[(face, "UPPERCASE" if fault == "full-uppercase" else "SMALL")] if mode == "SMALL" else normal
                size = 8 if fault == "smaller-point-size" and mode == "SMALL" else 11
                word = checker.WORD.upper() if mode == "UPPER" else checker.WORD
                writer.append((30 + normal.text_length(marker, fontsize=11), y), word, font=font, fontsize=size)
                writer.write_text(page)
                y += 25
        file = args.destination / (fault + ".pdf")
        document.save(file)
        document.close()
        evidence = {"passed": False, "checkerCalibrationOnly": True, "nativeAcceptance": False}
        try:
            checker.prove_pdf(file, args.family, args.audit, evidence)
        except Exception:
            evidence["error"] = traceback.format_exc()
        (args.destination / (fault + ".json")).write_text(json.dumps(evidence, indent=2) + "\n", encoding="utf-8")
        cases.append({"case": fault, "expectedAccepted": fault == "correct", "actualAccepted": evidence["passed"]})
    result = {"passed": all(item["expectedAccepted"] == item["actualAccepted"] for item in cases), "cases": cases,
              "scope": "Independent real PDF/font calibration only. No product executable, official fixture, or engine acceptance was run."}
    (args.destination / "summary.json").write_text(json.dumps(result, indent=2) + "\n", encoding="utf-8")
    print(json.dumps(result))
    if not result["passed"]:
        raise SystemExit(1)


if __name__ == "__main__":
    main()
