//! Ordinary-text offsets compose outside physical author font sizes. Native
//! code, equations and dedicated stories retain their own baseline. The final
//! PDF pipeline proves link ownership before it can move any annotation.
use super::*;
use crate::publication_typography::baseline::BaselineShiftPt;

impl Writer<'_> {
    pub(super) fn protected_baseline_object(&mut self, body: String) -> Result<String, String> {
        if self.baseline.is_none_or(|b| b.requested_pt() == 0.) || body.is_empty() {
            return Ok(body);
        }
        let block_id = self
            .source_id
            .clone()
            .filter(|id| !id.is_empty())
            .ok_or(crate::publication_typography::baseline::UNSUPPORTED)?;
        let nonce = self
            .mapping_nonce
            .ok_or(crate::publication_typography::baseline::UNSUPPORTED)?;
        if self.baseline_protected.len() >= 131072 {
            return Err("typst_output_limit".into());
        }
        let witness_id = format!("ink-{nonce}-{:08x}", self.baseline_protected.len());
        let uri = string(&format!(
            "{}{}",
            crate::publication_pagination::prefix(nonce),
            witness_id
        ));
        self.baseline_protected
            .push(crate::publication_pagination::BaselineProtectedRequest {
                block_id,
                witness_id,
            });
        Ok(format!("#link({uri})[{body}]"))
    }
    pub(super) fn body_paragraph(&mut self, values: &[Inline]) -> Result<String, String> {
        let mode = if self.ordinary_body && self.note_depth == 0 {
            self.local_line
                .or(crate::publication_typography::global_line_spacing(
                    self.style,
                )?)
        } else {
            None
        };
        let keep = self.local.as_ref().map_or_else(
            || {
                if self.note_depth > 0 {
                    crate::publication_layout::Layout::from_style(self.style).note_keep_paragraphs
                } else {
                    self.style["typography"]["keepLines"]
                        .as_bool()
                        .unwrap_or(false)
                }
            },
            |style| style.keep_lines,
        );
        if let Some(mode) = mode {
            let body = self.typed_paragraph(values, mode)?;
            Ok(if keep {
                format!("#block(breakable:false)[{body}]\n")
            } else {
                body
            })
        } else {
            let body = self.inlines(values)?;
            Ok(if keep {
                format!("#block(breakable:false)[#par[{body}]]\n")
            } else {
                format!("#par[{body}]\n")
            })
        }
    }

    pub(super) fn ordinary_text(&self, value: &str) -> Result<String, String> {
        let body = self.author_text(value, false)?;
        let Some(shift) = self.baseline.filter(|shift| shift.requested_pt() != 0.) else {
            return Ok(body);
        };
        if value.is_empty() {
            return Ok(body);
        }
        Ok(format!(
            "#text(baseline:{}pt)[{body}]",
            -shift.requested_pt()
        ))
    }

    pub(super) fn with_baseline<T>(
        &mut self,
        baseline: Option<BaselineShiftPt>,
        render: impl FnOnce(&mut Self) -> Result<T, String>,
    ) -> Result<T, String> {
        let previous = std::mem::replace(&mut self.baseline, baseline);
        let result = render(self);
        self.baseline = previous;
        result
    }

    pub(super) fn register_baseline_links(
        &mut self,
        values: &[Inline],
        shift: Option<BaselineShiftPt>,
    ) -> Result<(), String> {
        let Some(shift) = shift.filter(|shift| shift.requested_pt() != 0.) else {
            return Ok(());
        };
        let block_id = self
            .source_id
            .clone()
            .filter(|id| !id.is_empty())
            .ok_or(crate::publication_typography::baseline::UNSUPPORTED)?;
        let expected_links = shifted_link_count(values)?;
        if self.baseline_links.len() >= crate::publication_blocks::MAX_NODES
            || self
                .baseline_links
                .iter()
                .any(|request| request.block_id == block_id)
        {
            return Err("typst_invalid_content".into());
        }
        self.baseline_links
            .push(crate::publication_pagination::BaselineLinkRequest {
                block_id,
                rise_pt: shift.requested_pt(),
                expected_links,
            });
        Ok(())
    }
}

