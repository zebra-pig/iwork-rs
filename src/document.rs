//! Document-level view over a package: components, media, text and styles.

use std::collections::{BTreeMap, BTreeSet};
use std::ops::Range;

use crate::iwa::{self, ArchiveObject};
use crate::package::Package;
use crate::pb::{Message, Value};
use crate::style::{self, CreatedStyle, StyleDeletion, StyleKind, StyleUse, TextStyle};
use crate::table::{cell_type, CellValue};
use crate::text;
use crate::Error;

/// Which app wrote the document.
///
/// All three share the container, the IWA framing and most object types; the
/// kind only affects which document-level archives are present.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Pages,
    Numbers,
    Keynote,
    Unknown,
}

impl Kind {
    pub fn from_extension(path: &std::path::Path) -> Kind {
        match path
            .extension()
            .and_then(|e| e.to_str())
            .map(str::to_ascii_lowercase)
            .as_deref()
        {
            Some("pages") => Kind::Pages,
            Some("numbers") => Kind::Numbers,
            Some("key") => Kind::Keynote,
            _ => Kind::Unknown,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Kind::Pages => "Pages",
            Kind::Numbers => "Numbers",
            Kind::Keynote => "Keynote",
            Kind::Unknown => "unknown",
        }
    }
}

/// One entry of `TSP.PackageMetadata.components`.
#[derive(Debug, Clone)]
pub struct Component {
    pub identifier: u64,
    /// `preferred_name`, e.g. `"Document"` or `"Tables/DataList"`.
    pub preferred_name: String,
    /// `file_name` when the component has its own stream, e.g.
    /// `"Tables/DataList-874443-2"`. Empty when it shares the stream named by
    /// `preferred_name`.
    pub file_name: String,
    /// Number of `external_references` entries — a rough measure of how central
    /// the component is to the document.
    pub external_reference_count: usize,
}

impl Component {
    /// Package entry holding this component's objects.
    ///
    /// Verified against a 96-component Numbers document: every component
    /// resolves as `Index/{file_name or preferred_name}.iwa`.
    pub fn stream_name(&self) -> String {
        let stem = if self.file_name.is_empty() {
            &self.preferred_name
        } else {
            &self.file_name
        };
        format!("Index/{stem}.iwa")
    }
}

/// One entry of `TSP.PackageMetadata.datas` — the media registry.
#[derive(Debug, Clone)]
pub struct DataFile {
    /// Identifier drawables use to refer to this media. Not a package object id.
    pub identifier: u64,
    /// Name the file had when it was placed into the document.
    pub original_name: String,
    /// Name under `Data/`. Empty for assets that live in the app's own theme
    /// bundle and are therefore not copied into the package.
    pub stored_name: String,
    /// Path within the app's theme bundle, when this is a theme asset.
    pub asset_path: String,
    /// 20-byte digest iWork uses to deduplicate media.
    pub digest: Vec<u8>,
}

impl DataFile {
    /// Package entry holding the bytes, if they are stored in the package.
    pub fn entry_name(&self) -> Option<String> {
        (!self.stored_name.is_empty()).then(|| format!("Data/{}", self.stored_name))
    }
}

/// Which component owns what, and what each component already declares —
/// everything [`Document::undeclared_references`] needs, gathered in one pass.
#[derive(Default)]
struct ComponentIndex {
    /// Package entry name to the component stored there.
    by_stream: BTreeMap<String, u64>,
    /// Object identifier to the component it lives in.
    by_object: BTreeMap<u64, u64>,
    /// Component identifiers, which are also their root objects' identifiers.
    roots: BTreeSet<u64>,
    /// Objects each component declares in its `external_references`.
    declared: BTreeMap<u64, BTreeSet<u64>>,
}

/// Everything [`Document::set_cell`] has to find before it can write a byte.
///
/// A cell is addressed by row and column, but it is *stored* as a slice of one
/// tile's row buffer, and everything it says about itself is a key into a side
/// table that lives in another stream again. Gathering the addresses first
/// keeps the write itself short enough to read.
struct CellSite {
    /// `TST.Tile` holding the row.
    tile: u64,
    /// Index of the row's `TileRowInfo` **among the tile's fields**, not its
    /// row number: the field is what gets replaced.
    tile_row: usize,
    /// Whether this row's offsets count groups of four bytes.
    wide: bool,
    row: usize,
    column: usize,
    /// Every cell of the row, one slot per offset entry.
    records: Vec<Option<Vec<u8>>>,
    /// The target cell's bytes, if it has any.
    record: Option<Vec<u8>>,
    /// `TableDataList` for interned strings, and for data formats.
    strings: Option<u64>,
    formats: Option<u64>,
    /// `TST.HeaderStorageBucket`s carrying the per-row and per-column cell
    /// counts, which change when a cell appears or disappears.
    row_bucket: Vec<u64>,
    column_bucket: Option<u64>,
}

/// `TSWP.HyperlinkFieldArchive` — `{1: {1: uuid}, 2: url}`.
pub const TYPE_HYPERLINK_FIELD: u32 = 2032;

/// One attribute table of one storage, as [`Document::storages`] found it.
#[derive(Debug, Clone)]
pub struct StorageTable {
    pub field: u32,
    pub name: &'static str,
    pub anchoring: crate::text::Anchoring,
    pub entries: usize,
}

/// A `TSWP.StorageArchive` and everything anchored into it.
#[derive(Debug, Clone)]
pub struct StorageInventory {
    pub identifier: u64,
    pub stream: String,
    /// `StorageArchive.kind`: 0 body, 1 header, 2 footnote, 3 text box, 4 note,
    /// 5 cell, 6 unclassified, 7 table of contents, 8 undefined. The default is
    /// 3, so a storage with no field 1 is a text box.
    pub kind: u64,
    /// Length of the text in UTF-16 code units.
    pub length: u64,
    pub paragraphs: usize,
    pub tables: Vec<StorageTable>,
    /// A length-delimited field that is not a table this crate knows — the
    /// reason an edit on this storage would be refused.
    pub unknown_field: Option<u32>,
}

impl StorageInventory {
    pub fn kind_name(&self) -> &'static str {
        match self.kind {
            0 => "body",
            1 => "header",
            2 => "footnote",
            3 => "text box",
            4 => "note",
            5 => "cell",
            6 => "unclassified",
            7 => "table of contents",
            _ => "undefined",
        }
    }
}

/// A hyperlink or other smart field, and the text it covers.
#[derive(Debug, Clone)]
pub struct SmartField {
    pub storage: u64,
    /// The field archive — 2031 placeholder, 2032 hyperlink, 2034 date and
    /// time, 2036 mail merge, and the rest of 2031–2042.
    pub object: u64,
    pub message_type: u32,
    /// Characters the field covers, in UTF-16 code units.
    pub range: Range<u64>,
    /// Field 2 of the archive. On a hyperlink (2032) it is the **URL**; on a
    /// placeholder (2031) there is none; on a mail-merge field (2036) it is the
    /// contacts property the field stands for, which is why this is not called
    /// `url`.
    pub payload: Option<String>,
    pub text: String,
}

/// One paragraph's list state — see [`Document::list_paragraphs`].
#[derive(Debug, Clone)]
pub struct ListParagraph {
    pub range: Range<u64>,
    /// Indent depth, counted from 0. Stored as `first` in the paragraph-data
    /// table.
    pub level: u64,
    /// `TSWP.ListStyleArchive` in force, if the storage names one.
    pub style: Option<u64>,
    pub text: String,
}

/// The style over a run, resolved — see [`Document::style_of_run`].
#[derive(Debug, Clone)]
pub struct ResolvedStyle {
    /// The object the run points at, named or not.
    pub style: u64,
    /// Nearest ancestor with a name, which is the style a user would say the
    /// text is in.
    pub named: Option<u64>,
    pub name: Option<String>,
    /// Set on the anonymous archives iWork writes for directly formatted text.
    pub is_variation: bool,
    /// `override_count`, field 10 — how many properties the archive claims to
    /// override. Not maintained by this crate, and reported as it is found.
    pub override_count: Option<u64>,
    /// Properties set anywhere between the run and its named style, named where
    /// this crate knows the field and as `bag.field` where it does not.
    pub overrides: Vec<String>,
}

/// What one text edit did — see [`Document::replace_text`].
#[derive(Debug, Clone)]
pub struct TextEdit {
    pub storage: u64,
    /// Where it happened and how much moved, in UTF-16 code units.
    pub edit: crate::text::Edit,
    /// What became of the attribute tables.
    pub report: crate::text::EditReport,
}

/// A run of styled text, held by `TSWP.StorageArchive`.
#[derive(Debug, Clone)]
pub struct TextStorage {
    /// Object identifier — the handle `Document::set_text` takes.
    pub identifier: u64,
    /// Stream the object lives in.
    pub stream: String,
    pub text: String,
}

/// An open iWork document.
///
/// Object streams are decoded on open and re-encoded on save, so a
/// `open` → `save` cycle reproduces every object byte for byte.
pub struct Document {
    package: Package,
    kind: Kind,
    /// Decoded object streams, keyed by package entry name.
    streams: BTreeMap<String, Vec<ArchiveObject>>,
}

impl Document {
    pub fn open(path: impl AsRef<std::path::Path>) -> Result<Document, Error> {
        let path = path.as_ref();
        let package = Package::read(path)?;
        let mut document = Document::from_package(package)?;
        // Trust the extension when there is one; fall back to structure.
        let by_extension = Kind::from_extension(path);
        if by_extension != Kind::Unknown {
            document.kind = by_extension;
        }
        Ok(document)
    }

    pub fn from_package(package: Package) -> Result<Document, Error> {
        let mut streams = BTreeMap::new();
        for name in package.iwa_names() {
            let raw = package.get(&name).expect("name came from the package");
            let objects = iwa::parse(raw).map_err(|e| Error::Format(format!("{name}: {e}")))?;
            streams.insert(name, objects);
        }
        let kind = detect_kind(&package, &streams);
        Ok(Document {
            package,
            kind,
            streams,
        })
    }

    pub fn kind(&self) -> Kind {
        self.kind
    }

    pub fn package(&self) -> &Package {
        &self.package
    }

    pub fn stream_names(&self) -> impl Iterator<Item = &str> {
        self.streams.keys().map(String::as_str)
    }

    pub fn objects(&self) -> impl Iterator<Item = (&str, &ArchiveObject)> {
        self.streams
            .iter()
            .flat_map(|(name, objects)| objects.iter().map(move |o| (name.as_str(), o)))
    }

    pub fn object(&self, identifier: u64) -> Option<(&str, &ArchiveObject)> {
        self.objects().find(|(_, o)| o.identifier == identifier)
    }

    /// The `TSP.PackageMetadata` object, which indexes components and media.
    fn package_metadata(&self) -> Option<Message> {
        for (_, object) in self.objects() {
            if object.message_type() == crate::TYPE_PACKAGE_METADATA {
                return Message::decode(object.payload()).ok();
            }
        }
        None
    }

    pub fn components(&self) -> Vec<Component> {
        let Some(meta) = self.package_metadata() else {
            return Vec::new();
        };
        meta.all(3)
            .filter_map(|value| {
                let Value::Bytes(raw) = value else {
                    return None;
                };
                let c = Message::decode(raw).ok()?;
                Some(Component {
                    identifier: c.varint(1).unwrap_or(0),
                    preferred_name: utf8(c.bytes(2)),
                    file_name: utf8(c.bytes(3)),
                    external_reference_count: c.all(6).count(),
                })
            })
            .collect()
    }

    pub fn data_files(&self) -> Vec<DataFile> {
        let Some(meta) = self.package_metadata() else {
            return Vec::new();
        };
        meta.all(4)
            .filter_map(|value| {
                let Value::Bytes(raw) = value else {
                    return None;
                };
                let d = Message::decode(raw).ok()?;
                Some(DataFile {
                    identifier: d.varint(1).unwrap_or(0),
                    digest: d.bytes(2).unwrap_or_default().to_vec(),
                    original_name: utf8(d.bytes(3)),
                    stored_name: utf8(d.bytes(4)),
                    asset_path: utf8(d.bytes(5)),
                })
            })
            .collect()
    }

    /// Objects that carry a version patch — `MessageInfo.type == 0`.
    ///
    /// The archive format lets one object hold its message **and older
    /// encodings of it as patches**: a first message of the real type, then
    /// further messages of type `0`, each naming a `base_message_index`, an app
    /// version it is meant for, and the fields to drop from the base before
    /// merging. The first message is the newest encoding and the one the
    /// current app reads; the patches exist so that an older Numbers opening
    /// the same file gets a shape it understands.
    ///
    /// **What 15.3.1 actually writes, over this whole corpus and after an edit
    /// made by the app itself: exactly one patched object per Numbers document,
    /// and none at all in Pages or Keynote.** It is the `TN.UIStateArchive` in
    /// the view-state component, with three patches for 11.0, 10.1 and 10.0,
    /// each dropping field 28 from the base and supplying its own. No table
    /// archive — no tile, no data list, no model — carries one.
    ///
    /// So the rule this crate follows is short. Read the first message and
    /// ignore the patches, which is what the app does. Never write one. And
    /// **never rewrite the first message of an object that has them**, because
    /// the patches would then describe the object as it used to be, which is a
    /// document that says two different things depending on who opens it.
    /// [`Document::set_cell`] refuses on that ground rather than silently
    /// producing one.
    pub fn patched_objects(&self) -> Vec<(u64, usize)> {
        self.objects()
            .filter_map(|(_, object)| {
                let patches = object
                    .messages
                    .iter()
                    .filter(|m| m.message_type == 0)
                    .count();
                (patches > 0).then_some((object.identifier, patches))
            })
            .collect()
    }

    /// Highest object identifier the document has ever allocated.
    ///
    /// New objects must take identifiers above this, and the field must be
    /// bumped to match, or iWork will hand out identifiers that already exist.
    pub fn last_object_identifier(&self) -> Option<u64> {
        self.package_metadata()?.varint(1)
    }

    /// Every non-empty text storage in the document, in stream order.
    pub fn text_storages(&self) -> Vec<TextStorage> {
        let mut out = Vec::new();
        for (stream, object) in self.objects() {
            if object.message_type() != crate::TYPE_STORAGE {
                continue;
            }
            let Ok(storage) = Message::decode(object.payload()) else {
                continue;
            };
            let text = text::read(&storage);
            if !text::has_content(&text) {
                continue;
            }
            out.push(TextStorage {
                identifier: object.identifier,
                stream: stream.to_string(),
                text,
            });
        }
        out
    }

