//! Putting a drawable on a page, a sheet or a slide — the three apps' idea of
//! "here", and the one archive that serves all of them.
//!
//! A shape is a `TSWP.ShapeInfoArchive` wherever it lands; what differs is who
//! holds it. A Keynote slide *owns* its drawables and is named as their parent;
//! a Numbers sheet does the same; a Pages page owns nothing — the page group in
//! `TP.FloatingDrawablesArchive` names the shape, and the shape has no parent
//! at all. Writing a parent into a Pages shape is the mistake this file exists
//! to keep from coming back.
//!
//! Everything a new shape points at is borrowed from the document it lands in,
//! never invented: a style off a text shape already there (or the theme's
//! text-box preset), the stylesheet, a paragraph style, a list style. A
//! document with none of those is refused by name.

use std::path::{Path, PathBuf};

use iwork::drawable::Outline;
use iwork::{Document, Kind};

fn scratch(name: &str) -> PathBuf {
    let path = std::env::temp_dir().join(name);
    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_dir_all(&path);
    path
}

fn generated(name: &str) -> Option<PathBuf> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/generated")
        .join(name);
    path.exists().then_some(path)
}

macro_rules! fixture {
    ($name:expr) => {
        match generated($name) {
            Some(path) => Document::open(&path).unwrap(),
            None => {
                eprintln!("no {} — skipping (run scripts/make-fixtures.sh)", $name);
                return;
            }
        }
    };
}

/// A text box on a page of a Pages document made from nothing: the drawable is
/// there, it holds the words, and the document still hangs together.
#[test]
fn a_pages_document_from_nothing_can_hold_a_text_box() {
    let mut doc = Document::new(Kind::Pages).unwrap();
    let shape = doc
        .add_text_box(
            "page 1",
            "Ein Kasten aus dem Nichts",
            (72.0, 300.0),
            (400.0, 100.0),
        )
        .unwrap();

    let drawable = doc
        .drawables()
        .into_iter()
        .find(|d| d.identifier == shape)
        .expect("the box is a drawable");
    let frame = drawable.frame(None);
    assert_eq!((frame.x, frame.y), (72.0, 300.0));
    assert_eq!((frame.width, frame.height), (400.0, 100.0));
    let storage = drawable.text.expect("a text box has a storage");
    assert_eq!(
        doc.storage_text(storage).unwrap(),
        "Ein Kasten aus dem Nichts"
    );

    assert!(doc.problems().is_empty(), "{:?}", doc.problems());
    assert!(doc.undeclared_references().is_empty());
}

/// Pages owns nothing on a page: the shape names no parent, and the page group
/// names the shape.
#[test]
fn a_shape_on_a_page_has_no_parent_and_the_page_group_names_it() {
    let mut doc = Document::new(Kind::Pages).unwrap();
    let shape = doc
        .add_text_box("page 1", "hier", (10.0, 10.0), (100.0, 40.0))
        .unwrap();

    let drawable = doc
        .drawables()
        .into_iter()
        .find(|d| d.identifier == shape)
        .unwrap();
    assert_eq!(drawable.parent, None, "a Pages page owns nothing");
    assert_eq!(
        drawable.placement,
        iwork::drawable::Placement::Floating { page: 0 },
        "the first page's group holds it"
    );
}

/// A second box on the same page joins the group that is already there rather
/// than starting a second one for the same page.
#[test]
fn two_boxes_on_one_page_share_a_group() {
    let mut doc = Document::new(Kind::Pages).unwrap();
    doc.add_text_box("page 1", "eins", (10.0, 10.0), (100.0, 40.0))
        .unwrap();
    doc.add_text_box("page 1", "zwei", (10.0, 80.0), (100.0, 40.0))
        .unwrap();

    let on_page_one = doc
        .drawables()
        .into_iter()
        .filter(|d| d.placement == iwork::drawable::Placement::Floating { page: 0 })
        .count();
    assert_eq!(on_page_one, 2);

    let floating = doc
        .objects()
        .find(|(_, o)| o.message_type() == iwork::pages::TYPE_FLOATING_DRAWABLES)
        .map(|(_, o)| o.identifier)
        .expect("a Pages document has one");
    let archive = doc.archive(floating).unwrap();
    let groups = archive.fields.iter().filter(|f| f.number == 1).count();
    assert_eq!(groups, 1, "one page, one group");
}

