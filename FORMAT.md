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

  A component's identifier is also the identifier of its **root object**, so
  `{1: id}` on its own names the whole component.

#### External references

Field 6 of a `ComponentInfo` is the component's declaration of every object it
refers to that lives in **another** component:

```
repeated 6  {1: target's component, 2: target object, 3: is_weak}
```

Field 2 is omitted when the reference is to the component as a whole, and
field 3 is rare — of the 2,382 entries across six documents, 2,183 name an
object, 199 name only a component, and 47 set `is_weak`.

**This list is exact, and it is load-bearing.** Across five Pages documents and
one Keynote document, 4,362 objects — every reference that crosses a component
boundary is declared, without exception. Write a reference and leave
it undeclared and iWork never loads what it points at: a Pages document that
pointed a paragraph at an undeclared style **opened with the paragraph simply
unstyled**, as though the edit had never happened, and a second one crashed
Pages on open. The silent failure is the dangerous one — the file opening is no
evidence the edit survived.

`iwork check` asserts this, and `Document::apply_text_style` maintains it.

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
| 5 | **paragraph**-style table |
| 6 | packed paragraph/bidi flags |
| 7 | list-style table |
| 8 | **character**-attribute table |

Every attribute table has the same shape: repeated entries of
`{1: character_index, 2: reference to a style object}`, strictly increasing by
index. A run starts at its index and continues until the next entry.

Paragraphs are `\n` within a single storage; there is no per-paragraph object.
Each shape on the page owns its own storage — the Pages sample has 62 of them
for the body, headline, pull quotes, captions, chart labels and credits.

> **Fields 5 and 8 are the other way round from what the order suggests.** The
> evidence is right here: the run indices of field 5 "are exactly the paragraph
> starts", as recorded below — a character-attribute table has no reason to sit
> on paragraph boundaries and field 8's does not. Confirmed independently by
> importing a document in which each paragraph varied one property: alignment
> and indents landed in the style referenced from field 5, bold and font size in
> the one from field 8.
>
> A paragraph table may also carry a final entry at the *end* of the text, which
> is where the style of a paragraph not yet typed comes from.

Paragraphs end at `\n` — and also at **`U+0005`**, which appears where a
document changes layout mid-storage, and at **`U+0004`**, which marks a section
boundary. The paragraph table puts a run immediately after either, so reading
one as ordinary text splits the paragraphs one character wrong.

`U+0004` was found in a document Pages built from its "Project Proposal"
template, whose body storage reads `…123-4567\n\u{4}Company Name\n` at each of
its two section breaks, with a paragraph run on the `C`. It is the same
character that turns up *alone* as the whole of a body storage in a document
whose text lives in shapes — a section marker with nothing either side of it.

**Run indices are character offsets, not byte offsets.** In a storage reading
`"Von Benjamin Keller\nVeröffentlicht am 07.09.2017\nim Magazin …"` the
character-attribute table holds `[0, 20, 49]`, which is exactly the paragraph
starts counted in characters; counted in UTF-8 bytes they would be
`[0, 20, 50]`. Three further storages agree. The samples are entirely BMP, so
they cannot separate UTF-16 code units from Unicode scalars — this crate assumes
UTF-16, since the text model is NSString-backed.

Two placeholder characters show up as the entire contents of a storage:
`U+FFFC OBJECT REPLACEMENT CHARACTER` stands in for an embedded drawable (it is
what every Numbers table storage contains), and `U+0004` — the section marker
above — appears alone in the Pages body storage of a document whose text all
lives in shapes.

### Text styles — `TSWP.*StyleArchive` (types 2021–2023)

The objects an attribute table points at. Which table points at which kind is
consistent across the samples:

| Storage field | Points at | Type |
|---|---|---|
| 5 | paragraph styles | 2022 `TSWP.ParagraphStyleArchive` |
| 7 | list styles | 2023 `TSWP.ListStyleArchive` |
| 8 | character styles | 2021 `TSWP.CharacterStyleArchive` |

**2021 is the character archive and 2022 the paragraph one** — the opposite of
what public prior art says. Across six documents, all 12 styles of type 2021
carry an internal identifier of the form `character-style-…` and all 229 of type
2022 `…-paragraphstyle-…`; type 2022 also carries paragraph properties, which a
character style has no use for.

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
at 1.3.1, a flag at 1.4, and a property bag overriding a field or two. That is
what iWork writes when text is formatted directly rather than by picking a named
style, and it is why editing "Titel" can leave the title looking exactly as it
did — the run points at a variation that overrides the same field.

> **The two are not interchangeable.** An object that carries the variation flag
> *and* a name *and* no internal identifier, listed among the named styles, is
> not a thing iWork writes, and Pages crashes on opening a document containing
> one. Copy a variation and it stays a variation.

