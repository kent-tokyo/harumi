//! Digital signature verification for PDFs.
//!
//! This module provides signature verification for PKCS#7-signed PDFs.
//! Currently, signatures are extracted but not cryptographically verified (stub implementation).

use lopdf::{Dictionary, Object};

use crate::{Document, Result};

/// Information about a PDF signature field.
///
/// Returned by [`Document::verify_signatures`].
#[derive(Clone, Debug)]
pub struct SignatureInfo {
    /// The signature field name (from `/T` in the signature dictionary).
    pub field_name: String,
    /// Signer name extracted from the certificate's CN attribute, if available.
    pub signer_name: Option<String>,
    /// Signing time from the signature or CMS metadata, if available.
    pub signing_time: Option<String>,
    /// Whether the signature is valid (hash matches + certificate chain validates).
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
        _pdf_bytes: &[u8],
    ) -> Option<SignatureInfo> {
        // Get field name
        let field_name = match field.get(b"T") {
            Ok(Object::String(bytes, _)) => String::from_utf8_lossy(bytes).to_string(),
            _ => "unknown".to_string(),
        };

        // Get reason for signing
        let reason = match field.get(b"Reason") {
            Ok(Object::String(bytes, _)) => Some(String::from_utf8_lossy(bytes).to_string()),
            _ => None,
        };

        // Get signing time
        let signing_time = match field.get(b"M") {
            Ok(Object::String(bytes, _)) => Some(String::from_utf8_lossy(bytes).to_string()),
            _ => None,
        };

        // TODO: Implement actual signature validation
        // For now, always return is_valid = false to indicate validation is not yet implemented
        let is_valid = false;

        Some(SignatureInfo {
            field_name,
            signer_name: None, // TODO: Extract from certificate
            signing_time,
            is_valid,
            reason,
        })
    }
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
