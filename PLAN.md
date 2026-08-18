# Plan: implement the full iWork format in this crate

Working document for the autonomous build-out, 2026-08-17 onward. Checkboxes
are the live todo list; each phase is executed by a subagent and verified here
before the next begins. This Mac has Pages 15.3.1 and Numbers 15.3.1 installed
(as "Pages Creator Studio.app" / "Numbers Creator Studio.app" /
"Keynote Creator Studio.app" — genuine com.apple.Pages / com.apple.Numbers /
com.apple.iWork.Keynote bundles), all three scriptable via AppleScript and
confirmed working: probe documents created by each app parse correctly with
the crate, and Numbers evaluates formulas we can read back as an oracle.

## Ground rules, binding on every phase

1. **"The app opens it" is the acceptance test.** Every write feature ships
   with an app round-trip: write the document, open it in Pages/Numbers via
   the harness (phase 0), confirm it loads and — where readable back via
   AppleScript — that the edit took effect. `iwork check` learns every new
   invariant discovered this way.
2. **Nothing is lost, nothing is touched.** Wire-level editing stays: unknown
   objects pass through untouched, unchanged streams keep their exact bytes,
   a no-op save reproduces every entry byte for byte. Every phase re-runs the
   byte-identity tests.
3. **Copy, don't synthesise.** New objects come from copying ones that work,
   with the fields that must differ changed. Inventing a message from nothing
   crashed Pages before; it will again.
4. **FORMAT.md is the spec.** Every structural discovery lands there, asserted
   by a test, tagged Confirmed/Inferred/Unverified. Registry entries carry
   their evidence. README tables stay current.
5. **No iWork documents are committed.** Generated fixtures live in
   `tests/fixtures/` (gitignored); the *generator* is committed, so anyone
   with the apps can rebuild the corpus.
6. **The extracted protos are the map, not the territory.** The `.proto`
   files carried by numbers-parser, keynote-parser and iWorkFileFormat
   (cloned into the session scratchpad, never committed here — the Legal
   section stays true) are the starting point for naming types and fields
   and for knowing what a message *can* contain. They are evidence for the
   registry (`Inferred` until this repo's own tests observe the field), and
   a claim from them never enters FORMAT.md as Confirmed without a local
   probe backing it.
7. **The app's UI defines the feature list — but the screen is locked.**
   AppleScript covers a fraction of what the apps can put in a document,
   and System Events UI scripting is unavailable here: a locked screen
   exposes zero AXWindows, menu items validate as disabled, `activate`
   never returns (established during Phase 1b; accessibility permission
   itself is fine). The working substitute is **template mining**: Apple
   ships 79 `.nmbtemplate` bundles (and Pages/Keynote equivalents) built
   around exactly the features AppleScript cannot create — scan them with
   the crate for the type IDs you need, instantiate by template *id* (the
   bundle path, stable across Macs; names are localised), and the current
   app writes the whole structure out fresh. Features are enumerated from
   Apple's user guides (reference/features.md); probes are analysed with
   `iwork dump` diffs between incremental saves.
8. **Some features are read-and-pass-through, never authored.** Recorded
   presentations, smart annotations (iPad Pencil ink), EndNote citations,
   live-data cells, live video sources: the crate must carry them intact and
   may learn to read them, but must refuse to synthesise them — they encode
   state only the app (or another device) can produce. Naming them protects
   ground rule 3.
9. **Style discipline.** Match the repo's voice (literate commit messages,
   one idea per commit), rustfmt, no new dependencies without need. Work
   happens on branch `claude/full-spec`; each verified phase is one or more
   local commits (not pushed).

## Phase 0 — Fixture factory and app-validation harness

The foundation every later phase stands on.

- [x] `scripts/make-fixtures.sh` (+ AppleScript sources): generates a corpus
      into `tests/fixtures/generated/` using Pages and Numbers:
      - Pages: plain text, multi-paragraph styled text, lists, a table,
        an inserted image, non-Latin text (German umlauts + CJK + emoji for
        UTF-16 indexing).
      - Numbers: multiple sheets/tables, text/number/bool/date/duration
        cells, formulas referencing other cells, merged cells if scriptable,
        a second table styled differently.
      - Keynote: several slides from different master layouts, titles, body
        bullets, presenter notes, an image slide, a skipped slide.
- [x] `scripts/app-check.sh <doc> [expected-text]`: opens the document in the
      owning app via AppleScript with a timeout, fails loudly if the app
      refuses/crashes/dialogs, optionally reads body text / cell values back
      to confirm an edit, closes without saving. Must be callable from tests
      (`IWORK_APP_CHECK=1 cargo test`) and by later agents.
- [x] Integration tests pick up `tests/fixtures/generated/**` (recursive or
      flattened) and the whole existing suite passes against the new corpus.
- [x] Baseline recorded: object census per fixture, so later phases can see
      what they unlocked.

## Phase 1 — Tables: read

The largest missing area of the format. `TST` is cross-app: Pages and
Keynote tables use the same archives, so every claim gets three oracles,
not one.

- [x] Decode the table object graph: `TST.TableInfoArchive`,
      `TST.TableModelArchive`, `TST.TableDataStore`, tiles, row/column
      headers; map table → sheet → document (and table → page/slide in
      Pages/Keynote).
- [x] Decode cell storage (the current tile cell format): empty, text
      (string-table indirection), number, boolean, date, duration, error,
      rich-text cells, currency; merged-cell ranges. *(Error cells are
      decoded from the type byte and their formula-error key; no fixture
      produces one, so the value carries no message.)*
- [x] Structure state: header row *and column* counts (0–5 each), freeze
      flags, hidden rows/columns (filter-hidden and manually-hidden are
      different persisted states), explicit row/column sizes, table names.
      *(The two hidden counts are read separately from the model; the
      per-row `hidingState` is zero everywhere in the corpus, because
      nothing in Numbers' scripting interface hides a row — see 1b.)*
- [x] Cell data formats, read alongside values: number/currency/percentage/
      fraction/scientific/date-time/duration/text/automatic, stored per cell
      and per column; cell controls (checkbox, star rating, slider, stepper,
      pop-up menu with its item list) at least identified.
- [x] API: `doc.tables()`, `table.cell(row, col)`, typed `CellValue`.
- [x] CLI: `iwork tables <doc>`, `iwork cells <doc> <table-id>`,
      `iwork csv <doc> <table-id>`.
- [x] Cross-check against AppleScript: the harness reads the same cells via
      Numbers and the values must agree — the app is the oracle.
- [x] FORMAT.md: new §Tables with the tile/cell layout as observed.

## Phase 1b — Table data organisation: read

Everything layered on top of the cells; depends only on Phase 1.

- [x] Sort rules and filter rule sets (with the enabled/disabled toggle).
      *(Rules are read down to `predicate_type`, the qualifiers, the per-rule
      column and enable flag, and any immediate value. The **condition itself**
      is a `TSCE` formula in the shape 15.3.1 writes, so "greater than 500"
      reads as "predicate 37 against a formula" until Phase 5.)*
- [x] Categories: source column, subcategories, group membership and order,
      summary-row aggregate assignments, per-group collapsed state.
      *(Two shrinks, both for want of a document: **subcategories** — the
      recursive group tree is decoded, but no template ships a category more
      than one level deep, so nesting is unexercised; **per-group collapsed
      state** — located, decoded from `collapsed_group_uids`, and empty
      everywhere, because no template ships a collapsed group and no script can
      collapse one. Both are marked Unverified in FORMAT.md. Of the aggregate
      codes only `2 = Sum` is proven.)*
- [x] Conditional highlighting rules; custom cell formats (named,
      document-scoped, with conditional sub-rules).
      *(Conditional sub-rules of a custom format are counted, not decoded: the
      one custom format in the corpus has none.)*
- [x] Pivot tables: source reference, field assignments, summary functions,
      display modes, totals toggles — read and inventory.
      *(`show_as_type` — the display mode — is read and is 0 on every field in
      the corpus, so its other values are unverified.)*
- [x] Hidden rows and columns actually exercised, which Phase 1 could not do.
      `hidingState` 1 is the user, 2 is a filter; the model's five hidden
      counts are zero in a document that hides both.
- [x] FORMAT.md §Tables extended; pass-through tests so a save never damages
      any of it.

## Phase 2 — Tables: write

- [x] **Precondition: pin down the `type == 0` diff/merge mechanism.** The
      distilled references show both Python parsers implement it incorrectly
      and incompatibly; before any table write, spec it from local probes
      and teach the test suite what it means. *(Resolved by measurement:
      15.3.1 writes exactly one patched object per Numbers document — the
      view state — and none in Pages or Keynote; **no table archive carries
      one**, and having the app edit a cell and save produced no new ones.
      Read the base and ignore the patches; never write one; refuse to
      rewrite a patched object. FORMAT.md §3, `Document::patched_objects`,
      `iwork check` note, `tests/cells.rs`.)*
- [x] Edit an existing cell in place: text and number values first, then
      boolean/date; string-table maintenance; tile re-encode with the
      byte-identity rule for untouched tiles/streams. A written cell keeps
      (or is given) its data format — value and format travel together — and
      writing into a cell that carries a control definition preserves it.
      *(All five value kinds plus "empty". **Scope:** the row must already
      have stored cells — giving a row its first one needs a `TileRowInfo`
      and is left with adding a row. Refused by name: formula cells,
      rich-text cells, cells covered by a merge, patched objects.)*
- [x] `iwork set-cell <doc> <table> <row> <col> <value> <out>`, and the same
      cell as an A1 reference.
- [x] App round-trip: Numbers opens the result and reports the new value via
      AppleScript. `iwork check` learns any invariant broken along the way.
      *(`tests/cells.rs::numbers_reads_back_an_edited_cell`; `iwork check`
      gained the four table invariants in `Table::audit`.)*
- [ ] Stretch (only if in-place editing proves solid): add a row by copying
      an existing one. *(Not attempted — see the verification log for what it
      would take.)*

## Phase 3 — Drawables, geometry, media

- [x] Enumerate drawables: `TSD.DrawableArchive` subclasses — shapes, image
      drawables, groups, lines — with their geometry
      (`TSD.GeometryArchive`: position, size, rotation, flags) and z-order.
      *(Also movies, masks, tables, charts, placeholders and captions; and
      containment per app — Numbers sheets, Keynote slides, Pages section
      templates, floating drawables and text-anchored attachments. Groups and
      connection lines are decoded but **unexercised**: nothing in the corpus
      has one, because no script can make the apps group anything.)*
- [x] Write geometry: move/resize a drawable; app-verified.
      *(Keynote reads back a moved shape and image; the resize of a masked
      image reproduces what Pages wrote for the same edit, byte-identical in the
      mask and two ulps out in one float of the image.)*
- [x] Read object styling: fills (colour/gradient/image + tint), strokes,
      shadows, reflection, opacity, lock state — the `TSD` style surface.
      *(Colour fills, strokes with their dash/dot patterns, shadows with their
      non-zero defaults, reflection opacity and opacity, all resolved up the
      inheritance chain, and the lock on the drawable. **Gradient and image
      fills are decoded but Unverified** — nothing in the corpus has one, and
      no script sets a fill. Tint is part of an image fill and unexercised for
      the same reason.)*
- [x] Media: replace an existing image's bytes (Data entry +
      `TSP.DataReference` + digest/metadata fields as observed); insert an
      image by copying an existing image drawable. App-verified. **Caveat
      proven by the guides:** a drawable carries non-destructive edit state
      (crop/mask rect, mask shape, Instant Alpha mask, ten tone/colour
      adjustments) between the stored pixels and the render — replacement
      must surface that state, or a swapped image opens fine and renders
      wrong while the app round-trip passes. Read it before writing bytes.
      *(Replacement done and app-verified; the edit state is decoded and is a
      **named refusal**. **Insertion was not attempted** — see the log for what
      it needs. The caveat is real and was measured: the app's own replacement
      re-fits the picture into the old frame with a mask, which is why this one
      keeps the frame and warns instead.)*
