//! Options for each call. Each is built with chained methods from its `Default`, so options can be added in later
//! releases without breaking code that uses them:
//!
//! ```
//! use pdfpecker::{Margin, PageSize, PdfOptions};
//! let options = PdfOptions::new().page_size(PageSize::Letter).margin(Margin::all(54.0)).title("Invoice 42");
//! ```

use crate::error::{Error, Result};
use crate::json::Object;
use std::ops::RangeInclusive;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// The paper size of the pages. A CSS `@page { size: ... }` rule in the document wins over it.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
#[non_exhaustive]
pub enum PageSize {
    /// 297 × 420 mm.
    A3,
    /// 210 × 297 mm, the default.
    #[default]
    A4,
    /// 148 × 210 mm.
    A5,
    /// 8.5 × 11 inches.
    Letter,
    /// 8.5 × 14 inches.
    Legal,
    /// Any size, in millimetres.
    Millimeters {
        /// The page width in millimetres.
        width: f32,
        /// The page height in millimetres.
        height: f32,
    },
}

/// Page margins in points (72 per inch). A CSS `@page { margin: ... }` rule in the document wins over them.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Margin {
    /// The top margin in points.
    pub top: f32,
    /// The right margin in points.
    pub right: f32,
    /// The bottom margin in points.
    pub bottom: f32,
    /// The left margin in points.
    pub left: f32,
}

impl Margin {
    /// The same margin on all four sides, in points.
    pub fn all(points: f32) -> Self {
        Self {
            top: points,
            right: points,
            bottom: points,
            left: points,
        }
    }

    /// Four margins in points, in CSS order: top, right, bottom, left.
    pub fn new(top: f32, right: f32, bottom: f32, left: f32) -> Self {
        Self {
            top,
            right,
            bottom,
            left,
        }
    }
}

impl Default for Margin {
    /// Half an inch (36 points) on every side, PdfPecker's default.
    fn default() -> Self {
        Self::all(36.0)
    }
}

/// A PDF/A level for archiving.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Conformance {
    /// PDF/A-2b: the pages look the same in the future.
    PdfA2b,
    /// PDF/A-2u: PDF/A-2b, and all text can be extracted as Unicode.
    PdfA2u,
    /// PDF/A-3b: PDF/A-2b that may carry any attached file.
    PdfA3b,
    /// PDF/A-3u: PDF/A-3b, and all text can be extracted as Unicode.
    PdfA3u,
    /// PDF/A-4, on PDF 2.0. Only HTML and Markdown conversion write it.
    PdfA4,
    /// PDF/A-4f, which allows attachments. Only HTML and Markdown conversion write it.
    PdfA4f,
}

impl Conformance {
    fn name(self) -> &'static str {
        match self {
            Self::PdfA2b => "PDF/A-2b",
            Self::PdfA2u => "PDF/A-2u",
            Self::PdfA3b => "PDF/A-3b",
            Self::PdfA3u => "PDF/A-3u",
            Self::PdfA4 => "PDF/A-4",
            Self::PdfA4f => "PDF/A-4f",
        }
    }
}

/// How much structure the PDF carries for screen readers and text extraction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Accessibility {
    /// A tagged PDF: headings, paragraphs, lists, tables and figures in reading order.
    Tagged,
    /// PDF/UA-1, the accessibility standard, which also needs a title and a language.
    PdfUa1,
    /// PDF/UA-2, on PDF 2.0.
    PdfUa2,
}

impl Accessibility {
    fn name(self) -> &'static str {
        match self {
            Self::Tagged => "tagged",
            Self::PdfUa1 => "PDF/UA-1",
            Self::PdfUa2 => "PDF/UA-2",
        }
    }
}

/// The format of rendered pages.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[non_exhaustive]
pub enum ImageFormat {
    /// PNG, the default: lossless.
    #[default]
    Png,
    /// JPEG: small, for photos; see [`ImageOptions::quality`].
    Jpeg,
    /// WebP; see [`ImageOptions::quality`].
    WebP,
}

impl ImageFormat {
    fn name(self) -> &'static str {
        match self {
            Self::Png => "png",
            Self::Jpeg => "jpeg",
            Self::WebP => "webp",
        }
    }
}

