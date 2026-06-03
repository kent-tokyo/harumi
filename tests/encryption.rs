mod helpers;

use harumi::{Document, Error};

// ---------------------------------------------------------------------------
// Fixture builder
//
// Creates a minimal encrypted PDF in memory using harumi's re-exported lopdf.
// No external binary or fixture file needed — the doc is generated at test time.
// ---------------------------------------------------------------------------

fn make_encrypted_pdf(user_pw: &str, owner_pw: &str) -> Vec<u8> {
    use harumi::lopdf::{EncryptionState, EncryptionVersion, Object, Permissions, StringFormat};

    // Start with a harumi-generated plain PDF (correct structure guaranteed).
    let plain = {
        let mut doc = Document::new((595.0, 842.0)).unwrap();
        doc.save_to_bytes().unwrap()
    };

    // Load with lopdf, apply encryption, export.
    let mut inner = harumi::lopdf::Document::load_from(plain.as_slice())
        .expect("lopdf must load harumi-generated PDF");

    // PDF encryption requires a /ID array in the trailer; harumi's blank PDFs
    // don't include one by default, so we add a static placeholder here.
    let id_bytes = b"\x01\x23\x45\x67\x89\xab\xcd\xef\x01\x23\x45\x67\x89\xab\xcd\xef".to_vec();
    let id_obj = Object::Array(vec![
        Object::String(id_bytes.clone(), StringFormat::Hexadecimal),
        Object::String(id_bytes, StringFormat::Hexadecimal),
    ]);
    inner.trailer.set("ID", id_obj);

    let version = EncryptionVersion::V2 {
        document: &inner,
        owner_password: owner_pw,
        user_password: user_pw,
        key_length: 40,
        permissions: Permissions::all(),
    };
    let state =
        EncryptionState::try_from(version).expect("EncryptionState::try_from must not fail");
    inner.encrypt(&state).expect("encrypt must succeed");

    let mut buf = Vec::new();
    inner.save_to(&mut buf).expect("save_to must succeed");
    buf
}

// ---------------------------------------------------------------------------
// from_bytes_with_password
// ---------------------------------------------------------------------------

#[test]
fn bytes_correct_user_password_loads() {
    let pdf = make_encrypted_pdf("secret", "owner123");
    let doc = Document::from_bytes_with_password(&pdf, "secret").unwrap();
    assert_eq!(doc.page_count(), 1);
}

#[test]
fn bytes_correct_owner_password_loads() {
    let pdf = make_encrypted_pdf("secret", "owner123");
    let doc = Document::from_bytes_with_password(&pdf, "owner123").unwrap();
    assert_eq!(doc.page_count(), 1);
}

#[test]
fn bytes_wrong_password_returns_wrong_password_error() {
    let pdf = make_encrypted_pdf("secret", "owner123");
    match Document::from_bytes_with_password(&pdf, "wrong") {
        Err(Error::WrongPassword) => {}
        Ok(_) => panic!("expected WrongPassword but got Ok"),
        Err(e) => panic!("expected WrongPassword, got {e:?}"),
    }
}

#[test]
fn bytes_empty_password_on_pw_protected_doc_returns_wrong_password() {
    let pdf = make_encrypted_pdf("secret", "owner123");
    // Empty password should fail because user_pw is non-empty.
    // lopdf tries empty password first; here it won't match.
    match Document::from_bytes_with_password(&pdf, "") {
        Err(Error::WrongPassword) => {}
        Ok(_) => panic!("expected WrongPassword but got Ok"),
        Err(e) => panic!("expected WrongPassword, got {e:?}"),
    }
}

// ---------------------------------------------------------------------------
// from_file_with_password
// ---------------------------------------------------------------------------

#[test]
fn file_correct_password_loads() {
    let pdf = make_encrypted_pdf("filetest", "fileowner");
    let path = std::env::temp_dir().join("harumi_enc_test_correct.pdf");
    std::fs::write(&path, &pdf).unwrap();

    let result = Document::from_file_with_password(&path, "filetest");
    let _ = std::fs::remove_file(&path);

    let doc = result.unwrap();
    assert_eq!(doc.page_count(), 1);
}

#[test]
fn file_wrong_password_returns_wrong_password_error() {
    let pdf = make_encrypted_pdf("filetest", "fileowner");
    let path = std::env::temp_dir().join("harumi_enc_test_wrong.pdf");
    std::fs::write(&path, &pdf).unwrap();

    let result = Document::from_file_with_password(&path, "bad");
    let _ = std::fs::remove_file(&path);

    assert!(matches!(result, Err(Error::WrongPassword)));
}

