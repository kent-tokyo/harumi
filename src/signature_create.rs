//! Digital signature creation for PDFs.
//!
//! This module provides functionality to create PKCS#7/CMS signatures for PDF documents.
//! Requires the `digital-signature` feature.

#[cfg(feature = "digital-signature")]
pub mod inner {
    use crate::Result;
    use pkcs1::DecodeRsaPrivateKey;
    use pkcs8::DecodePrivateKey;
    use rsa::RsaPrivateKey;
    use sha2::{Digest, Sha256};

    /// Input format for X.509 certificates.
    pub enum CertificateInput {
        /// PEM-encoded X.509 certificate.
        Pem(Vec<u8>),
        /// DER-encoded X.509 certificate.
        Der(Vec<u8>),
    }

    /// Input format for private keys.
    pub enum PrivateKeyInput {
        /// PEM-encoded PKCS#1 or PKCS#8 private key.
        Pem(Vec<u8>),
        /// DER-encoded PKCS#1 or PKCS#8 private key.
        Der(Vec<u8>),
    }

    /// Options for creating a signature field in a PDF.
    pub struct SignatureFieldOptions {
        /// Field name (used in AcroForm).
        pub field_name: String,
        /// Reason for signing (stored in `/Reason` dictionary entry).
        pub reason: Option<String>,
        /// Contact information (stored in `/ContactInfo`).
        pub contact_info: Option<String>,
        /// Whether to lock the document after signing.
        pub lock_permissions: bool,
    }

    /// Context for signing PDF documents.
    pub struct SigningContext {
        cert_der: Vec<u8>,
        private_key: RsaPrivateKey,
        signer_name: String,
    }

    impl SigningContext {
        /// Create a signing context from a certificate and private key.
        pub fn from_cert_and_key(
            cert: CertificateInput,
            key: PrivateKeyInput,
        ) -> Result<Self> {
            let cert_der = match cert {
                CertificateInput::Pem(pem_bytes) => parse_pem_to_der(&pem_bytes, "CERTIFICATE")?,
                CertificateInput::Der(der_bytes) => der_bytes,
            };

            let key_der = match key {
                PrivateKeyInput::Pem(pem_bytes) => {
                    parse_pem_to_der(&pem_bytes, "PRIVATE KEY")
                        .or_else(|_| parse_pem_to_der(&pem_bytes, "RSA PRIVATE KEY"))?
                }
                PrivateKeyInput::Der(der_bytes) => der_bytes,
            };

            let signer_name = extract_subject_cn_from_der(&cert_der)
                .unwrap_or_else(|| "Unknown Signer".to_string());

            let private_key = RsaPrivateKey::from_pkcs8_der(&key_der)
                .or_else(|_| RsaPrivateKey::from_pkcs1_der(&key_der))
                .map_err(|e| crate::Error::InvalidPrivateKey(format!("{}", e)))?;

            Ok(SigningContext {
                cert_der,
                private_key,
                signer_name,
            })
        }

        /// Get the signer's name.
        pub fn signer_name(&self) -> &str {
            &self.signer_name
        }

        /// Get the certificate DER bytes.
        pub fn cert_der(&self) -> &[u8] {
            &self.cert_der
        }

        /// Get a reference to the private key.
        pub fn private_key(&self) -> &RsaPrivateKey {
            &self.private_key
        }
    }

    /// Parse PEM format to DER.
    fn parse_pem_to_der(pem_bytes: &[u8], block_name: &str) -> Result<Vec<u8>> {
        let pem_str = std::str::from_utf8(pem_bytes)
            .map_err(|e| crate::Error::InvalidCertificate(format!("Invalid UTF-8: {}", e)))?;

        let begin = format!("-----BEGIN {}-----", block_name);
        let end = format!("-----END {}-----", block_name);

        let start_idx = pem_str
            .find(&begin)
            .ok_or_else(|| {
                crate::Error::InvalidCertificate(format!("No {} block found in PEM", block_name))
            })?;

        let end_idx = pem_str.find(&end).ok_or_else(|| {
            crate::Error::InvalidCertificate(format!("No {} block found in PEM", block_name))
        })?;

        let content = &pem_str[start_idx + begin.len()..end_idx];
        let base64_str = content
            .lines()
            .map(|line| line.trim())
            .filter(|line| !line.is_empty())
            .collect::<String>();

        base64_decode(&base64_str)
    }

