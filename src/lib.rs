//! HTML and Markdown to PDF, and PDF to images, text and Markdown, with no browser.
//!
//! This crate runs [PdfPecker](https://github.com/Moonwalker1307/PdfPecker), a .NET library with its own HTML parser,
//! CSS engine, layout engine, PDF writer and PDF renderer, compiled ahead of time into a native library: the same
//! binary the `pdfpecker` packages on PyPI and npm ship. It runs no JavaScript and fetches nothing from the network.
//!
//! ```no_run
//! let pdf = pdfpecker::html_to_pdf("<h1>Invoice 42</h1><p>Total: <b>€ 1,250.00</b></p>", &Default::default())?;
//! std::fs::write("invoice.pdf", &pdf)?;
//!
//! let pngs = pdfpecker::pdf_to_images(&pdf, &pdfpecker::ImageOptions::new().dpi(96.0))?;
//! println!("{} page(s), {}", pngs.len(), pdfpecker::pdf_to_text(&pdf, &Default::default())?);
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```
//!
//! # The native library
//!
//! The build script downloads PdfPecker's native library for the target from PyPI (about 9 MB, once; it is cached),
//! checks it against the SHA-256 values the crate carries, and puts it in Cargo's build folder, where `cargo run` and
//! `cargo test` find it. To deploy a program, copy `pdfpecker_native.so` (`.dylib` on macOS, `.dll` on Windows) next
//! to its executable, or point `PDFPECKER_NATIVE_LIB` at it; [`library_path`] tells you which file was loaded.
//! Offline builds set `PDFPECKER_LIB_DIR` to a folder that holds the library.
//!
//! # Threads
//!
//! Every function is safe to call from several threads at once, and conversions run in parallel. They block the
//! calling thread, so async code runs them with `tokio::task::spawn_blocking` or its runtime's equivalent.

mod error;
mod json;
mod native;
mod options;

pub use error::{Error, ErrorKind, Result};
pub use options::{
    Accessibility, Conformance, ImageFormat, ImageOptions, Margin, MarkdownOptions, MergeOptions,
    PageCountOptions, PageSize, Pages, PdfAOptions, PdfOptions, TextOptions,
};

use std::path::{Path, PathBuf};

/// The version of the PdfPecker library inside the native library, which is also this crate's version.
pub fn library_version() -> Result<String> {
    native::library_version()
}

/// The native library file that was loaded, which is the file to ship with a program.
pub fn library_path() -> Result<PathBuf> {
    native::library_path()
}

/// Converts HTML to a PDF and returns its bytes.
///
/// CSS, including `@page` rules for size, margins, headers and footers, comes from `<style>` and
/// `<link rel="stylesheet">`. Relative images, stylesheets and fonts load only from [`PdfOptions::base_dir`].
pub fn html_to_pdf(html: &str, options: &PdfOptions) -> Result<Vec<u8>> {
    native::invoke("html_to_pdf", html.as_bytes(), &options.to_json()?)
}

/// Converts an HTML file to a PDF. Relative images, stylesheets and fonts resolve next to the file unless
/// [`PdfOptions::base_dir`] says otherwise.
pub fn html_file_to_pdf(path: impl AsRef<Path>, options: &PdfOptions) -> Result<Vec<u8>> {
    let (text, options) = read_document(path.as_ref(), options)?;
    html_to_pdf(&text, &options)
}

/// Converts Markdown (CommonMark with GitHub's tables, task lists, strikethrough and autolinks) to a PDF.
pub fn markdown_to_pdf(markdown: &str, options: &PdfOptions) -> Result<Vec<u8>> {
    native::invoke("markdown_to_pdf", markdown.as_bytes(), &options.to_json()?)
}

/// Converts a Markdown file to a PDF. Relative images resolve next to the file unless [`PdfOptions::base_dir`] says
/// otherwise.
pub fn markdown_file_to_pdf(path: impl AsRef<Path>, options: &PdfOptions) -> Result<Vec<u8>> {
    let (text, options) = read_document(path.as_ref(), options)?;
    markdown_to_pdf(&text, &options)
}

/// Renders pages of a PDF, any PDF and not only one PdfPecker made, to PNG, JPEG or WebP images, one per page.
pub fn pdf_to_images(pdf: &[u8], options: &ImageOptions) -> Result<Vec<Vec<u8>>> {
    native::unframe(&native::invoke("pdf_to_images", pdf, &options.to_json()?)?)
}

/// The number of pages in a PDF.
pub fn pdf_page_count(pdf: &[u8], options: &PageCountOptions) -> Result<usize> {
    let raw = native::invoke("pdf_page_count", pdf, &options.to_json()?)?;
    std::str::from_utf8(&raw)
        .ok()
        .and_then(|s| s.trim().parse().ok())
        .ok_or_else(|| {
            Error::new(
                ErrorKind::Internal,
                "the native library returned no page count",
            )
        })
}

/// Extracts the text of each page, in reading order, as one string per page.
pub fn pdf_to_text_pages(pdf: &[u8], options: &TextOptions) -> Result<Vec<String>> {
    native::unframe(&native::invoke("pdf_to_text", pdf, &options.to_json()?)?)?
        .into_iter()
        .map(|page| {
            String::from_utf8(page)
                .map_err(|_| Error::new(ErrorKind::Internal, "text is not UTF-8"))
        })
        .collect()
}

/// Extracts the text of a PDF in reading order, pages separated by a form feed (`\f`).
pub fn pdf_to_text(pdf: &[u8], options: &TextOptions) -> Result<String> {
    Ok(pdf_to_text_pages(pdf, options)?.join("\u{c}"))
}

/// Converts a PDF, any PDF and not only one PdfPecker made, to GitHub Flavored Markdown: headings, paragraphs, lists
/// and tables in reading order, for search indexes and language models.
///
/// A tagged PDF is read from its structure tree, other PDFs from their layout. Drawings and the text of scanned pages
/// are not part of the output.
pub fn pdf_to_markdown(pdf: &[u8], options: &MarkdownOptions) -> Result<String> {
    String::from_utf8(native::invoke("pdf_to_markdown", pdf, &options.to_json()?)?)
        .map_err(|_| Error::new(ErrorKind::Internal, "Markdown is not UTF-8"))
}

/// The outcome of [`pdf_to_pdfa`]: the converted PDF, what the conversion changed and what still breaks the rules.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct PdfAResult {
    /// The converted PDF.
    pub pdf: Vec<u8>,
    /// What the conversion changed, one line each.
    pub changes: Vec<String>,
    /// What could not be fixed, one line each.
    pub problems: Vec<String>,
}

impl PdfAResult {
    /// True when the conversion found nothing left that breaks the PDF/A rules it checks; veraPDF has the final word.
    pub fn is_conformant(&self) -> bool {
        self.problems.is_empty()
    }
}

/// Converts an existing PDF, any PDF and not only one PdfPecker made, to PDF/A for long-term archiving.
///
/// Fonts that are not embedded are embedded, colours made device independent, and JavaScript and other things PDF/A
/// forbids removed; text stays text. Encrypted and signed PDFs return [`ErrorKind::InvalidArgument`].
pub fn pdf_to_pdfa(pdf: &[u8], options: &PdfAOptions) -> Result<PdfAResult> {
    let mut parts =
        native::unframe(&native::invoke("pdf_to_pdfa", pdf, &options.to_json()?)?)?.into_iter();
    let (Some(pdf), Some(report), None) = (parts.next(), parts.next(), parts.next()) else {
        return Err(Error::new(
            ErrorKind::Internal,
            "the native library returned a malformed PDF/A result",
        ));
    };
    let report = String::from_utf8(report)
        .map_err(|_| Error::new(ErrorKind::Internal, "the report is not UTF-8"))?;
    Ok(PdfAResult {
        pdf,
        changes: json::string_list(&report, "changes")?,
        problems: json::string_list(&report, "problems")?,
    })
}

/// Merges PDFs into one, keeping links, bookmarks and form fields.
pub fn merge_pdfs<P: AsRef<[u8]>>(pdfs: &[P], options: &MergeOptions) -> Result<Vec<u8>> {
    native::invoke("merge_pdfs", &native::frame(pdfs)?, &options.to_json()?)
}

fn read_document(path: &Path, options: &PdfOptions) -> Result<(String, PdfOptions)> {
    let text = std::fs::read_to_string(path)
        .map_err(|e| Error::new(ErrorKind::Io, format!("{}: {e}", path.display())))?;
    let mut options = options.clone();
    if !options.has_base_dir() {
        let absolute = std::path::absolute(path)
            .map_err(|e| Error::new(ErrorKind::Io, format!("{}: {e}", path.display())))?;
        if let Some(dir) = absolute.parent() {
            options = options.base_dir(dir);
        }
    }
    Ok((text, options))
}
