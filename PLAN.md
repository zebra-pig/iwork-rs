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
6. **Style discipline.** Match the repo's voice (literate commit messages,
   one idea per commit), rustfmt, no new dependencies without need. Work
   happens on branch `claude/full-spec`; each verified phase is one or more
   local commits (not pushed).

## Phase 0 — Fixture factory and app-validation harness

The foundation every later phase stands on.

- [ ] `scripts/make-fixtures.sh` (+ AppleScript sources): generates a corpus
      into `tests/fixtures/generated/` using Pages and Numbers:
      - Pages: plain text, multi-paragraph styled text, lists, a table,
        an inserted image, non-Latin text (German umlauts + CJK + emoji for
        UTF-16 indexing).
      - Numbers: multiple sheets/tables, text/number/bool/date/duration
        cells, formulas referencing other cells, merged cells if scriptable,
        a second table styled differently.
      - Keynote: several slides from different master layouts, titles, body
        bullets, presenter notes, an image slide, a skipped slide.
- [ ] `scripts/app-check.sh <doc> [expected-text]`: opens the document in the
      owning app via AppleScript with a timeout, fails loudly if the app
      refuses/crashes/dialogs, optionally reads body text / cell values back
      to confirm an edit, closes without saving. Must be callable from tests
      (`IWORK_APP_CHECK=1 cargo test`) and by later agents.
- [ ] Integration tests pick up `tests/fixtures/generated/**` (recursive or
      flattened) and the whole existing suite passes against the new corpus.
- [ ] Baseline recorded: object census per fixture, so later phases can see
      what they unlocked.

## Phase 1 — Numbers tables: read

The largest missing area of the format.

- [ ] Decode the table object graph: `TST.TableInfoArchive`,
      `TST.TableModelArchive`, `TST.TableDataStore`, tiles, row/column
      headers; map table → sheet → document.
- [ ] Decode cell storage (the current tile cell format): empty, text
      (string-table indirection), number, boolean, date, duration, error,
      rich-text cells, currency; merged-cell ranges.
- [ ] API: `doc.tables()`, `table.cell(row, col)`, typed `CellValue`.
- [ ] CLI: `iwork tables <doc>`, `iwork cells <doc> <table-id>`,
      `iwork csv <doc> <table-id>`.
- [ ] Cross-check against AppleScript: the harness reads the same cells via
      Numbers and the values must agree — the app is the oracle.
- [ ] FORMAT.md: new §Tables with the tile/cell layout as observed.

## Phase 2 — Numbers tables: write

- [ ] Edit an existing cell in place: text and number values first, then
      boolean/date; string-table maintenance; tile re-encode with the
      byte-identity rule for untouched tiles/streams.
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
- [ ] Media: replace an existing image's bytes (Data entry +
      `TSP.DataReference` + digest/metadata fields as observed); insert an
      image by copying an existing image drawable. App-verified.
- [ ] CLI: `iwork drawables`, `iwork set-geometry`, `iwork replace-media`.
- [ ] FORMAT.md: §Drawables, §Media.

## Phase 4 — Text: finish the story

- [ ] Fix the standing limitation: editing text *remaps* attribute runs
      (paragraph, character, list, and every other attribute table field 5–…)
      instead of clamping them — styling after the edit survives.
- [ ] Range operations: insert/delete text at a range, not just replace-all.
- [ ] Hyperlinks and smart fields: read them; edit a link target;
      app-verified.
- [ ] Lists: read list styles/levels per paragraph; change a paragraph's
      list level.
- [ ] FORMAT.md: §Text updated with the full attribute-table inventory.

## Phase 5 — Formulas and the calculation engine (read)

- [ ] Decode `TSCE` formula archives to an AST; pretty-print as the formula
      text the user typed (cross-checked against AppleScript's `formula`
      property, which is the oracle).
- [ ] `iwork formulas <doc>`; cells CLI shows formula alongside cached value.
- [ ] FORMAT.md: §Formulas. Writing formulas is out of scope until reading
      is exhaustive.

## Phase 6 — Charts (read)

- [ ] Enumerate `TSCH` chart objects, their type, and extract series/category
      data (charts carry a private copy of their data).
- [ ] `iwork charts <doc>`. FORMAT.md §Charts.

## Phase 7 — Comments, metadata, document properties

- [ ] Read annotations/comments and their anchors, authors storage.
- [ ] Read+write document metadata (Properties.plist fields, custom format
      lists), regenerate `Metadata/DocumentIdentifier`/UUIDs correctly on
      "save as new document" so two edited copies don't collide in iCloud.
- [ ] Change-tracking: read-level survey only; document in FORMAT.md.

## Phase 8 — Keynote (Keynote 15.3.1 is installed — app-verified)

- [ ] Presenter-notes and slide-text extraction; slide/master/build/
      transition inventory surfaced in API + CLI.
- [ ] Write: edit slide text (title/body/notes) app-verified; duplicate a
      slide by copying; skip/unskip a slide; reorder slides.
- [ ] FORMAT.md §Keynote extended with what the probes prove; registry
      evidence upgraded from Inferred to Confirmed where the app accepts it.

## Phase 9 — Document creation and hardening

- [ ] `Document::from_template(path)`: duplicate a document into a fresh
      identity (new UUIDs, cleared view state) — the copy-don't-synthesise
      answer to "create a document".
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

## Execution notes

- One Opus subagent per phase, sequential (they share the tree); the
  orchestrator reviews between phases: `cargo test`, `iwork check` +
  `iwork roundtrip` over the corpus, app round-trip spot checks, then commits.
- If a phase discovers the format resists its plan (it will), the agent's
  brief is to shrink scope and land what is *proven* rather than force the
  roadmap; this file gets amended to match reality.
