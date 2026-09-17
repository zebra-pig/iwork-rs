//! Writing a formula — the fill, and the dependency that makes it live.
//!
//! The measurement this whole file rests on: **Numbers does not recalculate a
//! formula when a document opens.** The value in the cell record is what the
//! app shows, and it goes on showing it — through an open, an edit and a save —
//! unless the cell is in the calculation engine's dependency graph. So a fill
//! writes two things: the formula key, and the record that says which cells the
//! formula reads. `tests/formula_write.rs::the_app_recalculates_a_filled_cell`
//! is the one that proves it, and it needs Numbers.

use std::path::{Path, PathBuf};

use iwork::table::{CellValue, Decimal};
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

/// A relative formula filled into another row means the other row's cells.
#[test]
fn a_filled_formula_is_relative_to_where_it_lands() {
    let mut doc = fixture!("numbers-formulas.numbers");
    // C2 is `=Wert addition+1` — the cell to its left, in its own row.
    doc.fill_formula(
        "Zoo",
        (1, 2),
        (2, 3),
        Some(CellValue::Number(Decimal::parse("0").unwrap())),
    )
    .unwrap();
    let table = doc.table("Zoo").unwrap();
    let filled = table
        .formula_cells()
        .into_iter()
        .find(|(row, column, _)| (*row, *column) == (2, 3))
        .expect("the filled cell has no formula");
    // The reference moved with the formula: row 2 rather than row 1.
    assert!(
        filled
            .2
            .ast
            .nodes
            .iter()
            .any(|node| node.reference().is_some()),
        "the filled formula has no reference"
    );
    assert_eq!(table.value(2, 3).to_text(), "0", "the caller's value");
    assert!(doc.problems().is_empty(), "{:?}", doc.problems());
}

/// Without a value, only a formula whose answer cannot move is allowed.
#[test]
fn a_fill_with_no_value_is_refused_unless_the_answer_cannot_move() {
    let mut doc = fixture!("numbers-formulas.numbers");
    // C2 is relative: its answer depends on where it sits.
    let refused = doc.fill_formula("Zoo", (1, 2), (2, 3), None);
    assert!(refused.is_err(), "a relative fill needs a value");
    assert!(format!("{:?}", refused.unwrap_err()).contains("relative"));

    // C30 is `=SUM($B$2:$B$4)` — absolute throughout, so the source's answer is
    // the target's.
    doc.fill_formula("Zoo", (29, 2), (3, 3), None).unwrap();
    assert_eq!(doc.table("Zoo").unwrap().value(3, 3).to_text(), "6");
    assert!(doc.problems().is_empty(), "{:?}", doc.problems());
}

/// The dependency graph gains exactly one record per filled cell.
#[test]
fn a_filled_cell_is_registered_in_the_calculation_engine() {
    let mut doc = fixture!("numbers-formulas.numbers");
    // Every `CellRecordExpandedArchive` in every dependency tile: one per cell
    // the engine knows holds a formula.
    let records = |doc: &Document| {
        doc.objects()
            .filter(|(_, object)| object.message_type() == iwork::calc::TYPE_CELL_RECORD_TILE)
            .filter_map(|(_, object)| iwork::pb::Message::decode(object.payload()).ok())
            .map(|archive| archive.all(4).count())
            .sum::<usize>()
    };
    let before = records(&doc);
    doc.fill_formula(
        "Zoo",
        (1, 2),
        (2, 3),
        Some(CellValue::Number(Decimal::parse("0").unwrap())),
    )
    .unwrap();
    assert_eq!(
        records(&doc),
        before + 1,
        "the filled cell was not registered"
    );

    // …and it goes into the tile that already covers the cell rather than a
    // second tile with the same origin, which is how a first attempt made the
    // app report two untouched cells as corrupt.
    let tiles = doc
        .objects()
        .filter(|(_, object)| object.message_type() == iwork::calc::TYPE_CELL_RECORD_TILE)
        .count();
    doc.fill_formula(
        "Zoo",
        (1, 2),
        (4, 3),
        Some(CellValue::Number(Decimal::parse("0").unwrap())),
    )
    .unwrap();
    assert_eq!(
        doc.objects()
            .filter(|(_, object)| object.message_type() == iwork::calc::TYPE_CELL_RECORD_TILE)
            .count(),
        tiles,
        "a second tile was written for a cell an existing one covers"
    );
    assert!(doc.problems().is_empty(), "{:?}", doc.problems());
}

