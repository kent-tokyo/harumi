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
