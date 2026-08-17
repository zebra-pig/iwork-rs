//! Tables, read — and checked against the app that wrote them.
//!
//! Two halves. The first reads the generated corpus and asserts what is in it;
//! it needs no app and runs everywhere. The second, behind `IWORK_APP_CHECK=1`,
//! asks Numbers itself what every cell of every table holds and compares the
//! two answers cell by cell. **The app is the oracle**: a value this crate
//! decodes and Numbers does not agree with is a bug here, and the whole point
//! of decoding a format nobody documents is to have something to be wrong
//! against.
//!
//! Without `tests/fixtures/generated`, both halves pass having asserted
//! nothing and say so — `scripts/make-fixtures.sh` builds it.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use iwork::table::{CellFormat, CellValue, Merge, Table};
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

fn table(doc: &Document, name: &str) -> Table {
    doc.table(name)
        .unwrap_or_else(|| panic!("no table called {name}"))
}

macro_rules! fixture {
    ($name:expr) => {
        match open($name) {
            Some(doc) => doc,
            None => {
                eprintln!("no {} — skipping (run scripts/make-fixtures.sh)", $name);
                return;
            }
        }
    };
}

// -- the object graph --------------------------------------------------------

#[test]
fn tables_are_found_with_their_sheets_and_sizes() {
    let doc = fixture!("numbers-values.numbers");
    let tables = doc.tables();
    let named: Vec<(&str, usize, usize, Option<&str>)> = tables
        .iter()
        .map(|t| (t.name.as_str(), t.rows, t.columns, t.sheet.as_deref()))
        .collect();
    assert_eq!(
        named,
        vec![
            ("Zellarten", 10, 5, Some("Werte")),
            ("Zweite Tabelle", 4, 3, Some("Werte")),
            ("Kennzahlen", 10, 5, Some("Zweites Blatt")),
        ]
    );
    for t in &tables {
        assert!(t.problems.is_empty(), "{}: {:?}", t.name, t.problems);
        assert!(!t.table_id.is_empty(), "{} has no table_id", t.name);
        assert!(t.parent.is_some(), "{} has no parent drawable", t.name);
    }
}

/// Header counts, freeze flags and the stored row/column geometry.
#[test]
fn structure_state_is_read_from_the_model() {
    let doc = fixture!("numbers-values.numbers");
    let t = table(&doc, "Zellarten");
    assert_eq!(t.header_rows, 1);
    assert_eq!(t.header_columns, 1);
    assert_eq!(t.footer_rows, 0);
    assert!(t.header_rows_frozen);
    assert!(t.header_columns_frozen);
    assert_eq!(t.hidden_rows, 0);
    assert_eq!(t.hidden_columns, 0);
    assert_eq!(t.row_extents.len(), t.rows);
    assert_eq!(t.column_extents.len(), t.columns);
    // Nothing was resized, so no row or column carries a size of its own and
    // every one of them falls back to the table's default.
    assert!(t.row_extents.iter().all(|e| e.size.is_none()));
    assert!(t.column_extents.iter().all(|e| e.size.is_none()));
    assert_eq!(t.column_width(0), t.default_column_width);
    assert_eq!(t.row_height(0), t.default_row_height);
    assert!(t.row_extents.iter().all(|e| !e.hidden()));
    // Header entries do carry the per-row and per-column cell counts, which is
    // how many of them are non-empty.
    assert_eq!(t.row_extents[0].cell_count, 3, "A1:C1 are filled");
    assert_eq!(t.column_extents[0].cell_count, 8);
}

/// A row height and a column width set by hand, against the many that are not.
#[test]
fn explicit_row_and_column_sizes_are_told_from_the_default() {
    let doc = fixture!("numbers-formats.numbers");
    let t = table(&doc, "Spaltenformat");
    assert_eq!(
        t.column_extents[1].size,
        Some(150.0),
        "column B was widened"
    );
    assert_eq!(t.column_width(1), 150.0);
    assert_eq!(t.row_extents[2].size, Some(40.0), "row 3 was made taller");
    assert_eq!(t.row_height(2), 40.0);
    // Everything else still says nothing and means "the table's default".
    assert_eq!(t.column_extents[0].size, None);
    assert_eq!(t.column_width(0), t.default_column_width);
}

