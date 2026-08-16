# iwork-rs

Read and write Apple iWork documents — **Pages**, **Numbers** and **Keynote** —
from Rust, with no Apple software involved.

All three apps share one file format. It has never been documented by Apple,
so [`FORMAT.md`](FORMAT.md) writes down what it actually is, derived from real
documents and checked by the tests in this repository.

```rust
let mut doc = iwork::Document::open("Report.pages")?;
println!("{} document", doc.kind().as_str());      // "Pages"

for storage in doc.text_storages() {
    println!("{}: {}", storage.identifier, storage.text);
}

doc.set_text(6083, "A new headline")?;
doc.save("Report-edited.pages")?;
```

## CLI

```
cargo install --path .

iwork inspect   Report.pages              # package, components, media, object census
iwork text      Report.pages              # every text storage, with its object id
iwork set-text  Report.pages 6083 "…" out.pages
iwork objects   Budget.numbers 2001       # every object of one message type
iwork extract   Report.pages ./media      # embedded media, byte-identical
iwork roundtrip Report.pages out.pages    # decode and re-encode every object
```

## How it works

The format is four layers deep, and this crate gives you each of them:

| Layer | What it is | Module |
|---|---|---|
| 1 | ZIP package, every entry *stored* | [`package`](src/package.rs) |
| 2 | `Index/*.iwa` — raw Snappy blocks, 64 KiB each | [`iwa`](src/iwa.rs) |
| 3 | flat stream of length-delimited protobuf objects | [`iwa`](src/iwa.rs), [`pb`](src/pb.rs) |
| 4 | an object graph whose shape depends on the app | [`document`](src/document.rs) |

Apple does not publish the `.proto` definitions, and a numeric message type is
the only thing identifying a payload's schema. So this crate works at the
protobuf **wire level**: objects decode to fields and re-encode in place. Two
consequences worth knowing:

- **Nothing is lost.** An object this crate has no idea about is carried through
  untouched, so editing a headline cannot corrupt a chart.
- **Names are advisory.** [`registry`](src/registry.rs) maps type numbers to
  names like `TSWP.StorageArchive`, tagged `Confirmed`, `Inferred` or
  `Unverified`. It feeds human-readable output only; a wrong name cannot
  break parsing.

## What is verified

Everything below is asserted by `cargo test` when you supply fixtures.

| | Pages | Numbers | Keynote |
|---|---|---|---|
| Open, identify, decode every object | ✅ | ✅ | ❔ |
| Object streams survive re-encode byte for byte | ✅ | ✅ | ❔ |
| Components resolve to real streams | ✅ | ✅ (96 of them) | ❔ |
| Media registry resolves | ✅ | — (no media in samples) | ❔ |
| Text extraction | ✅ | ✅ | ❔ |
| Edit text, leave every other object alone | ✅ | ✅ | ❔ |

Developed against one real Pages document (a 15 MB German magazine article,
485 objects, two TIFFs and two charts) and two Numbers spreadsheets from
[numbers-parser](https://github.com/masaccio/numbers-parser)'s test suite
(738 and 647 objects, 97 and 37 streams).

### Keynote status

Keynote is **implemented but unverified** — no `.key` file was available when
this was written.

That is a narrower gap than it sounds. Layers 1–3 are not app-specific: Keynote
packages are the same stored ZIP, the same Snappy framing and the same
`TSP.ArchiveInfo` stream, and `keynote-parser` and friends have been treating
them that way for years. Text lives in the same `TSWP.StorageArchive`. What is
missing is layer 4: the `KN.*` document-level types are absent from the registry
rather than guessed at, and `Kind` detection falls back to the `.key` extension
because there is no sample to derive a structural signature from.

To close it, drop a `.key` into `tests/fixtures/` and run `cargo test`. If the
suite is green, Keynote is verified to the same standard as the other two, and
the registry can be filled in from `iwork inspect`.

## Testing

No iWork documents are committed — they are other people's files. Supply your
own:

```
cp ~/Documents/Anything.pages tests/fixtures/
cargo test

# or point at a directory you already have
IWORK_FIXTURES=~/Documents cargo test
```

With no fixtures the integration tests skip and say so, so a fresh clone is
green. The unit tests always run: they build synthetic archives in memory and
cover the parts that are easy to get subtly wrong — varint round-trips,
repeated-field ordering, and objects straddling a Snappy block boundary.

## Limitations

- **Previews go stale.** The `preview*.jpg` thumbnails are not regenerated, so
  the Finder and iCloud will show the old first page until iWork re-saves the
  document. Regenerating them means rendering the layout, which is a far larger
  project than reading and writing the file.
- **Editing text truncates styling past the edit.** Attribute runs are clamped
  into the new length, not remapped onto the new wording. See
  [`text::write`](src/text.rs).
- **No layout, no rendering, no formula evaluation.** This reads and rewrites
  the document; it does not understand it.
- **"iWork opens it" is not tested here.** The tests prove the bytes are
  structurally correct and survive an independent decode. Verifying that Pages,
  Numbers and Keynote accept the output needs a Mac in the loop.
- **Encrypted documents are not supported.**

## Prior art

[numbers-parser](https://github.com/masaccio/numbers-parser),
[keynote-parser](https://github.com/psobot/keynote-parser) and
[iWorkFileFormat](https://github.com/obriensp/iWorkFileFormat) mapped much of
this territory first, in Python and Objective-C. This crate is an independent
Rust implementation working from the bytes up, and is deliberately schema-less
where those projects carry extracted `.proto` files.

## Legal

Reverse engineering a file format to achieve interoperability is permitted under
EU Directive 2009/24/EC Art. 6 and Swiss URG Art. 21. This repository contains no
Apple code, no Apple `.proto` files and no iWork documents.

## License

MIT — see [LICENSE](LICENSE).
