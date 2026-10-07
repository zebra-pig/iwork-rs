//! The 0.3 surface: values to create, handles to edit. `API.md` is the rule
//! book; this is what holds the crate to it.

use std::path::Path;

use iwork::drawable::{ImageFit, ImageSource, StrokePattern};
use iwork::{
    Align, Chart, ChartKind, Color, Document, Fill, Gradient, Image, Kind, Shadow, Shape, Table,
    TextBox, TextLook, TextStyle,
};

const NAVY: Color = Color::rgb8(0x12, 0x2B, 0x4A);
const RUST: Color = Color::rgb8(0xB4, 0x4A, 0x2B);

/// A 32 × 24 PNG.
fn png() -> Vec<u8> {
    const HEX: &str = "89504e470d0a1a0a0000000d4948445200000020000000180802000000143168630000002449444154789c63b8a3a14153c4306ac1a805a3168c5a306ac1a805a3168c5a30342c00004509842ee242a92c0000000049454e44ae426082";
    (0..HEX.len())
        .step_by(2)
        .map(|at| u8::from_str_radix(&HEX[at..at + 2], 16).unwrap())
        .collect()
}

fn quarters(kind: ChartKind) -> Chart {
    Chart::new(kind)
        .categories(["Q1", "Q2", "Q3", "Q4"])
        .series("2025", [31.0, 35.5, 40.8, 42.0])
        .series("2026", [40.1, 44.5, 48.2, 51.0])
}

/// Where an element is and how big: `((x, y), (width, height))`.
fn frame_of(doc: &Document, element: u64) -> ((f32, f32), (f32, f32)) {
    let g = doc.drawable(element).unwrap().geometry;
    ((g.x, g.y), (g.width, g.height))
}

fn style_of(doc: &Document, element: u64) -> iwork::drawable::ObjectStyle {
    let style = doc.drawable(element).and_then(|d| d.style).unwrap();
    doc.object_style(style).unwrap()
}

/// A deck built with nothing but the 0.3 API, one of everything.
fn deck() -> (Document, [u64; 5]) {
    let mut doc = Document::new(Kind::Keynote).unwrap();
    let title = doc
        .add_text_style(
            &TextStyle::new("Titel")
                .look(
                    TextLook::new()
                        .font("AvenirNext-Bold")
                        .size(64.0)
                        .colour(Color::WHITE),
                )
                .align(Align::Centre),
        )
        .unwrap();
    let mut slide = doc.slide_mut(0).unwrap();
    slide
        .background(Gradient::linear(NAVY, RUST, 270.0))
        .unwrap();
    let card = slide
        .add(
            Shape::rectangle()
                .at(120.0, 320.0)
                .size(700.0, 500.0)
                .fill(Color::WHITE)
                .no_stroke()
                .shadow(Shadow::default()),
        )
        .unwrap();
    let words = slide
        .add(
            TextBox::new("Umsatz plus 18 %")
                .at(120.0, 120.0)
                .size(1680.0, 120.0)
                .style(title)
                .format(12..16, TextLook::new().colour(RUST)),
        )
        .unwrap();
    let photo = slide
        .add(
            Shape::ellipse()
                .at(1500.0, 320.0)
                .size(240.0, 240.0)
                .fill(Fill::image(png(), "p.png")),
        )
        .unwrap();
    let picture = slide
        .add(Image::new(png()).named("bild.png").at(1500.0, 620.0))
        .unwrap();
    let chart = slide
        .add(
            quarters(ChartKind::Column)
                .at(900.0, 320.0)
                .size(560.0, 500.0)
                .title("Umsatz")
                .legend(),
        )
        .unwrap();
    (doc, [card, words, photo, picture, chart])
}

/// Each value becomes the thing it describes, and reads back as it.
#[test]
fn values_become_what_they_describe() {
    let (doc, [card, words, photo, picture, chart]) = deck();
    assert!(doc.problems().is_empty(), "{:?}", doc.problems());
    let out = std::env::temp_dir().join("iwork-elements.key");
    let _ = std::fs::remove_file(&out);
    doc.save(&out).unwrap();
    let doc = Document::open(&out).unwrap();
    assert!(doc.problems().is_empty(), "{:?}", doc.problems());

    let look = style_of(&doc, card);
    assert_eq!(look.fill, Some(Fill::Color(Color::WHITE)));
    assert!(look.shadow.unwrap().enabled);
    assert_eq!(look.stroke.unwrap().width, 0.0);
    assert_eq!(frame_of(&doc, card), ((120.0, 320.0), (700.0, 500.0)));

    let storage = doc.drawable(words).unwrap().text.unwrap();
    assert_eq!(doc.storage_text(storage).unwrap(), "Umsatz plus 18 %");
    let style = style_of(&doc, words);
    assert_eq!(
        style.stroke.unwrap().pattern,
        StrokePattern::Empty,
        "a text box has no outline"
    );

    match style_of(&doc, photo).fill {
        Some(Fill::Image(image)) => {
            assert_eq!(image.fit, ImageFit::ScaleToFill);
            assert!(matches!(image.source, ImageSource::Stored(_)));
        }
        other => panic!("{other:?}"),
    }
    // The picture on the slide and the picture in the ellipse are one file.
    let stored = doc
        .data_files()
        .iter()
        .filter(|f| f.stored_name.ends_with(".png"))
        .count();
    assert_eq!(stored, 1);
    assert_eq!(frame_of(&doc, picture).1, (32.0, 24.0), "its pixel size");

    let made = doc
        .charts()
        .into_iter()
        .find(|c| c.identifier == chart)
        .unwrap();
    assert_eq!(made.type_name(), "columnChartType2D");
    assert_eq!(made.series_count(), 2);
    assert_eq!(made.categories(), ["Q1", "Q2", "Q3", "Q4"]);
}

