//! Write a Keynote deck from a slice of Rust data, with no Apple software
//! anywhere in the picture — and make it look like somebody designed it.
//!
//! ```text
//! cargo run --example deck -- Quarter.key
//! open Quarter.key
//! ```
//!
//! **Values to create, handles to edit.** Everything on a slide here is a
//! value — a [`Shape`], a [`TextBox`], a [`Chart`] — that says what it is,
//! where it goes and how it looks, and is handed to `slide.add(…)`. The
//! type is a handful of named paragraph styles, made once.

use iwork::{
    Chart, ChartKind, Color, Document, Gradient, Kind, Shadow, Shape, Table, TextBox, TextLook,
    TextStyle,
};

struct Region {
    name: &'static str,
    units: u32,
    revenue: f64,
    /// Share of the quarter's revenue, as a fraction.
    share: f32,
    /// Units, quarter by quarter.
    history: [f64; 4],
}

const SALES: &[Region] = &[
    Region {
        name: "Zürich",
        units: 1_240,
        revenue: 184_300.0,
        share: 0.46,
        history: [980.0, 1_050.0, 1_170.0, 1_240.0],
    },
    Region {
        name: "Genève",
        units: 980,
        revenue: 151_900.0,
        share: 0.38,
        history: [900.0, 870.0, 940.0, 980.0],
    },
    Region {
        name: "Lugano",
        units: 415,
        revenue: 62_250.0,
        share: 0.16,
        history: [310.0, 350.0, 390.0, 415.0],
    },
];

const NAVY: Color = Color::rgb8(0x12, 0x2B, 0x4A);
const DEEP: Color = Color::rgb8(0x0A, 0x16, 0x2B);
const SAND: Color = Color::rgb8(0xF6, 0xEC, 0xD9);
const RUST: Color = Color::rgb8(0xB4, 0x4A, 0x2B);
const INK: Color = Color::rgb8(0x1E, 0x1E, 0x1E);

/// The paragraph styles the deck is set in, by role.
struct Type {
    kicker: u64,
    title: u64,
    figure: u64,
    label: u64,
}

fn main() -> Result<(), iwork::Error> {
    let out = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "Quarter.key".to_string());

    // A new deck comes with one slide; one more per region, and two to close.
    let mut doc = Document::new(Kind::Keynote)?;
    for _ in 0..SALES.len() + 2 {
        doc.add_slide(None)?;
    }
    let style = |name: &str, font: &str, size: f32, colour: Color| {
        TextStyle::new(name).look(TextLook::new().font(font).size(size).colour(colour))
    };
    let styles = Type {
        kicker: doc.add_text_style(&style("Kicker", "AvenirNext-DemiBold", 28.0, RUST))?,
        title: doc.add_text_style(&style("Deck Title", "AvenirNext-Bold", 96.0, Color::WHITE))?,
        figure: doc.add_text_style(&style("Figure", "AvenirNext-Bold", 150.0, NAVY))?,
        label: doc.add_text_style(&style("Label", "AvenirNext-Regular", 34.0, INK))?,
    };

    cover(&mut doc, &styles)?;
    for (index, region) in SALES.iter().enumerate() {
        region_slide(&mut doc, index + 1, region, &styles)?;
    }
    trend(&mut doc, SALES.len() + 1, &styles)?;
    summary(&mut doc, SALES.len() + 2, &styles)?;

    doc.save(&out)?;
    println!("wrote {out} — {} slides", doc.slides().len());
    Ok(())
}

/// A filled rectangle with no outline — a bar, a rule, a block of colour.
fn block(colour: Color) -> Shape {
    Shape::rectangle().fill(colour).no_stroke()
}

fn cover(doc: &mut Document, styles: &Type) -> Result<(), iwork::Error> {
    let mut slide = doc.slide_mut(0)?;
    slide.background(Gradient::linear(NAVY, DEEP, 270.0))?;
    slide.add(block(RUST).at(120.0, 420.0).size(180.0, 10.0))?;
    slide.add(
        TextBox::new("Q3 2026")
            .at(120.0, 340.0)
            .size(1200.0, 60.0)
            .style(styles.kicker),
    )?;
    slide.add(
        TextBox::new("Sales by region")
            .at(120.0, 460.0)
            .size(1680.0, 260.0)
            .style(styles.title),
    )?;
    slide.notes("Three regions, one slide each, then the trend and the table.")?;
    slide.transition("dissolve")?;
    Ok(())
}

