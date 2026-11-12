//! Runs against the native library the build script fetched, or PDFPECKER_NATIVE_LIB.

use pdfpecker::*;
use std::time::{Duration, UNIX_EPOCH};

fn pdf(html: &str) -> Vec<u8> {
    html_to_pdf(html, &PdfOptions::new()).unwrap()
}

fn text(pdf: &[u8]) -> String {
    pdf_to_text(pdf, &TextOptions::new()).unwrap()
}

fn pages(pdf: &[u8]) -> usize {
    pdf_page_count(pdf, &PageCountOptions::new()).unwrap()
}

fn png_size(png: &[u8]) -> (u32, u32) {
    assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
    (
        u32::from_be_bytes(png[16..20].try_into().unwrap()),
        u32::from_be_bytes(png[20..24].try_into().unwrap()),
    )
}

fn crc32(data: &[u8]) -> u32 {
    let mut crc = !0u32;
    for &b in data {
        crc ^= b as u32;
        for _ in 0..8 {
            crc = (crc >> 1) ^ (0xedb8_8320 & (crc & 1).wrapping_neg());
        }
    }
    !crc
}

// A 4 x 4 red PNG, its pixels in one stored (uncompressed) Deflate block so no compressor is needed.
fn tiny_png() -> Vec<u8> {
    fn chunk(out: &mut Vec<u8>, kind: &[u8], data: &[u8]) {
        out.extend_from_slice(&(data.len() as u32).to_be_bytes());
        let body = [kind, data].concat();
        out.extend_from_slice(&body);
        out.extend_from_slice(&crc32(&body).to_be_bytes());
    }
    let row: Vec<u8> = std::iter::once(0).chain([200, 30, 30].repeat(4)).collect();
    let raw = row.repeat(4);
    let (mut a, mut b) = (1u32, 0u32);
    for &x in &raw {
        a = (a + x as u32) % 65521;
        b = (b + a) % 65521;
    }
    let mut zlib = vec![0x78, 0x01, 0x01];
    zlib.extend_from_slice(&(raw.len() as u16).to_le_bytes());
    zlib.extend_from_slice(&(!(raw.len() as u16)).to_le_bytes());
    zlib.extend_from_slice(&raw);
    zlib.extend_from_slice(&((b << 16) | a).to_be_bytes());
    let mut header = Vec::new();
    header.extend_from_slice(&4u32.to_be_bytes());
    header.extend_from_slice(&4u32.to_be_bytes());
    header.extend_from_slice(&[8, 2, 0, 0, 0]);
    let mut png = b"\x89PNG\r\n\x1a\n".to_vec();
    chunk(&mut png, b"IHDR", &header);
    chunk(&mut png, b"IDAT", &zlib);
    chunk(&mut png, b"IEND", &[]);
    png
}

fn long_html(rows: usize) -> String {
    let mut html = String::from("<table>");
    for i in 0..rows {
        html.push_str(&format!("<tr><td>Row {i}</td><td>{}</td></tr>", i * 3));
    }
    html + "</table>"
}

