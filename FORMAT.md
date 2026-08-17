# The iWork file format

What `.pages`, `.numbers` and `.key` actually contain, as of iWork 6/13-era
documents (`fileFormatVersion 2.3.4`).

Derived by observation from real documents; every structural claim here is
asserted by the test suite. Where something is inferred rather than proven, it
says so.

Documents used:

- a 15 MB Pages article — 485 objects, 8 streams, 2 TIFFs, 2 charts, German text
- two Numbers spreadsheets — 738 and 647 objects, 97 and 37 streams
- no Keynote document was available; see "Keynote" below

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
| Keynote | not observed | — |

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

**What a style archive contains is not documented here**, and `src/style.rs`
does not assume it. The samples show a nested base message carrying the style's
name and an internal identifier as plain strings, and a property bag beside it,
but which numbered field is the font size or the weight was not derived from the
samples and is deliberately not guessed at — a wrong field number would write
wrong bytes rather than merely print a wrong label. `iwork style <file> <id>`
prints the field tree so the numbers can be read off the document at hand.

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

No `.key` sample was available. Layers 1–3 are not app-specific and prior art
treats Keynote identically, so the container, framing, object stream and
`TSWP`/`TSD`/`TSS` objects should all apply unchanged. What is unknown is the
`KN.*` document-level types and the root archive's type number. Those are
deliberately absent from `src/registry.rs` rather than guessed at.

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