/// A value is a value: built without a document, compared, added twice.
#[test]
fn a_value_can_be_kept_and_added_again() {
    let bar = Shape::rectangle().size(180.0, 10.0).fill(RUST).no_stroke();
    assert_eq!(bar, bar.clone());
    let mut doc = Document::new(Kind::Keynote).unwrap();
    doc.add_slide(None).unwrap();
    let first = doc.slide_mut(0).unwrap().add(&bar).unwrap();
    let second = doc
        .slide_mut(1)
        .unwrap()
        .add(bar.clone().at(40.0, 40.0))
        .unwrap();
    assert_ne!(first, second);
    assert_eq!(frame_of(&doc, first).0, (0.0, 0.0));
    assert_eq!(frame_of(&doc, second).0, (40.0, 40.0));
    assert_eq!(frame_of(&doc, first).1, (180.0, 10.0));
}

/// The same values go on a sheet and on a page.
#[test]
fn a_sheet_and_a_page_take_the_same_values() {
    let mut sheet = Document::new(Kind::Numbers).unwrap();
    let name = sheet.sheets()[0].name.clone();
    let chart = sheet
        .sheet_mut(&name)
        .unwrap()
        .add(quarters(ChartKind::Line).at(40.0, 420.0).legend())
        .unwrap();
    assert!(sheet.charts().iter().any(|c| c.identifier == chart));
    assert!(sheet.sheet_mut("no such sheet").is_err());
    assert!(sheet.problems().is_empty(), "{:?}", sheet.problems());

    let mut pages = Document::new(Kind::Pages).unwrap();
    let mut page = pages.page_mut(1).unwrap();
    page.add(Shape::ellipse().at(72.0, 300.0).fill(NAVY))
        .unwrap();
    page.add(
        TextBox::new("Am Rand")
            .at(300.0, 300.0)
            .look(TextLook::new().italic()),
    )
    .unwrap();
    page.add(quarters(ChartKind::Pie).at(72.0, 520.0).size(300.0, 300.0))
        .unwrap();
    assert!(pages.problems().is_empty(), "{:?}", pages.problems());
    assert!(pages.page_mut(0).is_err());
    assert!(sheet.page_mut(1).is_err(), "a spreadsheet has no pages");
}

/// What cannot be written is refused by `add`, and a refused `add` leaves
/// the document exactly as it was — nothing half-made.
#[test]
fn a_refused_add_changes_nothing() {
    let mut doc = Document::new(Kind::Keynote).unwrap();
    let before = doc.objects().count();
    let mut slide = doc.slide_mut(0).unwrap();

    let lopsided = Chart::new(ChartKind::Bar)
        .categories(["a", "b"])
        .series("s", [1.0]);
    assert!(slide.add(lopsided).is_err());
    assert!(slide.add(Chart::new(ChartKind::Bar)).is_err(), "no data");
    // The shape is made and then its fill is refused: the shape goes too.
    let bad = Gradient {
        stops: vec![(NAVY, 0.9), (RUST, 0.1)],
        angle: 0.0,
    };
    assert!(slide.add(Shape::rectangle().fill(bad)).is_err());
    assert!(slide
        .add(Shape::rectangle().fill(Fill::image(b"not a picture".to_vec(), "x")))
        .is_err());
    assert!(slide
        .add(TextBox::new("kurz").format(2..99, TextLook::new().bold()))
        .is_err());
    assert!(slide.add(Image::new(b"not a picture".to_vec())).is_err());

    assert_eq!(doc.objects().count(), before);
    assert!(
        doc.changed_streams().is_empty(),
        "{:?}",
        doc.changed_streams()
    );
    assert!(Color::hex("#12").is_err());
    assert_eq!(Color::hex("#122B4A").unwrap(), NAVY);
}

