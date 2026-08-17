# The iWork file format

What `.pages`, `.numbers` and `.key` actually contain, as of iWork 6/13-era
documents (`fileFormatVersion 2.3.4`).

Derived by observation from real documents; every structural claim here is
asserted by the test suite. Where something is inferred rather than proven, it
says so.

Documents used:

- a 15 MB Pages article — 485 objects, 8 streams, 2 TIFFs, 2 charts, German text
- two Numbers spreadsheets — 738 and 647 objects, 97 and 37 streams
- four further Pages documents, used for the style graph — 654 styles between them
- a Keynote deck — 1204 objects, 30 streams, 19 masters, 5 slides, 33 media files

An older, pre-2013 `.pages` is an entirely different format — a bundle around an
XML `index.xml.gz`. None of this applies to those.

---

## 1. The package

A ZIP archive. **Every entry uses compression method 0 (stored).** iWork relies
on that so media can be mapped straight out of the file, and the index streams
carry their own compression already.

| Entry | Purpose |
|---|---|
| `Index/*.iwa` | The document — a protobuf object graph (§2–§4) |
| `Data/*` | Media, byte-identical to what was placed into the document |
| `Metadata/Properties.plist` | Binary plist: format version, document/share/version UUIDs |
| `Metadata/DocumentIdentifier` | Bare UUID, ASCII, no trailing newline |
| `Metadata/BuildVersionHistory.plist` | XML plist: which app builds have written the file |
| `preview.jpg`, `preview-web.jpg`, `preview-micro.jpg` | First-page thumbnails |

This layer is identical in all three apps. A Numbers document with no media has
no `Data/` directory at all, but the rest is unchanged.

`Properties.plist`, from the Pages sample:

```
fileFormatVersion = '2.3.4'
revision          = '0::4F18C3C1-1479-49CA-84B9-895C6DDFFBF0'
isMultiPage       = True
shareUUID         = '85848C13-7875-4C7E-BB9A-81BB1891ED72'
documentUUID      = '85848C13-7875-4C7E-BB9A-81BB1891ED72'
versionUUID       = '4F18C3C1-1479-49CA-84B9-895C6DDFFBF0'
```

---

## 2. IWA framing

Each `.iwa` is a sequence of **raw Snappy blocks** — no Snappy stream header, no
CRCs — behind a four-byte header:

```
repeat until EOF:
  u8   0x00                marker
  u24  compressed_length   little endian
  ..   snappy raw block    decompresses to at most 65536 bytes
```

The 64 KiB block size is confirmed by observation: a 43 KB stylesheet stream
decompresses to blocks of `[65536, 65536, 65536, 43329]`.

> **Object framing is independent of block boundaries.** A single object can
> straddle two Snappy blocks, so every block must be concatenated *before* the
> object stream is parsed. Parsing block by block is the classic way to get this
> wrong; it works on small documents and fails on large ones.

---

## 3. The object stream

The concatenated bytes are flat and length-delimited:

```
repeat until end of stream:
  varint  archive_info_length
  ..      TSP.ArchiveInfo
  ..      payload of each declared message, back to back
```

`TSP.ArchiveInfo`:

| Field | Type | Meaning |
|---|---|---|
| 1 | varint | object identifier, unique across the whole package |
| 2 | repeated message | `TSP.MessageInfo`, one per payload |

`TSP.MessageInfo`:

| Field | Type | Meaning |
|---|---|---|
| 1 | varint | message type — index into iWork's message registry |
| 2 | packed varints | schema version, e.g. `[1, 0, 5]` |
| 3 | varint | payload length in bytes |

Payloads follow the `ArchiveInfo` immediately, in declaration order. Every
object in every sample carries exactly one message.

**References** are always `{1: <object identifier>}` — a `TSP.Reference` wrapping
an integer. There are no file offsets anywhere; resolution is by identifier
across the whole package.

Apple ships no `.proto` files, and the message type is the only thing
identifying a payload's schema. A general reader either carries a registry
scraped out of the iWork binaries, or works at the wire level and infers meaning
from position in the graph. This crate does the latter.

### Type numbering

Ranges are grouped by framework and are stable across the three apps:

| Range | Framework | |
|---|---|---|
| 1–999 | TSP / TSK | persistence, document kit |
| 2000s | TSWP | word processing — text, character/paragraph/list styles |
| 3000s | TSD | drawables — images, groups, masks |
| 4000s | TSCE | calculation engine |
| 5000s | TSS | stylesheets |
| 6000s | TST | tables |
| 10000s | app | `TP.*` (Pages), `TN.*` (Numbers), `KN.*` (Keynote) |
| 11000s | TSP package | package metadata |
| 12000s | TSA / charts | |

---

## 4. The document graph

### Component index

One `TSP.PackageMetadata` (type 11006) is the table of contents.

- **field 1** — highest object identifier ever allocated. New objects must take
  identifiers above this, and this field must be bumped to match.
- **field 3** — repeated `ComponentInfo`:
  `{1: id, 2: preferred_name, 3: file_name, 6: repeated external reference,
  12: save_token}`.

  A component's stream is **`Index/{file_name or preferred_name}.iwa`** —
  verified against a 96-component Numbers document, 96 of 96.

- **field 4** — repeated `DataInfo`, the media registry:
  `{1: id, 2: 20-byte digest, 3: original name, 4: name under Data/,
  5: theme-asset path, 10: extension carrying pixel size}`.

  Assets that come from the app's own theme bundle are referenced by path in
  field 5 and **not** copied into the package — field 4 is empty for those.
  Drawables refer to media by the numeric `DataInfo` id, never by filename.

### Roots, per app

The root object always has identifier 1. Its **message type is what
distinguishes the three apps** in the object graph:

| App | Root type | Also distinctive |
|---|---|---|
| Pages | `10000` `TP.DocumentArchive` | `DocumentStylesheet` component |
| Numbers | `1` `TN.DocumentArchive` | ~100 components under `Index/Tables/` |
| Keynote | `1` `KN.DocumentArchive` | `Index/Slide*`, `Index/TemplateSlide-*` |

> **The root type does not identify the app.** Numbers and Keynote both use
> message type `1`: the app-level archives are numbered *per app*, starting from
> 1, so the same number means `TN.DocumentArchive` in one and
> `KN.DocumentArchive` in the other. Only Pages, at 10000, is unambiguous. The
> components are what separate the other two, and they do it cleanly.
>
> The framework ranges in §3 are not affected — `TSWP.StorageArchive` really is
> 2001 everywhere. It is the low numbers that collide.

The Pages root, decoded:

```
2  -> 3543  stylesheet         30/31  f32  595.28 × 841.89 pt (A4)
3  -> 3545                     32..35 f32  56.69 pt margins
4  -> 3473  body text storage  36/37  f32  header 35.43 / footer 42.52 pt
6  -> 3542                     43     str  'Brother_DCP_9020CDW'
7  -> 3547  theme              44     str  'iso-a4'
15 { 4: 'de_CH', 9: 'de_CH', 3: 'de', 9: 'Application/Blank/ISO' }
```

Geometry is plain `float`s in PostScript points. The last printer used is
persisted in the document.

### Text — `TSWP.StorageArchive` (type 2001)

The same in all three apps. Text and formatting are stored separately.

| Field | Contents |
|---|---|
| 2 | reference to the owning stylesheet |
| 3 | the text, UTF-8, **repeated** — long text is split into several runs |
| 5 | character-attribute table |
| 6 | packed paragraph/bidi flags |
| 7 | list-style table |
| 8 | paragraph-style table |

Every attribute table has the same shape: repeated entries of
`{1: character_index, 2: reference to a style object}`, strictly increasing by
index. A run starts at its index and continues until the next entry.

Paragraphs are `\n` within a single storage; there is no per-paragraph object.
Each shape on the page owns its own storage — the Pages sample has 62 of them
for the body, headline, pull quotes, captions, chart labels and credits.

**Run indices are character offsets, not byte offsets.** In a storage reading
`"Von Benjamin Keller\nVeröffentlicht am 07.09.2017\nim Magazin …"` the
character-attribute table holds `[0, 20, 49]`, which is exactly the paragraph
starts counted in characters; counted in UTF-8 bytes they would be
`[0, 20, 50]`. Three further storages agree. The samples are entirely BMP, so
they cannot separate UTF-16 code units from Unicode scalars — this crate assumes
UTF-16, since the text model is NSString-backed.

