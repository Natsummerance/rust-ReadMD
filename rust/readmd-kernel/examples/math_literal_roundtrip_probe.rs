//! Dev-only synthetic, read-only M06 projection diagnosis. No engine or host.
use readmd_kernel::{math_document, math_formats, math_layout};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{fs, io::Write, path::Path};

fn sha(bytes: &[u8]) -> String { format!("{:x}", Sha256::digest(bytes)) }
fn create(file: &Path, bytes: &[u8]) {
    fs::OpenOptions::new().write(true).create_new(true).open(file).unwrap().write_all(bytes).unwrap();
}
fn texts(node: &math_document::XmlNode) -> String {
    if node.local() == "t" { return node.text(); }
    node.children().map(texts).collect()
}
fn paths(node: &math_document::XmlNode, out: &mut Vec<Value>) {
    if node.local() == "path" {
        out.push(json!({"transform":node.attributes.get("transform"),
            "outlineSha256":node.attributes.get("d").map(|d|sha(d.as_bytes()))}));
    }
    for child in node.children() { paths(child, out); }
}
fn main() {
    let args = std::env::args().collect::<Vec<_>>();
    assert_eq!(args.len(), 2, "Expected one new absolute owned output directory");
    let dir = Path::new(&args[1]); assert!(dir.is_absolute() && !dir.exists());
    let repo = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap().parent().unwrap();
    assert!(dir.starts_with(repo.join(".cache/capability-upgrade")));
    fs::create_dir(dir).unwrap();
    let mut records = Vec::new();
    for (style, property) in [("default", ""), ("p", "<m:sty m:val=\"p\"/>"),
        ("i", "<m:sty m:val=\"i\"/>"), ("b", "<m:sty m:val=\"b\"/>")] {
        let original = format!("<m:oMath xmlns:m=\"{}\"><m:r><m:rPr><m:lit/>{property}</m:rPr><m:t>sin +/=-{{}}%#&amp;_\\^$~</m:t></m:r></m:oMath>", math_document::MATH_NS);
        let doc = math_document::parse(&original, "omml", false).unwrap();
        let tex = math_formats::tex(&doc.root);
        let first = math_document::parse(&tex, "latex", false).unwrap();
        let again = math_formats::tex(&first.root);
        let second = math_document::parse(&again, "latex", false).unwrap();
        for (phase, source, parsed, dialect) in [("original", &original, &doc, "omml"),
            ("first", &tex, &first, "latex"), ("second", &again, &second, "latex")] {
            let name = format!("{style}-{phase}");
            let f = math_layout::layout_source(source, (dialect == "omml").then_some(source.as_str()), false, 12.).unwrap();
            let svg = f.interactive_svg(); let mut glyph_paths = Vec::new();
            paths(&math_document::parse_xml(&svg).unwrap(), &mut glyph_paths);
            create(&dir.join(format!("{name}.svg")), svg.as_bytes());
            create(&dir.join(format!("{name}.xml")), parsed.root.xml().as_bytes());
            records.push(json!({"style":style,"phase":phase,"source":source,"text":texts(&parsed.root),
                "width":f.width,"height":f.height,"depth":f.depth,"paths":glyph_paths}));
        }
    }
    let report = json!({"devOnly":true,"acceptance":false,"productHostInvocations":0,"records":records});
    let bytes = serde_json::to_vec_pretty(&report).unwrap(); create(&dir.join("report.json"), &bytes);
    println!("{}",json!({"devOnly":true,"acceptance":false,"records":12,"reportSha256":sha(&bytes)}));
}
