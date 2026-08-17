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

// Text styles, by copy-and-adjust
for style in doc.text_styles() {
    println!("{} {} {:?}", style.identifier, style.kind.as_str(), style.name());
}
let kicker = doc.create_text_style(3712, "Kicker")?;
doc.apply_text_style(6083, 0..8, kicker.identifier)?;  // UTF-16 code units

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

iwork styles       Report.pages           # every text style, with its object id
iwork style        Report.pages 3712      # one style, field by field, and what uses it
iwork new-style    Report.pages 3712 Kicker out.pages
iwork set-style    Report.pages 3801 11.12=f32:18 out.pages
iwork apply-style  Report.pages 6083 0 8 3801 out.pages
iwork delete-style Report.pages 3801 3712 out.pages    # 3712 replaces it
iwork paragraphs   Report.pages 6083      # paragraph ranges, for apply-style
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

## Text styles

A `TSWP.StorageArchive` holds no formatting. It holds *attribute tables* —
lists of `{character_index, reference}` entries, each run reaching to the next —
and the objects those references land on are the styles. Fields 5, 7 and 8 point
at character, list and paragraph styles respectively.

[`style`](src/style.rs) gives you CRUD over them, and it splits along exactly
that line:

|  | How it works |
|---|---|
| **Read** | enumerate styles, their names, their fields, and every run that uses them |
| **Create** | **copy** an existing style, allocate an identifier above `PackageMetadata` field 1, list the copy wherever the template was listed |
| **Update** | rewrite a field by path (`11.12`), or hand the decoded archive to a closure |
| **Delete** | re-point or drop the runs, unlist it, refuse if a reference would be left dangling |
| **Apply** | point a character range at a style, splitting the run table and restoring what followed |

Everything that decides *which text gets which style* works on the attribute
tables, whose shape is asserted by the test suite. Nothing decides what is
*inside* a style: this crate does not claim to know that bold is field 1 or the
font size field 12, because unlike a wrong name in the registry, a wrong field
number there would write wrong bytes into your document. So `iwork style` prints
the field tree with a path for every field, and `iwork set-style` takes those
paths — the numbers come from the document in front of you, not from a guess.

Creating by copying is the same rule [`FORMAT.md`](FORMAT.md) gives for whole
documents, for the same reason: the Pages sample spends 313 objects on its
stylesheet, and a style that already works is a better starting point than a
synthesised one.

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
| Attribute tables point at styles of the matching kind | ❔ | ❔ | ❔ |
| Copy a style: one new object, text untouched | ❔ | ❔ | ❔ |
| Apply a style, leave every other stream alone | ❔ | ❔ | ❔ |

The three style rows are asserted by `cargo test` like everything else above
them, but no fixture was available when they were written, so none has yet been
run against a real document — hence ❔ rather than ✅ even for Pages. The style
*logic* is covered without fixtures by `tests/styles.rs`, which builds a
document in memory and runs the whole cycle through the real ZIP and IWA
writers; what the fixture rows would add is the confirmation that real documents
are shaped the way that synthetic one assumes.

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
- **Styles are edited by field number, not by name.** There is no
  `set_bold(true)`: the meaning of a style archive's fields is not published, so
  [`style`](src/style.rs) will not guess at it. Read the field tree with `iwork
  style`, compare two styles that differ in the way you want, and set the field
  that moved. New styles come from copying, so the fields you never touch stay
  whatever the template had.
- **Deleting a style is refused rather than forced.** If a reference this crate
  cannot account for would be left dangling, the delete fails and says which
  objects still hold one.
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
