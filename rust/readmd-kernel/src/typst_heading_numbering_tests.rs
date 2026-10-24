use super::*;

fn source(suffix:&str)->String{
    format!("<!-- readmd-numbering:v1 {{\"headings\":{{\"levels\":[{{\"format\":\"upperRoman\",\"start\":4}},{{\"format\":\"lowerLetter\",\"start\":3,\"restartAfter\":0}},{{\"format\":\"decimal\",\"start\":5,\"restartAfter\":1}}],\"separator\":\".\",\"suffix\":\"{suffix}\"}}}} -->\n\n# First {{#first}}\n\n## Second\n\n### Third\n\n## Fourth\n\n### Fifth\n\n# Sixth\n\n## Seventh\n\n### Eighth\n")
}
fn render_source(source:&str)->Source {
    let directory=tempfile::tempdir().unwrap();
    render(&crate::md_ast::parse(source),&serde_json::json!({"toc":{"enabled":true},"cover":{"enabled":false}}),"",directory.path(),"Headings").unwrap()
}

#[test]
fn zero_start_and_per_level_legal_template_do_not_use_negative_typst_counters() {
    let config=serde_json::json!({"headings":{"levels":[{"start":0,"text":"(%1)","suffix":"nothing"},
        {"format":"upperLetter","start":28,"text":"[%2:%1:%2]","suffix":"space","isLegal":true}]}});
    let output=render_source(&format!("<!-- readmd-numbering:v1 {config} -->\n\n# Zero\n\n## Legal\n\n# Manual {{.unnumbered}}\n")).text;
    assert!(output.contains("#counter(heading).update((0,))"));
    assert!(!output.contains("#counter(heading).update((-1"));
    assert!(output.contains("let readmd-value = readmd-levels.pos().at(0) - 1"));
    assert!(output.contains("#numbering(\"1\",readmd-levels.pos().at(1))"));
    assert!(output.contains("#text(\"[\")"));
    assert_eq!(output.matches("numbering:(..readmd-levels)").count(),2);
}

#[test]
fn native_heading_counter_arguments_honor_starts_and_restart_after_without_static_author_prefixes(){
    let output=render_source(&source("space")).text;
    for update in ["(3,)","(4,2,)","(4,3,4,)","(4,3,)","(4,4,5,)","(4,)","(5,4,)","(5,5,4,)"]{
        assert!(output.contains(&format!("#counter(heading).update({update})")),"missing {update}");
    }
    assert_eq!(output.matches("#heading(level:").count(),8);
    assert!(output.contains(&format!("#heading(level:2,numbering:(..readmd-levels) => [#numbering(\"I\",readmd-levels.pos().at(0))#text(\".\")#numbering({},readmd-levels.pos().at(1))#text(\" \")])",number_format("lowerLetter"))));
    assert!(output.contains("counter(heading).display(it.numbering) + it.body"));
    assert!(output.contains("#outline(depth:3)"));
    assert!(output.contains(&format!("#label({})",string(&anchor("first")))));
    for title in ["First","Second","Third","Fourth","Fifth","Sixth","Seventh","Eighth"]{
        assert!(output.contains(&format!("[#text(\"{title}\")]#label(")));
    }
    assert!(!output.contains("#text(\"IV.c"));
}

#[test]
fn styled_numbered_headings_keep_the_native_counter_renderer_and_original_author_body(){
    let input=source("nothing").replace("## Second","## Second<!-- readmd:layout v1 {\"id\":\"styled-heading\",\"text\":{\"size\":17,\"color\":\"#336699\"}} -->");
    let output=render_source(&input).text;
    assert!(output.contains("#show heading: it => context { counter(heading).display(it.numbering) + it.body }"));
    assert!(output.contains("size:17pt"));
    assert!(output.contains("numbering:(..readmd-levels)"));
    assert!(!output.contains("#text(\" \")])"));
    assert!(!output.contains("IV.c Second"));
}

#[test]
fn unsupported_tab_stops_are_diagnosed_and_unnumbered_heading_source_stays_legacy(){
    let output=render_source(&source("tab"));
    assert_eq!(output.warnings.iter().filter(|value|value.as_str()=="publication_numbering_tab_unsupported").count(),1);
    assert!(output.text.contains("#h(2em)"));
    let plain=render_source("# Old heading\n\n## Old subheading");
    assert!(!plain.text.contains("#counter(heading).update("));
    assert!(!plain.text.contains("numbering:(..readmd-levels)"));
    assert!(!plain.text.contains("counter(heading).display(it.numbering)"));
    assert!(plain.text.contains("#heading(level:1)[#text(\"Old heading\")]"));
}

#[test]
fn letter_headings_pages_and_notes_use_one_native_counter_formatter_beyond_z(){
    for (format,pattern) in [("upperLetter","A"),("lowerLetter","a")] {
        let callback=number_format(format);
        assert_eq!(callback,format!("(readmd-n => numbering(\"{pattern}\",calc.rem(readmd-n - 1,26) + 1) * (calc.quo(readmd-n - 1,26) + 1))"));
        let input=format!("<!-- readmd-numbering:v1 {{\"headings\":{{\"levels\":[{{\"format\":\"{format}\",\"start\":52}}]}}}} -->\n\n# First\n\nBody[^a].\n\n# Next\n\nOther[^b].\n\n[^a]: Alpha.\n\n[^b]: Beta.\n");
        for position in ["page","document"] {
            let directory=tempfile::tempdir().unwrap();let style=serde_json::json!({"cover":{"enabled":false},"toc":{"enabled":false},"footer":{"numberFormat":format,"start":52},"footnotes":{"position":position,"numberFormat":format,"start":52}});
            let output=render(&crate::md_ast::parse(&input),&style,"",directory.path(),"Boundary").unwrap().text;
            assert!(output.contains("#counter(heading).update((51,))")&&output.contains("#counter(heading).update((52,))"));
            assert!(output.contains(&format!("#numbering({callback},readmd-levels.pos().at(0))")));
            assert!(output.contains(&format!("#counter(page).display({callback})")));
            assert!(output.contains(&format!("#set footnote(numbering:{callback})")));
            if position=="document" {for index in [52,53]{assert_eq!(output.matches(&format!("#numbering({callback},{index})")).count(),2);}}
            assert!(output.contains("[#text(\"First\")]#label("));
        }
    }
}