/// The order in which [`pdf_to_text`](crate::pdf_to_text) reads a page's text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[non_exhaustive]
pub enum TextOrder {
    /// Reading order, the default: columns one after the other, and tables row by row.
    #[default]
    Reading,
    /// Lines from the top of the page to the bottom, left to right, as the text stands on the page.
    TopToBottom,
}

impl TextOrder {
    fn name(self) -> &'static str {
        match self {
            Self::Reading => "reading",
            Self::TopToBottom => "top_to_bottom",
        }
    }
}

/// A barcode symbology that [`read_barcodes`](crate::read_barcodes) can find.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum BarcodeFormat {
    /// QR code.
    QrCode,
    /// Data Matrix.
    DataMatrix,
    /// Code 128, GS1-128 included.
    Code128,
    /// EAN-13.
    Ean13,
}

impl BarcodeFormat {
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::QrCode => "qr_code",
            Self::DataMatrix => "data_matrix",
            Self::Code128 => "code128",
            Self::Ean13 => "ean13",
        }
    }

    pub(crate) fn from_name(name: &str) -> Option<Self> {
        match name {
            "qr_code" => Some(Self::QrCode),
            "data_matrix" => Some(Self::DataMatrix),
            "code128" => Some(Self::Code128),
            "ean13" => Some(Self::Ean13),
            _ => None,
        }
    }
}

/// Which pages a call reads, numbered from 1.
///
/// Made from a page number (`3`), a list (`[1, 3]` or `vec![1, 3]`), a range (`2..=4`) or a string in PdfPecker's
/// syntax (`"1-3,5"`, `"last"`, `"2-last"`). A page past the end is an [`ErrorKind::InvalidArgument`](crate::ErrorKind::InvalidArgument).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Pages(String);

impl From<u32> for Pages {
    fn from(page: u32) -> Self {
        Self(page.to_string())
    }
}

impl From<&str> for Pages {
    fn from(range: &str) -> Self {
        Self(range.to_string())
    }
}

impl From<String> for Pages {
    fn from(range: String) -> Self {
        Self(range)
    }
}

impl From<&[u32]> for Pages {
    fn from(pages: &[u32]) -> Self {
        Self(
            pages
                .iter()
                .map(u32::to_string)
                .collect::<Vec<_>>()
                .join(","),
        )
    }
}

impl<const N: usize> From<[u32; N]> for Pages {
    fn from(pages: [u32; N]) -> Self {
        Self::from(&pages[..])
    }
}

impl From<Vec<u32>> for Pages {
    fn from(pages: Vec<u32>) -> Self {
        Self::from(&pages[..])
    }
}

impl From<RangeInclusive<u32>> for Pages {
    fn from(range: RangeInclusive<u32>) -> Self {
        Self(format!("{}-{}", range.start(), range.end()))
    }
}

fn absolute(path: &Path, what: &str) -> Result<String> {
    let absolute = std::path::absolute(path)
        .map_err(|e| Error::argument(format!("{what}: {}: {e}", path.display())))?;
    absolute
        .into_os_string()
        .into_string()
        .map_err(|p| Error::argument(format!("{what} is not valid Unicode: {p:?}")))
}

fn unix_seconds(time: SystemTime) -> i64 {
    match time.duration_since(UNIX_EPOCH) {
        Ok(d) => d.as_secs() as i64,
        Err(e) => -(e.duration().as_secs_f64().ceil() as i64),
    }
}

// The native timer takes at most 2^31 - 1 milliseconds (about 24.8 days); the 1.23.0 library reports a longer
// timeout as an internal error, so it is checked here.
fn timeout(o: &mut Object, timeout: Option<Duration>) -> Result<()> {
    if let Some(t) = timeout {
        if t.is_zero() || t.as_secs_f64() > 2_147_483.0 {
            return Err(Error::argument(format!(
                "timeout must be above 0 and at most 2,147,483 seconds, not {t:?}"
            )));
        }
        o.num("timeout", t.as_secs_f64())?;
    }
    Ok(())
}

