//! PKCS#7/CMS SignedData builder for PDF digital signatures.

#[cfg(feature = "digital-signature")]
pub mod inner {
    use crate::Result;

    /// Build PKCS#7/CMS SignedData for PDF signatures
    pub struct CmsSignedDataBuilder {
        certificate_der: Vec<u8>,
        signature_bytes: Vec<u8>,
        _hash_bytes: Vec<u8>,
    }

    impl CmsSignedDataBuilder {
        /// Create a new CMS SignedData builder
        pub fn new(
            cert_der: Vec<u8>,
            signature: Vec<u8>,
            hash: Vec<u8>,
        ) -> Self {
            CmsSignedDataBuilder {
                certificate_der: cert_der,
                signature_bytes: signature,
                _hash_bytes: hash,
            }
        }

        /// Convert to hex-encoded PKCS#7 SignedData (DER format)
        /// v1.2.2: More complete PKCS#7 structure with OIDs and algorithm identifiers
        pub fn to_hex_string(&self) -> Result<String> {
            // Build PKCS#7 SignedData structure per RFC 2630:
            // ContentInfo { contentType=signedData, content=SignedData }
            // SignedData { version, digestAlgorithms, contentInfo, certificates, signerInfos }

            // SHA-256 OID: 2.16.840.1.101.3.4.2.1
            let sha256_oid = vec![0x60, 0x86, 0x48, 0x01, 0x65, 0x03, 0x04, 0x02, 0x01];
            // RSA Encryption OID: 1.2.840.113549.1.1.1
            let rsa_oid = vec![0x2a, 0x86, 0x48, 0x86, 0xf7, 0x0d, 0x01, 0x01, 0x01];
            // signedData OID: 1.2.840.113549.1.7.2
            let signed_data_oid = vec![0x2a, 0x86, 0x48, 0x86, 0xf7, 0x0d, 0x01, 0x07, 0x02];
            // data OID: 1.2.840.113549.1.7.1
            let data_oid = vec![0x2a, 0x86, 0x48, 0x86, 0xf7, 0x0d, 0x01, 0x07, 0x01];

            // Build DigestAlgorithmIdentifier SEQUENCE (SHA-256 + NULL params)
            let mut digest_alg = Vec::new();
            digest_alg.push(0x30); // SEQUENCE tag
            let digest_alg_len = 2 + sha256_oid.len() + 2; // OID + NULL
            self.encode_length(&mut digest_alg, digest_alg_len);
            digest_alg.push(0x06); // OID tag
            self.encode_length(&mut digest_alg, sha256_oid.len());
            digest_alg.extend_from_slice(&sha256_oid);
            digest_alg.push(0x05); // NULL tag
            digest_alg.push(0x00); // NULL length

            // Build DigestAlgorithms SET OF (just SHA-256)
            let mut digest_algs = Vec::new();
            digest_algs.push(0x31); // SET tag
            self.encode_length(&mut digest_algs, digest_alg.len());
            digest_algs.extend_from_slice(&digest_alg);

            // Build ContentInfo (data) - simplified: just empty content
            let mut content_info = Vec::new();
            content_info.push(0x30); // SEQUENCE tag
            let content_info_len = 2 + data_oid.len() + 2; // OID + [0] (empty)
            self.encode_length(&mut content_info, content_info_len);
            content_info.push(0x06); // OID tag
            self.encode_length(&mut content_info, data_oid.len());
            content_info.extend_from_slice(&data_oid);
            content_info.push(0xa0); // [0] EXPLICIT tag
            content_info.push(0x00); // length 0 (no data)

            // Build Certificate SET (with signing certificate)
            let mut certs = Vec::new();
            certs.push(0xa0); // [0] IMPLICIT tag for certificates
            self.encode_length(&mut certs, self.certificate_der.len());
            certs.extend_from_slice(&self.certificate_der);

            // Build SignerInfo
            let signer_info = self.build_signer_info(&sha256_oid, &rsa_oid)?;

            // Build SignerInfos SET OF
            let mut signer_infos = Vec::new();
            signer_infos.push(0x31); // SET tag
            self.encode_length(&mut signer_infos, signer_info.len());
            signer_infos.extend_from_slice(&signer_info);

            // Build SignedData SEQUENCE
            let mut signed_data = Vec::new();
            signed_data.push(0x30); // SEQUENCE tag

            // SignedData content: version (1 byte INTEGER=3) + digestAlgs + contentInfo + certs + signerInfos
            let signed_data_content_len = 3 + digest_algs.len() + content_info.len() + certs.len() + signer_infos.len();
            self.encode_length(&mut signed_data, signed_data_content_len);

            // Version 3
            signed_data.push(0x02); // INTEGER tag
            signed_data.push(0x01); // length
            signed_data.push(0x03); // value

            signed_data.extend_from_slice(&digest_algs);
            signed_data.extend_from_slice(&content_info);
            signed_data.extend_from_slice(&certs);
            signed_data.extend_from_slice(&signer_infos);

            // Build ContentInfo wrapper (signedData)
            let mut cms = Vec::new();
            cms.push(0x30); // SEQUENCE tag

            let cms_len = 2 + signed_data_oid.len() + signed_data.len();
            self.encode_length(&mut cms, cms_len);

            cms.push(0x06); // OID tag
            self.encode_length(&mut cms, signed_data_oid.len());
            cms.extend_from_slice(&signed_data_oid);

            cms.push(0xa0); // [0] EXPLICIT tag
            self.encode_length(&mut cms, signed_data.len());
            cms.extend_from_slice(&signed_data);

            Ok(to_hex(&cms))
        }

