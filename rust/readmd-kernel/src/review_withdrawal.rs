//! Restore collapsed review edits using the proposal's source order and identity.

use super::{
    compensation_mappings, ensure_text_context, map_geometry, mapping_at, review_store, shifted,
    slice, Action, Change, CompensationSource, Document, Mapping, Patch, TextEdit,
};
use rusqlite::Transaction;
use serde_json::Value;
use std::collections::HashMap;

const UNPROVEN_GROUP: &str = "transaction_overlapping_edits";

struct SourcePiece {
    event: u64,
    index: usize,
    from: usize,
    to: usize,
    output_from: usize,
    output_to: usize,
}

type RestoredRanges = HashMap<String, (usize, usize)>;

pub(super) fn plan(
    tx: &Transaction<'_>,
    document: &Document,
    mut changes: Vec<&Change>,
) -> Result<(Vec<TextEdit>, RestoredRanges), &'static str> {
    changes.sort_unstable_by_key(|change| (change.from, change.to));
    let groups = changes.chunk_by(|left, right| left.from == right.from);
    let members: HashMap<_, _> = groups
        .clone()
        .filter(|group| group.len() > 1)
        .flatten()
        .map(|&change| (change.id.as_str(), change))
        .collect();
    let pieces = proposal_sources(tx, document, &members)?;
    let mut edits = Vec::new();
    let mut ranges = HashMap::new();
    let mut shift = 0isize;
    for group in groups {
        let from = group[0].from;
        let to = group.last().unwrap().to;
        if edits
            .last()
            .is_some_and(|previous: &TextEdit| from < previous.to)
        {
            return Err(UNPROVEN_GROUP);
        }
        let insert = if let [change] = group {
            change
                .restore_original
                .as_ref()
                .unwrap_or(&change.original)
                .as_str()
                .into()
        } else {
            // A separate owner at the same point may belong to another source
            // frame or to an omitted piece of this proposal.
            if document.changes.iter().any(|change| {
                change.from == from
                    && change.application == "applied"
                    && change.decision != "withdrawn"
                    && !members.contains_key(change.id.as_str())
            }) {
                return Err(UNPROVEN_GROUP);
            }
            let ordered = source_order(document, group, &pieces)?;
            let mut position = shifted(from, shift);
            let mut insert = String::new();
            for change in ordered {
                let original = change.original.as_str();
                let end = position + original.encode_utf16().count();
                ranges.insert(change.id.clone(), (position, end));
                position = end;
                insert.push_str(original);
            }
            insert
        };
        shift += insert.encode_utf16().count() as isize - (to - from) as isize;
        edits.push(TextEdit { from, to, insert });
    }
    Ok((edits, ranges))
}

fn proposal_sources<'a>(
    tx: &Transaction<'_>,
    document: &Document,
    members: &HashMap<&'a str, &Change>,
) -> Result<HashMap<&'a str, SourcePiece>, &'static str> {
    let mut sources = HashMap::new();
    if members.is_empty() {
        return Ok(sources);
    }
    // Stream the bounded review store once. The proposal survives even when
    // Undo/Redo moves its action out of the undo stack.
    review_store::visit_proposals(tx, &document.document_id, |event, payload| {
        let record: Value =
            serde_json::from_str(payload).map_err(|_| "review_persistence_failed")?;
        let Some(patches) = record["patches"].as_array() else {
            return Ok(false);
        };
        let created: Vec<_> = patches
            .iter()
            .filter_map(|patch| {
                if !patch["before"].is_null() {
                    return None;
                }
                let (&id, &change) = members.get_key_value(patch["id"].as_str()?)?;
                Some((id, change, &patch["after"]))
            })
            .collect();
        if created.is_empty() {
            return Ok(false);
        }
        let mapping = mapping_at(document, event).ok_or(UNPROVEN_GROUP)?;
        let index = index_source_pieces(mapping)?;
        ensure_text_context(tx, payload)?;
        for (id, current, after) in created {
            let after: Change =
                serde_json::from_value(after.clone()).map_err(|_| "review_persistence_failed")?;
            if after.id != id
                || after.last_event_id != event
                || after.application != "applied"
                || after.decision != "pending"
                || after.taken_over
                || after.origin != current.origin
                || after.task_id != current.task_id
                || after.original != current.original
                || after.replacement != current.replacement
                || current.restore_original.is_some()
                || after.original.as_str().is_empty()
            {
                return Err(UNPROVEN_GROUP);
            }
            let index = index
                .get(&(
                    after.from,
                    after.to,
                    after.original.as_str(),
                    after.replacement.as_str(),
                ))
                .copied()
                .flatten()
                .ok_or(UNPROVEN_GROUP)?;
            let edit = &mapping.edits[index];
            if sources
                .insert(
                    id,
                    SourcePiece {
                        event,
                        index,
                        from: edit.from,
                        to: edit.to,
                        output_from: after.from,
                        output_to: after.to,
                    },
                )
                .is_some()
            {
                return Err(UNPROVEN_GROUP);
            }
        }
        Ok(sources.len() == members.len())
    })?;
    if sources.len() != members.len() {
        return Err(UNPROVEN_GROUP);
    }
    Ok(sources)
}

