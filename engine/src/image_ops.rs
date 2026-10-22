use crate::{naming, Error, Result};
use image::codecs::jpeg::JpegEncoder;
use image::codecs::png::{CompressionType, FilterType, PngEncoder};
use image::codecs::webp::WebPEncoder;
use image::{DynamicImage, ImageDecoder, ImageFormat, ImageReader, Rgba, RgbaImage};
use std::fs::File;
use std::io::BufWriter;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU32, Ordering};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Png,
    Jpg,
    Webp,
    Tiff,
    Bmp,
    Gif,
    Ico,
    Heic,
    Svg,
}

impl Format {
    pub fn from_ext(ext: &str) -> Option<Format> {
        Some(match ext.to_ascii_lowercase().as_str() {
            "png" => Format::Png,
            "jpg" | "jpeg" => Format::Jpg,
            "webp" => Format::Webp,
            "tif" | "tiff" => Format::Tiff,
            "bmp" => Format::Bmp,
            "gif" => Format::Gif,
            "ico" => Format::Ico,
            "heic" | "heif" => Format::Heic,
            "svg" => Format::Svg,
            _ => return None,
        })
    }

    pub fn ext(self) -> &'static str {
        match self {
            Format::Png => "png",
            Format::Jpg => "jpg",
            Format::Webp => "webp",
            Format::Tiff => "tiff",
            Format::Bmp => "bmp",
            Format::Gif => "gif",
            Format::Ico => "ico",
            Format::Heic => "heic",
            Format::Svg => "svg",
        }
    }

    pub fn readable(self) -> bool {
        self != Format::Heic || heic_supported()
    }
}

pub fn heic_supported() -> bool {
    cfg!(target_os = "macos")
}

fn temp_path(ext: &str) -> PathBuf {
    static N: AtomicU32 = AtomicU32::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!("shiftninja-{}-{n}.{ext}", std::process::id()))
}

fn sips(args: &[&str]) -> Result<()> {
    let out = Command::new("/usr/bin/sips").args(args).output()?;
    if out.status.success() {
        Ok(())
    } else {
        Err(Error::Other(format!("sips failed: {}", String::from_utf8_lossy(&out.stderr).trim())))
    }
}

pub fn png_bytes(img: &DynamicImage) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    img.write_with_encoder(PngEncoder::new_with_quality(&mut out, CompressionType::Fast, FilterType::Adaptive))?;
    Ok(out)
}

/// SVGs are drawn at a decent size (longest side at least 1024 px) so small icons stay sharp in every tool.
fn load_svg(path: &Path) -> Result<DynamicImage> {
    let mut options = resvg::usvg::Options::default();
    options.resources_dir = path.parent().map(Path::to_path_buf);
    options.fontdb_mut().load_system_fonts();
    let tree = resvg::usvg::Tree::from_data(&std::fs::read(path)?, &options)
        .map_err(|e| Error::Other(format!("couldn't read this SVG: {e}")))?;
    let size = tree.size();
    let longest = size.width().max(size.height()).max(1.0);
    let scale = (1024.0 / longest).clamp(1.0, 4096.0 / longest);
    let (w, h) = ((size.width() * scale).ceil() as u32, (size.height() * scale).ceil() as u32);
    let mut pixmap = resvg::tiny_skia::Pixmap::new(w.max(1), h.max(1)).ok_or_else(|| Error::Other("SVG is empty".into()))?;
    resvg::render(&tree, resvg::tiny_skia::Transform::from_scale(scale, scale), &mut pixmap.as_mut());
    let mut rgba = RgbaImage::new(pixmap.width(), pixmap.height());
    for (px, c) in rgba.pixels_mut().zip(pixmap.pixels()) {
        let c = c.demultiply();
        *px = Rgba([c.red(), c.green(), c.blue(), c.alpha()]);
    }
    Ok(DynamicImage::ImageRgba8(rgba))
}