    /// Simple base64 decoder.
    fn base64_decode(input: &str) -> Result<Vec<u8>> {
        const BASE64_CHARS: &str =
            "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

        let mut result = Vec::new();
        let input = input.replace('\n', "").replace('\r', "").replace(' ', "");

        for chunk in input.as_bytes().chunks(4) {
            if chunk.len() < 2 {
                continue;
            }

            let b1 = BASE64_CHARS
                .find(chunk[0] as char)
                .ok_or_else(|| crate::Error::InvalidCertificate("Invalid base64 char".into()))?
                as u8;

            let b2 = BASE64_CHARS
                .find(chunk[1] as char)
                .ok_or_else(|| crate::Error::InvalidCertificate("Invalid base64 char".into()))?
                as u8;

            result.push((b1 << 2) | (b2 >> 4));

            if chunk.len() > 2 && chunk[2] as char != '=' {
                let b3 = BASE64_CHARS
                    .find(chunk[2] as char)
                    .ok_or_else(|| {
                        crate::Error::InvalidCertificate("Invalid base64 char".into())
                    })?
                    as u8;

                result.push((b2 << 4) | (b3 >> 2));

                if chunk.len() > 3 && chunk[3] as char != '=' {
                    let b4 = BASE64_CHARS
                        .find(chunk[3] as char)
                        .ok_or_else(|| {
                            crate::Error::InvalidCertificate("Invalid base64 char".into())
                        })?
                        as u8;

                    result.push((b3 << 6) | b4);
                }
            }
        }

        Ok(result)
    }

    /// Extract CN (Common Name) from X.509 DER certificate.
    /// Simple parsing without full X.509 library dependency.
    fn extract_subject_cn_from_der(der_bytes: &[u8]) -> Option<String> {
        // Very basic DER parsing for CN extraction
        // In production, use x509-cert crate, but this is a fallback
        let cn_oid = &[0x06, 0x03, 0x55, 0x04, 0x03]; // 2.5.4.3 (CN OID)

        if let Some(pos) = find_subsequence(der_bytes, cn_oid) {
            if pos + 5 < der_bytes.len() {
                let tag = der_bytes[pos + 5];
                if tag == 0x13 || tag == 0x0C {
                    // UTF8String or PrintableString
                    if pos + 6 < der_bytes.len() {
                        let len = der_bytes[pos + 6] as usize;
                        if pos + 7 + len <= der_bytes.len() {
                            if let Ok(cn) = std::str::from_utf8(&der_bytes[pos + 7..pos + 7 + len])
                            {
                                return Some(cn.to_string());
                            }
                        }
                    }
                }
            }
        }
        None
    }

    /// Find a subsequence in a byte array.
    fn find_subsequence(haystack: &[u8], needle: &[u8]) -> Option<usize> {
        haystack.windows(needle.len()).position(|w| w == needle)
    }

    /// Hash the PDF content using SHA-256.
    pub fn hash_pdf_content(content: &[u8]) -> Vec<u8> {
        let mut hasher = Sha256::new();
        hasher.update(content);
        hasher.finalize().to_vec()
    }

    /// Create an RSA PKCS#1 v1.5 signature of the hash.
    pub fn sign_hash(private_key: &RsaPrivateKey, hash: &[u8]) -> Result<Vec<u8>> {
        use rsa::Pss;

        private_key
            .sign(Pss::new::<Sha256>(), hash)
            .map_err(|e| crate::Error::SignatureFailed(format!("{}", e)))
    }
}

#[cfg(feature = "digital-signature")]
pub use inner::*;
