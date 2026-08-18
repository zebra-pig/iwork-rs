//! Inserting a row into a table — the byte level, then the app.
//!
//! The write is deliberately narrow: a plain rectangular table held in one
//! tile, no categories, filters, pivots, conditional highlighting, hidden or
//! collapsed rows, footer rows, merges at or below the insertion, or formulas
//! whose references would shift. Everything else is refused **by name**, and a
//! refused insert has to leave the document byte for byte as it was — which is
//! what most of this file asserts. The one supported case is then handed to
//! Numbers behind `IWORK_APP_CHECK=1`: one more row, the new row empty, and
//! every row below the insertion still holding its value and its format.

use std::path::{Path, PathBuf};

use iwork::table::{CellValue, Uuid};
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

/// The supported case: a plain 17×3 table with no formulas, merges, filters or
/// categories. A row goes in at index 8; the table grows by one, the new row is
/// empty, every row below keeps its value, and every table in the document still
/// adds up.
#[test]
fn inserting_a_row_into_a_plain_table() {
    fixture!("numbers-formats.numbers");
    let mut doc = open("numbers-formats.numbers").unwrap();

    let before = doc.table("Formate").unwrap();
    assert_eq!(before.rows, 17);
    let below: Vec<CellValue> = (8..17).map(|r| before.value(r, 0)).collect();

    doc.insert_row("Formate", 8).unwrap();

    let after = doc.table("Formate").unwrap();
    assert_eq!(after.rows, 18, "the table did not grow");
    // The new row is empty across every column.
    for column in 0..after.columns {
        assert_eq!(
            after.value(8, column),
            CellValue::Empty,
            "the inserted row is not empty at column {column}"
        );
    }
    // Everything above is where it was.
    assert_eq!(after.value(0, 0), CellValue::Text("Format".into()));
    assert_eq!(after.value(7, 0), CellValue::Text("Text".into()));
    // Everything below moved down one and kept its value.
    for (offset, value) in below.into_iter().enumerate() {
        assert_eq!(
            after.value(9 + offset, 0),
            value,
            "the row that was at {} did not move to {}",
            8 + offset,
            9 + offset
        );
    }
    // The document is still internally consistent — every count adds up.
    assert!(doc.problems().is_empty(), "{:?}", doc.problems());
    for table in doc.tables() {
        assert!(
            table.audit().is_empty(),
            "{}: {:?}",
            table.name,
            table.audit()
        );
    }
}

/// The write touches only the model, the tile and the row bucket streams (the
/// UUID map shares the model's stream), and leaves every other entry
/// byte-identical.
#[test]
fn inserting_a_row_rewrites_only_the_streams_it_must() {
    fixture!("numbers-formats.numbers");
    let mut doc = open("numbers-formats.numbers").unwrap();
    doc.insert_row("Formate", 8).unwrap();

    let changed = doc.changed_streams();
    assert_eq!(changed.len(), 3, "{changed:?}");
    assert!(changed.iter().any(|s| s.contains("CalculationEngine")));
    assert!(changed
        .iter()
        .any(|s| s.contains("HeaderStorageBucket-904914")));
    assert!(changed.iter().any(|s| s.ends_with("Tile.iwa")));

    // Save it out and confirm every untouched entry is the original's bytes.
    let out = std::env::temp_dir().join("iwork-insert-row-streams.numbers");
    let _ = std::fs::remove_file(&out);
    let changed: Vec<String> = changed.iter().map(|s| s.to_string()).collect();
    doc.save(&out).unwrap();
    let before = iwork::Package::read(generated("numbers-formats.numbers").unwrap()).unwrap();
    let after = iwork::Package::read(&out).unwrap();
    for entry in before.names() {
        if changed.iter().any(|c| c == entry) {
            continue;
        }
        assert_eq!(before.get(entry), after.get(entry), "{entry} was rewritten");
    }
    let _ = std::fs::remove_file(&out);
}

