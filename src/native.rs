//! Loading the native library and calling its one entry point.
//!
//! The library is PdfPecker compiled with NativeAOT; its C ABI is PdfPecker.Python's (native/Exports.cs there):
//!
//! ```c
//! int  pdfpecker_invoke(const char* op, const uint8_t* input, int64_t input_len, const char* options_json,
//!                       uint8_t** output, int64_t* output_len, char** error);
//! void pdfpecker_free(void* p);
//! const char* pdfpecker_version(void);
//! int  pdfpecker_abi_version(void);
//! ```
//!
//! It is loaded with the platform's own dlopen or LoadLibrary, so the crate has no dependencies, and never unloaded:
//! the .NET runtime inside registers a destructor for every thread that called into it, which must still find its
//! code when that thread ends.

use crate::error::{Error, ErrorKind, Result};
use std::ffi::{CStr, CString, c_char, c_int, c_void};
use std::path::PathBuf;
use std::sync::OnceLock;

const ABI_VERSION: c_int = 1;

#[cfg(target_os = "windows")]
const LIBRARY: &str = "pdfpecker_native.dll";
#[cfg(target_os = "macos")]
const LIBRARY: &str = "pdfpecker_native.dylib";
#[cfg(not(any(target_os = "windows", target_os = "macos")))]
const LIBRARY: &str = "pdfpecker_native.so";

type InvokeFn = unsafe extern "C" fn(
    *const c_char,
    *const u8,
    i64,
    *const c_char,
    *mut *mut u8,
    *mut i64,
    *mut *mut c_char,
) -> c_int;
type FreeFn = unsafe extern "C" fn(*mut c_void);
type VersionFn = unsafe extern "C" fn() -> *const c_char;
type AbiFn = unsafe extern "C" fn() -> c_int;

struct Native {
    path: PathBuf,
    invoke: InvokeFn,
    free: FreeFn,
    version: String,
}

static NATIVE: OnceLock<std::result::Result<Native, String>> = OnceLock::new();

fn native() -> Result<&'static Native> {
    NATIVE
        .get_or_init(load)
        .as_ref()
        .map_err(|m| Error::new(ErrorKind::Library, m.clone()))
}

// Where the library is looked for, in order: PDFPECKER_NATIVE_LIB, next to the executable (how a built program is
// deployed), and where the build script put it (cargo run, cargo test).
fn candidates() -> Vec<PathBuf> {
    let mut paths = Vec::new();
    if let Some(path) = std::env::var_os("PDFPECKER_NATIVE_LIB").filter(|p| !p.is_empty()) {
        paths.push(PathBuf::from(path));
        return paths;
    }
    if let Some(dir) = std::env::current_exe()
        .ok()
        .and_then(|e| e.parent().map(PathBuf::from))
    {
        paths.push(dir.join(LIBRARY));
    }
    if let Some(built) = option_env!("PDFPECKER_BUILD_LIBRARY") {
        paths.push(PathBuf::from(built));
    }
    paths
}