    /// Replace the contents of one text storage.
    ///
    /// A full-range [`Document::replace_text`], which is what "select all and
    /// type" is. The first paragraph keeps its style and the paragraphs of the
    /// new text are given entries of their own where the storage had one per
    /// paragraph before; nothing is left pointing at a character that is not
    /// there any more.
    pub fn set_text(&mut self, identifier: u64, new_text: &str) -> Result<TextEdit, Error> {
        let length = text::length(&self.storage_text(identifier)?);
        self.replace_text(identifier, 0..length, new_text)
    }

    /// Insert text at a character index, moving everything anchored past it.
    ///
    /// `at` counts **UTF-16 code units**, like every character index in the
    /// format; see [`Document::replace_text`].
    pub fn insert_text(
        &mut self,
        identifier: u64,
        at: u64,
        new_text: &str,
    ) -> Result<TextEdit, Error> {
        self.replace_text(identifier, at..at, new_text)
    }

    /// Delete a range of a storage's text, moving everything anchored past it
    /// and dropping what was inside it.
    ///
    /// `range` counts **UTF-16 code units**; see [`Document::replace_text`].
    pub fn delete_text(&mut self, identifier: u64, range: Range<u64>) -> Result<TextEdit, Error> {
        self.replace_text(identifier, range, "")
    }

    /// Replace `range` of a storage's text with `new_text`, remapping every
    /// attribute table the storage carries.
    ///
    /// **Indices are UTF-16 code units**, which is what the format counts:
    /// `"🎬"` is two, and an index between its halves is
    /// [`Error::SplitSurrogate`] rather than a corrupted string.
    ///
    /// Everything anchored into the storage moves with the text — paragraph,
    /// list, character and layout styles, smart fields and hyperlinks, language
    /// and dictation runs, tracked insertions and deletions, comment and pencil
    /// annotations, bookmarks, drop caps, and the paragraph-start bookkeeping.
    /// [`crate::text::TABLES`] is the whole inventory and [`crate::text::Edit`]
    /// is what each kind does. Where the rules came from — a probe per rule,
    /// with Pages performing the edit — is in `FORMAT.md` §Text.
    ///
    /// What it refuses rather than damaging:
    ///
    /// * a range covering the character an object is anchored to
    ///   ([`Error::AnchoredObject`]) — an image, a footnote mark, a section
    ///   break. Pages deletes the object; this crate cannot, so it declines;
    /// * a storage carrying a table this crate does not know
    ///   ([`Error::UnknownAttributeTable`]);
    /// * text containing `U+FFFC`, `U+0004` or `U+0005`
    ///   ([`Error::UnwritableCharacter`]), which stand for objects;
    /// * an index inside a surrogate pair, or outside the text.
    pub fn replace_text(
        &mut self,
        identifier: u64,
        range: Range<u64>,
        new_text: &str,
    ) -> Result<TextEdit, Error> {
        if let Some(character) = new_text.chars().find(|c| text::UNWRITABLE.contains(c)) {
            return Err(Error::UnwritableCharacter { character });
        }

        for (name, objects) in self.streams.iter_mut() {
            for object in objects.iter_mut() {
                if object.identifier != identifier || object.message_type() != crate::TYPE_STORAGE {
                    continue;
                }
                let mut storage = Message::decode(object.payload())
                    .map_err(|e| Error::Format(format!("{name}: storage {identifier}: {e}")))?;
                if let Some(field) = text::unknown_table(&storage) {
                    return Err(Error::UnknownAttributeTable {
                        storage: identifier,
                        field,
                    });
                }

                let old = text::read(&storage);
                let length = text::length(&old);
                let (start, end) = (range.start, range.end.max(range.start));
                if end > length {
                    return Err(Error::TextRange {
                        storage: identifier,
                        index: end,
                        length,
                    });
                }
                let (Some(from), Some(to)) = (
                    text::utf16_offset(&old, start),
                    text::utf16_offset(&old, end),
                ) else {
                    let index = if text::utf16_offset(&old, start).is_none() {
                        start
                    } else {
                        end
                    };
                    return Err(Error::SplitSurrogate {
                        storage: identifier,
                        index,
                    });
                };

                let edit = text::Edit {
                    at: start,
                    removed: end - start,
                    inserted: text::length(new_text),
                };
                if let Some((field, index, target)) =
                    text::destroyed_anchors(&storage, &old, edit).first()
                {
                    return Err(Error::AnchoredObject {
                        storage: identifier,
                        index: *index,
                        table: text::table(*field)
                            .map(|t| t.name)
                            .unwrap_or("an anchor table"),
                        object: *target,
                    });
                }

                let mut text = String::with_capacity(old.len() - (to - from) + new_text.len());
                text.push_str(&old[..from]);
                text.push_str(new_text);
                text.push_str(&old[to..]);

                let report = text::apply(&mut storage, edit, &text);
                let payload = storage.encode();
                if payload != object.payload() {
                    object.messages[0].payload = payload;
                }
                return Ok(TextEdit {
                    storage: identifier,
                    edit,
                    report,
                });
            }
        }
        Err(Error::NoSuchObject(identifier))
    }

    // -- what a storage carries ----------------------------------------------

    /// Every `TSWP.StorageArchive` in the document, with the attribute tables it
    /// carries — the inventory an edit has to remap.
    ///
    /// Unlike [`Document::text_storages`] this reports the placeholders too, and
    /// the storages whose whole contents are a `U+FFFC`, because those are
    /// exactly the ones with something anchored into them.
    pub fn storages(&self) -> Vec<StorageInventory> {
        let mut out = Vec::new();
        for (stream, object) in self.objects() {
            if object.message_type() != crate::TYPE_STORAGE {
                continue;
            }
            let Ok(storage) = Message::decode(object.payload()) else {
                continue;
            };
            let text = text::read(&storage);
            let mut tables = Vec::new();
            for spec in text::TABLES {
                let Some(decoded) = storage.bytes(spec.field).and_then(crate::pb::decode_nested)
                else {
                    continue;
                };
                tables.push(StorageTable {
                    field: spec.field,
                    name: spec.name,
                    anchoring: spec.anchoring,
                    entries: text::entry_indices(&decoded, spec.anchoring).len(),
                });
            }
            out.push(StorageInventory {
                identifier: object.identifier,
                stream: stream.to_string(),
                kind: storage.varint(1).unwrap_or(3),
                length: text::length(&text),
                paragraphs: text::paragraph_ranges(&text).len(),
                unknown_field: text::unknown_table(&storage),
                tables,
            });
        }
        out
    }

    /// Every smart field in the document — hyperlinks above all, but also the
    /// placeholder, date, page-number and mail-merge fields that share the table.
    ///
    /// A smart field is a *run*: `StorageArchive` field 11 holds an entry
    /// carrying the field's object at the character it starts on, and the run
    /// reaches to the next entry — which for a field that does not run to the
    /// end of the text is an entry with **no object**, a terminator. Both shapes
    /// occur in Apple's own templates, and one link in `46_Business_Modern…`
    /// runs to the end with no terminator at all.
    pub fn smart_fields(&self) -> Vec<SmartField> {
        let mut out = Vec::new();
        for (_, object) in self.objects() {
            if object.message_type() != crate::TYPE_STORAGE {
                continue;
            }
            let Ok(storage) = Message::decode(object.payload()) else {
                continue;
            };
            let Some(table) = storage.bytes(11).and_then(crate::pb::decode_nested) else {
                continue;
            };
            let text = text::read(&storage);
            let length = text::length(&text);
            let entries = text::entry_indices(&table, text::Anchoring::Run);
            for (position, (index, target)) in entries.iter().enumerate() {
                let Some(target) = target else { continue };
                let end = entries
                    .get(position + 1)
                    .map(|(next, _)| *next)
                    .unwrap_or(length);
                let (_, field) = self.object(*target).unzip();
                let archive = field.and_then(|o| Message::decode(o.payload()).ok());
                out.push(SmartField {
                    storage: object.identifier,
                    object: *target,
                    message_type: field.map(|o| o.message_type()).unwrap_or(0),
                    range: *index..end,
                    payload: archive
                        .as_ref()
                        .and_then(|a| a.bytes(2))
                        .map(|b| String::from_utf8_lossy(b).into_owned()),
                    text: slice(&text, *index..end),
                });
            }
        }
        out
    }

    /// Point a hyperlink at a different address.
    ///
    /// `TSWP.HyperlinkFieldArchive` (2032) is `{1: {1: uuid}, 2: url}` — the
    /// whole of the link is that one string, so changing it is changing one
    /// field of one object and nothing else. What the *text* says is separate
    /// and stays as it was; a link reading "example.com" and pointing at
    /// somewhere else is a document the app writes happily.
    pub fn set_link_url(&mut self, identifier: u64, url: &str) -> Result<(), Error> {
        for (name, objects) in self.streams.iter_mut() {
            for object in objects.iter_mut() {
                if object.identifier != identifier {
                    continue;
                }
                if object.message_type() != TYPE_HYPERLINK_FIELD {
                    return Err(Error::Format(format!(
                        "object {identifier} is a {} archive, not a hyperlink (2032)",
                        object.message_type()
                    )));
                }
                let mut archive = Message::decode(object.payload())
                    .map_err(|e| Error::Format(format!("{name}: link {identifier}: {e}")))?;
                archive.set_in_order(2, Value::Bytes(url.as_bytes().to_vec()));
                object.messages[0].payload = archive.encode();
                return Ok(());
            }
        }
        Err(Error::NoSuchObject(identifier))
    }

    /// The list state of every paragraph of a storage: its level, and the list
    /// style in force.
    ///
    /// Two tables together say it, keyed on the same paragraph starts. Field 6
    /// (`table_para_data`) carries `{character_index, first, second}` and
    /// **`first` is the level**, counted from 0; field 7 (`table_list_style`)
    /// points at the `TSWP.ListStyleArchive`. Both are sparse — a paragraph with
    /// no entry keeps whatever the paragraph before it had — which is why they
    /// are read forward rather than looked up.
    ///
    /// Read off Apple's own templates: `60_Academic_Modern_PM` has one storage
    /// whose five paragraphs are levels 0 to 4 with a style override each, and
    /// `04_Real_Estate_Flyer` has three named list styles and a level change in
    /// one storage of fourteen paragraphs.
    pub fn list_paragraphs(&self, identifier: u64) -> Result<Vec<ListParagraph>, Error> {
        let storage = self.archive_of(identifier)?;
        let text = text::read(&storage);
        let levels = storage
            .bytes(6)
            .and_then(crate::pb::decode_nested)
            .map(|t| text::para_data(&t))
            .unwrap_or_default();
        let styles = storage
            .bytes(7)
            .and_then(crate::pb::decode_nested)
            .map(|t| text::entry_indices(&t, text::Anchoring::Paragraph))
            .unwrap_or_default();

        // Both tables are sparse and both carry forward: a paragraph with no
        // entry, and a paragraph whose entry names nothing, keep what the
        // paragraph before them had.
        let carried = |entries: &[(u64, Option<u64>)], at: u64| {
            entries
                .iter()
                .rev()
                .find(|(index, value)| *index <= at && value.is_some())
                .and_then(|(_, value)| *value)
        };
        Ok(text::paragraph_ranges(&text)
            .into_iter()
            .map(|range| ListParagraph {
                level: carried(&levels, range.start).unwrap_or(0),
                style: carried(&styles, range.start),
                text: slice(&text, range.clone()),
                range,
            })
            .collect())
    }

    /// The style in force over one character, resolved to the named style it
    /// comes from and what the run overrides on top of it.
    ///
    /// Most runs in a real document do not point at a named style. They point at
    /// a **variation**: an anonymous archive carrying [`style::IS_VARIATION`], a
    /// parent, and a property bag holding only what differs. That is what iWork
    /// writes when text is formatted directly rather than by picking a style
    /// from the list, and it is why editing "Title" can leave the title looking
    /// exactly as it did.
    ///
    /// This walks the parent chain to the first archive with a name and reports
    /// both ends: which named style the run ultimately is, and which properties
    /// the chain below it sets.
    pub fn style_of_run(
        &self,
        storage: u64,
        index: u64,
        kind: StyleKind,
    ) -> Result<Option<ResolvedStyle>, Error> {
        let archive = self.archive_of(storage)?;
        let Some(table) = archive
            .bytes(kind.attribute_table())
            .and_then(crate::pb::decode_nested)
        else {
            return Ok(None);
        };
        let anchoring = text::table(kind.attribute_table())
            .map(|t| t.anchoring)
            .unwrap_or(text::Anchoring::Run);
        let entries = text::entry_indices(&table, anchoring);
        // A paragraph entry with no object means "whatever was in force here"
        // — it is what Pages writes for a paragraph an edit has just created —
        // so resolving one walks back to the last entry that names a style. A
        // *run* entry with no object is a terminator and means exactly what it
        // says: no attribute from here on.
        let resolved = match anchoring {
            text::Anchoring::Paragraph => entries
                .iter()
                .rev()
                .find(|(at, target)| *at <= index && target.is_some()),
            _ => entries.iter().rev().find(|(at, _)| *at <= index),
        };
        let Some(identifier) = resolved.and_then(|(_, target)| *target) else {
            return Ok(None);
        };

        // What the *variations* set on the way down, and nothing from the named
        // style itself — a named style's bags carry every property it has,
        // defaults included, and calling those overrides would say a paragraph
        // overrides sixty things when it overrides one.
        let mut overrides: Vec<String> = Vec::new();
        let mut named = None;
        let mut name = None;
        let mut walker = Some(identifier);
        let mut seen = 0;
        while let (Some(current), true) = (walker, seen < 16) {
            seen += 1;
            let Some(style) = self.text_style(current) else {
                break;
            };
            if style.name.is_some() {
                named = Some(current);
                name = style.name.clone();
                break;
            }
            for bag in [11u32, 12u32] {
                let Some(properties) = style.archive.bytes(bag).and_then(crate::pb::decode_nested)
                else {
                    continue;
                };
                for field in &properties.fields {
                    let path = [bag, field.number];
                    let label = style::property::BY_NAME
                        .iter()
                        .find(|(_, known, _)| *known == path)
                        .map(|(label, _, _)| label.to_string())
                        .unwrap_or_else(|| format!("{bag}.{}", field.number));
                    if !overrides.contains(&label) {
                        overrides.push(label);
                    }
                }
            }
            walker = style.parent;
        }

        let archive = self.archive_of(identifier)?;
        Ok(Some(ResolvedStyle {
            style: identifier,
            named,
            name,
            is_variation: style::get_path(&archive, style::IS_VARIATION)
                .is_some_and(|v| v != Value::Varint(0)),
            override_count: match style::get_path(&archive, style::OVERRIDE_COUNT) {
                Some(Value::Varint(n)) => Some(n),
                _ => None,
            },
            overrides,
        }))
    }

    // -- tables --------------------------------------------------------------

