//! Editing text: what moves with it, and what is refused.
//!
//! The unit tests in `src/text.rs` prove the remapper against storages built by
//! hand, and against the ten archives Pages wrote when it was made to perform
//! the same edits. These are the ones that need whole documents: that an edit
//! keeps the invariants `iwork check` enforces, that it touches one stream, that
//! everything anchored into the storage is still where it belongs, and — behind
//! `IWORK_APP_CHECK=1` — that the app opens the result and reads the new words
//! back.

use std::path::{Path, PathBuf};

use iwork::text::Anchoring;
use iwork::{style::StyleKind, Document, Error};

fn generated(name: &str) -> Option<PathBuf> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/generated")
        .join(name);
    path.exists().then_some(path)
}

macro_rules! fixture {
    ($name:expr) => {
        match generated($name) {
            Some(path) => path,
            None => {
                eprintln!("no {} — skipping (run scripts/make-fixtures.sh)", $name);
                return;
            }
        }
    };
}

/// A password-protected package, which is not a fixture any test here can use:
/// its object streams are ciphertext and `Document::open` refuses it by design.
/// `tests/fixtures.rs` is where that refusal is asserted.
fn encrypted(path: &Path) -> bool {
    iwork::Package::read(path).is_ok_and(|package| package.contains(".iwpv2"))
}

fn every_fixture() -> Vec<PathBuf> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/generated");
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
        .filter(|path| !encrypted(path))
        .collect();
    found.sort();
    found
}

/// Write to a scratch file beside the fixtures and reopen it.
fn reopen(doc: &Document, name: &str) -> Document {
    let out = std::env::temp_dir().join(format!("iwork-text-{name}"));
    doc.save(&out).unwrap();
    let reopened = Document::open(&out).unwrap();
    let _ = std::fs::remove_file(&out);
    reopened
}

// -- the corpus, edited ------------------------------------------------------

/// Every storage in the corpus, edited in the middle, still keeps every rule
/// `iwork check` knows.
///
/// This is the test the old clamping would fail: pull four paragraph runs down
/// onto one index and the paragraph table stops sitting on paragraph starts.
#[test]
fn an_edit_anywhere_leaves_a_document_that_checks_out() {
    let mut edited = 0;
    for path in every_fixture() {
        let doc = Document::open(&path).unwrap();
        for storage in doc.storages() {
            if storage.length < 8 || storage.unknown_field.is_some() {
                continue;
            }
            let middle = storage.length / 2;
            for edit in [(middle, 0, "eingefügt"), (middle, 4, ""), (0, 3, "abc")] {
                let mut copy = Document::open(&path).unwrap();
                let outcome =
                    copy.replace_text(storage.identifier, edit.0..edit.0 + edit.1, edit.2);
                match outcome {
                    // A range covering an anchor is refused by name and the
                    // document is left exactly as it was.
                    Err(Error::AnchoredObject { .. }) | Err(Error::SplitSurrogate { .. }) => {
                        assert!(
                            copy.changed_streams().is_empty(),
                            "{}: a refused edit changed a stream",
                            path.display()
                        );
                        continue;
                    }
                    Err(e) => panic!("{}: storage {}: {e}", path.display(), storage.identifier),
                    Ok(_) => {}
                }
                assert_eq!(
                    copy.problems(),
                    Vec::<String>::new(),
                    "{}: storage {} after {edit:?}",
                    path.display(),
                    storage.identifier
                );
                edited += 1;
            }
        }
    }
    assert!(edited > 100, "only {edited} edits were made");
}

/// One edit, one stream — and every other object byte for byte.
#[test]
fn an_edit_touches_only_the_stream_it_is_in() {
    let path = fixture!("pages-styled.pages");
    let doc = Document::open(&path).unwrap();
    let storage = doc.text_storages().into_iter().next().unwrap();

    let mut edited = Document::open(&path).unwrap();
    edited
        .insert_text(storage.identifier, 5, "eingeschoben")
        .unwrap();
    assert_eq!(edited.changed_streams(), vec![storage.stream.as_str()]);

    // And an edit that changes nothing rewrites nothing.
    let mut untouched = Document::open(&path).unwrap();
    untouched.insert_text(storage.identifier, 5, "").unwrap();
    assert!(untouched.changed_streams().is_empty());
}

