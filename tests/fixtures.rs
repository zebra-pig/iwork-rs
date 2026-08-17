//! Integration tests against real documents.
//!
//! No iWork files are committed to this repository — they are other people's
//! documents, and the ones used during development are copyrighted. Drop any
//! `.pages`, `.numbers` or `.key` files into `tests/fixtures/` (or point
//! `IWORK_FIXTURES` at a directory) and these tests will exercise every one of
//! them. With no fixtures present they pass without asserting anything, and say
//! so.

use std::path::{Path, PathBuf};

use iwork::pb::Message;
use iwork::{iwa, style, Document, Kind, Package, StyleKind};

fn fixtures() -> Vec<PathBuf> {
    let dir = std::env::var("IWORK_FIXTURES")
        .map(PathBuf::from)
        .unwrap_or_else(|_| Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures"));
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return Vec::new();
    };
    let mut found: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| {
            matches!(
                p.extension().and_then(|e| e.to_str()),
                Some("pages") | Some("numbers") | Some("key")
            )
        })
        .collect();
    found.sort();
    found
}

/// Print once per test run rather than failing, so a fresh clone is green.
fn require_fixtures() -> Vec<PathBuf> {
    let found = fixtures();
    if found.is_empty() {
        eprintln!("no fixtures in tests/fixtures — skipping (see tests/fixtures/README.md)");
    }
    found
}

