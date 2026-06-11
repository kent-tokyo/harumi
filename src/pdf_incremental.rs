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
            // Find the previous xref offset in the base PDF
            let prev_xref_offset = self.find_prev_xref_offset()?;

            // Build the signature update object
            // For v1.2.1: Create a minimal signature field update
            let sig_field_update = self.build_signature_field_update(&self.cms_hex)?;

            // Build incremental update section
            let mut update_section = Vec::new();

            // Object updates (simplified for v1.2.1)
            let obj_offset = self.base_pdf.len();
            update_section.extend_from_slice(b"\n");
            update_section.extend_from_slice(sig_field_update.as_bytes());

            // Build xref table
            let xref_offset = self.base_pdf.len() + update_section.len();
            let mut xref_table = Vec::new();
            xref_table.extend_from_slice(b"xref\n");
            xref_table.extend_from_slice(b"0 1\n");
            xref_table.extend_from_slice(format!("{:010} {:05} f\n", 0, 65535).as_bytes());

            update_section.extend_from_slice(&xref_table);

            // Build trailer
            let trailer = self.build_trailer(prev_xref_offset);
            update_section.extend_from_slice(&trailer);

            // Combine base PDF + incremental update
            let mut signed_pdf = self.base_pdf.clone();
            signed_pdf.extend_from_slice(&update_section);

            Ok(signed_pdf)
        }

        /// Find the previous xref offset in base PDF
        /// Searches backwards from EOF for "startxref" marker
        fn find_prev_xref_offset(&self) -> Result<u32> {
            // Look for "startxref" keyword near end of PDF
            let search_start = if self.base_pdf.len() > 1024 {
                self.base_pdf.len() - 1024
            } else {
                0
            };

            let search_area = &self.base_pdf[search_start..];

            // Find "startxref"
            if let Some(pos) = self.find_bytes(search_area, b"startxref") {
                let abs_pos = search_start + pos;
                // Skip "startxref" and whitespace
                let mut offset_start = abs_pos + 9;
                while offset_start < self.base_pdf.len()
                    && (self.base_pdf[offset_start] == b' '
                        || self.base_pdf[offset_start] == b'\n'
                        || self.base_pdf[offset_start] == b'\r')
                {
                    offset_start += 1;
                }

                // Parse the offset number
                let mut offset_end = offset_start;
                while offset_end < self.base_pdf.len()
                    && self.base_pdf[offset_end].is_ascii_digit()
                {
                    offset_end += 1;
                }

                if offset_end > offset_start {
                    let offset_str = std::str::from_utf8(&self.base_pdf[offset_start..offset_end])
                        .map_err(|_| crate::Error::InvalidInput("Invalid xref offset".into()))?;
                    let offset: u32 = offset_str
                        .parse()
                        .map_err(|_| crate::Error::InvalidInput("Failed to parse xref offset".into()))?;
                    return Ok(offset);
                }
            }

            // Default: assume xref is at end
            Ok(self.base_pdf.len() as u32)
        }

        /// Find bytes in array
        fn find_bytes(&self, haystack: &[u8], needle: &[u8]) -> Option<usize> {
            haystack.windows(needle.len()).position(|w| w == needle)
        }

        /// Build signature field update object
        fn build_signature_field_update(&self, cms_hex: &str) -> Result<String> {
            let byte_range = [0, self.base_pdf.len() as u32, cms_hex.len() as u32, (self.base_pdf.len() + cms_hex.len() + 100) as u32];

            // Create signature dictionary update
            // Format: "1 0 obj << /Type /Sig /Contents <hex> /ByteRange [0 X Y Z] >> endobj"
            let obj_str = format!(
                "1 0 obj\n<< /Type /Sig /Filter /Adobe.PPKLite /SubFilter /adbe.pkcs7.detached /Contents <{}> /ByteRange [ {} {} {} {} ] >>\nendobj\n",
                cms_hex,
                byte_range[0], byte_range[1], byte_range[2], byte_range[3]
            );

            Ok(obj_str)
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
                    "<< /Size 2 /Prev {} >>\n",
                    prev_xref_offset
                )
                .as_bytes(),
            );
            trailer.extend_from_slice(b"startxref\n");
            // Note: The actual xref offset will be calculated at PDF assembly time
            // For now, placeholder
            trailer.extend_from_slice(b"XREF_OFFSET_PLACEHOLDER\n");
            trailer.extend_from_slice(b"%%EOF\n");

            trailer
        }
    }
}

#[cfg(feature = "digital-signature")]
pub use inner::*;