/// A table past one tile's 256 rows, which is the whole reason the fixture is
/// 301 rows long.
#[test]
fn a_table_larger_than_one_tile_reads_every_row() {
    let doc = fixture!("numbers-large.numbers");
    let t = table(&doc, "Zeilen");
    assert_eq!((t.rows, t.columns), (301, 9));
    assert!(t.problems.is_empty(), "{:?}", t.problems);
    assert_eq!(t.value(0, 0), CellValue::Text("Region".into()));
    // Row 300 is in the second tile; the first holds rows 0..255.
    assert_eq!(t.value(300, 7), CellValue::Text("Zeile 300".into()));
    assert_eq!(t.value(300, 2).to_text(), "900");
    let filled = t.cells().iter().filter(|c| !c.value.is_empty()).count();
    assert!(filled > 2000, "only {filled} cells decoded");
}

/// Every record in the corpus is consumed exactly, with nothing left over.
///
/// This is the check that the flag word's twenty-one bits are the right widths
/// in the right order. Get one of them wrong and the cursor ends somewhere
/// else: too early on the last cell of a row, too late on all the others. It
/// says nothing about what the fields *mean* — the oracle does that — but a
/// decoder that disagreed about the layout could not add up.
#[test]
fn every_cell_record_is_consumed_to_the_byte() {
    let mut records = 0usize;
    for name in [
        "numbers-values.numbers",
        "numbers-formats.numbers",
        "numbers-large.numbers",
        "pages-report.pages",
    ] {
        let Some(path) = generated(name) else {
            continue;
        };
        let doc = Document::open(&path).unwrap();
        for t in doc.tables() {
            assert!(t.problems.is_empty(), "{name}/{}: {:?}", t.name, t.problems);
            for cell in t.cells() {
                assert_eq!(
                    cell.record.trailing, 0,
                    "{name}/{}/r{}c{}: {} bytes past the last field",
                    t.name, cell.row, cell.column, cell.record.trailing
                );
                assert_eq!(cell.record.version, 5);
                assert_eq!(
                    cell.record.reserved, [0; 4],
                    "{name}/{}/r{}c{}: bytes 2-5 are not zero",
                    t.name, cell.row, cell.column
                );
                records += 1;
            }
        }
    }
    assert!(records > 2000, "only {records} records seen");
    eprintln!("{records} cell records decoded with nothing left over");
}

// -- cell values -------------------------------------------------------------

#[test]
fn every_cell_type_decodes() {
    let doc = fixture!("numbers-values.numbers");
    let t = table(&doc, "Zellarten");
    assert_eq!(t.value(0, 0), CellValue::Text("Art".into()));
    assert_eq!(
        t.value(1, 1),
        CellValue::Text("Zeichenkette mit Umlaut: Größe".into())
    );
    assert_eq!(t.value(2, 1).to_text(), "42");
    assert_eq!(t.value(2, 2).to_text(), "84", "the formula's cached value");
    assert_eq!(t.value(3, 2), CellValue::Bool(false));
    assert_eq!(t.value(4, 1).to_text(), "2024-03-01T10:30:00Z");
    assert_eq!(t.value(5, 1), CellValue::Duration(5400.0));
    assert_eq!(t.value(6, 1).to_text(), "3.14159");
    assert_eq!(t.value(6, 2).to_text(), "3.14");
    assert_eq!(t.value(7, 2).to_text(), "45.14159");
    assert!(t.cell(2, 2).unwrap().has_formula);
    assert!(!t.cell(2, 1).unwrap().has_formula);
}

/// Numbers keeps cell numbers as a decimal, not a `double`. `3.14159` must come
/// back as those digits and not as `3.1415899999999999`.
#[test]
fn numbers_keep_their_decimal_digits() {
    let doc = fixture!("numbers-values.numbers");
    let t = table(&doc, "Zellarten");
    let CellValue::Number(pi) = t.value(6, 1) else {
        panic!("B7 is not a number");
    };
    assert_eq!(pi.to_string(), "3.14159");
    // Spelled as a parse rather than a literal: the point is that the decimal
    // and the double agree, not that this happens to be a well-known constant.
    assert_eq!(pi.to_f64(), "3.14159".parse::<f64>().unwrap());
    // Numbers normalises to fifteen significant digits rather than storing the
    // shortest form, so the trailing zeroes are real and the printing has to
    // drop them.
    assert_eq!(pi.mantissa, 314_159_000_000_000);
    assert_eq!(pi.exponent, -14);
}