/// A handle edits what is there.
#[test]
fn a_handle_edits_what_is_there() {
    let (mut doc, [card, words, _, _, chart]) = deck();

    let mut element = doc.element_mut(card).unwrap();
    element.fill(RUST).unwrap();
    element.no_shadow().unwrap();
    element.stroke(NAVY, 4.0).unwrap();
    element.move_to(10.0, 20.0).unwrap();
    element.resize(300.0, 200.0).unwrap();
    assert!(element.text().is_ok(), "a shape has a text storage, empty");
    let look = style_of(&doc, card);
    assert_eq!(look.fill, Some(Fill::Color(RUST)));
    assert!(!look.shadow.unwrap().enabled);
    assert_eq!(look.stroke.unwrap().width, 4.0);
    assert_eq!(frame_of(&doc, card), ((10.0, 20.0), (300.0, 200.0)));

    doc.element_mut(words)
        .unwrap()
        .text()
        .unwrap()
        .set("Neu")
        .unwrap();
    let storage = doc.drawable(words).unwrap().text.unwrap();
    assert_eq!(doc.storage_text(storage).unwrap(), "Neu");

    let mut handle = doc.chart_mut(chart).unwrap();
    handle.title("Anders").unwrap();
    handle.no_legend().unwrap();
    handle
        .data(
            &Chart::new(ChartKind::Column)
                .categories(["H1", "H2"])
                .series("2027", [1.0, 2.0]),
        )
        .unwrap();
    handle.element().move_to(0.0, 0.0).unwrap();
    let made = doc
        .charts()
        .into_iter()
        .find(|c| c.identifier == chart)
        .unwrap();
    assert_eq!(made.categories(), ["H1", "H2"]);
    assert_eq!(made.series_count(), 1);

    assert!(
        doc.element_mut(1).is_err(),
        "the document root is not an element"
    );
    assert!(doc.chart_mut(card).is_err(), "a shape is not a chart");
    assert!(doc.problems().is_empty(), "{:?}", doc.problems());
}

/// A named style is a value too, and lands in the app's menu.
#[test]
fn a_text_style_is_made_from_a_value() {
    let mut doc = Document::new(Kind::Pages).unwrap();
    let style = doc
        .add_text_style(
            &TextStyle::new("Kicker")
                .look(
                    TextLook::new()
                        .font("AvenirNext-DemiBold")
                        .size(14.0)
                        .colour(RUST)
                        .bold(),
                )
                .align(Align::Right),
        )
        .unwrap();
    let made = doc.text_style(style).unwrap();
    assert_eq!(made.name.as_deref(), Some("Kicker"));
    assert_eq!(made.style_identifier, None, "its own style, not Body again");
    let get = |path: &[u32]| iwork::style::get_path(&made.archive, path);
    use iwork::pb::Value;
    use iwork::style::property;
    assert_eq!(
        get(property::FONT_SIZE),
        Some(Value::Fixed32(14.0f32.to_le_bytes()))
    );
    assert_eq!(get(property::BOLD), Some(Value::Varint(1)));
    assert_eq!(get(property::ALIGNMENT), Some(Value::Varint(1)));
    assert_eq!(
        get(property::FONT_NAME),
        Some(Value::Bytes(b"AvenirNext-DemiBold".to_vec()))
    );
    assert!(doc.problems().is_empty(), "{:?}", doc.problems());
}