/// The row UUID map gains one entry, stays a clean permutation, and stays sorted
/// by UUID — the invariant a positional read would have missed.
#[test]
fn the_uuid_map_gains_a_sorted_unique_entry() {
    fixture!("numbers-formats.numbers");
    let mut doc = open("numbers-formats.numbers").unwrap();
    let model = doc.table("Formate").unwrap().model;
    let uid_map = doc
        .archive(model)
        .unwrap()
        .bytes(46)
        .and_then(|raw| iwork::pb::decode_nested(raw).and_then(|m| m.varint(1)))
        .expect("the model names a ColumnRowUIDMap");

    doc.insert_row("Formate", 8).unwrap();

    // Every row 0..18 has exactly one UUID, and every UUID one index.
    let table = doc.table("Formate").unwrap();
    let mut uuids = std::collections::BTreeSet::new();
    for row in 0..table.rows {
        let uuid = table
            .uids
            .row_uid(row)
            .unwrap_or_else(|| panic!("row {row} has no UUID after the insert"));
        assert!(
            uuids.insert(uuid),
            "row {row}'s UUID collides with another's"
        );
        assert_eq!(
            table.uids.row(uuid),
            Some(row),
            "the map is not a bijection"
        );
    }
    assert_eq!(uuids.len(), 18);

    // Field 4 of the archive is sorted by the 128-bit value, high half first.
    let archive = doc.archive(uid_map).unwrap();
    let sorted: Vec<Uuid> = archive
        .all(4)
        .filter_map(|value| match value {
            iwork::pb::Value::Bytes(raw) => iwork::pb::decode_nested(raw).map(|m| Uuid::decode(&m)),
            _ => None,
        })
        .collect();
    assert_eq!(sorted.len(), 18);
    let mut in_order = sorted.clone();
    in_order.sort_by_key(|uuid| (uuid.upper, uuid.lower));
    assert_eq!(sorted, in_order, "the row UUID list is not sorted by UUID");
    // Fields 5 and 6 are inverse permutations of one another.
    let index_for_uid: Vec<u64> = archive
        .all(5)
        .filter_map(|v| match v {
            iwork::pb::Value::Varint(n) => Some(*n),
            _ => None,
        })
        .collect();
    let uid_for_index: Vec<u64> = archive
        .all(6)
        .filter_map(|v| match v {
            iwork::pb::Value::Varint(n) => Some(*n),
            _ => None,
        })
        .collect();
    assert_eq!(index_for_uid.len(), 18);
    assert_eq!(uid_for_index.len(), 18);
    for (position, &index) in index_for_uid.iter().enumerate() {
        assert_eq!(
            uid_for_index[index as usize] as usize, position,
            "field 5 and field 6 disagree"
        );
    }
}

/// Appending — `at == rows` — is allowed and grows the table at the end.
#[test]
fn a_row_can_be_appended() {
    fixture!("numbers-formats.numbers");
    let mut doc = open("numbers-formats.numbers").unwrap();
    doc.insert_row("Formate", 17).unwrap();
    let table = doc.table("Formate").unwrap();
    assert_eq!(table.rows, 18);
    assert_eq!(table.value(17, 0), CellValue::Empty);
    assert_eq!(table.value(16, 0), CellValue::Text("Datum".into()));
    assert!(table.audit().is_empty(), "{:?}", table.audit());
}

/// The inserted row is genuinely empty — it has no `TileRowInfo` at all, the
/// same shape the app gives a row with no cells. Filling it therefore needs the
/// "first cell in a row" path `set_cell` does not implement yet (Phase 2's own
/// documented boundary), so writing into it is refused rather than silently
/// half-done. Inserting the row and filling one that already has cells compose;
/// filling the brand-new one is left for the write that grows a row.
#[test]
fn filling_the_inserted_row_is_refused_until_first_cell_writes_land() {
    fixture!("numbers-formats.numbers");
    let mut doc = open("numbers-formats.numbers").unwrap();
    doc.insert_row("Formate", 8).unwrap();
    let err = doc
        .set_cell("Formate", 8, 0, CellValue::Text("Neu".into()))
        .expect_err("the inserted row has no TileRowInfo to write into")
        .to_string();
    assert!(err.contains("no stored cells"), "{err}");
    // The refused write left the inserted (empty) row and its neighbours intact.
    let table = doc.table("Formate").unwrap();
    assert_eq!(table.value(8, 0), CellValue::Empty);
    assert_eq!(table.value(9, 0), CellValue::Text("Zahlensystem".into()));
    assert!(table.audit().is_empty(), "{:?}", table.audit());
}

