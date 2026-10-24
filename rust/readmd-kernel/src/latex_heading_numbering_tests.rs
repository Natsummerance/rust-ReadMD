use super::*;
use serde_json::json;

fn six_levels()->String{
    let config=json!({"headings":{"levels":[
        {"format":"upperRoman","start":4},{"format":"upperLetter","start":27},
        {"format":"decimal","start":5},{"format":"lowerLetter","start":2},
        {"format":"lowerRoman","start":3},{"format":"decimal","start":7}
    ],"separator":".","suffix":"space"}});
    format!("<!-- readmd-numbering:v1 {config} -->\n\n# First {{#first}}\n\n## Second\n\n### Third\n\n#### Fourth\n\n##### Fifth\n\n###### Sixth\n\n# Later {{#later}}\n\n## Last\n")
}
fn render(source:&str,class:&str,shared:bool)->String{
    let mut options=LatexOptions{doc_class:class.into(),toc:true,bib_engine:String::new(),..Default::default()};
    if shared{options.publication_style=Some(crate::export_styles::sanitize(&json!({"cover":{"enabled":false},"frontMatter":{"mode":"none"},"headings":{"h1":{"keepNext":false}}})));}
    render_document_with_geometry(&crate::md_ast::parse(source),&options,&|_|None).unwrap().0
}
fn visible(source:&str)->&str{source.split_once("\\begin{document}\n").unwrap().1}

#[test]
fn per_level_templates_zero_and_legal_format_use_native_tex_counters() {
    let config=json!({"headings":{"levels":[{"start":0,"text":"Part %1)","suffix":"nothing"},
        {"format":"upperLetter","start":28,"text":"[%2/%1/%2]","suffix":"space"},
        {"format":"lowerRoman","start":3,"text":"%1:%2:%3 %x %9","isLegal":true,"suffix":"nothing"}]}});
    let source=format!("<!-- readmd-numbering:v1 {config} -->\n\n# Zero {{#zero}}\n\n## Letter\n\n### Legal\n\n# Manual {{.unnumbered}}\n");
    for class in ["article","book"] {
        let output=render(&source,class,false);let body=visible(&output);
        assert!(body.contains("\\setcounter{readmdh1}{-1}"));
        assert!(body.contains("\\endcsname{Part \\arabic{readmdh1})}"));
        assert!(body.contains("\\endcsname{[\\readmdAlph{\\value{readmdh2}}/\\arabic{readmdh1}/\\readmdAlph{\\value{readmdh2}}]}"));
        assert!(body.contains("\\endcsname{\\arabic{readmdh1}:\\arabic{readmdh2}:\\arabic{readmdh3} \\%x }"));
        assert_eq!(body.matches("\\readmdnumberedheadingmark{").count(),3);
        // The typeset title must use the native counter. Its separate plain
        // PDF bookmark text necessarily contains the resolved zero number.
        let command=if class=="book"{"\\chapter"}else{"\\section"};
        let native="\\readmdnumberedheadingmark{1}{Zero}\\label{sec:zero}\\csname thereadmdh1\\endcsname{}Zero";
        assert!(body.contains(&format!("{command}*{{\\texorpdfstring{{{native}}}{{Part 0)Zero}}}}")));
        assert_eq!(body.matches("Part 0)Zero").count(),1);
    }
}

#[test]
fn article_and_book_numbered_headings_use_six_native_counters_without_a_class_number_prefix(){
    for class in ["article","book"]{
        let output=render(&six_levels(),class,false);let body=visible(&output);
        for level in 1..=6{assert_eq!(output.matches(&format!("\\newcounter{{readmdh{level}}}")).count(),1);}
        assert!(output.find("\\newcounter{readmdh1}").unwrap()<output.find("\\begin{document}").unwrap());
        assert!(output.contains("\\int_to_Alph:n")&&output.contains("\\int_to_alph:n"));
        assert!(output.contains("\\prg_replicate:nn { \\int_div_truncate:nn {#1-1} {26} + 1 }"));
        assert!(output.contains("\\int_mod:nn {#1-1} {26} + 1"));
        assert!(body.contains("\\setcounter{readmdh1}{3}"));
        assert!(body.contains("\\setcounter{readmdh2}{26}"));
        assert!(body.contains("\\csname thereadmdh2\\endcsname{\\Roman{readmdh1}.\\readmdAlph{\\value{readmdh2}}}"));
        assert_eq!(body.matches("\\readmdnumberedheadingmark{").count(),8);
        assert_eq!(body.matches("\\readmdnumberedheadingtoc{").count(),8);
        assert!(body.contains("\\readmdnumberedheadingmark{1}{First}\\label{sec:first}\\csname thereadmdh1\\endcsname{}\\ First"));
        let top=if class=="book"{"\\chapter*{\\texorpdfstring"}else{"\\section*{\\texorpdfstring"};
        assert!(body.contains(top));
        let first_toc=if class=="book"{"chapter"}else{"section"};
        assert!(body.contains(&format!("\\readmdnumberedheadingtoc{{{first_toc}}}{{1}}{{First}}")));
        let sixth_toc=if class=="book"{"subparagraph"}else{"readmdsubparagraph"};
        assert!(body.contains(&format!("\\readmdnumberedheadingtoc{{{sixth_toc}}}{{6}}{{Sixth}}")));
        assert!(!body.contains("\\section{IV")&&!body.contains("\\chapter{IV"));
        assert!(!body.contains("\\section[IV")&&!body.contains("\\chapter[IV"));
        assert!(!output.contains("\\setcounter{tocdepth}")); // Legacy class defaults remain native.
    }
}