Fields within a message are written in **ascending field number**, everywhere
this has been looked at. Protobuf does not require it and a decoder will not
care, but a rewritten archive that appends a field at the end no longer looks
like anything the app would have produced, and "looks like what the app writes"
is the only correctness standard available without a Mac in the loop.

Field **11** is the character property bag — present in both kinds, because a
paragraph style carries character properties too. Field **12** is the paragraph
bag, and only paragraph styles have one.

These were settled by experiment: a document was built in which every paragraph
differed from a baseline in exactly one property, with unmistakable values, and
imported into Pages. Diffing each resulting style against the baseline leaves
one changed field per probe.

| Field | Meaning | Asked for → stored |
|---|---|---|
| 11.1 | bold toggle | bold → `1`, plus a `-Bold` font name |
| 11.2 | italic toggle | italic → `1`, plus `-Oblique` |
| 11.3 | font size, points | 37pt → `37` |
| 11.5 | PostScript font name | Courier New → `"CourierNewPSMT"` |
| 11.7 | **font colour** `{3: r, 4: g, 5: b, 6: a}` | `#123456` → `0.070588, 0.203922, 0.337255` |
| 11.9 | language | fr-FR → `"fr"` |
| 11.10 | 1 superscript, 2 subscript | |
| 11.11 | 1 underline, 2 double underline | |
| 11.12 | strikethrough | |
| 11.13 | 1 all caps, 2 small caps | |
| 11.14 | baseline shift | 6pt up → `12` |
| 11.21 | shadow `{1: colour, 2: angle, 3: offset, 5: opacity}` | → `45°, 1, 0.5` |
| 11.26 | text background | `#ABCDEF` → `0.670588, 0.803922, 0.937255` |
| 11.27 | tracking, as a fraction of font size | 3pt on 12pt → `0.25` |
| 11.44 | outline `{1: colour, 2: width}`, with 11.45 the switch | |
| 12.1 | alignment: 1 right, 2 centre, 3 justified | |
| 12.6 | paragraph background | `#FEDCBA` → `0.996078, 0.862745, 0.729412` |
| 12.7 | first-line indent, points | 36pt → `36` |
| 12.10 | keep with next | |
| 12.11 | left indent, points | 72pt → `72` |
| 12.13.2 | line spacing, as a multiple | 175% → `1.75` |
| 12.14 | page break before | |
| 12.19 | right indent, points | 48pt → `48` |
| 12.20 / 12.21 | space after / before, points | 23pt, 17pt |
| 12.25.1.1 | tab stop position, points | 144pt → `144` |
| 12.26 | widow and orphan control | |
| 12.32 / 12.15 / 12.45 | paragraph border, its colour and width | |
| 12.40.1 | reference to the paragraph's list style | |

A style keeps its text colour in **more than one place**, and they are expected
to agree: `11.7` the font colour, `11.46.1` the fill drawn inside the glyphs,
and `11.23`/`11.29` the strikethrough and underline colours that follow the
text. Choosing a colour in Pages writes all of them. The fill is what is drawn —
a title whose `11.7` said red and whose `11.46.1` still said black renders
black, which is a confusing way to discover this.

A colour is complete or it is nothing: all six fields, every time. A style
given a colour of `{3: r, 4: g, 5: b}` — no model, no alpha — is a document
Pages crashes on rather than opens, which is worth knowing before writing one.

All four colours came back byte-exact — `#123456` is `18/255, 52/255, 86/255` —
which is what makes the colour fields certain rather than merely plausible.
Horizontal character scaling was asked for and did not survive the import, so
either Pages does not store it or it lands somewhere this method cannot see.

Everything else in the bags is left unnamed rather than guessed at: a wrong
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
repeated 5  {1: parent, 2: child, 2: child…}  a style with its variations
```

A style is listed **twice**: once among field 1, and once as a child of its
parent in field 5. Every style in the six samples that names a parent and is
listed in a stylesheet appears in its parent's entry — 109 of 109. A copy that
joins the plain list but not the family is a shape no real document takes.

Field 5's entries and field 2's are told apart by their first field: a family
entry is headed by a reference, a keyed entry by a string. Only the family
entry may be extended; duplicating a keyed one would either collide on the key
or invent one.

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
  `HeaderStorageBucket` — roughly one stream per table side table. A two-sheet
  document produced 97 streams.
- The 6000s (TST) range dominates: `6004` (cell styles) and `6005` (data lists)
  account for over 150 objects in a small spreadsheet.
- `Index/CalculationEngine.iwa` holds the 4000s (TSCE) objects — **and, in every
  Numbers document written by 15.3.1 here, the `TST.TableInfoArchive` and
  `TST.TableModelArchive` objects too.** The table's tiles and data lists are in
  `Index/Tables/`; the model that points at them is not.
- **Cell text is not in `TSWP.StorageArchive`.** `iwork text` finds zero text
  storages in a Numbers document the app reads 2711 cell values out of: a
  Numbers cell's text is an integer key into the table's own string list. See
  §Tables. (Pages and Keynote tables are the other way round — their cells
  point at `TSWP.StorageArchive`s through the rich-text list.)

### Keynote

Layers 1–3 are identical, as expected: the same stored ZIP, the same Snappy
framing, the same object stream. Text is in `TSWP.StorageArchive` and styles in
the same attribute tables, so §"Text" and §"Text styles" apply unchanged.

Layer 4, from one deck:

```
1      KN.DocumentArchive      object 1; field 2 -> the show
2      KN.ShowArchive          2: theme  3: slide tree  4: size  5: stylesheet
4      KN.SlideNodeArchive     one per slide, in the slide tree; 2 -> the slide
5      KN.SlideArchive         1: style  4: transition  5: title placeholder
                               31: five body paragraph styles, one per level