/// The three outlines, and how many path elements each is drawn with.
#[test]
fn each_outline_is_drawn_with_its_own_path() {
    let cases = [
        (Outline::Rectangle, 6, (200.0f32, 100.0f32)),
        (Outline::Ellipse, 7, (200.0, 100.0)),
        (Outline::Line, 2, (200.0, 0.0)),
    ];
    for (outline, elements, size) in cases {
        let mut doc = Document::new(Kind::Pages).unwrap();
        let shape = doc
            .add_shape("page 1", outline, "", (0.0, 0.0), size)
            .unwrap();
        let drawable = doc
            .drawables()
            .into_iter()
            .find(|d| d.identifier == shape)
            .unwrap();
        let source = drawable
            .path_source
            .as_ref()
            .expect("a shape is drawn along one");
        assert_eq!(
            source.elements, elements,
            "{outline:?} is drawn with {elements} element(s)"
        );
        assert!(
            doc.problems().is_empty(),
            "{outline:?}: {:?}",
            doc.problems()
        );
    }
}

/// A size that is not one is refused by name, and a line may be flat because
/// that is what a rule is.
#[test]
fn a_shape_with_no_size_is_refused_and_a_flat_line_is_not() {
    let mut doc = Document::new(Kind::Pages).unwrap();
    let refusal = doc
        .add_text_box("page 1", "", (0.0, 0.0), (100.0, 0.0))
        .unwrap_err()
        .to_string();
    assert!(refusal.contains("needs a size"), "{refusal}");

    let refusal = doc
        .add_shape("page 1", Outline::Line, "", (0.0, 0.0), (0.0, 0.0))
        .unwrap_err()
        .to_string();
    assert!(refusal.contains("needs a size"), "{refusal}");

    doc.add_shape("page 1", Outline::Line, "", (0.0, 0.0), (300.0, 0.0))
        .expect("a horizontal rule is a line with no height");
}

/// A container nobody can find is refused by name, and pages are counted from
/// one.
#[test]
fn a_container_that_is_not_there_is_refused_by_name() {
    let mut doc = Document::new(Kind::Pages).unwrap();
    let refusal = doc
        .add_text_box("Blatt 9", "x", (0.0, 0.0), (10.0, 10.0))
        .unwrap_err()
        .to_string();
    assert!(refusal.contains("no slide, sheet or page"), "{refusal}");

    let refusal = doc
        .add_text_box("page 0", "x", (0.0, 0.0), (10.0, 10.0))
        .unwrap_err()
        .to_string();
    assert!(refusal.contains("counted from one"), "{refusal}");
}

/// A Keynote slide owns its drawable and is named as the parent — the other
/// half of the rule Pages breaks.
#[test]
fn a_slide_owns_the_box_it_is_given() {
    let mut doc = Document::new(Kind::Keynote).unwrap();
    let slide = doc.slides()[0].identifier;
    let shape = doc
        .add_text_box(
            &slide.to_string(),
            "Kasten auf der Folie",
            (100.0, 100.0),
            (600.0, 120.0),
        )
        .unwrap();

    let drawable = doc
        .drawables()
        .into_iter()
        .find(|d| d.identifier == shape)
        .unwrap();
    assert_eq!(drawable.parent, Some(slide));
    assert!(doc.problems().is_empty(), "{:?}", doc.problems());
    assert!(doc.undeclared_references().is_empty());
}

/// A Numbers sheet holds it too, named by the sheet's own name.
#[test]
fn a_sheet_holds_a_box_named_by_the_sheets_name() {
    let mut doc = Document::new_spreadsheet("Blatt", "T", 4, 3).unwrap();
    let shape = doc
        .add_text_box(
            "Blatt",
            "Kasten auf dem Blatt",
            (100.0, 400.0),
            (240.0, 80.0),
        )
        .unwrap();

    let drawable = doc
        .drawables()
        .into_iter()
        .find(|d| d.identifier == shape)
        .unwrap();
    assert!(drawable.parent.is_some(), "a sheet owns its drawables");
    assert!(doc.problems().is_empty(), "{:?}", doc.problems());
    assert!(doc.undeclared_references().is_empty());
}

/// A box added to a document the app wrote borrows a style that document
/// already has, rather than inventing one.
#[test]
fn a_box_borrows_a_style_the_document_already_has() {
    let mut doc = fixture!("pages-report.pages");
    let known: Vec<u64> = doc.objects().map(|(_, o)| o.identifier).collect();
    let shape = doc
        .add_text_box("page 2", "Randnotiz", (380.0, 120.0), (160.0, 90.0))
        .unwrap();
    let drawable = doc
        .drawables()
        .into_iter()
        .find(|d| d.identifier == shape)
        .unwrap();
    let style = drawable.style.expect("a shape is drawn with a style");
    assert!(
        known.contains(&style),
        "the style is one the document already had, not a new object"
    );
    assert!(doc.problems().is_empty(), "{:?}", doc.problems());
}

