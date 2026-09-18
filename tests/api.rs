//! The shape of the API, rather than the shape of the format.
//!
//! Everything here is a forwarding layer over writes the other test files
//! already prove; what it asserts is that the layer is honest — that a cell
//! named `"B3"` is the cell the archive calls row 2 column 1, that a handle
//! cannot go stale, and above all that **a sheet is not a grid**. A Numbers
//! sheet holds any number of tables, two sheets may hold tables of one name,
//! and an API that assumes one grid per sheet is wrong about the format rather
//! than merely inconvenient.

use std::path::{Path, PathBuf};

use iwork::table::{CellRef, CellValue, Decimal, Format, TableRef};
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

/// A1 and the archive's row and column are the same cell, both ways.
#[test]
fn a_cell_is_named_either_way() {
    for (name, row, column) in [
        ("A1", 0, 0),
        ("B3", 2, 1),
        ("Z1", 0, 25),
        ("AA1", 0, 26),
        ("C300", 299, 2),
    ] {
        assert_eq!(CellRef::from(name).resolve().unwrap(), (row, column));
        assert_eq!(
            CellRef::from((row, column)).resolve().unwrap(),
            (row, column)
        );
        // And a reference prints as the app writes it, whichever way it came
        // in — an error message about `r2c1` is one the reader has to decode.
        assert_eq!(CellRef::from((row, column)).to_string(), name);
        assert_eq!(CellRef::from(name).to_string(), name);
    }
    for bad in ["", "3", "B", "B0", "3B", "hello"] {
        assert!(
            CellRef::from(bad).resolve().is_err(),
            "{bad:?} was read as a cell"
        );
    }
}

/// A value is written from what the caller has, not from a string.
#[test]
fn a_value_comes_from_the_thing_it_is() {
    assert_eq!(CellValue::from("Region"), CellValue::Text("Region".into()));
    assert_eq!(CellValue::from(true), CellValue::Bool(true));
    assert_eq!(
        CellValue::from(1_240u32),
        CellValue::Number(Decimal::parse("1240").unwrap())
    );
    assert_eq!(
        CellValue::from(-7i64),
        CellValue::Number(Decimal::parse("-7").unwrap())
    );
    // A double keeps the value it stands for, not its integer part.
    assert_eq!(CellValue::from(0.25f64).to_text(), "0.25");
    // A hole in the data is an empty cell.
    assert_eq!(CellValue::from(None::<i64>), CellValue::Empty);
    assert_eq!(CellValue::from(Some(3i64)).to_text(), "3");
    // A string stays a string: a cell holding "007" is what was asked for.
    assert_eq!(CellValue::from("007"), CellValue::Text("007".into()));
}

/// **A sheet is a canvas, not a grid.** It holds a list of drawables, and the
/// tables are the ones that happen to be tables.
#[test]
fn a_sheet_holds_any_number_of_tables() {
    let doc = fixture!("numbers-formats.numbers");
    let sheets = doc.sheets();
    assert_eq!(sheets.len(), 1, "{:?}", sheets);
    let sheet = &sheets[0];
    assert_eq!(sheet.name, "Formate");
    let tables = sheet.tables(&doc);
    assert_eq!(tables.len(), 3, "one sheet, three tables");
    assert_eq!(
        tables.iter().map(|t| t.name.as_str()).collect::<Vec<_>>(),
        vec!["Formate", "Verbunden", "Spaltenformat"]
    );
    // Every table the document has is on some sheet, and says which.
    for table in doc.tables() {
        assert_eq!(table.sheet.as_deref(), Some("Formate"));
    }

    // Two sheets, and the drawables of each are its own.
    let doc = fixture!("numbers-values.numbers");
    let sheets = doc.sheets();
    assert_eq!(sheets.len(), 2);
    assert_eq!(sheets[0].tables(&doc).len(), 2);
    assert_eq!(sheets[1].tables(&doc).len(), 1);
    assert!(doc.sheet("Zweites Blatt").is_some());
    assert!(doc.sheet("keines").is_none());

    // Pages and Keynote have no sheets; their tables hang off pages and slides.
    let pages = fixture!("pages-report.pages");
    assert!(pages.sheets().is_empty());
    assert!(!pages.tables().is_empty(), "the report has a table");
}

