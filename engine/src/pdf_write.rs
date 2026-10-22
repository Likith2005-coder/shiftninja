use crate::Result;
use image::codecs::jpeg::JpegEncoder;
use image::DynamicImage;
use std::io::Write;
use std::path::Path;

const PT_PER_PX: f32 = 0.75;
pub const A4: (f32, f32) = (595.28, 841.89);
const A4_MARGIN: f32 = 24.0;
/// Long side cap for photos placed on A4: 300 dpi is all a printer can use.
const A4_MAX_PX: u32 = 3508;

/// One PDF page showing a single JPEG, scaled into `rect` (x, y, w, h in points).
pub struct Page {
    pub jpeg: Vec<u8>,
    pub px: (u32, u32),
    pub size: (f32, f32),
    pub rect: (f32, f32, f32, f32),
}

pub fn jpeg(img: &DynamicImage, quality: u8) -> Result<(Vec<u8>, (u32, u32))> {
    let rgb = DynamicImage::ImageRgb8(crate::image_ops::flatten_on_white(img));
    let mut out = Vec::new();
    JpegEncoder::new_with_quality(&mut out, quality).encode_image(&rgb)?;
    Ok((out, (rgb.width(), rgb.height())))
}

/// Each image becomes a page exactly its own size.
pub fn write_images(images: &[DynamicImage], out: &Path) -> Result<()> {
    let pages = images
        .iter()
        .map(|img| {
            let (jpeg, px) = jpeg(img, 92)?;
            let size = (px.0 as f32 * PT_PER_PX, px.1 as f32 * PT_PER_PX);
            Ok(Page { jpeg, px, size, rect: (0.0, 0.0, size.0, size.1) })
        })
        .collect::<Result<Vec<_>>>()?;
    write_pages(&pages, out)
}

/// A photo centred on an A4 page (landscape for wide photos), ready to print.
pub fn a4_page(img: &DynamicImage) -> Result<Page> {
    let img = if img.width().max(img.height()) > A4_MAX_PX { img.thumbnail(A4_MAX_PX, A4_MAX_PX) } else { img.clone() };
    let (jpeg, px) = jpeg(&img, 88)?;
    let size = if px.0 > px.1 { (A4.1, A4.0) } else { A4 };
    let (rw, rh) = (size.0 - 2.0 * A4_MARGIN, size.1 - 2.0 * A4_MARGIN);
    let k = (rw / px.0 as f32).min(rh / px.1 as f32);
    let (w, h) = (px.0 as f32 * k, px.1 as f32 * k);
    Ok(Page { jpeg, px, size, rect: ((size.0 - w) / 2.0, (size.1 - h) / 2.0, w, h) })
}

pub fn write_pages(pages: &[Page], out: &Path) -> Result<()> {
    let mut buf: Vec<u8> = Vec::new();
    let mut offsets: Vec<usize> = Vec::new();
    buf.extend_from_slice(b"%PDF-1.4\n%\xE2\xE3\xCF\xD3\n");

    let page_obj = |i: usize| 3 + i * 3;
    let kids: Vec<String> = (0..pages.len()).map(|i| format!("{} 0 R", page_obj(i))).collect();

    let obj = |buf: &mut Vec<u8>, offsets: &mut Vec<usize>, body: &[u8]| {
        offsets.push(buf.len());
        buf.extend_from_slice(format!("{} 0 obj\n", offsets.len()).as_bytes());
        buf.extend_from_slice(body);
        buf.extend_from_slice(b"\nendobj\n");
    };

    obj(&mut buf, &mut offsets, b"<< /Type /Catalog /Pages 2 0 R >>");
    obj(
        &mut buf,
        &mut offsets,
        format!("<< /Type /Pages /Kids [{}] /Count {} >>", kids.join(" "), pages.len()).as_bytes(),
    );

    for (i, page) in pages.iter().enumerate() {
        let ((pw, ph), (x, y, w, h), (iw, ih)) = (page.size, page.rect, page.px);
        let n = page_obj(i);
        obj(
            &mut buf,
            &mut offsets,
            format!(
                "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {pw:.2} {ph:.2}] \
                 /Resources << /XObject << /Im0 {} 0 R >> >> /Contents {} 0 R >>",
                n + 2,
                n + 1
            )
            .as_bytes(),
        );
        let content = format!("q {w:.2} 0 0 {h:.2} {x:.2} {y:.2} cm /Im0 Do Q");
        obj(
            &mut buf,
            &mut offsets,
            format!("<< /Length {} >>\nstream\n{content}\nendstream", content.len()).as_bytes(),
        );
        let mut image_obj = format!(
            "<< /Type /XObject /Subtype /Image /Width {iw} /Height {ih} /ColorSpace /DeviceRGB \
             /BitsPerComponent 8 /Filter /DCTDecode /Length {} >>\nstream\n",
            page.jpeg.len()
        )
        .into_bytes();
        image_obj.extend_from_slice(&page.jpeg);
        image_obj.extend_from_slice(b"\nendstream");
        obj(&mut buf, &mut offsets, &image_obj);
    }

    finish(buf, offsets, out)
}

pub(crate) fn finish(mut buf: Vec<u8>, offsets: Vec<usize>, out: &Path) -> Result<()> {
    let xref = buf.len();
    write!(buf, "xref\n0 {}\n0000000000 65535 f \n", offsets.len() + 1)?;
    for off in &offsets {
        write!(buf, "{off:010} 00000 n \n")?;
    }
    write!(buf, "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n", offsets.len() + 1)?;
    std::fs::write(out, buf)?;
    Ok(())
}
