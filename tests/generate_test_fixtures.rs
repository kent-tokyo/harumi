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

    let cert_path = "tests/fixtures/test_rsa_2048.crt";
    let key_path = "tests/fixtures/test_rsa_2048.der";

    println!("Generating test RSA 2048 key pair...");

    use rand::thread_rng;
    use rsa::{RsaPrivateKey, pkcs8::EncodePrivateKey};

    // Generate RSA 2048 key pair
    let mut rng = thread_rng();
    let bits = 2048;
    let private_key = RsaPrivateKey::new(&mut rng, bits).expect("Failed to generate RSA key pair");

    // Export private key as PKCS#8 DER
    let key_der = private_key
        .to_pkcs8_der()
        .expect("Failed to encode private key")
        .to_bytes()
        .to_vec();

    // Self-signed certificate for the *same* RSA key, so signatures made with
    // `key_der` verify against `cert_der`.
    let key_pair = rcgen::KeyPair::from_der(&key_der).expect("Load RSA key into rcgen");
    let mut params = rcgen::CertificateParams::new(vec!["test.example.com".to_string()]);
    params.alg = &rcgen::PKCS_RSA_SHA256;
    params.key_pair = Some(key_pair);
    params.distinguished_name = rcgen::DistinguishedName::new();
    params
        .distinguished_name
        .push(rcgen::DnType::CommonName, "harumi test signer");
    let cert = rcgen::Certificate::from_params(params).expect("Failed to generate cert");
    let cert_der = cert.serialize_der().expect("Failed to serialize cert");

    fs::write(cert_path, &cert_der).expect("Failed to write certificate");
    fs::write(key_path, &key_der).expect("Failed to write private key");

    println!("Test RSA fixtures generated successfully.");
}
