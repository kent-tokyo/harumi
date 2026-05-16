use std::{
    collections::{BTreeMap, HashMap, HashSet},
    path::Path,
};

use lopdf::{Dictionary, Object, ObjectId, Stream};
use ttf_parser::Face;

use crate::{
    content::text::text_stream,
    error::{Error, Result},
    font::{
        embed::{embed_cid_font, EmbedParams},
        subset::subset_font,
        EmbeddedFont, FontHandle,
    },
};

/// A single text placement descriptor for use with [`PageHandle::add_invisible_text_runs`].
///
/// # Example
/// ```no_run
/// # use harumi::{Document, TextRun};
/// # fn main() -> harumi::Result<()> {
/// # let mut doc = Document::from_bytes(&[])?;
/// # let font = doc.embed_font(&[])?;
/// doc.page(1)?.add_invisible_text_runs(&[
///     TextRun { text: "first line".into(), font, x: 72.0, y: 700.0, font_size: 12.0, render_mode: 3, color: [0.0; 3] },
///     TextRun { text: "second line".into(), font, x: 72.0, y: 685.0, font_size: 12.0, render_mode: 3, color: [0.0; 3] },
/// ])?;
/// # Ok(())
/// # }
/// ```
pub struct TextRun {
    /// The text to place.
    pub text: String,
    /// Font to use (obtained from [`Document::embed_font`]).
    pub font: FontHandle,
    /// X coordinate in PDF points (origin: bottom-left of page).
    pub x: f32,
    /// Y coordinate in PDF points (origin: bottom-left of page).
    pub y: f32,
    /// Font size in PDF points.
    pub font_size: f32,
    /// RGB fill color; each component in `0.0..=1.0`. Only applied when `render_mode == 0`.
    pub color: [f32; 3],
    /// PDF text render mode. `0` = visible, `3` = invisible (OCR search layer).
    pub render_mode: u8,
}

/// A pending text placement, stored until `save()` finalizes the document.
#[allow(dead_code)] // `opacity` is read only under the `draw` feature
struct PendingText {
    font: FontHandle,
    text: String,
    x: f32,
    y: f32,
    font_size: f32,
    render_mode: u8,
    color: [f32; 3],
    opacity: f32,
}

/// A pending operation on a page (text or drawing primitive).
enum PendingOp {
    Text(PendingText),
    #[cfg(feature = "draw")]
    Draw(crate::draw::DrawOp),
}

/// Per-page pending operations.
struct PendingPage {
    page_id: ObjectId,
    ops: Vec<PendingOp>,
}

/// PDF /Info dictionary fields.
///
/// Used with [`Document::metadata`] and [`Document::set_metadata`].
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PdfMetadata {
    pub title: Option<String>,
    pub author: Option<String>,
    pub subject: Option<String>,
    pub keywords: Option<String>,
    pub creator: Option<String>,
}

/// Raw font data stored before subsetting.
struct RawFont {
    ttf_bytes: Vec<u8>,
}

/// An existing PDF document that can be annotated with text overlays.
///
/// Load a document with [`Document::from_file`] or [`Document::from_bytes`],
/// add text with [`page`](Document::page), then write the result with
/// [`save`](Document::save).
///
/// # Deferred subsetting
///
/// [`embed_font`](Document::embed_font) is cheap — it only stores the raw TTF
/// bytes. At [`save`](Document::save) time, harumi collects all characters
/// used across every page and subsets each font exactly once.
pub struct Document {
    pub(crate) inner: lopdf::Document,
    raw_fonts: Vec<RawFont>,
    pending: Vec<PendingPage>,
    /// Set to true after the first successful `finalize()`. Prevents silent corruption
    /// when new ops are queued after a `save()` call (font subsets would mismatch).
    finalized: bool,
}

fn lopdf_string_to_rust(obj: &lopdf::Object) -> Option<String> {
    match obj {
        lopdf::Object::String(bytes, _) => {
            if bytes.starts_with(&[0xFE, 0xFF]) {
                let units: Vec<u16> = bytes[2..]
                    .chunks(2)
                    .map(|c| u16::from_be_bytes([c[0], c.get(1).copied().unwrap_or(0)]))
                    .collect();
                String::from_utf16(&units).ok()
            } else {
                String::from_utf8(bytes.clone())
                    .ok()
                    .or_else(|| Some(bytes.iter().map(|&b| b as char).collect()))
            }
        }
        _ => None,
    }
}

impl Document {
    /// Loads a PDF from a file path.
    ///
    /// # Errors
    /// Returns [`Error::Io`] if the file cannot be read, or [`Error::Pdf`] if
    /// the file is not a valid PDF.
    pub fn from_file(path: impl AsRef<Path>) -> Result<Self> {
        let inner = lopdf::Document::load(path)?;
        Ok(Self { inner, raw_fonts: Vec::new(), pending: Vec::new(), finalized: false })
    }

