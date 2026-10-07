# Cookbook — a document that looks designed

Short recipes for writing Numbers, Keynote and Pages documents with this
crate, each one **run** by `cargo test --doc`. The README says how the format
works and what was measured; this says what to type.

**Use 0.2.4 or later.** Earlier versions wrote documents with no way to give
anything a look, and 0.2.0 wrote a deck Keynote aborts on if it used one
picture twice.

## The one idea

Nothing in an iWork document carries its own colour or weight. A cell, a shape,
a slide and a paragraph each *name a style*, and usually share it with
everything else of their kind. To change one of them, this crate does what the
apps do: it gives that one thing a **variation** — a style of its own that
names the shared one as its parent and holds only what differs.

So the calls below take the *thing* (a cell range, a drawable, a slide), never
a style identifier, and you never have to manage the variations yourself.
Paragraph styles are the exception: you make them once, by name, and apply
them to ranges of text.

## Numbers: a table with a header, money, a total

```
# std::env::set_current_dir(std::env::temp_dir()).unwrap();
use iwork::drawable::Color;
use iwork::table::{CellText, Format};
use iwork::Document;

const NAVY: Color = Color { red: 0.07, green: 0.17, blue: 0.29, alpha: 1.0 };
const SAND: Color = Color { red: 0.96, green: 0.93, blue: 0.85, alpha: 1.0 };
const WHITE: Color = Color { red: 1.0, green: 1.0, blue: 1.0, alpha: 1.0 };

// Sheet name, table name, rows, columns. Row 1 is a header row.
let mut doc = Document::new_spreadsheet("Sales", "Q3", 4, 3)?;
let mut t = doc.table_mut("Q3")?;

t.set_block("A1", &[vec!["Region", "Units", "Revenue"]])?;
t.set("A2", "Zürich")?;
t.set("B2", 1_240)?;
t.currency("C2", 184_300.0, "CHF")?;        // money is a value type, not a format
t.set("A3", "Genève")?;
t.set("B3", 980)?;
t.currency("C3", 151_900.0, "CHF")?;

// A formula needs the value it shows until the app recalculates.
t.set("A4", "Total")?;
t.formula("B4", "=SUM(B2:B3)", 2_220)?;
// A format goes on cells that hold something: formatting an empty cell is
// refused, because a format on nothing is not a thing the app writes.
t.format("B2:B4", &Format::Number { decimals: Some(0) })?;

// The look: ranges, and empty cells can be painted too (C4 is one).
t.fill("A1:C1", Some(NAVY))?;
t.text_look("A1:C1", &CellText { colour: Some(WHITE), ..CellText::bold() })?;
t.fill("A4:C4", Some(SAND))?;
t.text_look("A4:C4", &CellText::bold())?;
t.column_width(0, Some(160.0))?;

doc.save("Sales.numbers")?;
# Ok::<(), iwork::Error>(())
```

A sheet is a canvas, not a grid: `doc.add_table_at("Sales", "Notes", 3, 2,
(520.0, 0.0))` puts a second table beside the first, and `doc.add_sheet(…)`
adds another sheet.

## Keynote: a slide with a background, a title, a shape

```
# std::env::set_current_dir(std::env::temp_dir()).unwrap();
use iwork::drawable::{Color, Frame, Outline};
use iwork::pb::Value;
use iwork::style::property;
use iwork::{Document, Kind};

const NAVY: Color = Color { red: 0.07, green: 0.17, blue: 0.29, alpha: 1.0 };
const RUST: Color = Color { red: 0.71, green: 0.29, blue: 0.17, alpha: 1.0 };

let mut doc = Document::new(Kind::Keynote)?;     // one slide, 1920 × 1080
doc.add_slide(None)?;                            // a second

// A paragraph style, made once by copying the deck's only one.
let body = doc
    .text_styles()
    .into_iter()
    .find(|s| s.name.as_deref() == Some("Body"))
    .expect("a new deck has Body")
    .identifier;
let title = doc.create_text_style(body, "Big Title")?.identifier;
doc.set_text_style_property(
    title,
    property::FONT_NAME,
    Some(Value::Bytes(b"AvenirNext-Bold".to_vec())),
)?;
doc.set_text_style_property(title, property::FONT_SIZE, Some(Value::Fixed32(96.0f32.to_le_bytes())))?;
doc.set_text_style_color(title, 1.0, 1.0, 1.0, 1.0)?;   // white, everywhere the style keeps it

// Slide 0: a background, a text box set in that style.
doc.slide_mut(0)?.background(Some(NAVY))?;
let text = "Sales by region";
let text_box = doc.slide_mut(0)?.add_text_box(
    text,
    Frame { x: 120.0, y: 440.0, width: 1680.0, height: 240.0 },
)?;
let storage = doc.drawable(text_box).and_then(|d| d.text).expect("a text box has text");
doc.text_mut(storage)?.style(0..text.encode_utf16().count() as u64, title)?;

// A shape, painted. The identifier is the drawable's, not a style's.
let slide = doc.slides()[0].identifier.to_string();
let bar = doc.add_shape(&slide, Outline::Rectangle, "", (120.0, 400.0), (180.0, 10.0))?;
doc.set_object_fill(bar, Some(RUST))?;
doc.set_object_stroke(bar, RUST, 0.0)?;
doc.set_object_opacity(bar, 0.9)?;

doc.slide_mut(0)?.notes("Presenter notes go here.")?;
doc.slide_mut(0)?.transition("dissolve")?;
doc.save("Talk.key")?;
# Ok::<(), iwork::Error>(())
```

