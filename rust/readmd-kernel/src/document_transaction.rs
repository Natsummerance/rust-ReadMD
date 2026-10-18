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
}