    /// Every table in the document, in object order.
    ///
    /// Tables are a cross-app archive: Numbers puts them on sheets, Pages on
    /// pages and Keynote on slides, and all three write the same `TST` object
    /// graph. See [`crate::table`] for what a table is made of.
    ///
    /// ```no_run
    /// # fn main() -> Result<(), iwork::Error> {
    /// # let doc = iwork::Document::open("Budget.numbers")?;
    /// for table in doc.tables() {
    ///     println!("{} — {}×{}", table.name, table.rows, table.columns);
    ///     println!("{}", table.value(1, 0).to_text());
    /// }
    /// # Ok(()) }
    /// ```
    pub fn tables(&self) -> Vec<crate::table::Table> {
        let mut tables = crate::table::tables(self);
        for table in &mut tables {
            crate::table::resolve_rich_text(self, table);
        }
        tables
    }

    /// One table by its `TST.TableInfoArchive` identifier, or by name.
    pub fn table(&self, wanted: &str) -> Option<crate::table::Table> {
        let by_id: Option<u64> = wanted.parse().ok();
        self.tables()
            .into_iter()
            .find(|t| Some(t.identifier) == by_id || t.name == wanted)
    }

    /// Every named cell format the document defines.
    ///
    /// Custom formats are document-scoped, not table-scoped: cells across every
    /// table reach into one list by UUID. See [`crate::table::CustomFormat`].
    pub fn custom_formats(&self) -> Vec<crate::table::CustomFormat> {
        crate::table::custom_formats(self)
    }

    /// Write a value into a cell that already exists.
    ///
    /// The narrow, provable operation: the grid keeps its shape, every row and
    /// column keeps its identity, and nothing that indexes rows by position —
    /// a category's group nodes, a filter's hidden state — has anything to
    /// re-point. What moves is the tile's storage for one row, and with it the
    /// interned string and format entries the cell reaches into.
    ///
    /// What travels with the value:
    ///
    /// * the cell's **data format**, because value and format travel together.
    ///   A cell that keeps its type keeps its format key untouched; a cell that
    ///   changes type is given the key another cell of the new type already
    ///   uses in this table, and its old slot's key is released — which is what
    ///   Numbers 15.3.1 does, observed by writing a number over a text cell.
    /// * everything this crate does not model. Bytes 2–5, byte 6's
    ///   chosen-format bits, byte 7, any trailing bytes, the cell and text
    ///   style keys, the control definition and — the one that would be
    ///   invisible until someone looked at the document — the **conditional
    ///   style and rule keys**, which are how a highlighted cell knows it is
    ///   highlighted.
    ///
    /// What it refuses, by name rather than by writing something plausible:
    /// a cell holding a formula (removing one means editing `TSCE`), a
    /// rich-text cell (its text is a `TSWP` storage, not a table string), a
    /// cell covered by a merge, a row with no stored cells at all, and any
    /// object carrying version patches — see [`Document::patched_objects`].
    pub fn set_cell(
        &mut self,
        wanted: &str,
        row: usize,
        column: usize,
        value: CellValue,
    ) -> Result<CellValue, Error> {
        let table = self
            .table(wanted)
            .ok_or_else(|| Error::Format(format!("no table called '{wanted}'")))?;
        let where_ = format!("{} r{row}c{column}", table.name);
        if row >= table.rows || column >= table.columns {
            return Err(Error::Format(format!(
                "{where_}: the table is {}×{}",
                table.rows, table.columns
            )));
        }
        if let Some(merge) = table.merge_covering(row, column) {
            if (merge.row, merge.column) != (row, column) {
                return Err(Error::Format(format!(
                    "{where_}: covered by the merge that begins at row {} column {}",
                    merge.row, merge.column
                )));
            }
        }
        match value {
            CellValue::Empty
            | CellValue::Text(_)
            | CellValue::Number(_)
            | CellValue::Bool(_)
            | CellValue::Date(_)
            | CellValue::Duration(_) => {}
            other => {
                return Err(Error::Format(format!(
                    "{where_}: this crate does not write {} cells",
                    other.kind()
                )))
            }
        }

        let site = self.cell_site(&table, row, column)?;
        let previous = table.value(row, column);
        let old = match &site.record {
            Some(bytes) => crate::table::decode_cell(bytes)
                .map_err(|e| Error::Format(format!("{where_}: {e}")))?,
            None => crate::table::CellRecord {
                version: 5,
                ..crate::table::CellRecord::default()
            },
        };
        if old.formula_id.is_some() {
            return Err(Error::Format(format!(
                "{where_}: holds a formula, and taking one out means editing the \
                 calculation engine — not this phase's job"
            )));
        }
        if old.cell_type == cell_type::RICH_TEXT || old.rich_id.is_some() {
            return Err(Error::Format(format!(
                "{where_}: holds rich text, whose words are in a TSWP storage rather \
                 than in the table"
            )));
        }

        // Nothing to do, and nothing to damage.
        if site.record.is_none() && value == CellValue::Empty {
            return Ok(previous);
        }

        let record = self.rewrite_record(&table, &site, old, &value, &where_)?;
        self.store_record(&site, record, &where_)?;
        Ok(previous)
    }

    /// Where a cell's bytes live, and which side tables reach it.
    fn cell_site(
        &self,
        table: &crate::table::Table,
        row: usize,
        column: usize,
    ) -> Result<CellSite, Error> {
        let where_ = format!("{} r{row}c{column}", table.name);
        let model = self.archive_of(table.model)?;
        let store = model
            .bytes(4)
            .and_then(crate::pb::decode_nested)
            .ok_or_else(|| Error::Format(format!("{where_}: the model has no data store")))?;
        let list = |field: u32| store.bytes(field).and_then(crate::table::reference);

        let tiles = store
            .bytes(3)
            .and_then(crate::pb::decode_nested)
            .ok_or_else(|| Error::Format(format!("{where_}: the data store has no tiles")))?;
        let tile_size = tiles.varint(2).unwrap_or(256) as usize;
        let wanted_tile = row / tile_size;
        let tile = tiles
            .all(1)
            .filter_map(|value| match value {
                Value::Bytes(raw) => crate::pb::decode_nested(raw),
                _ => None,
            })
            .find(|entry| entry.varint(1).unwrap_or(0) as usize == wanted_tile)
            .and_then(|entry| entry.bytes(2).and_then(crate::table::reference))
            .ok_or_else(|| Error::Format(format!("{where_}: no tile covers row {row}")))?;
        if self.patched_objects().iter().any(|&(id, _)| id == tile) {
            return Err(Error::Format(format!(
                "{where_}: tile {tile} carries version patches, and rewriting it would \
                 leave them describing the cell as it used to be"
            )));
        }

        let archive = self.archive_of(tile)?;
        let tile_row = row % tile_size;
        let position = archive
            .fields
            .iter()
            .position(|field| {
                field.number == 5
                    && matches!(&field.value, Value::Bytes(raw)
                        if crate::pb::decode_nested(raw)
                            .and_then(|info| info.varint(1))
                            .unwrap_or(u64::MAX) as usize == tile_row)
            })
            .ok_or_else(|| {
                Error::Format(format!(
                    "{where_}: row {row} has no stored cells, and giving a row its first \
                     one is not implemented"
                ))
            })?;
        let Value::Bytes(raw) = &archive.fields[position].value else {
            unreachable!("the position was found by matching on Bytes")
        };
        let info = crate::pb::decode_nested(raw)
            .ok_or_else(|| Error::Format(format!("{where_}: row storage does not decode")))?;
        let (Some(buffer), Some(offsets)) = (info.bytes(6), info.bytes(7)) else {
            return Err(Error::Format(format!(
                "{where_}: row {row} has no version-5 cell storage"
            )));
        };
        let wide = archive.varint(8).unwrap_or(0) != 0 || info.varint(8).unwrap_or(0) != 0;

        let slots = offsets.len() / 2;
        if column >= slots {
            return Err(Error::Format(format!(
                "{where_}: the row's offset array names only {slots} columns"
            )));
        }
        let mut records: Vec<Option<Vec<u8>>> = vec![None; slots];
        for (at, bytes) in crate::table::row_cells(buffer, offsets, wide)
            .into_iter()
            .flatten()
        {
            records[at] = Some(bytes.to_vec());
        }
        let record = records[column].clone();

        Ok(CellSite {
            tile,
            tile_row: position,
            wide,
            records,
            record,
            column,
            row,
            strings: list(4),
            formats: list(22),
            row_bucket: store
                .bytes(1)
                .and_then(crate::pb::decode_nested)
                .map(|storage| {
                    storage
                        .all(2)
                        .filter_map(|value| match value {
                            Value::Bytes(raw) => crate::table::reference(raw),
                            _ => None,
                        })
                        .collect::<Vec<u64>>()
                })
                .unwrap_or_default(),
            column_bucket: store.bytes(2).and_then(crate::table::reference),
        })
    }

    /// Turn the old record into the new one, maintaining the side tables.
    fn rewrite_record(
        &mut self,
        table: &crate::table::Table,
        site: &CellSite,
        old: crate::table::CellRecord,
        value: &CellValue,
        where_: &str,
    ) -> Result<Option<crate::table::CellRecord>, Error> {
        use crate::table::FormatSlot;

        let mut record = old.clone();
        record.version = 5;
        record.decimal = None;
        record.double = None;
        record.seconds = None;
        record.string_id = None;
        record.rich_id = None;
        record.formula_error_id = None;

        let slot = match value {
            CellValue::Empty => None,
            CellValue::Text(_) => Some(FormatSlot::Text),
            CellValue::Number(_) => Some(FormatSlot::Number),
            CellValue::Bool(_) => Some(FormatSlot::Boolean),
            CellValue::Date(_) => Some(FormatSlot::Date),
            CellValue::Duration(_) => Some(FormatSlot::Duration),
            _ => unreachable!("set_cell rejected every other value"),
        };
        record.cell_type = match value {
            CellValue::Empty => cell_type::EMPTY,
            CellValue::Text(_) => cell_type::TEXT,
            CellValue::Number(_) => cell_type::NUMBER,
            CellValue::Bool(_) => cell_type::BOOL,
            CellValue::Date(_) => cell_type::DATE,
            CellValue::Duration(_) => cell_type::DURATION,
            _ => unreachable!("set_cell rejected every other value"),
        };
        match value {
            CellValue::Number(decimal) => record.decimal = Some(*decimal),
            CellValue::Bool(flag) => record.double = Some(if *flag { 1.0 } else { 0.0 }),
            CellValue::Duration(seconds) => record.double = Some(*seconds),
            CellValue::Date(seconds) => record.seconds = Some(*seconds),
            _ => {}
        }

        // The string table first, so that rewriting a cell with the text it
        // already held reuses its entry rather than dropping and re-adding it.
        if let CellValue::Text(text) = value {
            let list = site
                .strings
                .ok_or_else(|| Error::Format(format!("{where_}: the table has no string list")))?;
            record.string_id = Some(self.intern_list_string(list, text)?);
        }
        if let Some(key) = old.string_id {
            let list = site
                .strings
                .ok_or_else(|| Error::Format(format!("{where_}: the table has no string list")))?;
            self.release_list_entry(list, key)?;
        }

        // An emptied cell keeps no record at all — the app deletes it, and the
        // covered half of a merge shows the same thing: nothing about an empty
        // cell is written down.
        if slot.is_none() {
            if let (Some(list), Some(key)) = (site.formats, old.format_id_in_current()) {
                self.release_list_entry(list, key)?;
            }
            return Ok(None);
        }
        let slot = slot.expect("checked above");

        if record.format_id_in(slot).is_none() {
            let donor = table
                .cells()
                .iter()
                .filter_map(|cell| {
                    cell.record
                        .format_id_in(slot)
                        .map(|key| (cell.record.explicit_format().is_some(), key))
                })
                // A cell whose format nobody chose is the better loan: its key
                // names the plain format for the slot rather than, say, the
                // percentage a neighbour was given.
                .min()
                .map(|(_, key)| key)
                .ok_or_else(|| {
                    Error::Format(format!(
                        "{where_}: no cell in this table carries a {slot:?} format to copy, \
                         and one invented here would be a format the document never defined"
                    ))
                })?;
            let list = site
                .formats
                .ok_or_else(|| Error::Format(format!("{where_}: the table has no format list")))?;
            self.retain_list_entry(list, donor)?;
            record.set_format_id_in(slot, Some(donor));

            // The slot the cell used to be in loses its key, and byte 6 loses
            // the claim that the user chose that format. Observed: a text cell
            // written over with a number in Numbers 15.3.1 comes back carrying
            // the number key only.
            if let Some(previous) = old.current_format() {
                if previous != slot {
                    if let Some(key) = old.format_id_in(previous) {
                        self.release_list_entry(list, key)?;
                    }
                    record.set_format_id_in(previous, None);
                    record.forget_explicit_format(previous);
                }
            }
        }
        record.format_kind = Some(crate::table::format_kind_of(slot));
        Ok(Some(record))
    }

    /// Put the rewritten record back into its tile, and keep the cell counts.
    fn store_record(
        &mut self,
        site: &CellSite,
        record: Option<crate::table::CellRecord>,
        where_: &str,
    ) -> Result<(), Error> {
        let had = site.record.is_some();
        let has = record.is_some();
        let encoded = match record {
            Some(record) => Some(
                record
                    .encode()
                    .map_err(|e| Error::Format(format!("{where_}: {e}")))?,
            ),
            None => None,
        };
        let mut records = site.records.clone();
        records[site.column] = encoded;
        let (buffer, offsets) = crate::table::encode_row(&records, site.wide)
            .map_err(|e| Error::Format(format!("{where_}: {e}")))?;
        let cells = records.iter().filter(|r| r.is_some()).count() as u64;

        let mut tile = self.archive_of(site.tile)?;
        let Value::Bytes(raw) = &tile.fields[site.tile_row].value else {
            unreachable!("cell_site found this field by matching on Bytes")
        };
        let mut info = crate::pb::decode_nested(raw)
            .ok_or_else(|| Error::Format(format!("{where_}: row storage does not decode")))?;
        info.set(2, Value::Varint(cells));
        info.set(6, Value::Bytes(buffer));
        info.set(7, Value::Bytes(offsets));
        tile.fields[site.tile_row].value = Value::Bytes(info.encode());
        self.set_archive(site.tile, &tile)?;

        if had != has {
            let step = if has { 1i64 } else { -1 };
            let bucket = site
                .row_bucket
                .iter()
                .copied()
                .find(|&id| self.bucket_has(id, site.row))
                .or_else(|| site.row_bucket.first().copied());
            if let Some(bucket) = bucket {
                self.step_bucket_count(bucket, site.row, step)?;
            }
            if let Some(bucket) = site.column_bucket {
                self.step_bucket_count(bucket, site.column, step)?;
            }
        }
        Ok(())
    }