/// The paragraph tables keep one entry per paragraph across a paragraph being
/// created and a paragraph being destroyed.
#[test]
fn paragraph_bookkeeping_survives_both_directions() {
    let path = fixture!("pages-styled.pages");
    let doc = Document::open(&path).unwrap();
    let storage = doc.text_storages().into_iter().next().unwrap().identifier;
    let before = doc.paragraph_ranges(storage).unwrap().len();

    let mut split = Document::open(&path).unwrap();
    let report = split.insert_text(storage, 20, "\rNeuer Absatz").unwrap();
    assert_eq!(split.paragraph_ranges(storage).unwrap().len(), before + 1);
    assert_eq!(report.report.added, 1, "the new paragraph got an entry");
    assert_eq!(split.problems(), Vec::<String>::new());

    // Deleting the break at 11 merges the first two paragraphs, and the entry
    // that began the second one goes with it.
    let mut merged = Document::open(&path).unwrap();
    let report = merged.delete_text(storage, 11..12).unwrap();
    assert_eq!(merged.paragraph_ranges(storage).unwrap().len(), before - 1);
    assert_eq!(report.report.dropped, 1);
    assert_eq!(merged.problems(), Vec::<String>::new());
}

/// Styles keep the words they were on. The storage carries four paragraph
/// styles; inserting into the middle of the second must leave the third and
/// fourth on the same words they started on.
#[test]
fn a_style_stays_on_its_own_words() {
    let path = fixture!("pages-styled.pages");
    let mut doc = Document::open(&path).unwrap();
    let storage = doc.text_storages().into_iter().next().unwrap().identifier;

    let before: Vec<(String, Option<u64>)> = doc
        .paragraph_ranges(storage)
        .unwrap()
        .into_iter()
        .map(|range| {
            let text = doc.storage_text(storage).unwrap();
            let style = doc
                .style_of_run(storage, range.start, StyleKind::Paragraph)
                .unwrap()
                .map(|r| r.style);
            (text[..].to_string().len().to_string(), style)
        })
        .collect();

    doc.insert_text(storage, 30, "XXXXX").unwrap();
    let after: Vec<Option<u64>> = doc
        .paragraph_ranges(storage)
        .unwrap()
        .into_iter()
        .map(|range| {
            doc.style_of_run(storage, range.start, StyleKind::Paragraph)
                .unwrap()
                .map(|r| r.style)
        })
        .collect();
    assert_eq!(
        after,
        before.iter().map(|(_, s)| *s).collect::<Vec<_>>(),
        "the paragraphs must still be in the styles they were in"
    );
}

/// An anchored image is not silently detached: the edit is refused, by name,
/// and the drawable is still anchored where it was.
#[test]
fn text_that_anchors_an_object_is_not_replaced() {
    let path = fixture!("pages-report.pages");
    let mut doc = Document::open(&path).unwrap();
    let body = doc.text_storages().into_iter().next().unwrap().identifier;

    // The photo is anchored at character 12 of the body storage.
    let anchor = doc
        .drawables()
        .into_iter()
        .filter_map(|d| match d.placement {
            // The photo, at character 12. The report's table is anchored too,
            // at 1500, which is the last character of the storage.
            iwork::Placement::InText { storage, character }
                if storage == body && d.kind == iwork::drawable::Kind::Image =>
            {
                Some(character)
            }
            _ => None,
        })
        .next()
        .expect("the report has an image anchored in its text");

    match doc.delete_text(body, anchor..anchor + 2) {
        Err(Error::AnchoredObject { index, table, .. }) => {
            assert_eq!(index, anchor);
            assert_eq!(table, "table_attachment");
        }
        other => panic!("expected a named refusal, got {other:?}"),
    }
    assert!(doc.changed_streams().is_empty());

    // Deleting text that does not cover the anchor is fine, and moves it.
    let moved = doc.delete_text(body, anchor + 2..anchor + 6).unwrap();
    assert!(moved.report.moved > 0);
    assert_eq!(doc.problems(), Vec::<String>::new());
}

