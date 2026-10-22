<p align="center">
  <img src="docs/shiftninja-logo.svg" width="128" alt="ShiftNinja logo" />
</p>

<h1 align="center">ShiftNinja</h1>

<p align="center"><b>PDF, Word &amp; photo tools.</b><br/>Drag a file, press Shift, drop it on a wheel. Works offline.<br/>Made by <b>Likith B S</b>.</p>

<h2 align="center">
  <a href="https://github.com/Likith2005-coder/shiftninja/releases/latest/download/ShiftNinja-Setup.exe">⬇ Download ShiftNinja (Windows)</a>
</h2>

---

## Install (1 minute)

1. Click **[Download ShiftNinja](https://github.com/Likith2005-coder/shiftninja/releases/latest/download/ShiftNinja-Setup.exe)**. The file `ShiftNinja-Setup.exe` downloads.
2. Double-click it.
3. If Windows says **"Windows protected your PC"**, click **More info → Run anyway**.
4. Click **Install**. Done: ShiftNinja starts with Windows and sits near the clock.

**हिंदी में:** ऊपर **Download** दबाएँ → फ़ाइल पर डबल-क्लिक करें → "Windows protected your PC" दिखे तो **More info → Run anyway** → **Install**।

Works on Windows 10 and 11. Microsoft Word on the computer gives the best PDF ↔ Word results (without it, a simpler built-in converter is used).

## How to use

1. Start dragging a PDF, Word file or photo in File Explorer or on the desktop.
2. While dragging, press **Shift**. A wheel opens under the mouse.
3. Drop the file on a petal. The new file is saved **next to the original**; the original is never changed.

- Press **Shift** again (or scroll) to switch between the **Convert** and **Tools** wheels.
- No dragging? Click a file and press **Ctrl + Alt + Space**, then click a petal or press its number.
- Select several files to convert them all, or to **Merge** them into one PDF.

## What it does

| File | Convert wheel | Tools wheel |
|---|---|---|
| **PDF** | Word, JPG, PNG, Text | Compress, Split pages, Rotate, Crop, Edit, Annotate, Add background, Redact |
| **Word** (.docx, .doc, .odt, .rtf) | PDF | |
| **Photo** (JPG, JPEG, PNG, WEBP, SVG, GIF, BMP, TIFF) | JPG, JPEG, PNG, WEBP, PDF, SVG, GIF, BMP, TIFF | Crop, Compress, Remove metadata, Edit, Annotate, Add background, Redact, **Passport photo sheet** |
| **2 or more** PDFs / photos | | **Merge** into one PDF |

- **Passport sheet:** 30 photos of 35 × 45 mm on one A4 sheet with cut lines, ready to print at 300 dpi.
- **Compress PDF:** makes PDFs small enough for WhatsApp and email (pages become images).
- **SVG output** wraps the picture in an .svg file for portals that ask for SVG; it isn't traced into vector shapes.

---

## For the owner: publishing an update

1. Raise `version` in `app/src-tauri/tauri.conf.json` (for example `0.1.0` → `0.2.0`).
2. Commit, then push a tag with the same number:

   ```bash
   git tag v0.2.0
   git push origin main v0.2.0
   ```

3. GitHub builds the installer by itself (about 15 minutes) and puts it on the [Releases](https://github.com/Likith2005-coder/shiftninja/releases) page. The download link above always points to the newest one.

To build it on your own computer instead: install [Rust](https://rustup.rs), Node.js and the Visual Studio C++ Build Tools, then run `scripts/fetch-pdfium.sh`, `cd app`, `npm install`, `npm run build`. The installer appears in `target/release/bundle/nsis/`. Technical notes are in [docs/DEVELOPING.md](docs/DEVELOPING.md).

## Licence

© 2026 Likith B S. All rights reserved; see [LICENSE](LICENSE). PDF rendering by [PDFium](https://pdfium.googlesource.com/pdfium/); font Inter Tight (SIL Open Font Licence).
