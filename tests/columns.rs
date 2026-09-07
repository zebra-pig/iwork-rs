//! Inserting a column into a table — the byte level, then the app.
//!
//! **A column is not a row turned sideways.** A row is a `TileRowInfo` of its
//! own, and inserting one shifts whole objects; a column exists only as *one
//! entry in every row's offset array*, so inserting one rewrites every row of
//! every tile — slice the row into its per-column records, open a gap, lay them
//! back out, and every offset after the gap moves. That is also why this is not
//! limited to a single tile the way the row insert is: the work is per row, and
//! a tile boundary is a row boundary.
//!
//! The refusals are the row insert's, read along the other axis: a categorised,
//! pivoted or filtered table, conditional highlighting, hidden columns, a merge
//! at or straddling the insertion, and any formula whose reference to the table
//! names a column at or after it. A refused insert leaves the document byte for
//! byte as it was, which is what several of these assert.

use std::path::{Path, PathBuf};

use iwork::table::CellValue;
use iwork::Document;

fn generated(name: &str) -> Option<PathBuf> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/generated")
        .join(name);
    path.exists().then_some(path)
}

fn open(name: &str) -> Option<Document> {
    let path = generated(name)?;
    Some(Document::open(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display())))
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

/// The supported case: a plain 17×3 table. A column goes in at index 1; the
/// table grows by one, the new column is empty, and every cell to the right of
/// it keeps its value *and* its data format.
#[test]
fn inserting_a_column_into_a_plain_table() {
    fixture!("numbers-formats.numbers");
    let mut doc = open("numbers-formats.numbers").unwrap();

    let before = doc.table("Formate").unwrap();
    assert_eq!(before.columns, 3);
    let format_of = |table: &iwork::table::Table, row: usize, column: usize| {
        table
            .cell(row, column)
            .map(|cell| cell.format.to_string())
            .unwrap_or_default()
    };
    let moved: Vec<(CellValue, String)> = (0..before.rows)
        .map(|r| (before.value(r, 1), format_of(&before, r, 1)))
        .collect();

    doc.insert_column("Formate", 1).unwrap();

    let after = doc.table("Formate").unwrap();
    assert_eq!(after.columns, 4);
    assert_eq!(after.rows, before.rows, "a column insert leaves rows alone");
    for (row, (value, format)) in moved.iter().enumerate() {
        assert_eq!(
            after.value(row, 1),
            CellValue::Empty,
            "row {row} of the new column is not empty"
        );
        assert_eq!(&after.value(row, 2), value, "row {row} lost its value");
        assert_eq!(
            &format_of(&after, row, 2),
            format,
            "row {row} lost its data format"
        );
    }
    assert!(doc.problems().is_empty(), "{:?}", doc.problems());
}

/// A column at the end, and then something written into it — which is the
/// whole point of making room.
#[test]
fn a_column_can_be_appended_and_then_filled() {
    fixture!("numbers-formats.numbers");
    let mut doc = open("numbers-formats.numbers").unwrap();
    let columns = doc.table("Formate").unwrap().columns;

    doc.insert_column("Formate", columns).unwrap();
    doc.set_cell("Formate", 0, columns, CellValue::Text("Neu".into()))
        .unwrap();
    doc.set_cell("Formate", 1, columns, CellValue::Text("Wert hier".into()))
        .unwrap();

    let table = doc.table("Formate").unwrap();
    assert_eq!(table.columns, columns + 1);
    assert_eq!(table.value(0, columns).to_text(), "Neu");
    assert_eq!(table.value(1, columns).to_text(), "Wert hier");
    assert!(doc.problems().is_empty(), "{:?}", doc.problems());
}

/// More than one tile is more of the same, not something new: the work is per
/// row and a tile boundary is a row boundary. The 301-row fixture has two.
#[test]
fn a_column_goes_into_a_table_of_more_than_one_tile() {
    fixture!("numbers-large.numbers");
    let mut doc = open("numbers-large.numbers").unwrap();
    let before = doc.table("Zeilen").unwrap();
    assert!(before.rows > 256, "the fixture is meant to span two tiles");
    let last = before.value(before.rows - 1, 0);
    let far = before.value(300, 3);

    doc.insert_column("Zeilen", 1).unwrap();

    let after = doc.table("Zeilen").unwrap();
    assert_eq!(after.columns, before.columns + 1);
    assert_eq!(after.value(after.rows - 1, 0), last, "the first column");
    assert_eq!(after.value(300, 4), far, "a row in the second tile");
    assert_eq!(after.value(300, 1), CellValue::Empty);
    assert!(doc.problems().is_empty(), "{:?}", doc.problems());
}

/// Past the end of the table is refused, and so is a table already as wide as
/// a row's offset array.
#[test]
fn a_column_past_the_end_is_refused() {
    fixture!("numbers-formats.numbers");
    let mut doc = open("numbers-formats.numbers").unwrap();
    let columns = doc.table("Formate").unwrap().columns;
    let refusal = doc
        .insert_column("Formate", columns + 1)
        .unwrap_err()
        .to_string();
    assert!(refusal.contains("goes in at 0..="), "{refusal}");
}