/// Indices are UTF-16 code units, and an edit may not land inside a pair.
#[test]
fn an_edit_inside_a_surrogate_pair_is_refused() {
    let path = fixture!("pages-unicode.pages");
    let mut doc = Document::open(&path).unwrap();
    let storage = doc.text_storages().into_iter().next().unwrap();
    let text = storage.text.clone();

    // The first astral character in the fixture, and the index of its low half.
    let mut units = 0u64;
    let mut split = None;
    for character in text.chars() {
        if character.len_utf16() == 2 {
            split = Some(units + 1);
            break;
        }
        units += character.len_utf16() as u64;
    }
    let split = split.expect("the unicode fixture has an emoji");

    match doc.insert_text(storage.identifier, split, "x") {
        Err(Error::SplitSurrogate { index, .. }) => assert_eq!(index, split),
        other => panic!("expected a named refusal, got {other:?}"),
    }
    // One code unit either side is fine, and the text comes back intact.
    doc.insert_text(storage.identifier, split - 1, "x").unwrap();
    let after = doc.storage_text(storage.identifier).unwrap();
    assert!(after.chars().count() == text.chars().count() + 1);
    assert_eq!(doc.problems(), Vec::<String>::new());
}

/// Characters that stand for objects are not text and are not written.
#[test]
fn a_placeholder_character_is_not_writable() {
    let path = fixture!("pages-plain.pages");
    let mut doc = Document::open(&path).unwrap();
    let storage = doc.text_storages().into_iter().next().unwrap().identifier;
    for character in iwork::text::UNWRITABLE {
        match doc.insert_text(storage, 0, &character.to_string()) {
            Err(Error::UnwritableCharacter { character: found }) => assert_eq!(found, *character),
            other => panic!("expected a named refusal for {character:?}, got {other:?}"),
        }
    }
}

// -- what a storage carries --------------------------------------------------

/// Every storage in the corpus is made of fields this crate can place. A field
/// it cannot place is a table an edit would remap by guesswork, and the whole
/// point of the inventory is that there is nowhere for one to hide.
#[test]
fn no_storage_in_the_corpus_carries_an_unknown_table() {
    let mut seen: std::collections::BTreeSet<u32> = Default::default();
    for path in every_fixture() {
        let doc = Document::open(&path).unwrap();
        for storage in doc.storages() {
            assert_eq!(
                storage.unknown_field,
                None,
                "{}: storage {}",
                path.display(),
                storage.identifier
            );
            seen.extend(storage.tables.iter().map(|t| t.field));
        }
    }
    // What the corpus actually exercises, so a change to it is visible.
    assert_eq!(
        seen.into_iter().collect::<Vec<u32>>(),
        vec![5, 6, 7, 8, 9, 11, 12, 14, 17, 19, 24, 28]
    );
}

/// Hyperlinks: read, and their target changed.
#[test]
fn a_hyperlink_is_read_and_repointed() {
    let path = fixture!("numbers-links.numbers");
    let mut doc = Document::open(&path).unwrap();

    let links: Vec<iwork::document::SmartField> = doc
        .smart_fields()
        .into_iter()
        .filter(|f| f.message_type == iwork::document::TYPE_HYPERLINK_FIELD)
        .collect();
    assert_eq!(links.len(), 2, "the invoice template has two links");
    assert_eq!(
        links[0].payload.as_deref(),
        Some("mailto:no_reply@example.com")
    );
    assert_eq!(links[0].text, "no_reply@example.com");
    assert_eq!(links[1].payload.as_deref(), Some("http://example.com"));
    // The second link runs to the end of its storage with no terminating entry.
    assert_eq!(links[1].text, "example.com");

    doc.set_link_url(links[1].object, "https://zebrapig.com/iwork")
        .unwrap();
    let doc = reopen(&doc, "link.numbers");
    let after = doc
        .smart_fields()
        .into_iter()
        .find(|f| f.object == links[1].object)
        .unwrap();
    assert_eq!(after.payload.as_deref(), Some("https://zebrapig.com/iwork"));
    assert_eq!(after.range, links[1].range, "the run did not move");
    assert_eq!(doc.problems(), Vec::<String>::new());
}

