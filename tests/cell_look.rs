//! The look of one cell: its own fill, and its own text.
//!
//! A cell does not carry a colour or a weight. It carries a key into its
//! table's style list (`DataStore.styleTable`), and the entry names a style —
//! for a cell that differs from its area, a **variation**: an archive that
//! names the area's style as its parent, says it is a variation, and holds
//! only what differs. Both shapes are read off tables Numbers wrote:
//!
//! ```text
//! TST.CellStyleArchive            TSWP.ParagraphStyleArchive
//! { 1: { 3: -> parent, 4: 1,      { 1: { 3: -> parent, 4: 1,
//!        5: -> stylesheet },             5: -> stylesheet },
//!   10: 1,                          10: 2,
//!   11: { 8: 1 } }                  11: { 3: 15.0 }, 12: { 43: 1 } }
//! ```
//!
//! and so is the rule that ties the list to the cells: an entry's count is the
//! number of cells naming it, whichever of the two keys they name it with —
//! kept by all 37 tables in the corpus, and by `Table::audit` now.
//!
//! What these tests cannot say is whether Numbers *draws* it. That is
//! `numbers_draws_the_look_of_one_cell`, gated behind `IWORK_APP_CHECK=1`.

// These exercise the 0.2 calls, which 0.3 keeps behind `#[deprecated]`;
// `tests/elements.rs` is the same ground through the 0.3 API.
#![allow(deprecated)]

use std::path::{Path, PathBuf};

use iwork::drawable::Color;
use iwork::pb::{Message, Value};
use iwork::table::{CellText, CellValue};
use iwork::Document;

const NAVY: Color = Color {
    red: 0.12,
    green: 0.22,
    blue: 0.38,
    alpha: 1.0,
};
const SAND: Color = Color {
    red: 0.98,
    green: 0.92,
    blue: 0.80,
    alpha: 1.0,
};

fn generated(name: &str) -> Option<PathBuf> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/generated")
        .join(name);
    path.exists().then_some(path)
}

fn fresh() -> Document {
    // One header row, one header column, as a table from nothing has.
    Document::new_spreadsheet("Blatt", "Tabelle", 5, 4).unwrap()
}

fn table(doc: &Document) -> iwork::table::TableInfo {
    doc.table("Tabelle").expect("the table")
}

/// The style a cell names for itself, as `(key, style object)`.
fn cell_style_of(doc: &Document, row: usize, column: usize) -> Option<(u32, u64)> {
    let table = table(doc);
    let key = table.cell(row, column)?.record.cell_style_id?;
    let style = table.side_tables().styles.entries.get(&key)?.reference?;
    Some((key, style))
}

fn text_style_of(doc: &Document, row: usize, column: usize) -> Option<(u32, u64)> {
    let table = table(doc);
    let key = table.cell(row, column)?.record.text_style_id?;
    let style = table.side_tables().styles.entries.get(&key)?.reference?;
    Some((key, style))
}

fn bag(archive: &Message, number: u32) -> Message {
    match archive.get(number) {
        Some(Value::Bytes(raw)) => Message::decode(raw).unwrap(),
        _ => Message::default(),
    }
}

fn clean(doc: &Document) {
    assert!(doc.problems().is_empty(), "{:?}", doc.problems());
    for table in doc.tables() {
        assert!(table.audit().is_empty(), "{:?}", table.audit());
    }
}