/// A native link can be moved as a whole only when all of its visible children
/// share the ordinary-text offset. Never infer this from its URL or copy text.
fn shifted_link_count(values: &[Inline]) -> Result<usize, String> {
    let mut count = 0usize;
    let mut pending = values
        .iter()
        .map(|value| (value, false))
        .collect::<Vec<_>>();
    let mut budget = 0usize;
    while let Some((value, inside_link)) = pending.pop() {
        if crate::speech::cancelled() {
            return Err("cancelled".into());
        }
        budget += 1;
        if budget > 100_000 {
            return Err("typst_output_limit".into());
        }
        match value {
            Inline::Link { children, .. } => {
                if inside_link || !has_visible_text(children) {
                    return Err(crate::publication_typography::baseline::UNSUPPORTED.into());
                }
                count = count.checked_add(1).ok_or("typst_output_limit")?;
                pending.extend(children.iter().map(|value| (value, true)));
            }
            Inline::Strong(children)
            | Inline::Emph(children)
            | Inline::AuthorStyled { children, .. } => {
                pending.extend(children.iter().map(|value| (value, inside_link)))
            }
            Inline::Styled { format, children } => {
                if format.vertical_align != crate::publication_rich::RichVerticalAlign::Baseline
                    || format.text_decoration != crate::publication_rich::RichDecoration::None
                {
                    return Err(crate::publication_typography::baseline::UNSUPPORTED.into());
                }
                pending.extend(children.iter().map(|value| (value, inside_link)));
            }
            Inline::Text(_) | Inline::SoftBreak | Inline::HardBreak => (),
            Inline::Html(value) if value.trim().is_empty() || value.trim().starts_with("<!--") => {
                ()
            }
            Inline::Code(_) | Inline::Math(_) | Inline::OfficeMath { .. } if !inside_link => (),
            _ => return Err(crate::publication_typography::baseline::UNSUPPORTED.into()),
        }
    }
    Ok(count)
}