fn load() -> std::result::Result<Native, String> {
    let paths = candidates();
    let Some(path) = paths.iter().find(|p| p.is_file()).cloned() else {
        let tried: Vec<String> = paths.iter().map(|p| p.display().to_string()).collect();
        return Err(format!(
            "PdfPecker's native library {LIBRARY} was not found (looked in: {}). Put it next to the executable, or \
             set PDFPECKER_NATIVE_LIB to its path. The build copies it into target/<profile>/build/pdfpecker-*/out. \
             PdfPecker runs on Linux (glibc and musl), macOS and Windows, on x64 and arm64.",
            if tried.is_empty() {
                "nowhere".to_string()
            } else {
                tried.join(", ")
            }
        ));
    };
    let handle = sys::open(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    let symbol = |name: &str| {
        let c = CString::new(name).unwrap();
        let p = sys::symbol(handle, &c);
        if p.is_null() {
            Err(format!(
                "{}: has no {name}; is it PdfPecker's native library?",
                path.display()
            ))
        } else {
            Ok(p)
        }
    };
    // SAFETY: the symbols are the library's exported functions with exactly these signatures (native/Exports.cs).
    unsafe {
        let abi: AbiFn =
            std::mem::transmute::<*mut c_void, AbiFn>(symbol("pdfpecker_abi_version")?);
        let found = abi();
        if found != ABI_VERSION {
            return Err(format!(
                "{} speaks ABI {found}, this crate expects {ABI_VERSION}; use the library of the crate's version",
                path.display()
            ));
        }
        let version: VersionFn =
            std::mem::transmute::<*mut c_void, VersionFn>(symbol("pdfpecker_version")?);
        Ok(Native {
            invoke: std::mem::transmute::<*mut c_void, InvokeFn>(symbol("pdfpecker_invoke")?),
            free: std::mem::transmute::<*mut c_void, FreeFn>(symbol("pdfpecker_free")?),
            version: CStr::from_ptr(version()).to_string_lossy().into_owned(),
            path,
        })
    }
}

pub(crate) fn library_version() -> Result<String> {
    Ok(native()?.version.clone())
}

pub(crate) fn library_path() -> Result<PathBuf> {
    Ok(native()?.path.clone())
}

/// Runs one operation. The library copies the input before it returns and allocates the output, which is copied into
/// a `Vec` and released here.
pub(crate) fn invoke(op: &str, input: &[u8], options_json: &str) -> Result<Vec<u8>> {
    let n = native()?;
    let op = CString::new(op).expect("operation names have no NUL");
    let options = CString::new(options_json)
        .map_err(|_| Error::argument("an option contains a NUL character"))?;
    let mut output: *mut u8 = std::ptr::null_mut();
    let mut output_len: i64 = 0;
    let mut error: *mut c_char = std::ptr::null_mut();
    let input_ptr = if input.is_empty() {
        std::ptr::NonNull::<u8>::dangling().as_ptr().cast_const()
    } else {
        input.as_ptr()
    };
    // SAFETY: every pointer is valid for the call; the library never keeps them after it returns.
    let status = unsafe {
        (n.invoke)(
            op.as_ptr(),
            input_ptr,
            input.len() as i64,
            options.as_ptr(),
            &mut output,
            &mut output_len,
            &mut error,
        )
    };
    // SAFETY: on failure *error, on success *output, are buffers the library allocated for the caller to free.
    unsafe {
        if status != 0 {
            let message = if error.is_null() {
                "PdfPecker failed".to_string()
            } else {
                let m = CStr::from_ptr(error).to_string_lossy().into_owned();
                (n.free)(error.cast());
                m
            };
            return Err(Error::from_status(status, message));
        }
        if output.is_null() {
            return Ok(Vec::new());
        }
        let bytes = std::slice::from_raw_parts(output, output_len.max(0) as usize).to_vec();
        (n.free)(output.cast());
        Ok(bytes)
    }
}

/// Lists (several PDFs in, several images or pages out) are an int32 count, then an int64 length and the bytes of
/// each item, little-endian.
pub(crate) fn frame<T: AsRef<[u8]>>(items: &[T]) -> Result<Vec<u8>> {
    let count = i32::try_from(items.len()).map_err(|_| Error::argument("too many items"))?;
    let mut out = Vec::with_capacity(4 + items.iter().map(|i| 8 + i.as_ref().len()).sum::<usize>());
    out.extend_from_slice(&count.to_le_bytes());
    for item in items {
        out.extend_from_slice(&(item.as_ref().len() as i64).to_le_bytes());
        out.extend_from_slice(item.as_ref());
    }
    Ok(out)
}

pub(crate) fn unframe(data: &[u8]) -> Result<Vec<Vec<u8>>> {
    let bad = || {
        Error::new(
            ErrorKind::Internal,
            "the native library returned a malformed list",
        )
    };
    let count = i32::from_le_bytes(data.get(..4).ok_or_else(bad)?.try_into().unwrap());
    let mut items = Vec::with_capacity(count.clamp(0, 100_000) as usize);
    let mut at = 4usize;
    for _ in 0..count.max(0) {
        let len = i64::from_le_bytes(data.get(at..at + 8).ok_or_else(bad)?.try_into().unwrap());
        let len = usize::try_from(len).map_err(|_| bad())?;
        at += 8;
        items.push(
            data.get(at..at.checked_add(len).ok_or_else(bad)?)
                .ok_or_else(bad)?
                .to_vec(),
        );
        at += len;
    }
    Ok(items)
}

#[cfg(unix)]
mod sys {
    use std::ffi::{CStr, CString, c_char, c_int, c_void};
    use std::os::unix::ffi::OsStrExt;
    use std::path::Path;

    // glibc before 2.34 keeps these in libdl; later versions and musl keep a libdl that forwards to libc.
    #[cfg_attr(any(target_os = "linux", target_os = "android"), link(name = "dl"))]
    unsafe extern "C" {
        fn dlopen(file: *const c_char, mode: c_int) -> *mut c_void;
        fn dlsym(handle: *mut c_void, name: *const c_char) -> *mut c_void;
        fn dlerror() -> *mut c_char;
    }

    const RTLD_NOW: c_int = 2;

    pub(super) fn open(path: &Path) -> Result<*mut c_void, String> {
        let c = CString::new(path.as_os_str().as_bytes())
            .map_err(|_| "the path contains a NUL character".to_string())?;
        // SAFETY: c is a valid C string; dlerror is read right after the failed call on the same thread.
        unsafe {
            let handle = dlopen(c.as_ptr(), RTLD_NOW);
            if handle.is_null() {
                let e = dlerror();
                return Err(if e.is_null() {
                    "dlopen failed".into()
                } else {
                    CStr::from_ptr(e).to_string_lossy().into_owned()
                });
            }
            Ok(handle)
        }
    }

    pub(super) fn symbol(handle: *mut c_void, name: &CStr) -> *mut c_void {
        // SAFETY: handle came from dlopen and is never closed.
        unsafe { dlsym(handle, name.as_ptr()) }
    }
}

#[cfg(windows)]
mod sys {
    use std::ffi::{CStr, c_char, c_void};
    use std::os::windows::ffi::OsStrExt;
    use std::path::Path;

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn LoadLibraryExW(name: *const u16, file: *mut c_void, flags: u32) -> *mut c_void;
        fn GetProcAddress(module: *mut c_void, name: *const c_char) -> *mut c_void;
        fn GetLastError() -> u32;
    }

    // Looks for the library's own dependencies next to it as well; they are all system DLLs today.
    const LOAD_WITH_ALTERED_SEARCH_PATH: u32 = 0x8;

    pub(super) fn open(path: &Path) -> Result<*mut c_void, String> {
        let wide: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
        // SAFETY: wide is NUL-terminated; GetLastError is read right after the failed call.
        unsafe {
            let module = LoadLibraryExW(
                wide.as_ptr(),
                std::ptr::null_mut(),
                LOAD_WITH_ALTERED_SEARCH_PATH,
            );
            if module.is_null() {
                return Err(format!(
                    "LoadLibrary failed with Windows error {}",
                    GetLastError()
                ));
            }
            Ok(module)
        }
    }

    pub(super) fn symbol(module: *mut c_void, name: &CStr) -> *mut c_void {
        // SAFETY: module came from LoadLibraryExW and is never freed.
        unsafe { GetProcAddress(module, name.as_ptr()) }
    }
}

#[cfg(not(any(unix, windows)))]
mod sys {
    use std::ffi::{CStr, c_void};
    use std::path::Path;

    pub(super) fn open(_: &Path) -> Result<*mut c_void, String> {
        Err("PdfPecker runs on Linux, macOS and Windows only".into())
    }

    pub(super) fn symbol(_: *mut c_void, _: &CStr) -> *mut c_void {
        std::ptr::null_mut()
    }
}
