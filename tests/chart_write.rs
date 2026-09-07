//! Writing a chart — the private grid, and the copy that carries it.
//!
//! A chart holds its data twice and the two copies are not the same thing. The
//! **private grid** is `ChartArchive.grid`, written inline, and it is what the
//! chart draws — all a Pages or Keynote chart has. A Numbers chart fed by a
//! table has a second layer, a `TN.ChartMediatorArchive` of `TSCE` formulas,
//! and its grid is a *cache* of what those last evaluated to. Writing numbers
//! into that cache would make the chart disagree with the table it claims to
//! follow, so a chart with a mediator is refused by name rather than quietly
//! given numbers of its own.
//!
//! Adding a chart is a **copy**, not an invention: a dozen theme styles stand
//! behind one — a preset, a chart style and non-style, a legend pair, two axis
//! pairs, six series styles, a list of paragraph styles — and none of them can
//! be made up honestly. The caller names a chart to copy; what the source keeps
//! in its own component is copied and renumbered, and what lives in the theme
//! is shared, exactly as two charts made from one preset share it in the app.

use std::path::{Path, PathBuf};

use iwork::chart::{ChartData, GridValue};
use iwork::Document;

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

/// New numbers in a chart that is already there, and a shape it did not have.
#[test]
fn a_charts_private_grid_can_be_rewritten() {
    let mut doc = fixture!("keynote-charts.key");
    let chart = doc.charts()[0].identifier;
    let was = doc.charts()[0].chart_type;

    let data = ChartData::numbers(
        &["Nord", "Süd", "Ost"],
        &["Jan", "Feb", "Mär", "Apr"],
        &[
            &[10.0, 20.0, 30.0, 40.0],
            &[15.0, 25.0, 35.0, 45.0],
            &[5.0, 7.0, 9.0, 11.0],
        ],
    );
    doc.set_chart_data(chart, &data).unwrap();

    let after = doc
        .charts()
        .into_iter()
        .find(|c| c.identifier == chart)
        .unwrap();
    assert_eq!(after.grid.row_names, ["Nord", "Süd", "Ost"]);
    assert_eq!(after.grid.column_names, ["Jan", "Feb", "Mär", "Apr"]);
    assert_eq!(after.grid.value(2, 3), GridValue::Number(11.0));
    assert_eq!(after.chart_type, was, "only the grid was written");
    assert!(doc.problems().is_empty(), "{:?}", doc.problems());
}

/// A row that was already there keeps its identity; a new one gets a new one.
#[test]
fn rewriting_keeps_the_ids_of_the_rows_that_stay() {
    let mut doc = fixture!("keynote-charts.key");
    let chart = doc.charts()[0].identifier;
    let before = doc.charts()[0].grid.row_ids.clone();
    assert_eq!(before.len(), 2, "the fixture chart has two series");

    doc.set_chart_data(
        chart,
        &ChartData::numbers(&["A", "B", "C"], &["x"], &[&[1.0], &[2.0], &[3.0]]),
    )
    .unwrap();

    let after = doc
        .charts()
        .into_iter()
        .find(|c| c.identifier == chart)
        .unwrap();
    assert_eq!(after.grid.row_ids.len(), 3);
    assert_eq!(
        after.grid.row_ids[0].0, before[0].0,
        "the first series is the same series"
    );
    assert_eq!(after.grid.row_ids[1].0, before[1].0);
    assert!(
        !before.iter().any(|(id, _)| *id == after.grid.row_ids[2].0),
        "the third is new and says so"
    );
}

/// A blank cell is a present, zero-length message: leaving it out would shift
/// every value after it one column to the left.
#[test]
fn a_blank_cell_stays_where_it_is() {
    let mut doc = fixture!("keynote-charts.key");
    let chart = doc.charts()[0].identifier;
    doc.set_chart_data(
        chart,
        &ChartData {
            row_names: vec!["A".into()],
            column_names: vec!["x".into(), "y".into(), "z".into()],
            rows: vec![vec![
                GridValue::Number(1.0),
                GridValue::Empty,
                GridValue::Number(3.0),
            ]],
        },
    )
    .unwrap();

    let grid = doc
        .charts()
        .into_iter()
        .find(|c| c.identifier == chart)
        .unwrap()
        .grid;
    assert_eq!(grid.value(0, 0), GridValue::Number(1.0));
    assert_eq!(grid.value(0, 1), GridValue::Empty);
    assert_eq!(grid.value(0, 2), GridValue::Number(3.0));
}

/// A Numbers chart's grid is a cache of formulas this crate does not evaluate,
/// so it is refused by name.
#[test]
fn a_chart_fed_by_a_table_is_refused() {
    let mut doc = fixture!("numbers-charts.numbers");
    let chart = doc
        .charts()
        .into_iter()
        .find(|c| c.mediator.is_some())
        .expect("a Numbers chart follows a table")
        .identifier;
    let refusal = doc
        .set_chart_data(chart, &ChartData::numbers(&["A"], &["x"], &[&[1.0]]))
        .unwrap_err()
        .to_string();
    assert!(refusal.contains("fed by a table"), "{refusal}");
    assert!(refusal.contains("cache"), "{refusal}");
}

/// Names and values that do not line up are refused rather than drawn short.
#[test]
fn data_that_does_not_line_up_is_refused() {
    let mut doc = fixture!("keynote-charts.key");
    let chart = doc.charts()[0].identifier;

    let refusal = doc
        .set_chart_data(
            chart,
            &ChartData {
                row_names: vec!["A".into(), "B".into()],
                column_names: vec!["x".into()],
                rows: vec![vec![GridValue::Number(1.0)]],
            },
        )
        .unwrap_err()
        .to_string();
    assert!(refusal.contains("row name"), "{refusal}");

    let refusal = doc
        .set_chart_data(
            chart,
            &ChartData {
                row_names: vec!["A".into()],
                column_names: vec!["x".into()],
                rows: vec![vec![GridValue::Number(1.0), GridValue::Number(2.0)]],
            },
        )
        .unwrap_err()
        .to_string();
    assert!(refusal.contains("column name"), "{refusal}");
}

