// Test RSA key generation script
// Run with: cargo run --manifest-path tests/fixtures/gen_test_keys.rs

use std::fs;

fn main() {
    // Generate stable RSA 2048-bit key pair using rcgen
    use rcgen::{generate_simple_self_signed_cert, CertificateParams};

    let mut params = CertificateParams::new(vec!["test.example.com".to_string()]);
    // Force RSA 2048 (rcgen default)
    let cert = rcgen::Certificate::from_params(params).expect("Failed to generate cert");

    let cert_der = cert.serialize_der().expect("Failed to serialize cert");
    let key_der = cert.serialize_private_key_der();

    // Save to fixtures
    fs::write("tests/fixtures/test_rsa_2048.crt", &cert_der)
        .expect("Failed to write certificate");
    fs::write("tests/fixtures/test_rsa_2048.der", &key_der)
        .expect("Failed to write private key");

    println!("✓ Generated test_rsa_2048.crt ({} bytes)", cert_der.len());
    println!("✓ Generated test_rsa_2048.der ({} bytes)", key_der.len());
}