/// A painted cell names a variation of its area's style, in the shape the app
/// writes one, and the list counts the cells that name it.
#[test]
fn a_painted_cell_names_a_variation_of_its_area_s_style() {
    let mut doc = fresh();
    doc.table_mut("Tabelle").unwrap().set("B2", 1240).unwrap();
    let before = table(&doc).side_tables().styles.entries.len();

    // B2 and C2: two body cells, one of them empty.
    let painted = doc
        .table_mut("Tabelle")
        .unwrap()
        .fill("B2:C2", Some(NAVY))
        .unwrap();
    assert_eq!(painted, 2);
    clean(&doc);

    let (key, style) = cell_style_of(&doc, 1, 1).expect("B2 names a style");
    assert_eq!(
        cell_style_of(&doc, 1, 2),
        Some((key, style)),
        "two cells painted alike share one variation and one entry"
    );
    let t = table(&doc);
    assert_eq!(t.side_tables().styles.entries.len(), before + 1);
    assert_eq!(
        t.side_tables().styles.entries[&key].refcount,
        2,
        "the entry counts the cells that name it"
    );

    // The shape: { 1: {3: parent, 4: 1, 5: stylesheet}, 10: 1, 11: {1: fill} }.
    let archive = doc.archive(style).unwrap();
    assert_eq!(
        archive.fields.iter().map(|f| f.number).collect::<Vec<_>>(),
        vec![1, 10, 11]
    );
    let model = doc.archive(t.model).unwrap();
    let body = iwork::style::reference_at(&model, &[18, 1]).expect("body_cell_style");
    assert_eq!(
        iwork::style::reference_at(&archive, &[1, 3, 1]),
        Some(body),
        "its parent is the body's cell style"
    );
    assert_eq!(
        iwork::style::get_path(&archive, iwork::style::IS_VARIATION),
        Some(Value::Varint(1))
    );
    assert!(iwork::style::reference_at(&archive, &[1, 5, 1]).is_some());
    assert_eq!(archive.varint(10), Some(1), "one property differs");
    let properties = bag(&archive, 11);
    assert_eq!(
        properties
            .fields
            .iter()
            .map(|f| f.number)
            .collect::<Vec<_>>(),
        vec![1],
        "and it is the fill"
    );
    match iwork::style::get_path(&archive, &[11, 1, 1, 3]) {
        Some(Value::Fixed32(bytes)) => assert!((f32::from_le_bytes(bytes) - 0.12).abs() < 1e-6),
        other => panic!("no red channel: {other:?}"),
    }

    // The empty one has a bare record: no value, the style key and nothing else.
    let empty = t.cell(1, 2).expect("C2 has a record now");
    assert_eq!(empty.value, CellValue::Empty);
    assert_eq!(empty.record.cell_type, 0);
    // And the value B2 had is untouched.
    assert_eq!(t.value(1, 1).to_text(), "1240");
}

/// A header cell's variation hangs off the header's style, not the body's —
/// so one colour across a header and a body is two variations.
#[test]
fn a_range_across_two_areas_makes_a_variation_for_each() {
    let mut doc = fresh();
    doc.table_mut("Tabelle")
        .unwrap()
        .fill("B1:B2", Some(NAVY))
        .unwrap();
    clean(&doc);

    let (header_key, header_style) = cell_style_of(&doc, 0, 1).unwrap();
    let (body_key, body_style) = cell_style_of(&doc, 1, 1).unwrap();
    assert_ne!(header_key, body_key);
    let model = doc.archive(table(&doc).model).unwrap();
    let parent = |style: u64| iwork::style::reference_at(&doc.archive(style).unwrap(), &[1, 3, 1]);
    assert_eq!(
        parent(header_style),
        iwork::style::reference_at(&model, &[19, 1])
    );
    assert_eq!(
        parent(body_style),
        iwork::style::reference_at(&model, &[18, 1])
    );
}

/// The same colour, asked for later, takes another reference to the entry that
/// is already there; a different one gets its own.
#[test]
fn a_look_already_in_the_list_is_reused() {
    let mut doc = fresh();
    doc.table_mut("Tabelle")
        .unwrap()
        .fill("B2", Some(NAVY))
        .unwrap();
    let (key, style) = cell_style_of(&doc, 1, 1).unwrap();

    doc.table_mut("Tabelle")
        .unwrap()
        .fill("C3:D3", Some(NAVY))
        .unwrap();
    assert_eq!(cell_style_of(&doc, 2, 2), Some((key, style)));
    assert_eq!(table(&doc).side_tables().styles.entries[&key].refcount, 3);

    doc.table_mut("Tabelle")
        .unwrap()
        .fill("B3", Some(SAND))
        .unwrap();
    let (other, _) = cell_style_of(&doc, 2, 1).unwrap();
    assert_ne!(other, key);
    clean(&doc);

    // Painting a cell the colour it already is changes nothing.
    assert_eq!(
        doc.table_mut("Tabelle")
            .unwrap()
            .fill("B2", Some(NAVY))
            .unwrap(),
        0
    );
}

/// `None` gives the reference back, and an empty cell that names nothing is
/// no cell: its record goes, and the entry with it when nobody is left.
#[test]
fn unpainting_gives_the_reference_back() {
    let mut doc = fresh();
    doc.table_mut("Tabelle")
        .unwrap()
        .set("B2", "Zürich")
        .unwrap();
    let cells_before = table(&doc).cells().len();
    let entries_before = table(&doc).side_tables().styles.entries.len();

    doc.table_mut("Tabelle")
        .unwrap()
        .fill("B2:C2", Some(NAVY))
        .unwrap();
    assert_eq!(
        table(&doc).cells().len(),
        cells_before + 1,
        "C2 got a record"
    );

    doc.table_mut("Tabelle")
        .unwrap()
        .fill("B2:C2", None)
        .unwrap();
    let t = table(&doc);
    assert_eq!(t.cells().len(), cells_before, "and lost it again");
    assert_eq!(t.side_tables().styles.entries.len(), entries_before);
    assert_eq!(t.cell(1, 1).unwrap().record.cell_style_id, None);
    assert_eq!(t.value(1, 1).to_text(), "Zürich");
    clean(&doc);
}

