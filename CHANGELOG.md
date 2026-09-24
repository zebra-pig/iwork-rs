# Changelog

What changed, and — because this is a reverse-engineered format — **how it was
established**. An entry that cannot say what the app did is an entry about
bytes nobody has watched being read.

## 0.2.1 — 2026-09-23

**Anyone writing pictures with 0.2.0 should take this one.** A document that
used the same image twice made Keynote abort on opening — not refuse, abort,
inside `TSPersistence`, before it drew anything and with nothing said to the
user. `iwork check` had called such a deck clean.

### Fixed

- **One stored file per picture.** iWork's media registry is keyed by content:
  one `Data/` entry per distinct set of bytes, every user pointing at it.
  `add_image` allocated a fresh entry per call, so the same PNG on two slides
  was stored twice and the app aborted. The digest was already being computed
  for the `DataInfo` and simply never consulted. `iwork check` now names any
  two registry entries holding the same bytes, and a deck built from one
  picture used twice opens.

### Added

- **Drawables can be painted** — `set_object_fill`, `set_object_stroke` and
  `set_object_opacity`. This is the first styling in the crate that both apps
  draw in a document built **from nothing**, and two things had to be found
  before it would stick, both by making the app save the file again:
  - a drawable needs a style *of its own*. A document from nothing points
    every shape at a theme preset, and Keynote regenerates presets on save —
    the colour was gone. The first paint now makes the variation the app
    itself makes, naming the preset as parent and carrying only what differs;
  - **`override_count` is not decoration.** A bag holding a colour while the
    count said nothing differed came back from the app's save stripped. It is
    maintained from the bag now, so the two cannot disagree.
- **Cell styles can be read** — `cell_styles()` gives each
  `TST.CellStyleArchive` its role name, fill and insets. `set_cell_style_fill`
  writes the fill, and Numbers draws it in a document Numbers wrote: asked for
  a header cell's background after a repaint it answered `5959,17617,36075`
  where it had answered `45231,46004,45746`.

### The shape of what is still missing

One cause, stated plainly because it is the next piece of work: **a document
this crate builds from nothing has a stub stylesheet, and the apps draw styles
that came from a real document while ignoring ones this crate invents.** Pages
gets one paragraph style, `Body`, and draws every paragraph of a generated
report at 12 pt where its own styles give `30 / 18 / 11`. Numbers gets the
named cell styles and a `TST.TableStyleArchive` of a few hundred bytes where
the real one is 8 344, of which 4 037 are the strokes behind gridlines. Both
are asserted as tests that say to delete themselves when it changes.

### Also

- 2 000 corrupted documents per run say rubbish comes back as an `Err` rather
  than a panic — flipped bits, truncation, and corrupted object streams
  repackaged around the damage, which is the only shape that reaches the
  parser.
- Fifteen fixture sweeps demanded a corpus and failed where there is none.
  They skip now, which is the rule this repository already had written down.


## 0.2.0 — 2026-09-21

The release that makes the crate usable from code rather than from a shell,
and the first one published to crates.io — 0.1.0 below is the repository's own
history, and was never uploaded.

### A spreadsheet library, not a byte editor

- **Cells are written in batches.** `set_cells` and `set_block` write many cells
  in one pass, and `set_cell` is one cell handed to the same path. All or
  nothing: a refused cell leaves the document byte for byte as it was. Writing a
  table one cell at a time was quadratic three times over — 15 000 cells took
  206s — and is now **100 000 cells in 0.26s**.
- **A row with no cells can be given its first**, which is what made an inserted
  row fillable. Numbers reads the value back.
- **Formulas from text.** `set_formula` parses `=SUM($B$2:$B$3)*2` and writes
  the `TSCE` node stream for it, registered in the calculation engine so the app
  recalculates it. Numbers prints the formula back in its own spelling.
- **Data formats, money, and sizes.** `set_format` writes number, percent,
  scientific, currency and date formats; `set_currency` writes *money*, which is
  a value type and not a format; `set_column_width` and `set_row_height` are one
  float each. The app draws all of them.
- **Rows and columns can be deleted**, giving back every reference their cells
  held, and **cells can be merged** — the range node being byte for byte the one
  the app writes.
- **A chart can be made to follow a table.** `bind_chart` writes the mediator,
  its formulas and the `owner_kind` 2 owner that makes the engine believe them.
  Numbers recalculates the chart when the table changes.

### An API a caller meets

- **Values convert.** `doc.set("B2", 1_240)` — no `Decimal::parse` and no
  `unwrap` that turns an unparseable number into an empty cell.
- **Cells are named `"B3"` or `(2, 1)`**, ranges are `"B2:D2"`, frames are
  `Frame { x, y, width, height }`, and every message about a cell prints A1.
- **Handles instead of object identifiers**: `table_mut`, `text_mut`,
  `body_mut`, `slide_mut`. They resolve once and hold no data, so they cannot go
  stale.
- **Every refusal carries a reason** — `Refusal::Merged`,
  `Refusal::HoldsFormula`, `Refusal::OutOfBounds` and a dozen more — so a caller
  can skip, grow or stop without matching on the text of a message.

### The organisation layer, in the three places it could be written honestly

- **Sort rules**, whole: one inline archive on the model. They say what to sort
  *by* — nothing here reorders a row.
- **A filter's switch** and its all/any mode. Its *rules* are not written:
  Numbers compiles a filter condition into a `TSCE` formula and this corpus
  carries one of those to learn from, which is a sample and not a pattern.
- **A conditional highlight's threshold**, in all four places a rule keeps it —
  the immediate value and the formula's literal, in each of the two shapes the
  set stores its rules in — for the two predicates whose meaning is established.

And a finding that is the opposite of what a reader would assume: **switching a
filter off does not un-hide its rows.** Which rows are hidden is stored, not
worked out on open; the app recomputes it when the filter is next touched in its
own interface.

### Shapes the format insists on

- **A sheet is not a grid.** A Numbers sheet holds any number of tables, with
  charts and shapes beside them, so `Sheet` carries its drawables and a table is
  named by a unique name, by sheet *and* name, or by identifier. An ambiguous
  name is refused with both sheets named.
- **A slide is not a page with a title slot.** A title is a placeholder the
  *layout* defines; a deck made from nothing has a layout that defines none, and
  `title` refuses there by name rather than inventing a text box.
- **A page-layout Pages document's body is never drawn.** It has the storage —
  and the app does not show it, which was measured by appending a paragraph and
  asking Pages for every word in the document. Writing to it is refused.

### Documents from nothing

- A new document carries a **calculation engine** and the styles a table is
  drawn with, so a table can be added to one — and Pages opens the result.
  Listing those styles in the stylesheet, which looked tidier, made Pages refuse
  the document outright.
- A new table carries the two `TSCE` **owners** a formula needs.
- A slide made from nothing can be given **presenter notes**.

### Fixed

- **A clock in the output.** Every ZIP entry was stamped with the current time,
  so the same document saved twice was two different files. Entries carry the
  ZIP epoch now, and a save is reproducible.
- Taking a reference to a `TableDataList` entry that is not there is an error
  rather than a silent no-op.

## 0.1.0

Reading: all three apps, four layers, the object graph — text, styles, tables,
formulas, drawables, media, charts, comments, tracked changes, the Keynote show.
Writing: text and its attribute tables, styles, cell values, geometry, media
replacement, slides, transitions, builds, comments, documents from templates and
from nothing. `FORMAT.md` is the specification derived along the way.
