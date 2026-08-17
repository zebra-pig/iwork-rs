//! Text style CRUD, against a document built in memory.
//!
//! The integration tests in `fixtures.rs` need real iWork files, and none are
//! committed. These do not: they assemble a package with a stylesheet, three
//! styles and a storage whose attribute tables point at them, write it through
//! the real ZIP and IWA writers, and read it back. That covers everything about
//! style editing that does not depend on Apple's schema — which, deliberately,
//! is all of it.

use std::ops::Range;

use iwork::iwa::{ArchiveMessage, ArchiveObject};
use iwork::pb::{self, Field, Message, Value};
use iwork::{style, Document, Error, Package, StyleKind};

const ROOT: u64 = 1;
const STYLESHEET: u64 = 2;
const EMPHASIS: u64 = 10;
const BODY: u64 = 11;
const HEADING: u64 = 12;
const STORAGE: u64 = 20;
const METADATA: u64 = 30;

const TEXT: &str = "Hallo Welt\nZweiter Absatz\n";

/// Storage fields the two tables live in. Not what the field order suggests.
const PARA_TABLE: u32 = 5;
const CHAR_TABLE: u32 = 8;

fn object(identifier: u64, message_type: u32, payload: &Message) -> ArchiveObject {
    ArchiveObject {
        identifier,
        messages: vec![ArchiveMessage {
            message_type,
            version: vec![1, 0, 5],
            extra: Vec::new(),
            payload: payload.encode(),
        }],
        extra: Vec::new(),
    }
}

fn nested(number: u32, message: &Message) -> Field {
    Field {
        number,
        value: Value::Bytes(message.encode()),
    }
}

/// A style archive shaped like the ones in real documents: a base message
/// carrying the name and the internal identifier, and a property bag beside it.
fn style_archive(name: &str, identifier: &str) -> Message {
    let mut base = Message::default();
    base.set(1, Value::Bytes(name.as_bytes().to_vec()));
    base.set(2, Value::Bytes(identifier.as_bytes().to_vec()));
    // Field 1.5 — every real style names the stylesheet it belongs to, and that
    // is how a copy knows which list to join.
    base.fields.push(nested(5, &style::reference(STYLESHEET)));

    let mut properties = Message::default();
    properties.set(12, Value::Fixed32(12.0f32.to_le_bytes()));

    let mut archive = Message::default();
    archive.fields.push(nested(1, &base));
    archive.fields.push(nested(11, &properties));
    archive
}

fn attribute_table(runs: &[(u64, u64)]) -> Message {
    let mut table = Message::default();
    for (start, target) in runs {
        let mut entry = Message::default();
        entry.set(1, Value::Varint(*start));
        entry.fields.push(nested(2, &style::reference(*target)));
        table.fields.push(nested(1, &entry));
    }
    table
}

/// A Pages-shaped document: root type 10000 in `Index/Document.iwa`.
fn document() -> Document {
    build(false)
}

/// The same, plus a five-slot positional array of bare style references on the
/// root — the shape a Keynote slide uses for its outline levels.
fn document_with_outline_array() -> Document {
    build(true)
}