/// Options of HTML and Markdown conversion.
#[derive(Debug, Clone, Default)]
pub struct PdfOptions {
    base_dir: Option<PathBuf>,
    allow_file_access: Option<bool>,
    page_size: Option<PageSize>,
    landscape: Option<bool>,
    margin: Option<Margin>,
    scale: Option<f32>,
    shrink_to_fit: Option<bool>,
    title: Option<String>,
    author: Option<String>,
    subject: Option<String>,
    keywords: Option<String>,
    language: Option<String>,
    header_text: Option<String>,
    footer_text: Option<String>,
    user_style_sheet: Option<String>,
    fonts: Vec<PathBuf>,
    use_system_fonts: Option<bool>,
    compress: Option<bool>,
    bookmarks: Option<bool>,
    max_pages: Option<u32>,
    creation_date: Option<SystemTime>,
    conformance: Option<Conformance>,
    accessibility: Option<Accessibility>,
    timeout: Option<Duration>,
}

impl PdfOptions {
    /// The defaults: A4, half-inch margins, compressed, bookmarks from headings, no local files.
    pub fn new() -> Self {
        Self::default()
    }

    /// The folder that relative image, stylesheet and font paths resolve against. Without it, a document from a
    /// string loads no local files. Files outside it load only with [`allow_file_access`](Self::allow_file_access).
    pub fn base_dir(mut self, dir: impl Into<PathBuf>) -> Self {
        self.base_dir = Some(dir.into());
        self
    }

    /// Lets the document load local files outside [`base_dir`](Self::base_dir), by absolute path or `../`. Only for
    /// documents you trust.
    pub fn allow_file_access(mut self, allow: bool) -> Self {
        self.allow_file_access = Some(allow);
        self
    }

    /// The paper size (default A4). A CSS `@page` size wins over it.
    pub fn page_size(mut self, size: PageSize) -> Self {
        self.page_size = Some(size);
        self
    }

    /// Turns the page size sideways.
    pub fn landscape(mut self, landscape: bool) -> Self {
        self.landscape = Some(landscape);
        self
    }

    /// The page margins in points (default 36, half an inch). A CSS `@page` margin wins over them.
    pub fn margin(mut self, margin: Margin) -> Self {
        self.margin = Some(margin);
        self
    }

    /// Zooms the content, such as 0.8 for 80%.
    pub fn scale(mut self, scale: f32) -> Self {
        self.scale = Some(scale);
        self
    }

    /// Shrinks content wider than the page to fit it (on by default).
    pub fn shrink_to_fit(mut self, shrink: bool) -> Self {
        self.shrink_to_fit = Some(shrink);
        self
    }

