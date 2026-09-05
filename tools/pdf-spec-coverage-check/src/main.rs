use std::{env, fs, path::PathBuf};

use harumi::Document;
use lopdf::{Document as LowLevelDocument, Object, Stream, dictionary};

fn usage() -> ! {
    eprintln!(
        "usage: harumi-pdf-spec-coverage-check <font.ttf> <red_1x1.png> <output-dir>"
    );
    std::process::exit(2);
}

fn save(path: &PathBuf, bytes: &[u8]) -> Result<(), Box<dyn std::error::Error>> {
    fs::write(path, bytes)?;
    Ok(())
}

fn inherited_page_tree() -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let mut doc = LowLevelDocument::with_version("1.4");
    let root_pages = doc.new_object_id();
    let intermediate_pages = doc.new_object_id();
    let content = doc.add_object(Object::Stream(Stream::new(
        dictionary! {},
        b"q Q\n".to_vec(),
    )));
    let page = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => "Page",
        "Parent" => Object::Reference(intermediate_pages),
        "Contents" => Object::Reference(content),
        "Resources" => Object::Dictionary(dictionary! {})
    }));
    doc.set_object(
        intermediate_pages,
        Object::Dictionary(dictionary! {
            "Type" => "Pages",
            "Parent" => Object::Reference(root_pages),
            "MediaBox" => vec![0.into(), 0.into(), 595.into(), 842.into()],
            "Count" => 1,
            "Kids" => vec![Object::Reference(page)]
        }),
    );
    doc.set_object(
        root_pages,
        Object::Dictionary(dictionary! {
            "Type" => "Pages",
            "Count" => 1,
            "Kids" => vec![Object::Reference(intermediate_pages)]
        }),
    );
    let catalog = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => "Catalog",
        "Pages" => Object::Reference(root_pages)
    }));
    doc.trailer.set("Root", Object::Reference(catalog));
    let mut bytes = Vec::new();
    doc.save_to(&mut bytes)?;
    Ok(bytes)
}

fn resources_and_contents() -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let mut doc = LowLevelDocument::with_version("1.4");
    let pages = doc.new_object_id();
    let page = doc.new_object_id();
    let stream = doc.add_object(Object::Stream(Stream::new(
        dictionary! {},
        b"BT /F1 18 Tf 72 700 Td (Resources Contents) Tj ET\n".to_vec(),
    )));
    doc.set_object(
        page,
        Object::Dictionary(dictionary! {
            "Type" => "Page",
            "Parent" => Object::Reference(pages),
            "MediaBox" => vec![0.into(), 0.into(), 595.into(), 842.into()],
            "Contents" => Object::Reference(stream),
            "Resources" => Object::Dictionary(dictionary! {
                "Font" => Object::Dictionary(dictionary! {
                    "F1" => Object::Dictionary(dictionary! {
                        "Type" => "Font",
                        "Subtype" => "Type1",
                        "BaseFont" => "Helvetica"
                    })
                })
            })
        }),
    );
    doc.set_object(
        pages,
        Object::Dictionary(dictionary! {
            "Type" => "Pages",
            "Count" => 1,
            "Kids" => vec![Object::Reference(page)]
        }),
    );
    let catalog = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => "Catalog",
        "Pages" => Object::Reference(pages)
    }));
    doc.trailer.set("Root", Object::Reference(catalog));
    let mut bytes = Vec::new();
    doc.save_to(&mut bytes)?;
    Ok(bytes)
}

fn embedded_font(font: &[u8]) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let mut doc = Document::new((595.0, 842.0))?;
    let font_handle = doc.embed_font(font)?;
    doc.page(1)?
        .add_text("font CMap", font_handle, [72.0, 700.0], 18.0, [0.0; 3])?;
    Ok(doc.save_to_bytes()?)
}

fn image_xobject(image: &[u8]) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let mut doc = Document::new((595.0, 842.0))?;
    doc.page(1)?.add_image(image, [72.0, 600.0, 120.0, 120.0])?;
    Ok(doc.save_to_bytes()?)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = env::args().skip(1);
    let font_path = PathBuf::from(args.next().unwrap_or_else(|| usage()));
    let image_path = PathBuf::from(args.next().unwrap_or_else(|| usage()));
    let output_dir = PathBuf::from(args.next().unwrap_or_else(|| usage()));
    fs::create_dir_all(&output_dir)?;
    let font = fs::read(font_path)?;
    let image = fs::read(image_path)?;

    let cases = [
        ("page-tree-inheritance.pdf", inherited_page_tree()?),
        ("resources-contents.pdf", resources_and_contents()?),
        ("font-cmap.pdf", embedded_font(&font)?),
        ("image-xobject.pdf", image_xobject(&image)?),
    ];
    for (name, bytes) in cases {
        let path = output_dir.join(name);
        save(&path, &bytes)?;
        println!("generated {} ({} bytes)", path.display(), bytes.len());
    }
    Ok(())
}