fn build(outline_array: bool) -> Document {
    let mut root = Message::default();
    root.fields.push(nested(2, &style::reference(STYLESHEET)));
    if outline_array {
        for _ in 0..5 {
            root.fields.push(nested(31, &style::reference(BODY)));
        }
    }

    // Three styles listed by plain reference, plus one keyed entry mapping the
    // well-known identifier "body" to a style.
    let mut keyed = Message::default();
    keyed.set(1, Value::Bytes(b"body".to_vec()));
    keyed.fields.push(nested(2, &style::reference(BODY)));

    let mut stylesheet = Message::default();
    stylesheet
        .fields
        .push(nested(2, &style::reference(EMPHASIS)));
    stylesheet.fields.push(nested(2, &style::reference(BODY)));
    stylesheet
        .fields
        .push(nested(2, &style::reference(HEADING)));
    stylesheet.fields.push(nested(3, &keyed));

    let mut storage = Message::default();
    storage
        .fields
        .push(nested(2, &style::reference(STYLESHEET)));
    storage.set(3, Value::Bytes(TEXT.as_bytes().to_vec()));
    // Character styles live in table 8 and paragraph styles in table 5 — the
    // opposite of what the field order suggests, and the opposite of what this
    // crate believed until the probe document settled it.
    storage
        .fields
        .push(nested(8, &attribute_table(&[(0, BODY), (6, EMPHASIS)])));
    storage
        .fields
        .push(nested(5, &attribute_table(&[(0, HEADING), (11, HEADING)])));

    let mut metadata = Message::default();
    metadata.set(1, Value::Varint(METADATA));

    let objects = vec![
        object(ROOT, 10000, &root),
        object(STYLESHEET, 5020, &stylesheet),
        object(
            EMPHASIS,
            style::TYPE_CHARACTER_STYLE,
            &style_archive("Emphasis", "emphasis"),
        ),
        object(
            BODY,
            style::TYPE_CHARACTER_STYLE,
            &style_archive("Body", "body"),
        ),
        object(
            HEADING,
            style::TYPE_PARAGRAPH_STYLE,
            &style_archive("Heading", "heading-1"),
        ),
        object(STORAGE, iwork::TYPE_STORAGE, &storage),
        object(METADATA, iwork::TYPE_PACKAGE_METADATA, &metadata),
    ];

    let package = Package {
        entries: vec![
            (
                "Metadata/DocumentIdentifier".to_string(),
                b"6E1E5A2C-0000-0000-0000-000000000000".to_vec(),
            ),
            (
                "Index/Document.iwa".to_string(),
                iwork::iwa::serialize(&objects),
            ),
        ],
    };
    Document::from_package(package).expect("synthetic package parses")
}

/// Write the document out and read it back, so every assertion is about a file
/// that survived the ZIP and IWA writers rather than the in-memory graph.
fn reopen(doc: &Document, tag: &str) -> Document {
    let out = std::env::temp_dir().join(format!("iwork-styles-{tag}.pages"));
    doc.save(&out).unwrap();
    let reopened = Document::open(&out).unwrap();
    let _ = std::fs::remove_file(&out);
    reopened
}

fn runs_of(doc: &Document, table: u32) -> Vec<(u64, Option<u64>)> {
    let (_, object) = doc.object(STORAGE).unwrap();
    let storage = Message::decode(object.payload()).unwrap();
    let table = pb::decode_nested(storage.bytes(table).unwrap()).unwrap();
    style::runs(&table)
        .into_iter()
        .map(|run| (run.start, run.style))
        .collect()
}

fn names(doc: &Document) -> Vec<(u64, String)> {
    doc.text_styles()
        .into_iter()
        .map(|s| (s.identifier, s.name.clone().unwrap_or_default()))
        .collect()
}

// -- read --------------------------------------------------------------------

#[test]
fn reads_every_style_with_its_kind_and_name() {
    let doc = document();
    assert_eq!(
        names(&doc),
        vec![
            (EMPHASIS, "Emphasis".to_string()),
            (BODY, "Body".to_string()),
            (HEADING, "Heading".to_string()),
        ]
    );

    let heading = doc.text_style(HEADING).unwrap();
    assert_eq!(heading.kind, StyleKind::Paragraph);
    assert_eq!(heading.kind.attribute_table(), 5);
    assert_eq!(heading.stream, "Index/Document.iwa");
    // The name is found at a path, and the internal identifier after it.
    assert_eq!(heading.name.as_deref(), Some("Heading"));
    assert_eq!(heading.style_identifier.as_deref(), Some("heading-1"));
    assert_eq!(
        heading.labels[1].text, "heading-1",
        "labels still list every string"
    );
    assert_eq!(doc.text_style(9999).map(|s| s.identifier), None);
}

#[test]
fn reads_where_a_style_is_used() {
    let doc = document();
    let used = doc.text_style_usage(EMPHASIS);
    assert_eq!(used.len(), 1);
    assert_eq!(used[0].storage, STORAGE);
    assert_eq!(used[0].table, 8);
    // "Hallo " is styled Body, the rest of the storage Emphasis.
    assert_eq!(used[0].range, 6..26);

    // Two entries in the paragraph table point at the same style.
    let heading = doc.text_style_usage(HEADING);
    assert_eq!(heading.len(), 2);
    assert_eq!(
        heading.iter().map(|u| u.range.clone()).collect::<Vec<_>>(),
        vec![0..11, 11..26]
    );

    assert!(doc.text_style_usage(9999).is_empty());
}

