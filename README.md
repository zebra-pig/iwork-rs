# iwork-rs

Read and write Apple iWork documents — **Pages**, **Numbers** and **Keynote** —
from Rust, on any platform, without Apple software.

- Create new documents from nothing, or from an iWork template.
- Read text, tables, cells, formulas, charts, slides, styles, comments and media.
- Edit existing documents: text, cells, formulas, formats, colours, shapes,
  images, charts, slides, transitions.
- Saving only rewrites what you changed; everything else is kept byte for byte,
  including parts of the document this crate doesn't understand.

```
cargo add iwork
```

The crate also ships a command-line tool, `iwork` — see [CLI](#cli).

## Quick start

### Numbers

```rust
use iwork::table::Format;
use iwork::{Color, Document, TextLook};

// Sheet name, table name, rows, columns.
let mut doc = Document::new_spreadsheet("Sales", "Q1", 4, 2)?;
let mut q1 = doc.table_mut("Q1")?;

q1.set_block("A1", &[vec!["Region", "Units"]])?;
q1.set("A2", "Zürich")?;
q1.set("B2", 1_240)?;
q1.set("A3", "Genève")?;
q1.set("B3", 980)?;

// A real formula — Numbers recalculates it. The value is what the cell shows
// until it does: this crate writes formulas but does not evaluate them.
q1.formula("B4", "=SUM(B2:B3)", 2_220)?;
q1.format("B2:B4", &Format::Number { decimals: Some(0) })?;

q1.fill("A1:B1", Color::rgb8(0x12, 0x2B, 0x4A))?;
q1.look("A1:B1", &TextLook::new().colour(Color::WHITE).bold())?;
q1.column_width(0, Some(140.0))?;

doc.save("Sales.numbers")?;
```

### Keynote

```rust
use iwork::{Chart, ChartKind, Color, Document, Kind, Shape, TextBox};

let mut doc = Document::new(Kind::Keynote)?;     // one slide, 1920 × 1080
doc.add_slide(None)?;

let mut slide = doc.slide_mut(0)?;
slide.background(Color::rgb8(0x12, 0x2B, 0x4A))?;
slide.add(TextBox::new("Quarterly review").at(120.0, 120.0).size(1680.0, 110.0))?;
slide.add(Shape::ellipse().at(1000.0, 320.0).size(240.0, 240.0).fill(Color::WHITE))?;
slide.notes("Presenter notes go here.")?;
slide.transition("dissolve")?;

doc.slide_mut(1)?.add(
    Chart::new(ChartKind::Column)
        .at(260.0, 200.0)
        .size(1400.0, 700.0)
        .categories(["Q1", "Q2", "Q3", "Q4"])
        .series("2025", [31.0, 35.5, 40.8, 42.0])
        .series("2026", [40.1, 44.5, 48.2, 51.0])
        .legend(),
)?;

doc.save("Talk.key")?;
```

### Pages

```rust
use iwork::{Document, Kind, TextLook, TextStyle};

let mut doc = Document::new(Kind::Pages)?;
let heading = doc.add_text_style(
    &TextStyle::new("Heading").look(TextLook::new().font("AvenirNext-Bold").size(20.0)),
)?;

let mut body = doc.body_mut()?;
body.set("Status report\nEverything is on schedule.")?;
body.style(0..13, heading)?;                       // ranges are UTF-16 code units
body.format(28..39, &TextLook::new().bold())?;  // "on schedule"

doc.save("Report.pages")?;
```

[`COOKBOOK.md`](COOKBOOK.md) has longer recipes for all three apps — styled
tables, gradients, shadows, images, named styles — and every one of them is
compiled and run by the test suite. [`API.md`](API.md) explains the shape of
the API.

## Opening and reading documents

```rust
let doc = iwork::Document::open("Budget.numbers")?;   // .pages, .numbers or .key
println!("{} document", doc.kind().as_str());

// Tables — in Numbers, and also on Pages pages and Keynote slides.
for table in doc.tables() {
    println!("{} ({}×{}) on {:?}", table.name, table.rows, table.columns, table.sheet);
    for cell in table.cells() {
        println!("  r{} c{}: {}", cell.row, cell.column, cell.value.to_text());
    }
}

// Text, wherever it lives: body, text boxes, shapes, headers, notes.
for storage in doc.text_storages() {
    println!("{}: {}", storage.identifier, storage.text);
}

// Charts, with the data they draw — and in Numbers, the table ranges they follow.
for chart in doc.charts() {
    for series in chart.series() {
        println!("{:?}: {:?}", series.name, series.values);
    }
}
```

Other readers on `Document` include `sheets()`, `text_styles()`,
`annotations()` (comments and tracked changes), slide and layout information,
drawables and media. See the [API docs](https://docs.rs/iwork).

## Editing existing documents

Open, change, save. Things are addressed by what they are — a table by name, a
slide by position, a cell by `"B3"` — not by internal object ids.

```rust
let mut doc = iwork::Document::open("Report.pages")?;

let mut body = doc.body_mut()?;
body.append("A new paragraph")?;
body.replace(40..55, "different words")?;

doc.table_mut("Prices")?.set("B2", 49.90)?;

doc.save("Report-edited.pages")?;
```

Text edits keep everything anchored in the text in place: style runs, links,
comments, inline images.

### Tables

In Numbers, a sheet is a canvas that can hold several tables, charts, shapes
and images. A table is named by its name if that is unique, by a
`("Sheet", "Table")` pair, or by its identifier; an ambiguous name is refused
rather than guessed. Writing many cells at once (`set_block`, `set_cells`) is much faster
than a loop of `set` calls, and is all-or-nothing.

Rows and columns can be inserted, deleted and resized; cells can be merged and
unmerged; values can be numbers, text, currency, dates and formulas.

### Templates

```rust
// A new document from one of Apple's templates (or a .kth Keynote theme),
// with a new document identity of its own.
let doc = iwork::Document::from_template(
    "/Applications/Pages.app/Contents/SharedSupport/Templates/08_Journal_Newsletter/ISO.template",
)?;
doc.save("Newsletter.pages")?;
```

`doc.save_as_new(path)` saves a copy with a fresh identity, as the apps' own
Save As does. `Package::write_as` converts between single-file and package
(directory) form; otherwise a document keeps the form it was opened in.

### Errors and refusals

When an edit can't be done safely, it is **refused** rather than guessed at,
and the document is left unchanged. Each refusal carries a reason you can
match on:

```rust
use iwork::Refusal;

match table.set(cell, value) {
    Err(e) if e.refusal() == Some(Refusal::Merged) => continue,      // covered by a merge
    Err(e) if e.refusal() == Some(Refusal::OutOfBounds) => grow()?,  // table too small
    Err(e) if e.refusal() == Some(Refusal::HoldsFormula) => {}       // leave it
    other => other?,
}
```

The error's `Display` text explains what was refused and why.

## CLI

```
cargo install iwork
```

Every command that changes a document takes an output path as its last
argument. Identifiers printed by the listing commands (`text`, `tables`,
`slides`, `drawables`, `styles`, …) are what the editing commands take.

**Create and inspect**

```
iwork create pages|numbers|keynote out.pages   # a new, empty document
iwork new <template> out.pages                 # a document from a template
iwork inspect     file                         # package overview
iwork check       file                         # look for structural problems
iwork metadata    file                         # identity, locale, template, history
iwork extract     file ./media                 # embedded media files
iwork duplicate   file copy.pages              # a copy with a new identity
iwork strip-previews file out                  # drop the (stale) thumbnails
```

**Text and styles**

```
iwork text        file                         # every text storage, with its id
iwork links       file                         # hyperlinks and smart fields
iwork paragraphs  file <storage>               # paragraph ranges, styles, list levels
iwork set-text    file <storage> "…" out
iwork insert-text file <storage> <at> "…" out
iwork delete-text file <storage> <from> <to> out
iwork styles      file                         # every text style
iwork apply-style file <storage> <from> <to> <style> out
iwork new-style   file <style> <name> out      # a copy of a style, under a new name
iwork annotations file                         # comments and tracked changes
iwork add-comment file <storage> <from> <to> <author> "…" out
```

**Tables (Numbers, and tables in Pages and Keynote)**

```
iwork tables      file                         # every table
iwork cells       file <table>                 # every cell, its type and format
iwork csv         file <table>                 # one table as CSV
iwork formulas    file                         # every formula and its cached value
iwork set-cell    file <table> B3 n:43 out
iwork set-cells   file <table> A2 rows.csv out
iwork set-formula file <table> C9 "=SUM(B2:B8)" n:1234 out
iwork set-format  file <table> B3 percent:1 out
iwork set-width   file <table> <column> <points> out
iwork set-height  file <table> <row> <points> out
iwork insert-row | delete-row | insert-column | delete-column  file <table> <index> out
iwork merge       file <table> B2 <rows> <columns> out
iwork unmerge     file <table> B2 out
iwork fill        file <table> A1:D1 '#122B4A' out
iwork text-look   file <table> A1:D1 bold color=#FFFFFF size=13 out
```

**Objects, media and charts**

```
iwork drawables   file                         # every placed object
iwork media       file                         # every media file and what uses it
iwork add-text-box file <page|slide> "…" x y w h out
iwork add-shape   file <page|slide> ellipse "label" x y w h out
iwork add-image   file <page|slide> photo.jpg x y w h out
iwork add-table   file <page|slide> <name> <rows> <columns> x y out
iwork set-geometry file <drawable> x y w h out
iwork paint       file <drawable> fill=#B44A2B stroke=#FFFFFF:4 opacity=0.9 out
iwork replace-media file <media> new.png out
iwork charts      file                         # every chart and its data
iwork set-chart-data file <chart> data.csv out
```

**Keynote**

```
iwork slides      file                         # slides, placeholders, notes, transitions, builds
iwork layouts     file                         # the theme's layouts
iwork background  file <slide number> '#122B4A' out
iwork set-notes   file <slide> "…" out
iwork effects                                  # the available transition effects
iwork set-transition file <slide> dissolve 1.5 out
iwork add-build   file <slide> <drawable> in out
iwork duplicate-slide | skip-slide | unskip-slide  file <slide> out
iwork move-slide  file <slide> <position> out
```

**Pages**

```
iwork sections    file                         # sections, headers, footers, page numbers
iwork structure   file                         # mode, paper size, page templates
```

## Limitations

- **Open the result in the app before you rely on it.** The tests check that
  written documents are structurally sound; they can't prove that every app
  version accepts every document. `iwork check` catches the known problems. If
  an app rejects, crashes on or silently changes a document this crate wrote,
  please report it with the smallest program that reproduces it.
- **Formulas are not evaluated.** You supply the value a formula cell shows;
  Numbers recalculates when the document is opened. Changing a cell that a
  formula depends on leaves that formula's stored result stale until then.
- **No layout or rendering.** The preview thumbnails are not redrawn after an
  edit (`iwork strip-previews` removes them), and text boxes that size
  themselves to their text have no stored size.
- **Password-protected documents** are detected and refused.
- **Not supported for writing:** deleting slides or changing a slide's layout,
  deleting section breaks, footnotes, bookmarks, comment replies, editing text
  with tracked changes, list levels, filter rules, categories and pivot tables,
  groups and movies, and charts whose data comes from a table. Most of these
  can still be read.
- **Deleting text that an image, table or footnote is anchored to** is refused.
- **Replacing an image** keeps its frame; resize it with `set-geometry` if the
  new picture has a different shape.
- **Documents made from nothing** contain only what the apps need to open them
  — for example, a new Pages document has a single paragraph style and no list
  styles. Start from a template for a full set of styles and layouts.

## Further reading

- [`COOKBOOK.md`](COOKBOOK.md) — longer, tested recipes.
- [`API.md`](API.md) — how the API is organised.
- [`CHANGELOG.md`](CHANGELOG.md) — what changed in each release.
- [`FORMAT.md`](FORMAT.md) — a description of the iWork file format itself.

## Prior art

[numbers-parser](https://github.com/masaccio/numbers-parser),
[keynote-parser](https://github.com/psobot/keynote-parser) and
[iWorkFileFormat](https://github.com/obriensp/iWorkFileFormat) mapped much of
this territory first, in Python and Objective-C.

## Legal

Reverse engineering a file format to achieve interoperability is permitted under
EU Directive 2009/24/EC Art. 6 and Swiss URG Art. 21. This repository contains no
Apple code, no Apple `.proto` files and no iWork documents.

## License

MIT — see [LICENSE](LICENSE).