/// An 8-pixel PNG, inline so the test needs no fixture: 32 × 24, which is what
/// the assertions below check the registry recorded.
fn png() -> Vec<u8> {
    const BASE64: &str = "iVBORw0KGgoAAAANSUhEUgAAACAAAAAYCAIAAAAUMWhjAAAAJElEQVR4nGO4o6FBU8QwasGo\
        BaMWjFowasGoBaMWjFowNCwAAEUJhC7iQqksAAAAAElFTkSuQmCC";
    let mut out = Vec::new();
    let mut bits = 0u32;
    let mut held = 0u32;
    for byte in BASE64.bytes().filter(|b| !b.is_ascii_whitespace()) {
        let value = match byte {
            b'A'..=b'Z' => byte - b'A',
            b'a'..=b'z' => byte - b'a' + 26,
            b'0'..=b'9' => byte - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            _ => continue,
        };
        bits = (bits << 6) | u32::from(value);
        held += 6;
        if held >= 8 {
            held -= 8;
            out.push((bits >> held) as u8);
        }
    }
    out
}

/// An image placed from nothing: the bytes go into the package, the registry
/// names them with their digest, and the drawable declares what it uses.
#[test]
fn an_image_placed_from_nothing_is_registered_and_declared() {
    let bytes = png();
    let mut doc = Document::new(Kind::Pages).unwrap();
    let image = doc
        .add_image(
            "page 1",
            &bytes,
            "probe.png",
            (100.0, 200.0),
            Some((160.0, 120.0)),
        )
        .unwrap();

    let drawable = doc
        .drawables()
        .into_iter()
        .find(|d| d.identifier == image)
        .expect("the image is a drawable");
    let media = drawable.media.as_ref().expect("an image carries media");
    assert_eq!(
        media.natural_size,
        Some((32.0, 24.0)),
        "the PNG's own pixels"
    );
    let frame = drawable.frame(None);
    assert_eq!((frame.width, frame.height), (160.0, 120.0));

    let file = doc
        .data_files()
        .into_iter()
        .find(|d| Some(d.identifier) == media.data)
        .expect("the registry lists it");
    assert_eq!(file.entry_name().as_deref(), Some("Data/probe-1011.png"));
    assert_eq!(
        file.digest,
        iwork::media::sha1(&bytes).to_vec(),
        "the digest is the SHA-1 of the bytes the app will read"
    );
    // `problems` is the one that matters: it is what catches an image whose
    // object does not declare the media it uses.
    assert!(doc.problems().is_empty(), "{:?}", doc.problems());
    assert!(doc.undeclared_references().is_empty());
}

/// No size given draws the picture at its own pixel size, which is what the app
/// does with one dropped in.
#[test]
fn an_image_with_no_size_is_drawn_at_its_own() {
    let mut doc = Document::new_spreadsheet("Blatt", "T", 4, 3).unwrap();
    let image = doc
        .add_image("Blatt", &png(), "probe.png", (100.0, 400.0), None)
        .unwrap();
    let frame = doc
        .drawables()
        .into_iter()
        .find(|d| d.identifier == image)
        .unwrap()
        .frame(None);
    assert_eq!((frame.width, frame.height), (32.0, 24.0));
}

/// Bytes whose pixel size cannot be read are refused rather than registered
/// with a size that is a guess.
#[test]
fn a_picture_that_is_not_a_png_or_a_jpeg_is_refused() {
    let mut doc = Document::new(Kind::Pages).unwrap();
    let refusal = doc
        .add_image("page 1", b"GIF89a nope", "probe.gif", (0.0, 0.0), None)
        .unwrap_err()
        .to_string();
    assert!(refusal.contains("not a PNG or a JPEG"), "{refusal}");
}

/// A new object does not inherit the neighbour's `MessageInfo` extras. The
/// neighbour is borrowed for its shape; its data references, object references
/// and version-patch fields are about the neighbour.
#[test]
fn a_new_object_claims_none_of_its_neighbours_media() {
    let mut doc = Document::new(Kind::Pages).unwrap();
    // An image first, so the next object is written beside one that *does*
    // declare media.
    doc.add_image("page 1", &png(), "probe.png", (0.0, 0.0), None)
        .unwrap();
    doc.add_text_box("page 1", "kein Bild", (0.0, 200.0), (100.0, 40.0))
        .unwrap();
    assert!(doc.problems().is_empty(), "{:?}", doc.problems());
}

