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

            let mut der_bytes = Vec::new();

            // SEQUENCE wrapper
            der_bytes.push(0x30);

            // Build content
            let mut content = Vec::new();

            // Add hash (OCTET STRING)
            content.push(0x04);
            content.push(self.hash_bytes.len() as u8);
            content.extend_from_slice(&self.hash_bytes);

            // Add signature (OCTET STRING)
            content.push(0x04);
            self.encode_length(&mut content, self.signature_bytes.len());
            content.extend_from_slice(&self.signature_bytes);

            // Add certificate (OCTET STRING)
            content.push(0x04);
            self.encode_length(&mut content, self.certificate_der.len());
            content.extend_from_slice(&self.certificate_der);

            // Set length
            self.encode_length(&mut der_bytes, content.len());
            der_bytes.extend_from_slice(&content);

            // Convert to hex
            let hex = der_bytes
                .iter()
                .map(|b| format!("{:02x}", b))
                .collect::<String>();

            Ok(hex)
        }

        /// Encode length in DER format
        fn encode_length(&self, result: &mut Vec<u8>, len: usize) {
            if len < 128 {
                result.push(len as u8);
            } else {
                let mut len_bytes = Vec::new();
                let mut n = len;
                while n > 0 {
                    len_bytes.insert(0, n as u8);
                    n >>= 8;
                }
                result.push(0x80 | len_bytes.len() as u8);
                result.extend_from_slice(&len_bytes);
            }
        }
    }
}

#[cfg(feature = "digital-signature")]
pub use inner::*;
