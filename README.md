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
    println!("{} {} {:?}", style.identifier, style.kind.as_str(), style.name);
}
let kicker = doc.create_text_style(3712, "Kicker")?;                    // copy one that works
doc.set_text_style_property(kicker.identifier, style::property::FONT_SIZE,
                            Some(Value::Fixed32(18f32.to_le_bytes())))?;
doc.apply_text_style(6083, 0..8, kicker.identifier)?;                   // UTF-16 code units

doc.save("Report-edited.pages")?;
```

## CLI

```
cargo install --path .

iwork inspect   Report.pages              # package, components, media, object census
iwork text      Report.pages              # every text storage, with its object id
iwork set-text  Report.pages 6083 "…" out.pages
iwork objects   Budget.numbers 2001       # every object of one message type
iwork dump      Talk.key 1                # one object, field by field
iwork check     Report.pages              # look for a broken object graph
iwork extract   Report.pages ./media      # embedded media, byte-identical
iwork roundtrip Report.pages out.pages    # decode and re-encode every object

iwork styles       Report.pages           # every text style, with its object id
iwork style        Report.pages 3712      # one style, field by field, and what uses it
iwork new-style    Report.pages 3712 Kicker out.pages
iwork set-style    Report.pages 3801 font-size=f32:18 out.pages
iwork set-style    Report.pages 3801 11.3=f32:18       out.pages   # the same field
iwork apply-style  Report.pages 6083 0 8 3801 out.pages
iwork delete-style Report.pages 3801 3712 out.pages    # 3712 replaces it
iwork paragraphs   Report.pages 6083      # paragraph ranges, for apply-style
iwork properties                          # every named style property, and its evidence
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
- **Nothing is touched either.** `save` re-encodes only the streams whose
  objects actually changed; the rest keep their original bytes exactly. Editing
  one style in a 97-stream Numbers document rewrites one stream, and a save with
  no edits reproduces every entry byte for byte. That is not just cheaper — a
  save that re-compresses everything moves every Snappy block boundary, and then
  nothing in the file distinguishes the edit you meant from the noise you did
  not. `iwork` prints which streams it rewrote.
- **Names are advisory.** [`registry`](src/registry.rs) maps type numbers to
  names like `TSWP.StorageArchive`, tagged `Confirmed`, `Inferred` or
  `Unverified`. It feeds human-readable output only; a wrong name cannot
  break parsing.

## Text styles

A `TSWP.StorageArchive` holds no formatting. It holds *attribute tables* —
lists of `{character_index, reference}` entries, each run reaching to the next —
and the objects those references land on are the styles. Field 5 points at
**paragraph** styles, field 7 at list styles and field 8 at **character** styles
— the opposite of what the field order suggests, and of what this crate assumed
until a probe document settled it.

[`style`](src/style.rs) gives you CRUD over them, and it splits along exactly
that line:

|  | How it works |
|---|---|
| **Read** | enumerate styles, their names, their fields, and every run that uses them |
| **Create** | **copy** an existing style, allocate an identifier above `PackageMetadata` field 1, list the copy wherever the template was listed |
| **Update** | rewrite a field by path (`11.3`) or by name (`font-size`), or hand the decoded archive to a closure |
| **Delete** | re-point or drop the runs, unlist it, refuse if a reference would be left dangling |
| **Apply** | point a character range at a style, splitting the run table and restoring what followed |

Everything that decides *which text gets which style* works on the attribute
tables, whose shape is asserted by the test suite.

What is *inside* a style is a weaker kind of knowledge, and the split matters:
a wrong name in the registry only prints wrong, while a wrong field number
writes wrong bytes into your document. So the fields are addressed by path, and
`iwork style` prints the tree with a path for every one — the numbers come from
the document in front of you. A handful have been pinned down by comparing 654
styles against names the app itself assigned, and those have names:

```
iwork style     Report.pages 2857                        # what is in there
iwork set-style Report.pages 2857 font-size=f32:18 out.pages
```

`iwork properties` lists them, with what backs each name:

```
  bold                     11.1         measured in an imported document
  underline-width          11.30        observed changing alongside a measured one
  outline-level            12.27        name only, not observed here
```

