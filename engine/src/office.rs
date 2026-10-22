//! PDF ↔ Word. Microsoft Word does it best, then LibreOffice; without either, a plain built-in converter
//! keeps the text and paragraphs.

use crate::pdf_ops::{self, TempFile};
use crate::{naming, pdf_write, Error, Result};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// Word normally takes 3–15 s, mostly starting up. Anything past this is stuck, so fall back rather than hang.
const TIMEOUT: Duration = Duration::from_secs(60);

/// `target` is "docx" (from a PDF) or "pdf" (from a Word document).
pub fn convert(path: &Path, target: &str) -> Result<PathBuf> {
    let out = naming::sibling(path, None, target);
    if std::env::var("SHIFTNINJA_OFFICE").as_deref() != Ok("builtin") {
        if cfg!(windows) && via_word(path, &out, target).is_ok() {
            return Ok(out);
        }
        if let Some(soffice) = find_soffice() {
            if via_libreoffice(&soffice, path, &out, target).is_ok() {
                return Ok(out);
            }
        }
    }
    let _ = std::fs::remove_file(&out);
    builtin(path, &out, target)?;
    Ok(out)
}

pub fn builtin(path: &Path, out: &Path, target: &str) -> Result<()> {
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
    match (ext.as_str(), target) {
        ("pdf", "docx") => write_docx(&pdf_ops::page_texts(path)?, out),
        ("docx", "pdf") => write_text_pdf(&read_docx(path)?, out),
        _ => Err(Error::Other(format!("Install Microsoft Word or LibreOffice to convert .{ext} files"))),
    }
}

fn run_with_timeout(mut cmd: Command) -> Result<()> {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000);
    }
    let mut child = cmd.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::piped()).spawn()?;
    let start = Instant::now();
    loop {
        if let Some(status) = child.try_wait()? {
            if status.success() {
                return Ok(());
            }
            let mut err = String::new();
            if let Some(mut s) = child.stderr.take() {
                let _ = s.read_to_string(&mut err);
            }
            return Err(Error::Other(err.trim().lines().next().unwrap_or("conversion failed").to_string()));
        }
        if start.elapsed() > TIMEOUT {
            let _ = child.kill();
            return Err(Error::Other("conversion took too long".into()));
        }
        std::thread::sleep(Duration::from_millis(150));
    }
}