/// A copy is a chart of its own: its own object, its own private non-styles,
/// the theme's styles shared.
#[test]
fn a_copied_chart_owns_what_was_private_and_shares_the_theme() {
    let mut doc = fixture!("pages-numbering.pages");
    let from = doc.charts()[0].identifier;
    let source = doc.charts()[0].clone();

    let chart = doc
        .add_chart(
            "page 1",
            from,
            &ChartData::numbers(
                &["Alpha", "Beta"],
                &["Q1", "Q2"],
                &[&[7.0, 8.0], &[3.0, 2.0]],
            ),
            (100.0, 100.0),
            (400.0, 300.0),
        )
        .unwrap();

    let copy = doc
        .charts()
        .into_iter()
        .find(|c| c.identifier == chart)
        .expect("the copy is a chart");
    assert_ne!(copy.identifier, source.identifier);
    assert_eq!(
        copy.chart_type, source.chart_type,
        "it looks like its source"
    );
    assert_eq!(copy.grid.row_names, ["Alpha", "Beta"]);
    assert_eq!(
        copy.series_theme_styles, source.series_theme_styles,
        "the theme's styles are shared, as they are between two charts in the app"
    );
    assert_ne!(
        copy.chart_non_style, source.chart_non_style,
        "what the source kept in its own component is the copy's own"
    );
    // It is on the page it was put on, at the rectangle it was given, and a
    // Pages page owns nothing — so no parent.
    let drawable = doc.drawable(chart).unwrap();
    assert_eq!(drawable.parent, None);
    assert_eq!(
        drawable.placement,
        iwork::drawable::Placement::Floating { page: 0 }
    );
    let frame = drawable.frame(None);
    assert_eq!((frame.x, frame.y), (100.0, 100.0));
    assert_eq!((frame.width, frame.height), (400.0, 300.0));

    assert!(doc.problems().is_empty(), "{:?}", doc.problems());
    assert!(doc.undeclared_references().is_empty());
}

/// A slide owns the copy it is given, the way it owns every other drawable.
#[test]
fn a_slide_owns_a_chart_copied_onto_it() {
    let mut doc = fixture!("keynote-charts.key");
    let from = doc.charts()[0].identifier;
    let slide = doc.slides()[0].identifier;
    let chart = doc
        .add_chart(
            &slide.to_string(),
            from,
            &ChartData::numbers(&["A"], &["x"], &[&[1.0]]),
            (100.0, 100.0),
            (400.0, 300.0),
        )
        .unwrap();
    assert_eq!(doc.drawable(chart).unwrap().parent, Some(slide));
    assert!(doc.problems().is_empty(), "{:?}", doc.problems());
}

/// Copying a chart that follows a table would claim to follow it too.
#[test]
fn a_chart_fed_by_a_table_is_not_copied_either() {
    let mut doc = fixture!("numbers-charts.numbers");
    let from = doc
        .charts()
        .into_iter()
        .find(|c| c.mediator.is_some())
        .unwrap()
        .identifier;
    let sheet = doc.tables()[0]
        .sheet
        .clone()
        .expect("a Numbers table is on a sheet");
    let refusal = doc
        .add_chart(
            &sheet,
            from,
            &ChartData::numbers(&["A"], &["x"], &[&[1.0]]),
            (0.0, 0.0),
            (100.0, 100.0),
        )
        .unwrap_err()
        .to_string();
    assert!(refusal.contains("mediator"), "{refusal}");
}

/// The apps read a rewritten chart back and write it out again. Off unless
/// `IWORK_APP_CHECK=1`.
#[test]
fn the_apps_resave_a_chart_this_crate_rewrote() {
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
    let data = ChartData::numbers(
        &["Nord", "Süd", "Ost"],
        &["Jan", "Feb", "Mär", "Apr"],
        &[
            &[10.0, 20.0, 30.0, 40.0],
            &[15.0, 25.0, 35.0, 45.0],
            &[5.0, 7.0, 9.0, 11.0],
        ],
    );
    let kept = |path: &Path| {
        let after = Document::open(path).unwrap();
        let chart = after
            .charts()
            .into_iter()
            .find(|c| c.grid.row_names == ["Nord", "Süd", "Ost"])
            .unwrap_or_else(|| panic!("the app dropped the data out of {}", path.display()));
        assert_eq!(chart.grid.column_names, ["Jan", "Feb", "Mär", "Apr"]);
        assert_eq!(chart.grid.value(2, 3), GridValue::Number(11.0));
        assert!(after.problems().is_empty(), "{:?}", after.problems());
    };

    let mut deck = fixture!("keynote-charts.key");
    let chart = deck.charts()[0].identifier;
    deck.set_chart_data(chart, &data).unwrap();
    let out = scratch("iwork-chart.key");
    deck.save(&out).unwrap();
    resave(&out);
    kept(&out);
    let _ = std::fs::remove_dir_all(&out);

    let mut doc = fixture!("pages-numbering.pages");
    let from = doc.charts()[0].identifier;
    doc.add_chart("page 1", from, &data, (100.0, 100.0), (400.0, 300.0))
        .unwrap();
    let out = scratch("iwork-chart.pages");
    doc.save(&out).unwrap();
    resave(&out);
    kept(&out);
    let _ = std::fs::remove_dir_all(&out);
}