#[test]
fn source_restart_rules_feed_native_counter_steps_even_when_lower_levels_never_restart(){
    let config=json!({"headings":{"levels":[{"format":"decimal","start":4},{"format":"lowerLetter","start":3,"restartAfter":0},{"format":"decimal","start":5,"restartAfter":1}],"separator":".","suffix":"nothing"}});
    let source=format!("<!-- readmd-numbering:v1 {config} -->\n\n# A\n\n## B\n\n### C\n\n## D\n\n### E\n\n# F\n\n## G\n\n### H\n");
    let output=render(&source,"article",false);let body=visible(&output);
    let mut states=Vec::new();let mut current=[0usize;6];
    for line in body.lines(){
        if let Some(rest)=line.strip_prefix("\\setcounter{readmdh"){
            let(level,value)=rest.split_once("}{").unwrap();current[level.parse::<usize>().unwrap()-1]=value.trim_end_matches('}').parse().unwrap();
        }
        if let Some(at)=line.find("\\readmdnumberedheadingmark{"){
            let rest=&line[at+"\\readmdnumberedheadingmark{".len()..];let depth=rest.split('}').next().unwrap().parse::<usize>().unwrap();current[depth-1]+=1;states.push(current[..depth].to_vec());
        }
    }
    assert_eq!(states,vec![vec![4],vec![4,3],vec![4,3,5],vec![4,4],vec![4,4,6],vec![5],vec![5,5],vec![5,5,5]]);
    assert!(!body.contains("\\endcsname{}\\ A")); // Empty suffix stays empty.
    assert!(body.contains("\\endcsname{}A"));
    assert!(!body.contains("\\endcsnameA"));
}

#[test]
fn shared_book_and_local_font_headings_preserve_one_native_number_and_valid_toc_destinations(){
    // Heading attributes must remain last on their line. Put a top-level local
    // directive before it instead of turning {#first} into author title text.
    let source=six_levels().replace("# First {#first}","<!-- readmd:layout v1 {\"id\":\"heading-local-font\",\"text\":{\"font\":\"TeX Gyre Termes\"}} -->\n\n# First {#first}");
    let parsed=crate::md_ast::parse(&source);
    let first=parsed.blocks.iter().map(Block::unstyled).find(|block|matches!(block,Block::Heading{..})).unwrap();
    if let Block::Heading{id,inlines,..}=first{assert_eq!(id,"first");assert_eq!(inline_plain(inlines),"First");}
    let output=render(&source,"book",true);let body=visible(&output);
    assert!(body.contains("\\begingroup\\readmdtermesblockfont"));
    assert!(body.contains("\\csname readmdheading1\\endcsname*{\\texorpdfstring"));
    assert!(!body.contains("\\chapter*{\\texorpdfstring"));
    assert!(body.contains("\\readmdnumberedheadingtoc{section}{1}{First}"));
    assert!(body.contains("\\readmdnumberedheadingtoc{readmdsubparagraph}{6}{Sixth}"));
    assert!(body.contains("\\readmdheadingfree"));
    assert!(output.contains("\\xdef\\readmdheadinghref{\\@currentHref}"));
    assert!(output.contains("\\let\\@currentHref\\readmdheadinghref"));
    assert!(output.contains("\\csname theHreadmdh6\\endcsname{\\arabic{readmdhanchor}}"));
    assert_eq!(body.matches("\\label{sec:first}").count(),1);
}