Two placeholder characters show up as the entire contents of a storage:
`U+FFFC OBJECT REPLACEMENT CHARACTER` stands in for an embedded drawable (it is
what every Numbers table storage contains), and `U+0004` appears alone in the
Pages body storage of a document whose text all lives in shapes.

### Text styles — `TSWP.*StyleArchive` (types 2021–2023)

The objects an attribute table points at. Which table points at which kind is
consistent across the samples:

| Storage field | Points at | Type |
|---|---|---|
| 5 | character styles | 2022 `TSWP.CharacterStyleArchive` |
| 7 | list styles | 2023 `TSWP.ListStyleArchive` |
| 8 | paragraph styles | 2021 `TSWP.ParagraphStyleArchive` |

Fields 9, 10 and 11 are further tables of the same shape whose targets have not
been identified. Field 6 is packed flags, not a table.

Styles are **shared, not owned**: several storages point at one style object,
and one storage points at the same style from several runs. So editing a style
changes every run that uses it, and giving one run different formatting means
making a new style object, not editing the one that is there.

Styles are listed in a stylesheet — the 5000s (TSS) range — two ways in the
Pages sample:

```
repeated  {1: <style id>}          plain references: the styles it contains
repeated  {1: 'body', 2: {1: id}}  keyed: a well-known identifier -> a style
```

Both are *inferred*. What is solid is that a plain reference is a plain
reference: `TSP.Reference` is `{1: id}` and nothing else, everywhere in the
format, which is enough to add a style to the list a template was in without
knowing the stylesheet's schema.

The base message, field 1, is the same in all three kinds:

| Field | Contents |
|---|---|
| 1.1 | name as the app shows it — `"Titel"`. Absent on variation styles |
| 1.2 | internal identifier — `"text-1-paragraphstyle-Title"` |
| 1.3.1 | reference to the style this one inherits from |
| 1.5.1 | reference to the stylesheet it belongs to |

Most styles in a real document are **variations**: anonymous, no 1.1, a parent
at 1.3.1, and a property bag overriding a field or two. That is what iWork
writes when text is formatted directly rather than by picking a named style,
and it is why editing "Titel" can leave the title looking exactly as it did —
the run points at a variation that overrides the same field.

The property bag is field 11. These were derived by comparing 654 styles
against names the app assigned them, and are the only fields whose *meaning* is
claimed here:

| Field | Meaning | How well established |
|---|---|---|
| 11.1 | bold toggle, 0/1 | good — set on every `Titel`; independent of the font's own weight, so a style on `HelveticaNeue-Bold` may leave it 0 |
| 11.2 | italic toggle, 0/1 | good — the only styles setting it are the `Zitat` ones, whose font is an upright cut |
| 11.3 | font size, points, f32 | strong — `Titel` 24/24/30/30, `Zwischenüberschrift 1` 16, `Text` 11/11/11/12 |
| 11.5 | PostScript font name | certain — the values are font names |
| 11.7 | colour: `{3: r, 4: g, 5: b, 6: a}`, floats 0–1 | shape certain, meaning inferred. Every one of 530 samples is opaque black, so they cannot show whether it is the *font's* colour |

Everything else in the bag is left unnamed rather than guessed at — a wrong
field number writes wrong bytes, where a wrong name in the registry only prints
wrong. `iwork style <file> <id>` prints the whole tree with a path per field,
which is how the table above was built and how the rest can be.

### Stylesheets

The document stylesheet is **type 401** — in the TSP/TSK range, not the TSS
range the name suggests. Every style names it at 1.5.1, which is the reliable
way to find it. It holds, at the top level:

```
repeated 1  {1: style id}                     the styles it contains
repeated 2  {1: identifier, 2: {1: style id}} keyed by internal identifier
repeated 5  {1: ref, 2: ref, …}               a style with its variations
```

The Pages sample's stylesheet carries 327 plain references and 267 keyed
entries. Types in the 5000s (TSS) also appear and are stylesheets of narrower
scope — six per document in the samples, attached to charts.

