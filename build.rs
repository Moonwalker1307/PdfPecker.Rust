//! Finds PdfPecker's native library for the target and tells the crate where it is.
//!
//! The library is not built here. It is the NativeAOT build of PdfPecker inside the pdfpecker wheel on PyPI, the one
//! the Python and npm packages ship, so all three run the same binary. The build script downloads the target's wheel
//! once, checks the wheel and the library against the SHA-256 values in wheels.txt, keeps the library in a cache and
//! copies it into OUT_DIR. The crate loads it at run time from there, from next to the executable or from
//! PDFPECKER_NATIVE_LIB.
//!
//! Environment variables:
//!   PDFPECKER_LIB_DIR    a folder that already holds the library: nothing is downloaded (offline builds)
//!   PDFPECKER_CACHE_DIR  where downloaded libraries are kept (default: the user's cache folder)

use sha2::{Digest, Sha256};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=wheels.txt");
    println!("cargo:rerun-if-changed=build.rs");
    for var in ["PDFPECKER_LIB_DIR", "PDFPECKER_CACHE_DIR", "DOCS_RS"] {
        println!("cargo:rerun-if-env-changed={var}");
    }
    // docs.rs builds the documentation without network access, and nothing runs there.
    if env::var_os("DOCS_RS").is_some() {
        return;
    }
    if let Err(message) = run() {
        // A failed download or a wrong checksum stops the build with the reason, rather than a crate that fails
        // later at run time.
        eprintln!("error: {message}");
        panic!("{message}");
    }
}

fn run() -> Result<(), String> {
    let os = env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let arch = env::var("CARGO_CFG_TARGET_ARCH").unwrap_or_default();
    let target_env = env::var("CARGO_CFG_TARGET_ENV").unwrap_or_default();
    let key = match (os.as_str(), arch.as_str(), target_env.as_str()) {
        ("linux", "x86_64", "musl") => "linux-x64-musl",
        ("linux", "aarch64", "musl") => "linux-arm64-musl",
        ("linux", "x86_64", _) => "linux-x64",
        ("linux", "aarch64", _) => "linux-arm64",
        ("macos", "aarch64", _) => "darwin-arm64",
        ("macos", "x86_64", _) => "darwin-x64",
        ("windows", "x86_64", _) => "win32-x64",
        ("windows", "aarch64", _) => "win32-arm64",
        _ => {
            // The crate still compiles, so a project that builds for several targets is not stopped; calls return
            // an error that names the target.
            println!(
                "cargo:warning=PdfPecker has no native library for {os} on {arch}; it runs on Linux (glibc and musl), \
                 macOS and Windows, on x64 and arm64"
            );
            return Ok(());
        }
    };
    if target_env == "musl"
        && env::var("CARGO_CFG_TARGET_FEATURE")
            .unwrap_or_default()
            .split(',')
            .any(|f| f == "crt-static")
    {
        return Err("PdfPecker loads a shared library, which a statically linked musl binary cannot do. Build with \
                    RUSTFLAGS=\"-C target-feature=-crt-static\" (Alpine's own Rust does this by default)."
            .into());
    }

    let manifest = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let version = env::var("CARGO_PKG_VERSION").unwrap();
    let wheels =
        fs::read_to_string(manifest.join("wheels.txt")).map_err(|e| format!("wheels.txt: {e}"))?;
    let entry = wheels
        .lines()
        .filter(|l| !l.starts_with('#'))
        .map(|l| l.split_whitespace().collect::<Vec<_>>())
        .find(|f| f.len() == 5 && f[0] == key)
        .ok_or_else(|| format!("wheels.txt has no wheel for {key}"))?;
    let (url, wheel_sha, library, library_sha) = (entry[1], entry[2], entry[3], entry[4]);

    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    let target = out_dir.join(library);
    let bytes = if let Some(dir) = env::var_os("PDFPECKER_LIB_DIR") {
        // A library the caller provides is taken as it is: it may be a build of their own.
        let path = Path::new(&dir).join(library);
        println!("cargo:rerun-if-changed={}", path.display());
        fs::read(&path).map_err(|e| format!("PDFPECKER_LIB_DIR: {}: {e}", path.display()))?
    } else {
        cached_or_downloaded(
            key,
            &version,
            url,
            wheel_sha,
            library,
            library_sha,
            &out_dir,
        )?
    };
    fs::write(&target, bytes).map_err(|e| format!("{}: {e}", target.display()))?;
    println!(
        "cargo:rustc-env=PDFPECKER_BUILD_LIBRARY={}",
        target.display()
    );
    Ok(())
}