fn temp_dir(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("pdfpecker-rust-{}-{name}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn the_library_version_is_the_crate_version() {
    assert_eq!(library_version().unwrap(), env!("CARGO_PKG_VERSION"));
    assert!(library_path().unwrap().is_file());
}

#[test]
fn html_to_pdf_and_text() {
    let pdf = pdf("<h1>Hello</h1><p>Grüße, ünïcode — “quotes”</p>");
    assert!(pdf.starts_with(b"%PDF-"));
    assert_eq!(pages(&pdf), 1);
    assert_eq!(text(&pdf), "Hello\n\nGrüße, ünïcode — “quotes”");
}

#[test]
fn a_fixed_date_makes_output_byte_identical() {
    let options = PdfOptions::new().creation_date(UNIX_EPOCH + Duration::from_secs(1_767_312_000));
    assert_eq!(
        html_to_pdf("<p>same</p>", &options).unwrap(),
        html_to_pdf("<p>same</p>", &options).unwrap()
    );
}

#[test]
fn page_size_landscape_and_margin() {
    let size = |options: PdfOptions| {
        let pdf = html_to_pdf("<p>x", &options).unwrap();
        png_size(&pdf_to_images(&pdf, &ImageOptions::new().dpi(72.0)).unwrap()[0])
    };
    assert_eq!(
        size(PdfOptions::new().page_size(PageSize::Letter)),
        (612, 792)
    );
    assert_eq!(
        size(PdfOptions::new().page_size(PageSize::A5).landscape(true)),
        (595, 420)
    );
    assert_eq!(
        size(
            PdfOptions::new()
                .page_size(PageSize::Millimeters {
                    width: 100.0,
                    height: 50.0
                })
                .margin(Margin::all(5.0))
        ),
        (283, 142)
    );
}

#[test]
fn document_properties() {
    let pdf = html_to_pdf(
        "<p>x",
        &PdfOptions::new()
            .title("My title")
            .author("Me")
            .compress(false),
    )
    .unwrap();
    let utf16: Vec<u8> = "\u{feff}My title"
        .encode_utf16()
        .flat_map(u16::to_be_bytes)
        .collect();
    assert!(
        pdf.windows(8).any(|w| w == b"My title") || pdf.windows(utf16.len()).any(|w| w == utf16)
    );
}

#[test]
fn pdfa_level_a_is_tagged() {
    let pdf = html_to_pdf(
        "<h1>Title</h1><p>Text",
        &PdfOptions::new()
            .conformance(Conformance::PdfA3a)
            .compress(false),
    )
    .unwrap();
    for needle in [&b"/StructTreeRoot"[..], b"/Lang"] {
        assert!(pdf.windows(needle.len()).any(|w| w == needle));
    }
}

#[test]
fn base_dir_loads_images_and_nothing_loads_without_it() {
    let dir = temp_dir("base");
    std::fs::write(dir.join("dot.png"), tiny_png()).unwrap();
    let html = r#"<img src="dot.png" style="width:100px;height:100px">"#;
    let without = pdf(html);
    let with = html_to_pdf(html, &PdfOptions::new().base_dir(&dir)).unwrap();
    assert!(with.len() > without.len());
    let sub = dir.join("sub");
    std::fs::create_dir_all(&sub).unwrap();
    let outside = r#"<img src="../dot.png" style="width:100px;height:100px">"#;
    assert!(
        html_to_pdf(
            outside,
            &PdfOptions::new().base_dir(&sub).allow_file_access(true)
        )
        .unwrap()
        .len()
            > html_to_pdf(outside, &PdfOptions::new().base_dir(&sub))
                .unwrap()
                .len()
    );
}

#[test]
fn html_and_markdown_files_resolve_images_next_to_them() {
    let dir = temp_dir("files");
    std::fs::write(dir.join("dot.png"), tiny_png()).unwrap();
    let html = r#"<h1>File</h1><img src="dot.png" width=40 height=40>"#;
    std::fs::write(dir.join("doc.html"), html).unwrap();
    let from_file =
        html_file_to_pdf(dir.join("doc.html"), &PdfOptions::new().title("File")).unwrap();
    assert!(text(&from_file).contains("File"));
    assert!(from_file.len() > pdf(html).len());
    std::fs::write(
        dir.join("doc.md"),
        "# Heading\n\n- [x] done\n- [ ] todo\n\n![dot](dot.png)\n",
    )
    .unwrap();
    assert!(
        text(&markdown_file_to_pdf(dir.join("doc.md"), &PdfOptions::new()).unwrap())
            .contains("Heading")
    );
    let missing = html_file_to_pdf(dir.join("missing.html"), &PdfOptions::new()).unwrap_err();
    assert_eq!(missing.kind(), ErrorKind::Io);
}

#[test]
fn markdown_to_pdf_works() {
    let pdf = markdown_to_pdf(
        "# Title\n\n| a | b |\n|---|---|\n| 1 | 2 |\n",
        &PdfOptions::new().page_size(PageSize::Letter),
    )
    .unwrap();
    let t = text(&pdf);
    assert!(t.starts_with("Title") && t.contains('1'), "{t}");
}

#[test]
fn footer_text_and_page_numbers() {
    let pdf = html_to_pdf(
        &long_html(300),
        &PdfOptions::new().footer_text("Page {page} of {pages}"),
    )
    .unwrap();
    let count = pages(&pdf);
    assert!(count > 1);
    assert!(
        pdf_to_text_pages(&pdf, &TextOptions::new()).unwrap()[1]
            .contains(&format!("Page 2 of {count}"))
    );
}

#[test]
fn pdf_to_images_formats_pages_and_size() {
    let pdf = pdf(&long_html(300));
    let count = pages(&pdf);
    let render = |o: ImageOptions| pdf_to_images(&pdf, &o).unwrap();
    assert_eq!(render(ImageOptions::new().dpi(20.0)).len(), count);
    assert_eq!(render(ImageOptions::new().dpi(20.0).pages("1-2")).len(), 2);
    assert_eq!(
        render(ImageOptions::new().dpi(20.0).pages([1, 1, 2])).len(),
        3
    );
    assert_eq!(render(ImageOptions::new().dpi(20.0).pages(1..=2)).len(), 2);
    assert_eq!(
        render(ImageOptions::new().dpi(20.0).pages("last")),
        render(ImageOptions::new().dpi(20.0).pages(count as u32))
    );
    assert_eq!(
        png_size(&render(ImageOptions::new().width(300).pages(1))[0]).0,
        300
    );
    let jpeg = &render(
        ImageOptions::new()
            .format(ImageFormat::Jpeg)
            .quality(60)
            .pages(1)
            .dpi(30.0),
    )[0];
    assert_eq!(&jpeg[..3], &[0xff, 0xd8, 0xff]);
    let webp = &render(
        ImageOptions::new()
            .format(ImageFormat::WebP)
            .pages(1)
            .dpi(30.0),
    )[0];
    assert_eq!((&webp[..4], &webp[8..12]), (&b"RIFF"[..], &b"WEBP"[..]));
}

#[test]
fn text_pages() {
    let pdf =
        pdf("<p>one</p><p style='break-before:page'>two</p><p style='break-before:page'>three</p>");
    assert_eq!(
        pdf_to_text_pages(&pdf, &TextOptions::new()).unwrap(),
        ["one", "two", "three"]
    );
    assert_eq!(
        pdf_to_text_pages(&pdf, &TextOptions::new().pages("2-3")).unwrap(),
        ["two", "three"]
    );
    assert_eq!(text(&pdf), "one\u{c}two\u{c}three");
}

#[test]
fn text_order() {
    let column = |side: &str| {
        (0..12)
            .map(|i| format!("{side} column sentence number {i} goes on here."))
            .collect::<Vec<_>>()
            .join(" ")
    };
    let pdf = pdf(&format!(
        "<div style='display: flex; gap: 40px'><p style='flex: 1'>{}</p><p style='flex: 1'>{}</p></div>",
        column("Left"),
        column("Right")
    ));
    let reading = text(&pdf);
    assert!(reading.find("Right").unwrap() > reading.find("number 11").unwrap());
    let across = pdf_to_text(&pdf, &TextOptions::new().order(TextOrder::TopToBottom)).unwrap();
    assert!(across.find("Right").unwrap() < across.find("number 11").unwrap());
}

#[test]
fn redact() {
    let pdf = pdf("<p>Customer 123456-7890 owes 42 EUR</p><p>Contact Example Customer Ltd.</p>");
    let redacted = redact_pdf(
        &pdf,
        &RedactOptions::new()
            .pattern(r"\b\d{6}-\d{4}\b")
            .text("example customer")
            .ignore_case(true),
    )
    .unwrap();
    let left = text(&redacted);
    assert!(
        !left.contains("123456") && !left.contains("Example"),
        "{left}"
    );
    assert!(left.contains("owes 42 EUR"), "{left}");
    let cut = redact_pdf(
        &pdf,
        &RedactOptions::new()
            .area(1, 0.0, 0.0, 2000.0, 2000.0)
            .fill_color("transparent"),
    )
    .unwrap();
    assert_eq!(text(&cut).trim(), "");
    let bad = redact_pdf(&pdf, &RedactOptions::new().pattern("(")).unwrap_err();
    assert_eq!(bad.kind(), ErrorKind::InvalidArgument);
    let past_end = redact_pdf(&pdf, &RedactOptions::new().area(9, 0.0, 0.0, 10.0, 10.0));
    assert_eq!(past_end.unwrap_err().kind(), ErrorKind::InvalidArgument);
}

#[test]
fn barcodes() {
    let pdf = pdf(&format!("<p>Pay here</p>{}", include_str!("qr.svg")));
    let found = read_barcodes(&pdf, &BarcodeOptions::new()).unwrap();
    let [code] = &found[..] else {
        panic!("{found:?}")
    };
    assert_eq!(code.format, BarcodeFormat::QrCode);
    assert_eq!(code.text, "https://example.com/invoice/42");
    assert_eq!(code.data, code.text.as_bytes());
    assert!(code.page == 1 && code.width > 0.0 && code.corners.len() == 4);
    let png = pdf_to_images(&pdf, &ImageOptions::new().dpi(150.0))
        .unwrap()
        .remove(0);
    let from_image = read_barcodes(&png, &BarcodeOptions::new()).unwrap();
    assert_eq!(from_image.len(), 1);
    assert_eq!(from_image[0].text, code.text);
    assert_eq!(from_image[0].page, 0);
    let none = read_barcodes(&png, &BarcodeOptions::new().formats([BarcodeFormat::Ean13]));
    assert!(none.unwrap().is_empty());
    let pages_on_image = read_barcodes(&png, &BarcodeOptions::new().pages(1));
    assert_eq!(
        pages_on_image.unwrap_err().kind(),
        ErrorKind::InvalidArgument
    );
}

#[test]
fn merge() {
    let (a, b) = (pdf("<p>first"), pdf("<p>second"));
    let merged = merge_pdfs(&[&a, &b], &MergeOptions::new().title("Both")).unwrap();
    assert_eq!(
        pdf_to_text_pages(&merged, &TextOptions::new()).unwrap(),
        ["first", "second"]
    );
    assert_eq!(
        pdf_to_text_pages(
            &merge_pdfs(&[a.clone(), a], &MergeOptions::new()).unwrap(),
            &TextOptions::new()
        )
        .unwrap(),
        ["first", "first"]
    );
}

#[test]
fn pdf_to_markdown_works() {
    let pdf = html_to_pdf(
        "<h1>Report</h1><p>See <a href='https://example.com'>the site</a>.</p><ul><li>one</li><li>two</li></ul>",
        &PdfOptions::new().accessibility(Accessibility::Tagged),
    )
    .unwrap();
    let markdown = pdf_to_markdown(&pdf, &MarkdownOptions::new()).unwrap();
    assert!(markdown.starts_with("# Report"), "{markdown}");
    assert!(markdown.contains("](https://example.com/)"));
    assert!(markdown.contains("- one") && markdown.contains("- two"));
    let commented = pdf_to_markdown(
        &pdf,
        &MarkdownOptions::new()
            .include_links(false)
            .page_comments(true),
    )
    .unwrap();
    assert!(commented.starts_with("<!-- page 1 -->"));
    assert_eq!(
        pdf_to_markdown(&pdf, &MarkdownOptions::new().pages(3))
            .unwrap_err()
            .kind(),
        ErrorKind::InvalidArgument
    );
}

#[test]
fn pdf_to_pdfa_works() {
    let pdf = html_to_pdf(
        "<p>archive me",
        &PdfOptions::new().creation_date(UNIX_EPOCH),
    )
    .unwrap();
    let result = pdf_to_pdfa(&pdf, &PdfAOptions::new().conformance(Conformance::PdfA3u)).unwrap();
    assert!(result.pdf.windows(6).any(|w| w == b"pdfaid"));
    assert!(result.is_conformant(), "{:?}", result.problems);
    assert_eq!(text(&result.pdf), "archive me");
    let e = pdf_to_pdfa(&pdf, &PdfAOptions::new().conformance(Conformance::PdfA4)).unwrap_err();
    assert_eq!(e.kind(), ErrorKind::InvalidArgument);
    assert!(e.message().contains("conformance"));
    assert_eq!(
        pdf_to_pdfa(b"not a pdf", &PdfAOptions::new())
            .unwrap_err()
            .kind(),
        ErrorKind::Unreadable
    );
}

#[test]
fn errors_have_kinds() {
    let kind = |r: Result<Vec<u8>>| r.unwrap_err().kind();
    assert_eq!(
        kind(html_to_pdf("x", &PdfOptions::new().scale(f32::NAN))),
        ErrorKind::InvalidArgument
    );
    assert_eq!(
        kind(html_to_pdf(
            &long_html(2000),
            &PdfOptions::new().max_pages(2)
        )),
        ErrorKind::Limit
    );
    assert_eq!(
        kind(html_to_pdf(
            &long_html(20000),
            &PdfOptions::new().timeout(Duration::from_millis(1))
        )),
        ErrorKind::Timeout
    );
    assert_eq!(
        kind(html_to_pdf(
            "x",
            &PdfOptions::new().timeout(Duration::from_secs(1 << 40))
        )),
        ErrorKind::InvalidArgument
    );
    assert_eq!(
        kind(html_to_pdf("x", &PdfOptions::new().timeout(Duration::ZERO))),
        ErrorKind::InvalidArgument
    );
    assert_eq!(
        kind(html_to_pdf(
            "x",
            &PdfOptions::new().font("/no/such/font.ttf")
        )),
        ErrorKind::Io
    );
    assert_eq!(
        pdf_page_count(b"this is not a PDF", &PageCountOptions::new())
            .unwrap_err()
            .kind(),
        ErrorKind::Unreadable
    );
    assert_eq!(
        pdf_to_images(b"%PDF-1.7\n%%EOF", &ImageOptions::new())
            .unwrap_err()
            .kind(),
        ErrorKind::Unreadable
    );
    let one = pdf("<p>x");
    let e = pdf_to_images(&one, &ImageOptions::new().pages(5)).unwrap_err();
    assert!(e.message().contains("out of range"), "{e}");
}

#[test]
fn conversions_run_in_parallel_with_identical_results() {
    let html = long_html(400);
    let options = PdfOptions::new().creation_date(UNIX_EPOCH);
    let expected = html_to_pdf(&html, &options).unwrap();
    std::thread::scope(|s| {
        let handles: Vec<_> = (0..8)
            .map(|_| s.spawn(|| html_to_pdf(&html, &options).unwrap()))
            .collect();
        for h in handles {
            assert_eq!(h.join().unwrap(), expected);
        }
    });
}

#[test]
fn hostile_input_fails_cleanly() {
    // Damaged PDFs are either repaired or rejected with an error; nothing may crash. This runs on a thread with Rust's
    // default stack size, as callers' worker threads do.
    std::thread::spawn(|| {
        let noise: Vec<u8> = (0..50).flat_map(|_| 0..=255u8).collect();
        for data in [&b"%PDF-1.7\n1 0 obj <</Type/Catalog/Pages 2 0 R>> endobj\ntrailer<</Root 1 0 R>>\n%%EOF"[..], &noise, b""] {
            let _ = pdf_to_images(data, &ImageOptions::new());
        }
        for html in ["<".repeat(10000), "<div>".repeat(5000), "&#xFFFFFFFF;".to_string()] {
            let _ = html_to_pdf(&html, &PdfOptions::new().timeout(Duration::from_secs(30)));
        }
    })
    .join()
    .unwrap();
}