> A bare `{1: id}` reference is not by itself proof of membership in a list.
> `KN.SlideArchive` field 31 holds five of them, one per outline level, and it
> is a positional array: adding an entry shifts the mapping rather than listing
> a style. Add to the stylesheet the style itself names, and nowhere else.

### Images — `TSD.ImageArchive` (type 3005)

```
1  { geometry: position (-157.00, -122.68), size 872.49 × 581.53 }
4  placed size 872.49 × 581.53   (points)
9  natural size 2126 × 1417      (pixels)
12 { 1: 11 }  -> DataInfo id 11
15 { 1: 16 }  -> filtered variant
16 { 1: 17 }  -> filtered thumbnail
19 crop path: explicit 4-point polygon
```

iWork keeps up to four representations of a placed image — original,
downscaled, filtered, filtered thumbnail — and chooses per context.

### Numbers specifics

Numbers is the same format with a different graph shape:

- Components are spread over `Index/Tables/` — `DataList`, `Tile`,
  `HeaderStorageBucket` — roughly one stream per table column or region. A
  two-sheet document produced 97 streams.
- The 6000s (TST) range dominates: `6004` and `6005` account for over 150
  objects in a small spreadsheet.
- `Index/CalculationEngine.iwa` holds the 4000s (TSCE) objects.
- Cell text still lives in `TSWP.StorageArchive`, so text extraction needs no
  special casing.

### Keynote

Layers 1–3 are identical, as expected: the same stored ZIP, the same Snappy
framing, the same object stream. Text is in `TSWP.StorageArchive` and styles in
the same attribute tables, so §"Text" and §"Text styles" apply unchanged.

Layer 4, from one deck:

```
1      KN.DocumentArchive      object 1; field 2 -> the show
2      KN.ShowArchive          3: repeated slide refs   4: {1: 1920.0, 2: 1080.0}
                               5: -> document stylesheet   2: -> theme
4      KN.SlideNodeArchive     one per slide, in the show's list; 2 -> the slide
5      KN.SlideArchive         1 -> its master
                               4.2.8: {"Transition", "none", 1.0, 0.5}
9      KN.MasterSlideArchive   one per Index/TemplateSlide-*.iwa
10     KN.ThemeArchive         1.3: theme name, e.g. "58_Startup_Simple_PM"
10024  KN.DropCapStyleArchive  identified "dropcap-style-N" in the TSS base
```

The slide size is a plain pair of floats in points — 1920 × 1080 in the sample,
so Keynote stores 16:9 at pixel dimensions rather than the 1024 × 768 of older
decks.

A deck's slides may hold no text at all: in the sample every one of the slides'
`TSWP.StorageArchive` objects is empty except one holding `U+FFFC`, and all the
readable text belongs to the masters' placeholders. Text extraction that finds
nothing on a slide is not necessarily a bug.

Beware the outline levels: `KN.SlideArchive` field 31 is five bare style
references, one per outline level. It looks exactly like a stylesheet's style
list and is a *positional array* — an entry added to it does not list a style,
it shifts the mapping from level to style.

---

## Writing documents

Generate **from a template**, not from nothing. The container, the framing and
the text model are all straightforward. The style graph is not: the Pages sample
spends 313 objects and 240 KB uncompressed on its stylesheet alone, and iWork is
unforgiving about dangling references. Opening a blank document, saving it, and
using that as a skeleton is far less work than synthesizing a valid stylesheet.

Rules a writer must respect:

1. **Stored ZIP entries only** — never deflate.
2. **Concatenate Snappy blocks before parsing**, and split at 64 KiB when
   writing.
3. **Allocate new object identifiers above `PackageMetadata` field 1**, and bump
   that field.
4. **Register every new `Data/` file** in the `DataInfo` table; drawables refer
   to media by id.
5. **Fix attribute-run tables** whenever text length changes.
6. **Keep run tables strictly increasing**, and free of entries that repeat the
   entry before them — an entry that draws no boundary is not what iWork writes,
   and editing accumulates them.
7. **Never leave a dangling reference.** Removing a style means removing every
   reference to it: the runs that use it *and* the stylesheet entries that list
   it.
8. **Previews go stale** — they are not regenerated by anything but iWork.