9      (unnamed)               one per Index/TemplateSlide-*.iwa
10     KN.ThemeArchive         1.3: theme name, e.g. "58_Startup_Simple_PM"
10024  drop-cap style          identified "dropcap-style-N" in the TSS base
```

The slide's field **31** is the trap: five bare style references, one per
outline level. It has the shape of a stylesheet's style list and is a
positional array — adding an entry shifts the mapping rather than listing a
style.

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

## 5. Tables — `TST`

The one part of the format that is not protobuf all the way down. `TST` is
cross-app: a Numbers sheet, a Pages page and a Keynote slide all hold the same
archives, and the only difference is what the table's drawable hangs off.

Everything below was read out of documents written by Numbers 15.3.1 and Pages
15.3.1, and the cell-level claims were then put to Numbers itself: `tests/tables.rs`
asks the app for the value, the data format and the formula of every cell of
every table of three spreadsheets through AppleScript and compares — **2943
cells, all agreeing**. Where something is stated without that backing it says
so.

### The object graph

```
TST.TableInfoArchive (6000)          the drawable: geometry, parent, caption
  1  TSD.DrawableArchive super
     2  parent           -> the sheet (Numbers) or the containing drawable
  2  TSP.Reference       -> TST.TableModelArchive
  7  TSP.UUID group_by_uuid          categories
  8  TSP.UUID hidden_states_uuid

TST.TableModelArchive (6001)
  1  string  table_id            uppercase UUID, e.g. "68B384C7-9E7F-…"
  4  TST.DataStore               INLINE, not a reference
  6  uint32  number_of_rows      counting headers and footers
  7  uint32  number_of_columns
  8  string  table_name          "Zellarten"
  9  uint32  number_of_header_rows          0–5
  10 uint32  number_of_header_columns       0–5
  11 uint32  number_of_footer_rows
  12 bool    header_rows_frozen
  13 bool    header_columns_frozen
  14 uint32  number_of_hidden_rows          user + filtered
  15 uint32  number_of_hidden_columns
  16 double  default_row_height             19.929931640625 in a new table
  17 double  default_column_width           98
  40 uint32  number_of_filtered_rows
  41 uint32  number_of_user_hidden_rows     filter-hidden and user-hidden are
  42 uint32  number_of_user_hidden_columns  separate counts, not one
  47 TST.MergeOwnerArchive        INLINE — the merges live here (below)
  70 TST.HiddenStatesOwnerArchive INLINE
  84 TSCE.HauntedOwnerArchive     the table's identity for cross-table formulas