/// Pages tables put their text in `TSWP.StorageArchive`s reached through the
/// rich-text list, not in the table's own string table.
#[test]
fn a_pages_table_reads_its_rich_text_cells() {
    let doc = fixture!("pages-report.pages");
    let tables = doc.tables();
    assert_eq!(tables.len(), 1, "the template document has one table");
    let t = &tables[0];
    assert_eq!((t.rows, t.columns), (6, 4));
    assert!(t.sheet.is_none(), "Pages has no sheets");
    assert_eq!(t.value(0, 0), CellValue::RichText("Description".into()));
    assert_eq!(t.value(0, 3), CellValue::RichText("Cost".into()));
    assert_eq!(t.value(1, 0), CellValue::Text("Süßwaren".into()));
    assert!(matches!(t.value(1, 2), CellValue::Currency(_)));
}

// -- formats, controls and merges --------------------------------------------

#[test]
fn each_data_format_is_read_alongside_its_value() {
    let doc = fixture!("numbers-formats.numbers");
    let t = table(&doc, "Formate");
    let by_label: BTreeMap<String, CellFormat> = (1..t.rows)
        .filter_map(|row| match t.value(row, 0) {
            CellValue::Text(label) => Some((label, t.cell(row, 1)?.format)),
            _ => None,
        })
        .collect();
    for (label, expected) in [
        ("Automatisch", CellFormat::Automatic),
        ("Zahl", CellFormat::Number),
        ("Währung", CellFormat::Currency),
        ("Prozent", CellFormat::Percentage),
        ("Wissenschaftlich", CellFormat::Scientific),
        ("Bruch", CellFormat::Fraction),
        ("Text", CellFormat::Text),
        ("Zahlensystem", CellFormat::NumeralSystem),
        ("Ankreuzfeld", CellFormat::Checkbox),
        ("Bewertung", CellFormat::Rating),
        ("Schieberegler", CellFormat::Slider),
        ("Schrittwert", CellFormat::Stepper),
        ("Einblendmenü", CellFormat::PopUpMenu),
        ("Datum formatiert", CellFormat::DateTime),
        ("Dauer formatiert", CellFormat::Duration),
        // The value is a date; nobody asked for a format, so there is none.
        ("Datum", CellFormat::Automatic),
    ] {
        assert_eq!(by_label.get(label), Some(&expected), "row labelled {label}");
    }
}

/// A control is a second thing on top of the format: a slider cell carries the
/// plain number format and a `TST.CellSpecArchive` saying it is a slider.
#[test]
fn control_cells_are_identified() {
    use iwork::table::CellControl;
    let doc = fixture!("numbers-formats.numbers");
    let t = table(&doc, "Formate");
    let controls: Vec<Option<CellControl>> = (1..t.rows)
        .filter_map(|row| t.cell(row, 1).map(|c| c.control))
        .collect();
    for wanted in [
        CellControl::Checkbox,
        CellControl::Rating,
        CellControl::Slider,
        CellControl::Stepper,
        CellControl::PopUpMenu,
    ] {
        assert!(
            controls.contains(&Some(wanted)),
            "no {} cell found",
            wanted.as_str()
        );
    }
}

#[test]
fn merged_ranges_are_read_from_the_merge_owner() {
    let doc = fixture!("numbers-formats.numbers");
    let t = table(&doc, "Verbunden");
    assert_eq!(
        t.merges,
        vec![
            // B2:D2, B4:B6, D4:F5, B8:C8 — across, down, a rectangle, and one
            // whose top-left cell was never given a value.
            Merge {
                row: 1,
                column: 1,
                rows: 1,
                columns: 3
            },
            Merge {
                row: 3,
                column: 1,
                rows: 3,
                columns: 1
            },
            Merge {
                row: 3,
                column: 3,
                rows: 2,
                columns: 3
            },
            Merge {
                row: 7,
                column: 1,
                rows: 1,
                columns: 2
            },
        ]
    );
    // The covered cells hold nothing at all — not even a `spanCellType`
    // record. Their offsets are the plain −1 of an empty cell.
    assert!(t.value(1, 2).is_empty());
    assert!(t.value(1, 3).is_empty());
    assert_eq!(t.value(1, 1), CellValue::Text("quer".into()));
}

