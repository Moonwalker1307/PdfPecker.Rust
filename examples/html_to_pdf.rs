//! cargo run --release --example html_to_pdf -- input.html output.pdf
//!
//! Converts an HTML file to a PDF; images and stylesheets next to the file are found.

use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [input, output] = args.as_slice() else {
        eprintln!("usage: html_to_pdf <input.html> <output.pdf>");
        return ExitCode::from(2);
    };
    let options = pdfpecker::PdfOptions::new().footer_text("Page {page} of {pages}");
    match pdfpecker::html_file_to_pdf(input, &options).and_then(|pdf| {
        std::fs::write(output, &pdf).map_err(pdfpecker::Error::from)?;
        Ok(pdf.len())
    }) {
        Ok(bytes) => {
            println!("{output}: {bytes} bytes");
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("{input}: {e}");
            ExitCode::FAILURE
        }
    }
}