    /// The document title, which PDF viewers show in their title bar.
    pub fn title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }

    /// The document author.
    pub fn author(mut self, author: impl Into<String>) -> Self {
        self.author = Some(author.into());
        self
    }

    /// The document subject.
    pub fn subject(mut self, subject: impl Into<String>) -> Self {
        self.subject = Some(subject.into());
        self
    }

    /// The document keywords.
    pub fn keywords(mut self, keywords: impl Into<String>) -> Self {
        self.keywords = Some(keywords.into());
        self
    }

    /// The document language, such as `"en-GB"`; it also picks the hyphenation patterns.
    pub fn language(mut self, language: impl Into<String>) -> Self {
        self.language = Some(language.into());
        self
    }

    /// A line of text at the top of every page; `{page}` and `{pages}` are replaced, as in `"Page {page} of {pages}"`.
    pub fn header_text(mut self, text: impl Into<String>) -> Self {
        self.header_text = Some(text.into());
        self
    }

    /// A line of text at the bottom of every page; `{page}` and `{pages}` are replaced.
    pub fn footer_text(mut self, text: impl Into<String>) -> Self {
        self.footer_text = Some(text.into());
        self
    }

    /// CSS applied after the document's own.
    pub fn user_style_sheet(mut self, css: impl Into<String>) -> Self {
        self.user_style_sheet = Some(css.into());
        self
    }

    /// A TrueType or OpenType file, or a folder of them, that CSS `font-family` can name. Without fonts, text uses the
    /// standard PDF fonts, which cover Western European characters only; add fonts for other scripts. Call it once
    /// for each file or folder.
    pub fn font(mut self, path: impl Into<PathBuf>) -> Self {
        self.fonts.push(path.into());
        self
    }

    /// Also looks up font families among the fonts installed on this machine.
    pub fn use_system_fonts(mut self, use_them: bool) -> Self {
        self.use_system_fonts = Some(use_them);
        self
    }

    /// Compresses the PDF's streams (on by default).
    pub fn compress(mut self, compress: bool) -> Self {
        self.compress = Some(compress);
        self
    }

    /// Makes bookmarks from the headings (on by default).
    pub fn bookmarks(mut self, bookmarks: bool) -> Self {
        self.bookmarks = Some(bookmarks);
        self
    }

    /// Stops with [`ErrorKind::Limit`](crate::ErrorKind::Limit) beyond this many pages (default 100,000).
    pub fn max_pages(mut self, pages: u32) -> Self {
        self.max_pages = Some(pages);
        self
    }

    /// A fixed creation date, which makes the output byte-identical from run to run.
    pub fn creation_date(mut self, date: SystemTime) -> Self {
        self.creation_date = Some(date);
        self
    }

    /// Writes a PDF/A file for archiving.
    pub fn conformance(mut self, conformance: Conformance) -> Self {
        self.conformance = Some(conformance);
        self
    }

    /// Writes a tagged or PDF/UA file for screen readers.
    pub fn accessibility(mut self, accessibility: Accessibility) -> Self {
        self.accessibility = Some(accessibility);
        self
    }

    /// Stops with [`ErrorKind::Timeout`](crate::ErrorKind::Timeout) after this long.
    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = Some(timeout);
        self
    }

    pub(crate) fn has_base_dir(&self) -> bool {
        self.base_dir.is_some()
    }

    pub(crate) fn to_json(&self) -> Result<String> {
        let mut o = Object::default();
        if let Some(dir) = &self.base_dir {
            o.str("base_dir", &absolute(dir, "base_dir")?);
        }
        if let Some(v) = self.allow_file_access {
            o.bool("allow_file_access", v);
        }
        match self.page_size {
            Some(PageSize::Millimeters { width, height }) => {
                o.nums("page_size", &[width as f64, height as f64])?
            }
            Some(size) => o.str(
                "page_size",
                match size {
                    PageSize::A3 => "A3",
                    PageSize::A5 => "A5",
                    PageSize::Letter => "Letter",
                    PageSize::Legal => "Legal",
                    _ => "A4",
                },
            ),
            None => {}
        }
        if let Some(v) = self.landscape {
            o.bool("landscape", v);
        }
        if let Some(m) = self.margin {
            o.nums(
                "margin",
                &[m.top as f64, m.right as f64, m.bottom as f64, m.left as f64],
            )?;
        }
        if let Some(v) = self.scale {
            o.num("scale", v as f64)?;
        }
        if let Some(v) = self.shrink_to_fit {
            o.bool("shrink_to_fit", v);
        }
        for (name, value) in [
            ("title", &self.title),
            ("author", &self.author),
            ("subject", &self.subject),
            ("keywords", &self.keywords),
            ("language", &self.language),
            ("header_text", &self.header_text),
            ("footer_text", &self.footer_text),
            ("user_style_sheet", &self.user_style_sheet),
        ] {
            if let Some(v) = value {
                o.str(name, v);
            }
        }
        if !self.fonts.is_empty() {
            let fonts = self
                .fonts
                .iter()
                .map(|f| absolute(f, "font"))
                .collect::<Result<Vec<_>>>()?;
            o.strs("fonts", &fonts);
        }
        if let Some(v) = self.use_system_fonts {
            o.bool("use_system_fonts", v);
        }
        if let Some(v) = self.compress {
            o.bool("compress", v);
        }
        if let Some(v) = self.bookmarks {
            o.bool("bookmarks", v);
        }
        if let Some(v) = self.max_pages {
            o.int("max_pages", v.min(i32::MAX as u32) as i64);
        }
        if let Some(v) = self.creation_date {
            o.int("creation_date", unix_seconds(v));
        }
        if let Some(v) = self.conformance {
            o.str("conformance", v.name());
        }
        if let Some(v) = self.accessibility {
            o.str("accessibility", v.name());
        }
        timeout(&mut o, self.timeout)?;
        Ok(o.finish())
    }
}

/// Options of [`pdf_to_images`](crate::pdf_to_images).
#[derive(Debug, Clone, Default)]
pub struct ImageOptions {
    dpi: Option<f32>,
    format: Option<ImageFormat>,
    pages: Option<Pages>,
    width: Option<u32>,
    height: Option<u32>,
    transparent: Option<bool>,
    quality: Option<u8>,
    password: Option<String>,
    timeout: Option<Duration>,
}

