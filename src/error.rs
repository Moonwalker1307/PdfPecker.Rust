use std::fmt;

/// The result type of every PdfPecker call.
pub type Result<T> = std::result::Result<T, Error>;

/// What kind of failure an [`Error`] is, so callers can tell failures apart without matching messages.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ErrorKind {
    /// An argument or option is not valid: a page past the end, a conformance level the call does not take.
    InvalidArgument,
    /// The input is not a readable PDF, image or font.
    Unreadable,
    /// The PDF is encrypted and no password, or a wrong one, was given.
    Password,
    /// A page, size or resource limit was reached, such as [`PdfOptions::max_pages`](crate::PdfOptions::max_pages).
    Limit,
    /// The call took longer than its timeout.
    Timeout,
    /// The conversion failed for another reason that PdfPecker reports.
    Failed,
    /// A file the document needs, such as a font, could not be read.
    Io,
    /// The native library could not be found or loaded, or does not fit this crate.
    Library,
    /// An unexpected failure inside the native library: a bug worth reporting.
    Internal,
}

/// A failed PdfPecker call: its [`ErrorKind`] and the message the library gave.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Error {
    kind: ErrorKind,
    message: String,
}

impl Error {
    pub(crate) fn new(kind: ErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }

    pub(crate) fn argument(message: impl Into<String>) -> Self {
        Self::new(ErrorKind::InvalidArgument, message)
    }

    // The statuses of pdfpecker_invoke, as PdfPecker.Python's native/Exports.cs defines them.
    pub(crate) fn from_status(status: i32, message: String) -> Self {
        let kind = match status {
            1 => ErrorKind::InvalidArgument,
            2 => ErrorKind::Unreadable,
            3 => ErrorKind::Password,
            4 => ErrorKind::Limit,
            5 => ErrorKind::Timeout,
            6 => ErrorKind::Failed,
            7 => ErrorKind::Io,
            _ => ErrorKind::Internal,
        };
        Self::new(kind, message)
    }

    /// What kind of failure this is.
    pub fn kind(&self) -> ErrorKind {
        self.kind
    }

    /// The message, as the library wrote it.
    pub fn message(&self) -> &str {
        &self.message
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for Error {}

impl From<std::io::Error> for Error {
    /// Lets `?` turn a failed file read or write around a conversion into an [`ErrorKind::Io`] error.
    fn from(e: std::io::Error) -> Self {
        Self::new(ErrorKind::Io, e.to_string())
    }
}