#[test]
fn shared_toc_depth_controls_the_native_reader_and_bookmarks_before_the_first_toc(){
    for class in ["article","book"]{
        for numbered in [false,true]{
            let source=if numbered{six_levels()}else{"# First {#first}\n\n## Second\n\n### Third\n\n#### Fourth\n\n##### Fifth\n\n###### Sixth\n".into()};
            for depth in [1,2,3,6]{
                let options=LatexOptions{doc_class:class.into(),toc:true,bib_engine:String::new(),
                    publication_style:Some(crate::export_styles::sanitize(&json!({"toc":{"enabled":true,"depth":depth},"cover":{"enabled":false},"frontMatter":{"mode":"none"}}))),..Default::default()};
                let output=render_document_with_geometry(&crate::md_ast::parse(&source),&options,&|_|None).unwrap().0;
                let toc=output.find("\\tableofcontents").unwrap();
                let setting=format!("\\setcounter{{tocdepth}}{{{depth}}}");
                assert_eq!(output.matches(&setting).count(),1);
                assert!(output.find(&setting).unwrap()<toc);
                assert!(output.find("\\providecommand{\\l@readmdsubparagraph}").unwrap()<output.find("\\begin{document}").unwrap());
                assert!(output.find("\\providecommand{\\toclevel@readmdsubparagraph}{6}").unwrap()<toc);
                // Shared book headings retain the same 1..6 coordinate system
                // as article. Legacy book chapter/subparagraph remain 0..5.
                assert!(output.contains("\\@startsection{section}{1}"));
                assert!(output.contains("\\@startsection{readmdsubparagraph}{6}"));
                if numbered{
                    assert!(output.contains("\\readmdnumberedheadingtoc{section}{1}{First}"));
                    assert!(output.contains("\\readmdnumberedheadingtoc{readmdsubparagraph}{6}{Sixth}"));
                }
            }
        }
    }
}

#[test]
fn an_explicitly_unconfigured_heading_level_is_unnumbered_and_keeps_its_toc_and_label(){
    let config=json!({"headings":{"levels":[{"start":3}]}});
    let source=format!("<!-- readmd-numbering:v1 {config} -->\n\n# Numbered\n\n## Plain lower {{#plain}}\n\n###### Plain sixth\n");
    for class in ["article","book"]{
        let output=render(&source,class,false);let body=visible(&output);
        assert_eq!(body.matches("\\readmdnumberedheadingmark{").count(),1);
        assert!(body.contains("*{\\texorpdfstring{Plain lower}{Plain lower}}\\label{sec:plain}"));
        assert!(body.contains("\\addcontentsline{toc}"));
        assert!(!body.contains("\\readmdnumberedheadingmark{2}"));
        assert!(!body.contains("\\readmdnumberedheadingmark{6}"));
    }
    let legacy=render_body(&crate::md_ast::parse("# Old {#old}\n\nBody.\n"),&|_|None);
    assert_eq!(legacy,"\\section{Old}\\label{sec:old}\n\nBody.\n\n");
    assert!(!render("# Old\n","article",false).contains("readmdhanchor"));
}

#[test]
fn numbered_toc_widths_measure_actual_reader_fonts_once_and_keep_native_layout(){
    for class in ["article","book"]{
        for shared in [false,true]{
            let output=render(&six_levels(),class,shared);
            let first=if class=="book"&&!shared{"chapter"}else{"section"};
            let second=if class=="book"&&!shared{"section"}else{"subsection"};
            assert!(output.contains("\\hbox{#1\\hskip\\fontdimen2\\font}"));
            assert!(output.contains("\\ifcsname readmdtocwidth@#1\\endcsname\\else"));
            assert!(output.contains("\\setbox\\readmdtocrowbox=\\vbox"));
            assert!(output.contains("\\csname readmdtocoriginal@#1\\endcsname{\\numberline{}}{}"));
            assert!(output.contains("\\readmdtocdottedline{##1}{##2}{\\csname readmdtocwidth@#1\\endcsname}"));
            let first_setup=format!("\\readmdtocsetup{{{first}}}{{\\readmdtocmeasure{{IV}}\\readmdtocmeasure{{V}}}}");
            let second_setup=format!("\\readmdtocsetup{{{second}}}{{\\readmdtocmeasure{{IV.AA}}\\readmdtocmeasure{{V.AA}}}}");
            assert_eq!(output.matches(&first_setup).count(),1);
            assert_eq!(output.matches(&second_setup).count(),1);
            assert!(output.find(&first_setup).unwrap()<output.find("\\begin{document}").unwrap());
            assert!(output.find(&second_setup).unwrap()<output.find("\\tableofcontents").unwrap());
        }
    }
    let config=json!({"headings":{"levels":[{"format":"upperRoman","start":3888},{"format":"upperLetter","start":53},{"start":10000}]}});
    let source=format!("<!-- readmd-numbering:v1 {config} -->\n\n# Long\n\n## Sub\n\n### Deep\n");
    let output=render(&source,"article",true);
    assert!(output.contains("\\readmdtocmeasure{MMMDCCCLXXXVIII.AAA.10000}"));
    assert!(!render("# Existing\n","article",false).contains("readmdtocsetup"));
    assert!(!render("# Existing\n","book",true).contains("readmdtocnumberbox"));
}

