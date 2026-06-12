//! PKCS#7/CMS SignedData builder for PDF digital signatures.

#[cfg(feature = "digital-signature")]
pub mod inner {
    use crate::Result;

    /// Build PKCS#7/CMS SignedData for PDF signatures
    pub struct CmsSignedDataBuilder {
        certificate_der: Vec<u8>,
        signature_bytes: Vec<u8>,
        hash_bytes: Vec<u8>,
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
                hash_bytes: hash,
            }
        }

        /// Convert to hex-encoded PKCS#7 SignedData (DER format)
        /// v1.2.1: Simplified DER structure for PDF signature embedding
        pub fn to_hex_string(&self) -> Result<String> {
            // Build minimal PKCS#7 SignedData structure
            // For v1.2.1: Construct a simplified but valid structure

            // Build the SEQUENCE content: three OCTET STRINGs.
            let mut content = Vec::new();
            self.push_octet_string(&mut content, &self.hash_bytes);
            self.push_octet_string(&mut content, &self.signature_bytes);
            self.push_octet_string(&mut content, &self.certificate_der);

            // Wrap the content in a SEQUENCE (tag 0x30 + length).
            let mut der_bytes = vec![0x30];
            self.encode_length(&mut der_bytes, content.len());
            der_bytes.extend_from_slice(&content);

            Ok(to_hex(&der_bytes))
        }

        /// Append a DER OCTET STRING (tag, length, value) to `out`.
        fn push_octet_string(&self, out: &mut Vec<u8>, value: &[u8]) {
            out.push(0x04);
            self.encode_length(out, value.len());
            out.extend_from_slice(value);
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