impl ImageOptions {
    /// The defaults: every page as a PNG at 150 dpi.
    pub fn new() -> Self {
        Self::default()
    }

    /// The resolution in dots per inch (default 150).
    pub fn dpi(mut self, dpi: f32) -> Self {
        self.dpi = Some(dpi);
        self
    }

    /// PNG (default), JPEG or WebP.
    pub fn format(mut self, format: ImageFormat) -> Self {
        self.format = Some(format);
        self
    }

    /// Which pages to render (default all).
    pub fn pages(mut self, pages: impl Into<Pages>) -> Self {
        self.pages = Some(pages.into());
        self
    }

    /// A fixed width in pixels instead of a resolution, keeping the aspect ratio.
    pub fn width(mut self, pixels: u32) -> Self {
        self.width = Some(pixels);
        self
    }

    /// A fixed height in pixels instead of a resolution, keeping the aspect ratio.
    pub fn height(mut self, pixels: u32) -> Self {
        self.height = Some(pixels);
        self
    }

    /// Leaves the page background transparent instead of white (PNG and WebP).
    pub fn transparent(mut self, transparent: bool) -> Self {
        self.transparent = Some(transparent);
        self
    }

    /// The quality of JPEG and WebP images, 1 to 100.
    pub fn quality(mut self, quality: u8) -> Self {
        self.quality = Some(quality);
        self
    }

    /// The password of an encrypted PDF.
    pub fn password(mut self, password: impl Into<String>) -> Self {
        self.password = Some(password.into());
        self
    }

    /// Stops with [`ErrorKind::Timeout`](crate::ErrorKind::Timeout) after this long.
    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = Some(timeout);
        self
    }

    pub(crate) fn to_json(&self) -> Result<String> {
        let mut o = Object::default();
        if let Some(v) = self.dpi {
            o.num("dpi", v as f64)?;
        }
        if let Some(v) = self.format {
            o.str("format", v.name());
        }
        if let Some(v) = &self.pages {
            o.str("pages", &v.0);
        }
        if let Some(v) = self.width {
            o.int("width", v.min(i32::MAX as u32) as i64);
        }
        if let Some(v) = self.height {
            o.int("height", v.min(i32::MAX as u32) as i64);
        }
        if let Some(v) = self.transparent {
            o.bool("transparent", v);
        }
        if let Some(v) = self.quality {
            o.int("quality", v as i64);
        }
        if let Some(v) = &self.password {
            o.str("password", v);
        }
        timeout(&mut o, self.timeout)?;
        Ok(o.finish())
    }
}

/// Options of [`pdf_to_text`](crate::pdf_to_text) and [`pdf_to_text_pages`](crate::pdf_to_text_pages).
#[derive(Debug, Clone, Default)]
pub struct TextOptions {
    pages: Option<Pages>,
    password: Option<String>,
    order: Option<TextOrder>,
    timeout: Option<Duration>,
}

impl TextOptions {
    /// The defaults: every page.
    pub fn new() -> Self {
        Self::default()
    }

    /// Which pages to read (default all).
    pub fn pages(mut self, pages: impl Into<Pages>) -> Self {
        self.pages = Some(pages.into());
        self
    }

    /// The password of an encrypted PDF.
    pub fn password(mut self, password: impl Into<String>) -> Self {
        self.password = Some(password.into());
        self
    }

    /// The order of the text on each page: [`TextOrder::Reading`] (default) or [`TextOrder::TopToBottom`].
    pub fn order(mut self, order: TextOrder) -> Self {
        self.order = Some(order);
        self
    }

    /// Stops with [`ErrorKind::Timeout`](crate::ErrorKind::Timeout) after this long.
    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = Some(timeout);
        self
    }

    pub(crate) fn to_json(&self) -> Result<String> {
        let mut o = Object::default();
        if let Some(v) = &self.pages {
            o.str("pages", &v.0);
        }
        if let Some(v) = &self.password {
            o.str("password", v);
        }
        if let Some(v) = self.order {
            o.str("order", v.name());
        }
        timeout(&mut o, self.timeout)?;
        Ok(o.finish())
    }
}

/// Options of [`pdf_to_markdown`](crate::pdf_to_markdown).
#[derive(Debug, Clone, Default)]
pub struct MarkdownOptions {
    pages: Option<Pages>,
    include_links: Option<bool>,
    include_annotations: Option<bool>,
    include_images: Option<bool>,
    page_comments: Option<bool>,
    max_pages: Option<u32>,
    password: Option<String>,
    timeout: Option<Duration>,
}

