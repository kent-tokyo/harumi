//! Digital signature verification for PDFs.
//!
//! This module provides complete signature verification for PKCS#7-signed PDFs,
//! including cryptographic validation and certificate extraction.

use lopdf::{Dictionary, Object};

use crate::{Document, Result};

/// Read a PDF string entry (`/Key (value)`) from a dictionary as a `String`.
///
/// Returns `None` when the key is absent or not a string object.
fn dict_string(dict: &Dictionary, key: &[u8]) -> Option<String> {
    match dict.get(key) {
        Ok(Object::String(bytes, _)) => Some(String::from_utf8_lossy(bytes).to_string()),
        _ => None,
    }
}

/// Information about a PDF signature field.
///
/// Returned by [`Document::verify_signatures`].
#[derive(Clone, Debug)]
pub struct SignatureInfo {
    /// The signature field name (from `/T` in the signature dictionary).
    pub field_name: String,
    /// Signer name from the signature's `/Name` entry, or else the signing
    /// certificate's subject common name (CN), if available.
    pub signer_name: Option<String>,
    /// Signing time from the signature dictionary's `/M` entry, if available.
    pub signing_time: Option<String>,
    /// Whether the signature is cryptographically valid: the `/ByteRange` covers the
    /// whole file except `/Contents`, its SHA-256 digest matches the signed
    /// `messageDigest`, and the RSA PKCS#1 v1.5 signature verifies against the
    /// certificate embedded in the CMS blob.
    ///
    /// The certificate itself is **not** checked against any trust anchor,
    /// revocation list or validity period; callers that need signer trust must
    /// validate the certificate chain separately.
    pub is_valid: bool,
    /// Reason for signing, from the `/Reason` field if present.
    pub reason: Option<String>,
}

impl Document {
    /// Verifies all signatures in the PDF document.
    ///
    /// # Arguments
    ///
    /// * `pdf_bytes` — The raw PDF file bytes. Required for byte-range validation.
    ///
    /// # Returns
    ///
    /// A vector of signature information objects. Returns an empty vector if
    /// the document has no signature fields.
    ///
    /// # Errors
    ///
    /// Returns an error if the PDF structure is malformed or unreadable.
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use harumi::Document;
    /// # fn main() -> harumi::Result<()> {
    /// let pdf_bytes = std::fs::read("signed.pdf")?;
    /// let doc = Document::from_bytes(&pdf_bytes)?;
    /// let signatures = doc.verify_signatures(&pdf_bytes)?;
    ///
    /// for sig in signatures {
    ///     if sig.is_valid {
    ///         println!("✓ Valid signature: {}", sig.field_name);
    ///     } else {
    ///         println!("✗ Invalid signature: {}", sig.field_name);
    ///     }
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub fn verify_signatures(&self, pdf_bytes: &[u8]) -> Result<Vec<SignatureInfo>> {
        let mut signatures = Vec::new();

        // Locate the AcroForm in the document catalog
        let root_ref = self.inner.trailer.get(b"Root")?.as_reference()?;

        let catalog = self.inner.get_object(root_ref)?.as_dict()?;

        // Get the AcroForm dictionary
        let acroform_ref = match catalog.get(b"AcroForm") {
            Ok(Object::Reference(id)) => id,
            _ => return Ok(signatures), // No AcroForm or not a reference = no forms/signatures
        };

        let acroform = match self.inner.get_object(*acroform_ref) {
            Ok(obj) => match obj.as_dict() {
                Ok(dict) => dict,
                Err(_) => return Ok(signatures),
            },
            Err(_) => return Ok(signatures),
        };

        // Get the Fields array
        let fields_array = match acroform.get(b"Fields") {
            Ok(Object::Array(arr)) => arr,
            _ => return Ok(signatures),
        };

        // Process each field, looking for signature fields
        for field_obj in fields_array {
            if let Object::Reference(field_id) = field_obj
                && let Ok(field_obj) = self.inner.get_object(*field_id)
                && let Ok(field) = field_obj.as_dict()
                && let Ok(Object::Name(name)) = field.get(b"FT")
                && name == b"Sig"
            {
                // This is a signature field
                if let Some(sig_info) = self.extract_signature_info(field, pdf_bytes) {
                    signatures.push(sig_info);
                }
            }
        }

        Ok(signatures)
    }

