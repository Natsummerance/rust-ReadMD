"""Independent bounded native paragraph-style, current-OPC and geometry QA.

No ReadMD, GUI, engine or network invocation. Expected style aliases are derived
from actual native pStyle/basedOn graphs, never from a product alias report.
"""
import argparse
import copy
import importlib.util
import json
from pathlib import Path
import sys
import xml.etree.ElementTree as ET

_spec = importlib.util.spec_from_file_location("native_styles_unit_oracle", Path(__file__).with_name("inspect-math-unit-measures.py"))
unit = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(unit)
base, opc, logical = unit.base, unit.opc, unit.logical
W, M, NS, w, m = unit.W, unit.M, unit.NS, unit.w, unit.m
Package, parse_xml = unit.Package, unit.parse_xml
KINDS = ("a", "b", "fallback")
RESERVED = {"Normal", "BodyText", "ReadMDMath", "Heading1", "Heading2", "Heading3", "Heading4", "Heading5", "Heading6", "Title", "Subtitle", "Code", "Quote", "ListParagraph", "TableText", "Caption", "FootnoteText", "EndnoteText", "Header", "Footer", "TOCHeading", "TOC1", "TOC2", "TOC3", "TOC4", "TOC5", "TOC6"}


def canonical(node):
    if node is None:
        return None
    return {"name": node.tag, "attributes": dict(sorted(node.attrib.items())),
            "text": node.text or "", "children": [canonical(child) for child in node]}


def context(root, part):
    result = unit.context_from_styles(root, part)
    result["definitions"] = {}
    result["docDefaultsTree"] = None
    if root is not None:
        defaults = root.findall("w:docDefaults", NS)
        assert len(defaults) <= 1
        result["docDefaultsTree"] = canonical(defaults[0]) if defaults else None
        result["paragraphDefaultsPresent"] = bool(defaults and defaults[0].find("w:pPrDefault", NS) is not None)
        for node in root.findall("w:style", NS):
            identity = node.get(w("styleId"))
            result["definitions"][identity] = {"id": identity, "type": node.get(w("type")),
                "default": node.get(w("default")) in ("1", "true", "on"),
                "basedOn": result["styles"][identity]["basedOn"],
                "pPr": canonical(node.find("w:pPr", NS)), "tree": canonical(node)}
    else:
        result["paragraphDefaultsPresent"] = False
    return result


def inspect_package(package):
    result = unit.inspect_package(package)
    part = package.related("styles")
    result["styleContext"] = context(None if part is None else package.xml(part), part)
    body = package.xml(package.document).find("w:body", NS)
    result["bodyChildren"] = [canonical(node) for node in body]
    result["stylesTree"] = canonical(package.xml(part)) if part else None
    result["oracleScope"] = "actual native sparse pPr, selected graph, exact Fraction points and current members"
    return result


def inspect_fragment(raw):
    text = raw.decode("utf-8") if isinstance(raw, bytes) else raw
    root = parse_xml((f'<qa xmlns:w="{W}" xmlns:m="{M}">' + text + '</qa>').encode())
    styles, props = root.findall("w:styles", NS), root.findall("w:pPr", NS)
    assert len(styles) <= 1 and len(props) <= 1
    current = context(styles[0] if styles else None, "source-context" if styles else None)
    result = base.inspect_fragment(raw)
    result["direct"] = logical.declaration(props[0] if props else None)
    result["effective"] = unit.effective(current, result["direct"])
    result["styleContext"] = current
    return result


