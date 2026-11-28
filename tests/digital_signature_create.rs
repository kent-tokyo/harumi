#![cfg(feature = "digital-signature")]

use harumi::{CertificateInput, Document, PrivateKeyInput, SignatureFieldOptions, SigningContext};

/// Load stable test certificate and private key from fixtures.
fn generate_test_cert_and_key() -> (Vec<u8>, Vec<u8>) {
    let cert_der = std::fs::read("tests/fixtures/test_rsa_2048.crt")
        .expect("Failed to read test_rsa_2048.crt fixture");
    let key_der = std::fs::read("tests/fixtures/test_rsa_2048.der")
        .expect("Failed to read test_rsa_2048.der fixture");

    (cert_der, key_der)
}

#[test]
fn test_add_signature_field() {
    let mut doc = Document::new((612.0, 792.0)).expect("Create document");

    let options = SignatureFieldOptions {
        field_name: "Signature1".into(),
        reason: Some("Testing".into()),
        contact_info: Some("test@example.com".into()),
        lock_permissions: false,
    };

    doc.add_signature_field(1, [50.0, 600.0, 200.0, 50.0], &options)
        .expect("Add signature field");

    // Test passed if no error
}

#[test]
fn test_signing_context_creation() {
    let (cert_der, key_der) = generate_test_cert_and_key();

    let ctx = SigningContext::from_cert_and_key(
        CertificateInput::Der(cert_der),
        PrivateKeyInput::Der(key_der),
    )
    .expect("Create signing context");

    // Check that signer name was extracted
    assert!(!ctx.signer_name().is_empty());
}

#[test]
fn test_sign_document_basic() {
    let mut doc = Document::new((612.0, 792.0)).expect("Create document");

    let options = SignatureFieldOptions {
        field_name: "Sig1".into(),
        reason: Some("Approval".into()),
        contact_info: None,
        lock_permissions: false,
    };

    doc.add_signature_field(1, [50.0, 600.0, 200.0, 50.0], &options)
        .expect("Add signature field");

    let (cert_der, key_der) = generate_test_cert_and_key();

    let ctx = SigningContext::from_cert_and_key(
        CertificateInput::Der(cert_der),
        PrivateKeyInput::Der(key_der),
    )
    .expect("Create signing context");

    let signed_bytes = doc.sign_document(&ctx, "Sig1").expect("Sign document");

    // Check that we got valid PDF bytes back
    assert!(!signed_bytes.is_empty());
    assert!(signed_bytes.starts_with(b"%PDF"));
}

#[test]
fn test_sign_document_with_content() {
    let mut doc = Document::new((612.0, 792.0)).expect("Create document");
    let font_bytes = include_bytes!("fixtures/NotoSansJP-Regular.ttf");
    let font = doc.embed_font(font_bytes).expect("Embed font");

    doc.page(1)
        .expect("Get page")
        .add_text("Test Document", font, [72.0, 700.0], 18.0, [0.0, 0.0, 0.0])
        .expect("Add text");

    let options = SignatureFieldOptions {
        field_name: "DocSig".into(),
        reason: Some("Document approval".into()),
        contact_info: None,
        lock_permissions: true,
    };

    doc.add_signature_field(1, [72.0, 600.0, 300.0, 50.0], &options)
        .expect("Add signature field");

    let (cert_der, key_der) = generate_test_cert_and_key();
    let ctx = SigningContext::from_cert_and_key(
        CertificateInput::Der(cert_der),
        PrivateKeyInput::Der(key_der),
    )
    .expect("Create signing context");

    let signed_bytes = doc.sign_document(&ctx, "DocSig").expect("Sign document");

    assert!(signed_bytes.len() > 1000); // Should have substantial content
    assert!(signed_bytes.starts_with(b"%PDF"));
}

fn sign_simple(field: &str) -> Vec<u8> {
    let mut doc = Document::new((612.0, 792.0)).expect("Create document");
    let options = SignatureFieldOptions {
        field_name: field.into(),
        reason: Some("Approval".into()),
        contact_info: None,
        lock_permissions: false,
    };
    doc.add_signature_field(1, [50.0, 600.0, 200.0, 50.0], &options)
        .expect("Add signature field");
    let (cert_der, key_der) = generate_test_cert_and_key();
    let ctx = SigningContext::from_cert_and_key(
        CertificateInput::Der(cert_der),
        PrivateKeyInput::Der(key_der),
    )
    .expect("Create signing context");
    doc.sign_document(&ctx, field).expect("Sign document")
}

#[test]
fn signed_document_keeps_pages_and_verifies() {
    let signed = sign_simple("Sig1");

    let doc = Document::from_bytes(&signed).expect("Reopen signed PDF");
    assert_eq!(doc.page_count(), 1, "signing must not break the page tree");

    let sigs = doc.verify_signatures(&signed).expect("Verify");
    assert_eq!(sigs.len(), 1);
    assert_eq!(sigs[0].field_name, "Sig1");
    assert_eq!(sigs[0].reason.as_deref(), Some("Approval"));
    assert!(sigs[0].signer_name.is_some());
    assert!(sigs[0].signing_time.is_some());
    assert!(sigs[0].is_valid, "freshly signed PDF must verify");
}

#[test]
fn byte_range_excludes_exactly_the_contents_string() {
    let signed = sign_simple("Sig1");
    let doc = lopdf::Document::load_mem(&signed).expect("load");
    let range: Vec<usize> = doc
        .objects
        .values()
        .filter_map(|o| o.as_dict().ok())
        .find_map(|d| d.get(b"ByteRange").ok())
        .and_then(|r| r.as_array().ok())
        .expect("ByteRange")
        .iter()
        .map(|o| o.as_i64().unwrap() as usize)
        .collect();
    assert_eq!(range[0], 0);
    assert_eq!(signed[range[1]], b'<');
    assert_eq!(signed[range[2] - 1], b'>');
    assert_eq!(range[2] + range[3], signed.len());
}

#[test]
fn tampered_document_fails_verification() {
    let mut signed = sign_simple("Sig1");
    // Flip a byte inside the signed range (the PDF header comment).
    signed[5] = if signed[5] == b'1' { b'2' } else { b'1' };
    let doc = Document::from_bytes(&signed).expect("Reopen");
    let sigs = doc.verify_signatures(&signed).expect("Verify");
    assert_eq!(sigs.len(), 1);
    assert!(
        !sigs[0].is_valid,
        "modified bytes must invalidate the signature"
    );
}

#[test]
fn appended_bytes_fail_verification() {
    let mut signed = sign_simple("Sig1");
    signed.extend_from_slice(b"\n% appended after signing\n");
    let doc = Document::from_bytes(&signed).expect("Reopen");
    let sigs = doc.verify_signatures(&signed).expect("Verify");
    assert!(
        !sigs[0].is_valid,
        "bytes outside the ByteRange must be detected"
    );
}

#[test]
fn signing_unknown_field_is_an_error() {
    let mut doc = Document::new((612.0, 792.0)).expect("Create document");
    let (cert_der, key_der) = generate_test_cert_and_key();
    let ctx = SigningContext::from_cert_and_key(
        CertificateInput::Der(cert_der),
        PrivateKeyInput::Der(key_der),
    )
    .expect("Create signing context");
    assert!(doc.sign_document(&ctx, "Missing").is_err());
}

#[test]
fn signed_pdf_written_for_external_check() {
    if let Ok(path) = std::env::var("HARUMI_SIGNED_PDF_OUT") {
        std::fs::write(path, sign_simple("Sig1")).unwrap();
    }
}