/// A name that two sheets both use is refused, not resolved to whichever came
/// first — and the sheet is how a caller says which.
#[test]
fn a_table_name_two_sheets_share_is_refused() {
    let mut doc = fixture!("numbers-pivot.numbers");
    let error = doc
        .table_mut("Sales")
        .err()
        .expect("two tables are called Sales")
        .to_string();
    assert!(error.contains("names 2 tables"), "{error}");
    assert!(error.contains("Pivot Table Basics"), "{error}");

    // By sheet, and by identifier.
    let first = doc
        .table_mut(("Pivot Table Basics", "Sales"))
        .unwrap()
        .identifier();
    let second = doc
        .table_mut(("Pivot Table Practice", "Sales"))
        .unwrap()
        .identifier();
    assert_ne!(first, second);
    assert_eq!(doc.table_mut(first).unwrap().identifier(), first);
    assert!(doc.table_mut(("Pivot Table Basics", "keine")).is_err());
    assert!(doc.table_mut(("kein Blatt", "Sales")).is_err());

    // The three ways of naming one, as the type sees them.
    assert_eq!(TableRef::from("Sales"), TableRef::Name("Sales"));
    assert_eq!(
        TableRef::from(("Blatt", "Sales")),
        TableRef::On("Blatt", "Sales")
    );
    assert_eq!(TableRef::from(7u64), TableRef::Identifier(7));
}

/// The handle writes what the `Document` methods write, and cannot go stale:
/// it holds a name, not a copy of the cells.
#[test]
fn a_handle_writes_and_never_goes_stale() {
    let mut doc = Document::new_spreadsheet("Blatt", "Q1", 6, 3).unwrap();
    let mut q1 = doc.table_mut("Q1").unwrap();

    q1.set_block("A1", &[vec!["Region", "Units"]]).unwrap();
    q1.set("A2", "Zürich").unwrap();
    q1.set("B2", 1_240).unwrap();
    q1.set("B3", 980).unwrap();
    // Read back through the handle — the write before it is visible, which a
    // snapshot taken at the start would not be.
    assert_eq!(q1.value("B2").unwrap().to_text(), "1240");
    assert_eq!(q1.read().rows, 6);

    q1.formula("B4", "=SUM(B2:B3)", 2_220).unwrap();
    q1.format("B4", &Format::Number { decimals: Some(0) })
        .unwrap();
    q1.width(0, Some(140.0)).unwrap();
    q1.height(0, Some(28.0)).unwrap();
    assert_eq!(q1.value("B4").unwrap().to_text(), "2220");

    q1.insert_row(1).unwrap();
    assert_eq!(q1.read().rows, 7);
    assert_eq!(
        q1.value("A3").unwrap().to_text(),
        "Zürich",
        "the row did not move down"
    );
    q1.delete_row(1).unwrap();
    assert_eq!(q1.read().rows, 6);
    assert_eq!(q1.value("A2").unwrap().to_text(), "Zürich");

    q1.set("C2", "weg").unwrap();
    q1.merge("C2", 1, 1).unwrap_err(); // one cell is not a merge
    q1.set("C3", 1).unwrap();
    q1.merge("C2", 2, 1).unwrap();
    assert_eq!(q1.read().merges.len(), 1);
    q1.unmerge("C2").unwrap();
    assert!(q1.read().merges.is_empty());

    let table = q1.read();
    assert!(table.audit().is_empty(), "{:?}", table.audit());
    assert!(doc.problems().is_empty(), "{:?}", doc.problems());
}

/// A cell named wrongly is refused by the handle before anything moves, and
/// says what it could not read.
#[test]
fn a_cell_that_is_not_a_cell_is_refused() {
    let mut doc = Document::new_spreadsheet("Blatt", "Q1", 3, 2).unwrap();
    let mut q1 = doc.table_mut("Q1").unwrap();
    let error = q1.set("nowhere", 1).expect_err("not a cell").to_string();
    assert!(error.contains("not a cell reference like B3"), "{error}");
    // Out of the table is the table's own refusal, and it names the cell the
    // way the caller does.
    let error = q1.set("Z9", 1).expect_err("past the table").to_string();
    assert!(error.contains("Z9"), "{error}");
    assert!(error.contains("the table is 3×2"), "{error}");
    assert!(doc.changed_streams().is_empty());
}

// -- text, and slides --------------------------------------------------------

