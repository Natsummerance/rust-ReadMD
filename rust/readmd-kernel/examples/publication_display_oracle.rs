//! Explicit private official RESULT calibration. No fixture is embedded or
//! redistributed, no native workflow is started, and this is not format QA.
use readmd_kernel::publication_rich::*;
use serde_json::json;
use sha2::{Digest, Sha256};
use std::{io::Write, path::Path};

#[derive(Clone, Debug, PartialEq, Eq)]
enum OracleToken {
    Text(String, RichFormat),
    Start(RichDisplay),
    End(RichDisplay),
}
fn oracle_text(output: &mut Vec<OracleToken>, text: &str, format: &RichFormat) {
    if text.is_empty() {
        return;
    }
    if let Some(OracleToken::Text(previous, previous_format)) = output.last_mut() {
        if previous_format == format {
            previous.push_str(text);
            return;
        }
    }
    output.push(OracleToken::Text(text.into(), format.clone()));
}
fn expected_result_tokens(result: &str) -> Vec<Vec<OracleToken>> {
    use quick_xml::{events::Event, reader::Reader};
    let mut reader = Reader::from_str(result);
    let mut stack: Vec<(String, RichFormat, Option<RichDisplay>)> = Vec::new();
    let mut entries: Vec<Vec<OracleToken>> = Vec::new();
    let mut in_entry = false;
    loop {
        match reader
            .read_event()
            .expect("independent RESULT must be valid restricted HTML/XML")
        {
            Event::Start(start) => {
                let name = start.local_name().as_ref().to_string();
                let mut format = stack
                    .last()
                    .map(|frame| frame.1.clone())
                    .unwrap_or_default();
                let mut class = None;
                for attribute in start.attributes() {
                    let attribute = attribute.unwrap();
                    assert_eq!(
                        attribute.key.as_ref(),
                        "class",
                        "unreviewed RESULT attribute"
                    );
                    class = Some(
                        attribute
                            .normalized_value(quick_xml::XmlVersion::Implicit1_0)
                            .unwrap()
                            .into_owned(),
                    );
                }
                let display = match (name.as_str(), class.as_deref()) {
                    ("div", Some("csl-bib-body")) => None,
                    ("div", Some("csl-entry")) => {
                        assert!(!in_entry);
                        in_entry = true;
                        entries.push(Vec::new());
                        None
                    }
                    ("div", Some("csl-block")) => Some(RichDisplay::Block),
                    ("div", Some("csl-indent")) => Some(RichDisplay::Indent),
                    ("div", Some("csl-left-margin")) => Some(RichDisplay::LeftMargin),
                    ("div", Some("csl-right-inline")) => Some(RichDisplay::RightInline),
                    ("i" | "em", None) => {
                        format.font_style = RichFontStyle::Italic;
                        None
                    }
                    _ => panic!("unreviewed independent RESULT element {name}/{class:?}"),
                };
                if let Some(display) = display {
                    entries
                        .last_mut()
                        .unwrap()
                        .push(OracleToken::Start(display));
                }
                // Retain the entry marker only in this parser's frame name.
                let frame_name = if class.as_deref() == Some("csl-entry") {
                    "entry".into()
                } else {
                    name
                };
                stack.push((frame_name, format, display));
            }
            Event::End(_) => {
                let (name, _, display) = stack.pop().expect("balanced independent RESULT");
                if let Some(display) = display {
                    entries.last_mut().unwrap().push(OracleToken::End(display));
                }
                if name == "entry" {
                    in_entry = false;
                }
            }
            Event::Text(text) => {
                let value = text.as_ref();
                // Only HTML source indentation between structural divs is
                // irrelevant. Inline punctuation and all other whitespace stay exact.
                if value.trim().is_empty()
                    && value.contains('\n')
                    && stack.last().is_some_and(|frame| {
                        frame.0 == "entry" || (frame.0 == "div" && frame.2.is_none())
                    })
                {
                    continue;
                }
                if in_entry {
                    oracle_text(entries.last_mut().unwrap(), value, &stack.last().unwrap().1);
                } else {
                    assert!(value.trim().is_empty());
                }
            }
            Event::GeneralRef(reference) => {
                let value = reference.as_ref();
                let c = match value {
                    "amp" => '&',
                    "lt" => '<',
                    "gt" => '>',
                    "quot" => '"',
                    "apos" => '\'',
                    _ if value.starts_with("#x") => {
                        char::from_u32(u32::from_str_radix(&value[2..], 16).unwrap()).unwrap()
                    }
                    _ if value.starts_with('#') => {
                        char::from_u32(value[1..].parse().unwrap()).unwrap()
                    }
                    _ => panic!("unreviewed independent RESULT entity"),
                };
                assert!(in_entry);
                oracle_text(
                    entries.last_mut().unwrap(),
                    &c.to_string(),
                    &stack.last().unwrap().1,
                );
            }
            Event::Eof => break,
            _ => panic!("unreviewed independent RESULT event"),
        }
    }
    assert!(stack.is_empty() && !in_entry);
    entries
}
fn actual_result_tokens(flow: &[RichLayoutNode]) -> Vec<OracleToken> {
    fn visit(flow: &[RichLayoutNode], output: &mut Vec<OracleToken>) {
        for node in flow {
            match node {
                RichLayoutNode::Inline { children } => {
                    for child in children {
                        let RichNode::Text { text, format } = child else {
                            panic!("display projection leaked an inline container")
                        };
                        oracle_text(output, text, format);
                    }
                }
                RichLayoutNode::Group { children, .. } => visit(children, output),
                RichLayoutNode::Link { .. } => {
                    panic!("official display fixture hyperlink was not reviewed")
                }
                RichLayoutNode::Block { children, indent } => {
                    let display = if *indent {
                        RichDisplay::Indent
                    } else {
                        RichDisplay::Block
                    };
                    output.push(OracleToken::Start(display));
                    visit(children, output);
                    output.push(OracleToken::End(display));
                }
                RichLayoutNode::Columns { left, right } => {
                    output.push(OracleToken::Start(RichDisplay::LeftMargin));
                    visit(left, output);
                    output.push(OracleToken::End(RichDisplay::LeftMargin));
                    if right
                        .iter()
                        .any(|node| matches!(node, RichLayoutNode::Block { indent: false, .. }))
                    {
                        for node in right {
                            if let RichLayoutNode::Block {
                                children,
                                indent: false,
                            } = node
                            {
                                output.push(OracleToken::Start(RichDisplay::RightInline));
                                visit(children, output);
                                output.push(OracleToken::End(RichDisplay::RightInline));
                            } else {
                                visit(std::slice::from_ref(node), output);
                            }
                        }
                    } else {
                        output.push(OracleToken::Start(RichDisplay::RightInline));
                        visit(right, output);
                        output.push(OracleToken::End(RichDisplay::RightInline));
                    }
                }
            }
        }
    }
    let mut output = Vec::new();
    visit(flow, &mut output);
    output
}

