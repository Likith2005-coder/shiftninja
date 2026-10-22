# Developing ShiftNinja

ShiftNinja is a Windows tray tool: drag a file, press **Shift**, drop it on a wheel.

## Build from source

You need [Rust](https://rustup.rs), Node.js 18+ and the Visual Studio C++ Build Tools ("Desktop development with C++").

```bash
git clone https://github.com/likith2005-coder/shiftninja
cd shiftninja
scripts/fetch-pdfium.sh     # downloads the PDF engine into lib/
cd app && npm install
npm run dev                 # run it
npm run build               # make the installer in target/release/bundle/nsis/
```

## Command line

Every wheel action also works from the terminal:

```bash
cargo run -p shiftninja-cli -- list scan.pdf               # which actions fit this file
cargo run -p shiftninja-cli -- run convert.docx scan.pdf   # PDF to Word
cargo run -p shiftninja-cli -- run tool.passport photo.jpg # passport sheet
```

## How it fits together

- **`engine/`**: every conversion and tool (images, PDFs, Word via `office.rs`). No UI.
- **`app/`**: the Tauri 2 tray app. The wheel, editors and the how-to card are plain HTML/CSS/JS in `app/ui/`.
- **`app/src-tauri/src/platform/windows.rs`**: spots "a file is being dragged from Explorer and Shift went down", reads the selection, and drives the wheel from the mouse (the release point picks the petal).

## Tests

```bash
SHIFTNINJA_PDFIUM="$PWD/lib" cargo test -p shiftninja-engine
```

For the UI, serve the repo (`python -m http.server`) and open `docs/dev/ui-tests.html`. `docs/dev/wheel-preview.html?demo`, `editor-preview.html?mode=crop` and `card-preview.html` preview each window without building the app.