    /// Loads a PDF from an in-memory byte slice.
    ///
    /// # Errors
    /// Returns [`Error::Pdf`] if the bytes do not represent a valid PDF.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        let inner = lopdf::Document::load_from(bytes)?;
        Ok(Self { inner, raw_fonts: Vec::new(), pending: Vec::new(), finalized: false })
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
                "page size must be positive, got ({}, {})", size.0, size.1
            )));
        }

        let mut inner = lopdf::Document::with_version("1.4");

        let pages_id = inner.new_object_id();
        let page_id = inner.new_object_id();

        inner.objects.insert(page_id, Object::Dictionary({
            let mut d = Dictionary::new();
            d.set("Type", Object::Name(b"Page".to_vec()));
            d.set("Parent", Object::Reference(pages_id));
            d.set("MediaBox", Object::Array(vec![
                Object::Integer(0),
                Object::Integer(0),
                Object::Real(size.0),
                Object::Real(size.1),
            ]));
            d.set("Resources", Object::Dictionary(Dictionary::new()));
            d
        }));

        inner.objects.insert(pages_id, Object::Dictionary({
            let mut d = Dictionary::new();
            d.set("Type", Object::Name(b"Pages".to_vec()));
            d.set("Kids", Object::Array(vec![Object::Reference(page_id)]));
            d.set("Count", Object::Integer(1));
            d
        }));

        let catalog_id = inner.add_object(Object::Dictionary({
            let mut d = Dictionary::new();
            d.set("Type", Object::Name(b"Catalog".to_vec()));
            d.set("Pages", Object::Reference(pages_id));
            d
        }));

        inner.trailer.set("Root", Object::Reference(catalog_id));

        Ok(Self { inner, raw_fonts: Vec::new(), pending: Vec::new(), finalized: false })
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
        let face = Face::parse(ttf_bytes, 0)
            .map_err(|e| Error::FontParse(e.to_string()))?;
        if face.units_per_em() == 0 {
            return Err(Error::FontParse("font units_per_em is 0".into()));
        }
        let idx = self.raw_fonts.len() as u32;
        self.raw_fonts.push(RawFont { ttf_bytes: ttf_bytes.to_vec() });
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
        let page_id = page_ids.get(&number).copied().ok_or(Error::PageNotFound(number))?;
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
        let target_id = page_ids.get(&number).copied().ok_or(Error::PageNotFound(number))?;
        if page_ids.len() <= 1 {
            return Err(Error::InvalidInput("cannot remove the only page".into()));
        }

        let all_ids: Vec<ObjectId> = page_ids.into_values().filter(|&id| id != target_id).collect();

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
                "page size must be positive, got ({}, {})", size.0, size.1
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
        let combined: Vec<ObjectId> =
            self_page_ids.into_iter().chain(other_page_ids).collect();
        let count = combined.len();
        let new_kids: Vec<Object> = combined.into_iter().map(Object::Reference).collect();
        let pages_dict = self.inner.get_object_mut(pages_root)?.as_dict_mut()?;
        pages_dict.set("Kids", Object::Array(new_kids));
        pages_dict.set("Count", Object::Integer(count as i64));

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

        Ok(Document { inner: new_inner, raw_fonts: Vec::new(), pending: Vec::new(), finalized: false })
    }

    /// Returns the document's `/Info` metadata fields.
    ///
    /// All fields are `None` if the document has no `/Info` dictionary.
    pub fn metadata(&self) -> Result<PdfMetadata> {
        use lopdf::Object;

        let info_dict: Option<lopdf::Dictionary> =
            match self.inner.trailer.get(b"Info").ok() {
                Some(Object::Reference(id)) => self
                    .inner
                    .get_object(*id)
                    .ok()
                    .and_then(|o| {
                        if let Object::Dictionary(d) = o { Some(d.clone()) } else { None }
                    }),
                Some(Object::Dictionary(d)) => Some(d.clone()),
                _ => None,
            };

        let field = |d: &lopdf::Dictionary, key: &[u8]| {
            d.get(key).ok().and_then(lopdf_string_to_rust)
        };

        Ok(match info_dict {
            Some(d) => PdfMetadata {
                title:    field(&d, b"Title"),
                author:   field(&d, b"Author"),
                subject:  field(&d, b"Subject"),
                keywords: field(&d, b"Keywords"),
                creator:  field(&d, b"Creator"),
            },
            None => PdfMetadata::default(),
        })
    }

    /// Writes (or replaces) the document's `/Info` metadata dictionary.
    ///
    /// Only `Some` fields are written; `None` fields are omitted.
    /// Can be called before or after adding text/shapes — metadata is independent of font subsetting.
    pub fn set_metadata(&mut self, meta: &PdfMetadata) -> Result<()> {
        use lopdf::{Object, StringFormat};

        let mut dict = lopdf::Dictionary::new();
        let mut set = |key: &[u8], val: &Option<String>| {
            if let Some(s) = val {
                dict.set(key, Object::String(s.as_bytes().to_vec(), StringFormat::Literal));
            }
        };
        set(b"Title",    &meta.title);
        set(b"Author",   &meta.author);
        set(b"Subject",  &meta.subject);
        set(b"Keywords", &meta.keywords);
        set(b"Creator",  &meta.creator);

        let info_id = self.inner.add_object(Object::Dictionary(dict));
        self.inner.trailer.set("Info", Object::Reference(info_id));
        Ok(())
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
    /// # Errors
    /// Returns [`Error::PageNotFound`] if `page` is out of range.
    pub fn extract_text_runs(&self, page: u32) -> Result<Vec<crate::extract::TextFragment>> {
        let all_pages = self.inner.get_pages();
        let page_id = all_pages.get(&page).copied().ok_or(Error::PageNotFound(page))?;
        crate::extract::extract_text_runs_from_page(&self.inner, page_id)
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
    /// let font = doc.embed_font(include_bytes!("../tests/fixtures/NotoSansJP-Regular.ttf"))?;
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
        self.inner.save_to(writer)?;
        Ok(())
    }

    /// Subsets fonts, embeds them, and injects all pending content streams.
    fn finalize(&mut self) -> Result<()> {
        if self.finalized && !self.pending.is_empty() {
            return Err(Error::InvalidInput(
                "save() called again after content was already written; create a new Document".into(),
            ));
        }
        if self.pending.is_empty() {
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
        }
        let mut embedded: HashMap<u32, EmbedState> = HashMap::new();

        for (&font_idx, chars) in &font_chars {
            let raw = self.raw_fonts.get(font_idx as usize)
                .ok_or(Error::InvalidFont(font_idx))?;

            let subset = subset_font(&raw.ttf_bytes, chars)?;

            let char_to_gid: BTreeMap<char, u16> = subset.gid_to_char
                .iter()
                .map(|(&gid, &ch)| (ch, gid))
                .collect();

            let face = Face::parse(&raw.ttf_bytes, 0)
                .map_err(|e| Error::FontParse(e.to_string()))?;
            let bb = face.global_bounding_box();
            let upm = face.units_per_em() as f64;
            let scale = |v: i16| -> i32 { (v as f64 * 1000.0 / upm).round() as i32 };

            let font_name = format!("HARUMI+F{}", font_idx);
            let pdf_name = format!("F{}", font_idx).into_bytes();

            let params = EmbedParams {
                font_name: &font_name,
                subset_bytes: subset.bytes,
                gid_to_char: subset.gid_to_char,
                gid_to_advance: subset.gid_to_advance,
                units_per_em: subset.units_per_em,
                font_bbox: [
                    scale(bb.x_min), scale(bb.y_min),
                    scale(bb.x_max), scale(bb.y_max),
                ],
                ascent: scale(face.ascender()),
                descent: scale(face.descender()),
                cap_height: scale(face.capital_height().unwrap_or(face.ascender())),
                font_kind: subset.font_kind,
            };

            let type0_id = embed_cid_font(&mut self.inner, params)?;

            embedded.insert(font_idx, EmbedState {
                ef: EmbeddedFont {
                    type0_id,
                    pdf_name,
                    gid_to_char: BTreeMap::new(),
                    gid_to_advance: BTreeMap::new(),
                    units_per_em: face.units_per_em(),
                },
                char_to_gid,
            });
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
                    PendingOp::Text(t) => {
                        let state = embedded.get(&t.font.0).ok_or(Error::InvalidFont(t.font.0))?;
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
                            &chars,
                            &state.char_to_gid,
                            t.render_mode,
                            t.color,
                            gs_opt.as_deref(),
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
                            DrawOp::Rect { rect, color, opacity } => {
                                let gs = gs_registry.register(*opacity);
                                page_stream.extend(shapes::rect_stream(rect, color, &gs));
                            }
                            DrawOp::RectStroke { rect, color, line_width, opacity } => {
                                let gs = gs_registry.register(*opacity);
                                page_stream.extend(shapes::rect_stroke_stream(rect, color, *line_width, &gs));
                            }
                            DrawOp::Line { from, to, color, width, opacity } => {
                                let gs = gs_registry.register(*opacity);
                                page_stream.extend(shapes::line_stream(from, to, color, *width, &gs));
                            }
                            DrawOp::Polygon { points, color, opacity, filled } => {
                                let gs = gs_registry.register(*opacity);
                                page_stream.extend(shapes::polygon_stream(points, color, &gs, *filled));
                            }
                            DrawOp::Polyline { points, color, width, opacity } => {
                                let gs = gs_registry.register(*opacity);
                                page_stream.extend(shapes::polyline_stream(points, color, *width, &gs));
                            }
                            #[cfg(feature = "image")]
                            DrawOp::Image { bytes, rect, opacity } => {
                                let img = crate::draw::image::prepare(bytes)?;
                                let xobj_id = crate::draw::image::embed_xobject(&mut self.inner, img)?;
                                let xobj_name = format!("Im{}", xobj_counter);
                                xobj_counter += 1;
                                let gs = gs_registry.register(*opacity);
                                page_stream.extend(crate::draw::image::image_stream(&xobj_name, rect, &gs));
                                xobj_entries.push((xobj_name, xobj_id));
                            }
                        }
                    }
                }
            }

            let new_stream_id = self.inner.add_object(Object::Stream(
                Stream::new(Dictionary::new(), page_stream),
            ));
            append_to_contents(&mut self.inner, page_id, new_stream_id)?;

            for font_idx in registered_fonts {
                let state = embedded.get(&font_idx).ok_or(Error::InvalidFont(font_idx))?;
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

        self.finalized = true;
        Ok(())
    }
}

