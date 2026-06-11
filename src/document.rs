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

/// A color value that can be either RGB or CMYK.
///
/// RGB is the default for most PDF operations and displays on screens.
/// CMYK is used primarily for print output.
///
/// # Example
/// ```
/// use harumi::Color;
///
/// let rgb = Color::Rgb([0.5, 0.2, 0.8]);
/// let cmyk = Color::Cmyk([0.0, 0.8, 0.2, 0.1]);
/// ```
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Color {
    /// RGB color: each component in `0.0..=1.0`.
    /// Emits PDF operators `rg` (fill) and `RG` (stroke).
    Rgb([f32; 3]),
    /// CMYK color: each component in `0.0..=1.0`.
    /// Emits PDF operators `k` (fill) and `K` (stroke).
    Cmyk([f32; 4]),
}

impl From<[f32; 3]> for Color {
    fn from(c: [f32; 3]) -> Self {
        Color::Rgb(c)
    }
}

impl From<[f32; 4]> for Color {
    fn from(c: [f32; 4]) -> Self {
        Color::Cmyk(c)
    }
}

/// A single text placement descriptor for use with [`PageHandle::add_invisible_text_runs`].
///
/// # Example
/// ```no_run
/// # use harumi::{Document, TextRun, Color};
/// # fn main() -> harumi::Result<()> {
/// # let mut doc = Document::from_bytes(&[])?;
/// # let font = doc.embed_font(&[])?;
/// doc.page(1)?.add_invisible_text_runs(&[
///     TextRun { text: "first line".into(), font, x: 72.0, y: 700.0, font_size: 12.0, render_mode: 3, color: Color::Rgb([0.0; 3]) },
///     TextRun { text: "second line".into(), font, x: 72.0, y: 685.0, font_size: 12.0, render_mode: 3, color: Color::Rgb([0.0; 3]) },
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
    /// Fill color (RGB or CMYK). Only applied when `render_mode == 0`.
    pub color: Color,
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
    color: Color,
    opacity: f32,
    rotation_degrees: f32,
    bold: bool,
    italic: bool,
}

/// A pending operation on a page (text or drawing primitive).
enum PendingOp {
    Text(PendingText),
    Replace(crate::replace::TextReplaceOp),
    ReplacePreserve(crate::replace::TextReplacePreserveOp),
    ReplaceResubset(crate::replace::TextReplaceResubsetOp),
    #[cfg(feature = "draw")]
    Draw(crate::draw::DrawOp),
}

/// Per-page pending operations.
struct PendingPage {
    page_id: ObjectId,
    ops: Vec<PendingOp>,
}

/// A document outline entry (bookmark) accumulated before save time.
struct PendingBookmark {
    title: String,
    page: u32,
    /// PDF y coordinate (bottom-left origin) for the destination anchor.
    y: f32,
    /// Outline hierarchy level: 0 = top-level (from add_bookmark), 1..6 = nested levels.
    level: u8,
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

/// The type of a PDF form field.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FieldType {
    /// Single-line or multiline text input (`/Tx`).
    Text,
    /// Checkbox or push-button (`/Btn`, non-radio).
    Checkbox,
    /// Radio button group (`/Btn` with radio flag set).
    Radio,
    /// Drop-down list or list box (`/Ch`).
    Choice,
    /// Digital signature field (`/Sig`).
    Signature,
    /// Unknown or unsupported field type.
    Unknown,
}

/// A PDF form field returned by [`Document::form_fields`].
#[non_exhaustive]
#[derive(Debug, Clone)]
pub struct FormField {
    /// The field name. For nested fields this is the dotted path,
    /// e.g. `"address.city"`.
    pub name: String,
    /// The kind of input widget.
    pub field_type: FieldType,
    /// The field's current string value.
    ///
    /// For text fields this is the entered text. For checkboxes/radio buttons
    /// this is the appearance state name (`"Yes"`, `"Off"`, etc.). For choice
    /// fields this is the selected option. Empty string when no value is set.
    pub value: String,
}

/// Options for creating a text field via [`Document::add_text_field`].
#[derive(Clone, Debug, Default)]
pub struct TextFieldOptions {
    /// Initial value of the field (`/V` entry). Default: empty string.
    pub default_value: String,
    /// Allow multiline input (`/Ff` bit 12, "Multiline" flag). Default: `false`.
    pub multiline: bool,
    /// Mark the field as read-only (`/Ff` bit 0, "ReadOnly" flag). Default: `false`.
    pub read_only: bool,
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
    pending_bookmarks: Vec<PendingBookmark>,
    /// Set to true after the first successful `finalize()`. Prevents silent corruption
    /// when new ops are queued after a `save()` call (font subsets would mismatch).
    finalized: bool,
    /// Pending encryption parameters. Applied just before write at save time.
    pending_encryption: Option<(String, String, EncAlgorithm)>,
}

/// Internal encryption algorithm selector for `pending_encryption`.
#[derive(Clone, Debug, PartialEq, Eq)]
enum EncAlgorithm {
    Rc4_128,
    Aes256,
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
        let page_id = all_pages
            .get(&page)
            .copied()
            .ok_or(Error::PageNotFound(page))?;
        crate::extract::extract_text_runs_from_page(&self.inner, page_id)
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

            let char_to_gid: BTreeMap<char, u16> = subset
                .gid_to_char
                .iter()
                .map(|(&gid, &ch)| (ch, gid))
                .collect();

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
            struct ReplaceWork {
                page_id: ObjectId,
                ops: Vec<(String, String, u32)>, // (old_text, new_text, font_idx)
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
                                Some((r.old_text.clone(), r.new_text.clone(), r.font.0))
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
                for (old_text, new_text, font_idx) in &item.ops {
                    let state = embedded
                        .get(font_idx)
                        .ok_or(Error::InvalidFont(*font_idx))?;
                    resolved.push(crate::replace::ResolvedReplacement {
                        old_text: old_text.clone(),
                        new_text: new_text.clone(),
                        new_pdf_font_name: state.ef.pdf_name.clone(),
                        char_to_gid: state.char_to_gid.clone(),
                        gid_to_advance: state.gid_to_advance.clone(),
                        units_per_em: state.units_per_em,
                    });
                }

                // Rewrite streams and replace /Contents.
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
                for (_, _, font_idx) in &item.ops {
                    let state = embedded
                        .get(font_idx)
                        .ok_or(Error::InvalidFont(*font_idx))?;
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
            color: Color::Rgb([0.0; 3]),
            opacity: 1.0,
            rotation_degrees: 0.0,
            bold: false,
            italic: false,
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
        color: impl Into<Color>,
    ) -> Result<()> {
        let color = color.into();
        check_finite(&[position[0], position[1], font_size], "add_text")?;
        self.push_text(PendingText {
            font,
            text: text.to_owned(),
            x: position[0],
            y: position[1],
            font_size,
            render_mode: 0,
            color,
            opacity: 1.0,
            rotation_degrees: 0.0,
            bold: false,
            italic: false,
        });
        Ok(())
    }

    /// Overlays visible text with optional bold/italic synthetic styling.
    ///
    /// Bold is simulated with PDF render mode 2 (fill+stroke).
    /// Italic is simulated with a 12° horizontal shear text matrix.
    /// Both may be combined. Color and font size work the same as [`add_text`].
    ///
    /// `position` is `[x, y]` in PDF points (origin: bottom-left of page).
    /// `color` is `[r, g, b]` where each component is in `0.0..=1.0`.
    #[allow(clippy::too_many_arguments)]
    pub fn add_text_styled(
        &mut self,
        text: &str,
        font: FontHandle,
        position: [f32; 2],
        font_size: f32,
        color: impl Into<Color>,
        bold: bool,
        italic: bool,
    ) -> Result<()> {
        let color = color.into();
        check_finite(&[position[0], position[1], font_size], "add_text_styled")?;
        self.push_text(PendingText {
            font,
            text: text.to_owned(),
            x: position[0],
            y: position[1],
            font_size,
            render_mode: 0,
            color,
            opacity: 1.0,
            rotation_degrees: 0.0,
            bold,
            italic,
        });
        Ok(())
    }

