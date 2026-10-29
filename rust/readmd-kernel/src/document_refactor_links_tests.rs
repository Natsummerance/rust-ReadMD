use super::*;
use std::path::PathBuf;

fn root() -> PathBuf {
    if cfg!(windows) {
        PathBuf::from(r"C:\readmd-link-plan")
    } else {
        PathBuf::from("/readmd-link-plan")
    }
}

fn plan_at(source: &str, file: &str, old: &str, new: &str) -> LinkEditPlan {
    let root = root();
    plan_link_updates(
        source,
        &root.join(file),
        &root.join(old),
        &root.join(new),
        &root,
    )
    .unwrap()
}

fn plan(source: &str, old: &str, new: &str) -> LinkEditPlan {
    plan_at(
        source,
        "docs/source.md",
        &format!("docs/{old}"),
        &format!("docs/{new}"),
    )
}

fn apply(source: &str, plan: &LinkEditPlan) -> String {
    assert!(
        plan.diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        plan.diagnostics
    );
    let mut result = source.to_owned();
    let mut end = source.len();
    for edit in plan.edits.iter().rev() {
        assert!(edit.start <= edit.end && edit.end <= end);
        assert!(source.is_char_boundary(edit.start) && source.is_char_boundary(edit.end));
        result.replace_range(edit.start..edit.end, &edit.replacement);
        end = edit.start;
    }
    result
}

#[test]
fn inline_links_nested_images_and_labels_keep_their_source() {
    let source = "导读😀 [outer ![inner](old.md \"image\")](old.md 'outer')\n[a [bracket] b](old.md)\n[a ` ](old.md) ` b](old.md)\n";
    let changes = plan(source, "old.md", "new.md");
    assert_eq!(changes.edits.len(), 4);
    assert_eq!(apply(source, &changes), "导读😀 [outer ![inner](new.md \"image\")](new.md 'outer')\n[a [bracket] b](new.md)\n[a ` ](old.md) ` b](new.md)\n");
}

#[test]
fn repeated_destinations_do_not_replace_prose_or_similar_names() {
    let source = "[old.md](old.md) old.md [two](./old.md) [different](older.md) ![old.md](old.md)";
    let changes = plan(source, "old.md", "new.md");
    assert_eq!(changes.edits.len(), 3);
    assert_eq!(
        apply(source, &changes),
        "[old.md](new.md) old.md [two](./new.md) [different](older.md) ![old.md](new.md)"
    );
}

#[test]
fn percent_encoded_unicode_spaces_suffixes_and_hex_case_survive() {
    let source = "[文档](%E6%97%A7%20%E6%96%87%E4%BB%B6.md?x=%E7#Part)\n";
    let changes = plan(source, "旧 文件.md", "新 文件.md");
    assert_eq!(
        apply(source, &changes),
        "[文档](%E6%96%B0%20%E6%96%87%E4%BB%B6.md?x=%E7#Part)\n"
    );
    let source = "[x](ol%64%2emd) [y](%6f%6c%64.md)";
    assert_eq!(
        apply(source, &plan(source, "old.md", "new.md")),
        "[x](new%2emd) [y](%6e%65%77.md)"
    );
}

#[test]
fn literal_unicode_and_angle_destinations_keep_titles_and_spacing() {
    let source = "[读](<旧 file.md> 'ti(t)le')\r\n![图](<旧 file.md#段落> \"caption\")\r\n";
    let changes = plan(source, "旧 file.md", "新 file.md");
    assert_eq!(
        apply(source, &changes),
        "[读](<新 file.md> 'ti(t)le')\r\n![图](<新 file.md#段落> \"caption\")\r\n"
    );
}

#[test]
fn reference_definitions_change_once_without_expanding_calls() {
    let source = "[one][REF] ![image][ref] [ref][] [ref]\r\n\r\n[ref]: <old.md?raw=1#part> \"Title\\\" keep\"\r\n[unused]: old.md 'unused'\r\n";
    let changes = plan(source, "old.md", "new.md");
    assert_eq!(changes.edits.len(), 2);
    assert_eq!(apply(source, &changes), "[one][REF] ![image][ref] [ref][] [ref]\r\n\r\n[ref]: <new.md?raw=1#part> \"Title\\\" keep\"\r\n[unused]: new.md 'unused'\r\n");
}

#[test]
fn multiline_reference_definition_offsets_preserve_crlf() {
    let source = "[label][id]\r\n\r\n[id]:\r\n  old.md\r\n  \"title\"\r\n";
    assert_eq!(
        apply(source, &plan(source, "old.md", "new.md")),
        "[label][id]\r\n\r\n[id]:\r\n  new.md\r\n  \"title\"\r\n"
    );
}