/// An SVG that carries the image inside it: opens anywhere an .svg is required, but isn't traced into shapes.
fn save_svg(img: &DynamicImage, out: &Path) -> Result<()> {
    use base64::Engine as _;
    let png = png_bytes(img)?;
    let (w, h) = (img.width(), img.height());
    let svg = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" xmlns:xlink=\"http://www.w3.org/1999/xlink\" width=\"{w}\" height=\"{h}\" viewBox=\"0 0 {w} {h}\">
  <image width=\"{w}\" height=\"{h}\" xlink:href=\"data:image/png;base64,{}\"/>
</svg>
",
        base64::engine::general_purpose::STANDARD.encode(png)
    );
    std::fs::write(out, svg)?;
    Ok(())
}

pub fn load(path: &Path) -> Result<DynamicImage> {
    if Format::from_ext(path.extension().and_then(|e| e.to_str()).unwrap_or("")) == Some(Format::Svg) {
        return load_svg(path);
    }
    if Format::from_ext(path.extension().and_then(|e| e.to_str()).unwrap_or("")) == Some(Format::Heic) {
        let tmp = temp_path("png");
        sips(&["-s", "format", "png", &path.to_string_lossy(), "--out", &tmp.to_string_lossy()])?;
        let img = image::open(&tmp);
        let _ = std::fs::remove_file(&tmp);
        return Ok(img?);
    }
    let mut decoder = ImageReader::open(path)?.with_guessed_format()?.into_decoder()?;
    let orientation = decoder.orientation()?;
    let mut img = DynamicImage::from_decoder(decoder)?;
    img.apply_orientation(orientation);
    Ok(img)
}

pub(crate) fn flatten_on_white(img: &DynamicImage) -> image::RgbImage {
    let rgba = img.to_rgba8();
    let mut bg = RgbaImage::from_pixel(rgba.width(), rgba.height(), Rgba([255, 255, 255, 255]));
    image::imageops::overlay(&mut bg, &rgba, 0, 0);
    DynamicImage::ImageRgba8(bg).to_rgb8()
}

pub fn save(img: &DynamicImage, out: &Path, format: Format, quality: u8) -> Result<()> {
    match format {
        Format::Jpg => {
            let w = BufWriter::new(File::create(out)?);
            JpegEncoder::new_with_quality(w, quality).encode_image(&flatten_on_white(img))?;
        }
        Format::Png => {
            let w = BufWriter::new(File::create(out)?);
            img.write_with_encoder(PngEncoder::new_with_quality(w, CompressionType::Best, FilterType::Adaptive))?;
        }
        Format::Webp => {
            let w = BufWriter::new(File::create(out)?);
            img.write_with_encoder(WebPEncoder::new_lossless(w))?;
        }
        Format::Gif | Format::Bmp | Format::Tiff => {
            let fmt = match format {
                Format::Gif => ImageFormat::Gif,
                Format::Bmp => ImageFormat::Bmp,
                _ => ImageFormat::Tiff,
            };
            DynamicImage::ImageRgba8(img.to_rgba8()).save_with_format(out, fmt)?;
        }
        Format::Ico => {
            let icon = if img.width() > 256 || img.height() > 256 { img.thumbnail(256, 256) } else { img.clone() };
            DynamicImage::ImageRgba8(icon.to_rgba8()).save_with_format(out, ImageFormat::Ico)?;
        }
        Format::Svg => save_svg(img, out)?,
        Format::Heic => {
            if !heic_supported() {
                return Err(Error::Unsupported("HEIC".into()));
            }
            let tmp = temp_path("png");
            save(img, &tmp, Format::Png, quality)?;
            let res = sips(&["-s", "format", "heic", &tmp.to_string_lossy(), "--out", &out.to_string_lossy()]);
            let _ = std::fs::remove_file(&tmp);
            res?;
        }
    }
    Ok(())
}

