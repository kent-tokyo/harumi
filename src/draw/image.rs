use lopdf::{Dictionary, Object, ObjectId, Stream};

use crate::error::{Error, Result};

pub(crate) struct PreparedImage {
    pub width: u32,
    pub height: u32,
    pub data: ImageData,
}

pub(crate) enum ImageData {
    /// Original JPEG bytes — embedded as-is with DCTDecode.
    Jpeg(Vec<u8>),
    /// Decoded raw RGB bytes (3 bytes/pixel, top-to-bottom, left-to-right). Fully opaque.
    Rgb { bytes: Vec<u8> },
    /// Decoded raw RGB bytes plus a separate alpha channel (1 byte/pixel).
    /// Embedded as an Image XObject with a DeviceGray SMask sub-object.
    RgbWithAlpha { rgb: Vec<u8>, alpha: Vec<u8> },
}

/// Prepare an image for PDF embedding.
///
/// JPEG files are embedded without decoding (DCTDecode filter).
/// PNG files are decoded via the `png` crate:
/// - Fully opaque PNG → `ImageData::Rgb` (no SMask needed)
/// - PNG with any transparent pixel → `ImageData::RgbWithAlpha` (PDF SMask)
pub(crate) fn prepare(bytes: &[u8]) -> Result<PreparedImage> {
    if bytes.starts_with(b"\xff\xd8\xff") {
        let (w, h) = parse_jpeg_dims(bytes)?;
        return Ok(PreparedImage {
            width: w,
            height: h,
            data: ImageData::Jpeg(bytes.to_vec()),
        });
    }

    // PNG: parse header to get dimensions without full decode.
    let decoder = png::Decoder::new(std::io::Cursor::new(bytes));
    let reader = decoder
        .read_info()
        .map_err(|e| Error::ImageDecode(e.to_string()))?;
    let (w, h) = (reader.info().width, reader.info().height);
    let pixel_count = w as u64 * h as u64;
    if pixel_count > 200_000_000 {
        return Err(Error::InvalidInput(format!(
            "image too large: {w}x{h} = {} pixels (limit 200 MP)",
            pixel_count
        )));
    }

    // Full PNG decode: read all pixels.
    let decoder = png::Decoder::new(std::io::Cursor::new(bytes));
    let mut reader = decoder
        .read_info()
        .map_err(|e| Error::ImageDecode(e.to_string()))?;
    let mut buf = vec![0u8; reader.output_buffer_size()];
    let _ = reader
        .next_frame(&mut buf)
        .map_err(|e| Error::ImageDecode(e.to_string()))?;

    // Parse color type and extract RGB/RGBA data.
    let info = reader.info();
    let has_alpha = matches!(
        info.color_type,
        png::ColorType::Rgba | png::ColorType::GrayscaleAlpha
    );

    let pixel_bytes = match info.color_type {
        png::ColorType::Rgb => {
            // Already RGB, no alpha check needed.
            buf.truncate(w as usize * h as usize * 3);
            buf
        }
        png::ColorType::Rgba => {
            // Has alpha; check for transparency.
            let rgba = buf;
            let mut rgb = Vec::with_capacity((pixel_count * 3) as usize);
            let mut alpha = Vec::with_capacity(pixel_count as usize);
            let mut any_alpha_less_than_255 = false;
            for chunk in rgba.chunks_exact(4) {
                rgb.extend_from_slice(&chunk[..3]);
                alpha.push(chunk[3]);
                if chunk[3] < 255 {
                    any_alpha_less_than_255 = true;
                }
            }
            if any_alpha_less_than_255 {
                return Ok(PreparedImage {
                    width: w,
                    height: h,
                    data: ImageData::RgbWithAlpha { rgb, alpha },
                });
            }
            rgb
        }
        png::ColorType::Grayscale => {
            // Grayscale: convert to RGB by duplicating.
            let gray = buf;
            let mut rgb = Vec::with_capacity((pixel_count * 3) as usize);
            for &g in &gray {
                rgb.extend_from_slice(&[g, g, g]);
            }
            rgb
        }
        png::ColorType::GrayscaleAlpha => {
            // Grayscale + alpha.
            let ga = buf;
            let mut rgb = Vec::with_capacity((pixel_count * 3) as usize);
            let mut alpha = Vec::with_capacity(pixel_count as usize);
            let mut any_alpha_less_than_255 = false;
            for chunk in ga.chunks_exact(2) {
                let g = chunk[0];
                rgb.extend_from_slice(&[g, g, g]);
                alpha.push(chunk[1]);
                if chunk[1] < 255 {
                    any_alpha_less_than_255 = true;
                }
            }
            if any_alpha_less_than_255 {
                return Ok(PreparedImage {
                    width: w,
                    height: h,
                    data: ImageData::RgbWithAlpha { rgb, alpha },
                });
            }
            rgb
        }
        png::ColorType::Indexed => {
            return Err(Error::ImageDecode("indexed color PNG not supported".into()));
        }
    };

    Ok(PreparedImage {
        width: w,
        height: h,
        data: ImageData::Rgb { bytes: pixel_bytes },
    })
}