#[test]
fn escaped_and_balanced_parentheses_remain_valid_destinations() {
    let source = "[a](old\\(name\\).md \"title\") [b](old(name).md)";
    assert_eq!(
        apply(source, &plan(source, "old(name).md", "new(other).md")),
        "[a](new\\(other\\).md \"title\") [b](new(other).md)"
    );
    let source = "[a](old.md)";
    assert_eq!(
        apply(source, &plan(source, "old.md", "new(part).md")),
        "[a](new\\(part\\).md)"
    );
}

#[test]
fn character_entities_are_resolved_without_reformatting_shared_tokens() {
    let source = "[x](ol&#100;&#46;md) [y](old&#46;md#part)";
    assert_eq!(
        apply(source, &plan(source, "old.md", "new.md")),
        "[x](new&#46;md) [y](new&#46;md#part)"
    );
}

#[test]
fn query_and_fragment_are_not_path_characters_after_percent_decoding() {
    let source = "[x](old%23part.md?query=old.md#fragment) [unrelated](old.md#part.md)";
    assert_eq!(
        apply(source, &plan(source, "old#part.md", "new#part.md")),
        "[x](new%23part.md?query=old.md#fragment) [unrelated](old.md#part.md)"
    );
    let source = "[x](old.md?keep=old.md#old.md) [y](old.md#old.md?keep=1)";
    assert_eq!(
        apply(source, &plan(source, "old.md", "new.md")),
        "[x](new.md?keep=old.md#old.md) [y](new.md#old.md?keep=1)"
    );
}

#[test]
fn normalized_relative_parent_paths_within_root_preserve_their_spelling() {
    let source =
        "[x](./sub/../old.md#sec?keep=1) [y](../docs/old.md?a=1#b) [z](..%2Fdocs%2Fold.md)";
    assert_eq!(
        apply(source, &plan(source, "old.md", "new.md")),
        "[x](./sub/../new.md#sec?keep=1) [y](../docs/new.md?a=1#b) [z](..%2Fdocs%2Fnew.md)"
    );
}

#[test]
fn explicit_wiki_links_keep_aliases_fragments_spacing_and_embed_markers() {
    let source =
        "[[old.md|label]] [[./old.md#Part|alias]] ![[old.md]] [[ old.md | spaced ]] [[other.md]]";
    let changes = plan(source, "old.md", "new.md");
    assert_eq!(changes.edits.len(), 4);
    assert_eq!(
        apply(source, &changes),
        "[[new.md|label]] [[./new.md#Part|alias]] ![[new.md]] [[ new.md | spaced ]] [[other.md]]"
    );
}

#[test]
fn ambiguous_extensionless_or_global_wiki_names_block_commit() {
    let source = "[[old]] [[old#part|label]] [[./old|path]]";
    let changes = plan(source, "old.md", "new.md");
    assert!(changes.edits.is_empty());
    assert_eq!(changes.diagnostics.len(), 3);
    assert!(changes
        .diagnostics
        .iter()
        .all(|d| d.code == "ambiguous_wiki_target"));
    let changes = plan_at(
        "[[old.md]] [[old]]",
        "elsewhere/source.md",
        "docs/old.md",
        "docs/new.md",
    );
    assert_eq!(changes.diagnostics.len(), 2);
    assert!(changes.edits.is_empty());
}

#[test]
fn code_blocks_code_spans_math_metadata_and_escaped_literals_are_opaque() {
    let source = "---\ntitle: '[x](old.md)'\n---\n\n```md\n[x](old.md)\n[[old.md]]\n```\n\n    [x](old.md)\n\n`[x](old.md)` \\[x](old.md) \\[[old.md]] $[x](old.md)$\n\n[x](old.md)\n";
    let changes = plan(source, "old.md", "new.md");
    assert_eq!(changes.edits.len(), 1);
    let expected = source.strip_suffix("[x](old.md)\n").unwrap().to_owned() + "[x](new.md)\n";
    assert_eq!(apply(source, &changes), expected);
}

#[test]
fn html_references_are_blockers_and_html_examples_are_not_edited() {
    let source = "<a href=\"old.md#part\">old</a>\n\n<span>[x](old.md)</span>\n\n<img src=old.md>\n\n<!-- <a href=\"old.md\"> -->\n";
    let changes = plan(source, "old.md", "new.md");
    assert!(changes.edits.is_empty());
    assert_eq!(changes.diagnostics.len(), 3);
    assert!(changes
        .diagnostics
        .iter()
        .all(|d| d.code == "unsupported_html_reference"));
}

