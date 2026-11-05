//! Revision-checked UTF-16 edits shared by native UI, structured tools and MCP.
//! This module never writes files. Persistence remains the guarded save service.
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const MAX_DOCUMENT_BYTES: usize = 32 * 1024 * 1024;
pub const MAX_OPERATIONS: usize = 4096;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TextEdit {
    pub from: usize,
    pub to: usize,
    pub insert: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContentTransaction {
    pub base_revision: String,
    pub origin: String,
    pub edits: Vec<TextEdit>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransactionResult {
    pub content: String,
    pub revision: String,
    pub changed: bool,
    pub inverse: Vec<TextEdit>,
}

pub fn revision(content: &str) -> String {
    format!("{:x}", Sha256::digest(content.as_bytes()))
}

/// A boundary inside a surrogate pair is invalid, even though it fits in usize.
pub fn byte_offset(content: &str, utf16: usize) -> Result<usize, &'static str> {
    let mut offset = 0;
    for (byte, ch) in content.char_indices() {
        if offset == utf16 {
            return Ok(byte);
        }
        offset += ch.len_utf16();
        if offset > utf16 {
            return Err("transaction_invalid_range");
        }
    }
    if offset == utf16 {
        Ok(content.len())
    } else {
        Err("transaction_invalid_range")
    }
}

pub fn apply(content: &str, tx: &ContentTransaction) -> Result<TransactionResult, &'static str> {
    let mut result = apply_impl(content, tx)?;
    result.inverse = canonical_inverse(result.inverse)?;
    Ok(result)
}

/// Review records one inverse fragment per forward edit for causal range mapping.
/// These fragments must be checked individually before canonicalizing for replay.
pub(crate) fn apply_with_inverse_fragments(
    content: &str,
    tx: &ContentTransaction,
) -> Result<TransactionResult, &'static str> {
    apply_impl(content, tx)
}

/// Coalesce only trusted inverse prefixes collapsed by adjacent forward deletes.
/// This is never used to normalize client edits before their strict validation.
pub(crate) fn canonical_inverse(
    fragments: Vec<TextEdit>,
) -> Result<Vec<TextEdit>, &'static str> {
    if fragments.len() > MAX_OPERATIONS {
        return Err("transaction_budget");
    }
    let mut result: Vec<TextEdit> = Vec::with_capacity(fragments.len());
    let mut bytes = 0usize;
    for edit in fragments {
        if edit.from > edit.to {
            return Err("transaction_overlapping_edits");
        }
        bytes = bytes
            .checked_add(edit.insert.len())
            .ok_or("transaction_budget")?;
        if bytes > MAX_DOCUMENT_BYTES {
            return Err("transaction_budget");
        }
        if let Some(previous) = result.last_mut() {
            if previous.from == previous.to && previous.from == edit.from {
                previous.to = edit.to;
                previous.insert.push_str(&edit.insert);
                continue;
            }
            if edit.from < previous.to || edit.from == previous.from {
                return Err("transaction_overlapping_edits");
            }
        }
        result.push(edit);
    }
    Ok(result)
}