impl MarkdownOptions {
    /// The defaults: every page, with links and annotation text, without images.
    pub fn new() -> Self {
        Self::default()
    }

    /// Which pages to read (default all).
    pub fn pages(mut self, pages: impl Into<Pages>) -> Self {
        self.pages = Some(pages.into());
        self
    }

    /// Text that links to a web or email address becomes a Markdown link (on by default).
    pub fn include_links(mut self, include: bool) -> Self {
        self.include_links = Some(include);
        self
    }

    /// Includes the text of annotations, such as form field values (on by default).
    pub fn include_annotations(mut self, include: bool) -> Self {
        self.include_annotations = Some(include);
        self
    }

    /// Includes pictures as `data:` URI images where they sit in the text (off by default: they are long).
    pub fn include_images(mut self, include: bool) -> Self {
        self.include_images = Some(include);
        self
    }

    /// Starts each page with an HTML comment such as `<!-- page 3 -->`, so a passage can be traced to its page.
    pub fn page_comments(mut self, comments: bool) -> Self {
        self.page_comments = Some(comments);
        self
    }

    /// Stops with [`ErrorKind::Limit`](crate::ErrorKind::Limit) beyond this many pages (default 10,000).
    pub fn max_pages(mut self, pages: u32) -> Self {
        self.max_pages = Some(pages);
        self
    }

    /// The password of an encrypted PDF.
    pub fn password(mut self, password: impl Into<String>) -> Self {
        self.password = Some(password.into());
        self
    }

    /// Stops with [`ErrorKind::Timeout`](crate::ErrorKind::Timeout) after this long.
    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = Some(timeout);
        self
    }

    pub(crate) fn to_json(&self) -> Result<String> {
        let mut o = Object::default();
        if let Some(v) = &self.pages {
            o.str("pages", &v.0);
        }
        for (name, value) in [
            ("include_links", self.include_links),
            ("include_annotations", self.include_annotations),
            ("include_images", self.include_images),
            ("page_comments", self.page_comments),
        ] {
            if let Some(v) = value {
                o.bool(name, v);
            }
        }
        if let Some(v) = self.max_pages {
            o.int("max_pages", v.min(i32::MAX as u32) as i64);
        }
        if let Some(v) = &self.password {
            o.str("password", v);
        }
        timeout(&mut o, self.timeout)?;
        Ok(o.finish())
    }
}

/// Options of [`pdf_page_count`](crate::pdf_page_count).
#[derive(Debug, Clone, Default)]
pub struct PageCountOptions {
    password: Option<String>,
}

impl PageCountOptions {
    /// The defaults: no password.
    pub fn new() -> Self {
        Self::default()
    }

    /// The password of an encrypted PDF.
    pub fn password(mut self, password: impl Into<String>) -> Self {
        self.password = Some(password.into());
        self
    }

    pub(crate) fn to_json(&self) -> Result<String> {
        let mut o = Object::default();
        if let Some(v) = &self.password {
            o.str("password", v);
        }
        Ok(o.finish())
    }
}

/// Options of [`pdf_to_pdfa`](crate::pdf_to_pdfa).
#[derive(Debug, Clone, Default)]
pub struct PdfAOptions {
    conformance: Option<Conformance>,
    timeout: Option<Duration>,
}

impl PdfAOptions {
    /// The defaults: PDF/A-2b.
    pub fn new() -> Self {
        Self::default()
    }

    /// PDF/A-2b (default), PDF/A-2u, PDF/A-3b or PDF/A-3u. PDF/A-2 keeps only attachments that are PDF/A files
    /// themselves; PDF/A-3 keeps every attachment.
    pub fn conformance(mut self, conformance: Conformance) -> Self {
        self.conformance = Some(conformance);
        self
    }

    /// Stops with [`ErrorKind::Timeout`](crate::ErrorKind::Timeout) after this long.
    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = Some(timeout);
        self
    }

    pub(crate) fn to_json(&self) -> Result<String> {
        let mut o = Object::default();
        if let Some(v) = self.conformance {
            o.str("conformance", v.name());
        }
        timeout(&mut o, self.timeout)?;
        Ok(o.finish())
    }
}

