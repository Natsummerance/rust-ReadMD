//! Recover the most recent media selection and cue edits across app restarts.
use super::selection::{Cue, Selection, MAX_TASK_SELECTION_MS, MAX_TIMESTAMP_MS};
use crate::{
    document_operations::{self, Observation},
    server::Response,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{path::Path, time::Duration};

const MAX_BYTES: u64 = 950 * 1024;
const FILE: &str = "media-transcribe-draft.json";

#[derive(Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Range {
    track: u32,
    from: String,
    to: String,
    language: String,
}

#[derive(Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Transcript {
    name: String,
    track: u32,
    start_ms: u64,
    end_ms: u64,
    language: Option<String>,
    engine: String,
    cues: Vec<Cue>,
}

impl Transcript {
    fn valid(&self) -> bool {
        !self.name.is_empty()
            && self.name.len() <= 4096
            && valid_engine(&self.engine)
            && self
                .language
                .as_ref()
                .is_none_or(|language| language.len() <= 64)
            && self.start_ms < self.end_ms
            && self.end_ms <= MAX_TIMESTAMP_MS
            && self.end_ms - self.start_ms <= MAX_TASK_SELECTION_MS
            && !self.cues.is_empty()
            && self.cues.iter().all(|cue| {
                self.start_ms <= cue.start_ms
                    && cue.start_ms < cue.end_ms
                    && cue.end_ms <= self.end_ms
            })
            && self
                .cues
                .windows(2)
                .all(|pair| pair[0].start_ms <= pair[1].start_ms)
    }

    fn same_timing(&self, other: &Self) -> bool {
        self.name == other.name
            && self.track == other.track
            && self.start_ms == other.start_ms
            && self.end_ms == other.end_ms
            && self.language == other.language
            && self.engine == other.engine
            && self.cues.len() == other.cues.len()
            && self
                .cues
                .iter()
                .zip(&other.cues)
                .all(|(left, right)| left.start_ms == right.start_ms && left.end_ms == right.end_ms)
    }
}

#[derive(Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Draft {
    path: String,
    source_revision: String,
    range: Range,
    transcript: Option<Transcript>,
}

