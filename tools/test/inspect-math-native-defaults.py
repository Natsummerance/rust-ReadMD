"""Independent native rPrDefault continuation oracle (ZIP/XML, no product code).

This deliberately does not relax the earlier Missing/Empty-source C68 oracle.
The finite non-four-axis domain is actual rFonts/lang/szCs, checked as complete
merged properties. Application interoperability remains an independent gate.
"""
import argparse
import copy
import importlib.util
import json
from pathlib import Path
import sys
import xml.etree.ElementTree as ET

_spec = importlib.util.spec_from_file_location("native_defaults_base", Path(__file__).with_name("inspect-math-native-runs.py"))
base = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(base)
w, m, one, canonical, opc = base.w, base.m, base.one, base.canonical, base.opc
KINDS = ("original", "inherited", "no-character", "lang-mismatch", "szcs-mismatch")
FONT = "Arial"


def context(package):
    part = package.related("styles", optional=False)
    return base.Context(package.xml(part), part)


def full_properties(ctx, paragraph, site):
    chain = [ctx.definitions[identity] for identity in paragraph["paragraphChain"]]
    extras = base.finite_other_effective(ctx, chain, site["direct"])
    return {"four": site["effective"]["axes"], "other": extras}


def inspect(package):
    result, ctx = base.inspect_package(package), context(package)
    result["rPrDefaultTree"] = canonical(one(one(ctx.root, w("docDefaults")), w("rPrDefault")))
    assert not result["auxiliaryParts"], "A private payload cannot be native default evidence"
    assert not result["themeParts"], "This finite oracle does not qualify an unresolved theme"
    for paragraph in result["paragraphs"]:
        for site in paragraph["sites"]:
            site["fullEffective"] = full_properties(ctx, paragraph, site)
            site["automaticSelectedId"] = None if site["direct"]["rStylePresent"] else ctx.character_default
    result["finiteGlobalProperties"] = ["b", "i", "sz", "color", "rFonts", "lang", "szCs"]
    result["allOfficeGlobalPropertiesCertified"] = False
    return result


def write_owned(input_file, output_file, original, parts):
    before = base.Package(input_file)
    assert set(parts) == set(before.parts), "Fixture changed OPC member inventory"
    assert not output_file.exists() and not output_file.is_symlink()
    opc.write_package(output_file, parts)
    assert base.Package(output_file).parts == parts
    assert input_file.read_bytes() == original
    return {"inputSha256": opc.digest(original), "outputSha256": opc.digest(output_file.read_bytes()),
            "changedMembers": [name for name in parts if parts[name] != before.parts[name]],
            "allOtherMembersByteIdentical": True, "sourceFileUnchanged": True}