/// A link's run moves with the text it covers.
#[test]
fn editing_around_a_link_moves_it() {
    let path = fixture!("numbers-links.numbers");
    let mut doc = Document::open(&path).unwrap();
    let link = doc
        .smart_fields()
        .into_iter()
        .find(|f| f.message_type == iwork::document::TYPE_HYPERLINK_FIELD)
        .unwrap();

    doc.insert_text(link.storage, 0, "Kontakt: ").unwrap();
    let moved = doc
        .smart_fields()
        .into_iter()
        .find(|f| f.object == link.object)
        .unwrap();
    assert_eq!(moved.range.start, link.range.start + 9);
    assert_eq!(moved.range.end, link.range.end + 9);
    assert_eq!(moved.text, link.text, "and it still covers the same words");
    assert_eq!(doc.problems(), Vec::<String>::new());
}

/// Lists: the level per paragraph and the style in force, read off a document
/// Pages wrote from its Real Estate Flyer template.
#[test]
fn list_levels_and_styles_are_read_per_paragraph() {
    let path = fixture!("pages-lists.pages");
    let doc = Document::open(&path).unwrap();
    let body = doc
        .storages()
        .into_iter()
        .find(|s| s.kind == 0)
        .expect("a body storage");
    let paragraphs = doc.list_paragraphs(body.identifier).unwrap();
    assert_eq!(paragraphs.len(), 14);

    let named = |style: Option<u64>| {
        style
            .and_then(|id| doc.text_style(id))
            .and_then(|s| s.name)
            .unwrap_or_default()
    };
    let shape: Vec<(u64, String)> = paragraphs
        .iter()
        .map(|p| (p.level, named(p.style)))
        .collect();
    assert_eq!(
        shape,
        vec![
            (0, "None".into()),
            (0, "None".into()),
            (0, "Bullet 2".into()),
            (0, "Bullet 2".into()),
            (0, "Bullet 2".into()),
            (0, "Bullet 2".into()),
            (0, "None".into()),
            (0, "None".into()),
            (0, "None".into()),
            (0, "Bullet".into()),
            (0, "Bullet".into()),
            // Two paragraphs one level in — "Use the Tab key to indent."
            (1, "Bullet".into()),
            (1, "Bullet".into()),
            (0, "Bullet".into()),
        ]
    );

    // The level survives an edit inside the indented paragraph.
    let mut edited = Document::open(&path).unwrap();
    let indented = paragraphs[11].range.clone();
    edited
        .insert_text(body.identifier, indented.start + 4, "sehr ")
        .unwrap();
    let after = edited.list_paragraphs(body.identifier).unwrap();
    assert_eq!(after[11].level, 1);
    assert_eq!(after[12].level, 1);
    assert_eq!(after[13].level, 0);
    assert_eq!(edited.problems(), Vec::<String>::new());
}

/// Named style, or named style plus local overrides — the distinction a run
/// carries and the reason editing "Title" can change nothing on screen.
#[test]
fn a_run_reports_its_named_style_and_its_overrides() {
    let path = fixture!("pages-styled.pages");
    let doc = Document::open(&path).unwrap();
    let storage = doc.text_storages().into_iter().next().unwrap().identifier;

    let mut variations = 0;
    let mut named = 0;
    for range in doc.paragraph_ranges(storage).unwrap() {
        let resolved = doc
            .style_of_run(storage, range.start, StyleKind::Paragraph)
            .unwrap()
            .expect("every paragraph has a style");
        assert!(
            resolved.name.is_some(),
            "a variation must still resolve to the named style it descends from"
        );
        if resolved.is_variation {
            variations += 1;
            assert_ne!(
                Some(resolved.style),
                resolved.named,
                "a variation is not the style it inherits from"
            );
            assert!(
                !resolved.overrides.is_empty(),
                "a variation that overrides nothing would be pointless"
            );
        } else {
            named += 1;
            assert_eq!(Some(resolved.style), resolved.named);
        }
    }
    assert!(
        variations > 0 && named > 0,
        "the styled fixture has both kinds: {variations} variation(s), {named} named"
    );

    // The red paragraph's variation overrides the font colour and nothing else
    // this crate can name.
    let red = doc
        .style_of_run(storage, 12, StyleKind::Paragraph)
        .unwrap()
        .unwrap();
    assert!(
        red.overrides.iter().any(|o| o == "font-color"),
        "expected a colour override, got {:?}",
        red.overrides
    );
}

