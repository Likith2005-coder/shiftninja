use image::{DynamicImage, Rgb, RgbImage};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

fn photo(dir: &Path, name: &str, w: u32, h: u32) -> PathBuf {
    let img = RgbImage::from_fn(w, h, |x, y| Rgb([(x * 255 / w) as u8, (y * 255 / h) as u8, 90]));
    let path = dir.join(name);
    DynamicImage::ImageRgb8(img).save(&path).unwrap();
    path
}

fn noisy(dir: &Path, name: &str, w: u32, h: u32) -> PathBuf {
    let mut seed = 7u32;
    let img = RgbImage::from_fn(w, h, |_, _| {
        seed = seed.wrapping_mul(1_103_515_245).wrapping_add(12_345);
        let v = (seed >> 16) as u8;
        Rgb([v, v.wrapping_mul(3), v.wrapping_add(80)])
    });
    let path = dir.join(name);
    DynamicImage::ImageRgb8(img).save(&path).unwrap();
    path
}

fn titles(paths: &[PathBuf], wheel: engine::Wheel) -> Vec<String> {
    engine::actions_for(paths).into_iter().filter(|a| a.wheel == wheel).map(|a| a.title).collect()
}

fn pages(pdf: &Path) -> usize {
    match engine::run("tool.split", &[pdf.to_path_buf()]) {
        Ok(parts) => {
            parts.iter().for_each(|p| std::fs::remove_file(p).unwrap());
            parts.len()
        }
        Err(e) if e.to_string().contains("only one page") => 1,
        Err(e) => panic!("{e}"),
    }
}

#[test]
fn wheels_offer_the_new_tools() {
    let dir = tempfile::tempdir().unwrap();
    let a = photo(dir.path(), "a.png", 40, 30);
    let b = photo(dir.path(), "b.jpg", 40, 30);
    let pdf = engine::run("convert.pdf", &[a.clone()]).unwrap().remove(0);
    let docx = dir.path().join("letter.docx");
    std::fs::write(&docx, b"").unwrap();

    assert_eq!(titles(&[pdf.clone()], engine::Wheel::Convert), ["JPG", "PNG", "TXT", "WORD"]);
    assert_eq!(
        titles(&[pdf.clone()], engine::Wheel::Tools),
        ["Compress", "Split", "Rotate", "Crop", "Edit", "Annotate", "Add BG", "Redact"]
    );
    assert_eq!(titles(&[docx], engine::Wheel::Convert), ["PDF"]);
    assert_eq!(titles(&[pdf.clone(), pdf.clone()], engine::Wheel::Tools)[0], "Merge");
    let two_photos = titles(&[a.clone(), b], engine::Wheel::Tools);
    assert_eq!(two_photos[0], "Merge");
    assert!(!two_photos.contains(&"Passport".to_string()));
    assert!(titles(&[a.clone(), a], engine::Wheel::Convert).contains(&"WEBP".to_string()));
    for wheel in [engine::Wheel::Convert, engine::Wheel::Tools] {
        assert!(titles(&[pdf.clone()], wheel).iter().all(|t| t.len() <= 8));
    }
}

#[test]
fn merge_split_and_rotate() {
    let dir = tempfile::tempdir().unwrap();
    let wide = photo(dir.path(), "Wide.png", 200, 100);
    let tall = photo(dir.path(), "tall.jpg", 100, 200);
    let one = engine::run("convert.pdf", &[tall.clone()]).unwrap().remove(0);

    let merged = engine::run("tool.merge", &[wide.clone(), one.clone(), tall]).unwrap();
    assert_eq!(merged, vec![dir.path().join("Wide Merged.pdf")]);
    assert_eq!(pages(&merged[0]), 3);

    let parts = engine::run("tool.split", &merged).unwrap();
    assert_eq!(parts.len(), 3);
    assert_eq!(parts[0], dir.path().join("Wide Merged - Page 1.pdf"));
    assert!(engine::run("tool.split", &[one]).unwrap_err().to_string().contains("only one page"));

    let landscape = image::open(&engine::run("convert.jpg", &[parts[0].clone()]).unwrap()[0]).unwrap();
    assert!(landscape.width() > landscape.height(), "wide photo sits on a landscape A4 page");
    let rotated = engine::run("tool.pdfrotate", &[parts[0].clone()]).unwrap();
    assert_eq!(rotated, vec![dir.path().join("Wide Merged - Page 1 Rotated.pdf")]);
    let turned = image::open(&engine::run("convert.jpg", &rotated).unwrap()[0]).unwrap();
    assert!(turned.height() > turned.width(), "rotation turns the page");
}

