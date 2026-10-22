use crate::image_ops::Format;
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Image(Format),
    Pdf,
    Document,
    Other,
}

impl Kind {
    pub fn of(path: &Path) -> Kind {
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_ascii_lowercase())
            .unwrap_or_default();
        if ext == "pdf" {
            return Kind::Pdf;
        }
        if matches!(ext.as_str(), "docx" | "doc" | "odt" | "rtf") {
            return Kind::Document;
        }
        match Format::from_ext(&ext) {
            Some(f) if f.readable() => Kind::Image(f),
            _ => Kind::Other,
        }
    }
}