Things worth knowing:

- **Text ranges are UTF-16 code units**, and a paragraph style applies to
  whole paragraphs — the range grows to the paragraphs it touches.
- **`slide.title(…)` and `slide.body(…)` write a layout's placeholders.** A
  deck from nothing has one plain layout, a title over a body; start from one
  of Apple's themes with `Document::from_template("…/Wide.kth")` to get
  layouts worth the name.
- **A table on a slide** is `slide.add_table("Name", rows, columns)?`, which
  returns an identifier `doc.table_mut(id)?` takes — and then everything in the
  Numbers recipe applies to it.
- **The same picture on many slides** is fine: `add_image` stores it once.

## Keynote: emphasis, gradients, shadows, pictures, a chart

```
# std::env::set_current_dir(std::env::temp_dir()).unwrap();
use iwork::chart::{ChartData, ChartKind};
use iwork::drawable::{Color, Frame, Gradient, ImageFit, Outline, Shadow};
use iwork::text::TextLook;
use iwork::{Document, Kind};

const NAVY: Color = Color { red: 0.07, green: 0.17, blue: 0.29, alpha: 1.0 };
const TEAL: Color = Color { red: 0.12, green: 0.54, blue: 0.49, alpha: 1.0 };
const RUST: Color = Color { red: 0.71, green: 0.29, blue: 0.17, alpha: 1.0 };
const WHITE: Color = Color { red: 1.0, green: 1.0, blue: 1.0, alpha: 1.0 };

let mut doc = Document::new(Kind::Keynote)?;
doc.add_slide(None)?;
let first = doc.slides()[0].identifier.to_string();
let second = doc.slides()[1].identifier.to_string();

// A background that runs from navy at the top to teal at the bottom. The
// angle is the one Keynote's inspector shows.
doc.slide_mut(0)?.background_gradient(&Gradient::linear(NAVY, TEAL, 270.0))?;

// One sentence, three looks: ranges are UTF-16 code units, and each run gets
// only what its `TextLook` sets.
let words = "Revenue grew 18 % on last year";
let text_box = doc.slide_mut(0)?.add_text_box(
    words,
    Frame { x: 120.0, y: 120.0, width: 1500.0, height: 120.0 },
)?;
let storage = doc.drawable(text_box).and_then(|d| d.text).expect("a text box has text");
let mut text = doc.text_mut(storage)?;
text.format(0..30, &TextLook { size: Some(64.0), colour: Some(WHITE), ..TextLook::default() })?;
text.format(13..17, &TextLook { bold: Some(true), colour: Some(RUST), ..TextLook::default() })?;

// A card: white, no outline, a soft shadow under it.
let card = doc.add_shape(&first, Outline::Rectangle, "", (120.0, 320.0), (760.0, 520.0))?;
doc.set_object_fill(card, Some(WHITE))?;
doc.set_object_stroke(card, WHITE, 0.0)?;
doc.set_object_shadow(card, Some(Shadow { offset: 14.0, radius: 30, ..Shadow::default() }))?;

// A shape filled with a gradient of its own, three stops.
let band = doc.add_shape(&first, Outline::Ellipse, "", (1000.0, 320.0), (520.0, 520.0))?;
doc.set_object_gradient(
    band,
    &Gradient { stops: vec![(RUST, 0.0), (WHITE, 0.5), (TEAL, 1.0)], angle: 45.0 },
)?;

// A picture as a fill: PNG or JPEG bytes, cropped by the shape it fills.
# let picture: Vec<u8> = {
#     // A 1 × 1 PNG.
#     let hex = "89504e470d0a1a0a0000000d4948445200000001000000010802000000907753de0000000c4944415408d763f8cfc000000301010018dd8db00000000049454e44ae426082";
#     (0..hex.len()).step_by(2).map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap()).collect()
# };
let photo = doc.add_shape(&first, Outline::Ellipse, "", (1560.0, 120.0), (240.0, 240.0))?;
doc.set_object_image_fill(photo, &picture, "portrait.png", ImageFit::ScaleToFill)?;

// A chart from data: each row a series, each column a category.
let data = ChartData::numbers(
    &["2025", "2026"],
    &["Q1", "Q2", "Q3", "Q4"],
    &[&[31.0, 35.5, 40.8, 42.0], &[40.1, 44.5, 48.2, 51.0]],
);
doc.new_chart(&second, ChartKind::Column, &data, (260.0, 140.0), (1400.0, 800.0))?;

doc.save("Looks.key")?;
# Ok::<(), iwork::Error>(())
```

