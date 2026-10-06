//! Write a Keynote deck from a slice of Rust data, with no Apple software
//! anywhere in the picture — and make it look like somebody designed it.
//!
//! ```text
//! cargo run --example deck -- Quarter.key
//! open Quarter.key
//! ```
//!
//! Three things do the looking, and each is the same idea: *one* slide, shape
//! or paragraph is given a variation of the style it had — the parent's
//! reference and only what differs — which is how Keynote stores something
//! somebody changed by hand.
//!
//! * a slide's **background** is its slide style's fill, so a painted slide
//!   gets a slide style of its own rather than repainting its layout;
//! * a shape's **fill, outline and opacity** are its object style's, so a
//!   painted shape gets one of its own rather than repainting a theme preset;
//! * a paragraph's **face, size and colour** are a paragraph style's, made
//!   here by copying the deck's one style, `Body`, and changing what differs.
//!
//! **A slide is not a page with a title slot.** What a slide can hold is
//! decided by the *layout* it is built on. A deck made from nothing has one
//! layout, so the words here go in text boxes placed on the slide; start from
//! one of Apple's themes with `Document::from_template` and `slide.title(…)`
//! and `slide.body(…)` write the theme's own placeholders.

use iwork::drawable::{Color, Frame, Outline};
use iwork::pb::Value;
use iwork::style::property;
use iwork::table::CellText;
use iwork::{Document, Kind};

struct Region {
    name: &'static str,
    units: u32,
    revenue: f64,
    /// Share of the quarter's revenue, as a fraction.
    share: f32,
}

const SALES: &[Region] = &[
    Region {
        name: "Zürich",
        units: 1_240,
        revenue: 184_300.0,
        share: 0.46,
    },
    Region {
        name: "Genève",
        units: 980,
        revenue: 151_900.0,
        share: 0.38,
    },
    Region {
        name: "Lugano",
        units: 415,
        revenue: 62_250.0,
        share: 0.16,
    },
];

const NAVY: Color = rgb(0x12, 0x2B, 0x4A);
const SAND: Color = rgb(0xF6, 0xEC, 0xD9);
const RUST: Color = rgb(0xB4, 0x4A, 0x2B);
const INK: Color = rgb(0x1E, 0x1E, 0x1E);
const WHITE: Color = rgb(0xFF, 0xFF, 0xFF);

const fn rgb(red: u8, green: u8, blue: u8) -> Color {
    Color {
        red: red as f32 / 255.0,
        green: green as f32 / 255.0,
        blue: blue as f32 / 255.0,
        alpha: 1.0,
    }
}

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

    // A new deck comes with one slide; one more per region, and a summary.
    let mut doc = Document::new(Kind::Keynote)?;
    for _ in 0..SALES.len() + 1 {
        doc.add_slide(None)?;
    }
    let styles = type_styles(&mut doc)?;

    cover(&mut doc, &styles)?;
    for (index, region) in SALES.iter().enumerate() {
        region_slide(&mut doc, index + 1, region, &styles)?;
    }
    summary(&mut doc, SALES.len() + 1, &styles)?;

    doc.save(&out)?;
    println!("wrote {out} — {} slides", doc.slides().len());
    Ok(())
}

/// A deck made from nothing has one paragraph style to its name. Everything
/// else is a copy of it with the fields that must differ changed.
fn type_styles(doc: &mut Document) -> Result<Type, iwork::Error> {
    let body = doc
        .text_styles()
        .into_iter()
        .find(|style| style.name.as_deref() == Some("Body"))
        .map(|style| style.identifier)
        .ok_or_else(|| iwork::Error::Format("a new deck has a Body style".into()))?;

    let mut make =
        |name: &str, font: &str, size: f32, colour: Color| -> Result<u64, iwork::Error> {
            let style = doc.create_text_style(body, name)?.identifier;
            doc.set_text_style_property(
                style,
                property::FONT_NAME,
                Some(Value::Bytes(font.as_bytes().to_vec())),
            )?;
            doc.set_text_style_property(
                style,
                property::FONT_SIZE,
                Some(Value::Fixed32(size.to_le_bytes())),
            )?;
            // Every place the style keeps its text colour — the app paints with
            // the fill inside the glyphs, not with the font colour.
            doc.set_text_style_color(style, colour.red, colour.green, colour.blue, 1.0)?;
            Ok(style)
        };
    Ok(Type {
        kicker: make("Kicker", "AvenirNext-DemiBold", 22.0, RUST)?,
        title: make("Deck Title", "AvenirNext-Bold", 96.0, WHITE)?,
        figure: make("Figure", "AvenirNext-Bold", 150.0, NAVY)?,
        label: make("Label", "AvenirNext-Regular", 34.0, INK)?,
    })
}

/// Put a line of text on a slide and set it in one of the deck's styles.
fn line(
    doc: &mut Document,
    slide: usize,
    text: &str,
    frame: Frame,
    style: u64,
) -> Result<u64, iwork::Error> {
    let drawable = doc.slide_mut(slide)?.add_text_box(text, frame)?;
    let storage = doc
        .drawable(drawable)
        .and_then(|found| found.text)
        .ok_or_else(|| iwork::Error::Format("a text box has a storage".into()))?;
    let length = text.encode_utf16().count() as u64;
    doc.text_mut(storage)?.style(0..length, style)?;
    Ok(drawable)
}

