use lopdf::{Object, ObjectId};

use crate::error::{Error, Result};

/// Format of the bytes stored in [`PageImage::bytes`].
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PageImageFormat {
    /// Complete JPEG file bytes (DCTDecode stream).
    Jpeg,
    /// Complete PNG file bytes (all other raster formats).
    Png,
}

/// A raster image extracted from a PDF page.
///
/// Returned by [`Document::extract_page_image`](crate::Document::extract_page_image).
///
/// ## Scanned-PDF only
///
/// This API extracts an existing Image XObject from the page — it does **not**
/// rasterize the page. It works reliably only for scanned PDFs where each page
/// is a single raster image. Text and vector pages have no Image XObject and
/// will return [`Error::InvalidInput`](crate::Error::InvalidInput).
///
/// For general PDF-to-image rasterization (any PDF type), use a renderer such
/// as `pdfium-render`, which requires a C++ dependency.
#[non_exhaustive]
#[derive(Debug, Clone)]
pub struct PageImage {
    /// Image width in pixels.
    pub width: u32,
    /// Image height in pixels.
    pub height: u32,
    /// Raw image file bytes. JPEG bytes when `format == Jpeg`; PNG bytes when `format == Png`.
    pub bytes: Vec<u8>,
    /// Encoding of `bytes`.
    pub format: PageImageFormat,
}

/// Returns the primary filter name for a stream, handling both Name and Array forms.
fn filter_name(stream: &lopdf::Stream) -> Option<Vec<u8>> {
    match stream.dict.get(b"Filter").ok()? {
        Object::Name(n) => Some(n.clone()),
        // PDF allows /Filter to be a single-element array: [/FlateDecode]
        Object::Array(arr) => arr.first().and_then(|o| {
            if let Object::Name(n) = o {
                Some(n.clone())
            } else {
                None
            }
        }),
        _ => None,
    }
}

/// Collects the ObjectIds of all Image XObjects referenced from the page's /Resources.
fn page_image_xobjects(doc: &lopdf::Document, page_id: ObjectId) -> Vec<ObjectId> {
    (|| -> Option<Vec<ObjectId>> {
        let page_obj = doc.get_object(page_id).ok()?;
        let page_dict = page_obj.as_dict().ok()?;
        let resources_obj = page_dict.get(b"Resources").ok()?;
        let resources_dict = crate::extract::resolve_dict(doc, resources_obj)?;
        let xobj_obj = resources_dict.get(b"XObject").ok()?;
        let xobj_dict = crate::extract::resolve_dict(doc, xobj_obj)?;

        Some(
            xobj_dict
                .iter()
                .filter_map(|(_, val)| {
                    let Object::Reference(id) = val else {
                        return None;
                    };
                    let obj = doc.get_object(*id).ok()?;
                    let stream = obj.as_stream().ok()?;
                    let subtype = stream.dict.get(b"Subtype").ok().and_then(|o| {
                        if let Object::Name(n) = o {
                            Some(n.as_slice())
                        } else {
                            None
                        }
                    });
                    if subtype == Some(b"Image") {
                        Some(*id)
                    } else {
                        None
                    }
                })
                .collect(),
        )
    })()
    .unwrap_or_default()
}