    /// Extracts signature information from a signature field dictionary.
    fn extract_signature_info(
        &self,
        field: &Dictionary,
        pdf_bytes: &[u8],
    ) -> Option<SignatureInfo> {
        let field_name = dict_string(field, b"T").unwrap_or_else(|| "unknown".to_string());

        // Resolve the signature dictionary referenced by /V.
        let sig_dict_ref = field.get(b"V").ok()?.as_reference().ok()?;
        let sig_dict = self.inner.get_object(sig_dict_ref).ok()?.as_dict().ok()?;

        let reason = dict_string(sig_dict, b"Reason").or_else(|| dict_string(field, b"Reason"));
        let signing_time = dict_string(sig_dict, b"M");

        // lopdf has already decoded the hex string into raw DER bytes.
        let contents = match sig_dict.get(b"Contents") {
            Ok(Object::String(bytes, _)) => bytes.as_slice(),
            _ => return None,
        };

        let byte_range = match sig_dict.get(b"ByteRange") {
            Ok(Object::Array(arr)) if arr.len() == 4 => {
                let mut out = [0usize; 4];
                for (slot, obj) in out.iter_mut().zip(arr) {
                    *slot = usize::try_from(obj.as_i64().ok()?).ok()?;
                }
                out
            }
            _ => return None,
        };

        let verification = verify_cms(contents, byte_range, pdf_bytes);
        let signer_name = dict_string(sig_dict, b"Name")
            .or_else(|| verification.as_ref().and_then(|v| v.signer_name.clone()));

        Some(SignatureInfo {
            field_name,
            signer_name,
            signing_time,
            is_valid: verification.is_some_and(|v| v.is_valid),
            reason,
        })
    }
}

/// Outcome of checking one CMS signature.
struct CmsVerification {
    is_valid: bool,
    signer_name: Option<String>,
}

#[cfg(not(feature = "digital-signature"))]
fn verify_cms(_contents: &[u8], _byte_range: [usize; 4], _pdf: &[u8]) -> Option<CmsVerification> {
    None
}

/// Verify a detached CMS `SignedData` against the PDF bytes covered by `byte_range`.
///
/// Returns `None` when the blob cannot be parsed at all.
#[cfg(feature = "digital-signature")]
fn verify_cms(contents: &[u8], byte_range: [usize; 4], pdf: &[u8]) -> Option<CmsVerification> {
    use crate::signature_create::inner::{
        OID_MESSAGE_DIGEST, OID_SHA256, digest_byte_range, rsa_public_key_from_cert,
        subject_common_name,
    };
    use cms::cert::CertificateChoices;
    use cms::content_info::ContentInfo;
    use cms::signed_data::{SignedData, SignerIdentifier};
    use der::{Decode, Encode, SliceReader};
    use rsa::Pkcs1v15Sign;
    use sha2::{Digest, Sha256};

    // /Contents is zero-padded after the DER value: decode exactly one value.
    let mut reader = SliceReader::new(contents).ok()?;
    let content_info = ContentInfo::decode(&mut reader).ok()?;
    let signed_data: SignedData = content_info.content.decode_as().ok()?;
    let signer = signed_data.signer_infos.0.iter().next()?;

    let certificate = signed_data
        .certificates
        .as_ref()?
        .0
        .iter()
        .filter_map(|c| match c {
            CertificateChoices::Certificate(cert) => Some(cert),
            _ => None,
        })
        .find(|cert| match &signer.sid {
            SignerIdentifier::IssuerAndSerialNumber(ias) => {
                cert.tbs_certificate.issuer == ias.issuer
                    && cert.tbs_certificate.serial_number == ias.serial_number
            }
            SignerIdentifier::SubjectKeyIdentifier(_) => true,
        })?;
    let signer_name = subject_common_name(certificate);
    let invalid = || {
        Some(CmsVerification {
            is_valid: false,
            signer_name: signer_name.clone(),
        })
    };

    // The signed ranges must be in order and cover the whole file except /Contents;
    // anything appended later would otherwise go unnoticed.
    let [s1, l1, s2, l2] = byte_range;
    if s1 != 0 || l1 > s2 || s2.checked_add(l2) != Some(pdf.len()) {
        return invalid();
    }
    if signer.digest_alg.oid != OID_SHA256 {
        return invalid();
    }
    let Some(content_digest) = digest_byte_range(pdf, byte_range) else {
        return invalid();
    };
    let Some(public_key) = rsa_public_key_from_cert(certificate) else {
        return invalid();
    };

    let signed_digest = match &signer.signed_attrs {
        Some(attrs) => {
            let message_digest = attrs
                .iter()
                .find(|a| a.oid == OID_MESSAGE_DIGEST)
                .and_then(|a| a.values.iter().next())
                .and_then(|v| v.decode_as::<der::asn1::OctetString>().ok());
            if message_digest.as_ref().map(|d| d.as_bytes()) != Some(content_digest.as_slice()) {
                return invalid();
            }
            let Ok(attrs_der) = attrs.to_der() else {
                return invalid();
            };
            Sha256::digest(&attrs_der).to_vec()
        }
        None => content_digest,
    };

    let is_valid = public_key
        .verify(
            Pkcs1v15Sign::new::<Sha256>(),
            &signed_digest,
            signer.signature.as_bytes(),
        )
        .is_ok();
    Some(CmsVerification {
        is_valid,
        signer_name,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn verify_signatures_returns_empty_for_unsigned_doc() {
        let mut doc = Document::new((100.0, 100.0)).unwrap();
        let pdf_bytes = doc.save_to_bytes().unwrap();
        let signatures = doc.verify_signatures(&pdf_bytes).unwrap();
        assert_eq!(
            signatures.len(),
            0,
            "unsigned document should have no signatures"
        );
    }
}
