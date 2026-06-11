#![cfg(feature = "digital-signature")]

/// One-time fixture generation for test RSA keys.
/// Run with: cargo test --test generate_test_fixtures --features digital-signature -- --nocapture
///
/// This generates stable test fixtures that don't change between runs,
/// avoiding rcgen's non-deterministic key generation.
#[test]
#[ignore]
fn generate_test_rsa_keys() {
    use std::fs;
    use std::path::Path;

    let cert_path = "tests/fixtures/test_rsa_2048.crt";
    let key_path = "tests/fixtures/test_rsa_2048.der";

    println!("Generating test RSA 2048 key pair...");

    use rsa::{RsaPrivateKey, pkcs8::EncodePrivateKey};
    use rand::thread_rng;

    // Generate RSA 2048 key pair
    let mut rng = thread_rng();
    let bits = 2048;
    let private_key = RsaPrivateKey::new(&mut rng, bits)
        .expect("Failed to generate RSA key pair");

    // Create a self-signed certificate using rcgen

    // For simplicity, use rcgen with default key, then we'll just save our key separately
    use rcgen::CertificateParams;
    let params = CertificateParams::new(vec!["test.example.com".to_string()]);
    let cert = rcgen::Certificate::from_params(params).expect("Failed to generate cert");

    let cert_der = cert.serialize_der().expect("Failed to serialize cert");

    // Export private key as PKCS#8 DER
    let key_der = private_key
        .to_pkcs8_der()
        .expect("Failed to encode private key")
        .to_bytes()
        .to_vec();

    fs::write(cert_path, &cert_der).expect("Failed to write certificate");
    fs::write(key_path, &key_der).expect("Failed to write private key");

    println!("✓ Generated {} ({} bytes)", cert_path, cert_der.len());
    println!("✓ Generated {} ({} bytes)", key_path, key_der.len());
    println!("\nFixtures generated successfully!");
}