fn apply_impl(content: &str, tx: &ContentTransaction) -> Result<TransactionResult, &'static str> {
    if content.len() > MAX_DOCUMENT_BYTES || tx.edits.len() > MAX_OPERATIONS {
        return Err("transaction_budget");
    }
    if !matches!(
        tx.origin.as_str(),
        "editor"
            | "formula"
            | "table"
            | "ai"
            | "workspace"
            | "review"
            | "mcp"
            | "drawing"
            | "openapi"
    ) {
        return Err("transaction_invalid_origin");
    }
    if tx.base_revision != revision(content) {
        return Err("transaction_conflict");
    }
    let mut edits: Vec<_> = tx.edits.iter().collect();
    edits.sort_by_key(|e| (e.from, e.to));
    let mut previous = None;
    let mut byte_edits = Vec::with_capacity(edits.len());
    let mut size = content.len();
    for e in &edits {
        if e.from > e.to || previous.is_some_and(|(start, end)| e.from < end || e.from == start) {
            return Err("transaction_overlapping_edits");
        }
        let from = byte_offset(content, e.from)?;
        let to = byte_offset(content, e.to)?;
        size = size
            .checked_sub(to - from)
            .and_then(|s| s.checked_add(e.insert.len()))
            .ok_or("transaction_budget")?;
        if size > MAX_DOCUMENT_BYTES {
            return Err("transaction_budget");
        }
        byte_edits.push((from, to, *e));
        previous = Some((e.from, e.to));
    }
    let mut output = String::with_capacity(size);
    let mut cursor = 0;
    let mut shift: isize = 0;
    let mut inverse = Vec::with_capacity(edits.len());
    for (from, to, e) in byte_edits {
        output.push_str(&content[cursor..from]);
        output.push_str(&e.insert);
        let position = e
            .from
            .checked_add_signed(shift)
            .ok_or("transaction_invalid_range")?;
        let inserted = e.insert.encode_utf16().count();
        inverse.push(TextEdit {
            from: position,
            to: position + inserted,
            insert: content[from..to].to_string(),
        });
        shift += inserted as isize - (e.to - e.from) as isize;
        cursor = to;
    }
    output.push_str(&content[cursor..]);
    Ok(TransactionResult {
        revision: revision(&output),
        changed: output != content,
        content: output,
        inverse,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn tx(source: &str, edits: Vec<TextEdit>) -> ContentTransaction {
        ContentTransaction {
            base_revision: revision(source),
            origin: "formula".into(),
            edits,
        }
    }
    #[test]
    fn unicode_edits_are_one_invertible_transaction() {
        let source = "甲𝛂 + x\r\n";
        let result = apply(
            source,
            &tx(
                source,
                vec![
                    TextEdit {
                        from: 1,
                        to: 3,
                        insert: "θ".into(),
                    },
                    TextEdit {
                        from: 6,
                        to: 7,
                        insert: "y²".into(),
                    },
                ],
            ),
        )
        .unwrap();
        assert_eq!(result.content, "甲θ + y²\r\n");
        assert_eq!(
            apply(&result.content, &tx(&result.content, result.inverse))
                .unwrap()
                .content,
            source
        );
    }
    #[test]
    fn stale_overlapping_and_surrogate_edits_are_rejected() {
        let source = "𝛂x";
        assert_eq!(
            apply(source, &tx("old", vec![])).unwrap_err(),
            "transaction_conflict"
        );
        assert_eq!(
            byte_offset(source, 1).unwrap_err(),
            "transaction_invalid_range"
        );
        let e = TextEdit {
            from: 0,
            to: 2,
            insert: "a".into(),
        };
        assert_eq!(
            apply(source, &tx(source, vec![e.clone(), e])).unwrap_err(),
            "transaction_overlapping_edits"
        );
        assert!(!apply(source, &tx(source, vec![])).unwrap().changed);
    }

    #[test]
    fn adjacent_deletions_return_an_applicable_inverse() {
        let source = "😀ab!";
        for replacement in ["", "β😀"] {
            let result = apply(source, &tx(source, vec![
                TextEdit { from: 2, to: 3, insert: String::new() },
                TextEdit { from: 3, to: 4, insert: replacement.into() },
            ])).unwrap();
            assert_eq!(result.content, format!("😀{replacement}!"));
            let restored = apply(&result.content, &tx(&result.content, result.inverse)).unwrap();
            assert_eq!(restored.content, source);
            assert_eq!(
                apply(&restored.content, &tx(&restored.content, restored.inverse)).unwrap().content,
                result.content
            );
        }
    }

    #[test]
    fn raw_fragments_retain_sorted_forward_correspondence() {
        let source = "😀ab!";
        let forward = tx(source, vec![
            TextEdit { from: 3, to: 4, insert: "β😀".into() },
            TextEdit { from: 2, to: 3, insert: String::new() },
        ]);
        let raw = apply_with_inverse_fragments(source, &forward).unwrap();
        assert_eq!(raw.content, "😀β😀!");
        assert_eq!(raw.inverse, vec![
            TextEdit { from: 2, to: 2, insert: "a".into() },
            TextEdit { from: 2, to: 5, insert: "b".into() },
        ]);
        // Review validates these two fragments against two forward inserts;
        // they are deliberately not a valid client transaction on their own.
        assert_eq!(
            apply(&raw.content, &tx(&raw.content, raw.inverse.clone())).unwrap_err(),
            "transaction_overlapping_edits"
        );
        let inverse = canonical_inverse(raw.inverse).unwrap();
        assert_eq!(inverse, vec![TextEdit { from: 2, to: 5, insert: "ab".into() }]);
        let public = apply(source, &forward).unwrap();
        assert_eq!(public.content, raw.content);
        assert_eq!(public.inverse, inverse);
        assert_eq!(
            apply(&raw.content, &tx(&raw.content, inverse)).unwrap().content,
            source
        );
    }

    #[test]
    fn collapsed_prefix_chain_preserves_bom_crlf_and_replacement() {
        let source = "\u{feff}😀abc\r\n!";
        let after = "\u{feff}😀β😀\r\n!";
        let result = apply(source, &tx(source, vec![
            TextEdit { from: 3, to: 4, insert: String::new() },
            TextEdit { from: 4, to: 5, insert: String::new() },
            TextEdit { from: 5, to: 6, insert: "β😀".into() },
        ])).unwrap();
        assert_eq!(result.content.as_bytes(), after.as_bytes());
        assert_eq!(result.inverse, vec![TextEdit { from: 3, to: 6, insert: "abc".into() }]);
        let restored = apply(after, &tx(after, result.inverse)).unwrap();
        assert_eq!(restored.content.as_bytes(), source.as_bytes());
        assert_eq!(
            apply(source, &tx(source, restored.inverse)).unwrap().content.as_bytes(),
            after.as_bytes()
        );
    }

    #[test]
    fn deletion_followed_by_boundary_insertion_has_one_applicable_inverse() {
        let source = "😀ab!";
        let result = apply(source, &tx(source, vec![
            TextEdit { from: 2, to: 3, insert: String::new() },
            TextEdit { from: 3, to: 3, insert: "XY".into() },
        ])).unwrap();
        assert_eq!(result.content, "😀XYb!");
        assert_eq!(result.inverse, vec![TextEdit { from: 2, to: 4, insert: "a".into() }]);
        assert_eq!(
            apply(&result.content, &tx(&result.content, result.inverse)).unwrap().content,
            source
        );
    }

    #[test]
    fn disjoint_inverses_remain_distinct() {
        let source = "😀ab!cd";
        let result = apply(source, &tx(source, vec![
            TextEdit { from: 2, to: 3, insert: String::new() },
            TextEdit { from: 5, to: 6, insert: "X".into() },
        ])).unwrap();
        assert_eq!(result.content, "😀b!Xd");
        assert_eq!(result.inverse, vec![
            TextEdit { from: 2, to: 2, insert: "a".into() },
            TextEdit { from: 4, to: 5, insert: "c".into() },
        ]);
        assert_eq!(
            apply(&result.content, &tx(&result.content, result.inverse)).unwrap().content,
            source
        );
    }

    #[test]
    fn canonical_inverse_rejects_unrelated_overlap_or_reordering() {
        for fragments in [
            vec![
                TextEdit { from: 2, to: 4, insert: "a".into() },
                TextEdit { from: 2, to: 2, insert: "b".into() },
            ],
            vec![
                TextEdit { from: 2, to: 4, insert: "a".into() },
                TextEdit { from: 3, to: 5, insert: "b".into() },
            ],
            vec![
                TextEdit { from: 4, to: 4, insert: "a".into() },
                TextEdit { from: 2, to: 2, insert: "b".into() },
            ],
            vec![TextEdit { from: 3, to: 2, insert: "a".into() }],
        ] {
            assert_eq!(canonical_inverse(fragments).unwrap_err(), "transaction_overlapping_edits");
        }
        let adjacent = vec![
            TextEdit { from: 2, to: 4, insert: "a".into() },
            TextEdit { from: 4, to: 4, insert: "b".into() },
        ];
        assert_eq!(canonical_inverse(adjacent.clone()).unwrap(), adjacent);
    }

    #[test]
    fn client_same_start_edits_are_not_canonicalized() {
        let source = "😀ab!";
        for edits in [
            vec![
                TextEdit { from: 2, to: 2, insert: "X".into() },
                TextEdit { from: 2, to: 3, insert: "Y".into() },
            ],
            vec![
                TextEdit { from: 2, to: 2, insert: "X".into() },
                TextEdit { from: 2, to: 2, insert: "Y".into() },
            ],
            vec![
                TextEdit { from: 2, to: 4, insert: "X".into() },
                TextEdit { from: 3, to: 4, insert: "Y".into() },
            ],
        ] {
            let request = tx(source, edits);
            assert_eq!(apply(source, &request).unwrap_err(), "transaction_overlapping_edits");
            assert_eq!(apply_with_inverse_fragments(source, &request).unwrap_err(), "transaction_overlapping_edits");
        }
    }

    #[test]
    fn canonical_inverse_operation_budget_is_unchanged() {
        let fragments = (0..=MAX_OPERATIONS).map(|point| TextEdit {
            from: point, to: point, insert: String::new(),
        }).collect();
        assert_eq!(canonical_inverse(fragments).unwrap_err(), "transaction_budget");
    }

    #[test]
    fn every_valid_two_edit_utf16_transaction_has_closed_inverse() {
        let source = "\u{feff}😀ab\r\n";
        let boundaries = [0, 1, 3, 4, 5, 6, 7];
        let replacements = ["", "β😀", "|"];
        let mut checked = 0;
        for first_from in 0..boundaries.len() {
            for first_to in first_from..boundaries.len() {
                for second_from in first_to..boundaries.len() {
                    if second_from == first_from {
                        continue;
                    }
                    for second_to in second_from..boundaries.len() {
                        for first_insert in replacements {
                            for second_insert in replacements {
                                let forward = tx(source, vec![
                                    TextEdit { from: boundaries[first_from], to: boundaries[first_to], insert: first_insert.into() },
                                    TextEdit { from: boundaries[second_from], to: boundaries[second_to], insert: second_insert.into() },
                                ]);
                                let result = apply(source, &forward).unwrap();
                                let restored = apply(&result.content, &tx(&result.content, result.inverse)).unwrap();
                                assert_eq!(restored.content.as_bytes(), source.as_bytes());
                                let redone = apply(source, &tx(source, restored.inverse)).unwrap();
                                assert_eq!(redone.content, result.content);
                                checked += 1;
                            }
                        }
                    }
                }
            }
        }
        assert_eq!(checked, 1638);
    }
}
