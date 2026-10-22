use crate::image_ops::{self, Format};
use crate::{naming, pdf_write, Error, Result};
use pdfium_render::prelude::*;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

fn pdf_err(e: PdfiumError) -> Error {
    Error::Pdf(e.to_string())
}

fn search_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(p) = std::env::var_os("SHIFTNINJA_PDFIUM") {
        dirs.push(PathBuf::from(p));
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            dirs.push(dir.to_path_buf());
            dirs.push(dir.join("lib"));
            dirs.push(dir.join("../lib"));
            dirs.push(dir.join("../Resources/lib"));
            dirs.push(dir.join("../lib/shiftninja"));
        }
    }
    dirs.push(PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../lib")));
    dirs
}

fn pdfium() -> Result<&'static Pdfium> {
    static PDFIUM: OnceLock<std::result::Result<Pdfium, String>> = OnceLock::new();
    PDFIUM
        .get_or_init(|| {
            for dir in search_dirs() {
                let lib = Pdfium::pdfium_platform_library_name_at_path(&dir);
                if lib.exists() {
                    if let Ok(bindings) = Pdfium::bind_to_library(&lib) {
                        return Ok(Pdfium::new(bindings));
                    }
                }
            }
            Pdfium::bind_to_system_library()
                .map(Pdfium::new)
                .map_err(|_| "Pdfium library not found. Run scripts/fetch-pdfium.sh".to_string())
        })
        .as_ref()
        .map_err(|e| Error::Pdf(e.clone()))
}

pub fn to_images(path: &Path, format: Format) -> Result<Vec<PathBuf>> {
    let doc = pdfium()?.load_pdf_from_file(path, None).map_err(pdf_err)?;
    let pages = doc.pages();
    let count = pages.len();
    let config = PdfRenderConfig::new().scale_page_by_factor(2.0);
    let mut out = Vec::new();
    for (i, page) in pages.iter().enumerate() {
        let img = page.render_with_config(&config).map_err(pdf_err)?.as_image().map_err(pdf_err)?;
        let suffix = (count > 1).then(|| format!("p{}", i + 1));
        let dest = naming::sibling(path, suffix.as_deref(), format.ext());
        image_ops::save(&img, &dest, format, 90)?;
        out.push(dest);
    }
    Ok(out)
}

pub fn render_page(path: &Path, index: PdfPageIndex) -> Result<image::DynamicImage> {
    let doc = pdfium()?.load_pdf_from_file(path, None).map_err(pdf_err)?;
    let page = doc.pages().get(index).map_err(pdf_err)?;
    let config = PdfRenderConfig::new().scale_page_by_factor(2.0);
    let image = page.render_with_config(&config).map_err(pdf_err)?.as_image().map_err(pdf_err)?;
    Ok(image)
}

pub fn to_text(path: &Path) -> Result<PathBuf> {
    let doc = pdfium()?.load_pdf_from_file(path, None).map_err(pdf_err)?;
    let mut text = String::new();
    for page in doc.pages().iter() {
        text.push_str(&page.text().map_err(pdf_err)?.all());
        text.push_str("\n\n");
    }
    if text.trim().is_empty() {
        return Err(Error::Other("This PDF is a scan – it has no text to copy".into()));
    }
    let dest = naming::sibling(path, None, "txt");
    std::fs::write(&dest, text.trim_end())?;
    Ok(dest)
}

pub fn page_texts(path: &Path) -> Result<Vec<String>> {
    let doc = pdfium()?.load_pdf_from_file(path, None).map_err(pdf_err)?;
    let texts = doc.pages().iter().map(|page| page.text().map(|t| t.all()).map_err(pdf_err)).collect();
    texts
}