    /// Does this `TST.HeaderStorageBucket` have an entry for `index`?
    fn bucket_has(&self, bucket: u64, index: usize) -> bool {
        self.archive_of(bucket).is_ok_and(|archive| {
            archive.all(2).any(|value| match value {
                Value::Bytes(raw) => crate::pb::decode_nested(raw)
                    .and_then(|entry| entry.varint(1))
                    .is_some_and(|at| at as usize == index),
                _ => false,
            })
        })
    }

    /// Move a row's or column's stored cell count, adding the entry if the row
    /// or column had none — which is what an all-empty column has.
    fn step_bucket_count(&mut self, bucket: u64, index: usize, step: i64) -> Result<(), Error> {
        let mut archive = self.archive_of(bucket)?;
        for field in archive.fields.iter_mut() {
            if field.number != 2 {
                continue;
            }
            let Value::Bytes(raw) = &field.value else {
                continue;
            };
            let Some(mut entry) = crate::pb::decode_nested(raw) else {
                continue;
            };
            if entry.varint(1).unwrap_or(u64::MAX) as usize != index {
                continue;
            }
            let count = entry.varint(4).unwrap_or(0) as i64 + step;
            entry.set_in_order(4, Value::Varint(count.max(0) as u64));
            field.value = Value::Bytes(entry.encode());
            return self.set_archive(bucket, &archive);
        }
        if step > 0 {
            let mut entry = Message::default();
            entry.set_in_order(1, Value::Varint(index as u64));
            entry.set_in_order(2, Value::Fixed32(0f32.to_le_bytes()));
            entry.set_in_order(3, Value::Varint(0));
            entry.set_in_order(4, Value::Varint(step as u64));
            archive.append_in_order(2, Value::Bytes(entry.encode()));
            return self.set_archive(bucket, &archive);
        }
        Ok(())
    }

    /// Take a reference to an interned string, adding it if it is not there.
    ///
    /// Numbers hands out the **smallest free key** and keeps `nextListID` as a
    /// high-water mark that only rises — observed by watching keys 6 and 13 be
    /// freed and then handed out again while the field stayed at 15. This takes
    /// the simpler half of that: a key at or above the mark can collide with
    /// nothing.
    fn intern_list_string(&mut self, list: u64, text: &str) -> Result<u32, Error> {
        let mut archive = self.archive_of(list)?;
        for field in archive.fields.iter_mut() {
            if field.number != 3 {
                continue;
            }
            let Value::Bytes(raw) = &field.value else {
                continue;
            };
            let Some(mut entry) = crate::pb::decode_nested(raw) else {
                continue;
            };
            if entry.bytes(3) != Some(text.as_bytes()) {
                continue;
            }
            let key = entry.varint(1).unwrap_or(0) as u32;
            entry.set_in_order(2, Value::Varint(entry.varint(2).unwrap_or(0) + 1));
            field.value = Value::Bytes(entry.encode());
            self.set_archive(list, &archive)?;
            return Ok(key);
        }

        // Above the mark *and* above every key present, because the mark is
        // only trustworthy while the app is the one maintaining it.
        let highest = archive
            .all(3)
            .filter_map(|value| match value {
                Value::Bytes(raw) => crate::pb::decode_nested(raw)?.varint(1),
                _ => None,
            })
            .max()
            .unwrap_or(0);
        let key = archive.varint(2).unwrap_or(0).max(highest + 1).max(1) as u32;
        let mut entry = Message::default();
        entry.set_in_order(1, Value::Varint(u64::from(key)));
        entry.set_in_order(2, Value::Varint(1));
        entry.set_in_order(3, Value::Bytes(text.as_bytes().to_vec()));
        archive.set_in_order(2, Value::Varint(u64::from(key) + 1));
        archive.append_in_order(3, Value::Bytes(entry.encode()));
        self.set_archive(list, &archive)?;
        Ok(key)
    }

    fn retain_list_entry(&mut self, list: u64, key: u32) -> Result<(), Error> {
        self.step_list_entry(list, key, 1)
    }

    /// Give up one reference to a `TableDataList` entry, dropping it at zero.
    ///
    /// Dropping is what the app does: emptying the one cell that held a string
    /// removed its entry outright, and the key was later handed out again to a
    /// different string.
    fn release_list_entry(&mut self, list: u64, key: u32) -> Result<(), Error> {
        self.step_list_entry(list, key, -1)
    }

    fn step_list_entry(&mut self, list: u64, key: u32, step: i64) -> Result<(), Error> {
        let mut archive = self.archive_of(list)?;
        let mut drop = None;
        for (at, field) in archive.fields.iter_mut().enumerate() {
            if field.number != 3 {
                continue;
            }
            let Value::Bytes(raw) = &field.value else {
                continue;
            };
            let Some(mut entry) = crate::pb::decode_nested(raw) else {
                continue;
            };
            if entry.varint(1).unwrap_or(0) as u32 != key {
                continue;
            }
            let count = entry.varint(2).unwrap_or(0) as i64 + step;
            if count <= 0 {
                drop = Some(at);
            } else {
                entry.set_in_order(2, Value::Varint(count as u64));
                field.value = Value::Bytes(entry.encode());
            }
            break;
        }
        if let Some(at) = drop {
            archive.fields.remove(at);
        }
        self.set_archive(list, &archive)
    }

    // -- drawables -----------------------------------------------------------

    /// Every drawable in the document — images, shapes, text boxes, lines,
    /// movies, groups, tables and charts — in stream and z-order.
    ///
    /// See [`crate::drawable`] for what a drawable is made of and why the
    /// enumeration does not assume how deep the geometry sits.
    pub fn drawables(&self) -> Vec<crate::drawable::Drawable> {
        crate::drawable::drawables(self)
    }

    /// One drawable by object identifier.
    pub fn drawable(&self, identifier: u64) -> Option<crate::drawable::Drawable> {
        self.drawables()
            .into_iter()
            .find(|d| d.identifier == identifier)
    }

    /// A drawable's fill, stroke, shadow, reflection and opacity, resolved up
    /// the style chain.
    pub fn object_style(&self, identifier: u64) -> Option<crate::drawable::ObjectStyle> {
        crate::drawable::object_style(self, identifier)
    }

    /// Move or resize a drawable.
    ///
    /// The rectangle is the one the **app** reports, which for a masked image
    /// is the mask's window and not the picture's own rectangle. Give `None`
    /// for either half to leave it alone.
    ///
    /// What travels with it, because the app maintains it and a document that
    /// does not is inconsistent with every other document:
    ///
    /// * a media drawable's `originalSize`, which Keynote and Pages both
    ///   rewrote to the new size when a script resized an image;
    /// * a mask's geometry and its path source's natural size. Resizing a
    ///   masked image scales the whole assembly by one factor: Pages, asked to
    ///   make a 475-point-wide masked photo 300 wide, multiplied the picture's
    ///   own size, the mask's offset, the mask's size and the mask path's
    ///   natural size by 300/475 and moved the picture so that
    ///   `image.position + mask.position` still landed on the frame's corner.
    ///   That is what this reproduces, with the horizontal and vertical factors
    ///   taken separately — which reduces to what was observed whenever the
    ///   resize is proportional, and is **unverified** when it is not, because
    ///   the app would not perform one: every image in the corpus has its
    ///   aspect ratio locked.
    ///
    /// Rotation, the geometry flags and everything else in the archive are left
    /// exactly as they were.
    ///
    /// Refused by name: an object carrying version patches (see
    /// [`Document::patched_objects`]), and resizing something whose current
    /// width or height is zero, where the scale factor is not a number. A
    /// **locked** drawable is not refused — the lock is a rule the app's UI
    /// keeps, not one the format keeps — but it is reported, because the app
    /// will not let a user undo the move by hand.
    pub fn set_geometry(
        &mut self,
        identifier: u64,
        position: Option<(f32, f32)>,
        size: Option<(f32, f32)>,
    ) -> Result<crate::drawable::GeometryChange, Error> {
        use crate::drawable::Frame;

        let drawable = self
            .drawable(identifier)
            .ok_or(Error::NoSuchObject(identifier))?;
        let mask = drawable.mask().and_then(|id| self.drawable(id));
        let before = drawable.frame(mask.as_ref());
        for object in [Some(identifier), mask.as_ref().map(|m| m.identifier)]
            .into_iter()
            .flatten()
        {
            if self.patched_objects().iter().any(|&(id, _)| id == object) {
                return Err(Error::Format(format!(
                    "drawable {identifier}: object {object} carries version patches, and \
                     rewriting it would leave them describing where it used to be"
                )));
            }
        }

        let after = Frame {
            x: position.map(|p| p.0).unwrap_or(before.x),
            y: position.map(|p| p.1).unwrap_or(before.y),
            width: size.map(|s| s.0).unwrap_or(before.width),
            height: size.map(|s| s.1).unwrap_or(before.height),
        };
        let resizing = after.width != before.width || after.height != before.height;
        if resizing && (before.width == 0.0 || before.height == 0.0) {
            return Err(Error::Format(format!(
                "drawable {identifier}: it is {} × {} and there is no factor that scales \
                 zero to something else",
                before.width, before.height
            )));
        }
        let (sx, sy) = if resizing {
            (after.width / before.width, after.height / before.height)
        } else {
            (1.0, 1.0)
        };

        // The requested rectangle is the *reported* one, whose origin is the
        // rotated bounding box's corner. Turn it back into the unrotated
        // origin the archive stores, which is the inverse of what
        // `Drawable::frame` does and is the identity at zero degrees.
        let (extent_x, extent_y) =
            crate::drawable::rotated_extent(after.width, after.height, drawable.geometry.angle);
        let base = Frame {
            x: after.x + extent_x / 2.0 - after.width / 2.0,
            y: after.y + extent_y / 2.0 - after.height / 2.0,
            width: after.width,
            height: after.height,
        };

        let mut rewritten = Vec::new();
        let mut geometry = drawable.geometry;
        match &mask {
            Some(mask) => {
                geometry.width *= sx;
                geometry.height *= sy;
                let mut mask_geometry = mask.geometry;
                mask_geometry.x *= sx;
                mask_geometry.y *= sy;
                mask_geometry.width = base.width;
                mask_geometry.height = base.height;
                geometry.x = base.x - mask_geometry.x;
                geometry.y = base.y - mask_geometry.y;
                self.write_geometry(mask.identifier, &mask.path, mask_geometry)?;
                if resizing {
                    self.scale_path_source(mask, base.width, base.height)?;
                }
                rewritten.push(mask.identifier);
            }
            None => {
                geometry.x = base.x;
                geometry.y = base.y;
                geometry.width = base.width;
                geometry.height = base.height;
            }
        }
        self.write_geometry(identifier, &drawable.path, geometry)?;
        if resizing && mask.is_none() && drawable.path_source.is_some() {
            self.scale_path_source(&drawable, base.width, base.height)?;
        }
        rewritten.push(identifier);

        // Media keeps its placed size beside its geometry, and the app moves
        // the two together.
        if resizing && drawable.kind.is_media() {
            let mut archive = self.archive_of(identifier)?;
            let field = match drawable.kind {
                crate::drawable::Kind::Image => crate::drawable::image_field::ORIGINAL_SIZE,
                _ => 20,
            };
            let path: Vec<u32> = drawable.path[..drawable.path.len().saturating_sub(1)].to_vec();
            let mut body = if path.is_empty() {
                archive.clone()
            } else {
                match style::get_path(&archive, &path) {
                    Some(Value::Bytes(raw)) => crate::pb::decode_nested(&raw).unwrap_or_default(),
                    _ => Message::default(),
                }
            };
            if body.get(field).is_some() {
                let mut point = body
                    .bytes(field)
                    .and_then(crate::pb::decode_nested)
                    .unwrap_or_default();
                point.set_in_order(1, Value::Fixed32(geometry.width.to_le_bytes()));
                point.set_in_order(2, Value::Fixed32(geometry.height.to_le_bytes()));
                body.set_in_order(field, Value::Bytes(point.encode()));
                if path.is_empty() {
                    archive = body;
                } else {
                    style::set_path(&mut archive, &path, Some(Value::Bytes(body.encode())))
                        .map_err(|e| Error::Format(format!("drawable {identifier}: {e}")))?;
                }
                self.set_archive(identifier, &archive)?;
            }
        }

        Ok(crate::drawable::GeometryChange {
            drawable: identifier,
            before,
            after,
            mask: mask.map(|m| m.identifier),
            rewritten,
        })
    }