    /// Queues multiple text placements in one call.
    ///
    /// All runs across the entire document are collected before subsetting,
    /// so each font is subsetted exactly once regardless of how many runs use it.
    pub fn add_invisible_text_runs(&mut self, runs: &[TextRun]) -> Result<()> {
        for run in runs {
            check_finite(&[run.x, run.y, run.font_size], "add_invisible_text_runs")?;
            self.push_text(PendingText {
                font: run.font,
                text: run.text.clone(),
                x: run.x,
                y: run.y,
                font_size: run.font_size,
                render_mode: run.render_mode,
                color: run.color,
                opacity: 1.0,
                rotation_degrees: 0.0,
                bold: false,
                italic: false,
            });
        }
        Ok(())
    }

    /// Replaces all occurrences of `old_text` in this page's existing content streams
    /// with `new_text` rendered in `font`.
    ///
    /// Matching is per PDF text operator: `old_text` must exactly equal the decoded
    /// content of a single `Tj` operator or one string element within a `TJ` array.
    /// Text split across multiple operators is not matched.
    ///
    /// Width compensation is applied automatically via a `Td` operator so that
    /// subsequent text on the same line is not displaced.
    ///
    /// Returns the number of matches found (and queued for replacement). A return
    /// value of `0` means `old_text` was not found; no modification is queued.
    ///
    /// # Errors
    /// Returns [`Error::InvalidFont`] if `font` was not registered on this document,
    /// or [`Error::InvalidInput`] if called after [`save`](Document::save).
    pub fn replace_text(
        &mut self,
        old_text: &str,
        new_text: &str,
        font: FontHandle,
    ) -> Result<usize> {
        if self.doc.raw_fonts.get(font.0 as usize).is_none() {
            return Err(Error::InvalidFont(font.0));
        }
        let count =
            crate::replace::count_matches_in_page(&self.doc.inner, self.page_id, old_text, None)?;
        if count > 0 {
            self.push_op(PendingOp::Replace(crate::replace::TextReplaceOp {
                font,
                old_text: old_text.to_owned(),
                new_text: new_text.to_owned(),
            }));
        }
        Ok(count)
    }

    /// Replaces all occurrences of `old_text` in this page's existing content streams
    /// with `new_text`, reusing the font already embedded in the PDF at that position.
    ///
    /// Unlike [`replace_text`](PageHandle::replace_text), no `FontHandle` is required —
    /// harumi reads the font reference from the preceding `Tf` operator in the stream.
    ///
    /// Returns the number of matches found (and queued for replacement). Glyph
    /// availability is validated eagerly: if any character in `new_text` is absent
    /// from the existing font's ToUnicode mapping (e.g. the font is subsetted),
    /// `Err(FontCharNotMapped)` is returned immediately so the caller can fall back
    /// to [`replace_text`](PageHandle::replace_text) with an explicit font.
    ///
    /// # Errors
    /// Returns [`Error::FontCharNotMapped`](crate::Error::FontCharNotMapped) if any
    /// character in `new_text` is not present in the font's ToUnicode mapping.
    pub fn replace_text_preserve_font(&mut self, old_text: &str, new_text: &str) -> Result<usize> {
        let count = crate::replace::count_matches_in_page(
            &self.doc.inner,
            self.page_id,
            old_text,
            Some(new_text),
        )?;
        if count > 0 {
            self.push_op(PendingOp::ReplacePreserve(
                crate::replace::TextReplacePreserveOp {
                    old_text: old_text.to_owned(),
                    new_text: new_text.to_owned(),
                },
            ));
        }
        Ok(count)
    }

    /// Replaces all occurrences of `old_text` in this page's existing content streams
    /// with `new_text`, expanding the font subset if necessary.
    ///
    /// Unlike [`replace_text_preserve_font`](PageHandle::replace_text_preserve_font),
    /// this method succeeds even when `new_text` contains characters absent from the
    /// current font subset.  The caller must supply the **original, unsubsetted** font
    /// bytes (`font_bytes`) so that harumi can rebuild the subset.
    ///
    /// After the new subset is embedded the GID numbering may change; harumi
    /// automatically re-encodes all content streams on every page that references
    /// the same font.
    ///
    /// # Limitations
    /// Only **CIDFontType2** fonts with `CIDToGIDMap /Identity` are supported
    /// (which is what harumi embeds).  Type1 / simple TrueType fonts will return
    /// [`Error::InvalidInput`].
    ///
    /// Returns the number of matches found (and queued for replacement).
    /// A return value of `0` means `old_text` was not found; no modification is queued.
    ///
    /// # Errors
    /// - [`Error::InvalidInput`] if called after [`save`](Document::save), or if the
    ///   font in the PDF is not a supported CIDFontType2.
    /// - [`Error::FontParse`] if `font_bytes` cannot be parsed as a TTF/OTF font.
    pub fn replace_text_resubset(
        &mut self,
        old_text: &str,
        new_text: &str,
        font_bytes: &[u8],
    ) -> Result<usize> {
        if self.doc.finalized {
            return Err(Error::InvalidInput(
                "replace_text_resubset called after save()".into(),
            ));
        }
        // Validate font bytes eagerly.
        let face =
            ttf_parser::Face::parse(font_bytes, 0).map_err(|e| Error::FontParse(e.to_string()))?;
        if face.units_per_em() == 0 {
            return Err(Error::FontParse("font units_per_em is 0".into()));
        }
        let count =
            crate::replace::count_matches_in_page(&self.doc.inner, self.page_id, old_text, None)?;
        if count > 0 {
            self.push_op(PendingOp::ReplaceResubset(
                crate::replace::TextReplaceResubsetOp {
                    old_text: old_text.to_owned(),
                    new_text: new_text.to_owned(),
                    font_bytes: font_bytes.to_vec(),
                    wrap: None,
                },
            ));
        }
        Ok(count)
    }

    /// Like [`replace_text_resubset`], but wraps `new_text` to multiple lines if it exceeds the line width.
    ///
    /// # Parameters
    /// - `old_text`: Text to find
    /// - `new_text`: Replacement text (will be wrapped if too long)
    /// - `font_bytes`: TTF font bytes for subsetting and width calculation
    /// - `line_height`: Vertical spacing between wrapped lines (e.g., font_size * 1.2).
    ///   If 0.0, defaults to font_size * 1.2.
    ///
    /// # Limitations
    /// - Single-font replacements only (no font switching within wrapped text)
    /// - Estimates page width as ~450pt (A4 with standard margins); PDFs with custom widths may wrap differently
    /// - Does not handle multi-column layouts
    pub fn replace_text_resubset_with_wrap(
        &mut self,
        old_text: &str,
        new_text: &str,
        font_bytes: &[u8],
        line_height: f32,
    ) -> Result<usize> {
        if self.doc.finalized {
            return Err(Error::InvalidInput(
                "replace_text_resubset_with_wrap called after save()".into(),
            ));
        }

        // Validate line_height: must be finite and non-negative.
        if !line_height.is_finite() || line_height < 0.0 {
            return Err(Error::InvalidInput(format!(
                "line_height must be finite and non-negative, got {}",
                line_height
            )));
        }

        let face =
            ttf_parser::Face::parse(font_bytes, 0).map_err(|e| Error::FontParse(e.to_string()))?;
        if face.units_per_em() == 0 {
            return Err(Error::FontParse("font units_per_em is 0".into()));
        }

        let count =
            crate::replace::count_matches_in_page(&self.doc.inner, self.page_id, old_text, None)?;

        if count > 0 {
            let max_width = self.page_width_for_wrap();
            // Ensure max_width is positive (safety check on MediaBox calculation).
            if max_width <= 0.0 {
                return Err(Error::InvalidInput(format!(
                    "page width for wrapping is non-positive ({}pt); cannot wrap",
                    max_width
                )));
            }
            let effective_lh = if line_height > 0.0 { line_height } else { 14.4 };

            self.push_op(PendingOp::ReplaceResubset(
                crate::replace::TextReplaceResubsetOp {
                    old_text: old_text.to_owned(),
                    new_text: new_text.to_owned(),
                    font_bytes: font_bytes.to_vec(),
                    wrap: Some(crate::replace::WrapParams {
                        font_bytes: font_bytes.to_vec(),
                        line_height: effective_lh,
                        max_width,
                    }),
                },
            ));
        }

        Ok(count)
    }

    /// Estimate page width for text wrapping from MediaBox.
    ///
    /// Returns MediaBox width minus 144pt (72pt margins × 2).
    /// Falls back to A4 default (451pt) if page dimensions are unavailable.
    fn page_width_for_wrap(&self) -> f32 {
        self.media_box()
            .map(|b| (b[2] - b[0]) - 144.0)
            .unwrap_or(451.0)
    }

