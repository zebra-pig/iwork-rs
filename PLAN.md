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
- [x] Stretch (only if in-place editing proves solid): add a row by copying
      an existing one. *(Done on `feature/add-row`, not merged: `insert_row`
      grows a plain single-tile table and Numbers reads the extra empty row
      back; every unproven case — categorised/filtered/multi-tile/merge-crossing/
      formula-crossing — is a named refusal. See the 2026-08-19 log entry.)*

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

- [x] Document mode: word-processing vs page-layout (`isMultiPage`),
      reported by `iwork inspect`; sections and section breaks, per-section
      page-numbering rules, backgrounds.
      *(The flag is `TP.SettingsArchive.body`, not an `isMultiPage` — there is
      no such field in the 15.3.1 schema. Confirmed three ways: the app's
      read-only `document body`, and the fact that of the 640 bundled templates
      the 388 with `body` false are **exactly** the 388 carrying a
      `TP.PageTemplateArchive`. Section ranges are checked against the app
      character for character.)*
- [x] Headers/footers (three zones, match-previous, hide-on-first-page);
      paper size/orientation/margins/facing pages.
      *(Three headers and three footers per section template, in 3144 template
      instances and 66 corpus ones, never any other count. The zone order
      left/centre/right is **Inferred** — the evidence is an LTR/RTL mirror
      pair. `orientation` is 0 even on landscape templates, so the page size is
      what says which way up it is.)*