/// The anchoring of a table is what decides what an edit does to it, so it is
/// worth asserting that the corpus's tables are anchored the way this crate
/// believes: every entry of a character-anchored table sits on a `U+FFFC` or a
/// `U+0004`, which is what makes them unremappable.
#[test]
fn a_character_anchored_entry_sits_on_a_placeholder() {
    let mut checked = 0;
    for path in every_fixture() {
        let doc = Document::open(&path).unwrap();
        for storage in doc.storages() {
            let text: Vec<u16> = doc
                .storage_text(storage.identifier)
                .unwrap()
                .encode_utf16()
                .collect();
            for table in storage
                .tables
                .iter()
                .filter(|t| t.anchoring == Anchoring::Character)
            {
                let (_, object) = doc.object(storage.identifier).unwrap();
                let archive = iwork::pb::Message::decode(object.payload()).unwrap();
                let decoded =
                    iwork::pb::decode_nested(archive.bytes(table.field).unwrap()).unwrap();
                for (index, _) in iwork::text::entry_indices(&decoded, Anchoring::Character) {
                    let unit = text.get(index as usize).copied();
                    assert!(
                        matches!(unit, Some(0xFFFC) | None),
                        "{}: storage {} {} entry at {index} sits on U+{:04X}",
                        path.display(),
                        storage.identifier,
                        table.name,
                        unit.unwrap_or(0)
                    );
                    checked += 1;
                }
            }
        }
    }
    assert!(checked > 0, "nothing was checked");
}

// -- the app -----------------------------------------------------------------

fn app_check(path: &Path, expected: &str) {
    let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("scripts/app-check.sh");
    let status = std::process::Command::new(&script)
        .arg(path)
        .arg(expected)
        .status()
        .unwrap_or_else(|e| panic!("{}: {e}", script.display()));
    assert!(
        status.success(),
        "{}: the app did not accept it, or did not read back {expected:?} (exit {:?})",
        path.display(),
        status.code()
    );
}