/// A table on a Pages page: a drawable like any other, so it floats in a page
/// group and names no parent — and every cell of it is writable.
#[test]
fn a_table_can_float_on_a_pages_page() {
    let mut doc = fixture!("pages-report.pages");
    let table = doc
        .add_table_at("page 2", "Preise", 4, 3, (72.0, 400.0))
        .unwrap();
    doc.set_cell(
        "Preise",
        0,
        0,
        iwork::table::CellValue::Text("Posten".into()),
    )
    .unwrap();

    let drawable = doc
        .drawables()
        .into_iter()
        .find(|d| d.identifier == table)
        .expect("a table is a drawable");
    assert_eq!(drawable.kind, iwork::drawable::Kind::Table);
    assert_eq!(drawable.parent, None, "a Pages page owns nothing");
    assert_eq!(
        drawable.placement,
        iwork::drawable::Placement::Floating { page: 1 }
    );
    let frame = drawable.frame(None);
    assert_eq!((frame.x, frame.y), (72.0, 400.0));
    assert_eq!(doc.table("Preise").unwrap().value(0, 0).to_text(), "Posten");
    assert!(doc.problems().is_empty(), "{:?}", doc.problems());
    assert!(doc.undeclared_references().is_empty());
}

/// A document with no table to borrow styles from is refused by name rather
/// than given an invented table style.
#[test]
fn a_table_needs_a_table_to_borrow_styles_from() {
    let mut doc = Document::new(Kind::Pages).unwrap();
    let refusal = doc
        .add_table_at("page 1", "Preise", 4, 3, (0.0, 0.0))
        .unwrap_err()
        .to_string();
    assert!(
        refusal.contains("no table to borrow styles from"),
        "{refusal}"
    );
}

