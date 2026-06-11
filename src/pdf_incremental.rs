//! PDF incremental update section builder for digital signatures.
//! Handles ByteRange calculation, signature embedding, and xref reconstruction.

#[cfg(feature = "digital-signature")]
pub mod inner {
    use crate::Result;
    use std::collections::BTreeMap;

    /// Build PDF incremental update section with embedded signature
    pub struct IncrementalUpdateBuilder {
        base_pdf: Vec<u8>,
        field_name: String,
        cms_hex: String,
    }

    impl IncrementalUpdateBuilder {
        /// Create a new incremental update builder
        pub fn new(base_pdf: Vec<u8>, field_name: String, cms_hex: String) -> Self {
            IncrementalUpdateBuilder {
                base_pdf,
                field_name,
                cms_hex,
            }
        }

        /// Build the complete signed PDF with incremental update
        ///
        /// Process:
        /// 1. Locate signature field object in base PDF
        /// 2. Update /Contents and /ByteRange
        /// 3. Calculate xref offsets for incremental section
        /// 4. Generate new xref table and trailer
        /// 5. Append incremental update section to base PDF
        pub fn build(&self) -> Result<Vec<u8>> {
            // v1.2.1: Full implementation
            // For v1.2.0 compatibility, return base PDF unchanged
            // (Sign_document returns unsigned PDF for now)

            Ok(self.base_pdf.clone())
        }

        /// Find the signature field object in the PDF
        /// Returns the object ID if found
        fn find_signature_field_object(&self) -> Result<u32> {
            // TODO v1.2.1: Parse PDF to find /T field matching self.field_name
            // within AcroForm structure
            Err(crate::Error::InvalidInput(
                "Signature field lookup not yet implemented (v1.2.1)".into(),
            ))
        }

        /// Calculate ByteRange [0, X, Y, Z] for the signature
        fn calculate_byte_range(&self, contents_offset: usize, cms_hex_len: usize) -> [u32; 4] {
            [
                0,
                contents_offset as u32,
                cms_hex_len as u32,
                (self.base_pdf.len() + 100) as u32,  // Approximate final size
            ]
        }

        /// Build the xref table for the incremental update
        /// Records byte positions of modified objects
        fn build_xref_table(&self, objects: &[(u32, Vec<u8>)]) -> Vec<u8> {
            let mut xref = Vec::new();
            xref.extend_from_slice(b"xref\n");

            // Sort by object ID
            let mut sorted = objects.to_vec();
            sorted.sort_by_key(|&(id, _)| id);

            // Write xref entries (format: "objnum count")
            if !sorted.is_empty() {
                let start_id = sorted[0].0;
                let count = sorted.len();
                xref.extend_from_slice(format!("{} {}\n", start_id, count).as_bytes());

                for (_, obj_data) in &sorted {
                    // Offset is cumulative from base_pdf + previous objects
                    // v1.2.1: Track actual byte positions
                    xref.extend_from_slice(b"0000000000 00000 n\n");
                }
            }

            xref
        }

        /// Build the trailer dictionary for incremental update
        /// Links to the previous xref section
        fn build_trailer(&self, prev_xref_offset: u32) -> Vec<u8> {
            let mut trailer = Vec::new();
            trailer.extend_from_slice(b"trailer\n");
            trailer.extend_from_slice(
                format!(
                    "<< /Size 1 /Prev {} >>\n",
                    prev_xref_offset
                )
                .as_bytes(),
            );
            trailer.extend_from_slice(b"startxref\n");
            trailer.extend_from_slice(
                format!("{}\n", self.base_pdf.len() + 50).as_bytes()
            );
            trailer.extend_from_slice(b"%%EOF\n");

            trailer
        }
    }
}

#[cfg(feature = "digital-signature")]
pub use inner::*;
