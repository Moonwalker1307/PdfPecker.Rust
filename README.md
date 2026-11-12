# pdfpecker for Rust

Fast HTML and Markdown to PDF, and PDF to images, text and Markdown, for Rust. No browser, no Chromium download, no
.NET to install, and no runtime dependencies.

```
cargo add pdfpecker
```

```rust
use pdfpecker::{ImageOptions, PageSize, PdfOptions, TextOptions};

let pdf = pdfpecker::html_to_pdf("<h1>Invoice 42</h1><p>Total: <b>1,250.00 EUR</b></p>", &PdfOptions::new())?;
std::fs::write("invoice.pdf", &pdf)?;

// Images and CSS resolve next to the file.
let invoice = pdfpecker::html_file_to_pdf("templates/invoice.html", &PdfOptions::new())?;
let readme = pdfpecker::markdown_file_to_pdf("README.md", &PdfOptions::new().page_size(PageSize::Letter))?;

let pngs = pdfpecker::pdf_to_images(&pdf, &ImageOptions::new().dpi(96.0))?; // one PNG per page
let text = pdfpecker::pdf_to_text(&pdf, &TextOptions::new())?;               // pages separated by \f
let both = pdfpecker::merge_pdfs(&[&invoice, &readme], &Default::default())?;
```