    /// Swap the bytes an image is drawn from.
    ///
    /// `target` is an image drawable or a media-registry identifier, as
    /// `iwork drawables` and `iwork media` print them. The bytes are written
    /// into the package, the `DataInfo` is brought back into step — digest,
    /// name, byte length and recorded pixel size — and every drawable showing
    /// that picture has its `naturalSize`, its traced outline and its
    /// "was replaced" flag updated, which is what Keynote did when a script
    /// gave one of its images a new file.
    ///
    /// **What is deliberately not touched: the frame.** The app re-fits a
    /// replaced picture by scaling it to fill the old window and cropping the
    /// overflow with a mask — an 8 × 8 frame given a 32 × 24 picture came back
    /// as a 10.67 × 8 image behind an 8 × 8 mask offset by 1.33. This does the
    /// simpler thing and leaves the geometry alone, so a picture of a different
    /// shape is drawn stretched; [`crate::MediaReplacement::aspect_note`] says
    /// so, and `set_geometry` is how to fix it.
    ///
    /// **What it refuses, and why this method exists at all.** A drawable can
    /// carry state that sits *between* the stored pixels and what is drawn: a
    /// crop, a mask shaped like something other than a rectangle, an Instant
    /// Alpha knockout path, tone and colour adjustments, cached renderings of
    /// the old pixels, a traced outline of them. None of it is in the file
    /// being swapped in, and none of it can be recomputed here. Replacing bytes
    /// under any of it produces a document that opens, reports the same
    /// geometry through AppleScript, passes every structural check — and
    /// renders the wrong thing. So it is refused by name, with
    /// [`Error::NonDestructiveEdit`] listing what was found. An *identity* mask
    /// — the whole picture, no crop, which is what the app installs when it
    /// replaces an image — is not an objection.
    pub fn replace_media(
        &mut self,
        target: u64,
        bytes: &[u8],
        preferred_name: &str,
        pixel_size: Option<(f32, f32)>,
    ) -> Result<crate::media::MediaReplacement, Error> {
        use crate::drawable::image_field;
        use crate::media::{self, field as data_field};

        let drawables = self.drawables();
        let data = match drawables
            .iter()
            .find(|d| d.identifier == target)
            .and_then(|d| d.media.as_ref())
            .and_then(|m| m.data)
        {
            Some(data) => data,
            None => target,
        };
        let file = self
            .data_files()
            .into_iter()
            .find(|d| d.identifier == data)
            .ok_or_else(|| Error::Format(format!("no media registered as {data}")))?;

        // The edit state first, and before anything else that could refuse:
        // it is the answer a caller most needs to hear, and a theme asset that
        // is *also* cropped should say so.
        let users: Vec<&crate::drawable::Drawable> = drawables
            .iter()
            .filter(|d| d.media.as_ref().and_then(|m| m.data) == Some(data))
            .collect();
        for drawable in &users {
            let objections = drawable
                .edit_state
                .as_ref()
                .map(|state| state.objections())
                .unwrap_or_default();
            if !objections.is_empty() {
                return Err(Error::NonDestructiveEdit {
                    drawable: drawable.identifier,
                    reasons: objections,
                });
            }
        }

        let was = file.entry_name().ok_or_else(|| {
            Error::Format(format!(
                "media {data} ({}) lives in the app's theme bundle rather than in the \
                 document, so there are no bytes here to replace",
                file.original_name
            ))
        })?;

        let new_size = match pixel_size.or_else(|| media::pixel_size(bytes)) {
            Some(size) => size,
            None => {
                return Err(Error::Format(
                    "the replacement is not a PNG or a JPEG, so its pixel size cannot be \
                     read here — pass it explicitly"
                        .into(),
                ))
            }
        };
        let digest = media::sha1(bytes);
        let now = format!("Data/{}", media::stored_name(preferred_name, data));

        // The registry entry first: if the metadata cannot be rewritten,
        // nothing has been touched.
        let mut old_pixel_size = None;
        let metadata = self
            .objects()
            .find(|(_, object)| object.message_type() == crate::TYPE_PACKAGE_METADATA)
            .map(|(_, object)| object.identifier)
            .ok_or_else(|| Error::Format("no TSP.PackageMetadata".into()))?;
        let mut archive = self.archive_of(metadata)?;
        let mut found = false;
        for field in &mut archive.fields {
            if field.number != 4 {
                continue;
            }
            let Value::Bytes(raw) = &field.value else {
                continue;
            };
            let Some(mut info) = crate::pb::decode_nested(raw) else {
                continue;
            };
            if info.varint(data_field::IDENTIFIER) != Some(data) {
                continue;
            }
            old_pixel_size = media::attribute_pixel_size(&info);
            info.set_in_order(data_field::DIGEST, Value::Bytes(digest.to_vec()));
            info.set_in_order(
                data_field::PREFERRED_FILE_NAME,
                Value::Bytes(preferred_name.as_bytes().to_vec()),
            );
            info.set_in_order(
                data_field::FILE_NAME,
                Value::Bytes(
                    now.strip_prefix("Data/")
                        .unwrap_or(&now)
                        .as_bytes()
                        .to_vec(),
                ),
            );
            if info.get(data_field::MATERIALIZED_LENGTH).is_some() {
                info.set_in_order(
                    data_field::MATERIALIZED_LENGTH,
                    Value::Varint(bytes.len() as u64),
                );
            }
            media::set_attribute_pixel_size(&mut info, new_size.0, new_size.1);
            field.value = Value::Bytes(info.encode());
            found = true;
            break;
        }
        if !found {
            return Err(Error::Format(format!(
                "the media registry has no entry {data}"
            )));
        }
        self.set_archive(metadata, &archive)?;

        // Then the bytes, keeping the entry where it was in the package.
        match self
            .package
            .entries
            .iter_mut()
            .find(|(name, _)| *name == was)
        {
            Some(entry) => {
                entry.0 = now.clone();
                entry.1 = bytes.to_vec();
            }
            None => self.package.set(&now, bytes.to_vec()),
        }

        // Then everything that has to agree with the new picture.
        let mut updated = Vec::new();
        for drawable in &users {
            if drawable.kind != crate::drawable::Kind::Image {
                continue;
            }
            let mut archive = self.archive_of(drawable.identifier)?;
            let body_path: Vec<u32> =
                drawable.path[..drawable.path.len().saturating_sub(1)].to_vec();
            let mut body = if body_path.is_empty() {
                archive.clone()
            } else {
                match style::get_path(&archive, &body_path) {
                    Some(Value::Bytes(raw)) => crate::pb::decode_nested(&raw).unwrap_or_default(),
                    _ => continue,
                }
            };
            let mut point = body
                .bytes(image_field::NATURAL_SIZE)
                .and_then(crate::pb::decode_nested)
                .unwrap_or_default();
            point.set_in_order(1, Value::Fixed32(new_size.0.to_le_bytes()));
            point.set_in_order(2, Value::Fixed32(new_size.1.to_le_bytes()));
            body.set_in_order(image_field::NATURAL_SIZE, Value::Bytes(point.encode()));

            // The traced outline is the picture's own rectangle and the app
            // rewrites it with the picture. Anything else was refused above.
            if body.get(image_field::TRACED_PATH).is_some() {
                body.set_in_order(
                    image_field::TRACED_PATH,
                    Value::Bytes(
                        crate::drawable::natural_rectangle(new_size.0, new_size.1).encode(),
                    ),
                );
            }
            let flags = body.varint(image_field::FLAGS).unwrap_or(0)
                | u64::from(crate::drawable::media_flag::WAS_REPLACED);
            body.set_in_order(image_field::FLAGS, Value::Varint(flags));

            if body_path.is_empty() {
                archive = body;
            } else {
                style::set_path(&mut archive, &body_path, Some(Value::Bytes(body.encode())))
                    .map_err(|e| Error::Format(format!("drawable {}: {e}", drawable.identifier)))?;
            }
            self.set_archive(drawable.identifier, &archive)?;
            updated.push(drawable.identifier);
        }

        let aspect_changed = old_pixel_size.is_some_and(|(w, h)| {
            w > 0.0
                && h > 0.0
                && ((w / h) - (new_size.0 / new_size.1)).abs() > 0.01 * (w / h).max(0.01)
        });
        Ok(crate::media::MediaReplacement {
            data,
            was,
            now,
            digest,
            bytes: bytes.len(),
            old_pixel_size,
            new_pixel_size: new_size,
            drawables: updated,
            aspect_changed,
        })
    }

    /// Write a geometry back at the field path the drawable was found through.
    fn write_geometry(
        &mut self,
        identifier: u64,
        path: &[u32],
        geometry: crate::drawable::Geometry,
    ) -> Result<(), Error> {
        let mut archive = self.archive_of(identifier)?;
        let mut full: Vec<u32> = path.to_vec();
        full.push(crate::drawable::field::GEOMETRY);
        let Some(Value::Bytes(raw)) = style::get_path(&archive, &full) else {
            return Err(Error::Format(format!(
                "drawable {identifier}: no geometry at field {}",
                full.iter()
                    .map(u32::to_string)
                    .collect::<Vec<_>>()
                    .join(".")
            )));
        };
        let mut message = crate::pb::decode_nested(&raw).ok_or_else(|| {
            Error::Format(format!("drawable {identifier}: geometry does not decode"))
        })?;
        geometry.write_into(&mut message);
        style::set_path(&mut archive, &full, Some(Value::Bytes(message.encode())))
            .map_err(|e| Error::Format(format!("drawable {identifier}: {e}")))?;
        self.set_archive(identifier, &archive)
    }

    /// Resize a shape's or a mask's path source with the object.
    ///
    /// **A shape's size lives in two places**, which is the thing this exists
    /// for. Told to make a 200 × 200 Keynote shape 444 × 128, the app rewrote
    /// the geometry *and* the bezier path source — its natural size and every
    /// one of the rectangle's corners — and a document with only the geometry
    /// changed opens with the app still reporting 200 × 200. Nothing about that
    /// is visible from the archive alone; it took writing one and asking.
    ///
    /// **A mask is the exception, and it is the app's exception rather than a
    /// simplification here.** Asked to resize the cropped photo in the Pages
    /// report, Pages changed the mask path's natural size from 475 × 383 to
    /// 300 × 241.89 and left every point of the path exactly where it was —
    /// while Keynote, resizing a shape, moved both. So masks get the natural
    /// size only, shapes get both, and each matches what the app that owns it
    /// wrote. The points are additionally left alone when they are plainly not
    /// drawn in the frame's coordinates, which is the case for a line.
    fn scale_path_source(
        &mut self,
        drawable: &crate::drawable::Drawable,
        width: f32,
        height: f32,
    ) -> Result<(), Error> {
        let identifier = drawable.identifier;
        let mut archive = self.archive_of(identifier)?;
        // The path source hangs off the concrete class — field 3 of a shape,
        // field 2 of a mask — which is one level above the drawable archive.
        let body_path: Vec<u32> = drawable.path[..drawable.path.len().saturating_sub(1)].to_vec();
        let field = match drawable.kind {
            crate::drawable::Kind::Mask => 2u32,
            _ => 3,
        };
        let mut body = if body_path.is_empty() {
            archive.clone()
        } else {
            match style::get_path(&archive, &body_path) {
                Some(Value::Bytes(raw)) => crate::pb::decode_nested(&raw).unwrap_or_default(),
                _ => return Ok(()),
            }
        };
        let Some(mut source) = body.bytes(field).and_then(crate::pb::decode_nested) else {
            return Ok(());
        };

        // The natural size sits at a different field per path-source kind: 3
        // for the point and scalar sources, 2 for the bezier ones, 1 for a
        // callout. Only the arm that is set is touched.
        for (arm, natural) in [(3u32, 3u32), (4, 3), (5, 2), (6, 1), (8, 2)] {
            let Some(mut arm_body) = source.bytes(arm).and_then(crate::pb::decode_nested) else {
                continue;
            };
            let Some(mut point) = arm_body.bytes(natural).and_then(crate::pb::decode_nested) else {
                continue;
            };
            let was = (float_field(&point, 1), float_field(&point, 2));
            point.set_in_order(1, Value::Fixed32(width.to_le_bytes()));
            point.set_in_order(2, Value::Fixed32(height.to_le_bytes()));
            arm_body.set_in_order(natural, Value::Bytes(point.encode()));

            if matches!(arm, 5 | 8) && drawable.kind != crate::drawable::Kind::Mask {
                let factor = |before: f32, after: f32| {
                    if before.abs() < f32::EPSILON {
                        1.0
                    } else {
                        after / before
                    }
                };
                scale_path_points(
                    &mut arm_body,
                    3,
                    was,
                    (factor(was.0, width), factor(was.1, height)),
                );
            }

            source.set_in_order(arm, Value::Bytes(arm_body.encode()));
            body.set_in_order(field, Value::Bytes(source.encode()));
            if body_path.is_empty() {
                archive = body;
            } else {
                style::set_path(&mut archive, &body_path, Some(Value::Bytes(body.encode())))
                    .map_err(|e| Error::Format(format!("drawable {identifier}: {e}")))?;
            }
            return self.set_archive(identifier, &archive);
        }
        Ok(())
    }

    // -- text styles ---------------------------------------------------------

    /// Every character, paragraph and list style in the document.
    ///
    /// These are the objects the attribute tables of a `TSWP.StorageArchive`
    /// point at; see [`crate::style`] for what is and is not known about their
    /// contents.
    pub fn text_styles(&self) -> Vec<TextStyle> {
        let mut out = Vec::new();
        for (stream, object) in self.objects() {
            let Some(message) = object.messages.first() else {
                continue;
            };
            let Some(kind) = StyleKind::from_message_type(message.message_type) else {
                continue;
            };
            let Ok(archive) = Message::decode(&message.payload) else {
                continue;
            };
            out.push(TextStyle {
                identifier: object.identifier,
                stream: stream.to_string(),
                kind,
                name: style::string_at(&archive, style::NAME),
                style_identifier: style::string_at(&archive, style::STYLE_IDENTIFIER),
                parent: style::reference_at(&archive, style::PARENT),
                stylesheet: style::reference_at(&archive, style::STYLESHEET),
                labels: style::labels(&archive),
                archive,
            });
        }
        out
    }

    pub fn text_style(&self, identifier: u64) -> Option<TextStyle> {
        self.text_styles()
            .into_iter()
            .find(|s| s.identifier == identifier)
    }

    /// Every run of text that points at a style, across the whole document.
    ///
    /// This reads all six attribute tables rather than only the one belonging
    /// to the style's kind, so a style used somewhere unexpected still shows up.
    pub fn text_style_usage(&self, identifier: u64) -> Vec<StyleUse> {
        let mut out = Vec::new();
        for (stream, object) in self.objects() {
            let Some(message) = object.messages.first() else {
                continue;
            };
            if message.message_type != crate::TYPE_STORAGE {
                continue;
            }
            let Ok(storage) = Message::decode(&message.payload) else {
                continue;
            };
            let length = text::length(&text::read(&storage));
            for &table in text::ATTRIBUTE_TABLES {
                let Some(table_message) = storage.bytes(table).and_then(crate::pb::decode_nested)
                else {
                    continue;
                };
                let runs = style::runs(&table_message);
                for (i, run) in runs.iter().enumerate() {
                    if run.style != Some(identifier) {
                        continue;
                    }
                    let end = runs.get(i + 1).map(|next| next.start).unwrap_or(length);
                    out.push(StyleUse {
                        storage: object.identifier,
                        stream: stream.to_string(),
                        table,
                        range: run.start..end.max(run.start),
                    });
                }
            }
        }
        out
    }