- [x] Footnotes/endnotes: mode, markers, restart rules, note bodies as their
      own text storages (they must survive Phase 4's remapping).
      *(**Shrunk to a documented boundary.** The four settings are read and are
      the defaults in every document reachable from here. The containment is
      written up from the 15.3.1 schema and is **Unverified**: there is no
      storage of kind 2 and no `table_footnote` entry in this corpus or in any
      of the 901 bundled templates, no dictionary can author one, and no
      user-authored document is available. The reader reports and never fails;
      a test is the tripwire if that ever changes.)*
- [x] TOC (style-inclusion mapping), bookmarks (the anchor side of
      link-to-bookmark), page templates/masters, columns.
      *(TOC in four archives, with the finding that a document has **two**
      settings objects that disagree on purpose. Page templates, and "masters"
      is the 2013 word — 10143 is `TP.SectionTemplateArchive`. Columns are
      per-paragraph, not per-section, and their widths are **fractions of the
      text width**. **Bookmarks are the same boundary as footnotes**: not one
      of the 901 bundled templates has a `TSWP.BookmarkFieldArchive`.)*
- [x] Linked text boxes: the named thread joining boxes into one flow — a
      storage is not 1:1 with a drawable.
      *(Numbered, not named: `user_interface_identifier` is a thread's only
      identity.)*
- [x] Write (app-verified, smallest useful set): edit header/footer text;
      edit a footnote's text.
      *(Header and footer done, and verified the hard way — Pages has no header
      property at all, so the edited document is handed back to Pages, **saved
      by Pages**, and decoded again. **A footnote's text could not be edited
      because no footnote exists to edit.** Section-break deletion was found to
      be silently damaging and is now refused by name; see the log.)*
- [x] FORMAT.md: §Pages structure.

## Phase 5 — Formulas and the calculation engine (read)

- [x] Decode `TSCE` formula archives to an AST; pretty-print as the formula
      text the user typed (cross-checked against AppleScript's `formula`
      property, which is the oracle). *(273 of 273 formulas outside a pivot
      table match the app character for character. **A pivot is the named
      exception**: its formulas are `CATEGORY_REF_NODE`s, which point at a
      group rather than a rectangle, and its stored table is smaller than the
      one the app shows.)*
- [x] The reference model in full: absolute/relative flags per axis, named
      references, whole-row/column and header-name references, cross-table
      (`Table 2::B2`) and cross-sheet references — which resolve by table
      *identity*, not name string; stored error states. *(Proven by renaming
      the table after writing the formula. **A cross-sheet reference has no
      sheet in it**: a table's name is document-wide, and the `Sheet::Table::`
      form is Unverified because nothing here makes two tables share a name.)*
- [x] `iwork formulas <doc>`; cells CLI shows formula alongside cached value.
      *(And `iwork organise` prints each filter and highlighting rule's
      condition, which closes the gap Phase 1b left open.)*
- [x] FORMAT.md: §Formulas. Writing formulas is out of scope until reading
      is exhaustive. *(Nothing writes a formula; writing rule 17 says why, and
      `set_cell` still refuses a formula cell by name.)*

## Phase 6 — Charts (read)

- [x] Enumerate `TSCH` chart objects (cross-app: Pages and Keynote carry
      charts too), their type (~25 named types incl. 3D and interactive),
      and extract series/category data (charts carry a private copy of
      their data, *distinct from* their `TSCE` references back into tables —
      read both and say which is which). *(33 charts in four documents plus 94
      in the 69 bundled templates that have one; 23 of the 28 `ChartType`
      values observed, every 3-D family but the donut, one interactive chart.
      Nothing decodes a `TSCH.Generated.*` property archive — titles, axis
      labels, error bars, the 3-D scene and the interactive control style are
      carried and unread.)*
- [x] `iwork charts <doc>`. FORMAT.md §Charts.

## Phase 7 — Comments, metadata, document properties

- [x] Read annotations/comments and their anchors, authors storage —
      including resolved/unresolved state, reply threads (author + timestamp
      per reply), reviewer text highlights (annotation-layer, distinct from
      formatting highlight), and anchors into cells and chart elements, not
      just text. *(The author storage is decoded and is **empty in all 924
      documents on this machine** — 23 fixtures and all 901 bundled templates.
      Comments, replies, their three anchor kinds and the two highlight tables
      are decoded from the 15.3.1 schema and marked Unverified, because nothing
      here has one and no scripting dictionary will make one. Two tripwire
      tests fail the day a fixture does. **Resolved/unresolved state is not in
      the 15.3.1 `TSD.CommentStorageArchive` at all** — it has text, date,
      author, replies and a UUID, and no such field; whatever carries it is not
      in this schema and is not invented here.)*
- [x] Read+write document metadata (Properties.plist fields, custom format
      lists), regenerate `Metadata/DocumentIdentifier`/UUIDs correctly on
      "save as new document" so two edited copies don't collide in iCloud.
      *(Both plist forms read, the binary one written; `Document::save_as_new`
      and `iwork duplicate` implement the rule Pages' own Save As was measured
      applying. The iCloud half is Inferred — see the log entry.)*
- [x] Change-tracking: read-level survey only; document in FORMAT.md.
      *(All ten `TP` fields read and at their defaults everywhere; the two text
      tables decoded; an edit through either is refused by name.)*
- [x] Password detection, pulled forward from phase 9: `set password` turned
      out to be scriptable in all three apps, so a locked package could be
      *made* and measured rather than described. `Error::Encrypted`, a fixture,
      and the shape written down in FORMAT.md §11.

## Phase 8a — Keynote: inventory and text (app-verified)

- [x] Presenter-notes and slide-text extraction; slide/master/build/
      transition *inventory* (names and counts — parameters are 8b's job)
      surfaced in API + CLI. *(`Document::show|slides|slide_layouts`,
      `iwork slides|layouts`. The build inventory is honestly empty: there is
      no `KN.BuildArchive` in any of the four decks or in any of the 182
      bundled themes, and nothing can make one — see the log.)*
- [x] Write: edit slide text (title/body/notes) app-verified; duplicate a
      slide by copying; skip/unskip a slide; reorder slides.
- [x] FORMAT.md §Keynote extended with what the probes prove; registry
      evidence upgraded from Inferred to Confirmed where the app accepts it.
      *(§13 is now a full section; fourteen `KN` entries added or corrected,
      type 9 among them.)*

## Phase 8b — Keynote: builds, transitions, playback (read)

- [x] Build parameters: effect, direction, duration, build order, delivery
      mode (On Click / After Transition / With/After Build n + delay),
      action builds with motion paths, by-bullet-group text builds.
      *(Decoded from the 15.3.1 schema and honestly labelled **Unverified**:
      there is no build in six decks or in 182 themes and nothing can make
      one — 8a's boundary, respected. `keynote::Build` and `BuildChunk` report
      one if a deck from outside ever carries one, and
      `no_fixture_has_a_build_yet` fails the day a fixture does.)*
- [x] Transition parameters incl. Magic Move's match modes; playback
      settings (presentation type, loop, auto-advance); soundtrack.
      *(The `custom_*` block measured by diffing 44 otherwise identical slides,
      one per effect; the four scriptable playback settings measured against a
      deck that moves them; the presentation type and the two self-playing
      delays schema-only because nothing can move them; the soundtrack empty in
      every deck and its track list decoded and unexercised.)*
- [x] Recorded presentations: identify and pass through (never author —
      ground rule 8). *(And live-video cameras with them: one per deck, the
      collection's `default_source` and in no `sources` list.)*
- [x] FORMAT.md §Keynote: builds/transitions as observed.
      *(§13 gained six sections: transitions, the `custom_*` table with an
      evidence tag per field, direction, playback, the soundtrack, builds, and
      recordings and cameras.)*

## Phase 9 — Document creation and hardening

- [x] `Document::from_template(path)`: duplicate a document into a fresh
      identity (new UUIDs, cleared view state) — the copy-don't-synthesise
      answer to "create a document". Accept `.template`/`.kth`/`.nmbtemplate`
      bundles, which is what "create a document" means to a user. *(Done, and
      the identity rule turned out to differ from `save_as_new`: a document from
      a template gets a new `stableDocumentUUID` too. No view state to clear —
      no bundled template has any. `iwork new`.)*
- [x] Package-form documents: File > Advanced > Change File Type saves a
      real *directory* instead of a ZIP (Apple recommends it above ~500 MB).
      Detect and read both forms; `iwork inspect` says which it has. *(Read,
      written and preserved; the app opens one this crate wrote. All 901 bundled
      templates are ZIPs, so the hand-built package was the only route.)*
- [x] Encrypted documents: detect, fail with a named error (not a parse
      failure), refuse to write. The common hostile-bytes case. *(Done in
      phase 7 — the detection is exact and the fixture is real. What remains
      for phase 9 is the *package-form* variant of the same question, and
      whatever a hostile `.iwpv2` does to the fuzzer.)*
- [x] Decide and document the preview-staleness rule (byte-identity says
      leave `preview*.jpg`; correctness says they now lie — pick one,
      record it in FORMAT.md, teach `iwork check` to note it). *(Decided:
      leave them. Nothing refers to them, the app redraws them on its next
      save, and removing them would cost byte-identity. `iwork check` says
      nothing; `iwork inspect` says how many. `strip_previews` is offered and
      never automatic — FORMAT.md §1.)*
- [x] Fuzz the decoders (cargo-fuzz or dumb byte-mutation harness) so hostile
      files fail cleanly, never panic. *(`tests/fuzz.rs`: six levels, corpus
      seeded, deterministic, budget-bounded, `catch_unwind`. Five panics found
      and fixed. `cargo fuzz` needs nightly and this machine has stable only.)*
- [x] Final pass over README/FORMAT.md; verification table updated to match
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

- 2026-08-18 — **Phase 4b complete (Pages document structure).** New module
  `src/pages.rs`; `doc.structure() | sections() | header_footers() |
  column_layouts()`; `iwork sections` and `iwork structure`, and `iwork inspect`
  now says which of the two kinds of Pages document it has; four new `iwork
  check` invariants; `Error::SectionBreak`; two new harness scripts
  (`section-oracle.sh`, `resave.sh`); FORMAT.md §8; the `TP` registry block
  rebuilt from six entries to twelve, four of which were wrong; five new
  fixtures. `cargo fmt --check` and `cargo clippy --all-targets -D warnings`
  clean; `cargo test --all-targets` green: 107 unit + 16 cell + 18 drawable +
  15 fixture + **17 pages** + 34 style + 22 table + 14 text + 3 doc.
  `IWORK_APP_CHECK=1` green over the whole suite, twenty fixtures.

  **What the app was made to say, and what it would not.** Pages' dictionary
  has *three* structural properties in it — `document body`, the `sections`
  element, and `body text` of a section — and no header, footer, footnote,
  column or contents property at all. There is no `make new section`, and
  `delete section 2` answers **-10000**. So the phase divided in two: what the
  app could confirm, which was confirmed exactly; and what nothing could
  confirm, which is said out loud.

  | Question | How it was settled |
  |---|---|
  | Where does a section start and stop? | `body text of section i`, compared **character for character** across four documents |
  | Which mode is this document in? | `document body`, a read-only boolean, against `TP.SettingsArchive.body` |
  | Are the three header zones left, centre, right? | **Not settled.** Inferred from an LTR/RTL mirror pair |
  | What does merging two sections do? | **Not settled.** Pages refuses the edit four different ways |
  | Did a written header survive? | Pages opened the document and **saved it**; the text came back out of the file Pages wrote |

  **The section arithmetic, and it is exact.** A section's entry sits at a
  paragraph start, on the character *after* the `U+0004`, so section *i* covers
  `[start(i), start(i+1) − 1)` and the break belongs to neither side. Pages
  reports the three sections of `pages-report` as 145, 923 and 432 characters;
  the entries are at 0, 146 and 1070 in a 1502-unit storage. 146−0−1 = 145,
  1070−146−1 = 923, 1502−1070 = 432. The test compares the *text*, not the
  lengths, over `pages-report`, `pages-numbering`, `pages-book` and
  `pages-layout`.

  **Two independent signals for the document mode, and neither is
  `isMultiPage`** — there is no such field in the 15.3.1 schema. It is
  `TP.SettingsArchive.body`, false for a page-layout document. The second
  signal came from counting: of the 640 bundled Pages templates, the **388
  whose `body` is false are precisely the 388 that carry a
  `TP.PageTemplateArchive`** — equal sets, no exception either way. That took
  one scan and settles what a page template is for.

  **A page-layout document has no sections to the app.** `pages-layout` carries
  two `TP.SectionArchive`s with their six section templates between them and
  thirty-six header and footer storages, and Pages answers `count of sections`
  with 0. The `sections` element is word-processing only. The archives are still there and
  are still what the headers hang off, so the decoder reports them — and the
  oracle comparison stops at that point rather than pretending to disagree.

  **The bug this phase found in the crate, and it was Phase 4's own note that
  was wrong.** Phase 4 recorded that deleting a section break was already
  refused. It was not. The refusal lives in `destroyed_anchors`, which walks the
  *character*-anchored tables — and the same phase had established that a
  section's entry is anchored like a **paragraph**, so field 17 was never looked
  at. Deleting character 145 of `pages-report` went through quietly: one entry
  dropped, one `TP.SectionArchive` with nothing pointing at it, and with it
  three section templates, eighteen header and footer storages and a guide
  storage. The new `iwork check` invariant is what caught it — a
  section that does not begin at 0 begins after a `U+0004` — which is the phase's
  best argument for writing invariants down.

  **The section-merge decision: refuse, and the refusal is evidence-backed.**
  Four routes were tried and all four say the same thing — Pages will not
  perform this edit for anyone to watch:

  - `delete section 2 of document 1` → **-10000**, "AppleEvent handler failed";
  - there is no `make new section` in the dictionary;
  - the menu item needs a key window, which a locked screen does not have (the
    Phase 1b finding, unchanged);
  - `set body text of section 2 to ""` leaves the break **exactly where it
    was**, with a zero-length section behind it — so a section with no text is a
    legal state, and it is not a merge. (`set body text of section 2 to
    "Kurz."` likewise: entries moved 0, 146, 1070 → 0, 146, 152.)

  Since nothing can say which of two merged sections keeps its page templates,
  its eighteen header and footer storages, its guides and its background,
  `Error::SectionBreak` names the break, the section and what is unknown.
  `destroyed_sections` is a separate function from `destroyed_anchors` because
  the *kind* of answer differs: deleting an image's `U+FFFC` has an observed
  consequence and refusing is refusing to reproduce it; deleting a section break
  has no observed consequence at all.

  **Five fixtures, all from Apple's templates, chosen by scanning all 640.**

  | Fixture | Template | What only it has |
  |---|---|---|
  | `pages-book` | `11B_Novel_Modern` | facing pages (18 of 640), six sections |
  | `pages-toc` | `00C_Textbook_Portrait` | a table of contents (**2** of 640) |
  | `pages-layout` | `08_Journal_Newsletter` | page-layout mode, a page template, a linked-text-box thread (19 of 640), the only header and footer text in the corpus |
  | `pages-numbering` | `65_Sales_Bold_Report_PM` | page numbers restarting at 2, a section that hides its header on page one, a section background fill, a section hyperlink UUID |
  | `pages-columns` | `02_ResearchPaper_JP` | multi-column body — **the only one of the 640** — with both an equal and a non-equal layout |

  The last is Apple's bundle renamed, the second time this corpus has needed
  that: Pages on this machine does not offer the Japanese templates, so
  `every template whose id is "Application/02_ResearchPaper_JP/ISO"` comes back
  empty. A `.template` is the same ZIP a `.pages` is and Pages opens the copy.

  Corpus decode summary — `iwork structure`, no app involved:

  | Fixture | Mode | Sections | Header/footer storages | …with text | Page templates | Threads | Contents |
  |---|---|--:|--:|--:|--:|--:|--:|
  | `pages-plain` | word processing | 1 | 18 | 0 | — | — | 1 |
  | `pages-styled` | word processing | 1 | 18 | 0 | — | — | 1 |
  | `pages-unicode` | word processing | 1 | 18 | 0 | — | — | 1 |
  | `pages-lists` | word processing | 1 | 18 | 0 | — | — | 1 |
  | `pages-report` | word processing | 3 | 54 | 0 | — | — | 1 |
  | `pages-book` | word processing | 6 | 108 | 0 | — | — | 1 |
  | `pages-columns` | word processing | 2 | 36 | 0 | — | — | 1 |
  | `pages-numbering` | word processing | 2 | 36 | 1 | — | — | 1 |
  | `pages-toc` | word processing | 3 | 54 | 0 | — | — | **2** |
  | `pages-layout` | **page layout** | 2 | 36 | **12** | 1 | 1 | 1 |

  396 header and footer storages across ten documents and **thirteen of them
  have any text in them at all**, which is what an empty template's headers
  look like and why a fixture with header text had to be hunted for.

  **Findings worth keeping.**

  - **Column widths and gaps are fractions of the text width, not points.**
    The one non-equal layout reads `first 0.26090077`, `gap 0.035152942`,
    `width 0.7039463` — summing to exactly 1.0 — and the equal two-column
    neighbour's gap of 0.03527747 would be three hundredths of a point
    otherwise. Columns are also **per paragraph, not per section**: they hang
    off `table_layout_style` (field 12), and only the body storage has one.
  - **A table of contents has two settings archives and they disagree on
    purpose.** The document's own (`toc_styles`, scope 0) names the styles the
    whole document is judged by; each placed list carries its own (scope 1). In
    `pages-toc` that is two rules against six. Reading one is reading the wrong
    one.
  - **A thread is numbered, not named.** `TSWP.FlowInfoArchive`'s only identity
    is `user_interface_identifier`.
  - **A page number's *format* is not on the section.** The section says
    continue-or-restart-at-N; what the number is drawn as is a
    `TSWP.NumberAttachmentArchive` behind the `U+FFFC` in the footer, with
    `kind` (page number / page count / footnote mark) and
    `number_format_name`. 129 of them across the 640 templates and every one is
    kind 0, format 0, `"decimal"`, so the field is Confirmed and its other
    values are not.
  - **`orientation` is 0 even on the landscape templates.** Pages swaps width
    and height instead.
  - **A header's text is often not text.** The date in the newsletter's header
    is a `TSWP.DateTimeSmartFieldArchive` and the storage holds the string it
    last rendered to, so rewriting the header removes the field and freezes the
    date. A page number is a `U+FFFC` and goes the same way. Now a test.
  - **Four registry entries were wrong and all four were the 2013 gap.** 10012
    is the settings (not the theme, which is 10001); 10015 the z-order (not the
    settings); 10016 the guide map (not a `TP.BodyStorageArchive`, a message no
    version of the schema has); 10143 the **section template** (2013's "page
    master", and this table's "page layout"). Nothing else in the `TP` block
    misdecoded, because nothing else was being read.

  **Two harness faults, both of the class the README warns about — again.**

  `IWORK_APP_CHECK=1 cargo test` began failing on a *different* fixture each
  run, always with "the app that owns it would not open it", always on a
  document that opens perfectly on its own. Six test binaries now drive the
  apps and the lock serialises them, but a busy app can still miss a 120-second
  answer, and "no answer" was being reported as "refused". `app-check.sh` now
  makes **two attempts**, killing the app between them: a document the app
  genuinely refuses is refused twice, and a busy app is not busy from a cold
  start.

  Pulling that thread found the second, and it has been there since Phase 2.
  **`osa_acquire` was not re-entrant**, and `app-check.sh --self-test` calls
  `check` twice in one process — so the second call met a lock held by *its own
  pid*, found the holder alive, and waited the full `IWORK_APP_LOCK_TIMEOUT` of
  1800 seconds before stealing the lock from itself. Half an hour of a script
  sleeping, and the answer afterwards correct, which is the worst way for a bug
  to behave; it was invisible until this phase happened to run the self-test
  and watch the clock. `osa_acquire` now returns at once if `OSA_LOCK` is
  already set.

  The retry then went into the shared library as **`osa_try`**, and all five
  entry points that drive the apps use it — `app-check.sh`, the three oracles
  and `resave.sh`. It was needed: on one run Pages stopped answering during
  `section-oracle.sh` and took the section comparison down with it, on a
  document it had read correctly two minutes earlier.

  `--self-test` still refuses the corrupted copy, which is the check that the
  check is looking.

  **What has no source anywhere, stated plainly.**

  - **Footnotes and endnotes.** No storage of `kind = 2` and no `table_footnote`
    entry exists in this corpus or in any of the 901 bundled templates. Pages'
    dictionary has no footnote command. The four settings fields are read and
    are 0, 0, 0, 10 everywhere, so the *fields* are Confirmed and every
    non-default value is Unverified; the containment (a `U+FFFC`, a
    `TSWP.FootnoteReferenceAttachmentArchive`, its `contained_storage`) is
    written down from the 15.3.1 schema and has never been decoded.
    `no_storage_in_the_corpus_is_a_footnote_body` is the tripwire.
  - **Bookmarks.** All 901 templates the three apps ship were swept for three
    things at once — a storage of kind 2, a
    `TSWP.FootnoteReferenceAttachmentArchive` and a
    `TSWP.BookmarkFieldArchive`. **Zero, zero and zero.** Same rule, same
    honesty.
  - **`section_template_even_odd_pages_different` (19)** is 0 in all 1048
    sections of the bundled templates, and `section_start_kind` (20) is 0 in all
    of them too.
  - **The zone order** left/centre/right is Inferred. All three zones of a strip
    point at the same paragraph style, so alignment does not distinguish them;
    the evidence is that `08_Journal_Newsletter` fills zone 2 and
    `08_Newsletter_RTL`, the same design mirrored, fills zone 0.

  What Phase 5 (formulas) should know:

  - Nothing in this phase touches `TSCE`, but `pages-numbering` and
    `pages-report` both carry a calculation engine, and the Pages table in
    `pages-report` is the cross-app `TST` fixture Phase 1 used. Pages documents
    are a second oracle-free source of formulas.

  What Phase 7 (comments, metadata) should know:

  - **`Document::structure()` is the pattern for an app-specific reader**: one
    pass over `objects()` into a `BTreeMap<id, (type, Message)>`, then walk. It
    costs a decode of the whole document and is fine at this scale.
  - `TP.SettingsArchive` fields 12–17 are the change-tracking display switches
    (`show_ct_markup`, `show_ct_deletions`, `ct_bubbles_visibility`,
    `change_bars_visible`, `format_changes_visible`, `annotations_visible`) and
    `TP.DocumentArchive` has `change_tracking_enabled` (40), `change_sessions`
    (16) and `most_recent_change_session` (17). All present, none exercised.
  - **`resave.sh` is the tool this phase built and Phase 7 will want.** For
    anything no dictionary reports, having the app open the document and save it
    is a real acceptance test: what comes back was written by the app from its
    own model.

  What Phase 8a (Keynote) should know:

  - `resave.sh` takes any of the three documents, not just a `.pages`.
  - The **two attempts** in `app-check.sh` matter more the more binaries drive
    the apps; a Keynote phase adds another.

- 2026-08-18 — **Phase 5 complete (formulas and the calculation engine, read).**
  New module `src/formula.rs` (plus a generated `src/formula_functions.rs`),
  `Table::formula | formula_cells | names`, `table::formulas | names |
  predicate_text`, `iwork formulas`, formula text in `iwork cells` and rule
  conditions in `iwork organise`, a new `iwork check` invariant, FORMAT.md §9
  and writing rule 17, the `TSCE` registry block rebuilt from two entries to
  twelve, README rows, and a new fixture. `cargo fmt --check` and `cargo clippy
  --all-targets -D warnings` clean; `cargo test --all-targets` green: 115 unit +
  16 cell + 18 drawable + 15 fixture + **14 formula** + 17 pages + 34 style +
  22 table + 14 text + 3 doc. `IWORK_APP_CHECK=1 cargo test --all-targets`
  green over the whole suite, twenty-one fixtures. **Nothing writes a formula.**

  **The oracle, and it is the strongest one this repository has.** `formula of
  cell` does not hand back a structure — it hands back the *text a user would
  see in the formula bar*. The comparison is therefore character for character,
  and it is exact everywhere except a pivot table:

  | Fixture | Formulas the app reports | Matching exactly |
  |---|--:|--:|
  | `numbers-formulas` | 98 | **98** |
  | `numbers-rules` | 138 | **138** |
  | `numbers-sorted` | 15 | **15** |
  | `numbers-links` | 13 | **13** |
  | `numbers-values` | 7 | **7** |
  | `numbers-large` | 2 | **2** |
  | `numbers-pivot` | 32 | 0 — deferred, see below |
  | **total** | **305** | **273 of 273 outside a pivot** |

  Structural evidence that needs no app: **907 node arrays, 1582 nodes, across
  all twenty-one fixtures**, every one decoding, validating field by field
  against the 15.3.1 schema and re-encoding to the bytes it came from. Forty
  node types and forty-eight function ids appear in the corpus.

  **The new fixture is a zoo, and it exists because AppleScript can put any
  string into a cell.** No script here will make a category, a filter or a
  chart — but a string beginning with `=` is a formula, so the whole of `TSCE`
  is reachable from a script even though none of its structures are.
  `numbers-formulas.numbers` carries **ninety-five cases**, one per node type,
  operator, reference shape, literal kind and naming rule, in nine tables shaped
  to exercise the naming rules (no headers, two header rows, a duplicate header
  name, header names full of punctuation). The builder reports every case the
  app *refused* rather than leaving a silent hole: three were, and were
  replaced — Numbers 15.3.1 parses neither a direct lambda application
  (`LAMBDA(x,x+1)(2)`), nor a duration literal (`1d 4h`), nor a date literal.

  Two of the cases are only possible because the fixture is built rather than
  found, and both are the phase's key proofs: the table `Alt` is renamed to
  `Neu` **after** the formula pointing at it is written, and a column is removed
  **after** the formula pointing at it is written.

  **The identity proof.** `=Alt::A1` is written, `Alt` becomes `Neu`, the
  document is saved. Afterwards the string `Alt` is nowhere in the file, the AST
  is unchanged, and both Numbers and this crate print `=Neu::A1`. What an AST
  carries is a `TSP.CFUUIDArchive`, and it is **not** the table's `table_id` and
  **not** its `haunted_owner.owner_uid` — it is the `base_owner_uid` of the
  `TSCE.FormulaOwnerDependenciesArchive` whose `owner_kind` is 35. Matching on
  either of the other two finds nothing at all, which is how the walk was found:
  the first two candidates missed every table in the document. In this corpus
  the base is the haunted UUID's lower half minus 35 — every owner a table has
  is a numbered offset from one base — but the join is a lookup and never
  arithmetic. The CFUUID's four words map to `TSP.UUID` as
  `lower = w0 | w1 << 32`, `upper = w2 | w3 << 32`, checked word for word
  against all nine tables of the zoo.

  **The 35/36 finding, which was the named trap.** Fields 35 and 36 of
  `ASTNodeArchive` changed type *in place* at 14.4: 35 from a nested
  `ASTNodeArrayArchive` to a `string`, 36 from a nested `ASTLetNodeWhitespace`
  to a `bool`. Field 35 keeps wire type LEN, so a ≤13.1 decoder parses a
  whitespace string as a nested AST and says nothing is wrong. **15.3.1 writes
  the new shape and this corpus proves it.** `=LET(x,2,y,3,x×y)` produces two
  `LET_BIND_NODE`s, `{1: 52, 34: "x", 36: 0, 37: 1}` and
  `{1: 52, 34: "y", 36: 1, 37: 2}` — field 36 is a varint on the wire, and it
  carries the meaning the 14.4 name gives it, because there are also **two**
  `END_SCOPE_NODE`s (one per binding) and only one `LET(…)` in the text. The
  decoder's schema table gates on the version and never on the wire type found;
  a unit test asserts that the old shape at field 36 is refused.

  **What a formula's text actually needs, none of which is in the file.**

  - **Header names.** Numbers prints references by the *text of header cells*,
    worked out at render time. Nine rules, all read off the zoo: the last header
    row names a column and the last header column names a row; a cell reference
    uses names only when it has both and only for a body cell; a whole-column
    reference uses the column name alone; **a range never uses names**; a name
    that names more than one row is not a name (`numbers-links` has eleven rows
    whose header cell reads `Item name`, and the app prints `=C2×D2`); a name
    unique in the document needs no table prefix even across tables; where two
    tables share a name the **first** keeps it bare (`SUM(Menge)` for `Daten`,
    `SUM(Daten2::Menge)` for `Daten2` — which table counts as first is
    *Inferred*, and this crate uses creation order); `$` goes in front of the
    name of the axis it anchors; and a name containing an operator character is
    single-quoted with an embedded `'` doubled — `A+B` → `'A+B'`, `it's` →
    `'it''s'`, while `x y` and `SUM` are printed bare.
  - **Unicode operators.** `×`, `÷`, `−` (U+2212, for subtraction *and*
    negation), `≥`, `≤`, `≠`. ASCII does not match the app.
  - **Function names are not localised**, and the separator is a comma. On a
    machine driving Numbers in German the oracle answers `SUM`, `VLOOKUP`,
    `IFERROR`, `FIND.CASEINSENSITIVE`.
  - **Whitespace is nodes.** `APPEND_WHITESPACE_NODE` appends to the top of the
    stack and `PREPEND_WHITESPACE_NODE` prepends to it; `= B60 + 1` round-trips
    with its spaces where the user put them.
  - **Parentheses are stored, not implied.** `=(B11+1)×2` writes a `LIST_NODE`
    with one argument, so nothing here reasons about precedence.

  **Findings worth keeping.**

  - **A formula archive in a data list carries no host cell.** All four host
    fields are absent from every entry in this corpus, so relative references
    resolve against the cell holding the key — which is why one entry can serve
    many cells. `=SEQUENCE(1,3)` spills into the two cells beside it and all
    three share one key.
  - **A number literal is stored twice** and the decimal128 is the one that is
    right. `high == 0x3040000000000000` means an exact integer and `low` is it;
    otherwise the value is `±mantissa × 10^exponent` and rendering from the
    digits is what keeps `=0.1+0.2` from printing as the nearest double.
  - **The two coordinate encodings are adjacent and different.**
    `AST_column`/`AST_row` are zigzag `sint32`; a colon tract's relative offsets
    are plain `int32` varints, so −1 is ten bytes. An omitted `range_end` means
    "the same as `range_begin`". Rows saturate at `0x7fffffff` and columns at
    `0x7fff`.
  - **`B` and `B:B` produce the same archive**, and the app prints one of them
    back. So does a whole row typed as `2:2`.
  - **Two function ids have no published name.** 337 is the internal function
    behind a spilled cell, and **Numbers itself prints `(null)` for it** — so
    this crate prints `(null)` too and matches the app exactly. 175 appears only
    inside a `TN.ChartMediatorArchive`, wrapping each of a chart's operands.
    Both sit in the mined table's holes.
  - **A well-formedness check is what makes a document-wide walk safe.** A
    `TSCE.CellRecordExpandedArchive` is `{1: column, 2: row}`, which is a legal
    *node* (`MULTIPLICATION_NODE` with function index 0) and a nonsensical
    *program*; requiring the node stream to evaluate without underflowing its
    stack rejects it. Without that check the corpus appeared to contain node
    types in the tens of millions.
  - **A pivot's stored table is its base and the app shows a larger view.**
    `Sales Pivot` is 7×5 on disk and 10×6 to Numbers; the grand-total row and
    column exist only in the view, and seventeen of the app's thirty-two
    formulas for that table sit where there is no cell record at all.
  - **Every document has a calculation engine**, Pages and Keynote included,
    with the empty owner-dependencies and named-reference archives that go with
    it. `pages-report.pages` has four real formulas in its table, including
    `=SUM(D)` — and **no dictionary will report them**, because Pages has no
    table property at all. That decode is verified structurally and by nothing
    else.
  - **There are no named ranges.** No `TSCE` archive stores one; a "name" is a
    header cell's text. The tracked-reference store holds the ASTs of the
    references the engine is tracking, which in this corpus are exactly the
    header-cell references.

  **Phase 1b's opaque predicate is now readable.** `numbers-rules`'s filter rule
  reported as "predicate 37 against a formula" is
  `=IF(LEN("–")≠LEN(A3),TRUE,IF(ISERROR(FIND.CASEINSENSITIVE("–",A3)),TRUE,FIND.CASEINSENSITIVE("–",A3)≠1))`
  — "does not begin with an en-dash" — and its four conditional-highlighting
  rules read `=#CELL>0`, `=#CELL<0` and two `FIND.CASEINSENSITIVE` tests for an
  arrow. `#CELL` is this crate's spelling, not the app's: the subject of a
  highlighting rule is a `LINKED_CELL_REF_NODE` with no coordinates at all, and
  no dictionary reports a conditional rule, so there is no app text to match.

  **What resisted.**

  - **A pivot's category references are decoded and not printed the way the app
    prints them.** `TSCE.CategoryReferenceArchive` is read in full — group-by
    owner, column, aggregate code, group level, group path — but Numbers spells
    one `Source::$Units $January::Electric::Bicycles (Sum)`, which needs the
    source table's group tree, the names of its groups, the aggregate's name
    (Apple publishes none; only `2 = Sum` is proven) and the rule for where the
    `$` markers go. One document is behind all of it. The text is `#CATEGORY!`
    and the oracle test counts and names those cells rather than skipping them.
  - **`DATE_NODE` and `DURATION_NODE` literals were not obtainable.** Numbers
    15.3.1 refuses `=1d 4h` and every date-literal spelling tried; the nodes are
    decoded by shape and are Unverified. So are `TOKEN_NODE`, the two legacy
    reference nodes, `COLON_NODE`, `UID_REFERENCE_NODE`, `COLON_NODE_WITH_UIDS`,
    `VIEW_TRACT_REF_NODE`, `INTERSECTION_NODE` and the two linked column/row
    refs.
  - **A cross-*sheet* reference has no sheet in it.** A table's name is
    document-wide, so `=Fern::A2` reaches a table on another sheet with no sheet
    prefix at all. The `Sheet::Table::` form the references describe is
    Unverified: nothing here makes two tables share a name.
  - **Keynote has no formula fixture**, for the same reason it has no table one.
  - **The dependency graph is untouched.** `CellRecordTileArchive`,
    `RangePrecedentsTileArchive` and the packed `EdgesArchive` words are carried
    through and not decoded; nothing about formula *text* needs them, and the
    packed edge layout is unverified in every published source.

  What Phase 6 (charts) should know:

  - **A chart's `TSCE` references are in its `TN.ChartMediatorArchive` (12006)**,
    and every one of them wraps its single operand in **function 175**, which
    has no published name. `numbers-rules` has 51 of them. That is the "distinct
    from the chart's private copy of its data" half of the phase's brief, and it
    is already decodable: `formula::Formula::decode` on the archive, then
    `Reference::resolve` against the host.
  - The chart-style archives (5022–5031) are the shape that fooled the first
    version of the AST walk here: `{1: varint, 2: message}` repeated, which is a
    node array to a decoder that only checks field numbers. Wire types and the
    stack check separate them.

  What Phase 2's stretch (adding a row) and Phase 7 should know:

  - **A formula's references are indexes, not UUIDs** — except a `#REF!`, which
    is UUIDs. Inserting or deleting a row moves every relative reference that
    crosses it, and the app rewrites the ASTs when it does that
    (`TSCE.FormulaRewriteCommandArchive` is the undo record of exactly that).
    Any row insert that does not rewrite formulas produces a document that opens
    and computes the wrong answer, which is worse than one that refuses.
  - **A cell caches its formula's result and the app trusts it until it
    recalculates.** That is why `set_cell` refuses a formula cell, and why
    writing a formula is a phase of its own rather than a small addition to this
    one — writing rule 17 lists what it needs.

- 2026-08-18 — **Phase 6 complete (charts, read).** New module `src/chart.rs`,
  `doc.charts()`, `iwork charts`, four new `iwork check` invariants, FORMAT.md
  §10 and a correction to §3, the `TSCH` registry block from three entries to
  sixteen, README rows, and two new fixtures. `cargo fmt --check` and `cargo
  clippy --all-targets -D warnings` clean; `cargo test --all-targets` green:
  120 unit + 16 cell + **18 chart** + 18 drawable + 15 fixture + 14 formula +
  17 pages + 34 style + 22 table + 14 text + 4 doc. `IWORK_APP_CHECK=1 cargo
  test --all-targets` green over the whole suite, twenty-three fixtures.
  **Nothing writes to a chart.**

  **There is no read-back oracle for a chart, and that is a fact about the
  apps, not a gap in the harness.** `chart` is an element of a Keynote slide, a
  Numbers sheet and a Pages document, and in all three dictionaries the class is
  `<class name="chart" inherits="iWork item">` with **no properties of its
  own** — position, size, rotation, opacity, and not a word about type, data or
  series. `formula of cell` has no counterpart here.

  **So the oracle is the input.** Keynote's `add chart` is the only
  chart-creating command in any of the three apps and it takes row names, column
  names, a grid of numbers, a type and a grouping. `keynote-charts.key` is
  eighteen charts built that way, each with its own hundred — chart *i* holds
  `i×100 + 1, 2, 3` and `+11, 12, 13` — so no two share a value and a
  mis-ordered read cannot pass. **All 108 values, both row names and all three
  column names, agree for all seventeen types**, plus the eighteenth chart,
  which is grouped by column and whose series is therefore `Jan = 7001, 7011`
  rather than a category.

  Chart-type coverage, and the mapping that had to be measured:

  | Source | Types |
  |---|---|
  | Keynote zoo (17 slides) | 1–9, 12–19 — **every 3-D family but the donut** |
  | `21_Simple_Charts` (12 charts) | 1, 2, 4, 5, 6, 9, 11, 20, 22, 25, 27 |
  | `numbers-rules`, `pages-numbering` | 2, 8, 11 |
  | **corpus total** | **23 of the 28**, all named |

  Never seen: 0 `undefinedChartType`, 10 `mixedChartType2D` (one exists, in
  `28_GradeBook`, and no fixture takes it), 21, 23, 24 (the multi-data bar,
  scatter and bubble charts) and 26 `donutChartType3D`.

  `add chart`'s `type` is a *legacy* seventeen-value enumeration that predates
  `TSCH.ChartType`, and the mapping is neither the identity nor an ordering:
  `vertical_bar_2d` is a **column** chart (1) and `horizontal_bar_2d` is a bar
  chart (2). The whole table is in the fixture script's header and in FORMAT.md
  §10.

  **The private copy and the live references, which was the phase's brief.**

  | | Pages | Keynote | Numbers |
  |---|---|---|---|
  | `ChartArchive.grid` (field 7, inline) | yes | yes | yes |
  | `ChartArchive.mediator` (field 8) | **no** | **no** | yes |
  | what it draws | the grid | the grid | the grid |
  | what it follows | nothing | nothing | the mediator |

  The grid is what is drawn in all three. In Numbers it is a **cache** of what
  the mediator's `TSCE` formulas last evaluated to; elsewhere it is the data and
  there is nothing behind it. All twelve Numbers charts have a mediator with one
  data formula per series; all nineteen Pages and Keynote charts have none.
  `iwork charts` prints the grid as a table and then either `fed by
  Comparison of Units Sold by Year!B2:D2` or `private data only — no mediator,
  nothing to follow`.

  **Function 175, and the correction to Phase 5's note about it.** Every
  reference a mediator holds goes through a `FUNCTION_NODE` of index 175 —
  **915 of them across the 69 chart-bearing bundles, not one unwrapped** — and
  175 has no published name. But it is **not** the one-argument wrapper Phase 5
  recorded: its arity is nought, one or three, because a series may be fed by
  several disjoint ranges (`175(B7, D7, F7)` in `26_MortgageCalculator`) and a
  label list may name nothing (`175()`). The node is dropped and its *operands*
  are printed, one per value the fragment leaves on the stack — which is why
  `Formula::text` moved down onto `Ast` and gained `Ast::texts`.

  Two more things a mediator holds that are not references: a bubble chart's row
  labels are string **literals** written into the mediator, and a `#REF!`
  survives there like anywhere else. Both are in `21_Simple_Charts` and both are
  asserted.

  **The trap the domain is known for, and it bit.**
  `TSP.SparseReferenceArray.count` was the named one and cost nothing — 66
  arrays in the corpus, no gaps, so it is asserted from the other side (every
  index inside the count, the dense form as long as the count). The one that
  actually bit was **the blank grid cell**: a blank is a *present, zero-length*
  `GridValue`, this crate's `decode_nested` refuses empty bytes on purpose, and
  the first grid reader therefore filtered the blank out — which does not lose a
  blank, it **shifts every value after it one column left**. Empty is handled
  before the decode now, on both levels, with a unit test built from bytes.
  No fixture has one: `missing value` in `add chart`'s data is refused with
  -1700, so the real-document behaviour is Inferred.

  **A bug in Phase 3, found because a chart was in the wrong place.**
  `TP.FloatingDrawablesArchive` is **two levels**: field 1 is one entry per
  *page*, `{1: page index, 4: [{1: reference}]}`, and Phase 3 read field 1 as a
  list of references — taking the page number for an object identifier and
  recording nothing. **Every floating drawable in every Pages document has been
  `Placement::Unknown` since Phase 3**, invisible because the fixtures that had
  one had it anchored in text instead. `pages-numbering` puts its chart on the
  page, and "unplaced" is an unacceptable answer to where a chart is.
  `Placement::Floating` now carries the page number.

  **A correction to Phase 2, and the first `diff_field_path` in this corpus.**
  Phase 2 measured that 15.3.1 writes exactly one patched object per Numbers
  document — the `TN.UIStateArchive` — and that `diff_field_path` is absent
  from everything. **A chart of a type that did not exist in an older release
  patches itself**, and it uses the field path:

  | | donut | radar |
  |---|---|---|
  | base `MessageInfo.version` | `[2, 0, 25]` | `[2, 0, 25]` |
  | `diff_merge_version` (8) | `[2, 3, ∞]` | `[11, 1, ∞]` |
  | `diff_field_path` (9) | `{1: [10000]}` | `{1: [10000]}` |
  | payload | `{1: 5}` — pie | `{1: 2}` — bar |

  The path reaches into `TSCH.ChartArchive` and the whole patch is
  `chart_type`, set to the nearest type the older app has. Donut arrived in 10.2
  and radar in 11.2. The standing rule is unchanged and now has a second reason
  to exist: never rewrite the first message of an object with patches.

  **Structural evidence that needs no app.** 2090 chart-domain archives across
  the corpus decode and re-encode to the bytes they came from; 33 chart
  drawables are all exactly the two-field sandwich `{1, 10000}`; every grid is
  rectangular with one name per row and per column; every chart is placed; and
  the four new `iwork check` invariants are kept by every fixture **and by all
  69 chart-bearing bundles**, which were also checked one by one.

  Corpus decode summary — `iwork charts`, no app involved:

  | Fixture | Charts | Types | Mediators |
  |---|--:|---|--:|
  | `keynote-charts.key` | 18 | 1–9, 12–19 | 0 |
  | `numbers-charts.numbers` | 12 | 1, 2, 4, 5, 6, 9, 11, 20, 22, 25, 27 | 12 |
  | `numbers-rules.numbers` | 2 | 2, 11 | 2 |
  | `pages-numbering.pages` | 1 | 8 | 0 |

  **The two fixtures, and why each had to come from where it did.**
  `numbers-charts.numbers` is Apple's `21_Simple_Charts` instantiated, because
  Numbers has no chart command at all — all 901 bundles were scanned, 69 have a
  chart, and that one has twelve covering eleven types including the only
  interactive chart and a two-axis chart. `keynote-charts.key` is built, because
  a template cannot give known values.

  Three AppleScript findings, each of which cost a run and all three now in the
  script's header:

  - the chart-type constants only resolve **inside** the `tell application`
    block — outside it `pie_2d` is an undefined variable;
  - every slide must exist before any chart goes on: interleaving `make new
    slide` with `add chart` made Keynote lose the document reference (-1728,
    "Can't get document id …") halfway through the run;
  - **`add chart` ignores the slide it is given.** Its direct parameter is
    documented as "the slide to add the chart to" and is not used — every chart
    lands on the document's *current slide*, so the first build had eighteen
    charts stacked on one. `set current slide of doc` before each call is what
    places them. (`count of charts of slide 1` also answered 0 for a slide that
    had one, which is the same fact from the other side.)

  **What resisted.**

  - **Every `TSCH.Generated.*` property archive.** The six presets and the
    axis, legend and series styles under them are enumerated, counted and
    carried; **not one property inside them is read.** Titles, axis labels and
    ranges, number formats, error bars, trendlines, gap widths, corner radii,
    the interactive chart's control style (Buttons Only vs slider) and the whole
    3-D scene — depth, lighting, bevel, rotation — all live there. This is by
    far the largest thing the phase does not do, and it is a phase of its own:
    the property numbering is stable in the *persisted* archives but the
    property *set* is chart-family-dependent, and a series fill read without the
    family rule is the wrong colour.
  - **3-D is read-level only**, as the brief allowed. The type says a chart is
    3-D and `scene3d_settings_constant_depth` (10002) is on every chart, 3-D or
    not; the scene payloads are `TSCH.Chart3D*` messages inside a style's
    extension 10000, and `TSD.FillArchive` gains a `fill3d` at extension
    **100** — one of the places a decoder that assumes "TSCH extensions start at
    10000" silently loses data.
  - **The interactive chart's control style is not decoded**; only its current
    data set is, and that is `ChartArchive.multidataset_index` in the *model*.
    The schema's other home — `TSCH.ChartUIState` at `TN.UIStateArchive` field
    23, with an `upgraded_to_ui_state` flag at extension 10021 — is **absent**:
    the document's view state has no field 23 and no chart carries 10021.
  - **The pre-UFF chart model (5000–5017) has never been decoded.** Not one of
    the 901 bundles has one, so the legacy grid's `repeated double` rows — which
    cannot express a blank at all, which is why old imported charts show
    spurious zeroes — is written down from the schema and asserted absent by a
    tripwire test.
  - **Reference lines.** Extension 10005 is on every chart and holds an empty
    `ChartReferenceLinesArchive`; 5030 is in every document as part of the
    theme; **5031, the non-style, is in none of the 901 bundles**, because no
    template has a reference line and no script can add one.
  - **Five chart types have no source**: 0, 10, 21, 23, 24 and 26. Mixed (10)
    exists in `28_GradeBook` and was not worth a fixture of its own; the three
    multi-data variants and the 3-D donut exist in no bundle at all and no
    script makes one.
  - **`local_series_indexes` / `remote_series_indexes`** are read and reported
    and their meaning when they disagree is unexercised — `numbers-rules` has a
    chart whose local index is `0xFFFFFFFF`.
  - **Pixel correctness, as always, cannot be verified here.** The app round
    trip proves the documents open; nothing on a locked screen can see what is
    drawn.

  What Phase 7 (comments, metadata) should know:

  - **A chart element is one of the three comment anchor kinds**, and this phase
    did not meet it: no fixture and no bundled template has a comment on a chart.
    What a chart-anchored comment would hang off is `TSD.DrawableArchive.comment`
    (field 6) on the 5021 — the same field an image's comment uses, already read
    by `Drawable::comment` — or, for a *sub-element* of a chart, something in
    the `TSCH` selection-path archives (5145–5152), none of which is persisted
    in any document here.
  - `Document::charts()` follows `Document::structure()`'s pattern: one pass
    over `objects()` into a `BTreeMap<id, (type, Message)>`, then a walk over
    `drawables()`. It costs a decode of the whole document twice over and is
    fine at this scale.

  What Phase 8a/8b (Keynote) should know:

  - **Keynote will make a chart from a script**, which is the only drawable
    besides shapes, lines, text items and images that it will make, and the only
    one where the app is told the *content*. `keynote-charts.key` is nineteen
    slides and is now the largest deck in the corpus.
  - **`add chart` ignores its direct parameter and uses the current slide.**
    Anything else that takes a container from a script deserves the same
    suspicion; `make new shape at end of shapes of slide i` does honour it.
  - A chart on a slide can carry a **Magic Chart build** (features.md §Keynote),
    which is 8b's; nothing here touches `KN` build archives.

  What Phase 9 (hardening) should know:

  - **A chart's grid is a cache and a document can be internally inconsistent
    on purpose.** Nothing here recalculates one, so `iwork check` deliberately
    does *not* compare a Numbers chart's grid against the cells its mediator
    points at: they legitimately disagree between the app's last recalculation
    and the next open. If that check is ever wanted it has to be a note, not a
    problem — the same shape as the stale formula cache in §9.

- 2026-08-18 — **Phase 7 complete (comments, metadata, document properties).**
  Three new modules — `src/plist.rs`, `src/metadata.rs`, `src/annotations.rs`;
  `doc.metadata() | annotations() | save_as_new()`; `iwork metadata |
  annotations | duplicate`, three new lines on `iwork inspect` and a tracking
  line on `iwork structure`; two new errors (`Encrypted`, `TrackedChanges`);
  three new `iwork check` invariants; a new fixture (`pages-locked.pages`) and
  the builder for it; FORMAT.md §11 and §12 and writing rules 19–21; two
  registry entries corrected and seven added. `cargo fmt --check` and `cargo
  clippy --all-targets -D warnings` clean; `cargo test --all-targets` green:
  **139 unit** + 16 cell + 18 chart + 18 drawable + **24 fixture** + 14 formula
  + **18 pages** + 34 style + 22 table + **15 text** + 4 doc.
  `IWORK_APP_CHECK=1` green over the whole suite, twenty-four fixtures.

  **The probe that changed the phase.** The brief asked two questions of the
  sdefs before anything else, and both answers were decisive.

  | Probe | Answer |
  |---|---|
  | Does Pages expose change tracking? | **No.** `sdef` finds the string "change" nowhere in Pages.sdef. No `change tracking enabled`, no accept/reject, nothing. Tables 21/22 stay Unverified |
  | Does anything set alt text? | **Yes**, `description` on an `image`, read-write, cocoa key `scriptAccessibilityDescription` — but phase 6 already reads it (`TSD.DrawableArchive.accessibility_description`, field 8) and nine fixtures carry one |
  | Does anything set a password? | **Yes, and this was not expected.** `set password`, `remove password` and a read-only `password protected` are in the *shared* iWork suite, so all three apps have them |

  So the encryption item was pulled forward from phase 9 and done with real
  evidence instead of from format knowledge, and change tracking stayed a
  read-level survey exactly as the plan predicted.

  **What a password does to a package**, from four locked documents — one per
  app, plus a re-lock with a different password and no hint:

  | | Locked |
  |---|---|
  | `.iwpv2` | new, **104 bytes in all four**, beginning `02 00 01 00 A0 86 01 00` — 2, 1, 100000 — then 96 bytes that differ every time |
  | `.iwph` | new, the hint as raw UTF-8; absent when no hint was given |
  | `Index/*.iwa`, `Data/*`, `BuildVersionHistory.plist` | ciphertext |
  | `Properties.plist`, `DocumentIdentifier` | plain, unchanged in shape |
  | `preview*.jpg` | **gone** |

  Detection is the presence of `.iwpv2` and it is exact: no unencrypted
  document in the corpus or in any of the 901 bundles has one.
  `Document::open` refuses before anything tries to decompress a stream, which
  it previously reported as "unexpected chunk marker 0x56".

  **The identity rule, and how it was settled.** `pages-plain.pages` was opened
  by Pages and written to a second path with `save doc in file`; the original
  was left byte for byte alone and the copy differed in exactly five keys.

  | Key | Save As |
  |---|---|
  | `documentUUID`, `shareUUID`, `privateUUID`, `versionUUID` | all new |
  | `revision` | new, `"0::"` + the new version |
  | `stableDocumentUUID` | **unchanged — the lineage** |
  | `DocumentIdentifier` | new, the new `documentUUID` |
  | build history, format version, the three booleans | unchanged |

  `save_as_new` implements that table. Then the app was asked twice more, and
  this is the strongest evidence in the phase: `iwork duplicate` followed by
  `resave.sh` **twice** left `documentUUID`, `shareUUID`, `privateUUID` and
  `stableDocumentUUID` exactly as this crate wrote them and moved only the
  version — while a plain `cp` of the same fixture, resaved, came back with a
  new `documentUUID`, `shareUUID` and `privateUUID` and the stable one kept.
  **Pages re-identifies a byte copy of its own accord and leaves a properly
  re-identified one alone.** Numbers reproduces it on a `.numbers`, and all
  three apps open a copy this crate wrote.

  **What could not be settled, said plainly.** Opening two documents at once
  does *not* distinguish them: Pages opens an original and a byte-identical
  copy side by side and reports two documents with two paths, exactly as it
  does for a properly re-identified pair. The identity matters to the sync
  layer and there is no iCloud account here to watch it matter, so "two edited
  copies don't collide in iCloud" rests on the Save As measurement and on the
  app re-identifying byte copies unprompted. Marked Inferred in FORMAT.md.

  **A registry entry that was wrong in an interesting way.** 11014 and 11015
  were `TSP.AnnotationAuthorArchive` and its storage, carried over from prior
  art, so every document in the corpus reported three annotation authors. The
  15.3.1 registry names them `TSP.DataMetadata` and `TSP.DataMetadataMap`, and
  the payloads agree exactly: the map's `data_identifier`s in `pages-report`
  are 11, 10 and 14 — precisely its three *theme assets*, the images it names
  and does not carry — and each metadata is one `fallback_color`, what iWork
  draws where the asset is not there. Both are now Confirmed. The real
  annotation authors are 212/213 and there are none.

  **The sweep, and it is the whole story of this phase's read side.** 924
  documents — 23 fixtures and all 901 bundles — carry **exactly one**
  `TSK.AnnotationAuthorStorageArchive` each, and in every one of them the
  payload is **zero bytes long**. Not one 212, 2013, 2014, 2060, 2061, 2062 or
  3056 anywhere; not one storage with field 21, 22, 23 or 25. So the storage is
  Confirmed and everything below it is Unverified, with `iwork annotations`
  reporting what it finds and two tripwire tests failing the day a fixture
  finally has one.

  Three things worth knowing that the schema said and no document could:

  - **A comment reaches text through two hops**, not one: `table_highlight`
    (23) or `table_overlapping_highlight` (25) → `TSWP.HighlightArchive` (2013)
    → `TSD.CommentStorageArchive` (3056). A decoder that takes one hop lands on
    the highlight.
  - **A cell's comment is an interned entry**, in a `TST.TableDataList` of
    `listType = 10` — the same string-table indirection every other cell
    payload uses.
  - **There is no resolved flag.** `TSD.CommentStorageArchive` has text, date,
    author, replies and a UUID, and the string `resolv` does not occur in any
    of the 121 descriptor files carved out of 15.3.1. Whatever records that a
    thread is resolved is not in this schema, and nothing here invents it.

  **Two plists, two formats.** `Properties.plist` is `bplist00`;
  `BuildVersionHistory.plist` is XML. A reader that assumes one and meets the
  other reports a corrupt document. `src/plist.rs` reads both and writes the
  binary one, with no new dependency: 945 plists across the corpus and the
  bundles parse, and the binary ones re-serialise to something `plutil` prints
  identically. It is deliberately *not* byte-identical to CoreFoundation's
  output — Pages writes 21 objects for 20 values, referencing one `false` three
  times and orphaning two more, while writing three equal UUID strings out in
  full — so ten keys come to 443 bytes here and 525 there. Pages read the
  result and saved it back without changing a key.

  **Where the document's own description lives, and it is cross-app.**
  `TSA.DocumentArchive` is the `super` of each app's document archive at a
  *different* field: Pages 15, Numbers 8, Keynote 3. It carries
  `document_language` (a proofing language — `en-GB` on a document whose locale
  is `en_CH`), `template_identifier`
  (`Application/26_Stocks/Traditional`) and `custom_format_list`, which is the
  `TSK.CustomFormatListArchive` phase 1b already read — the document-scoped
  list the brief asked to be cross-linked. One level in, `TSK.DocumentArchive`
  has `locale_identifier`, `creation_locale_identifier` and
  `annotation_author_storage`.

  A trap found on the way: **`en_US` re-encodes as a valid one-field
  submessage**, because `e` is `0x65`, the tag byte of a `fixed32` at field 12.
  A decoder that tries the submessage interpretation first reports the
  document's locale as `200061.72`, which is what `iwork dump` does.

  Three AppleScript traps, all compiler rather than app, all in one script:
  `before` is a reserved word, so `set before to password protected of doc`
  will not compile; `locked` is the iWork item property, so `set locked to …`
  compiles and then fails at run time with -10006; and `set password pw` will
  not compile with a *variable* in the direct parameter, because the compiler
  reads `password` as the first word of `password protected`. The call is built
  as text and compiled at run time with `run script`.

  What Phase 8a/8b (Keynote) should know:

  - **A Keynote deck has the same identity surface as everything else**, and
    `save_as_new` is app-verified on one. If 8a duplicates a *slide* it should
    not confuse that with duplicating the document.
  - `TSA.DocumentArchive` is at field **3** in `KN.DocumentArchive`, not 15 or
    8; `crate::metadata::super_field` is the one place that knows.
  - The corpus filter in every test file now excludes password-protected
    packages **by shape** (`.iwpv2`), not by name. A new test file that walks
    `tests/fixtures/generated` needs the same three lines or it will trip over
    `pages-locked.pages`.

  What Phase 9 (hardening) should know:

  - **Encryption detection is done and is exact**; what is left is the
    package-*form* document (a real directory instead of a ZIP), which
    `Package::read` does not handle at all, and the fuzzer's view of a hostile
    `.iwpv2` or a hostile plist. `src/plist.rs` is new attack surface and was
    written to refuse rather than guess — every unknown marker, every truncated
    offset table and every nesting depth over 32 is an `Error::Format` — but it
    has not been fuzzed.
  - **`save_as_new` is the honest half of `from_template`.** Phase 9's
    `Document::from_template` wants exactly this identity rewrite plus cleared
    view state; the identity half is done, app-verified, and the rule it
    follows is the app's own.
  - **The preview-staleness rule now has a second data point**: a
    password-protected package has **no previews at all**, so "leave them
    alone" is not the only thing the apps do with them.
  - `Properties.plist` carries `hasExternalReferenceOrMissingData` and
    `hasUnmaterializedRemoteData`, both false everywhere. They are exactly the
    flags a hardening phase would want to set or check, and nothing here has
    seen a true one.

- 2026-08-18 — **Phase 8a complete (Keynote: inventory and text, app-verified).**
  New module `src/keynote.rs`, `Document::show | slides | slide_layouts |
  set_slide_skipped | move_slide | duplicate_slide | set_presenter_notes`,
  `iwork slides | layouts | set-notes | skip-slide | unskip-slide | move-slide |
  duplicate-slide`, six new `iwork check` invariants, FORMAT.md §13 and writing
  rules 22–23, fourteen `KN` registry entries, a new oracle
  (`scripts/slide-oracle.sh`), a new fixture (`keynote-slides.key`) and
  `tests/keynote.rs` — 24 tests, eleven of which the app answers.
  `cargo fmt --check` and `cargo clippy --all-targets -D warnings` clean;
  `cargo test --all-targets` green: 145 unit + 16 cell + 18 chart + 18 drawable
  + 24 fixture + 14 formula + **24 keynote** + 18 pages + 34 style + 22 table +
  14 text + 4 doc. `IWORK_APP_CHECK=1 cargo test --all-targets` green over all
  24 fixtures.

  **The oracle agreed about everything it was asked.** `slide-oracle.sh` reads
  the show line, every layout by name and every slide's number, base layout,
  skipped flag, title showing, body showing, title, body, presenter notes and
  transition. Over `keynote-deck` and `keynote-slides`: slide counts 6 and 8,
  layout counts 17 and 17, sizes 1920 × 1080, **34 layout names in the app's
  order**, and **14 slides × nine fields** — no disagreements anywhere, before
  or after the writes.

  What the show graph turned out to be, and where the old §Keynote was wrong:

  - **The slide tree is inline and positional.** `KN.ShowArchive.slideTree` (3)
    is a `KN.SlideTreeArchive` written into the show, not a reference, and its
    repeated field 2 *is* the deck order. Keynote's own `move slide 1 to after
    slide 3` rewrites that repeated field and **nothing else** — every node and
    every slide component came back byte for byte.
  - **Type 9 was not a slide-template archive.** It is `KN.SlideStyleArchive`,
    it lives in the document stylesheet, and there is one per *layout* because
    every slide on a layout shares it — which is why counting one per
    `Index/TemplateSlide-*.iwa` looked right.
  - **`KN.SlideArchive` field 7 is `owned_drawables` and field 42 is
    `drawables_z_order`.** Phase 3 recorded field 7 as the z-order; it is
    ownership, and the two hold the same members in every deck here, so nothing
    in this corpus tells them apart by content. The placeholder fields do: field
    5 can name a title that is in neither.
  - **"Title showing" is ownership.** Keynote reports `title showing` false for
    a slide whose field 5 still names a placeholder holding text; what it means
    is membership of field 7. Twelve out of twelve on `keynote-deck`, the
    "Statement" slide included, whose title reads "Eine Behauptung" and is not
    drawn.
  - **A skipped slide has no number.** The app answers `slide number` with -1
    and numbers the rest around it, so the number is arithmetic over the deck.
    `Slide::number` is an `Option` for that reason.
  - **"Slide numbers showing" is a flag on every node**, not on the show:
    `isSlideNumberVisible` (18) is 1 on all eight nodes of the deck whose
    numbers are on, and `KN.ShowArchive.slideNumbersVisible` (6) is **absent**
    in both the deck with numbers and the deck without.
  - **A slide is a component**, whose identifier is its `KN.SlideArchive`'s own,
    with its node in `Index/Document.iwa` beside the show. Nothing else in the
    format splits one user-visible thing across two components.
  - Presenter notes are `KN.NoteArchive` (15) → a storage of **kind 4**, and the
    kind-4 storages of a deck are exactly its notes — asserted as set equality
    over all four decks.
  - Transitions decode to the identifier the app's own dictionary lists:
    `apple:dissolve`, `apple:push`, `apple:magic-move-implied-motion-path`,
    `apple:wipe`, `com.apple.iWork.Keynote.KLNConfetti`, with duration, delay
    and `is_automatic` matching the app exactly.

  **The duplicate is the crown and it landed.** Keynote's own `duplicate slide`
  was measured first — save, duplicate, save, diff — and the recipe reproduced:
  a new `Index/Slide-<id>.iwa` with the source stream's objects one for one
  (19 for a text slide, 22 for an image slide), every reference *inside* the
  stream remapped and every reference *out* of it left alone; a new
  `KN.SlideNodeArchive` identical to the source's but for the slide it names and
  `thumbnailsAreDirty`; a new `TSP.ComponentInfo` with the same external and
  data references, the data references' *using object* remapped, and fresh
  object UUIDs; one more entry in the slide tree; and no `Data/` bytes copied.

  Verified: Keynote opens the deck, reports 7 slides where there were 6, reads
  the copy's layout, title, body and presenter notes back, then reads the
  *edited* copy's own title and notes back while the original keeps its own;
  `iwork check` clean; `resave.sh` survives, so what is on disk afterwards was
  written by Keynote from its own model and still holds the copy under the
  identifier this crate gave it. Three streams are rewritten and no more:
  `Index/Document.iwa`, `Index/Metadata.iwa` and the new one.

  **The thing that made the first attempt fail is worth the whole entry.** A
  copy with the stream, the node, the tree entry and the component entry all
  correct still did not work: Keynote opened the deck, counted seven slides, and
  answered `missing value` for the seventh's base layout, title and body. The
  missing piece was in `Index/Document.iwa`'s *component*, which declares
  `{component_identifier: <slide>}` for every slide component its nodes point at
  — 475 declarations in a six-slide deck. `undeclared_references` had excused
  root references on the grounds that a component's identifier is its root
  object's identifier; that was wrong. The rule now covers them, with `Document`
  and `DocumentMetadata` (objects 1 and 71) the only exemptions, and reports
  **zero** undeclared references over all 23 documents of the corpus — so it
  costs nothing on files the apps wrote and catches the one this crate can now
  create.

  What resisted, honestly:

  - **There is no build anywhere.** `KN.SlideArchive.builds` (2) and
    `buildChunks` (43) are empty in all four decks *and* in all 182 bundled
    `.kth` themes, which were scanned for types 8 and 153. Keynote's dictionary
    has no build vocabulary at all, and template mining does not help because a
    theme carries masters, not animations. The build count is honestly zero and
    `KN.BuildArchive` stays Inferred.
  - **The theme's display name is not in the document.** The stored name is
    `21_BasicWhite`; the app says `Basic White`. Nothing in the package holds
    the second.
  - **No slide is created from nothing and none is deleted.** A new slide is a
    new component, and ground rule 3 says copy; a deletion has to unwind the
    tree entry, the node, the component, its metadata entry, its media refcounts
    and its declarations together, and no probe has watched Keynote do it.
  - **A copy's thumbnail is the original's**, marked dirty — which is exactly
    what the app's own duplicate leaves behind, so it is a limitation shared
    with Keynote rather than a defect.

  Two more app traps for the harness's collection: `automatic` is the
  cell-format constant `NMCTfaut`, so `set automatic to "-"` compiles and fails
  at run time with -10003 (the `plain` trap again); and a text item cannot be
  *made* with its text — `make new text item … with properties {object
  text:"…"}` answers -10000 — and has to be made inside a `tell slide` block.

  What Phase 8b (builds, transitions, playback) should know:

  - **The transition chain is decoded and named**: `KN.SlideArchive.transition`
    (4) → `KN.TransitionArchive.attributes` (2) →
    `KN.TransitionAttributesArchive.animationAttributes` (8) →
    `KN.AnimationAttributesArchive`, whose fields 1–6, 11 and 16 are read.
    Everything 8b wants beyond the inventory is the *sibling* fields of
    `KN.TransitionAttributesArchive` — 9–20, the `custom_*` block: twist, mosaic
    size and type, bounce, magic-move fade-unmatched, timing curve, text
    delivery, motion blur, travel distance, angle, blur amount. All named from
    the 15.3.1 schema, none exercised. `transition properties` is settable from
    a script, and `keynote-slides.applescript` shows how, so 8b can make a
    fixture per effect and read the `custom_*` fields the app fills in — but the
    dictionary exposes only effect, duration, delay and automatic, so the rest
    have to be diffed between two decks rather than read back.
  - **The 42 effect identifiers are already known**, from the app's `transition
    effects` enumeration: the sdef gives the cocoa string for each, and those
    strings are exactly what lands in `effect`. That table is the mapping 8b
    needs and it does not need a probe.
  - **Builds need a deck from outside this corpus.** Nothing here can make one.
    If one turns up, `Slide::builds` and `Slide::build_chunks` already count
    them and `KN.BuildArchive`/`KN.BuildChunkArchive` field maps are in
    `reference/protos-15.3/keynote/KNArchives.proto`.
  - **Playback settings are read already** and are all at their defaults:
    `KN.ShowArchive` loop (8), mode (9), autoplay delays (10, 11), idle timer
    (15, 16), plays-on-open (18), and `KN.Soundtrack` (21) with volume 1, mode
    "play once" and no media — one per deck, every deck. The dictionary has
    `auto loop`, `auto play`, `auto restart` and `maximum idle duration`, so
    these *can* be exercised.
  - **A recorded presentation would hang off `KN.ShowArchive.recording` (7)**,
    and there is none; ground rule 8 says read and pass through, never author.

- 2026-08-18 — **Phase 8b complete (Keynote: builds, transitions, playback,
  read).** Two new fixtures (`keynote-transitions.key`, `keynote-playback.key`),
  a new probe (`scripts/transition-direction-probe.sh` + `applescript/
  keynote-powerpoint.applescript`), a `playback` line in the slide oracle, the
  whole `custom_*` block and the effect table in `src/keynote.rs`, schema-level
  `Build`/`BuildChunk`/`Recording`/`LiveVideoSource`, six new sections in
  FORMAT.md §13, three registry entries added and four rewritten, and eight new
  tests in `tests/keynote.rs`. `cargo fmt --check` and `cargo clippy
  --all-targets -D warnings` clean; `cargo test --all-targets` green: 152 unit +
  16 cell + 18 chart + 18 drawable + 24 fixture + 14 formula + **32 keynote** +
  18 pages + 34 style + 22 table + 15 text + 4 doc. `IWORK_APP_CHECK=1
  cargo test --all-targets` green over all **26 fixtures**, the two new decks
  among them.

  **What got A/B evidence, and what could not.** The rule the phase turned on is
  that `transition settings` is a four-member record — effect, duration, delay,
  automatic — so anything else about a transition has to be *diffed* rather than
  read back. `keynote-transitions` is that diff: **44 blank slides, one per
  effect in the app's own enumeration, identical in every other respect**, plus
  two control slides that repeat an effect at another duration, delay and
  automatic flag.

  - **The `custom_*` block belongs to the effect.** Eleven of the 44 effects
    write a parameter and thirty-three write none: `custom_bounce` on six object
    effects, `custom_twist` = 3.3 on Twist, `custom_travel_distance` = 1 on Fade
    and Move, `custom_angle` = 90 and `custom_blur_amount` = 0.5 on Radial Wipe,
    and Magic Move's three — `custom_magic_move_fade_unmatched_objects` = true,
    `custom_timing_curve` = 4 (ease in and out), `custom_text_delivery_type` = 1
    (by object). The controls carry **the same block** at other timings, which is
    what says the block is a function of the effect alone.
  - **Absent is not false.** `apple:scale` and `apple:ca-revolve` both write
    `custom_bounce` = *false*; the app writes the parameters an effect *has*,
    whatever their value. Every one is an `Option` for that reason, and a
    decoder that read absent as false would both invent a parameter and lose one.
  - **Magic Move has no match mode.** Its whole surface is fields 13, 15 and 16 —
    fade unmatched, acceleration, text granularity. What matches is the objects'
    identity; the only choice about the rest is whether they fade.
  - **Fade Through Colour writes a `TSP.Color`** at animation field 7 — black,
    opaque — and is the only one of the 44 that writes one.
  - **`custom_mosaic_size`, `custom_mosaic_type` and `custom_motion_blur` stay
    schema-only.** Mosaic was set from a script and wrote neither of its two.
  - **The playback A/B landed on four fields.** `keynote-playback` sets `auto
    loop`, `auto play`, `auto restart` and `maximum idle duration`, and differs
    from every other deck in exactly fields 8, 18, 15 and 16. `mode` (9) and the
    two self-playing delays (10, 11) are written explicitly at their defaults by
    every deck and have **no scripting term at all**, so they stay schema-only.

  **The trap of the phase: `maximum idle duration` is in minutes and field 16 is
  in seconds.** `set maximum idle duration to 137` wrote 8220. The oracle now
  compares the app's minutes with the field ÷ 60 on every deck, because a reader
  that took the number at face value would say a deck restarts after two and a
  quarter hours.

  **Direction came through PowerPoint, and that is the whole story.** Keynote
  writes no direction: field 4 is absent from every transition in six decks and
  182 themes, and the record has no member for it. The one door left that does
  not need the user interface is the importer — export a deck to `.pptx`, patch
  the `<p:transition>` elements, open the result, save as `.key`. Eight values,
  two families, consistent across `apple:push`, `apple:wipe`, `apple:slide` and
  `BLTBlinds`: **11 left-to-right, 12 right-to-left, 13 top-to-bottom, 14
  bottom-to-top; 21–24 the four diagonals.** The same probe confirmed
  `custom_text_delivery_type` **2 = by word** and **3 = by character** from
  `<p159:morph option="byWord"/>`. Marked *Observed, through the importer* —
  weaker than the rest of §13 and labelled so.

  That probe also produced a fact about the format itself: **a transition
  belongs to the slide it leaves.** PowerPoint's belongs to the slide being
  entered, and Keynote's importer shifts the whole deck by one to reconcile them
  — pptx slide *n+1* lands on Keynote slide *n*, and the last is dropped. It cost
  an hour to notice and it explains the whole first round of results.

  **All 44 effect identifiers are now app-verified rather than transcribed.**
  `keynote::EFFECTS` pairs the dictionary's name with the identifier on the wire,
  and the oracle test makes Keynote read all 46 slides back and compares every
  pairing. Three could not have been guessed: `apple:revolve` is *flip*,
  `apple:ca-revolve` is *object revolve*, `apple:slide` is *move in*.

  **The soundtrack is a dead end, and now a documented one.** Every deck has a
  `KN.Soundtrack` and every one is empty — volume 1, play once, eleven bytes.
  There is no soundtrack term anywhere in the sdef. The nearest thing, `audio
  clip`, is an element of a *slide*, and `make new audio clip … with properties
  {file name:…}` is the worst kind of failure: **no error at all**, the slide's
  `audio clips` still count zero, and the saved package holds no new `Data/`
  entry. So `movie_media` (3) is decoded as what the schema says it is — a
  repeated `TSP.DataReference`, an ordered list of media ids into the same table
  `iwork media` prints — and left Unverified.

  **Builds are 8a's boundary and it held.** Zero `KN.BuildArchive` in six decks,
  zero in 182 themes, no dictionary vocabulary. `KN.BuildArchive`,
  `KN.BuildAttributesArchive` and `KN.BuildChunkArchive` are decoded from the
  15.3.1 schema — drawable, delivery string carried verbatim, event trigger, the
  shared animation attributes, text delivery and delivery option, the action
  motion path, the start/end offsets a by-bullet-group build would use — and
  `no_fixture_has_a_build_yet` fails the day a fixture produces one, which is the
  signal to measure rather than to trust.

  **Two never-author cases identified.** There is no `KN.RecordingArchive`
  anywhere (Record Slideshow is menu-only), and there is exactly one
  `KN.LiveVideoSource` per deck — `"Default Camera"`, `is_default_source` true.
  It is the collection's `default_source` and is in **no** `sources` list, so a
  reader that walked the list would report no cameras at all.

  What Phase 9 should know:

  - **The corpus is 26 fixtures now**, two of them Keynote decks built for
    diffing. `keynote-transitions` has 46 slides and 1677 objects and is the
    largest deck here; `iwork check` and `iwork roundtrip` are clean on both.
  - **A `.pptx` round trip is a probe route the harness now owns.**
    `scripts/transition-direction-probe.sh` exports, patches and re-imports, and
    the shift-by-one is handled. Anything else Keynote's importer can express and
    its dictionary cannot — build directions among them, if PowerPoint animation
    XML survives the import — is reachable the same way. It is worth trying
    against `<p:anim>`/`<p:animEffect>` before declaring builds unreachable
    forever; this phase did not, because the schema decode was the deliverable.
  - **Nothing in 8b writes.** The phase is read-only on documents by brief, so
    there is no new writing rule and rules 22–23 stand unchanged.
  - **`Slide::builds` and `Slide::build_chunks` changed type** from `usize` to
    `Vec<Build>` / `Vec<BuildChunk>`, and `Soundtrack::tracks` from a field to a
    method. `Show::recording` is now `Option<Recording>` rather than
    `Option<u64>`. Anything outside this repository using those would break.

- 2026-08-18 — **Phase 9 complete (document creation and hardening).**
  `package::Form` and the package form read, written and preserved;
  `Document::from_template`, `previews`, `strip_previews`;
  `metadata::Lineage`, `assign_identity`, `root_archive`,
  `set_template_identifier`; `iwork new` and `iwork strip-previews`, two new
  lines on `iwork inspect`, a 56th `iwork check` invariant; a new test file
  (`tests/documents.rs`, ten tests) and the fuzzer (`tests/fuzz.rs`, the
  harness plus four regression tests); FORMAT.md §1 rewritten with the two
  shapes and the preview decision, a table of contents, §11's template-identity
  section and writing rules 24–26; README's verification table, Limitations and
  a fuzzing section. `cargo fmt --check` and `cargo clippy --all-targets -D
  warnings` clean; `cargo test --all-targets` green: **156 unit** + 16 cell +
  18 chart + **10 document** + 19 drawable + 24 fixture + 14 formula + **5
  fuzz** + 32 keynote + 18 pages + 34 style + 22 table + 15 text + 4 doc =
  **387**. `IWORK_APP_CHECK=1` green over the whole suite, one test target at a time, and
  over the 26 readable fixtures. Two of the app-driving tests failed once each
  when every test binary ran at once and passed on a rerun — the harness's
  known "the app was busy" mode, which looks exactly like "the app refused" and
  is why `osa_try` gives everything a second chance from a cold app. Running
  the targets one at a time is the reliable way to see the suite green.

  **The template bundles are ZIPs, all 901 of them.** The brief expected
  Apple's bundled templates to be the directory form of a package and they are
  not: `file` says "Zip archive data, compression method=store" for every
  `.template`, `.nmbtemplate` and `.kth` in the three apps. So the package form
  had to be built by hand, and that turned out to be the whole probe.

  **What a package form is, measured two ways.** A fixture unzipped into
  `PkgProbe.pages/` — and `mdls` on the two shapes of the same document is the
  clean answer:

  | On disk | `kMDItemContentType` | conforms to |
  |---|---|---|
  | the file | `com.apple.iwork.pages.sffpages` | — *single-file format* |
  | the directory | `com.apple.iwork.pages.pages` | `com.apple.package`, `public.directory` |

  Pages **opened the hand-built package**, read it back, and — asked to save it
  — **wrote one file back over the directory**. `Change File Type` is a menu
  item and the screen is locked, so what a document a *user* has set to the
  package form does on save is unknown; what is known is that a package handed
  to the app and saved comes back a ZIP. This crate keeps the shape instead,
  and all three apps opened a package it wrote (`pages-book`,
  `numbers-categories`, `keynote-charts`, each as a directory).

  **A document from a template is not a copy of the template**, and the corpus
  proved it without a new probe. `00C_Textbook_Portrait/ISO.template` has
  `stableDocumentUUID` `A0C50246-…` against its own `documentUUID`
  `65D82B29-…` — a template with a lineage — and `pages-toc.pages`, which Pages
  made from exactly that bundle, has `stableDocumentUUID` equal to its own new
  `documentUUID`. `pages-lists` and `numbers-categories` agree. So `save_as_new`
  and `from_template` follow *two* rules, and `metadata::Lineage` is which.
  The app also writes `TSA.DocumentArchive.template_identifier`
  (`Application/04_Real_Estate_Flyer/ISO`, the bundle's path without its
  extension) and the bundle has none, so `from_template` derives it from the
  path when the template is one of the app's own and claims nothing otherwise.

  There was **no view state to clear**: not one bundled template has an
  `Index/ViewState.iwa`, a `BuildVersionHistory.plist` or a preview. All three
  apps opened a document `from_template` wrote, **saved it, and left every UUID
  where this crate put it** — the same acceptance signal phase 7 got for
  `save_as_new`, and the thing Pages does *not* do to a plain byte copy.

  **The preview decision, on four pieces of evidence.**

  | Question | Answer |
  |---|---|
  | Does anything in the document refer to a preview? | **No.** The string `preview` occurs in no object stream of 928 packages |
  | Is a document without them valid? | **Yes.** 901 templates have none; two fixtures are renamed templates the apps open; a locked package has none |
  | Who redraws them? | **The app.** Three documents made from templates had none; after one save each by Pages, Numbers and Keynote, each had three |
  | What would removing them cost? | Byte-identity, which a no-op save promises |

  So they are **left alone**, `iwork check` stays silent, `iwork inspect` says
  how many there are, and `strip_previews` is offered and never automatic — with
  the app watched opening a stripped document of each of the three kinds.

  **The fuzzer, and the five things it found.** `tests/fuzz.rs`: corpus-seeded,
  splitmix64-deterministic, budget-bounded, everything under `catch_unwind`,
  17,253 seeds. The level that matters is `object` — the mutated payload goes
  *back* into its object and the stream is re-framed, so the document opens and
  every reader runs on it. The obvious approach, mutating the compressed entry,
  was tried first and dies in Snappy nearly every time.

  | Where | The input | What happened |
  |---|---|---|
  | `iwa::decompress` | a Snappy block declaring 256 MB in six bytes | `snap` allocates the declaration. Refused now against the 64 KiB block size, which the census makes a limit: 24,358 blocks, maximum 65,536, none over |
  | `iwa::parse` | `MessageInfo.length` = 2^60 | `vec![0; len]`. Now checked against what is left of the stream |
  | `iwa::ArchiveObject::payload` | an `ArchiveInfo` with no `MessageInfo` — three legal bytes | `messages[0]` panicked. Answers no bytes now |
  | `plist` | `0xDF 0x13` + eight `0xFF`: a dictionary of 2^64−1 entries | `length * 2` and `at + length` wrap — a panic in debug, a wrong length in release. Three places, all checked |
  | `Package::from_bytes` | a ZIP entry claiming 2 GB | `Vec::with_capacity` of the claim. Capacity is now bounded by what is there |

  A sixth, found by reading rather than by fuzzing: **`iwork extract` joined
  `Data/…` names to a directory** without checking them, so
  `Data/../../../evil` was written exactly there. `package::entry_path` is
  public now and both callers use it, and `iwork check` reports an entry name
  that is not a plain relative path — invariant 56.

  After the fixes: **45,000 mutations at the shallow levels and 7,500 at the
  object level, in both build profiles, plus a twenty-minute release run over
  all six — no panics.** Debug matters as much as release here: the plist
  overflows are only panics with overflow checks on, and release is where the
  reach is (36,965 mutations in ten minutes against 8,167).

  **`cargo fuzz` is not part of this.** There is no nightly toolchain on this
  machine (`rustup toolchain list` → `stable-aarch64-apple-darwin` alone) and
  libFuzzer needs `-Z sanitizer`; the brief said not to fight it, so the
  committed harness is the whole of the fuzzing story rather than half of it.

  **What could not be settled.**

  - Whether a document a user has switched to the package form stays a package
    when the app saves it. The menu item needs a window.
  - What identifier a *user* template (in `~/Library/Containers/…`) has. Only
    the app's own bundles have a derivable one, so nothing is claimed for the
    rest.
  - Whether the readers are safe against shapes this corpus does not contain.
    Dumb mutation over 27 documents is coverage, not proof.

- 2026-08-18 — **The whole build-out, in summary.** Ten phases in two days,
  0 through 9, each one landed only what the app or the schema would confirm.

  | | |
  |---|---|
  | Corpus | **27 documents** built by Pages, Numbers and Keynote 15.3.1 (26 in the walkers; `pages-locked.pages` is excluded by shape), plus **901 bundled templates** swept repeatedly — 927 readable packages in all |
  | Code | 18 modules and a CLI — 27,700 lines of Rust, and 10,300 more of tests — on two dependencies (`snap`, `zip`), with none added since phase 0 |
  | Tests | **387** — 156 unit, 227 integration across 13 files, 4 doc — all green, and green again with `IWORK_APP_CHECK=1` |
  | `iwork check` | **56 invariants**, every one of them discovered by watching a real document keep it |
  | CLI | **46 verbs** |
  | Registry | **145 message types**, each carrying its evidence |
  | FORMAT.md | 13 sections and 26 writing rules, every structural claim tagged Confirmed, Inferred or Unverified |

  **What the apps confirmed, totalled.** 2,943 cells compared against Numbers,
  every one agreeing on value, format and formula-ness; 273 of 273 formulas
  matching the app's text character for character outside pivots; 108 chart
  values across an 18-chart zoo; every rectangle in the drawable corpus against
  what the app reports; four documents' sections compared character for
  character; 46 slides and 34 layout names read back from Keynote, nine fields
  each, and all 44 transition effect identifiers paired against the
  dictionary's names. Every writing feature ends the same way: the app opens
  the document, and where a dictionary can read the edit back, it reads it back.

  **The three ways this repository learned things**, in order of how much they
  produced: the apps' scripting dictionaries (the oracles above); **template
  mining** — 901 bundles carrying the features no script can author, which is
  where lists, pivots, categories, filters, conditional highlighting, custom
  formats and hyperlink fields came from; and the **15.3.1 descriptors** carved
  out of the installed binaries, which settled every field number that no
  document in the corpus exercises. One door was opened by neither: transition
  *direction*, which came back through Keynote's PowerPoint importer.

  **What is left, and what a phase 10 would be.**

  - **Footnotes, endnotes, bookmarks, comments, replies, tracked changes,
    builds.** Six features with no source anywhere on this machine: not in the
    corpus, not in 901 templates, not in any dictionary. Everything this crate
    says about them is read off the 15.3.1 schema and marked Unverified, with
    tripwire tests that fail the day a fixture finally has one. A phase 10 with
    one real user document — or an unlocked screen — would settle all six.
  - **Writing a formula**, which is not an AST problem: the cached value, the
    dependency graph's edge encoding and cross-table `base_owner_uid` tracking
    all have to be right, and rule 17 refuses until they are.
  - **The `type == 0` version-patch mechanism**, a named phase 2 precondition
    still open: an object carrying patches is read and never rewritten.
  - **Structural writes** — adding a row, a column, a slide, a section — each
    of which is several coordinated edits nothing here has watched an app
    perform.
  - **The PowerPoint importer as a general probe.** It reached transition
    direction; whether `<p:anim>` survives the import and would produce a
    `KN.BuildArchive` is the cheapest known route to the builds gap and was
    never tried.
  - **Rendering, layout and evaluation** stay out of scope, and everything that
    depends on them — preview regeneration, a shape that sizes itself to its
    text, a re-fitted image frame — stays refused or reported rather than
    guessed.

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

## After the build-out

- 2026-08-18 — **The unlocked screen, and the six features that needed it.**
  The user sat down at the Mac, the screen unlocked, and ground rule 7's
  blocked half opened: UI scripting works again. Six fixtures now exist that
  no dictionary and no template could produce — `pages-footnotes`,
  `pages-comments`, `pages-tracked`, `pages-bookmarks`, `keynote-builds`,
  `numbers-hidden` — with reproducible recipes in
  `scripts/applescript/*-ui.applescript` behind `make-fixtures.sh --ui`
  (which probes for AX windows and skips cleanly when locked). What they
  settled, each previously Unverified or wrong:
  - The change/highlight anchor tables (21/22/23) are **wrappers** with
    repeated field-1 entries like every attribute table; the schema-only
    reading was one level too shallow and reported every comment unattached.
    Comments anchor through `TSWP.HighlightArchive` at exactly the selected
    characters; changes anchor the same way; the author storage finally
    carries a real author.
  - A footnote mark is **U+000E**, not U+FFFC; the notes are the corpus's
    first kind-2 storages, anchors at 26 and 77 as made.
  - A bookmark archive is nameless (`{UUID, 2 varints}`), and the table's
    terminator entries are not bookmarks (a reader counting entries said 3
    where 2 exist).
  - Builds: `animation_type` `"In"`/`"Out"` is what separates build-in from
    build-out; menu "Disappear" stores `apple:bc-appear`. Eight measured;
    action builds and chunk timing remain schema-only.
  - `iwork tables` now reads hidden rows/columns from the extents (the model
    count fields are dead, as 1b found); `numbers-hidden` exercises
    user-hidden rows for the first time.
  Registry: 212, 2008, 2013, 2035, 2060, 2062, 3056 and KN 8 upgraded to
  Confirmed. Five tripwires flipped into pinning tests; tripwires remain for
  replies, resolved state, cell comments, table 25, and accept/reject
  residue. Full suite green (14 binaries), fmt/clippy clean.

- 2026-08-18 — **A review of the text and style write path, and the twelve
  things it found.** One critical, and it was the previous entry's discovery
  turning round to bite: `destroyed_anchors` decided whether an edit destroyed
  an anchor by *reading the character*, and its list was `U+FFFC` and `U+0004`,
  so a footnote mark — `U+000E` — walked through. `iwork delete-text
  pages-footnotes.pages 1732539 20 30` exited 0, orphaned the attachment and
  its kind-2 storage, and `check` found nothing. The character is no longer
  consulted at all: a character-anchored entry whose character goes is
  destroyed whatever the character was, and not recognising it is the reason
  to stop rather than a licence to proceed. `U+000E` joined `UNWRITABLE` from
  the other side, and `check` learned to report a footnote attachment no mark
  anchors and a kind-2 storage no attachment contains.

  Two writers were producing the table their own checker rejects — one that
  begins after character 0. `apply-style` on a storage with no table of that
  kind wrote its first entry at the range's start; `delete-style … None`
  dropped the entry at 0 outright. Both now write what Pages writes, a head
  entry carrying nothing, which FORMAT.md's own probe fixture had recorded
  (`[0 nil, 19 bold, 30 nil]`) without anyone reading it as a rule.

  The rest: an attribute table whose bytes fail `decode_nested` was skipped by
  the remapper *and* by `problems()` while every other table moved (now
  `Error::UndecodableAttributeTable`); text that is not UTF-8 was read lossily
  and written back (now `Error::InvalidText`); the anchor and section checks
  read only the first occurrence of a field `apply` rewrites in full; the
  end-of-text paragraph entry was dropped by an insertion at the end of the
  text, which is what typing at the end of a text box is; `apply_text_style`
  clamped an out-of-range range instead of refusing it and appended its new
  field out of order. `text::refusal` now answers all six preconditions in one
  place and `apply` asserts it in debug builds, because a contract listing
  three of its six checks is how the other three come to be skipped.

  Every fix carries the reviewer's own reproduction as a test. Full suite
  green with `IWORK_APP_CHECK=1` — including Pages opening an edit made beside
  a footnote and reading it back, with both notes still attached.

- 2026-08-19 — **Six rounds of adversarial review, and what they cost.**
  After the build-out the whole crate was read back against itself, one
  subsystem per pass — the plist and package layer, the table reader and
  writer, formulas, drawables and geometry, the Keynote writers, and the docs
  themselves — each pass finding what the last had no reason to look at. Four
  of the findings were critical, and each is now a test named after the shape
  that caused it:
  - **A refused cell write left the document half-applied.** `set_cell` mutated
    the interned string and format lists on its way to a refusal it only
    reached later, so a write that came back an error had already moved bytes.
    The whole write is now *planned before a byte moves* — the refusal is
    decided first, and a document a write declines is left byte for byte as it
    was.
  - **The crate could delete or write through a file it did not create** — a
    save over a symlink, or a package entry escaping its directory, followed
    the link or the path. It now refuses to write through anything it did not
    make, and an entry name that would leave the package is refused.
  - **A resized curve moved only its first control point.** Every baked point
    of a path source has to scale with the frame; scaling one left the other
    two where they were, so the shape opened deformed. All points move now, and
    the result is diffed against the document Pages wrote for the same resize.
  - **An image was replaced under cached renderings of the old pixels.** A
    stored thumbnail, a traced outline or an aspect read from `naturalSize`
    survived a byte swap the app would have refused, giving a document that
    passes every check and draws the wrong thing. Replacement now refuses when
    that state is present, and the identity mask is measured against the
    picture's own geometry rather than the `originalSize` the app fills with
    the mask window.
  The rest fell into four classes. **Bounded arithmetic:** a tile id, a merge
  extent, a formula's nesting depth, a plist length doubled or a wide integer —
  each now yields a bounded error rather than an overflow or an abort, asserted
  by splicing the hostile shape into a real stream. **Writers against their own
  checker:** two Keynote and Pages writers emitted graphs their own `check`
  would reject (a slide the deck does not declare, a zone addressed by the
  wrong index), caught by making each writer prove the invariant it maintains.
  **Reads that claimed more than they knew:** four drawable and chart readers
  reported absence they had not established; each now resolves the object or
  says nothing. **Doc truth:** counts that had rotted, an unbacked plist
  round-trip figure, `tests/media.rs` cited for `tests/drawables.rs`, and the
  seven-deck corpus — the docs were made to claim only what a named test pins.
  The fuzzer was extended from the decoders to the *pretty-printers*, so a
  panic in `iwork dump` or `drawables` on a mutated document is now a test
  failure too. Full suite green in both profiles; nothing here changed a wire
  format.

- 2026-08-19 — **Add-row (Phase 2 stretch, on `feature/add-row`).**
  `Document::insert_row(table, at)`, `iwork insert-row`, FORMAT.md §5 "Inserting
  a row", README CLI line and two status rows, `tests/rows.rs` (8 tests). `cargo
  fmt --check` and `cargo clippy --all-targets -D warnings` clean; `cargo test
  --all-targets` green; `IWORK_APP_CHECK=1 cargo test --test rows
  numbers_reads_back_an_inserted_row` green. **Not merged to main; not pushed.**

  **What it supports, app-verified.** A plain rectangular table held in one tile,
  with no categories, filters, pivots, conditional highlighting, hidden or
  collapsed rows, footer rows, merges at or below the insertion, or formulas
  whose references would shift. `numbers-formats.numbers`'s `Formate` table
  (17×3, no formulas, no merges) is the fixture: `insert_row("Formate", 8)`
  grows it to 18×3, and **Numbers opens the result and reads it back** — one more
  row, row 9 empty across all columns, and every row below the insertion holding
  its value *and* its format and control (the checkbox, rating, slider, stepper,
  pop-up, date and duration cells all shifted down intact). The oracle
  (`scripts/table-oracle.sh`) reports `row count` 18 and the shifted A-column
  values, checked against the pre-edit values in
  `tests/rows.rs::numbers_reads_back_an_inserted_row`.

  **The four objects that move**, no more — three of 97 package entries, the
  `ColumnRowUIDMap` sharing the model's stream, every other entry byte-identical:

  | Object | What insertion does |
  |---|---|
  | `TableModelArchive` field 6 | `number_of_rows` + 1; the dead hidden-counts left alone |
  | `TST.Tile` | `tile_row_index ≥ at` bumped; **no new `TileRowInfo`** (the empty row has none), so `numrows` is unchanged |
  | row `HeaderStorageBucket` | indices `≥ at` bumped, one `{at, 0, 0, 0}` added |
  | `ColumnRowUIDMapArchive` | row half (4/5/6) rebuilt: indices shifted, a fresh per-table-unique UUID minted at `at`, re-emitted **sorted by UUID, high half first** |

  **The mechanics that had to be right, and what each cost.** The tile keys rows
  by `tile_row_index`, not position, so a shift is a per-`TileRowInfo` field-1
  bump; the empty row deliberately gets *no* `TileRowInfo`, which matches the
  app's own invariant for a row with no cells — and is why `set_cell` still can't
  fill the new row (giving a row its first cell is the unbuilt "first-cell"
  write). The `ColumnRowUIDMap` was the trap Phase 1b flagged: field 4 is
  **sorted by UUID, not by index**, so the row arrays are regenerated from
  scratch and re-sorted (high 64 bits first, the order the app writes) rather
  than patched positionally. The new row UUID is the one thing **minted, not
  copied** — uniqueness is per table (row UUIDs collide across tables), the app
  exposes no row UUID to verify it against, so any high-entropy value the table
  lacks serves, derived deterministically for a reproducible document.

  **Refusals, by name, and the one that is subtle.** Multi-tile, categorised,
  filtered, pivoted, conditionally highlighted, hidden/collapsed-row and
  footer-row tables refuse because no fixture proves the write and a wrong guess
  corrupts silently. Merges at or straddling the insertion refuse because a merge
  is an absolute-row formula nothing here rewrites. **Formulas** get a precise
  check: an inserted row is safe for a whole-column reference (unbounded row
  axis) and for a relative reference whose host and referent fall on the same
  side of `at` (both shift together), but breaks an absolute reference to a row
  `≥ at` or a bounded range the insertion crosses — checked across *every* table,
  since a cross-table reference into the target breaks the same way. This is why
  `numbers-large`'s `SUM(C2:C301)` (a bounded, relative range) is refused for
  insertion at row 5, and `numbers-values`'s `Zweite Tabelle` (`SUM(B2:B3)`, host
  and range both below the insertion at row 1) is allowed. Every refusal is
  decided before a byte moves; `insert_row_refuses_what_it_cannot_verify` asserts
  `changed_streams()` is empty after each.

  **The honest boundary.** The inserted row is genuinely empty — no cell storage
  of its own — so this delivers "insert an empty row", not "insert a row with
  values": filling the new row needs the first-cell-in-a-row write, and
  `filling_the_inserted_row_is_refused_until_first_cell_writes_land` pins that
  `set_cell` refuses it by name rather than half-doing it. And **no corpus
  fixture combines a category with a filter**, so the two-addressing-schemes path
  is refused rather than verified — being conservative where nothing can prove
  the write, exactly as the add-row notes warned.

  Recommendation: the supported case is app-verified and every unsupported case
  is a named refusal that leaves the document byte-identical, so this is safe to
  merge on its own terms. The one judgement call worth a human's eye before it
  lands on main is the **formula allowance** — permitting an insert when the
  references provably shift with their host (the `Zweite Tabelle` case) is
  correct by construction but is *not* app-verified, only the no-formula case is;
  a reviewer who prefers strictly-verified-only could tighten it to refuse any
  table with a formula that references it at all.