fn section<'a>(source: &'a str, name: &str) -> Option<&'a str> {
    let pattern = format!(r"(?s)>>=+ {name} =+>>\r?\n(.*?)\r?\n<<=+ {name} =+<<");
    regex::Regex::new(&pattern)
        .unwrap()
        .captures(source)
        .map(|capture| capture.get(1).unwrap().as_str())
}
fn main() {
    let args: Vec<_> = std::env::args().collect();
    assert_eq!(args.len(), 3, "usage: publication_display_oracle <absolute private index.json> <new absolute private report.json>");
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap();
    let private_root = workspace.join(".cache/private").canonicalize().unwrap();
    let index_path = Path::new(&args[1]);
    let report_path = Path::new(&args[2]);
    assert!(index_path.is_absolute() && report_path.is_absolute());
    assert!(index_path
        .canonicalize()
        .unwrap()
        .starts_with(&private_root));
    assert!(report_path
        .parent()
        .unwrap()
        .canonicalize()
        .unwrap()
        .starts_with(&private_root));
    assert!(!report_path.exists(), "preserve every previous report");
    let index_bytes = std::fs::read(index_path).unwrap();
    let index_sha = format!("{:x}", Sha256::digest(&index_bytes));
    assert_eq!(
        index_sha,
        "8772285e403e85e51ef708217d225f72732712fd0c5e0c973289bb59796c4fe4"
    );
    let index: serde_json::Value = serde_json::from_slice(&index_bytes).unwrap();
    let fixtures = index["fixtures"].as_array().unwrap();
    assert_eq!(fixtures.len(), 5);
    let old_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    let mut cases = Vec::new();
    for fixture in fixtures {
        let mut compared = false;
        let mut result_sha256 = None;
        let started = std::time::Instant::now();
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            assert_eq!(fixture["repository"], "citation-style-language/test-suite");
            assert_eq!(
                fixture["commit"],
                "6eefc5b07c6969ab8999e48542acbcc131cba864"
            );
            let relative = fixture["localPath"].as_str().unwrap();
            assert!(
                relative.starts_with(".cache/private/publication-context-inputs/display-20261015/")
                    && !relative.contains("..")
            );
            let source = std::fs::read_to_string(workspace.join(relative)).unwrap();
            assert_eq!(source.len() as u64, fixture["bytes"].as_u64().unwrap());
            assert_eq!(
                readmd_kernel::document_transaction::revision(&source),
                fixture["sha256"]
            );
            // Acquisition independently checked each fixed tree Git blob. This
            // test binds that whole index and rechecks the exact source SHA256.
            assert_eq!(fixture["gitBlobSha1"].as_str().unwrap().len(), 40);
            assert_eq!(section(&source, "MODE"), Some("bibliography"));
            result_sha256 = Some(format!(
                "{:x}",
                Sha256::digest(section(&source, "RESULT").unwrap().as_bytes())
            ));
            let input: serde_json::Value =
                serde_json::from_str(section(&source, "INPUT").unwrap()).unwrap();
            let clusters: serde_json::Value =
                if let Some(items) = section(&source, "CITATION-ITEMS") {
                    serde_json::from_str(items).unwrap()
                } else {
                    json!([input
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|item| json!({"id":item["id"]}))
                        .collect::<Vec<_>>()])
                };
            let request = readmd_kernel::academic::parse_request(json!({"format":"csl-json","data":section(&source,"INPUT").unwrap(),"csl":section(&source,"CSL").unwrap(),"clusters":clusters})).unwrap();
            let response = readmd_kernel::academic::citations(&request).unwrap();
            let actual: Vec<_> = response["bibliography"]["items"]
                .as_array()
                .unwrap()
                .iter()
                .map(|item| {
                    let flow: Vec<RichLayoutNode> =
                        serde_json::from_value(item["layout"].clone()).unwrap();
                    actual_result_tokens(&flow)
                })
                .collect();
            let expected = expected_result_tokens(section(&source, "RESULT").unwrap());
            compared = true;
            assert_eq!(
                actual, expected,
                "fixed independent official RESULT: {}",
                fixture["path"]
            );
        }));
        let failure = outcome.err().map(|error| {
            error
                .downcast_ref::<String>()
                .cloned()
                .or_else(|| error.downcast_ref::<&str>().map(|value| value.to_string()))
                .unwrap_or_else(|| "non-string QA failure".into())
        });
        let passed = failure.is_none();
        cases.push(json!({"fixture":fixture["path"],"sha256":fixture["sha256"],"resultSha256":result_sha256,
            "compared":compared,"passed":passed,"elapsedMs":started.elapsed().as_millis(),"failure":failure}));
        println!(
            "{}: {}",
            fixture["path"],
            if passed {
                "passed"
            } else {
                "failed; details private"
            }
        );
    }
    std::panic::set_hook(old_hook);
    let passed = cases.iter().filter(|case| case["passed"] == true).count();
    let example_path = workspace.join("rust/readmd-kernel/examples/publication_display_oracle.rs");
    let executable = std::env::current_exe().unwrap();
    let report = json!({"schemaVersion":1,"scope":"independent-original-CSL-display-RESULT-structure",
        "devOnly":true,"processorInvokedInProcess":true,"nativeWorkflowExecuted":false,"fullFormatAcceptance":false,
        "indexSha256":index_sha,"sourceSha256":format!("{:x}",Sha256::digest(std::fs::read(example_path).unwrap())),
        "executableSha256":format!("{:x}",Sha256::digest(std::fs::read(executable).unwrap())),
        "mandatoryFixtures":5,"attemptedFixtures":cases.len(),"actualComparedFixtures":cases.iter().filter(|case|case["compared"]==true).count(),"passedFixtures":passed,
        "failedFixtures":cases.len()-passed,"skippedFixtures":0,"passed":passed==5,"cases":cases,
        "privateOnly":"Unknown-license original RESULTs and any failure detail must not be redistributed."});
    let bytes = serde_json::to_vec_pretty(&report).unwrap();
    let mut output = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(report_path)
        .unwrap();
    output.write_all(&bytes).unwrap();
    output.write_all(b"\n").unwrap();
    println!("official display RESULT: {passed}/5; skipped 0; native/format acceptance false");
    if passed != 5 {
        std::process::exit(1);
    }
}
