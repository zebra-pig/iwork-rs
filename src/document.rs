//! Document-level view over a package: components, media and text.

use std::collections::BTreeMap;

use crate::iwa::{self, ArchiveObject};
use crate::package::Package;
use crate::pb::{Message, Value};
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

    /// Re-encode every modified stream and write the package out.
    pub fn save(&self, path: impl AsRef<std::path::Path>) -> Result<(), Error> {
        let mut package = self.package.clone();
        for (name, objects) in &self.streams {
            package.set(name, iwa::serialize(objects));
        }
        package.write(path)
    }
}

fn utf8(bytes: Option<&[u8]>) -> String {
    bytes
        .map(|b| String::from_utf8_lossy(b).into_owned())
        .unwrap_or_default()
}

/// Identify the app from the object graph alone.
///
/// Used when there is no filename to go by. The root archive is the strongest
/// signal: Pages writes `TP.DocumentArchive` (10000) as object 1, Numbers
/// writes `TN.DocumentArchive` (1). Numbers is confirmed further by its
/// `Index/Tables/` components, which the other two apps do not produce.
fn detect_kind(package: &Package, streams: &BTreeMap<String, Vec<ArchiveObject>>) -> Kind {
    if package.names().any(|n| n.starts_with("Index/Tables/")) {
        return Kind::Numbers;
    }
    let root_type = streams
        .get("Index/Document.iwa")
        .and_then(|objects| objects.iter().find(|o| o.identifier == 1))
        .map(ArchiveObject::message_type);
    match root_type {
        Some(10000) => Kind::Pages,
        Some(1) => Kind::Numbers,
        _ => Kind::Unknown,
    }
}