Most were established by experiment rather than by correlation. A document was
built in which every paragraph differed from a baseline in exactly one property
— 37pt, `#123456`, 175%, 17pt — and imported into Pages; diffing each resulting
style against the baseline's leaves one changed field per probe. All four probe
colours came back byte-exact, which is what turned the colour field from a guess
into a fact. `bold` and `italic` are *toggles*, independent of the font's own
weight.

Some properties come in pairs — the value, and a boolean saying it is
deliberately *none*. Removing a field means "inherit from the parent style";
setting the companion means "explicitly nothing". Those are different documents.

**A style does not have one text colour, it has up to four.** The font colour,
the fill drawn inside the glyphs, and the underline and strikethrough colours
that follow the text — Pages writes all of them together, and **the fill is what
gets drawn**. Setting only `red`/`green`/`blue` leaves the fill behind, and the
text renders in its old colour. Use `set_text_style_color`, or `iwork set-color`,
which writes every one the style keeps:

```
iwork set-color Report.pages 2857 0.85 0.10 0.10 out.pages
```

**Setting a property whose container is missing fails, deliberately.** A style
with no colour cannot simply be given one: a colour is `{model, r, g, b, a,
space}`, and a container this crate invents would hold only the channels it was
asked for. Pages crashes on opening such a document — confirmed, not theorised.
Get the container from a style that has one, with `create_text_style` or
`copy_text_style_property`, then change the channels on the copy.