def fixture_parts(kind):
    assert kind in KINDS
    parts = unit.fixture_parts("right" if kind == "b" else "left")
    package = Package(parts=parts)
    document, style_part = package.xml(package.document), package.related("styles")
    body = document.find("w:body", NS)
    alignment = "right" if kind == "b" else "left"
    props = [f'<w:pStyle w:val="Equation"/><w:ind w:left="1mm" w:right="1cm"/><w:jc w:val="{alignment}"/>',
             '<w:pStyle w:val="Equation"/><w:ind/>' if kind == "b" else '<w:pStyle w:val="Equation"/><w:ind w:left="0"/>', ""]
    ordinal = 0
    for paragraph in body:
        if paragraph.tag != w("p"):
            continue
        for old in paragraph.findall("w:pPr", NS):
            paragraph.remove(old)
        if paragraph.find("m:oMathPara", NS) is not None:
            inside = props[ordinal]
            ordinal += 1
        else:
            inside = '<w:pStyle w:val="NativeBody"/>'
        paragraph.insert(0, parse_xml((f'<w:pPr xmlns:w="{W}">' + inside + '</w:pPr>').encode()))
    assert ordinal == 3
    defaults = "" if kind == "a" else ('<w:docDefaults><w:pPrDefault><w:pPr/></w:pPrDefault></w:docDefaults>' if kind == "b" else '<w:docDefaults><w:pPrDefault><w:pPr><w:ind w:left="2pc" w:right="1.5pi"/><w:jc w:val="left"/></w:pPr></w:pPrDefault></w:docDefaults>')
    base_id = "Normal" if kind == "b" else "Base"
    middle = '<w:style w:type="paragraph" w:styleId="ReadMDMath"><w:basedOn w:val="Normal"/><w:pPr><w:ind/></w:pPr></w:style>' if kind == "b" else ""
    leaf_parent = "ReadMDMath" if kind == "b" else base_id
    default_properties = "" if kind == "fallback" else f'<w:ind w:left="2pc" w:right="1.5pi"/><w:jc w:val="{alignment}"/>'
    styles = f'''<w:styles xmlns:w="{W}">{defaults}
<w:style w:type="paragraph" w:styleId="{base_id}"><w:pPr><w:ind w:left="0.5in" w:right="6pt"/><w:jc w:val="center"/></w:pPr></w:style>{middle}
<w:style w:type="paragraph" w:styleId="Equation"><w:basedOn w:val="{leaf_parent}"/><w:pPr><w:ind w:right="18.125pt"/><w:jc w:val="{alignment}"/></w:pPr></w:style>
<w:style w:type="paragraph" w:default="1" w:styleId="DefaultMath"><w:pPr>{default_properties}</w:pPr></w:style>
<w:style w:type="paragraph" w:styleId="NativeBody"><w:pPr><w:ind w:left="0" w:right="0"/><w:jc w:val="left"/></w:pPr></w:style></w:styles>'''
    parts[package.document], parts[style_part] = opc.xml_bytes(document), styles.encode()
    assert not any(name.startswith("customXml/") for name in parts)
    return parts


def selected_graph(inspection, row):
    definitions = inspection["styleContext"]["definitions"]
    identities = row["effective"]["styleChain"]
    result = []
    for identity in identities:
        assert identity in definitions
        result.append(definitions[identity])
    assert result and result[-1]["id"] == row["effective"]["selectedStyle"]
    return result


def mapped_provenance(value, mapping):
    return {axis: ("style:" + mapping[origin[6:]] if origin.startswith("style:") else origin)
            for axis, origin in value.items()}


def math_shape(node):
    name = node["name"].rsplit("}", 1)[-1]
    return {"name": name, "text": node.get("text", "") if name == "t" else "",
            "children": [math_shape(child) for child in node.get("children", [])
                         if child["name"].startswith("{" + M + "}") and not child["name"].rsplit("}", 1)[-1].endswith("Pr")]}