/// Everything the writer will not do, refused by name, and each refusal leaves
/// the document byte for byte as it was.
#[test]
fn insert_row_refuses_what_it_cannot_verify() {
    // Out of bounds.
    if let Some(mut doc) = open("numbers-formats.numbers") {
        let err = doc
            .insert_row("Formate", 99)
            .expect_err("past the end")
            .to_string();
        assert!(err.contains("goes in at 0..=17"), "{err}");
        assert!(doc.changed_streams().is_empty());
    }

    // A merge at or below the insertion.
    if let Some(mut doc) = open("numbers-formats.numbers") {
        let err = doc
            .insert_row("Verbunden", 0)
            .expect_err("a merge would shift")
            .to_string();
        assert!(err.contains("merge"), "{err}");
        assert!(doc.changed_streams().is_empty());
    }

    // A bounded formula whose range the insertion would cross.
    if let Some(mut doc) = open("numbers-large.numbers") {
        let err = doc
            .insert_row("Zeilen", 5)
            .expect_err("a formula range crosses row 5")
            .to_string();
        assert!(err.contains("formula"), "{err}");
        assert!(doc.changed_streams().is_empty());
    }

    // A categorised table.
    if let Some(mut doc) = open("numbers-categories.numbers") {
        let table = doc.tables().into_iter().next().map(|t| t.identifier);
        if let Some(id) = table {
            let err = doc
                .insert_row(&id.to_string(), 1)
                .expect_err("categorised")
                .to_string();
            assert!(err.contains("categorised"), "{err}");
            assert!(doc.changed_streams().is_empty());
        }
    }

    // A footer row.
    if let Some(mut doc) = open("numbers-sorted.numbers") {
        let err = doc
            .insert_row("Reading Log", 2)
            .expect_err("footer row")
            .to_string();
        assert!(err.contains("footer"), "{err}");
        assert!(doc.changed_streams().is_empty());
    }
}

/// A safe formula stays computable: inserting a row into `Zweite Tabelle` above
/// its `SUM(B2:B3)` host, but with both range ends on the host's side of the
/// insertion, is allowed and keeps the table consistent.
#[test]
fn a_relative_reference_that_shifts_with_its_host_is_allowed() {
    fixture!("numbers-values.numbers");
    let mut doc = open("numbers-values.numbers").unwrap();
    // Row 1 is above B4 (row 3) and its range B2:B3 (rows 1,2). Inserting there
    // shifts host and range together, so the reference stays correct.
    doc.insert_row("Zweite Tabelle", 1).unwrap();
    let table = doc.table("Zweite Tabelle").unwrap();
    assert_eq!(table.rows, 5);
    assert_eq!(table.value(1, 0), CellValue::Empty);
    assert!(table.audit().is_empty(), "{:?}", table.audit());
    assert!(doc.problems().is_empty(), "{:?}", doc.problems());
}

// -- the oracle --------------------------------------------------------------

/// Ask Numbers to open the inserted-row document and read it back.
///
/// Off unless `IWORK_APP_CHECK=1`. The two halves matter equally: the table
/// reports one more row with the new one empty, and every row below the
/// insertion reads back the value it had before — which a writer that damaged
/// the tile's row indices or the row bucket would fail.
#[test]
fn numbers_reads_back_an_inserted_row() {
    if std::env::var("IWORK_APP_CHECK").as_deref() != Ok("1") {
        eprintln!("IWORK_APP_CHECK is not 1 — skipping the app round trip");
        return;
    }
    let _ = fixture!("numbers-formats.numbers");
    let mut doc = open("numbers-formats.numbers").unwrap();
    let before: Vec<String> = (0..17)
        .map(|r| doc.table("Formate").unwrap().value(r, 0).to_text())
        .collect();
    doc.insert_row("Formate", 8).unwrap();

    let out = std::env::temp_dir().join("iwork-insert-row.numbers");
    let _ = std::fs::remove_file(&out);
    doc.save(&out).unwrap();

    let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("scripts/table-oracle.sh");
    let output = std::process::Command::new(&script)
        .arg(&out)
        .output()
        .unwrap_or_else(|e| panic!("{}: {e}", script.display()));
    assert!(
        output.status.success(),
        "Numbers would not open the document with an inserted row:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let text = String::from_utf8_lossy(&output.stdout).into_owned();

    // Read the Formate table's row count and its column-A values back.
    let mut rows = 0usize;
    let mut said: std::collections::BTreeMap<String, String> = std::collections::BTreeMap::new();
    let mut in_formate = false;
    for line in text.lines() {
        let field: Vec<&str> = line.split('\t').collect();
        match field.first() {
            Some(&"table") => {
                in_formate = field.get(1) == Some(&"Formate");
                if in_formate {
                    rows = field.get(2).and_then(|n| n.parse().ok()).unwrap_or(0);
                }
            }
            Some(&"cell") if in_formate && field.len() >= 4 => {
                said.insert(field[1].to_string(), field[3].to_string());
            }
            _ => {}
        }
    }

    assert_eq!(rows, 18, "the app did not see one more row");
    // The inserted row is empty.
    assert_eq!(said.get("A9").map(String::as_str), Some(""), "A9 not empty");
    // A1..A8 are unchanged; A9 is new; A10..A18 are the old A9..A17.
    for (index, value) in before.iter().enumerate() {
        let name = format!("A{}", if index < 8 { index + 1 } else { index + 2 });
        assert_eq!(
            said.get(&name).map(String::as_str),
            Some(value.as_str()),
            "{name} did not read back as {value:?}"
        );
    }
    let _ = std::fs::remove_file(&out);
}
