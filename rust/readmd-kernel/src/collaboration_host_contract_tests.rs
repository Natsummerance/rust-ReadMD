//! Offline protocol tests use the real service and its SQLite journal.
use super::*;
use base64::{engine::general_purpose::STANDARD, Engine};
use yrs::updates::{decoder::Decode, encoder::Encode};
use yrs::{Doc, GetString, OffsetKind, Options, ReadTxn, Text, Transact, Update};

fn sync(
    service: &Service,
    credential: &crate::collaboration::Credential,
    body: Value,
) -> ApiResult<Value> {
    session_request(service, &credential.room, "sync", &credential.token, body)
}

fn client(service: &Service, credential: &crate::collaboration::Credential) -> Doc {
    let doc = Doc::with_options(Options {
        offset_kind: OffsetKind::Utf16,
        ..Default::default()
    });
    let initial = sync(service, credential, json!({"vector":STANDARD.encode([0])})).unwrap();
    doc.transact_mut()
        .apply_update(
            Update::decode_v1(
                &STANDARD
                    .decode(initial["update"].as_str().unwrap())
                    .unwrap(),
            )
            .unwrap(),
        )
        .unwrap();
    doc
}

#[test]
fn malformed_mutation_fields_are_rejected_without_changing_access_or_acknowledging_sync() {
    let data = tempfile::tempdir().unwrap();
    let service = Service::open(&data.path().join("collaboration.sqlite")).unwrap();
    let owner = service.create("Contract", "Original 🦀", "Owner").unwrap();
    let editor = service
        .invite(&owner.room, &owner.token, Role::Editor, "Editor")
        .unwrap();
    for body in [
        json!({"member":editor.member}),
        json!({"member":editor.member,"role":false}),
        json!({"member":editor.member,"role":[]}),
        json!({"member":editor.member,"role":"invalid"}),
    ] {
        let error =
            session_request(&service, &owner.room, "permission", &owner.token, body).unwrap_err();
        assert_eq!(error.status, 422);
        assert_eq!(error.code, "collaboration_invalid");
        assert_eq!(
            service.snapshot(&editor.room, &editor.token).unwrap()["role"],
            "editor"
        );
    }
    for update in [
        Value::Null,
        json!(false),
        json!(7),
        json!([]),
        json!({"data":"AA=="}),
    ] {
        let error = sync(
            &service,
            &editor,
            json!({"vector":STANDARD.encode([0]),"update":update,"presence":{"cursor":1}}),
        )
        .unwrap_err();
        assert_eq!(error.status, 422);
        assert_eq!(error.code, "collaboration_invalid");
        assert_eq!(
            service.snapshot(&owner.room, &owner.token).unwrap()["content"],
            "Original 🦀"
        );
    }
    // Omission is the existing read-only polling request. Explicit null is not.
    let polled = sync(&service, &editor, json!({"vector":STANDARD.encode([0])})).unwrap();
    assert_eq!(polled["synced"], true);
    assert!(polled["peers"].as_array().unwrap().is_empty());
    session_request(
        &service,
        &owner.room,
        "permission",
        &owner.token,
        json!({"member":editor.member,"role":null}),
    )
    .unwrap();
    assert_eq!(
        service
            .snapshot(&editor.room, &editor.token)
            .unwrap_err()
            .status,
        403
    );
}

#[test]
fn rejected_sync_payload_can_be_retried_as_a_real_delta_once_and_survives_restart() {
    let data = tempfile::tempdir().unwrap();
    let path = data.path().join("collaboration.sqlite");
    let service = Service::open(&path).unwrap();
    let owner = service.create("Offline", "Original 🦀", "Owner").unwrap();
    let doc = client(&service, &owner);
    let vector = doc.transact().state_vector();
    let text = doc.get_or_insert_text("content");
    let end = text.len(&doc.transact());
    text.insert(&mut doc.transact_mut(), end, " queued offline");
    let update = STANDARD.encode(doc.transact().encode_state_as_update_v1(&vector));
    let vector = STANDARD.encode(vector.encode_v1());
    assert_eq!(
        sync(
            &service,
            &owner,
            json!({"vector":vector,"update":{"data":update}})
        )
        .unwrap_err()
        .status,
        422
    );
    assert_eq!(
        service.snapshot(&owner.room, &owner.token).unwrap()["content"],
        "Original 🦀"
    );
    // The caller still owns the rejected bytes; retry and duplicate delivery
    // pass through the normal Yrs journal, not a test-only content replacement.
    for _ in 0..2 {
        assert_eq!(
            sync(&service, &owner, json!({"vector":vector,"update":update})).unwrap()["synced"],
            true
        );
    }
    let expected = text.get_string(&doc.transact());
    assert_eq!(
        service.snapshot(&owner.room, &owner.token).unwrap()["content"],
        expected
    );
    drop(service);
    let service = Service::open(&path).unwrap();
    assert_eq!(
        service.snapshot(&owner.room, &owner.token).unwrap()["content"],
        expected
    );
}