impl Draft {
    fn valid(&self) -> bool {
        Path::new(&self.path).is_absolute()
            && super::is_supported_media(&self.path)
            && self.path.len() <= 32_768
            && !self.path.contains('\0')
            && self.source_revision.len() == 64
            && self.source_revision.bytes().all(|b| b.is_ascii_hexdigit())
            && self.range.from.len() <= 64
            && self.range.to.len() <= 64
            && self.range.language.len() <= 64
            && self.transcript.as_ref().is_none_or(Transcript::valid)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RunnerIdentity {
    pub(super) engine: String,
    pub(super) recognizer_revision: String,
    pub(super) decoder_revision: String,
}

impl RunnerIdentity {
    fn valid(&self) -> bool {
        valid_engine(&self.engine)
            && [&self.recognizer_revision, &self.decoder_revision]
                .iter()
                .all(|value| !value.is_empty() && value.len() <= 256)
    }
}

fn valid_engine(engine: &str) -> bool {
    matches!(engine, "system-speech" | "whisper.cpp")
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Checkpoint {
    pub(super) id: String,
    pub(super) selection: Selection,
    pub(super) language: String,
    pub(super) runner: RunnerIdentity,
    pub(super) resolved_language: Option<String>,
    pub(super) next_ms: u64,
    pub(super) done: bool,
    has_cues: bool,
}

impl Checkpoint {
    fn valid(&self, draft: &Draft) -> bool {
        crate::cancel::valid_id(&self.id)
            && self.selection.valid(MAX_TASK_SELECTION_MS)
            && self.selection.start_ms <= self.next_ms
            && self.next_ms <= self.selection.end_ms
            && (self.done || self.next_ms < self.selection.end_ms)
            && valid_language(&self.language)
            && self.runner.valid()
            && self.resolved_language.as_deref().is_none_or(valid_language)
            && (!self.has_cues
                || draft.transcript.as_ref().is_some_and(|transcript| {
                    transcript.track == self.selection.track
                        && transcript.start_ms == self.selection.start_ms
                        && transcript.end_ms == self.next_ms
                        && transcript.language == self.resolved_language
                }))
    }
}

fn valid_language(language: &str) -> bool {
    !language.is_empty()
        && language.len() <= 64
        && language
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Record {
    schema: u8,
    revision: Option<String>,
    draft: Option<Draft>,
    #[serde(default)]
    pub(super) checkpoint: Option<Checkpoint>,
}

impl Record {
    pub(super) fn response(&self) -> Response {
        Response::json(&json!({
            "ok": true, "revision": self.revision, "draft": self.draft, "checkpoint": self.checkpoint
        })).header("Cache-Control", "no-store")
    }
}

fn read(path: &Path) -> Result<(Observation, Record), &'static str> {
    let current =
        document_operations::observe(path, MAX_BYTES).map_err(|_| "media_draft_unavailable")?;
    let record = match &current {
        Observation::Missing => Record {
            schema: 1,
            revision: None,
            draft: None,
            checkpoint: None,
        },
        Observation::Regular(file) => {
            let record: Record =
                serde_json::from_slice(file.bytes()).map_err(|_| "media_draft_unavailable")?;
            if record.schema != 1
                || !record
                    .revision
                    .as_deref()
                    .is_some_and(crate::cancel::valid_id)
                || !record.draft.as_ref().is_none_or(Draft::valid)
                || !record.checkpoint.as_ref().is_none_or(|checkpoint| {
                    record
                        .draft
                        .as_ref()
                        .is_some_and(|draft| checkpoint.valid(draft))
                })
            {
                return Err("media_draft_unavailable");
            }
            record
        }
        Observation::Unsupported(_) => return Err("media_draft_unavailable"),
    };
    Ok((current, record))
}

fn save(profile: &Path, body: &Value) -> Result<Record, &'static str> {
    let incoming: Option<Draft> =
        serde_json::from_value(body.get("draft").ok_or("media_draft_invalid")?.clone())
            .map_err(|_| "media_draft_invalid")?;
    if !incoming.as_ref().is_none_or(Draft::valid) {
        return Err("media_draft_invalid");
    }
    let expected = match body.get("revision") {
        Some(Value::Null) => None,
        Some(Value::String(value)) if crate::cancel::valid_id(value) => Some(value.as_str()),
        _ => return Err("media_draft_invalid"),
    };
    update(profile, |stored| {
        // Retry after an interrupted response without creating a second version.
        if stored.draft == incoming {
            return Ok(false);
        }
        if stored.revision.as_deref() != expected {
            return Err("media_draft_conflict");
        }
        let same_source = stored
            .draft
            .as_ref()
            .zip(incoming.as_ref())
            .is_some_and(|(old, new)| {
                old.path == new.path && old.source_revision == new.source_revision
            });
        if !same_source {
            stored.checkpoint = None;
        } else if stored
            .checkpoint
            .as_ref()
            .is_some_and(|checkpoint| checkpoint.has_cues)
        {
            let old = stored
                .draft
                .as_ref()
                .and_then(|draft| draft.transcript.as_ref());
            let new = incoming
                .as_ref()
                .and_then(|draft| draft.transcript.as_ref());
            if !old
                .zip(new)
                .is_some_and(|(left, right)| left.same_timing(right))
            {
                return Err("media_draft_invalid");
            }
        }
        stored.draft = incoming;
        Ok(true)
    })
}

fn update(
    profile: &Path,
    action: impl FnOnce(&mut Record) -> Result<bool, &'static str>,
) -> Result<Record, &'static str> {
    let guard = document_operations::acquire(profile, Duration::from_secs(2))
        .map_err(|_| "media_draft_unavailable")?;
    let target = profile.join(FILE);
    let (current, mut record) = read(&target)?;
    if !action(&mut record)? {
        return Ok(record);
    }
    record.revision = Some(uuid::Uuid::new_v4().to_string());
    let serialized = serde_json::to_vec(&record).map_err(|_| "media_draft_invalid")?;
    if serialized.len() as u64 > MAX_BYTES {
        return Err("media_draft_invalid");
    }
    match current {
        Observation::Regular(file) => {
            guard
                .verify_existing(&target, file.fingerprint(), MAX_BYTES)
                .map_err(|_| "media_draft_conflict")?;
        }
        Observation::Missing => guard
            .verify_missing(&target)
            .map_err(|_| "media_draft_conflict")?,
        Observation::Unsupported(_) => unreachable!(),
    }
    crate::content::write_bytes_atomic(&target, &serialized)
        .map_err(|_| "media_draft_unavailable")?;
    Ok(record)
}

#[derive(Deserialize)]
pub(super) struct StepRequest {
    checkpoint_id: Option<String>,
    cursor_ms: Option<u64>,
    draft_revision: Option<String>,
}

pub(super) fn saved_runner(
    profile: &Path,
    request: &StepRequest,
) -> Result<Option<RunnerIdentity>, &'static str> {
    let Some(id) = &request.checkpoint_id else {
        return Ok(None);
    };
    let (_, record) = read(&profile.join(FILE))?;
    let checkpoint = record
        .checkpoint
        .filter(|checkpoint| &checkpoint.id == id)
        .ok_or("media_resume_conflict")?;
    Ok(Some(checkpoint.runner))
}

