#!/usr/bin/env bash
# Tests this machine's platform twice: the crate in this checkout, then the packaged crate used as a dependency of a
# fresh project, the way users get it from crates.io.
set -euo pipefail
cd "$(dirname "$0")/.."
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
RUSTDOCFLAGS="-D warnings" cargo doc --no-deps
cargo package --allow-dirty
crate=$(ls target/package/pdfpecker-*.crate | head -n 1)
rm -rf target/install-test
mkdir -p target/install-test/src
tar -xzf "$crate" -C target/install-test
dir=$(basename "$crate" .crate)
cat > target/install-test/Cargo.toml <<TOML
[package]
name = "install-test"
version = "0.0.0"
edition = "2024"

[dependencies]
pdfpecker = { path = "$dir" }

[workspace]
TOML
cat > target/install-test/src/main.rs <<'RS'
fn main() {
    let pdf = pdfpecker::html_to_pdf("<h1>Installed</h1>", &Default::default()).unwrap();
    let text = pdfpecker::pdf_to_text(&pdf, &Default::default()).unwrap();
    assert_eq!(text, "Installed");
    println!("{} from {}", pdfpecker::library_version().unwrap(), pdfpecker::library_path().unwrap().display());
}
RS
(cd target/install-test && cargo run --quiet)