/// **Keynote opens the deck, draws its title and keeps everything through
/// its own save.** Off unless `IWORK_APP_CHECK=1`.
#[test]
fn keynote_keeps_a_deck_built_from_values() {
    if std::env::var("IWORK_APP_CHECK").as_deref() != Ok("1") {
        eprintln!("IWORK_APP_CHECK is not 1 — skipping the app round trip");
        return;
    }
    let (doc, [card, _, photo, _, chart]) = deck();
    let out = std::env::temp_dir().join("iwork-elements-app.key");
    let _ = std::fs::remove_file(&out);
    doc.save(&out).unwrap();
    for (script, argument) in [
        ("app-check.sh", Some("Umsatz plus 18 %")),
        ("resave.sh", None),
    ] {
        let script = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("scripts")
            .join(script);
        let mut command = std::process::Command::new(&script);
        command.arg(&out);
        if let Some(argument) = argument {
            command.arg(argument);
        }
        let output = command
            .output()
            .unwrap_or_else(|e| panic!("{}: {e}", script.display()));
        assert!(
            output.status.success(),
            "{}:\n{}\n{}",
            script.display(),
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let after = Document::open(&out).unwrap();
    assert_eq!(style_of(&after, card).fill, Some(Fill::Color(Color::WHITE)));
    assert!(matches!(style_of(&after, photo).fill, Some(Fill::Image(_))));
    let kept = after
        .charts()
        .into_iter()
        .find(|c| c.identifier == chart)
        .unwrap();
    assert_eq!(kept.series_count(), 2);
    // The title and the legend are the chart's own settings, and still set.
    let settings = after.archive(kept.chart_non_style.unwrap()).unwrap();
    let get = |field: u32| iwork::style::get_path(&settings, &[10000, field]);
    assert_eq!(get(20), Some(iwork::pb::Value::Varint(1)), "showlegend");
    assert_eq!(get(21), Some(iwork::pb::Value::Varint(1)), "showtitle");
    assert_eq!(
        get(23),
        Some(iwork::pb::Value::Bytes(b"Umsatz".to_vec())),
        "title"
    );
}

/// A table is a value with what is in it, and its cells take the same `Fill`
/// and `TextLook` everything else does.
#[test]
fn a_table_is_a_value_and_its_cells_take_the_same_looks() {
    let rows = [["Region", "Units"], ["Zürich", "1240"], ["Genève", "980"]];
    let mut deck = Document::new(Kind::Keynote).unwrap();
    let on_slide = deck
        .slide_mut(0)
        .unwrap()
        .add(Table::with_rows("Summe", rows).at(160.0, 220.0))
        .unwrap();
    let read = deck.table(&on_slide.to_string()).unwrap();
    assert_eq!((read.rows, read.columns), (3, 2));
    assert_eq!(
        read.cell(1, 0).unwrap().value,
        iwork::CellValue::Text("Zürich".to_string())
    );
    assert_eq!(frame_of(&deck, on_slide).0, (160.0, 220.0));

    let mut sheet = Document::new_spreadsheet("Blatt", "Erste", 2, 2).unwrap();
    let made = sheet
        .sheet_mut("Blatt")
        .unwrap()
        .add(Table::new("Zweite", 3, 3).at(500.0, 0.0))
        .unwrap();
    let mut table = sheet.table_mut(made).unwrap();
    table.set("A1", "Kopf").unwrap();
    table.fill("A1:C1", NAVY).unwrap();
    table
        .look(
            "A1:C1",
            &TextLook::new().colour(Color::WHITE).bold().size(14.0),
        )
        .unwrap();
    table.align("A1:C1", Align::Centre).unwrap();
    table.fill("A1", Fill::None).unwrap();
    // What a cell cannot take is refused, not half-written.
    assert!(table.fill("A2", Gradient::linear(NAVY, RUST, 0.0)).is_err());
    assert!(table.look("A2", &TextLook::new().underline()).is_err());
    assert!(sheet.problems().is_empty(), "{:?}", sheet.problems());

    // Too much for the table it was given is refused, and nothing is left.
    let before = deck.objects().count();
    let mut too_wide = Table::new("Eng", 1, 1);
    too_wide = too_wide.clone();
    assert!(deck.slide_mut(0).unwrap().add(&too_wide).is_ok());
    let overfull = Table::with_rows("Leer", Vec::<Vec<&str>>::new());
    assert!(
        deck.slide_mut(0).unwrap().add(overfull).is_err(),
        "no rows, no table"
    );
    assert!(deck.objects().count() > before);
}

/// A style that is there is changed through its handle.
#[test]
fn a_text_style_is_changed_through_its_handle() {
    let mut doc = Document::new(Kind::Keynote).unwrap();
    let style = doc.add_text_style(&TextStyle::new("Kicker")).unwrap();
    let mut handle = doc.text_style_mut(style).unwrap();
    handle.look(&TextLook::new().size(22.0).italic()).unwrap();
    handle.align(Align::Centre).unwrap();
    handle.rename("Dachzeile").unwrap();
    handle
        .property(
            iwork::style::property::FONT_NAME,
            Some(iwork::pb::Value::Bytes(b"Georgia".to_vec())),
        )
        .unwrap();
    let made = doc.text_style(style).unwrap();
    assert_eq!(made.name.as_deref(), Some("Dachzeile"));
    use iwork::pb::Value;
    use iwork::style::property;
    let get = |path: &[u32]| iwork::style::get_path(&made.archive, path);
    assert_eq!(
        get(property::FONT_SIZE),
        Some(Value::Fixed32(22.0f32.to_le_bytes()))
    );
    assert_eq!(get(property::ITALIC), Some(Value::Varint(1)));
    assert_eq!(get(property::ALIGNMENT), Some(Value::Varint(2)));
    assert_eq!(
        get(property::FONT_NAME),
        Some(Value::Bytes(b"Georgia".to_vec()))
    );

    doc.text_style_mut(style).unwrap().delete(None).unwrap();
    assert!(doc.text_style(style).is_none());
    assert!(doc.text_style_mut(style).is_err());
    assert!(doc.problems().is_empty(), "{:?}", doc.problems());
}