#[test]
fn a_reference_definition_in_an_open_html_container_is_not_rewritten() {
    let source = "<span>\n\n[id]: old.md\n\n</span>\n";
    let changes = plan(source, "old.md", "new.md");
    assert!(changes.edits.is_empty());
    assert_eq!(changes.diagnostics.len(), 1);
    assert_eq!(changes.diagnostics[0].code, "unsupported_html_reference");
}

#[test]
fn generated_destinations_reparse_to_the_exact_new_filename() {
    let old_names = [
        ("old.md", "old.md"),
        ("old file.md", "old%20file.md"),
        ("(old).md", "(old).md"),
        ("旧.md", "%E6%97%A7.md"),
        ("old&part.md", "old&amp;part.md"),
        ("old[part].md", "old\\[part\\].md"),
    ];
    let new_names = [
        "new.md",
        "new file.md",
        "new(part).md",
        "new#part.md",
        "new%part.md",
        "新.md",
        "new&copy;.md",
        "new[part].md",
    ];
    for (old_name, raw) in old_names {
        for new_name in new_names {
            let source = format!("[label]({raw}?keep=1#section \"unchanged title\")");
            let updated = apply(&source, &plan(&source, old_name, new_name));
            let links: Vec<_> = Parser::new(&updated)
                .filter_map(|event| match event {
                    Event::Start(Tag::Link {
                        dest_url, title, ..
                    }) => Some((dest_url.into_string(), title.into_string())),
                    _ => None,
                })
                .collect();
            assert_eq!(links.len(), 1, "{old_name} -> {new_name}");
            assert_eq!(links[0].1, "unchanged title");
            let raw_path = links[0].0.strip_suffix("?keep=1#section").unwrap();
            assert_eq!(
                percent_encoding::percent_decode_str(raw_path)
                    .decode_utf8()
                    .unwrap(),
                new_name
            );
        }
    }
}

#[test]
fn an_unlocatable_parser_authorized_target_is_a_blocker() {
    let source = format!("[label `{}`](old.md)", "](ignored.md) ".repeat(65));
    let changes = plan(&source, "old.md", "new.md");
    assert!(changes.edits.is_empty());
    assert_eq!(
        changes.diagnostics[0].code,
        "unsupported_destination_syntax"
    );
}

#[test]
fn external_protocol_fragment_only_and_query_only_links_are_untouched() {
    let source = "[a](HTTP://site/old.md) [b](//host/old.md) [c](mailto:old.md) [d](data:text/plain,old.md) [e](file:///old.md) [f](#old.md) [g](?file=old.md) [[https://site/old.md|external]]";
    let changes = plan(source, "old.md", "new.md");
    assert!(changes.edits.is_empty() && changes.diagnostics.is_empty());
}

#[test]
fn private_and_outside_workspace_paths_are_not_authorized_even_after_dotdot() {
    let source = "[a](.private/../old.md) [b](../../docs/old.md) [c](vendor/../old.md) [d](.%70rivate/../old.md)";
    let changes = plan(source, "old.md", "new.md");
    assert!(changes.edits.is_empty() && changes.diagnostics.is_empty());
    assert!(changes.local_references.is_empty());
}

#[test]
fn unrelated_short_name_and_unicode_spellings_are_physical_audit_candidates() {
    let source = "[short](OLDNAM~1.MD) [unicode](Kelvin.md#part)";
    let changes = plan(source, "Old Name Long.md", "New Name.md");
    assert!(changes.edits.is_empty() && changes.diagnostics.is_empty());
    assert_eq!(changes.local_references.len(), 2);
    assert_eq!(
        changes.local_references[0].path,
        root().join("docs/OLDNAM~1.MD")
    );
    assert_eq!(
        changes.local_references[1].path,
        root().join("docs/Kelvin.md")
    );
    assert_eq!(
        &source[changes.local_references[0].start..changes.local_references[0].end],
        "OLDNAM~1.MD"
    );
    assert_eq!(
        &source[changes.local_references[1].start..changes.local_references[1].end],
        "Kelvin.md#part"
    );
}

