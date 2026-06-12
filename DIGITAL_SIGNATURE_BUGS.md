# Digital Signature Implementation: Critical Bugs & Correctness Issues

## Executive Summary

The digital signature implementation has **10 critical issues** that prevent it from functioning correctly. The signatures cannot be validated by PDF readers, the PDF structure is malformed, and the cryptographic operations are stubbed out. This would fail in any real-world usage.

---

## CRITICAL ISSUES

### BUG #1: startxref Offset is Literal String Instead of Number

**Severity:** CRITICAL - Makes PDF unreadable

**Location:** `src/pdf_incremental.rs:188`

**Current Code:**
```rust
trailer.extend_from_slice(b"startxref\n");
trailer.extend_from_slice(b"XREF_OFFSET_PLACEHOLDER\n");  // ← BUG!
trailer.extend_from_slice(b"%%EOF\n");
```

**Issue:** The trailer should contain a numeric byte offset, but this writes the literal string "XREF_OFFSET_PLACEHOLDER". Any PDF reader will fail to parse this.

**Expected:** Should calculate `xref_offset` (which IS computed on line 51 but never used):
```rust
let xref_offset = self.base_pdf.len() + update_section.len();
```

Then format it correctly:
```rust
trailer.extend_from_slice(format!("{}\n", xref_offset).as_bytes());
```

**Impact:** Signed PDFs cannot be opened in any standard PDF reader. Immediate parse failure.

**Reproduction:** 
1. Call `document.sign_document()` 
2. Try opening the output with Adobe Reader or Preview
3. Both will fail with parse error

---

### BUG #2: ByteRange Calculation Returns Wrong Final Value

**Severity:** CRITICAL - Signature validation will fail

**Location:** `src/signature_create.rs:230-248` and `src/pdf_incremental.rs:124`

**Current Code:**
```rust
pub fn calculate_byte_range(
    pre_contents_offset: u32,
    hex_string_length: u32,
    total_pdf_size: u32,
) -> Result<[u32; 4]> {
    let start2 = pre_contents_offset + hex_string_length;
    let length2 = total_pdf_size - start2;  // ← Calculated but not used!
    
    Ok([0, pre_contents_offset, hex_string_length, total_pdf_size])  // ← Wrong!
}
```

Also in pdf_incremental.rs:
```rust
let byte_range = [0, self.base_pdf.len() as u32, cms_hex.len() as u32, 
                  (self.base_pdf.len() + cms_hex.len() + 100) as u32];  // ← Why +100?!
```

**Issue:** The return value is `[0, X, Y, Z]` where:
- Element [3] should be the **remaining bytes AFTER the hex placeholder**
- But returns **total_pdf_size** instead

Per PDF spec ISO 32000-2, ByteRange is `[start1, length1, start2, length2]`:
- `start1=0` ✓
- `length1=pre_contents_offset` ✓
- `start2=pre_contents_offset + hex_string_length` ✓ (but returned as [2])
- `length2=remaining bytes after placeholder` ✗ (returns total_pdf_size instead)

**Expected:** Should return `[0, X, length(hex), remaining_bytes]`

**Impact:** Any PDF reader verifying the signature will recompute ByteRange and find it doesn't match. Signature validation will fail. The "+100" is an arbitrary magic number.

---

### BUG #3: Dummy Signature Generation - Returns Fixed Bytes

**Severity:** CRITICAL - Cryptographic bypass

**Location:** `src/signature_create.rs:218-222`

**Current Code:**
```rust
pub fn sign_hash(_private_key: &RsaPrivateKey, _hash: &[u8]) -> Result<Vec<u8>> {
    // v1.2.0: Placeholder. Signature generation will be fully implemented in v1.2.1
    // when PDF incremental update and PKCS#7/CMS are integrated.
    Ok(vec![0xDE, 0xAD, 0xBE, 0xEF])  // Dummy signature for testing
}
```