/// Word only ever sees a uniquely named temporary copy: a crash can't get the customer's file put on Word's
/// "caused a serious error last time" list, and their original is never locked.
fn via_word(path: &Path, out: &Path, target: &str) -> Result<()> {
    let format = if target == "pdf" { 17 } else { 16 };
    let src_ext = path.extension().and_then(|e| e.to_str()).unwrap_or("docx");
    let (input, output, pid) = (TempFile::new(src_ext), TempFile::new(target), TempFile::new("pid"));
    std::fs::copy(path, &input.0)?;
    let quote = |p: &Path| p.to_string_lossy().replace('\'', "''");
    let script = format!(
        "$ErrorActionPreference = 'Stop'\r\n\
         $before = @(Get-Process WINWORD -ErrorAction SilentlyContinue | ForEach-Object Id)\r\n\
         # A past crash (an add-in, a killed conversion) leaves startup items behind, and Word then waits on an\r\n\
         # invisible 'serious problem, start in safe mode?' prompt. Word clears these itself on a clean start;\r\n\
         # do it for it, but only while no Word is open, since a running Word's items are live.\r\n\
         Get-ChildItem 'HKCU:\\Software\\Microsoft\\Office' -ErrorAction SilentlyContinue | ForEach-Object {{\r\n\
           $opts = Join-Path $_.PSPath 'Word\\Options'\r\n\
           if (Test-Path $opts) {{ New-ItemProperty -Path $opts -Name DisableConvertPdfWarning -Value 1 -PropertyType DWord -Force | Out-Null }}\r\n\
           $items = Join-Path $_.PSPath 'Word\\Resiliency\\StartupItems'\r\n\
           if ($before.Count -eq 0 -and (Test-Path $items)) {{\r\n\
             (Get-Item $items).Property | ForEach-Object {{ Remove-ItemProperty -Path $items -Name $_ -ErrorAction SilentlyContinue }}\r\n\
           }}\r\n\
         }}\r\n\
         $word = New-Object -ComObject Word.Application\r\n\
         Get-Process WINWORD -ErrorAction SilentlyContinue | Where-Object {{ $before -notcontains $_.Id }} |\r\n\
           ForEach-Object {{ Set-Content -LiteralPath '{pid}' -Value $_.Id }}\r\n\
         $dontSave = 0\r\n\
         try {{\r\n\
           $word.Visible = $false\r\n\
           $word.DisplayAlerts = 0\r\n\
           $word.AutomationSecurity = 3\r\n\
           $doc = $word.Documents.Open('{input}', $false, $true, $false)\r\n\
           try {{ $doc.SaveAs2('{output}', {format}) }} finally {{ $doc.Close([ref]$dontSave) }}\r\n\
         }} finally {{\r\n\
           $word.Quit([ref]$dontSave)\r\n\
           [void][Runtime.InteropServices.Marshal]::ReleaseComObject($word)\r\n\
           # Add-ins can keep a quit Word alive in the background; give it a moment, then close the one we started.\r\n\
           $mine = Get-Content -LiteralPath '{pid}' -ErrorAction SilentlyContinue\r\n\
           if ($mine) {{\r\n\
             $p = Get-Process -Id $mine -ErrorAction SilentlyContinue\r\n\
             if ($p -and -not $p.WaitForExit(2000)) {{ Stop-Process -Id $mine -Force -ErrorAction SilentlyContinue }}\r\n\
           }}\r\n\
         }}\r\n",
        pid = quote(&pid.0),
        input = quote(&input.0),
        output = quote(&output.0),
    );
    let ps1 = TempFile::new("ps1");
    let mut bytes = vec![0xEF, 0xBB, 0xBF];
    bytes.extend_from_slice(script.as_bytes());
    std::fs::write(&ps1.0, bytes)?;
    let mut cmd = Command::new("powershell");
    cmd.args(["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-File"]).arg(&ps1.0);
    if let Err(e) = run_with_timeout(cmd) {
        // Only ever the Word this conversion started; never the user's own open documents.
        if let Ok(id) = std::fs::read_to_string(&pid.0) {
            let mut kill = Command::new("taskkill");
            kill.args(["/F", "/PID", id.trim()]);
            let _ = run_with_timeout(kill);
        }
        return Err(e);
    }
    if !output.0.exists() {
        return Err(Error::Other("Word didn't write the file".into()));
    }
    std::fs::copy(&output.0, out)?;
    Ok(())
}

fn find_soffice() -> Option<PathBuf> {
    let mut candidates = Vec::new();
    for var in ["ProgramFiles", "ProgramFiles(x86)"] {
        if let Some(dir) = std::env::var_os(var) {
            candidates.push(PathBuf::from(dir).join("LibreOffice/program/soffice.exe"));
        }
    }
    candidates.push(PathBuf::from("/Applications/LibreOffice.app/Contents/MacOS/soffice"));
    candidates.push(PathBuf::from("/usr/bin/soffice"));
    candidates.push(PathBuf::from("/usr/bin/libreoffice"));
    candidates.into_iter().find(|p| p.exists())
}

fn via_libreoffice(soffice: &Path, path: &Path, out: &Path, target: &str) -> Result<()> {
    let dir = TempFile::new("lo");
    std::fs::create_dir_all(&dir.0)?;
    let mut cmd = Command::new(soffice);
    cmd.arg("--headless");
    if target == "docx" {
        cmd.args(["--infilter=writer_pdf_import", "--convert-to", "docx:MS Word 2007 XML"]);
    } else {
        cmd.args(["--convert-to", "pdf"]);
    }
    cmd.arg("--outdir").arg(&dir.0).arg(path);
    let result = run_with_timeout(cmd).and_then(|()| {
        let made = dir.0.join(format!("{}.{target}", path.file_stem().unwrap_or_default().to_string_lossy()));
        std::fs::copy(&made, out).map(|_| ()).map_err(Error::from)
    });
    let _ = std::fs::remove_dir_all(&dir.0);
    result
}

