# API — how this crate is meant to be used, and how it grows

`FORMAT.md` says what the bytes are and `PLAN.md` what is still wrong with
them. This says what the *public surface* looks like and why, so that a new
feature has one obvious shape instead of five plausible ones.

It is a rule book. A change that breaks a rule here changes the rule here
first, in the same commit, with the reason.

## Where this came from

Up to 0.2.4 the API grew one verified capability at a time, each as a function
on `Document` taking a `u64`: 124 public methods, four ways to say where
something goes, raw protobuf values for a font size. An outside user asked to
build a ten-slide deck wrote the same four-step helper for every text box.

What other libraries settled on, read before writing this:

| library | creating | placing | editing |
|---|---|---|---|
| `rust_xlsxwriter` | `Chart::new(kind)`, `Format::new()`, `Image::new()` — plain values | `worksheet.insert_chart(row, col, &chart)` | — (write-only) |
| PptxGenJS | `slide.addChart(type, data, options)` | `x, y, w, h` are among the object's own options | — (write-only) |
| python-pptx | `chart_data` value, then `shapes.add_chart(…)` | arguments of the add call | the call returns the object; `chart.has_legend = True` |
| ExcelJS | `sheet.getCell('A1')` | — | assign properties on the handle: `.value`, `.font`, `.fill` |

and the Rust API Guidelines, the ones that bite here: constructors are
inherent `new` (C-CTOR), complex values are built with builders (C-BUILDER),
conversions use `From`/`Into` (C-CONV-TRAITS), functions are generic over
what they can be (C-GENERIC), arguments say what they mean with types rather
than `bool` or bare numbers (C-CUSTOM-TYPE, C-NEWTYPE), getters have no `get_`
(C-GETTER), public types are `Debug + Clone + PartialEq` (C-COMMON-TRAITS),
arguments are validated (C-VALIDATE), traits nobody else should implement are
sealed (C-SEALED), and structs that will grow keep their fields private
(C-STRUCT-PRIVATE).

## The model

A document is a tree, and the API is that tree:

```text
Document
 ├─ slides / sheets / pages      containers — things are added to these
 │   └─ elements                 Shape, TextBox, Image, Table, Chart
 │       └─ text                 a text box's, a cell's, a shape's
 ├─ text styles                  named, shared by every text in the document
 └─ the low level                objects, archives, problems — for tools
```

**Values to create, handles to edit.** There are two kinds of type and every
public type is one or the other.

* A **value** describes something that does not exist yet: `Chart`, `Shape`,
  `TextBox`, `Image`, `Table`, `TextStyle`, `Fill`, `Gradient`, `Shadow`,
  `Stroke`, `TextLook`, `Frame`, `Color`. It owns its data, borrows nothing,
  cannot fail to build, and is `Debug + Clone + PartialEq`. It can be made by
  a helper function, tested without a document, and added twice.
* A **handle** is a thing that *is* in a document: `SlideMut`, `CanvasMut`
  (a sheet or a page), `ElementMut`, `ChartMut`, `TableMut`, `TextMut`,
  `TextStyleMut`. It borrows the
  document mutably, every method on it is one edit, and every edit returns
  `Result`. A handle is only ever obtained from its parent
  (`doc.slide_mut(0)?`, `doc.chart_mut(id)?`).

Reading is a third, older thing: `doc.slides()`, `doc.charts()`,
`doc.elements()` return plain snapshot structs. **A snapshot is named
`…Info`**, so the bare noun is free for the value: `Chart` is what you build,
`ChartInfo` is what `doc.charts()` tells you. `ChartInfo`, `TextStyleInfo` and
`TableInfo` were renamed in 0.3.0; `ElementInfo` (was `Drawable`), `SlideInfo` and
`SheetInfo` in 0.4.0.

## The rules

1. **One way to add: `container.add(value)`.** One method, one argument.
   Where it goes and how big it is are properties of the value —
   `.at(x, y)`, `.size(w, h)`, `.frame(Frame)` — like its fill or its title.
   A value with no size takes its natural one (an image its pixels, a table
   its rows) or a stated default; a value with no position goes to the
   container's origin.
   `add` returns the new element's identifier, an `ElementId`.
2. **Builders consume and return `Self`.** `Chart::new(kind).title("x")` is an
   expression, so it can sit inside `slide.add(…)`. `add` takes `impl Element`
   and `&T` is an `Element` wherever `T` is, so a value kept in a variable can
   be added again without a `.clone()`.
3. **One type per idea, used for reading and for writing.** `Fill` is a
   colour, a gradient, a picture or nothing, on a shape, a slide's background
   and a table cell alike. `TextLook` is how characters look, in a paragraph
   style, on a run and in a cell. No `set_x_fill` / `set_x_gradient` /
   `set_x_image_fill` families.
4. **Types, not conventions.** No raw protobuf `Value` in an everyday call, no
   container named by a string, no `(f32, f32)` whose order has to be
   remembered where a `Frame` or two named methods will do, no `bool`
   argument (`.legend()` and `.no_legend()`, not `.legend(true)`).
5. **Generic where it costs nothing.** Text is `impl Into<String>`, lists are
   `impl IntoIterator`, a colour is accepted wherever a `Fill` is
   (`impl Into<Fill>`).