**Issue:** 
- Parameters `_private_key` and `_hash` are prefixed with `_` (unused)
- Returns fixed bytes `[0xDE, 0xAD, 0xBE, 0xEF]` (4 bytes) regardless of input
- Does NOT perform any RSA signing
- Does NOT use the hash
- Does NOT use the private key

**Impact:** 
1. All PDFs signed with this code have identical signature bytes `DEADBEEF`
2. No actual cryptographic proof of authorship
3. Any two PDFs signed with same cert have same signature (deterministic but wrong)
4. Cannot be cryptographically verified

---

### BUG #4: Malformed PKCS#7/CMS Structure

**Severity:** CRITICAL - Signature unverifiable

**Location:** `src/cms_builder.rs:30-68`

**Current Code:**
```rust
pub fn to_hex_string(&self) -> Result<String> {
    let mut der_bytes = Vec::new();
    der_bytes.push(0x30);  // SEQUENCE tag
    
    let mut content = Vec::new();
    
    // Add hash (OCTET STRING)
    content.push(0x04);
    content.push(self.hash_bytes.len() as u8);  // ← BUG!
    content.extend_from_slice(&self.hash_bytes);
    // ... more fields
}
```

**Issues:**

1. **DER Length Encoding Bug (line 44):** 
   ```rust
   content.push(self.hash_bytes.len() as u8);
   ```
   This only works for lengths < 128 bytes. For longer data, DER requires multi-byte length encoding (e.g., `0x82` for 2-byte length). This code doesn't handle that.

2. **Incorrect Structure:** The code just wraps three OCTET STRINGs in a SEQUENCE. This is not PKCS#7 SignedData format. Real PKCS#7 requires:
   - ContentInfo wrapper
   - SignedData structure with proper OIDs
   - DigestAlgorithmIdentifier (SHA-256 OID, etc.)
   - SignerInfo with DigestAlgorithm, DigestEncryption, Attributes
   - SignatureAlgorithm OID

3. **Missing Critical Fields:**
   - No OID for digest algorithm (SHA-256 = 2.16.840.1.101.3.4.2.1)
   - No OID for RSA encryption (1.2.840.113549.1.1.1)
   - No authenticatedAttributes (required for PDF signatures)
   - No timestamp token

**Impact:** Any cryptographic validator will reject this as invalid PKCS#7. Adobe Reader cannot parse it.

---

### BUG #5: Verification Always Returns "Valid" Regardless of Signature

**Severity:** CRITICAL - Security bypass

**Location:** `src/signature.rs:166-167`

**Current Code:**
```rust
// v1.2.2: Mark as valid if signature structure is present
// TODO v1.2.3: Implement cryptographic validation (RSA signature check)
let is_valid = true;  // ← ALWAYS TRUE
```

Even worse, the extracted values are never used:
```rust
let _sig_hex = match sig_dict.get(b"Contents") { ... };
let _byte_range = match sig_dict.get(b"ByteRange") { ... };
// Both prefixed with _ (unused)

// Later:
let is_valid = true;  // Never actually validates!
```

**Issue:** The method extracts signature contents and ByteRange but:
1. Never validates the hash
2. Never verifies the RSA signature
3. Never checks certificate
4. Returns `is_valid = true` unconditionally

**Impact:** 
- `document.verify_signatures()` returns `is_valid: true` for ANY signature
- Even tampered PDFs will report valid signatures
- False sense of security

---

### BUG #6: xref Table Not Properly Generated

**Severity:** CRITICAL - PDF structure corruption

**Location:** `src/pdf_incremental.rs:50-68`

**Current Code:**
```rust
let xref_offset = self.base_pdf.len() + update_section.len();  // Calculated
let mut xref_table = Vec::new();
xref_table.extend_from_slice(b"xref\n");
xref_table.extend_from_slice(b"0 1\n");
xref_table.extend_from_slice(format!("{:010} {:05} f\n", 0, 65535).as_bytes());
// Result: "0000000000 65535 f" - the free list head marker

update_section.extend_from_slice(&xref_table);
let trailer = self.build_trailer(prev_xref_offset);  // Still uses OLD offset
```

**Issues:**

