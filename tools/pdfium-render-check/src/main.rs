use std::{env, path::Path};

use pdfium_render::prelude::*;

fn usage() -> ! {
    eprintln!(
        "usage: harumi-pdfium-render-check <input.pdf> <output.png> [page-index] [target-width]"
    );
    std::process::exit(2);
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = env::args().skip(1);
    let input = args.next().unwrap_or_else(|| usage());
    let output = args.next().unwrap_or_else(|| usage());
    let page_index = args.next().map_or(Ok(0i32), |value| value.parse::<i32>())?;
    let target_width = args
        .next()
        .map_or(Ok(1600i32), |value| value.parse::<i32>())?;
    if page_index < 0 {
        return Err("page-index must not be negative".into());
    }
    if target_width <= 0 {
        return Err("target-width must be positive".into());
    }

    let bindings = if let Ok(path) = env::var("PDFIUM_LIBRARY_PATH") {
        Pdfium::bind_to_library(path)?
    } else {
        Pdfium::bind_to_library(Pdfium::pdfium_platform_library_name_at_path("."))
            .or_else(|_| Pdfium::bind_to_system_library())?
    };
    let pdfium = Pdfium::new(bindings);
    let document = pdfium.load_pdf_from_file(Path::new(&input), None)?;
    let page = document.pages().get(page_index)?;
    let image = page
        .render_with_config(&PdfRenderConfig::new().set_target_width(target_width))?
        .as_image()?;
    image.save(Path::new(&output))?;

    println!(
        "rendered page {} of {} to {} ({}x{})",
        page_index + 1,
        document.pages().len(),
        output,
        image.width(),
        image.height()
    );
    Ok(())
}
