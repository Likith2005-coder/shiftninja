use std::path::{Path, PathBuf};

pub fn sibling(original: &Path, suffix: Option<&str>, ext: &str) -> PathBuf {
    let dir = original.parent().unwrap_or_else(|| Path::new("."));
    let stem = original.file_stem().and_then(|s| s.to_str()).unwrap_or("file");
    let base = match suffix {
        Some(s) => format!("{stem} {s}"),
        None => stem.to_string(),
    };
    let mut candidate = dir.join(format!("{base}.{ext}"));
    let mut n = 2;
    while candidate.exists() {
        candidate = dir.join(format!("{base} {n}.{ext}"));
        n += 1;
    }
    candidate
}