/// A merge is stored as an absolute formula nothing here rewrites, so one at or
/// straddling the insertion is refused by name — and the document is untouched.
#[test]
fn a_merge_at_the_insertion_is_refused() {
    fixture!("numbers-values.numbers");
    let mut doc = open("numbers-values.numbers").unwrap();
    let refusal = doc.insert_column("Zellarten", 1).unwrap_err().to_string();
    assert!(refusal.contains("merge"), "{refusal}");
    assert!(
        doc.changed_streams().is_empty(),
        "a refused insert touched {:?}",
        doc.changed_streams()
    );
}

/// A formula whose reference to the table would shift is refused: inserting a
/// column moves the cell out from under it, and rewriting a `TSCE` AST is a
/// phase of its own.
#[test]
fn a_formula_that_would_shift_is_refused() {
    fixture!("numbers-formulas.numbers");
    let mut doc = open("numbers-formulas.numbers").unwrap();
    let refusal = doc.insert_column("Zoo", 1).unwrap_err().to_string();
    assert!(refusal.contains("formula"), "{refusal}");
    assert!(refusal.contains("column"), "{refusal}");
}

/// Everything the row insert refuses along its axis, this one refuses along
/// the other — named, so the caller learns which.
#[test]
fn the_hard_tables_are_refused_by_name() {
    let cases: [(&str, &str, &str); 3] = [
        ("numbers-categories.numbers", "categorised", "categor"),
        ("numbers-pivot.numbers", "a pivot", "pivot"),
        ("numbers-rules.numbers", "highlighted", "highlighting"),
    ];
    for (file, what, needle) in cases {
        let Some(mut doc) = open(file) else {
            eprintln!("no {file} — skipping");
            continue;
        };
        let Some(table) = doc
            .tables()
            .into_iter()
            .find(|t| match needle {
                "categor" => !t.categories.is_empty(),
                "pivot" => t.pivot.is_some(),
                _ => !t.conditional_styles.is_empty(),
            })
            .map(|t| t.identifier.to_string())
        else {
            eprintln!("{file} has no {what} table — skipping");
            continue;
        };
        let refusal = doc.insert_column(&table, 1).unwrap_err().to_string();
        assert!(
            refusal.contains(needle),
            "{file}/{table} was refused for the wrong reason: {refusal}"
        );
    }
}

/// Ask Numbers to open the widened document and read it back.
///
/// Off unless `IWORK_APP_CHECK=1`. The comparison is against **the app's own
/// reading of the original**, not against this crate's — the app prints a
/// number in its own way (`1.2345678E+4` where `to_text` says `12345.678`), and
/// a test that compared the two would be testing formatting rather than the
/// insert. Both halves matter: the app sees one more column, and every cell to
/// the right of the insertion reads back what it read back before — which a
/// writer that damaged a row's offsets would fail, silently and in the middle.
#[test]
fn numbers_reads_back_an_inserted_column() {
    if std::env::var("IWORK_APP_CHECK").as_deref() != Ok("1") {
        eprintln!("IWORK_APP_CHECK is not 1 — skipping the app round trip");
        return;
    }
    let path = fixture!("numbers-formats.numbers");
    let ask = |document: &Path| -> (usize, std::collections::BTreeMap<String, String>) {
        let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("scripts/table-oracle.sh");
        let output = std::process::Command::new(&script)
            .arg(document)
            .output()
            .unwrap_or_else(|e| panic!("{}: {e}", script.display()));
        assert!(
            output.status.success(),
            "Numbers would not open {}:\n{}",
            document.display(),
            String::from_utf8_lossy(&output.stderr)
        );
        let text = String::from_utf8_lossy(&output.stdout).into_owned();
        let mut columns = 0usize;
        let mut said = std::collections::BTreeMap::new();
        let mut in_formate = false;
        for line in text.lines() {
            let field: Vec<&str> = line.split('\t').collect();
            match field.first() {
                Some(&"table") => {
                    in_formate = field.get(1) == Some(&"Formate");
                    if in_formate {
                        columns = field.get(3).and_then(|n| n.parse().ok()).unwrap_or(0);
                    }
                }
                Some(&"cell") if in_formate && field.len() >= 4 => {
                    said.insert(field[1].to_string(), field[3].to_string());
                }
                _ => {}
            }
        }
        (columns, said)
    };

    let (was_wide, before) = ask(&path);
    assert_eq!(was_wide, 3, "the fixture is meant to be three columns");

    let mut doc = open("numbers-formats.numbers").unwrap();
    let rows = doc.table("Formate").unwrap().rows;
    doc.insert_column("Formate", 1).unwrap();
    let out = std::env::temp_dir().join("iwork-insert-column.numbers");
    let _ = std::fs::remove_file(&out);
    doc.save(&out).unwrap();

    let (now_wide, after) = ask(&out);
    assert_eq!(now_wide, 4, "the app did not see one more column");
    for row in 1..=rows {
        assert_eq!(
            after.get(&format!("B{row}")).map(String::as_str),
            Some(""),
            "B{row} is not empty"
        );
        assert_eq!(
            after.get(&format!("C{row}")),
            before.get(&format!("B{row}")),
            "C{row} is not what the app read out of B{row} before"
        );
        assert_eq!(
            after.get(&format!("A{row}")),
            before.get(&format!("A{row}")),
            "A{row} moved, and nothing before the insertion should"
        );
    }
    let _ = std::fs::remove_file(&out);
}
