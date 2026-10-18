//! QA-only encrypted PDF fixtures. Not a product engine or release binary.
use lopdf::{
    encryption::{EncryptionState, EncryptionVersion, Permissions},
    encryption::crypt_filters::{Aes256CryptFilter, CryptFilter},
    Document, Object,
};
use std::{collections::BTreeMap, path::PathBuf, sync::Arc};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let root = PathBuf::from(args.next().ok_or("missing fixture directory")?);
    if args.next().as_deref() == Some("verify") {
        let path = root.join("Encrypted.pdf");
        let locked = Document::load(&path)?;
        assert!(locked.is_encrypted());
        assert!(locked.authenticate_password("incorrect").is_err());
        let document = Document::load_with_password(&path, "readmd-fixture-user")?;
        assert_eq!(document.get_pages().len(), 2);
        let page = *document.get_pages().get(&1).ok_or("missing page")?;
        // This independent parser intentionally resolves only indirect Annots.
        // PDFium used to reopen its own output while other readers lost notes.
        let notes = document.get_page_annotations(page)?;
        assert!(notes.iter().any(|note| note
            .get(b"Contents")
            .ok()
            .and_then(|v| v.as_str().ok())
            .is_some_and(|bytes| {
                let text = if bytes.starts_with(&[0xfe, 0xff]) {
                    String::from_utf16_lossy(
                        &bytes[2..]
                            .chunks_exact(2)
                            .map(|b| u16::from_be_bytes([b[0], b[1]]))
                            .collect::<Vec<_>>(),
                    )
                } else {
                    String::from_utf8_lossy(bytes).into_owned()
                };
                text == "Encrypted reviewed"
            })));
        assert!(matches!(
            document.get_dictionary(page)?.get(b"Annots")?,
            Object::Array(_) | Object::Reference(_)
        ));
        println!("{{\"encryptionPreserved\":true,\"annotationReadableByIndependentParser\":true}}");
    } else {
        let base = Permissions::PRINTABLE | Permissions::COPYABLE;
        for (name, permissions) in [
            (
                "Encrypted.pdf",
                base | Permissions::MODIFIABLE | Permissions::ANNOTABLE | Permissions::FILLABLE,
            ),
            ("ReadOnly.pdf", base),
            ("ReviewOnly.pdf", base | Permissions::ANNOTABLE),
        ] {
            let mut document = Document::load(root.join("Original.pdf"))?;
            let mut key = [0u8; 32];
            getrandom::fill(&mut key).map_err(|_| "random fixture key failed")?;
            let filter: Arc<dyn CryptFilter> = Arc::new(Aes256CryptFilter);
            let state = EncryptionState::try_from(EncryptionVersion::V5 {
                encrypt_metadata: true,
                crypt_filters: BTreeMap::from([(b"StdCF".to_vec(), filter)]),
                file_encryption_key: &key,
                stream_filter: b"StdCF".to_vec(),
                string_filter: b"StdCF".to_vec(),
                owner_password: "readmd-fixture-owner",
                user_password: "readmd-fixture-user",
                permissions,
            })?;
            document.encrypt(&state)?;
            document.save(root.join(name))?;
        }
        println!("{{\"created\":3}}");
    }
    Ok(())
}
