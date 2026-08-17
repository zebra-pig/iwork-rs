//! Document-level view over a package: components, media, text and styles.

use std::collections::BTreeMap;
use std::ops::Range;

use crate::iwa::{self, ArchiveObject};
use crate::package::Package;
use crate::pb::{Message, Value};
use crate::style::{self, CreatedStyle, StyleDeletion, StyleKind, StyleUse, TextStyle};
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
                if registrations_cloned > 0 {
                    self.set_archive(stylesheet, &sheet)?;
                }
            }
        }

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
                let removed = style::remove_registrations(&mut edited, identifier);
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
        self.set_archive(storage, &archive)
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

        // Style archives must resolve their parent and stylesheet.
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
        }
        problems
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
