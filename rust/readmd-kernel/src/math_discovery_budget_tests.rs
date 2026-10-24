use super::{discover_markdown, markdown_ranges, DiscoveryWork};
use std::sync::{atomic::{AtomicUsize, Ordering}, Arc};

fn unfinished_candidates() -> String {
    format!("A <span data-readmd-text=\"v1\" data-size-pt=\"30\">ok</span> {}", ":math[".repeat(4096))
}

#[test]
fn unfinished_discovery_consumes_a_shared_bounded_work_budget() {
    let source = unfinished_candidates();
    assert!((24 * 1024..26 * 1024).contains(&source.len()));
    let mut work = DiscoveryWork::new(source.len()).unwrap();
    let allowance = work.remaining;
    assert_eq!(discover_markdown(&source, &mut work), Err("math_budget"));
    assert!(work.spent > source.len());
    assert!(work.spent <= allowance);
    assert_eq!(work.spent + work.remaining, allowance);
    assert!(work.remaining < 16);
    assert_eq!(markdown_ranges(&source), Err("math_budget"));
}

#[test]
fn cancellation_interrupts_discovery_before_any_returned_range_loop() {
    let source = unfinished_candidates();
    let mut work = DiscoveryWork::new(source.len()).unwrap();
    let allowance = work.remaining;
    let checks = Arc::new(AtomicUsize::new(0));
    let observed = checks.clone();
    let mut delivered = 0;
    let result = crate::speech::with_cancel(
        Arc::new(move || observed.fetch_add(1, Ordering::SeqCst) + 1 >= 100),
        || -> Result<(), &'static str> {
            for _ in discover_markdown(&source, &mut work)? { delivered += 1; }
            Ok(())
        },
    );
    assert_eq!(result, Err("cancelled"));
    assert_eq!(delivered, 0);
    assert_eq!(checks.load(Ordering::SeqCst), 100);
    assert!(work.spent > source.len() * 2);
    assert!(work.spent < allowance);
    assert_eq!(work.spent + work.remaining, allowance);
}

#[test]
fn range_limit_is_exact_and_never_returns_a_successful_prefix() {
    for separator in [" ", ""] {
        let exact = std::iter::repeat(":math[x]").take(4000).collect::<Vec<_>>().join(separator);
        assert_eq!(markdown_ranges(&exact).unwrap().len(), 4000);
        let excessive = format!("{exact}{separator}:math[y]");
        assert_eq!(markdown_ranges(&excessive), Err("math_budget"));
    }
}

#[test]
fn unmatched_candidate_does_not_hide_a_later_normal_or_nested_candidate() {
    for source in ["A :math[unfinished\nNext :math[x^2]", "A :math[unfinished :math[x^2]"] {
        let ranges = markdown_ranges(source).unwrap();
        assert_eq!(ranges.len(), 1);
        assert_eq!(&source[ranges[0].from..ranges[0].to], ":math[x^2]");
        assert_eq!(&source[ranges[0].body_from..ranges[0].body_to], "x^2");
    }
}

#[test]
fn unicode_bom_crlf_quotes_escapes_and_nested_brackets_keep_byte_ranges() {
    let source = "\u{feff}😀 é\r\nA :math[matrix([[a,b],[c,d]])] and :math[\"x]y\\\"z\"]";
    let ranges = markdown_ranges(source).unwrap();
    assert_eq!(ranges.len(), 2);
    assert_eq!(&source[ranges[0].body_from..ranges[0].body_to], "matrix([[a,b],[c,d]])");
    assert_eq!(&source[ranges[1].body_from..ranges[1].body_to], "\"x]y\\\"z\"");
    for range in ranges {
        for at in [range.from, range.to, range.body_from, range.body_to] { assert!(source.is_char_boundary(at)); }
        assert_eq!(&source[range.from..range.body_from], ":math[");
        assert_eq!(&source[range.body_to..range.to], "]");
    }
}

