mod helpers;

use harumi::{Document, Error};

// ---------------------------------------------------------------------------
// set_encryption: produce an encrypted PDF from scratch
// ---------------------------------------------------------------------------

#[test]
fn encrypted_output_requires_password_to_open() {
    let mut doc = Document::new((595.0, 842.0)).unwrap();
    doc.set_encryption("user123", "owner456").unwrap();
    let bytes = doc.save_to_bytes().unwrap();

    // Must be openable with correct password.
    let reopened = Document::from_bytes_with_password(&bytes, "user123").unwrap();
    assert_eq!(reopened.page_count(), 1);
    assert!(reopened.is_encrypted());
}

#[test]
fn wrong_password_on_encrypted_output_returns_error() {
    let mut doc = Document::new((595.0, 842.0)).unwrap();
    doc.set_encryption("secret", "boss").unwrap();
    let bytes = doc.save_to_bytes().unwrap();

    match Document::from_bytes_with_password(&bytes, "wrong") {
        Err(Error::WrongPassword) => {}
        Ok(_) => panic!("expected WrongPassword but got Ok"),
        Err(e) => panic!("expected WrongPassword, got {e:?}"),
    }
}

#[test]
fn owner_password_also_opens_encrypted_output() {
    let mut doc = Document::new((595.0, 842.0)).unwrap();
    doc.set_encryption("user", "owner999").unwrap();
    let bytes = doc.save_to_bytes().unwrap();

    let reopened = Document::from_bytes_with_password(&bytes, "owner999").unwrap();
    assert_eq!(reopened.page_count(), 1);
}

#[test]
fn empty_user_password_opens_without_password() {
    // Empty user password means anyone can open; owner password restricts editing.
    let mut doc = Document::new((595.0, 842.0)).unwrap();
    doc.set_encryption("", "editonly").unwrap();
    let bytes = doc.save_to_bytes().unwrap();

    // Opening with empty password should succeed.
    let _ = Document::from_bytes(&bytes).unwrap();
}

#[test]
fn set_encryption_after_save_returns_error() {
    let mut doc = Document::new((595.0, 842.0)).unwrap();
    let font_bytes = std::fs::read("tests/fixtures/NotoSansJP-Regular.ttf").unwrap();
    let font = doc.embed_font(&font_bytes).unwrap();
    doc.page(1)
        .unwrap()
        .add_invisible_text("test", font, [72.0, 700.0], 12.0)
        .unwrap();
    let _ = doc.save_to_bytes().unwrap();
    assert!(matches!(
        doc.set_encryption("pw", "pw"),
        Err(Error::InvalidInput(_))
    ));
}

#[test]
fn encrypted_pdf_with_content_roundtrips() {
    let font_bytes = std::fs::read("tests/fixtures/NotoSansJP-Regular.ttf").unwrap();

    let mut doc = Document::new((595.0, 842.0)).unwrap();
    let font = doc.embed_font(&font_bytes).unwrap();
    doc.page(1)
        .unwrap()
        .add_invisible_text("日本語テスト", font, [72.0, 700.0], 12.0)
        .unwrap();
    doc.set_encryption("pw", "owpw").unwrap();
    let bytes = doc.save_to_bytes().unwrap();

    let reopened = Document::from_bytes_with_password(&bytes, "pw").unwrap();
    assert_eq!(reopened.page_count(), 1);
    assert!(reopened.is_encrypted());
}

#[test]
fn set_encryption_via_save_to_file() {
    let mut doc = Document::new((595.0, 842.0)).unwrap();
    doc.set_encryption("file_user", "file_owner").unwrap();

    let path = std::env::temp_dir().join("harumi_write_enc_test.pdf");
    doc.save(&path).unwrap();

    let reopened = Document::from_file_with_password(&path, "file_user").unwrap();
    let _ = std::fs::remove_file(&path);
    assert_eq!(reopened.page_count(), 1);
}

// ---------------------------------------------------------------------------
// set_encryption_aes256: AES-256-CBC (PDF V5/R6) write encryption
// ---------------------------------------------------------------------------