#[test]
fn reads_paragraphs_to_apply_paragraph_styles_to() {
    let doc = document();
    assert_eq!(doc.paragraph_ranges(STORAGE).unwrap(), vec![0..11, 11..26]);
    assert_eq!(doc.storage_text(STORAGE).unwrap(), TEXT);
    assert!(matches!(
        doc.paragraph_ranges(EMPHASIS),
        Err(Error::Format(_))
    ));
}

// -- create ------------------------------------------------------------------

#[test]
fn creating_copies_a_style_and_allocates_an_identifier_above_the_high_water_mark() {
    let mut doc = document();
    let created = doc.create_text_style(BODY, "Kicker").unwrap();

    assert_eq!(created.identifier, METADATA + 1);
    assert_eq!(created.template, BODY);
    assert_eq!(created.stream, "Index/Document.iwa");
    // The plain reference in the stylesheet is cloned; the keyed "body" entry
    // is not, because a second style claiming to be the body style is worse
    // than one that is merely listed.
    assert_eq!(created.registrations_cloned, 1);

    let doc = reopen(&doc, "create");
    let new = doc.text_style(created.identifier).unwrap();
    assert_eq!(new.name.as_deref(), Some("Kicker"));
    assert_eq!(new.kind, StyleKind::Character);
    // Everything but the name comes from the template.
    let template = doc.text_style(BODY).unwrap();
    assert_eq!(
        style::get_path(&new.archive, &[11, 12]),
        style::get_path(&template.archive, &[11, 12])
    );
    assert_eq!(
        new.style_identifier.as_deref(),
        Some("body"),
        "the rest of the copy is intact"
    );

    // The high-water mark moved, so iWork will not reissue the identifier.
    assert_eq!(doc.last_object_identifier(), Some(created.identifier));
    assert_eq!(doc.next_object_identifier(), created.identifier + 1);

    let (_, stylesheet) = doc.object(STYLESHEET).unwrap();
    let sheet = Message::decode(stylesheet.payload()).unwrap();
    assert_eq!(style::count_references(&sheet, created.identifier), 1);
    assert_eq!(
        style::count_references(&sheet, BODY),
        2,
        "the template keeps both its plain and its keyed entry"
    );

    // A new style is listed, not yet applied to anything.
    assert!(doc.text_style_usage(created.identifier).is_empty());
}

/// Bare references outside the stylesheet must be left alone.
///
/// A Keynote `KN.SlideArchive` holds five bare style references in field 31,
/// one per outline level. They look exactly like a stylesheet's style list and
/// are nothing of the kind: adding a sixth does not list a style, it corrupts
/// the mapping from level to style. A real deck grew 20 spurious entries this
/// way before `create_text_style` was made to use the stylesheet the style
/// itself names.
#[test]
fn creating_leaves_positional_reference_arrays_alone() {
    let mut doc = document_with_outline_array();
    let created = doc.create_text_style(BODY, "Kicker").unwrap();
    assert_eq!(
        created.registrations_cloned, 1,
        "only the stylesheet's own list may gain an entry"
    );

    let doc = reopen(&doc, "outline-array");
    let root = Message::decode(doc.object(ROOT).unwrap().1.payload()).unwrap();
    assert_eq!(
        root.fields.iter().filter(|f| f.number == 31).count(),
        5,
        "the positional array must keep its length"
    );
    assert_eq!(style::count_references(&root, created.identifier), 0);
    assert_eq!(style::count_references(&root, BODY), 5);
}

/// The same array must not be picked apart by a delete either: the style is
/// genuinely in use there, so the delete is refused rather than shifting it.
#[test]
fn deleting_refuses_while_a_positional_array_still_points_at_the_style() {
    let mut doc = document_with_outline_array();
    match doc.delete_text_style(BODY, Some(EMPHASIS)) {
        Err(Error::StyleInUse { references, .. }) => assert!(references.contains(&ROOT)),
        other => panic!("expected StyleInUse, got {other:?}"),
    }
    let root = Message::decode(doc.object(ROOT).unwrap().1.payload()).unwrap();
    assert_eq!(root.fields.iter().filter(|f| f.number == 31).count(), 5);
}