/// A value written into a painted cell keeps the paint, and **emptying it does
/// too** — the record is not deleted, because a cell with a look and no value
/// is a cell. Deleting it took the colour with the value and left the list
/// counting a reference nobody held.
#[test]
fn a_painted_cell_keeps_its_paint_when_its_value_changes_or_goes() {
    let mut doc = fresh();
    doc.table_mut("Tabelle")
        .unwrap()
        .fill("B2", Some(NAVY))
        .unwrap();
    let named = cell_style_of(&doc, 1, 1).unwrap();

    doc.table_mut("Tabelle").unwrap().set("B2", 42).unwrap();
    assert_eq!(cell_style_of(&doc, 1, 1), Some(named));
    assert_eq!(table(&doc).value(1, 1).to_text(), "42");
    clean(&doc);

    doc.table_mut("Tabelle")
        .unwrap()
        .set("B2", CellValue::Empty)
        .unwrap();
    assert_eq!(
        cell_style_of(&doc, 1, 1),
        Some(named),
        "emptied, not deleted: the paint is still there"
    );
    assert_eq!(table(&doc).value(1, 1), CellValue::Empty);
    clean(&doc);
}

/// Bold is a variation of the text style the cell's area has, carrying the
/// bold toggle and nothing else; a colour goes to the font colour and to the
/// fill inside the glyphs, which is the one the app paints with.
#[test]
fn the_text_of_one_cell_is_a_variation_of_its_area_s_text_style() {
    let mut doc = fresh();
    doc.table_mut("Tabelle")
        .unwrap()
        .set("B2", "Zürich")
        .unwrap();
    doc.table_mut("Tabelle")
        .unwrap()
        .text_look("B2", &CellText::bold())
        .unwrap();
    clean(&doc);

    let (_, style) = text_style_of(&doc, 1, 1).expect("B2 names a text style");
    let archive = doc.archive(style).unwrap();
    let model = doc.archive(table(&doc).model).unwrap();
    assert_eq!(
        iwork::style::reference_at(&archive, &[1, 3, 1]),
        iwork::style::reference_at(&model, &[24, 1]),
        "its parent is body_text_style"
    );
    assert_eq!(archive.varint(10), Some(1));
    assert_eq!(
        iwork::style::get_path(&archive, iwork::style::property::BOLD),
        Some(Value::Varint(1))
    );
    assert_eq!(bag(&archive, 11).fields.len(), 1);
    assert!(
        archive.get(12).is_none(),
        "nothing about the paragraph differs"
    );

    // Colour on top of bold: a sibling variation with three properties, and
    // the count says three.
    doc.table_mut("Tabelle")
        .unwrap()
        .text_look("B2", &CellText::coloured(NAVY))
        .unwrap();
    let (_, style) = text_style_of(&doc, 1, 1).unwrap();
    let archive = doc.archive(style).unwrap();
    assert_eq!(
        iwork::style::reference_at(&archive, &[1, 3, 1]),
        iwork::style::reference_at(&model, &[24, 1]),
        "still a variation of the area's style, not of the variation"
    );
    assert_eq!(archive.varint(10), Some(3), "bold, font colour, fill");
    for path in [&[11u32, 7, 5][..], &[11, 46, 1, 5]] {
        match iwork::style::get_path(&archive, path) {
            Some(Value::Fixed32(bytes)) => {
                assert!((f32::from_le_bytes(bytes) - 0.38).abs() < 1e-6, "{path:?}")
            }
            other => panic!("{path:?} carries no blue: {other:?}"),
        }
    }
    clean(&doc);

    // A size and a face, in one call, on a header cell.
    doc.table_mut("Tabelle")
        .unwrap()
        .text_look(
            "A1:D1",
            &CellText {
                size: Some(14.0),
                font: Some("HelveticaNeue-Medium".into()),
                ..CellText::default()
            },
        )
        .unwrap();
    let (_, style) = text_style_of(&doc, 0, 2).unwrap();
    let archive = doc.archive(style).unwrap();
    assert_eq!(
        iwork::style::reference_at(&archive, &[1, 3, 1]),
        iwork::style::reference_at(&model, &[25, 1]),
        "header_row_text_style"
    );
    assert_eq!(archive.varint(10), Some(2));
    clean(&doc);
}

