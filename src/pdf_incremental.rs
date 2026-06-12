//! PDF incremental update section builder for digital signatures.
//! Handles ByteRange calculation, signature embedding, and xref reconstruction.

#[cfg(feature = "digital-signature")]
pub mod inner {
    use crate::Result;

    /// Build PDF incremental update section with embedded signature
    pub struct IncrementalUpdateBuilder {
        base_pdf: Vec<u8>,
        /// Reserved for the real field-update logic (currently object 1 is
        /// overwritten unconditionally; see `build_signature_field_update`).
        _field_name: String,
        cms_hex: String,
        signer_name: Option<String>,
    }

    impl IncrementalUpdateBuilder {
        /// Create a new incremental update builder
        pub fn new(
            base_pdf: Vec<u8>,
            field_name: String,
            cms_hex: String,
            signer_name: Option<String>,
        ) -> Self {
            IncrementalUpdateBuilder {
                base_pdf,
                _field_name: field_name,
                cms_hex,
                signer_name,
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

            // Build incremental update section with placeholders
            let mut update_section = Vec::new();

            // Object updates (simplified for v1.2.1)
            update_section.extend_from_slice(b"\n");
            let sig_field_offset = update_section.len();

            // Build the signature update object
            // For v1.2.1: Create a minimal signature field update
            let sig_field_update = self.build_signature_field_update(&self.cms_hex)?;
            update_section.extend_from_slice(sig_field_update.as_bytes());

            // Build xref table
            let mut xref_table = Vec::new();
            xref_table.extend_from_slice(b"xref\n");
            xref_table.extend_from_slice(b"1 1\n");
            let sig_obj_offset = (self.base_pdf.len() + sig_field_offset) as u32;
            xref_table.extend_from_slice(
                format!("{:010} 00000 n\n", sig_obj_offset).as_bytes()
            );

            // Calculate xref offset (where xref table starts in incremental section)
            let xref_offset = self.base_pdf.len() + sig_field_offset + sig_field_update.len();

            // Build trailer with actual xref offset
            let trailer = self.build_trailer(prev_xref_offset, xref_offset as u32);

            // Now we can calculate the final structure size and correct ByteRange
            // After final assembly: [base_pdf][newline][sig_obj][xref][trailer]
            let final_size = self.base_pdf.len() + update_section.len() + xref_table.len() + trailer.len();
            let hex_len = self.cms_hex.len() as u32;
            let length2 = final_size as u32 - (self.base_pdf.len() as u32 + hex_len);

            // Rebuild signature field update with correct ByteRange
            let sig_field_with_range = self.build_signature_field_update_with_range(
                &self.cms_hex,
                self.base_pdf.len() as u32,
                hex_len,
                length2,
            )?;

            // Rebuild update section with corrected signature object
            let mut update_section = Vec::new();
            update_section.extend_from_slice(b"\n");
            update_section.extend_from_slice(sig_field_with_range.as_bytes());
            update_section.extend_from_slice(&xref_table);
            update_section.extend_from_slice(&trailer);

            // Combine base PDF + incremental update
            let mut signed_pdf = self.base_pdf.clone();
            signed_pdf.extend_from_slice(&update_section);

            Ok(signed_pdf)
        }

        /// Find the previous xref offset in base PDF.
        ///
        /// Searches the last 1 KiB for the `startxref` marker and parses the
        /// decimal offset that follows it. Falls back to the PDF length when no
        /// marker is found.
        fn find_prev_xref_offset(&self) -> Result<u32> {
            let pdf = &self.base_pdf;
            let search_start = pdf.len().saturating_sub(1024);

            let marker_pos = pdf[search_start..]
                .windows(b"startxref".len())
                .position(|w| w == b"startxref")
                .map(|pos| search_start + pos);

            let Some(marker_pos) = marker_pos else {
                // No marker found: assume xref is at end.
                return Ok(pdf.len() as u32);
            };

            // Skip "startxref" and the whitespace immediately following it.
            let after_marker = &pdf[marker_pos + "startxref".len()..];
            let whitespace_len = after_marker
                .iter()
                .take_while(|b| matches!(b, b' ' | b'\n' | b'\r'))
                .count();

            // Read the contiguous digit run that follows.
            let digits = &after_marker[whitespace_len..];
            let digit_len = digits.iter().take_while(|b| b.is_ascii_digit()).count();

            let offset = std::str::from_utf8(&digits[..digit_len])
                .ok()
                .and_then(|s| s.parse::<u32>().ok())
                .unwrap_or(pdf.len() as u32);

            Ok(offset)
        }

        /// Build signature field update object (used internally for pre-calculation)
        fn build_signature_field_update(&self, cms_hex: &str) -> Result<String> {
            // Build with placeholder ByteRange for size calculation
            let obj_str = format!(
                "1 0 obj\n<< /Type /Sig /Filter /Adobe.PPKLite /SubFilter /adbe.pkcs7.detached /Contents <{}> /ByteRange [ 0 0 0 0 ] >>\nendobj\n",
                cms_hex
            );
            Ok(obj_str)
        }

        /// Build signature field update object with correct ByteRange values
        fn build_signature_field_update_with_range(
            &self,
            cms_hex: &str,
            length1: u32,
            hex_len: u32,
            length2: u32,
        ) -> Result<String> {
            // ByteRange per PDF spec ISO 32000-2:
            // [start1, length1, start2, length2]
            let start1 = 0;
            let start2 = length1 + hex_len;

            let mut obj_str = format!(
                "1 0 obj\n<< /Type /Sig /Filter /Adobe.PPKLite /SubFilter /adbe.pkcs7.detached /Contents <{}> /ByteRange [ {} {} {} {} ]",
                cms_hex,
                start1, length1, start2, length2
            );

            // Add signer name if available
            if let Some(name) = &self.signer_name {
                // PDF strings need proper escaping for special characters
                let escaped = name.replace('\\', "\\\\").replace('(', "\\(").replace(')', "\\)");
                obj_str.push_str(&format!(" /Name ({}) ", escaped));
            }

            // Add signing time in PDF date format (D:YYYYMMDDHHmmSS+HH'mm')
            // For v1.2.2: Use fixed value (in real implementation, use chrono or time crate)
            let datetime = "D:202406121200Z";

            obj_str.push_str(&format!(" /M ({}) ", datetime));
            obj_str.push_str(">>\nendobj\n");

            Ok(obj_str)
        }

        /// Build the trailer dictionary for incremental update
        /// Links to the previous xref section
        fn build_trailer(&self, prev_xref_offset: u32, xref_offset: u32) -> Vec<u8> {
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
            trailer.extend_from_slice(format!("{}\n", xref_offset).as_bytes());
            trailer.extend_from_slice(b"%%EOF\n");

            trailer
        }
    }
}

#[cfg(feature = "digital-signature")]
pub use inner::*;