/// The apps read the words back. Off unless `IWORK_APP_CHECK=1`.
#[test]
fn the_apps_read_back_a_box_this_crate_put_on_a_page() {
    if std::env::var("IWORK_APP_CHECK").as_deref() != Ok("1") {
        eprintln!("IWORK_APP_CHECK is not 1 — skipping the app round trip");
        return;
    }
    let check = |path: &Path, expected: &str| {
        let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("scripts/app-check.sh");
        let output = std::process::Command::new(&script)
            .arg(path)
            .arg(expected)
            .output()
            .unwrap_or_else(|e| panic!("{}: {e}", script.display()));
        assert!(
            output.status.success(),
            "the app would not read {expected:?} back out of {}:\n{}\n{}",
            path.display(),
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    };

    let mut doc = Document::new(Kind::Pages).unwrap();
    doc.add_text_box(
        "page 1",
        "Kasten aus dem Nichts",
        (72.0, 300.0),
        (400.0, 100.0),
    )
    .unwrap();
    doc.add_shape(
        "page 1",
        Outline::Ellipse,
        "Kreis",
        (72.0, 450.0),
        (200.0, 200.0),
    )
    .unwrap();
    doc.add_shape("page 1", Outline::Line, "", (72.0, 700.0), (400.0, 0.0))
        .unwrap();
    let out = scratch("iwork-drawn.pages");
    doc.save(&out).unwrap();
    check(&out, "Kasten aus dem Nichts");
    let _ = std::fs::remove_dir_all(&out);

    let mut deck = Document::new(Kind::Keynote).unwrap();
    let slide = deck.slides()[0].identifier;
    deck.add_text_box(
        &slide.to_string(),
        "Kasten auf der Folie",
        (100.0, 100.0),
        (600.0, 120.0),
    )
    .unwrap();
    let out = scratch("iwork-drawn.key");
    deck.save(&out).unwrap();
    check(&out, "Kasten auf der Folie");
    let _ = std::fs::remove_dir_all(&out);

    let mut sheet = Document::new_spreadsheet("Blatt", "T", 4, 3).unwrap();
    sheet
        .add_text_box(
            "Blatt",
            "Kasten auf dem Blatt",
            (100.0, 400.0),
            (240.0, 80.0),
        )
        .unwrap();
    sheet
        .add_image("Blatt", &png(), "probe.png", (100.0, 520.0), None)
        .unwrap();
    let out = scratch("iwork-drawn.numbers");
    sheet.save(&out).unwrap();
    check(&out, "Kasten auf dem Blatt");
    let _ = std::fs::remove_dir_all(&out);
}

/// A picture placed from nothing goes through the app's own model and comes
/// back with its bytes. Off unless `IWORK_APP_CHECK=1`.
#[test]
fn the_apps_resave_a_document_with_an_added_image() {
    if std::env::var("IWORK_APP_CHECK").as_deref() != Ok("1") {
        eprintln!("IWORK_APP_CHECK is not 1 — skipping the resave");
        return;
    }
    let bytes = png();
    let digest = iwork::media::sha1(&bytes).to_vec();
    let resave = |path: &Path| {
        let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("scripts/resave.sh");
        let status = std::process::Command::new(&script)
            .arg(path)
            .status()
            .unwrap_or_else(|e| panic!("{}: {e}", script.display()));
        assert!(
            status.success(),
            "the app would not resave {}",
            path.display()
        );
    };
    let kept = |path: &Path, size: (f32, f32)| {
        let after = Document::open(path).unwrap();
        let files = after.data_files();
        let image = after
            .drawables()
            .into_iter()
            .find(|d| {
                d.media
                    .as_ref()
                    .and_then(|m| m.data)
                    .and_then(|data| files.iter().find(|f| f.identifier == data))
                    .is_some_and(|file| file.digest == digest)
            })
            .unwrap_or_else(|| panic!("the app dropped the picture out of {}", path.display()));
        let frame = image.frame(None);
        assert_eq!((frame.width, frame.height), size);
        assert!(after.problems().is_empty(), "{:?}", after.problems());
    };

    let mut doc = Document::new(Kind::Pages).unwrap();
    doc.add_image(
        "page 1",
        &bytes,
        "probe.png",
        (100.0, 200.0),
        Some((160.0, 120.0)),
    )
    .unwrap();
    let out = scratch("iwork-resaved-image.pages");
    doc.save(&out).unwrap();
    resave(&out);
    kept(&out, (160.0, 120.0));
    let _ = std::fs::remove_dir_all(&out);

    let mut deck = Document::new(Kind::Keynote).unwrap();
    let slide = deck.slides()[0].identifier;
    deck.add_image(
        &slide.to_string(),
        &bytes,
        "probe.png",
        (200.0, 200.0),
        Some((320.0, 240.0)),
    )
    .unwrap();
    let out = scratch("iwork-resaved-image.key");
    deck.save(&out).unwrap();
    resave(&out);
    kept(&out, (320.0, 240.0));
    let _ = std::fs::remove_dir_all(&out);
}

/// The measure past opening: the app loads the box into its own model and
/// writes it back. Off unless `IWORK_APP_CHECK=1`.
#[test]
fn pages_and_keynote_resave_a_document_with_an_added_box() {
    if std::env::var("IWORK_APP_CHECK").as_deref() != Ok("1") {
        eprintln!("IWORK_APP_CHECK is not 1 — skipping the resave");
        return;
    }
    let resave = |path: &Path| {
        let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("scripts/resave.sh");
        let status = std::process::Command::new(&script)
            .arg(path)
            .status()
            .unwrap_or_else(|e| panic!("{}: {e}", script.display()));
        assert!(
            status.success(),
            "the app would not resave {}",
            path.display()
        );
    };
    // Whatever the app wrote back, the box is still there, still that size,
    // and still holding its words.
    let kept = |path: &Path, text: &str, size: (f32, f32)| {
        let after = Document::open(path).unwrap();
        let box_ = after
            .drawables()
            .into_iter()
            .find(|d| {
                d.text
                    .and_then(|s| after.storage_text(s).ok())
                    .is_some_and(|t| t == text)
            })
            .unwrap_or_else(|| panic!("the app dropped the box out of {}", path.display()));
        let frame = box_.frame(None);
        assert_eq!((frame.width, frame.height), size);
        assert!(after.problems().is_empty(), "{:?}", after.problems());
    };

    let mut doc = Document::new(Kind::Pages).unwrap();
    doc.add_text_box(
        "page 1",
        "Kasten durch Pages hindurch",
        (72.0, 300.0),
        (400.0, 100.0),
    )
    .unwrap();
    let out = scratch("iwork-resaved-box.pages");
    doc.save(&out).unwrap();
    resave(&out);
    kept(&out, "Kasten durch Pages hindurch", (400.0, 100.0));
    let _ = std::fs::remove_dir_all(&out);

    let mut deck = Document::new(Kind::Keynote).unwrap();
    let slide = deck.slides()[0].identifier;
    deck.add_shape(
        &slide.to_string(),
        Outline::Ellipse,
        "Ellipse durch Keynote hindurch",
        (200.0, 200.0),
        (300.0, 200.0),
    )
    .unwrap();
    let out = scratch("iwork-resaved-box.key");
    deck.save(&out).unwrap();
    resave(&out);
    kept(&out, "Ellipse durch Keynote hindurch", (300.0, 200.0));
    let _ = std::fs::remove_dir_all(&out);
}