// ---- built-in PDF → DOCX ----

fn xml_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\t' | '\n' => out.push(c),
            c if (c as u32) < 0x20 || c == '\u{FFFE}' || c == '\u{FFFF}' => {}
            c => out.push(c),
        }
    }
    out
}

fn paragraph(text: &str) -> String {
    if text.trim().is_empty() {
        return "<w:p/>".into();
    }
    format!("<w:p><w:r><w:t xml:space=\"preserve\">{}</w:t></w:r></w:p>", xml_escape(text))
}

pub fn write_docx(pages: &[String], out: &Path) -> Result<()> {
    let mut body = String::new();
    for (i, page) in pages.iter().enumerate() {
        if i > 0 {
            body.push_str("<w:p><w:r><w:br w:type=\"page\"/></w:r></w:p>");
        }
        for line in page.lines() {
            body.push_str(&paragraph(line.trim_end_matches('\r')));
        }
    }
    let document = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\
         <w:document xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\"><w:body>{body}\
         <w:sectPr><w:pgSz w:w=\"11906\" w:h=\"16838\"/>\
         <w:pgMar w:top=\"1134\" w:right=\"1134\" w:bottom=\"1134\" w:left=\"1134\" w:header=\"708\" w:footer=\"708\" w:gutter=\"0\"/>\
         </w:sectPr></w:body></w:document>"
    );
    let files = [
        (
            "[Content_Types].xml",
            "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\
             <Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\">\
             <Default Extension=\"rels\" ContentType=\"application/vnd.openxmlformats-package.relationships+xml\"/>\
             <Default Extension=\"xml\" ContentType=\"application/xml\"/>\
             <Override PartName=\"/word/document.xml\" \
             ContentType=\"application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml\"/></Types>"
                .to_string(),
        ),
        (
            "_rels/.rels",
            "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\
             <Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">\
             <Relationship Id=\"rId1\" \
             Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument\" \
             Target=\"word/document.xml\"/></Relationships>"
                .to_string(),
        ),
        ("word/document.xml", document),
    ];
    let mut zip = zip::ZipWriter::new(std::fs::File::create(out)?);
    let options = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    for (name, content) in files {
        zip.start_file(name, options).map_err(zip_err)?;
        zip.write_all(content.as_bytes())?;
    }
    zip.finish().map_err(zip_err)?;
    Ok(())
}

fn zip_err(e: zip::result::ZipError) -> Error {
    Error::Other(format!("Word file error: {e}"))
}

// ---- built-in DOCX → PDF ----

#[derive(Debug, PartialEq)]
pub enum Block {
    Para { text: String, heading: bool },
    PageBreak,
}

pub fn read_docx(path: &Path) -> Result<Vec<Block>> {
    let mut archive = zip::ZipArchive::new(std::fs::File::open(path)?).map_err(zip_err)?;
    let mut xml = String::new();
    archive.by_name("word/document.xml").map_err(zip_err)?.read_to_string(&mut xml)?;
    Ok(parse_document(&xml))
}