6. **Names.** Values are nouns (`Chart`). Handles are the noun plus `Mut`.
   Constructors are `new` or a named alternative (`Shape::rectangle()`).
   Builder methods are the bare property (`.fill(…)`, `.title(…)`), and on a
   handle the same word is the edit. No `get_`, no `set_` on builders; `set_`
   only where a handle method would otherwise read as a getter. British
   spelling in prose, and `colour` in identifiers this crate names — as it has
   been since 0.1.
7. **Refusal is a value.** A call that would have to guess returns
   `Error::Refused { reason, detail }`. Building a value never fails;
   validation happens in `add`, before anything is written, so a refused `add`
   leaves the document as it was.
8. **Nothing the apps were not asked about.** A builder method exists only for
   something an app was watched drawing and keeping through its own save
   (`PLAN.md` ground rule 1a). The API does not get ahead of the format.
9. **The low level stays, and stays low.** `objects`, `archive`,
   `set_archive_for`, `problems` and the per-format modules are public for
   tools and repairs. Recipes never need them; if one does, that is a missing
   handle method.
10. **Sealed and private.** `Element` is sealed. Values have private fields
    and builder methods, so a property can be added without a major version.

## The surface, as it should end up

```rust
// values
Color::rgb(0.07, 0.17, 0.29)   Color::hex("#122B4A")?   Color::WHITE
Fill::None | Fill::Color(c) | Fill::Gradient(g) | Fill::Image(i)      // From<Color>, From<Gradient>
Gradient::linear(from, to, angle)   Shadow::default()   Stroke::new(colour, width)
TextLook::new().font("AvenirNext-Bold").size(60.0).colour(c).bold()
TextStyle::new("Title").look(look).align(Align::Centre)

Shape::rectangle() / ::ellipse() / ::line()   .fill() .stroke() .no_stroke() .shadow() .opacity() .text()
TextBox::new("words")                         .style(id) .look(look) .format(range, look)
Image::new(bytes).named("photo.png")
Table::new("Name", rows, columns)             Table::with_rows("Name", [[…], […]])   (only .at)
Chart::new(ChartKind::Column)                 .categories([…]) .series("2025", […]) .title("…") .legend()
// every one of them but Table: .at(x, y) .size(w, h) .frame(Frame)

// document
let title = doc.add_text_style(TextStyle::new("Title").look(…))?;
let id = doc.slide_mut(0)?.add(Chart::new(…)…)?;       // also sheet_mut(name), page_mut(1)
doc.slide_mut(0)?.background(Fill)?;

// handles
doc.element_mut(id)?   .fill() .stroke() .shadow() .opacity() .move_to() .resize() .text()
doc.chart_mut(id)?     .data(…) .kind(…) .title(…) .legend() + everything on element_mut
doc.table_mut(name)?   .set() .formula() .currency() .format() .fill(range, Fill) .look(range, &TextLook) .align() …
doc.text_mut(id)?      .set() .insert() .delete() .replace() .style() .format()
doc.text_style_mut(id)? .look() .align() .rename() .property() .delete()
```

## How it got here

| release | what |
|---|---|
| 0.3.0 | values + `add`; one `Fill`; `TextLook` and typed `TextStyle`; handles for elements, charts, tables, text and text styles; snapshots `ChartInfo`, `TextStyleInfo`, `TableInfo`. Every flat function on `Document` that a handle covers deprecated, with the replacement named. |
| 0.4.0 | The flat layer **removed** from the public API, with `CellText` and the 0.2 aliases: there is one way to do each thing. Snapshots `ElementInfo` (`doc.elements()`), `SlideInfo`, `SheetInfo`. Typed identifiers: `add` returns an `ElementId`, `add_text_style` a `StyleId`. The `iwork` binary goes through the handles like any other caller. |

Still flat on `Document`, because no handle is yet the better home:
`set_cells`, `fill_formula`, `set_filter_enabled`,
`set_conditional_threshold`, `bind_chart`, `copy_chart`, `add_comment`,
`replace_media`, `set_geometry`, `create_text_style`,
`copy_text_style_property`, `update_text_style`, `add_slide`, `add_sheet`
and the Pages structure. Each moves when it gets a handle method, and by
rule 1 of the checklist below nothing new is added there.

Identifiers a snapshot reports are still `u64` — `ElementInfo::identifier`,
`TextStyleInfo::identifier` — and convert into the typed ones. Typing them at
the source is the next step, with a `StorageId` for text.

**The tests written against the flat calls still run**, through
`tests/common`, which spells each old call as the handle call it became. So
every one of them now tests a handle.

## Checklist for a new capability

1. Was an app watched drawing it and keeping it through a save? (Rule 8.)
2. Is it a property of an existing value? Then a builder method there and the
   same word on the handle — nothing on `Document`.
3. Is it a new kind of thing? Then a value, `impl Element` if it can be
   placed, and a `…Mut` handle if it can be edited afterwards.
4. Does a recipe in `COOKBOOK.md` use it, run by `cargo test --doc`?
5. Does it read without a comment? `slide.add(Shape::ellipse().fill(RED))`
   does. If the call needs explaining, rename it.