def fixture(input_file, output_file, kind):
    assert kind in KINDS
    original = input_file.read_bytes()
    package = base.Package(input_file)
    before = inspect(package)
    assert len(before["paragraphs"]) == 3
    assert before["styles"]["runDefaults"]["rPrPresent"]
    assert not before["styles"]["characterDefault"]
    parts = dict(package.parts)
    authored_changes = []
    global_author_choices = []
    if kind != "original":
        styles = package.xml(package.related("styles", optional=False))
        defaults = one(one(one(styles, w("docDefaults")), w("rPrDefault")), w("rPr"))
        fonts, lang, size = one(defaults, w("rFonts")), one(defaults, w("lang")), one(defaults, w("szCs"))
        assert fonts is not None and lang is not None and size is not None
        assert set(fonts.attrib) == {w(axis) for axis in ("ascii", "hAnsi", "eastAsia", "cs")}
        assert all(fonts.get(w(axis)) == FONT for axis in ("ascii", "hAnsi", "eastAsia", "cs")), "Inheritance fixture requires the actual Arial package defaults; do not rewrite an incompatible source"
        global_author_choices.append({"property": "rFonts", "choice": FONT,
                                      "action": "retain-actual-global", "before": canonical(fonts),
                                      "after": canonical(fonts), "consumerPresetFont": FONT})
        original_defaults = {"lang": copy.deepcopy(lang), "szCs": copy.deepcopy(size)}
        if kind == "lang-mismatch":
            assert lang.get(w("val")) != "fr-FR"
            lang.set(w("val"), "fr-FR")
        if kind == "szcs-mismatch":
            assert size.get(w("val")) != "28"
            size.set(w("val"), "28")
        document = package.xml(package.document)
        order = [w(name) for name in ("rStyle", "rFonts", "b", "i", "color", "sz", "szCs", "lang")]
        for paragraph in before["paragraphs"]:
            selected = list(one(document, w("body")))[paragraph["bodyIndex"]]
            sites = [node for node in selected.iter() if node.tag in (m("r"), m("ctrlPr"))]
            assert len(sites) == 3
            for ordinal, site in enumerate(sites):
                pr = one(site, w("rPr"))
                assert pr is not None
                if paragraph["ordinal"] == 0:
                    for tag in ("rFonts", "lang", "szCs"):
                        value = one(pr, w(tag))
                        if value is not None:
                            authored_changes.append({"paragraph": 0, "site": ordinal, "property": tag,
                                                     "action": "remove-actual-direct", "before": canonical(value)})
                            pr.remove(value)
                        else:
                            authored_changes.append({"paragraph": 0, "site": ordinal, "property": tag,
                                                     "action": "retain-already-inherited"})
                    if kind == "no-character":
                        selected_style = one(pr, w("rStyle"))
                        assert selected_style is not None
                        pr.remove(selected_style)
                        authored_changes.append({"paragraph": 0, "site": ordinal, "property": "rStyle",
                                                 "action": "remove-actual-direct", "before": canonical(selected_style)})
                else:
                    for tag, value in original_defaults.items():
                        if one(pr, w(tag)) is None:
                            pr.append(copy.deepcopy(value))
                            authored_changes.append({"paragraph": paragraph["ordinal"], "site": ordinal,
                                                     "property": tag, "action": "author-new-direct-shadow",
                                                     "after": canonical(value)})
                assert all(child.tag in order for child in pr), "Finite fixture encountered an unproved direct property"
                pr[:] = sorted(pr, key=lambda child: order.index(child.tag))
        parts[package.document] = opc.xml_bytes(document)
        parts[package.related("styles")] = opc.xml_bytes(styles)
    after = inspect(base.Package(parts=parts))
    if kind != "original":
        for paragraph in after["paragraphs"]:
            for site in paragraph["sites"]:
                extras = {entry["name"] for entry in site["direct"]["otherProperties"]}
                for tag in ("rFonts", "lang", "szCs"):
                    assert (w(tag) in extras) == (paragraph["ordinal"] != 0)
                if kind == "no-character" and paragraph["ordinal"] == 0:
                    assert not site["direct"]["rStylePresent"] and not site["characterChain"]
    receipt = write_owned(input_file, output_file, original, parts)
    return {"passed": True, "kind": kind, "authoredNativeFixture": True,
            "derivedFromActualProductNative": True, "privatePayloadUsed": False,
            "intentionalSourceInheritance": kind != "original", "explicitFixtureOperations": authored_changes,
            "globalAuthorChoices": global_author_choices,
            **receipt, "inspection": after}