    /// Copy a style, giving the copy a new name and a new object identifier.
    ///
    /// Styles are created by copying rather than by synthesis, for the reason
    /// `FORMAT.md` gives for whole documents: the style graph is large, iWork is
    /// unforgiving about dangling references, and a style that already works is
    /// a far better starting point than a guess at the schema. The copy lands in
    /// the template's stream, right after it, and is listed in every stylesheet
    /// that listed the template by plain reference.
    ///
    /// The identifier comes from above `TSP.PackageMetadata` field 1, which is
    /// then bumped, so iWork will not later hand the same number to something
    /// else.
    ///
    /// **A copy of a variation style does not get the name.** Named styles and
    /// variations are different things: a named style carries a name at
    /// [`style::NAME`] and an identifier at [`style::STYLE_IDENTIFIER`], while a
    /// variation carries neither, a parent, and a flag saying it is one. Naming
    /// the copy of a variation produces an object that claims to be a variation,
    /// has a name, has no identifier, and is listed among the named styles —
    /// and Pages crashes on opening the document. It was doing exactly that
    /// until a document that crashed showed up.
    ///
    /// The copy is still made, still listed, and still usable — it is simply
    /// anonymous, which is what a variation is. `CreatedStyle::name` reports
    /// whether the name was applied. Turning a variation into a named style
    /// properly would mean synthesising a style identifier and clearing the
    /// variation flag, which is more invention than this crate is willing to do
    /// without a document to check it against.
    pub fn create_text_style(&mut self, template: u64, name: &str) -> Result<CreatedStyle, Error> {
        let source = self
            .text_style(template)
            .ok_or(Error::NoSuchStyle(template))?;
        let (stream, index) = self.locate(template).expect("the style was just found");
        let identifier = self.next_object_identifier();

        let mut archive = source.archive.clone();
        let applied_name = source.name.is_some().then(|| name.to_string());
        if applied_name.is_some() {
            style::set_path(
                &mut archive,
                style::NAME,
                Some(Value::Bytes(name.as_bytes().to_vec())),
            )
            .map_err(|e| Error::Format(format!("style {template}: {e}")))?;
        }

        // Bump the high-water mark first: if there is no package metadata to
        // bump, no object is added at all.
        self.set_last_object_identifier(identifier)?;

        let mut object = self.streams[&stream][index].clone();
        object.identifier = identifier;
        object.messages[0].payload = archive.encode();
        self.streams
            .get_mut(&stream)
            .expect("stream came from the document")
            .insert(index + 1, object);

        // List the copy wherever the template is listed — in the template's own
        // stylesheet, which the template names. See `style::clone_registrations`
        // for why anywhere-a-reference-looks-right is not good enough.
        let mut registrations_cloned = 0;
        if let Some(stylesheet) = source.stylesheet {
            if let Ok(mut sheet) = self.archive_of(stylesheet) {
                registrations_cloned = style::clone_registrations(&mut sheet, template, identifier);
                // A style is listed twice: once among the stylesheet's styles,
                // once under its parent. Both, or the copy is an orphan.
                if let Some(parent) = source.parent {
                    if style::clone_sibling(&mut sheet, parent, template, identifier) {
                        registrations_cloned += 1;
                    }
                }
                if registrations_cloned > 0 {
                    self.set_archive(stylesheet, &sheet)?;
                }
            }
        }
        self.declare_external_references();

        Ok(CreatedStyle {
            identifier,
            template,
            stream,
            registrations_cloned,
            name: applied_name,
        })
    }

    /// Rewrite a style's name.
    ///
    /// Naming an unnamed variation style is allowed and gives it a name; the
    /// field is created if it is not there.
    pub fn rename_text_style(&mut self, identifier: u64, name: &str) -> Result<(), Error> {
        self.set_text_style_property(
            identifier,
            style::NAME,
            Some(Value::Bytes(name.as_bytes().to_vec())),
        )
    }

    /// Edit a style archive directly.
    ///
    /// The escape hatch for everything this crate does not model: the archive
    /// arrives decoded into wire fields and is re-encoded in place afterwards.
    pub fn update_text_style(
        &mut self,
        identifier: u64,
        edit: impl FnOnce(&mut Message),
    ) -> Result<(), Error> {
        let style = self
            .text_style(identifier)
            .ok_or(Error::NoSuchStyle(identifier))?;
        let mut archive = style.archive;
        edit(&mut archive);
        self.set_archive(identifier, &archive)
    }

    /// Set one field of a style, addressed by a path of field numbers, or
    /// remove it when `value` is `None`.
    ///
    /// `iwork style <file> <id>` prints those paths. Nothing here knows what a
    /// given field *means* — see [`crate::style`].
    pub fn set_text_style_property(
        &mut self,
        identifier: u64,
        path: &[u32],
        value: Option<Value>,
    ) -> Result<(), Error> {
        let style = self
            .text_style(identifier)
            .ok_or(Error::NoSuchStyle(identifier))?;
        let mut archive = style.archive;
        style::set_path(&mut archive, path, value)
            .map_err(|e| Error::Format(format!("style {identifier}: {e}")))?;
        self.set_archive(identifier, &archive)
    }

    /// Set the colour of a style's text, everywhere the style keeps it.
    ///
    /// A style does not have one text colour. It has up to four — the font
    /// colour, the fill drawn inside the glyphs, and the underline and
    /// strikethrough colours that follow the text — and Pages writes all of
    /// them together. Setting only the font colour leaves the fill behind, and
    /// the fill is what is drawn: a title whose `11.7` says red and whose
    /// `11.46.1` still says black renders black. That was not a guess; it took
    /// a document that came back with a bigger title and no red.
    ///
    /// Returns how many were set. **Zero means the style keeps no colour at
    /// all** — nothing was written, because a colour this crate invents is
    /// missing fields that make Pages refuse the document. Copy the colour from
    /// a style that has one, with [`Document::copy_text_style_property`].
    ///
    /// Channels are `0.0..=1.0`, as the format stores them.
    pub fn set_text_style_color(
        &mut self,
        identifier: u64,
        red: f32,
        green: f32,
        blue: f32,
        alpha: f32,
    ) -> Result<usize, Error> {
        let style = self
            .text_style(identifier)
            .ok_or(Error::NoSuchStyle(identifier))?;
        let mut archive = style.archive;
        let mut set = 0;
        for path in style::property::TEXT_COLOR_PATHS {
            let Some(Value::Bytes(raw)) = style::get_path(&archive, path) else {
                continue;
            };
            let Some(mut colour) = crate::pb::decode_nested(&raw) else {
                continue;
            };
            if !style::is_color(&colour) {
                continue;
            }
            style::set_channels(&mut colour, red, green, blue, alpha);
            style::set_path(&mut archive, path, Some(Value::Bytes(colour.encode())))
                .map_err(|e| Error::Format(format!("style {identifier}: {e}")))?;
            set += 1;
        }
        if set > 0 {
            self.set_archive(identifier, &archive)?;
        }
        Ok(set)
    }

    /// Copy one property subtree from another style.
    ///
    /// The way to give a style a property whose container it does not have.
    /// [`Document::set_text_style_property`] will not invent a container,
    /// because a container it invents holds only the fields it was asked for —
    /// a colour written as `{r, g, b}`, with no model and no alpha, crashes
    /// Pages on opening. Lifting a whole working subtree across avoids the
    /// question: the copy is a colour that a real document already contains.
    ///
    /// Take the colour from a style that has one, then change the channels:
    ///
    /// ```no_run
    /// # fn main() -> Result<(), iwork::Error> {
    /// # let mut doc = iwork::Document::open("Report.pages")?;
    /// use iwork::style::property;
    /// doc.copy_text_style_property(3712, 3801, property::FONT_COLOR)?;
    /// doc.set_text_style_property(3801, property::RED,
    ///     Some(iwork::pb::Value::Fixed32(0.85f32.to_le_bytes())))?;
    /// # Ok(()) }
    /// ```
    pub fn copy_text_style_property(
        &mut self,
        from: u64,
        to: u64,
        path: &[u32],
    ) -> Result<(), Error> {
        let source = self.text_style(from).ok_or(Error::NoSuchStyle(from))?;
        let value = style::get_path(&source.archive, path).ok_or_else(|| {
            Error::Format(format!(
                "style {from} has no field {} to copy",
                dotted(path)
            ))
        })?;
        let target = self.text_style(to).ok_or(Error::NoSuchStyle(to))?;
        let mut archive = target.archive;
        // The parent of the leaf has to exist in the target; copy it whole if
        // the target lacks it, which is the case this method is for.
        if let Some((leaf, parents)) = path.split_last() {
            if !parents.is_empty() && style::get_path(&archive, parents).is_none() {
                let container = style::get_path(&source.archive, parents).ok_or_else(|| {
                    Error::Format(format!("style {from} has no field {}", dotted(parents)))
                })?;
                style::set_path(&mut archive, parents, Some(container))
                    .map_err(|e| Error::Format(format!("style {to}: {e}")))?;
            }
            let _ = leaf;
        }
        style::set_path(&mut archive, path, Some(value))
            .map_err(|e| Error::Format(format!("style {to}: {e}")))?;
        self.set_archive(to, &archive)
    }

    /// Remove a style, and every reference to it this crate can account for.
    ///
    /// Runs that use it are pointed at `replace_with`, or dropped when that is
    /// `None` — a dropped run lets the preceding one extend over its text.
    /// References from stylesheets are removed.
    ///
    /// If anything *else* in the document still refers to the style — another
    /// style naming it as a parent, a keyed stylesheet entry, an archive this
    /// crate does not model — the delete is refused with [`Error::StyleInUse`]
    /// and the document is left untouched. iWork is unforgiving about dangling
    /// references, so a delete that cannot be completed cleanly is not
    /// completed at all.
    pub fn delete_text_style(
        &mut self,
        identifier: u64,
        replace_with: Option<u64>,
    ) -> Result<StyleDeletion, Error> {
        let style = self
            .text_style(identifier)
            .ok_or(Error::NoSuchStyle(identifier))?;
        if let Some(replacement) = replace_with {
            let other = self
                .text_style(replacement)
                .ok_or(Error::NoSuchStyle(replacement))?;
            if other.kind != style.kind {
                return Err(Error::Format(format!(
                    "cannot replace a {} style with a {} style",
                    style.kind.as_str(),
                    other.kind.as_str()
                )));
            }
        }

        let mut deletion = StyleDeletion {
            identifier,
            runs_repointed: 0,
            runs_dropped: 0,
            registrations_removed: 0,
        };
        let mut edits: Vec<(u64, Message)> = Vec::new();
        let mut still_referenced: Vec<u64> = Vec::new();

        // Work out the whole edit before touching anything, so a refusal leaves
        // the document exactly as it was.
        for (_, object) in self.objects() {
            if object.identifier == identifier {
                continue;
            }
            let Some(message) = object.messages.first() else {
                continue;
            };
            let Ok(archive) = Message::decode(&message.payload) else {
                continue;
            };
            if style::count_references(&archive, identifier) == 0 {
                continue;
            }

            let mut edited = archive.clone();
            let mut touched = 0;
            if message.message_type == crate::TYPE_STORAGE {
                for &table in text::ATTRIBUTE_TABLES {
                    let Some(mut runs) = edited.bytes(table).and_then(crate::pb::decode_nested)
                    else {
                        continue;
                    };
                    let changed = style::repoint(&mut runs, identifier, replace_with);
                    if changed > 0 {
                        edited.set(table, Value::Bytes(runs.encode()));
                        touched += changed;
                    }
                }
                match replace_with {
                    Some(_) => deletion.runs_repointed += touched,
                    None => deletion.runs_dropped += touched,
                }
            }
            // Unlist it, but only from the stylesheet it says it belongs to.
            // Other objects hold bare references that are positions rather than
            // memberships — a Keynote slide's outline levels, say — and pulling
            // one out of those would shift the rest. Anything still holding a
            // reference after this is reported below and the delete is refused,
            // which is the right outcome for a style that is genuinely in use.
            if Some(object.identifier) == style.stylesheet {
                let removed = style::remove_registrations(&mut edited, identifier)
                    + style::remove_sibling(&mut edited, identifier);
                deletion.registrations_removed += removed;
                touched += removed;
            }

            if style::count_references(&edited, identifier) > 0 {
                still_referenced.push(object.identifier);
            } else if touched > 0 {
                edits.push((object.identifier, edited));
            }
        }

        if !still_referenced.is_empty() {
            return Err(Error::StyleInUse {
                identifier,
                references: still_referenced,
            });
        }

        for (target, archive) in edits {
            self.set_archive(target, &archive)?;
        }
        let (stream, index) = self.locate(identifier).expect("the style was just found");
        self.streams
            .get_mut(&stream)
            .expect("stream came from the document")
            .remove(index);
        // `TSP.PackageMetadata` field 1 is a high-water mark, not a count, so it
        // stays where it is — lowering it would let iWork reuse the identifier.
        Ok(deletion)
    }

    /// Point a range of one storage's text at a style.
    ///
    /// The range is in UTF-16 code units, the unit run indices are counted in.
    /// Which attribute table is edited follows from the style's kind. Paragraph
    /// and list styles apply to whole paragraphs, so give them ranges from
    /// [`Document::paragraph_ranges`] rather than arbitrary offsets.
    pub fn apply_text_style(
        &mut self,
        storage: u64,
        range: Range<u64>,
        style_identifier: u64,
    ) -> Result<(), Error> {
        let kind = self
            .text_style(style_identifier)
            .ok_or(Error::NoSuchStyle(style_identifier))?
            .kind;
        let mut archive = self.storage_archive(storage)?;
        let body = text::read(&archive);
        let length = text::length(&body);
        let table_field = kind.attribute_table();

        // A paragraph style applies to whole paragraphs, so the range grows to
        // the paragraphs it touches. Writing a paragraph run inside a paragraph
        // is not a smaller version of the same edit — it is a table iWork never
        // writes, and Keynote's own parser is documented as rendering a text box
        // 2^16 points tall and then crashing on one.
        let range = match kind {
            StyleKind::Paragraph | StyleKind::List => {
                let paragraphs = text::paragraph_ranges(&body);
                let start = paragraphs
                    .iter()
                    .rev()
                    .find(|p| p.start <= range.start)
                    .map(|p| p.start)
                    .unwrap_or(range.start);
                let end = paragraphs
                    .iter()
                    .find(|p| p.end >= range.end)
                    .map(|p| p.end)
                    .unwrap_or(range.end);
                start..end.max(start)
            }
            StyleKind::Character => range,
        };

        let mut table = match archive.bytes(table_field) {
            Some(raw) => crate::pb::decode_nested(raw).ok_or_else(|| {
                Error::Format(format!(
                    "storage {storage}: field {table_field} is not an attribute table"
                ))
            })?,
            None => Message::default(),
        };
        style::apply(&mut table, range, style_identifier, length);
        archive.set(table_field, Value::Bytes(table.encode()));
        self.set_archive(storage, &archive)?;
        // The style usually lives in the document stylesheet and the storage in
        // the document body — different components, so the reference has to be
        // declared before iWork will resolve it.
        self.declare_external_references();
        Ok(())
    }

    /// Character ranges of the paragraphs in one storage, in UTF-16 code units.
    pub fn paragraph_ranges(&self, storage: u64) -> Result<Vec<Range<u64>>, Error> {
        Ok(text::paragraph_ranges(&self.storage_text(storage)?))
    }

    /// Text of one storage by identifier, placeholders and all.
    ///
    /// [`Document::text_storages`] hides storages whose contents are only an
    /// anchor for something else; this does not, because a caller addressing a
    /// storage by identifier has already chosen it.
    pub fn storage_text(&self, storage: u64) -> Result<String, Error> {
        Ok(text::read(&self.storage_archive(storage)?))
    }

    /// The Pages document spine: mode, paper, sections, headers and footers,
    /// page templates, threads, contents lists, footnotes and bookmarks.
    ///
    /// `None` for a document with no `TP.DocumentArchive`, which is every
    /// Numbers and Keynote document.
    pub fn structure(&self) -> Option<crate::pages::Structure> {
        crate::pages::structure(self)
    }

    /// Sections of a Pages document, with the range of body text each covers.
    pub fn sections(&self) -> Vec<crate::pages::Section> {
        self.structure().map(|s| s.sections).unwrap_or_default()
    }

