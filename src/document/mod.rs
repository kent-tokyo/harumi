mod helpers;
mod page;
mod types;

pub use helpers::{calculate_text_width, font_covers_char, glyph_advance_pt, wrap_paragraph};
pub use page::{PageHandle, VerticalAlign};
pub use types::{
    AttachmentInfo, BatchEntry, BoxFitOptions, Color, Document, FieldType, FitOptions, FitResult,
    FormField, FragmentReplaceFailureReason, FragmentReplaceOpts, OverflowPolicy, PdfMetadata,
    PlacementStatus, ReplaceOptions, TextFieldOptions, TextRun,
};

#[cfg(feature = "draw")]
pub use page::DebugOverlayOptions;

use std::{
    collections::{BTreeMap, HashMap, HashSet},
    path::Path,
};

use lopdf::{Dictionary, Object, ObjectId, Stream, StringFormat};
use ttf_parser::Face;

use crate::{
    content::text::text_stream,
    error::{Error, Result},
    font::{
        EmbeddedFont, FontHandle,
        embed::{EmbedParams, embed_cid_font},
        subset::subset_font,
    },
};

use helpers::{
    acroform_id, add_font_to_resources, add_font_to_xobject_resources, add_xobject_to_resources,
    append_annotation_to_page, append_to_contents, check_finite, collect_field_ids,
    collect_fields_recursive, ensure_acroform, generate_file_id, inherited_media_box_raw,
    inherited_resources, lopdf_string_to_rust, map_lopdf_password_err, pdf_text_string,
    realize_page_inherited_attrs, root_pages_id, wrap_page_contents_in_q_q,
};
use types::{EncAlgorithm, PendingBookmark, PendingOp, RawFont};

#[cfg(feature = "draw")]
use helpers::add_ext_gstate_to_resources;

impl Document {
    /// Loads a PDF from a file path.
    ///
    /// # Errors
    /// Returns [`Error::Io`] if the file cannot be read, or [`Error::Pdf`] if
    /// the file is not a valid PDF.
    fn from_inner(inner: lopdf::Document) -> Self {
        Self {
            inner,
            raw_fonts: Vec::new(),
            pending: Vec::new(),
            pending_bookmarks: Vec::new(),
            finalized: false,
            pending_encryption: None,
        }
    }

    pub fn from_file(path: impl AsRef<Path>) -> Result<Self> {
        Ok(Self::from_inner(lopdf::Document::load(path)?))
    }

