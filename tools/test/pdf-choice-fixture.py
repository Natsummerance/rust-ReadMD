"""Synthetic combo fixtures and independent canonical/Widget persistence checks."""
import json
import sys
from pathlib import Path

from pypdf import PdfReader, PdfWriter
from pypdf.generic import ArrayObject, DictionaryObject, NameObject, NumberObject
from reportlab.pdfgen import canvas


def create(directory):
    directory = Path(directory)
    directory.mkdir(parents=True, exist_ok=True)
    original = directory / "single.pdf"
    document = canvas.Canvas(str(original), pagesize=(595, 842))
    document.drawString(50, 785, "Delivery preference")
    document.acroForm.choice(
        name="delivery", value="STD", x=50, y=700, width=300, height=30,
        fieldFlags="combo", options=[
            ("Standard delivery", "STD"),
            ("Express delivery", "EXP"),
            ("Express delivery", "EXP2"),
        ],
    )
    document.showPage()
    document.drawString(50, 785, "The same choice on the second page")
    document.showPage()
    document.drawString(50, 785, "This page must remain unchanged")
    document.save()

    for name, flags in [("multi", 1 << 17), ("readonly", (1 << 17) | 1),
                        ("editable", (1 << 17) | (1 << 18)), ("list", 0)]:
        writer = PdfWriter()
        writer.clone_document_from_reader(PdfReader(original))
        reference = writer.pages[0]["/Annots"][0]
        widget = reference.get_object()
        field_keys = ["/FT", "/Ff", "/T", "/V", "/DV", "/Opt", "/DA"]
        parent = DictionaryObject({NameObject(k): widget[k] for k in field_keys})
        parent[NameObject("/Ff")] = NumberObject(flags)
        parent_ref = writer._add_object(parent)
        for key in field_keys:
            widget.pop(key, None)
        widget[NameObject("/Parent")] = parent_ref
        sibling = DictionaryObject(widget)
        sibling[NameObject("/P")] = writer.pages[1].indirect_reference
        sibling_ref = writer._add_object(sibling)
        writer.pages[1][NameObject("/Annots")] = ArrayObject([sibling_ref])
        parent[NameObject("/Kids")] = ArrayObject([reference, sibling_ref])
        writer.root_object["/AcroForm"][NameObject("/Fields")] = ArrayObject([parent_ref])
        writer.write(directory / (name + ".pdf"))
    writer = PdfWriter()
    writer.clone_document_from_reader(PdfReader(original))
    sibling = DictionaryObject(writer.pages[0]["/Annots"][0].get_object())
    sibling[NameObject("/P")] = writer.pages[1].indirect_reference
    sibling_ref = writer._add_object(sibling)
    writer.pages[1][NameObject("/Annots")] = ArrayObject([sibling_ref])
    writer.root_object["/AcroForm"]["/Fields"].append(sibling_ref)
    writer.write(directory / "ambiguous.pdf")
    print(json.dumps({"created": [p.name for p in directory.glob("*.pdf")]}))


def inspect(path, expected):
    document = PdfReader(path)
    fields = document.get_fields()
    assert list(fields) == ["delivery"], fields
    assert fields["delivery"]["/V"] == expected, fields
    assert fields["delivery"]["/Opt"][2] == ["EXP2", "Express delivery"]
    widgets = []
    for page in document.pages:
        for reference in page.get("/Annots", []):
            widget = reference.get_object()
            if widget.get("/Subtype") != "/Widget":
                continue
            field = widget.get("/Parent", widget).get_object()
            assert widget.get("/V", field.get("/V")) == expected
            if "/Parent" in widget:
                assert reference in field["/Kids"]
                assert widget["/Parent"] == document.trailer["/Root"]["/AcroForm"]["/Fields"][0].get_object()
            appearance = widget["/AP"]["/N"].get_data()
            assert appearance
            if expected == "EXP2":
                assert b"Express delivery" in appearance
                assert b"Standard delivery" not in appearance
            widgets.append({"value": str(field["/V"]), "appearanceBytes": len(appearance)})
    assert len(widgets) == 2, widgets
    print(json.dumps({"canonicalValue": str(fields["delivery"]["/V"]), "widgets": widgets, "interactive": True}))


if sys.argv[1] == "create":
    create(sys.argv[2])
else:
    inspect(sys.argv[2], sys.argv[3])