    /// Every header and footer storage, named by section, template page and
    /// zone.
    ///
    /// The storage identifier each one carries is what [`Document::set_text`]
    /// takes: a header is a `TSWP.StorageArchive` like any other, and editing
    /// one goes through the same remapping as editing the body.
    pub fn header_footers(&self) -> Vec<crate::pages::HeaderFooter> {
        self.structure()
            .map(|s| s.header_footers)
            .unwrap_or_default()
    }

    /// Column layouts in force over one storage's paragraphs.
    pub fn column_layouts(&self, storage: u64) -> Vec<crate::pages::ColumnLayout> {
        crate::pages::column_layouts(self, storage)
    }

    /// Everything about the object graph that looks wrong.
    ///
    /// Written to be run against an *unedited* document first: whatever it says
    /// about a file iWork itself produced is a bug in this function, not in the
    /// file. Its value is the difference — a check that passes on the original
    /// and fails after an edit has found something the edit broke.
    ///
    /// It cannot say a document will open. It can say a document is missing an
    /// object something points at, which is the failure `FORMAT.md` warns about
    /// and the one this crate is most likely to cause.
    pub fn problems(&self) -> Vec<String> {
        let mut problems = Vec::new();
        let mut seen: BTreeMap<u64, usize> = BTreeMap::new();
        for (_, object) in self.objects() {
            *seen.entry(object.identifier).or_default() += 1;
        }
        for (identifier, count) in seen.iter().filter(|(_, n)| **n > 1) {
            problems.push(format!("object {identifier} is defined {count} times"));
        }

        let highest = seen.keys().copied().max().unwrap_or(0);
        match self.last_object_identifier() {
            Some(mark) if mark < highest => problems.push(format!(
                "package metadata says the highest identifier is {mark}, but {highest} exists"
            )),
            None => problems.push("no TSP.PackageMetadata".to_string()),
            _ => {}
        }

        for component in self.components() {
            let name = component.stream_name();
            if !self.package.contains(&name) {
                problems.push(format!(
                    "component {} points at missing {name}",
                    component.identifier
                ));
            }
        }

        let styles: BTreeMap<u64, StyleKind> = self
            .text_styles()
            .into_iter()
            .map(|s| (s.identifier, s.kind))
            .collect();

        for (stream, object) in self.objects() {
            let Some(message) = object.messages.first() else {
                continue;
            };
            if message.message_type != crate::TYPE_STORAGE {
                continue;
            }
            let Ok(storage) = Message::decode(&message.payload) else {
                problems.push(format!("storage {} does not decode", object.identifier));
                continue;
            };
            let body = text::read(&storage);
            let length = text::length(&body);
            let starts: Vec<u64> = text::paragraph_ranges(&body)
                .into_iter()
                .map(|r| r.start)
                .collect();

            // A field this crate cannot place is not a fault in the document —
            // it is a gap in the crate, and one that makes an edit unsafe. Say
            // so, because `check` is where a caller finds out before writing.
            if let Some(field) = text::unknown_table(&storage) {
                problems.push(format!(
                    "{stream} storage {}: field {field} is not an attribute table \
                     this crate knows, so an edit to this storage is refused",
                    object.identifier
                ));
            }

            for spec in text::TABLES {
                let field = spec.field;
                let Some(table) = storage.bytes(field).and_then(crate::pb::decode_nested) else {
                    continue;
                };
                let where_ = format!("{stream} storage {} {}", object.identifier, spec.name);
                if spec.anchoring == text::Anchoring::Range {
                    // These carry explicit ranges rather than run starts, so
                    // the rules are different: a range has to fit in the text
                    // and cover something.
                    for (location, span) in text::ranges(&table) {
                        if location + span > length {
                            problems.push(format!(
                                "{where_}: range {location}..{} runs past the text, \
                                 which is {length} long",
                                location + span
                            ));
                        }
                        if span == 0 {
                            problems.push(format!("{where_}: range at {location} is empty"));
                        }
                    }
                    continue;
                }
                let entries = text::entry_indices(&table, spec.anchoring);
                let mut previous: Option<u64> = None;
                for (index, _) in &entries {
                    if previous.is_some_and(|p| *index <= p) {
                        problems.push(format!(
                            "{where_}: entry index {index} does not increase (after {})",
                            previous.unwrap()
                        ));
                    }
                    previous = Some(*index);
                    if *index > length {
                        problems.push(format!(
                            "{where_}: entry at {index} but the text is {length} long"
                        ));
                    }
                    // A paragraph attribute may only sit where a paragraph
                    // begins, or at the very end of the text — the slot the
                    // style of a paragraph not yet typed comes from. Keynote
                    // renders a text box 2^16 points tall and then crashes on a
                    // paragraph run past the end of the text.
                    if spec.anchoring == text::Anchoring::Paragraph
                        && *index != length
                        && !starts.contains(index)
                    {
                        problems.push(format!(
                            "{where_}: entry at {index} is neither a paragraph start \
                             nor the end of the text"
                        ));
                    }
                }
                // A run or paragraph table describes the text from its first
                // entry onward, so one that starts late leaves characters with
                // no attribute at all. An anchor table is a set of points and
                // has no such obligation.
                if let Some((first, _)) = entries.first() {
                    if *first != 0 && spec.anchoring != text::Anchoring::Character {
                        problems.push(format!(
                            "{where_}: the first entry is at {first}, so characters \
                             0..{first} have no attribute"
                        ));
                    }
                }
                for (index, target) in &entries {
                    let where_ = format!("{stream} storage {} table {field}", object.identifier);
                    let _ = index;
                    let Some(target) = target else { continue };
                    let target = *target;
                    if self.object(target).is_none() {
                        problems.push(format!("{where_}: run points at missing object {target}"));
                    } else if let Some(kind) = styles.get(&target) {
                        if kind.attribute_table() != field {
                            problems.push(format!(
                                "{where_}: run points at a {} style, which belongs in table {}",
                                kind.as_str(),
                                kind.attribute_table()
                            ));
                        }
                    }
                }
            }
        }

        // Style archives must resolve their parent and stylesheet, and a style
        // listed in a stylesheet must also be grouped under its parent.
        for style in self.text_styles() {
            for (what, target) in [("parent", style.parent), ("stylesheet", style.stylesheet)] {
                let Some(target) = target else { continue };
                if target != 0 && self.object(target).is_none() {
                    problems.push(format!(
                        "style {}: {what} {target} does not exist",
                        style.identifier
                    ));
                }
            }
            let (Some(parent), Some(sheet)) = (style.parent, style.stylesheet) else {
                continue;
            };
            let Ok(archive) = self.archive_of(sheet) else {
                continue;
            };
            if style::count_references(&archive, style.identifier) == 1 {
                problems.push(format!(
                    "style {}: listed in stylesheet {sheet} but not grouped under \
                     its parent {parent}",
                    style.identifier
                ));
            }
        }

        // Drawables and the media registry check each other. Every rule here
        // is one the apps keep across the whole corpus, and one a writer can
        // break without the document failing to open.
        problems.extend(self.media_problems());

        for (component, object, target) in self.undeclared_references() {
            problems.push(format!(
                "object {object} refers to {target} in another component, which \
                 component {component} does not declare"
            ));
        }

        // The Pages spine checks itself: sections that begin on a break,
        // section templates with their three zones each, threads whose boxes
        // exist. See `structure_problems`.
        problems.extend(self.structure_problems());

        // Tables check themselves: keys that resolve, refcounts that match the
        // cells, cell counts that match the records. See `Table::audit` for why
        // each of those is a rule.
        for table in self.tables() {
            for problem in &table.problems {
                problems.push(format!(
                    "table {} {}: {problem}",
                    table.identifier, table.name
                ));
            }
            problems.extend(table.audit());
        }
        problems
    }

    /// Everything about a Pages document's structure that looks wrong.
    ///
    /// Four rules, each kept by every Pages document in the corpus **and by
    /// all 640 bundled Pages templates**, and each one breakable by an edit
    /// that moves text without thinking about what is anchored to it:
    ///
    /// * **A section that does not begin at 0 begins after a `U+0004`.** The
    ///   break is what makes the section, and an entry that has drifted off one
    ///   is a section boundary the app will not draw.
    /// * **A section's template pages are section templates.** Field 23, 24
    ///   and 25 of `TP.SectionArchive` reach `TP.SectionTemplateArchive`s, and
    ///   a section with none has no headers, footers or page background at all.
    /// * **A section template has three headers and three footers, and every
    ///   one is a storage of kind 1.** Never any other count: 1734 of them
    ///   across the bundled templates and 306 in this corpus.
    /// * **A thread's storage and boxes exist.** A linked-text-box thread that
    ///   names a missing box is a flow with a hole in it.
    fn structure_problems(&self) -> Vec<String> {
        let mut problems = Vec::new();
        let Some(structure) = self.structure() else {
            return problems;
        };
        let body: Vec<u16> = structure
            .body_storage
            .and_then(|id| self.storage_text(id).ok())
            .unwrap_or_default()
            .encode_utf16()
            .collect();

        for section in &structure.sections {
            if section.start > 0 {
                let before = body.get(section.start as usize - 1).copied();
                if before != Some(0x0004) {
                    problems.push(format!(
                        "section {} begins at character {} and the character before it \
                         is not the U+0004 that starts a section",
                        section.identifier, section.start
                    ));
                }
            }
            if section.templates.iter().all(Option::is_none) {
                problems.push(format!(
                    "section {} has no first, even or odd section template",
                    section.identifier
                ));
            }
            for template in section.templates.iter().flatten() {
                match self.object(*template) {
                    Some((_, object))
                        if object.message_type() == crate::pages::TYPE_SECTION_TEMPLATE => {}
                    Some((_, object)) => problems.push(format!(
                        "section {}: page template {template} is type {}, not a \
                         TP.SectionTemplateArchive",
                        section.identifier,
                        object.message_type()
                    )),
                    None => problems.push(format!(
                        "section {}: page template {template} does not exist",
                        section.identifier
                    )),
                }
            }
        }

        let mut zones: BTreeMap<(u64, bool), usize> = BTreeMap::new();
        for entry in &structure.header_footers {
            *zones
                .entry((entry.section_template, entry.footer))
                .or_default() += 1;
            match self.object(entry.storage) {
                Some((_, object)) if object.message_type() == crate::TYPE_STORAGE => {
                    let kind = Message::decode(object.payload())
                        .ok()
                        .and_then(|m| m.varint(1));
                    if kind != Some(1) {
                        problems.push(format!(
                            "{} {} of section template {} is a storage of kind {}, not 1",
                            entry.kind(),
                            entry.storage,
                            entry.section_template,
                            kind.map(|k| k.to_string()).unwrap_or_else(|| "none".into())
                        ));
                    }
                }
                _ => problems.push(format!(
                    "{} {} of section template {} is not a text storage",
                    entry.kind(),
                    entry.storage,
                    entry.section_template
                )),
            }
        }
        for ((template, footer), count) in zones {
            if count != 3 {
                problems.push(format!(
                    "section template {template} has {count} {}(s), not three",
                    if footer { "footer" } else { "header" }
                ));
            }
        }

        for thread in &structure.threads {
            for object in thread.storage.iter().chain(thread.boxes.iter()) {
                if self.object(*object).is_none() {
                    problems.push(format!(
                        "linked-text-box thread {} names missing object {object}",
                        thread.identifier
                    ));
                }
            }
        }
        problems
    }

    /// Everything about drawables and media that looks wrong.
    ///
    /// Five rules, each of them kept by every document in the corpus and each
    /// of them breakable by a writer without the app noticing at open time:
    ///
    /// * **A data reference resolves.** A drawable naming media the registry
    ///   does not list draws nothing, silently.
    /// * **A stored file is there and its digest is its bytes.** The digest is
    ///   a raw SHA-1 and iWork uses it to recognise the file; writing new bytes
    ///   under an old digest is accepted when the document opens and is a lie
    ///   afterwards.
    /// * **`MessageInfo.data_references` lists what the payload uses.** The
    ///   framing carries a packed list of the data ids inside each object, and
    ///   across the corpus it is exactly the set the archive names — 12
    ///   drawables in two documents, no extras and none missing.
    /// * **A mask's parent is the image that names it.** The link is written
    ///   both ways and the pair is meaningless if they disagree.
    /// * **A drawable's parent exists.**
    fn media_problems(&self) -> Vec<String> {
        use crate::pb::Reader;
        let mut problems = Vec::new();
        let files = self.data_files();
        let known: BTreeSet<u64> = files.iter().map(|f| f.identifier).collect();

        for file in &files {
            let Some(entry) = file.entry_name() else {
                continue;
            };
            let Some(bytes) = self.package.get(&entry) else {
                problems.push(format!(
                    "media {} names {entry}, which the package does not have",
                    file.identifier
                ));
                continue;
            };
            if !file.digest.is_empty() && file.digest != crate::media::sha1(bytes) {
                problems.push(format!(
                    "media {}: the digest is not the SHA-1 of {entry}",
                    file.identifier
                ));
            }
        }

        let drawables = self.drawables();
        for drawable in &drawables {
            if let Some(parent) = drawable.parent {
                if self.object(parent).is_none() {
                    problems.push(format!(
                        "drawable {} has parent {parent}, which does not exist",
                        drawable.identifier
                    ));
                }
            }
            if let Some(mask) = drawable.mask() {
                match self.drawable(mask) {
                    Some(mask_drawable) if mask_drawable.parent == Some(drawable.identifier) => {}
                    Some(mask_drawable) => problems.push(format!(
                        "image {} is masked by {mask}, whose parent is {:?}",
                        drawable.identifier, mask_drawable.parent
                    )),
                    None => problems.push(format!(
                        "image {} is masked by {mask}, which does not exist",
                        drawable.identifier
                    )),
                }
            }

            let used: BTreeSet<u64> = drawable
                .media
                .iter()
                .flat_map(|media| [media.data, media.poster])
                .flatten()
                .collect();
            for data in &used {
                if !known.contains(data) {
                    problems.push(format!(
                        "drawable {} refers to media {data}, which the registry does not list",
                        drawable.identifier
                    ));
                }
            }
            if used.is_empty() {
                continue;
            }
            let Some((_, object)) = self.object(drawable.identifier) else {
                continue;
            };
            let declared: BTreeSet<u64> = object
                .messages
                .iter()
                .flat_map(|message| message.extra.iter())
                .filter(|field| field.number == 6)
                .filter_map(|field| match &field.value {
                    Value::Bytes(raw) => Some(raw),
                    _ => None,
                })
                .flat_map(|raw| {
                    let mut reader = Reader::new(raw);
                    let mut out = Vec::new();
                    while !reader.done() {
                        match reader.varint() {
                            Ok(value) => out.push(value),
                            Err(_) => break,
                        }
                    }
                    out
                })
                .collect();
            let missing: Vec<String> = used
                .difference(&declared)
                .map(u64::to_string)
                .collect::<Vec<_>>();
            if !missing.is_empty() {
                problems.push(format!(
                    "drawable {} uses media {} without listing it in the object's \
                     data references",
                    drawable.identifier,
                    missing.join(", ")
                ));
            }
        }
        problems
    }