/// The fill and the text are two keys in one list, and the entry counts both.
#[test]
fn fill_and_text_are_two_keys_into_one_list() {
    let mut doc = fresh();
    let mut t = doc.table_mut("Tabelle").unwrap();
    t.set_block("A1", &[vec!["Region", "Einheiten", "Ertrag", "Anteil"]])
        .unwrap();
    t.fill("A1:D1", Some(NAVY)).unwrap();
    t.text_look(
        "A1:D1",
        &CellText::coloured(Color {
            red: 1.0,
            green: 1.0,
            blue: 1.0,
            alpha: 1.0,
        }),
    )
    .unwrap();
    clean(&doc);

    let (fill_key, _) = cell_style_of(&doc, 0, 0).unwrap();
    let (text_key, _) = text_style_of(&doc, 0, 0).unwrap();
    assert_ne!(fill_key, text_key);
    let t = table(&doc);
    // A1 is the corner: header row wins over header column for it.
    assert_eq!(t.side_tables().styles.entries[&fill_key].refcount, 4);
    assert_eq!(t.side_tables().styles.entries[&text_key].refcount, 4);
}

/// It survives a save and a reopen, references and all.
#[test]
fn the_look_survives_a_round_trip() {
    let mut doc = fresh();
    let mut t = doc.table_mut("Tabelle").unwrap();
    t.set("B2", 1240).unwrap();
    t.fill("A2:D2", Some(SAND)).unwrap();
    t.text_look("B2", &CellText::bold()).unwrap();

    let out = std::env::temp_dir().join("iwork-cell-look.numbers");
    let _ = std::fs::remove_file(&out);
    doc.save(&out).unwrap();
    let back = Document::open(&out).unwrap();
    clean(&back);
    assert!(cell_style_of(&back, 1, 3).is_some());
    assert!(text_style_of(&back, 1, 1).is_some());
    assert_eq!(back.undeclared_references(), Vec::new());
    let _ = std::fs::remove_file(&out);
}

/// Refused by name, and nothing moves.
#[test]
fn an_impossible_cell_is_refused() {
    let mut doc = fresh();
    let error = doc
        .table_mut("Tabelle")
        .unwrap()
        .fill("Z9", Some(NAVY))
        .unwrap_err();
    assert_eq!(
        error.refusal(),
        Some(iwork::Refusal::OutOfBounds),
        "{error:?}"
    );

    doc.table_mut("Tabelle").unwrap().set("B2", "kopf").unwrap();
    doc.table_mut("Tabelle").unwrap().merge("B2:C2").unwrap();
    let before = doc
        .table("Tabelle")
        .unwrap()
        .side_tables()
        .styles
        .entries
        .len();
    let error = doc
        .table_mut("Tabelle")
        .unwrap()
        .fill("C2", Some(NAVY))
        .unwrap_err();
    assert_eq!(error.refusal(), Some(iwork::Refusal::Merged), "{error:?}");
    assert_eq!(
        doc.table("Tabelle")
            .unwrap()
            .side_tables()
            .styles
            .entries
            .len(),
        before,
        "a refused write leaves the list as it was"
    );
    // The merge's own cell takes it.
    doc.table_mut("Tabelle")
        .unwrap()
        .fill("B2", Some(NAVY))
        .unwrap();
    clean(&doc);
}