pub(super) fn start_step(
    profile: &Path,
    path: &str,
    source_revision: &str,
    selection: &Selection,
    language: &str,
    runner: &RunnerIdentity,
    request: &StepRequest,
) -> Result<(Record, bool), &'static str> {
    if !valid_language(language) {
        return Err("media_selection_invalid");
    }
    let stored = update(profile, |record| {
        if crate::speech::cancelled() {
            return Err("cancelled");
        }
        let draft = record.draft.as_ref().ok_or("media_draft_conflict")?;
        if draft.path != path || draft.source_revision != source_revision {
            return Err("media_source_changed");
        }
        if let Some(id) = &request.checkpoint_id {
            let checkpoint = record.checkpoint.as_ref().ok_or("media_resume_conflict")?;
            if checkpoint.id != *id
                || checkpoint.selection != *selection
                || checkpoint.language != language
                || checkpoint.runner != *runner
                || request
                    .cursor_ms
                    .is_none_or(|cursor| cursor < selection.start_ms || cursor > checkpoint.next_ms)
            {
                return Err("media_resume_conflict");
            }
            return Ok(false);
        }
        if request.cursor_ms.is_some() || record.revision != request.draft_revision {
            return Err("media_draft_conflict");
        }
        if let Some(checkpoint) = record.checkpoint.as_ref().filter(|checkpoint| {
            checkpoint.selection == *selection && checkpoint.language == language
        }) {
            return if checkpoint.done {
                Ok(false)
            } else {
                Err("media_resume_required")
            };
        }
        record.checkpoint = Some(Checkpoint {
            id: uuid::Uuid::new_v4().to_string(),
            selection: *selection,
            language: language.into(),
            runner: runner.clone(),
            resolved_language: None,
            next_ms: selection.start_ms,
            done: false,
            has_cues: false,
        });
        Ok(true)
    })?;
    let checkpoint = stored.checkpoint.as_ref().ok_or("media_resume_conflict")?;
    let run = !checkpoint.done
        && request
            .cursor_ms
            .is_none_or(|cursor| cursor == checkpoint.next_ms);
    Ok((stored, run))
}

pub(super) struct Chunk {
    pub(super) next_ms: u64,
    pub(super) done: bool,
    pub(super) language: Option<String>,
    pub(super) cues: Vec<Cue>,
}

pub(super) fn accept_chunk(
    profile: &Path,
    step: &Checkpoint,
    chunk: Chunk,
) -> Result<Record, &'static str> {
    update(profile, |record| {
        if crate::speech::cancelled() {
            return Err("cancelled");
        }
        let checkpoint = record.checkpoint.as_mut().ok_or("media_resume_conflict")?;
        if checkpoint.id != step.id {
            return Err("media_resume_conflict");
        }
        if checkpoint.done || checkpoint.next_ms > step.next_ms {
            return Ok(false);
        }
        if checkpoint != step
            || chunk.next_ms < step.next_ms
            || chunk.next_ms > step.selection.end_ms
            || (!chunk.done && chunk.next_ms == step.next_ms)
            || (step.resolved_language.is_some() && step.resolved_language != chunk.language)
            || !chunk.language.as_deref().is_none_or(valid_language)
            || chunk.cues.iter().any(|cue| {
                cue.start_ms < step.next_ms
                    || cue.start_ms >= cue.end_ms
                    || cue.end_ms > chunk.next_ms
            })
        {
            return Err("media_resume_conflict");
        }
        let draft = record.draft.as_mut().ok_or("media_draft_conflict")?;
        if checkpoint.has_cues {
            let transcript = draft.transcript.as_mut().ok_or("media_draft_conflict")?;
            transcript.cues.extend(chunk.cues);
            transcript.end_ms = chunk.next_ms;
        } else if !chunk.cues.is_empty() {
            draft.transcript = Some(Transcript {
                name: super::basename(&draft.path),
                track: checkpoint.selection.track,
                start_ms: checkpoint.selection.start_ms,
                end_ms: chunk.next_ms,
                language: chunk.language.clone(),
                engine: checkpoint.runner.engine.clone(),
                cues: chunk.cues,
            });
            checkpoint.has_cues = true;
        }
        checkpoint.next_ms = chunk.next_ms;
        checkpoint.done = chunk.done || chunk.next_ms == checkpoint.selection.end_ms;
        checkpoint.resolved_language = chunk.language;
        if !checkpoint.valid(draft) {
            return Err("media_draft_invalid");
        }
        Ok(true)
    })
}