    /// Scans the page for `old_text` and validates that all characters in `new_text`
    /// are present in the existing font's ToUnicode mapping — without modifying the document.
    ///
    /// Returns the number of occurrences of `old_text` found on this page.
    /// A return value of `0` means no replacement would occur.
    ///
    /// Use this to decide whether to call
    /// [`replace_text_preserve_font`](PageHandle::replace_text_preserve_font)
    /// (which would mutate the document) or fall back to
    /// [`replace_text`](PageHandle::replace_text) with an explicit font.
    ///
    /// # Errors
    /// Returns [`Error::FontCharNotMapped`](crate::Error::FontCharNotMapped) if any
    /// character in `new_text` is absent from the font's ToUnicode mapping.
    pub fn can_replace_text(&self, old_text: &str, new_text: &str) -> Result<usize> {
        crate::replace::count_matches_in_page(
            &self.doc.inner,
            self.page_id,
            old_text,
            Some(new_text),
        )
    }

    /// Adds a clickable URL link annotation to this page.
    ///
    /// `rect` is `[x, y, width, height]` in PDF points (origin: bottom-left).
    /// The annotation has no visible border; the clickable area is invisible in
    /// normal view but interactive in PDF viewers. The link is written into the
    /// PDF object graph immediately — it does not require a `save()` call to take
    /// effect, but it will be included in the saved output.
    ///
    /// # Example
    /// ```no_run
    /// # use harumi::Document;
    /// # fn main() -> harumi::Result<()> {
    /// let mut doc = Document::from_file("report.pdf")?;
    /// // Clickable "website" label at the bottom of page 1
    /// doc.page(1)?.add_link_url([72.0, 40.0, 200.0, 20.0], "https://example.com")?;
    /// doc.save("report_linked.pdf")?;
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// # Errors
    /// Returns [`Error::InvalidInput`] if `url` is empty or coordinates contain NaN/Infinity.
    ///
    /// # Security note
    /// The `url` string is written verbatim into the PDF `/URI` action. Do **not** pass
    /// user-supplied strings without validation: `javascript:`, `data:`, and `file://`
    /// URIs are accepted by the PDF spec but may be exploited by a malicious caller.
    pub fn add_link_url(&mut self, rect: [f32; 4], url: &str) -> Result<()> {
        check_finite(&[rect[0], rect[1], rect[2], rect[3]], "add_link_url")?;
        if url.is_empty() {
            return Err(Error::InvalidInput("url must not be empty".into()));
        }
        let mut action = Dictionary::new();
        action.set("Type", Object::Name(b"Action".to_vec()));
        action.set("S", Object::Name(b"URI".to_vec()));
        action.set(
            "URI",
            Object::String(url.as_bytes().to_vec(), lopdf::StringFormat::Literal),
        );

        let mut d = build_link_annot_base(rect);
        d.set("A", Object::Dictionary(action));
        let annot_id = self.doc.inner.add_object(Object::Dictionary(d));
        append_annotation_to_page(&mut self.doc.inner, self.page_id, annot_id)
    }

    /// Adds an internal link annotation that navigates to a specific page.
    ///
    /// `rect` is `[x, y, width, height]` in PDF points (origin: bottom-left).
    /// `target_page` is the 1-indexed destination page; clicking the annotation
    /// jumps to the top of that page.
    ///
    /// # Example
    /// ```no_run
    /// # use harumi::Document;
    /// # fn main() -> harumi::Result<()> {
    /// let mut doc = Document::from_file("report.pdf")?;
    /// // Table-of-contents entry on page 1 that links to page 5
    /// doc.page(1)?.add_link_internal([72.0, 700.0, 300.0, 14.0], 5)?;
    /// doc.save("report_with_toc.pdf")?;
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// # Errors
    /// Returns [`Error::PageNotFound`] if `target_page` is out of range, or
    /// [`Error::InvalidInput`] if coordinates contain NaN/Infinity.
    pub fn add_link_internal(&mut self, rect: [f32; 4], target_page: u32) -> Result<()> {
        check_finite(&[rect[0], rect[1], rect[2], rect[3]], "add_link_internal")?;
        let page_ids = self.doc.inner.get_pages();
        let target_id = page_ids
            .get(&target_page)
            .copied()
            .ok_or(Error::PageNotFound(target_page))?;

        let dest = Object::Array(vec![
            Object::Reference(target_id),
            Object::Name(b"XYZ".to_vec()),
            Object::Null,
            Object::Null,
            Object::Null,
        ]);
        let mut d = build_link_annot_base(rect);
        d.set("Dest", dest);
        let annot_id = self.doc.inner.add_object(Object::Dictionary(d));
        append_annotation_to_page(&mut self.doc.inner, self.page_id, annot_id)
    }

    // -----------------------------------------------------------------------
    // Markup annotations (Highlight / Underline / StrikeOut)
    // -----------------------------------------------------------------------

    /// Adds a highlight annotation over the given area.
    ///
    /// `rect` is `[x, y, width, height]` in PDF points. `color` is an RGB
    /// triple in `0.0..=1.0`; a typical yellow highlight is `[1.0, 1.0, 0.0]`.
    ///
    /// # Errors
    /// Returns [`Error::InvalidInput`] if any coordinate is NaN/Infinity.
    pub fn add_highlight(&mut self, rect: [f32; 4], color: impl Into<Color>) -> Result<()> {
        let color = color.into();
        check_finite(&[rect[0], rect[1], rect[2], rect[3]], "add_highlight")?;
        check_positive_size(rect[2], rect[3], "add_highlight")?;
        let d = build_markup_annot(b"Highlight", rect, color);
        let annot_id = self.doc.inner.add_object(Object::Dictionary(d));
        append_annotation_to_page(&mut self.doc.inner, self.page_id, annot_id)
    }

    /// Adds an underline annotation under the given area.
    ///
    /// # Errors
    /// Returns [`Error::InvalidInput`] if any coordinate is NaN/Infinity.
    pub fn add_underline(&mut self, rect: [f32; 4], color: impl Into<Color>) -> Result<()> {
        let color = color.into();
        check_finite(&[rect[0], rect[1], rect[2], rect[3]], "add_underline")?;
        check_positive_size(rect[2], rect[3], "add_underline")?;
        let d = build_markup_annot(b"Underline", rect, color);
        let annot_id = self.doc.inner.add_object(Object::Dictionary(d));
        append_annotation_to_page(&mut self.doc.inner, self.page_id, annot_id)
    }

    /// Adds a strikeout (strikethrough) annotation over the given area.
    ///
    /// # Errors
    /// Returns [`Error::InvalidInput`] if any coordinate is NaN/Infinity.
    pub fn add_strikeout(&mut self, rect: [f32; 4], color: impl Into<Color>) -> Result<()> {
        let color = color.into();
        check_finite(&[rect[0], rect[1], rect[2], rect[3]], "add_strikeout")?;
        check_positive_size(rect[2], rect[3], "add_strikeout")?;
        let d = build_markup_annot(b"StrikeOut", rect, color);
        let annot_id = self.doc.inner.add_object(Object::Dictionary(d));
        append_annotation_to_page(&mut self.doc.inner, self.page_id, annot_id)
    }

    /// Adds a squiggly (wavy underline) annotation under the given area.
    ///
    /// # Errors
    /// Returns [`Error::InvalidInput`] if any coordinate is NaN/Infinity.
    pub fn add_squiggly(&mut self, rect: [f32; 4], color: impl Into<Color>) -> Result<()> {
        let color = color.into();
        check_finite(&[rect[0], rect[1], rect[2], rect[3]], "add_squiggly")?;
        check_positive_size(rect[2], rect[3], "add_squiggly")?;
        let d = build_markup_annot(b"Squiggly", rect, color);
        let annot_id = self.doc.inner.add_object(Object::Dictionary(d));
        append_annotation_to_page(&mut self.doc.inner, self.page_id, annot_id)
    }