fn cached_or_downloaded(
    key: &str,
    version: &str,
    url: &str,
    wheel_sha: &str,
    library: &str,
    library_sha: &str,
    out_dir: &Path,
) -> Result<Vec<u8>, String> {
    let cache = cache_dir().map(|d| d.join(version).join(key).join(library));
    if let Some(path) = &cache {
        if let Ok(bytes) = fs::read(path) {
            if hex(&Sha256::digest(&bytes)) == library_sha {
                return Ok(bytes);
            }
        }
    }
    let wheel_path = out_dir.join(format!("pdfpecker-{version}-{key}.whl"));
    download(url, &wheel_path)?;
    let wheel = fs::read(&wheel_path).map_err(|e| format!("{}: {e}", wheel_path.display()))?;
    let _ = fs::remove_file(&wheel_path);
    if hex(&Sha256::digest(&wheel)) != wheel_sha {
        return Err(format!(
            "{url}: the download does not match the SHA-256 in wheels.txt"
        ));
    }
    let bytes = unzip(&wheel, &format!("pdfpecker/{library}"))?;
    if hex(&Sha256::digest(&bytes)) != library_sha {
        return Err(format!(
            "{library} in {url} does not match the SHA-256 in wheels.txt"
        ));
    }
    if let Some(path) = &cache {
        // The cache only saves downloads, so a cache that cannot be written is not an error. Writing to a temporary
        // name and renaming keeps a parallel build from reading half a file.
        if let Some(parent) = path.parent() {
            let temp = parent.join(format!(".{library}.{}", std::process::id()));
            if fs::create_dir_all(parent).is_ok()
                && fs::write(&temp, &bytes).is_ok()
                && fs::rename(&temp, path).is_err()
            {
                let _ = fs::remove_file(&temp);
            }
        }
    }
    Ok(bytes)
}

fn cache_dir() -> Option<PathBuf> {
    if let Some(dir) = env::var_os("PDFPECKER_CACHE_DIR") {
        return Some(PathBuf::from(dir));
    }
    let home = |var: &str| {
        env::var_os(var)
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
    };
    let base = if cfg!(windows) {
        home("LOCALAPPDATA")
    } else if cfg!(target_os = "macos") {
        home("HOME").map(|h| h.join("Library").join("Caches"))
    } else {
        home("XDG_CACHE_HOME").or_else(|| home("HOME").map(|h| h.join(".cache")))
    };
    base.map(|b| b.join("pdfpecker-rust"))
}

// curl ships with Windows 10 and later, macOS and nearly every Linux; wget covers minimal Linux images. Either keeps
// TLS out of the build dependencies.
fn download(url: &str, to: &Path) -> Result<(), String> {
    let attempts: [(&str, Vec<&str>); 2] = [
        (
            "curl",
            vec![
                "--fail",
                "--location",
                "--silent",
                "--show-error",
                "--retry",
                "3",
                "--output",
            ],
        ),
        ("wget", vec!["--quiet", "--tries=3", "--output-document"]),
    ];
    let mut errors = Vec::new();
    for (tool, args) in attempts {
        match Command::new(tool).args(&args).arg(to).arg(url).output() {
            Ok(out) if out.status.success() => return Ok(()),
            Ok(out) => errors.push(format!(
                "{tool}: {}",
                String::from_utf8_lossy(&out.stderr).trim()
            )),
            Err(e) => errors.push(format!("{tool}: {e}")),
        }
    }
    Err(format!(
        "could not download PdfPecker's native library from {url} ({}). Install curl, or download the wheel yourself, \
         take pdfpecker/pdfpecker_native.* out of it (a wheel is a zip file) and set PDFPECKER_LIB_DIR to its folder.",
        errors.join("; ")
    ))
}

// Reads one file out of a zip archive (a wheel). Wheels are far below 4 GB, so there is no ZIP64 to handle.
fn unzip(zip: &[u8], wanted: &str) -> Result<Vec<u8>, String> {
    let u16_at = |i: usize| {
        zip.get(i..i + 2)
            .map(|b| u16::from_le_bytes([b[0], b[1]]) as usize)
    };
    let u32_at = |i: usize| {
        zip.get(i..i + 4)
            .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as usize)
    };
    let damaged = || "the wheel is not a readable zip archive".to_string();
    let end = (zip.len().saturating_sub(65557)..zip.len().saturating_sub(21))
        .rev()
        .find(|&i| u32_at(i) == Some(0x0605_4b50))
        .ok_or_else(damaged)?;
    let count = u16_at(end + 10).ok_or_else(damaged)?;
    let mut at = u32_at(end + 16).ok_or_else(damaged)?;
    for _ in 0..count {
        if u32_at(at) != Some(0x0201_4b50) {
            return Err(damaged());
        }
        let method = u16_at(at + 10).ok_or_else(damaged)?;
        let compressed = u32_at(at + 20).ok_or_else(damaged)?;
        let size = u32_at(at + 24).ok_or_else(damaged)?;
        let name_len = u16_at(at + 28).ok_or_else(damaged)?;
        let extra_len = u16_at(at + 30).ok_or_else(damaged)?;
        let comment_len = u16_at(at + 32).ok_or_else(damaged)?;
        let local = u32_at(at + 42).ok_or_else(damaged)?;
        let name = zip.get(at + 46..at + 46 + name_len).ok_or_else(damaged)?;
        at += 46 + name_len + extra_len + comment_len;
        if name != wanted.as_bytes() {
            continue;
        }
        let start = local
            + 30
            + u16_at(local + 26).ok_or_else(damaged)?
            + u16_at(local + 28).ok_or_else(damaged)?;
        let data = zip.get(start..start + compressed).ok_or_else(damaged)?;
        let out = match method {
            0 => data.to_vec(),
            8 => miniz_oxide::inflate::decompress_to_vec(data).map_err(|_| damaged())?,
            _ => return Err(format!("{wanted} uses zip method {method}")),
        };
        if out.len() != size {
            return Err(format!("{wanted} has the wrong size"));
        }
        return Ok(out);
    }
    Err(format!("{wanted} is not in the wheel"))
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
