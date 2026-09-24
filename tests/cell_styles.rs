//! How a table cell is painted — read, and the one property that is written.
//!
//! Numbers keeps a cell style per *role* in the document stylesheet:
//! `tableCell-0-headerRowStyle`, `-bodyStyle`, `-headerColumnStyle`, the five
//! category levels and the rest. They are shared, so repainting one repaints
//! every table whose table style names it.
//!
//! The measurement that decides what this crate claims: painting a style in a
//! document **Numbers wrote** changes what Numbers draws — asked for the
//! header cell's background afterwards, it answered `5959,17617,36075` where
//! it had answered `45231,46004,45746`. Painting the same style in a document
//! built from nothing changes nothing on screen, because that document's
//! `TST.TableStyleArchive` is a stub and nothing routes a row to a cell style
//! by role. Both facts are asserted below, the second as the limitation it is.

use std::path::{Path, PathBuf};

use iwork::drawable::{Color, Fill};
use iwork::Document;

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

fn named(doc: &Document, name: &str) -> Option<iwork::table::CellStyle> {
    doc.cell_styles()
        .into_iter()
        .find(|style| style.name.as_deref() == Some(name))
}

/// A document Numbers wrote names its cell styles by role and paints some of
/// them. The header row is grey; the body is a fill that is *present and
/// paints nothing*, which is not the same as a missing fill.
#[test]
fn a_real_document_names_its_cell_styles_by_role() {
    let doc = fixture!("numbers-values.numbers");

    let header = named(&doc, "tableCell-0-headerRowStyle").expect("a header row style");
    match header.fill {
        Some(Fill::Color(colour)) => {
            assert!(
                colour.red > 0.5 && colour.alpha == 1.0,
                "the header row is painted a light grey: {colour:?}"
            );
        }
        other => panic!("expected a colour on the header row, got {other:?}"),
    }

    let body = named(&doc, "tableCell-0-bodyStyle").expect("a body style");
    assert!(
        matches!(body.fill, Some(Fill::None)),
        "a body cell carries an empty fill archive, not a missing one: {:?}",
        body.fill
    );

    // Numbers gives every cell four points of inset.
    assert_eq!(header.insets, Some([4.0, 4.0, 4.0, 4.0]));

    // The five category levels step through progressively darker greys, which
    // is how a grouped table shows its depth.
    let level = |n: u32| {
        named(&doc, &format!("tableCell-0-categoryLevel{n}Row"))
            .and_then(|s| match s.fill {
                Some(Fill::Color(c)) => Some(c.red),
                _ => None,
            })
            .unwrap_or(f32::NAN)
    };
    assert!(
        level(1) > level(2) && level(2) > level(3),
        "each category level is darker than the last: {} {} {}",
        level(1),
        level(2),
        level(3)
    );
}

/// Painting a style, and unpainting it, round-trips through the archive.
#[test]
fn a_fill_written_to_a_cell_style_is_read_back() {
    let mut doc = fixture!("numbers-values.numbers");
    let header = named(&doc, "tableCell-0-headerRowStyle")
        .expect("a header row style")
        .identifier;

    let blue = Color {
        red: 0.11,
        green: 0.35,
        blue: 0.62,
        alpha: 1.0,
    };
    doc.set_cell_style_fill(header, Some(blue)).unwrap();

    match named(&doc, "tableCell-0-headerRowStyle").and_then(|s| s.fill) {
        Some(Fill::Color(c)) => {
            assert!((c.red - 0.11).abs() < 1e-6, "{c:?}");
            assert!((c.green - 0.35).abs() < 1e-6, "{c:?}");
            assert!((c.blue - 0.62).abs() < 1e-6, "{c:?}");
        }
        other => panic!("expected the colour back, got {other:?}"),
    }
    assert!(doc.problems().is_empty(), "{:?}", doc.problems());

    // `None` leaves the empty archive Numbers writes for "paints nothing",
    // which reads back as `Fill::None` and not as an absent fill.
    doc.set_cell_style_fill(header, None).unwrap();
    assert!(matches!(
        named(&doc, "tableCell-0-headerRowStyle").and_then(|s| s.fill),
        Some(Fill::None)
    ));
    assert!(doc.problems().is_empty(), "{:?}", doc.problems());
}

