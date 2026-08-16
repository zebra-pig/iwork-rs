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

With no fixtures present the integration tests pass without asserting anything
and print a note saying they were skipped, so a fresh clone is green.

Good fixtures to add, in rough order of usefulness:

- a Keynote presentation — the format is currently unverified for Keynote
- a document with tables, charts or embedded media
- a document in a language that is not English, to exercise text indexing
- a document saved by an old version of Pages/Numbers/Keynote