#[test]
fn aes256_encrypted_output_requires_password() {
    let mut doc = Document::new((595.0, 842.0)).unwrap();
    doc.set_encryption_aes256("user123", "owner456").unwrap();
    let bytes = doc.save_to_bytes().unwrap();

    let reopened = Document::from_bytes_with_password(&bytes, "user123").unwrap();
    assert_eq!(reopened.page_count(), 1);
    assert!(reopened.is_encrypted());
}

#[test]
fn aes256_wrong_password_returns_error() {
    let mut doc = Document::new((595.0, 842.0)).unwrap();
    doc.set_encryption_aes256("secret", "boss").unwrap();
    let bytes = doc.save_to_bytes().unwrap();

    match Document::from_bytes_with_password(&bytes, "wrong") {
        Err(Error::WrongPassword) => {}
        Ok(_) => panic!("expected WrongPassword but got Ok"),
        Err(e) => panic!("expected WrongPassword, got {e:?}"),
    }
}

#[test]
fn aes256_owner_password_opens_output() {
    let mut doc = Document::new((595.0, 842.0)).unwrap();
    doc.set_encryption_aes256("user", "owner999").unwrap();
    let bytes = doc.save_to_bytes().unwrap();

    let reopened = Document::from_bytes_with_password(&bytes, "owner999").unwrap();
    assert_eq!(reopened.page_count(), 1);
}

#[test]
fn aes256_empty_user_password_opens_without_password() {
    let mut doc = Document::new((595.0, 842.0)).unwrap();
    doc.set_encryption_aes256("", "editonly").unwrap();
    let bytes = doc.save_to_bytes().unwrap();

    let _ = Document::from_bytes(&bytes).unwrap();
}

#[test]
fn aes256_after_save_returns_error() {
    // finalize() only sets finalized=true when there are pending ops, so we add content.
    let font_bytes = std::fs::read("tests/fixtures/NotoSansJP-Regular.ttf").unwrap();
    let mut doc = Document::new((595.0, 842.0)).unwrap();
    let font = doc.embed_font(&font_bytes).unwrap();
    doc.page(1)
        .unwrap()
        .add_invisible_text("test", font, [72.0, 700.0], 12.0)
        .unwrap();
    let _ = doc.save_to_bytes().unwrap();
    assert!(matches!(
        doc.set_encryption_aes256("pw", "pw"),
        Err(Error::InvalidInput(_))
    ));
}

#[test]
fn aes256_encrypted_pdf_with_content_roundtrips() {
    let font_bytes = std::fs::read("tests/fixtures/NotoSansJP-Regular.ttf").unwrap();
    let mut doc = Document::new((595.0, 842.0)).unwrap();
    let font = doc.embed_font(&font_bytes).unwrap();
    doc.page(1)
        .unwrap()
        .add_invisible_text("日本語テスト", font, [72.0, 700.0], 12.0)
        .unwrap();
    doc.set_encryption_aes256("pw", "owpw").unwrap();
    let bytes = doc.save_to_bytes().unwrap();

    let reopened = Document::from_bytes_with_password(&bytes, "pw").unwrap();
    assert_eq!(reopened.page_count(), 1);
    assert!(reopened.is_encrypted());
}

/// Verify the /Encrypt dictionary specifies V=5 (AES-256) and R=6.
#[test]
fn aes256_encrypt_dict_has_v5_r6() {
    let mut doc = Document::new((595.0, 842.0)).unwrap();
    doc.set_encryption_aes256("u", "o").unwrap();
    let bytes = doc.save_to_bytes().unwrap();

    // Parse with lopdf and inspect the /Encrypt dictionary.
    let lpdf = lopdf::Document::load_from(bytes.as_slice()).unwrap();
    let encrypt_ref = lpdf
        .trailer
        .get(b"Encrypt")
        .unwrap()
        .as_reference()
        .unwrap();
    let encrypt_dict = lpdf.get_object(encrypt_ref).unwrap().as_dict().unwrap();

    let v = encrypt_dict.get(b"V").unwrap().as_i64().unwrap();
    let r = encrypt_dict.get(b"R").unwrap().as_i64().unwrap();
    assert_eq!(v, 5, "/Encrypt /V should be 5 for AES-256; got {v}");
    assert_eq!(r, 6, "/Encrypt /R should be 6 for PDF 2.0 AES-256; got {r}");
}
