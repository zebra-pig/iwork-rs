//! Documents made from nothing.
//!
//! Every other test in this repository starts from a document an app wrote.
//! These start from nothing at all: `Document::new` assembles the object graph,
//! the component index and the identity in memory, and what comes out has to be
//! a document by every measure the crate already has — it decodes, it round
//! trips, `check` finds no broken references, and the text goes in and comes
//! back out.
//!
//! The last measure is the app itself, behind `IWORK_APP_CHECK=1`, and it is
//! the one that matters: `Document::new(Kind::Pages)` was refused by Pages
//! until the component index declared a component's root the way iWork declares
//! one, and nothing on this side of the file could have told us.

use std::path::Path;

use iwork::{Document, Kind};

fn scratch(name: &str) -> std::path::PathBuf {
    let path = std::env::temp_dir().join(name);
    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_dir_all(&path);
    path
}

/// The shape of a new Pages document, asserted where it is fixed by the format
/// rather than by taste: object 1 is the document, 71 is its metadata, and the
/// component index is object 2 and says how high the identifiers have gone.
#[test]
fn a_new_pages_document_has_the_objects_the_format_fixes() {
    let doc = Document::new(Kind::Pages).unwrap();
    assert_eq!(doc.kind(), Kind::Pages);

    let (_, root) = doc.object(1).expect("no object 1");
    assert_eq!(root.message_type(), 10000, "object 1 is TP.DocumentArchive");
    let (stream, metadata) = doc.object(71).expect("no object 71");
    assert_eq!(metadata.message_type(), 11011, "object 71 is the metadata");
    assert_eq!(stream, "Index/DocumentMetadata.iwa");
    let (stream, index) = doc.object(2).expect("no object 2");
    assert_eq!(index.message_type(), iwork::TYPE_PACKAGE_METADATA);
    assert_eq!(stream, "Index/Metadata.iwa");

    // The high-water mark covers every identifier in the package. Understating
    // it is how a document hands out an identifier something already has.
    let highest = doc.objects().map(|(_, o)| o.identifier).max().unwrap();
    assert!(
        doc.last_object_identifier().unwrap() >= highest,
        "the component index understates the highest identifier"
    );
    assert!(doc.problems().is_empty(), "{:?}", doc.problems());
}

/// Every reference that leaves the component it is written in is declared.
///
/// The rule that took two hours: a *root* is declared as `{component}` and
/// anything else as `{component, object}`, and Pages refuses a package that
/// gets it the wrong way round.
#[test]
fn a_new_document_declares_every_reference_that_leaves_a_component() {
    let doc = Document::new(Kind::Pages).unwrap();
    assert!(
        doc.undeclared_references().is_empty(),
        "{:?}",
        doc.undeclared_references()
    );
}

/// It survives being written and read, and reading it back changes nothing.
#[test]
fn a_new_pages_document_round_trips_through_a_file() {
    let path = scratch("iwork-create-roundtrip.pages");
    let made = Document::new(Kind::Pages).unwrap();
    made.save(&path).unwrap();

    let reopened = Document::open(&path).unwrap();
    assert_eq!(reopened.kind(), Kind::Pages);
    assert_eq!(reopened.objects().count(), made.objects().count());
    assert!(reopened.problems().is_empty(), "{:?}", reopened.problems());

    // A save of what was just opened reproduces the file byte for byte, which
    // is the invariant the whole crate is built on.
    let again = scratch("iwork-create-roundtrip-2.pages");
    reopened.save(&again).unwrap();
    assert_eq!(
        std::fs::read(&path).unwrap(),
        std::fs::read(&again).unwrap(),
        "a new document does not survive a no-op save"
    );
    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_file(&again);
}

/// Two documents are two documents: a new identity every time, or two files
/// made this way would collide in iCloud.
#[test]
fn every_new_document_has_an_identity_of_its_own() {
    let one = Document::new(Kind::Pages).unwrap();
    let two = Document::new(Kind::Pages).unwrap();
    let uuid = |doc: &Document| {
        doc.metadata()
            .unwrap()
            .properties
            .unwrap()
            .document_uuid
            .unwrap()
    };
    assert_ne!(uuid(&one), uuid(&two));

    // …and the three places that say which document this is agree, which is
    // what `problems` checks and what a bad identity looks like.
    let properties = one.metadata().unwrap().properties.unwrap();
    assert_eq!(properties.document_uuid, properties.share_uuid);
    assert_eq!(properties.document_uuid, properties.stable_document_uuid);
    assert!(properties
        .revision
        .unwrap()
        .ends_with(&properties.version_uuid.unwrap()));
}

/// The body is empty, and paragraphs go into it in order.
#[test]
fn paragraphs_go_into_the_body_of_a_new_document() {
    let mut doc = Document::new(Kind::Pages).unwrap();
    let body = doc.body_storage().expect("a new document has a body");
    assert_eq!(doc.storage_text(body).unwrap(), "");

    doc.append_paragraph("First").unwrap();
    doc.append_paragraph("Second").unwrap();
    // The first paragraph brings no newline with it; the second does.
    assert_eq!(doc.storage_text(body).unwrap(), "First\nSecond");
    assert_eq!(doc.paragraph_ranges(body).unwrap().len(), 2);
    assert!(doc.problems().is_empty(), "{:?}", doc.problems());
}

/// Paper is a choice, and the only thing it changes.
#[test]
fn paper_sets_the_page_and_nothing_else() {
    let a4 = Document::new_on(Kind::Pages, iwork::create::Paper::A4).unwrap();
    let letter = Document::new_on(Kind::Pages, iwork::create::Paper::Letter).unwrap();
    let setup = |doc: &Document| doc.structure().expect("a Pages document has one").setup;
    assert_eq!(setup(&a4).width.round(), 595.0);
    assert_eq!(setup(&a4).height.round(), 842.0);
    assert_eq!(setup(&a4).paper_id, "iso-a4");
    assert_eq!(setup(&letter).width.round(), 612.0);
    assert_eq!(setup(&letter).height.round(), 792.0);
    assert_eq!(setup(&letter).paper_id, "na-letter");
    assert_eq!(a4.objects().count(), letter.objects().count());
}

/// The other two apps say so by name rather than writing a broken document.
#[test]
fn the_apps_that_cannot_be_created_yet_are_refused_by_name() {
    for kind in [Kind::Numbers, Kind::Keynote, Kind::Unknown] {
        let refused = Document::new(kind);
        assert!(refused.is_err(), "{kind:?} should not be creatable yet");
    }
}

/// The measure that counts. Off unless `IWORK_APP_CHECK=1`.
#[test]
fn pages_opens_a_document_this_crate_made_from_nothing() {
    if std::env::var("IWORK_APP_CHECK").as_deref() != Ok("1") {
        eprintln!("IWORK_APP_CHECK is not 1 — skipping the app round trip");
        return;
    }
    let mut doc = Document::new(Kind::Pages).unwrap();
    doc.append_paragraph("Aus dem Nichts, mit Umlaut: Größe")
        .unwrap();
    let out = scratch("iwork-created.pages");
    doc.save(&out).unwrap();

    let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("scripts/app-check.sh");
    let output = std::process::Command::new(&script)
        .arg(&out)
        .arg("Aus dem Nichts, mit Umlaut: Größe")
        .output()
        .unwrap_or_else(|e| panic!("{}: {e}", script.display()));
    assert!(
        output.status.success(),
        "Pages would not open a document made from nothing:\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let _ = std::fs::remove_file(&out);
}
