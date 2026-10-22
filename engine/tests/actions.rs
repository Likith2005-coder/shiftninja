use image::{DynamicImage, Rgba, RgbaImage};
use std::path::{Path, PathBuf};

fn sample_png(dir: &Path, name: &str, w: u32, h: u32) -> PathBuf {
    let img = RgbaImage::from_fn(w, h, |x, y| Rgba([(x * 255 / w) as u8, (y * 255 / h) as u8, 120, 255]));
    let path = dir.join(name);
    DynamicImage::ImageRgba8(img).save(&path).unwrap();
    path
}

fn ids(paths: &[PathBuf]) -> Vec<String> {
    engine::actions_for(paths).into_iter().map(|a| a.id).collect()
}

#[test]
fn image_wheel_hides_its_own_format() {
    let dir = tempfile::tempdir().unwrap();
    let png = sample_png(dir.path(), "a.png", 8, 8);
    let ids = ids(&[png]);
    assert!(ids.contains(&"convert.jpg".to_string()));
    assert!(!ids.contains(&"convert.png".to_string()));
    assert!(ids.contains(&"tool.compress".to_string()));
}

#[test]
fn tools_wheel_matches_the_reel() {
    let dir = tempfile::tempdir().unwrap();
    let png = sample_png(dir.path(), "a.png", 8, 8);
    let tools: Vec<String> = engine::actions_for(&[png.clone()])
        .into_iter()
        .filter(|a| matches!(a.wheel, engine::Wheel::Tools))
        .map(|a| a.title)
        .collect();
    assert_eq!(tools, ["Crop", "Compress", "Metadata", "Edit", "Annotate", "Add BG", "Redact", "Passport"]);
    assert!(engine::run("tool.crop", &[png]).is_err(), "panel tools can't run without their editor");
}

#[test]
fn editor_load_and_save_round_trip() {
    let dir = tempfile::tempdir().unwrap();
    let png = sample_png(dir.path(), "Shot.png", 30, 20);
    let src = engine::load_for_edit(&png).unwrap();
    assert_eq!((src.width, src.height, src.out_ext), (30, 20, "png"));
    let out = engine::save_edit(&png, "Cropped", "png", &src.png).unwrap();
    assert_eq!(out, dir.path().join("Shot Cropped.png"));
    assert_eq!(image::open(&out).unwrap().width(), 30);
    assert!(engine::save_edit(&png, "../evil", "png", &src.png).is_err());
    assert!(engine::save_edit(&png, "x", "exe", &src.png).is_err());
}

#[test]
fn mixed_selection_only_offers_shared_actions() {
    let dir = tempfile::tempdir().unwrap();
    let png = sample_png(dir.path(), "a.png", 8, 8);
    let pdf = dir.path().join("b.pdf");
    std::fs::write(&pdf, b"%PDF-1.4").unwrap();
    assert_eq!(ids(&[png, pdf]), vec!["convert.jpg".to_string(), "tool.merge".to_string()]);
}

#[test]
fn editors_only_offered_for_a_single_file() {
    let dir = tempfile::tempdir().unwrap();
    let a = sample_png(dir.path(), "a.png", 8, 8);
    let b = sample_png(dir.path(), "b.png", 8, 8);
    let has_crop = |paths: &[PathBuf]| engine::actions_for(paths).iter().any(|x| x.id == "tool.crop");
    assert!(has_crop(&[a.clone()]));
    assert!(!has_crop(&[a.clone(), b.clone()]));
    assert!(engine::actions_for(&[a, b]).iter().any(|x| x.id == "tool.compress"));
}

#[test]
fn unknown_files_get_no_slices() {
    assert!(ids(&[PathBuf::from("notes.xyz")]).is_empty());
}

#[test]
fn converts_png_to_every_target_next_to_original() {
    let dir = tempfile::tempdir().unwrap();
    let png = sample_png(dir.path(), "Page 001.png", 64, 40);
    for ext in ["jpg", "webp", "tiff", "bmp", "gif", "pdf"] {
        let out = engine::run(&format!("convert.{ext}"), &[png.clone()]).unwrap();
        assert_eq!(out, vec![dir.path().join(format!("Page 001.{ext}"))], "{ext}");
        assert!(std::fs::metadata(&out[0]).unwrap().len() > 0);
    }
    let back = image::open(dir.path().join("Page 001.jpg")).unwrap();
    assert_eq!((back.width(), back.height()), (64, 40));
}

#[test]
fn never_overwrites_existing_files() {
    let dir = tempfile::tempdir().unwrap();
    let png = sample_png(dir.path(), "a.png", 8, 8);
    std::fs::write(dir.path().join("a.jpg"), b"keep me").unwrap();
    let out = engine::run("convert.jpg", &[png]).unwrap();
    assert_eq!(out, vec![dir.path().join("a 2.jpg")]);
    assert_eq!(std::fs::read(dir.path().join("a.jpg")).unwrap(), b"keep me");
}

#[test]
fn tools_resize_rotate_and_crop() {
    let dir = tempfile::tempdir().unwrap();
    let png = sample_png(dir.path(), "a.png", 100, 60);
    let dims = |id: &str| {
        let out = engine::run(id, &[png.clone()]).unwrap();
        let img = image::open(&out[0]).unwrap();
        (img.width(), img.height())
    };
    assert_eq!(dims("tool.half"), (50, 30));
    assert_eq!(dims("tool.rotate"), (60, 100));
    assert_eq!(dims("tool.square"), (60, 60));
    assert_eq!(dims("tool.story"), (33, 60));
}

#[test]
fn image_to_pdf_round_trips_through_pdfium() {
    let dir = tempfile::tempdir().unwrap();
    let png = sample_png(dir.path(), "scan.png", 200, 100);
    let pdf = engine::run("convert.pdf", &[png]).unwrap().remove(0);
    let jpgs = match engine::run("convert.jpg", &[pdf]) {
        Ok(j) => j,
        Err(e) if e.to_string().contains("Pdfium library not found") => {
            eprintln!("skipping: {e}");
            return;
        }
        Err(e) => panic!("{e}"),
    };
    assert_eq!(jpgs, vec![dir.path().join("scan.jpg")]);
    let img = image::open(&jpgs[0]).unwrap();
    assert_eq!((img.width(), img.height()), (300, 150));
}