def compare_rows(source, native, native_rows=None, source_kind="source", aliases=None, uncontested=False):
    rows = native["paragraphs"] if native_rows is None else native_rows
    assert len(rows) == len(source["paragraphs"])
    mapping, bindings = {} if aliases is None else aliases, []
    for before, after in zip(source["paragraphs"], rows):
        original, generated = selected_graph(source, before), selected_graph(native, after)
        assert len(original) == len(generated), "Selected native chain length changed"
        local_map = {}
        for old, new in zip(original, generated):
            assert old["type"] == new["type"] == "paragraph"
            assert old["pPr"] == new["pPr"], "Selected native style sparse pPr changed"
            assert not new["default"], "Imported style replaced package paragraph default"
            key = source_kind + ":" + old["id"]
            assert key not in mapping or mapping[key] == new["id"], "One source style got multiple aliases"
            mapping[key], local_map[old["id"]] = new["id"], new["id"]
            if old["id"] in RESERVED:
                assert new["id"] != old["id"], "Template style collision was not aliased"
            elif uncontested:
                assert new["id"] == old["id"], "Uncontested native style ID was rewritten"
        for old, new in zip(original, generated):
            assert new["basedOn"] == (local_map[old["basedOn"]] if old["basedOn"] else None), "Native basedOn graph changed"
        expected_direct = copy.deepcopy(before["direct"])
        synthesized = not expected_direct["pStylePresent"]
        expected_direct.update({"pPrPresent": True, "pStylePresent": True, "pStyle": generated[-1]["id"]})
        assert after["direct"] == expected_direct, "Native direct pPr lost sparse/empty/zero/raw declarations"
        assert after["effective"]["axes"] == before["effective"]["axes"]
        assert after["effective"]["measures"] == before["effective"]["measures"]
        assert after["effective"]["provenance"] == mapped_provenance(before["effective"]["provenance"], local_map)
        assert [math_shape(node) for node in after["mathObjects"]] == [math_shape(node) for node in before["mathObjects"]], "Native formula content changed"
        assert after["runDeclarations"] == before["runDeclarations"], "Native explicit math fonts/size changed"
        assert after["mathTexts"] == ["ab"]
        assert after["effective"]["unsupported"] == []
        bindings.append({"ordinal": before["ordinal"], "sourceStyle": original[-1]["id"], "nativeStyle": generated[-1]["id"],
                         "bindingSynthesized": synthesized, "originalProvenance": before["effective"]["provenance"],
                         "nativeProvenance": after["effective"]["provenance"], "sparseAxesPreserved": True})
    return {"aliasMap": mapping, "bindings": bindings, "passed": True}


def compare_native(source_file, native_file, kind):
    source, native = inspect_package(Package(source_file)), inspect_package(Package(native_file))
    assert kind in {"a", "b"}
    result = compare_rows(source, native, source_kind=kind, uncontested=True)
    result.update({"sourceSha256": opc.digest(source_file.read_bytes()), "nativeSha256": opc.digest(native_file.read_bytes()),
                   "nativeSourceProvenanceCertified": True, "fullWordStylePackageCertified": False})
    return result


def compare_combined(a_file, b_file, native_file, reverse=False):
    a, b, native = inspect_package(Package(a_file)), inspect_package(Package(b_file)), inspect_package(Package(native_file))
    assert len(native["paragraphs"]) == 6
    sources = [("b", b), ("a", a)] if reverse else [("a", a), ("b", b)]
    mapping, bindings = {}, []
    for offset, (kind, source) in enumerate(sources):
        result = compare_rows(source, native, native["paragraphs"][offset * 3:(offset + 1) * 3], kind, mapping)
        bindings.extend(result["bindings"])
    for identity in ("Equation", "DefaultMath"):
        assert mapping["a:" + identity] != identity and mapping["b:" + identity] != identity
        assert mapping["a:" + identity] != mapping["b:" + identity], "Distinct source definitions merged"
    assert mapping["a:Base"] == "Base"
    return {"passed": True, "aliasMap": mapping, "bindings": bindings, "reverse": reverse,
            "sourceOrder": [kind for kind, _ in sources], "nativeSha256": opc.digest(native_file.read_bytes()),
            "nativeSourceProvenanceCertified": True, "fullWordStylePackageCertified": False}