#[test]
fn malformed_direct_numbering_fails_before_a_usable_standalone_document_is_returned(){
    let mut document=crate::md_ast::parse(&six_levels());
    if let Some(Block::Heading{numbering:Some(numbering),..})=document.blocks.iter_mut().find(|block|matches!(block,Block::Heading{..})){
        // Zero is a supported native start. An actual out-of-budget number
        // must still fail before any usable standalone document is returned.
        numbering.numbers[0]=crate::cross_references::MAX_NUMBER+1;
    }else{panic!("numbered heading missing");}
    assert_eq!(render_document_with_geometry(&document,&LatexOptions::default(),&|_|None).unwrap_err(),"publication_numbering_invalid");
}

#[test]
fn label_keys_preserve_plain_ascii_and_roundtrip_delimited_unicode_scalars_without_collisions(){
    fn decode(key:&str)->String{
        let mut encoded=key.strip_prefix("sec:").unwrap();let mut decoded=String::new();
        while !encoded.is_empty(){
            if let Some(rest)=encoded.strip_prefix('.'){
                let (scalar,tail)=rest.split_once('.').unwrap();
                assert!(!scalar.is_empty()&&scalar.bytes().all(|byte|byte.is_ascii_hexdigit()));
                decoded.push(char::from_u32(u32::from_str_radix(scalar,16).unwrap()).unwrap());encoded=tail;
            }else{
                let character=encoded.chars().next().unwrap();
                assert!(character.is_ascii_alphanumeric()||matches!(character,'-'|'_'));
                decoded.push(character);encoded=&encoded[character.len_utf8()..];
            }
        }
        decoded
    }
    let plain="ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789_-";
    assert_eq!(label_key(plain),format!("sec:{plain}"));
    for (id,key) in [("α","sec:.3b1."),("u3b1","sec:u3b1"),("x:y","sec:x.3a.y"),
        ("xu3ay","sec:xu3ay"),(".","sec:.2e."),("u2e","sec:u2e"),
        (".3b1.","sec:.2e.3b1.2e."),("u2e3b1u2e","sec:u2e3b1u2e")]{
        assert_eq!(label_key(id),key);assert_eq!(decode(key),id);
    }
    let alphabet:Vec<char>=(0..=127).filter_map(char::from_u32).chain(['α','β','中','🙂','\u{80}','\u{2028}','\u{10ffff}']).collect();
    let mut identities=std::collections::HashMap::<String,String>::new();
    for first in &alphabet{for second in &alphabet{
        let id=format!("{first}{second}");let key=label_key(&id);assert_eq!(decode(&key),id);
        assert!(identities.insert(key,id).is_none(),"Two distinct scalar sequences shared a key");
    }}
}

#[test]
fn numbered_unicode_and_escape_looking_heading_ids_keep_exactly_matching_distinct_tex_links(){
    let ids=["α","u3b1","x:y","xu3ay",".","u2e",".3b1.","u2e3b1u2e"];
    let keys=["sec:.3b1.","sec:u3b1","sec:x.3a.y","sec:xu3ay","sec:.2e.","sec:u2e","sec:.2e.3b1.2e.","sec:u2e3b1u2e"];
    let mut source="<!-- readmd-numbering:v1 {\"headings\":{\"levels\":[{}]}} -->\n\n".to_string();
    for (index,id) in ids.iter().enumerate(){
        source.push_str(&format!("# IDHEAD{} {{#{id}}}\n\nIDBODY{}.\n\n",index+1,index+1));
    }
    for (index,id) in ids.iter().enumerate(){source.push_str(&format!("[IDLINK{}](#{id})\n\n",index+1));}
    let document=crate::md_ast::parse(&source);
    let actual:Vec<_>=document.blocks.iter().filter_map(|block|if let Block::Heading{id,inlines,..}=block.unstyled(){Some((id.as_str(),inline_plain(inlines)))}else{None}).collect();
    assert_eq!(actual.iter().map(|(id,_)|*id).collect::<Vec<_>>(),ids);
    for (index,(_,title)) in actual.iter().enumerate(){assert_eq!(title,&format!("IDHEAD{}",index+1));}
    for shared in [false,true]{
        let output=render(&source,"article",shared);
        for (index,key) in keys.iter().enumerate(){
            assert_eq!(output.matches(&format!("\\label{{{key}}}")).count(),1);
            assert!(output.contains(&format!("\\hyperref[{key}]{{IDLINK{}}}",index+1)));
        }
        assert_eq!(output.matches("\\readmdnumberedheadingmark{1}").count(),ids.len());
        assert!(!output.contains("\\label{sec:x:y}")&&!output.contains("\\label{sec:α}"));
    }
}