/// A cell style is not just any object, and saying so costs one lookup.
#[test]
fn painting_something_that_is_not_a_cell_style_is_refused() {
    let doc = Document::new_spreadsheet("Blatt", "Tabelle", 2, 2).unwrap();
    let table = doc.tables().first().map(|t| t.identifier).expect("a table");
    let mut doc = doc;
    let error = doc
        .set_cell_style_fill(
            table,
            Some(Color {
                red: 1.0,
                green: 0.0,
                blue: 0.0,
                alpha: 1.0,
            }),
        )
        .unwrap_err();
    assert_eq!(
        error.refusal(),
        Some(iwork::Refusal::WrongSlot),
        "{error:?}"
    );
}

/// **The limitation, asserted.** A document from nothing carries every named
/// cell style, and painting one is accepted and read back — and Numbers still
/// draws the table unstyled, because its `TST.TableStyleArchive` is a stub.
/// This test exists so that the day the archive is written properly, it fails
/// and someone comes back to delete it.
#[test]
fn a_table_from_nothing_has_the_styles_and_not_the_archive_that_uses_them() {
    let mut doc = Document::new_spreadsheet("Blatt", "Tabelle", 3, 2).unwrap();

    let names: Vec<String> = doc
        .cell_styles()
        .into_iter()
        .filter_map(|s| s.name)
        .collect();
    for role in [
        "tableCell-0-bodyStyle",
        "tableCell-0-headerRowStyle",
        "tableCell-0-headerColumnStyle",
        "tableCell-0-footerRowStyle",
    ] {
        assert!(names.iter().any(|n| n == role), "{role} missing: {names:?}");
    }
    // Every one of them starts unpainted.
    assert!(
        doc.cell_styles()
            .iter()
            .all(|s| matches!(s.fill, Some(Fill::None) | None)),
        "a table from nothing paints nothing"
    );

    let header = named(&doc, "tableCell-0-headerRowStyle")
        .expect("a header row style")
        .identifier;
    doc.set_cell_style_fill(
        header,
        Some(Color {
            red: 0.11,
            green: 0.35,
            blue: 0.62,
            alpha: 1.0,
        }),
    )
    .unwrap();
    assert!(doc.problems().is_empty(), "{:?}", doc.problems());

    // The table style is the piece that is missing. Numbers writes 8 344
    // bytes here; this is two orders of magnitude short of that, and the
    // difference is what draws gridlines, borders and the per-role routing.
    let style = doc
        .objects()
        .find(|(_, object)| object.message_type() == 6003)
        .map(|(_, object)| object.identifier)
        .expect("a TST.TableStyleArchive");
    let size = doc.archive(style).unwrap().encode().len();
    assert!(
        size < 1_000,
        "the table style is still a stub at {size} bytes — if this has grown, \
         check whether Numbers now draws a table built from nothing, and if it \
         does, delete this test"
    );
}

/// **`override_count` decides whether the app keeps the bag at all.**
///
/// Every `TST.CellStyleArchive` Numbers writes carries `10 = 4` beside its
/// four-entry property bag. A document from this crate carried none, and
/// Numbers' own save deleted field 11 from all seventeen cell styles — the
/// fill, and the four-point insets nobody had asked it to touch, 91 bytes
/// down to 30. The count is written now, and the bags survive.
#[test]
fn a_cell_style_carries_the_override_count_the_app_writes() {
    let doc = Document::new_spreadsheet("Blatt", "Tabelle", 2, 2).unwrap();
    for style in doc.cell_styles() {
        let archive = doc.archive(style.identifier).unwrap();
        let count = iwork::style::get_path(&archive, iwork::style::OVERRIDE_COUNT);
        assert!(
            matches!(count, Some(iwork::pb::Value::Varint(n)) if n > 0),
            "{:?} has no override count: {count:?} — Numbers deletes the bag of a \
             style that claims to override nothing",
            style.name
        );
        // And it agrees with the bag it describes.
        assert_eq!(
            count,
            Some(iwork::pb::Value::Varint(4)),
            "{:?}: Numbers writes 4 beside a four-entry bag",
            style.name
        );
    }
}