fn unescape(s: &str) -> String {
    if !s.contains('&') {
        return s.to_string();
    }
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(at) = rest.find('&') {
        out.push_str(&rest[..at]);
        rest = &rest[at..];
        let Some(end) = rest.find(';') else { break };
        let entity = &rest[1..end];
        let decoded = match entity {
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "apos" => Some('\''),
            e if e.starts_with("#x") => u32::from_str_radix(&e[2..], 16).ok().and_then(char::from_u32),
            e if e.starts_with('#') => e[1..].parse().ok().and_then(char::from_u32),
            _ => None,
        };
        match decoded {
            Some(c) => {
                out.push(c);
                rest = &rest[end + 1..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

fn attr<'a>(tag: &'a str, name: &str) -> Option<&'a str> {
    let at = tag.find(&format!("{name}=\""))? + name.len() + 2;
    tag[at..].find('"').map(|end| &tag[at..at + end])
}

/// Just enough WordprocessingML: paragraphs, runs of text, tabs, line and page breaks, heading styles.
pub fn parse_document(xml: &str) -> Vec<Block> {
    let mut blocks = Vec::new();
    let (mut text, mut heading, mut in_text) = (String::new(), false, false);
    let mut rest = xml;
    while let Some(open) = rest.find('<') {
        if in_text {
            text.push_str(&unescape(&rest[..open]));
        }
        let Some(close) = rest[open..].find('>') else { break };
        let tag = &rest[open + 1..open + close];
        rest = &rest[open + close + 1..];
        let closing = tag.starts_with('/');
        let name = tag.trim_start_matches('/').split([' ', '/', '\t', '\r', '\n']).next().unwrap_or("");
        match (name, closing) {
            ("w:p", false) => {
                text.clear();
                heading = false;
                if tag.ends_with('/') {
                    blocks.push(Block::Para { text: String::new(), heading: false });
                }
            }
            ("w:p", true) => blocks.push(Block::Para { text: std::mem::take(&mut text), heading }),
            ("w:pStyle", _) => {
                heading = attr(tag, "w:val").is_some_and(|v| v.starts_with("Heading") || v == "Title");
            }
            ("w:t", false) => in_text = !tag.ends_with('/'),
            ("w:t", true) => in_text = false,
            ("w:tab", _) => text.push_str("    "),
            ("w:br", _) if attr(tag, "w:type") == Some("page") => {
                blocks.push(Block::Para { text: std::mem::take(&mut text), heading });
                blocks.push(Block::PageBreak);
            }
            ("w:br", _) | ("w:cr", _) => text.push('\n'),
            _ => {}
        }
    }
    blocks
}

/// Helvetica advance widths (1/1000 em) for ' '..='~'.
const HELVETICA: [u16; 95] = [
    278, 278, 355, 556, 556, 889, 667, 191, 333, 333, 389, 584, 278, 333, 278, 278, 556, 556, 556, 556, 556, 556,
    556, 556, 556, 556, 278, 278, 584, 584, 584, 556, 1015, 667, 667, 722, 722, 667, 611, 778, 722, 278, 500, 667,
    556, 833, 722, 778, 667, 778, 722, 667, 611, 722, 667, 944, 667, 667, 611, 278, 278, 278, 469, 556, 333, 556,
    556, 500, 556, 556, 278, 556, 556, 222, 222, 500, 222, 833, 556, 556, 556, 556, 333, 500, 278, 556, 500, 722,
    500, 500, 500, 334, 260, 334, 584,
];

/// WinAnsi byte for a character the standard PDF fonts can draw.
fn win_ansi(c: char) -> u8 {
    match c {
        ' '..='~' => c as u8,
        '\u{A0}'..='\u{FF}' => c as u32 as u8,
        '\u{20AC}' => 0x80,
        '\u{2026}' => 0x85,
        '\u{2018}' => 0x91,
        '\u{2019}' => 0x92,
        '\u{201C}' => 0x93,
        '\u{201D}' => 0x94,
        '\u{2022}' => 0x95,
        '\u{2013}' => 0x96,
        '\u{2014}' => 0x97,
        _ => b'?',
    }
}

fn width(bytes: &[u8], size: f32, bold: bool) -> f32 {
    let em: u32 = bytes
        .iter()
        .map(|&b| if (32..=126).contains(&b) { HELVETICA[(b - 32) as usize] as u32 } else { 556 })
        .sum();
    em as f32 * size / 1000.0 * if bold { 1.06 } else { 1.0 }
}

fn wrap(text: &str, size: f32, bold: bool, max: f32) -> Vec<Vec<u8>> {
    let mut lines = Vec::new();
    for raw in text.split('\n') {
        let mut line: Vec<u8> = Vec::new();
        for word in raw.split(' ') {
            let word: Vec<u8> = word.chars().map(win_ansi).collect();
            let candidate = if line.is_empty() { word.clone() } else { [line.as_slice(), b" ", &word].concat() };
            if width(&candidate, size, bold) <= max || line.is_empty() {
                line = candidate;
            } else {
                lines.push(std::mem::replace(&mut line, word));
            }
            while width(&line, size, bold) > max && line.len() > 1 {
                let mut cut = line.len() - 1;
                while cut > 1 && width(&line[..cut], size, bold) > max {
                    cut -= 1;
                }
                let tail = line.split_off(cut);
                lines.push(std::mem::replace(&mut line, tail));
            }
        }
        lines.push(line);
    }
    lines
}

fn pdf_string(bytes: &[u8]) -> String {
    let mut out = String::from("(");
    for &b in bytes {
        match b {
            b'(' | b')' | b'\\' => {
                out.push('\\');
                out.push(b as char);
            }
            32..=126 => out.push(b as char),
            _ => out.push_str(&format!("\\{b:03o}")),
        }
    }
    out.push(')');
    out
}

pub fn write_text_pdf(blocks: &[Block], out: &Path) -> Result<()> {
    const MARGIN: f32 = 56.7;
    let (pw, ph) = pdf_write::A4;
    let max = pw - 2.0 * MARGIN;
    let mut pages: Vec<String> = vec![String::new()];
    let mut y = ph - MARGIN;
    for block in blocks {
        let Block::Para { text, heading } = block else {
            if !pages.last().unwrap().is_empty() {
                pages.push(String::new());
                y = ph - MARGIN;
            }
            continue;
        };
        let (size, font) = if *heading { (15.0, "F2") } else { (11.0, "F1") };
        let lead = size * 1.35;
        for line in wrap(text, size, *heading, max) {
            if y - lead < MARGIN {
                pages.push(String::new());
                y = ph - MARGIN;
            }
            y -= lead;
            if !line.is_empty() {
                let page = pages.last_mut().unwrap();
                page.push_str(&format!("BT /{font} {size} Tf {MARGIN:.2} {y:.2} Td {} Tj ET\n", pdf_string(&line)));
            }
        }
        y -= size * 0.5;
    }

    let mut buf: Vec<u8> = b"%PDF-1.4\n%\xE2\xE3\xCF\xD3\n".to_vec();
    let mut offsets: Vec<usize> = Vec::new();
    let mut obj = |buf: &mut Vec<u8>, body: &[u8]| {
        offsets.push(buf.len());
        buf.extend_from_slice(format!("{} 0 obj\n", offsets.len()).as_bytes());
        buf.extend_from_slice(body);
        buf.extend_from_slice(b"\nendobj\n");
    };
    let page_obj = |i: usize| 5 + i * 2;
    let kids: Vec<String> = (0..pages.len()).map(|i| format!("{} 0 R", page_obj(i))).collect();
    obj(&mut buf, b"<< /Type /Catalog /Pages 2 0 R >>");
    obj(&mut buf, format!("<< /Type /Pages /Kids [{}] /Count {} >>", kids.join(" "), pages.len()).as_bytes());
    obj(&mut buf, b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>");
    obj(&mut buf, b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica-Bold /Encoding /WinAnsiEncoding >>");
    for (i, content) in pages.iter().enumerate() {
        obj(
            &mut buf,
            format!(
                "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {pw:.2} {ph:.2}] \
                 /Resources << /Font << /F1 3 0 R /F2 4 0 R >> >> /Contents {} 0 R >>",
                page_obj(i) + 1
            )
            .as_bytes(),
        );
        obj(&mut buf, format!("<< /Length {} >>\nstream\n{content}endstream", content.len()).as_bytes());
    }
    pdf_write::finish(buf, offsets, out)
}