/// Text is where Pages and Keynote live, and every text call takes a storage
/// identifier — a number the caller has to find first. The handle finds it
/// once.
#[test]
fn a_text_handle_edits_without_an_identifier_in_sight() {
    let mut doc = fixture!("pages-plain.pages");
    let before = {
        let body = doc.body_mut().unwrap();
        body.read().unwrap()
    };
    assert!(!before.is_empty());

    let mut body = doc.body_mut().unwrap();
    body.append("Ein neuer Absatz").unwrap();
    let after = body.read().unwrap();
    assert_eq!(after, format!("{before}\nEin neuer Absatz"));
    assert_eq!(body.paragraphs().unwrap().len(), 2);

    // The ordinary edits, all in UTF-16 code units as the format counts them.
    body.insert(0, "Vorn: ").unwrap();
    assert!(body.read().unwrap().starts_with("Vorn: "));
    body.delete(0..6).unwrap();
    assert_eq!(body.read().unwrap(), after);
    body.replace(0..4, "XXXX").unwrap();
    assert!(body.read().unwrap().starts_with("XXXX"));
    body.set("Alles neu").unwrap();
    assert_eq!(body.read().unwrap(), "Alles neu");

    assert!(doc.problems().is_empty(), "{:?}", doc.problems());
}

/// A document with no body says so rather than handing back nothing.
#[test]
fn only_a_pages_document_has_a_body() {
    let mut numbers = Document::new_spreadsheet("Blatt", "T", 2, 2).unwrap();
    let error = numbers.body_mut().err().expect("no body").to_string();
    assert!(error.contains("has no body text"), "{error}");
    assert!(Document::new(iwork::Kind::Pages)
        .unwrap()
        .body_mut()
        .is_ok());
    // And a storage that is not there is an error now rather than at the first
    // edit.
    assert!(numbers.text_mut(99_999).is_err());
}

/// A slide is addressed by its position in the deck or by an identifier, and
/// **a slide is not a page with a title slot**: a title is a placeholder the
/// *layout* defines, so a layout that defines none refuses the write by name
/// rather than inventing a text box and calling it a title.
#[test]
fn a_slide_is_what_its_layout_makes_it() {
    // A deck from nothing: one slide, a layout with no placeholders at all.
    let mut doc = Document::new(iwork::Kind::Keynote).unwrap();
    let mut slide = doc.slide_mut(0).unwrap();
    assert_eq!(slide.read().layout_name, "Title");
    let error = slide
        .title("Hallo")
        .expect_err("no placeholder")
        .to_string();
    assert!(error.contains("gives it no title"), "{error}");
    assert!(error.contains("a box and not a title"), "{error}");

    // What it *can* hold: drawables of its own, and a transition.
    let box_ = slide
        .add_text_box("Von nichts", (100.0, 120.0), (600.0, 120.0))
        .unwrap();
    assert!(slide.read().drawables.contains(&box_));
    slide.transition("dissolve", Some(1.5), None).unwrap();
    assert_eq!(slide.read().transition.effect, "apple:dissolve");
    assert_eq!(slide.read().transition.duration, 1.5);

    // Positions and identifiers name the same slide; past the end is an error.
    let identifier = slide.identifier();
    assert_eq!(
        doc.slide_mut(iwork::keynote::SlideRef::Identifier(identifier))
            .unwrap()
            .identifier(),
        identifier
    );
    let error = doc.slide_mut(9).err().expect("no slide 9").to_string();
    assert!(error.contains("the deck has 1"), "{error}");

    assert!(doc.problems().is_empty(), "{:?}", doc.problems());
}

/// A deck the app made has placeholders, and then the title and body *are*
/// writes — which is the half a from-nothing deck cannot show.
#[test]
fn a_slide_with_placeholders_takes_its_title_and_notes() {
    let mut doc = fixture!("keynote-deck.key");
    let mut slide = doc.slide_mut(1).unwrap();
    assert!(
        slide.read().title.is_some(),
        "the fixture's layouts have placeholders"
    );

    slide.title("Quartalsbericht").unwrap();
    assert_eq!(slide.read().title_text(), "Quartalsbericht");
    slide.notes("Zahlen sind vorläufig").unwrap();
    assert_eq!(slide.read().notes, "Zahlen sind vorläufig");
    slide.skip(true).unwrap();
    assert!(slide.read().skipped);
    slide.skip(false).unwrap();

    let moved = slide.move_to(0).unwrap();
    assert_eq!(moved, 0);
    assert_eq!(doc.slides()[0].title_text(), "Quartalsbericht");
    assert!(doc.problems().is_empty(), "{:?}", doc.problems());
}
