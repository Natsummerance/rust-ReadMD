"""Read the actual /Prev PDF revision; never synthesize a baseline PDF."""
import argparse
import hashlib
import io
import json
from pathlib import Path
import re
from pypdf import PdfReader
from pypdf.generic import NumberObject, StreamObject, read_object

parser = argparse.ArgumentParser()
parser.add_argument("file", type=Path)
parser.add_argument("baseline", type=Path)
args = parser.parse_args()
data = args.file.read_bytes()
assert len(data) <= 32 * 1024 * 1024
reader = PdfReader(io.BytesIO(data), strict=True)
final = re.search(rb"(?:\r\n|\r|\n)startxref\s+(\d+)\s+%%EOF[\t\r\n ]*\Z", data)
assert final is not None, "Final startxref/EOF record is missing"
final_offset = int(final.group(1))
assert 0 < final_offset < len(data), "Final startxref is outside the actual PDF"
previous = reader.trailer.get("/Prev")
previous_source = "trailer"
xref_stream_object = None
if previous is None:
    # pypdf merges only Root/Info/ID/Size from an xref stream into trailer.
    # Parse the actual final xref object, never an arbitrary /Prev in the file.
    header = re.match(rb"(\d+)\s+(\d+)\s+obj\b", data[final_offset:final_offset + 128])
    assert header is not None, "Publication metadata did not retain an inspectable original PDF revision"
    stream = io.BytesIO(data)
    stream.seek(final_offset)
    identity = reader.read_object_header(stream)
    assert identity == (int(header.group(1)), int(header.group(2))), "Final xref object header mismatch"
    xref = read_object(stream, reader)
    assert isinstance(xref, StreamObject) and xref.get("/Type") == "/XRef", "Final startxref does not point to an xref stream"
    previous = xref.get("/Prev")
    previous_source = "final-xref-stream"
    xref_stream_object = list(identity)
assert previous is not None, "Publication metadata did not retain an inspectable original PDF revision"
assert isinstance(previous, NumberObject), "The actual /Prev must be an integer PDF object"
previous = int(previous)
assert 0 < previous < final_offset, "The actual /Prev must identify an earlier xref"
matches = [match for match in re.finditer(rb"startxref\s+(\d+)\s+%%EOF(?:\r\n|\r|\n)?", data)
           if int(match.group(1)) == previous]
assert len(matches) == 1, "The /Prev startxref does not identify exactly one original revision"
before = data[:matches[0].end()]
assert len(before) < len(data)
assert args.baseline.parent.resolve() == args.file.parent.resolve(), "Baseline must stay in the owned output directory"
assert not args.baseline.exists(), "Preserve the first independent revision evidence"
baseline = PdfReader(io.BytesIO(before), strict=True)
assert len(baseline.pages) == len(reader.pages)
with args.baseline.open("xb") as output:
    output.write(before)
print(json.dumps({"baseline": str(args.baseline), "sha256": hashlib.sha256(before).hexdigest(),
                  "bytes": len(before), "previousStartXref": previous, "finalStartXref": final_offset,
                  "previousSource": previous_source, "xrefStreamObject": xref_stream_object,
                  "scope": "Exact original PDF revision embedded in the final incremental file. This is not a separately captured engine output; prefix equality is inherent in extraction. Independent comparisons cover decoded page contents/resources, annotations, non-metadata objects and raster pixels."}))
