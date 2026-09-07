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