- **A chart is a real chart**, editable in the app: column, bar, line, area,
  pie and the stacked three. On a deck made from one of Apple's themes it
  takes the theme's look; on one made from nothing it brings the look of
  Keynote's white theme with it. It has no legend until somebody turns one on
  in the app.
- **`format` works in Pages too** — `doc.body_mut()?.format(range, &look)` —
  and on any text storage of a document the apps made.
- **Gradients are linear.** Shadows are drop shadows; `set_object_shadow(id,
  None)` switches one off.

## Pages: a title, a heading and body text

```
# std::env::set_current_dir(std::env::temp_dir()).unwrap();
use iwork::pb::Value;
use iwork::style::property;
use iwork::{Document, Kind};

let mut doc = Document::new(Kind::Pages)?;
let body = doc
    .text_styles()
    .into_iter()
    .find(|s| s.name.as_deref() == Some("Body"))
    .expect("a blank document has Body")
    .identifier;

// A style per role, each a copy of Body with what differs changed.
let mut make = |name: &str, font: &str, size: f32| -> Result<u64, iwork::Error> {
    let style = doc.create_text_style(body, name)?.identifier;
    doc.set_text_style_property(style, property::FONT_NAME, Some(Value::Bytes(font.as_bytes().to_vec())))?;
    doc.set_text_style_property(style, property::FONT_SIZE, Some(Value::Fixed32(size.to_le_bytes())))?;
    Ok(style)
};
let title = make("Report Title", "AvenirNext-Bold", 28.0)?;
let heading = make("Section", "AvenirNext-DemiBold", 14.0)?;

// The text first — one paragraph per line — then a style per paragraph.
let lines = [
    ("Rolling stock availability", Some(title)),
    ("Where the quarter stands", Some(heading)),
    ("Availability finished the quarter at 91.4 per cent.", None),
];
let text: Vec<&str> = lines.iter().map(|(line, _)| *line).collect();
doc.body_mut()?.set(&text.join("\n"))?;

let mut at = 0u64;
for (line, style) in lines {
    let length = line.encode_utf16().count() as u64;
    if let Some(style) = style {
        doc.body_mut()?.style(at..at + length, style)?;
    }
    at += length + 1;                            // the newline
}
doc.save("Report.pages")?;
# Ok::<(), iwork::Error>(())
```

A blank Pages document has exactly one paragraph style to copy, `Body`, and
no list styles — so real bullet lists are not available on a document made
from nothing. Bold *words* are: `doc.body_mut()?.format(range,
&TextLook::bold())`. Open a document Pages made (`Document::open`) and its
Title, Heading and list styles are all there to apply.

## From the shell

The same looks, for something that drives the `iwork` binary instead of
linking the crate:

```text
iwork create numbers Sales.numbers
iwork set-cell  Sales.numbers "Table 1" A1 Region out.numbers
iwork fill      out.numbers "Table 1" A1:C1 '#122B4A' out.numbers
iwork text-look out.numbers "Table 1" A1:C1 bold color=#FFFFFF size=13 out.numbers

iwork create keynote Talk.key
iwork background Talk.key 1 '#122B4A' Talk.key          # slide 1
iwork add-shape  Talk.key <slide id> ellipse "Q3" 200 200 300 300 Talk.key
iwork paint      Talk.key <drawable id> fill=#B44A2B stroke=#FFFFFF:4 opacity=0.9 Talk.key
```

`iwork slides`, `iwork tables` and `iwork drawables` print the identifiers.

## Before you trust the file

1. `iwork check file` — it knows the invariants that were learned the hard way.
2. **Open it in the app.** The checker is sharper every time something gets
   past it and is still not a substitute; `scripts/app-check.sh file` does it
   from the shell.
3. If the app refuses, crashes, or quietly changes what you wrote, that is a
   bug in this crate and worth a report with the smallest program that does it.

Refusals are values. A call that would have to guess returns
`Error::Refused { reason, detail }` instead, and `error.refusal()` gives the
reason as an enum to match on — `Merged`, `OutOfBounds`, `HoldsFormula` and the
rest.