def mutate_current(input_file, output_file, operation):
    input_sha = opc.digest(input_file.read_bytes())
    before_parts = Package(input_file).parts
    parts = copy.deepcopy(before_parts)
    package = Package(parts=parts)
    before = inspect_package(package)
    assert len(before["paragraphs"]) == 3 and operation in {"delete-direct-left", "style-right"}
    if operation == "delete-direct-left":
        part = package.document
        root = package.xml(part)
        paragraph = list(root.find("w:body", NS))[before["paragraphs"][1]["bodyIndex"]]
        ind = paragraph.find("w:pPr/w:ind", NS)
        assert ind is not None and ind.get(w("left")) == "0"
        del ind.attrib[w("left")]
    else:
        part = package.related("styles")
        root = package.xml(part)
        selected = before["paragraphs"][1]["effective"]["selectedStyle"]
        styles = [node for node in root.findall("w:style", NS) if node.get(w("styleId")) == selected]
        assert len(styles) == 1
        ind = styles[0].find("w:pPr/w:ind", NS)
        assert ind is not None and ind.get(w("right")) == "18.125pt"
        ind.set(w("right"), "1mm")
    parts[part] = opc.xml_bytes(root)
    after = inspect_package(Package(parts=parts))
    assert set(parts) == set(before_parts)
    changed = [name for name in parts if parts[name] != before_parts[name]]
    assert changed == [part], "Current-native mutation changed an unplanned member"
    expected = copy.deepcopy(before["bodyChildren"] if operation == "delete-direct-left" else before["styleContext"]["definitions"])
    if operation == "delete-direct-left":
        body_index = before["paragraphs"][1]["bodyIndex"]
        props = expected[body_index]["children"][0]
        inds = [node for node in props["children"] if node["name"] == w("ind")]
        assert len(inds) == 1
        del inds[0]["attributes"][w("left")]
        assert after["bodyChildren"] == expected
        assert after["styleContext"] == before["styleContext"]
    else:
        selected = before["paragraphs"][1]["effective"]["selectedStyle"]
        old_styles, new_styles = before["styleContext"]["definitions"], after["styleContext"]["definitions"]
        for identity in old_styles:
            if identity != selected:
                assert new_styles[identity] == old_styles[identity]
        assert after["bodyChildren"] == before["bodyChildren"]
        expected_style = copy.deepcopy(old_styles[selected])
        for field in ("pPr",):
            inds = [node for node in expected_style[field]["children"] if node["name"] == w("ind")]
            assert len(inds) == 1
            inds[0]["attributes"][w("right")] = "1mm"
        props = [node for node in expected_style["tree"]["children"] if node["name"] == w("pPr")]
        assert len(props) == 1
        inds = [node for node in props[0]["children"] if node["name"] == w("ind")]
        inds[0]["attributes"][w("right")] = "1mm"
        assert new_styles[selected] == expected_style
    for i in (0, 2):
        assert after["paragraphs"][i] == before["paragraphs"][i], "Current mutation affected a shadowed/adjacent formula"
    for i in range(3):
        assert after["paragraphs"][i]["mathObjects"] == before["paragraphs"][i]["mathObjects"]
    selected_after = after["paragraphs"][1]
    axis, wanted = ("left", "0.5in") if operation == "delete-direct-left" else ("right", "1mm")
    assert selected_after["effective"]["axes"][axis] == wanted
    assert selected_after["effective"]["provenance"][axis].startswith("style:")
    opc.write_package(output_file, parts)
    assert opc.digest(input_file.read_bytes()) == input_sha, "Current-native mutation rewrote the original"
    return {"operation": operation, "inputSha256": input_sha, "outputSha256": opc.digest(output_file.read_bytes()),
            "changedMembers": changed, "allOtherMembersByteIdentical": True, "addedMembers": [], "before": before,
            "inspection": after, "sourcePayloadUsedAsNativeEvidence": False, "oldAuxiliaryConflictCertified": False}


