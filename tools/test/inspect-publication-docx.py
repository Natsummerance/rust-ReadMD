"""Independent test-only OOXML inspection; the application never uses Python."""
import json
import sys
import xml.etree.ElementTree as ET
import zipfile

sys.stdout.reconfigure(encoding="utf-8")
ns = {"w": "http://schemas.openxmlformats.org/wordprocessingml/2006/main"}
w = "{" + ns["w"] + "}"
with zipfile.ZipFile(sys.argv[1]) as package:
    document = ET.fromstring(package.read("word/document.xml"))
    sections = []
    for section in document.findall(".//w:sectPr", ns):
        numbering = section.find("w:pgNumType", ns)
        sections.append({
            "Columns": section.find("w:cols", ns).get(w + "num"),
            "Start": numbering.get(w + "start", ""),
            "Format": numbering.get(w + "fmt"),
            "FirstPage": section.find("w:titlePg", ns) is not None,
            "Margins": ET.tostring(section.find("w:pgMar", ns), encoding="unicode"),
        })
    parts = {}
    local = []
    for paragraph in document.findall(".//w:body/w:p", ns):
        props = paragraph.find("w:pPr", ns)
        if props is not None and props.find("w:ind", ns) is not None:
            local.append({
                "Text": "".join(paragraph.itertext()),
                "Properties": ET.tostring(props, encoding="unicode"),
                "Runs": [ET.tostring(p, encoding="unicode") for p in paragraph.findall(".//w:rPr", ns)],
            })
    for name, key in [("styles", "Styles"), ("footer1", "Footer"), ("settings", "Settings")]:
        if "word/" + name + ".xml" not in package.namelist():
            if name != "footer1":
                raise ValueError("Required Word part is missing: " + name)
            parts[key] = ""
            continue
        data = package.read("word/" + name + ".xml").decode("utf-8")
        ET.fromstring(data)
        parts[key] = data
    print(json.dumps({"Sections": sections, "LocalParagraphs": local, **parts}, ensure_ascii=False))