/// Add an Image XObject to the lopdf document and return its object ID.
pub(crate) fn embed_xobject(doc: &mut lopdf::Document, img: PreparedImage) -> Result<ObjectId> {
    match img.data {
        ImageData::Jpeg(bytes) => {
            let mut dict = Dictionary::new();
            dict.set("Type", Object::Name(b"XObject".to_vec()));
            dict.set("Subtype", Object::Name(b"Image".to_vec()));
            dict.set("Width", Object::Integer(img.width as i64));
            dict.set("Height", Object::Integer(img.height as i64));
            dict.set("ColorSpace", Object::Name(b"DeviceRGB".to_vec()));
            dict.set("BitsPerComponent", Object::Integer(8));
            dict.set("Filter", Object::Name(b"DCTDecode".to_vec()));
            Ok(doc.add_object(Object::Stream(Stream::new(dict, bytes))))
        }
        ImageData::Rgb { bytes } => {
            let mut dict = Dictionary::new();
            dict.set("Type", Object::Name(b"XObject".to_vec()));
            dict.set("Subtype", Object::Name(b"Image".to_vec()));
            dict.set("Width", Object::Integer(img.width as i64));
            dict.set("Height", Object::Integer(img.height as i64));
            dict.set("ColorSpace", Object::Name(b"DeviceRGB".to_vec()));
            dict.set("BitsPerComponent", Object::Integer(8));
            let mut stream = Stream::new(dict, bytes);
            let _ = stream.compress();
            Ok(doc.add_object(Object::Stream(stream)))
        }
        ImageData::RgbWithAlpha { rgb, alpha } => {
            // SMask sub-object: grayscale image carrying the alpha channel.
            // Not registered in /Resources — referenced only by the main image dict.
            let mut smask_dict = Dictionary::new();
            smask_dict.set("Type", Object::Name(b"XObject".to_vec()));
            smask_dict.set("Subtype", Object::Name(b"Image".to_vec()));
            smask_dict.set("Width", Object::Integer(img.width as i64));
            smask_dict.set("Height", Object::Integer(img.height as i64));
            smask_dict.set("ColorSpace", Object::Name(b"DeviceGray".to_vec()));
            smask_dict.set("BitsPerComponent", Object::Integer(8));
            let mut smask_stream = Stream::new(smask_dict, alpha);
            let _ = smask_stream.compress();
            let smask_id = doc.add_object(Object::Stream(smask_stream));

            let mut dict = Dictionary::new();
            dict.set("Type", Object::Name(b"XObject".to_vec()));
            dict.set("Subtype", Object::Name(b"Image".to_vec()));
            dict.set("Width", Object::Integer(img.width as i64));
            dict.set("Height", Object::Integer(img.height as i64));
            dict.set("ColorSpace", Object::Name(b"DeviceRGB".to_vec()));
            dict.set("BitsPerComponent", Object::Integer(8));
            dict.set("SMask", Object::Reference(smask_id));
            let mut stream = Stream::new(dict, rgb);
            let _ = stream.compress();
            Ok(doc.add_object(Object::Stream(stream)))
        }
    }
}