    // -- components ----------------------------------------------------------

    /// References that leave the component they are written in without being
    /// declared in it, as `(component, referring object, target)`.
    ///
    /// A package is loaded component by component. Within a component an
    /// identifier resolves against the objects in the same stream; to reach an
    /// object in another component, the referring component must list it in its
    /// `TSP.ComponentInfo.external_references` as
    /// `{1: target's component, 2: target}`. iWork keeps that list exact: across
    /// five Pages documents and one Keynote document — some five thousand
    /// objects — every cross-component reference is declared and this returns
    /// nothing.
    ///
    /// An undeclared reference does not always crash. A Pages document that
    /// pointed a paragraph at an undeclared style opened with the paragraph
    /// simply unstyled, as though the edit had never been made. That silence is
    /// the reason to check rather than to trust the file opening.
    ///
    /// A reference to a component's *root* object is not a cross-component
    /// reference — the component identifier is the root object's identifier, and
    /// naming a component is how a component is reached.
    pub fn undeclared_references(&self) -> Vec<(u64, u64, u64)> {
        let Some(index) = self.component_index() else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for (stream, object) in self.objects() {
            let Some(&from) = index.by_stream.get(stream) else {
                continue;
            };
            let mut targets = Vec::new();
            for message in &object.messages {
                if let Ok(archive) = Message::decode(&message.payload) {
                    targets.extend(style::references(&archive));
                }
            }
            targets.sort_unstable();
            targets.dedup();
            for target in targets {
                let Some(&to) = index.by_object.get(&target) else {
                    continue;
                };
                if to == from || index.roots.contains(&target) {
                    continue;
                }
                if !index
                    .declared
                    .get(&from)
                    .is_some_and(|d| d.contains(&target))
                {
                    out.push((from, object.identifier, target));
                }
            }
        }
        out
    }

    /// Declare every reference [`Document::undeclared_references`] reports, so
    /// that the objects an edit points at can actually be loaded.
    ///
    /// Nothing is remembered between calls: the declarations are derived from
    /// the objects as they now stand, so this cannot drift out of step with the
    /// edits, and running it twice adds nothing the second time. Existing
    /// declarations are never removed — iWork's own are kept as written, and a
    /// declaration for an object that is no longer referenced is inert.
    ///
    /// Returns the number of declarations added.
    pub fn declare_external_references(&mut self) -> usize {
        let missing = self.undeclared_references();
        if missing.is_empty() {
            return 0;
        }
        let Some(index) = self.component_index() else {
            return 0;
        };
        let Some((stream, position)) = self
            .objects()
            .find(|(_, o)| o.message_type() == crate::TYPE_PACKAGE_METADATA)
            .map(|(s, o)| (s.to_string(), o.identifier))
            .and_then(|(s, id)| self.locate(id).map(|(_, i)| (s, i)))
        else {
            return 0;
        };

        let object = &mut self
            .streams
            .get_mut(&stream)
            .expect("stream came from the document")[position];
        let message = object.messages.first_mut().expect("metadata has a payload");
        let Ok(mut metadata) = Message::decode(&message.payload) else {
            return 0;
        };

        let mut added = 0;
        for field in &mut metadata.fields {
            if field.number != 3 {
                continue;
            }
            let Value::Bytes(raw) = &field.value else {
                continue;
            };
            let Ok(mut info) = Message::decode(raw) else {
                continue;
            };
            let Some(component) = info.varint(1) else {
                continue;
            };
            let mut wanted: Vec<u64> = missing
                .iter()
                .filter(|(from, _, _)| *from == component)
                .map(|(_, _, target)| *target)
                .collect();
            wanted.sort_unstable();
            wanted.dedup();
            for target in wanted {
                let mut entry = Message::default();
                entry.set(1, Value::Varint(index.by_object[&target]));
                entry.set(2, Value::Varint(target));
                info.append_in_order(6, Value::Bytes(entry.encode()));
                added += 1;
            }
            if added > 0 {
                field.value = Value::Bytes(info.encode());
            }
        }
        if added > 0 {
            message.payload = metadata.encode();
        }
        added
    }

    /// Which component each stream and each object belongs to, and what each
    /// component already declares.
    fn component_index(&self) -> Option<ComponentIndex> {
        let metadata = self.package_metadata()?;
        let mut index = ComponentIndex::default();
        for component in self.components() {
            index.roots.insert(component.identifier);
            index
                .by_stream
                .insert(component.stream_name(), component.identifier);
        }
        for (stream, object) in self.objects() {
            if let Some(&component) = index.by_stream.get(stream) {
                index.by_object.insert(object.identifier, component);
            }
        }
        for value in metadata.all(3) {
            let Value::Bytes(raw) = value else { continue };
            let Ok(info) = Message::decode(raw) else {
                continue;
            };
            let Some(component) = info.varint(1) else {
                continue;
            };
            let declared = index.declared.entry(component).or_default();
            for value in info.all(6) {
                let Value::Bytes(raw) = value else { continue };
                if let Ok(entry) = Message::decode(raw) {
                    if let Some(target) = entry.varint(2) {
                        declared.insert(target);
                    }
                }
            }
        }
        Some(index)
    }

    /// Identifier a new object may take: above every identifier in use and above
    /// the package's own high-water mark.
    pub fn next_object_identifier(&self) -> u64 {
        let highest = self
            .objects()
            .map(|(_, object)| object.identifier)
            .max()
            .unwrap_or(0);
        highest.max(self.last_object_identifier().unwrap_or(0)) + 1
    }

    // -- internals -----------------------------------------------------------

    /// Stream and position of an object, for the methods that rewrite one.
    fn locate(&self, identifier: u64) -> Option<(String, usize)> {
        self.streams.iter().find_map(|(name, objects)| {
            objects
                .iter()
                .position(|object| object.identifier == identifier)
                .map(|index| (name.clone(), index))
        })
    }

    /// Decode any object's first message, at the wire level.
    ///
    /// The escape hatch for a caller that knows a message this crate does not
    /// model: the fields come back as they were written, in the order they were
    /// written, and an unrecognised one is a field like any other.
    pub fn archive(&self, identifier: u64) -> Result<Message, Error> {
        self.archive_of(identifier)
    }

    /// Decode any object's first message.
    fn archive_of(&self, identifier: u64) -> Result<Message, Error> {
        let (name, object) = self
            .object(identifier)
            .ok_or(Error::NoSuchObject(identifier))?;
        let message = object
            .messages
            .first()
            .ok_or_else(|| Error::Format(format!("object {identifier} carries no message")))?;
        Message::decode(&message.payload)
            .map_err(|e| Error::Format(format!("{name}: object {identifier}: {e}")))
    }

    fn storage_archive(&self, identifier: u64) -> Result<Message, Error> {
        let (name, object) = self
            .object(identifier)
            .ok_or(Error::NoSuchObject(identifier))?;
        if object.message_type() != crate::TYPE_STORAGE {
            return Err(Error::Format(format!(
                "object {identifier} is not a text storage"
            )));
        }
        Message::decode(object.payload())
            .map_err(|e| Error::Format(format!("{name}: storage {identifier}: {e}")))
    }

    fn set_archive(&mut self, identifier: u64, archive: &Message) -> Result<(), Error> {
        let (stream, index) = self
            .locate(identifier)
            .ok_or(Error::NoSuchObject(identifier))?;
        let object = &mut self
            .streams
            .get_mut(&stream)
            .expect("stream came from the document")[index];
        let message = object
            .messages
            .first_mut()
            .ok_or_else(|| Error::Format(format!("object {identifier} carries no message")))?;
        message.payload = archive.encode();
        Ok(())
    }

    /// Raise `TSP.PackageMetadata` field 1 so iWork does not reissue an
    /// identifier this crate has already handed out.
    fn set_last_object_identifier(&mut self, value: u64) -> Result<(), Error> {
        let metadata = self
            .objects()
            .find(|(_, object)| object.message_type() == crate::TYPE_PACKAGE_METADATA)
            .map(|(_, object)| object.identifier)
            .ok_or_else(|| {
                Error::Format(
                    "no TSP.PackageMetadata: cannot allocate an object identifier safely".into(),
                )
            })?;
        let (stream, index) = self.locate(metadata).expect("the object was just found");
        let object = &mut self
            .streams
            .get_mut(&stream)
            .expect("stream came from the document")[index];
        let message = object
            .messages
            .first_mut()
            .expect("the message type came from this message");
        let mut archive = Message::decode(&message.payload)
            .map_err(|e| Error::Format(format!("package metadata: {e}")))?;
        if archive.varint(1).unwrap_or(0) < value {
            archive.set(1, Value::Varint(value));
            message.payload = archive.encode();
        }
        Ok(())
    }

    /// Write the package out, re-encoding only the streams that changed.
    ///
    /// A stream whose objects still frame to the bytes they were read from
    /// keeps its **original entry, byte for byte** — not merely an equivalent
    /// one. That matters beyond saving work: re-compressing an untouched stream
    /// moves every Snappy block boundary in it, so a document edited in one
    /// place would otherwise differ from the original everywhere, and there
    /// would be no way to tell an intended change from an incidental one by
    /// looking at the file. Editing one style in a 97-stream Numbers document
    /// now rewrites one stream.
    ///
    /// Nothing is remembered to make this work: the comparison is against the
    /// bytes actually in the package, so it cannot drift out of step with the
    /// edits the way a dirty flag can.
    pub fn save(&self, path: impl AsRef<std::path::Path>) -> Result<(), Error> {
        let mut package = self.package.clone();
        for (name, objects) in &self.streams {
            let framed = iwa::serialize_stream(objects);
            if !self.stream_matches(name, &framed) {
                package.set(name, iwa::compress(&framed));
            }
        }
        package.write(path)
    }

    /// Streams whose objects no longer match the bytes they were read from —
    /// exactly the entries [`Document::save`] would rewrite.
    pub fn changed_streams(&self) -> Vec<&str> {
        self.streams
            .iter()
            .filter(|(name, objects)| !self.stream_matches(name, &iwa::serialize_stream(objects)))
            .map(|(name, _)| name.as_str())
            .collect()
    }

    /// Does the package still hold exactly these framed bytes for `name`?
    fn stream_matches(&self, name: &str, framed: &[u8]) -> bool {
        self.package
            .get(name)
            .and_then(|raw| iwa::decompress(raw).ok())
            .is_some_and(|original| original == framed)
    }
}

/// A float field, or zero.
fn float_field(message: &Message, number: u32) -> f32 {
    match message.get(number) {
        Some(Value::Fixed32(bytes)) => f32::from_le_bytes(*bytes),
        _ => 0.0,
    }
}

/// Scale every point of a baked path, when the path is drawn in the space the
/// natural size describes.
fn scale_path_points(body: &mut Message, field: u32, was: (f32, f32), scale: (f32, f32)) {
    let Some(mut path) = body.bytes(field).and_then(crate::pb::decode_nested) else {
        return;
    };
    let mut extent = (0.0f32, 0.0f32);
    let mut points: Vec<Message> = Vec::new();
    for value in path.all(1) {
        let Value::Bytes(raw) = value else { return };
        let Some(element) = crate::pb::decode_nested(raw) else {
            return;
        };
        if let Some(point) = element.bytes(2).and_then(crate::pb::decode_nested) {
            extent.0 = extent.0.max(float_field(&point, 1));
            extent.1 = extent.1.max(float_field(&point, 2));
        }
        points.push(element);
    }
    // Not drawn in the frame's coordinates: leave it exactly as it is.
    let close = |a: f32, b: f32| (a - b).abs() <= (a.abs().max(b.abs()) * 1e-3).max(1e-3);
    if !close(extent.0, was.0) || !close(extent.1, was.1) {
        return;
    }
    for element in &mut points {
        let Some(mut point) = element.bytes(2).and_then(crate::pb::decode_nested) else {
            continue;
        };
        point.set_in_order(
            1,
            Value::Fixed32((float_field(&point, 1) * scale.0).to_le_bytes()),
        );
        point.set_in_order(
            2,
            Value::Fixed32((float_field(&point, 2) * scale.1).to_le_bytes()),
        );
        element.set_in_order(2, Value::Bytes(point.encode()));
    }
    path.clear(1);
    for element in points {
        path.append_in_order(1, Value::Bytes(element.encode()));
    }
    body.set_in_order(field, Value::Bytes(path.encode()));
}

fn dotted(path: &[u32]) -> String {
    path.iter()
        .map(u32::to_string)
        .collect::<Vec<_>>()
        .join(".")
}

fn utf8(bytes: Option<&[u8]>) -> String {
    bytes
        .map(|b| String::from_utf8_lossy(b).into_owned())
        .unwrap_or_default()
}

/// The characters of `text` in a UTF-16 range, as far as the range is real.
fn slice(text: &str, range: Range<u64>) -> String {
    let from = crate::text::utf16_offset(text, range.start).unwrap_or(text.len());
    let to = crate::text::utf16_offset(text, range.end).unwrap_or(text.len());
    text.get(from..to).unwrap_or_default().to_string()
}

/// Identify the app from the object graph alone.
///
/// Used when there is no filename to go by, and it cannot lean on the root
/// archive's type alone: **Numbers and Keynote both put message type 1 at
/// object 1.** The app-level archives are numbered per app, so a `1` means
/// `TN.DocumentArchive` in one and `KN.DocumentArchive` in the other, and a
/// Keynote deck read by root type alone comes back as a spreadsheet.
///
/// The components are what separate them, and they are unambiguous: Numbers
/// spreads tables over `Index/Tables/`, Keynote writes a stream per slide and
/// per master. Pages is the one app whose root type is its own — 10000
/// `TP.DocumentArchive`.
fn detect_kind(package: &Package, streams: &BTreeMap<String, Vec<ArchiveObject>>) -> Kind {
    if package.names().any(|n| n.starts_with("Index/Tables/")) {
        return Kind::Numbers;
    }
    if package
        .names()
        .any(|n| n.starts_with("Index/Slide") || n.starts_with("Index/TemplateSlide"))
    {
        return Kind::Keynote;
    }
    let root_type = streams
        .get("Index/Document.iwa")
        .and_then(|objects| objects.iter().find(|o| o.identifier == 1))
        .map(ArchiveObject::message_type);
    match root_type {
        Some(10000) => Kind::Pages,
        // Ambiguous on its own, but a Keynote package has slide streams and has
        // already been caught above.
        Some(1) => Kind::Numbers,
        _ => Kind::Unknown,
    }
}
