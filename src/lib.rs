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
//! // Editing text remaps everything anchored into the storage — style runs,
//! // hyperlinks, list levels, anchored drawables ([`text`]). Indices are
//! // UTF-16 code units.
//! doc.insert_text(6083, 12, "eingeschoben ")?;
//! doc.delete_text(6083, 40..55)?;
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

pub use document::{Component, DataFile, Document, Kind, TextEdit, TextStorage};
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
    /// A character index was outside the storage's text.
    TextRange {
        storage: u64,
        index: u64,
        length: u64,
    },
    /// A character index landed between the halves of a surrogate pair —
    /// inside an emoji, in other words. Indices count UTF-16 code units, and an
    /// edit at half a character would leave two unpaired surrogates.
    SplitSurrogate {
        storage: u64,
        index: u64,
    },
    /// The deleted range covered the character an object is anchored to: the
    /// `U+FFFC` an image or a footnote mark stands in, the `U+0004` a section
    /// begins after. Deleting it means deleting the object — Pages does exactly
    /// that, removing the drawable, its mask, its z-order entry and its media
    /// registration — and this crate will not, so it refuses.
    AnchoredObject {
        storage: u64,
        index: u64,
        table: &'static str,
        object: Option<u64>,
    },
    /// The storage carries a length-delimited field that is not one of the
    /// attribute tables this crate knows. It may well be one, and remapping it
    /// by guesswork is how an edit silently damages a document.
    UnknownAttributeTable {
        storage: u64,
        field: u32,
    },
    /// Text to be written contains a character that only means something with
    /// an object behind it — see [`text::UNWRITABLE`].
    UnwritableCharacter {
        character: char,
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
            Error::TextRange {
                storage,
                index,
                length,
            } => write!(
                f,
                "storage {storage}: character {index} is outside its text, which is \
                 {length} UTF-16 code unit(s) long"
            ),
            Error::SplitSurrogate { storage, index } => write!(
                f,
                "storage {storage}: character {index} is the second half of a surrogate \
                 pair — indices count UTF-16 code units and an edit may not split one"
            ),
            Error::AnchoredObject {
                storage,
                index,
                table,
                object,
            } => write!(
                f,
                "storage {storage}: character {index} anchors {} in {table}, and deleting \
                 it means deleting that object from the whole document — which this crate \
                 does not do",
                match object {
                    Some(id) => format!("object {id}"),
                    None => "something".to_string(),
                }
            ),
            Error::UnknownAttributeTable { storage, field } => write!(
                f,
                "storage {storage}: field {field} is not an attribute table this crate \
                 knows, and an edit would have to guess how its entries are anchored"
            ),
            Error::UnwritableCharacter { character } => write!(
                f,
                "U+{:04X} stands for an object rather than for itself, and this crate \
                 will not write one into text",
                *character as u32
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