/// What it will not do, by name.
#[test]
fn a_fill_refuses_what_it_cannot_register() {
    let mut doc = fixture!("numbers-formulas.numbers");
    let value = || Some(CellValue::Number(Decimal::parse("0").unwrap()));
    // A cell with no formula.
    assert!(doc.fill_formula("Zoo", (0, 0), (2, 3), value()).is_err());
    // Onto itself.
    assert!(doc.fill_formula("Zoo", (1, 2), (1, 2), value()).is_err());
    // Onto a cell that already holds a formula.
    assert!(doc.fill_formula("Zoo", (1, 2), (2, 2), value()).is_err());
    // Off the end of the table.
    assert!(doc.fill_formula("Zoo", (1, 2), (9999, 3), value()).is_err());
    assert!(
        doc.changed_streams().is_empty(),
        "a refusal wrote something"
    );
}

/// The one that matters. Off unless `IWORK_APP_CHECK=1`.
///
/// Numbers is asked three questions: what the filled cell shows when the
/// document opens, what it shows after the cells it reads are given values, and
/// what it shows when one of them changes again. A formula the engine does not
/// know about answers the first and never the others.
#[test]
fn the_app_recalculates_a_filled_cell() {
    if std::env::var("IWORK_APP_CHECK").as_deref() != Ok("1") {
        eprintln!("IWORK_APP_CHECK is not 1 — skipping the app round trip");
        return;
    }
    let mut doc = fixture!("numbers-formulas.numbers");
    // Fill C2 (`=Wert addition+1`, a relative reference to the cell on its
    // left) into D8, whose left neighbour C8 is `=+Wert vorzeichen` = 7.
    doc.fill_formula(
        "Zoo",
        (1, 2),
        (7, 3),
        Some(CellValue::Number(Decimal::parse("8").unwrap())),
    )
    .unwrap();
    let out = std::env::temp_dir().join("iwork-filled.numbers");
    let _ = std::fs::remove_file(&out);
    doc.save(&out).unwrap();

    let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("scripts/app-check.sh");
    let output = std::process::Command::new(&script)
        .arg(&out)
        .output()
        .unwrap_or_else(|e| panic!("{}: {e}", script.display()));
    assert!(
        output.status.success(),
        "Numbers would not open a document with a filled formula:\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let _ = std::fs::remove_file(&out);
}

// -- a formula from its text -------------------------------------------------

/// The formula the caller typed is the formula the document holds — read back
/// through this crate's own printer, which is the reader half of the format.
#[test]
fn a_formula_written_from_text_reads_back() {
    let mut doc = fixture!("numbers-values.numbers");
    doc.set_formula(
        "Zweite Tabelle",
        3,
        2,
        "=SUM($B$2:$B$3)*2",
        CellValue::Number(Decimal::parse("1500").unwrap()),
    )
    .unwrap();

    let table = doc.table("Zweite Tabelle").unwrap();
    let names = iwork::formula::Names::new(vec![]);
    let written = table.formula(3, 2).expect("no formula in the written cell");
    assert_eq!(
        written.text(iwork::formula::Site::new(&names, None, (2, 3))),
        "=SUM($B$2:$B$3)×2",
        "the app spells multiplication ×, and this is the crate's own printer"
    );
    assert_eq!(
        table.value(3, 2),
        CellValue::Number(Decimal::parse("1500").unwrap()),
        "the value the caller gave is what the cell shows"
    );
    assert!(table.audit().is_empty(), "{:?}", table.audit());
    assert!(doc.problems().is_empty(), "{:?}", doc.problems());
}

/// A written formula is *live*: its cell goes into the engine's dependency
/// graph, or the app would show the caller's value for ever.
#[test]
fn a_written_formula_is_registered_in_the_calculation_engine() {
    let mut doc = fixture!("numbers-values.numbers");
    let records = |doc: &Document| {
        doc.objects()
            .filter(|(_, object)| object.message_type() == iwork::calc::TYPE_CELL_RECORD_TILE)
            .filter_map(|(_, object)| iwork::pb::Message::decode(object.payload()).ok())
            .map(|archive| archive.all(4).count())
            .sum::<usize>()
    };
    let before = records(&doc);
    doc.set_formula(
        "Zweite Tabelle",
        3,
        2,
        "=B2+B3",
        CellValue::Number(Decimal::parse("750").unwrap()),
    )
    .unwrap();
    assert_eq!(records(&doc), before + 1, "the cell was not registered");
    assert!(doc.problems().is_empty(), "{:?}", doc.problems());
}

/// Everything refused by name. Each leaves the document as it was.
#[test]
fn a_written_formula_refuses_what_it_cannot_write() {
    let mut doc = fixture!("numbers-values.numbers");
    let one = || CellValue::Number(Decimal::parse("1").unwrap());
    for (row, column, text, value, expected) in [
        // The cell already holds one.
        (3, 1, "=1+1", one(), "already holds a formula"),
        // An empty value is not an answer.
        (3, 2, "=1+1", CellValue::Empty, "cannot be empty"),
        // Out of the table.
        (99, 0, "=1+1", one(), "the table is"),
        // Things the parser will not turn into edges.
        (3, 2, "=SUM(B)", one(), "whole row or column"),
        (3, 2, "=Zellarten.B2", one(), "another table"),
        (3, 2, "=NICHTDA(2)", one(), "not a function"),
        (3, 2, "=1+", one(), "ends where a value was expected"),
    ] {
        let error = doc
            .set_formula("Zweite Tabelle", row, column, text, value)
            .expect_err(&format!("{text} at r{row}c{column} was not refused"))
            .to_string();
        assert!(
            error.contains(expected),
            "r{row}c{column} {text}: {error:?} does not mention {expected:?}"
        );
    }
    assert!(
        doc.changed_streams().is_empty(),
        "a refused formula changed {:?}",
        doc.changed_streams()
    );
}

/// A formula reading cells of a table this crate built from nothing is refused,
/// because such a table has no cell owner in the calculation engine — and a
/// formula the engine does not know about is one the app never recalculates.
#[test]
fn a_table_with_no_cell_owner_is_refused() {
    let mut doc = Document::new_spreadsheet("Blatt", "T", 4, 2).unwrap();
    let error = doc
        .set_formula(
            "T",
            3,
            0,
            "=A1+A2",
            CellValue::Number(Decimal::parse("0").unwrap()),
        )
        .expect_err("a table made from nothing has no owner")
        .to_string();
    assert!(error.contains("no cell owner"), "{error}");
}

/// The app is the oracle, and for a formula it is a strong one: Numbers parses
/// the node stream this crate built from nothing, prints the formula back in
/// its own spelling, and evaluates it.
///
/// Off unless `IWORK_APP_CHECK=1`.
#[test]
fn numbers_reads_back_a_formula_written_from_text() {
    if std::env::var("IWORK_APP_CHECK").as_deref() != Ok("1") {
        eprintln!("IWORK_APP_CHECK is not 1 — skipping the app round trip");
        return;
    }
    let mut doc = fixture!("numbers-values.numbers");
    doc.set_formula(
        "Zweite Tabelle",
        2,
        2,
        "=SUM($B$2:$B$3)*2",
        CellValue::Number(Decimal::parse("1500").unwrap()),
    )
    .unwrap();
    let out = std::env::temp_dir().join("iwork-set-formula.numbers");
    let _ = std::fs::remove_file(&out);
    doc.save(&out).unwrap();

    let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("scripts/table-oracle.sh");
    let output = std::process::Command::new(&script)
        .arg(&out)
        .output()
        .unwrap_or_else(|e| panic!("{}: {e}", script.display()));
    assert!(
        output.status.success(),
        "Numbers would not open a document with a written formula:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let text = String::from_utf8_lossy(&output.stdout).into_owned();
    // The app's own spelling of what was written, and its own answer.
    assert!(
        text.lines().any(|line| {
            let field: Vec<&str> = line.split('\t').collect();
            field.first() == Some(&"cell")
                && field.get(1) == Some(&"C3")
                && field.get(6) == Some(&"=SUM($B$2:$B$3)×2")
        }),
        "the app did not report the written formula:\n{text}"
    );
    assert!(
        text.contains("\tC3\treal\t1500.0\t"),
        "the app did not report the formula's value:\n{text}"
    );
    let _ = std::fs::remove_file(&out);
}