    /// Loads a PDF from an in-memory byte slice.
    ///
    /// # Errors
    /// Returns [`Error::Pdf`] if the bytes do not represent a valid PDF.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        Ok(Self::from_inner(lopdf::Document::load_from(bytes)?))
    }

    /// Loads a password-protected PDF from a file path.
    ///
    /// The document is decrypted during loading. Both owner and user passwords are accepted.
    ///
    /// # Errors
    /// Returns [`Error::WrongPassword`] if the password is incorrect, or [`Error::Pdf`] /
    /// [`Error::Io`] for other failures.
    pub fn from_file_with_password(path: impl AsRef<Path>, password: &str) -> Result<Self> {
        let inner =
            lopdf::Document::load_with_password(path, password).map_err(map_lopdf_password_err)?;
        Ok(Self::from_inner(inner))
    }

    /// Loads a password-protected PDF from an in-memory byte slice.
    ///
    /// The document is decrypted during loading. Both owner and user passwords are accepted.
    ///
    /// # Errors
    /// Returns [`Error::WrongPassword`] if the password is incorrect, or [`Error::Pdf`] for
    /// other failures.
    pub fn from_bytes_with_password(bytes: &[u8], password: &str) -> Result<Self> {
        let inner = lopdf::Document::load_from_with_password(bytes, password)
            .map_err(map_lopdf_password_err)?;
        Ok(Self::from_inner(inner))
    }

    /// Configures the document to be saved with password protection.
    ///
    /// Encryption is applied at [`save`](Document::save) time using 128-bit RC4
    /// (PDF standard revision 3, compatible with all PDF readers).
    ///
    /// Pass an empty `user_password` to allow anyone to open the file while still
    /// restricting editing with the owner password.
    ///
    /// # Errors
    /// Returns [`Error::InvalidInput`] if called after [`save`](Document::save).
    pub fn set_encryption(&mut self, user_password: &str, owner_password: &str) -> Result<()> {
        if self.finalized {
            return Err(Error::InvalidInput(
                "set_encryption() called after save(); create a new Document".into(),
            ));
        }
        self.pending_encryption = Some((
            user_password.to_owned(),
            owner_password.to_owned(),
            EncAlgorithm::Rc4_128,
        ));
        Ok(())
    }

    /// Configures the document to be saved with AES-256 password protection (PDF 2.0).
    ///
    /// AES-256-CBC encryption is applied at [`save`](Document::save) time.
    /// A fresh 32-byte cryptographically secure random key is generated for every `save()` call
    /// via the OS-level RNG (`getrandom`); the key is never derived from the password alone.
    ///
    /// Pass an empty `user_password` to allow anyone to open the file while still
    /// restricting editing with the owner password.
    ///
    /// # Compatibility
    /// AES-256 (V5/R6) requires PDF 2.0 and is supported by Acrobat X+, Chrome, Firefox,
    /// and most modern PDF readers. Use [`set_encryption`](Document::set_encryption) (RC4-128)
    /// for maximum backwards compatibility with older viewers.
    ///
    /// # Errors
    /// Returns [`Error::InvalidInput`] if called after [`save`](Document::save).
    /// Returns [`Error::InvalidInput`] if the OS RNG is unavailable at save time.
    pub fn set_encryption_aes256(
        &mut self,
        user_password: &str,
        owner_password: &str,
    ) -> Result<()> {
        if self.finalized {
            return Err(Error::InvalidInput(
                "set_encryption_aes256() called after save(); create a new Document".into(),
            ));
        }
        self.pending_encryption = Some((
            user_password.to_owned(),
            owner_password.to_owned(),
            EncAlgorithm::Aes256,
        ));
        Ok(())
    }

    /// Returns `true` if the PDF was encrypted when it was loaded.
    ///
    /// A document opened with [`from_file_with_password`](Document::from_file_with_password)
    /// is decrypted in memory and this method still returns `true`.
    /// A document opened with [`from_file`](Document::from_file) that required no password
    /// returns `false`.
    pub fn is_encrypted(&self) -> bool {
        self.inner.was_encrypted()
    }

    /// Creates a new single-page blank PDF document.
    ///
    /// `size` is `(width, height)` in PDF points (1 pt = 1/72 inch).
    /// A4 = (595.0, 842.0), US Letter = (612.0, 792.0).
    ///
    /// To add more pages, use [`insert_blank_page`](Document::insert_blank_page).
    ///
    /// # Errors
    /// Returns [`Error::InvalidInput`] if size contains NaN/Inf or non-positive values.
    pub fn new(size: (f32, f32)) -> Result<Self> {
        check_finite(&[size.0, size.1], "Document::new")?;
        if size.0 <= 0.0 || size.1 <= 0.0 {
            return Err(Error::InvalidInput(format!(
                "page size must be positive, got ({}, {})",
                size.0, size.1
            )));
        }

        let mut inner = lopdf::Document::with_version("1.4");

        let pages_id = inner.new_object_id();
        let page_id = inner.new_object_id();

        inner.objects.insert(
            page_id,
            Object::Dictionary({
                let mut d = Dictionary::new();
                d.set("Type", Object::Name(b"Page".to_vec()));
                d.set("Parent", Object::Reference(pages_id));
                d.set(
                    "MediaBox",
                    Object::Array(vec![
                        Object::Integer(0),
                        Object::Integer(0),
                        Object::Real(size.0),
                        Object::Real(size.1),
                    ]),
                );
                d.set("Resources", Object::Dictionary(Dictionary::new()));
                d
            }),
        );

        inner.objects.insert(
            pages_id,
            Object::Dictionary({
                let mut d = Dictionary::new();
                d.set("Type", Object::Name(b"Pages".to_vec()));
                d.set("Kids", Object::Array(vec![Object::Reference(page_id)]));
                d.set("Count", Object::Integer(1));
                d
            }),
        );

        let catalog_id = inner.add_object(Object::Dictionary({
            let mut d = Dictionary::new();
            d.set("Type", Object::Name(b"Catalog".to_vec()));
            d.set("Pages", Object::Reference(pages_id));
            d
        }));

        inner.trailer.set("Root", Object::Reference(catalog_id));

        Ok(Self::from_inner(inner))
    }

    /// Returns the number of pages in the document.
    ///
    /// # Example
    /// ```no_run
    /// # use harumi::Document;
    /// let doc = Document::from_file("input.pdf")?;
    /// println!("{} pages", doc.page_count());
    /// # Ok::<(), harumi::Error>(())
    /// ```
    pub fn page_count(&self) -> u32 {
        self.inner.get_pages().len() as u32
    }

    /// Registers a TrueType font for later embedding.
    ///
    /// This call is cheap — it only stores `ttf_bytes` in memory. The actual
    /// subsetting and PDF embedding happen during [`save`](Document::save).
    ///
    /// The returned [`FontHandle`] can be reused across any number of pages and
    /// text runs within this document.
    ///
    /// Calling `embed_font` twice with identical bytes creates two independent
    /// font handles, each embedded separately at `save()` time. This is correct
    /// when the handles are used for different purposes but wasteful if they are
    /// identical — reuse the same handle instead.
    ///
    /// # Errors
    /// Returns [`Error::UnsupportedFontKind`] if the font is an OpenType/CFF
    /// file (`OTTO` magic bytes); those are not yet supported.
    pub fn embed_font(&mut self, ttf_bytes: &[u8]) -> Result<FontHandle> {
        // Validate the font early so callers get an actionable error at registration time.
        let face = Face::parse(ttf_bytes, 0).map_err(|e| Error::FontParse(e.to_string()))?;
        if face.units_per_em() == 0 {
            return Err(Error::FontParse("font units_per_em is 0".into()));
        }
        let idx = self.raw_fonts.len() as u32;
        self.raw_fonts.push(RawFont {
            ttf_bytes: ttf_bytes.to_vec(),
        });
        Ok(FontHandle(idx))
    }

    /// Returns a handle to a page for queuing text overlays (1-indexed).
    ///
    /// # Errors
    /// Returns [`Error::PageNotFound`] if `number` is greater than
    /// [`page_count`](Document::page_count) or zero.
    pub fn page(&mut self, number: u32) -> Result<PageHandle<'_>> {
        if self.finalized {
            return Err(Error::InvalidInput(
                "cannot add content after save(); create a new Document".into(),
            ));
        }
        let page_ids = self.inner.get_pages();
        let page_id = page_ids
            .get(&number)
            .copied()
            .ok_or(Error::PageNotFound(number))?;
        Ok(PageHandle { doc: self, page_id })
    }

    /// Rotates page `number` by adding `degrees` to its current `/Rotate` value.
    ///
    /// `degrees` must be a multiple of 90. Negative values rotate counter-clockwise.
    /// The result is normalized to 0, 90, 180, or 270. Calling this method twice
    /// accumulates the rotation (e.g. two calls with 90 results in 180).
    ///
    /// # Example
    /// ```no_run
    /// # use harumi::Document;
    /// # fn main() -> harumi::Result<()> {
    /// let mut doc = Document::from_file("scan.pdf")?;
    /// doc.rotate_page(1, 90)?;   // rotate page 1 clockwise by 90°
    /// doc.save("rotated.pdf")?;
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// # Errors
    /// Returns [`Error::PageNotFound`] if `number` is out of range, or
    /// [`Error::InvalidInput`] if `degrees` is not a multiple of 90 or [`save`](Document::save)
    /// has already been called.
    pub fn rotate_page(&mut self, number: u32, degrees: i32) -> Result<()> {
        if self.finalized {
            return Err(Error::InvalidInput(
                "page manipulation after save() is not supported; create a new Document".into(),
            ));
        }
        if degrees % 90 != 0 {
            return Err(Error::InvalidInput(format!(
                "degrees must be a multiple of 90, got {degrees}"
            )));
        }
        let page_ids = self.inner.get_pages();
        let page_id = page_ids
            .get(&number)
            .copied()
            .ok_or(Error::PageNotFound(number))?;
        let page_dict = self.inner.get_object_mut(page_id)?.as_dict_mut()?;
        // Accept both Integer and Real /Rotate (some PDF generators emit 270.0).
        let current: i64 = match page_dict.get(b"Rotate").ok() {
            Some(Object::Integer(n)) => *n,
            Some(Object::Real(n)) => *n as i64,
            _ => 0,
        };
        // Use i64 throughout to avoid i32 overflow on crafted inputs.
        let new_rotate = ((current + degrees as i64) % 360 + 360) % 360;
        page_dict.set("Rotate", Object::Integer(new_rotate));
        Ok(())
    }

    /// Removes page `number` from the document (1-indexed).
    ///
    /// Pages after the removed page are renumbered. Cannot remove the last
    /// remaining page.
    ///
    /// # Example
    /// ```no_run
    /// # use harumi::Document;
    /// # fn main() -> harumi::Result<()> {
    /// let mut doc = Document::from_file("input.pdf")?;
    /// doc.remove_page(2)?;  // remove the second page
    /// doc.save("output.pdf")?;
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// # Errors
    /// Returns [`Error::PageNotFound`] if `number` is out of range, or
    /// [`Error::InvalidInput`] if the document has only one page or [`save`](Document::save)
    /// has already been called.
    pub fn remove_page(&mut self, number: u32) -> Result<()> {
        if self.finalized {
            return Err(Error::InvalidInput(
                "page manipulation after save() is not supported; create a new Document".into(),
            ));
        }
        let page_ids = self.inner.get_pages();
        let target_id = page_ids
            .get(&number)
            .copied()
            .ok_or(Error::PageNotFound(number))?;
        if page_ids.len() <= 1 {
            return Err(Error::InvalidInput("cannot remove the only page".into()));
        }

        let all_ids: Vec<ObjectId> = page_ids
            .into_values()
            .filter(|&id| id != target_id)
            .collect();

        // Materialize inherited attributes (MediaBox, CropBox, etc.) from any
        // intermediate /Pages nodes before those nodes leave the ancestry chain.
        for &pid in &all_ids {
            realize_page_inherited_attrs(&mut self.inner, pid)?;
        }

        let pages_root = root_pages_id(&self.inner)?;
        for &pid in &all_ids {
            let d = self.inner.get_object_mut(pid)?.as_dict_mut()?;
            d.set("Parent", Object::Reference(pages_root));
        }
        let count = all_ids.len();
        let new_kids: Vec<Object> = all_ids.into_iter().map(Object::Reference).collect();
        let pages_dict = self.inner.get_object_mut(pages_root)?.as_dict_mut()?;
        pages_dict.set("Kids", Object::Array(new_kids));
        pages_dict.set("Count", Object::Integer(count as i64));

        // Remove any pending ops queued for the removed page (correctness: avoids
        // save() attempting to write to an object that no longer exists in the tree).
        self.pending.retain(|p| p.page_id != target_id);
        // Remove the page dict object itself to free space (safe: reference already
        // removed from /Kids above; shared stream objects are intentionally left).
        self.inner.objects.remove(&target_id);

        Ok(())
    }

    /// Inserts a blank page into the document (1-indexed).
    ///
    /// `after = 0` prepends the new page before all existing pages.
    /// `after = page_count()` appends the new page after all existing pages.
    /// `size` is `(width, height)` in PDF points (e.g. `(595.0, 842.0)` for A4).
    ///
    /// # Example
    /// ```no_run
    /// # use harumi::Document;
    /// # fn main() -> harumi::Result<()> {
    /// let mut doc = Document::from_file("input.pdf")?;
    /// doc.insert_blank_page(0, (595.0, 842.0))?;   // prepend blank A4 page
    /// doc.insert_blank_page(doc.page_count(), (612.0, 792.0))?;  // append blank Letter page
    /// doc.save("output.pdf")?;
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// # Errors
    /// Returns [`Error::InvalidInput`] if `after > page_count()` or [`save`](Document::save)
    /// has already been called.
    pub fn insert_blank_page(&mut self, after: u32, size: (f32, f32)) -> Result<()> {
        if self.finalized {
            return Err(Error::InvalidInput(
                "page manipulation after save() is not supported; create a new Document".into(),
            ));
        }
        check_finite(&[size.0, size.1], "insert_blank_page")?;
        if size.0 <= 0.0 || size.1 <= 0.0 {
            return Err(Error::InvalidInput(format!(
                "page size must be positive, got ({}, {})",
                size.0, size.1
            )));
        }
        let count = self.page_count();
        if after > count {
            return Err(Error::InvalidInput(format!(
                "after={after} exceeds page_count={count}"
            )));
        }

        let pages_root = root_pages_id(&self.inner)?;

        let new_page_id = {
            let mut d = Dictionary::new();
            d.set("Type", Object::Name(b"Page".to_vec()));
            d.set("Parent", Object::Reference(pages_root));
            d.set(
                "MediaBox",
                Object::Array(vec![
                    Object::Integer(0),
                    Object::Integer(0),
                    Object::Real(size.0),
                    Object::Real(size.1),
                ]),
            );
            d.set("Resources", Object::Dictionary(Dictionary::new()));
            self.inner.add_object(Object::Dictionary(d))
        };

        let mut all_ids: Vec<ObjectId> = self.inner.get_pages().into_values().collect();

        // Materialize inherited attributes for existing pages before re-parenting.
        for &pid in &all_ids {
            realize_page_inherited_attrs(&mut self.inner, pid)?;
        }

        all_ids.insert(after as usize, new_page_id);

        for &pid in &all_ids {
            let d = self.inner.get_object_mut(pid)?.as_dict_mut()?;
            d.set("Parent", Object::Reference(pages_root));
        }
        let new_count = all_ids.len();
        let new_kids: Vec<Object> = all_ids.into_iter().map(Object::Reference).collect();
        let pages_dict = self.inner.get_object_mut(pages_root)?.as_dict_mut()?;
        pages_dict.set("Kids", Object::Array(new_kids));
        pages_dict.set("Count", Object::Integer(new_count as i64));
        Ok(())
    }

    /// Reorders the pages of the document.
    ///
    /// `new_order[i]` is the **1-indexed** old page number that should become new page `i + 1`.
    /// Every old page number must appear exactly once.
    ///
    /// # Example
    /// ```no_run
    /// # use harumi::Document;
    /// # fn main() -> harumi::Result<()> {
    /// let mut doc = Document::from_file("input.pdf")?;
    /// // Reverse a 3-page document: new p1=old p3, new p2=old p2, new p3=old p1
    /// doc.reorder_pages(&[3, 2, 1])?;
    /// doc.save("output.pdf")?;
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// # Errors
    /// Returns [`Error::InvalidInput`] if `new_order.len() != page_count()` or contains
    /// duplicates, [`Error::PageNotFound`] if any entry is 0 or out of range, or
    /// [`Error::InvalidInput`] if [`save`](Document::save) has already been called.
    pub fn reorder_pages(&mut self, new_order: &[u32]) -> Result<()> {
        if self.finalized {
            return Err(Error::InvalidInput(
                "page manipulation after save() is not supported; create a new Document".into(),
            ));
        }
        let count = self.page_count();
        if new_order.len() != count as usize {
            return Err(Error::InvalidInput(format!(
                "new_order has {} entries but document has {} pages",
                new_order.len(),
                count,
            )));
        }

        let mut seen = vec![false; (count + 1) as usize];
        for &n in new_order {
            if n == 0 || n > count {
                return Err(Error::PageNotFound(n));
            }
            if seen[n as usize] {
                return Err(Error::InvalidInput(format!(
                    "duplicate page number {n} in new_order"
                )));
            }
            seen[n as usize] = true;
        }

        let page_ids = self.inner.get_pages();
        let ordered_ids: Vec<ObjectId> = new_order
            .iter()
            .map(|&n| page_ids.get(&n).copied().ok_or(Error::PageNotFound(n)))
            .collect::<Result<Vec<_>>>()?;

        // Materialize inherited attributes before re-parenting.
        for &pid in &ordered_ids {
            realize_page_inherited_attrs(&mut self.inner, pid)?;
        }

        let pages_root = root_pages_id(&self.inner)?;
        for &pid in &ordered_ids {
            let d = self.inner.get_object_mut(pid)?.as_dict_mut()?;
            d.set("Parent", Object::Reference(pages_root));
        }
        let new_kids: Vec<Object> = ordered_ids.into_iter().map(Object::Reference).collect();
        let pages_dict = self.inner.get_object_mut(pages_root)?.as_dict_mut()?;
        pages_dict.set("Kids", Object::Array(new_kids));
        Ok(())
    }

    /// Appends all pages from `other` to the end of this document.
    ///
    /// `other` must have no pending (unflushed) operations — use a freshly loaded
    /// document (`from_file` / `from_bytes`) or reload after `save_to_bytes()`.
    ///
    /// # What is preserved
    /// All page content, embedded fonts, images, and resources from `other`.
    ///
    /// # What is NOT preserved
    /// `other`'s Outlines/Bookmarks, AcroForm, and `/Info` metadata (author,
    /// creation date, etc.). `other`'s catalog and root Pages object become
    /// unreferenced objects in the merged file.
    ///
    /// # Example
    /// ```no_run
    /// # use harumi::Document;
    /// # fn main() -> harumi::Result<()> {
    /// let mut base = Document::from_file("a.pdf")?;
    /// let appendix = Document::from_file("b.pdf")?;
    /// base.merge_from(appendix)?;
    /// base.save("merged.pdf")?;
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// # Errors
    /// Returns [`Error::InvalidInput`] if this document has been finalized via
    /// [`save`](Document::save) or if `other` has unflushed pending operations.
    pub fn merge_from(&mut self, other: Document) -> Result<()> {
        if self.finalized {
            return Err(Error::InvalidInput(
                "merge_from after save() is not supported; create a new Document".into(),
            ));
        }
        if !other.pending.is_empty() {
            return Err(Error::InvalidInput(
                "other has unflushed pending operations; call save_to_bytes()+from_bytes() first"
                    .into(),
            ));
        }

        // Renumber other's object IDs so they don't collide with self's.
        let mut other_inner = other.inner;
        other_inner.renumber_objects_with(self.inner.max_id + 1);

        // Collect other's page IDs (already renumbered) before we consume other_inner.
        let other_page_ids: Vec<ObjectId> = other_inner.get_pages().into_values().collect();

        // Merge all of other's objects into self.
        // other's /Catalog and /Pages root become orphan objects — acceptable.
        self.inner.objects.extend(other_inner.objects);
        self.inner.max_id = other_inner.max_id;

        // Get self's pages root and current ordered page list.
        let pages_root = root_pages_id(&self.inner)?;
        let self_page_ids: Vec<ObjectId> = self.inner.get_pages().into_values().collect();

        // Re-parent each of other's pages to self's /Pages root.
        for &pid in &other_page_ids {
            let d = self.inner.get_object_mut(pid)?.as_dict_mut()?;
            d.set("Parent", Object::Reference(pages_root));
        }

        // Rebuild /Kids with self's pages followed by other's pages, update /Count.
        let combined: Vec<ObjectId> = self_page_ids.into_iter().chain(other_page_ids).collect();
        let count = combined.len();
        let new_kids: Vec<Object> = combined.into_iter().map(Object::Reference).collect();
        let pages_dict = self.inner.get_object_mut(pages_root)?.as_dict_mut()?;
        pages_dict.set("Kids", Object::Array(new_kids));
        pages_dict.set("Count", Object::Integer(count as i64));

        Ok(())
    }

    /// Overlays each page of `other` on top of the corresponding page of `self`.
    ///
    /// Each page of `other` is embedded as a PDF Form XObject and rendered on top
    /// of the matching page in `self` via the `Do` operator. Pages in `self` with
    /// no corresponding `other` page are left untouched.
    ///
    /// The overlay is appended at the end of each base page's content stream, so it
    /// renders on top of existing content. The base page's coordinate system is
    /// preserved; if the base page uses an unbalanced `cm` without `q/Q`, the overlay
    /// position may be affected.
    ///
    /// # What is preserved from `other`
    /// Page content, embedded fonts, images, ExtGState (opacity), and other resources.
    ///
    /// # What is NOT preserved from `other`
    /// Outlines/Bookmarks, AcroForm, /Names, /PageLabels, /OpenAction, /StructTreeRoot.
    ///
    /// # Errors
    /// Returns [`Error::InvalidInput`] if called after [`save`](Document::save) or if
    /// `other` has unflushed pending operations (call `save_to_bytes()+from_bytes()` first).
    pub fn overlay_from(&mut self, other: Document) -> Result<()> {
        if self.finalized {
            return Err(Error::InvalidInput(
                "overlay_from after save() is not supported; create a new Document".into(),
            ));
        }
        if !other.pending.is_empty() {
            return Err(Error::InvalidInput(
                "other has unflushed pending operations; call save_to_bytes()+from_bytes() first"
                    .into(),
            ));
        }

        // Renumber other's object IDs to avoid collision with self.
        let mut other_inner = other.inner;
        other_inner.renumber_objects_with(self.inner.max_id + 1);

        // Collect ordered page IDs from both documents.
        let other_pages: Vec<ObjectId> = other_inner.get_pages().into_values().collect();
        let self_pages: Vec<ObjectId> = self.inner.get_pages().into_values().collect();

        // Merge all of other's objects into self (other's /Catalog and /Pages become orphans).
        self.inner.objects.extend(other_inner.objects);
        self.inner.max_id = other_inner.max_id;

        let page_count = self_pages.len().min(other_pages.len());
        for i in 0..page_count {
            let self_page_id = self_pages[i];
            let other_page_id = other_pages[i];

            // Get the other page's MediaBox in [x1, y1, x2, y2] for the Form XObject BBox.
            let bbox = inherited_media_box_raw(&self.inner, other_page_id);

            // Get the other page's /Resources (with parent-chain inheritance).
            let resources_dict =
                inherited_resources(&self.inner, other_page_id).unwrap_or_default();

            // Decode and concatenate all of the other page's content streams.
            let content_streams = crate::extract::page_content_streams(&self.inner, other_page_id);
            let content_bytes: Vec<u8> = {
                let mut all: Vec<u8> = Vec::new();
                for (j, bytes) in content_streams.into_iter().enumerate() {
                    if j > 0 {
                        all.push(b'\n');
                    }
                    all.extend_from_slice(&bytes);
                }
                all
            };

            // Build a Form XObject from the other page's content and resources.
            let mut form_dict = Dictionary::new();
            form_dict.set("Type", Object::Name(b"XObject".to_vec()));
            form_dict.set("Subtype", Object::Name(b"Form".to_vec()));
            form_dict.set(
                "BBox",
                Object::Array(vec![
                    Object::Real(bbox[0]),
                    Object::Real(bbox[1]),
                    Object::Real(bbox[2]),
                    Object::Real(bbox[3]),
                ]),
            );
            form_dict.set("Resources", Object::Dictionary(resources_dict));
            let form_stream = Stream::new(form_dict, content_bytes);
            let form_id = self.inner.add_object(Object::Stream(form_stream));

            // Register the Form XObject in the base page's /Resources /XObject dict.
            let xobj_name = format!("OVRL{i}");
            add_xobject_to_resources(&mut self.inner, self_page_id, xobj_name.as_bytes(), form_id)?;

            // Isolate the existing page CTM before adding the overlay, so any
            // unbalanced `cm` in the existing content doesn't affect the overlay.
            wrap_page_contents_in_q_q(&mut self.inner, self_page_id)?;

            // Append the Do operator to invoke the overlay.
            let do_bytes = format!("/{xobj_name} Do\n").into_bytes();
            let do_stream_id = self
                .inner
                .add_object(Object::Stream(Stream::new(Dictionary::new(), do_bytes)));
            append_to_contents(&mut self.inner, self_page_id, do_stream_id)?;
        }

        Ok(())
    }

    /// Extracts the specified pages into a new document.
    ///
    /// `page_numbers` is 1-indexed and controls the order of pages in the result —
    /// e.g. `&[3, 1]` puts original page 3 first.
    ///
    /// # What is preserved
    /// Page content, embedded fonts, images, and resources.
    ///
    /// # What is NOT preserved
    /// Outlines/Bookmarks, AcroForm, /Names, /PageLabels, /OpenAction,
    /// /StructTreeRoot, and any pending (unflushed) text/draw operations.
    /// Page properties (MediaBox, Rotate, Resources) inherited from intermediate
    /// `/Pages` nodes in the source document are not preserved — pages that rely
    /// on such inheritance should have those properties set directly on the page
    /// dict before extraction.
    ///
    /// Objects referenced exclusively by removed pages (content streams, font
    /// files, image XObjects) are **not** garbage-collected from the output — the
    /// extracted PDF may be larger than expected when only a few pages are kept
    /// from a large document.
    ///
    /// # Errors
    /// Returns [`Error::InvalidInput`] if `page_numbers` is empty or contains duplicates.
    /// Returns [`Error::PageNotFound`] if any number is out of range.
    pub fn extract_pages(&self, page_numbers: &[u32]) -> Result<Document> {
        if page_numbers.is_empty() {
            return Err(Error::InvalidInput("page_numbers must not be empty".into()));
        }

        let all_pages = self.inner.get_pages();
        let mut seen = vec![false; all_pages.len() + 1]; // 1-indexed guard
        let mut keep_ids: Vec<ObjectId> = Vec::with_capacity(page_numbers.len());

        for &n in page_numbers {
            let id = all_pages.get(&n).copied().ok_or(Error::PageNotFound(n))?;
            // n is already validated to be in 1..=all_pages.len(), so seen[n as usize] is in bounds.
            let idx = n as usize;
            if seen[idx] {
                return Err(Error::InvalidInput(format!("duplicate page number: {n}")));
            }
            seen[idx] = true;
            keep_ids.push(id);
        }

        let mut new_inner = self.inner.clone();

        // Rebuild /Kids to only contain the requested pages in the requested order.
        let pages_root = root_pages_id(&new_inner)?;
        for &pid in &keep_ids {
            let d = new_inner.get_object_mut(pid)?.as_dict_mut()?;
            d.set("Parent", Object::Reference(pages_root));
        }
        let new_kids: Vec<Object> = keep_ids.iter().map(|&id| Object::Reference(id)).collect();
        let pages_dict = new_inner.get_object_mut(pages_root)?.as_dict_mut()?;
        pages_dict.set("Kids", Object::Array(new_kids));
        pages_dict.set("Count", Object::Integer(keep_ids.len() as i64));

        // Remove page dict objects that are not in the extracted set.
        let keep_set: HashSet<ObjectId> = keep_ids.iter().copied().collect();
        for &id in all_pages.values() {
            if !keep_set.contains(&id) {
                new_inner.objects.remove(&id);
            }
        }

        // Strip catalog entries that may reference deleted page objects.
        let root_ref = new_inner.trailer.get(b"Root")?.as_reference()?;
        let catalog = new_inner.get_object_mut(root_ref)?.as_dict_mut()?;
        catalog.remove(b"Outlines");
        catalog.remove(b"AcroForm");
        catalog.remove(b"Names");
        catalog.remove(b"PageLabels");
        catalog.remove(b"OpenAction");
        catalog.remove(b"StructTreeRoot");

        Ok(Document::from_inner(new_inner))
    }

    /// Returns the document's `/Info` metadata fields.
    ///
    /// All fields are `None` if the document has no `/Info` dictionary.
    pub fn metadata(&self) -> Result<PdfMetadata> {
        use lopdf::Object;

        let info_dict: Option<lopdf::Dictionary> = match self.inner.trailer.get(b"Info").ok() {
            Some(Object::Reference(id)) => self.inner.get_object(*id).ok().and_then(|o| {
                if let Object::Dictionary(d) = o {
                    Some(d.clone())
                } else {
                    None
                }
            }),
            Some(Object::Dictionary(d)) => Some(d.clone()),
            _ => None,
        };

        let field =
            |d: &lopdf::Dictionary, key: &[u8]| d.get(key).ok().and_then(lopdf_string_to_rust);

        Ok(match info_dict {
            Some(d) => PdfMetadata {
                title: field(&d, b"Title"),
                author: field(&d, b"Author"),
                subject: field(&d, b"Subject"),
                keywords: field(&d, b"Keywords"),
                creator: field(&d, b"Creator"),
            },
            None => PdfMetadata::default(),
        })
    }

    /// Writes (or replaces) the document's `/Info` metadata dictionary.
    ///
    /// Only `Some` fields are written; `None` fields are omitted.
    /// Can be called before or after adding text/shapes — metadata is independent of font subsetting.
    ///
    /// # Errors
    /// Returns [`Error::InvalidInput`] if the document has already been saved.
    pub fn set_metadata(&mut self, meta: &PdfMetadata) -> Result<()> {
        if self.finalized {
            return Err(Error::InvalidInput(
                "set_metadata() called after save(); create a new Document".into(),
            ));
        }
        use lopdf::{Object, StringFormat};

        let mut dict = lopdf::Dictionary::new();
        let mut set = |key: &[u8], val: &Option<String>| {
            if let Some(s) = val {
                dict.set(
                    key,
                    Object::String(s.as_bytes().to_vec(), StringFormat::Literal),
                );
            }
        };
        set(b"Title", &meta.title);
        set(b"Author", &meta.author);
        set(b"Subject", &meta.subject);
        set(b"Keywords", &meta.keywords);
        set(b"Creator", &meta.creator);

        let info_id = self.inner.add_object(Object::Dictionary(dict));
        self.inner.trailer.set("Info", Object::Reference(info_id));
        Ok(())
    }

    // -----------------------------------------------------------------------
    // AcroForm
    // -----------------------------------------------------------------------

    /// Returns all interactive form fields present in the document.
    ///
    /// Fields are collected recursively from the `/AcroForm` field tree.
    /// Leaf fields carry a [`FieldType`] and their current string value.
    /// Non-interactive documents (no `/AcroForm`) return an empty `Vec`.
    ///
    /// # Errors
    /// Returns [`Error::Pdf`] only on structurally malformed AcroForm data.
    pub fn form_fields(&self) -> Result<Vec<FormField>> {
        let Some(acroform_id) = acroform_id(&self.inner) else {
            return Ok(Vec::new());
        };
        let Ok(acroform) = self.inner.get_object(acroform_id)?.as_dict() else {
            return Ok(Vec::new());
        };
        let Ok(fields_obj) = acroform.get(b"Fields") else {
            return Ok(Vec::new());
        };
        let field_refs: Vec<Object> = match fields_obj {
            Object::Array(arr) => arr.clone(),
            Object::Reference(id) => match self.inner.get_object(*id)? {
                Object::Array(arr) => arr.clone(),
                _ => return Ok(Vec::new()),
            },
            _ => return Ok(Vec::new()),
        };
        let mut out = Vec::new();
        collect_fields_recursive(&self.inner, &field_refs, "", &mut out);
        Ok(out)
    }

    /// Fills one or more form fields by name.
    ///
    /// `values` is a slice of `(field_name, value_string)` pairs.
    ///
    /// For **text fields** the string is used as-is. For **checkboxes** the
    /// strings `"true"`, `"yes"`, `"on"`, `"1"` (case-insensitive) set the field
    /// to its `/Yes` state; everything else sets it to `/Off`.
    ///
    /// The method sets `/NeedAppearances true` on the `/AcroForm` dictionary
    /// so that PDF viewers regenerate the visual appearance.
    ///
    /// Returns the number of fields that were updated.
    ///
    /// # Errors
    /// Returns [`Error::InvalidInput`] if called after [`save`](Document::save).
    pub fn fill_form(&mut self, values: &[(&str, &str)]) -> Result<usize> {
        if self.finalized {
            return Err(Error::InvalidInput(
                "fill_form() called after save(); create a new Document".into(),
            ));
        }
        if values.is_empty() {
            return Ok(0);
        }
        let Some(acroform_id) = acroform_id(&self.inner) else {
            return Ok(0);
        };

        // Collect field IDs and their types so we can update them.
        let field_ids = collect_field_ids(&self.inner, acroform_id);

        let mut updated = 0usize;
        for (name, value) in values {
            for (id, ft, _partial_name) in &field_ids {
                if _partial_name == name {
                    if let Ok(dict) = self.inner.get_object_mut(*id)?.as_dict_mut() {
                        match ft {
                            FieldType::Checkbox | FieldType::Radio => {
                                let on = matches!(
                                    value.to_ascii_lowercase().as_str(),
                                    "true" | "yes" | "on" | "1"
                                );
                                let state = if on { b"Yes".as_ref() } else { b"Off".as_ref() };
                                dict.set("V", Object::Name(state.to_vec()));
                                dict.set("AS", Object::Name(state.to_vec()));
                            }
                            _ => {
                                dict.set("V", pdf_text_string(value));
                            }
                        }
                        updated += 1;
                    }
                    break;
                }
            }
        }

        // Signal viewers to regenerate appearances.
        if updated > 0
            && let Ok(d) = self.inner.get_object_mut(acroform_id)?.as_dict_mut()
        {
            d.set("NeedAppearances", Object::Boolean(true));
        }
        Ok(updated)
    }

    /// Creates a new text field on the specified page.
    ///
    /// The field is added to the document's `/AcroForm` if it doesn't exist,
    /// and registered in the page's `/Annots` array.
    ///
    /// The `rect` parameter is `[x, y, width, height]` in PDF points
    /// (origin: bottom-left of page).
    ///
    /// # Errors
    /// - [`Error::PageNotFound`] if the page number is invalid.
    /// - [`Error::InvalidInput`] if field creation fails.
    pub fn add_text_field(
        &mut self,
        page: u32,
        name: &str,
        rect: [f32; 4],
        options: &TextFieldOptions,
    ) -> Result<()> {
        if self.finalized {
            return Err(Error::InvalidInput(
                "add_text_field() called after save(); create a new Document".into(),
            ));
        }

        let page_id = *self
            .inner
            .get_pages()
            .get(&page)
            .ok_or(Error::PageNotFound(page))?;

        check_finite(&rect, "add_text_field rect")?;

        let acroform_id = ensure_acroform(&mut self.inner)?;

        // Build widget dictionary
        let mut widget_dict = Dictionary::new();
        widget_dict.set("Type", Object::Name(b"Annot".to_vec()));
        widget_dict.set("Subtype", Object::Name(b"Widget".to_vec()));
        widget_dict.set("FT", Object::Name(b"Tx".to_vec()));
        widget_dict.set(
            "T",
            Object::String(name.as_bytes().to_vec(), StringFormat::Literal),
        );
        widget_dict.set(
            "Rect",
            Object::Array(vec![
                Object::Real(rect[0]),
                Object::Real(rect[1]),
                Object::Real(rect[0] + rect[2]),
                Object::Real(rect[1] + rect[3]),
            ]),
        );
        widget_dict.set("P", Object::Reference(page_id));
        widget_dict.set(
            "DA",
            Object::String(b"(/Helv 12 Tf 0 g)".to_vec(), StringFormat::Literal),
        );

        // Set flags
        let mut flags = 0i64;
        if options.multiline {
            flags |= 1 << 12;
        }
        if options.read_only {
            flags |= 1;
        }
        if flags != 0 {
            widget_dict.set("Ff", Object::Integer(flags));
        }

        // Set default value
        if !options.default_value.is_empty() {
            widget_dict.set("V", pdf_text_string(&options.default_value));
        }

        let widget_id = self.inner.add_object(Object::Dictionary(widget_dict));

        // Add to AcroForm /Fields array
        let acroform_dict = self.inner.get_object_mut(acroform_id)?.as_dict_mut()?;
        let fields_arr = if let Ok(Object::Array(arr)) = acroform_dict.get(b"Fields") {
            let mut arr = arr.clone();
            arr.push(Object::Reference(widget_id));
            arr
        } else {
            vec![Object::Reference(widget_id)]
        };
        acroform_dict.set("Fields", Object::Array(fields_arr));

        // Add to page /Annots array
        append_annotation_to_page(&mut self.inner, page_id, widget_id)?;

        // Signal viewers to regenerate appearances
        if let Ok(d) = self.inner.get_object_mut(acroform_id)?.as_dict_mut() {
            d.set("NeedAppearances", Object::Boolean(true));
        }

        Ok(())
    }

    /// Creates a new checkbox field on the specified page.
    ///
    /// The `rect` parameter is `[x, y, width, height]` in PDF points.
    ///
    /// # Errors
    /// - [`Error::PageNotFound`] if the page number is invalid.
    /// - [`Error::InvalidInput`] if field creation fails.
    pub fn add_checkbox(
        &mut self,
        page: u32,
        name: &str,
        rect: [f32; 4],
        checked: bool,
    ) -> Result<()> {
        if self.finalized {
            return Err(Error::InvalidInput(
                "add_checkbox() called after save(); create a new Document".into(),
            ));
        }

        let page_id = *self
            .inner
            .get_pages()
            .get(&page)
            .ok_or(Error::PageNotFound(page))?;

        check_finite(&rect, "add_checkbox rect")?;

        let acroform_id = ensure_acroform(&mut self.inner)?;

        let state = if checked {
            b"Yes".as_ref()
        } else {
            b"Off".as_ref()
        };

        let mut widget_dict = Dictionary::new();
        widget_dict.set("Type", Object::Name(b"Annot".to_vec()));
        widget_dict.set("Subtype", Object::Name(b"Widget".to_vec()));
        widget_dict.set("FT", Object::Name(b"Btn".to_vec()));
        widget_dict.set(
            "T",
            Object::String(name.as_bytes().to_vec(), StringFormat::Literal),
        );
        widget_dict.set(
            "Rect",
            Object::Array(vec![
                Object::Real(rect[0]),
                Object::Real(rect[1]),
                Object::Real(rect[0] + rect[2]),
                Object::Real(rect[1] + rect[3]),
            ]),
        );
        widget_dict.set("P", Object::Reference(page_id));
        widget_dict.set("V", Object::Name(state.to_vec()));
        widget_dict.set("AS", Object::Name(state.to_vec()));
        widget_dict.set(
            "DA",
            Object::String(b"(/Helv 12 Tf 0 g)".to_vec(), StringFormat::Literal),
        );

        let widget_id = self.inner.add_object(Object::Dictionary(widget_dict));

        // Add to AcroForm /Fields array
        let acroform_dict = self.inner.get_object_mut(acroform_id)?.as_dict_mut()?;
        let fields_arr = if let Ok(Object::Array(arr)) = acroform_dict.get(b"Fields") {
            let mut arr = arr.clone();
            arr.push(Object::Reference(widget_id));
            arr
        } else {
            vec![Object::Reference(widget_id)]
        };
        acroform_dict.set("Fields", Object::Array(fields_arr));

        // Add to page /Annots array
        append_annotation_to_page(&mut self.inner, page_id, widget_id)?;

        // Signal viewers to regenerate appearances
        if let Ok(d) = self.inner.get_object_mut(acroform_id)?.as_dict_mut() {
            d.set("NeedAppearances", Object::Boolean(true));
        }

        Ok(())
    }

    /// Creates a new radio button group on the specified page.
    ///
    /// Each button in `buttons` is a `(value_name, rect)` pair where `rect` is
    /// `[x, y, width, height]` in PDF points.
    ///
    /// # Errors
    /// - [`Error::PageNotFound`] if the page number is invalid.
    /// - [`Error::InvalidInput`] if field creation fails or `buttons` is empty.
    pub fn add_radio_group(
        &mut self,
        page: u32,
        group_name: &str,
        buttons: &[(&str, [f32; 4])],
        selected: Option<&str>,
    ) -> Result<()> {
        if self.finalized {
            return Err(Error::InvalidInput(
                "add_radio_group() called after save(); create a new Document".into(),
            ));
        }

        if buttons.is_empty() {
            return Err(Error::InvalidInput(
                "radio group must have at least one button".into(),
            ));
        }

        let page_id = *self
            .inner
            .get_pages()
            .get(&page)
            .ok_or(Error::PageNotFound(page))?;

        let acroform_id = ensure_acroform(&mut self.inner)?;

        // Determine which button is selected
        let selected_value = selected.unwrap_or_else(|| buttons[0].0);

        // Create parent field dictionary (intermediate node, no /Subtype)
        let mut parent_dict = Dictionary::new();
        parent_dict.set("FT", Object::Name(b"Btn".to_vec()));
        parent_dict.set("Ff", Object::Integer(1 << 15)); // Radio flag
        parent_dict.set(
            "T",
            Object::String(group_name.as_bytes().to_vec(), StringFormat::Literal),
        );
        parent_dict.set("V", Object::Name(selected_value.as_bytes().to_vec()));

        let parent_id = self
            .inner
            .add_object(Object::Dictionary(parent_dict.clone()));

        let mut kids = Vec::new();
        for (value_name, rect) in buttons {
            check_finite(rect, "radio button rect")?;

            let is_selected = value_name == &selected_value;
            let state = if is_selected {
                b"Yes".as_ref()
            } else {
                b"Off".as_ref()
            };

            let mut button_dict = Dictionary::new();
            button_dict.set("Type", Object::Name(b"Annot".to_vec()));
            button_dict.set("Subtype", Object::Name(b"Widget".to_vec()));
            button_dict.set(
                "Rect",
                Object::Array(vec![
                    Object::Real(rect[0]),
                    Object::Real(rect[1]),
                    Object::Real(rect[0] + rect[2]),
                    Object::Real(rect[1] + rect[3]),
                ]),
            );
            button_dict.set("Parent", Object::Reference(parent_id));
            button_dict.set("P", Object::Reference(page_id));
            button_dict.set("AS", Object::Name(state.to_vec()));
            button_dict.set("V", Object::Name(value_name.as_bytes().to_vec()));
            button_dict.set(
                "DA",
                Object::String(b"(/Helv 12 Tf 0 g)".to_vec(), StringFormat::Literal),
            );

            let button_id = self.inner.add_object(Object::Dictionary(button_dict));
            kids.push(Object::Reference(button_id));

            // Add to page /Annots array
            append_annotation_to_page(&mut self.inner, page_id, button_id)?;
        }

        // Update parent dict with /Kids array
        let parent_dict_mut = self.inner.get_object_mut(parent_id)?.as_dict_mut()?;
        parent_dict_mut.set("Kids", Object::Array(kids));

        // Add to AcroForm /Fields array
        let acroform_dict = self.inner.get_object_mut(acroform_id)?.as_dict_mut()?;
        let fields_arr = if let Ok(Object::Array(arr)) = acroform_dict.get(b"Fields") {
            let mut arr = arr.clone();
            arr.push(Object::Reference(parent_id));
            arr
        } else {
            vec![Object::Reference(parent_id)]
        };
        acroform_dict.set("Fields", Object::Array(fields_arr));

        // Signal viewers to regenerate appearances
        if let Ok(d) = self.inner.get_object_mut(acroform_id)?.as_dict_mut() {
            d.set("NeedAppearances", Object::Boolean(true));
        }

        Ok(())
    }

    /// Adds a digital signature field to a page.
    ///
    /// The signature field is a widget annotation that will hold a digital signature.
    /// After calling this method, use [`sign_document`](Document::sign_document) to create
    /// the actual signature (requires the `digital-signature` feature).
    ///
    /// # Arguments
    ///
    /// * `page` — 1-indexed page number
    /// * `rect` — [x, y, width, height] of the signature field
    /// * `options` — [`SignatureFieldOptions`](crate::SignatureFieldOptions) with field metadata
    ///
    /// # Example
    /// ```no_run
    /// # #[cfg(feature = "digital-signature")] {
    /// # use harumi::{Document, SignatureFieldOptions};
    /// # fn main() -> harumi::Result<()> {
    /// let mut doc = Document::from_file("unsigned.pdf")?;
    /// let options = SignatureFieldOptions {
    ///     field_name: "Sig1".into(),
    ///     reason: Some("Approval".into()),
    ///     contact_info: None,
    ///     lock_permissions: false,
    /// };
    /// doc.add_signature_field(1, [50.0, 600.0, 200.0, 50.0], &options)?;
    /// # Ok(())
    /// # }
    /// # }
    /// ```
    #[cfg(feature = "digital-signature")]
    pub fn add_signature_field(
        &mut self,
        page: u32,
        rect: [f32; 4],
        options: &crate::SignatureFieldOptions,
    ) -> Result<()> {
        if self.finalized {
            return Err(Error::InvalidInput(
                "add_signature_field() called after save()".into(),
            ));
        }

        check_finite(&rect, "signature field rect")?;

        let page_id = *self
            .inner
            .get_pages()
            .get(&page)
            .ok_or(Error::PageNotFound(page))?;

        let acroform_id = ensure_acroform(&mut self.inner)?;

        let mut sig_dict = Dictionary::new();
        sig_dict.set("Type", Object::Name(b"Sig".to_vec()));
        sig_dict.set("Filter", Object::Name(b"Adobe.PPKLite".to_vec()));
        sig_dict.set("SubFilter", Object::Name(b"adbe.pkcs7.detached".to_vec()));

        if let Some(reason) = &options.reason {
            sig_dict.set(
                "Reason",
                Object::String(reason.as_bytes().to_vec(), StringFormat::Literal),
            );
        }

        if let Some(contact) = &options.contact_info {
            sig_dict.set(
                "ContactInfo",
                Object::String(contact.as_bytes().to_vec(), StringFormat::Literal),
            );
        }

        let sig_id = self.inner.add_object(Object::Dictionary(sig_dict));

        let mut field_dict = Dictionary::new();
        field_dict.set("Type", Object::Name(b"Annot".to_vec()));
        field_dict.set("Subtype", Object::Name(b"Widget".to_vec()));
        field_dict.set("FT", Object::Name(b"Sig".to_vec()));
        field_dict.set(
            "T",
            Object::String(
                options.field_name.as_bytes().to_vec(),
                StringFormat::Literal,
            ),
        );
        field_dict.set(
            "Rect",
            Object::Array(vec![
                Object::Real(rect[0]),
                Object::Real(rect[1]),
                Object::Real(rect[0] + rect[2]),
                Object::Real(rect[1] + rect[3]),
            ]),
        );
        field_dict.set("V", Object::Reference(sig_id));
        field_dict.set("P", Object::Reference(page_id));
        field_dict.set(
            "DA",
            Object::String(b"(/Helv 12 Tf 0 g)".to_vec(), StringFormat::Literal),
        );

        let field_id = self.inner.add_object(Object::Dictionary(field_dict));

        // Add to page /Annots array
        append_annotation_to_page(&mut self.inner, page_id, field_id)?;

        // Add to AcroForm /Fields array
        let acroform_dict = self.inner.get_object_mut(acroform_id)?.as_dict_mut()?;
        let fields_arr = if let Ok(Object::Array(arr)) = acroform_dict.get(b"Fields") {
            let mut arr = arr.clone();
            arr.push(Object::Reference(field_id));
            arr
        } else {
            vec![Object::Reference(field_id)]
        };
        acroform_dict.set("Fields", Object::Array(fields_arr));

        if let Ok(d) = self.inner.get_object_mut(acroform_id)?.as_dict_mut() {
            d.set("NeedAppearances", Object::Boolean(true));
        }

        Ok(())
    }

    /// Appends a named bookmark (PDF document outline entry) pointing to a
    /// specific position on a page.
    ///
    /// Bookmarks appear in the "Bookmarks" or "Outline" panel of most PDF
    /// viewers. Multiple calls build a flat list in the order they are added.
    /// Nested bookmarks are not yet supported.
    ///
    /// `title` — display label shown in the bookmarks panel.
    /// `page`  — 1-indexed destination page number.
    /// `y`     — PDF y coordinate (points, bottom-left origin) to scroll to on
    ///           the destination page. Pass `page_height` to land at the very
    ///           top of the page.
    ///
    /// # Example
    /// ```no_run
    /// # use harumi::Document;
    /// # fn main() -> harumi::Result<()> {
    /// let mut doc = Document::from_file("report.pdf")?;
    /// let (_, h) = doc.page(1)?.size()?;
    /// doc.add_bookmark("Chapter 1 — Introduction", 1, h)?;
    /// doc.add_bookmark("Chapter 2 — Results",      3, h)?;
    /// doc.save("report_with_bookmarks.pdf")?;
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// # Errors
    /// Returns [`Error::PageNotFound`] if `page` is 0 or exceeds the page count,
    /// or [`Error::InvalidInput`] if [`save`](Document::save) has already been called.
    pub fn add_bookmark(&mut self, title: &str, page: u32, y: f32) -> Result<()> {
        if self.finalized {
            return Err(Error::InvalidInput(
                "add_bookmark after save() is not supported; create a new Document".into(),
            ));
        }
        let count = self.page_count();
        if page == 0 || page > count {
            return Err(Error::PageNotFound(page));
        }
        self.pending_bookmarks.push(PendingBookmark {
            title: title.to_owned(),
            page,
            y,
            level: 0,
        });
        Ok(())
    }

    /// Adds an outline item (bookmark) with explicit hierarchy level.
    ///
    /// `level` is the nesting depth (1 = top-level, 2 = subsection, etc.).
    /// Levels are automatically clamped to 1–6. When creating outlines programmatically,
    /// use consecutive levels (1, 2, 2, 3, 2, ...) for proper nesting.
    ///
    /// # Errors
    /// Returns [`Error::PageNotFound`] if `page` is 0 or exceeds the page count,
    /// or [`Error::InvalidInput`] if [`save`](Document::save) has already been called.
    pub fn add_outline_item(&mut self, title: &str, page: u32, y: f32, level: u8) -> Result<()> {
        if self.finalized {
            return Err(Error::InvalidInput(
                "add_outline_item after save() is not supported; create a new Document".into(),
            ));
        }
        let count = self.page_count();
        if page == 0 || page > count {
            return Err(Error::PageNotFound(page));
        }
        let level = level.clamp(1, 6);
        self.pending_bookmarks.push(PendingBookmark {
            title: title.to_owned(),
            page,
            y,
            level,
        });
        Ok(())
    }

    /// Removes all bookmarks (table of contents) from the document.
    ///
    /// Clears both:
    /// - Pending bookmarks queued with [`add_bookmark`](Document::add_bookmark) or
    ///   [`add_outline_item`](Document::add_outline_item) that have not yet been saved.
    /// - Any `/Outlines` tree already present in a loaded PDF.
    ///
    /// # Errors
    /// Returns [`Error::Pdf`] if the document's catalog is structurally malformed.
    pub fn clear_outline(&mut self) -> Result<()> {
        self.pending_bookmarks.clear();
        let root_ref = self.inner.trailer.get(b"Root")?.as_reference()?;
        let catalog = self.inner.get_object_mut(root_ref)?.as_dict_mut()?;
        catalog.remove(b"Outlines");
        Ok(())
    }

    /// Embeds a file as a PDF attachment.
    ///
    /// The attachment is stored in the document's `/Names /EmbeddedFiles` name tree
    /// and is visible in PDF viewers that support attachments (Adobe Acrobat, Preview, etc.).
    /// The data is compressed with FlateDecode before embedding.
    ///
    /// `mime_type` is stored as the `/Subtype` of the embedded stream (e.g. `"text/plain"`,
    /// `"image/png"`). Pass an empty string to omit it.
    ///
    /// # Errors
    /// Returns [`Error::InvalidInput`] if called after [`save`](Document::save) or if
    /// `filename` is empty.
    pub fn attach_file(&mut self, filename: &str, data: &[u8], mime_type: &str) -> Result<()> {
        if self.finalized {
            return Err(Error::InvalidInput(
                "attach_file after save() is not supported".into(),
            ));
        }
        if filename.is_empty() {
            return Err(Error::InvalidInput("filename must not be empty".into()));
        }

        // Build the EmbeddedFile stream.
        let mut ef_dict = Dictionary::new();
        ef_dict.set("Type", Object::Name(b"EmbeddedFile".to_vec()));
        if !mime_type.is_empty() {
            ef_dict.set("Subtype", Object::Name(mime_type.as_bytes().to_vec()));
        }
        let mut params = Dictionary::new();
        params.set("Size", Object::Integer(data.len() as i64));
        ef_dict.set("Params", Object::Dictionary(params));
        let mut ef_stream = Stream::new(ef_dict, data.to_vec());
        let _ = ef_stream.compress();
        let ef_id = self.inner.add_object(Object::Stream(ef_stream));

        // Build the Filespec dictionary.
        let mut fs_dict = Dictionary::new();
        fs_dict.set("Type", Object::Name(b"Filespec".to_vec()));
        fs_dict.set(
            "F",
            Object::String(filename.as_bytes().to_vec(), StringFormat::Literal),
        );
        fs_dict.set("UF", pdf_text_string(filename));
        let mut ef_ref_dict = Dictionary::new();
        ef_ref_dict.set("F", Object::Reference(ef_id));
        fs_dict.set("EF", Object::Dictionary(ef_ref_dict));
        let fs_id = self.inner.add_object(Object::Dictionary(fs_dict));

        // Insert into /Catalog /Names /EmbeddedFiles /Names array (sorted by filename).
        let root_ref = self.inner.trailer.get(b"Root")?.as_reference()?;

        // Get or create /Names dict in catalog.
        let names_id: ObjectId = {
            let catalog = self.inner.get_object(root_ref)?.as_dict()?;
            match catalog.get(b"Names").ok() {
                Some(Object::Reference(r)) => *r,
                Some(Object::Dictionary(_)) => {
                    // Inline dict — upgrade to indirect reference.
                    let d = catalog.get(b"Names")?.as_dict()?.clone();
                    let id = self.inner.add_object(Object::Dictionary(d));
                    self.inner
                        .get_object_mut(root_ref)?
                        .as_dict_mut()?
                        .set("Names", Object::Reference(id));
                    id
                }
                _ => {
                    let id = self.inner.add_object(Object::Dictionary(Dictionary::new()));
                    self.inner
                        .get_object_mut(root_ref)?
                        .as_dict_mut()?
                        .set("Names", Object::Reference(id));
                    id
                }
            }
        };

        // Get or create /EmbeddedFiles entry in /Names dict.
        let ef_names_id: ObjectId = {
            let names_dict = self.inner.get_object(names_id)?.as_dict()?;
            match names_dict.get(b"EmbeddedFiles").ok() {
                Some(Object::Reference(r)) => *r,
                Some(Object::Dictionary(_)) => {
                    let d = names_dict.get(b"EmbeddedFiles")?.as_dict()?.clone();
                    let id = self.inner.add_object(Object::Dictionary(d));
                    self.inner
                        .get_object_mut(names_id)?
                        .as_dict_mut()?
                        .set("EmbeddedFiles", Object::Reference(id));
                    id
                }
                _ => {
                    let mut d = Dictionary::new();
                    d.set("Names", Object::Array(vec![]));
                    let id = self.inner.add_object(Object::Dictionary(d));
                    self.inner
                        .get_object_mut(names_id)?
                        .as_dict_mut()?
                        .set("EmbeddedFiles", Object::Reference(id));
                    id
                }
            }
        };

        // Append [filename_string, filespec_ref] to the /Names array.
        let ef_names_dict = self.inner.get_object_mut(ef_names_id)?.as_dict_mut()?;
        let names_arr = match ef_names_dict.get_mut(b"Names") {
            Ok(obj) => obj.as_array_mut()?,
            Err(_) => {
                ef_names_dict.set("Names", Object::Array(vec![]));
                ef_names_dict.get_mut(b"Names")?.as_array_mut()?
            }
        };
        names_arr.push(Object::String(
            filename.as_bytes().to_vec(),
            StringFormat::Literal,
        ));
        names_arr.push(Object::Reference(fs_id));

        // PDF spec §7.9.6 requires /Names entries to be sorted ascending by key.
        // Sort in-place, treating the flat array as (key, value) pairs.
        let n = names_arr.len();
        if n >= 4 {
            let mut pairs: Vec<(Vec<u8>, Object)> = Vec::with_capacity(n / 2);
            let mut iter = std::mem::take(names_arr).into_iter();
            while let (Some(k), Some(v)) = (iter.next(), iter.next()) {
                let key_bytes = match &k {
                    Object::String(b, _) => b.clone(),
                    _ => vec![],
                };
                pairs.push((key_bytes, v));
            }
            pairs.sort_by(|a, b| a.0.cmp(&b.0));
            for (key_bytes, val) in pairs {
                names_arr.push(Object::String(key_bytes, StringFormat::Literal));
                names_arr.push(val);
            }
        }

        Ok(())
    }

    /// Returns information about all files attached to this document.
    ///
    /// Reads the `/Names /EmbeddedFiles` name tree. Only flat (non-nested) `/Names`
    /// arrays are currently traversed; hierarchical name trees with `/Kids` nodes are
    /// skipped.
    ///
    /// # Errors
    /// Returns [`Error::Pdf`] only on structurally malformed catalog data.
    pub fn list_attachments(&self) -> Result<Vec<AttachmentInfo>> {
        let root_ref = self.inner.trailer.get(b"Root")?.as_reference()?;
        let catalog = self.inner.get_object(root_ref)?.as_dict()?;

        let names_dict = match catalog.get(b"Names").ok() {
            Some(Object::Reference(r)) => self.inner.get_object(*r)?.as_dict()?.clone(),
            Some(Object::Dictionary(d)) => d.clone(),
            _ => return Ok(vec![]),
        };

        let ef_dict = match names_dict.get(b"EmbeddedFiles").ok() {
            Some(Object::Reference(r)) => self.inner.get_object(*r)?.as_dict()?.clone(),
            Some(Object::Dictionary(d)) => d.clone(),
            _ => return Ok(vec![]),
        };

        let names_arr = match ef_dict.get(b"Names").ok() {
            Some(Object::Array(a)) => a.clone(),
            _ => return Ok(vec![]),
        };

        let mut results = Vec::new();
        let mut iter = names_arr.iter();
        while let (Some(_key), Some(val)) = (iter.next(), iter.next()) {
            let fs_dict = match val {
                Object::Reference(r) => match self.inner.get_object(*r).ok() {
                    Some(o) => match o.as_dict().ok() {
                        Some(d) => d.clone(),
                        None => continue,
                    },
                    None => continue,
                },
                Object::Dictionary(d) => d.clone(),
                _ => continue,
            };

            // Resolve filename from /F entry.
            let filename = match fs_dict.get(b"F").ok() {
                Some(Object::String(bytes, _)) => String::from_utf8_lossy(bytes).into_owned(),
                _ => continue,
            };

            // Resolve the EmbeddedFile stream for size and MIME type.
            let (size, mime_type) = match fs_dict.get(b"EF").ok() {
                Some(ef_obj) => {
                    let ef_ref_dict = match ef_obj {
                        Object::Dictionary(d) => Some(d.clone()),
                        Object::Reference(r) => self
                            .inner
                            .get_object(*r)
                            .ok()
                            .and_then(|o| o.as_dict().ok().cloned()),
                        _ => None,
                    };
                    if let Some(efd) = ef_ref_dict {
                        let ef_stream_obj = match efd.get(b"F").ok() {
                            Some(Object::Reference(r)) => self.inner.get_object(*r).ok().cloned(),
                            _ => None,
                        };
                        if let Some(Object::Stream(s)) = ef_stream_obj {
                            let size = match s.dict.get(b"Params").ok() {
                                Some(Object::Dictionary(p)) => match p.get(b"Size").ok() {
                                    Some(Object::Integer(n)) => *n as usize,
                                    _ => 0,
                                },
                                _ => 0,
                            };
                            let mime = match s.dict.get(b"Subtype").ok() {
                                Some(Object::Name(n)) => {
                                    Some(String::from_utf8_lossy(n).into_owned())
                                }
                                _ => None,
                            };
                            (size, mime)
                        } else {
                            (0, None)
                        }
                    } else {
                        (0, None)
                    }
                }
                _ => (0, None),
            };

            results.push(AttachmentInfo {
                filename,
                size,
                mime_type,
            });
        }

        Ok(results)
    }

    /// Extracts positioned text fragments from a page.
    ///
    /// Only content streams that use Identity-H encoded CID fonts (as written by
    /// harumi) are decoded. Other content is silently skipped.
    ///
    /// Pending (not-yet-saved) text operations are **not** included — call
    /// [`save_to_bytes`](Document::save_to_bytes) and reload first if you need
    /// to read back text you just added.
    ///
    /// Measure the rendered width of `text` using the font registered as `font`.
    ///
    /// Returns the total advance width in PDF points.  Characters not covered by the font
    /// contribute zero width.
    ///
    /// # Errors
    /// Returns [`Error::InvalidFont`] if `font` was not registered on this document.
    /// Returns [`Error::FontParse`] if the raw font bytes cannot be parsed.
    pub fn measure_text(&self, text: &str, font: FontHandle, font_size: f32) -> Result<f32> {
        let raw = self
            .raw_fonts
            .get(font.0 as usize)
            .ok_or(Error::InvalidFont(font.0))?;
        let face = ttf_parser::Face::parse(&raw.ttf_bytes, 0)
            .map_err(|e| Error::FontParse(e.to_string()))?;
        Ok(helpers::text_width_with_face(text, &face, font_size))
    }

    /// Plan how `text` lays out within `rect` using `font`, without mutating the document.
    ///
    /// `rect` = `[x, y, width, height]` in PDF points (bottom-left origin).
    ///
    /// Returns a [`FitResult`](crate::FitResult) describing the wrapped/shrunk lines,
    /// final font size, actual occupied rectangle, and horizontal/vertical overflow flags.
    /// The geometry (`line_height = font_size * 1.2`, `wrap_paragraph` algorithm) is
    /// identical to the draw path, so collision checks on the returned `used_rect` reflect
    /// what would actually be rendered.
    ///
    /// # Errors
    /// Returns [`Error::InvalidFont`] if `font` was not registered on this document.
    /// Returns [`Error::FontParse`] if the raw font bytes cannot be parsed.
    pub fn fit_text_to_box(
        &self,
        text: &str,
        font: FontHandle,
        rect: [f32; 4],
        font_size: f32,
        opts: types::BoxFitOptions,
    ) -> Result<types::FitResult> {
        let raw = self
            .raw_fonts
            .get(font.0 as usize)
            .ok_or(Error::InvalidFont(font.0))?;
        let face = ttf_parser::Face::parse(&raw.ttf_bytes, 0)
            .map_err(|e| Error::FontParse(e.to_string()))?;
        Ok(helpers::plan_text_fit(text, &face, rect, font_size, &opts))
    }

    /// Plan replacement text for a batch of [`LayoutRegion`](crate::LayoutRegion)s,
    /// fitting each replacement into the region's `usable_rect` and detecting
    /// collisions between the resulting [`FitResult::used_rect`](crate::FitResult)s.
    ///
    /// `regions` and `replacements` are zipped; the shorter slice determines how
    /// many plans are returned.
    ///
    /// The initial font size for each region is derived from the mean `font_size`
    /// of its source fragments (fallback `10.0`).  [`BoxFitOptions`](crate::BoxFitOptions)
    /// controls wrapping and shrink-to-fit behaviour.
    ///
    /// # Errors
    /// Returns [`Error::InvalidFont`] or [`Error::FontParse`] if `font` is invalid.
    pub fn plan_text_for_regions(
        &self,
        regions: &[crate::extract::LayoutRegion],
        replacements: &[String],
        font: FontHandle,
        opts: types::BoxFitOptions,
    ) -> Result<Vec<crate::extract::RegionFitPlan>> {
        let raw = self
            .raw_fonts
            .get(font.0 as usize)
            .ok_or(Error::InvalidFont(font.0))?;
        let face = ttf_parser::Face::parse(&raw.ttf_bytes, 0)
            .map_err(|e| Error::FontParse(e.to_string()))?;

        let pairs: Vec<_> = regions.iter().zip(replacements.iter()).collect();
        let mut fits: Vec<types::FitResult> = Vec::with_capacity(pairs.len());

        for (region, replacement) in &pairs {
            let font_size = {
                let sizes: Vec<f32> = region
                    .fragments
                    .iter()
                    .map(|f| f.font_size)
                    .filter(|fs| fs.is_finite() && *fs > 0.0)
                    .collect();
                if sizes.is_empty() {
                    10.0_f32
                } else {
                    sizes.iter().sum::<f32>() / sizes.len() as f32
                }
            };
            let fit =
                helpers::plan_text_fit(replacement, &face, region.usable_rect, font_size, &opts);
            fits.push(fit);
        }

        // Detect collisions among all used_rects
        let placed: Vec<crate::extract::PlacedBox> = fits
            .iter()
            .map(|f| crate::extract::PlacedBox::new(f.used_rect))
            .collect();
        let all_collisions = crate::extract::detect_collisions(&placed);
        let all_classified = crate::extract::classify_collisions(regions, &all_collisions);

        let plans = pairs
            .into_iter()
            .enumerate()
            .zip(fits)
            .map(|((i, (region, _)), fit)| {
                let collisions = all_classified
                    .iter()
                    .filter(|cc| cc.collision.index_a == i || cc.collision.index_b == i)
                    .cloned()
                    .collect();
                crate::extract::RegionFitPlan {
                    region: region.clone(),
                    fit,
                    collisions,
                }
            })
            .collect();

        Ok(plans)
    }

    /// Like [`plan_text_for_regions`](Document::plan_text_for_regions) but with
    /// per-region fitting policies.
    ///
    /// `options` maps to `regions` by index.  When `options.len() < regions.len()`,
    /// the remaining regions use [`RegionTextFitOptions::for_role`](crate::RegionTextFitOptions::for_role)
    /// with their assigned role.  Pass `&[]` to use role-based defaults for all regions.
    ///
    /// The effective fitting rectangle is derived from each region's `source_bbox` and
    /// `usable_rect` according to [`BaselinePolicy`](crate::BaselinePolicy) and
    /// [`WidthPolicy`](crate::WidthPolicy):
    ///
    /// | `baseline`                 | y / height used                     |
    /// |----------------------------|-------------------------------------|
    /// | `PreserveSourceBaseline`   | `source_bbox[1]` / `source_bbox[3]` |
    /// | `TopAlignToRegion`         | `usable_rect[1]` / `usable_rect[3]` |
    /// | `CenterInRegion`           | centred slot within `usable_rect`   |
    ///
    /// | `width`                  | width used                                           |
    /// |--------------------------|------------------------------------------------------|
    /// | `SourceLineWidth`        | `source_bbox[2]`                                     |
    /// | `RegionUsableWidth` /    | `usable_rect[2]`                                     |
    /// | `ClampToColumn`          |                                                      |
    /// | `ClampBeforeNextRegion`  | gap to nearest same-row sibling at a higher column   |
    ///
    /// # Errors
    /// Returns [`Error::InvalidFont`] or [`Error::FontParse`] if `font` is invalid.
    pub fn plan_text_for_regions_with_policy(
        &self,
        regions: &[crate::extract::LayoutRegion],
        replacements: &[String],
        font: FontHandle,
        options: &[crate::extract::RegionTextFitOptions],
    ) -> Result<Vec<crate::extract::RegionFitPlan>> {
        use crate::extract::{BaselinePolicy, RegionTextFitOptions, WidthPolicy};

        let raw = self
            .raw_fonts
            .get(font.0 as usize)
            .ok_or(Error::InvalidFont(font.0))?;
        let face = ttf_parser::Face::parse(&raw.ttf_bytes, 0)
            .map_err(|e| Error::FontParse(e.to_string()))?;

        let pairs: Vec<_> = regions.iter().zip(replacements.iter()).collect();
        let mut fits: Vec<types::FitResult> = Vec::with_capacity(pairs.len());

        for (i, (region, replacement)) in pairs.iter().enumerate() {
            let opts = options
                .get(i)
                .cloned()
                .unwrap_or_else(|| RegionTextFitOptions::for_role(&region.role));

            // --- Derive x and width ---
            let base_x = if opts.preserve_source_x {
                region.source_bbox[0]
            } else {
                region.usable_rect[0]
            };
            let w = match opts.width {
                WidthPolicy::SourceLineWidth => region.source_bbox[2],
                WidthPolicy::RegionUsableWidth | WidthPolicy::ClampToColumn => {
                    region.usable_rect[2]
                }
                WidthPolicy::ClampBeforeNextRegion => {
                    let min_sib_x = regions
                        .iter()
                        .filter(|r| r.row == region.row && r.col > region.col)
                        .map(|r| r.source_bbox[0])
                        .fold(f32::INFINITY, f32::min);
                    if min_sib_x.is_finite() {
                        (min_sib_x - base_x - 2.0).max(1.0)
                    } else {
                        region.usable_rect[2]
                    }
                }
            };

            // --- Derive y and height ---
            let (eff_y, eff_h) = match opts.baseline {
                BaselinePolicy::PreserveSourceBaseline => {
                    (region.source_bbox[1], region.source_bbox[3].max(1.0))
                }
                BaselinePolicy::TopAlignToRegion => (region.usable_rect[1], region.usable_rect[3]),
                BaselinePolicy::CenterInRegion => {
                    let center = region.usable_rect[1] + region.usable_rect[3] / 2.0;
                    let h = region.source_bbox[3]
                        .max(region.usable_rect[3] * 0.5)
                        .max(1.0);
                    (center - h / 2.0, h)
                }
            };

            let eff_rect = [base_x, eff_y, w.max(1.0), eff_h];

            let font_size = {
                let sizes: Vec<f32> = region
                    .fragments
                    .iter()
                    .map(|f| f.font_size)
                    .filter(|fs| fs.is_finite() && *fs > 0.0)
                    .collect();
                if sizes.is_empty() {
                    10.0_f32
                } else {
                    sizes.iter().sum::<f32>() / sizes.len() as f32
                }
            };

            let box_opts = types::BoxFitOptions {
                min_font_size: opts.min_font_size,
                max_lines: opts.max_lines,
                ..types::BoxFitOptions::default()
            };

            let fit = helpers::plan_text_fit(replacement, &face, eff_rect, font_size, &box_opts);
            fits.push(fit);
        }

        let placed: Vec<crate::extract::PlacedBox> = fits
            .iter()
            .map(|f| crate::extract::PlacedBox::new(f.used_rect))
            .collect();
        let all_collisions = crate::extract::detect_collisions(&placed);
        let all_classified = crate::extract::classify_collisions(regions, &all_collisions);

        let plans = pairs
            .into_iter()
            .enumerate()
            .zip(fits)
            .map(|((i, (region, _)), fit)| {
                let collisions = all_classified
                    .iter()
                    .filter(|cc| cc.collision.index_a == i || cc.collision.index_b == i)
                    .cloned()
                    .collect();
                crate::extract::RegionFitPlan {
                    region: region.clone(),
                    fit,
                    collisions,
                }
            })
            .collect();

        Ok(plans)
    }

    /// # Errors
    /// Returns [`Error::PageNotFound`] if `page` is out of range.
    pub fn extract_text_runs(&self, page: u32) -> Result<Vec<crate::extract::TextFragment>> {
        let all_pages = self.inner.get_pages();
        let page_id = all_pages
            .get(&page)
            .copied()
            .ok_or(Error::PageNotFound(page))?;
        crate::extract::extract_text_runs_from_page(&self.inner, page_id)
    }

    /// Like [`extract_text_runs`](Document::extract_text_runs) but also returns
    /// [`ExtractionWarning`](crate::ExtractionWarning)s for content streams or Form XObjects
    /// that could not be decompressed.
    ///
    /// A non-empty warning list typically means some text was extracted from a raw
    /// (undecompressed) fallback and may be incomplete.  Use it to diagnose why a
    /// page returns fewer fragments than expected (e.g. AES-256 encrypted PDFs).
    ///
    /// # Errors
    /// Returns [`Error::PageNotFound`] if `page` is out of range.
    pub fn extract_text_runs_verbose(
        &self,
        page: u32,
    ) -> Result<(
        Vec<crate::extract::TextFragment>,
        Vec<crate::extract::ExtractionWarning>,
    )> {
        let all_pages = self.inner.get_pages();
        let page_id = all_pages
            .get(&page)
            .copied()
            .ok_or(Error::PageNotFound(page))?;
        crate::extract::extract_text_runs_from_page_verbose(&self.inner, page_id)
    }

    /// Extracts the decoded text from a single page as a plain string.
    ///
    /// This is a convenience wrapper over [`extract_text_runs`](Document::extract_text_runs).
    /// The text is returned in content-stream order, concatenated without adding spacing.
    ///
    /// # Errors
    /// Returns [`Error::PageNotFound`] if `page` is out of range.
    pub fn extract_text(&self, page: u32) -> Result<String> {
        let fragments = self.extract_text_runs(page)?;
        Ok(fragments
            .into_iter()
            .map(|frag| frag.text)
            .collect::<Vec<_>>()
            .join(""))
    }

    /// Extracts text runs from all pages, returning a flat vector of
    /// `(page_number, TextFragment)` tuples.
    ///
    /// Pages are numbered starting from 1. This is a convenience method
    /// combining the results of [`extract_text_runs`](Document::extract_text_runs)
    /// across all pages in reading order.
    ///
    /// # Errors
    /// Returns an error if any page cannot be read.
    ///
    /// # Example
    /// ```no_run
    /// # use harumi::Document;
    /// # fn main() -> harumi::Result<()> {
    /// # let doc = Document::from_file("example.pdf")?;
    /// let all_runs = doc.extract_all_text_runs()?;
    /// for (page_num, fragment) in all_runs {
    ///     println!("Page {}: {} at ({}, {})", page_num, fragment.text, fragment.x, fragment.y);
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub fn extract_all_text_runs(&self) -> Result<Vec<(u32, crate::extract::TextFragment)>> {
        // SECURITY: Prevent unbounded memory allocation from malicious PDFs.
        const MAX_PAGES_SAFE: u32 = 1_000_000;
        let page_count = self.page_count();
        if page_count > MAX_PAGES_SAFE {
            return Err(Error::InvalidInput(format!(
                "PDF has {} pages, exceeds safe limit of {}",
                page_count, MAX_PAGES_SAFE
            )));
        }

        let page_count_usize = page_count as usize;
        let mut result = Vec::with_capacity(page_count_usize);
        for page_num in 1..=page_count {
            let fragments = self.extract_text_runs(page_num)?;
            result.extend(fragments.into_iter().map(|frag| (page_num, frag)));
        }
        Ok(result)
    }

    /// Extracts the largest raster image embedded on the given page (1-indexed).
    ///
    /// Designed for **scanned PDFs** where each page is a single Image XObject.
    /// JPEG (DCTDecode) images are returned as-is; other formats are decoded and
    /// re-encoded as PNG. When a page contains multiple images the one with the
    /// greatest `width × height` area is returned.
    ///
    /// This method does **not** rasterize the page — it only retrieves an already-embedded
    /// image. Text and vector pages will return [`Error::InvalidInput`].
    ///
    /// Requires the **`image`** feature.
    ///
    /// # Example
    /// ```no_run
    /// # use harumi::Document;
    /// # fn main() -> harumi::Result<()> {
    /// let doc = Document::from_file("scanned.pdf")?;
    /// let img = doc.extract_page_image(1)?;
    /// std::fs::write("page1.jpg", &img.bytes)?;
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// # Errors
    /// - [`Error::PageNotFound`] if `page_number` is zero or exceeds the page count.
    /// - [`Error::InvalidInput`] if the page contains no Image XObject or the image
    ///   uses an unsupported filter (`CCITTFaxDecode`, `JBIG2Decode`, etc.).
    #[cfg(feature = "image")]
    pub fn extract_page_image(&self, page_number: u32) -> Result<crate::extract_image::PageImage> {
        let page_ids = self.inner.get_pages();
        let page_id = page_ids
            .get(&page_number)
            .copied()
            .ok_or(Error::PageNotFound(page_number))?;
        crate::extract_image::extract_largest_image_on_page(&self.inner, page_id)
    }

    /// Returns bounding boxes `[x, y, width, height]` in PDF points for every
    /// axis-aligned Image XObject on `page_number` (1-indexed).
    ///
    /// The coordinates use the PDF bottom-left origin.  Use the bounding boxes to
    /// detect image regions and avoid covering them with white rectangles during
    /// overlay translation.
    ///
    /// Returns an empty `Vec` for pages with no images, or when image placement
    /// uses a rotated/sheared matrix (uncommon in standard documents).
    ///
    /// # Errors
    /// Returns [`Error::PageNotFound`] if `page_number` is zero or exceeds page count.
    pub fn page_image_bboxes(&self, page_number: u32) -> Result<Vec<[f32; 4]>> {
        let page_ids = self.inner.get_pages();
        let page_id = page_ids
            .get(&page_number)
            .copied()
            .ok_or(Error::PageNotFound(page_number))?;
        Ok(crate::extract::extract_image_bboxes_from_page(
            &self.inner,
            page_id,
        ))
    }

    /// Assess page-level layout quality for a batch of planned replacement text.
    ///
    /// This combines [`PageFitSummary`](crate::PageFitSummary), detailed
    /// [`LayoutIssue`](crate::LayoutIssue) generation, collision severity, and
    /// image-overlap checks using [`Document::page_image_bboxes`].
    ///
    /// # Errors
    /// Returns [`Error::PageNotFound`] if `page_number` is zero or exceeds page count.
    pub fn assess_page_layout_quality(
        &self,
        page_number: u32,
        plans: &[crate::RegionFitPlan],
    ) -> Result<crate::PageLayoutQuality> {
        let image_bboxes = self.page_image_bboxes(page_number)?;
        Ok(crate::PageLayoutQuality::from_plans(
            page_number,
            plans,
            &image_bboxes,
        ))
    }

    /// Like [`assess_page_layout_quality`](Self::assess_page_layout_quality) but also checks
    /// the planned placements against `rules` (ruling lines / table borders).
    ///
    /// Issues of kind [`crate::LayoutIssueKind::TextVsTableBorder`] are added for any
    /// placement that overlaps a ruling line.  Pass `&[]` to skip rule checking.
    ///
    /// # Errors
    /// - [`Error::PageNotFound`] if `page_number` is out of range.
    pub fn assess_page_layout_quality_with_rules(
        &self,
        page_number: u32,
        plans: &[crate::RegionFitPlan],
        rules: &[crate::VectorRule],
    ) -> Result<crate::PageLayoutQuality> {
        let image_bboxes = self.page_image_bboxes(page_number)?;
        Ok(crate::PageLayoutQuality::from_plans_with_rules(
            page_number,
            plans,
            &image_bboxes,
            rules,
        ))
    }

    /// Extract ruling lines and table/box borders from a page's vector graphics.
    ///
    /// Analyses the raw content stream for stroked line segments and thin filled
    /// rectangles (`re` operators where `min(width, height) ≤ 3 pt`), which are the
    /// two most common forms for table and section borders in business PDFs.
    ///
    /// The current transformation matrix (`cm`, `q`/`Q`) is tracked so rules drawn in
    /// scaled/translated coordinates are returned in page (MediaBox) coordinates.
    ///
    /// **Limitation**: rules drawn exclusively inside Form XObjects are not returned.
    ///
    /// # Errors
    /// - [`Error::PageNotFound`] if `page_number` is zero or exceeds the page count.
    pub fn extract_vector_rules(&self, page_number: u32) -> Result<Vec<crate::VectorRule>> {
        let page_ids = self.inner.get_pages();
        let page_id = page_ids
            .get(&page_number)
            .copied()
            .ok_or(Error::PageNotFound(page_number))?;
        let media_box = helpers::inherited_media_box_raw(&self.inner, page_id);
        let page_height = media_box[3] - media_box[1];
        let content_streams = crate::extract::page_content_streams(&self.inner, page_id);
        let rules = content_streams
            .iter()
            .flat_map(|c| crate::extract::extract_vector_rules(c, page_height))
            .collect();
        Ok(rules)
    }

    /// Extracts all raster images embedded on the given page (1-indexed).
    ///
    /// Designed for **scanned PDFs** where a page may contain multiple Image XObjects.
    /// JPEG (DCTDecode) images are returned as-is; other formats are decoded and
    /// re-encoded as PNG. The returned vector preserves the order of XObjects in
    /// the page's `/Resources /XObject` dictionary.
    ///
    /// This method does **not** rasterize the page — it only retrieves already-embedded
    /// images. Text and vector pages will return [`Error::InvalidInput`].
    ///
    /// Requires the **`image`** feature.
    ///
    /// # Example
    /// ```no_run
    /// # use harumi::Document;
    /// # fn main() -> harumi::Result<()> {
    /// let doc = Document::from_file("scanned.pdf")?;
    /// let images = doc.extract_page_images(1)?;
    /// for (i, img) in images.iter().enumerate() {
    ///     std::fs::write(format!("page1_image{}.png", i), &img.bytes)?;
    /// }
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// # Errors
    /// - [`Error::PageNotFound`] if `page_number` is zero or exceeds the page count.
    /// - [`Error::InvalidInput`] if the page contains no Image XObject or all images
    ///   fail extraction (due to unsupported filters like `CCITTFaxDecode`).
    #[cfg(feature = "image")]
    pub fn extract_page_images(
        &self,
        page_number: u32,
    ) -> Result<Vec<crate::extract_image::PageImage>> {
        let page_ids = self.inner.get_pages();
        let page_id = page_ids
            .get(&page_number)
            .copied()
            .ok_or(Error::PageNotFound(page_number))?;
        crate::extract_image::extract_all_images_on_page(&self.inner, page_id)
    }

    /// Finalizes the document (subsets fonts, embeds them, writes content streams)
    /// and saves to a file.
    ///
    /// The original PDF structure is preserved; harumi only appends new objects
    /// and content streams.
    ///
    /// # Errors
    /// Propagates font subsetting, PDF mutation, or I/O errors.
    pub fn save(&mut self, path: impl AsRef<Path>) -> Result<()> {
        self.finalize()?;
        self.apply_pending_encryption()?;
        self.inner.save(path)?;
        Ok(())
    }

    /// Finalizes and returns the document as an in-memory `Vec<u8>`.
    ///
    /// Equivalent to calling [`save_to_writer`](Document::save_to_writer) with a
    /// `Vec<u8>` buffer. Useful in Tauri commands or any context where writing to
    /// a file is inconvenient.
    ///
    /// # Example
    /// ```no_run
    /// # use harumi::Document;
    /// # fn main() -> harumi::Result<()> {
    /// let mut doc = Document::from_file("input.pdf")?;
    /// let bytes = doc.save_to_bytes()?;
    /// // e.g. return bytes from a Tauri command or HTTP handler
    /// # Ok(())
    /// # }
    /// ```
    /// Signs the document with a digital signature (requires `digital-signature` feature).
    ///
    /// The signature is embedded into the specified signature field. The field must have been
    /// added previously with [`add_signature_field`](Document::add_signature_field).
    ///
    /// # Arguments
    ///
    /// * `context` — [`SigningContext`](crate::SigningContext) containing certificate and private key
    /// * `field_name` — Name of the signature field to sign into
    ///
    /// # Returns
    ///
    /// Signed PDF bytes as a `Vec<u8>`.
    ///
    /// # Example
    /// ```no_run
    /// # #[cfg(feature = "digital-signature")] {
    /// # use harumi::{Document, SignatureFieldOptions, SigningContext, CertificateInput, PrivateKeyInput};
    /// # fn main() -> harumi::Result<()> {
    /// let mut doc = Document::new((612.0, 792.0))?;
    /// let options = SignatureFieldOptions {
    ///     field_name: "Sig1".into(),
    ///     reason: Some("Approval".into()),
    ///     contact_info: None,
    ///     lock_permissions: false,
    /// };
    /// doc.add_signature_field(1, [50.0, 600.0, 200.0, 50.0], &options)?;
    ///
    /// let cert_bytes = std::fs::read("cert.der")?;
    /// let key_bytes = std::fs::read("key.der")?;
    /// let ctx = SigningContext::from_cert_and_key(
    ///     CertificateInput::Der(cert_bytes),
    ///     PrivateKeyInput::Der(key_bytes),
    /// )?;
    ///
    /// let signed_pdf = doc.sign_document(&ctx, "Sig1")?;
    /// std::fs::write("signed.pdf", signed_pdf)?;
    /// # Ok(())
    /// # }
    /// # }
    /// ```
    #[cfg(feature = "digital-signature")]
    pub fn sign_document(
        &mut self,
        context: &crate::SigningContext,
        field_name: &str,
    ) -> Result<Vec<u8>> {
        use crate::cms_builder::CmsSignedDataBuilder;
        use crate::pdf_incremental::IncrementalUpdateBuilder;
        use crate::signature_create::inner::{hash_pdf_content, sign_hash};

        if self.finalized {
            return Err(Error::InvalidInput(
                "sign_document() called after save()".into(),
            ));
        }

        // Phase 5 Implementation (v1.2.1):
        // 1. Generate base PDF
        let base_pdf = self.save_to_bytes()?;

        // 2. Hash PDF content for signing
        let hash = hash_pdf_content(&base_pdf);

        // 3. Generate RSA signature
        let sig_bytes = sign_hash(context.private_key(), &hash)?;

        // 4. Build PKCS#7/CMS SignedData
        let cms_builder = CmsSignedDataBuilder::new(context.cert_der().to_vec(), sig_bytes, hash);
        let cms_hex = cms_builder.to_hex_string()?;

        // 5. Build PDF incremental update section with metadata
        let incremental_builder = IncrementalUpdateBuilder::new(
            base_pdf,
            field_name.to_string(),
            cms_hex,
            Some(context.signer_name().to_string()),
        );
        let signed_pdf = incremental_builder.build()?;

        Ok(signed_pdf)
    }

    pub fn save_to_bytes(&mut self) -> Result<Vec<u8>> {
        let mut buf = Vec::new();
        self.save_to_writer(&mut buf)?;
        Ok(buf)
    }

    /// Finalizes and writes the document to an arbitrary [`Write`](std::io::Write) sink.
    ///
    /// Useful for writing to an in-memory buffer or a network stream.
    ///
    /// # Example
    /// ```no_run
    /// # use harumi::Document;
    /// # fn main() -> harumi::Result<()> {
    /// let mut doc = Document::from_file("input.pdf")?;
    /// let font = doc.embed_font(include_bytes!("../../tests/fixtures/NotoSansJP-Regular.ttf"))?;
    /// doc.page(1)?.add_invisible_text("検索可能なテキスト", font, [72.0, 700.0], 12.0)?;
    ///
    /// let mut buf = Vec::new();
    /// doc.save_to_writer(&mut buf)?;
    /// // `buf` now contains the complete PDF bytes
    /// # Ok(())
    /// # }
    /// ```
    pub fn save_to_writer(&mut self, writer: &mut impl std::io::Write) -> Result<()> {
        self.finalize()?;
        self.apply_pending_encryption()?;
        self.inner.save_to(writer)?;
        Ok(())
    }

    /// Applies `pending_encryption` to the inner document if set.
    fn apply_pending_encryption(&mut self) -> Result<()> {
        let Some((user_pw, owner_pw, algorithm)) = self.pending_encryption.take() else {
            return Ok(());
        };
        use lopdf::{EncryptionState, EncryptionVersion, Object, Permissions, StringFormat};

        // /ID is required for RC4 key derivation. V5/AES-256 doesn't need it but we set
        // it anyway for PDF spec compliance.
        if self.inner.trailer.get(b"ID").is_err() {
            let id = generate_file_id();
            self.inner.trailer.set(
                "ID",
                Object::Array(vec![
                    Object::String(id.to_vec(), StringFormat::Hexadecimal),
                    Object::String(id.to_vec(), StringFormat::Hexadecimal),
                ]),
            );
        }

        match algorithm {
            EncAlgorithm::Rc4_128 => {
                let version = EncryptionVersion::V2 {
                    document: &self.inner,
                    owner_password: &owner_pw,
                    user_password: &user_pw,
                    key_length: 128,
                    permissions: Permissions::all(),
                };
                let state = EncryptionState::try_from(version)
                    .map_err(|e| Error::InvalidInput(format!("encryption setup: {e}")))?;
                self.inner.encrypt(&state).map_err(Error::Pdf)?;
            }
            EncAlgorithm::Aes256 => {
                use lopdf::encryption::crypt_filters::Aes256CryptFilter;
                use std::collections::BTreeMap;
                use std::sync::Arc;

                // Generate a fresh 32-byte cryptographically secure random key.
                // getrandom::fill() uses the OS RNG (e.g. getrandom(2) / SecRandomCopyBytes).
                // Failure is propagated — we never fall back to a weaker source.
                let mut file_key = [0u8; 32];
                getrandom::fill(&mut file_key).map_err(|e| {
                    Error::InvalidInput(format!("AES-256 key generation failed: {e}"))
                })?;

                let crypt_filter: Arc<dyn lopdf::encryption::crypt_filters::CryptFilter> =
                    Arc::new(Aes256CryptFilter);

                let version = EncryptionVersion::V5 {
                    encrypt_metadata: true,
                    crypt_filters: BTreeMap::from([(b"StdCF".to_vec(), crypt_filter)]),
                    file_encryption_key: &file_key,
                    stream_filter: b"StdCF".to_vec(),
                    string_filter: b"StdCF".to_vec(),
                    owner_password: &owner_pw,
                    user_password: &user_pw,
                    permissions: Permissions::all(),
                };
                let state = EncryptionState::try_from(version)
                    .map_err(|e| Error::InvalidInput(format!("AES-256 encryption setup: {e}")))?;
                self.inner.encrypt(&state).map_err(Error::Pdf)?;
            }
        }
        Ok(())
    }

    /// Subsets fonts, embeds them, and injects all pending content streams.
    fn finalize(&mut self) -> Result<()> {
        if self.finalized && !self.pending.is_empty() {
            return Err(Error::InvalidInput(
                "save() called again after content was already written; create a new Document"
                    .into(),
            ));
        }
        if self.pending.is_empty() && self.pending_bookmarks.is_empty() {
            return Ok(());
        }

        // Pass 1: collect all chars per font across every page (single subset per font).
        let mut font_chars: HashMap<u32, Vec<char>> = HashMap::new();
        for page in &self.pending {
            for op in &page.ops {
                match op {
                    PendingOp::Text(t) => {
                        let chars = font_chars.entry(t.font.0).or_default();
                        chars.extend(t.text.chars());
                    }
                    PendingOp::Replace(r) => {
                        let chars = font_chars.entry(r.font.0).or_default();
                        chars.extend(r.new_text.chars());
                    }
                    PendingOp::ReplacePreserve(_) => {}
                    PendingOp::ReplaceResubset(_) => {} // uses existing PDF font
                    #[cfg(feature = "draw")]
                    PendingOp::Draw(_) => {}
                }
            }
        }
        for chars in font_chars.values_mut() {
            chars.sort_unstable();
            chars.dedup();
        }

        // Pass 2: subset + embed each font once, keep char→GID map alongside.
        struct EmbedState {
            ef: EmbeddedFont,
            char_to_gid: BTreeMap<char, u16>,
            gid_to_advance: BTreeMap<u16, u16>,
            units_per_em: u16,
        }
        let mut embedded: HashMap<u32, EmbedState> = HashMap::new();

        for (&font_idx, chars) in &font_chars {
            let raw = self
                .raw_fonts
                .get(font_idx as usize)
                .ok_or(Error::InvalidFont(font_idx))?;

            let subset = subset_font(&raw.ttf_bytes, chars)?;

            // Use the pre-built char_to_gid from SubsetResult, which correctly maps
            // ALL input chars even when two chars share the same underlying glyph.
            let char_to_gid = subset.char_to_gid.clone();

            let face =
                Face::parse(&raw.ttf_bytes, 0).map_err(|e| Error::FontParse(e.to_string()))?;
            let bb = face.global_bounding_box();
            let upm = face.units_per_em() as f64;
            let scale = |v: i16| -> i32 { (v as f64 * 1000.0 / upm).round() as i32 };

            let font_name = format!("HARUMI+HR{}", font_idx);
            let pdf_name = format!("HR{}", font_idx).into_bytes();

            let saved_gid_to_char = subset.gid_to_char.clone();
            let saved_gid_to_advance = subset.gid_to_advance.clone();
            let params = EmbedParams {
                font_name: &font_name,
                subset_bytes: subset.bytes,
                gid_to_char: subset.gid_to_char,
                gid_to_advance: subset.gid_to_advance,
                units_per_em: subset.units_per_em,
                font_bbox: [
                    scale(bb.x_min),
                    scale(bb.y_min),
                    scale(bb.x_max),
                    scale(bb.y_max),
                ],
                ascent: scale(face.ascender()),
                descent: scale(face.descender()),
                cap_height: scale(face.capital_height().unwrap_or(face.ascender())),
                font_kind: subset.font_kind,
            };

            let type0_id = embed_cid_font(&mut self.inner, params)?;
            let upm = face.units_per_em();

            embedded.insert(
                font_idx,
                EmbedState {
                    ef: EmbeddedFont {
                        type0_id,
                        pdf_name,
                        gid_to_char: saved_gid_to_char,
                        gid_to_advance: saved_gid_to_advance.clone(),
                        units_per_em: upm,
                    },
                    char_to_gid,
                    gid_to_advance: saved_gid_to_advance,
                    units_per_em: upm,
                },
            );
        }

        // Replace pass: rewrite existing content streams for pages with Replace ops.
        {
            // Collect work items (cloned) to avoid borrow conflicts with self.inner.
            struct ReplaceOp {
                old_text: String,
                new_text: String,
                font_idx: u32,
                font_size_override: Option<f32>,
                char_spacing: Option<f32>,
            }
            struct ReplaceWork {
                page_id: ObjectId,
                ops: Vec<ReplaceOp>,
            }
            let work: Vec<ReplaceWork> = self
                .pending
                .iter()
                .filter_map(|page| {
                    let ops: Vec<_> = page
                        .ops
                        .iter()
                        .filter_map(|op| {
                            if let PendingOp::Replace(r) = op {
                                Some(ReplaceOp {
                                    old_text: r.old_text.clone(),
                                    new_text: r.new_text.clone(),
                                    font_idx: r.font.0,
                                    font_size_override: r.font_size_override,
                                    char_spacing: r.char_spacing,
                                })
                            } else {
                                None
                            }
                        })
                        .collect();
                    if ops.is_empty() {
                        None
                    } else {
                        Some(ReplaceWork {
                            page_id: page.page_id,
                            ops,
                        })
                    }
                })
                .collect();

            for item in work {
                // Build resolved replacements from embedded font info.
                let mut resolved: Vec<crate::replace::ResolvedReplacement> = Vec::new();
                for op in &item.ops {
                    let state = embedded
                        .get(&op.font_idx)
                        .ok_or(Error::InvalidFont(op.font_idx))?;
                    resolved.push(crate::replace::ResolvedReplacement {
                        old_text: op.old_text.clone(),
                        new_text: op.new_text.clone(),
                        new_pdf_font_name: state.ef.pdf_name.clone(),
                        char_to_gid: state.char_to_gid.clone(),
                        gid_to_advance: state.gid_to_advance.clone(),
                        units_per_em: state.units_per_em,
                        font_size_override: op.font_size_override,
                        char_spacing: op.char_spacing,
                    });
                }

                // Rewrite page content streams and replace /Contents.
                let (new_content, fonts_used) =
                    crate::replace::rewrite_page_streams(&self.inner, item.page_id, &resolved);
                let new_stream_id = self
                    .inner
                    .add_object(Object::Stream(Stream::new(Dictionary::new(), new_content)));
                self.inner
                    .get_object_mut(item.page_id)?
                    .as_dict_mut()?
                    .set("Contents", Object::Reference(new_stream_id));

                // Only register fonts that were actually used in the rewritten stream.
                // This avoids overwriting existing font entries when no replacement matched.
                let mut registered: std::collections::HashSet<Vec<u8>> =
                    std::collections::HashSet::new();
                for op in &item.ops {
                    let state = embedded
                        .get(&op.font_idx)
                        .ok_or(Error::InvalidFont(op.font_idx))?;
                    if fonts_used.contains(&state.ef.pdf_name)
                        && registered.insert(state.ef.pdf_name.clone())
                    {
                        add_font_to_resources(
                            &mut self.inner,
                            item.page_id,
                            &state.ef.pdf_name,
                            state.ef.type0_id,
                        )?;
                    }
                }

                // Also rewrite Form XObject streams — Chrome/Skia PDFs store all text
                // inside Form XObjects, not in the page content streams above.
                let xobj_rewrites = crate::replace::rewrite_form_xobject_streams(
                    &self.inner,
                    item.page_id,
                    &resolved,
                );
                for (xobj_id, new_xobj_content, xobj_fonts_used) in xobj_rewrites {
                    // Update the XObject stream content (decompressed, no filter).
                    if let Ok(obj) = self.inner.get_object_mut(xobj_id)
                        && let Ok(stream) = obj.as_stream_mut()
                    {
                        stream.dict.remove(b"Filter");
                        stream.dict.remove(b"DecodeParms");
                        stream
                            .dict
                            .set("Length", Object::Integer(new_xobj_content.len() as i64));
                        stream.content = new_xobj_content;
                        stream.allows_compression = false;
                    }
                    // Register new fonts in the XObject's own /Resources/Font.
                    let mut xobj_registered: std::collections::HashSet<Vec<u8>> =
                        std::collections::HashSet::new();
                    for op in &item.ops {
                        let font_idx = op.font_idx;
                        let state = embedded
                            .get(&font_idx)
                            .ok_or(Error::InvalidFont(font_idx))?;
                        if xobj_fonts_used.contains(&state.ef.pdf_name)
                            && xobj_registered.insert(state.ef.pdf_name.clone())
                        {
                            add_font_to_xobject_resources(
                                &mut self.inner,
                                xobj_id,
                                &state.ef.pdf_name,
                                state.ef.type0_id,
                            )?;
                        }
                    }
                }
            }
        }

        // ReplaceResubset pass: expand font subset and re-encode before replacement.
        {
            use crate::resubset::{ResubsetWork, find_fonts_for_text};
            use std::collections::HashMap as HM;

            // Collect (page_id, op) pairs.
            let resubset_items: Vec<(ObjectId, crate::replace::TextReplaceResubsetOp)> = self
                .pending
                .iter()
                .flat_map(|page| {
                    page.ops.iter().filter_map(|op| {
                        if let PendingOp::ReplaceResubset(r) = op {
                            Some((
                                page.page_id,
                                crate::replace::TextReplaceResubsetOp {
                                    old_text: r.old_text.clone(),
                                    new_text: r.new_text.clone(),
                                    font_bytes: r.font_bytes.clone(),
                                    wrap: r.wrap.clone(),
                                },
                            ))
                        } else {
                            None
                        }
                    })
                })
                .collect();

            if !resubset_items.is_empty() {
                // All page IDs in the document (to re-encode pages that use the font).
                let all_page_ids: Vec<ObjectId> = self.inner.page_iter().collect();

                // Group by font_name → ResubsetWork.
                let mut by_font: HM<Vec<u8>, ResubsetWork> = HM::new();
                for (page_id, op) in resubset_items {
                    let font_names = find_fonts_for_text(&self.inner, page_id, &op.old_text);
                    for font_name in font_names {
                        let work =
                            by_font
                                .entry(font_name.clone())
                                .or_insert_with(|| ResubsetWork {
                                    font_name: font_name.clone(),
                                    font_bytes: op.font_bytes.clone(),
                                    replacements: Vec::new(),
                                    wrap_params_by_old_text: HM::new(),
                                });
                        work.replacements
                            .push((page_id, op.old_text.clone(), op.new_text.clone()));
                        // Add wrap params if present.
                        if let Some(wp) = &op.wrap {
                            work.wrap_params_by_old_text
                                .insert(op.old_text.clone(), wp.clone());
                        }
                    }
                }

                for (_, work) in by_font {
                    crate::resubset::resubset_and_replace(&mut self.inner, &work, &all_page_ids)?;
                }
            }
        }

        // ReplacePreserve pass: rewrite streams using the font already embedded in the PDF.
        {
            struct PreserveWork {
                page_id: ObjectId,
                ops: Vec<(String, String)>,
            }
            let work: Vec<PreserveWork> = self
                .pending
                .iter()
                .filter_map(|page| {
                    let ops: Vec<_> = page
                        .ops
                        .iter()
                        .filter_map(|op| {
                            if let PendingOp::ReplacePreserve(r) = op {
                                Some((r.old_text.clone(), r.new_text.clone()))
                            } else {
                                None
                            }
                        })
                        .collect();
                    if ops.is_empty() {
                        None
                    } else {
                        Some(PreserveWork {
                            page_id: page.page_id,
                            ops,
                        })
                    }
                })
                .collect();

            for item in work {
                let preserve_ops: Vec<crate::replace::TextReplacePreserveOp> = item
                    .ops
                    .into_iter()
                    .map(
                        |(old_text, new_text)| crate::replace::TextReplacePreserveOp {
                            old_text,
                            new_text,
                        },
                    )
                    .collect();
                let new_content = crate::replace::rewrite_page_streams_preserve_font(
                    &self.inner,
                    item.page_id,
                    &preserve_ops,
                    None,
                )?;
                let new_stream_id = self
                    .inner
                    .add_object(Object::Stream(Stream::new(Dictionary::new(), new_content)));
                self.inner
                    .get_object_mut(item.page_id)?
                    .as_dict_mut()?
                    .set("Contents", Object::Reference(new_stream_id));
            }
        }

        // Pass 3: build one content stream per page and update /Resources.
        let pending = std::mem::take(&mut self.pending);
        for page in pending {
            let page_id = page.page_id;
            let mut page_stream = Vec::new();

            let mut registered_fonts: Vec<u32> = Vec::new();

            #[cfg(feature = "draw")]
            let mut gs_registry = crate::draw::ExtGStateRegistry::new();
            #[cfg(feature = "image")]
            let mut xobj_entries: Vec<(String, lopdf::ObjectId)> = Vec::new();
            #[cfg(feature = "image")]
            let mut xobj_counter: u32 = 0;

            for op in &page.ops {
                match op {
                    PendingOp::Replace(_) => {}         // handled in replace pass above
                    PendingOp::ReplacePreserve(_) => {} // handled in ReplacePreserve pass above
                    PendingOp::ReplaceResubset(_) => {} // handled in resubset pass above
                    PendingOp::Text(t) => {
                        let state = embedded
                            .get(&t.font.0)
                            .ok_or(Error::InvalidFont(t.font.0))?;
                        let chars: Vec<char> = t.text.chars().collect();
                        #[cfg(feature = "draw")]
                        let gs_opt = if t.opacity < 1.0 {
                            Some(gs_registry.register(t.opacity))
                        } else {
                            None
                        };
                        #[cfg(not(feature = "draw"))]
                        let gs_opt: Option<String> = None;
                        let fragment = text_stream(
                            &state.ef.pdf_name,
                            t.font_size,
                            t.x,
                            t.y,
                            t.rotation_degrees,
                            &chars,
                            &state.char_to_gid,
                            t.render_mode,
                            t.color,
                            gs_opt.as_deref(),
                            t.bold,
                            t.italic,
                            t.char_spacing,
                        );
                        page_stream.extend_from_slice(&fragment);
                        if !registered_fonts.contains(&t.font.0) {
                            registered_fonts.push(t.font.0);
                        }
                    }
                    #[cfg(feature = "draw")]
                    PendingOp::Draw(draw_op) => {
                        use crate::draw::{DrawOp, shapes};
                        match draw_op {
                            DrawOp::Rect {
                                rect,
                                color,
                                opacity,
                            } => {
                                let gs = gs_registry.register(*opacity);
                                page_stream.extend(shapes::rect_stream(rect, *color, &gs));
                            }
                            DrawOp::RectStroke {
                                rect,
                                color,
                                line_width,
                                opacity,
                            } => {
                                let gs = gs_registry.register(*opacity);
                                page_stream.extend(shapes::rect_stroke_stream(
                                    rect,
                                    *color,
                                    *line_width,
                                    &gs,
                                ));
                            }
                            DrawOp::Line {
                                from,
                                to,
                                color,
                                width,
                                opacity,
                            } => {
                                let gs = gs_registry.register(*opacity);
                                page_stream
                                    .extend(shapes::line_stream(from, to, *color, *width, &gs));
                            }
                            DrawOp::Polygon {
                                points,
                                color,
                                opacity,
                                filled,
                                stroke_width,
                            } => {
                                let gs = gs_registry.register(*opacity);
                                page_stream.extend(shapes::polygon_stream(
                                    points,
                                    *color,
                                    &gs,
                                    *filled,
                                    *stroke_width,
                                ));
                            }
                            DrawOp::Polyline {
                                points,
                                color,
                                width,
                                opacity,
                            } => {
                                let gs = gs_registry.register(*opacity);
                                page_stream
                                    .extend(shapes::polyline_stream(points, *color, *width, &gs));
                            }
                            DrawOp::Ellipse {
                                rect,
                                color,
                                opacity,
                                filled,
                                stroke_width,
                            } => {
                                let gs = gs_registry.register(*opacity);
                                page_stream.extend(shapes::ellipse_stream(
                                    rect,
                                    *color,
                                    &gs,
                                    *filled,
                                    *stroke_width,
                                ));
                            }
                            DrawOp::Path {
                                points,
                                closed,
                                color,
                                opacity,
                                filled,
                                stroke_width,
                            } => {
                                let gs = gs_registry.register(*opacity);
                                page_stream.extend(shapes::path_stream(
                                    points,
                                    *closed,
                                    *color,
                                    &gs,
                                    *filled,
                                    *stroke_width,
                                ));
                            }
                            #[cfg(feature = "image")]
                            DrawOp::Image {
                                bytes,
                                rect,
                                opacity,
                            } => {
                                let img = crate::draw::image::prepare(bytes)?;
                                let xobj_id =
                                    crate::draw::image::embed_xobject(&mut self.inner, img)?;
                                let xobj_name = format!("Im{}", xobj_counter);
                                xobj_counter += 1;
                                let gs = gs_registry.register(*opacity);
                                page_stream.extend(crate::draw::image::image_stream(
                                    &xobj_name, rect, &gs,
                                ));
                                xobj_entries.push((xobj_name, xobj_id));
                            }
                        }
                    }
                }
            }

            let new_stream_id = self
                .inner
                .add_object(Object::Stream(Stream::new(Dictionary::new(), page_stream)));
            // Isolate existing page CTM before appending, so any unbalanced `cm` in
            // the existing content doesn't affect the newly appended text/draw stream.
            wrap_page_contents_in_q_q(&mut self.inner, page_id)?;
            append_to_contents(&mut self.inner, page_id, new_stream_id)?;

            for font_idx in registered_fonts {
                let state = embedded
                    .get(&font_idx)
                    .ok_or(Error::InvalidFont(font_idx))?;
                add_font_to_resources(
                    &mut self.inner,
                    page_id,
                    &state.ef.pdf_name,
                    state.ef.type0_id,
                )?;
            }

            #[cfg(feature = "draw")]
            if !gs_registry.is_empty() {
                add_ext_gstate_to_resources(&mut self.inner, page_id, gs_registry)?;
            }

            #[cfg(feature = "image")]
            for (name, obj_id) in xobj_entries {
                add_xobject_to_resources(&mut self.inner, page_id, name.as_bytes(), obj_id)?;
            }
        }

        self.build_outlines_from_bookmarks()?;
        self.finalized = true;
        Ok(())
    }

    /// Builds the PDF `/Outlines` tree from `pending_bookmarks` and links it into the `/Catalog`.
    ///
    /// If the document already has an `/Outlines` tree (e.g. loaded from an existing PDF), the
    /// new items are **appended** to the existing flat list rather than replacing it.
    ///
    /// **Limitation:** `/Count` merging is only accurate for flat (non-nested) outline trees.
    /// For PDFs whose existing outlines have nested children, the count may be imprecise, but
    /// the linked-list structure remains navigable.
    #[allow(clippy::type_complexity)]
    fn build_outlines_from_bookmarks(&mut self) -> Result<()> {
        if self.pending_bookmarks.is_empty() {
            return Ok(());
        }

        let page_ids = self.inner.get_pages();
        let bookmarks = std::mem::take(&mut self.pending_bookmarks);
        let n = bookmarks.len();

        // Check if all items are flat (level == 0): use existing algorithm.
        let all_flat = bookmarks.iter().all(|b| b.level == 0);
        if all_flat {
            return self.build_outlines_flat(page_ids, bookmarks, n);
        }

        // Hierarchical outline: normalize levels to 1..=6 if any item has level 0 mixed in.
        let levels: Vec<u8> = bookmarks.iter().map(|b| b.level.max(1)).collect();

        // Pre-allocate object IDs for all items.
        let item_ids: Vec<ObjectId> = (0..n).map(|_| self.inner.new_object_id()).collect();

        // Get or create root outline object.
        let root_ref = self.inner.trailer.get(b"Root")?.as_reference()?;
        let existing_outline_id: Option<ObjectId> = self
            .inner
            .get_object(root_ref)?
            .as_dict()?
            .get(b"Outlines")
            .and_then(|o| o.as_reference())
            .ok();
        let outline_root_id = existing_outline_id.unwrap_or_else(|| self.inner.new_object_id());

        // Build outline item objects with parent/sibling links.
        // Stack: (level, item_idx, first_child_idx, last_child_idx, child_count)
        let mut stack: Vec<(u8, usize, Option<usize>, Option<usize>, u32)> =
            vec![(0, usize::MAX, None, None, 0)];

        for i in 0..n {
            let curr_level = levels[i];

            // Pop items deeper than current level, closing their children.
            while stack.len() > 1 && stack.last().unwrap().0 >= curr_level {
                let (_, popped_idx, first_child, last_child, child_count) = stack.pop().unwrap();
                if popped_idx != usize::MAX && child_count > 0 {
                    let popped_dict = self
                        .inner
                        .get_object_mut(item_ids[popped_idx])?
                        .as_dict_mut()?;
                    if let Some(first) = first_child {
                        popped_dict.set("First", Object::Reference(item_ids[first]));
                    }
                    if let Some(last) = last_child {
                        popped_dict.set("Last", Object::Reference(item_ids[last]));
                    }
                    popped_dict.set("Count", Object::Integer(child_count as i64));
                }
            }

            let page_id = page_ids
                .get(&bookmarks[i].page)
                .copied()
                .ok_or(Error::PageNotFound(bookmarks[i].page))?;

            let mut d = Dictionary::new();
            d.set("Title", pdf_text_string(&bookmarks[i].title));
            d.set(
                "Dest",
                Object::Array(vec![
                    Object::Reference(page_id),
                    Object::Name(b"XYZ".to_vec()),
                    Object::Null,
                    Object::Real(bookmarks[i].y),
                    Object::Null,
                ]),
            );

            // Determine parent: the top of the stack.
            let parent_idx = if stack.last().unwrap().0 == 0 {
                outline_root_id // Root outline object.
            } else {
                item_ids[stack.last().unwrap().1]
            };
            d.set("Parent", Object::Reference(parent_idx));

            // Sibling links: link to previous item at same level.
            if i > 0 && levels[i - 1] == curr_level {
                d.set("Prev", Object::Reference(item_ids[i - 1]));
                self.inner
                    .get_object_mut(item_ids[i - 1])?
                    .as_dict_mut()?
                    .set("Next", Object::Reference(item_ids[i]));
            }

            self.inner
                .objects
                .insert(item_ids[i], Object::Dictionary(d));

            // Update parent on the stack: record this as a child.
            let top = stack.last_mut().unwrap();
            if top.2.is_none() {
                top.2 = Some(i);
            }
            top.3 = Some(i);
            top.4 += 1;

            // Push a new stack frame for the current item (may have children).
            stack.push((curr_level, i, None, None, 0));
        }

        // Close any remaining open items on the stack.
        while stack.len() > 1 {
            let (_, popped_idx, first_child, last_child, child_count) = stack.pop().unwrap();
            if popped_idx != usize::MAX && child_count > 0 {
                let popped_dict = self
                    .inner
                    .get_object_mut(item_ids[popped_idx])?
                    .as_dict_mut()?;
                if let Some(first) = first_child {
                    popped_dict.set("First", Object::Reference(item_ids[first]));
                }
                if let Some(last) = last_child {
                    popped_dict.set("Last", Object::Reference(item_ids[last]));
                }
                popped_dict.set("Count", Object::Integer(child_count as i64));
            }
        }

        // Write /Outlines root. Collect top-level items to link as root's direct children.
        let top_level_items: Vec<usize> = (0..n).filter(|i| levels[*i] == 1).collect();
        if !top_level_items.is_empty() {
            let first_top = top_level_items[0];
            let last_top = top_level_items[top_level_items.len() - 1];

            // The root /Count includes ALL items (both top-level and nested).
            // Per PDF spec, this is the number of outline items and their descendants visible when expanded.
            let new_count = n as i64;

            if let Some(oid) = existing_outline_id {
                let root_d = self.inner.get_object_mut(oid)?.as_dict_mut()?;
                root_d.set("First", Object::Reference(item_ids[first_top]));
                root_d.set("Last", Object::Reference(item_ids[last_top]));
                let existing_count = root_d.get(b"Count").and_then(|o| o.as_i64()).unwrap_or(0);
                root_d.set("Count", Object::Integer(existing_count + new_count));
            } else {
                let mut root_dict = Dictionary::new();
                root_dict.set("Type", Object::Name(b"Outlines".to_vec()));
                root_dict.set("First", Object::Reference(item_ids[first_top]));
                root_dict.set("Last", Object::Reference(item_ids[last_top]));
                root_dict.set("Count", Object::Integer(new_count));
                self.inner
                    .objects
                    .insert(outline_root_id, Object::Dictionary(root_dict));

                let catalog = self.inner.get_object_mut(root_ref)?.as_dict_mut()?;
                catalog.set("Outlines", Object::Reference(outline_root_id));
            }
        }

        Ok(())
    }

    /// Build a flat outline tree (backward compatibility when all levels == 0).
    fn build_outlines_flat(
        &mut self,
        page_ids: std::collections::BTreeMap<u32, ObjectId>,
        bookmarks: Vec<PendingBookmark>,
        n: usize,
    ) -> Result<()> {
        let item_ids: Vec<ObjectId> = (0..n).map(|_| self.inner.new_object_id()).collect();

        let root_ref = self.inner.trailer.get(b"Root")?.as_reference()?;
        let existing_outline_id: Option<ObjectId> = self
            .inner
            .get_object(root_ref)?
            .as_dict()?
            .get(b"Outlines")
            .and_then(|o| o.as_reference())
            .ok();

        let (outline_root_id, prev_last_opt, existing_count) = match existing_outline_id {
            Some(oid) => {
                let root_d = self.inner.get_object(oid)?.as_dict()?;
                let last_id = root_d.get(b"Last")?.as_reference().ok();
                let count = root_d.get(b"Count").and_then(|o| o.as_i64()).unwrap_or(0);
                (oid, last_id, count)
            }
            None => (self.inner.new_object_id(), None, 0),
        };

        for (i, bm) in bookmarks.iter().enumerate() {
            let page_id = page_ids
                .get(&bm.page)
                .copied()
                .ok_or(Error::PageNotFound(bm.page))?;

            let mut d = Dictionary::new();
            d.set("Title", pdf_text_string(&bm.title));
            d.set(
                "Dest",
                Object::Array(vec![
                    Object::Reference(page_id),
                    Object::Name(b"XYZ".to_vec()),
                    Object::Null,
                    Object::Real(bm.y),
                    Object::Null,
                ]),
            );
            d.set("Parent", Object::Reference(outline_root_id));

            let prev_id = if i == 0 {
                prev_last_opt
            } else {
                Some(item_ids[i - 1])
            };
            if let Some(pid) = prev_id {
                d.set("Prev", Object::Reference(pid));
            }
            if i + 1 < n {
                d.set("Next", Object::Reference(item_ids[i + 1]));
            }
            self.inner
                .objects
                .insert(item_ids[i], Object::Dictionary(d));
        }

        let new_total = existing_count + n as i64;

        if let Some(oid) = existing_outline_id {
            if let Some(old_last) = prev_last_opt {
                self.inner
                    .get_object_mut(old_last)?
                    .as_dict_mut()?
                    .set("Next", Object::Reference(item_ids[0]));
            }
            let root_d = self.inner.get_object_mut(oid)?.as_dict_mut()?;
            root_d.set("Last", Object::Reference(item_ids[n - 1]));
            root_d.set("Count", Object::Integer(new_total));
        } else {
            let mut root_dict = Dictionary::new();
            root_dict.set("Type", Object::Name(b"Outlines".to_vec()));
            root_dict.set("First", Object::Reference(item_ids[0]));
            root_dict.set("Last", Object::Reference(item_ids[n - 1]));
            root_dict.set("Count", Object::Integer(new_total));
            self.inner
                .objects
                .insert(outline_root_id, Object::Dictionary(root_dict));

            let catalog = self.inner.get_object_mut(root_ref)?.as_dict_mut()?;
            catalog.set("Outlines", Object::Reference(outline_root_id));
        }

        Ok(())
    }

    /// Atomically replaces multiple text pairs on a single page, preserving the existing font.
    ///
    /// Validates all text pairs before queuing any replacements. If any pair fails
    /// validation, **no replacements are queued** and an error is returned. This
    /// atomic behavior is more robust than manually calling
    /// [`PageHandle::replace_text_preserve_font`] in a loop, which would queue
    /// replacements incrementally.
    ///
    /// Returns the total number of text matches found across all pairs.
    ///
    /// # Performance Note
    /// This method scans the page content twice: once during validation, once during
    /// replacement. This two-phase approach ensures atomicity (all-or-nothing semantics)
    /// but at the cost of redundant parsing. For single replacements, prefer
    /// [`PageHandle::replace_text_preserve_font`] directly.
    ///
    /// # Arguments
    /// * `page` — 1-indexed page number
    /// * `pairs` — slice of `(&str, &str)` tuples where the first string is the
    ///   text to find and the second is the replacement text
    ///
    /// # Errors
    /// Returns [`Error::PageNotFound`] if the page does not exist, or
    /// [`Error::FontCharNotMapped`] if any replacement text contains characters
    /// absent from the page's font. **No replacements are queued if validation fails.**
    ///
    /// # Example
    /// ```no_run
    /// # use harumi::Document;
    /// # fn main() -> harumi::Result<()> {
    /// # let mut doc = Document::from_file("example.pdf")?;
    /// let pairs = vec![
    ///     ("Hello", "こんにちは"),
    ///     ("World", "世界"),
    /// ];
    /// let count = doc.batch_replace_text_preserve_font(1, &pairs)?;
    /// println!("Replaced {} total matches", count);
    /// # Ok(())
    /// # }
    /// ```
    pub fn batch_replace_text_preserve_font(
        &mut self,
        page: u32,
        pairs: &[(&str, &str)],
    ) -> Result<usize> {
        // SECURITY: Check finalized state before any operations.
        if self.finalized {
            return Err(Error::InvalidInput(
                "batch_replace_text_preserve_font called after save()".into(),
            ));
        }

        // Validate page exists once, upfront.
        let page_id = self
            .inner
            .get_pages()
            .get(&page)
            .copied()
            .ok_or(Error::PageNotFound(page))?;

        // Validation phase: check all pairs before queuing any.
        // Borrow self.inner immutably to validate.
        let mut pair_counts = Vec::with_capacity(pairs.len());
        let mut seen_old_texts = std::collections::HashSet::new();

        for &(old_text, new_text) in pairs {
            // SECURITY: Prevent double-counting with duplicate old_text.
            // Each search text can only be replaced once per batch.
            if !seen_old_texts.insert(old_text) {
                return Err(Error::InvalidInput(format!(
                    "duplicate search text in batch: '{}'",
                    old_text
                )));
            }

            let count = crate::replace::count_matches_in_page(
                &self.inner,
                page_id,
                old_text,
                Some(new_text),
            )?;
            pair_counts.push(count);
        }

        // Execution phase: all pairs are validated; now queue replacements.
        // Release the immutable borrow of self.inner before acquiring mutable borrow of page_handle.
        let mut page_handle = self.page(page)?;
        let mut total_count: usize = 0;

        for (count, &(old_text, new_text)) in pair_counts.iter().zip(pairs) {
            page_handle.replace_text_preserve_font(old_text, new_text)?;
            total_count = total_count.saturating_add(*count);
        }

        Ok(total_count)
    }
}