    /// Permanently blacks out the given area with a PDF Redact annotation.
    ///
    /// `rect` is `[x, y, width, height]` in PDF points (origin: bottom-left).
    ///
    /// This adds a standard `/Redact` annotation with a solid black appearance stream.
    /// Note: this does NOT scrub underlying text or images from content streams —
    /// it renders a black box on screen and in print. To permanently remove
    /// underlying content, the document must be processed by a compliant
    /// PDF reader's "Apply Redactions" command.
    ///
    /// # Errors
    /// Returns [`Error::InvalidInput`] if coordinates are NaN/Infinity or
    /// width/height are non-positive.
    pub fn redact(&mut self, rect: [f32; 4]) -> Result<()> {
        check_finite(&[rect[0], rect[1], rect[2], rect[3]], "redact")?;
        check_positive_size(rect[2], rect[3], "redact")?;

        let x1 = rect[0];
        let y1 = rect[1];
        let x2 = rect[0] + rect[2];
        let y2 = rect[1] + rect[3];
        let w = rect[2];
        let h = rect[3];

        // Build the appearance stream content: solid black rectangle fill.
        let ap_content = format!("q\n0 0 0 rg\n0 0 {:.4} {:.4} re\nf\nQ\n", w, h);

        // Build the appearance stream XObject dictionary.
        let mut ap_dict = Dictionary::new();
        ap_dict.set("Type", Object::Name(b"XObject".to_vec()));
        ap_dict.set("Subtype", Object::Name(b"Form".to_vec()));
        ap_dict.set(
            "BBox",
            Object::Array(vec![
                Object::Real(0.0),
                Object::Real(0.0),
                Object::Real(w),
                Object::Real(h),
            ]),
        );
        ap_dict.set("Resources", Object::Dictionary(Dictionary::new()));

        // Create the appearance stream object.
        let ap_stream = Stream::new(ap_dict, ap_content.into_bytes());
        let ap_id = self.doc.inner.add_object(Object::Stream(ap_stream));

        // Build the /Redact annotation dictionary.
        let mut d = Dictionary::new();
        d.set("Type", Object::Name(b"Annot".to_vec()));
        d.set("Subtype", Object::Name(b"Redact".to_vec()));
        d.set(
            "Rect",
            Object::Array(vec![
                Object::Real(x1),
                Object::Real(y1),
                Object::Real(x2),
                Object::Real(y2),
            ]),
        );
        // Interior color: solid black (0, 0, 0 in RGB).
        d.set(
            "IC",
            Object::Array(vec![
                Object::Real(0.0),
                Object::Real(0.0),
                Object::Real(0.0),
            ]),
        );
        // Appearance stream.
        let mut ap = Dictionary::new();
        ap.set("N", Object::Reference(ap_id));
        d.set("AP", Object::Dictionary(ap));
        // No visible border.
        d.set(
            "Border",
            Object::Array(vec![
                Object::Integer(0),
                Object::Integer(0),
                Object::Integer(0),
            ]),
        );
        // Print flag.
        d.set("F", Object::Integer(4));

        let annot_id = self.doc.inner.add_object(Object::Dictionary(d));
        append_annotation_to_page(&mut self.doc.inner, self.page_id, annot_id)
    }

    /// Adds a text (sticky-note) annotation at the given point.
    ///
    /// `point` is `[x, y]` in PDF points (origin: bottom-left). The icon
    /// appears at the given position; viewers typically display a 20×20 pt icon.
    /// `contents` is the note body (Unicode, UTF-16BE encoded in the PDF).
    ///
    /// # Errors
    /// Returns [`Error::InvalidInput`] if any coordinate is NaN/Infinity.
    pub fn add_sticky_note(&mut self, point: [f32; 2], contents: &str) -> Result<()> {
        check_finite(&[point[0], point[1]], "add_sticky_note")?;
        let mut d = Dictionary::new();
        d.set("Type", Object::Name(b"Annot".to_vec()));
        d.set("Subtype", Object::Name(b"Text".to_vec()));
        d.set(
            "Rect",
            Object::Array(vec![
                Object::Real(point[0]),
                Object::Real(point[1]),
                Object::Real(point[0] + 20.0),
                Object::Real(point[1] + 20.0),
            ]),
        );
        d.set("Contents", pdf_text_string(contents));
        d.set("Open", Object::Boolean(false));
        let annot_id = self.doc.inner.add_object(Object::Dictionary(d));
        append_annotation_to_page(&mut self.doc.inner, self.page_id, annot_id)
    }

