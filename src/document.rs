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
    /// The attribute-run tables are rewritten to stay within the new length —
    /// see [`crate::text::write`] for what that does and does not preserve.
    pub fn set_text(&mut self, identifier: u64, new_text: &str) -> Result<(), Error> {
        for (name, objects) in self.streams.iter_mut() {
            for object in objects.iter_mut() {
                if object.identifier != identifier || object.message_type() != crate::TYPE_STORAGE {
                    continue;
                }
                let mut storage = Message::decode(object.payload())
                    .map_err(|e| Error::Format(format!("{name}: storage {identifier}: {e}")))?;
                text::write(&mut storage, new_text);
                object.messages[0].payload = storage.encode();
                return Ok(());
            }
        }
        Err(Error::NoSuchObject(identifier))
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
        let length = text::length(&text::read(&archive));
        let table_field = kind.attribute_table();

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
            let length = text::length(&text::read(&storage));
            for &field in text::ATTRIBUTE_TABLES {
                let Some(table) = storage.bytes(field).and_then(crate::pb::decode_nested) else {
                    continue;
                };
                let mut previous: Option<u64> = None;
                for run in style::runs(&table) {
                    let where_ = format!("{stream} storage {} table {field}", object.identifier);
                    if previous.is_some_and(|p| run.start <= p) {
                        problems.push(format!(
                            "{where_}: run index {} does not increase (after {})",
                            run.start,
                            previous.unwrap()
                        ));
                    }
                    previous = Some(run.start);
                    if run.start > length {
                        problems.push(format!(
                            "{where_}: run starts at {} but the text is {length} long",
                            run.start
                        ));
                    }
                    let Some(target) = run.style else { continue };
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

        for (component, object, target) in self.undeclared_references() {
            problems.push(format!(
                "object {object} refers to {target} in another component, which \
                 component {component} does not declare"
            ));
        }

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