fn has_visible_text(values: &[Inline]) -> bool {
    let mut pending = values.iter().collect::<Vec<_>>();
    let mut budget = 0usize;
    while let Some(value) = pending.pop() {
        budget += 1;
        if budget > 100_000 {
            return false;
        }
        match value {
            Inline::Text(value)
                if value
                    .chars()
                    .any(|ch| !ch.is_whitespace() && !ch.is_control()) =>
            {
                return true
            }
            Inline::Strong(children)
            | Inline::Emph(children)
            | Inline::AuthorStyled { children, .. }
            | Inline::Styled { children, .. } => pending.extend(children),
            _ => (),
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn shift(pt: f64) -> Option<BaselineShiftPt> {
        Some(crate::publication_typography::baseline::parse(&json!(pt)).unwrap())
    }
    fn link(values: Vec<Inline>) -> Inline {
        Inline::Link {
            href: "https://example.test/repeated".into(),
            title: String::new(),
            children: values,
        }
    }

    #[test]
    fn repeated_uri_counts_identities_and_does_not_move_protected_link_children() {
        let ordinary = link(vec![Inline::Strong(vec![Inline::Text("Same".into())])]);
        assert_eq!(
            shifted_link_count(&[ordinary.clone(), ordinary]).unwrap(),
            2
        );
        assert_eq!(
            shifted_link_count(&[
                Inline::Text("Body".into()),
                Inline::Code("CODE".into()),
                Inline::Math("x".into())
            ])
            .unwrap(),
            0
        );
        for child in [
            Inline::Code("CODE".into()),
            Inline::Math("x".into()),
            Inline::FootnoteRef("n".into()),
            Inline::Text("   \n".into()),
            link(vec![Inline::Text("nested".into())]),
        ] {
            assert_eq!(
                shifted_link_count(&[link(vec![child])]).unwrap_err(),
                crate::publication_typography::baseline::UNSUPPORTED
            );
        }
    }

    #[test]
    fn ordinary_offset_composes_outside_author_sizes_and_scope_restores_after_error() {
        let style = crate::export_styles::sanitize(&json!({}));
        let dir = tempfile::tempdir().unwrap();
        let mut writer = super::super::columns_tests::writer(&style, "", dir.path(), &[], None);
        writer.author = Some(ResolvedTextStyle::base(17., 23.));
        writer.baseline = shift(3.25);
        let ordinary = writer.ordinary_text("ABC").unwrap();
        assert!(ordinary.starts_with("#text(baseline:-3.25pt)["));
        assert!(ordinary.contains("#text(size:17pt)"));
        let code = writer.author_text("CODE", true).unwrap();
        assert!(!code.contains("baseline:"));
        let failure: Result<(), String> = writer.with_baseline(shift(-2.5), |nested| {
            assert!(nested
                .ordinary_text("Down")
                .unwrap()
                .starts_with("#text(baseline:2.5pt)"));
            Err("cancelled".into())
        });
        assert_eq!(failure.unwrap_err(), "cancelled");
        assert_eq!(writer.baseline, shift(3.25));
        writer.baseline = shift(0.);
        assert_eq!(
            writer.ordinary_text("ABC").unwrap(),
            writer.author_text("ABC", false).unwrap()
        );
    }

    #[test]
    fn source_identity_survives_styled_recursion_and_local_zero_without_text_matching() {
        let style = crate::export_styles::sanitize(&json!({"typography":{"baselineShiftPt":3}}));
        let dir = tempfile::tempdir().unwrap();
        let nonce = "0123456789abcdef0123456789abcdef";
        let paragraph = || Block::Paragraph(vec![Inline::Text("Same body".into())]);
        let marker = |id: &str| Block::Html(format!("<!-- readmd:page-source {nonce} {id} -->"));
        let blocks = vec![
            marker("first"),
            paragraph(),
            marker("reset"),
            Block::Styled {
                attributes: json!({"id":"local-reset","text":{"baselineShiftPt":0}}),
                block: Box::new(paragraph()),
            },
            marker("third"),
            Block::Styled {
                attributes: json!({"id":"local-third","text":{"baselineShiftPt":-2.5}}),
                block: Box::new(paragraph()),
            },
        ];
        let mut writer =
            super::super::columns_tests::writer(&style, "", dir.path(), &blocks, Some(nonce));
        let source = writer.blocks(&blocks).unwrap();
        assert_eq!(writer.baseline_links.len(), 2);
        assert_eq!(writer.baseline_links[0].block_id, "first");
        assert_eq!(writer.baseline_links[0].rise_pt, 3.);
        assert_eq!(writer.baseline_links[1].block_id, "third");
        assert_eq!(writer.baseline_links[1].rise_pt, -2.5);
        assert_eq!(writer.baseline_links[0].expected_links, 0);
        assert_eq!(source.matches("#text(baseline:-3pt)").count(), 1);
        assert_eq!(source.matches("#text(baseline:2.5pt)").count(), 1);
        assert!(writer.source_id.is_none());
        assert!(writer.baseline.is_none());
        assert!(writer.local_baseline.is_none());
    }

    #[test]
    fn missing_or_repeated_source_id_cannot_fall_back_to_url_or_text() {
        let style = crate::export_styles::sanitize(&json!({}));
        let dir = tempfile::tempdir().unwrap();
        let mut writer = super::super::columns_tests::writer(&style, "", dir.path(), &[], None);
        let values = vec![link(vec![Inline::Text("Same body".into())])];
        assert_eq!(
            writer
                .register_baseline_links(&values, shift(3.))
                .unwrap_err(),
            crate::publication_typography::baseline::UNSUPPORTED
        );
        writer.source_id = Some("unique".into());
        writer.register_baseline_links(&values, shift(3.)).unwrap();
        assert_eq!(
            writer
                .register_baseline_links(&values, shift(-2.5))
                .unwrap_err(),
            "typst_invalid_content"
        );
    }

    #[test]
    fn protected_code_and_math_witnesses_exist_only_in_nonzero_mapped_source_scope() {
        let style = crate::export_styles::sanitize(&json!({}));
        let dir = tempfile::tempdir().unwrap();
        let nonce = "0123456789abcdef0123456789abcdef";
        let mut writer =
            super::super::columns_tests::writer(&style, "", dir.path(), &[], Some(nonce));
        let code = writer.author_text("CODE", true).unwrap();
        assert_eq!(
            writer.protected_baseline_object(code.clone()).unwrap(),
            code
        );
        writer.baseline = shift(3.);
        assert!(writer.protected_baseline_object(code.clone()).is_err());
        writer.source_id = Some("second-repeat".into());
        let protected = writer.protected_baseline_object(code.clone()).unwrap();
        assert!(protected.contains(&code));
        assert!(!protected.contains("baseline:"));
        assert_eq!(writer.baseline_protected[0].block_id, "second-repeat");
        let math = writer
            .protected_baseline_object("#box(baseline:2pt)[SVG]".into())
            .unwrap();
        assert!(math.contains("#box(baseline:2pt)[SVG]"));
        assert_ne!(
            writer.baseline_protected[0].witness_id,
            writer.baseline_protected[1].witness_id
        );
        writer.baseline = shift(0.);
        assert_eq!(
            writer.protected_baseline_object(code.clone()).unwrap(),
            code
        );
        assert_eq!(writer.baseline_protected.len(), 2);
    }
}