fn region_slide(
    doc: &mut Document,
    index: usize,
    region: &Region,
    styles: &Type,
) -> Result<(), iwork::Error> {
    let mut slide = doc.slide_mut(index)?;
    slide.background(SAND)?;
    // A navy band down the left edge, and a bar as long as the region's share.
    slide.add(block(NAVY).at(0.0, 0.0).size(60.0, 1080.0))?;
    slide.add(block(Color::WHITE).at(160.0, 880.0).size(1600.0, 26.0))?;
    let bar = slide.add(
        block(RUST)
            .at(160.0, 880.0)
            .size(1600.0 * region.share, 26.0),
    )?;
    slide.add(
        TextBox::new(region.name)
            .at(160.0, 140.0)
            .size(1200.0, 60.0)
            .style(styles.kicker),
    )?;
    let figure = slide.add(
        TextBox::new(region.units.to_string())
            .at(160.0, 250.0)
            .size(1600.0, 300.0)
            .style(styles.figure),
    )?;
    // One line, and the share in it set apart: a run with a look of its own.
    let share = format!("{:.0} %", region.share * 100.0);
    let line = format!(
        "units  ·  CHF {:.0}  ·  {share} of the quarter",
        region.revenue
    );
    let from = line
        .find(&share)
        .map_or(0, |at| line[..at].encode_utf16().count()) as u64;
    let to = from + share.encode_utf16().count() as u64;
    slide.add(
        TextBox::new(line)
            .at(160.0, 620.0)
            .size(1600.0, 80.0)
            .style(styles.label)
            .format(from..to, TextLook::new().bold().colour(RUST)),
    )?;

    slide.notes(&format!(
        "{}: {} units, CHF {:.0}.",
        region.name, region.units, region.revenue
    ))?;
    slide.transition("dissolve")?;
    // The figure comes in, then the bar.
    slide.add_build(figure, iwork::keynote::BuildKind::In)?;
    slide.add_build(bar, iwork::keynote::BuildKind::In)?;
    Ok(())
}

fn trend(doc: &mut Document, index: usize, styles: &Type) -> Result<(), iwork::Error> {
    let mut slide = doc.slide_mut(index)?;
    slide.add(
        TextBox::new("Four quarters")
            .at(160.0, 100.0)
            .size(1200.0, 60.0)
            .style(styles.kicker),
    )?;
    // A card with a shadow under it, and a chart on the card.
    slide.add(
        block(Color::WHITE)
            .at(160.0, 200.0)
            .size(1600.0, 760.0)
            .shadow(Shadow::default()),
    )?;
    let mut chart = Chart::new(ChartKind::Line)
        .at(260.0, 330.0)
        .size(1400.0, 560.0)
        .categories(["Q4", "Q1", "Q2", "Q3"])
        .title("Units sold")
        .legend();
    for region in SALES {
        chart = chart.series(region.name, region.history);
    }
    slide.add(chart)?;
    slide.transition("dissolve")?;
    Ok(())
}

fn summary(doc: &mut Document, index: usize, styles: &Type) -> Result<(), iwork::Error> {
    let mut slide = doc.slide_mut(index)?;
    slide.add(
        TextBox::new("The quarter")
            .at(160.0, 100.0)
            .size(1200.0, 60.0)
            .style(styles.kicker),
    )?;
    // A table is a table wherever it is: the same cells, formats and looks as
    // on a Numbers sheet.
    let table = slide.add(Table::new("Summary", SALES.len() + 2, 3).at(160.0, 220.0))?;
    slide.transition("dissolve")?;

    let total = SALES.len() + 2;
    let mut cells = doc.table_mut(table)?;
    cells.set_block("A1", &[vec!["Region", "Units", "Revenue"]])?;
    for (index, region) in SALES.iter().enumerate() {
        let row = index + 2;
        cells.set(format!("A{row}"), region.name)?;
        cells.set(format!("B{row}"), region.units)?;
        cells.currency(format!("C{row}"), region.revenue, "CHF")?;
    }
    cells.set(format!("A{total}"), "Total")?;
    cells.set(
        format!("B{total}"),
        SALES.iter().map(|region| region.units).sum::<u32>(),
    )?;
    cells.currency(
        format!("C{total}"),
        SALES.iter().map(|region| region.revenue).sum::<f64>(),
        "CHF",
    )?;
    // Big enough to read from the back of the room.
    let everything = format!("A1:C{total}");
    cells.look(everything, &TextLook::new().size(30.0))?;
    for column in 0..3 {
        cells.column_width(column, Some(520.0))?;
    }
    for row in 0..total {
        cells.row_height(row, Some(80.0))?;
    }
    cells.fill("A1:C1", NAVY)?;
    cells.look("A1:C1", &TextLook::new().colour(Color::WHITE))?;
    cells.fill(format!("A{total}:C{total}"), SAND)?;
    cells.look(format!("A{total}:C{total}"), &TextLook::new().bold())?;
    Ok(())
}