#[test]
fn file_nonexistent_path_returns_io_error() {
    let result = Document::from_file_with_password("/nonexistent/path/test.pdf", "pw");
    assert!(matches!(result, Err(Error::Io(_))));
}

// ---------------------------------------------------------------------------
// is_encrypted
// ---------------------------------------------------------------------------

#[test]
fn is_encrypted_true_after_password_load() {
    let pdf = make_encrypted_pdf("pw", "ow");
    let doc = Document::from_bytes_with_password(&pdf, "pw").unwrap();
    assert!(
        doc.is_encrypted(),
        "is_encrypted() must be true for a document loaded with a password"
    );
}

#[test]
fn is_encrypted_false_for_blank_new_document() {
    let doc = Document::new((595.0, 842.0)).unwrap();
    assert!(!doc.is_encrypted());
}

#[test]
fn is_encrypted_false_for_plain_from_bytes() {
    let pdf = helpers::minimal_pdf_bytes();
    let doc = Document::from_bytes(&pdf).unwrap();
    assert!(!doc.is_encrypted());
}

// ---------------------------------------------------------------------------
// Operations on decrypted documents
// ---------------------------------------------------------------------------

#[test]
fn decrypted_doc_page_ops_work() {
    let pdf = make_encrypted_pdf("ops", "opsowner");
    let mut doc = Document::from_bytes_with_password(&pdf, "ops").unwrap();

    // Page access
    assert_eq!(doc.page_count(), 1);
    let (w, h) = doc.page(1).unwrap().size().unwrap();
    assert!((w - 595.0).abs() < 1.0);
    assert!((h - 842.0).abs() < 1.0);

    // Can insert a blank page
    doc.insert_blank_page(1, (595.0, 842.0)).unwrap();
    assert_eq!(doc.page_count(), 2);
}

#[test]
fn decrypted_doc_can_embed_font_and_save() {
    let font_bytes = std::fs::read("tests/fixtures/NotoSansJP-Regular.ttf")
        .expect("NotoSansJP fixture must exist");

    let pdf = make_encrypted_pdf("fontsave", "fsowner");
    let mut doc = Document::from_bytes_with_password(&pdf, "fontsave").unwrap();
    let font = doc.embed_font(&font_bytes).unwrap();
    doc.page(1)
        .unwrap()
        .add_invisible_text("テスト", font, [72.0, 700.0], 12.0)
        .unwrap();

    let out = doc.save_to_bytes().unwrap();
    // The saved PDF must be valid and readable (no password needed — we don't re-encrypt).
    let reloaded = Document::from_bytes(&out).unwrap();
    assert_eq!(reloaded.page_count(), 1);
}

// ---------------------------------------------------------------------------
// Consistency: loading plain vs encrypted round-trip
// ---------------------------------------------------------------------------

#[test]
fn password_roundtrip_page_count_matches_plain() {
    // Plain
    let mut plain_doc = Document::new((595.0, 842.0)).unwrap();
    plain_doc.insert_blank_page(1, (595.0, 842.0)).unwrap(); // 2 pages
    let plain_bytes = plain_doc.save_to_bytes().unwrap();
    let plain_count = Document::from_bytes(&plain_bytes).unwrap().page_count();

    // Encrypt that same PDF
    let mut inner =
        harumi::lopdf::Document::load_from(plain_bytes.as_slice()).unwrap();
    use harumi::lopdf::{EncryptionState, EncryptionVersion, Object, Permissions, StringFormat};
    let id_b = b"\x01\x23\x45\x67\x89\xab\xcd\xef\x01\x23\x45\x67\x89\xab\xcd\xef".to_vec();
    inner.trailer.set("ID", Object::Array(vec![
        Object::String(id_b.clone(), StringFormat::Hexadecimal),
        Object::String(id_b, StringFormat::Hexadecimal),
    ]));
    let ver = EncryptionVersion::V2 {
        document: &inner,
        owner_password: "owner",
        user_password: "user",
        key_length: 40,
        permissions: Permissions::all(),
    };
    inner
        .encrypt(&EncryptionState::try_from(ver).unwrap())
        .unwrap();
    let mut enc_buf = Vec::new();
    inner.save_to(&mut enc_buf).unwrap();

    let enc_count = Document::from_bytes_with_password(&enc_buf, "user")
        .unwrap()
        .page_count();

    assert_eq!(plain_count, enc_count, "page count must survive encryption round-trip");
}