#[test]
fn downgrade_and_revocation_apply_to_delayed_writes_and_attachment_deduplication() {
    let data = tempfile::tempdir().unwrap();
    let path = data.path().join("collaboration.sqlite");
    let service = Service::open(&path).unwrap();
    let owner = service.create("Access", "Original", "Owner").unwrap();
    let editor = service
        .invite(&owner.room, &owner.token, Role::Editor, "Editor")
        .unwrap();
    let doc = client(&service, &editor);
    let vector = doc.transact().state_vector();
    doc.get_or_insert_text("content")
        .insert(&mut doc.transact_mut(), 0, "Unsent ");
    let update = STANDARD.encode(doc.transact().encode_state_as_update_v1(&vector));
    let vector = STANDARD.encode(vector.encode_v1());
    let asset = service
        .attachment(
            &owner.room,
            &owner.token,
            "source.txt",
            "text/plain",
            b"synthetic source",
        )
        .unwrap();
    sync(
        &service,
        &editor,
        json!({"vector":vector,"presence":{"cursor":0}}),
    )
    .unwrap();
    for role in [Role::Reviewer, Role::Reader] {
        service
            .permission(&owner.room, &owner.token, &editor.member, Some(role))
            .unwrap();
        assert_eq!(
            sync(&service, &editor, json!({"vector":vector,"update":update}))
                .unwrap_err()
                .status,
            403
        );
        assert_eq!(
            service
                .attachment(
                    &editor.room,
                    &editor.token,
                    "duplicate.txt",
                    "text/plain",
                    b"synthetic source"
                )
                .unwrap_err()
                .status,
            403
        );
        assert_eq!(
            service
                .read_attachment(&editor.room, &editor.token, &asset)
                .unwrap(),
            b"synthetic source"
        );
        assert_eq!(
            service.attachments(&editor.room, &editor.token).unwrap()["attachments"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
    }
    service
        .permission(&owner.room, &owner.token, &editor.member, None)
        .unwrap();
    assert_eq!(
        sync(&service, &editor, json!({"vector":vector,"update":update}))
            .unwrap_err()
            .status,
        403
    );
    assert_eq!(
        service
            .attachment(
                &editor.room,
                &editor.token,
                "duplicate.txt",
                "text/plain",
                b"synthetic source"
            )
            .unwrap_err()
            .status,
        403
    );
    assert_eq!(
        service
            .read_attachment(&editor.room, &editor.token, &asset)
            .unwrap_err()
            .status,
        403
    );
    assert_eq!(
        service
            .attachments(&editor.room, &editor.token)
            .unwrap_err()
            .status,
        403
    );
    assert_eq!(
        service
            .review(&editor.room, &editor.token, json!({"text":"Late comment"}))
            .unwrap_err()
            .status,
        403
    );
    let polled = sync(&service, &owner, json!({"vector":STANDARD.encode([0])})).unwrap();
    assert!(polled["peers"]
        .as_array()
        .unwrap()
        .iter()
        .all(|peer| peer["member"] != editor.member));
    drop(service);
    let service = Service::open(&path).unwrap();
    assert_eq!(
        service
            .read_attachment(&editor.room, &editor.token, &asset)
            .unwrap_err()
            .status,
        403
    );
    assert_eq!(
        service.snapshot(&owner.room, &owner.token).unwrap()["content"],
        "Original"
    );
}