#[test]
fn creating_twice_does_not_reuse_an_identifier() {
    let mut doc = document();
    let first = doc.create_text_style(BODY, "One").unwrap().identifier;
    let second = doc.create_text_style(BODY, "Two").unwrap().identifier;
    assert_ne!(first, second);

    let doc = reopen(&doc, "create-twice");
    assert_eq!(doc.text_style(first).unwrap().name.as_deref(), Some("One"));
    assert_eq!(doc.text_style(second).unwrap().name.as_deref(), Some("Two"));
    assert_eq!(doc.text_styles().len(), 5);
}

/// Copying a variation style must not name the copy.
///
/// A named style has a name and an internal identifier; a variation has neither,
/// plus a parent and a flag saying it is one. A copy that is flagged a variation,
/// carries a name, has no identifier and sits in the stylesheet's named list is
/// none of those things, and Pages crashes on opening the document. Two of five
/// real documents did exactly that.
#[test]
fn copying_a_variation_style_leaves_the_copy_anonymous() {
    let mut doc = document();
    let variation = doc.create_text_style(BODY, "Base").unwrap().identifier;
    // Make it a variation: drop the name and identifier, add a parent and flag.
    doc.set_text_style_property(variation, style::NAME, None)
        .unwrap();
    doc.set_text_style_property(variation, style::STYLE_IDENTIFIER, None)
        .unwrap();
    doc.set_text_style_property(variation, style::PARENT, Some(Value::Varint(BODY)))
        .unwrap();
    doc.set_text_style_property(variation, &[1, 4], Some(Value::Varint(1)))
        .unwrap();
    assert!(doc.text_style(variation).unwrap().name.is_none());

    let created = doc.create_text_style(variation, "Wunschname").unwrap();
    assert_eq!(created.name, None, "the name must not be applied");

    let doc = reopen(&doc, "variation-copy");
    let copy = doc.text_style(created.identifier).unwrap();
    assert!(copy.name.is_none(), "the copy stays anonymous");
    assert_eq!(copy.parent, Some(BODY), "and stays a variation");
    assert_eq!(
        copy.archive,
        doc.text_style(variation).unwrap().archive,
        "a copy of a variation differs from its template in nothing at all"
    );

    // A named template still gets the name, as before.
    let mut doc = document();
    let named = doc.create_text_style(BODY, "Kicker").unwrap();
    assert_eq!(named.name.as_deref(), Some("Kicker"));
}

/// New fields go where iWork would have written them, in ascending order.
#[test]
fn new_fields_are_inserted_in_field_order() {
    let mut doc = document();
    doc.set_text_style_property(BODY, style::property::ITALIC, Some(Value::Varint(1)))
        .unwrap();
    doc.set_text_style_property(BODY, style::property::BOLD, Some(Value::Varint(1)))
        .unwrap();

    let doc = reopen(&doc, "field-order");
    let bag = pb::decode_nested(doc.text_style(BODY).unwrap().archive.bytes(11).unwrap()).unwrap();
    let numbers: Vec<u32> = bag.fields.iter().map(|f| f.number).collect();
    assert_eq!(
        numbers,
        vec![1, 2, 12],
        "1 and 2 inserted before the existing 12"
    );
    let mut sorted = numbers.clone();
    sorted.sort_unstable();
    assert_eq!(numbers, sorted);
}

#[test]
fn creating_from_something_that_is_not_a_style_fails() {
    let mut doc = document();
    assert!(matches!(
        doc.create_text_style(STORAGE, "Nope"),
        Err(Error::NoSuchStyle(STORAGE))
    ));
    assert_eq!(doc.text_styles().len(), 3, "nothing was added");
}

// -- update ------------------------------------------------------------------

#[test]
fn renaming_writes_to_the_field_the_name_came_from() {
    let mut doc = document();
    doc.rename_text_style(HEADING, "Grosse Uberschrift")
        .unwrap();

    let doc = reopen(&doc, "rename");
    let heading = doc.text_style(HEADING).unwrap();
    assert_eq!(heading.name.as_deref(), Some("Grosse Uberschrift"));
    assert_eq!(
        heading.style_identifier.as_deref(),
        Some("heading-1"),
        "the internal identifier is left alone"
    );
}