def compare(source_file, native_file, materialized=False):
    source, result = inspect(base.Package(source_file)), inspect(base.Package(native_file))
    assert source["paragraphs"] and len(source["paragraphs"]) == len(result["paragraphs"])
    assert source["mathPr"] == result["mathPr"], "Math document defaults changed"
    complete_equal = source["rPrDefaultTree"] == result["rPrDefaultTree"]
    if not materialized:
        assert complete_equal, "Full real rPrDefault differs, even if four present axes coincide"
        assert source["styles"]["characterDefault"] == result["styles"]["characterDefault"], "Automatic character default selection changed"
    aliases, owners, bindings = {}, {}, []
    for left, right in zip(source["paragraphs"], result["paragraphs"]):
        assert left["mathShape"] == right["mathShape"], "Math structure changed"
        assert len(left["sites"]) == len(right["sites"])
        mapping = {}
        if not materialized:
            mapping.update(base.bind_chain(source, result, left["paragraphChain"], right["paragraphChain"], "paragraph", "global", aliases, owners))
            for old, new in zip(left["sites"], right["sites"]):
                if old["characterChain"] or new["characterChain"]:
                    assert old["direct"]["rStylePresent"] and new["direct"]["rStylePresent"], "Unqualified automatic default alias"
                    mapping.update(base.bind_chain(source, result, old["characterChain"], new["characterChain"], "character", "global", aliases, owners))
            assert right["pPr"] == base.mapped_tree(left["pPr"], mapping)
        for old, new in zip(left["sites"], right["sites"]):
            assert (old["kind"], old["path"], old["text"]) == (new["kind"], new["path"], new["text"]), "Run/control inventory changed"
            assert old["fullEffective"] == new["fullEffective"], "A real effective Word property changed"
            if not materialized:
                assert new["direct"]["tree"] == base.mapped_tree(old["direct"]["tree"], mapping), "Authored sparse direct property was added, removed or changed"
                assert old["automaticSelectedId"] == new["automaticSelectedId"]
                for axis, origin in old["effective"]["origins"].items():
                    if origin.startswith(("paragraph:", "character:")):
                        kind, identity = origin.split(":", 1)
                        origin = kind + ":" + mapping[identity]
                    assert origin == new["effective"]["origins"][axis], "Four-axis provenance changed"
            bindings.append({"paragraph": left["ordinal"], "ordinal": old["ordinal"], "role": old["kind"],
                             "sourceDirectProperties": old["direct"]["tree"], "nativeDirectProperties": new["direct"]["tree"],
                             "sourceEffectiveProperties": old["fullEffective"], "nativeEffectiveProperties": new["fullEffective"],
                             "sourceGlobalState": "explicit" if source["styles"]["runDefaults"]["tree"]["children"] else "empty",
                             "automaticSelectedId": new["automaticSelectedId"], "automaticChain": new["characterChain"]})
    return {"passed": True, "nativeInheritanceCertified": not materialized,
            "materializedConsumptionCertified": materialized, "completeRprDefaultEqual": complete_equal,
            "sourceSha256": opc.digest(source_file.read_bytes()), "nativeSha256": opc.digest(native_file.read_bytes()),
            "bindings": bindings, "aliasMap": aliases, "privatePayloadUsed": False,
            "allOfficeGlobalPropertiesCertified": False, "wordApplicationCertified": False}


def mutate(input_file, output_file, operation):
    assert operation in ("lang", "szCs", "rFonts")
    original = input_file.read_bytes()
    package = base.Package(input_file)
    before = inspect(package)
    parts, member = dict(package.parts), package.related("styles", optional=False)
    styles = package.xml(member)
    default = one(one(one(styles, w("docDefaults")), w("rPrDefault")), w("rPr"))
    target = one(default, w(operation))
    assert target is not None
    if operation == "rFonts":
        assert all(target.get(w(axis)) == FONT for axis in ("ascii", "hAnsi", "eastAsia", "cs")), "Current font mutation requires actual Arial defaults"
        for axis in ("ascii", "hAnsi", "eastAsia", "cs"):
            target.set(w(axis), "DejaVu Sans")
    else:
        value = "de-DE" if operation == "lang" else "30"
        assert target.get(w("val")) != value
        target.set(w("val"), value)
    parts[member] = opc.xml_bytes(styles)
    after = inspect(base.Package(parts=parts))
    assert after["bodyChildren"] == before["bodyChildren"], "Global-only edit changed document XML"
    assert after["styles"]["definitions"] == before["styles"]["definitions"], "Global-only edit changed a style"
    assert after["mathPr"] == before["mathPr"] and after["sectPr"] == before["sectPr"]
    changed, shadowed = [], []
    for old_p, new_p in zip(before["paragraphs"], after["paragraphs"]):
        for old, new in zip(old_p["sites"], new_p["sites"]):
            assert old["direct"] == new["direct"]
            assert old["fullEffective"]["four"] == new["fullEffective"]["four"]
            old_props, new_props = old["fullEffective"]["other"], new["fullEffective"]["other"]
            assert {key: value for key, value in old_props.items() if key != w(operation)} == {key: value for key, value in new_props.items() if key != w(operation)}
            direct = any(entry["name"] == w(operation) for entry in old["direct"]["otherProperties"])
            if not direct:
                assert old_props[w(operation)] != new_props[w(operation)], "Real global inheritor did not follow native default edit"
                changed.append({"paragraph": old_p["ordinal"], "ordinal": old["ordinal"], "role": old["kind"]})
            else:
                assert old_props == new_props, "Authored direct property did not shadow the changed global"
                shadowed.append({"paragraph": old_p["ordinal"], "ordinal": old["ordinal"], "role": old["kind"]})
    assert changed and shadowed, "Mutation requires both real inheritors and real direct shadows"
    receipt = write_owned(input_file, output_file, original, parts)
    assert receipt["changedMembers"] == [member]
    return {"passed": True, "operation": operation, **receipt, "changedSites": changed,
            "shadowedSites": shadowed, "actualCurrentNativeDefaultsWon": True,
            "sourcePayloadUsed": False, "wordApplicationCertified": False, "inspection": after}