/// Options of [`merge_pdfs`](crate::merge_pdfs).
#[derive(Debug, Clone, Default)]
pub struct MergeOptions {
    title: Option<String>,
    author: Option<String>,
    bookmarks: Option<bool>,
    creation_date: Option<SystemTime>,
    timeout: Option<Duration>,
}

impl MergeOptions {
    /// The defaults: keeps each file's bookmarks, one after the other.
    pub fn new() -> Self {
        Self::default()
    }

    /// The merged document's title.
    pub fn title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }

    /// The merged document's author.
    pub fn author(mut self, author: impl Into<String>) -> Self {
        self.author = Some(author.into());
        self
    }

    /// Keeps each file's bookmarks, one after the other (on by default).
    pub fn bookmarks(mut self, bookmarks: bool) -> Self {
        self.bookmarks = Some(bookmarks);
        self
    }

    /// A fixed creation date, which makes the output byte-identical from run to run.
    pub fn creation_date(mut self, date: SystemTime) -> Self {
        self.creation_date = Some(date);
        self
    }

    /// Stops with [`ErrorKind::Timeout`](crate::ErrorKind::Timeout) after this long.
    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = Some(timeout);
        self
    }

    pub(crate) fn to_json(&self) -> Result<String> {
        let mut o = Object::default();
        if let Some(v) = &self.title {
            o.str("title", v);
        }
        if let Some(v) = &self.author {
            o.str("author", v);
        }
        if let Some(v) = self.bookmarks {
            o.bool("bookmarks", v);
        }
        if let Some(v) = self.creation_date {
            o.int("creation_date", unix_seconds(v));
        }
        timeout(&mut o, self.timeout)?;
        Ok(o.finish())
    }
}

/// What [`redact_pdf`](crate::redact_pdf) removes, and how the removed places look.
///
/// Text, a pattern and areas can be combined; each adds to what is removed.
#[derive(Debug, Clone, Default)]
pub struct RedactOptions {
    text: Option<String>,
    pattern: Option<String>,
    areas: Vec<[f64; 5]>,
    ignore_case: Option<bool>,
    fill_color: Option<String>,
    timeout: Option<Duration>,
}

impl RedactOptions {
    /// Nothing to remove yet, and black boxes over what is removed.
    pub fn new() -> Self {
        Self::default()
    }

    /// Removes every word that holds this text; a phrase matches across the words of a line.
    pub fn text(mut self, text: impl Into<String>) -> Self {
        self.text = Some(text.into());
        self
    }

    /// Removes every word that this regular expression matches, in .NET syntax, which agrees with Rust's `regex`
    /// crate for common patterns.
    pub fn pattern(mut self, pattern: impl Into<String>) -> Self {
        self.pattern = Some(pattern.into());
        self
    }

    /// Removes everything in a rectangle on a page (numbered from 1), in points from the page's top-left corner.
    pub fn area(mut self, page: u32, x: f64, y: f64, width: f64, height: f64) -> Self {
        self.areas.push([f64::from(page), x, y, width, height]);
        self
    }

    /// Compares [`text`](Self::text) and [`pattern`](Self::pattern) without regard to case (off by default).
    pub fn ignore_case(mut self, ignore_case: bool) -> Self {
        self.ignore_case = Some(ignore_case);
        self
    }

    /// The colour of the boxes drawn over what was removed, as a CSS colour (default black); `"transparent"` draws
    /// no box.
    pub fn fill_color(mut self, color: impl Into<String>) -> Self {
        self.fill_color = Some(color.into());
        self
    }

    /// Stops with [`ErrorKind::Timeout`](crate::ErrorKind::Timeout) after this long.
    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = Some(timeout);
        self
    }

    pub(crate) fn to_json(&self) -> Result<String> {
        let mut o = Object::default();
        if let Some(v) = &self.text {
            o.str("text", v);
        }
        if let Some(v) = &self.pattern {
            o.str("pattern", v);
        }
        if !self.areas.is_empty() {
            o.num_lists("areas", &self.areas)?;
        }
        if let Some(v) = self.ignore_case {
            o.bool("ignore_case", v);
        }
        if let Some(v) = &self.fill_color {
            o.str("fill_color", v);
        }
        timeout(&mut o, self.timeout)?;
        Ok(o.finish())
    }
}