type PieceIndex<'a> = HashMap<(usize, usize, &'a str, &'a str), Option<usize>>;

fn index_source_pieces(mapping: &Mapping) -> Result<PieceIndex<'_>, &'static str> {
    if mapping.edits.len() != mapping.inverse.len() {
        return Err(UNPROVEN_GROUP);
    }
    let mut index = HashMap::new();
    let mut shift = 0isize;
    for (position, (edit, inverse)) in mapping.edits.iter().zip(&mapping.inverse).enumerate() {
        let removed = edit.to.checked_sub(edit.from).ok_or(UNPROVEN_GROUP)?;
        let inserted = edit.insert.encode_utf16().count();
        if inverse.insert.encode_utf16().count() != removed
            || edit.from.checked_add_signed(shift) != Some(inverse.from)
            || inverse.to.checked_sub(inverse.from) != Some(inserted)
        {
            return Err(UNPROVEN_GROUP);
        }
        let key = (
            inverse.from,
            inverse.to,
            inverse.insert.as_str(),
            edit.insert.as_str(),
        );
        index
            .entry(key)
            .and_modify(|unique| *unique = None)
            .or_insert(Some(position));
        shift += inserted as isize - removed as isize;
    }
    Ok(index)
}

fn source_order<'a>(
    document: &Document,
    group: &[&'a Change],
    pieces: &HashMap<&str, SourcePiece>,
) -> Result<Vec<&'a Change>, &'static str> {
    let mut ordered = group.to_vec();
    ordered.sort_unstable_by_key(|change| pieces[change.id.as_str()].index);
    let first = &pieces[ordered[0].id.as_str()];
    let last = &pieces[ordered.last().unwrap().id.as_str()];
    for pair in ordered.windows(2) {
        let left = &pieces[pair[0].id.as_str()];
        let right = &pieces[pair[1].id.as_str()];
        if left.event != right.event
            || left.index + 1 != right.index
            || left.to != right.from
            || left.output_from != right.output_from
            || left.output_to != right.output_from
        {
            return Err(UNPROVEN_GROUP);
        }
    }
    let (mut from, mut to) = (first.output_from, last.output_to);
    let (mut anchor, mut end) = (from, from);
    for index in compensation_mappings(&document.mappings, first.event)? {
        let edits = &document.mappings[index].edits;
        if map_geometry(&mut anchor, &mut end, edits) || map_geometry(&mut from, &mut to, edits) {
            return Err(UNPROVEN_GROUP);
        }
    }
    for (change, expected_to) in ordered
        .iter()
        .zip(std::iter::repeat_n(from, ordered.len() - 1).chain(std::iter::once(to)))
    {
        if change.from != anchor || change.from != from || change.to != expected_to {
            return Err(UNPROVEN_GROUP);
        }
    }
    Ok(ordered)
}

pub(super) fn compensated_anchor(
    document: &Document,
    target: &Action,
    patch: &Patch,
    current: Option<&Change>,
    sources: &[CompensationSource<'_>],
    effective_mappings: &[usize],
) -> Result<Option<usize>, &'static str> {
    let (Some(before), Some(after), Some(current)) = (&patch.before, &patch.after, current) else {
        return Ok(None);
    };
    if before.from != before.to
        || before.application != "applied"
        || before.taken_over
        || before.decision == "withdrawn"
        || !before.replacement.as_str().is_empty()
        || after.decision != "withdrawn"
        || current.decision != "withdrawn"
        || after.from >= after.to
        || current.original != before.original
    {
        return Ok(None);
    }
    let (mut from, mut to) = (after.from, after.to);
    for &index in effective_mappings {
        if map_geometry(&mut from, &mut to, &document.mappings[index].edits) {
            return Ok(None);
        }
    }
    if (from, to) != (current.from, current.to) {
        return Ok(None);
    }
    // The withdrawn span proves which executed edit owns this empty anchor.
    // That edit's exact inverse restores its original edge as a single point.
    for ((source, inverse), edit) in sources.iter().zip(&target.inverse).zip(&target.edits) {
        if !source.exact
            || after.from < inverse.from
            || inverse.to < after.to
            || (before.from != source.from && before.from != source.to)
        {
            continue;
        }
        let piece = slice(
            &edit.insert,
            after.from - inverse.from,
            after.to - inverse.from,
        )?;
        if piece == before.original.as_str() {
            return Ok(Some(if before.from == source.from {
                source.output_from
            } else {
                source.output_to
            }));
        }
    }
    Ok(None)
}
