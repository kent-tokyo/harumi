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
        let input: String = input
            .chars()
            .filter(|c| !matches!(c, '\n' | '\r' | ' '))
            .collect();

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

        // The OID is immediately followed by the value's tag (offset +5),
        // its length (offset +6), and then the UTF-8/printable bytes.
        let pos = find_subsequence(der_bytes, cn_oid)?;
        if pos + 6 >= der_bytes.len() {
            return None;
        }

        // Only UTF8String (0x0C) and PrintableString (0x13) carry a CN here.
        let tag = der_bytes[pos + 5];
        if tag != 0x13 && tag != 0x0C {
            return None;
        }

        let len = der_bytes[pos + 6] as usize;
        let value = der_bytes.get(pos + 7..pos + 7 + len)?;
        std::str::from_utf8(value).ok().map(str::to_string)
    }

    /// Find a subsequence in a byte array.
    fn find_subsequence(haystack: &[u8], needle: &[u8]) -> Option<usize> {
        haystack.windows(needle.len()).position(|w| w == needle)
    }

    /// Hash the PDF content using SHA-256.
    /// Per PDF spec ISO 32000-2, the signature hash is calculated over the ByteRange only:
    /// - bytes [0, start2) and bytes [start2 + length2, EOF)
    ///
    /// The /Contents placeholder itself (between start2 and length2) is excluded.
    pub fn hash_pdf_content_with_byte_range(
        content: &[u8],
        byte_range: [u32; 4],
    ) -> Vec<u8> {
        let mut hasher = Sha256::new();
        let start1 = byte_range[0] as usize;
        let length1 = byte_range[1] as usize;
        let start2 = byte_range[2] as usize;
        let length2 = byte_range[3] as usize;

        // Hash [start1, start1 + length1)
        if start1 + length1 <= content.len() {
            hasher.update(&content[start1..start1 + length1]);
        }

        // Hash [start2, start2 + length2)
        if start2 + length2 <= content.len() {
            hasher.update(&content[start2..start2 + length2]);
        }

        hasher.finalize().to_vec()
    }

    /// Hash the PDF content using SHA-256.
    /// NOTE: This hashes the entire PDF. For proper signature verification,
    /// use `hash_pdf_content_with_byte_range` after the ByteRange is known.
    pub fn hash_pdf_content(content: &[u8]) -> Vec<u8> {
        let mut hasher = Sha256::new();
        hasher.update(content);
        hasher.finalize().to_vec()
    }

    /// Create an RSA signature using PKCS#1 v1.5 with SHA-256 digest info.
    pub fn sign_hash(private_key: &RsaPrivateKey, hash: &[u8]) -> Result<Vec<u8>> {
        use rsa::traits::{PublicKeyParts, PrivateKeyParts};
        use num_bigint::BigUint;

        // Build DigestInfo with SHA-256 OID and hash
        let sha256_oid = vec![0x60, 0x86, 0x48, 0x01, 0x65, 0x03, 0x04, 0x02, 0x01];

        // AlgorithmIdentifier SEQUENCE for SHA-256
        let mut alg_id = vec![0x30]; // SEQUENCE tag
        let alg_content_len = 2 + sha256_oid.len() + 2;
        encode_der_length(&mut alg_id, alg_content_len);
        alg_id.push(0x06); // OID tag
        encode_der_length(&mut alg_id, sha256_oid.len());
        alg_id.extend_from_slice(&sha256_oid);
        alg_id.push(0x05); // NULL tag
        alg_id.push(0x00);

        // DigestInfo SEQUENCE
        let mut digest_info = vec![0x30]; // SEQUENCE tag
        let digest_info_content_len = alg_id.len() + 2 + hash.len();
        encode_der_length(&mut digest_info, digest_info_content_len);
        digest_info.extend_from_slice(&alg_id);
        digest_info.push(0x04); // OCTET STRING tag
        encode_der_length(&mut digest_info, hash.len());
        digest_info.extend_from_slice(hash);

        // Apply PKCS#1 v1.5 signature padding
        let modulus_size = private_key.size();
        let padded = build_pkcs1v15_signature_padding(&digest_info, modulus_size)?;

        // Perform RSA operation: signature = padded_message^d mod n
        let m = BigUint::from_bytes_be(&padded);
        let d = BigUint::from_bytes_be(&private_key.d().to_bytes_be());
        let n = BigUint::from_bytes_be(&private_key.n().to_bytes_be());

        let signature_int = m.modpow(&d, &n);

        // Convert back to bytes, padding with zeros to match modulus size
        let mut signature = signature_int.to_bytes_be();
        if signature.len() < modulus_size {
            let mut padded_sig = vec![0u8; modulus_size - signature.len()];
            padded_sig.extend_from_slice(&signature);
            signature = padded_sig;
        }

        Ok(signature)
    }

    /// Build PKCS#1 v1.5 signature padding.
    /// Format: 0x00 || 0x01 || PS || 0x00 || DigestInfo
    /// where PS is 0xFF bytes (padding string)
    fn build_pkcs1v15_signature_padding(digest_info: &[u8], modulus_size: usize) -> Result<Vec<u8>> {
        if digest_info.len() + 11 > modulus_size {
            return Err(crate::Error::SignatureFailed(
                "Digest too large for PKCS#1 v1.5 padding".into(),
            ));
        }

        let mut padded = Vec::with_capacity(modulus_size);
        padded.push(0x00);
        padded.push(0x01);

        // Padding string: 0xFF bytes
        let ps_len = modulus_size - digest_info.len() - 3;
        padded.extend(std::iter::repeat_n(0xFF, ps_len));

        padded.push(0x00); // Separator
        padded.extend_from_slice(digest_info);

        Ok(padded)
    }

    /// Encode a length value in DER format.
    /// Supports both short form (length < 128) and long form (multi-byte).
    fn encode_der_length(result: &mut Vec<u8>, len: usize) {
        if len < 128 {
            result.push(len as u8);
        } else {
            let be = len.to_be_bytes();
            let significant = &be[be.iter().take_while(|&&b| b == 0).count()..];
            result.push(0x80 | significant.len() as u8);
            result.extend_from_slice(significant);
        }
    }


    /// Calculate PDF signature ByteRange [0, X, Y, Z]
    ///
    /// Per PDF spec ISO 32000-2, ByteRange is [start1, length1, start2, length2]:
    /// - start1=0, length1=bytes before /Contents hex placeholder
    /// - start2=bytes at which hex placeholder starts, length2=bytes after placeholder
    ///
    /// For PDF incremental update with signature:
    /// - [0, X] = bytes before /Contents hex placeholder
    /// - [X, Y] = length of hex placeholder (2 × DER signature size)
    /// - [Y, Z] = remaining bytes after placeholder (xref + trailer)
    pub fn calculate_byte_range(
        pre_contents_offset: u32,
        hex_string_length: u32,
        total_pdf_size: u32,
    ) -> Result<[u32; 4]> {
        if pre_contents_offset == 0 || hex_string_length == 0 {
            return Err(crate::Error::InvalidInput(
                "ByteRange offset and length must be > 0".into(),
            ));
        }

        // ByteRange format: [start1, length1, start2, length2]
        let start1 = 0;
        let length1 = pre_contents_offset;
        let start2 = pre_contents_offset + hex_string_length;
        let length2 = total_pdf_size - start2;

        Ok([start1, length1, start2, length2])
    }
}

#[cfg(feature = "digital-signature")]
pub use inner::*;