#[test]
fn opens_and_identifies_every_fixture() {
    for path in require_fixtures() {
        let doc = Document::open(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        assert_ne!(
            doc.kind(),
            Kind::Unknown,
            "{}: could not identify the document",
            path.display()
        );
        assert!(
            doc.objects().count() > 0,
            "{}: no objects decoded",
            path.display()
        );
        assert!(
            doc.package().contains("Metadata/DocumentIdentifier"),
            "{}: not an iWork package",
            path.display()
        );
    }
}

/// Every entry in an iWork package is stored, never deflated, and this crate
/// must keep it that way — the media is meant to be mappable in place.
#[test]
fn writes_only_stored_entries() {
    for path in require_fixtures() {
        let doc = Document::open(&path).unwrap();
        let out = std::env::temp_dir().join(format!(
            "iwork-stored-{}",
            path.file_name().unwrap().to_string_lossy()
        ));
        doc.save(&out).unwrap();

        let bytes = std::fs::read(&out).unwrap();
        let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
        for i in 0..zip.len() {
            let entry = zip.by_index(i).unwrap();
            assert_eq!(
                entry.compression(),
                zip::CompressionMethod::Stored,
                "{}: {} was compressed",
                path.display(),
                entry.name()
            );
        }
        let _ = std::fs::remove_file(&out);
    }
}

/// Decoding and re-encoding must reproduce each object stream byte for byte.
/// Only the Snappy block boundaries may move, which no reader can observe.
#[test]
fn object_streams_survive_a_roundtrip() {
    for path in require_fixtures() {
        let original = Package::read(&path).unwrap();
        let doc = Document::open(&path).unwrap();
        let out = std::env::temp_dir().join(format!(
            "iwork-roundtrip-{}",
            path.file_name().unwrap().to_string_lossy()
        ));
        doc.save(&out).unwrap();
        let rewritten = Package::read(&out).unwrap();

        assert_eq!(
            original.names().collect::<Vec<_>>(),
            rewritten.names().collect::<Vec<_>>(),
            "{}: entry names or order changed",
            path.display()
        );

        for name in original.names() {
            let before = original.get(name).unwrap();
            let after = rewritten.get(name).unwrap();
            if name.ends_with(".iwa") {
                assert_eq!(
                    iwa::decompress(before).unwrap(),
                    iwa::decompress(after).unwrap(),
                    "{}: {name} changed on re-encode",
                    path.display()
                );
            } else {
                assert_eq!(before, after, "{}: {name} changed", path.display());
            }
        }
        let _ = std::fs::remove_file(&out);
    }
}

/// Editing one storage must leave every other object in the document alone.
#[test]
fn editing_text_touches_only_its_own_stream() {
    for path in require_fixtures() {
        let doc = Document::open(&path).unwrap();
        let Some(target) = doc.text_storages().into_iter().next() else {
            continue; // a document with no text is fine, just not useful here
        };

        let mut edited = Document::open(&path).unwrap();
        let replacement = "Ersetzt durch iwork-rs — 🎬";
        edited.set_text(target.identifier, replacement).unwrap();
        let out = std::env::temp_dir().join(format!(
            "iwork-edit-{}",
            path.file_name().unwrap().to_string_lossy()
        ));
        edited.save(&out).unwrap();

        let reopened = Document::open(&out).unwrap();
        let found = reopened
            .text_storages()
            .into_iter()
            .find(|s| s.identifier == target.identifier)
            .expect("edited storage is still there");
        assert_eq!(found.text, replacement, "{}", path.display());

        assert_eq!(
            reopened.objects().count(),
            doc.objects().count(),
            "{}: object count changed",
            path.display()
        );

        // Only the stream holding the edited storage may differ.
        let before = Package::read(&path).unwrap();
        let after = Package::read(&out).unwrap();
        for name in before.names().filter(|n| n.ends_with(".iwa")) {
            if name == target.stream {
                continue;
            }
            assert_eq!(
                iwa::decompress(before.get(name).unwrap()).unwrap(),
                iwa::decompress(after.get(name).unwrap()).unwrap(),
                "{}: {name} changed while editing {}",
                path.display(),
                target.stream
            );
        }
        let _ = std::fs::remove_file(&out);
    }
}

/// Components must resolve to entries that actually exist. Numbers is the real
/// test here: it spreads ~100 components over `Index/Tables/`.
#[test]
fn components_resolve_to_real_streams() {
    for path in require_fixtures() {
        let doc = Document::open(&path).unwrap();
        for component in doc.components() {
            let name = component.stream_name();
            assert!(
                doc.package().contains(&name),
                "{}: component {} ({}) points at missing {name}",
                path.display(),
                component.identifier,
                component.preferred_name
            );
        }
    }
}

/// Every style a run of text points at must be an object that exists, and the
/// three tables must point at the kind of style this crate says they do. This
/// is the claim `style::StyleKind::attribute_table` rests on, so a document
/// that disagrees should fail loudly rather than be edited on a wrong premise.
#[test]
fn attribute_tables_point_at_styles_of_the_matching_kind() {
    for path in require_fixtures() {
        let doc = Document::open(&path).unwrap();
        let styles: Vec<_> = doc
            .text_styles()
            .into_iter()
            .map(|s| (s.identifier, s.kind))
            .collect();

        for (stream, object) in doc.objects() {
            if object.message_type() != iwork::TYPE_STORAGE {
                continue;
            }
            let Ok(storage) = Message::decode(object.payload()) else {
                continue;
            };
            for kind in [StyleKind::Character, StyleKind::Paragraph, StyleKind::List] {
                let Some(table) = storage
                    .bytes(kind.attribute_table())
                    .and_then(iwork::pb::decode_nested)
                else {
                    continue;
                };
                for run in style::runs(&table) {
                    let Some(target) = run.style else { continue };
                    assert!(
                        doc.object(target).is_some(),
                        "{}: {stream} storage {} points at missing object {target}",
                        path.display(),
                        object.identifier
                    );
                    if let Some((_, found)) = styles.iter().find(|(id, _)| *id == target) {
                        assert_eq!(
                            *found,
                            kind,
                            "{}: storage {} field {} points at a {} style",
                            path.display(),
                            object.identifier,
                            kind.attribute_table(),
                            found.as_str()
                        );
                    }
                }
            }
        }
    }
}

/// Copying a style must add exactly one object, leave the text alone, and take
/// an identifier the document has not used.
#[test]
fn creating_a_style_adds_one_object_and_nothing_else() {
    for path in require_fixtures() {
        let doc = Document::open(&path).unwrap();
        let Some(template) = doc.text_styles().into_iter().next() else {
            continue; // a document with no styles is fine, just not useful here
        };
        let used: Vec<u64> = doc.objects().map(|(_, o)| o.identifier).collect();

        let mut edited = Document::open(&path).unwrap();
        let created = edited
            .create_text_style(template.identifier, "iwork-rs")
            .unwrap();
        assert!(
            !used.contains(&created.identifier),
            "{}: identifier {} was already in use",
            path.display(),
            created.identifier
        );

        let out = std::env::temp_dir().join(format!(
            "iwork-new-style-{}",
            path.file_name().unwrap().to_string_lossy()
        ));
        edited.save(&out).unwrap();

        let reopened = Document::open(&out).unwrap();
        assert_eq!(
            reopened.objects().count(),
            doc.objects().count() + 1,
            "{}: object count",
            path.display()
        );
        let new = reopened.text_style(created.identifier).unwrap();
        assert_eq!(new.kind, template.kind, "{}", path.display());
        assert_eq!(new.name(), Some("iwork-rs"), "{}", path.display());
        assert_eq!(
            reopened.last_object_identifier(),
            Some(created.identifier),
            "{}: high-water mark was not bumped",
            path.display()
        );

        let before: Vec<_> = doc.text_storages().into_iter().map(|s| s.text).collect();
        let after: Vec<_> = reopened
            .text_storages()
            .into_iter()
            .map(|s| s.text)
            .collect();
        assert_eq!(before, after, "{}: text changed", path.display());
        let _ = std::fs::remove_file(&out);
    }
}

/// Applying a style must change which style a range uses and nothing else —
/// not the text, not the object count, not the other streams.
#[test]
fn applying_a_style_touches_only_its_own_stream() {
    for path in require_fixtures() {
        let doc = Document::open(&path).unwrap();
        let Some(target) = doc.text_storages().into_iter().next() else {
            continue;
        };
        let Some(style) = doc
            .text_styles()
            .into_iter()
            .find(|s| s.kind == StyleKind::Character)
        else {
            continue;
        };

        let mut edited = Document::open(&path).unwrap();
        let range = 0..1.min(target.text.encode_utf16().count() as u64);
        if range.is_empty() {
            continue;
        }
        edited
            .apply_text_style(target.identifier, range.clone(), style.identifier)
            .unwrap();
        let out = std::env::temp_dir().join(format!(
            "iwork-apply-style-{}",
            path.file_name().unwrap().to_string_lossy()
        ));
        edited.save(&out).unwrap();

        let reopened = Document::open(&out).unwrap();
        assert_eq!(
            reopened.storage_text(target.identifier).unwrap(),
            doc.storage_text(target.identifier).unwrap(),
            "{}: text changed",
            path.display()
        );
        assert_eq!(
            reopened.objects().count(),
            doc.objects().count(),
            "{}: object count changed",
            path.display()
        );
        assert!(
            reopened
                .text_style_usage(style.identifier)
                .iter()
                .any(|u| u.storage == target.identifier && u.range.start == range.start),
            "{}: the run was not applied",
            path.display()
        );

        let before = Package::read(&path).unwrap();
        let after = Package::read(&out).unwrap();
        for name in before.names().filter(|n| n.ends_with(".iwa")) {
            if name == target.stream {
                continue;
            }
            assert_eq!(
                iwa::decompress(before.get(name).unwrap()).unwrap(),
                iwa::decompress(after.get(name).unwrap()).unwrap(),
                "{}: {name} changed while styling {}",
                path.display(),
                target.stream
            );
        }
        let _ = std::fs::remove_file(&out);
    }
}

/// Media registered in `TSP.PackageMetadata` must be present, except for theme
/// assets, which are referenced by path and deliberately not stored.
#[test]
fn registered_media_is_present() {
    for path in require_fixtures() {
        let doc = Document::open(&path).unwrap();
        for data in doc.data_files() {
            let Some(entry) = data.entry_name() else {
                assert!(
                    !data.asset_path.is_empty(),
                    "{}: media {} is neither stored nor a theme asset",
                    path.display(),
                    data.identifier
                );
                continue;
            };
            assert!(
                doc.package().contains(&entry),
                "{}: media {} points at missing {entry}",
                path.display(),
                data.identifier
            );
        }
    }
}