pub(crate) fn endpoint(profile: &Path, operation: &str, body: &Value) -> Response {
    let result = if operation == "draft_load" {
        read(&profile.join(FILE)).map(|(_, record)| record)
    } else {
        save(profile, body)
    };
    match result {
        Ok(record) => record.response(),
        Err(code) => {
            let status = match code {
                "media_draft_conflict" => 409,
                "media_draft_invalid" => 400,
                _ => 507,
            };
            Response::json_status(status, &json!({"ok": false, "error_code": code}))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edited_draft_reopens_without_the_source_and_stale_writer_cannot_replace_it() {
        let directory = tempfile::tempdir().unwrap();
        let profile = directory.path().canonicalize().unwrap();
        let source = profile.join("removed.mkv");
        let draft = json!({
            "path":source,"source_revision":"a".repeat(64),"range":{"track":2,"from":"5.25","to":"8","language":"en-US"},
            "transcript":{"name":"removed.mkv","track":2,"start_ms":5250,"end_ms":8000,"language":"en-US",
                "engine":"system-speech","cues":[{"start_ms":5300,"end_ms":6100,"text":"corrected cue"}]}
        });
        let first = save(&profile, &json!({"draft":draft,"revision":null})).unwrap();
        let retry = save(&profile, &json!({"draft":draft,"revision":null})).unwrap();
        assert_eq!(retry.revision, first.revision);
        let reopened = read(&profile.join(FILE)).unwrap().1;
        assert_eq!(reopened.revision, first.revision);
        assert_eq!(
            reopened.draft.unwrap().transcript.unwrap().cues[0].text,
            "corrected cue"
        );
        assert!(!source.exists());
        assert_eq!(
            save(&profile, &json!({"draft":null,"revision":null})).unwrap_err(),
            "media_draft_conflict"
        );
        let removed = save(&profile, &json!({"draft":null,"revision":first.revision})).unwrap();
        assert!(removed.draft.is_none());
        assert!(read(&profile.join(FILE)).unwrap().1.draft.is_none());
    }

    #[test]
    fn checkpoint_resumes_exactly_and_keeps_first_chunk_edits_after_a_silent_chunk() {
        let dir = tempfile::tempdir().unwrap();
        let profile = dir.path().canonicalize().unwrap();
        let source = profile.join("long.mkv");
        let path = source.to_str().unwrap();
        let source_revision = "b".repeat(64);
        let draft = json!({
            "path": path, "source_revision": source_revision,
            "range": {"track": 2, "from": "1.25", "to": "361.25", "language": "auto"},
            "transcript": null
        });
        let initial = save(&profile, &json!({"draft":draft,"revision":null})).unwrap();
        let selection = Selection {
            track: 2,
            start_ms: 1250,
            end_ms: 361250,
        };
        let runner = RunnerIdentity {
            engine: "system-speech".into(),
            recognizer_revision: "system".into(),
            decoder_revision: "decoder 1".into(),
        };
        let new = StepRequest {
            checkpoint_id: None,
            cursor_ms: None,
            draft_revision: initial.revision,
        };
        let (begun, can_run) = start_step(
            &profile,
            path,
            &source_revision,
            &selection,
            "auto",
            &runner,
            &new,
        )
        .unwrap();
        assert!(can_run);
        let first = begun.checkpoint.unwrap();
        let at_first = StepRequest {
            checkpoint_id: Some(first.id.clone()),
            cursor_ms: Some(1250),
            draft_revision: None,
        };
        let first_chunk = || Chunk {
            next_ms: 121250,
            done: false,
            language: Some("en-US".into()),
            cues: vec![Cue {
                start_ms: 1270,
                end_ms: 2750,
                text: "First cue".into(),
            }],
        };
        let interrupted = crate::speech::with_cancel(std::sync::Arc::new(|| true), || {
            accept_chunk(&profile, &first, first_chunk())
        });
        assert_eq!(interrupted.unwrap_err(), "cancelled");
        assert_eq!(
            read(&profile.join(FILE))
                .unwrap()
                .1
                .checkpoint
                .unwrap()
                .next_ms,
            1250
        );

        let recorded = accept_chunk(&profile, &first, first_chunk()).unwrap();
        let retry = accept_chunk(&profile, &first, first_chunk()).unwrap();
        assert_eq!(recorded.revision, retry.revision);
        let (_, should_repeat) = start_step(
            &profile,
            path,
            &source_revision,
            &selection,
            "auto",
            &runner,
            &at_first,
        )
        .unwrap();
        assert!(!should_repeat);
        let mut edited = serde_json::to_value(recorded.draft).unwrap();
        edited["transcript"]["cues"][0]["text"] = json!("User corrected first cue");
        save(
            &profile,
            &json!({"draft":edited,"revision":recorded.revision}),
        )
        .unwrap();

        let resumed = StepRequest {
            checkpoint_id: Some(first.id.clone()),
            cursor_ms: Some(121250),
            draft_revision: None,
        };
        assert_eq!(
            saved_runner(&profile, &resumed).unwrap(),
            Some(runner.clone())
        );
        for mismatch in [
            Selection {
                end_ms: selection.end_ms - 1000,
                ..selection
            },
            Selection {
                track: 3,
                ..selection
            },
            Selection {
                start_ms: 0,
                ..selection
            },
        ] {
            assert_eq!(
                start_step(
                    &profile,
                    path,
                    &source_revision,
                    &mismatch,
                    "auto",
                    &runner,
                    &resumed
                )
                .unwrap_err(),
                "media_resume_conflict"
            );
        }
        assert_eq!(
            start_step(
                &profile,
                path,
                &source_revision,
                &selection,
                "en-US",
                &runner,
                &resumed
            )
            .unwrap_err(),
            "media_resume_conflict"
        );
        assert_eq!(
            start_step(
                &profile,
                path,
                &"c".repeat(64),
                &selection,
                "auto",
                &runner,
                &resumed
            )
            .unwrap_err(),
            "media_source_changed"
        );
        let changed_runner = RunnerIdentity {
            decoder_revision: "decoder 2".into(),
            ..runner.clone()
        };
        assert_eq!(
            start_step(
                &profile,
                path,
                &source_revision,
                &selection,
                "auto",
                &changed_runner,
                &resumed
            )
            .unwrap_err(),
            "media_resume_conflict"
        );
        let (reopened, run) = start_step(
            &profile,
            path,
            &source_revision,
            &selection,
            "auto",
            &runner,
            &resumed,
        )
        .unwrap();
        assert!(run);
        let second = reopened.checkpoint.unwrap();
        assert_eq!(second.resolved_language.as_deref(), Some("en-US"));
        let silent = accept_chunk(
            &profile,
            &second,
            Chunk {
                next_ms: 241250,
                done: false,
                language: Some("en-US".into()),
                cues: Vec::new(),
            },
        )
        .unwrap()
        .checkpoint
        .unwrap();
        let last = accept_chunk(
            &profile,
            &silent,
            Chunk {
                next_ms: 315000,
                done: true,
                language: Some("en-US".into()),
                cues: vec![Cue {
                    start_ms: 241250,
                    end_ms: 241900,
                    text: "Last cue".into(),
                }],
            },
        )
        .unwrap();
        assert!(last.checkpoint.as_ref().unwrap().done);
        let transcript = last.draft.unwrap().transcript.unwrap();
        assert_eq!(transcript.end_ms, 315000);
        assert_eq!(transcript.cues.len(), 2);
        assert_eq!(transcript.cues[0].text, "User corrected first cue");
        assert_eq!(transcript.cues[1].start_ms, 241250);
    }
}