#[test]
fn a_simple_merge_is_found_in_the_values_fixture() {
    let doc = fixture!("numbers-values.numbers");
    let t = table(&doc, "Zellarten");
    assert_eq!(
        t.merges,
        vec![Merge {
            row: 8,
            column: 3,
            rows: 1,
            columns: 2
        }],
        "D9:E9"
    );
}

// -- the oracle --------------------------------------------------------------

/// One cell as Numbers reports it.
#[derive(Debug)]
struct OracleCell {
    name: String,
    class: String,
    value: String,
    formatted: String,
    format: String,
    formula: String,
}

struct OracleTable {
    name: String,
    rows: usize,
    columns: usize,
    header_rows: u32,
    header_columns: u32,
    footer_rows: u32,
    cells: Vec<OracleCell>,
}

fn read_oracle(document: &Path) -> Vec<OracleTable> {
    let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("scripts/table-oracle.sh");
    let output = std::process::Command::new(&script)
        .arg(document)
        .output()
        .unwrap_or_else(|e| panic!("{}: {e}", script.display()));
    assert!(
        output.status.success(),
        "{}: Numbers would not report on {}:\n{}",
        script.display(),
        document.display(),
        String::from_utf8_lossy(&output.stderr)
    );

    let text = String::from_utf8_lossy(&output.stdout).into_owned();
    let mut tables: Vec<OracleTable> = Vec::new();
    for line in text.lines() {
        let field: Vec<&str> = line.split('\t').collect();
        match field.first() {
            Some(&"table") if field.len() >= 7 => tables.push(OracleTable {
                name: field[1].to_string(),
                rows: field[2].parse().unwrap(),
                columns: field[3].parse().unwrap(),
                header_rows: field[4].parse().unwrap(),
                header_columns: field[5].parse().unwrap(),
                footer_rows: field[6].parse().unwrap(),
                cells: Vec::new(),
            }),
            Some(&"cell") if field.len() >= 7 => {
                let table = tables.last_mut().expect("a cell before any table");
                table.cells.push(OracleCell {
                    name: field[1].to_string(),
                    class: field[2].to_string(),
                    value: field[3].to_string(),
                    formatted: field[4].to_string(),
                    format: field[5].to_string(),
                    formula: field[6].to_string(),
                });
            }
            _ => {}
        }
    }
    tables
}

/// `A1` for a position, the way the app names cells.
fn reference_name(row: usize, column: usize) -> String {
    let mut letters = String::new();
    let mut n = column + 1;
    while n > 0 {
        letters.insert(0, (b'A' + ((n - 1) % 26) as u8) as char);
        n = (n - 1) / 26;
    }
    format!("{letters}{}", row + 1)
}

/// Does this crate's value agree with what the app said?
///
/// Some of the app's answers are locale- and timezone-dependent and cannot be
/// compared literally:
///
/// * a `real` comes back as AppleScript renders it — `255.0`, `1.2345678E+4` —
///   so numbers are compared as numbers, not as strings;
/// * a `date` comes back in the machine's timezone and its own long form,
///   while the stored seconds are timezone-naive. The *formatted* value is what
///   the app draws in the cell and what the stored number means, so dates are
///   compared against that;
/// * a duration is a `real` of seconds, which is exactly what is stored.
fn agrees(ours: &CellValue, oracle: &OracleCell) -> bool {
    match ours {
        CellValue::Empty | CellValue::Span => oracle.class == "empty",
        CellValue::Text(text) | CellValue::RichText(text) => text == &oracle.value,
        CellValue::Bool(b) => oracle.value.eq_ignore_ascii_case(&b.to_string()),
        CellValue::Number(d) | CellValue::Currency(d) => {
            near(d.to_f64(), oracle.value.parse::<f64>().ok())
        }
        CellValue::Duration(seconds) => near(*seconds, oracle.value.parse::<f64>().ok()),
        CellValue::Date(seconds) => {
            // "01.03.2024 10:30" or "01.03.2024" — compare the parts the
            // stored seconds actually determine.
            let iso = iwork::table::format_date(*seconds);
            let (date, time) = iso.split_at(10);
            let day = &date[8..10];
            let month = &date[5..7];
            let year = &date[..4];
            oracle.formatted.contains(&format!("{day}.{month}.{year}"))
                && (oracle.formatted.len() <= 10 || oracle.formatted.contains(&time[1..6]))
        }
        CellValue::Error => oracle.class == "empty" || !oracle.formula.is_empty(),
        CellValue::Unknown(_) => false,
    }
}

