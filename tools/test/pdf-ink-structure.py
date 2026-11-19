"""Read-only native annotation inspection for explicitly selected QA files."""
import ctypes as c
import json
from pathlib import Path
import sys

library, pdf, source, expected = sys.argv[1:]
expected = json.loads(expected)
lib = c.CDLL(library)


def bind(name, result, *args):
    fn = getattr(lib, name)
    fn.restype = result
    fn.argtypes = list(args)
    return fn


init = bind("FPDF_InitLibrary", None)
destroy = bind("FPDF_DestroyLibrary", None)
load = bind("FPDF_LoadDocument", c.c_void_p, c.c_char_p, c.c_char_p)
close_doc = bind("FPDF_CloseDocument", None, c.c_void_p)
load_page = bind("FPDF_LoadPage", c.c_void_p, c.c_void_p, c.c_int)
close_page = bind("FPDF_ClosePage", None, c.c_void_p)
count = bind("FPDFPage_GetAnnotCount", c.c_int, c.c_void_p)
get = bind("FPDFPage_GetAnnot", c.c_void_p, c.c_void_p, c.c_int)
close_annot = bind("FPDFPage_CloseAnnot", None, c.c_void_p)
kind = bind("FPDFAnnot_GetSubtype", c.c_int, c.c_void_p)
string = bind("FPDFAnnot_GetStringValue", c.c_ulong, c.c_void_p, c.c_char_p, c.c_void_p, c.c_ulong)
objects = bind("FPDFAnnot_GetObjectCount", c.c_int, c.c_void_p)
get_object = bind("FPDFAnnot_GetObject", c.c_void_p, c.c_void_p, c.c_int)
object_type = bind("FPDFPageObj_GetType", c.c_int, c.c_void_p)
cap = bind("FPDFPageObj_GetLineCap", c.c_int, c.c_void_p)
join = bind("FPDFPageObj_GetLineJoin", c.c_int, c.c_void_p)
ink_count = bind("FPDFAnnot_GetInkListCount", c.c_ulong, c.c_void_p)
ink_points = bind("FPDFAnnot_GetInkListPath", c.c_ulong, c.c_void_p, c.c_ulong, c.c_void_p, c.c_ulong)
init()
doc = load(str(Path(pdf).resolve()).encode(), None)
assert doc
page = load_page(doc, 0)
assert page
try:
    found = False
    for index in range(count(page)):
        annotation = get(page, index)
        try:
            if kind(annotation) != 15:
                continue
            size = string(annotation, b"ReadMDInk", None, 0)
            assert 0 < size < 2 * 1024 * 1024
            data = c.create_string_buffer(size)
            assert string(annotation, b"ReadMDInk", data, size) == size
            assert bytes(data).decode("utf-16-le").rstrip("\0") == Path(source).read_text()
            assert objects(annotation) == ink_count(annotation) == len(expected)
            for stroke, length in enumerate(expected):
                assert ink_points(annotation, stroke, None, 0) == length
                obj = get_object(annotation, stroke)
                assert object_type(obj) == 2 and cap(obj) == join(obj) == 1
            found = True
        finally:
            close_annot(annotation)
    assert found
finally:
    close_page(page)
    close_doc(doc)
    destroy()
print(json.dumps({"nativeStrokes": len(expected), "pointCounts": expected, "separateRoundVectorPaths": True, "embeddedSvgExact": True}))