/// Builds a `PageImage` from a single Image XObject.
///
/// - `DCTDecode` → JPEG bytes returned as-is.
/// - `FlateDecode` or no filter → raw pixels decoded then re-encoded as PNG.
/// - Anything else → `Error::InvalidInput`.
fn extract_xobject_image(doc: &lopdf::Document, id: ObjectId) -> Result<PageImage> {
    let obj = doc.get_object(id)?;
    let stream = obj.as_stream()?;

    let width = stream
        .dict
        .get(b"Width")
        .ok()
        .and_then(|o| o.as_i64().ok())
        .map(|n| n as u32)
        .unwrap_or(0);
    let height = stream
        .dict
        .get(b"Height")
        .ok()
        .and_then(|o| o.as_i64().ok())
        .map(|n| n as u32)
        .unwrap_or(0);

    let filter = filter_name(stream);

    match filter.as_deref() {
        Some(b"DCTDecode") => {
            let (w, h) = if width > 0 && height > 0 {
                (width, height)
            } else {
                crate::draw::image::parse_jpeg_dims(&stream.content)?
            };
            Ok(PageImage {
                width: w,
                height: h,
                bytes: stream.content.clone(),
                format: PageImageFormat::Jpeg,
            })
        }
        Some(b"FlateDecode") | None => {
            let raw_pixels = if filter.is_some() {
                let mut owned = stream.clone();
                owned.decompress()?;
                owned.content
            } else {
                stream.content.clone()
            };

            if width == 0 || height == 0 {
                return Err(Error::InvalidInput(
                    "Image XObject has no valid Width/Height".into(),
                ));
            }

            let channels: u32 = match stream.dict.get(b"ColorSpace").ok().and_then(|o| {
                if let Object::Name(n) = o {
                    Some(n.as_slice())
                } else {
                    None
                }
            }) {
                Some(b"DeviceGray") => 1,
                _ => 3, // DeviceRGB or unrecognised → assume RGB
            };

            let expected = width as usize * height as usize * channels as usize;
            if raw_pixels.len() != expected {
                return Err(Error::InvalidInput(format!(
                    "pixel buffer mismatch: expected {expected} bytes ({width}×{height}×{channels}ch), got {}",
                    raw_pixels.len()
                )));
            }

            let mut cursor = std::io::Cursor::new(Vec::new());
            if channels == 1 {
                let mut encoder = png::Encoder::new(&mut cursor, width, height);
                encoder.set_color(png::ColorType::Grayscale);
                encoder.set_depth(png::BitDepth::Eight);
                let mut writer = encoder
                    .write_header()
                    .map_err(|e| Error::ImageDecode(e.to_string()))?;
                writer
                    .write_image_data(&raw_pixels)
                    .map_err(|e| Error::ImageDecode(e.to_string()))?;
            } else {
                let mut encoder = png::Encoder::new(&mut cursor, width, height);
                encoder.set_color(png::ColorType::Rgb);
                encoder.set_depth(png::BitDepth::Eight);
                let mut writer = encoder
                    .write_header()
                    .map_err(|e| Error::ImageDecode(e.to_string()))?;
                writer
                    .write_image_data(&raw_pixels)
                    .map_err(|e| Error::ImageDecode(e.to_string()))?;
            }

            Ok(PageImage {
                width,
                height,
                bytes: cursor.into_inner(),
                format: PageImageFormat::Png,
            })
        }
        Some(name) => Err(Error::InvalidInput(format!(
            "unsupported image filter '{}'; only DCTDecode (JPEG) and FlateDecode are supported",
            String::from_utf8_lossy(name)
        ))),
    }
}

/// Extracts the largest Image XObject on the page.
///
/// When the page has exactly one image the dimension check is skipped.
/// When multiple images are present the one with the greatest `Width × Height` wins.
pub(crate) fn extract_largest_image_on_page(
    doc: &lopdf::Document,
    page_id: ObjectId,
) -> Result<PageImage> {
    let ids = page_image_xobjects(doc, page_id);
    if ids.is_empty() {
        return Err(Error::InvalidInput(
            "page contains no Image XObject; only scanned PDFs (one image per page) are supported"
                .into(),
        ));
    }
    if ids.len() == 1 {
        return extract_xobject_image(doc, ids[0]);
    }
    let area_of = |id: ObjectId| -> u64 {
        (|| -> Option<u64> {
            let obj = doc.get_object(id).ok()?;
            let stream = obj.as_stream().ok()?;
            let w = stream.dict.get(b"Width").ok()?.as_i64().ok()? as u64;
            let h = stream.dict.get(b"Height").ok()?.as_i64().ok()? as u64;
            Some(w * h)
        })()
        .unwrap_or(0)
    };
    let best_id = ids.into_iter().max_by_key(|&id| area_of(id)).unwrap();
    extract_xobject_image(doc, best_id)
}

/// Extracts all Image XObjects from a page.
///
/// Returns all images found on the page. Per-image extraction errors are skipped,
/// allowing partial success if some XObjects are corrupted.
/// Returns an error only if no images are found on the page.
pub(crate) fn extract_all_images_on_page(
    doc: &lopdf::Document,
    page_id: ObjectId,
) -> Result<Vec<PageImage>> {
    let ids = page_image_xobjects(doc, page_id);
    if ids.is_empty() {
        return Err(Error::InvalidInput("page contains no Image XObject".into()));
    }
    let images: Vec<PageImage> = ids
        .into_iter()
        .filter_map(|id| extract_xobject_image(doc, id).ok())
        .collect();
    if images.is_empty() {
        return Err(Error::InvalidInput(
            "all Image XObjects on page failed to extract".into(),
        ));
    }
    Ok(images)
}