def isolation(baseline_file, mixed_file):
    baseline, mixed = inspect(base.Package(baseline_file)), inspect(base.Package(mixed_file))
    prefix = baseline["bodyChildren"][:-1]
    assert mixed["bodyChildren"][:len(prefix)] == prefix, "Imported context polluted pre-existing ordinary/bare-math body"
    assert baseline["styles"]["docDefaultsTree"] == mixed["styles"]["docDefaultsTree"]
    for identity, style in baseline["styles"]["definitions"].items():
        assert style == mixed["styles"]["definitions"][identity], "Imported context changed a template style"
    for key in ("settingsTree", "themeParts", "sectPr"):
        assert baseline[key] == mixed[key], "Imported context polluted " + key
    assert mixed["ordinaryRuns"][:len(baseline["ordinaryRuns"])] == baseline["ordinaryRuns"]
    return {"passed": True, "ordinaryBodyAndBareMathUnchanged": True, "completeTemplateGlobalsUnchanged": True,
            "baselineSha256": opc.digest(baseline_file.read_bytes()), "mixedSha256": opc.digest(mixed_file.read_bytes()),
            "sourcePayloadUsed": False, "wordApplicationCertified": False}


def main():
    parser = argparse.ArgumentParser()
    commands = parser.add_subparsers(dest="command", required=True)
    commands.add_parser("inspect").add_argument("input", type=Path)
    make = commands.add_parser("fixture")
    make.add_argument("--input", type=Path, required=True)
    make.add_argument("--output", type=Path, required=True)
    make.add_argument("--kind", choices=KINDS, required=True)
    for name in ("compare", "materialized"):
        item = commands.add_parser(name)
        item.add_argument("--source", type=Path, required=True)
        item.add_argument("--native", type=Path, required=True)
    change = commands.add_parser("mutate")
    change.add_argument("--input", type=Path, required=True)
    change.add_argument("--output", type=Path, required=True)
    change.add_argument("--operation", choices=("lang", "szCs", "rFonts"), required=True)
    isolated = commands.add_parser("isolation")
    isolated.add_argument("--baseline", type=Path, required=True)
    isolated.add_argument("--mixed", type=Path, required=True)
    args = parser.parse_args()
    if args.command == "inspect":
        result = inspect(base.Package(args.input))
    elif args.command == "fixture":
        result = fixture(args.input, args.output, args.kind)
    elif args.command in ("compare", "materialized"):
        result = compare(args.source, args.native, args.command == "materialized")
    elif args.command == "mutate":
        result = mutate(args.input, args.output, args.operation)
    else:
        result = isolation(args.baseline, args.mixed)
    print(json.dumps(result, ensure_ascii=False))


if __name__ == "__main__":
    if hasattr(sys.stdout, "reconfigure"):
        sys.stdout.reconfigure(encoding="utf-8")
    main()
