# Cookbook — a document that looks designed

Short recipes for writing Numbers, Keynote and Pages documents with this
crate, each one **run** by `cargo test --doc`. The README says how the format
works and what was measured; this says what to type.

**This is the 0.3 API.** Earlier versions wrote documents with no way to give
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

## Keynote: a deck

Everything on a slide is a **value** — a `Shape`, a `TextBox`, an `Image`, a
`Chart` — that says what it is, where it goes and how it looks, and is handed
to `slide.add(…)`. Building one cannot fail; `add` can, and a refused `add`
leaves the document as it was.

```
# std::env::set_current_dir(std::env::temp_dir()).unwrap();
use iwork::{
    Align, Chart, ChartKind, Color, Document, Fill, Gradient, Kind, Shadow, Shape, TextBox,
    TextLook, TextStyle,
};

const NAVY: Color = Color::rgb8(0x12, 0x2B, 0x4A);
const TEAL: Color = Color::rgb8(0x1F, 0x8A, 0x7E);
const RUST: Color = Color::rgb8(0xB4, 0x4A, 0x2B);

let mut doc = Document::new(Kind::Keynote)?;     // one slide, 1920 × 1080
doc.add_slide(None)?;                            // a second

// Named paragraph styles, made once. They show up in Keynote's style menu.
let title = doc.add_text_style(
    &TextStyle::new("Big Title")
        .look(TextLook::new().font("AvenirNext-Bold").size(72.0).colour(Color::WHITE)),
)?;
let caption = doc.add_text_style(
    &TextStyle::new("Caption")
        .look(TextLook::new().size(28.0).colour(Color::WHITE))
        .align(Align::Right),
)?;

let mut slide = doc.slide_mut(0)?;
slide.background(Gradient::linear(NAVY, TEAL, 270.0))?;     // or a plain Color

// Text in a named style, with one run set apart. Ranges are UTF-16 units.
slide.add(
    TextBox::new("Revenue grew 18 % on last year")
        .at(120.0, 120.0)
        .size(1680.0, 110.0)
        .style(title)
        .format(13..17, TextLook::new().colour(RUST)),
)?;
slide.add(TextBox::new("Q3 2026").at(120.0, 940.0).size(1680.0, 50.0).style(caption))?;

// A card: white, no outline, a soft shadow under it.
slide.add(
    Shape::rectangle()
        .at(120.0, 320.0)
        .size(760.0, 520.0)
        .fill(Color::WHITE)
        .no_stroke()
        .shadow(Shadow::default()),
)?;

// A value is a value: keep it, change a copy, add both.
let dot = Shape::ellipse().size(240.0, 240.0).no_stroke();
slide.add(dot.clone().at(1000.0, 320.0).fill(Gradient::linear(RUST, Color::WHITE, 45.0)))?;
# let picture: Vec<u8> = {
#     let hex = "89504e470d0a1a0a0000000d4948445200000001000000010802000000907753de0000000c4944415408d763f8cfc000000301010018dd8db00000000049454e44ae426082";
#     (0..hex.len()).step_by(2).map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap()).collect()
# };
// A picture as a fill — PNG or JPEG bytes — cropped by the shape it fills.
let photo = slide.add(dot.at(1300.0, 320.0).fill(Fill::image(picture, "portrait.png")))?;

slide.notes("Presenter notes go here.")?;
slide.transition("dissolve")?;

// A chart from data: one series per thing compared, a value per category.
let chart = doc.slide_mut(1)?.add(
    Chart::new(ChartKind::Column)
        .at(260.0, 200.0)
        .size(1400.0, 700.0)
        .categories(["Q1", "Q2", "Q3", "Q4"])
        .series("2025", [31.0, 35.5, 40.8, 42.0])
        .series("2026", [40.1, 44.5, 48.2, 51.0])
        .title("Revenue by quarter")
        .legend(),
)?;

// Handles edit what is there: `add` returned the identifiers.
doc.element_mut(photo)?.stroke(Color::WHITE, 6.0)?;
doc.chart_mut(chart)?.title("Revenue, CHF m")?;

doc.save("Talk.key")?;
# Ok::<(), iwork::Error>(())
```

Things worth knowing:

- **`.at(x, y)` and `.size(w, h)` are properties of the value.** Left out,
  the position is the slide's origin and the size is the thing's own: an
  image its pixels, a chart 800 × 500, a shape 200 × 200.
- **`Fill` is one type**: a `Color`, a `Gradient`, `Fill::image(bytes, name)`
  or `Fill::None`, for a shape and for a slide's background alike.
  Gradients are linear; the angle is the one Keynote's inspector shows.
- **A chart is a real chart**, editable in the app: column, bar, line, area,
  pie and the stacked three. On a deck made from one of Apple's themes it
  takes the theme's look. The legend goes where Keynote puts it.
- **`slide.title(…)` and `slide.body(…)` write a layout's placeholders.** A
  deck from nothing has one plain layout, a title over a body; start from one
  of Apple's themes with `Document::from_template("…/Wide.kth")` to get
  layouts worth the name.
- **A table** is `slide.add(iwork::element::Table::new("Name", rows,
  columns).at(x, y))?`, which returns an identifier `doc.table_mut(id)?`
  takes — and then everything in the Numbers recipe applies to it.
- **The same values go on a Numbers sheet and a Pages page**:
  `doc.sheet_mut("Sheet 1")?.add(…)`, `doc.page_mut(1)?.add(…)`.
- **The same picture on many slides** is fine: it is stored once.

## Pages: a title, a heading and body text

```
# std::env::set_current_dir(std::env::temp_dir()).unwrap();
use iwork::{Document, Kind, TextLook, TextStyle};

let mut doc = Document::new(Kind::Pages)?;

// A style per role.
let title = doc.add_text_style(
    &TextStyle::new("Report Title").look(TextLook::new().font("AvenirNext-Bold").size(28.0)),
)?;
let heading = doc.add_text_style(
    &TextStyle::new("Section").look(TextLook::new().font("AvenirNext-DemiBold").size(14.0)),
)?;

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
// One word in the last line, bold.
doc.body_mut()?.format(at - 9..at - 1, &TextLook::new().bold())?;
doc.save("Report.pages")?;
# Ok::<(), iwork::Error>(())
```

A blank Pages document has exactly one paragraph style to copy, `Body`, and
no list styles — so real bullet lists are not available on a document made
from nothing. Bold *words* are, as above. Open a document Pages made (`Document::open`) and its
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
