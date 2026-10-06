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

/// **A table made from nothing is drawn as a table.**
///
/// It used not to be. The `TST.TableStyleArchive` this crate wrote was a name
/// and a stylesheet reference; the one Numbers writes carries 63 properties,
/// and gridlines, borders and the line under a header are among them — they
/// are `TST.TableStylePropertiesArchive`, not properties of cells. The test
/// that stood here asserted the stub and said to delete itself the day it
/// grew. It grew.
///
/// What is asserted is the default Numbers gives a new table, value for value:
/// gridlines and the border on, a header row and header column in their greys,
/// a body that paints nothing.
#[test]
fn a_table_from_nothing_carries_the_look_numbers_gives_one() {
    let doc = Document::new_spreadsheet("Blatt", "Tabelle", 3, 2).unwrap();

    // The roles, under the names the app's own schema gives them.
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
        "tableCell-0-categoryLevel1Row",
        "tableCell-0-labelLevel1Row",
        "tableCell-0-pivotBodySummaryRow",
        "tableCell-0-pivotBodySummaryColumn",
        "tableCell-0-pivotHeaderColumnSummary",
    ] {
        assert!(names.iter().any(|n| n == role), "{role} missing: {names:?}");
    }
    for invented in ["groupLevel1Style", "pivotHeaderStyle", "pivotValueStyle"] {
        assert!(
            !names.iter().any(|n| n.ends_with(invented)),
            "{invented} is a name no document Numbers wrote carries"
        );
    }

    // Header row grey, header column grey, body and footer present and empty.
    match named(&doc, "tableCell-0-headerRowStyle").and_then(|s| s.fill) {
        Some(Fill::Color(c)) => assert!((c.red - 0.743_613_24).abs() < 1e-6, "{c:?}"),
        other => panic!("the header row is painted: {other:?}"),
    }
    match named(&doc, "tableCell-0-headerColumnStyle").and_then(|s| s.fill) {
        Some(Fill::Color(c)) => assert!((c.red - 0.862_404_9).abs() < 1e-6, "{c:?}"),
        other => panic!("the header column is painted: {other:?}"),
    }
    for role in ["tableCell-0-bodyStyle", "tableCell-0-footerRowStyle"] {
        assert!(
            matches!(named(&doc, role).and_then(|s| s.fill), Some(Fill::None)),
            "{role} carries a fill that paints nothing"
        );
    }

    // The table style: gridlines and the border on, sixteen strokes by role,
    // and a count that is the number of properties in the bag.
    let style = doc
        .objects()
        .find(|(_, object)| object.message_type() == 6003)
        .map(|(_, object)| object.identifier)
        .expect("a TST.TableStyleArchive");
    let archive = doc.archive(style).unwrap();
    let bag = match iwork::style::get_path(&archive, &[11]) {
        Some(iwork::pb::Value::Bytes(raw)) => iwork::pb::Message::decode(&raw).unwrap(),
        other => panic!("the table style has no properties: {other:?}"),
    };
    for (field, what) in [
        (33u32, "v_strokes_visible"),
        (34, "h_strokes_visible"),
        (35, "hr_separator_visible"),
        (38, "table_border_visible"),
    ] {
        assert_eq!(bag.varint(field), Some(1), "{what} is on");
    }
    for field in 46..=61u32 {
        assert!(bag.bytes(field).is_some(), "stroke {field} is there");
    }
    assert_eq!(
        iwork::style::get_path(&archive, iwork::style::OVERRIDE_COUNT),
        Some(iwork::pb::Value::Varint(bag.fields.len() as u64)),
        "override_count is the number of properties in the bag"
    );
    assert!(doc.problems().is_empty(), "{:?}", doc.problems());
}

/// **The body text style goes in the body's slot.**
///
/// `TST.TableModelArchive` names its slots: 24 is `body_text_style`, 25 the
/// header row's, 26 the header column's, 27 the footer's. This crate wrote its
/// bold "Table Header" style into 24 and its plain one into the other three —
/// the body in bold and the headers not. Nothing showed it, because the styles
/// carried no `override_count` and Numbers was discarding what they said.
#[test]
fn the_body_of_a_table_is_set_in_the_regular_face_and_its_headers_in_the_bold() {
    let doc = Document::new_spreadsheet("Blatt", "Tabelle", 3, 2).unwrap();
    let model = doc
        .objects()
        .find(|(_, object)| object.message_type() == 6001)
        .map(|(_, object)| object.identifier)
        .expect("a table model");
    let archive = doc.archive(model).unwrap();
    let font_at = |slot: u32| -> String {
        let style = iwork::style::reference_at(&archive, &[slot, 1])
            .unwrap_or_else(|| panic!("slot {slot} names no style"));
        let style = doc.archive(style).unwrap();
        iwork::style::string_at(&style, iwork::style::property::FONT_NAME).unwrap_or_default()
    };
    assert_eq!(font_at(24), "HelveticaNeue", "body_text_style");
    assert_eq!(font_at(25), "HelveticaNeue-Bold", "header_row_text_style");
    assert_eq!(
        font_at(26),
        "HelveticaNeue-Bold",
        "header_column_text_style"
    );
    assert_eq!(font_at(27), "HelveticaNeue-Bold", "footer_row_text_style");
}