#[test]
fn setting_and_clearing_a_property_by_path() {
    let mut doc = document();
    doc.set_text_style_property(BODY, &[11, 12], Some(Value::Fixed32(18.0f32.to_le_bytes())))
        .unwrap();
    doc.set_text_style_property(BODY, &[11, 1], Some(Value::Varint(1)))
        .unwrap();
    doc.set_text_style_property(EMPHASIS, &[11, 12], None)
        .unwrap();

    let doc = reopen(&doc, "properties");
    let body = doc.text_style(BODY).unwrap();
    assert_eq!(
        style::get_path(&body.archive, &[11, 12]),
        Some(Value::Fixed32(18.0f32.to_le_bytes()))
    );
    assert_eq!(
        style::get_path(&body.archive, &[11, 1]),
        Some(Value::Varint(1))
    );
    assert_eq!(body.name.as_deref(), Some("Body"), "the name survived");

    let emphasis = doc.text_style(EMPHASIS).unwrap();
    assert_eq!(style::get_path(&emphasis.archive, &[11, 12]), None);
}

#[test]
fn updating_hands_over_the_raw_archive() {
    let mut doc = document();
    doc.update_text_style(BODY, |archive| {
        archive.set(99, Value::Varint(7));
    })
    .unwrap();

    let doc = reopen(&doc, "update");
    assert_eq!(doc.text_style(BODY).unwrap().archive.varint(99), Some(7));
}

// -- apply -------------------------------------------------------------------

#[test]
fn applying_a_character_style_splits_the_run_and_restores_the_tail() {
    let mut doc = document();
    let created = doc.create_text_style(BODY, "Kicker").unwrap().identifier;
    doc.apply_text_style(STORAGE, 2..4, created).unwrap();

    let doc = reopen(&doc, "apply");
    assert_eq!(
        runs_of(&doc, CHAR_TABLE),
        vec![
            (0, Some(BODY)),
            (2, Some(created)),
            (4, Some(BODY)),
            (6, Some(EMPHASIS)),
        ]
    );
    assert_eq!(doc.text_style_usage(created)[0].range, 2..4);
    assert_eq!(
        doc.storage_text(STORAGE).unwrap(),
        TEXT,
        "styling text does not change it"
    );
}

#[test]
fn applying_a_paragraph_style_uses_the_paragraph_table() {
    let mut doc = document();
    let created = doc.create_text_style(HEADING, "Untertitel").unwrap();
    let second = doc.paragraph_ranges(STORAGE).unwrap()[1].clone();
    doc.apply_text_style(STORAGE, second, created.identifier)
        .unwrap();

    let doc = reopen(&doc, "apply-paragraph");
    assert_eq!(
        runs_of(&doc, PARA_TABLE),
        vec![(0, Some(HEADING)), (11, Some(created.identifier))]
    );
    assert_eq!(
        runs_of(&doc, CHAR_TABLE),
        vec![(0, Some(BODY)), (6, Some(EMPHASIS))],
        "the character table is untouched"
    );
}

#[test]
fn applying_past_the_end_of_the_text_is_clamped() {
    let mut doc = document();
    doc.apply_text_style(STORAGE, 20..9_000, EMPHASIS).unwrap();
    assert_eq!(
        runs_of(&doc, CHAR_TABLE),
        vec![(0, Some(BODY)), (6, Some(EMPHASIS))]
    );
}

#[test]
fn applying_to_something_that_is_not_a_storage_or_a_style_fails() {
    let mut doc = document();
    assert!(matches!(
        doc.apply_text_style(STORAGE, 0..1, STYLESHEET),
        Err(Error::NoSuchStyle(STYLESHEET))
    ));
    assert!(matches!(
        doc.apply_text_style(STYLESHEET, 0..1, BODY),
        Err(Error::Format(_))
    ));
}

// -- delete ------------------------------------------------------------------

#[test]
fn deleting_repoints_runs_at_a_replacement() {
    let mut doc = document();
    let deleted = doc.delete_text_style(EMPHASIS, Some(BODY)).unwrap();

    assert_eq!(deleted.runs_repointed, 1);
    assert_eq!(deleted.runs_dropped, 0);
    assert_eq!(deleted.registrations_removed, 1);

    let doc = reopen(&doc, "delete-replace");
    assert!(doc.text_style(EMPHASIS).is_none());
    assert_eq!(doc.text_styles().len(), 2);
    // Both runs now say Body, so they are one run.
    assert_eq!(runs_of(&doc, CHAR_TABLE), vec![(0, Some(BODY))]);

    // Nothing anywhere still points at the style that is gone.
    for (_, object) in doc.objects() {
        let archive = Message::decode(object.payload()).unwrap();
        assert_eq!(
            style::count_references(&archive, EMPHASIS),
            0,
            "object {} still refers to the deleted style",
            object.identifier
        );
    }
}