#[test]
fn audit_candidates_decode_escapes_entities_and_percent_paths_without_suffixes() {
    let source = "[x](../docs/other\\(part\\).md?keep=1#section)\n\n[id]: other%20file&#46;md#part\n\n<a href=\"third%20file.md?x=1&amp;y=2#part\">x</a>";
    let changes = plan(source, "old.md", "new.md");
    assert!(changes.edits.is_empty() && changes.diagnostics.is_empty());
    let paths: Vec<_> = changes
        .local_references
        .iter()
        .map(|reference| reference.path.clone())
        .collect();
    assert_eq!(
        paths,
        [
            root().join("docs/other(part).md"),
            root().join("docs/other file.md"),
            root().join("docs/third file.md")
        ]
    );
    for reference in &changes.local_references {
        assert!(reference.path.is_absolute());
        assert!(reference.path.starts_with(root()));
        assert!(source.is_char_boundary(reference.start) && source.is_char_boundary(reference.end));
    }
    assert_eq!(
        &source[changes.local_references[2].start..changes.local_references[2].end],
        "third%20file.md?x=1&amp;y=2#part"
    );
}

#[test]
fn wiki_audit_candidates_include_local_markdown_completion_without_guessing_global_matches() {
    let source = "[[OLDNAM~1#part|alias]]";
    let changes = plan(source, "Old Name Long.md", "New Name.md");
    assert!(changes.edits.is_empty() && changes.diagnostics.is_empty());
    assert_eq!(changes.local_references.len(), 4);
    assert_eq!(
        changes.local_references[0].path,
        root().join("docs/OLDNAM~1")
    );
    assert_eq!(
        changes.local_references[1].path,
        root().join("docs/OLDNAM~1.markdown")
    );
    assert_eq!(
        changes.local_references[2].path,
        root().join("docs/OLDNAM~1.md")
    );
    assert_eq!(
        changes.local_references[3].path,
        root().join("docs/OLDNAM~1.mdown")
    );
    assert!(changes
        .local_references
        .iter()
        .all(|reference| { &source[reference.start..reference.end] == "OLDNAM~1#part" }));
}

#[test]
fn supported_markdown_extensions_rewrite_without_becoming_md_or_plain_text() {
    for extension in ["md", "markdown", "mdown"] {
        let old = format!("chapter.{extension}");
        let new = format!("renamed.{extension}");
        let source = format!("[inline]({old}#part) [[{old}|alias]]\n\n[id]: ./{old}?keep=1\n");
        let changes = plan(&source, &old, &new);
        assert_eq!(changes.edits.len(), 3);
        assert_eq!(
            apply(&source, &changes),
            format!("[inline]({new}#part) [[{new}|alias]]\n\n[id]: ./{new}?keep=1\n")
        );
    }
    let root = root();
    for (old, new) in [("old.txt", "new.txt"), ("old.markdown", "new.md")] {
        assert_eq!(
            plan_link_updates(
                "",
                &root.join("docs/source.md"),
                &root.join(format!("docs/{old}")),
                &root.join(format!("docs/{new}")),
                &root
            )
            .unwrap_err(),
            "link_rename_requires_markdown"
        );
    }
}

#[test]
fn extensionless_markdown_and_mdown_targets_block_instead_of_becoming_dangling() {
    for extension in ["markdown", "mdown"] {
        let changes = plan(
            "[[./chapter]]",
            &format!("chapter.{extension}"),
            &format!("renamed.{extension}"),
        );
        assert!(changes.edits.is_empty());
        assert_eq!(changes.diagnostics[0].code, "ambiguous_wiki_target");
        assert!(changes
            .local_references
            .iter()
            .any(|reference| reference.path == root().join(format!("docs/chapter.{extension}"))));
    }
}

#[test]
fn code_urls_fragment_only_and_private_paths_never_authorize_audit_reads() {
    let source = "`[x](old.md)`\n\n```md\n[x](old.md)\n```\n\n[external](https://example.test/old.md) [network](//host/old.md) [fragment](#old.md) [private](.private/old.md) [outside](../../old.md)";
    let changes = plan(source, "old.md", "new.md");
    assert!(changes.edits.is_empty() && changes.diagnostics.is_empty());
    assert!(changes.local_references.is_empty());
}

#[test]
fn unsupported_root_relative_and_extensionless_targets_block_commit() {
    let source = "[a](/docs/old.md) [b](old) [c](old.md)";
    let changes = plan(source, "old.md", "new.md");
    assert_eq!(changes.edits.len(), 1);
    assert_eq!(
        changes
            .diagnostics
            .iter()
            .map(|d| d.code.as_str())
            .collect::<Vec<_>>(),
        [
            "unsupported_root_relative_reference",
            "unsupported_extensionless_target"
        ]
    );
}