/// The claim of the whole phase, put to the app.
///
/// Off unless `IWORK_APP_CHECK=1`. Three edits — an insertion into the middle
/// of a styled paragraph, a deletion across a paragraph boundary, and text
/// changed inside a Keynote shape — each written, opened by the app that owns
/// it, and read back. The document's drawables must still be where they were,
/// and `check` must still be clean.
#[test]
fn the_app_opens_an_edited_document_and_reads_the_new_words_back() {
    if std::env::var("IWORK_APP_CHECK").as_deref() != Ok("1") {
        eprintln!("IWORK_APP_CHECK is not 1 — skipping the app round trip");
        return;
    }

    // 1. Insert into the middle of a styled paragraph.
    let path = fixture!("pages-styled.pages");
    let mut doc = Document::open(&path).unwrap();
    let storage = doc.text_storages().into_iter().next().unwrap().identifier;
    doc.insert_text(storage, 22, "ZWISCHENGESCHOBEN ").unwrap();
    let out = std::env::temp_dir().join("iwork-insert.pages");
    doc.save(&out).unwrap();
    app_check(&out, "ZWISCHENGESCHOBEN");

    // 2. Delete across a paragraph boundary — the edit that used to leave a
    //    paragraph run where no paragraph begins.
    let mut doc = Document::open(&path).unwrap();
    doc.delete_text(storage, 5..20).unwrap();
    let out = std::env::temp_dir().join("iwork-delete.pages");
    doc.save(&out).unwrap();
    app_check(&out, "Übersr Absatz");

    // 3. A drawable-bearing document: the photo must survive an edit to the
    //    text it is anchored in, and stay where it was.
    let path = fixture!("pages-report.pages");
    let before = Document::open(&path).unwrap();
    let body = before
        .text_storages()
        .into_iter()
        .next()
        .unwrap()
        .identifier;
    let places: Vec<(u64, f32, f32)> = before
        .drawables()
        .iter()
        .map(|d| (d.identifier, d.geometry.x, d.geometry.y))
        .collect();
    let mut doc = Document::open(&path).unwrap();
    doc.replace_text(body, 20..30, "Projektantrag").unwrap();
    let out = std::env::temp_dir().join("iwork-anchor.pages");
    doc.save(&out).unwrap();
    let after = Document::open(&out).unwrap();
    assert_eq!(
        after
            .drawables()
            .iter()
            .map(|d| (d.identifier, d.geometry.x, d.geometry.y))
            .collect::<Vec<_>>(),
        places,
        "every drawable must still be there and still be where it was"
    );
    assert_eq!(after.problems(), Vec::<String>::new());
    app_check(&out, "Projektantrag");

    // 4. Text inside a Keynote shape — a storage that is not the body.
    let path = fixture!("keynote-shapes.key");
    let mut doc = Document::open(&path).unwrap();
    // Not simply the first shape with a storage. A Keynote drawable's caption
    // is a storage whose whole contents are the `U+FFFC` of an attachment, and
    // replacing that is refused, correctly; and most of the deck's shapes are
    // on *layouts* (`TemplateSlide-*`), which the app does not enumerate, so an
    // edit to one would come back as "the app did not read it" when the app was
    // never going to.
    let shape = doc
        .drawables()
        .into_iter()
        .filter(|d| d.stream.contains("/Slide-"))
        .filter_map(|d| d.text)
        .find(|storage| {
            Document::open(&path)
                .unwrap()
                .set_text(*storage, "x")
                .is_ok()
        })
        .expect("a shape on a slide whose text can be replaced");
    let length = iwork::text::length(&doc.storage_text(shape).unwrap());
    doc.replace_text(shape, 0..length, "Neuer Formtext")
        .unwrap();
    let out = std::env::temp_dir().join("iwork-shape.key");
    doc.save(&out).unwrap();
    app_check(&out, "Neuer Formtext");

    // 5. A hyperlink repointed, and text inserted in front of it. Numbers has
    //    no way to report a link's target — no app's dictionary has one, which
    //    is why the fixture had to be a template — so what the app can say is
    //    that it opens the document and reads the words the link covers. The
    //    URL itself is checked by decoding the result, above.
    let path = fixture!("numbers-links.numbers");
    let mut doc = Document::open(&path).unwrap();
    let link = doc
        .smart_fields()
        .into_iter()
        .find(|f| f.message_type == iwork::document::TYPE_HYPERLINK_FIELD)
        .unwrap();
    doc.set_link_url(link.object, "mailto:leonce@zebrapig.com")
        .unwrap();
    doc.insert_text(link.storage, 0, "Kontakt\n").unwrap();
    let out = std::env::temp_dir().join("iwork-link.numbers");
    doc.save(&out).unwrap();
    let after = Document::open(&out).unwrap();
    let moved = after
        .smart_fields()
        .into_iter()
        .find(|f| f.object == link.object)
        .unwrap();
    assert_eq!(moved.payload.as_deref(), Some("mailto:leonce@zebrapig.com"));
    assert_eq!(moved.text, link.text);
    app_check(&out, &link.text);

    // 6. And the app's own save of an edited document still decodes here — the
    //    strongest statement available without a screen: whatever Keynote made
    //    of what this crate wrote, it is something this crate reads back.
    let resaved = Document::open(&out).unwrap();
    assert_eq!(resaved.problems(), Vec::<String>::new());
}