TST.DataStore (inline at TableModelArchive #4)
  1  HeaderStorage rowHeaders     INLINE: {1: hash fn, 2: repeated Reference}
  2  Reference     columnHeaders  a SINGLE HeaderStorageBucket, not a storage
  3  TileStorage   tiles          INLINE: {1: repeated {1: tileid, 2: Reference},
                                           2: tile_size (256), 3: wide rows}
  4  Reference  stringTable       -> TableDataList, listType 1  STRING
  5  Reference  styleTable                          listType 4  STYLE
  6  Reference  formula_table                       listType 3  FORMULA
  12 Reference  formulaErrorTable                   listType 5
  13 Reference  merge_region_map   ** absent from every document seen here **
  17 Reference  rich_text_table                     listType 8  RICH_TEXT
  21 Reference  control_cell_spec_table             listType 12 CONTROL_CELL_SPEC
  22 Reference  format_table                        listType 2  FORMAT
```

The row/column asymmetry at `DataStore` 1 and 2 is real and is Apple's: rows get
a list of bucket references, columns get one bucket.

`TST.HeaderStorageBucket` (6006) holds repeated `{1: index, 2: float size,
3: hidingState, 4: numberOfCells, 5: cell_style, 6: text_style}`. **Field 2 is a
literal `0` for a row or column still at the table's default**, and the entry is
missing altogether for a row or column with no cells; a size of its own means
the user dragged it. In `numbers-formats.numbers` exactly one column carries
`150` and one row carries `40`, and every other row and column of every other
table in the corpus carries `0` or nothing. `hidingState` is 0 throughout the
corpus — nothing here can hide a row from a script, so filter-hidden versus
user-hidden is not yet separated at this level.

`TST.TableDataList` (6005, and 6201 as a second registration) is the interning
mechanism the whole cell format rests on: `{1: listType, 2: nextListID,
3: repeated ListEntry}`, where a `ListEntry` is `{1: key, 2: refcount}` plus one
payload field — `3` a string, `4` a reference, `5` a formula, `6` a
`TSK.FormatStructArchive`, `9` a rich-text payload reference, `12` a
`TST.CellSpecArchive`. **Keys start at 1 and a stored key of 0 means "none"**, so
it is a map and not an array.

### Tiles and rows

`TST.Tile` (6002) holds `{1: maxColumn, 2: maxRow, 3: numCells, 4: numrows,
5: repeated TileRowInfo, 6: storage_version, 7: last_saved_in_BNC,
8: should_use_wide_rows}`. A tile covers `tile_size` = **256** rows; the 301-row
fixture has two, and a row's absolute index is
`tileid * tile_size + tile_row_index`. Rows with no cells have no `TileRowInfo`
at all, so counting them is not an alternative.

**Numbers 15.3.1 writes neither field 6 nor field 7 on the tile.** The published
advice is to refuse a tile whose `last_saved_in_BNC` is not `true`; every tile
in this corpus would be refused by that rule. The version that is actually there
is `TileRowInfo` field 5, and it says 5.

`TST.TileRowInfo`:

| # | Contents |
|---|---|
| 1 | `tile_row_index`, within the tile |
| 2 | `cell_count` — non-empty cells in the row |
| 3, 4 | the pre-BNC buffer and offsets. `required`, always present, **meaningless** |
| 5 | `storage_version` — 5 |
| 6 | `cell_storage_buffer` — the cell records, concatenated |
| 7 | `cell_offsets` — `int16[]`, little-endian, **signed**, one per column |
| 8 | `has_wide_offsets` — offsets count groups of four bytes |

Slicing a row: `-1` means the column has no cell, and a record runs to the
**next non-negative** offset, not to the next one. The array is padded out with
`-1` well past the table's width — 255 entries for a five-column table.

### The cell record

Fixed 12-byte header, then optional payloads.

| Byte | Contents |
|---|---|
| 0 | storage version. **5**, or this is not the layout below |
| 1 | cell type, `TST.CellType` (below) |
| 2–5 | zero in all 2515 records read here |
| 6 | which data format the user *chose* (below) |
| 7 | undecoded. `0x08` on currency cells; `0x00` otherwise |
| 8–11 | `u32` flag word, little-endian |

Each set bit of the flag word introduces one payload, and **the payloads follow
the header in ascending bit order**:

| Bit | Bytes | Field |
|---|--:|---|
| `0x00000001` | 16 | decimal128 value — number and currency |
| `0x00000002` | 8 | `double` — boolean (`> 0.0`) and duration (seconds) |
| `0x00000004` | 8 | `double` — date, seconds since 2001-01-01, timezone-naive |
| `0x00000008` | 4 | key into STRING |
| `0x00000010` | 4 | key into RICH_TEXT_PAYLOAD |
| `0x00000020` | 4 | cell style key |
| `0x00000040` | 4 | text style key |
| `0x00000080` | 4 | conditional style key |
| `0x00000100` | 4 | conditional rule key |
| `0x00000200` | 4 | key into FORMULA |
| `0x00000400` | 4 | key into CONTROL_CELL_SPEC |
| `0x00000800` | 4 | key into FORMULA_ERROR |
| `0x00001000` | 4 | **format kind** — which of the six format slots is this cell's own |
| `0x00002000` | 4 | number format key |
| `0x00004000` | 4 | currency format key |
| `0x00008000` | 4 | date format key |
| `0x00010000` | 4 | duration format key |
| `0x00020000` | 4 | text format key |
| `0x00040000` | 4 | boolean format key |
| `0x00080000` | 4 | comment key |
| `0x00100000` | 4 | import-warning key |

The empty cell is the header with `flags = 0`:
`05 00 00 00 00 00 00 00 00 00 00 00`.

**The order is load-bearing and it is not the only order in the record.** A bit
whose meaning is of no interest still advances the cursor by its width, and byte
6 numbers the same formats differently — duration is `0x04` there and `0x10000`
here, date is `0x08` there and `0x8000` here. A decoder that took its field
positions from byte 6 would return two keys swapped with every length still
adding up. Two things say the order above is the right one, and neither is a
proto file: every one of 2515 records in the corpus ends exactly on its last
field (`every_cell_record_is_consumed_to_the_byte`), and the `0x1000` payload —
1 number, 2 currency, 3 date, 4 duration, 5 text, 6 boolean — numbers the six
format slots in exactly this sequence.

Byte 1, `TST.CellType`: `0` empty, `1` span, `2` number, `3` text, `4` formula,
`5` date, `6` boolean, `7` duration, `8` error, `9` rich text, and **`10`
currency, which is in no published enum**. This is *not* `TST.CellValueType`,
which the protobuf form of a cell uses and which numbers almost every case
differently.

decimal128 is Apple's layout, not IEEE 754's: sign is bit 7 of byte 15, a
14-bit exponent is bits 6–0 of byte 15 followed by bits 7–1 of byte 14, biased
by `0x1820`, and a 113-bit mantissa is bytes 0–13 plus bit 0 of byte 14, little
endian. Numbers normalises to fifteen significant digits — `3.14159` is stored
as `314159000000000 × 10⁻¹⁴` — so a reader that prints the mantissa without
trimming gets the right number spelled wrong.

### Data formats

A cell's format is a `TSK.FormatStructArchive` interned in the FORMAT list, and
`format_type` (field 1) is what names it. The values start at 256:

| | | | |
|--:|---|--:|---|
| 256 | number | 262 | fraction |
| 257 | currency | 263 | checkbox |
| 258 | percent | 267 | rating |
| 259 | scientific | 268 | duration |
| 260 | automatic | 269 | numeral system |
| 261 | date and time | | |

Each was read off a cell whose format Numbers then named through AppleScript.
264, 265 and 266 are unclaimed: three gaps between checkbox and rating, and
exactly the three remaining controls, but nothing here produced one — a pop-up,
stepper or slider cell carries a plain **number** format and a control
definition instead.

Which format a cell displays takes three things, and the first two are easy to
miss:

1. **A control wins.** A slider cell is `format_type` 256 plus a
   `TST.CellSpecArchive` whose `interaction_type` is 5, and Numbers calls it a
   slider.
2. **Byte 6 says whether any format was chosen at all** — `0x01` number,
   `0x02` currency, `0x04` duration, `0x08` date, `0x20` boolean, `0x80` text.
   Every cell carries a format key in some slot; a plain text cell points at
   `format_type` 260. Without byte 6 there is nothing separating a cell that
   holds a number from a cell the user made a number, and both would report the
   same format. (The distilled prior art calls byte 6 a second presence mask
   over the same keys. It is not: it is zero on cells that do carry keys.)
3. **The chosen slot has to be the cell's own**, which is what the `0x1000`
   payload says. The header of a column formatted as currency is a *text* cell
   carrying a currency key with the currency bit set, and Numbers draws plain
   text and reports `automatic`.

The text slot is its own answer: Numbers writes `format_type` 260 into it, which
everywhere else means automatic, and calls the cell text.

Control cells are `TST.CellSpecArchive` in the CONTROL_CELL_SPEC list:
`{1: interaction_type, 2: formula, 3/4/5: range min/max/increment,
6: pop-up menu model, 7: start with first}`. Observed values of
`interaction_type`: **4** stepper, **5** slider, **6** rating, **7** pop-up menu
(with a `TST.PopUpMenuModel`, 6206, holding the items), **8** checkbox.

### Merged ranges

A merge is stored nowhere near the cells it covers. The covered cells have no
record at all — not even a `spanCellType` one — and their offsets are the plain
`-1` of an empty cell, so nothing about a cell says it is merged away.

What Numbers 15.3.1 writes instead is a **formula per merged range**, in the
formula store of the table's `merge_owner` (`TableModelArchive` field 47):

```
47 MergeOwnerArchive
   1 CFUUIDArchive owner_id
   2 FormulaStoreArchive
     2 next_formula_index
     3 repeated FormulaStorePair { 1: index, 2: TSCE.FormulaArchive }
                                          1: ASTNodeArrayArchive
                                             1: repeated ASTNodeArchive
```

The first node of each formula carries the range. `COLON_TRACT_NODE` (type 67)
puts it in `AST_colon_tract` (field 40) as absolute column (3) and row (4)
ranges of `{1: range_begin, 2: range_end?}` — an absent `range_end` means the
same as `range_begin`. `CELL_REFERENCE_NODE` (type 36) puts a single cell in
`AST_column` (26) and `AST_row` (27), **zigzag-encoded**, which is a merge one
cell square: Numbers really does write those, and the way to get one is to type
into the top-left cell of a merge, which pulls that cell back out of it.

`DataStore.merge_region_map` — the form the published references describe, with
`CellID.packedData` = `(column << 16) | row` — is read as a fallback and is
**unverified here**: nothing this repository can produce writes it. The same
goes for the merge owner's `TSCE.FormulaOwnerDependenciesArchive`
back-dependencies, which mirror the ranges and are read by nothing.

The app confirms the ranges without ever offering a merge property: **a
merged-away cell is reported by AppleScript under the name, value and format of
the cell the merge began in.** `B2:D2` comes back as `B2 B2 B2`, which is a
merge map by another route, and `tests/tables.rs` checks the decoded merges
against it.

### How a table is organised

Everything above is the cell grid. Layered on top of it are the sort rules and
filters that decide which rows are shown and in what order, the categories that
group them, the conditional-highlighting rules that recolour them, the custom
formats that reformat them, and the pivot tables that summarise them.

**None of this is addressable by index.** Cells are; organisation is not. Every
row, column, group and owner carries a `TSP.UUID` (`{1: lower, 2: upper}`),
because a sort or a filter moves indexes and a UUID survives it.

Everything below was read out of four documents Numbers 15.3.1 wrote from
Apple's own templates — `Categories`, `Pivot Table Basics`, `My Stocks` and
`Note Taking Colourful Log`. **Nothing in Numbers' scripting interface can
sort, filter, categorise, highlight or pivot a table**, and the menu items that
can need a document window, so a template is the only way this repository can
produce one of these documents. `scripts/make-fixtures.sh` builds them; the
recipe is a template `id` — the path inside the app bundle, which is the same
on every Mac, unlike the localised `name`.

#### The UUID index — `TST.ColumnRowUIDMapArchive` (6267)

```
1 repeated TSP.UUID sorted_column_uids     SORTED BY UUID, not by position
2 repeated uint32   column_index_for_uid   the index of sorted_column_uids[i]
3 repeated uint32   column_uid_for_index
4 repeated TSP.UUID sorted_row_uids
5 repeated uint32   row_index_for_uid
6 repeated uint32   row_uid_for_index
```

`TableModelArchive` field 46 is the *base* map — the one to use. Field 6 of the
table **info** points at a second one for the *view* order, which is longer,
because a categorised table's view includes its summary rows.

The trap is field 1's name. Reading `sorted_column_uids[i]` as "the UUID of
column *i*" is correct for every column that is a fixed point of the
permutation in field 2, which in a five-column table is usually most of them,
so the mistake survives a casual check and then mislabels one pivot field.

#### Hidden rows and columns

`HeaderStorageBucket` field 3, `hidingState`, has two values in the corpus and
both are in one document:

| `hidingState` | Meaning |
|---|---|
| 0 | visible |
| 1 | the user hid it |
| 2 | a filter hid it |

`numbers-rules.numbers` has three columns hidden by hand and nine rows hidden
by a filter rule, and `tests/tables.rs` asserts both. Phase 1 could only report
the number, because nothing in the scripting interface hides a row.

**The model's hidden counts are not maintained.** `number_of_hidden_rows` (14),
`number_of_hidden_columns` (15), `number_of_filtered_rows` (40),
`number_of_user_hidden_rows` (41) and `number_of_user_hidden_columns` (42) are
all **zero** in that document, while three columns and nine rows are hidden. A
reader that trusts them reports a table with nothing hidden. Only `hidingState`
is reliable.

The other half is `TableModelArchive` field 70, an inline
`HiddenStatesOwnerArchive`:

```
70 HiddenStatesOwnerArchive
   1 TSP.UUID owner_uid
   2 repeated HiddenStatesArchive
     1 TSP.UUID hidden_states_uid
     2 HiddenStateExtentArchive column_hidden_state_extent
     3 HiddenStateExtentArchive row_hidden_state_extent

HiddenStateExtentArchive
   1 TSP.UUID hidden_state_extent_uid
   2 repeated RowOrColumnState base_hidden_states
        { 1: TSP.UUID row_or_column_uid, 2: user_hidden, 3: filtered,
          4: pivot_hidden }
   3 enum   row_or_column_direction   0 column, 1 row
   7 repeated TSP.UUID collapsed_group_uids
   8 Reference filter_set
```

For **columns** this agrees with `hidingState` exactly: the three entries with
`user_hidden` are the three columns whose state is 1. For **rows** it does not —
the extent carries an entry with `filtered` set for *every body row of the
table*, not only the nine the filter hides. Whatever `base_hidden_states` means
on the row side, it is not "the hidden rows"; the crate reads it and reports it
separately, and `hidingState` is what answers the question.

#### Sort — `TableModelArchive` field 44

```
44 TableSortOrderArchive
   1 enum SortType   0 entire_table, 1 row_range
   2 repeated SortRuleArchive { 1: uint32 index, 2: Direction 0 asc / 1 desc }
```

Every table in every document carries this field with `type = 0` and no rules;
an empty archive is not a sort. `numbers-sorted.numbers` has one rule, on
column index 2, ascending.

#### Filters — `TST.FilterSetArchive` (6220)

```
1 enum FilterSetType     0 All (AND), 1 Any (OR)         default All
2 bool is_enabled                                        default TRUE
3 repeated FilterRulePrePivotArchive filter_rules_prepivot
4 bool needs_formula_rewrite_for_import
5 repeated uint32 filter_offsets      the column each rule is on
6 repeated bool   filter_enabled      per-rule, for the field-7 rules
7 repeated FilterRuleArchive filter_rules
```

A document with three tables carries **seven** of these, all eight bytes long
and all empty — a table has a filter set whether or not it filters anything, so
"has a filter set" is not "is filtered".

**Numbers 15.3.1 writes field 3, the slot the published references call
legacy** — not field 7, which they call current. The two are wire-incompatible
at the same number and are told apart structurally: `FilterRulePrePivotArchive`
is `{1: FormulaPredicatePrePivotArchive, 2: bool disabled}` whose predicate
begins with a length-delimited *formula* at field 1, while `FilterRuleArchive`
is `{1: FormulaPredicateArchive}` whose field 1 is a varint `predicate_type`.
Field 1's wire type is the discriminator.

`FormulaPredicateArchive` (current): `1 predicate_type`, `2/3 qualifier1/2`,
`4/5/6 param_value0/1/2` (`FormulaPredArgArchive`, whose `arg_value` at 2 is a
`FormulaPredArgDataArchive` — `1 double`, `4 string`, `5 date`, `6 duration`,
`8 bool`), `7 formula`, `8 for_conditional_style`.
`FormulaPredicatePrePivotArchive` (legacy): `1 formula`, `2 predicate_type`,
`3/4 qualifier1/2`, `5/6/7 param_index1/2/0`.

`predicate_type` codes are **not named here**. Apple publishes none, and this
crate has four values from one document: 7 and 9 against a number, 36 against a
string, 37 in a filter. Naming the rest would be a guess.

#### Categories — `TST.GroupByArchive` (6373)

Written **twice** in the same document: inline at `TableModelArchive` field 81,
the field the schema marks `category_owner_deprecated`, and again by reference
at field 86 through a `CategoryOwnerRefArchive` (6372), whose one repeated field
is a list of references. The two hold the same tree. The referenced form is the
one to prefer.

```
6373 GroupByArchive
  1  TSP.UUID group_by_uid
  2  repeated GroupColumnArchive   { 1: column_uid, 2: grouping_type,
                                     3: grouping_functor, 4: grouping_column_uid }
  3  GroupNodeArchive group_node_root       inline …
  5  repeated ColumnAggregateArchive        the summary-row assignments
  6  bool is_enabled                        the Categories switch
  17 repeated Reference aggregator_ref  -> 6382
  18 Reference group_node_root_ref      -> 6383   … and again by reference

6383 GroupNodeArchive        ** field 2 does not exist **
  1  TSP.UUID group_uid
  3  repeated GroupNodeArchive child        inline …
  5  repeated CellCoordinateArchive agg_formula_coords
  6  FormatManagerArchive format_manager
  7  CellValueArchive group_cell_value      the value the group is on
  8  IndexSetArchive row_indexes
  9  IndexSetArchive row_lookup_uids
  10 repeated Reference child_ref           … and again by reference

ColumnAggregateArchive
  1 column_uid   2 level   3 agg_type   4 show_as_type
```

15.3.1 writes field 9 and not field 8, and its ranges are row indexes:
`{range_begin, range_end}` **inclusive**. In the fixture the two groups claim
rows 1–5 and 6–10, and those are exactly the rows whose grouping column holds
`Andy` and `Chloe`.

`agg_type` **2 is Sum**, proven twice: the pivot fixture's value column is drawn
by the app as `Units (Sum)`, and the category fixture's accumulator for the same
code holds 275, the sum of its ten values — not their count, mean, minimum or
maximum. The other codes are unknown and are reported as codes.

The summary rows themselves are a `TST.SummaryModelArchive` (6316) hanging off
the table **info** at field 4, with its own inline `DataStore`. Its per-level
heights and visibilities are the unbounded lists at fields **26–28**; the
fixed five-level fields 11–25 are deprecated, and a fresh table writes six
entries in each list.

**Collapsed groups are `HiddenStateExtentArchive.collapsed_group_uids` (7)** —
the only persisted home for the state; `TST.ExpandCollapseStateArchive` exists
but appears solely inside command (undo) archives. **Unverified**: no template
ships a collapsed group and no script can collapse one, so the field is decoded
and empty everywhere here.

#### Pivot tables — `TST.PivotOwnerArchive` (6370)

```
    ** there is no field 1 **
2  TSP.UUID pivot_owner_uid
3  GroupColumnListArchive grouping_columns_for_rows      { 1: repeated GroupColumn }
4  GroupColumnListArchive grouping_columns_for_columns
5  ColumnAggregateListArchive aggregate_columns          the Values well
6  int32 flattening_dimension
7  bool  is_empty_pivot
8  TSP.UUID source_table_uid
11 bool  hide_grand_total_rows
12 string source_table_name
13 bool  hide_grand_total_columns
```

Reached from `TableModelArchive` field 85; `TableInfoArchive.is_a_pivot_table`
(16) is the flag that says which of the two tables sharing the archive is the
pivot.

**A pivot's fields name columns of its source table**, so resolving them needs
that table's UUID map, not the pivot's own. The join is not equality:
`source_table_uid` and the source's `haunted_owner.owner_uid`
(`TableModelArchive` 84) **differ in their lower halves by a small constant** —
35 in the fixture — because every owner a table has is a numbered offset from
one base UUID. The **upper half is the table's identity** and is what matches.

Checked against the app's own output: the pivot table Numbers drew in the same
document reads `Power` and `Product` down the side, `Date (Month)` across the
top and `Units (Sum)` for the values, and those are columns 2, 1, 0 and 3 of
`Sales` — which is what the rules resolve to. The date field's `grouping_type`
is 7 and it carries a `grouping_functor`; the plain ones are type 0 with none.

#### Conditional highlighting — `TST.ConditionalStyleSetArchive` (6010)

```
1 uint32 ruleCount
2 repeated ConditionalStyleRulePrePivot rules_prepivot
3 ConditionalStyleRules rules   { 1: repeated ConditionalStyleRule }

ConditionalStyleRule  { 1: FormulaPredicateArchive, 2: cell_style, 3: text_style }
```

Here — unlike the filter — **15.3.1 writes both slots, with the same rules in
each**. They are not equivalent: only the current shape (field 3) carries the
value the rule compares against, because the pre-pivot shape keeps it inside a
formula. Reading both double-counts; reading only field 2 loses the values.

A set is reached by key from the table's CONDITIONAL_STYLE `TableDataList`
(`DataStore` field 18), so a whole column of highlighted cells shares one set
and one key. The fixture's four rules come back as predicate 7 and 9 against
`"0"` and predicate 36 against `"↑"` and `"↓"`, which is what its inspector
shows.

#### Custom cell formats — `TSK.CustomFormatListArchive` (222)

Document-scoped, not table-scoped: one archive per document, and cells reach
into it by UUID.

```
222 CustomFormatListArchive
  1 repeated TSP.UUID uuids            parallel arrays
  2 repeated CustomFormatArchive custom_formats
        { 1: name, 2: format_type_pre_bnc, 3: FormatStructArchive default_format,
          4: repeated Condition, 5: format_type }
```

`Condition` is `{1: condition_type, 2: float, 3: FormatStructArchive, 4: double}`
— the conditional sub-rules a custom format can switch on. The format string the
user typed is `FormatStructArchive.custom_format_string` (18).
`numbers-rules.numbers` holds one: `Millions`, `format_type` 270,
`#,###.##M`. **270 is outside the 256–269 range §Data formats records** and is
not otherwise observed.

### What a Numbers document does not have

No `TSWP.StorageArchive` anywhere: `iwork text` reads nothing out of a
spreadsheet whose cells the app reads 2711 values out of. Pages and Keynote
tables are the opposite — their cells are `automaticCellType` (9) with a
rich-text key into the RICH_TEXT_PAYLOAD list, whose entries are
`TST.RichTextPayloadArchive` (6218) `{1: storage, 2: range, 3: cellid}`, and the
text is in that storage. The Pages fixture's table mixes both: cells the
template wrote are rich text, cells a script wrote are plain strings in the
string table.

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
   writing. Re-compressing a stream you did not change is not free of
   consequence: the block boundaries move, so the entry differs from the
   original even though the objects do not. Leave untouched streams alone and an
   edited document differs from its original only where it was edited.
3. **Allocate new object identifiers above `PackageMetadata` field 1**, and bump
   that field.
4. **Register every new `Data/` file** in the `DataInfo` table; drawables refer
   to media by id.
5. **Fix attribute-run tables** whenever text length changes.
6. **Keep run tables strictly increasing**, and free of entries that repeat the
   entry before them — an entry that draws no boundary is not what iWork writes,
   and editing accumulates them.
7. **Never leave a dangling reference.** Removing a style means removing every
   reference to it: the runs that use it, the stylesheet entries that list it,
   *and* its place in its parent's family entry.
8. **Declare every reference that leaves its component** in the referring
   component's `external_references`. An undeclared one does not always crash —
   it can simply make the edit invisible, which is worse.
9. **Previews go stale** — they are not regenerated by anything but iWork.