#[test]
fn deleting_without_a_replacement_drops_the_runs() {
    let mut doc = document();
    let deleted = doc.delete_text_style(EMPHASIS, None).unwrap();
    assert_eq!(deleted.runs_dropped, 1);
    assert_eq!(deleted.runs_repointed, 0);

    let doc = reopen(&doc, "delete-drop");
    // The Body run now extends over the text the deleted style covered.
    assert_eq!(runs_of(&doc, CHAR_TABLE), vec![(0, Some(BODY))]);
}

/// Deleting must not leave a reference behind anywhere — iWork is unforgiving
/// about those — so a style something unmodelled still points at stays put.
#[test]
fn deleting_a_style_something_else_refers_to_is_refused() {
    let mut doc = document();
    // The keyed "body" entry in the stylesheet is deliberately not rewritten.
    let refused = doc.delete_text_style(BODY, None);
    match refused {
        Err(Error::StyleInUse {
            identifier,
            references,
        }) => {
            assert_eq!(identifier, BODY);
            assert_eq!(references, vec![STYLESHEET]);
        }
        other => panic!("expected StyleInUse, got {other:?}"),
    }

    // And the document is exactly as it was.
    let doc = reopen(&doc, "delete-refused");
    assert_eq!(names(&doc).len(), 3);
    assert_eq!(
        runs_of(&doc, CHAR_TABLE),
        vec![(0, Some(BODY)), (6, Some(EMPHASIS))]
    );
    let (_, stylesheet) = doc.object(STYLESHEET).unwrap();
    assert_eq!(
        style::count_references(&Message::decode(stylesheet.payload()).unwrap(), BODY),
        2
    );
}

#[test]
fn deleting_rejects_a_replacement_of_the_wrong_kind() {
    let mut doc = document();
    assert!(matches!(
        doc.delete_text_style(EMPHASIS, Some(HEADING)),
        Err(Error::Format(_))
    ));
    assert!(matches!(
        doc.delete_text_style(EMPHASIS, Some(9999)),
        Err(Error::NoSuchStyle(9999))
    ));
    assert_eq!(doc.text_styles().len(), 3);
}

/// A style created, applied, renamed and deleted again should leave the
/// document's other objects exactly where they started.
#[test]
fn a_full_crud_cycle_returns_the_document_to_its_original_shape() {
    let original = document();
    let before: Vec<(u64, u32)> = original
        .objects()
        .map(|(_, o)| (o.identifier, o.message_type()))
        .collect();
    let before_runs: Vec<(u64, Option<u64>)> = runs_of(&original, CHAR_TABLE);

    let mut doc = document();
    let created = doc.create_text_style(BODY, "Kicker").unwrap().identifier;
    doc.apply_text_style(STORAGE, 2..4, created).unwrap();
    doc.rename_text_style(created, "Kicker 2").unwrap();
    doc.set_text_style_property(created, &[11, 12], Some(Value::Varint(9)))
        .unwrap();
    doc.delete_text_style(created, Some(BODY)).unwrap();

    let doc = reopen(&doc, "cycle");
    let after: Vec<(u64, u32)> = doc
        .objects()
        .map(|(_, o)| (o.identifier, o.message_type()))
        .collect();
    assert_eq!(after, before);
    assert_eq!(runs_of(&doc, CHAR_TABLE), before_runs);
    // Only the high-water mark is left changed, and deliberately so: it records
    // that an identifier was handed out, even though nothing holds it now.
    assert_eq!(doc.last_object_identifier(), Some(created));
}

/// Ranges are UTF-16 code units, the unit iWork counts run indices in, so a
/// storage full of astral characters must index the same way.
#[test]
fn ranges_are_counted_in_utf16_code_units() {
    let mut doc = document();
    doc.set_text(STORAGE, "\u{1F600}\u{1F600}ab").unwrap();
    assert_eq!(doc.paragraph_ranges(STORAGE).unwrap(), vec![0..6]);
    doc.apply_text_style(STORAGE, 4..6, EMPHASIS).unwrap();
    let used = doc.text_style_usage(EMPHASIS);
    assert_eq!(
        used.iter()
            .map(|u| u.range.clone())
            .collect::<Vec<Range<u64>>>(),
        vec![4..6]
    );
}