#[test]
fn real_inline_opaque_html_is_inert_without_matching_similar_tag_names() {
    let raw_code = "A <code><span data-readmd-text=\"v1\" data-size-pt=\"30\">:math[x^2]</span></code>";
    assert!(markdown_ranges(raw_code).unwrap().is_empty());
    assert_eq!(crate::publication_inline_text::validate_source(raw_code), Ok(()));
    assert!(crate::publication_inline_view::project(raw_code, &serde_json::json!({})).is_ok());
    for source in [
        "A <CoDe class=\"example\"><span>:math[x]</span></cOdE>",
        "A <code><code>:math[x]</code>:math[y]</code>",
        "A <code>:math[x]",
        "A <pre>:math[x]</pre>",
        "A <annotation-xml>:math[x]</annotation-xml>",
        "A `<code>:math[x]</code>`",
        "A [<code>:math[x]</code>](https://example.invalid)",
        "A ![<code>:math[x]</code>](image.png)",
        "```text\n<code>:math[x]</code>\n```",
    ] { assert!(markdown_ranges(source).unwrap().is_empty(), "{source}"); }
    for source in ["A <codebook>:math[x]</codebook>", "A <annotation-xml-extra>:math[x]</annotation-xml-extra>"] {
        let ranges = markdown_ranges(source).unwrap();
        assert_eq!(ranges.len(), 1, "{source}");
        assert_eq!(&source[ranges[0].body_from..ranges[0].body_to], "x");
    }
    let after_code = "A <code>:math[example]</code> :math[live]";
    let ranges = markdown_ranges(after_code).unwrap();
    assert_eq!(ranges.len(), 1);
    assert_eq!(&after_code[ranges[0].body_from..ranges[0].body_to], "live");
}

#[test]
fn discovery_budget_propagates_through_actual_callers_and_first_run_source() {
    let source = unfinished_candidates();
    assert_eq!(super::expand_markdown(&source).unwrap_err(), "math_budget");
    assert_eq!(crate::html_math::lower_dsl(&source, 12.).err().as_deref(), Some("math_budget"));
    let migration_source = format!("{source}\n{}not-a-payload-->", crate::math_extension::PREFIX);
    assert_eq!(crate::math_migration::plan(&migration_source).unwrap_err(), "math_budget");
    assert_eq!(crate::publication_inline_text::validate_source(&source), Err("math_budget"));
    assert_eq!(crate::publication_inline_view::project(&source, &serde_json::json!({})).unwrap_err(), "math_budget");
    // First-run owned source also reaches the raw discovery work limit.
    // These assertions cover the observable errors, not either internal scan.
    let first_run = format!("<span data-readmd-text=\"v1\" data-size-pt=\"30\">{}</span>", ":math[".repeat(4096));
    assert_eq!(markdown_ranges(&first_run), Err("math_budget"));
    assert_eq!(crate::publication_inline_text::validate_source(&first_run), Err("math_budget"));
    assert_eq!(crate::publication_inline_view::project(&first_run, &serde_json::json!({})).unwrap_err(), "math_budget");
}

#[test]
fn immediate_cancellation_is_propagated_by_discovery_and_existing_callers() {
    let source = "A <span data-readmd-text=\"v1\" data-size-pt=\"30\">text</span> :math[x]";
    crate::speech::with_cancel(Arc::new(|| true), || {
        assert_eq!(markdown_ranges(source), Err("cancelled"));
        assert_eq!(super::expand_markdown(source).unwrap_err(), "cancelled");
        assert_eq!(crate::html_math::lower_dsl(source, 12.).err().as_deref(), Some("cancelled"));
        let migration_source = format!("{source}\n{}not-a-payload-->", crate::math_extension::PREFIX);
        assert_eq!(crate::math_migration::plan(&migration_source).unwrap_err(), "cancelled");
        assert_eq!(crate::publication_inline_text::validate_source(source), Err("cancelled"));
        assert_eq!(crate::publication_inline_view::project(source, &serde_json::json!({})).unwrap_err(), "cancelled");
    });
}