#[test]
fn invalid_local_percent_encoding_is_diagnosed_without_a_guess() {
    let changes = plan("[a](old%ZZ.md) [b](old%FF.md)", "old.md", "new.md");
    assert!(changes.edits.is_empty());
    assert_eq!(changes.diagnostics.len(), 2);
    assert!(changes
        .diagnostics
        .iter()
        .all(|d| d.code == "invalid_percent_encoding"));
}

#[test]
fn new_reserved_filename_characters_are_encoded_without_changing_the_url_suffix() {
    let source = "[a](old.md#heading)";
    assert_eq!(
        apply(source, &plan(source, "old.md", "new#part&copy;.md")),
        "[a](new%23part%26copy;.md#heading)"
    );
}

#[test]
fn utf8_bom_and_crlf_are_never_rewritten() {
    let source = "\u{feff}[文](old.md)\r\n\r\n[id]: old.md\r\n";
    assert_eq!(
        apply(source, &plan(source, "old.md", "new.md")),
        "\u{feff}[文](new.md)\r\n\r\n[id]: new.md\r\n"
    );
}

#[test]
fn edit_offsets_and_unchanged_bytes_are_exact() {
    let source = "😀 [label](./old.md#heading \"title\") tail\r\n";
    let changes = plan(source, "old.md", "new.md");
    assert_eq!(changes.edits.len(), 1);
    let edit = &changes.edits[0];
    assert_eq!(&source[edit.start..edit.end], "old");
    assert_eq!(edit.replacement, "new");
    let updated = apply(source, &changes);
    assert_eq!(&updated[..edit.start], &source[..edit.start]);
    assert_eq!(
        &updated[edit.start + edit.replacement.len()..],
        &source[edit.end..]
    );
}

#[test]
fn invalid_authority_inputs_are_rejected_before_planning() {
    let root = root();
    let source = root.join("docs/source.md");
    let old = root.join("docs/old.md");
    assert_eq!(
        plan_link_updates("", &source, &old, &root.join("elsewhere/new.md"), &root).unwrap_err(),
        "link_rename_must_stay_in_directory"
    );
    assert_eq!(
        plan_link_updates("", &source, &old, &root.join("docs/new.txt"), &root).unwrap_err(),
        "link_rename_requires_markdown"
    );
    assert_eq!(
        plan_link_updates(
            "",
            Path::new("source.md"),
            &old,
            &root.join("docs/new.md"),
            &root
        )
        .unwrap_err(),
        "link_path_must_be_absolute"
    );
    assert_eq!(
        plan_link_updates(
            "",
            &root.join(".private/source.md"),
            &old,
            &root.join("docs/new.md"),
            &root
        )
        .unwrap_err(),
        "private_link_path"
    );
}

#[cfg(windows)]
#[test]
fn windows_ascii_case_is_insensitive_and_non_ascii_case_aliases_are_blocked() {
    assert_eq!(
        apply("[x](OLD.MD)", &plan("[x](OLD.MD)", "old.md", "new.md")),
        "[x](new.md)"
    );
    let changes = plan("[x](Ä.md)", "ä.md", "new.md");
    assert!(changes.edits.is_empty());
    assert_eq!(changes.diagnostics[0].code, "ambiguous_windows_case");
    let changes = plan("[x](C:/readmd-link-plan/docs/old.md)", "old.md", "new.md");
    assert!(changes.edits.is_empty());
    assert_eq!(
        changes.diagnostics[0].code,
        "unsupported_absolute_reference"
    );
}

#[cfg(windows)]
#[test]
fn absolute_audit_candidates_allow_the_authorized_dot_root_but_not_private_children() {
    let root = PathBuf::from(r"C:\.authorized\workspace");
    let source = "[alias](C:/.authorized/workspace/docs/OLDNAM~1.MD) [private](C:/.authorized/workspace/.private/../docs/OLDNAM~1.MD)";
    let changes = plan_link_updates(
        source,
        &root.join("docs/source.md"),
        &root.join("docs/Old Name.md"),
        &root.join("docs/New Name.md"),
        &root,
    )
    .unwrap();
    assert!(changes.edits.is_empty() && changes.diagnostics.is_empty());
    assert_eq!(changes.local_references.len(), 1);
    assert_eq!(
        changes.local_references[0].path,
        root.join("docs/OLDNAM~1.MD")
    );
}

#[cfg(not(windows))]
#[test]
fn unix_path_case_is_exact() {
    let changes = plan("[x](OLD.MD)", "old.md", "new.md");
    assert!(changes.edits.is_empty() && changes.diagnostics.is_empty());
}