1. **Wrong Object Numbers:** The xref table should list the **updated objects** in this incremental section. The signature object was created by `add_signature_field()` with some object ID (e.g., 3), but the xref table doesn't reference it.

2. **Calculated but Unused:** `xref_offset` is computed but never used. It should be passed to `build_trailer()`.

3. **Incorrect Entry Format:** For an updated object 3, should be:
   ```
   xref
   3 1
   0000001234 00000 n
   ```
   (where 1234 is the byte offset of object 3 in the incremental section)

4. **Hardcoded Entry:** The "0 1" assumes only the free list. There should be an entry for each modified object.

**Impact:** PDF readers cannot locate the signature object in the incremental update. Links are broken.

---

### BUG #7: Signature Object ID Hardcoded as "1 0 obj"

**Severity:** CRITICAL - Signature not linked to form field

**Location:** `src/pdf_incremental.rs:129`

**Current Code:**
```rust
let obj_str = format!(
    "1 0 obj\n<< /Type /Sig ... >>\nendobj\n",
    cms_hex, byte_range[0], byte_range[1], byte_range[2], byte_range[3]
);
```

**Issue:**
- The signature dictionary is hardcoded as object "1 0 obj"
- But `add_signature_field()` created a signature object with a different ID
- The incremental update creates a NEW "1 0 obj" which may conflict

For example:
1. `add_signature_field()` → creates signature dict as object 3
2. `sign_document()` → tries to add signature as object 1
3. Result: either object 1 is now the signature (object 3 orphaned) OR collision

**Expected:** Need to:
1. Find the actual signature object ID from the form field
2. Update that object in the incremental section
3. Ensure xref table references the correct object

**Impact:** Signature field in the form is not linked to the actual signature.

---

### BUG #8: Hash Calculated on Entire PDF Instead of ByteRange

**Severity:** HIGH - Hash verification will fail

**Location:** `src/signature_create.rs:209-214`

**Current Code:**
```rust
pub fn hash_pdf_content(content: &[u8]) -> Vec<u8> {
    let mut hasher = Sha256::new();
    hasher.update(content);  // Hashes ENTIRE PDF
    hasher.finalize().to_vec()
}
```

**Issue:** Per PDF spec ISO 32000-2, the signature hash should be calculated over the ByteRange areas only:
- Hash bytes [0, X)
- Hash bytes [Y, EOF)
- Skip the /Contents placeholder itself

Current code hashes the entire base_pdf.

**Expected:**
```rust
pub fn hash_pdf_content_with_byte_range(content: &[u8], byte_range: [u32; 4]) -> Vec<u8> {
    let mut hasher = Sha256::new();
    // Hash [0, byte_range[0] + byte_range[1])
    hasher.update(&content[0..(byte_range[0] + byte_range[1]) as usize]);
    // Hash [byte_range[2], EOF)
    hasher.update(&content[byte_range[2] as usize..]);
    hasher.finalize().to_vec()
}
```

**Impact:** Hash doesn't match what external tools compute. Signatures fail verification.

---

### BUG #9: Signature Dictionary Missing Metadata Fields

**Severity:** MEDIUM - Incomplete signature metadata

**Location:** `src/pdf_incremental.rs:127-132`

**Current Code:**
```rust
let obj_str = format!(
    "1 0 obj\n<< /Type /Sig /Filter /Adobe.PPKLite /SubFilter /adbe.pkcs7.detached /Contents <{}> /ByteRange [ {} {} {} {} ] >>\nendobj\n",
    cms_hex, byte_range[0], byte_range[1], byte_range[2], byte_range[3]
);
```

**Missing Per PDF Spec:**
- `/M` (signing time) - From certificate or SignatureFieldOptions
- `/Reason` - From SignatureFieldOptions (passed to add_signature_field but never used)
- `/ContactInfo` - From SignatureFieldOptions (never used)
- `/Name` - Signer name from certificate CN (available but not included)
- `/Location` - Could be added

**Issue:** The signature metadata was collected in `add_signature_field()` but completely ignored during signing.

**Impact:** Adobe Reader cannot display signature reason/time. Metadata is lost.