Creating by copying is the same rule [`FORMAT.md`](FORMAT.md) gives for whole
documents, for the same reason: the Pages sample spends 313 objects on its
stylesheet, and a style that already works is a better starting point than a
synthesised one. A copy is listed in the stylesheet the template names — and
only there. Bare style references elsewhere can be positions rather than
memberships (a Keynote slide's five outline levels are exactly that), and adding
an entry to one of those corrupts it.

## What is verified

Everything below is asserted by `cargo test` when you supply fixtures.

| | Pages | Numbers | Keynote |
|---|---|---|---|
| Open, identify, decode every object | ✅ | ✅ | ✅ |
| Object streams survive re-encode byte for byte | ✅ | ✅ | ✅ |
| Components resolve to real streams | ✅ | ✅ (96 of them) | ✅ (29) |
| Media registry resolves | ✅ | — (no media in samples) | ✅ (33) |
| Text extraction | ✅ | ✅ | ✅ |
| Edit text, leave every other object alone | ✅ | ✅ | ✅ |
| Attribute tables point at styles of the matching kind | ✅ | ✅ | ✅ |
| Copy a style: one new object, text untouched | ✅ | ✅ | ✅ |
| Apply a style, leave every other stream alone | ✅ | ✅ | ✅ |
| A copy keeps the template's kind (named vs variation) | ✅ | ✅ | ✅ |

Developed against one real Pages document (a 15 MB German magazine article,
485 objects, two TIFFs and two charts) and two Numbers spreadsheets from
[numbers-parser](https://github.com/masaccio/numbers-parser)'s test suite
(738 and 647 objects, 97 and 37 streams). The style work was checked against
four further Pages documents and one Keynote deck — 654 styles in all.

### Keynote status

Keynote **is** verified now, against one deck: 1204 objects, 30 streams, 19
masters, 5 slides. Layers 1–3 turned out to be exactly as predicted — same
stored ZIP, same Snappy framing, same object stream, text in the same
`TSWP.StorageArchive`, styles in the same attribute tables. Layer 4 held one
surprise worth knowing about:

**Numbers and Keynote both number their document archive `1`.** The app-level
archives are numbered per app, so the root object's type cannot tell those two
apart, and a `.key` read by type alone came back as a spreadsheet. `Kind`
detection now goes by components — `Index/Tables/` for Numbers, `Index/Slide*`
for Keynote — and [`registry`](src/registry.rs) entries carry the app they
belong to, so type 1 resolves to `TN.DocumentArchive` or `KN.DocumentArchive`
depending on the document, and to neither when the kind is unknown.

The `KN.*` types derived from that deck are in the registry with their evidence:
the show and its slide list, slide nodes, slides (which carry their transition),
masters, the theme (which carries its name), and drop-cap styles. See
[`FORMAT.md`](FORMAT.md#keynote).

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
green. Fixtures are found recursively, so a whole directory tree of them works.
The unit tests always run: they build synthetic archives in memory and cover the
parts that are easy to get subtly wrong — varint round-trips, repeated-field
ordering, and objects straddling a Snappy block boundary.

If you have Pages, Numbers and Keynote, you can have the apps write you a corpus
instead:

```
scripts/make-fixtures.sh          # into tests/fixtures/generated/, gitignored
```

Seven documents that between them cover plain and styled text, non-Latin text
including emoji, a table and an image, two sheets of typed cells and formulas, a
300-row imported table, and a deck of slides with presenter notes and a skipped
slide. Existing files are left alone unless `--force` is given.

And the check the rest of the suite cannot make — does the app open it?

```
scripts/app-check.sh out.pages "A new headline"   # exit 0 if Pages agrees
scripts/app-check.sh --self-test Report.pages     # prove it fails when it should

IWORK_APP_CHECK=1 cargo test                      # every fixture, through the app
```

`app-check.sh` opens the document in the app that owns its extension, reads back
body text, cell values, slide text and presenter notes, looks for a string if
you give it one, and closes without saving. `--self-test` corrupts a copy of a
document it has just accepted and checks that the app refuses it, because a
harness that always says yes is worse than none.

## Limitations

- **Previews go stale.** The `preview*.jpg` thumbnails are not regenerated, so
  the Finder and iCloud will show the old first page until iWork re-saves the
  document. Regenerating them means rendering the layout, which is a far larger
  project than reading and writing the file.
- **Editing text truncates styling past the edit.** Attribute runs are clamped
  into the new length, not remapped onto the new wording. See
  [`text::write`](src/text.rs).
- **Most style fields have no name.** Ten are named; the rest of the property
  bag is addressed by number, because the meaning is not published and
  [`style`](src/style.rs) will not guess. Read the tree with `iwork style`,
  compare two styles that differ in the way you want, and set the field that
  moved. New styles come from copying, so fields you never touch keep whatever
  the template had.
- **Editing a named style may change nothing.** Text usually points at an
  anonymous *variation* style that inherits from the named one and overrides
  some fields. `iwork style` prints what a style inherits from; edit the style
  the runs actually point at.
- **A copy of a variation style stays anonymous.** Named styles and variations
  are different things, and an object that is flagged a variation, carries a
  name and has no internal identifier is neither — Pages crashes on opening the
  document. `create_text_style` therefore applies the requested name only when
  the template has one, and reports which happened.
- **Deleting a style is refused rather than forced.** If a reference this crate
  cannot account for would be left dangling, the delete fails and says which
  objects still hold one.
- **A reference that leaves its component has to be declared.** The document
  body and the stylesheet are separate components, so pointing text at a style
  is two edits: the run, and a `ComponentInfo.external_references` entry saying
  where the style lives. Without the second, iWork never loads the style — and
  the failure is *quiet*: one such document opened in Pages with the paragraph
  simply unstyled, as though nothing had been done, and another crashed on open.
  `apply_text_style` and `create_text_style` maintain the declarations;
  `iwork check` reports any that are missing.
- **No layout, no rendering, no formula evaluation.** This reads and rewrites
  the document; it does not understand it.
- **"iWork opens it" is not tested here, and it is not a formality.** The tests
  prove the bytes are structurally correct and survive an independent decode.
  They cannot prove an app will accept the result, and the difference is real:
  documents that pass every check in this repository — including `iwork check`,
  which finds nothing wrong with them — have crashed Pages on opening. Each time,
  the fix has been to find an invariant the real documents hold to exactly, teach
  `iwork check` to assert it, and maintain it on write; the checker is that much
  sharper each round, and still not a substitute for opening the file. Anything
  written by this crate needs trying in the app before it is trusted —
  `scripts/app-check.sh` is how, and `IWORK_APP_CHECK=1 cargo test` runs it over
  every fixture, on a machine that has the apps.
- **Applying a character style may not change how text looks.** Pointing a run
  at a different character style is accepted and survives a reopen, but has not
  been observed to change the rendering, so something else evidently wins.
  Unresolved.
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