def compare_isolation(baseline_file, mixed_file, source_file):
    baseline, mixed, source = (inspect_package(Package(file)) for file in (baseline_file, mixed_file, source_file))
    assert len(baseline["paragraphs"]) == 1 and len(mixed["paragraphs"]) == 4
    native = compare_rows(source, mixed, mixed["paragraphs"][1:], "a", uncontested=True)
    added_ids = set(native["aliasMap"].values())
    old_defs, new_defs = baseline["styleContext"]["definitions"], mixed["styleContext"]["definitions"]
    assert set(new_defs) - set(old_defs) == added_ids
    for identity in old_defs:
        assert new_defs[identity] == old_defs[identity], "Ordinary template style changed"
    assert baseline["styleContext"]["docDefaultsTree"] == mixed["styleContext"]["docDefaultsTree"], "Ordinary docDefaults tree changed"
    assert baseline["styleContext"]["defaults"] == mixed["styleContext"]["defaults"], "Ordinary missing/explicit defaults changed"
    assert baseline["styleContext"]["default"] == mixed["styleContext"]["default"] == "Normal"
    assert baseline["styleContext"]["paragraphDefaultsPresent"] == mixed["styleContext"]["paragraphDefaultsPresent"]
    before = baseline["bodyChildren"]
    assert before[-1]["name"] == w("sectPr")
    assert mixed["bodyChildren"][-1] == before[-1], "Ordinary final sectPr changed"
    assert mixed["bodyChildren"][:len(before) - 1] == before[:-1], "Ordinary body/direct pPr/bare formula changed"
    a, b = baseline["paragraphs"][0], mixed["paragraphs"][0]
    for field in ("direct", "effective", "mathObjects", "mathTexts", "runDeclarations"):
        assert a[field] == b[field], "Bare formula inherited imported formatting"
    return {"passed": True, "ordinaryTemplateDefinitionsByteSemanticEqual": True, "ordinaryBodyAndBareFormulaEqual": True,
            "completeDocDefaultsEqual": True, "finalSectPrEqual": True,
            "missingAndExplicitZeroNotNormalized": True, "addedImportedStyles": sorted(added_ids), "bindings": native["bindings"]}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="action", required=True)
    fixture = sub.add_parser("fixture")
    fixture.add_argument("--kind", choices=KINDS, required=True)
    fixture.add_argument("--output", type=Path, required=True)
    for action in ("inspect", "fragment", "mathml", "flow", "epub", "pdf", "svg-ink"):
        sub.add_parser(action).add_argument("input", type=Path)
    compare = sub.add_parser("compare")
    compare.add_argument("--source", type=Path, required=True)
    compare.add_argument("--native", type=Path, required=True)
    compare.add_argument("--kind", choices=("a", "b"), required=True)
    combined = sub.add_parser("combined")
    combined.add_argument("--a", type=Path, required=True)
    combined.add_argument("--b", type=Path, required=True)
    combined.add_argument("--native", type=Path, required=True)
    combined.add_argument("--reverse", action="store_true")
    mutation = sub.add_parser("mutate-current")
    mutation.add_argument("--input", type=Path, required=True)
    mutation.add_argument("--output", type=Path, required=True)
    mutation.add_argument("--operation", choices=("delete-direct-left", "style-right"), required=True)
    isolation = sub.add_parser("isolation")
    isolation.add_argument("--baseline", type=Path, required=True)
    isolation.add_argument("--mixed", type=Path, required=True)
    isolation.add_argument("--source", type=Path, required=True)
    args = parser.parse_args()
    if args.action == "fixture":
        opc.write_package(args.output, fixture_parts(args.kind))
        result = {"kind": args.kind, "nativeOnly": True, "sha256": opc.digest(args.output.read_bytes()), "inspection": inspect_package(Package(args.output))}
    elif args.action == "compare":
        result = compare_native(args.source, args.native, args.kind)
    elif args.action == "combined":
        result = compare_combined(args.a, args.b, args.native, args.reverse)
    elif args.action == "mutate-current":
        result = mutate_current(args.input, args.output, args.operation)
    elif args.action == "isolation":
        result = compare_isolation(args.baseline, args.mixed, args.source)
    elif args.action == "inspect":
        result = inspect_package(Package(args.input))
    elif args.action == "fragment":
        result = inspect_fragment(args.input.read_bytes())
    else:
        if args.action in {"mathml", "svg-ink"}:
            parse_xml(args.input.read_bytes())
        function = {"mathml": logical.inspect_mathml, "flow": logical.inspect_flow, "epub": logical.inspect_epub,
                    "pdf": base.inspect_pdf, "svg-ink": unit.svg_ink}[args.action]
        result = function(args.input if args.action in {"epub", "pdf"} else args.input.read_bytes())
    print(json.dumps(result, ensure_ascii=False, separators=(",", ":")))


if __name__ == "__main__":
    sys.stdout.reconfigure(encoding="utf-8")
    main()
