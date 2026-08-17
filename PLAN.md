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
7. **The app's UI defines the feature list.** AppleScript covers a fraction
   of what the apps can put in a document. Features are enumerated from the
   apps' menus/inspectors and from Apple's published user guides, and probe
   documents exercising them are produced by driving the UI (System Events
   UI scripting) when plain AppleScript cannot — then analysed with
   `iwork dump` to pin down what they write.
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

- [ ] Sort rules and filter rule sets (with the enabled/disabled toggle).
- [ ] Categories: source column, subcategories, group membership and order,
      summary-row aggregate assignments, per-group collapsed state.
- [ ] Conditional highlighting rules; custom cell formats (named,
      document-scoped, with conditional sub-rules).
- [ ] Pivot tables: source reference, field assignments, summary functions,
      display modes, totals toggles — read and inventory.
- [ ] FORMAT.md §Tables extended; pass-through tests so a save never damages
      any of it.

## Phase 2 — Tables: write

- [ ] **Precondition: pin down the `type == 0` diff/merge mechanism.** The
      distilled references show both Python parsers implement it incorrectly
      and incompatibly; before any table write, spec it from local probes
      and teach the test suite what it means.
- [ ] Edit an existing cell in place: text and number values first, then
      boolean/date; string-table maintenance; tile re-encode with the
      byte-identity rule for untouched tiles/streams. A written cell keeps
      (or is given) its data format — value and format travel together — and
      writing into a cell that carries a control definition preserves it.
- [ ] `iwork set-cell <doc> <table> <row> <col> <value> <out>`.
- [ ] App round-trip: Numbers opens the result and reports the new value via
      AppleScript. `iwork check` learns any invariant broken along the way.
- [ ] Stretch (only if in-place editing proves solid): add a row by copying
      an existing one.

## Phase 3 — Drawables, geometry, media

- [ ] Enumerate drawables: `TSD.DrawableArchive` subclasses — shapes, image
      drawables, groups, lines — with their geometry
      (`TSD.GeometryArchive`: position, size, rotation, flags) and z-order.
- [ ] Write geometry: move/resize a drawable; app-verified.
- [ ] Read object styling: fills (colour/gradient/image + tint), strokes,
      shadows, reflection, opacity, lock state — the `TSD` style surface.
- [ ] Media: replace an existing image's bytes (Data entry +
      `TSP.DataReference` + digest/metadata fields as observed); insert an
      image by copying an existing image drawable. App-verified. **Caveat
      proven by the guides:** a drawable carries non-destructive edit state
      (crop/mask rect, mask shape, Instant Alpha mask, ten tone/colour
      adjustments) between the stored pixels and the render — replacement
      must surface that state, or a swapped image opens fine and renders
      wrong while the app round-trip passes. Read it before writing bytes.
- [ ] Inventory (read-level) of the wider media model: video/audio with trim
      points and poster frames, galleries, drawings (stroke order is
      load-bearing — "Animate Drawing" replays it), 3D objects; pass-through
      tests for each.
- [ ] CLI: `iwork drawables`, `iwork set-geometry`, `iwork replace-media`.
- [ ] FORMAT.md: §Drawables, §Media.

## Phase 4 — Text: finish the story

- [ ] Fix the standing limitation, widened to its true scope: editing text
      remaps **every range anchored into the storage**, not just style runs —
      paragraph/character/list attribute tables, and equally tracked-change
      ranges, comment anchors, smart-annotation anchors, bookmark anchors,
      footnote anchors and ruby (phonetic guide) runs. The current clamping
      silently damages all of these today; this is a correctness fix, not a
      feature.
- [ ] Range operations: insert/delete text at a range, not just replace-all.
- [ ] Hyperlinks and smart fields: read them; edit a link target;
      app-verified.
- [ ] Lists: read list styles/levels per paragraph; change a paragraph's
      list level.
- [ ] The style-override flag (`TSS`): know whether a run is "named style"
      or "named style plus local overrides" — a prerequisite for preserving
      styling across edits, which this phase promises.
- [ ] FORMAT.md: §Text updated with the full attribute-table inventory,
      including bidi/vertical-text/ruby tables where the corpus can produce
      them.

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