/// PDFs and photos, in the order given, into one PDF. Photos go on A4 pages.
pub fn merge(paths: &[PathBuf], out: &Path) -> Result<()> {
    let pdfium = pdfium()?;
    let mut merged = pdfium.create_new_pdf().map_err(pdf_err)?;
    for path in paths {
        let staged;
        let source = match crate::Kind::of(path) {
            crate::Kind::Pdf => path.as_path(),
            crate::Kind::Image(_) => {
                staged = TempFile::new("pdf");
                pdf_write::write_pages(&[pdf_write::a4_page(&image_ops::load(path)?)?], &staged.0)?;
                staged.0.as_path()
            }
            _ => return Err(Error::Other(format!("{} isn't a PDF or a photo", file_name(path)))),
        };
        let doc = pdfium.load_pdf_from_file(source, None).map_err(pdf_err)?;
        merged.pages_mut().append(&doc).map_err(pdf_err)?;
    }
    merged.save_to_file(out).map_err(pdf_err)
}

pub fn split(path: &Path) -> Result<Vec<PathBuf>> {
    let pdfium = pdfium()?;
    let doc = pdfium.load_pdf_from_file(path, None).map_err(pdf_err)?;
    let count = doc.pages().len();
    if count < 2 {
        return Err(Error::Other("This PDF has only one page".into()));
    }
    let mut out = Vec::new();
    for i in 0..count {
        let mut single = pdfium.create_new_pdf().map_err(pdf_err)?;
        single.pages_mut().copy_page_from_document(&doc, i, 0).map_err(pdf_err)?;
        let dest = naming::sibling(path, Some(&format!("- Page {}", i + 1)), "pdf");
        single.save_to_file(&dest).map_err(pdf_err)?;
        out.push(dest);
    }
    Ok(out)
}

pub fn rotate(path: &Path) -> Result<PathBuf> {
    let doc = pdfium()?.load_pdf_from_file(path, None).map_err(pdf_err)?;
    for i in 0..doc.pages().len() {
        let mut page = doc.pages().get(i).map_err(pdf_err)?;
        let next = match page.rotation().map_err(pdf_err)? {
            PdfPageRenderRotation::None => PdfPageRenderRotation::Degrees90,
            PdfPageRenderRotation::Degrees90 => PdfPageRenderRotation::Degrees180,
            PdfPageRenderRotation::Degrees180 => PdfPageRenderRotation::Degrees270,
            PdfPageRenderRotation::Degrees270 => PdfPageRenderRotation::None,
        };
        page.set_rotation(next);
    }
    let dest = naming::sibling(path, Some("Rotated"), "pdf");
    doc.save_to_file(&dest).map_err(pdf_err)?;
    Ok(dest)
}

/// Re-draws every page as a 150 dpi JPEG. Great for scans; text stops being selectable.
pub fn compress(path: &Path) -> Result<PathBuf> {
    const DPI: f32 = 150.0;
    let doc = pdfium()?.load_pdf_from_file(path, None).map_err(pdf_err)?;
    let mut pages = Vec::new();
    for page in doc.pages().iter() {
        let size = (page.width().value, page.height().value);
        let config = PdfRenderConfig::new().scale_page_by_factor(DPI / 72.0);
        let img = page.render_with_config(&config).map_err(pdf_err)?.as_image().map_err(pdf_err)?;
        let (jpeg, px) = pdf_write::jpeg(&img, 70)?;
        pages.push(pdf_write::Page { jpeg, px, size, rect: (0.0, 0.0, size.0, size.1) });
    }
    let dest = naming::sibling(path, Some("Compressed"), "pdf");
    pdf_write::write_pages(&pages, &dest)?;
    let (before, after) = (std::fs::metadata(path)?.len(), std::fs::metadata(&dest)?.len());
    if after >= before {
        let _ = std::fs::remove_file(&dest);
        return Err(Error::Other("This PDF is already as small as it gets".into()));
    }
    Ok(dest)
}

fn file_name(path: &Path) -> String {
    path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
}

/// A scratch file removed when dropped.
pub(crate) struct TempFile(pub PathBuf);

impl TempFile {
    pub fn new(ext: &str) -> TempFile {
        use std::sync::atomic::{AtomicU32, Ordering};
        static N: AtomicU32 = AtomicU32::new(0);
        let n = N.fetch_add(1, Ordering::Relaxed);
        TempFile(std::env::temp_dir().join(format!("shiftninja-{}-{n}.{ext}", std::process::id())))
    }
}

impl Drop for TempFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}