/// A cell in a table Numbers wrote that *already* has a variation — its own
/// padding — gets a sibling carrying both, not a variation of the variation.
#[test]
fn a_cell_that_already_varies_gets_a_sibling_carrying_both() {
    let Some(path) = generated("numbers-links.numbers") else {
        eprintln!("no numbers-links.numbers — skipping (run scripts/make-fixtures.sh)");
        return;
    };
    let mut doc = Document::open(&path).unwrap();
    let t = doc.table("Budget-1").expect("the table");
    let cell = t
        .cells()
        .iter()
        .find(|c| c.record.cell_style_id.is_some())
        .map(|c| (c.row, c.column, c.record.cell_style_id.unwrap()))
        .expect("a cell with its own cell style");
    let old_style = t.side_tables().styles.entries[&cell.2].reference.unwrap();
    let old = doc.archive(old_style).unwrap();
    let parent = iwork::style::reference_at(&old, &[1, 3, 1]).expect("it is a variation");
    let had: Vec<u32> = bag(&old, 11).fields.iter().map(|f| f.number).collect();
    let users = t.side_tables().styles.entries[&cell.2].refcount;

    doc.set_cell_fill("Budget-1", [(cell.0, cell.1)], Some(SAND))
        .unwrap();

    let t = doc.table("Budget-1").unwrap();
    let key = t
        .cell(cell.0, cell.1)
        .unwrap()
        .record
        .cell_style_id
        .unwrap();
    assert_ne!(
        key, cell.2,
        "a new entry: the other cells keep the old look"
    );
    let style = t.side_tables().styles.entries[&key].reference.unwrap();
    let archive = doc.archive(style).unwrap();
    assert_eq!(
        iwork::style::reference_at(&archive, &[1, 3, 1]),
        Some(parent)
    );
    let mut expected = had.clone();
    expected.push(1);
    expected.sort_unstable();
    let now: Vec<u32> = bag(&archive, 11).fields.iter().map(|f| f.number).collect();
    assert_eq!(now, expected, "what it had, and the fill");
    assert_eq!(archive.varint(10), Some(expected.len() as u64));
    assert_eq!(
        t.side_tables().styles.entries[&cell.2].refcount,
        users - 1,
        "the old entry gave one reference back"
    );
    assert!(t.audit().is_empty(), "{:?}", t.audit());
    assert!(doc.problems().is_empty(), "{:?}", doc.problems());
}

/// **The acceptance test: Numbers draws it.** Off unless `IWORK_APP_CHECK=1`.
#[test]
fn numbers_draws_the_look_of_one_cell() {
    if std::env::var("IWORK_APP_CHECK").as_deref() != Ok("1") {
        eprintln!("IWORK_APP_CHECK is not 1 — skipping the cell-look oracle");
        return;
    }
    let mut doc = fresh();
    let mut t = doc.table_mut("Tabelle").unwrap();
    t.set_block("A1", &[vec!["Region", "Einheiten", "Ertrag", "Anteil"]])
        .unwrap();
    t.set("A2", "Zürich").unwrap();
    t.set("B2", 1240).unwrap();
    t.set("C2", 184_300).unwrap();
    // B2 painted and bold; C2 left alone; D3 painted and empty.
    t.fill("B2", Some(NAVY)).unwrap();
    t.text_look("B2", &CellText::bold()).unwrap();
    t.fill("D3", Some(SAND)).unwrap();

    let out = std::env::temp_dir().join("iwork-cell-look-app.numbers");
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
    let channels = |text: &str| -> Vec<u32> {
        text.split(',')
            .filter_map(|n| n.trim().parse().ok())
            .collect()
    };
    // cell, name, background, font name, font size, text colour
    let painted = cell("B2");
    let background = channels(&painted[2]);
    assert!(
        background.len() == 3 && background[2] > background[0],
        "B2 is painted navy — more blue than red: {painted:?}"
    );
    assert!(painted[3].contains("Bold"), "B2 is bold: {painted:?}");
    let plain = cell("C2");
    assert_eq!(plain[2], "none", "C2 is not painted: {plain:?}");
    assert!(!plain[3].contains("Bold"), "C2 is not bold: {plain:?}");
    let empty = cell("D3");
    assert_ne!(empty[2], "none", "an empty cell is painted too: {empty:?}");
    let _ = std::fs::remove_file(&out);
}

/// Alignment is a paragraph property, so it lands in the variation's second
/// bag — the shape `{ 11: …, 12: … }` a text variation in the corpus has —
/// and the count covers both.
#[test]
fn alignment_goes_in_the_paragraph_bag() {
    let mut doc = fresh();
    doc.table_mut("Tabelle")
        .unwrap()
        .set("B2", "Mitte")
        .unwrap();
    doc.table_mut("Tabelle")
        .unwrap()
        .text_look(
            "B2",
            &CellText {
                align: Some(iwork::table::Align::Centre),
                ..CellText::bold()
            },
        )
        .unwrap();
    let (_, style) = text_style_of(&doc, 1, 1).unwrap();
    let archive = doc.archive(style).unwrap();
    assert_eq!(
        iwork::style::get_path(&archive, iwork::style::property::ALIGNMENT),
        Some(Value::Varint(2))
    );
    assert_eq!(bag(&archive, 11).fields.len(), 1, "bold");
    assert_eq!(bag(&archive, 12).fields.len(), 1, "alignment");
    assert_eq!(archive.varint(10), Some(2), "the count covers both bags");
    clean(&doc);
}
