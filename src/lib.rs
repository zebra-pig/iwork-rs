//! Read and write Apple iWork documents — Pages, Numbers and Keynote.
//!
//! All three apps share one format, in four layers:
//!
//! 1. a ZIP package whose entries are all *stored* ([`package`]);
//! 2. `Index/*.iwa` streams framed as raw Snappy blocks ([`iwa`]);
//! 3. a flat stream of length-delimited protobuf objects ([`iwa`], [`pb`]);
//! 4. an object graph whose shape depends on the app ([`document`]).
//!
//! Apple does not publish the `.proto` definitions and the message type is the
//! only thing identifying a payload's schema, so this crate works at the
//! protobuf **wire level**: objects are decoded into fields and re-encoded in
//! place. That is enough to inspect a document and rewrite parts of it without
//! knowing every message definition, and it means an unrecognised object is
//! carried through untouched rather than lost.
//!
//! ```no_run
//! # fn main() -> Result<(), iwork::Error> {
//! let mut doc = iwork::Document::open("Report.pages")?;
//! println!("{} document", doc.kind().as_str());
//! for storage in doc.text_storages() {
//!     println!("{}: {}", storage.identifier, storage.text);
//! }
//! doc.set_text(6083, "A new headline")?;
//! doc.save("Report-edited.pages")?;
//! # Ok(()) }
//! ```

pub mod document;
pub mod iwa;
pub mod package;
pub mod pb;
pub mod registry;
pub mod text;

pub use document::{Component, DataFile, Document, Kind, TextStorage};
pub use package::Package;

/// `TSWP.StorageArchive` — a run of styled text. Same in all three apps.
pub const TYPE_STORAGE: u32 = 2001;
/// `TSP.PackageMetadata` — the component and media index.
pub const TYPE_PACKAGE_METADATA: u32 = 11006;

#[derive(Debug)]
pub enum Error {
    Io(std::io::Error),
    Zip(zip::result::ZipError),
    /// The package parsed as a ZIP but its contents are not a document we
    /// understand. Carries a human-readable reason.
    Format(String),
    NoSuchObject(u64),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Io(e) => write!(f, "{e}"),
            Error::Zip(e) => write!(f, "{e}"),
            Error::Format(m) => write!(f, "{m}"),
            Error::NoSuchObject(id) => write!(f, "no text storage with identifier {id}"),
        }
    }
}

impl std::error::Error for Error {}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Error {
        Error::Io(e)
    }
}

impl From<zip::result::ZipError> for Error {
    fn from(e: zip::result::ZipError) -> Error {
        Error::Zip(e)
    }
}