/// Vertical alignment for [`PageHandle::add_text_box_aligned`].
pub enum VerticalAlign {
    /// Text starts at the top of the box (default).
    Top,
    /// Text block is centered vertically in the box.
    Center,
    /// Text block ends at the bottom of the box.
    Bottom,
}

/// A handle to a specific page for queuing text overlays.
///
/// Obtained via [`Document::page`]. All queued operations are written to the
/// PDF during [`Document::save`].
pub struct PageHandle<'doc> {
    doc: &'doc mut Document,
    page_id: ObjectId,
}

impl<'doc> PageHandle<'doc> {
    /// Queues a single invisible text placement on this page.
    ///
    /// The text is rendered with PDF render mode 3 (`Tr 3`): it is not painted
    /// on screen but is fully selectable and searchable. This is the standard
    /// approach for OCR text layers.
    ///
    /// `position` is `[x, y]` in PDF points (origin: bottom-left of page).
    ///
    /// # Example
    /// ```no_run
    /// # use harumi::Document;
    /// # fn main() -> harumi::Result<()> {
    /// # let mut doc = Document::from_bytes(&[])?;
    /// # let font = doc.embed_font(&[])?;
    /// doc.page(1)?.add_invisible_text("検索可能なテキスト", font, [72.0, 700.0], 12.0)?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn add_invisible_text(
        &mut self,
        text: &str,
        font: FontHandle,
        position: [f32; 2],
        font_size: f32,
    ) -> Result<()> {
        check_finite(&[position[0], position[1], font_size], "add_invisible_text")?;
        self.push_text(PendingText {
            font,
            text: text.to_owned(),
            x: position[0],
            y: position[1],
            font_size,
            render_mode: 3,
            color: [0.0; 3],
            opacity: 1.0,
        });
        Ok(())
    }

    /// Queues a visible text placement with the given RGB color.
    ///
    /// The text is rendered with PDF render mode 0 (`Tr 0`): filled with the
    /// specified color. Use this for watermarks, stamps, or any annotation that
    /// should be visible in the PDF.
    ///
    /// `position` is `[x, y]` in PDF points (origin: bottom-left of page).
    /// `color` is `[r, g, b]` where each component is in `0.0..=1.0`.
    ///
    /// # Example
    /// ```no_run
    /// # use harumi::Document;
    /// # fn main() -> harumi::Result<()> {
    /// # let mut doc = Document::from_bytes(&[])?;
    /// # let font = doc.embed_font(&[])?;
    /// // Red "CONFIDENTIAL" stamp in the center of the page
    /// let (w, h) = doc.page(1)?.size()?;
    /// doc.page(1)?.add_text("CONFIDENTIAL", font, [w / 2.0 - 60.0, h / 2.0], 24.0, [0.8, 0.0, 0.0])?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn add_text(
        &mut self,
        text: &str,
        font: FontHandle,
        position: [f32; 2],
        font_size: f32,
        color: [f32; 3],
    ) -> Result<()> {
        check_finite(&[position[0], position[1], font_size, color[0], color[1], color[2]], "add_text")?;
        self.push_text(PendingText {
            font,
            text: text.to_owned(),
            x: position[0],
            y: position[1],
            font_size,
            render_mode: 0,
            color,
            opacity: 1.0,
        });
        Ok(())
    }

    /// Queues multiple text placements in one call.
    ///
    /// All runs across the entire document are collected before subsetting,
    /// so each font is subsetted exactly once regardless of how many runs use it.
    pub fn add_invisible_text_runs(&mut self, runs: &[TextRun]) -> Result<()> {
        for run in runs {
            check_finite(&[run.x, run.y, run.font_size, run.color[0], run.color[1], run.color[2]], "add_invisible_text_runs")?;
            self.push_text(PendingText {
                font: run.font,
                text: run.text.clone(),
                x: run.x,
                y: run.y,
                font_size: run.font_size,
                render_mode: run.render_mode,
                color: run.color,
                opacity: 1.0,
            });
        }
        Ok(())
    }

    /// Overlays multi-line visible text within a bounding box.
    ///
    /// `rect` is `[x, y, width, height]` in PDF points (origin: bottom-left).
    /// Text wraps at word boundaries for Latin text, or at any character for CJK.
    /// Lines outside the box bounds are silently clipped.
    /// `line_height` sets the vertical distance between baselines; pass `0.0` to use
    /// `font_size * 1.2`.
    ///
    /// Equivalent to `add_text_box_aligned(..., VerticalAlign::Top)`.
    ///
    /// # Example
    /// ```no_run
    /// # use harumi::Document;
    /// # fn main() -> harumi::Result<()> {
    /// # let mut doc = Document::from_bytes(&[])?;
    /// # let font = doc.embed_font(&[])?;
    /// // Fill a 300pt-wide column with black text at 11pt, auto line-height
    /// doc.page(1)?.add_text_box(
    ///     "This is a long sentence that will wrap automatically.",
    ///     font,
    ///     [72.0, 400.0, 300.0, 200.0],
    ///     11.0,
    ///     [0.0, 0.0, 0.0],
    ///     0.0,
    /// )?;
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// # Errors
    /// Returns [`Error::InvalidFont`] if `font` is not registered on this document,
    /// or [`Error::FontParse`] if the font bytes cannot be parsed.
    pub fn add_text_box(
        &mut self,
        text: &str,
        font: FontHandle,
        rect: [f32; 4],
        font_size: f32,
        color: [f32; 3],
        line_height: f32,
    ) -> Result<()> {
        self.add_text_box_aligned(text, font, rect, font_size, color, line_height, VerticalAlign::Top)
    }

    /// Overlays multi-line visible text within a bounding box with explicit vertical alignment.
    ///
    /// `rect` is `[x, y, width, height]` in PDF points (origin: bottom-left).
    /// Text wraps at word boundaries for Latin text, or at any character for CJK.
    /// Lines outside the box bounds are silently clipped (top and bottom).
    /// `line_height` sets the vertical distance between baselines; pass `0.0` to use
    /// `font_size * 1.2`.
    ///
    /// # Example
    /// ```no_run
    /// # use harumi::{Document, VerticalAlign};
    /// # fn main() -> harumi::Result<()> {
    /// # let mut doc = Document::from_bytes(&[])?;
    /// # let font = doc.embed_font(&[])?;
    /// // Vertically center a label inside a 100pt-tall cell
    /// doc.page(1)?.add_text_box_aligned(
    ///     "Centered",
    ///     font,
    ///     [72.0, 350.0, 200.0, 100.0],
    ///     12.0,
    ///     [0.0, 0.0, 0.0],
    ///     0.0,
    ///     VerticalAlign::Center,
    /// )?;
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// # Errors
    /// Returns [`Error::InvalidFont`] if `font` is not registered on this document,
    /// or [`Error::FontParse`] if the font bytes cannot be parsed.
    #[allow(clippy::too_many_arguments)]
    pub fn add_text_box_aligned(
        &mut self,
        text: &str,
        font: FontHandle,
        rect: [f32; 4],
        font_size: f32,
        color: [f32; 3],
        line_height: f32,
        align: VerticalAlign,
    ) -> Result<()> {
        check_finite(&[rect[0], rect[1], rect[2], rect[3], font_size, color[0], color[1], color[2], line_height], "add_text_box_aligned")?;
        if rect[2] <= 0.0 || rect[3] <= 0.0 {
            return Ok(());
        }

        let raw = self.doc.raw_fonts.get(font.0 as usize)
            .ok_or(Error::InvalidFont(font.0))?;
        let face = Face::parse(&raw.ttf_bytes, 0)
            .map_err(|e| Error::FontParse(e.to_string()))?;

        let box_width = rect[2];
        let effective_lh = if line_height <= 0.0 { font_size * 1.2 } else { line_height };

        let mut all_lines: Vec<String> = Vec::new();
        for paragraph in text.split('\n') {
            all_lines.extend(wrap_paragraph(paragraph, &face, font_size, box_width));
        }

        let n = all_lines.len() as f32;
        let start_y = match align {
            VerticalAlign::Top    => rect[1] + rect[3] - font_size,
            VerticalAlign::Bottom => rect[1] + (n - 1.0) * effective_lh,
            VerticalAlign::Center => rect[1] + rect[3] / 2.0 + ((n - 1.0) * effective_lh - font_size) / 2.0,
        };
        let top = rect[1] + rect[3];
        let bottom = rect[1];

        for (i, line) in all_lines.iter().enumerate() {
            let y = start_y - i as f32 * effective_lh;
            if y > top || y < bottom {
                continue;
            }
            self.push_text(PendingText {
                font,
                text: line.clone(),
                x: rect[0],
                y,
                font_size,
                render_mode: 0,
                color,
                opacity: 1.0,
            });
        }
        Ok(())
    }

    /// Returns the page dimensions in PDF points as `(width, height)`.
    ///
    /// Reads the `/MediaBox` entry directly from the page dictionary.
    /// Standard page sizes:
    ///
    /// | Format | Width (pt) | Height (pt) |
    /// |--------|-----------|------------|
    /// | A4     | 595       | 842        |
    /// | Letter | 612       | 792        |
    /// | A3     | 842       | 1190       |
    ///
    /// # Errors
    /// Returns [`Error::Pdf`] if the page has no `/MediaBox` entry (rare but
    /// possible for pages that inherit `/MediaBox` from a parent node).
    pub fn size(&self) -> Result<(f32, f32)> {
        // Walk up the page tree (max 32 hops) to find an inherited MediaBox.
        let mut current_id = self.page_id;
        for _ in 0..32 {
            let (media_box_opt, parent_opt) = {
                let obj = self.doc.inner.get_object(current_id)?;
                let dict = obj.as_dict()?;
                (
                    dict.get(b"MediaBox").ok().cloned(),
                    dict.get(b"Parent").ok().cloned(),
                )
            };
            if let Some(mb) = media_box_opt {
                let arr = mb.as_array()?;
                if arr.len() < 4 {
                    return Err(Error::Pdf(lopdf::Error::DictKey("MediaBox".to_string())));
                }
                let get = |i: usize| -> f32 {
                    match &arr[i] {
                        lopdf::Object::Integer(v) => *v as f32,
                        lopdf::Object::Real(v) => *v,
                        _ => 0.0,
                    }
                };
                return Ok((get(2) - get(0), get(3) - get(1)));
            }
            match parent_opt {
                Some(Object::Reference(id)) => current_id = id,
                _ => break,
            }
        }
        Err(Error::Pdf(lopdf::Error::DictKey("MediaBox".to_string())))
    }

    fn push_op(&mut self, op: PendingOp) {
        let page_id = self.page_id;
        match self.doc.pending.iter_mut().find(|p| p.page_id == page_id) {
            Some(p) => p.ops.push(op),
            None => self.doc.pending.push(PendingPage { page_id, ops: vec![op] }),
        }
    }

    fn push_text(&mut self, text_op: PendingText) {
        self.push_op(PendingOp::Text(text_op));
    }
}