/// Returns the content stream fragment that renders the image at `rect`.
///
/// `rect` = `[x, y, width, height]` in PDF points (origin bottom-left).
/// The PDF cm operator maps the unit square to `rect`.
pub(crate) fn image_stream(xobj_name: &str, rect: &[f32; 4], gs_name: &str) -> Vec<u8> {
    format!(
        "q\n/{gs} gs\n{w:.4} 0 0 {h:.4} {x:.4} {y:.4} cm\n/{name} Do\nQ\n",
        gs = gs_name,
        w = rect[2],
        h = rect[3],
        x = rect[0],
        y = rect[1],
        name = xobj_name,
    )
    .into_bytes()
}

/// Parse JPEG dimensions by scanning SOF markers (FF C0–CF, except DHT/DAC/etc.).
pub(crate) fn parse_jpeg_dims(data: &[u8]) -> Result<(u32, u32)> {
    let mut i = 2; // skip SOI marker (FF D8)
    while i < data.len() {
        if data[i] != 0xFF {
            return Err(Error::ImageDecode("malformed JPEG: expected marker".into()));
        }
        // Skip 0xFF fill bytes (JPEG spec §B.1.1.2: any number of 0xFF may precede a marker).
        while i + 1 < data.len() && data[i + 1] == 0xFF {
            i += 1;
        }
        if i + 1 >= data.len() {
            break;
        }
        let marker = data[i + 1];
        // SOF markers carry image dimensions; skip others.
        if matches!(
            marker,
            0xC0 | 0xC1
                | 0xC2
                | 0xC3
                | 0xC5
                | 0xC6
                | 0xC7
                | 0xC9
                | 0xCA
                | 0xCB
                | 0xCD
                | 0xCE
                | 0xCF
        ) && i + 8 < data.len()
        {
            let h = u16::from_be_bytes([data[i + 5], data[i + 6]]) as u32;
            let w = u16::from_be_bytes([data[i + 7], data[i + 8]]) as u32;
            if w > 0 && h > 0 {
                return Ok((w, h));
            }
        }
        // Stand-alone markers (RST0-RST7, SOI, EOI, TEM) carry no length field.
        if matches!(marker, 0xD0..=0xD9 | 0x01) {
            i += 2;
            continue;
        }
        if i + 3 >= data.len() {
            break;
        }
        let seg_len = u16::from_be_bytes([data[i + 2], data[i + 3]]) as usize;
        if seg_len < 2 {
            break;
        }
        i += 2 + seg_len;
    }
    Err(Error::ImageDecode(
        "JPEG: could not find SOF marker with valid dimensions".into(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_jpeg_with_sof(w: u16, h: u16, fill_bytes: usize) -> Vec<u8> {
        // SOI + optional 0xFF fill padding + SOF0 + EOI
        let mut data = vec![0xFF, 0xD8]; // SOI
        // APP0-like junk segment so the parser skips it
        data.extend_from_slice(&[0xFF, 0xE0, 0x00, 0x10]); // marker + length=16
        data.extend_from_slice(&[0u8; 14]); // padding up to length
        // 0xFF fill bytes before SOF0 (the Mi-3 case)
        for _ in 0..fill_bytes {
            data.push(0xFF);
        }
        // SOF0: FF C0 len(2) precision(1) height(2) width(2) components(1)
        data.extend_from_slice(&[0xFF, 0xC0, 0x00, 0x11, 0x08]);
        data.extend_from_slice(&h.to_be_bytes());
        data.extend_from_slice(&w.to_be_bytes());
        data.push(0x03); // 3 components
        data.extend_from_slice(&[0u8; 12]); // component data
        data.extend_from_slice(&[0xFF, 0xD9]); // EOI
        data
    }

    #[test]
    fn jpeg_dims_no_fill_bytes() {
        let data = make_jpeg_with_sof(640, 480, 0);
        assert_eq!(parse_jpeg_dims(&data).unwrap(), (640, 480));
    }

    #[test]
    fn jpeg_dims_with_fill_bytes() {
        // JPEG spec §B.1.1.2: 0xFF padding before the marker byte is legal
        let data = make_jpeg_with_sof(320, 240, 3);
        assert_eq!(parse_jpeg_dims(&data).unwrap(), (320, 240));
    }
}
