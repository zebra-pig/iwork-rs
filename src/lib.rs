//! Read and write Apple iWork documents — Pages, Numbers and Keynote.
//!
//! All three apps share one format, in four layers:
//!
//! 1. a ZIP package whose entries are all *stored* ([`package`]);
//! 2. `Index/*.iwa` streams framed as raw Snappy blocks ([`iwa`]);
//! 3. a flat stream of length-delimited protobuf objects ([`iwa`], [`pb`]);
//! 4. an object graph whose shape depends on the app ([`document`], [`style`]).
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
//!
//! // Text styles are shared objects a range of text points at ([`style`]).
//! for style in doc.text_styles() {
//!     println!("{} {} {:?}", style.identifier, style.kind.as_str(), style.name);
//! }
//! let kicker = doc.create_text_style(3712, "Kicker")?;
//! doc.set_text_style_property(
//!     kicker.identifier,
//!     iwork::style::property::FONT_SIZE,
//!     Some(iwork::pb::Value::Fixed32(18f32.to_le_bytes())),
//! )?;
//! doc.apply_text_style(6083, 0..8, kicker.identifier)?;
//!
//! doc.save("Report-edited.pages")?;
//! # Ok(()) }
//! ```

pub mod document;
pub mod drawable;
pub mod iwa;
pub mod media;
pub mod package;
pub mod pb;
pub mod registry;
pub mod style;
pub mod table;
pub mod text;

pub use document::{Component, DataFile, Document, Kind, TextStorage};
pub use drawable::{Drawable, Geometry, Placement};
pub use media::MediaReplacement;
pub use package::Package;
pub use style::{CreatedStyle, Label, StyleDeletion, StyleKind, StyleUse, TextStyle};
pub use table::{Cell, CellControl, CellFormat, CellValue, Merge, Table};

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
    NoSuchStyle(u64),
    /// A style could not be deleted because references to it would have been
    /// left dangling. Carries the objects that still refer to it.
    StyleInUse {
        identifier: u64,
        references: Vec<u64>,
    },
    /// An image's bytes were not replaced, because something between the stored
    /// pixels and the render was computed from the old ones — a crop, a shaped
    /// mask, an Instant Alpha path, tone adjustments, a cached rendering, a
    /// traced outline. Swapping the bytes under any of those leaves a document
    /// that opens, reports the same geometry, and draws the wrong thing.
    NonDestructiveEdit {
        drawable: u64,
        reasons: Vec<String>,
    },
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Io(e) => write!(f, "{e}"),
            Error::Zip(e) => write!(f, "{e}"),
            Error::Format(m) => write!(f, "{m}"),
            Error::NoSuchObject(id) => write!(f, "no object with identifier {id}"),
            Error::NoSuchStyle(id) => write!(f, "no text style with identifier {id}"),
            Error::NonDestructiveEdit { drawable, reasons } => write!(
                f,
                "drawable {drawable} carries edit state a replacement would falsify: {}",
                reasons.join("; ")
            ),
            Error::StyleInUse {
                identifier,
                references,
            } => {
                let list: Vec<String> = references.iter().map(u64::to_string).collect();
                write!(
                    f,
                    "text style {identifier} is still referenced by {} object(s): {}",
                    references.len(),
                    list.join(", ")
                )
            }
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