- [x] Inventory (read-level) of the wider media model: video/audio with trim
      points and poster frames, galleries, drawings (stroke order is
      load-bearing — "Animate Drawing" replays it), 3D objects; pass-through
      tests for each.
      *(All of it is identified — movies by their playback fields, the rest by
      the proto2 extension grafted onto the host archive. Only movies are
      **exercised**, and the two in the corpus are Keynote live-video
      placeholders with no film; galleries, 3D objects, drawings and web video
      are Unverified for want of a document.)*
- [x] CLI: `iwork drawables`, `iwork set-geometry`, `iwork replace-media`.
      *(Plus `iwork media`, which lists the registry with its digests and says
      whether each still matches its bytes.)*
- [x] FORMAT.md: §Drawables, §Media.

## Phase 4 — Text: finish the story

- [x] Fix the standing limitation, widened to its true scope: editing text
      remaps **every range anchored into the storage**, not just style runs —
      paragraph/character/list attribute tables, and equally tracked-change
      ranges, comment anchors, smart-annotation anchors, bookmark anchors,
      footnote anchors and ruby (phonetic guide) runs. The current clamping
      silently damages all of these today; this is a correctness fix, not a
      feature.
      *(All twenty-two tables of the 15.3.1 schema, in four anchorings.
      Attachment, footnote and section anchors are **refused** rather than
      remapped — deleting the character an image hangs off is how Pages deletes
      the image, and that reaches the z-order and the media registry. A
      length-delimited field outside the inventory is refused too.)*
- [x] Range operations: insert/delete text at a range, not just replace-all.
      *(`insert_text`, `delete_text`, `replace_text`; `set_text` is a full-range
      replace. `iwork insert-text` / `delete-text`.)*
- [x] Hyperlinks and smart fields: read them; edit a link target;
      app-verified. *(2032's field 2 is the URL; `smart_fields`, `set_link_url`,
      `iwork links`. **The fixture is Apple's template bundle renamed** —
      nothing can author a hyperlink here, and instantiating a template strips
      them. Numbers opens the edited document and reads the linked words back;
      no app's dictionary can report a URL, so the target itself is verified by
      decoding.)*
- [x] Lists: read list styles/levels per paragraph; change a paragraph's
      list level. *(Read: field 6's `first` is the level, field 7 the style,
      both sparse and both carried forward; `list_paragraphs`, `iwork
      paragraphs`. **The write was not attempted** — no app here can be made to
      change a list level, so there is nothing to verify a write against.)*
- [x] The style-override flag (`TSS`): know whether a run is "named style"
      or "named style plus local overrides" — a prerequisite for preserving
      styling across edits, which this phase promises.
      *(`style_of_run` → `ResolvedStyle`: the object, the named style it
      descends from, whether it is a variation, `override_count` as found, and
      the properties the variations set.)*
- [x] FORMAT.md: §Text updated with the full attribute-table inventory,
      including bidi/vertical-text/ruby tables where the corpus can produce
      them. *(The corpus produces none of the last three, and neither does any
      of the 901 bundled templates: fields 15, 16, 18, 20–23, 25 and 26 are
      absent from all of them. Named from the schema, handled by shape.)*

## Phase 4b — Pages document structure

The word-processing spine; Pages is the app this repo drives most
confidently. Read-first, then the safest writes.

- [ ] Document mode: word-processing vs page-layout (`isMultiPage`),
      reported by `iwork inspect`; sections and section breaks, per-section
      page-numbering rules, backgrounds.
- [ ] Headers/footers (three zones, match-previous, hide-on-first-page);
      paper size/orientation/margins/facing pages.
