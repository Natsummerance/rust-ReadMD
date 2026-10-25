"""Independent Windows fault-lane material derivation and OCR text checks.

No product import; the only runtime dependency is the QA Python's installed
PyMuPDF. The tile derivation must stay deterministic for a fixed source image
and PyMuPDF build: geometry is pinned at 3000x3500 and the source digest is
checked against the locked repository probe before anything is derived.
"""
import argparse
import hashlib
import json
import sys
from pathlib import Path

PROBE_SHA256 = "b978c564d7f48e172d6bf3fa52f18cb81938f69348dbf531165ca68ca6150b29"
PROBE_WIDTH, PROBE_HEIGHT = 1000, 170
TILE_WIDTH, TILE_HEIGHT = 3000, 3500
REQUIRED_TOKENS = ("READMD", "12345")
MAX_MATERIAL_BYTES = 64 * 1024 * 1024


def digest(value):
    return hashlib.sha256(value).hexdigest()


def valid_tile_geometry(width, height):
    """Fixed geometry only; a caller may never reshape the derived material."""
    assert isinstance(width, int) and isinstance(height, int), "Tile geometry must be integers"
    assert width == TILE_WIDTH and height == TILE_HEIGHT, "Tile geometry is fixed at 3000x3500"
    return True


def fixed_summary(source, output, width, height):
    """Pure summary of a derived material; no path or process claims."""
    assert valid_tile_geometry(width, height)
    assert isinstance(source, (bytes, bytearray)) and isinstance(output, (bytes, bytearray))
    source, output = bytes(source), bytes(output)
    assert source and output, "A derived material needs real source and output bytes"
    assert len(source) <= MAX_MATERIAL_BYTES and len(output) <= MAX_MATERIAL_BYTES
    return {
        "kind": "tiled-ocr-material",
        "sourceSha256": digest(source),
        "outputSha256": digest(output),
        "width": width,
        "height": height,
        "sourceBytes": len(source),
        "outputBytes": len(output),
        "sourcePixelsPreserved": True,
    }


def validate_ocr_text(text):
    """Both required tokens must appear; a single missing token is a failure."""
    assert isinstance(text, str), "OCR result must be text"
    flat = "".join(text.split())
    assert flat, "OCR result is empty"
    missing = [token for token in REQUIRED_TOKENS if token not in flat]
    assert not missing, "OCR text missing required tokens: " + ",".join(missing)
    return {
        "passed": True,
        "tokens": {token: token in flat for token in REQUIRED_TOKENS},
        "visibleCharacters": len(flat),
    }


def tile(args):
    import pymupdf

    valid_tile_geometry(args.width, args.height)
    source = Path(args.source).resolve(strict=True)
    output = Path(args.output).resolve()
    assert source.is_file() and source.stat().st_size <= MAX_MATERIAL_BYTES
    raw = source.read_bytes()
    assert digest(raw) == PROBE_SHA256, "Unknown OCR probe source image"
    assert output.parent.is_dir()
    document = pymupdf.open()
    page = document.new_page(width=TILE_WIDTH, height=TILE_HEIGHT)
    shape = page.new_shape()
    shape.draw_rect(pymupdf.Rect(0, 0, TILE_WIDTH, TILE_HEIGHT))
    shape.finish(color=None, fill=(1, 1, 1))
    shape.commit()
    tiles = 0
    # Native-scale repeats only: the probe's pixels are never resampled.
    for x in range(0, TILE_WIDTH, PROBE_WIDTH):
        for y in range(0, TILE_HEIGHT, PROBE_HEIGHT):
            page.insert_image(pymupdf.Rect(x, y, x + PROBE_WIDTH, y + PROBE_HEIGHT), stream=raw)
            tiles += 1
    pixmap = page.get_pixmap(dpi=72)
    assert pixmap.width == TILE_WIDTH and pixmap.height == TILE_HEIGHT, "Rendered material geometry drifted"
    pixmap.save(str(output))
    written = output.read_bytes()
    assert len(written) <= MAX_MATERIAL_BYTES
    summary = fixed_summary(raw, written, TILE_WIDTH, TILE_HEIGHT)
    summary["tiles"] = tiles
    summary["path"] = str(output)
    return summary


def ocr(args):
    text = Path(args.input).resolve(strict=True).read_text(encoding="utf-8")
    assert len(text.encode("utf-8")) <= MAX_MATERIAL_BYTES
    result = validate_ocr_text(text)
    result["input"] = str(args.input)
    return result


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="action", required=True)
    derived = sub.add_parser("tile")
    derived.add_argument("--source", required=True)
    derived.add_argument("--output", required=True)
    derived.add_argument("--width", type=int, default=TILE_WIDTH)
    derived.add_argument("--height", type=int, default=TILE_HEIGHT)
    checked = sub.add_parser("ocr")
    checked.add_argument("--input", required=True)
    args = parser.parse_args(argv)
    result = tile(args) if args.action == "tile" else ocr(args)
    print(json.dumps(result, ensure_ascii=False, allow_nan=False, indent=2))
    return result


if __name__ == "__main__":
    sys.stdout.reconfigure(encoding="utf-8")
    main()