// ---------------------------------------------------------------------------
// draw feature: add_rect, add_line
// ---------------------------------------------------------------------------
#[cfg(feature = "draw")]
impl<'doc> PageHandle<'doc> {
    /// Overlays a filled rectangle on this page.
    ///
    /// `rect` is `[x, y, width, height]` in PDF points (origin: bottom-left).
    /// `color` is `[r, g, b]` in `0.0..=1.0`.
    /// `opacity` is in `0.0` (fully transparent) to `1.0` (fully opaque).
    ///
    /// # Example
    /// ```no_run
    /// # use harumi::Document;
    /// # fn main() -> harumi::Result<()> {
    /// # let mut doc = Document::from_bytes(&[])?;
    /// // Semi-transparent yellow highlight band, 14pt tall
    /// doc.page(1)?.add_rect([72.0, 690.0, 300.0, 14.0], [1.0, 1.0, 0.0], 0.4)?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn add_rect(&mut self, rect: [f32; 4], color: [f32; 3], opacity: f32) -> Result<()> {
        check_finite(&[rect[0], rect[1], rect[2], rect[3], color[0], color[1], color[2], opacity], "add_rect")?;
        self.push_op(PendingOp::Draw(crate::draw::DrawOp::Rect { rect, color, opacity }));
        Ok(())
    }

    /// Overlays a stroked rectangle border (no fill) on this page.
    ///
    /// `rect` is `[x, y, width, height]` in PDF points (origin: bottom-left).
    /// `color` is `[r, g, b]` in `0.0..=1.0`.
    /// `line_width` is the stroke width in PDF points.
    /// `opacity` is in `0.0..=1.0`.
    pub fn add_rect_stroke(
        &mut self,
        rect: [f32; 4],
        color: [f32; 3],
        line_width: f32,
        opacity: f32,
    ) -> Result<()> {
        check_finite(&[rect[0], rect[1], rect[2], rect[3], color[0], color[1], color[2], line_width, opacity], "add_rect_stroke")?;
        self.push_op(PendingOp::Draw(crate::draw::DrawOp::RectStroke {
            rect,
            color,
            line_width,
            opacity,
        }));
        Ok(())
    }

    /// Overlays a closed polygon on this page.
    ///
    /// `points` is a slice of `[x, y]` vertices in PDF points (origin: bottom-left).
    /// At least 2 points are required; fewer produce no output.
    /// `color` is `[r, g, b]` in `0.0..=1.0`.
    /// `opacity` is in `0.0..=1.0`.
    /// `filled = true` fills the polygon; `filled = false` strokes it.
    pub fn add_polygon(
        &mut self,
        points: &[[f32; 2]],
        color: [f32; 3],
        opacity: f32,
        filled: bool,
    ) -> Result<()> {
        {
            let coords: Vec<f32> = points.iter().flat_map(|p| p.iter().copied()).collect();
            check_finite(&coords, "add_polygon points")?;
        }
        check_finite(&[color[0], color[1], color[2], opacity], "add_polygon")?;
        self.push_op(PendingOp::Draw(crate::draw::DrawOp::Polygon {
            points: points.to_vec(),
            color,
            opacity,
            filled,
        }));
        Ok(())
    }

    /// Overlays a stroked line segment on this page.
    ///
    /// `from` and `to` are endpoints in PDF points (origin: bottom-left).
    /// `color` is `[r, g, b]` in `0.0..=1.0`.
    /// `line_width` is the stroke width in PDF points.
    /// `opacity` is in `0.0..=1.0`.
    ///
    /// # Example
    /// ```no_run
    /// # use harumi::Document;
    /// # fn main() -> harumi::Result<()> {
    /// # let mut doc = Document::from_bytes(&[])?;
    /// // Horizontal black rule at y=600, 1pt wide
    /// doc.page(1)?.add_line([72.0, 600.0], [520.0, 600.0], [0.0, 0.0, 0.0], 1.0, 1.0)?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn add_line(
        &mut self,
        from: [f32; 2],
        to: [f32; 2],
        color: [f32; 3],
        line_width: f32,
        opacity: f32,
    ) -> Result<()> {
        check_finite(&[from[0], from[1], to[0], to[1], color[0], color[1], color[2], line_width, opacity], "add_line")?;
        self.push_op(PendingOp::Draw(crate::draw::DrawOp::Line {
            from,
            to,
            color,
            width: line_width,
            opacity,
        }));
        Ok(())
    }

    /// Overlays a stroked open polyline (multi-segment path) on this page.
    ///
    /// `points` is a slice of `[x, y]` vertices in PDF points (origin: bottom-left).
    /// At least 2 points are required; fewer produce no output.
    /// Unlike [`add_polygon`](PageHandle::add_polygon), the path is left open (not closed).
    /// `color` is `[r, g, b]` in `0.0..=1.0`.
    /// `line_width` is the stroke width in PDF points.
    /// `opacity` is in `0.0..=1.0`.
    pub fn add_polyline(
        &mut self,
        points: &[[f32; 2]],
        color: [f32; 3],
        line_width: f32,
        opacity: f32,
    ) -> Result<()> {
        if points.len() < 2 {
            return Ok(());
        }
        {
            let coords: Vec<f32> = points.iter().flat_map(|p| p.iter().copied()).collect();
            check_finite(&coords, "add_polyline points")?;
        }
        check_finite(&[color[0], color[1], color[2], line_width, opacity], "add_polyline")?;
        self.push_op(PendingOp::Draw(crate::draw::DrawOp::Polyline {
            points: points.to_vec(),
            color,
            width: line_width,
            opacity,
        }));
        Ok(())
    }

    /// Overlays visible text with opacity on this page.
    ///
    /// Like [`add_text`](PageHandle::add_text) but applies a uniform fill opacity
    /// via an ExtGState (`/ca`). `opacity` is in `0.0` (transparent) to `1.0` (opaque).
    pub fn add_text_with_opacity(
        &mut self,
        text: &str,
        font: FontHandle,
        position: [f32; 2],
        font_size: f32,
        color: [f32; 3],
        opacity: f32,
    ) -> Result<()> {
        check_finite(
            &[position[0], position[1], font_size, color[0], color[1], color[2], opacity],
            "add_text_with_opacity",
        )?;
        self.push_text(PendingText {
            font,
            text: text.to_owned(),
            x: position[0],
            y: position[1],
            font_size,
            render_mode: 0,
            color,
            opacity,
        });
        Ok(())
    }

    /// Overlays multi-line visible text in a bounding box with opacity.
    ///
    /// Like [`add_text_box`](PageHandle::add_text_box) but applies a uniform fill opacity.
    /// `opacity` is in `0.0` (transparent) to `1.0` (opaque).
    #[allow(clippy::too_many_arguments)]
    pub fn add_text_box_with_opacity(
        &mut self,
        text: &str,
        font: FontHandle,
        rect: [f32; 4],
        font_size: f32,
        color: [f32; 3],
        line_height: f32,
        opacity: f32,
    ) -> Result<()> {
        check_finite(
            &[rect[0], rect[1], rect[2], rect[3], font_size,
              color[0], color[1], color[2], line_height, opacity],
            "add_text_box_with_opacity",
        )?;
        if rect[2] <= 0.0 || rect[3] <= 0.0 {
            return Ok(());
        }

        let raw = self.doc.raw_fonts.get(font.0 as usize)
            .ok_or(Error::InvalidFont(font.0))?;
        let face = Face::parse(&raw.ttf_bytes, 0)
            .map_err(|e| Error::FontParse(e.to_string()))?;

        let box_width = rect[2];
        let effective_lh = if line_height <= 0.0 { font_size * 1.2 } else { line_height };

        let mut all_lines: Vec<String> = Vec::new();
        for paragraph in text.split('\n') {
            all_lines.extend(wrap_paragraph(paragraph, &face, font_size, box_width));
        }

        let start_y = rect[1] + rect[3] - font_size;
        let top = rect[1] + rect[3];
        let bottom = rect[1];

        for (i, line) in all_lines.iter().enumerate() {
            let y = start_y - i as f32 * effective_lh;
            if y > top || y < bottom {
                continue;
            }
            self.push_text(PendingText {
                font,
                text: line.clone(),
                x: rect[0],
                y,
                font_size,
                render_mode: 0,
                color,
                opacity,
            });
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// image feature: add_image, add_image_with_opacity
// ---------------------------------------------------------------------------
#[cfg(feature = "image")]
impl<'doc> PageHandle<'doc> {
    /// Overlays a raster image (JPEG or PNG) on this page at full opacity.
    ///
    /// `image_bytes` is the raw file content (JPEG or PNG).
    /// `rect` is `[x, y, width, height]` in PDF points (origin: bottom-left).
    ///
    /// PNG images with an alpha channel are composited against a white
    /// background. True transparency (PDF SMask) is planned for v0.3.
    pub fn add_image(&mut self, image_bytes: &[u8], rect: [f32; 4]) -> Result<()> {
        self.add_image_with_opacity(image_bytes, rect, 1.0)
    }

    /// Overlays a raster image with the given opacity.
    ///
    /// `opacity` is in `0.0` (fully transparent) to `1.0` (fully opaque).
    pub fn add_image_with_opacity(
        &mut self,
        image_bytes: &[u8],
        rect: [f32; 4],
        opacity: f32,
    ) -> Result<()> {
        check_finite(&[rect[0], rect[1], rect[2], rect[3], opacity], "add_image_with_opacity")?;
        self.push_op(PendingOp::Draw(crate::draw::DrawOp::Image {
            bytes: image_bytes.to_vec(),
            rect,
            opacity,
        }));
        Ok(())
    }
}

fn check_finite(values: &[f32], label: &str) -> Result<()> {
    if values.iter().any(|v| !v.is_finite()) {
        return Err(Error::InvalidInput(format!("{label} contains NaN or Infinity")));
    }
    Ok(())
}

/// Returns true for characters that can line-break at any position (CJK scripts).
fn is_cjk(ch: char) -> bool {
    matches!(
        ch as u32,
        0x3000..=0x9FFF    // CJK unified ideographs, hiragana, katakana, etc.
        | 0xF900..=0xFAFF  // CJK compatibility ideographs
        | 0xFE30..=0xFE4F  // CJK compatibility forms
        | 0xFF00..=0xFFEF  // fullwidth / halfwidth forms
        | 0x20000..=0x2A6DF | 0x2A700..=0x2CEAF  // CJK extension B / C / D
    )
}

/// Width of one character in PDF points given the font face and font size.
fn glyph_advance_pt(face: &Face, ch: char, font_size: f32) -> f32 {
    let upem = face.units_per_em() as f32;
    face.glyph_index(ch)
        .and_then(|g| face.glyph_hor_advance(g))
        .map(|adv| adv as f32 * font_size / upem)
        .unwrap_or(font_size * 0.5)
}

/// Greedy line-breaking for a single paragraph (no embedded newlines).
fn wrap_paragraph(paragraph: &str, face: &Face, font_size: f32, box_width: f32) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut current_w: f32 = 0.0;
    // byte index of last ASCII space in `current`; width after that space (= start of next word)
    let mut last_space_byte: Option<usize> = None;
    let mut width_at_word_start: f32 = 0.0;

    for ch in paragraph.chars() {
        let ch_w = glyph_advance_pt(face, ch, font_size);

        if current_w + ch_w > box_width && !current.is_empty() {
            if is_cjk(ch) || last_space_byte.is_none() {
                // CJK or no word boundary found → break at the current character
                lines.push(std::mem::take(&mut current));
                current_w = 0.0;
                last_space_byte = None;
            } else {
                // Break at the last space: emit everything before it, keep the word after
                let sp = last_space_byte.unwrap();
                let word = current[sp + 1..].to_owned(); // sp+1 safe: space is ASCII (1 byte)
                current.truncate(sp);
                lines.push(std::mem::take(&mut current));
                current = word;
                current_w = (current_w - width_at_word_start).max(0.0);
                last_space_byte = None;
            }
        }

        if ch == ' ' {
            last_space_byte = Some(current.len()); // byte index of space before it is pushed
            width_at_word_start = current_w + ch_w; // total width including the space
        }
        current.push(ch);
        current_w += ch_w;
    }

    if !current.is_empty() {
        lines.push(current);
    }
    lines
}

fn root_pages_id(doc: &lopdf::Document) -> Result<ObjectId> {
    let root_ref = doc.trailer.get(b"Root")?.as_reference()?;
    let catalog = doc.get_object(root_ref)?.as_dict()?;
    Ok(catalog.get(b"Pages")?.as_reference()?)
}

fn append_to_contents(
    doc: &mut lopdf::Document,
    page_id: ObjectId,
    new_stream_id: ObjectId,
) -> Result<()> {
    let contents_ref = doc
        .get_object(page_id)?
        .as_dict()?
        .get(b"Contents")
        .ok()
        .cloned();

    let new_ref = Object::Reference(new_stream_id);

    match contents_ref {
        Some(Object::Reference(r)) => {
            // Check whether the reference points to an Array (indirect Contents array,
            // common in InDesign-generated PDFs) or a single content stream.
            let is_array = doc.get_object(r)
                .ok()
                .map(|o| matches!(o, Object::Array(_)))
                .unwrap_or(false);
            if is_array {
                let arr_obj = doc.get_object_mut(r)?.as_array_mut()?;
                arr_obj.push(new_ref);
            } else {
                let arr = Object::Array(vec![Object::Reference(r), new_ref]);
                doc.get_object_mut(page_id)?.as_dict_mut()?.set("Contents", arr);
            }
        }
        Some(Object::Array(mut arr)) => {
            arr.push(new_ref);
            doc.get_object_mut(page_id)?.as_dict_mut()?.set("Contents", Object::Array(arr));
        }
        None => {
            doc.get_object_mut(page_id)?.as_dict_mut()?.set("Contents", new_ref);
        }
        _ => {}
    }
    Ok(())
}

fn add_font_to_resources(
    doc: &mut lopdf::Document,
    page_id: ObjectId,
    pdf_name: &[u8],
    type0_id: ObjectId,
) -> Result<()> {
    let resources_id: Option<ObjectId> = {
        let page_dict = doc.get_object(page_id)?.as_dict()?;
        match page_dict.get(b"Resources").ok() {
            Some(Object::Reference(r)) => Some(*r),
            _ => None,
        }
    };

    let font_ref = Object::Reference(type0_id);

    if let Some(res_id) = resources_id {
        let res_dict = doc.get_object_mut(res_id)?.as_dict_mut()?;
        ensure_font_entry(res_dict, pdf_name, font_ref);
    } else {
        let page_dict = doc.get_object_mut(page_id)?.as_dict_mut()?;
        match page_dict.get_mut(b"Resources") {
            Ok(res_obj) => {
                let res_dict = res_obj.as_dict_mut()?;
                ensure_font_entry(res_dict, pdf_name, font_ref);
            }
            Err(_) => {
                let mut font_dict = Dictionary::new();
                font_dict.set(pdf_name, font_ref);
                let mut res_dict = Dictionary::new();
                res_dict.set("Font", Object::Dictionary(font_dict));
                page_dict.set("Resources", Object::Dictionary(res_dict));
            }
        }
    }

    Ok(())
}

fn ensure_font_entry(res_dict: &mut Dictionary, pdf_name: &[u8], font_ref: Object) {
    match res_dict.get_mut(b"Font") {
        Ok(font_obj) => {
            if let Ok(fd) = font_obj.as_dict_mut() {
                fd.set(pdf_name, font_ref);
            }
        }
        Err(_) => {
            let mut font_dict = Dictionary::new();
            font_dict.set(pdf_name, font_ref);
            res_dict.set("Font", Object::Dictionary(font_dict));
        }
    }
}

/// Resolves the Resources dict for a page (direct or indirect) and applies `f`.
#[cfg(any(feature = "draw", feature = "image"))]
fn with_resources_dict_mut<F>(doc: &mut lopdf::Document, page_id: ObjectId, f: F) -> Result<()>
where
    F: FnOnce(&mut Dictionary),
{
    let resources_id: Option<ObjectId> = {
        let page_dict = doc.get_object(page_id)?.as_dict()?;
        match page_dict.get(b"Resources").ok() {
            Some(Object::Reference(r)) => Some(*r),
            _ => None,
        }
    };

    if let Some(res_id) = resources_id {
        f(doc.get_object_mut(res_id)?.as_dict_mut()?);
    } else {
        let page_dict = doc.get_object_mut(page_id)?.as_dict_mut()?;
        match page_dict.get_mut(b"Resources") {
            Ok(res_obj) => f(res_obj.as_dict_mut()?),
            Err(_) => {
                let mut res_dict = Dictionary::new();
                f(&mut res_dict);
                page_dict.set("Resources", Object::Dictionary(res_dict));
            }
        }
    }
    Ok(())
}

#[cfg(feature = "draw")]
fn add_ext_gstate_to_resources(
    doc: &mut lopdf::Document,
    page_id: ObjectId,
    registry: crate::draw::ExtGStateRegistry,
) -> Result<()> {
    let ext_g_dict = registry.to_lopdf_dict();
    with_resources_dict_mut(doc, page_id, |res| {
        match res.get_mut(b"ExtGState") {
            Ok(obj) => {
                if let Ok(existing) = obj.as_dict_mut() {
                    for (k, v) in ext_g_dict.iter() {
                        existing.set(k.as_slice(), v.clone());
                    }
                }
            }
            Err(_) => {
                res.set("ExtGState", Object::Dictionary(ext_g_dict.clone()));
            }
        }
    })
}

#[cfg(feature = "image")]
fn add_xobject_to_resources(
    doc: &mut lopdf::Document,
    page_id: ObjectId,
    name: &[u8],
    xobj_id: ObjectId,
) -> Result<()> {
    let xobj_ref = Object::Reference(xobj_id);
    with_resources_dict_mut(doc, page_id, |res| {
        match res.get_mut(b"XObject") {
            Ok(obj) => {
                if let Ok(d) = obj.as_dict_mut() {
                    d.set(name, xobj_ref.clone());
                }
            }
            Err(_) => {
                let mut xobj_dict = Dictionary::new();
                xobj_dict.set(name, xobj_ref.clone());
                res.set("XObject", Object::Dictionary(xobj_dict));
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn document_is_send() {
        fn assert_send<T: Send>() {}
        assert_send::<Document>();
    }
}