fn near(ours: f64, theirs: Option<f64>) -> bool {
    theirs.is_some_and(|t| (ours - t).abs() <= 1e-9 * ours.abs().max(1.0))
}

/// Every cell of every table of both Numbers fixtures, against Numbers.
///
/// Off unless `IWORK_APP_CHECK=1`: it drives the app. What it compares is the
/// value, the data format and whether the cell holds a formula — and, through
/// the names the app gives merged cells, the merged ranges too. A merged-away
/// cell reports the name of the cell it was merged into, which is the only way
/// the scripting interface admits a merge exists.
#[test]
fn every_cell_agrees_with_numbers() {
    if std::env::var("IWORK_APP_CHECK").as_deref() != Ok("1") {
        eprintln!("IWORK_APP_CHECK is not 1 — skipping the oracle comparison");
        return;
    }
    let mut compared = 0usize;
    for name in [
        "numbers-values.numbers",
        "numbers-formats.numbers",
        "numbers-large.numbers",
    ] {
        let Some(path) = generated(name) else {
            eprintln!("no {name} — skipping");
            continue;
        };
        let doc = Document::open(&path).unwrap();
        let ours = doc.tables();
        let theirs = read_oracle(&path);
        assert_eq!(
            ours.len(),
            theirs.len(),
            "{name}: we found {} tables, Numbers {}",
            ours.len(),
            theirs.len()
        );

        for (mine, app) in ours.iter().zip(&theirs) {
            assert_eq!(mine.name, app.name, "{name}: table names differ");
            assert_eq!(
                (mine.rows, mine.columns),
                (app.rows, app.columns),
                "{name}/{}",
                app.name
            );
            assert_eq!(mine.header_rows, app.header_rows, "{name}/{}", app.name);
            assert_eq!(
                mine.header_columns, app.header_columns,
                "{name}/{}",
                app.name
            );
            assert_eq!(mine.footer_rows, app.footer_rows, "{name}/{}", app.name);
            assert_eq!(
                app.cells.len(),
                app.rows * app.columns,
                "{name}/{}: the app reported a different number of cells",
                app.name
            );

            for (index, cell) in app.cells.iter().enumerate() {
                let row = index / app.columns;
                let column = index % app.columns;
                let where_ = format!("{name}/{}/{}", app.name, reference_name(row, column));

                // The app does not report a merged-away cell at all: it
                // repeats the cell the merge began in, name, value and
                // format. So the names it gives back *are* the merge map, and
                // the value to compare is the anchor's.
                let (row, column) = mine
                    .merges
                    .iter()
                    .find(|m| {
                        (m.row..m.row + m.rows).contains(&row)
                            && (m.column..m.column + m.columns).contains(&column)
                    })
                    .map(|m| (m.row, m.column))
                    .unwrap_or((row, column));
                assert_eq!(
                    cell.name,
                    reference_name(row, column),
                    "{where_}: the app calls this cell {}",
                    cell.name
                );
                let value = mine.value(row, column);

                assert!(
                    agrees(&value, cell),
                    "{where_}: we say {value:?}, Numbers says class {} value {:?} formatted {:?}",
                    cell.class,
                    cell.value,
                    cell.formatted
                );
                if let Some(decoded) = mine.cell(row, column) {
                    assert_eq!(
                        decoded.format.as_str(),
                        cell.format,
                        "{where_}: data format"
                    );
                    assert_eq!(
                        decoded.has_formula,
                        !cell.formula.is_empty(),
                        "{where_}: the app {} a formula",
                        if cell.formula.is_empty() {
                            "reports no"
                        } else {
                            "reports"
                        }
                    );
                } else {
                    assert_eq!(cell.format, "automatic", "{where_}: no record but a format");
                }
                compared += 1;
            }
        }
    }
    eprintln!("{compared} cells compared against Numbers");
    assert!(compared > 0, "nothing was compared");
}