#[test]
fn compress_pdf_shrinks_scans() {
    let dir = tempfile::tempdir().unwrap();
    let scan = noisy(dir.path(), "scan.png", 2480, 3508);
    let pdf = engine::run("tool.merge", &[scan]).unwrap().remove(0);
    let small = engine::run("tool.pdfcompress", &[pdf.clone()]).unwrap().remove(0);
    assert_eq!(small, dir.path().join("scan Merged Compressed.pdf"));
    let (before, after) = (std::fs::metadata(&pdf).unwrap().len(), std::fs::metadata(&small).unwrap().len());
    assert!(after < before, "{after} < {before}");
    assert_eq!(pages(&small), 1);
}

#[test]
fn passport_sheet_is_a4_at_300_dpi() {
    let dir = tempfile::tempdir().unwrap();
    let face = photo(dir.path(), "Ravi.jpg", 600, 800);
    let out = engine::run("tool.passport", &[face]).unwrap();
    assert_eq!(out, vec![dir.path().join("Ravi Passport Sheet.jpg")]);
    let sheet = image::open(&out[0]).unwrap().to_rgb8();
    assert_eq!(sheet.dimensions(), (2480, 3508));
    assert_eq!(sheet.get_pixel(5, 5).0, [255, 255, 255], "white margin");
    assert_ne!(sheet.get_pixel(159 + 200, 101 + 260).0, [255, 255, 255], "first photo is placed");
}

fn docx_fixture(path: &Path) {
    let body = "<w:p><w:pPr><w:pStyle w:val=\"Heading1\"/></w:pPr><w:r><w:t>Rent Agreement</w:t></w:r></w:p>\
        <w:p><w:r><w:t xml:space=\"preserve\">Rent &amp; deposit: Rs 15,000 </w:t></w:r><w:r><w:t>per month</w:t></w:r></w:p>\
        <w:p><w:r><w:br w:type=\"page\"/></w:r></w:p>\
        <w:p><w:r><w:t>Signed by both parties</w:t></w:r></w:p>";
    let xml = format!(
        "<?xml version=\"1.0\"?><w:document xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\">\
         <w:body>{body}</w:body></w:document>"
    );
    let mut zip = zip::ZipWriter::new(std::fs::File::create(path).unwrap());
    zip.start_file("word/document.xml", zip::write::SimpleFileOptions::default()).unwrap();
    zip.write_all(xml.as_bytes()).unwrap();
    zip.finish().unwrap();
}

#[test]
fn built_in_word_conversion_round_trips() {
    std::env::set_var("SHIFTNINJA_OFFICE", "builtin");
    let dir = tempfile::tempdir().unwrap();
    let docx = dir.path().join("Agreement.docx");
    docx_fixture(&docx);

    let pdf = engine::run("convert.pdf", &[docx]).unwrap().remove(0);
    assert_eq!(pdf, dir.path().join("Agreement.pdf"));
    assert_eq!(pages(&pdf), 2, "page break honoured");
    let txt = engine::run("convert.txt", &[pdf.clone()]).unwrap().remove(0);
    let text = std::fs::read_to_string(txt).unwrap();
    for words in ["Rent Agreement", "Rent & deposit: Rs 15,000 per month", "Signed by both parties"] {
        assert!(text.contains(words), "{words:?} in {text:?}");
    }

    let back = engine::run("convert.docx", &[pdf]).unwrap().remove(0);
    assert_eq!(back, dir.path().join("Agreement 2.docx"));
    let mut xml = String::new();
    zip::ZipArchive::new(std::fs::File::open(&back).unwrap())
        .unwrap()
        .by_name("word/document.xml")
        .unwrap()
        .read_to_string(&mut xml)
        .unwrap();
    assert!(xml.contains("Rent &amp; deposit") && xml.contains("w:type=\"page\""), "{xml}");

    let doc = dir.path().join("old.doc");
    std::fs::write(&doc, b"not really").unwrap();
    let err = engine::run("convert.pdf", &[doc]).unwrap_err().to_string();
    assert!(err.contains("Install Microsoft Word or LibreOffice"), "{err}");
}
