mod image_ops;
mod kind;
mod naming;
mod office;
mod pdf_ops;
mod pdf_write;

use serde::Serialize;
use std::path::{Path, PathBuf};

pub use kind::Kind;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0} can't be used on this file")]
    Unsupported(String),
    #[error("couldn't read or write a file: {0}")]
    Io(#[from] std::io::Error),
    #[error("image error: {0}")]
    Image(#[from] image::ImageError),
    #[error("PDF error: {0}")]
    Pdf(String),
    #[error("{0}")]
    Other(String),
}

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Wheel {
    Convert,
    Tools,
}

#[derive(Debug, Clone, Serialize)]
pub struct ActionInfo {
    pub id: String,
    pub title: String,
    pub wheel: Wheel,
    pub panel: Option<String>,
}

fn info(id: &str, title: &str, wheel: Wheel) -> ActionInfo {
    ActionInfo { id: id.into(), title: title.into(), wheel, panel: None }
}

fn panel(name: &str, title: &str) -> ActionInfo {
    ActionInfo { id: format!("tool.{name}"), title: title.into(), wheel: Wheel::Tools, panel: Some(name.into()) }
}

const PANELS: &[(&str, &str)] = &[
    ("crop", "Crop"),
    ("edit", "Edit"),
    ("annotate", "Annotate"),
    ("addbg", "Add BG"),
    ("redact", "Redact"),
];

/// JPG and JPEG are the same picture; some government portals insist on the .jpeg spelling, so both are offered.
const IMAGE_TARGETS: &[(&str, &str)] = &[
    ("jpg", "JPG"),
    ("jpeg", "JPEG"),
    ("png", "PNG"),
    ("webp", "WEBP"),
    ("pdf", "PDF"),
    ("svg", "SVG"),
    ("heic", "HEIC"),
    ("gif", "GIF"),
    ("bmp", "BMP"),
    ("tiff", "TIFF"),
];

const IMAGE_TOOLS: &[(&str, &str)] = &[("tool.compress", "Compress"), ("tool.strip", "Metadata")];

const IMAGE_EXTRAS: &[(&str, &str)] = &[
    ("tool.half", "Half size"),
    ("tool.rotate", "Rotate"),
    ("tool.square", "Square 1:1"),
    ("tool.story", "Story 9:16"),
];

const PDF_TARGETS: &[(&str, &str)] = &[("jpg", "JPG"), ("png", "PNG"), ("txt", "TXT"), ("docx", "WORD")];

const PDF_TOOLS: &[(&str, &str)] =
    &[("tool.pdfcompress", "Compress"), ("tool.split", "Split"), ("tool.pdfrotate", "Rotate")];

const MERGE: (&str, &str) = ("tool.merge", "Merge");

fn actions_for_kind(kind: Kind, src_ext: &str) -> Vec<ActionInfo> {
    let mut out = Vec::new();
    match kind {
        Kind::Image(_) => {
            let src_ext = if src_ext == "tif" { "tiff" } else { src_ext };
            for (ext, title) in IMAGE_TARGETS {
                if *ext == "heic" && !image_ops::heic_supported() {
                    continue;
                }
                if *ext != src_ext {
                    out.push(info(&format!("convert.{ext}"), title, Wheel::Convert));
                }
            }
            // The wheel shows eight petals; keep the rarely used formats from pushing the common ones off.
            out.truncate(8);
            out.push(panel(PANELS[0].0, PANELS[0].1));
            for (id, title) in IMAGE_TOOLS {
                out.push(info(id, title, Wheel::Tools));
            }
            for (name, title) in &PANELS[1..] {
                out.push(panel(name, title));
            }
            out.push(info("tool.passport", "Passport", Wheel::Tools));
        }
        Kind::Pdf => {
            for (ext, title) in PDF_TARGETS {
                out.push(info(&format!("convert.{ext}"), title, Wheel::Convert));
            }
            for (id, title) in PDF_TOOLS {
                out.push(info(id, title, Wheel::Tools));
            }
            for (name, title) in PANELS {
                out.push(panel(name, title));
            }
        }
        Kind::Document => out.push(info("convert.pdf", "PDF", Wheel::Convert)),
        Kind::Other => {}
    }
    out
}

pub fn actions_for(paths: &[PathBuf]) -> Vec<ActionInfo> {
    let ext_of = |p: &PathBuf| p.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
    let mut iter = paths.iter().map(|p| actions_for_kind(Kind::of(p), &ext_of(p)));
    let Some(first) = iter.next() else { return Vec::new() };
    let shared = iter.fold(first, |acc, next| {
        acc.into_iter().filter(|a| next.iter().any(|b| b.id == a.id)).collect::<Vec<_>>()
    });
    if paths.len() > 1 {
        let mut multi: Vec<ActionInfo> = shared.into_iter().filter(|a| a.panel.is_none() && a.id != "tool.passport").collect();
        if paths.iter().all(|p| matches!(Kind::of(p), Kind::Pdf | Kind::Image(_))) {
            let at = multi.iter().position(|a| a.wheel == Wheel::Tools).unwrap_or(multi.len());
            multi.insert(at, info(MERGE.0, MERGE.1, Wheel::Tools));
        }
        return multi;
    }
    shared
}

pub fn all_actions_for(paths: &[PathBuf]) -> Vec<ActionInfo> {
    let mut all: Vec<ActionInfo> = actions_for(paths).into_iter().filter(|a| a.panel.is_none()).collect();
    if !paths.is_empty() && paths.iter().all(|p| matches!(Kind::of(p), Kind::Image(_))) {
        all.extend(IMAGE_EXTRAS.iter().map(|(id, title)| info(id, title, Wheel::Tools)));
    }
    all
}

pub struct EditSource {
    pub png: Vec<u8>,
    pub width: u32,
    pub height: u32,
    pub out_ext: &'static str,
}

pub fn load_for_edit(path: &Path) -> Result<EditSource> {
    let (img, out_ext) = match Kind::of(path) {
        Kind::Image(f) => {
            let ext = if matches!(f, image_ops::Format::Jpg | image_ops::Format::Heic) { "jpg" } else { "png" };
            (image_ops::load(path)?, ext)
        }
        Kind::Pdf => (pdf_ops::render_page(path, 0)?, "jpg"),
        Kind::Document | Kind::Other => return Err(Error::Unsupported("editing".into())),
    };
    Ok(EditSource { png: image_ops::png_bytes(&img)?, width: img.width(), height: img.height(), out_ext })
}

pub fn save_edit(original: &Path, suffix: &str, ext: &str, bytes: &[u8]) -> Result<PathBuf> {
    if !matches!(ext, "png" | "jpg") || suffix.is_empty() || suffix.contains(['/', '\\']) {
        return Err(Error::Other(format!("can't save as {suffix:?}.{ext}")));
    }
    let out = naming::sibling(original, Some(suffix), ext);
    std::fs::write(&out, bytes)?;
    Ok(out)
}

pub fn run(action_id: &str, paths: &[PathBuf]) -> Result<Vec<PathBuf>> {
    if action_id == MERGE.0 {
        let first = paths.first().ok_or_else(|| Error::Other("Choose files to merge".into()))?;
        let out = naming::sibling(first, Some("Merged"), "pdf");
        pdf_ops::merge(paths, &out)?;
        return Ok(vec![out]);
    }
    let mut created = Vec::new();
    for path in paths {
        created.extend(run_one(action_id, path)?);
    }
    Ok(created)
}

fn run_one(action_id: &str, path: &Path) -> Result<Vec<PathBuf>> {
    let unsupported = || Error::Unsupported(action_id.to_string());
    if let Some(name) = action_id.strip_prefix("tool.").filter(|n| PANELS.iter().any(|(p, _)| p == n)) {
        return Err(Error::Other(format!("{name} needs its editor: open it from the wheel")));
    }
    match (Kind::of(path), action_id.split_once('.')) {
        (Kind::Image(_), Some(("convert", "pdf"))) => {
            let img = image_ops::load(path)?;
            let out = naming::sibling(path, None, "pdf");
            pdf_write::write_images(&[img], &out)?;
            Ok(vec![out])
        }
        (Kind::Image(_), Some(("convert", ext))) => {
            let target = image_ops::Format::from_ext(ext).ok_or_else(unsupported)?;
            let img = image_ops::load(path)?;
            let out = naming::sibling(path, None, if ext == "jpeg" { "jpeg" } else { target.ext() });
            image_ops::save(&img, &out, target, 90)?;
            Ok(vec![out])
        }
        (Kind::Image(_), Some(("tool", "passport"))) => Ok(vec![image_ops::passport_sheet(path)?]),
        (Kind::Image(src), Some(("tool", tool))) => {
            let out = image_ops::tool(path, src, tool)?;
            Ok(vec![out])
        }
        (Kind::Pdf, Some(("convert", "txt"))) => Ok(vec![pdf_ops::to_text(path)?]),
        (Kind::Pdf, Some(("convert", "docx"))) => Ok(vec![office::convert(path, "docx")?]),
        (Kind::Document, Some(("convert", "pdf"))) => Ok(vec![office::convert(path, "pdf")?]),
        (Kind::Pdf, Some(("tool", "split"))) => pdf_ops::split(path),
        (Kind::Pdf, Some(("tool", "pdfrotate"))) => Ok(vec![pdf_ops::rotate(path)?]),
        (Kind::Pdf, Some(("tool", "pdfcompress"))) => Ok(vec![pdf_ops::compress(path)?]),
        (Kind::Pdf, Some(("convert", ext))) => {
            let target = image_ops::Format::from_ext(ext).ok_or_else(unsupported)?;
            pdf_ops::to_images(path, target)
        }
        _ => Err(unsupported()),
    }
}

pub fn total_size(paths: &[PathBuf]) -> u64 {
    paths.iter().filter_map(|p| std::fs::metadata(p).ok()).map(|m| m.len()).sum()
}

#[derive(Debug, Clone, Serialize)]
pub struct ToolInfo {
    pub key: &'static str,
    pub id: &'static str,
    pub title: &'static str,
    pub blurb: &'static str,
    pub group: &'static str,
    pub accepts: Vec<&'static str>,
    pub min_files: usize,
    pub panel: Option<String>,
}

const PHOTOS: &[&str] = &["jpg", "jpeg", "png", "webp", "svg", "tif", "tiff", "bmp", "gif"];
const WORD: &[&str] = &["docx", "doc", "odt", "rtf"];

/// Every tool, most useful first.
pub fn tools() -> Vec<ToolInfo> {
    let photos_and_pdf = || [PHOTOS, &["pdf"]].concat();
    let t = |key, id, title, blurb, group, accepts: Vec<&'static str>, min_files| ToolInfo {
        key,
        id,
        title,
        blurb,
        group,
        accepts,
        min_files,
        panel: None,
    };
    let mut all = vec![
        t("pdf-to-word", "convert.docx", "PDF to Word", "Turn a PDF into an editable Word file", "Convert", vec!["pdf"], 1),
        t("word-to-pdf", "convert.pdf", "Word to PDF", "Make a PDF from a Word document", "Convert", WORD.to_vec(), 1),
        t("merge", "tool.merge", "Merge PDFs", "Join PDFs and photos into one PDF", "PDF", photos_and_pdf(), 2),
        t("photos-to-pdf", "tool.merge", "Photos to PDF", "Put photos on A4 pages in one PDF", "PDF", PHOTOS.to_vec(), 1),
        t("passport", "tool.passport", "Passport photos", "30 passport-size photos on one A4 sheet", "Image", PHOTOS.to_vec(), 1),
        t("compress-pdf", "tool.pdfcompress", "Compress PDF", "Shrink a PDF to send on WhatsApp or email", "PDF", vec!["pdf"], 1),
        t("split", "tool.split", "Split PDF", "Save every page as its own PDF", "PDF", vec!["pdf"], 1),
        t("pdf-to-jpg", "convert.jpg", "PDF to JPG", "Save every page as a photo", "Convert", vec!["pdf"], 1),
        t("rotate-pdf", "tool.pdfrotate", "Rotate PDF", "Turn every page a quarter turn", "PDF", vec!["pdf"], 1),
        t("compress-photo", "tool.compress", "Compress photo", "Make a photo smaller", "Image", PHOTOS.to_vec(), 1),
        t("to-jpg", "convert.jpg", "Photo to JPG", "Convert any photo to JPG", "Convert", PHOTOS.to_vec(), 1),
        t("to-png", "convert.png", "Photo to PNG", "Convert any photo to PNG", "Convert", PHOTOS.to_vec(), 1),
    ];
    let blurbs = [
        ("crop", "Crop a photo or PDF page"),
        ("edit", "Rotate, resize, brighten"),
        ("annotate", "Draw arrows, boxes, text"),
        ("addbg", "Put a photo on a nice background"),
        ("redact", "Hide private details"),
    ];
    for ((name, title), (_, blurb)) in PANELS.iter().zip(blurbs) {
        let mut tool = t(name, "", title, blurb, "Image", photos_and_pdf(), 1);
        tool.id = match *name {
            "crop" => "tool.crop",
            "edit" => "tool.edit",
            "annotate" => "tool.annotate",
            "addbg" => "tool.addbg",
            _ => "tool.redact",
        };
        tool.panel = Some(name.to_string());
        all.push(tool);
    }
    all
}