---

### BUG #10: Incremental Update Section Not Properly Computed

**Severity:** HIGH - Offset calculation errors

**Location:** `src/pdf_incremental.rs:34-68`

**Issues:**

1. **xref_offset Calculated But Not Used:**
   ```rust
   let xref_offset = self.base_pdf.len() + update_section.len();  // ← Line 51
   let trailer = self.build_trailer(prev_xref_offset);  // ← Ignores xref_offset!
   ```
   Should pass the NEW offset to `build_trailer()`.

2. **Trailer Has Placeholder Instead of Actual Value:**
   As noted in BUG #1, the trailer uses "XREF_OFFSET_PLACEHOLDER" instead of xref_offset.

3. **No Actual Byte Offset Tracking:**
   When building the signature field object, no tracking of where it ends:
   ```rust
   let sig_field_update = self.build_signature_field_update(&self.cms_hex)?;
   // No byte count of sig_field_update!
   update_section.extend_from_slice(sig_field_update.as_bytes());
   // Now xref_offset calculation is off!
   ```

**Impact:** xref offset points to wrong byte position, breaking PDF structure.

---

## EDGE CASES NOT HANDLED

### 1. Empty PDFs or PDFs with No AcroForm
- `add_signature_field()` calls `ensure_acroform()` (creates AcroForm)
- But `sign_document()` doesn't verify the AcroForm was created
- Incremental update assumes form field exists

### 2. Multiple Signatures
- Hardcoded "1 0 obj" means only one signature possible
- Second signature would conflict with first
- No increment of object ID per signature

### 3. Very Large PDFs (>4GB)
- Using `u32` for offsets (max 4GB)
- PDF spec allows larger files
- Should use `u64`

### 4. Special Characters in Certificate CN
- `extract_subject_cn_from_der()` is naive DER parsing
- Doesn't handle:
  - Extended ASCII/UTF-8 properly
  - Long length encodings (>255 bytes)
  - Complex certificate structures

### 5. Existing Signatures in PDF
- If PDF already has signature, adding another would corrupt structure
- No validation that this is the first signature

---

## SUMMARY OF BUGS

| Bug | Severity | Category | Impact |
|-----|----------|----------|--------|
| #1: startxref placeholder | CRITICAL | Structure | PDFs unreadable |
| #2: ByteRange calculation | CRITICAL | Validation | Signatures fail verification |
| #3: Dummy signature bytes | CRITICAL | Crypto | No actual signing |
| #4: Malformed PKCS#7 | CRITICAL | Structure | Unverifiable signature |
| #5: Verify always returns true | CRITICAL | Security | False validation |
| #6: xref table wrong | CRITICAL | Structure | Incremental update broken |
| #7: Object ID hardcoded | CRITICAL | Linking | Signature orphaned |
| #8: Hash entire PDF | HIGH | Validation | Hash mismatch |
| #9: Missing metadata | MEDIUM | Metadata | Incomplete signature info |
| #10: Offset calculations | HIGH | Structure | Wrong byte positions |

---

## RECOMMENDED FIXES

### Priority 1 (Immediate - Makes PDFs readable):
1. Fix startxref placeholder → use computed xref_offset
2. Find and update actual signature object ID (don't hardcode "1 0 obj")
3. Fix ByteRange calculation → return correct remaining bytes

### Priority 2 (Makes signatures valid):
4. Implement actual RSA signing (use rsa crate PKCS#1 v1.5)
5. Implement proper PKCS#7/CMS structure using cms crate
6. Calculate hash over ByteRange only, not entire PDF
7. Generate correct xref table with actual object offsets

### Priority 3 (Makes verification work):
8. Implement cryptographic verification in verify_signatures()
9. Add metadata fields to signature dictionary
10. Handle multiple signatures, use u64 for offsets

### Testing:
- Add tests that validate signed PDFs in Adobe Reader
- Test hash matching with external PDF tools
- Test with existing signed PDFs from Adobe
- Test edge cases (empty PDFs, large PDFs, special chars)