- [ ] Footnotes/endnotes: mode, markers, restart rules, note bodies as their
      own text storages (they must survive Phase 4's remapping).
- [ ] TOC (style-inclusion mapping), bookmarks (the anchor side of
      link-to-bookmark), page templates/masters, columns.
- [ ] Linked text boxes: the named thread joining boxes into one flow — a
      storage is not 1:1 with a drawable.
- [ ] Write (app-verified, smallest useful set): edit header/footer text;
      edit a footnote's text.
- [ ] FORMAT.md: §Pages structure.

## Phase 5 — Formulas and the calculation engine (read)

- [ ] Decode `TSCE` formula archives to an AST; pretty-print as the formula
      text the user typed (cross-checked against AppleScript's `formula`
      property, which is the oracle).
- [ ] The reference model in full: absolute/relative flags per axis, named
      references, whole-row/column and header-name references, cross-table
      (`Table 2::B2`) and cross-sheet references — which resolve by table
      *identity*, not name string; stored error states.
- [ ] `iwork formulas <doc>`; cells CLI shows formula alongside cached value.
- [ ] FORMAT.md: §Formulas. Writing formulas is out of scope until reading
      is exhaustive.

## Phase 6 — Charts (read)

- [ ] Enumerate `TSCH` chart objects (cross-app: Pages and Keynote carry
      charts too), their type (~25 named types incl. 3D and interactive),
      and extract series/category data (charts carry a private copy of
      their data, *distinct from* their `TSCE` references back into tables —
      read both and say which is which).
- [ ] `iwork charts <doc>`. FORMAT.md §Charts.

## Phase 7 — Comments, metadata, document properties

- [ ] Read annotations/comments and their anchors, authors storage —
      including resolved/unresolved state, reply threads (author + timestamp
      per reply), reviewer text highlights (annotation-layer, distinct from
      formatting highlight), and anchors into cells and chart elements, not
      just text.
- [ ] Read+write document metadata (Properties.plist fields, custom format
      lists), regenerate `Metadata/DocumentIdentifier`/UUIDs correctly on
      "save as new document" so two edited copies don't collide in iCloud.
- [ ] Change-tracking: read-level survey only; document in FORMAT.md.

## Phase 8a — Keynote: inventory and text (app-verified)

- [ ] Presenter-notes and slide-text extraction; slide/master/build/
      transition *inventory* (names and counts — parameters are 8b's job)
      surfaced in API + CLI.
- [ ] Write: edit slide text (title/body/notes) app-verified; duplicate a
      slide by copying; skip/unskip a slide; reorder slides.
- [ ] FORMAT.md §Keynote extended with what the probes prove; registry
      evidence upgraded from Inferred to Confirmed where the app accepts it.

## Phase 8b — Keynote: builds, transitions, playback (read)

- [ ] Build parameters: effect, direction, duration, build order, delivery
      mode (On Click / After Transition / With/After Build n + delay),
      action builds with motion paths, by-bullet-group text builds.
- [ ] Transition parameters incl. Magic Move's match modes; playback
      settings (presentation type, loop, auto-advance); soundtrack.
- [ ] Recorded presentations: identify and pass through (never author —
      ground rule 8).
- [ ] FORMAT.md §Keynote: builds/transitions as observed.

## Phase 9 — Document creation and hardening

- [ ] `Document::from_template(path)`: duplicate a document into a fresh
      identity (new UUIDs, cleared view state) — the copy-don't-synthesise
      answer to "create a document". Accept `.template`/`.kth`/`.nmbtemplate`
      bundles, which is what "create a document" means to a user.
- [ ] Package-form documents: File > Advanced > Change File Type saves a
      real *directory* instead of a ZIP (Apple recommends it above ~500 MB).
      Detect and read both forms; `iwork inspect` says which it has.
- [ ] Encrypted documents: detect, fail with a named error (not a parse
      failure), refuse to write. The common hostile-bytes case.
- [ ] Decide and document the preview-staleness rule (byte-identity says
      leave `preview*.jpg`; correctness says they now lie — pick one,
      record it in FORMAT.md, teach `iwork check` to note it).
- [ ] Fuzz the decoders (cargo-fuzz or dumb byte-mutation harness) so hostile
      files fail cleanly, never panic.
- [ ] Final pass over README/FORMAT.md; verification table updated to match
      reality.

## Verification log

Filled in as phases complete: what was proven, by which test, against which
fixture, and what the app accepted.

- 2026-08-17 — Phase 0 baseline: crate builds clean, 34 unit + 2 doc tests
  pass; AppleScript → Pages 15.3.1 → `.pages` → `iwork inspect|text` loop
  confirmed working end to end (probe document, 6 components, text extracted).

- 2026-08-17 — **Phase 0 complete.** `scripts/make-fixtures.sh` builds seven
  documents with the three apps; `scripts/app-check.sh` opens one and reads it
  back; `tests/fixtures.rs` finds fixtures recursively and gained one test
  (`every_fixture_opens_in_the_app_that_owns_it`, `IWORK_APP_CHECK=1`).
  `cargo test` is green over the whole corpus: 52 unit + 15 fixture + 34 style
  + 2 doc tests.

  Baseline census — `iwork inspect|text|styles|check|roundtrip` over every
  fixture. `check` found no problems anywhere and `roundtrip` re-encoded every
  object of every document with the identifiers unchanged.

  | Fixture | Kind | Objects | Streams | Data entries | Media registered | Text storages | Styles |
  |---|---|--:|--:|--:|--:|--:|--:|
  | `pages-plain.pages` | Pages | 575 | 7 | 0 | 4 | 1 | 259 |
  | `pages-styled.pages` | Pages | 578 | 7 | 0 | 4 | 1 | 262 |
  | `pages-unicode.pages` | Pages | 575 | 7 | 0 | 4 | 1 | 259 |
  | `pages-report.pages` | Pages | 706 | 37 | 1 | 4 | 7 | 196 |
  | `numbers-values.numbers` | Numbers | 797 | 97 | 0 | 3 | 0 | 256 |
  | `numbers-large.numbers` | Numbers | 638 | 38 | 0 | 3 | 0 | 256 |
  | `keynote-deck.key` | Keynote | 1069 | 30 | 25 | 35 | 44 | 240 |

  What the corpus holds: `pages-plain` one paragraph; `pages-styled` four
  paragraphs at three faces, sizes and a colour; `pages-unicode` umlauts, CJK,
  emoji (surrogate pairs), a ZWJ sequence, a flag and a combining mark;
  `pages-report` a document from the "Project Proposal" template — a 6×4 table
  with cells written by script, a photo, two section breaks;
  `numbers-values` two sheets and three tables with text, number, boolean,
  date, duration, decimal, a merged range and five formulas (including one
  reaching across tables); `numbers-large` a 301×9 table imported from CSV,
  with `SUM` and `AVERAGE` over whole columns; `keynote-deck` five slides on
  four layouts, titles, bullets, presenter notes on all of them, a skipped
  slide and an image slide.

  App acceptance: all seven documents open in the app that owns them and read
  back their text (`scripts/app-check.sh`, one per fixture). `--self-test`
  passes for `.pages`, `.numbers` and `.key`: a copy with one `Index/*.iwa`
  replaced by random bytes of the same length — still a valid ZIP, every entry
  stored and present — is refused by all three apps, so the harness is looking.

  **One crate bug, found by the corpus and fixed here.** `pages-report.pages`
  broke `the_paragraph_table_holds_entries_at_paragraph_starts`: a paragraph
  run at character 146 where no paragraph started. The text there reads
  `…123-4567\n\u{4}Company Name\n` — **`U+0004` is a section break and ends a
  paragraph**, exactly as `U+0005` ends one at a layout change, and the
  paragraph table puts its run on the character after it. Added to
  `text::PARAGRAPH_BREAKS` with a unit test and written up in FORMAT.md §Text.
  Both occurrences in that document are section boundaries; it is the same
  character FORMAT.md already recorded appearing *alone* in a body storage.

  What Phase 1 should know:

  - **Numbers table text is not in `TSWP` storages.** `iwork text` finds zero
    text storages in both Numbers fixtures, while the app reads back 117 and
    2711 cell values from the same documents. Cell strings live in the table's
    own storage, and finding them is Phase 1's job.
  - `numbers-values.numbers` spends 97 streams on 797 objects and
    `numbers-large.numbers` 38 on 638 — the per-table components the README
    already notes. The 301-row table is deliberately past any plausible
    single-tile capacity.
  - The app is the oracle and is already wired up: `formatted value of every
    cell of <table>` is one Apple event per table (2400 cells in about a
    second), which is what `scripts/applescript/check-numbers.applescript`
    uses. `value` and `formula` are equally readable — Numbers returns `84.0`
    and `=B1×2` for a cell written as `=B1*2`.

  What the apps would not do, probed against the dictionaries rather than
  assumed, and worth not re-litigating:

  - Pages will not create a table, an image, a shape or a text item from a
    script — `make new table` answers "Don't know how to create
    TMAScriptTableInfoProxy". Choosing the *template* a new document starts
    from is scriptable, so the fixture with a table and a photo comes from
    "Project Proposal", and its cells are then written by script.
  - Pages has no bold and no italic: rich text carries `font`, `size` and
    `color` only, so weight and slant go through the face name.
  - Keynote 15.3.1 has no master slides. Slides have a `base layout` and the
    document has `slide layout` elements; `skipped` must be set after the
    slide exists, because passing it to `make new slide` is accepted and
    ignored.
  - Numbers refuses to merge a range straddling the header row or column.
  - **`open` does not always answer.** Told to open a document — a `.pages` or
    an imported `.csv` — the app opens it and then never replies to the event.
    Everything here therefore hands the file to `open(1)` and polls for the
    document to appear. Likewise `close every document saving no` came back
    -10699 on a restored Numbers session that then refused `every document`
    with -1728, while closing `document 1` in a loop went through all three.

- 2026-08-18 — **Phase 1 complete (tables, read).** New module `src/table.rs`,
  `doc.tables()` / `table.cell(row, col)` / typed `CellValue`, `iwork tables |
  cells [--raw] | csv`, FORMAT.md §5 Tables, and the TST registry block rebuilt from four entries to
  sixteen — two of the four were wrong (6004 is the cell style, 6005 the data
  list). `cargo test
  --all-targets` green: 68 unit + 15 fixture + 34 style + 13 table tests;
  `cargo fmt --check` and `cargo clippy -D warnings` clean; the byte-identity
  and `iwork check` tests over the whole corpus are untouched and still pass.
  Nothing writes to a table.

  **What the oracle proved.** `tests/tables.rs::every_cell_agrees_with_numbers`
  (`IWORK_APP_CHECK=1`) drives `scripts/table-oracle.sh` →
  `applescript/table-oracle.applescript`, which reports name, class, value,
  formatted value, data format and formula for every cell of every table, plus
  row heights, column widths and header counts — six Apple events per table.
  Compared against the decoder: **2943 cells across three spreadsheets, every
  one agreeing** on value, on data format, and on whether it holds a formula;
  plus 9 tables agreeing on name, row and column count, and header-row,
  header-column and footer-row counts. Values are compared numerically where
  AppleScript's rendering is locale-dependent, and dates against the app's
  *formatted* value, since `value` comes back in the machine's timezone while
  the stored seconds are naive.

  Structural evidence that needs no app: `every_cell_record_is_consumed_to_the_byte`
  decodes **2515 records** from four documents and every one ends exactly on
  its last field, with bytes 2–5 zero throughout. That is what pins the flag
  word's twenty-one widths and their order.

  **The two bit orders, settled.** The payloads are consumed in ascending
  flag-word bit order; byte 6 is *not* a second presence mask over the same
  keys, as the distilled reference has it — it says which data format the user
  **chose**, and it is zero on most cells, all of which do carry format keys.
  Two independent confirmations of the flag-word order: the byte-exact record
  lengths above, and the `0x1000` payload, which numbers the six format slots
  1 number, 2 currency, 3 date, 4 duration, 5 text, 6 boolean — the flag word's
  sequence, not byte 6's. That payload is what the reference calls `suggest_id`;
  it is the cell's *current* format slot, and it is the third thing needed to
  report a format the way the app does.

  **Data-format codes, read off cells the app then named:** 256 number,
  257 currency, 258 percent, 259 scientific, 260 automatic, 261 date and time,
  262 fraction, 263 checkbox, 267 rating, 268 duration, 269 numeral system.
  264–266 unclaimed — three gaps, and exactly the three remaining controls,
  but a pop-up/stepper/slider cell carries a plain *number* format plus a
  `TST.CellSpecArchive`, so nothing here produced one. Control interaction
  types observed: 4 stepper, 5 slider, 6 rating, 7 pop-up menu, 8 checkbox.

  **Merges resisted the documented route and were solved another way.**
  `DataStore.merge_region_map` is absent from every document these apps write,
  the merge owner's `FormulaOwnerDependenciesArchive` back-dependencies are
  empty, and a merged-away cell has *no cell record at all* — not even a
  `spanCellType` one. What Numbers 15.3.1 writes is one formula per merged
  range in `TableModelArchive.merge_owner.formula_store`: a `COLON_TRACT_NODE`
  carrying absolute column and row ranges, or a `CELL_REFERENCE_NODE` (zigzag)
  for a one-cell merge. The region-map path is implemented as a fallback and is
  **Unverified**. Confirming this needed a trick, because AppleScript exposes no
  merge property: **a merged-away cell is reported under the name, value and
  format of the cell the merge began in**, so `B2:D2` comes back as `B2 B2 B2`,
  and the test checks the decoded merges against those names.

  Two app behaviours found on the way, both now recorded in the fixture
  scripts: writing a value into the top-left cell of an existing merge **pulls
  that cell back out of the merge** (`merge range "B2:D2"` then `set value of
  cell "B2"` leaves the app reporting a merge of C2:D2), so fixtures write
  values first; and `Tile.last_saved_in_BNC` is simply **not written** by
  15.3.1, so the published "refuse any tile without it" rule refuses every tile
  in this corpus. The version that is there is `TileRowInfo.storage_version`.

  New fixture `numbers-formats.numbers` (three tables): fifteen data formats
  applied by script, five control cells, four merges of four shapes including
  one whose anchor was never given a value, and one row and one column resized
  by hand — the last of which proves that a header entry's `size` of `0` means
  "the table's default", which is what every other row and column in the corpus
  carries. `numbers-values.numbers` was rebuilt with its merge written the
  other way round, so D9:E9 is now a real two-cell merge.

  Corpus decode summary — `iwork tables`, no app involved:

  | Fixture | Tables | Cells decoded | Merges |
  |---|--:|--:|--:|
  | `numbers-values.numbers` | 3 | 37 | 1 |
  | `numbers-formats.numbers` | 3 | 43 | 4 |
  | `numbers-large.numbers` | 1 (301×9, two tiles) | 2411 | 0 |
  | `pages-report.pages` | 1 (6×4, rich text) | 24 | 0 |

  What Phase 1b and Phase 2 should know:

  - **Keynote has no table fixture and cannot get one from a script.** Neither
    AppleScript nor any bundled theme puts a table on a slide; the archives are
    the same ones Pages uses, and the Pages fixture exercises them. UI scripting
    was left on the table as the bonus it was flagged as.
  - **Hidden rows and columns are decoded but never exercised.** Numbers'
    scripting dictionary has no `hidden` on a row or a column and no sort or
    filter command, so `hidingState` is 0 everywhere and the filter-hidden /
    user-hidden distinction is read from the model's counts only. 1b needs a
    UI-scripted or hand-made fixture — or `TST.FilterSetArchive` objects, of
    which the corpus already has seven per document with `is_enabled` and no
    rules.
  - **The cell record is understood well enough to write one.** Every field's
    width and order is pinned by the length check, and the three things a
    written cell must carry beside its value are now named: the format key in
    the right slot, byte 6 if the format is to be the user's choice, and the
    `0x1000` format-kind payload. Numbers writes `0x1000` on *every* cell.
  - **Formulas are keys, not text.** A formula cell carries only a key into the
    FORMULA `TableDataList`; `Cell::has_formula` says a formula is there and
    Phase 5 is what turns it into `=SUM(B3,B7)`. The cached value is in the
    cell like any other value, which is why the oracle agrees on `84` for
    `=B3*2` without anything here understanding the formula.
  - **`iwork cells --raw`** prints the record header and every key for each
    cell. It is the tool for the next question about this format.

- 2026-08-18 — **Phase 1b complete (table data organisation, read).** New
  section in `src/table.rs`, `iwork organise`, FORMAT.md §5 "How a table is
  organised", twelve registry entries, and four new fixtures. `cargo fmt
  --check` and `cargo clippy -D warnings` clean; `cargo test --all-targets`
  green: 68 unit + 15 fixture + 34 style + 22 table + 3 doc tests. Nothing
  writes to a table.

  **The fixture problem, and how it was actually solved.** The plan assumed UI
  scripting. It could not be used: the Mac's screen was locked for the whole
  session (`CGSSessionScreenIsLocked` true, display asleep), and a locked screen
  means **no `AXWindow` at all** — `System Events` reports the menu bar and zero
  windows, every menu item validates as disabled, and `activate` never returns.
  Accessibility permission was fine; there was simply nothing to drive.
  Selecting cells without the UI works (`set selection range of table … to
  range "A4:A5"`), but the menu item that would act on the selection stays
  disabled without a key window, so `Table ▸ Hide Rows` did nothing, twice.

  What worked instead: **Apple ships templates built around exactly these
  features.** Scanning all 79 bundled `.nmbtemplate` bundles with this crate for
  the TST category/pivot/filter types found `21_BasicCategories` (a category
  with a SUM summary), `21_Pivot_Table_Basics` (two pivots, one deliberately
  empty), `26_Stocks` (a filter that hides rows, columns hidden by hand,
  conditional highlighting, a custom cell format) and
  `44_Notetaking_Colorful_Log_PM` (a sort rule). `make new document with
  properties {document template: …}` then has **Numbers 15.3.1 write the whole
  structure out again**, which is what a fixture is for. Templates are addressed
  by `id` — `Application/21_BasicCategories/Traditional`, the path inside the
  bundle — never by the localised `name`.

  Two app behaviours cost time and are now recorded in the scripts. `close doc
  saving no` on a document that has just been saved to a new location does not
  answer for minutes **and deletes the file that was just written** — three
  fixtures were lost that way before `saving yes` (which returns at once) fixed
  it, and `build_template` now fails loudly if a fixture is not on disk after
  its builder said it was. And no template in the bundle has a collapsed
  category group, a category more than one level deep, or a hidden row that a
  filter did not hide.

  **What the documents proved, feature by feature.**

  | Feature | Fixture | Evidence |
  |---|---|---|
  | Sort rule | `numbers-sorted` | one rule, column C ascending; every other table's `TableSortOrderArchive` is present and empty |
  | Filter set | `numbers-rules` | one rule on column A, set enabled, match All; the rows it hides are the nine whose `hidingState` is 2 |
  | Hidden rows/columns | `numbers-rules` | three columns `hidingState` 1 = user-hidden, matching the extent's three `user_hidden` entries exactly; nine rows `hidingState` 2 = filter-hidden |
  | Category | `numbers-categories` | grouped by column B; the two groups' rows are exactly the rows whose column B holds `Andy` / `Chloe`; summary = Sum on column E |
  | Pivot | `numbers-pivot` | fields resolve to columns 2, 1, 0, 3 of `Sales` — which is what the app drew in the pivot table beside them: `Power`, `Product`, `Date (Month)`, `Units (Sum)` |
  | Conditional highlighting | `numbers-rules` | four rules, predicates 7 and 9 against `"0"` and 36 against `"↑"`/`"↓"`, each with a cell and a text style |
  | Custom cell format | `numbers-rules` | `Millions`, `format_type` 270, `#,###.##M`, document-scoped |
  | Pass-through | all four | a save reproduces every entry byte for byte, asserted by name |

  **Four things the published references get wrong, each found the hard way.**

  - `ColumnRowUIDMapArchive` field 1 is `sorted_column_uids` — sorted **by
    UUID**, not by position — with field 2 giving each one's index. A positional
    reading is right for every column that is a fixed point of that permutation,
    which in a five-column table is most of them.
  - **Filters are written in the pre-pivot slot** (`FilterSetArchive` field 3),
    not the field 7 the references call current. **Conditional highlighting is
    written in both**, and only the current shape carries the value the rule
    compares against — read both and you double-count, read only the old one and
    the values vanish.
  - The model's five hidden-row/column counts (fields 14, 15, 40, 41, 42) are
    **all zero** in a document with three hidden columns and nine hidden rows.
    Only `hidingState` is reliable. Phase 1's `iwork tables` line was reporting
    those counts and would have said "nothing hidden".
  - A pivot's `source_table_uid` is **not equal** to the source table's
    `haunted_owner.owner_uid`: the lower halves differ by a small constant (35),
    because every owner a table has is a numbered offset from one base UUID. The
    upper half is the table's identity and is what joins.

  Also worth keeping: a document with three tables carries **seven** empty
  `FilterSetArchive`s and an empty `TableSortOrderArchive` per table, so "has
  one" is not "uses one"; 15.3.1 writes categories **twice**, inline at the
  deprecated `TableModelArchive` field 81 and by reference at field 86; and
  `format_type` **270** (a custom format) is outside the 256–269 range Phase 1
  recorded.

  **What stays Unverified, and why.** Nested subcategories and per-group
  collapsed state: both are decoded — the group tree recurses, and
  `HiddenStateExtentArchive.collapsed_group_uids` is the only persisted home for
  a collapse — but no template ships either and no script can make one. A
  custom format's conditional sub-rules are counted, not decoded, for the same
  reason. Aggregate codes other than `2 = Sum` and every `predicate_type` code
  are reported as numbers; Apple publishes no names and this corpus has four
  predicate values from one document. `show_as_type` (a pivot's display mode) is
  read and is 0 everywhere.

  `numbers-rules` comes from the `My Stocks` template and carries **live
  stock-quote cells** — ground rule 8's "read and pass through, never author",
  now exercised by a fixture. Its quote values and the extent's filtered-row
  list change between rebuilds, so the tests assert its structure and never its
  numbers.

  What Phase 2 (cell writes) must know:

  - **A cell that is hidden, filtered, categorised or highlighted is still an
    ordinary cell record**; none of this phase's structures live in the tile.
    But four of them are *keyed off the cell*: the conditional-style key
    (`0x80`) and conditional-rule key (`0x100`) in the flag word reach a
    `ConditionalStyleSetArchive`, and a written cell that drops them loses its
    highlighting.
  - **Writing a cell changes nothing about a category or a filter by itself,
    and that is the danger.** A category's group nodes carry row *index* ranges
    and a filter's hidden state carries row *UUIDs*; a write that adds or moves
    a row must update both or the document opens with groups that claim the
    wrong rows. Editing a value in place does not.
  - `TableModelArchive` fields 14, 15, 40, 41 and 42 are dead in 15.3.1 output —
    do not maintain them and do not trust them.
  - Row and column UUIDs are allocated per table and **collide across tables**:
    two freshly created five-column tables in different documents have the same
    five column UUIDs. Any UUID index must be per table.

- 2026-08-18 — **Phase 2 complete (tables, write — in place).** `set_cell` on
  `Document`, a cell-record *encoder* in `src/table.rs`, `iwork set-cell`, four
  new `iwork check` invariants, FORMAT.md §3 "Version patches" and §5 "Writing
  one cell", and `tests/cells.rs` — 16 tests. `cargo fmt --check` and `cargo
  clippy -D warnings` clean; `cargo test --all-targets` green: 68 unit + 16 cell
  + 15 fixture + 34 style + 22 table + 3 doc. `IWORK_APP_CHECK=1` green over the
  whole suite: all twelve fixtures still open in their apps, the 2943-cell
  oracle comparison still agrees, and the new write round trip passes.

  **One harness bug, and it was this phase that made it visible.** `cargo test`
  runs test *binaries* in parallel, and with the write round trip there are now
  three of them driving the apps. Every script here begins by closing whatever
  the app is holding, so two at once close each other's documents, and the
  failure reads `the app that owns it would not open it` about a fixture that
  opens perfectly well on its own — which is exactly what happened, once, on a
  run whose repeat was green. `scripts/lib/osa.sh` gained `osa_acquire`: a lock
  directory taken before the app is warmed, carrying the holder's pid so an
  abandoned lock is stolen rather than waited out. Every entry point takes it.

  Pulling that thread found **two more harness faults, both of the kind the
  README warns about — a check that says yes when it should say no.**
  `table-oracle.sh` read `$?` after `if ! osa_run …`, which is the status of the
  *negation*: every failure exited 0 with an empty answer, so a Numbers that had
  timed out on the 2411-cell fixture looked like a decoder disagreeing about a
  document it reads correctly. And the oracle script opened a document and then
  read `document 1` — only the right document when nothing else is open, which
  is not true of an app that restores its last session; the failure mode is a
  complete, plausible reading of *another file*, and it was seen as an edited
  fixture reporting its pre-edit values. It now waits for the document by name,
  matching two, because Numbers answers `numbers-values` for a document it wrote
  itself (the Finder's hide-extension flag) and `iwork-set-cell.numbers` for one
  this crate wrote.

  **The `type == 0` precondition, resolved by measuring rather than
  implementing.** The published references disagree about the merge rules
  because nothing 15.3.1 writes exercises them. Over all twelve fixtures:
  **one patched object per Numbers document and none at all in Pages or
  Keynote** — the `TN.UIStateArchive` (12026) in the view-state component, with
  three patches for 11.0, 10.1 and 10.0, `base_message_index` 0,
  `diff_field_path` **absent**, `fields_to_remove` `[28]`, and a payload that is
  nothing but two copies of field 28. Then the harder question: does the app
  *produce* diffs when it saves an edit? Numbers was made to open
  `numbers-values`, change B3 by script, and save. It rewrote ten of 103
  entries — two tiles, the calculation engine, the document, the metadata, three
  previews, `Metadata/Properties.plist`, and a view-state component that came
  back under a **new stream name with new object identifiers** — and added no
  patched object anywhere. **No tile, data list, model or header bucket carries
  a patch**, so a cell write never has to merge or re-emit one. The rule is
  therefore the blunt one, and it is now three lines in FORMAT.md and a refusal
  in `set_cell` rather than an unimplementable merge.

  **Everything else about the write path came from the same probe technique** —
  have the app do it, then diff the objects. Five findings, each now a test:

  | Question | What the app did | Where it landed |
  |---|---|---|
  | Are `TileRowInfo` fields 3/4 (pre-BNC) live? | Byte-identical before and after an edit to the row they describe | Dead, confirmed; the writer leaves them |
  | Duplicate strings | `D2 := "Zahl"` took **string key 6**, the entry `A3` had just released | Interning is by value, refcounted |
  | Released strings | Keys 6 and 13 vanished from the list when their last cell let go; key 14 was then handed out | Entries are removed at zero |
  | `nextListID` | Stayed at 15 while freed keys 6 and 13 were reused | A high-water mark; the app takes the smallest free key, this crate takes the mark |
  | A text cell made a number | Came back with `number_format` and **no** `text_format` | The slot key moves with the type; both lists' refcounts moved by one each |
  | An emptied cell | **No record at all**, and both header buckets' counts fell by one | `CellValue::Empty` deletes the record |
  | A cell in a column with no bucket entry | The app **created** `{index, 0, 0, 1}` | So does this |

  **A correction to Phase 1.** FORMAT.md said 15.3.1 writes neither
  `storage_version` nor `last_saved_in_BNC` on a tile. It writes both — field 6
  is `5` and field 7 is `true` on every one of the 37 tiles in the corpus. The
  Phase 1 reading was of the `TileStorage` message above the tile, whose fields
  1–3 are the tile list, the tile size and the wide-row flag. The genuinely dead
  field is `numCells` (3), which is `0` on a tile holding 2411 cells. Two tiles
  of the pivot fixture set `should_use_wide_rows`, so the encoder's ×4 offset
  scaling is exercised by a real document and not only by a unit test.

  **What the app accepted.** `tests/cells.rs::numbers_reads_back_an_edited_cell`
  makes nine edits to `numbers-values` — a number, a string, a date, a duration,
  a boolean into a cell that had no record, a number into a column that had no
  header entry, a text cell turned into a number, and a cell emptied — saves,
  and drives the Phase 1 oracle. Numbers reports all nine, reports every
  untouched cell unchanged, and keeps the `D9:E9` merge (still two cells both
  called `D9`). Pages opens an edited `pages-report` and reads back the new cell
  text through `scripts/app-check.sh`.

  **The one thing a caller must know:** `C3` holds `=B3×2`, and after writing
  `B3 := 43` the file still caches `84`, because nothing here evaluates a
  formula. **Numbers recalculates on open and answers 86.** So a stale cache is
  a limitation of readers, not a corruption of documents — but it is why
  `set_cell` refuses to write *into* a formula cell, where the formula itself
  would have to go.

  **Stream-rewrite counts**, printed by `iwork set-cell` and asserted in
  `editing_a_cell_rewrites_only_the_streams_it_has_to`. Out of 97 entries:

  | Edit | Entries rewritten |
  |---|--:|
  | a number over a number | 1 (the tile) |
  | text over text | 2 (the tile, the string list) |
  | filling a cell that had no record | 5 (+ the format list, both header buckets) |
  | writing a cell the value it already holds | **0** |

  **`iwork check` learned four table invariants**, and all twelve fixtures
  already keep them: every key a cell holds resolves in its list; a list entry's
  refcount is the number of cells pointing at it; a list key is below
  `nextListID`; a row's and a column's `numberOfCells` is how many records it
  has. The first version of the refcount rule failed on three documents and was
  wrong, not the documents: it counted one format key per cell, and **a cell
  keeps a key for every format it has ever been given** — the header of a
  currency column is a *text* cell carrying both. `check` also prints patched
  objects as a note rather than a problem: it is a state, not a fault.

  What Phase 3 (drawables, geometry, media) should know:

  - **Probe first, implement second.** Every non-obvious rule above came from
    making the app perform the edit and diffing the object streams, not from a
    schema. The loop is `cp` the fixture, drive it with AppleScript (`save doc`
    then `close doc saving yes` — never `saving no`), and compare with
    `iwork objects` / `iwork dump`. It costs a minute and settles questions the
    references disagree about.
  - **A save by the app is not a small diff.** Numbers rewrote ten entries for
    one changed digit, renamed a component and reallocated its objects. Do not
    expect an app-made document to be comparable to this crate's output entry by
    entry; compare *objects*, and only the ones the edit concerns.
  - **Fixed-layout records carry fields nobody has decoded**, and drawables have
    them too. The rule that made the cell encoder safe — decode, mutate the
    fields you understand, re-encode everything else verbatim, and assert
    `encode(decode(x)) == x` over the whole corpus — applies unchanged to
    geometry and to media metadata.
  - **Refcounted side tables are a pattern, not a table quirk.** `DataInfo` and
    the media registry are the same shape; expect an entry nobody references to
    be removed rather than left.
  - The stretch goal that was not attempted, and what it needs: adding a row
    means a new `TileRowInfo` (including the two "required" pre-BNC fields,
    which would have to be copied from a donor row), a row UUID that is unique
    *within the table* (they collide across tables), an entry in the row header
    bucket and in `ColumnRowUIDMapArchive` — which is sorted by UUID, not by
    index — and, in a categorised or filtered table, the group nodes' row index
    ranges and the hidden-state extent's row UUIDs, which are two different
    addressing schemes for the same rows. None of that is hard; all of it has to
    be app-verified together, and there was no fixture combining a category with
    a filter to verify it against.

- 2026-08-18 — **Phase 3 complete (drawables, geometry, media).** Two new
  modules, `src/drawable.rs` and `src/media.rs`; `doc.drawables()`,
  `doc.object_style()`, `doc.set_geometry()`, `doc.replace_media()`; four new
  CLI verbs (`drawables`, `media`, `set-geometry`, `replace-media`); five new
  `iwork check` invariants; FORMAT.md §6 Drawables and §7 Media plus three new
  writing rules; a new fixture and a new oracle. `cargo fmt --check` and `cargo
  clippy --all-targets` clean; `cargo test --all-targets` green: 79 unit + 16
  cell + 18 drawable + 15 fixture + 34 style + 22 table + 3 doc.
  `IWORK_APP_CHECK=1 cargo test` green over the whole suite, thirteen fixtures.

  **What the apps proved, and how.** Every non-obvious rule below came from
  copying a fixture, making the app perform the edit by AppleScript, saving, and
  diffing objects — the Phase 2 method, unchanged.

  | Question | What the app did | What it settled |
  |---|---|---|
  | Where is a drawable's geometry? | — | Field 1 of the innermost message reached by walking field 1; one to four levels deep in this corpus |
  | What rectangle does the app report? | Pages says 60×123 475×383 for a photo whose archive says 33.86×66.28 511.86×466.13 | **A masked image is reported as its mask**, offset by the picture's own position |
  | …and a rotated one? | Keynote says 470×57 220×180 for a 220×180 shape at 100×100 turned 30° | Position is the **rotated bounding box's corner**; size is not rotated |
  | Move an image | One `TSD.ImageArchive` changed: position, size, **and `originalSize`** | Media keeps its placed size beside its geometry and both move |
  | Resize a shape | Geometry **and** the bezier path source — natural size and all six corners | **A shape's size lives in two places**; changing one leaves the app reporting the other |
  | Resize a masked image | Picture size, mask offset, mask size and the mask path's natural size all ×300/475; the mask path's *points* untouched | The assembly scales by one factor; masks take the natural size only |
  | Replace an image's file | New `DataInfo`, `naturalSize` and traced path rewritten to the new pixel size, `flags` 0→2, and a **mask installed** to re-fit the picture into the old frame | What a replacement has to maintain, and what this crate deliberately does not do |
  | Replace it again | The previous `DataInfo` **and its `Data/` entry vanished** | The media registry is refcounted, exactly like a table's string list |

  **The non-destructive-state finding, which was the phase's open question.**
  It is real, it is detectable, and replacement is safe only when it is absent.
  Between an image's stored pixels and what is drawn sit `mask` (5), an Instant
  Alpha path (10), twelve adjustments plus enhance (14), four references to
  renderings **derived from the old pixels** (13, 15, 16, 17), a traced outline
  (19) and `background_removed` (22). None of it is in a replacement file and
  none of it can be recomputed from one. `replace_media` therefore refuses by
  name — `Error::NonDestructiveEdit`, listing what it found — and the refusal
  leaves the document untouched, which `tests/drawables.rs` asserts object by
  object. **An identity mask is not an objection**: the app installs one when it
  replaces an image itself, and a window over the whole picture hides nothing.
  A traced path that is the plain rectangle of the picture's natural size — every
  one in the corpus — is rewritten with the new size, as the app rewrites it.

  The honest remainder: the frame does *not* follow the new picture. The app's
  own replacement scale-to-fills the picture into the old window and crops the
  overflow (an 8×8 frame given a 32×24 picture became a 10.67×8 image behind an
  8×8 mask offset by 1.33); this crate keeps the frame, so a replacement of a
  different aspect ratio is drawn stretched, says so, and points at
  `set-geometry`.

  **Digest, confirmed rather than believed.** `TSP.DataInfo` field 2 is a raw
  20-byte SHA-1 of the file's bytes: `shasum Data/probe-9077.png` and field 2
  agree, and `tests/drawables.rs::every_digest_is_the_sha1_of_the_bytes` checks
  every stored file in the corpus. SHA-1 is implemented in `src/media.rs` — sixty
  lines, four RFC 3174 vectors and the four padding boundaries — rather than
  taken as a dependency.

  **Stream-rewrite counts.**

  | Edit | Entries rewritten |
  |---|--:|
  | move and resize a shape (25-entry deck) | 1 (the slide) |
  | move or resize a masked image (37-entry Pages document) | 1 (the document) |
  | replace an image used by two slides (30-entry deck) | 3 (the metadata and both slides) |
  | write a rectangle an object already has | **0** |

  For contrast, the app's own save of one image move rewrote 21 objects, renamed
  the view-state component and reallocated its identifiers — the Phase 2 warning
  that an app save is not a small diff, confirmed again.

  **How close the writer gets to the app.** Asked to make the Pages report's
  cropped photo 300 points wide — the same edit Pages had been made to perform —
  this crate produced a **byte-identical mask archive** and an image archive
  differing only in the last two ulps of one float. The shape resize matches
  Keynote's output except where the app left −1.1e-13 and this crate leaves 0.

  **Five new `iwork check` invariants**, all kept by every fixture: a data
  reference resolves in the registry; a stored file is present and its digest is
  the SHA-1 of its bytes; `MessageInfo.data_references` (framing field 6, packed
  varints) lists exactly the data ids the payload uses — verified over every
  media-bearing drawable in the corpus; a mask's parent is the image that names
  it; a drawable's parent exists.

  **A registry correction worth its own note.** The framework ranges were wrong
  in two places: **5000–5999 is `TSCH`, not `TSS`**, so the six chart-style
  presets every document carries, and the eighteen axis and thirty-six series
  styles under them, were being reported as stylesheets and themes; and nothing
  lives in 1000–1999. `TSS` is 400–499 — the stylesheet is 401 and was not in
  the table at all. In the drawables block, 3016 is the media style rather than
  a theme and 3047 the guide storage; 2011 is `TSWP.ShapeInfoArchive`, which is
  what nearly every shape is, rather than a selection. All were Inferred.

  **New fixture: `keynote-shapes.key`.** Keynote is the only app that will make
  a drawable from a script — Pages and Numbers answer `make new shape` with
  "Don't know how to create TMAScriptShapeInfoProxy" — and `TSD` is cross-app, so
  one deck serves all three. It carries a shape, a shape turned 30°, a shape at
  50% opacity with a 40% reflection, a text box, a line, an image, a locked
  shape, and an image the app itself cropped by being told to show a square
  picture in a 4:3 frame. `make new group` and `make new movie` are accepted and
  do nothing, so groups and movies stay read-only.

  **What resisted.**

  - **A shape that sizes itself to its text has no size in the file.** Its
    stored height is 0, flags 1, and its stored position is the centre of a box
    that exists only once the text is laid out: Keynote reports such a text box
    58 points above the archive's position and 115 tall. `Geometry::fits_its_text`
    names the case; nothing here can resolve it without doing layout, and it is
    excluded from the oracle comparison for that reason.
  - **Groups and connection lines are decoded and unexercised.** No script
    groups anything and no bundled theme ships a group, so `TSD.GroupArchive`'s
    children list is read on trust.
  - **Gradient and image fills, tint, and every media kind but movies** are the
    same story: decoded, never seen. The two movies in the corpus are Keynote
    live-video placeholders with no film — ground rule 8 material, read and named
    and never authored.
  - **Non-proportional resizing of a masked image is Unverified.** Every image
    in the corpus has `aspect_ratio_locked` set and the app will not perform one,
    so the horizontal and vertical factors are applied separately on the strength
    of the proportional case alone.
  - **Pixel correctness cannot be verified here.** The app round trip proves the
    document opens, that the picture is still where it was and that the app
    accepts the registry entry. Nothing on a locked screen can see what is drawn.
  - **Inserting an image was not attempted** (the stretch goal). What it needs:
    an `TSD.ImageArchive` copied from one that works with a new identifier above
    `PackageMetadata` field 1; its two `StandinCaptionArchive`s copied with it,
    because every drawable in a Keynote theme has them; an entry appended to the
    container's drawable list (`KN.SlideArchive` field 7, `TN.SheetArchive` field
    2, or a Pages attachment table entry *and* a `TP.DrawablesZOrderArchive`
    entry); a `parent` pointing back; a new `DataInfo` and `Data/` entry, whose
    identifier comes from a counter this phase never had to find, because
    replacement reuses the one that is there; and `MessageInfo.object_references`
    and `data_references` written for the new object, which nothing here has had
    to synthesise yet — every write so far has only changed values inside an
    object that already declared what it points at.

  What Phase 4 (text) should know:

  - **A shape owns its text**, at field 2 of `TSWP.ShapeInfoArchive` (and field 4
    again, the same reference). `iwork drawables` prints the storage id, so the
    text in a slide's shapes is reachable from the drawable side.
  - **A Pages drawable can be anchored in text**, and then its `parent` is the
    body storage and its place is an entry in that storage's **attachment table,
    field 9** — `{1: character index, 2: → TSWP.DrawableAttachmentArchive}`. Any
    remapping of character indices has to move those entries with everything
    else, and dropping one detaches an image.
  - `TP.DrawablesZOrderArchive` (10015) lists every drawable in the document in
    one order, the body storage included; it is depth, not placement.

  What Phase 4 also wants from this phase, recorded here because it was found
  while looking for something else:

  - **A drawable already carries a hyperlink of its own**, at
    `Drawable::hyperlink` — separate from the text smart fields, and read since
    Phase 3.

  What Phase 8a (Keynote) should know:

  - **`KN.SlideArchive` field 7 is the slide's drawable list, in z-order back to
    front**, and fields 5, 6, 20 and 30 are placeholders that also appear in it,
    or in the layout's case do not. A slide's stream is `Index/Slide-*.iwa` and a
    layout's is `Index/TemplateSlide-*.iwa`, both holding a type 5 archive.
  - **Keynote will create a shape, a text item, a line and an image from a
    script**, and set position, size, rotation, opacity, reflection, lock and
    accessibility description on them — which is the whole basis of the shapes
    fixture. It will not create a group or a movie, and says "ok" when asked.
  - `set file name of image` really does replace the picture, which is how the
    app's own replacement path was observed.
  - The theme carries live-video sources, and a slide-number placeholder that is
    referenced from field 20 rather than from the drawable list.

- 2026-08-18 — **Phase 4 complete (text: the correctness fix).** The standing
  limitation is gone: `insert_text`, `delete_text`, `replace_text` and a
  `set_text` that is a full-range replace, all of which **remap** every table
  anchored into the storage instead of clamping it. `text::TABLES` is the
  twenty-two-table inventory, `iwork storages | links | insert-text |
  delete-text` are the new verbs, `iwork paragraphs` grew list level and style,
  `iwork check` gained four invariants, FORMAT.md §Text is rewritten and the
  README's clamping bullet is removed. `cargo fmt --check` and `cargo clippy
  --all-targets` clean; `cargo test --all-targets` green: 100 unit + 16 cell +
  18 drawable + 15 fixture + 34 style + 22 table + **14 text** + 3 doc.
  `IWORK_APP_CHECK=1` green over the whole suite, fifteen fixtures.

  **The headline number: ten edits, ten byte-identical archives.** Every rule
  below came from making Pages perform an edit and diffing the storage — the
  Phase 2 method, unchanged — and the way they were checked is that the crate
  was then asked to perform the same ten edits and its output compared with the
  app's *byte for byte*. All ten match.

  | # | Edit on `pages-styled` / a bolded copy | What the app did |
  |---:|---|---|
  | 1 | delete `[5, 20)`, across the first paragraph break | ¶ `0, 12, 74, 128` → `0, 59, 113` — the entry inside the range is **dropped** |
  | 2 | insert 8 units at 21 | → `0, 12, 82, 136` — a plain shift |
  | 3 | delete `[12, 20)`, beginning at a paragraph start | → `0, 12, 66, 120` — the entry **at** the start stays |
  | 4 | delete `[12, 74)`, one whole paragraph, break included | → `0, 12, 66`: **red survived at 12 and italic vanished** |
  | 5 | delete `[30, 90)`, middle of one paragraph to middle of the next | → `0, 12, 68` |
  | 6 | insert `\rNEU` at 21 | → `0, 12, **22 with no style**, 78, 132` |
  | 7 | delete `[19, 30)`, a character run's whole extent | the **character table field is removed** |
  | 8 | delete `[15, 25)`, across a run's start | char `0, 19, 30` → `0, 15, 20` |
  | 9 | delete `[24, 40)`, across a run's end | → `0, 19, 24` |
  | 10 | replace `[18, 19)` with four units, at a run boundary | → `0, 22, 33` — the run's start **moved**, so typed text joins the run before it |

  **Two different models, and that is the finding.** A *run* table behaves
  exactly like an attributed string: surviving characters keep their attributes,
  a run whose extent is wholly deleted disappears, and text arriving at a
  boundary joins the run before it. A *paragraph* table does not — case 4 is the
  proof, and it is counter-intuitive: deleting the whole of paragraph 2 left
  paragraph 3's text wearing paragraph 2's style. Paragraph style is anchored to
  the paragraph start, not carried by the characters. Getting this from a schema
  was impossible; getting it from reasoning would have got it wrong.

  **Two bugs the phase found in the crate, both in what a paragraph *is*.**

  - **`\r` ends a paragraph**, and it is the separator these apps write most
    often, because AppleScript's `return` is one. Four storages in the corpus —
    `pages-styled`, `pages-unicode`, two Keynote decks — were being read as one
    long paragraph. It hid behind the test that should have caught it, which
    skips a storage it believes has fewer than two paragraphs: exactly the shape
    the bug produced.
  - **`U+000C` ends one too** — the page or column break from the Insert menu.
    Found by running the new "a paragraph entry sits at a paragraph start"
    invariant over **all 901 template bundles the three apps ship**, where 40
    Pages templates put a run right after one in the shape `…\n\u{c}Text`.

  With all five break characters counted (`\n \r \f U+0005 U+0004`), every
  paragraph-anchored entry in all 901 bundles sits at a paragraph start or at
  the end of the text, and so does every one of the 389 storages in this
  repository's corpus. That sweep is the strongest evidence in the phase and
  cost one shell loop.

  **What is refused, and why refusing is the right answer.** Deleting the
  `U+FFFC` an image is anchored to made Pages **delete the image**: the
  `TSD.ImageArchive`, its mask and their places in the drawable list all went.
  That reaches the z-order and the media registry, which this phase does not
  touch, so `Error::AnchoredObject` names the object and leaves the document
  alone. A section break (`U+0004`) is the same case one character away — a
  section's entry sits on the character *after* its break, so what a delete
  destroys is the break — and merging two `TP.SectionArchive`s is Phase 4b's.
  Also refused: an index inside a surrogate pair, an index outside the text,
  text containing `U+FFFC`/`U+0004`/`U+0005`, and **a storage carrying a
  length-delimited field outside the inventory**, which is the boundary the
  brief asked for: no storage in the corpus or in any of the 901 bundles has
  one, and remapping a table nobody knows about is how an edit damages a
  document in silence.

  **`iwork check` learned four invariants**, all kept by the corpus and by all
  901 bundles: entry indices increase and fit the text; a paragraph-anchored
  entry sits at a paragraph start or at the end; a run or paragraph table begins
  at 0; an overlapping-highlight range fits and covers something. The second one
  caught `apply_text_style` writing a paragraph run inside a paragraph, which is
  the shape Keynote's own parser is documented as rendering 2^16 points tall
  before crashing; the range now grows to the paragraphs it touches.

  **Two fixtures, and what it took to get them.**

  - `pages-lists.pages`, from Apple's Real Estate Flyer: three named list styles
    in one storage, two paragraphs one level in, and a `U+0005` inside a
    paragraph. Pages' rich text carries `font`, `size` and `color` and no list
    property at all, so no script can make a list — a new
    `pages-from-template.applescript` builds it the way the Numbers fixtures are
    built.
  - `numbers-links.numbers` is **Apple's template bundle renamed**, and that is
    a first for this corpus. All 901 bundles were scanned for
    `TSWP.HyperlinkFieldArchive`: five objects, in three Numbers templates, none
    in the 640 Pages templates or the 182 Keynote themes. No app's dictionary
    has a link command (`sdef` over all three returns only `sourceURL`); setting
    a Pages body text to a sentence containing a URL does not auto-link it; and
    **instantiating any of the three templates strips the links**. A
    `.nmbtemplate` is the same ZIP a `.numbers` is, Numbers opens the copy and
    reads it back, and it carries the two run shapes a smart field comes in — a
    link terminated by an entry with no field, and a link running to the end of
    the text with no terminator at all.

  **What the apps accepted.** `tests/text.rs::the_app_opens_an_edited_document_and_reads_the_new_words_back`
  makes six round trips: text inserted into the middle of a styled paragraph and
  read back by Pages; a delete across a paragraph boundary read back; a
  replacement in `pages-report`, whose anchored photo and table are **still
  there and still at the same coordinates** afterwards, drawable by drawable;
  text replaced inside a Keynote shape and read back; a hyperlink repointed with
  text inserted in front of it, opened by Numbers, its run still covering the
  same words; and the edited deck decoded again with `check` clean.

  **One harness fault, of the class the README warns about.** The three
  `check-*.applescript` readers took `document 1`, which is the right document
  only when nothing else is open — not true of an app restoring its session, and
  not true when two test binaries drive the apps at once. The failure is a
  complete, plausible reading of *another file*: an edited deck reported the
  words it had before the edit, and only in the parallel run. They now wait for
  the document **by name**, matching the name with and without its extension,
  which is what `table-oracle.applescript` was taught in Phase 2 for the same
  reason. The lesson keeps arriving in the same envelope.

  **What resisted.**

  - **Changing a paragraph's list level was not attempted.** The level is read
    (field 6's `first`, confirmed against a Keynote theme whose five paragraphs
    are levels 0 to 4), but nothing here can make an app perform that edit, so
    there is nothing to check a write against. Ground rule 1 says that is not a
    write.
  - **Eight of the twenty-two tables were never seen**: bookmarks, footnotes,
    ruby, dictation, tracked insertions and deletions, comment highlights and
    pencil annotations are absent from the corpus *and* from all 901 bundled
    templates. They are named from the schema and remapped by shape. The
    overlapping-range remapping in particular has no document behind it and is
    Unverified.
  - **The character-style table is nearly untouched by the corpus**: two
    storages, in a Numbers pivot template. The rules for it come from the probe
    documents, which is why the probe documents were made.
  - **A link's target cannot be verified through an app.** No dictionary reports
    a URL. The app proves the document opens and the linked words are still
    there; the URL is checked by decoding.
  - **`check-numbers.applescript` was reading only tables.** A Numbers sheet
    holds shapes and text items like any other iWork container, and the
    templates put addresses, notes and the only hyperlinks in the whole install
    in them — so an edit to any of that came back as "not found" about a
    document that contained it. It now reads them, which is also what Phase 4b
    and 8a will want.

  What Phase 4b (Pages structure) should know:

  - **A section's entry is at a paragraph start, not on the break character.**
    `pages-report` has entries at 0 and 146, reading `…\n\u{4}Company Name`,
    with the entry on the `C`. Deleting a `U+0004` is refused here precisely
    because merging the two `TP.SectionArchive`s is 4b's job.
  - **A footnote body is a storage of `kind = 2`,** and there is not one
    anywhere: not in the corpus, not in any of the 901 bundles. A footnote
    fixture has to be authored by hand or not at all. The same is true of
    comments and of tracked changes.
  - `iwork storages` prints every storage with its `kind` and its tables, which
    is the tool for finding headers (kind 1), notes (4) and cells (5).
  - **`U+000C` is a page or column break** and a document with columns will have
    them; `TSWP.ColumnStyleArchive` (2024) hangs off `table_layout_style`
    (field 12), which four storages in the corpus carry.

  What Phase 7 (comments, metadata) should know:

  - **Comment anchors come in two tables and only one of them is a run table.**
    `table_highlight` (23) is the non-overlapping form; `table_overlapping_highlight`
    (25) carries explicit `TSP.Range`s so two comments may cover the same
    characters. This crate remaps both, and has never seen either.
  - **Tracked deletions leave their characters in the text.** A reader that
    ignores `table_deletion` (22) shows deleted text as live text, and an edit
    that ignores it moves a deletion marker onto text nobody deleted.
  - `TSWP.ChangeArchive` (2060) has no zero value for its kind — insertion is 1
    and deletion is 2 — so a default-zero enum there is an invalid archive
    rather than the first variant.

## Execution notes

- **Reference material lives in `reference/` (gitignored, like fixtures):**
  `features.md` — 735 document-affecting features enumerated from Apple's
  Pages/Numbers/Keynote user guides, with a gaps analysis this plan's
  1b/4b/8b phases came from; `distilled/` — per-domain field references
  mined from the extracted protos of numbers-parser, keynote-parser and
  iWorkFileFormat (clones in the session scratchpad under `reference/`).
  Every phase agent reads its domain's distilled file and its features
  section before touching code. `reference/protos-15.3/` holds the CURRENT
  schemas: descriptors carved from the installed 15.3.1 binaries (121 files;
  1300+ messages and a registry-dump of 580–633 type IDs per app, verified
  four independent ways) — prefer these over the older mined tables. That
  extraction closed the distilled set's thin spots: the TP/Pages 2013 gap
  (63→75 messages; the "page master"→"section template" rename plus a list
  of field-number reuses that silently mis-decode under 2013 schemas — see
  its NOTES.md before Phase 4b), and the TST category/pivot field tables
  (byte-identical 14.5→15.3.1). Confirmed renumberings: 5120→5143,
  5121→5142, 3046→220, 139→111. Outside TP the mined ID registry had zero
  errors. Two finds relevant to existing code: `TSP.Color` field 13
  `headroom` (HDR, default 1), and four new version-keyed style buckets in
  `TSS.StylesheetArchive` (fields 23–26). Still open: the `type == 0` diff
  mechanism (a named Phase 2 precondition).
- Parallelizable work (proto mining, feature enumeration, independent
  probes) runs as multi-agent workflows — authorized by the user
  ("ultracode"). Implementation phases that touch the tree stay sequential
  or use isolated worktrees.
- One Opus subagent per phase, sequential (they share the tree); the
  orchestrator reviews between phases: `cargo test`, `iwork check` +
  `iwork roundtrip` over the corpus, app round-trip spot checks, then commits.
- If a phase discovers the format resists its plan (it will), the agent's
  brief is to shrink scope and land what is *proven* rather than force the
  roadmap; this file gets amended to match reality.