/// A filled rectangle with no outline — a bar, a rule, a block of colour.
fn block(
    doc: &mut Document,
    slide: usize,
    frame: Frame,
    colour: Color,
) -> Result<u64, iwork::Error> {
    let container = doc.slides()[slide].identifier.to_string();
    let shape = doc.add_shape(
        &container,
        Outline::Rectangle,
        "",
        (frame.x, frame.y),
        (frame.width, frame.height),
    )?;
    // The first paint gives the shape a style of its own; the next two edit it.
    doc.set_object_fill(shape, Some(colour))?;
    doc.set_object_stroke(shape, colour, 0.0)?;
    Ok(shape)
}

fn cover(doc: &mut Document, styles: &Type) -> Result<(), iwork::Error> {
    doc.slide_mut(0)?.background(Some(NAVY))?;
    block(
        doc,
        0,
        Frame {
            x: 120.0,
            y: 420.0,
            width: 180.0,
            height: 10.0,
        },
        RUST,
    )?;
    line(
        doc,
        0,
        "Q3 2026",
        Frame {
            x: 120.0,
            y: 330.0,
            width: 1200.0,
            height: 60.0,
        },
        styles.kicker,
    )?;
    line(
        doc,
        0,
        "Sales by region",
        Frame {
            x: 120.0,
            y: 460.0,
            width: 1680.0,
            height: 260.0,
        },
        styles.title,
    )?;
    let mut slide = doc.slide_mut(0)?;
    slide.notes("Three regions, one slide each, then the table.")?;
    slide.transition("dissolve")?;
    Ok(())
}

fn region_slide(
    doc: &mut Document,
    slide: usize,
    region: &Region,
    styles: &Type,
) -> Result<(), iwork::Error> {
    doc.slide_mut(slide)?.background(Some(SAND))?;
    // A navy band down the left edge, and a bar as long as the region's share.
    block(
        doc,
        slide,
        Frame {
            x: 0.0,
            y: 0.0,
            width: 60.0,
            height: 1080.0,
        },
        NAVY,
    )?;
    block(
        doc,
        slide,
        Frame {
            x: 160.0,
            y: 880.0,
            width: 1600.0,
            height: 26.0,
        },
        WHITE,
    )?;
    let bar = block(
        doc,
        slide,
        Frame {
            x: 160.0,
            y: 880.0,
            width: 1600.0 * region.share,
            height: 26.0,
        },
        RUST,
    )?;
    line(
        doc,
        slide,
        region.name,
        Frame {
            x: 160.0,
            y: 140.0,
            width: 1200.0,
            height: 60.0,
        },
        styles.kicker,
    )?;
    let figure = line(
        doc,
        slide,
        &format!("{}", region.units),
        Frame {
            x: 160.0,
            y: 250.0,
            width: 1600.0,
            height: 300.0,
        },
        styles.figure,
    )?;
    line(
        doc,
        slide,
        &format!(
            "units  ·  CHF {:.0}  ·  {:.0} % of the quarter",
            region.revenue,
            region.share * 100.0
        ),
        Frame {
            x: 160.0,
            y: 620.0,
            width: 1600.0,
            height: 80.0,
        },
        styles.label,
    )?;

    let mut handle = doc.slide_mut(slide)?;
    handle.notes(&format!(
        "{}: {} units, CHF {:.0}.",
        region.name, region.units, region.revenue
    ))?;
    handle.transition("dissolve")?;
    // The figure comes in, then the bar.
    handle.add_build(figure, iwork::keynote::BuildKind::In)?;
    handle.add_build(bar, iwork::keynote::BuildKind::In)?;
    Ok(())
}

fn summary(doc: &mut Document, slide: usize, styles: &Type) -> Result<(), iwork::Error> {
    line(
        doc,
        slide,
        "The quarter",
        Frame {
            x: 160.0,
            y: 120.0,
            width: 1200.0,
            height: 60.0,
        },
        styles.kicker,
    )?;
    // A table is a table wherever it is: the same cells, formats and looks as
    // on a Numbers sheet.
    let table = doc
        .slide_mut(slide)?
        .add_table("Summary", SALES.len() + 2, 3)?;
    let mut cells = doc.table_mut(table)?;
    cells.set_block("A1", &[vec!["Region", "Units", "Revenue"]])?;
    for (index, region) in SALES.iter().enumerate() {
        let row = index + 2;
        cells.set(format!("A{row}"), region.name)?;
        cells.set(format!("B{row}"), region.units)?;
        cells.currency(format!("C{row}"), region.revenue, "CHF")?;
    }
    let total = SALES.len() + 2;
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
    cells.fill("A1:C1", Some(NAVY))?;
    cells.text_look("A1:C1", &CellText::coloured(WHITE))?;
    cells.fill(format!("A{total}:C{total}"), Some(SAND))?;
    cells.text_look(format!("A{total}:C{total}"), &CellText::bold())?;

    doc.slide_mut(slide)?.transition("dissolve")?;
    Ok(())
}
