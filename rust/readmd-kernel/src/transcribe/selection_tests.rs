use super::*;
use std::time::Instant;

#[test]
fn range_requires_ordered_millisecond_integers_and_checks_limits() {
    assert!(Selection::read(
        &json!({"track":0,"start_ms":0,"end_ms":1000}),
        MAX_PREVIEW_MS
    )
    .is_ok());
    for payload in [
        json!({"track":0,"start_ms":-1,"end_ms":5}),
        json!({"track":0,"start_ms":2.5,"end_ms":5}),
        json!({"track":0,"start_ms":5,"end_ms":5}),
        json!({"start_ms":0,"end_ms":5}),
    ] {
        assert_eq!(
            Selection::read(&payload, MAX_PREVIEW_MS).unwrap_err(),
            "media_selection_invalid"
        );
    }
    assert_eq!(
        Selection::read(
            &json!({"track":0,"start_ms":10,"end_ms":16000}),
            MAX_PREVIEW_MS
        )
        .unwrap_err(),
        "media_range_too_long"
    );
}

#[test]
fn recognizer_subtitles_use_source_milliseconds_and_escape_vtt_text() {
    let selection = Selection {
        track: 4,
        start_ms: 3_605_025,
        end_ms: 3_609_030,
    };
    let answer = recognize_selection(
        Path::new("source.mov"),
        &selection,
        Path::new("pcm.wav"),
        2100,
        Some("en"),
        |path, language| {
            assert_eq!(path, Path::new("pcm.wav"));
            assert_eq!(language, Some("en"));
            Ok(json!({"language":"en-US", "segments":[
                {"start":1.5,"end":4.0,"text":" last "},
                {"start":2.2,"end":2.3,"text":"out of range"},
                {"start":0.105,"end":0.3,"text":"a < b & c\n\nnext"},
                {"start":1.0,"end":1.0,"text":"invalid"},
                {"start":0.5,"end":0.8,"text":" "},
            ]}))
        },
    )
    .unwrap();
    let data: Value = serde_json::from_slice(&answer.body).unwrap();
    assert_eq!(data["track"], 4);
    assert_eq!(data["end_ms"], 3_607_125);
    assert_eq!(data["cues"][0]["start_ms"], 3_605_130);
    assert_eq!(data["cues"][1]["end_ms"], 3_607_125);
    assert_eq!(data["cues"].as_array().unwrap().len(), 2);
    assert_eq!(
        data["vtt"],
        "WEBVTT\n\n01:00:05.130 --> 01:00:05.325\na &lt; b &amp; c\nnext\n\n01:00:06.525 --> 01:00:07.125\nlast\n\n"
    );
    assert!(data["content"]
        .as_str()
        .unwrap()
        .contains("**[01:00:05]** a < b & c"));
}

fn pcm_frames(file: &Path) -> Vec<i16> {
    let bytes = fs::read(file).unwrap();
    assert_eq!(&bytes[..4], b"RIFF");
    let mut at = 12;
    while at + 8 <= bytes.len() {
        let size = u32::from_le_bytes(bytes[at + 4..at + 8].try_into().unwrap()) as usize;
        if &bytes[at..at + 4] == b"data" {
            return bytes[at + 8..at + 8 + size]
                .chunks_exact(2)
                .map(|word| i16::from_le_bytes(word.try_into().unwrap()))
                .collect();
        }
        at += 8 + size + size % 2;
    }
    panic!("No PCM data");
}