        /// Build SignerInfo SEQUENCE for PKCS#7 SignedData
        fn build_signer_info(&self, sha256_oid: &[u8], rsa_oid: &[u8]) -> Result<Vec<u8>> {
            let mut signer_info = Vec::new();
            signer_info.push(0x30); // SEQUENCE tag

            // SignerInfo content: version + digestAlgorithm + encryptionAlgorithm + encryptedDigest
            // For v1.2.2: Simplified version without IssuerAndSerialNumber and attributes
            // version (INTEGER 1)
            let version = vec![0x02, 0x01, 0x01];

            // DigestAlgorithmIdentifier
            let mut digest_alg = Vec::new();
            digest_alg.push(0x30); // SEQUENCE tag
            let digest_alg_len = 2 + sha256_oid.len() + 2; // OID + NULL
            self.encode_length(&mut digest_alg, digest_alg_len);
            digest_alg.push(0x06); // OID tag
            self.encode_length(&mut digest_alg, sha256_oid.len());
            digest_alg.extend_from_slice(sha256_oid);
            digest_alg.push(0x05); // NULL tag
            digest_alg.push(0x00); // NULL length

            // DigestEncryptionAlgorithmIdentifier (RSA)
            let mut enc_alg = Vec::new();
            enc_alg.push(0x30); // SEQUENCE tag
            let enc_alg_len = 2 + rsa_oid.len() + 2;
            self.encode_length(&mut enc_alg, enc_alg_len);
            enc_alg.push(0x06); // OID tag
            self.encode_length(&mut enc_alg, rsa_oid.len());
            enc_alg.extend_from_slice(rsa_oid);
            enc_alg.push(0x05); // NULL tag
            enc_alg.push(0x00); // NULL length

            // EncryptedDigest (OCTET STRING with signature bytes)
            let mut enc_digest = Vec::new();
            enc_digest.push(0x04); // OCTET STRING tag
            self.encode_length(&mut enc_digest, self.signature_bytes.len());
            enc_digest.extend_from_slice(&self.signature_bytes);

            // Calculate total length
            let signer_info_content_len = version.len() + digest_alg.len() + enc_alg.len() + enc_digest.len();
            self.encode_length(&mut signer_info, signer_info_content_len);
            signer_info.extend_from_slice(&version);
            signer_info.extend_from_slice(&digest_alg);
            signer_info.extend_from_slice(&enc_alg);
            signer_info.extend_from_slice(&enc_digest);

            Ok(signer_info)
        }

        /// Encode a length in DER format.
        ///
        /// Lengths below 128 use the short form (a single byte). Larger lengths
        /// use the long form: `0x80 | byte_count` followed by the big-endian
        /// minimal-width bytes.
        fn encode_length(&self, result: &mut Vec<u8>, len: usize) {
            if len < 128 {
                result.push(len as u8);
                return;
            }

            let be = len.to_be_bytes();
            let significant = &be[be.iter().take_while(|&&b| b == 0).count()..];
            result.push(0x80 | significant.len() as u8);
            result.extend_from_slice(significant);
        }
    }

    /// Lowercase hex-encode a byte slice.
    fn to_hex(bytes: &[u8]) -> String {
        use std::fmt::Write;
        bytes.iter().fold(String::with_capacity(bytes.len() * 2), |mut acc, b| {
            let _ = write!(acc, "{:02x}", b);
            acc
        })
    }
}

#[cfg(feature = "digital-signature")]
pub use inner::*;