/// A document an older version of this crate wrote names nine of the roles
/// differently, and a table can still be added to it.
///
/// A Pages document carries the table styles with no table to copy the
/// references from, so `add_table` finds them by name — and this one is given
/// the names 0.2.2 and earlier wrote before it is asked.
#[test]
fn a_table_can_still_be_added_where_the_old_role_names_are() {
    const RENAMED: &[(&str, &str)] = &[
        ("tableCell-0-labelLevel1Row", "tableCell-0-groupLevel1Style"),
        ("tableCell-0-labelLevel2Row", "tableCell-0-groupLevel2Style"),
        ("tableCell-0-labelLevel3Row", "tableCell-0-groupLevel3Style"),
        ("tableCell-0-labelLevel4Row", "tableCell-0-groupLevel4Style"),
        ("tableCell-0-labelLevel5Row", "tableCell-0-groupLevel5Style"),
        (
            "tableCell-0-pivotBodySummaryRow",
            "tableCell-0-pivotHeaderStyle",
        ),
        (
            "tableCell-0-pivotBodySummaryColumn",
            "tableCell-0-pivotValueStyle",
        ),
        (
            "tableCell-0-pivotHeaderColumnSummary",
            "tableCell-0-pivotTotalStyle",
        ),
        (
            "text-0-paragraphstyle-Table Label 1",
            "text-0-paragraphstyle-Table Group 1",
        ),
        (
            "text-0-paragraphstyle-Table Label 2",
            "text-0-paragraphstyle-Table Group 2",
        ),
        (
            "text-0-paragraphstyle-Table Label 3",
            "text-0-paragraphstyle-Table Group 3",
        ),
        (
            "text-0-paragraphstyle-Table Label 4",
            "text-0-paragraphstyle-Table Group 4",
        ),
        (
            "text-0-paragraphstyle-Table Label 5",
            "text-0-paragraphstyle-Table Group 5",
        ),
    ];
    let mut doc = Document::new(iwork::Kind::Pages).unwrap();
    let styles: Vec<u64> = doc
        .objects()
        .filter(|(_, object)| matches!(object.message_type(), 6004 | 2022))
        .map(|(_, object)| object.identifier)
        .collect();
    let mut renamed = 0;
    for identifier in styles {
        let mut archive = doc.archive(identifier).unwrap();
        let Some(current) = iwork::style::string_at(&archive, &[1, 2]) else {
            continue;
        };
        // The old document had no header-column text style of its own either.
        if current == "text-0-paragraphstyle-Table Header Column" {
            iwork::style::set_path(
                &mut archive,
                &[1, 2],
                Some(iwork::pb::Value::Bytes(
                    b"text-0-paragraphstyle-Unused".to_vec(),
                )),
            )
            .unwrap();
            doc.set_archive_for(identifier, &archive).unwrap();
            continue;
        }
        if let Some((_, old)) = RENAMED.iter().find(|(new, _)| *new == current) {
            iwork::style::set_path(
                &mut archive,
                &[1, 2],
                Some(iwork::pb::Value::Bytes(old.as_bytes().to_vec())),
            )
            .unwrap();
            doc.set_archive_for(identifier, &archive).unwrap();
            renamed += 1;
        }
    }
    assert_eq!(
        renamed,
        RENAMED.len(),
        "every renamed role was found to rename back"
    );

    doc.add_table("page 1", "Zahlen", 3, 2).unwrap();
    assert_eq!(doc.tables().len(), 1);
    assert!(doc.problems().is_empty(), "{:?}", doc.problems());
}

/// **The acceptance test for the look: Numbers paints the header row.**
/// Off unless `IWORK_APP_CHECK=1`.
#[test]
fn numbers_draws_a_table_from_nothing_with_a_shaded_bold_header() {
    if std::env::var("IWORK_APP_CHECK").as_deref() != Ok("1") {
        eprintln!("IWORK_APP_CHECK is not 1 — skipping the cell-look oracle");
        return;
    }
    let mut doc = Document::new_spreadsheet("Blatt", "Tabelle", 3, 3).unwrap();
    let mut table = doc.table_mut("Tabelle").unwrap();
    table
        .set_block("A1", &[vec!["Region", "Einheiten", "Ertrag"]])
        .unwrap();
    table.set("A2", "Zürich").unwrap();
    table.set("B2", 1240).unwrap();
    table.set("C2", 184_300).unwrap();

    let out = std::env::temp_dir().join("iwork-look.numbers");
    let _ = std::fs::remove_file(&out);
    doc.save(&out).unwrap();

    let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("scripts/cell-look-oracle.sh");
    let output = std::process::Command::new(&script)
        .arg(&out)
        .output()
        .unwrap_or_else(|e| panic!("{}: {e}", script.display()));
    assert!(
        output.status.success(),
        "Numbers would not answer:\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let answer = String::from_utf8_lossy(&output.stdout).to_string();
    let cell = |name: &str| -> Vec<String> {
        answer
            .lines()
            .map(|line| line.split('\t').map(str::to_string).collect::<Vec<_>>())
            .find(|f| {
                f.first().map(String::as_str) == Some("cell")
                    && f.get(1).map(String::as_str) == Some(name)
            })
            .unwrap_or_else(|| panic!("no cell {name} in:\n{answer}"))
    };
    // cell, name, background, font name, font size, text colour
    let header = cell("B1");
    let body = cell("B2");
    assert_ne!(header[2], "none", "the header row is painted: {header:?}");
    assert!(
        header[3].contains("Bold"),
        "the header row is set in the bold face: {header:?}"
    );
    assert!(
        !body[3].contains("Bold"),
        "the body is set in the regular face: {body:?}"
    );
    let _ = std::fs::remove_file(&out);
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