This is the [PdfPecker](https://github.com/Moonwalker1307/PdfPecker) .NET library, compiled ahead of time into a
native library: the same library the [Python](https://pypi.org/project/pdfpecker/) and
[npm](https://www.npmjs.com/package/pdfpecker) packages ship. PdfPecker has its own HTML parser, CSS engine, layout
engine, PDF writer and PDF renderer, so a one-page invoice takes under a millisecond and no browser ever starts. It
runs no JavaScript in the documents and never goes online: images, stylesheets and fonts load from `base_dir`. The
crate version is the library version it holds. Documentation of what PdfPecker renders, its CSS support and its
limitations: [pdfpecker.net/docs](https://pdfpecker.net/docs/).

## Platforms and deployment

Rust 1.85 or later on Linux (glibc 2.28+ and musl, x64 and arm64), macOS 12+ (Apple Silicon and Intel) and Windows
(x64 and arm64).

The build script downloads the target's native library from the pdfpecker wheel on PyPI (about 9 MB, once; it is kept
in your cache folder) with `curl` or `wget`, and checks the wheel and the library against SHA-256 values that the crate
carries. `cargo run` and `cargo test` then find it on their own. To ship a program, copy the library next to the
executable:

| Platform | File | From |
|---|---|---|
| Linux | `pdfpecker_native.so` | `target/<profile>/build/pdfpecker-*/out/` |
| macOS | `pdfpecker_native.dylib` | the same |
| Windows | `pdfpecker_native.dll` | the same |

`pdfpecker::library_path()` returns the file that was loaded. The library is looked for in `PDFPECKER_NATIVE_LIB`
(a file path), then next to the executable, then where the build put it.

| Build-time variable | Effect |
|---|---|
| `PDFPECKER_LIB_DIR` | A folder that already holds the library; nothing is downloaded (offline and air-gapped builds). |
| `PDFPECKER_CACHE_DIR` | Where downloaded libraries are kept, instead of the user's cache folder. |

On musl (Alpine), the program must be linked dynamically to load the library: build with
`RUSTFLAGS="-C target-feature=-crt-static"`. The build stops with that advice otherwise.

## Speed

From Rust on a 4-core Linux cloud VM, median time per document after warm-up; the documents are the benchmark pages
of the PdfPecker repository:

| | Invoice, 1 page | Feature tour, 5 pages | Report, 29 pages | New process, first PDF | Peak memory, one PDF |
|---|---:|---:|---:|---:|---:|
| **pdfpecker** | **0.72 ms** | **2.9 ms** | **28 ms** | **22 ms** | **26 MB** |
| Chromium 153 via chromiumoxide 0.9 | 18 ms | 32 ms | 231 ms | 115 ms | 396 MB |
| IronPress 1.7 | 41 ms | 168 ms | 1,873 ms | 185 ms | 40 MB |

Conversions run in parallel from several threads: 6,600 invoices a second on the 4 cores.

## API

Every function returns `pdfpecker::Result`. PDFs are given as `&[u8]`; options are built with chained methods from
`::new()` or `Default::default()`.

| Function | Returns |
|---|---|
| `html_to_pdf(html, &PdfOptions)` | the PDF bytes |
| `html_file_to_pdf(path, &PdfOptions)` | the same for a file; `base_dir` is the file's folder |
| `markdown_to_pdf(markdown, &PdfOptions)` | a PDF from CommonMark with GitHub tables, task lists, strikethrough and footnotes |
| `markdown_file_to_pdf(path, &PdfOptions)` | as `html_file_to_pdf` |
| `pdf_to_images(pdf, &ImageOptions)` | encoded images, one per page: `dpi`, `format` (PNG, JPEG, WebP), `pages`, `width`, `height`, `transparent`, `quality`, `password` |
| `pdf_to_text(pdf, &TextOptions)` | the text in reading order, pages separated by `\f`: `pages`, `password`, `order` (`TextOrder::Reading` or `TopToBottom`) |
| `pdf_to_text_pages(pdf, &TextOptions)` | the text of each page |
| `pdf_to_markdown(pdf, &MarkdownOptions)` | GitHub Flavored Markdown of any PDF, for search indexes and language models: `pages`, `include_links`, `include_annotations`, `include_images`, `page_comments`, `max_pages`, `password` |
| `pdf_page_count(pdf, &PageCountOptions)` | the number of pages: `password` |
| `merge_pdfs(&[pdfs], &MergeOptions)` | one PDF from several: `title`, `author`, `bookmarks`, `creation_date` |
| `pdf_to_pdfa(pdf, &PdfAOptions)` | any PDF converted to PDF/A-2b (default), -2u, -3b or -3u, as `PdfAResult { pdf, changes, problems }` |
| `redact_pdf(pdf, &RedactOptions)` | the PDF with text, images and drawings removed for good: `text`, `pattern`, `area`, `ignore_case`, `fill_color` |
| `read_barcodes(data, &BarcodeOptions)` | the QR, Data Matrix, Code 128 and EAN-13 codes in an image or a PDF, as `Barcode { format, text, data, page, x, y, width, height, corners, .. }`: `formats`, `try_inverted`, `pages`, `password`, `dpi` |
| `library_version()`, `library_path()` | the PdfPecker version inside, and the library file loaded |

`pages` takes a page number (`3`), an array or `Vec` (`[1, 3]`), a range (`2..=4`) or a string such as `"1-3,5"` or
`"last"`, numbered from 1. Every call that reads or writes a document also takes a `timeout`.

Options of the HTML and Markdown functions (`PdfOptions`):

| Option | Meaning |
|---|---|
| `base_dir` | The folder relative image, stylesheet and font paths resolve against. Without it, a document from a string loads no local files. |
| `allow_file_access` | Let the document read files outside `base_dir` too. Only for HTML you trust. |
| `page_size`, `landscape` | `PageSize::A3`, `A4` (default), `A5`, `Letter`, `Legal` or `Millimeters { width, height }`. A CSS `@page { size }` rule wins. |
| `margin` | `Margin::all(points)` or `Margin::new(top, right, bottom, left)`, 72 points per inch. Default 36. CSS `@page { margin }` wins. |
| `scale`, `shrink_to_fit` | Zoom the content; shrink content wider than the page (on by default). |
| `header_text`, `footer_text` | A line of text in the top or bottom margin; `{page}` and `{pages}` are replaced. CSS margin boxes (`@top-center`, ...) do more. |
| `font`, `use_system_fonts` | A TrueType/OpenType file or folder that CSS `font-family` can name (call once per file or folder); or look families up among the installed fonts. Without fonts, text uses the standard PDF fonts, which cover Western European characters only. |
| `title`, `author`, `subject`, `keywords`, `language` | Document properties. |
| `user_style_sheet` | CSS applied after the document's own. |
| `compress`, `bookmarks` | Compress the streams; make bookmarks from the headings (both on by default). |
| `max_pages` | Fail with `ErrorKind::Limit` beyond this many pages (default 100,000). |
| `creation_date` | A fixed `SystemTime`: the output is then byte-identical from run to run. |
| `conformance` | `Conformance::PdfA2b`, `PdfA3b`, `PdfA2u`, `PdfA3u`, `PdfA2a`, `PdfA3a`, `PdfA4` or `PdfA4f`. Level A always writes a tagged structure tree and a language. |
| `accessibility` | `Accessibility::Tagged`, `PdfUa1` or `PdfUa2`. |

Errors carry an `ErrorKind`: `InvalidArgument` (a bad option value, a page past the end), `Unreadable` (not a readable
PDF, image or font), `Password`, `Limit`, `Timeout`, `Failed`, `Io` (a font or image file that could not be read),
`Library` (the native library is missing or does not fit) and `Internal`.

Every function is safe to call from several threads at once. The calls block, so async code runs them with
`tokio::task::spawn_blocking` or its runtime's equivalent.

## Building from this repository

The crate uses the native libraries in the [Python package](https://github.com/Moonwalker1307/PdfPecker.Python)'s
wheels on PyPI, which are built from the released PdfPecker NuGet package; nothing is compiled from .NET here.

```
cargo test                               # downloads this machine's library on the first build
ci/test.sh                               # also packages the crate and tests it as a dependency
python3 scripts/update-wheels.py         # after changing the version in Cargo.toml: writes wheels.txt
```

## Releasing

After a PdfPecker release reaches PyPI (the Python package follows each NuGet release), a pull request here changes
`version` in `Cargo.toml`, runs `scripts/update-wheels.py` and adds functions or options for what the release added.
The owner then publishes a GitHub release tagged `v<version>`; the CI workflow tests every platform and publishes the
crate to crates.io.

## License

MIT. The native library also contains the .NET runtime, zlib-ng and the fonts and data PdfPecker bundles, each under
its own license: see `THIRD-PARTY-NOTICES.md`.