/// Painting bumps the count, because painting adds a property.
#[test]
fn painting_a_cell_style_keeps_the_count_in_step_with_the_bag() {
    let mut doc = Document::new_spreadsheet("Blatt", "Tabelle", 2, 2).unwrap();
    let header = named(&doc, "tableCell-0-headerRowStyle")
        .expect("a header row style")
        .identifier;
    doc.set_cell_style_fill(
        header,
        Some(Color {
            red: 0.12,
            green: 0.22,
            blue: 0.38,
            alpha: 1.0,
        }),
    )
    .unwrap();

    let archive = doc.archive(header).unwrap();
    let bag = match iwork::style::get_path(&archive, &[11]) {
        Some(iwork::pb::Value::Bytes(raw)) => iwork::pb::Message::decode(&raw).unwrap(),
        other => panic!("no property bag: {other:?}"),
    };
    let count = match iwork::style::get_path(&archive, iwork::style::OVERRIDE_COUNT) {
        Some(iwork::pb::Value::Varint(n)) => n as usize,
        other => panic!("no override count: {other:?}"),
    };
    assert_eq!(
        count,
        bag.fields.len(),
        "the count follows the bag, so the two cannot disagree"
    );
}

/// **The acceptance test: Numbers' own save keeps the paint.**
/// Off unless `IWORK_APP_CHECK=1`.
#[test]
fn numbers_keeps_a_cell_fill_through_its_own_save() {
    if std::env::var("IWORK_APP_CHECK").as_deref() != Ok("1") {
        eprintln!("IWORK_APP_CHECK is not 1 — skipping the resave");
        return;
    }
    let mut doc = Document::new_spreadsheet("Blatt", "Tabelle", 3, 3).unwrap();
    let body = named(&doc, "tableCell-0-bodyStyle").unwrap().identifier;
    let blue = Color {
        red: 0.92,
        green: 0.94,
        blue: 0.97,
        alpha: 1.0,
    };
    doc.set_cell_style_fill(body, Some(blue)).unwrap();
    doc.table_mut("Tabelle")
        .unwrap()
        .set_block("A1", &[vec!["Region", "Einheiten", "Ertrag"]])
        .unwrap();

    let out = std::env::temp_dir().join("iwork-painted-cells.numbers");
    let _ = std::fs::remove_file(&out);
    doc.save(&out).unwrap();

    let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("scripts/resave.sh");
    let output = std::process::Command::new(&script)
        .arg(&out)
        .output()
        .unwrap_or_else(|e| panic!("{}: {e}", script.display()));
    assert!(
        output.status.success(),
        "Numbers would not resave it:\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let after = Document::open(&out).unwrap();
    match named(&after, "tableCell-0-bodyStyle").and_then(|s| s.fill) {
        Some(Fill::Color(c)) => {
            assert!((c.red - 0.92).abs() < 1e-4, "Numbers kept the fill: {c:?}");
            assert!((c.blue - 0.97).abs() < 1e-4, "{c:?}");
        }
        other => panic!("Numbers deleted the fill: {other:?}"),
    }
    // And the insets the crate wrote and never asked Numbers to touch.
    assert_eq!(
        named(&after, "tableCell-0-bodyStyle").and_then(|s| s.insets),
        Some([4.0, 4.0, 4.0, 4.0])
    );
    let _ = std::fs::remove_file(&out);
}