/// Options of [`read_barcodes`](crate::read_barcodes).
#[derive(Debug, Clone, Default)]
pub struct BarcodeOptions {
    formats: Vec<BarcodeFormat>,
    try_inverted: Option<bool>,
    pages: Option<Pages>,
    password: Option<String>,
    dpi: Option<f64>,
    timeout: Option<Duration>,
}

impl BarcodeOptions {
    /// The defaults: every format, light-on-dark codes too, and for a PDF every page at 300 dpi.
    pub fn new() -> Self {
        Self::default()
    }

    /// Looks only for these formats (default all).
    pub fn formats(mut self, formats: impl IntoIterator<Item = BarcodeFormat>) -> Self {
        self.formats = formats.into_iter().collect();
        self
    }

    /// Also looks for light codes on a dark background (on by default).
    pub fn try_inverted(mut self, try_inverted: bool) -> Self {
        self.try_inverted = Some(try_inverted);
        self
    }

    /// For a PDF, which pages to search (default all).
    pub fn pages(mut self, pages: impl Into<Pages>) -> Self {
        self.pages = Some(pages.into());
        self
    }

    /// For a PDF, its password.
    pub fn password(mut self, password: impl Into<String>) -> Self {
        self.password = Some(password.into());
        self
    }

    /// For a PDF, the resolution pages are rendered at (default 300).
    pub fn dpi(mut self, dpi: f64) -> Self {
        self.dpi = Some(dpi);
        self
    }

    /// Stops with [`ErrorKind::Timeout`](crate::ErrorKind::Timeout) after this long.
    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = Some(timeout);
        self
    }

    pub(crate) fn to_json(&self) -> Result<String> {
        let mut o = Object::default();
        if !self.formats.is_empty() {
            let names: Vec<String> = self.formats.iter().map(|f| f.name().to_string()).collect();
            o.strs("formats", &names);
        }
        if let Some(v) = self.try_inverted {
            o.bool("try_inverted", v);
        }
        if let Some(v) = &self.pages {
            o.str("pages", &v.0);
        }
        if let Some(v) = &self.password {
            o.str("password", v);
        }
        if let Some(v) = self.dpi {
            o.num("dpi", v)?;
        }
        timeout(&mut o, self.timeout)?;
        Ok(o.finish())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn option_names_are_the_native_ones() {
        let json = PdfOptions::new()
            .page_size(PageSize::Millimeters {
                width: 100.0,
                height: 50.0,
            })
            .margin(Margin::new(1.0, 2.0, 3.0, 4.0))
            .footer_text("Page {page}")
            .creation_date(UNIX_EPOCH + Duration::from_secs(5))
            .conformance(Conformance::PdfA3u)
            .timeout(Duration::from_millis(1500))
            .to_json()
            .unwrap();
        assert_eq!(
            json,
            r#"{"page_size":[100,50],"margin":[1,2,3,4],"footer_text":"Page {page}","creation_date":5,"conformance":"PDF/A-3u","timeout":1.5}"#
        );
        assert_eq!(PdfOptions::new().to_json().unwrap(), "{}");
        assert_eq!(
            ImageOptions::new().pages([1, 3]).to_json().unwrap(),
            r#"{"pages":"1,3"}"#
        );
        assert_eq!(
            TextOptions::new().pages(2..=4).to_json().unwrap(),
            r#"{"pages":"2-4"}"#
        );
        assert_eq!(
            TextOptions::new()
                .order(TextOrder::TopToBottom)
                .to_json()
                .unwrap(),
            r#"{"order":"top_to_bottom"}"#
        );
        assert_eq!(
            RedactOptions::new()
                .text("secret")
                .area(2, 10.0, 20.5, 30.0, 40.0)
                .fill_color("transparent")
                .to_json()
                .unwrap(),
            r#"{"text":"secret","areas":[[2,10,20.5,30,40]],"fill_color":"transparent"}"#
        );
        assert_eq!(
            BarcodeOptions::new()
                .formats([BarcodeFormat::QrCode, BarcodeFormat::Ean13])
                .try_inverted(false)
                .to_json()
                .unwrap(),
            r#"{"formats":["qr_code","ean13"],"try_inverted":false}"#
        );
        assert_eq!(unix_seconds(UNIX_EPOCH - Duration::from_millis(1500)), -2);
    }
}