fn crop_to_ratio(img: &DynamicImage, rw: u32, rh: u32) -> DynamicImage {
    let (w, h) = (img.width(), img.height());
    let (cw, ch) = if w * rh > h * rw { (h * rw / rh, h) } else { (w, w * rh / rw) };
    img.crop_imm((w - cw) / 2, (h - ch) / 2, cw, ch)
}

pub fn tool(path: &Path, src: Format, tool: &str) -> Result<PathBuf> {
    let img = load(path)?;
    // Results of tools on an SVG are pictures, so they come out as PNG rather than a wrapped SVG.
    let keep = if matches!(src, Format::Svg) || (matches!(src, Format::Heic) && !heic_supported()) { Format::Png } else { src };
    let (result, suffix, format, quality) = match tool {
        "compress" => {
            let fmt = if matches!(src, Format::Jpg | Format::Png | Format::Heic) { keep } else { Format::Jpg };
            (img, "Compressed", fmt, 70)
        }
        "half" => {
            let (w, h) = ((img.width() / 2).max(1), (img.height() / 2).max(1));
            (img.resize_exact(w, h, image::imageops::FilterType::Lanczos3), "Half", keep, 90)
        }
        "rotate" => (img.rotate90(), "Rotated", keep, 92),
        "strip" => (img, "Clean", keep, 95),
        "square" => (crop_to_ratio(&img, 1, 1), "Square", keep, 92),
        "story" => (crop_to_ratio(&img, 9, 16), "9x16", keep, 92),
        _ => return Err(Error::Unsupported(format!("tool.{tool}"))),
    };
    let out = naming::sibling(path, Some(suffix), format.ext());
    save(&result, &out, format, quality)?;
    Ok(out)
}

/// 35 × 45 mm photos tiled on an A4 sheet at 300 dpi, with light cut guides between them.
pub fn passport_sheet(path: &Path) -> Result<PathBuf> {
    const SHEET: (u32, u32) = (2480, 3508);
    const PHOTO: (u32, u32) = (413, 531);
    const GAP: u32 = 24;
    const GRID: (u32, u32) = (5, 6);
    let photo = crop_to_ratio(&load(path)?, 35, 45).resize_exact(PHOTO.0, PHOTO.1, image::imageops::FilterType::Lanczos3);
    let photo = flatten_on_white(&photo);
    let mut sheet = image::RgbImage::from_pixel(SHEET.0, SHEET.1, image::Rgb([255, 255, 255]));
    let span = |n: u32, size: u32| n * size + (n - 1) * GAP;
    let (x0, y0) = ((SHEET.0 - span(GRID.0, PHOTO.0)) / 2, (SHEET.1 - span(GRID.1, PHOTO.1)) / 2);
    let guide = image::Rgb([200, 200, 200]);
    for c in 0..=GRID.0 {
        let x = (x0 + c * (PHOTO.0 + GAP)).saturating_sub(GAP / 2).min(SHEET.0 - 1);
        for y in 0..SHEET.1 {
            sheet.put_pixel(x, y, guide);
        }
    }
    for r in 0..=GRID.1 {
        let y = (y0 + r * (PHOTO.1 + GAP)).saturating_sub(GAP / 2).min(SHEET.1 - 1);
        for x in 0..SHEET.0 {
            sheet.put_pixel(x, y, guide);
        }
    }
    for r in 0..GRID.1 {
        for c in 0..GRID.0 {
            let (x, y) = (x0 + c * (PHOTO.0 + GAP), y0 + r * (PHOTO.1 + GAP));
            image::imageops::replace(&mut sheet, &photo, x as i64, y as i64);
        }
    }
    let out = naming::sibling(path, Some("Passport Sheet"), "jpg");
    let mut encoder = JpegEncoder::new_with_quality(BufWriter::new(File::create(&out)?), 95);
    encoder.set_pixel_density(image::codecs::jpeg::PixelDensity::dpi(300));
    encoder.encode_image(&DynamicImage::ImageRgb8(sheet))?;
    Ok(out)
}