    // -----------------------------------------------------------------------

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
        color: impl Into<Color>,
        line_height: f32,
    ) -> Result<()> {
        let color = color.into();
        self.add_text_box_aligned(
            text,
            font,
            rect,
            font_size,
            color,
            line_height,
            VerticalAlign::Top,
        )
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
        color: impl Into<Color>,
        line_height: f32,
        align: VerticalAlign,
    ) -> Result<()> {
        let color = color.into();
        check_finite(
            &[rect[0], rect[1], rect[2], rect[3], font_size, line_height],
            "add_text_box_aligned",
        )?;
        if rect[2] <= 0.0 || rect[3] <= 0.0 {
            return Ok(());
        }

        let raw = self
            .doc
            .raw_fonts
            .get(font.0 as usize)
            .ok_or(Error::InvalidFont(font.0))?;
        let face = Face::parse(&raw.ttf_bytes, 0).map_err(|e| Error::FontParse(e.to_string()))?;

        let box_width = rect[2];
        let effective_lh = if line_height <= 0.0 {
            font_size * 1.2
        } else {
            line_height
        };

        let mut all_lines: Vec<String> = Vec::new();
        for paragraph in text.split('\n') {
            all_lines.extend(wrap_paragraph(paragraph, &face, font_size, box_width));
        }

        let n = all_lines.len() as f32;
        let start_y = match align {
            VerticalAlign::Top => rect[1] + rect[3] - font_size,
            VerticalAlign::Bottom => rect[1] + (n - 1.0) * effective_lh,
            VerticalAlign::Center => {
                rect[1] + rect[3] / 2.0 + ((n - 1.0) * effective_lh - font_size) / 2.0
            }
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
                rotation_degrees: 0.0,
                bold: false,
                italic: false,
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

    // -----------------------------------------------------------------------
    // Page boxes (CropBox, MediaBox, TrimBox, BleedBox, ArtBox)
    // -----------------------------------------------------------------------

    /// Returns the `/CropBox` of this page in `[x, y, width, height]` format (PDF points).
    ///
    /// Returns `None` when no `/CropBox` is set on this page (the visible area is
    /// then determined by the [`MediaBox`](Self::media_box)).
    pub fn crop_box(&self) -> Result<Option<[f32; 4]>> {
        read_page_box(&self.doc.inner, self.page_id, b"CropBox")
    }

    /// Sets the `/CropBox` of this page. The crop box clips the visible area.
    ///
    /// `rect` is `[x, y, width, height]` in PDF points (origin: bottom-left).
    ///
    /// # Errors
    /// Returns [`Error::InvalidInput`] if coordinates contain NaN/Infinity.
    pub fn set_crop_box(&mut self, rect: [f32; 4]) -> Result<()> {
        check_finite(&rect, "set_crop_box")?;
        set_page_box(&mut self.doc.inner, self.page_id, b"CropBox", rect)
    }

    /// Returns the `/MediaBox` of this page in `[x, y, width, height]` format (PDF points).
    ///
    /// Walks up the page tree to find an inherited value, like [`size`](Self::size) does.
    pub fn media_box(&self) -> Result<[f32; 4]> {
        let mut current_id = self.page_id;
        for _ in 0..32 {
            let (mb_opt, parent_opt) = {
                let dict = self.doc.inner.get_object(current_id)?.as_dict()?;
                (
                    dict.get(b"MediaBox").ok().cloned(),
                    dict.get(b"Parent").ok().cloned(),
                )
            };
            if let Some(mb) = mb_opt {
                return parse_box_array(&mb);
            }
            match parent_opt {
                Some(Object::Reference(id)) => current_id = id,
                _ => break,
            }
        }
        Err(Error::Pdf(lopdf::Error::DictKey("MediaBox".to_string())))
    }

    /// Overrides the `/MediaBox` of this page.
    ///
    /// `rect` is `[x, y, width, height]` in PDF points.
    ///
    /// # Errors
    /// Returns [`Error::InvalidInput`] if coordinates contain NaN/Infinity.
    pub fn set_media_box(&mut self, rect: [f32; 4]) -> Result<()> {
        check_finite(&rect, "set_media_box")?;
        set_page_box(&mut self.doc.inner, self.page_id, b"MediaBox", rect)
    }

    /// Returns the `/TrimBox` of this page, or `None` if unset.
    pub fn trim_box(&self) -> Result<Option<[f32; 4]>> {
        read_page_box(&self.doc.inner, self.page_id, b"TrimBox")
    }

    /// Sets the `/TrimBox` of this page (intended print area after trimming).
    ///
    /// # Errors
    /// Returns [`Error::InvalidInput`] if coordinates contain NaN/Infinity.
    pub fn set_trim_box(&mut self, rect: [f32; 4]) -> Result<()> {
        check_finite(&rect, "set_trim_box")?;
        set_page_box(&mut self.doc.inner, self.page_id, b"TrimBox", rect)
    }

    /// Returns the `/BleedBox` of this page, or `None` if unset.
    pub fn bleed_box(&self) -> Result<Option<[f32; 4]>> {
        read_page_box(&self.doc.inner, self.page_id, b"BleedBox")
    }

    /// Sets the `/BleedBox` of this page (area for bleed in print production).
    ///
    /// # Errors
    /// Returns [`Error::InvalidInput`] if coordinates contain NaN/Infinity.
    pub fn set_bleed_box(&mut self, rect: [f32; 4]) -> Result<()> {
        check_finite(&rect, "set_bleed_box")?;
        set_page_box(&mut self.doc.inner, self.page_id, b"BleedBox", rect)
    }

    // -----------------------------------------------------------------------

    fn push_op(&mut self, op: PendingOp) {
        let page_id = self.page_id;
        match self.doc.pending.iter_mut().find(|p| p.page_id == page_id) {
            Some(p) => p.ops.push(op),
            None => self.doc.pending.push(PendingPage {
                page_id,
                ops: vec![op],
            }),
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
    pub fn add_rect(
        &mut self,
        rect: [f32; 4],
        color: impl Into<Color>,
        opacity: f32,
    ) -> Result<()> {
        let color = color.into();
        check_finite(&[rect[0], rect[1], rect[2], rect[3], opacity], "add_rect")?;
        self.push_op(PendingOp::Draw(crate::draw::DrawOp::Rect {
            rect,
            color,
            opacity,
        }));
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
        color: impl Into<Color>,
        line_width: f32,
        opacity: f32,
    ) -> Result<()> {
        let color = color.into();
        check_finite(
            &[rect[0], rect[1], rect[2], rect[3], line_width, opacity],
            "add_rect_stroke",
        )?;
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
    /// `filled = true` fills the polygon. `stroke_width > 0` strokes the outline.
    /// Both can be active simultaneously (`B` operator).
    pub fn add_polygon(
        &mut self,
        points: &[[f32; 2]],
        color: impl Into<Color>,
        opacity: f32,
        filled: bool,
        stroke_width: f32,
    ) -> Result<()> {
        let color = color.into();
        {
            let coords: Vec<f32> = points.iter().flat_map(|p| p.iter().copied()).collect();
            check_finite(&coords, "add_polygon points")?;
        }
        check_finite(&[opacity, stroke_width], "add_polygon")?;
        self.push_op(PendingOp::Draw(crate::draw::DrawOp::Polygon {
            points: points.to_vec(),
            color,
            opacity,
            filled,
            stroke_width,
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
        color: impl Into<Color>,
        line_width: f32,
        opacity: f32,
    ) -> Result<()> {
        let color = color.into();
        check_finite(
            &[from[0], from[1], to[0], to[1], line_width, opacity],
            "add_line",
        )?;
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
        color: impl Into<Color>,
        line_width: f32,
        opacity: f32,
    ) -> Result<()> {
        let color = color.into();
        if points.len() < 2 {
            return Ok(());
        }
        {
            let coords: Vec<f32> = points.iter().flat_map(|p| p.iter().copied()).collect();
            check_finite(&coords, "add_polyline points")?;
        }
        check_finite(&[line_width, opacity], "add_polyline")?;
        self.push_op(PendingOp::Draw(crate::draw::DrawOp::Polyline {
            points: points.to_vec(),
            color,
            width: line_width,
            opacity,
        }));
        Ok(())
    }

    /// Overlays an ellipse (or circle) on this page.
    ///
    /// `rect` is `[x, y, width, height]` — the bounding box of the ellipse in PDF points
    /// (origin: bottom-left). For a circle, set `width == height`.
    /// `color` is `[r, g, b]` in `0.0..=1.0`.
    /// `opacity` is in `0.0` (transparent) to `1.0` (opaque).
    /// `filled = true` fills the ellipse. `stroke_width > 0` strokes the outline.
    /// Both can be active simultaneously (`B` operator).
    pub fn add_ellipse(
        &mut self,
        rect: [f32; 4],
        color: impl Into<Color>,
        opacity: f32,
        filled: bool,
        stroke_width: f32,
    ) -> Result<()> {
        let color = color.into();
        check_finite(
            &[rect[0], rect[1], rect[2], rect[3], opacity, stroke_width],
            "add_ellipse",
        )?;
        if rect[2] <= 0.0 || rect[3] <= 0.0 {
            return Err(Error::InvalidInput(
                "add_ellipse: width and height must be positive".into(),
            ));
        }
        self.push_op(PendingOp::Draw(crate::draw::DrawOp::Ellipse {
            rect,
            color,
            opacity,
            filled,
            stroke_width,
        }));
        Ok(())
    }

    /// Overlays an open or closed path on this page.
    ///
    /// `points` is a slice of `[x, y]` vertices in PDF points (origin: bottom-left).
    /// At least 2 points are required; fewer produce no output.
    /// `closed = true` closes the path (`h`); `closed = false` leaves it open.
    /// `color` is `[r, g, b]` in `0.0..=1.0`.
    /// `filled = true` fills the interior. `stroke_width > 0` strokes the outline.
    /// Both can be active simultaneously (`B` operator).
    /// `opacity` is in `0.0..=1.0`.
    pub fn add_path(
        &mut self,
        points: &[[f32; 2]],
        closed: bool,
        color: impl Into<Color>,
        filled: bool,
        stroke_width: f32,
        opacity: f32,
    ) -> Result<()> {
        let color = color.into();
        if points.len() < 2 {
            return Ok(());
        }
        {
            let coords: Vec<f32> = points.iter().flat_map(|p| p.iter().copied()).collect();
            check_finite(&coords, "add_path points")?;
        }
        check_finite(&[stroke_width, opacity], "add_path")?;
        self.push_op(PendingOp::Draw(crate::draw::DrawOp::Path {
            points: points.to_vec(),
            closed,
            color,
            opacity,
            filled,
            stroke_width,
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
        color: impl Into<Color>,
        opacity: f32,
    ) -> Result<()> {
        let color = color.into();
        check_finite(
            &[position[0], position[1], font_size, opacity],
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
            rotation_degrees: 0.0,
            bold: false,
            italic: false,
        });
        Ok(())
    }

    /// Overlays visible text with rotation and opacity on this page.
    ///
    /// `rotation_degrees` rotates the text counter-clockwise around `position`.
    /// Use `0.0` for horizontal text. Internally emits a PDF `Tm` text matrix when
    /// `rotation_degrees != 0.0`, enabling arbitrary angles including CJK watermarks.
    ///
    /// `opacity` is in `0.0` (transparent) to `1.0` (opaque).
    #[allow(clippy::too_many_arguments)]
    pub fn add_text_with_rotation(
        &mut self,
        text: &str,
        font: FontHandle,
        position: [f32; 2],
        font_size: f32,
        color: impl Into<Color>,
        opacity: f32,
        rotation_degrees: f32,
    ) -> Result<()> {
        let color = color.into();
        check_finite(
            &[
                position[0],
                position[1],
                font_size,
                opacity,
                rotation_degrees,
            ],
            "add_text_with_rotation",
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
            rotation_degrees,
            bold: false,
            italic: false,
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
        color: impl Into<Color>,
        line_height: f32,
        opacity: f32,
    ) -> Result<()> {
        let color = color.into();
        check_finite(
            &[
                rect[0],
                rect[1],
                rect[2],
                rect[3],
                font_size,
                line_height,
                opacity,
            ],
            "add_text_box_with_opacity",
        )?;
        if rect[2] <= 0.0 || rect[3] <= 0.0 {
            return Ok(());
        }

        let raw = self
            .doc
            .raw_fonts
            .get(font.0 as usize)
            .ok_or(Error::InvalidFont(font.0))?;
        let face = Face::parse(&raw.ttf_bytes, 0).map_err(|e| Error::FontParse(e.to_string()))?;

        let box_width = rect[2];
        let effective_lh = if line_height <= 0.0 {
            font_size * 1.2
        } else {
            line_height
        };

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
                rotation_degrees: 0.0,
                bold: false,
                italic: false,
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
        check_finite(
            &[rect[0], rect[1], rect[2], rect[3], opacity],
            "add_image_with_opacity",
        )?;
        self.push_op(PendingOp::Draw(crate::draw::DrawOp::Image {
            bytes: image_bytes.to_vec(),
            rect,
            opacity,
        }));
        Ok(())
    }
}

/// Encodes a Rust `&str` as a PDF text string.
///
/// ASCII-only strings use a literal byte encoding. Strings containing non-ASCII
/// characters (e.g. CJK) are encoded as UTF-16BE with a 0xFE 0xFF BOM prefix,
/// which is the standard for PDF `/Title` and other text string fields.
fn pdf_text_string(s: &str) -> Object {
    use lopdf::StringFormat;
    if s.is_ascii() {
        return Object::String(s.as_bytes().to_vec(), StringFormat::Literal);
    }
    let mut bytes: Vec<u8> = vec![0xFE, 0xFF]; // UTF-16BE BOM
    for unit in s.encode_utf16() {
        bytes.push((unit >> 8) as u8);
        bytes.push((unit & 0xFF) as u8);
    }
    Object::String(bytes, StringFormat::Literal)
}

/// Returns the ObjectId of the /AcroForm dictionary if one exists.
fn acroform_id(doc: &lopdf::Document) -> Option<ObjectId> {
    let root_ref = doc.trailer.get(b"Root").ok()?.as_reference().ok()?;
    let catalog = doc.get_object(root_ref).ok()?.as_dict().ok()?;
    catalog.get(b"AcroForm").ok()?.as_reference().ok()
}

/// Ensures an /AcroForm entry exists in the document catalog.
/// Returns the ObjectId of the /AcroForm dictionary (either existing or newly created).
fn ensure_acroform(doc: &mut lopdf::Document) -> Result<ObjectId> {
    // Check if AcroForm already exists
    if let Some(id) = acroform_id(doc) {
        return Ok(id);
    }

    // Create a new /AcroForm dictionary with an empty /Fields array
    let mut acroform_dict = Dictionary::new();
    acroform_dict.set("Fields", Object::Array(Vec::new()));
    let acroform_id = doc.add_object(Object::Dictionary(acroform_dict));

    // Get the catalog and add the /AcroForm reference
    let root_ref = doc
        .trailer
        .get(b"Root")
        .ok()
        .and_then(|o| o.as_reference().ok())
        .ok_or(Error::InvalidInput("catalog not found".into()))?;

    let catalog = doc.get_object_mut(root_ref)?.as_dict_mut()?;
    catalog.set("AcroForm", Object::Reference(acroform_id));

    Ok(acroform_id)
}

/// Recursively collects `FormField` entries from a PDF field array.
fn collect_fields_recursive(
    doc: &lopdf::Document,
    field_refs: &[Object],
    parent_name: &str,
    out: &mut Vec<FormField>,
) {
    for obj in field_refs {
        let id = match obj {
            Object::Reference(id) => *id,
            _ => continue,
        };
        let Ok(field_obj) = doc.get_object(id) else {
            continue;
        };
        let Ok(fd) = field_obj.as_dict() else {
            continue;
        };

        let partial = fd
            .get(b"T")
            .ok()
            .and_then(|o| match o {
                Object::String(b, _) => String::from_utf8(b.clone()).ok().or_else(|| {
                    if b.starts_with(&[0xFE, 0xFF]) {
                        let units: Vec<u16> = b[2..]
                            .chunks(2)
                            .map(|c| u16::from_be_bytes([c[0], c.get(1).copied().unwrap_or(0)]))
                            .collect();
                        String::from_utf16(&units).ok()
                    } else {
                        None
                    }
                }),
                _ => None,
            })
            .unwrap_or_default();

        let full_name = if parent_name.is_empty() {
            partial.clone()
        } else if partial.is_empty() {
            parent_name.to_owned()
        } else {
            format!("{parent_name}.{partial}")
        };

        // If /Kids present: intermediate node, recurse.
        if let Ok(kids_obj) = fd.get(b"Kids") {
            let kids: Vec<Object> = match kids_obj {
                Object::Array(arr) => arr.clone(),
                Object::Reference(kid_id) => doc
                    .get_object(*kid_id)
                    .ok()
                    .and_then(|o| {
                        if let Object::Array(a) = o {
                            Some(a.clone())
                        } else {
                            None
                        }
                    })
                    .unwrap_or_default(),
                _ => vec![],
            };
            collect_fields_recursive(doc, &kids, &full_name, out);
            continue;
        }

        // Leaf field.
        let ft = fd.get(b"FT").ok().and_then(|o| {
            if let Object::Name(n) = o {
                Some(n.as_slice())
            } else {
                None
            }
        });

        let field_type = match ft {
            Some(b"Tx") => FieldType::Text,
            Some(b"Btn") => {
                let flags = fd
                    .get(b"Ff")
                    .ok()
                    .and_then(|o| o.as_i64().ok())
                    .unwrap_or(0);
                if flags & (1 << 15) != 0 {
                    FieldType::Radio
                } else {
                    FieldType::Checkbox
                }
            }
            Some(b"Ch") => FieldType::Choice,
            Some(b"Sig") => FieldType::Signature,
            _ => FieldType::Unknown,
        };

        let value = fd
            .get(b"V")
            .ok()
            .map(|v| match v {
                Object::String(b, _) => {
                    if b.starts_with(&[0xFE, 0xFF]) {
                        let units: Vec<u16> = b[2..]
                            .chunks(2)
                            .map(|c| u16::from_be_bytes([c[0], c.get(1).copied().unwrap_or(0)]))
                            .collect();
                        String::from_utf16(&units).unwrap_or_default()
                    } else {
                        String::from_utf8(b.clone()).unwrap_or_default()
                    }
                }
                Object::Name(n) => String::from_utf8_lossy(n).into_owned(),
                _ => String::new(),
            })
            .unwrap_or_default();

        if !full_name.is_empty() {
            out.push(FormField {
                name: full_name,
                field_type,
                value,
            });
        }
    }
}

/// Collects (ObjectId, FieldType, full_name) for all leaf fields under /AcroForm.
fn collect_field_ids(
    doc: &lopdf::Document,
    acroform_id: ObjectId,
) -> Vec<(ObjectId, FieldType, String)> {
    let Ok(acroform) = doc.get_object(acroform_id).and_then(|o| o.as_dict()) else {
        return vec![];
    };
    let field_refs: Vec<Object> = match acroform.get(b"Fields") {
        Ok(Object::Array(arr)) => arr.clone(),
        Ok(Object::Reference(id)) => doc
            .get_object(*id)
            .ok()
            .and_then(|o| {
                if let Object::Array(a) = o {
                    Some(a.clone())
                } else {
                    None
                }
            })
            .unwrap_or_default(),
        _ => return vec![],
    };

    let mut out = Vec::new();
    collect_field_ids_recursive(doc, &field_refs, "", &mut out);
    out
}

fn collect_field_ids_recursive(
    doc: &lopdf::Document,
    field_refs: &[Object],
    parent_name: &str,
    out: &mut Vec<(ObjectId, FieldType, String)>,
) {
    for obj in field_refs {
        let id = match obj {
            Object::Reference(id) => *id,
            _ => continue,
        };
        let Ok(field_obj) = doc.get_object(id) else {
            continue;
        };
        let Ok(fd) = field_obj.as_dict() else {
            continue;
        };

        let partial = fd
            .get(b"T")
            .ok()
            .and_then(lopdf_string_to_rust)
            .unwrap_or_default();

        let full_name = if parent_name.is_empty() {
            partial.clone()
        } else if partial.is_empty() {
            parent_name.to_owned()
        } else {
            format!("{parent_name}.{partial}")
        };

        if let Ok(kids_obj) = fd.get(b"Kids") {
            let kids: Vec<Object> = match kids_obj {
                Object::Array(arr) => arr.clone(),
                Object::Reference(kid_id) => doc
                    .get_object(*kid_id)
                    .ok()
                    .and_then(|o| {
                        if let Object::Array(a) = o {
                            Some(a.clone())
                        } else {
                            None
                        }
                    })
                    .unwrap_or_default(),
                _ => vec![],
            };
            collect_field_ids_recursive(doc, &kids, &full_name, out);
            continue;
        }

        let ft = fd.get(b"FT").ok().and_then(|o| {
            if let Object::Name(n) = o {
                Some(n.as_slice())
            } else {
                None
            }
        });
        let field_type = match ft {
            Some(b"Tx") => FieldType::Text,
            Some(b"Btn") => {
                let flags = fd
                    .get(b"Ff")
                    .ok()
                    .and_then(|o| o.as_i64().ok())
                    .unwrap_or(0);
                if flags & (1 << 15) != 0 {
                    FieldType::Radio
                } else {
                    FieldType::Checkbox
                }
            }
            Some(b"Ch") => FieldType::Choice,
            Some(b"Sig") => FieldType::Signature,
            _ => FieldType::Unknown,
        };

        if !full_name.is_empty() {
            out.push((id, field_type, full_name));
        }
    }
}

/// Builds a markup annotation dictionary (Highlight, Underline, StrikeOut).
fn build_markup_annot(subtype: &[u8], rect: [f32; 4], color: Color) -> Dictionary {
    let x2 = rect[0] + rect[2];
    let y2 = rect[1] + rect[3];
    let mut d = Dictionary::new();
    d.set("Type", Object::Name(b"Annot".to_vec()));
    d.set("Subtype", Object::Name(subtype.to_vec()));
    d.set(
        "Rect",
        Object::Array(vec![
            Object::Real(rect[0]),
            Object::Real(rect[1]),
            Object::Real(x2),
            Object::Real(y2),
        ]),
    );
    // QuadPoints: upper-left, upper-right, lower-left, lower-right (Acrobat convention)
    d.set(
        "QuadPoints",
        Object::Array(vec![
            Object::Real(rect[0]),
            Object::Real(y2),
            Object::Real(x2),
            Object::Real(y2),
            Object::Real(rect[0]),
            Object::Real(rect[1]),
            Object::Real(x2),
            Object::Real(rect[1]),
        ]),
    );
    let color_array = match color {
        Color::Rgb(c) => vec![Object::Real(c[0]), Object::Real(c[1]), Object::Real(c[2])],
        Color::Cmyk(c) => vec![
            Object::Real(c[0]),
            Object::Real(c[1]),
            Object::Real(c[2]),
            Object::Real(c[3]),
        ],
    };
    d.set("C", Object::Array(color_array));
    d.set(
        "Border",
        Object::Array(vec![
            Object::Integer(0),
            Object::Integer(0),
            Object::Integer(0),
        ]),
    );
    d
}

/// Builds the common fields of a /Link annotation dictionary (without the /A or /Dest key).
fn build_link_annot_base(rect: [f32; 4]) -> Dictionary {
    let mut d = Dictionary::new();
    d.set("Type", Object::Name(b"Annot".to_vec()));
    d.set("Subtype", Object::Name(b"Link".to_vec()));
    d.set(
        "Rect",
        Object::Array(vec![
            Object::Real(rect[0]),
            Object::Real(rect[1]),
            Object::Real(rect[0] + rect[2]),
            Object::Real(rect[1] + rect[3]),
        ]),
    );
    // No visible border ([0 0 0] = no border)
    d.set(
        "Border",
        Object::Array(vec![
            Object::Integer(0),
            Object::Integer(0),
            Object::Integer(0),
        ]),
    );
    d
}

/// Appends an annotation object reference to the /Annots array of a page dictionary.
///
/// Handles both the case where /Annots is a direct array and where it is an
/// indirect reference to an array object.
fn append_annotation_to_page(
    doc: &mut lopdf::Document,
    page_id: ObjectId,
    annot_id: ObjectId,
) -> Result<()> {
    let new_ref = Object::Reference(annot_id);

    // Read the current /Annots value without borrowing `doc` mutably.
    let annots_val = doc
        .get_object(page_id)?
        .as_dict()?
        .get(b"Annots")
        .ok()
        .cloned();

    match annots_val {
        Some(Object::Array(mut arr)) => {
            arr.push(new_ref);
            doc.get_object_mut(page_id)?
                .as_dict_mut()?
                .set("Annots", Object::Array(arr));
        }
        Some(Object::Reference(arr_id)) => {
            // /Annots points to an indirect array object.
            let is_array = doc
                .get_object(arr_id)
                .ok()
                .map(|o| matches!(o, Object::Array(_)))
                .unwrap_or(false);
            if is_array {
                doc.get_object_mut(arr_id)?.as_array_mut()?.push(new_ref);
            } else {
                // Indirect reference doesn't point to an array — replace with direct array.
                doc.get_object_mut(page_id)?
                    .as_dict_mut()?
                    .set("Annots", Object::Array(vec![new_ref]));
            }
        }
        _ => {
            // No /Annots entry (or malformed) — create a fresh direct array.
            doc.get_object_mut(page_id)?
                .as_dict_mut()?
                .set("Annots", Object::Array(vec![new_ref]));
        }
    }
    Ok(())
}

/// Reads a named page box (e.g. CropBox) from the page dict.
/// Returns `None` when the key is absent. Parses `[x1 y1 x2 y2]` → `[x, y, w, h]`.
fn read_page_box(doc: &lopdf::Document, page_id: ObjectId, key: &[u8]) -> Result<Option<[f32; 4]>> {
    let dict = doc.get_object(page_id)?.as_dict()?;
    match dict.get(key).ok().cloned() {
        Some(Object::Reference(ref_id)) => parse_box_array(doc.get_object(ref_id)?).map(Some),
        Some(obj) => parse_box_array(&obj).map(Some),
        None => Ok(None),
    }
}

fn parse_box_array(obj: &Object) -> Result<[f32; 4]> {
    let arr = obj.as_array()?;
    if arr.len() < 4 {
        return Err(Error::Pdf(lopdf::Error::DictKey(
            "box array too short".to_string(),
        )));
    }
    let get = |i: usize| -> f32 {
        match &arr[i] {
            Object::Integer(v) => *v as f32,
            Object::Real(v) => *v,
            _ => 0.0,
        }
    };
    let (x1, y1, x2, y2) = (get(0), get(1), get(2), get(3));
    Ok([x1, y1, x2 - x1, y2 - y1])
}

/// Writes a named page box (e.g. CropBox) to the page dict.
/// Accepts `[x, y, w, h]` and stores as `[x1 y1 x2 y2]`.
fn set_page_box(
    doc: &mut lopdf::Document,
    page_id: ObjectId,
    key: &[u8],
    rect: [f32; 4],
) -> Result<()> {
    let box_arr = Object::Array(vec![
        Object::Real(rect[0]),
        Object::Real(rect[1]),
        Object::Real(rect[0] + rect[2]),
        Object::Real(rect[1] + rect[3]),
    ]);
    doc.get_object_mut(page_id)?
        .as_dict_mut()?
        .set(key, box_arr);
    Ok(())
}

/// Returns a 16-byte pseudo-random document ID using system time + PID.
/// Used as the /ID trailer entry required by PDF encryption (RC4/AES).
fn generate_file_id() -> [u8; 16] {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let pid = std::process::id() as u128;
    // LCG mix so that docs saved at the same nanosecond differ.
    // Mix time + PID using Knuth's MMIX LCG constants (a=6364136223846793005,
    // c=1442695040888963407) so that docs saved in the same process at similar
    // times produce distinct IDs. Not cryptographically secure, but sufficient
    // for PDF's /ID uniqueness requirement (PDF spec §14.4).
    let mixed = nanos
        .wrapping_mul(6364136223846793005u128)
        .wrapping_add(pid.wrapping_mul(1442695040888963407u128));
    let mut id = [0u8; 16];
    id.copy_from_slice(&mixed.to_le_bytes());
    id
}

fn map_lopdf_password_err(e: lopdf::Error) -> Error {
    match e {
        lopdf::Error::InvalidPassword => Error::WrongPassword,
        lopdf::Error::IO(io_err) => Error::Io(io_err),
        other => Error::Pdf(other),
    }
}

fn check_finite(values: &[f32], label: &str) -> Result<()> {
    if values.iter().any(|v| !v.is_finite()) {
        return Err(Error::InvalidInput(format!(
            "{label} contains NaN or Infinity"
        )));
    }
    Ok(())
}

fn check_positive_size(width: f32, height: f32, label: &str) -> Result<()> {
    if width <= 0.0 || height <= 0.0 {
        return Err(Error::InvalidInput(format!(
            "{label}: rect width and height must be positive, got ({width}, {height})"
        )));
    }
    Ok(())
}

/// Returns true for characters that can line-break at any position (CJK scripts).
pub(crate) fn is_cjk(ch: char) -> bool {
    matches!(
        ch as u32,
        0x1100..=0x11FF    // Hangul Jamo
        | 0x3000..=0x9FFF  // CJK unified ideographs, hiragana, katakana, etc.
        | 0xA960..=0xA97F  // Hangul Jamo Extended-A
        | 0xAC00..=0xD7FF  // Hangul syllables + Jamo Extended-B
        | 0xF900..=0xFAFF  // CJK compatibility ideographs
        | 0xFE30..=0xFE4F  // CJK compatibility forms
        | 0xFF00..=0xFFEF  // fullwidth / halfwidth forms
        | 0x20000..=0x2A6DF | 0x2A700..=0x2CEAF  // CJK extension B / C / D
    )
}

/// Width of one character in PDF points given the font face and font size.
/// Returns None if the character is not present in the font (no glyph mapping).
pub fn glyph_advance_pt(face: &Face, ch: char, font_size: f32) -> Option<f32> {
    let upem = face.units_per_em() as f32;
    face.glyph_index(ch)
        .and_then(|g| face.glyph_hor_advance(g))
        .map(|adv| adv as f32 * font_size / upem)
}

/// Calculate the total width of a text string in PDF points from raw TTF bytes.
///
/// This helper is useful for checking text overflow without needing access to Font objects.
/// Returns None if the font bytes are invalid or if any character is missing from the font.
pub fn calculate_text_width(text: &str, font_bytes: &[u8], font_size: f32) -> Option<f32> {
    let face = ttf_parser::Face::parse(font_bytes, 0).ok()?;
    let mut width = 0.0;
    for ch in text.chars() {
        width += glyph_advance_pt(&face, ch, font_size)?;
    }
    Some(width)
}

/// Greedy line-breaking for a single paragraph (no embedded newlines).
pub fn wrap_paragraph(paragraph: &str, face: &Face, font_size: f32, box_width: f32) -> Vec<String> {
    // Validate inputs.
    // If font_size or box_width is invalid, return the paragraph as a single line.
    if !font_size.is_finite() || font_size <= 0.0 || !box_width.is_finite() || box_width <= 0.0 {
        return if paragraph.is_empty() {
            Vec::new()
        } else {
            vec![paragraph.to_owned()]
        };
    }

    let mut lines: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut current_w: f32 = 0.0;
    // byte index of last ASCII space in `current`; width after that space (= start of next word)
    let mut last_space_byte: Option<usize> = None;
    let mut width_at_word_start: f32 = 0.0;

    for ch in paragraph.chars() {
        let ch_w = glyph_advance_pt(face, ch, font_size).unwrap_or(font_size * 0.5);

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

/// Materialize inherited PDF page attributes onto `page_id` before the page's
/// `/Parent` is changed (i.e., before the tree is flattened).
///
/// The PDF spec allows `/MediaBox`, `/CropBox`, `/Rotate`, `/Resources`, and
/// `/UserUnit` to be placed on intermediate `/Pages` nodes and inherited by
/// descendant pages. When those intermediate nodes are bypassed (by re-parenting
/// pages directly to the root), any values they hold are no longer reachable via
/// the inheritance chain. This function copies the missing values directly onto
/// the page dict so they survive the re-parenting.
///
/// Closest ancestor wins: the first ancestor to provide a value for a given key
/// is used; outer ancestors' values for the same key are ignored.
fn realize_page_inherited_attrs(doc: &mut lopdf::Document, page_id: ObjectId) -> Result<()> {
    const INHERITABLE: &[&[u8]] = &[
        b"MediaBox",
        b"CropBox",
        b"Rotate",
        b"Resources",
        b"UserUnit",
    ];

    // Walk up the parent chain and collect attrs missing from the page itself.
    let mut to_apply: Vec<(Vec<u8>, Object)> = Vec::new();
    let mut cursor = page_id;
    let mut depth = 0u32;

    loop {
        if depth > 64 {
            break; // cycle / pathological-depth guard
        }
        depth += 1;

        // Get cursor's /Parent reference.
        let parent_id = match doc
            .get_object(cursor)
            .ok()
            .and_then(|o| o.as_dict().ok())
            .and_then(|d| d.get(b"Parent").ok())
            .and_then(|o| {
                if let Object::Reference(id) = o {
                    Some(*id)
                } else {
                    None
                }
            }) {
            Some(id) => id,
            None => break,
        };

        // Only inherit from /Pages nodes.
        let parent_is_pages = doc
            .get_object(parent_id)
            .ok()
            .and_then(|o| o.as_dict().ok())
            .and_then(|d| d.get(b"Type").ok())
            .and_then(|o| {
                if let Object::Name(n) = o {
                    Some(n.as_slice() == b"Pages")
                } else {
                    None
                }
            })
            .unwrap_or(false);

        if !parent_is_pages {
            break;
        }

        // Clone the parent dict so we can check its keys without holding a borrow.
        let parent_dict = match doc
            .get_object(parent_id)
            .ok()
            .and_then(|o| o.as_dict().ok())
        {
            Some(d) => d.clone(),
            None => break,
        };
        // Clone the page dict to check which keys are already present.
        let page_dict = match doc.get_object(page_id).ok().and_then(|o| o.as_dict().ok()) {
            Some(d) => d.clone(),
            None => break,
        };

        for &key in INHERITABLE {
            // Already on the page itself — skip.
            if page_dict.get(key).is_ok() {
                continue;
            }
            // Already queued from a closer ancestor — skip.
            if to_apply.iter().any(|(k, _)| k.as_slice() == key) {
                continue;
            }
            // Inherit from this ancestor.
            if let Ok(val) = parent_dict.get(key) {
                to_apply.push((key.to_vec(), val.clone()));
            }
        }

        cursor = parent_id;
    }

    // Write collected attributes onto the page dict.
    if !to_apply.is_empty() {
        let page_dict = doc.get_object_mut(page_id)?.as_dict_mut()?;
        for (key, val) in to_apply {
            page_dict.set(key, val);
        }
    }

    Ok(())
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
            let is_array = doc
                .get_object(r)
                .ok()
                .map(|o| matches!(o, Object::Array(_)))
                .unwrap_or(false);
            if is_array {
                let arr_obj = doc.get_object_mut(r)?.as_array_mut()?;
                arr_obj.push(new_ref);
            } else {
                let arr = Object::Array(vec![Object::Reference(r), new_ref]);
                doc.get_object_mut(page_id)?
                    .as_dict_mut()?
                    .set("Contents", arr);
            }
        }
        Some(Object::Array(mut arr)) => {
            arr.push(new_ref);
            doc.get_object_mut(page_id)?
                .as_dict_mut()?
                .set("Contents", Object::Array(arr));
        }
        None => {
            doc.get_object_mut(page_id)?
                .as_dict_mut()?
                .set("Contents", new_ref);
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
    with_resources_dict_mut(doc, page_id, |res| match res.get_mut(b"ExtGState") {
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
    with_resources_dict_mut(doc, page_id, |res| match res.get_mut(b"XObject") {
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

    #[test]
    fn is_cjk_cjk_unified_ideographs() {
        assert!(is_cjk('日')); // U+65E5
        assert!(is_cjk('本')); // U+672C
        assert!(is_cjk('語')); // U+8A9E
    }

    #[test]
    fn is_cjk_hiragana_katakana() {
        assert!(is_cjk('あ')); // Hiragana U+3042
        assert!(is_cjk('ア')); // Katakana U+30A2
        assert!(is_cjk('ん')); // Hiragana U+3093
    }

    #[test]
    fn is_cjk_korean_hangul() {
        assert!(is_cjk('가')); // U+AC00
        assert!(is_cjk('나')); // U+B098
        assert!(is_cjk('힣')); // U+D7A3 (last Hangul syllable)
    }

    #[test]
    fn is_cjk_hangul_jamo() {
        assert!(is_cjk('ㄱ')); // Hangul Jamo U+1100
        assert!(is_cjk('ㅏ')); // Hangul Jamo U+1161
    }

    #[test]
    fn is_cjk_cjk_extension_planes() {
        assert!(is_cjk('\u{20000}')); // CJK Extension B U+20000
        assert!(is_cjk('\u{2A6D0}')); // CJK Extension C U+2A6D0
    }

    #[test]
    fn is_cjk_non_cjk_returns_false() {
        assert!(!is_cjk('a'));
        assert!(!is_cjk('A'));
        assert!(!is_cjk('1'));
        assert!(!is_cjk(' '));
        assert!(!is_cjk('é')); // Latin Extended
    }
}
