# Test fixtures

This directory is empty on purpose. iWork documents are other people's files,
and the ones used to develop this crate are copyrighted, so none are committed.

Drop any `.pages`, `.numbers` or `.key` files here and `cargo test` will run the
whole integration suite against every one of them:

```
cp ~/Documents/Something.pages tests/fixtures/
cargo test
```

Or point at a directory you already have:

```
IWORK_FIXTURES=~/Documents cargo test
```

Subdirectories are searched too, which is how the generated corpus is found.

With no fixtures present the integration tests pass without asserting anything
and print a note saying they were skipped, so a fresh clone is green.

## Generated fixtures

If you have Pages, Numbers and Keynote, the apps will write you a corpus:

```
../../scripts/make-fixtures.sh
```

It builds twenty documents into `generated/`: plain, styled and non-Latin text;
a table and a photo; lists; sections, facing pages, a table of contents, a
page-layout document with linked text boxes, page numbering and columns; typed
cells, formats, formulas, sort rules, filters, categories, pivots and
hyperlinks; a 301-row imported table; and two decks with presenter notes,
shapes and a skipped slide. Existing files are left alone unless `--force` is
given. Nothing in `generated/` is committed either; the generator is.

Most of the interesting ones come from Apple's own templates, because no
scripting dictionary can create the feature: `make new document with properties
{document template: …}` has the app write the whole structure out again, which
is what a fixture is for. Two are the template bundle itself, renamed — a
`.template` is the same ZIP a document is — for the two features whose only
source in the entire install is a template this locale does not offer.

Good fixtures to add, in rough order of usefulness:

- **a Pages document with a footnote, an endnote or a bookmark.** There is no
  such thing anywhere here: not in the corpus, not in any of the 901 templates
  the three apps ship, and no dictionary can author one. Everything this crate
  says about footnote containment and bookmark anchors is read off the schema
  and marked Unverified.
- a document with comments, replies or tracked changes — the same gap
- a document with a table on a Keynote slide, which nothing here can produce
- a document saved by an old version of Pages/Numbers/Keynote