#[test]
fn ffmpeg_selects_the_requested_delayed_track_on_the_original_clock() {
    let Ok(decoder) = Decoder::from_host() else {
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    let media = dir.path().join("two tracks %.mkv");
    let status = crate::silent_command(&decoder.ffmpeg)
        .args([
            "-nostdin",
            "-v",
            "error",
            "-f",
            "lavfi",
            "-i",
            "sine=f=440:r=16000:d=6",
            "-itsoffset",
            "2",
            "-f",
            "lavfi",
            "-i",
            "sine=f=900:r=16000:d=3",
            "-map",
            "0:a",
            "-map",
            "1:a",
            "-c:a",
            "pcm_s16le",
            "-metadata:s:a:1",
            "language=eng",
        ])
        .arg(&media)
        .status()
        .unwrap();
    assert!(status.success());
    let info = decoder.probe(&media).unwrap();
    assert_eq!(info.duration_ms, Some(6000));
    assert_eq!(info.tracks.len(), 2);
    let delayed = &info.tracks[1];
    assert_eq!(delayed.start_ms, 2000);
    assert_eq!(delayed.language.as_deref(), Some("eng"));
    let wave = dir.path().join("clip.wav");
    let mut selection = Selection {
        track: delayed.index,
        start_ms: 1250,
        end_ms: 3000,
    };
    selection.validate_track(&info).unwrap();
    assert_eq!(decoder.decode(&media, &selection, &wave).unwrap(), 1750);
    let samples = pcm_frames(&wave);
    assert_eq!(samples.len(), 28_000);
    let first_audio = samples
        .iter()
        .position(|sample| sample.unsigned_abs() > 10)
        .unwrap();
    assert_eq!(
        first_audio, 12_001,
        "the second track starts at 2 seconds on the original clock"
    );
    let cycles = samples[12_000..]
        .windows(2)
        .filter(|pair| pair[0] <= 0 && pair[1] > 0)
        .count();
    assert!(
        (899..=901).contains(&cycles),
        "received the selected 900 Hz track, measured {cycles}"
    );

    let original = revision(&media).unwrap();
    selection.track = 50;
    assert_eq!(
        selection.validate_track(&info).unwrap_err(),
        "media_audio_track_invalid"
    );
    fs::write(&media, b"replacement").unwrap();
    assert_eq!(
        ensure_revision(&media, &original).unwrap_err(),
        "media_source_changed"
    );
}

#[test]
fn cancellation_reaps_an_active_ffmpeg_process() {
    let Ok(decoder) = Decoder::from_host() else {
        return;
    };
    let started = Instant::now();
    let result = speech::with_cancel(
        std::sync::Arc::new(move || started.elapsed() >= Duration::from_millis(150)),
        || {
            speech::run(
                crate::silent_command(&decoder.ffmpeg).args([
                    "-nostdin",
                    "-v",
                    "error",
                    "-re",
                    "-f",
                    "lavfi",
                    "-i",
                    "sine=d=20",
                    "-f",
                    "null",
                    "-",
                ]),
                &[],
                Duration::from_secs(5),
            )
        },
    );
    assert_eq!(result.unwrap_err(), "cancelled");
    assert!(!speech::cancelled());
    assert!(started.elapsed() < Duration::from_secs(5));
}

#[test]
fn recognized_chunks_keep_disjoint_source_times_and_finish_at_the_audio_tail() {
    let Ok(decoder) = Decoder::from_host() else {
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    let profile = dir.path().canonicalize().unwrap();
    let source = profile.join("long % take.wav");
    let status = crate::silent_command(&decoder.ffmpeg)
        .args([
            "-nostdin",
            "-v",
            "error",
            "-f",
            "lavfi",
            "-i",
            "sine=f=440:r=16000:d=124",
            "-c:a",
            "pcm_s16le",
        ])
        .arg(&source)
        .status()
        .unwrap();
    assert!(status.success());
    let input = source.to_str().unwrap();
    let source_revision = revision(&source).unwrap();
    let selection = Selection {
        track: 0,
        start_ms: 2250,
        end_ms: 125750,
    };
    let initial = draft::endpoint(
        &profile,
        "draft_save",
        &json!({
            "revision": null,
            "draft": {
                "path": input, "source_revision": source_revision,
                "range": {"track":0,"from":"2.25","to":"125.75","language":"auto"},
                "transcript": null,
            },
        }),
    );
    assert_eq!(initial.status, 200);
    let revision: Value = serde_json::from_slice(&initial.body).unwrap();
    let request = serde_json::from_value(json!({"draft_revision":revision["revision"]})).unwrap();
    let runner = RunnerIdentity {
        engine: "system-speech".into(),
        recognizer_revision: "system-speech".into(),
        decoder_revision: decoder.identity().unwrap(),
    };
    let (record, _) = draft::start_step(
        &profile,
        input,
        &source_revision,
        &selection,
        "auto",
        &runner,
        &request,
    )
    .unwrap();
    let first = record.checkpoint.unwrap();
    let complete = recognize_chunk(
        &decoder,
        &source,
        &source_revision,
        &first,
        |wave, language| {
            assert_eq!(language, Some("auto"));
            assert_eq!(speech::wave_duration(wave).unwrap(), Some(120.0));
            Ok(
                json!({"engine":"system-speech","language":"en-US","segments":[
                    {"start":119.900,"end":120.100,"text":"Ends exactly at the boundary"},
                    {"start":120.000,"end":120.100,"text":"Outside this chunk"},
                ]}),
            )
        },
    )
    .unwrap();
    assert!(!complete.done);
    assert_eq!(complete.next_ms, 122250);
    assert_eq!(complete.cues.len(), 1);
    assert_eq!(complete.cues[0].end_ms, 122250);
    let second = draft::accept_chunk(&profile, &first, complete)
        .unwrap()
        .checkpoint
        .unwrap();
    let tail = recognize_chunk(
        &decoder,
        &source,
        &source_revision,
        &second,
        |wave, language| {
            assert_eq!(language, Some("en-US"));
            assert_eq!(speech::wave_duration(wave).unwrap(), Some(1.75));
            Ok(
                json!({"engine":"system-speech","language":"en-US","segments":[
                    {"start":0.0,"end":1.0,"text":"Next chunk"},
                ]}),
            )
        },
    )
    .unwrap();
    assert!(tail.done);
    assert_eq!(tail.next_ms, 124000);
    assert_eq!(tail.cues[0].start_ms, 122250);
    let response = draft::accept_chunk(&profile, &second, tail)
        .unwrap()
        .response();
    let result: Value = serde_json::from_slice(&response.body).unwrap();
    let cues = result["draft"]["transcript"]["cues"].as_array().unwrap();
    assert_eq!(cues.len(), 2);
    assert_eq!(cues[0]["end_ms"], cues[1]["start_ms"]);
}
