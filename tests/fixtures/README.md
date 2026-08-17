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

It builds seven documents into `generated/` — plain and styled text, non-Latin
text including emoji, a table and a photo, typed cells and formulas across two
sheets, a 301-row imported table, and a deck with presenter notes and a skipped
slide. Existing files are left alone unless `--force` is given. Nothing in
`generated/` is committed either; the generator is.

Good fixtures to add, in rough order of usefulness:

- a Keynote presentation — the format is currently unverified for Keynote
- a document with tables, charts or embedded media
- a document in a language that is not English, to exercise text indexing
- a document saved by an old version of Pages/Numbers/Keynote
