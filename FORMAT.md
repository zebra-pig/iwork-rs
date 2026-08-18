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

Payloads follow the `ArchiveInfo` immediately, in declaration order. Almost
every object carries exactly one message; the exceptions are below.

### Version patches — `MessageInfo.type == 0`

`0` is not a message type. It marks a **patch**: an older encoding of the object
the first message already holds, kept so that an older app opening the same file
gets a shape it understands. The `ArchiveInfo` says `should_merge` (field 3) and
each patch carries the extra fields that place it:

| Field | Meaning |
|---|---|
| 7 | `base_message_index` — which message this patches. `0` in everything here |
| 8 | `diff_merge_version` — the app version the patch is *for* |
| 9 | `diff_field_path` — when present, the payload is a partial message applied at that path rather than to the whole object |
| 10 | `fields_to_remove` — drop these from the base before merging |
| 11 | `diff_read_version` — the minimum reader |

A patch's `MessageInfo.version` is the sentinel `[0xFFFF, 0xFFFF, 0xFFFFFFFF]`.

**What Numbers, Pages and Keynote 15.3.1 actually write** — measured over the
twelve-document corpus and again after an edit made by Numbers itself:

- **One patched object per Numbers document** — the `TN.UIStateArchive` (12026)
  in the view-state component, always with three patches, for `[11,0,*]`,
  `[10,1,*]` and `[10,0,*]`, each with `fields_to_remove = [28]` and a payload
  holding nothing but two copies of field 28. So the merge is: take the base,
  drop field 28, add the patch's — an older encoding of one field.
- **And charts, which is Phase 6's correction to this section.** A chart whose
  *type* postdates an older release carries a patch of its own, and that one
  does use `diff_field_path`: the path is `[10000]`, into
  `TSCH.ChartArchive`, and the payload is the single field `chart_type` set to
  the nearest type the older app has — donut (25) falls back to pie (5), radar
  (27) to bar (2). See §10. No Pages or Keynote document in the corpus carries
  a patch — but none of them has a donut or a radar chart either, and the
  archive is cross-app, so "Pages never patches" is not the rule; "a chart too
  new for an old reader patches itself" is.
- **No table archive carries one.** No tile, no `TableDataList`, no
  `TableModelArchive`, no header bucket. Editing a cell therefore never has to
  merge a patch or re-emit one, which is the whole reason this had to be
  settled before any table write shipped.
- Having Numbers edit one cell and save produced no new patched object: the
  same single view-state archive, and ten of 103 entries rewritten — the two
  tiles holding the changed cell and the formula that depends on it, the
  calculation engine, the document, the metadata, the previews, and a
  view-state component that came back under a **new stream name with new object
  identifiers**. The app reallocates that component on every save.

The rule this crate follows, and the reason for each half:

1. **Read the first message; ignore the patches.** That is what the current app
   does, and it is what makes `iwork dump` agree with the app.
2. **Never write one.** There is nothing to gain: a document this crate writes
   is a document 15.3.1 wrote, with one value changed.
3. **Never rewrite the first message of an object that has patches.** The
   patches would then describe the object as it used to be, and the file would
   say two different things depending on who opened it. `Document::set_cell`
   refuses on that ground; `iwork check` prints the patched objects as a note.

The published implementations get this wrong in two different directions —
keynote-parser raises on a `diff_field_path` longer than one element,
numbers-parser ignores `fields_to_remove` silently. A single-element
`diff_field_path` *is* exercised, by every donut and radar chart; a longer one
still is not.

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

The same in all three apps. Text and formatting are stored separately: field 3
is the text, and everything else that says anything about it is an **attribute
table** — a list of entries anchored to character indices.

| Field | Contents | Wire |
|---|---|---|
| 1 | `kind`: 0 body, 1 header, 2 footnote, 3 text box, 4 note, 5 cell, 6 unclassified, 7 TOC, 8 undefined. Default 3 | varint |
| 2 | reference to the owning stylesheet | message |
| 3 | the text, UTF-8, **repeated** — in practice always one element | bytes |
| 4 | `has_itext` — the storage holds fields or attachments | varint |
| 10 | `in_document` — reachable from the body, as against the pasteboard | varint |
| 13 | a gap in every version of the schema | — |

#### The full attribute-table inventory

Twenty-two of them. The list is Apple's, read off the `TSWPArchives.proto`
descriptor carved from the installed 15.3.1 binaries; the **anchoring** column
is this repository's, and is what decides what an edit does to each one.

| Field | Apple's name | Anchoring | Entries point at | In the corpus |
|---:|---|---|---|---|
| 5 | `table_para_style` | paragraph | `TSWP.ParagraphStyleArchive` (2022) | 389 storages |
| 6 | `table_para_data` | paragraph | `{first, second}` — **`first` is the list level** | 389 |
| 7 | `table_list_style` | paragraph | `TSWP.ListStyleArchive` (2023) | 389 |
| 8 | `table_char_style` | run | `TSWP.CharacterStyleArchive` (2021) | 2 |
| 9 | `table_attachment` | character | `TSWP.DrawableAttachmentArchive` (2003), `TSWP.NumberAttachmentArchive` (2043) | 18 |
| 11 | `table_smartfield` | run | the smart fields, 2031–2042 — **2032 is the hyperlink** | 7 |
| 12 | `table_layout_style` | paragraph | `TSWP.ColumnStyleArchive` (2024) | 4 |
| 14 | `table_para_starts` | paragraph | `{first, second}`, both 0 | 389 |
| 15 | `table_bookmark` | run | `TSWP.BookmarkFieldArchive` (2035) | — |
| 16 | `table_footnote` | character | `TSWP.FootnoteReferenceAttachmentArchive` (2008) | — |
| 17 | `table_section` | paragraph | `TP.SectionArchive` / `TSWP.SectionPlaceholderArchive` (10011) | 4 |
| 18 | `table_rubyfield` | run | `TSWP.RubyFieldArchive` (2042) | — |
| 19 | `table_language` | run | a BCP-47 string | 1 |
| 20 | `table_dictation` | run | a dictation metadata string | — |
| 21 | `table_insertion` | run | `TSWP.ChangeArchive` (2060), insertion | — |
| 22 | `table_deletion` | run | `TSWP.ChangeArchive` (2060), deletion | — |
| 23 | `table_highlight` | run | `TSWP.HighlightArchive` (2013) — a comment anchor | — |
| 24 | `table_para_bidi` | paragraph | `{first, second}` — writing direction | 389 |
| 25 | `table_overlapping_highlight` | range | `TSWP.HighlightArchive` (2013), as explicit ranges | — |
| 26 | `table_pencil_annotation` | range | `TSWP.PencilAnnotationArchive` (2016) | — |
| 27 | `table_tatechuyoko` | run | `TSWP.TateChuYokoFieldArchive` (10023) | — |
| 28 | `table_drop_cap_style` | paragraph | `TSWP.DropCapStyleArchive` (10024) | 208 |

The counts are over the thirteen generated fixtures, 389 storages. The eight
tables with no count were never seen: no bundled template has a footnote, a
tracked change, a comment, a bookmark or ruby text either — all 901 template
bundles were scanned and every one of fields 15, 16, 18, 20–23, 25 and 26 is
absent from all of them. They are named here from the schema, and this crate
handles them by shape rather than by having met one.

**A field outside this list, and outside the plain ones, is an error.** No
storage in the corpus or in any of the 901 bundles has one, and `iwork check`
says so if one turns up, because a table nobody knows about is a table a
remapping breaks in silence.

#### Four entry shapes

```
ObjectAttributeTable            1 repeated { 1 character_index, 2 → object }
StringAttributeTable            1 repeated { 1 character_index, 2 string }
ParaDataAttributeTable          1 repeated { 1 character_index, 2 first, 3 second }
OverlappingFieldAttributeTable  1 repeated { 1 TSP.Range{1 location, 2 length}, 2 → object }
```

The first three are keyed by a single index and are strictly increasing. Only
the last carries an explicit range, which is what lets two comment highlights
cover the same characters.

**An entry with no object is meaningful, not corrupt.** It terminates the run
before it — a hyperlink that stops short of the end of the text has one — or
asserts that there is deliberately nothing here (the drop-cap table's `{0}`), or
says that a paragraph takes whatever was in force, which is exactly what Pages
writes when it splits one. 177 of the 589 paragraph-style entries in the corpus
have no object.

> **Fields 5 and 8 are the other way round from what the order suggests.** The
> evidence is right here: the run indices of field 5 "are exactly the paragraph
> starts", as recorded below — a character-attribute table has no reason to sit
> on paragraph boundaries and field 8's does not. Confirmed independently by
> importing a document in which each paragraph varied one property: alignment
> and indents landed in the style referenced from field 5, bold and font size in
> the one from field 8.
>
> A paragraph table may also carry a final entry at the *end* of the text, which
> is where the style of a paragraph not yet typed comes from — 276 of the 389
> storages in the corpus have one.

Paragraphs are `\n` within a single storage; there is no per-paragraph object.
Each shape on the page owns its own storage — the Pages sample has 62 of them
for the body, headline, pull quotes, captions, chart labels and credits.

#### Where a paragraph ends

Five characters end one: `\n`, **`\r` `U+000D`**, **`U+000C`** (the page or
column break from the Insert menu), **`U+0005`** (a layout change mid-storage)
and **`U+0004`** (a section boundary). The paragraph table puts a run
immediately after any of them, so reading one as ordinary text splits the
paragraphs one character wrong — which is enough to make a paragraph style land
on the wrong words, and enough to make an edit remap onto them.

The list is not guesswork: with all five counted, **every paragraph-anchored
entry of every storage in all 901 bundled templates sits on a paragraph start**
(or at the end of the text). With `\r` uncounted, four storages in this
repository's own corpus failed; with `U+000C` uncounted, 40 Pages templates
did.

**`\r` is the separator the apps write most often.** AppleScript's `return` is a
carriage return, so every document built by script has them: `pages-styled`'s
body reads `Überschrift\rEin roter Absatz…` and its paragraph table holds
`[0, 12, 74, 128]` — the character after each `\r`. Reading `\r` as ordinary
text makes a four-paragraph storage look like one paragraph of 171 characters,
which is what this crate did until the paragraph tables of all 389 storages in
the corpus were checked against the paragraph starts. With `\r` counted, every
one of them is *exactly* the paragraph starts, plus in 276 cases a trailing
entry at the end of the text.

`U+0004` was found in a document Pages built from its "Project Proposal"
template, whose body storage reads `…123-4567\n\u{4}Company Name\n` at each of
its two section breaks, with a paragraph run on the `C`. It is the same
character that turns up *alone* as the whole of a body storage in a document
whose text lives in shapes — a section marker with nothing either side of it.

Two placeholder characters show up as the entire contents of a storage:
`U+FFFC OBJECT REPLACEMENT CHARACTER` stands in for an embedded drawable (it is
what every Numbers table storage contains), and `U+0004` — the section marker
above — appears alone in the Pages body storage of a document whose text all
lives in shapes.

### Editing text — what moves with it

Ten probes, each one Pages performing an edit on a fixture, saving, and the
storage archive diffed. `pages-styled`'s body is
`Überschrift\rEin roter Absatz…\rEin kursiver Absatz…\rEin ganz…` — 171 code
units with paragraph runs at 0, 12, 74 and 128 pointing at four different
styles. A second fixture was made by having Pages set a bold 22pt font over
characters 19–29, which gave it a character table of `[0 nil, 19 bold, 30 nil]`.

| # | Edit | Paragraph table before → after | Character table before → after |
|---:|---|---|---|
| 1 | delete `[5, 20)`, across the first break | `0, 12, 74, 128` → `0, 59, 113` | |
| 2 | insert 8 units at 21 | → `0, 12, 82, 136` | |
| 3 | delete `[12, 20)`, from a paragraph start | → `0, 12, 66, 120` | |
| 4 | delete `[12, 74)`, one paragraph whole | → `0, 12, 66` | |
| 5 | delete `[30, 90)`, spanning a break | → `0, 12, 68` | |
| 6 | insert `\rNEU` at 21 | → `0, 12, **22 nil**, 78, 132` | |
| 7 | delete `[19, 30)`, a run's whole extent | | `0, 19, 30` → **the field is removed** |
| 8 | delete `[15, 25)`, across a run's start | | → `0, 15, 20` |
| 9 | delete `[24, 40)`, across a run's end | | → `0, 19, 24` |
| 10 | replace `[18, 19)` with four units | | → `0, 22, 33` |

Three rules come out of that, and they are not the same rule:

**A run table behaves exactly like an attributed string.** A character that
survives keeps its attributes (8, 9); a run whose whole extent goes, goes with
it (7); and text arriving at a boundary joins the run *before* it (10 — the
run's start moved from 19 to 22 rather than staying to cover the new text). So
an index inside the removed range lands on the far side of whatever replaced it,
and two entries landing on the same index resolve in favour of the **later**
one. The only exception is the entry at index 0, which stays at 0: it is where
the first run begins, and text inserted at the very start of a storage has no
earlier run to take its attributes from.

**A paragraph table does not.** Told to delete a whole paragraph, break
included (4), Pages kept the *deleted* paragraph's style on the boundary and
dropped the style of the paragraph that moved up into it — red survived at 12
and italic vanished. Paragraph style is anchored to the paragraph start, not
carried by the characters, so an entry at the edit's own index stays put and a
collision resolves in favour of the **earlier** one. An entry that lands
anywhere that is no longer a paragraph start is dropped (1, 5).

**A new paragraph gets an entry with no object** (6). Pages wrote `{1: 22}` and
nothing else for the paragraph its insertion created — a nil attribute, meaning
"whatever was in force here". It did that only in the paragraph-style table,
which had an entry per paragraph already; the list-style, paragraph-data, bidi
and drop-cap tables of the same storage were sparse and it added nothing to
them.

Pages also **removes a run table that has stopped saying anything** (7): after
the only styled run was deleted the character table would have read "nothing,
from 0", and the field is simply not there in what Pages wrote.

> `Document::replace_text` reproduces all ten. Given the same fixture and the
> same edit it writes a **byte-identical `TSWP.StorageArchive`** in every one of
> the ten cases — which is a stronger statement than "the app opens it", and the
> only reason to believe the rules above rather than a plausible reading of them.

#### What cannot be remapped

An **attachment** entry names a single character, and that character *is* the
attachment: `U+FFFC`, with the drawable, footnote or number hanging off it.
Delete it and Pages deletes the object — told to delete a range covering the
photo's `U+FFFC` in `pages-report`, it removed the `TSD.ImageArchive`, its mask,
and their entries in the drawable list. That is a document-wide operation
touching the z-order and the media registry, so this crate refuses the edit by
name instead (`Error::AnchoredObject`) and leaves the document untouched.

A **section break** is the same case seen from one character away: a section's
entry sits on the character *after* its `U+0004` — `pages-report` has entries at
0 and 146, reading `…123-4567\n\u{4}Company Name`, with the entry on the `C` —
so what a delete destroys is the break, not the entry. Deleting one merges two
`TP.SectionArchive`s, and is refused for the same reason.

`U+FFFC`, `U+0004` and `U+0005` are equally refused as *input*: each stands for
an object rather than for itself, and a section break with no section behind it
is a document that claims a section it does not have.

#### Indices are UTF-16 code units

Everywhere: in the tables, in this crate's API, in the paragraph ranges. **Run
indices are character offsets, not byte offsets.** In a storage reading
`"Von Benjamin Keller\nVeröffentlicht am 07.09.2017\nim Magazin …"` the
character-attribute table holds `[0, 20, 49]`, which is exactly the paragraph
starts counted in characters; counted in UTF-8 bytes they would be
`[0, 20, 50]`. Three further storages agree. Those samples are entirely BMP, so
they cannot separate UTF-16 code units from Unicode scalars — UTF-16 is what the
text model uses, being NSString-backed, and an emoji therefore counts as two.

An edit may not land between the halves of a surrogate pair; the result would be
two unpaired surrogates, which is not a string. `Error::SplitSurrogate` says so.

### Smart fields and hyperlinks — `table_smartfield` (field 11)

Every smart field wraps a `TSWP.SmartFieldArchive`, which in 15.3.1 is one
field: `{1: uuid string}`. What is around it says which kind it is.

```
TSWP.HyperlinkFieldArchive        2032   1 → SmartFieldArchive, 2 string url
TSWP.PlaceholderSmartFieldArchive 2031   1 → SmartFieldArchive, 2 bool localizable
TSWP.DateTimeSmartFieldArchive    2034   format, locale, styles, TSP.Date
TSWP.MergeSmartFieldArchive       2036   1 → Placeholder, 2 contacts property, …
```

A link's target is field 2 of the 2032 archive and nothing else — one string.
The *text* is separate and does not follow it, which is how a link reading
"example.com" can point somewhere else, and is a shape the app writes happily.

Field 11 is a **run** table, and the runs come in two shapes. In
`46_Business_Modern_Invoice_PM`, one storage reading
`123-456-7890\nno_reply@example.com\nexample.com` carries

```
{0}                  no field: plain text
{13, → mailto:…}     the address, 13..32
{33}                 no field: the run ends at the newline
{34, → http://…}     the second link, 34.. and no terminator at all
```

so a run ends at the next entry, terminated or not, and a field running to the
end of the text simply has no entry after it.

**Nothing in this repository can author one.** All 901 bundled templates were
scanned: five 2032 objects, in three Numbers templates, and none at all in the
640 Pages templates or the 182 Keynote themes. None of the three apps'
scripting dictionaries has a link command — `sdef` over all three returns
nothing but `sourceURL`. Setting a Pages body text to a sentence containing a
URL and an e-mail address does not auto-link either. And **instantiating a
template strips the links**: all three of the templates that have them write the
document out with the text and without the smart fields. The fixture is
therefore Apple's bundle renamed, which Numbers opens and reads back.

### Lists — `table_para_data` (6) and `table_list_style` (7)

Two tables keyed on the same paragraph starts, and both sparse: a paragraph with
no entry keeps what the paragraph before it had.

* Field 6 is `{character_index, first, second}` and **`first` is the indent
  level**, counted from 0. Keynote's `60_Academic_Modern_PM` theme has a storage
  whose five paragraphs read "Body Level One" to "Body Level Five" with `first`
  0, 1, 2, 3, 4 — and `table_list_style` naming a style override at each of the
  same five indices. `second` is 0 everywhere in the corpus.
* Field 7 points at a `TSWP.ListStyleArchive` (2023), whose per-level arrays are
  parallel repeated fields: field 11 the label kind per level (0 none, 2 glyph,
  3 number), field 13 the indent in points per level, and **field 16 the literal
  bullet string per level** — `"-", "•", "-", "•", …` in Apple's "Note Taking"
  style, and no field 16 at all in the one named "None".

`pages-lists`, from the Real Estate Flyer template, has fourteen paragraphs
across three named list styles with two of them one level in. Changing a
paragraph's level is **not implemented**: it would mean writing field 6, and
nothing available here can make an app perform the edit to check the result
against — Pages' rich text carries `font`, `size` and `color` and no list
property at all, and the menu item needs a window. Read-only until a probe can
prove a write.

### Named style, or named style plus overrides

Most runs in a real document do not point at a named style. They point at a
**variation**: an anonymous archive with no `1.1`, a parent at `1.3.1`, the flag
at `1.4`, and a property bag holding only what differs. `pages-styled`'s four
paragraphs are one named style and three variations of it.

The distinction is what makes "I edited the Title style and nothing happened"
happen, and it is what an edit has to preserve, so it is worth reading directly:
`Document::style_of_run` walks from the run to the first ancestor with a name
and reports both — the object the run points at, the named style it descends
from, and the properties set in between. Only the variations' bags count. A
named style's bags carry every property it has, defaults included, so counting
those would report a paragraph as overriding sixty things when it overrides one.

`override_count` (field 10) is the archive's own claim about how many it
overrides. It is read and reported, never maintained: the Real Estate Flyer's
named styles all say 57.

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

Images, shapes, movies and everything else with a position have a section of
their own: see §6 Drawables and §7 Media.

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

**Every tile in the corpus carries field 6 = `5` and field 7 = `true`**, so the
published advice — refuse a tile whose `last_saved_in_BNC` is not `true` —
accepts all of them. (Phase 1 recorded the opposite, having read the
`TileStorage` message above the tile, whose fields 1–3 are the tile list, the
tile size and the wide-row flag. Corrected in Phase 2 and asserted by
`tests/cells.rs::every_tile_says_it_was_last_saved_by_the_current_storage_engine`.)
`TileRowInfo` field 5 says 5 as well.

**Field 3, `numCells`, is dead**: it is `0` on a tile holding 2411 of them.
Field 4 does count the `TileRowInfo`s. Field 8, `should_use_wide_rows`, is set
on two tiles of the pivot fixture and nowhere else.

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

### Writing one cell

Everything above is what a reader needs. What follows is what a *writer* needs,
and all of it was established the same way: by having Numbers 15.3.1 make the
edit first and looking at what moved.

**The rewrite is local, and that is the point.** Changing one value moves one
row's `cell_storage_buffer` and its `cell_offsets`, which moves one `TST.Tile`,
which moves one package entry. Nothing about the grid's shape changes, so
nothing that indexes rows by position — a category's group nodes — and nothing
that names them by UUID — a filter's hidden state — has anything to re-point.
`iwork set-cell` rewrites **one** of a Numbers document's 97 entries for a
number, two for a string, and five when a cell appears or disappears.

**The record is edited, never re-synthesised.** A cell keeps far more than its
value: the cell and text style keys, a control definition, a comment key, byte 7
(undecoded, `0x08` on currency), bytes 2–5, and — the one that is invisible
until someone looks — the **conditional style and rule keys** at flag bits
`0x80` and `0x100`, which are how a highlighted cell knows it is highlighted. A
writer that rebuilt the record from the parts it understands would drop them and
nothing would say so. The encoder here is the decoder's exact inverse: every one
of the 2515 records in the corpus re-encodes to the bytes it came from.

**Value and format travel together.** A cell that keeps its type keeps its
format key untouched. A cell that changes type gets the key another cell of the
new type already uses in the same table, and gives up its old slot's key —
which is what the app does: a text cell written over with a number came back
carrying `number_format` and no `text_format` at all. Byte 6 loses its claim
about the slot that went, and keeps every other bit.

**Both interned lists are refcounted, and an entry nobody points at is
removed.** Observed by moving a string between cells:

- Writing a string another cell already holds takes a **second reference to the
  entry that is there**, rather than adding one beside it.
- Releasing the last reference **removes the entry outright**. A key that was
  freed can be handed out again to a different string.
- New entries go at the smallest free key; `nextListID` (field 2) is a
  high-water mark that only rises, and a key below it may be absent. This crate
  takes the simpler half — allocate at the mark and raise it — which can collide
  with nothing.

The same holds for the FORMAT list, to the unit: one text cell becoming a number
moved entry 1 down by one and entry 2 up by one.

**A row and a column each count their stored cells.** `HeaderStorageBucket`
field 4 is that count, and the app keeps it exact: filling one empty cell moved
both, and **created the column's entry** where an all-empty column had none.
An emptied cell has no record at all — the app deletes it, exactly as a
merged-away cell has none.

**A stale cached value is a limitation, not a corruption.** A formula cell
carries the value the app last computed, and nothing here can recompute one:
after `set-cell B3 43`, `C3` still holds `84` for `=B3×2` in the file.
**Numbers recalculates on open** and reports `86`, so the document is correct in
the app — but a *reader* that trusts the cache is not, and that is why
`set-cell` refuses to write into a formula cell at all: taking a formula out
means editing `TSCE`, which nothing here does.

What `set_cell` refuses, by name rather than by writing something plausible: a
formula cell, a rich-text cell (its words are a `TSWP` storage), a cell covered
by a merge, a row with no stored cells, and any object carrying version patches.

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

## 6. Drawables — `TSD`

A drawable is anything placed on a page, a sheet or a slide: an image, a shape,
a text box, a line, a movie, a group, a table, a chart. `TSD` type ids live in
the **common** registry, so the same number means the same thing in all three
apps — one table serves Pages, Numbers and Keynote — and a shape made in
Keynote is the same archive as a shape imported into Pages. Confirmed against
the three type registries carved out of the 15.3.1 binaries, which agree entry
for entry across the 3000-block.

### Inheritance is nesting

`super` is field 1 and is a submessage, so a class hierarchy is literally
nesting on the wire. A Keynote title placeholder is four levels deep:

```
KN.PlaceholderArchive (7)
  1: TSWP.ShapeInfoArchive (2011)
    1: TSD.ShapeArchive (3004)
      1: TSD.DrawableArchive (3002)
        1: geometry
```

An image is one level (`TSD.ImageArchive` → `DrawableArchive`), a mask is one,
a table is one. **Nothing may assume a depth.** This crate walks field 1 until
it reaches a message whose own field 1 is a geometry — a message with a point,
a size, a small varint and a float and nothing else — and every read and write
goes through the path that walk returns. Across the corpus that path is one,
two or three levels; the four-level case is a Keynote placeholder.

### `TSD.GeometryArchive`

| Field | Wire | Contents |
|---|---|---|
| 1 | message | position: `{1: x, 2: y}` as two floats, points, y downward |
| 2 | message | size: `{1: width, 2: height}` |
| 3 | varint | flags |
| 4 | f32 | angle, **degrees counter-clockwise** about the centre |

Position is in the **parent's** coordinate space. Flags are 3 on an ordinary
object, 1 on a shape that sizes itself to its text, 0 on a zero-size one; the
individual bits are not decoded here and are carried through.

The angle is degrees and it is counter-clockwise: a line drawn from
(100, 600) to (500, 700) — down and to the right — is stored as a 412.31-point
geometry at **345.96°**, which is −14.04°, the angle that line makes.

### What the app calls an object's rectangle is not the geometry

Two corrections sit between the archive and what AppleScript reports, and both
were established by asking the app rather than by reading a schema.

**A masked image is reported as its mask.** `TSD.ImageArchive.mask` (field 5)
points at a `TSD.MaskArchive` (3006) whose `super.parent` is the image, so the
mask's geometry is in the *image's* coordinate space. Pages reports the photo in
`pages-report.pages` at 60 × 123, 475 × 383. The archive says:

```
image  33.86 × 66.28,  511.86 × 466.13
mask   25.89 × 56.52,  475.00 × 383.00     (parent: the image)
```

`33.86 + 25.89 = 59.75` and `66.28 + 56.52 = 122.80`, and the size is the
mask's. **The app's rectangle is `image.position + mask.position` by
`mask.size`.**

**Position is the corner of the rotated bounding box; size is not rotated.** A
220 × 180 shape stored at 100 × 100 and turned 30° is reported by Keynote at
**470 × 57**, still 220 × 180 — which is the centre (610, 190) minus half of
`(220·cos30 + 180·sin30) × (220·sin30 + 180·cos30)`. The line above, stored at
93.84 × 650, comes back at exactly 100 × 600 by the same arithmetic.

What no rule can correct for: a shape that **sizes itself to its text** stores a
height of 0 and flags 1, and its stored position is the centre of a box that
only exists once the text has been laid out. Keynote reports such a text box 58
points above the stored position and 115 points tall — half the height it
computed. `Geometry::fits_its_text` says when a rectangle is an anchor rather
than a box.

### Containment and z-order

Containment is written **twice**: downward from the container and upward as each
drawable's `parent`. The downward list is the one that carries z-order, back to
front.

| App | Archive | Field |
|---|---|---|
| Numbers | `TN.SheetArchive` (2) | 2 |
| Keynote | `KN.SlideArchive` (5) | 7 |
| Pages | `TP.FloatingDrawablesArchive` (10010) | 1 |
| Pages | `TP.SectionTemplateArchive` (10143) | 3 |
| Pages | `TP.DrawablesZOrderArchive` (10015) | 1 — depth only |
| any | `TSD.GroupArchive` (3008) | 2 |
| any | `TSWP.StorageArchive` (2001) | 9 — the attachment table |

Pages is the one that needs care. `TP.DrawablesZOrderArchive` lists **every**
drawable in one document-wide order — the body storage and the image anchored
inside it side by side — so it answers "how deep" and nothing about "where".
Reading it as the floating list reports an anchored image as floating.

An **anchored or inline** drawable in Pages is reached through the body
storage's attachment table: `{1: character index, 2: → TSWP.DrawableAttachmentArchive}`,
and the attachment's field 1 is the drawable. The report fixture's photo is
anchored at character 12 of storage 57940, and its `parent` is that storage.

A drawable named by no container list still has a `parent`, and that is how a
mask (parent: its image) and a Keynote slide-number placeholder (referenced from
`KN.SlideArchive` field 20, not field 7) are placed.

### Shapes, text boxes and lines

All three are `TSWP.ShapeInfoArchive` (2011) — `{1: TSD.ShapeArchive,
2: owned text storage, 4: the same storage again, 6: a flag}` — and the
difference between them is the path source, not the type. `TSD.ShapeArchive` is
`{1: DrawableArchive, 2: style, 3: PathSourceArchive, 6: stroke pattern offset}`.

`TSD.PathSourceArchive` is a tagged union: `{1: horizontal flip, 2: vertical
flip}` plus exactly one of `3` point (arrows, stars), `4` scalar (rounded
rectangle, polygon, chevron), `5` bezier, `6` callout, `7` connection line,
`8` editable bezier. Everything Keynote creates from a script is a **baked
bezier**: a rectangle is six elements and a line is two.

Six, not five, because **iWork writes a redundant `moveTo(0, 0)` after the
closing element**, and both public writers reproduce it deliberately. Every
rectangle in the corpus — shape paths, mask paths, image traced paths — has it.
A path without it is not what iWork writes.

### The object style — `TSD.ShapeStyleArchive` (3015) / `TSD.MediaStyleArchive` (3016)

Fill, stroke, opacity, shadow and reflection are not on the drawable. They are
on a style object it points at, usually a `TSWP.ShapeStyleArchive` (2025) which
wraps the `TSD` one at field 1 and adds its own text-inset properties at 10/11.

```
2025 { 1: 3015 { 1: TSS.StyleArchive, 10: override_count, 11: properties },
       10: override_count, 11: TSWP properties }
```

**The two property numberings differ, and the difference is silent:**

| | Shape (3015 field 11) | Media (3016 field 11) |
|---|--:|--:|
| fill | 1 | — |
| stroke | 2 | 1 |
| opacity | 3 | 2 |
| shadow | 4 | 3 |
| reflection | 5 | 4 |

A media style has no fill, so everything after it moves down one. Reading a
media style with a shape style's numbering reports its stroke as a fill.

**A property a style does not carry is inherited from its parent**
(`TSS.StyleArchive` field 3). Told to set a shape to 50% opacity with a 40%
reflection, Keynote wrote a **new variation style** carrying nothing but
`{11: {3: 0.5, 5: {1: 0.4}}}`, `is_variation: true` and a parent — and every
other property of that shape comes from the parent. Resolution therefore walks
the chain; `iwork drawables` prints the result.

Leaf shapes, as observed:

- **Fill** (`TSD.FillArchive`): a union of `1` colour, `2` gradient, `3` image
  fill. An **empty message is a fill that paints nothing**, which is not the
  same as an absent one.
- **Stroke**: `{1: colour, 2: width, 3: cap, 4: join, 5: miter limit,
  6: pattern}`. The pattern's `type` is `1` solid, `2` empty (no line at all),
  `0` patterned — and **dashes and dots are both type 0**, told apart by the
  first entry being below 1. `count` is not the number of floats: iWork writes
  six whatever it says.
- **Shadow**: every field has a non-zero default — angle 315, offset 5, radius
  1 (an *integer*), opacity 1, and `is_enabled` defaults to **true**, so an
  absent flag means the shadow is on.
- **Reflection**: `{1: opacity}`, defaulting to 0.5. Keynote wrote 0.4 for
  "reflection value 40".
- **Colour**: `{1: model, 3: r, 4: g, 5: b, 6: a, 12: colour space,
  13: headroom}`, channels 0–1. Always write `model`, `a` and the space: a
  colour serialised without alpha has been observed to crash Pages.

### Writing geometry

`iwork set-geometry` takes the rectangle the *app* reports and converts it back:
the rotated bounding box's corner becomes the stored corner, and a masked image
is addressed by its mask's window. Rotation, flags and every other field are
left exactly as they were.

Three things travel with a resize, all of them because the app moves them too:

1. **A media drawable's `originalSize`.** Keynote and Pages both rewrote field 4
   of an image to the new size when a script resized it.
2. **A shape's path source.** *A shape's size lives in two places.* Told to make
   a 200 × 200 shape 444 × 128, Keynote rewrote the geometry **and** the bezier
   path source — its natural size and all six corners. A document with only the
   geometry changed opens with the app still reporting 200 × 200, which is a
   silent wrong render, and nothing in the archive says so.
3. **A masked image's whole assembly.** Asked to make the 475-point-wide cropped
   photo 300 wide, Pages multiplied the picture's own size, the mask's offset,
   the mask's size and the mask path's natural size by 300/475, and moved the
   picture so that `image.position + mask.position` still landed on the frame's
   corner. This crate reproduces that transformation; against the document Pages
   itself wrote, the mask comes out **byte-identical** and the image differs in
   the last two ulps of one float.

**A mask is the exception to (2), and it is the app's exception.** Resizing the
mask changed its path's *natural size* and left every point of the path where it
was, while resizing the shape moved both. So masks get the natural size only.

Horizontal and vertical scale factors are taken separately, which reduces to
what was observed whenever the resize is proportional. Every image in the corpus
has `aspect_ratio_locked` set and the app will not perform a non-proportional
one, so that case is **Unverified**.

---

## 7. Media — `TSP.DataInfo` and the `Data/` directory

**TSD has no `bytes` fields at all.** Every blob is a `TSP.DataReference`
(`{1: identifier}`) into `TSP.PackageMetadata.datas`, a list of `TSP.DataInfo`
records, each naming a file under `Data/`.

| Field | Contents |
|---|---|
| 1 | identifier — what a `DataReference` names |
| 2 | **a raw 20-byte SHA-1 of the file's bytes** |
| 3 | `preferred_file_name` — the name the file had outside the document |
| 4 | `file_name` — the entry under `Data/`, `<stem>-<identifier>.<ext>` |
| 5 | path inside the app's theme bundle, for assets not copied in |
| 10 | attributes; `100` is `TSD.ImageDataAttributes`, whose field 1 is the pixel size |
| 18 | `materialized_length` — the file's byte length |

The digest was checked rather than assumed: `shasum Data/probe-9077.png` gives
`9b157e9e…`, and so does field 2. `tests/drawables.rs` checks every stored file
in the corpus.

Two things follow that a writer must know.

**A theme asset has no bytes in the document.** `file_name` is empty and field 5
names a path inside the app's own bundle — `ginger/02_theme/aa043252_750x683`.
The photo in the Pages report fixture is one: the package holds only its
thumbnail. There is nothing to replace in place.

**The registry is refcounted, exactly like a table's interned string list.**
Replacing an image in Keynote twice left the first replacement's `DataInfo`
*and* its `Data/` entry gone the moment nothing referred to them, and the next
one took a fresh identifier. An entry nobody uses is removed, not left.

`MessageInfo.data_references` — field 6 of the framing, packed varints — lists
the data ids the object's payload uses. Across the corpus it is exactly that
set, no extras and none missing, which makes it an invariant `iwork check`
enforces.

### The non-destructive edit state

**This is the caveat PLAN.md flagged, and it is real.** Between an image's
stored pixels and what is drawn sit:

| Where | What |
|---|---|
| `ImageArchive.mask` (5) → `MaskArchive` (3006) | the crop, or a mask shaped like something other than a rectangle |
| `ImageArchive.instantAlphaPath` (10) | the Instant Alpha knockout path |
| `ImageArchive.imageAdjustments` (14) | exposure, saturation, contrast, highlights, shadows, sharpness, denoise, temperature, tint, levels, gamma, enhance — `top_level` defaults to **1**, the rest to 0 |
| `ImageArchive.originalData` (13), `adjustedImageData` (15), `thumbnailAdjustedImageData` (16), `enhancedImageData` (17) | renderings **derived from the old pixels** |
| `ImageArchive.traced_path` (19) | an outline of the content |
| `ImageArchive.background_removed` (22) | |

None of it is in a file being swapped in and none of it can be recomputed from
one. Replacing the bytes underneath produces a document that opens, reports the
same geometry through AppleScript, passes every structural check — and renders
the wrong thing. `Document::replace_media` therefore **refuses by name**,
listing what it found.

What is *not* an objection: an **identity mask**, whose window is the whole
picture at 0, 0. That is what the app installs when it replaces an image itself,
and it hides nothing. Nor is a `traced_path` that is the plain rectangle of the
picture's natural size — every one in the corpus is — which is rewritten with
the new size, as the app rewrites it.

### What the app does when it replaces an image

Keynote's `set file name of image` is scriptable, so its own replacement could
be watched. Given a 32 × 24 picture for an image in an 8 × 8 frame it:

- allocated a **new** `DataInfo` and `Data/` entry, `probe-a-9084.png`, with the
  digest, the byte length and the pixel size of the new file;
- set `naturalSize` (9) to 32 × 24 and rewrote `traced_path` (19) as the
  rectangle of that size;
- set `flags` (7) to **2** — `was_media_replaced`; the other bit, 1, is
  `is_placeholder`, which is what a theme's picture carries;
- left the geometry and `originalSize` alone, and instead **scaled the picture
  to fill the old frame and cropped the overflow with a mask**: the image became
  10.67 × 8 at x − 1.33 behind an 8 × 8 mask offset by 1.33, so the frame the
  app reports never moved.

`replace_media` does all of that except the last: it swaps the bytes in place
under the same identifier, keeps the frame, and **says** when the new picture is
a different shape from the old one, because it will then be drawn stretched.
Fitting it is `set-geometry`'s job.

**What an app round trip can and cannot prove here.** It proves the document
opens, that the picture is still where it was, and that the app is content with
the registry entry. It cannot prove the pixels drawn are the new ones: nothing
on a locked screen can see what is rendered.

### The wider media model, read

| Thing | Where | State |
|---|---|---|
| Movies and audio | `TSD.MovieArchive` (3007): `movieData` 14, `posterImageData` 15, trim `3`/`4`, poster time 5, volume 7, `audioOnly` 9, `loop_option` 24 (6 is the deprecated integer), `movieRemoteURL` 17 | read; the only ones in the corpus are Keynote's live-video placeholders |
| Live video | `MovieArchive.is_live_video` (30) and the Keynote-only `KN.LiveVideoInfo` extension at field 100 | read and named; **never authored** (ground rule 8) |
| Galleries | `TSA.GalleryInfo`, extension **200** of `TSD.ImageArchive` | detected by the extension's presence |
| Web video | `TSA.WebVideoInfo`, extension **300** of `ImageArchive` | detected |
| 3D objects | `TSA.Object3DInfo`, extension **200** of `MovieArchive` | detected |
| Freehand drawings | `TSD.FreehandDrawingArchive`, extension **100** of `TSD.GroupArchive` | detected; stroke order is load-bearing for "Animate Drawing" and is carried, never rebuilt |
| Pencil annotations | `DrawableArchive.pencil_annotations` (9) → `TSD.PencilAnnotationArchive` (3086) → `TSD.PencilAnnotationStorageArchive` (**242**, not a 3000-block id) | counted per drawable |
| Equations | extensions 100–103 of `ImageArchive` | detected |

A proto2 extension is a plain high-numbered field on the wire, so the *host*
message is the only thing that says what it means: 100 on a movie is live video,
100 on a group is a freehand drawing, 100 on an image is an equation.

---

## 8. Pages structure — `TP`

The archives only Pages writes: the document mode, the sections, the headers
and footers, the page templates, the linked-text-box threads, the tables of
contents and the footnote settings. Everything below was decoded from documents
Pages 15.3.1 wrote — five fixtures built from its own templates — and checked
against the app wherever the app would say anything at all.

> **The published `TP` tables are twelve years old and this range moved.** Both
> mined baselines are the same iWork '13 snapshot. Since then "page master"
> became **section template**, six commands changed superclass, and several
> field numbers were reused with incompatible types. Seven ids in the whole
> 730-id registry carry a different message than the mined references claim and
> **all seven are `TP`**. Names here come from the descriptors carved out of the
> installed binaries (`reference/protos-15.3/pages/TPArchives.proto`).

### The two document modes

`TP.SettingsArchive.body` (field 1, default true). True is a word-processing
document; false is a **page layout** one, which has no body text and whose
pages are made of named page templates instead.

Two more signals say the same thing, and neither was assumed:

- **The app agrees.** `document body` is a read-only boolean in Pages'
  dictionary and it is this field. Checked over four fixtures.
- **A page-layout document is exactly one with a `TP.PageTemplateArchive`.**
  Over all 640 bundled Pages templates the 388 whose `body` is false are
  *precisely* the 388 carrying one, with no exception either way. A
  word-processing document has none.

**A page-layout document has no sections to the app.** `pages-layout` carries
two `TP.SectionArchive`s — three section templates each, thirty-six header and
footer storages between them — and Pages answers `count of sections` with 0. The `sections` element is word-processing only, the way the Document
inspector is; the archives are still there and are still what the headers hang
off.

Confirmed. `Document::structure()`, `iwork structure`,
`tests/pages.rs::a_page_layout_document_is_the_one_with_page_templates`.

### Paper and margins — `TP.DocumentArchive`

| # | Field | Note |
|--:|---|---|
| 30, 31 | `page_width`, `page_height` | PostScript points |
| 32–35 | left, right, top, bottom margin | |
| 36, 37 | `header_margin`, `footer_margin` | |
| 38 | `page_scale` | |
| 42 | `orientation` | **0 in every document seen**, landscape templates included |
| 43, 44 | `printer_id`, `paper_id` | the last printer used is persisted |
| 21 | `uses_single_header_footer` | |
| 39 | `lays_out_body_vertically` | |
| 47 | `flow_info_container` | the linked-text-box threads |
| 48 | `page_templates` | repeated; page layout only |
| 14 | `toc_styles` | repeated `TSWP.TOCSettingsArchive` |

**Orientation is the page size, not the flag.** Field 42 is 0 on the landscape
templates too, so Pages swaps width and height rather than setting a flag;
`PageSetup::portrait` compares the two. Facing pages is elsewhere again —
`TP.SettingsArchive.facing_pages` (34), set in 18 of the 640 templates, all of
them novel-shaped.

### Sections — `TP.SectionArchive` (10011)

A section is an entry in the **body storage's `table_section`** (field 17),
anchored like a paragraph: at the character *after* the `U+0004` that begins
it, never on the break itself. `TP.DocumentArchive.section` (5) exists in the
schema and is absent from every document in this corpus.

So a section's text is:

```
section i  =  [ start(i), start(i+1) − 1 )        the break belongs to neither
last       =  [ start(n), end of the text )
first      =    start 0, with no break in front of it
```

**Checked against the app, character for character.** Pages reports the three
sections of `pages-report` as 145, 923 and 432 characters; the entries are at
0, 146 and 1070 in a 1502-unit storage, and 146−0−1, 1070−146−1 and 1502−1070
are those three numbers. Four documents, every section, text compared and not
merely lengths.

Fields 1–16 are all `OBSOLETE_`. What 15.3.1 writes:

| # | Field | Observed |
|--:|---|---|
| 17 | `inherit_previous_header_footer` | 969 of 1048 sections in the bundled templates |
| 18 | `section_template_first_page_different` | 6 |
| 19 | `section_template_even_odd_pages_different` | 0 — **Unverified** |
| 20 | `section_start_kind` | 0 everywhere — the other values are **Unverified** |
| 21 | `section_page_number_kind` | 0 continue, **1 start at**; 7 sections use 1 |
| 22 | `section_page_number_start` | 1 normally, 2 in three sections |
| 23, 24, 25 | first / even / odd `TP.SectionTemplateArchive` | all three always present |
| 26 | `name` | "Blank", "Section", "Chapter …", "Cover" |
| 28 | `…first_page_hides_header_footer` | 120 sections |
| 29 | `user_defined_guide_storage` | |
| 30 | `background_fill` | 237 sections |
| 31 | `section_hyperlink_uuid` | what a link to a section points at |

Confirmed, except where the table says otherwise.

### Headers and footers — `TP.SectionTemplateArchive` (10143)

2013 called this a page master; the registry called it a page layout; it is a
**section template**, and each section has three of them — one for its first
page, one for even pages, one for odd.

```
TP.SectionTemplateArchive
  1  repeated headers   → three TSWP.StorageArchive, kind = 1
  2  repeated footers   → three more, also kind = 1
  3  repeated section_template_drawables
  4  page_template_uuidpath
```

**Always exactly three of each.** 3144 section templates across the 640
bundled templates and 66 across this corpus — 396 header and footer storages —
and never any other count, which is Pages' three header and three footer
fields. A *footer* storage is `kind = 1` as well; the
only thing that makes it a footer is being in field 2.

The zone order is **left, centre, right**, and that is *Inferred*: nothing in
the archive names them, and all three zones of a strip point at the same
paragraph style, so alignment does not say either. The evidence is a mirror
pair — `08_Journal_Newsletter` puts its date in header zone 2 and its page
number in footer zone 2, and `08_Newsletter_RTL`, the same design laid out
right to left, puts both in zone 0. Content that changes ends when the design
is mirrored is content addressed by side.

Match-previous is `TP.SectionArchive.inherit_previous_header_footer` (17) for a
section and `TP.PageTemplateArchive.headers_footers_match_previous_page` (4) —
the message's only *required* field — for a page template.

**A header's text is often not text.** The date in `pages-layout`'s header is a
`TSWP.DateTimeSmartFieldArchive` and the storage holds the string it last
rendered to; replacing the text removes the field and freezes the date. A page
number is a `U+FFFC` with an attachment behind it, and replacing the text
removes that too.

### Where a page number's format lives

Not on the section. `TP.SectionArchive` says whether numbering continues or
restarts and at what; what the number is *drawn as* is a
`TSWP.NumberAttachmentArchive` (2043) behind the `U+FFFC`, reached through the
storage's ordinary `table_attachment` (field 9):

```
TSWP.NumberAttachmentArchive
  1  super → TSWP.TextualAttachmentArchive
       1  string_equivalent
       2  kind    0 page number, 1 page count, 2 footnote mark
  2  number_format
  3  string_value
  4  number_format_name     "decimal"
```

So a document may number one section in roman and another in arabic with
neither section archive saying anything about it. Across the 640 bundled Pages
templates there are **129 of these and every one is kind 0, format 0,
`"decimal"`** — the field is Confirmed, the other kinds and formats are
Unverified.

### Page templates — `TP.PageTemplateArchive` (10017)

Page-layout documents only.

```
1  name        "Blank"
2  repeated section_template_drawables
3  repeated placeholder_drawables  {tag, drawable, z_index}
4  headers_footers_match_previous_page   (required)
5  hide_headers_footers
6  background_fill
7  guide_storage
```

### Linked text boxes — `TSWP.FlowInfoArchive` (2410)

`TP.DocumentArchive.flow_info_container` (47) → `TSWP.FlowInfoContainerArchive`
(2411) → a list of threads.

```
TSWP.FlowInfoArchive
  1  text_storage                 one storage for the whole thread
  2  repeated textboxes           the boxes, in flow order
  3  user_interface_identifier    the thread's number
```

**A thread is numbered, not named.** `user_interface_identifier` is its only
identity — the number the app shows as "Text Box 1". This is the case §Text's
"a storage is not one-to-one with a drawable" was written for: two
`TSWP.ShapeInfoArchive`s share one storage, and an edit to the text moves the
words between the boxes without either box changing.

Nineteen of the 640 bundled templates carry a thread; every one of the 640
carries the container, empty or not.

### Columns — `TSWP.ColumnStyleArchive` (2024)

Columns are **not a property of a section**. They are a column style reached
from the body storage's `table_layout_style` (field 12), which is anchored per
paragraph, so a document whose sections are set differently has several entries
in that one table and the ranges say where each applies. Only the body storage
carries the table; a text box in a thread has none.

```
ColumnStylePropertiesArchive
  6  columns_null            "deliberately no columns"
  7  columns → ColumnsArchive
       1  equal_columns      {count, gap}
       2  non_equal_columns  {first, [{gap, width} …]}
  9  margins    11  padding    5  vertical_alignment    12  writing_direction
```

**Widths and gaps are fractions of the text width, not points.** The single
non-equal layout in the whole install — `02_ResearchPaper_JP`, the only Pages
template with more than one column — reads `first 0.26090077`,
`gap 0.035152942`, `width 0.7039463`, which sum to exactly 1.0; its equal
two-column neighbour has `gap 0.03527747` on a page whose text is 515 points
wide, and three hundredths of a point would be no gap at all.

That template is also the only source: **639 of the 640 bundled templates are
one column.** It cannot be instantiated on this machine — the Japanese
templates are not in this locale's list — so the fixture is the bundle renamed.

### Tables of contents

Four archives, and the first thing to know is that there are **two settings
objects and they disagree**:

| Type | Message | What it is |
|---|---|---|
| 2051 | `TSWP.TOCSettingsArchive` | the style-inclusion map: `{toc_name, toc_scope, [{paragraph_style, toc_entry_style, show_in_toc}]}` |
| 2240 | `TSWP.TOCInfoArchive` | the placed list, which is a drawable: `{super, toc_settings, [toc_entry_data], [page_number_ranges], sync…}` |
| 2052 | `TSWP.TOCEntryInstanceArchive` | one line as last laid out: paragraph index, page number, number format, **heading text**, indexed style, list level |
| 2026 | `TSWP.TOCEntryStyleArchive` | a paragraph style plus `{page_number_style, show_page_number}` |

`TP.DocumentArchive.toc_styles` (14) holds the document's own settings, with
`toc_scope` 0; a placed list carries its own copy with `toc_scope` 1. In
`pages-toc` the document's names two paragraph styles and the placed list's
names six, of which two are included. Reading only one of them is reading the
wrong one.

A paragraph style also carries `show_in_toc` (33), `toc_style_id` (35) and
`show_in_toc_navigator` (44) of its own, which is the same decision written a
second time.

**Only two of the 640 bundled Pages templates have a contents list at all**
(`00C_Textbook_Portrait`, both variants), which is why there is one fixture and
not several.

### Footnotes and endnotes — nothing to decode

`TP.SettingsArchive` records the settings, and every Pages document has them:

| # | Field | Values |
|--:|---|---|
| 30 | `footnote_kind` | 0 footnotes, 1 document endnotes, 2 section endnotes |
| 31 | `footnote_format` | 0 numeric, 1 roman, 2 symbolic, 3 Japanese numeric, 4 Japanese ideographic, 5 Arabic numeric |
| 32 | `footnote_numbering` | 0 continuous, 1 restart each page, 2 restart each section |
| 33 | `footnote_gap` | points between the body and the notes |

**Every one of those is 0, 0, 0, 10 in every document reachable from here**, so
the settings are Confirmed as fields and every non-default value is
**Unverified**.

The containment is worse off than that: it is **Inferred from the schema and
nothing has ever decoded one**.

```
body storage
  16  table_footnote          character-anchored, entry on a U+FFFC
        → TSWP.FootnoteReferenceAttachmentArchive (2008)
             1  super (TSWP.TextualAttachmentArchive, kind = 2 footnote mark)
             2  contained_storage  → the note's own TSWP.StorageArchive, kind = 2
             3  custom_mark_string
```

There is **no storage of kind 2 anywhere**: not in this corpus, not in any of
the 901 templates the three apps ship, and no `table_footnote` entry either.
Neither AppleScript nor a template can author one — Pages' dictionary has no
footnote command — and no iWork-authored document from a real user is available
here. So this crate reads the shape above, reports whatever it finds, and never
fails; `tests/pages.rs::no_storage_in_the_corpus_is_a_footnote_body` is the
tripwire that says so out loud if a fixture ever grows one.

### Bookmarks — the same story

`table_bookmark` (field 15) → `TSWP.BookmarkFieldArchive` (2035), a run-anchored
table naming a range. **Not one of the 901 templates the three apps ship
carries a `TSWP.BookmarkFieldArchive`**, and neither does anything in this
corpus. The same sweep found **zero** `TSWP.FootnoteReferenceAttachmentArchive`s
and **zero** storages of kind 2, which is the footnote boundary above measured
the same way. A
bookmark is made by naming a range in the app's UI and nothing reachable here
can name a range. Read, reported, **Unverified**.

`TP.DocumentArchive` field 46
(`show_in_bookmarks_list_paragraph_styles_property_initialized`) and the
paragraph-style property `show_in_bookmarks_list` (43) are present and are the
*other* half — which headings appear in the bookmarks list — and are read.

### Deleting a section break

**Refused, and the refusal is the finding.**

Deleting the `U+0004` that begins a section merges two sections into one, and
which of the two keeps its three section templates, its eighteen header and
footer storages, its guide storage and its background fill is not something any
probe here could establish. Pages will not perform the edit for anyone to
watch:

- `delete section 2 of document 1` answers **-10000**, "AppleEvent handler
  failed";
- there is no `make new section` in the dictionary;
- the menu item that would do it needs a key window, which a locked screen does
  not provide;
- and `set body text of section 2 to ""` leaves the break exactly where it was,
  with a **zero-length section** behind it — so a section with no text is a
  legal state and is not a merge.

`Error::SectionBreak` names the break, the section and what is not known.
`iwork check` catches the damage after the fact: a section that does not begin
at 0 begins on the character after a `U+0004`, and that invariant is what found
the hole in the first place.

---

## 9. Formulas — `TSCE`

The calculation engine, which is the same in all three apps: Pages tables carry
formulas, Keynote's could, and the archives are identical because `TSCE` lives
in the shared registry. Everything below was decoded from documents Numbers
15.3.1 wrote and, wherever the app will say anything, checked against what the
app prints in its formula bar — `formula of cell`, which is the strongest
oracle in this repository because it hands back the *text*, not a structure.

The corpus for it is `numbers-formulas.numbers`, a zoo of **ninety-five
formulas** built by AppleScript: one per node type, operator, reference shape,
literal kind and naming rule, plus a table renamed after the formula pointing at
it was written and a column deleted after the formula pointing at it was
written. `scripts/applescript/numbers-formulas.applescript` is the whole list.

### Where a formula is

A cell holds a **key**, not a formula: the `0x200` payload of its record (§5) is
a `uint32` into the table's FORMULA `TST.TableDataList`, and the entry there
carries a `TSCE.FormulaArchive` at field 5.

Two consequences, and both are load-bearing:

* **The archive has no host cell.** `FormulaArchive` has `host_column` (2),
  `host_row` (3) and their two sign bools (4, 5), and 15.3.1 leaves all four out
  of every entry in this corpus. The host is the cell that holds the key, so
  relative references resolve against *the referring cell*.
* **One entry can serve many cells.** `=SEQUENCE(1,3)` spills into the two cells
  beside it and all three key the same formula; a filled-down column is the same
  trick. The refcount on the list entry is how many cells point at it, exactly
  as for strings and formats.

Formula archives are also found at `TST.FormulaPredicateArchive` field 7 and its
pre-pivot twin's field 1 (a filter or conditional-highlighting condition — §5),
in the merge owner's formula store (§5, "Merged cells"),
in `TSCE.TrackedReferenceArchive` (4005) inside the tracked-reference store, and
in `TN.ChartMediatorArchive` (12006), which is where a chart keeps its
references back into a table.

### The AST

`FormulaArchive.AST_node_array` (field 1) is an `ASTNodeArrayArchive`: a
repeated field 1 of `ASTNodeArchive`, in **post-order**. Evaluate the stream
onto a stack; each node pops its operands and pushes its result; one value is
left at the end.

That last sentence is also the test. A node array whose stream underflows its
own stack is not a node array, and this is how a walk of a whole document tells
a formula apart from the many other repeated-field-1 messages an iWork file is
full of — `TSCE.CellRecordExpandedArchive` is `{1: column, 2: row}`, which is a
legal *node* (`MULTIPLICATION_NODE` with `AST_function_node_index` 0) and a
nonsensical *program*. `Ast::is_well_formed` is that check, and `iwork check`
runs it over every formula in every table.

`ASTNodeArchive` has forty-six fields; the discriminator is `AST_node_type`
(field 1), and the full table with wire types is `NODE_FIELDS` in
`src/formula.rs`. The node types this corpus produces, with what the app writes
them for:

| # | Node | Written by | Payload |
|--:|---|---|---|
| 1–6 | `ADDITION`…`CONCATENATION` | `+ − × ÷ ^ &` | — (binary) |
| 7–12 | `GREATER_THAN`…`NOT_EQUAL_TO` | `> ≥ < ≤ = ≠` | — (binary) |
| 13 | `NEGATION` | unary `−` | — |
| 14 | `PLUS_SIGN` | unary `+` | — |
| 15 | `PERCENT` | postfix `%` | — |
| 16 | `FUNCTION` | any call | 2 index, 3 arity |
| 17 | `NUMBER` | a literal | 4 double, 42/43 decimal128 |
| 18 | `BOOLEAN` | `TRUE`, `FALSE` | 5 |
| 19 | `STRING` | `"…"` | 6, **unescaped** |
| 22 | `EMPTY_ARGUMENT` | `SUM(A1,,B1)` | — |
| 24 | `ARRAY` | `{1,2;3,4}` | 11 columns, 12 rows |
| 25 | `LIST` | `(…)` — parentheses | 13 arity |
| 32/33 | `APPEND`/`PREPEND_WHITESPACE` | spaces the user typed | 25 |
| 34/35 | `BEGIN`/`END_THUNK` | a lazily evaluated argument | — |
| 36 | `CELL_REFERENCE` | `A1`, `$B$2`, `B`, `2:2` | 26 column, 27 row, 28 table |
| 46 | `REFERENCE_ERROR_WITH_UIDS` | `#REF!` | 26/27 saturated, 38 uids |
| 52 | `LET_BIND` | `LET(x,…)` | 34 name, 36 continuation, 37 symbol |
| 53 | `VAR` | a bound variable | 37 symbol |
| 54 | `END_SCOPE` | closes one `LET` binding | — |
| 55 | `LAMBDA` | `LAMBDA(x,…)` | 45 idents |
| 56/57 | `BEGIN`/`END_LAMBDA_THUNK` | a lambda's body | — |
| 63 | `LINKED_CELL_REF` | the cell a highlighting rule styles | 28 table only |
| 66 | `CATEGORY_REF` | a pivot cell | 39 reference, 44 levels |
| 67 | `COLON_TRACT` | any range | 33 sticky bits, 40 tract |
| 70 | `SPILL_RANGE` | postfix `#` | — |

Forty node types and forty-eight function ids appear across the twenty-one
fixtures, in 907 node arrays and 1582 nodes. Types this crate names but has
never seen: `DATE`(20) and `DURATION`(21) literals, `TOKEN`(23), `THUNK`(26)
with an inline nested array, the two legacy reference nodes (27, 28),
`COLON`(29), `REFERENCE_ERROR`(30), `UNKNOWN_FUNCTION`(31), `COLON_NODE_WITH_UIDS`(45),
`UID_REFERENCE`(48), `LINKED_COLUMN`/`ROW_REF`(64, 65), `VIEW_TRACT_REF`(68) and
`INTERSECTION`(69). They are decoded by shape and are **Unverified**.

> **The trap: fields 35 and 36 changed type in place at 14.4.** Up to 13.1,
> field 35 was `AST_let_e2`, a nested `ASTNodeArrayArchive`, and field 36 was
> `AST_let_whitespace`, a nested `ASTLetNodeWhitespace`. From 14.4 they are
> `AST_let_whitespace` (a **string**) and `AST_let_is_continuation` (a **bool**).
> Field 35 keeps wire type LEN, so an old schema parses a whitespace string as a
> nested AST and reports nothing wrong; field 36 goes LEN → varint, so an old
> schema throws.
>
> **15.3.1 writes the new shape, and this corpus proves it.** `=LET(x,2,y,3,x×y)`
> produces two `LET_BIND_NODE`s: `{1: 52, 34: "x", 36: 0, 37: 1}` and
> `{1: 52, 34: "y", 36: 1, 37: 2}`. Field 36 is a varint on the wire and it
> carries the meaning the 14.4 name gives it — the second binding *continues*
> the first `LET` rather than opening a new one — because there are also two
> `END_SCOPE_NODE`s, one per binding, and only one `LET(…)` in the text. A
> decoder must gate on the document's version and never on the wire type found.

### Number literals are stored twice

`NUMBER_NODE` carries a binary double at field 4 **and** an IEEE-754 decimal128
at fields 42 (low) and 43 (high). The decimal is the authoritative one:

```
sign     = high >> 63
exponent = ((high >> 49) & 0x3fff) − 6176
mantissa = ((high & (2^49 − 1)) << 64) | low
value    = ±mantissa × 10^exponent
```

`high == 0x3040000000000000` is exponent 0 with an empty high mantissa, i.e. an
exact integer, and `low` *is* the integer — which is what keeps `1` from being
printed as `1.0`. `0.1` is `low = 1, high = (6176−1) << 49`, and rendering it
from the digits rather than from the double is what makes `=0.1+0.2` print as
`=0.1+0.2`. Formula text never uses exponent notation, so `1e−5` is `0.00001`.

A literal is never negative: a leading minus is a separate `NEGATION_NODE`.

### The reference model

A reference names a column and a row, each **absolutely or relatively**, and
either axis may be absent — which is what a whole-column or whole-row reference
is.

`CELL_REFERENCE_NODE` (36) carries `AST_column` (26) and `AST_row` (27), each an
`{1: index, 2: absolute}` pair. **The index is a zigzag `sint32`**: absolute
means the 0-based index, relative means a signed offset from the host cell.
Numbers writes `2: false` explicitly rather than relying on the default.

* `=B25` in `C25` → column `{1: 1, 2: 0}` = −1, row `{1: 0, 2: 0}` = 0.
* `=$B$2` → column `{1: 2, 2: 1}` = absolute 1, row `{1: 2, 2: 1}` = absolute 1.
* `=B$2`, `=$B2` → one flag each; the axes are independent.
* `=SUM(B)` → **field 27 absent**. A whole column.
* `=SUM(2:2)` → **field 26 absent**. A whole row.
* `=COUNT(B:B)` produces the same archive as `=SUM(B)`: one shape, two
  spellings, and the app prints one of them back.

`COLON_TRACT_NODE` (67) is every range. It carries `AST_sticky_bits` (33) —
four required bools, `begin_row`, `begin_column`, `end_row`, `end_column` — and
`AST_colon_tract` (40), which holds up to four lists: `relative_column` (1),
`relative_row` (2), `absolute_column` (3), `absolute_row` (4), each an entry of
`{1: range_begin, 2: range_end?}`.

* **The relative offsets are plain `int32` varints, not zigzag.** −1 is ten
  bytes. The adjacent `AST_column`/`AST_row` *are* zigzag. Mixing the two
  decodings is the silent-corruption bug of this schema.
* **An omitted `range_end` means `range_end == range_begin`**, not 0 and not
  unbounded.
* The sticky bits choose which list each end reads. `=SUM($B2:B$4)` writes
  sticky `{0,1,1,0}` and **both** lists on both axes; the begin column comes
  from `absolute_column[0]`, the end column from `relative_column[0]`.

The saturation sentinels differ by axis: a row saturates at `0x7fffffff` and a
column at `0x7fff`. A reference with both saturated is a stored `#REF!`, written
as `REFERENCE_ERROR_WITH_UIDS` (46) with an `AST_tract_list` (38) of the UUIDs
that used to be there. The fixture makes one by deleting a column after the
formula pointing at it was written.

### Cross-table references resolve by identity

A reference outside its own table carries `AST_cross_table_reference_extra_info`
(field 28), whose field 1 is a `TSP.CFUUIDArchive`. **No table name appears in
any formula anywhere in the file.**

The UUID is the target table's `base_owner_uid`, and reaching it is a two-step
walk that nothing else in the format needs:

```
TST.TableModelArchive.haunted_owner (84) → TSCE.HauntedOwnerArchive.owner_uid
  → the TSCE.FormulaOwnerDependenciesArchive (4008) whose formula_owner_uid is
    that UUID and whose owner_kind (3) is 35
    → its base_owner_uid (12)          ← this is what an AST writes
```

Matching on `haunted_owner.owner_uid` itself finds nothing: in this corpus the
base is the haunted UUID's lower half minus 35, because every owner a table has
is a numbered offset from one base — but the join is a lookup and never
arithmetic.

The `TSP.CFUUIDArchive` form splits the 128-bit value into four 32-bit words:
`lower = w0 | w1 << 32`, `upper = w2 | w3 << 32`, where `w0`…`w3` are fields
2–5. Checked word for word against the `base_owner_uid` of all nine tables of
the zoo.

**The proof that this is identity and not name.** The fixture writes
`=Alt::A1`, then renames the table `Alt` to `Neu`, then saves. Afterwards the
string `Alt` is nowhere in the document, the AST is byte-identical to what it
would have been, and both Numbers and this crate print `=Neu::A1`.

Cross-table references are **relative** like any other: `=Daten::B2` written in
`C35` stores column −1 and row −33 and resolves them against the host's
position, in the other table's coordinate space.

### Header names, which are not in the file either

Numbers prints references by **the text of header cells**, and works it out at
render time from the cells themselves. There is a document-wide cache of these
names — `TST.HeaderNameMgrArchive` (6366) and its `HeaderNameMgrTileArchive`
(6365) fragments, reached from `CalculationEngineArchive.header_name_manager`
(field 14) — but the names in it are copies of cell text.

The rules, all read off the zoo and the corpus:

1. **A column is named by the last header row; a row by the last header
   column.** `Doppelkopf` has two header rows and the app prints the second
   one's text.
2. **A cell reference uses names only when it has both**, and the cell must be
   a body cell — row ≥ header rows *and* column ≥ header columns. A reference
   into the header row prints `B1`; a reference into a table with a header row
   and no header column prints `C2`.
3. **A whole-column reference uses the column name alone**, a whole-row
   reference the row name alone.
4. **A range never uses names.** `=SUM(B2:B4)` stays in A1 notation in a table
   where every row and column is named.
5. **A name must name one thing.** `numbers-links.numbers` has eleven rows whose
   header cell all read `Item name`, and the app prints `=C2×D2` there.
6. **A name that is unique in the document needs no table prefix, across tables
   as well as within one.** `=Daten::B2` prints as `=Menge Schrauben`.
7. **Where two tables share a name, the first one keeps it bare.** `Menge` is a
   column header of both `Daten` and `Daten2`; the app prints `SUM(Menge)` for
   the first and `SUM(Daten2::Menge)` for the second. Which table counts as
   first is *Inferred*: this crate uses document order — tables sorted by object
   identifier, which is creation order — and one document is behind it.
8. **A `$` goes in front of the name of the axis it anchors**: `$Wert $addition`,
   `Wert $addition`, `$Wert addition`.
9. **A name is wrapped in single quotes when it contains an operator
   character**, and an embedded `'` is doubled. `A+B` → `'A+B'`,
   `Preis (netto)` → `'Preis (netto)'`, `it's` → `'it''s'`,
   `groesser-gleich` → `'groesser-gleich'`. A space does not force quoting
   (`x y`), and neither does a function's name (`SUM`). Proven characters:
   `+ - ( ) '`; the rest of the set in `NEEDS_QUOTING` is the grammar's other
   operators and is Inferred.

### The text a formula prints as

This crate's canonical output is **the app's**, and the comparison is character
for character. Two things that surprise:

* **The operators are Unicode.** `×` (U+00D7), `÷` (U+00F7), `−` (U+2212 for
  both subtraction and negation), `≥`, `≤`, `≠`. Emitting ASCII `*`, `/`, `-`,
  `>=`, `<=`, `<>` does not match the app.
* **Function names are not localised and the separator is a comma.** On a
  machine whose Numbers is running in German, the oracle reports `SUM`,
  `VLOOKUP`, `IFERROR`, `FIND.CASEINSENSITIVE` and `SUM(1,2,3,4,5)`. The only
  spellings Numbers localises in a formula are the operators, and those are the
  same everywhere too.

Whitespace the user typed is preserved as its own nodes:
`APPEND_WHITESPACE_NODE` appends to the value on top of the stack,
`PREPEND_WHITESPACE_NODE` prepends to it. `= B60 + 1` is a reference, an append,
a literal, a prepend, an addition and one more prepend, and it round-trips.

Parentheses are not implied by precedence — they are a `LIST_NODE` with one
argument, so `=(B11+1)×2` is stored with the grouping the user typed and this
crate never has to reason about precedence.

Three renderings are the crate's own rather than the app's, because the app has
no formula text for them:

| Node | Printed | Why |
|---|---|---|
| `LINKED_CELL_REF` (63) | `#CELL` | The subject of a conditional-highlighting rule: a node with no coordinates at all. No dictionary reports a conditional rule, so there is nothing to match. `=#CELL>0` reads the way the rule reads. |
| `LINKED_COLUMN_REF` / `LINKED_ROW_REF` | `#COLUMN`, `#ROW` | Same, unexercised. |
| `CATEGORY_REF` (66) | `#CATEGORY!` | See below. |

And one function id has no name anywhere: **337**, the internal function behind
a spilled cell, which **Numbers itself prints as `(null)`** — so this crate
prints `(null)` too and matches the app exactly. **175** is the same kind of
hole and appears only inside `TN.ChartMediatorArchive`, wrapping each of a
chart's operands; it has no name to give it either.

### What is not printed the way the app prints it

A pivot cell's formula is a `CATEGORY_REF_NODE` (66) carrying a
`TSCE.CategoryReferenceArchive`: a group-by owner UUID, the column being
summarised, an aggregate code, a group level and a path of group UUIDs down the
category tree. Numbers spells one as

```
Table 1 as Pivot Source Table::$Units $January::Electric::Bicycles (Sum)
```

which needs the source table's group tree, the names of its groups, the
aggregate's name (Apple publishes none; only `2 = Sum` is proven — §5) and the
rule for where the `$` markers go. The 32 formulas of `numbers-pivot.numbers`
are the only ones in this corpus. The archive is decoded to
`formula::CategoryReference` and the text is `#CATEGORY!`; the oracle test
counts these separately and names them rather than skipping them.

A second thing about pivots was found while measuring that gap and belongs
here: **a pivot's stored table is its base, and the app shows a larger view.**
`Sales Pivot` is 7×5 on disk and 10×6 to Numbers — the grand-total row and the
grand-total column exist only in the view, and seventeen of the app's
thirty-two formulas for that table sit at positions with no cell record at all.
`TSCE.CoordMapperArchive` is the base→view mapping; nothing here reads it.

Everywhere else the agreement is exact: **273 of 273 formulas outside a pivot
match the app character for character**, across `numbers-values`,
`numbers-formulas`, `numbers-large`, `numbers-links`, `numbers-rules` and
`numbers-sorted`.

### The calculation engine object

`TSCE.CalculationEngineArchive` (4000) is one per document and **every document
has one**, Pages and Keynote included, with the empty
`FormulaOwnerDependenciesArchive` and `NamedReferenceManagerArchive` that go
with it. Its `dependency_tracker` (2) holds the owner-id map and the list of
per-owner dependency archives; `saved_locale_identifier` is field **16** in
modern files (field 5 is a 4.2-era compatibility value and reading it gives the
wrong locale).

Dependency edges — `TSCE.CellRecordTileArchive` (4009),
`RangePrecedentsTileArchive` (4010), the packed `EdgesArchive` words — are
recalculation metadata. This crate carries them through untouched and does not
decode them; nothing about formula *text* needs them.

**There are no named ranges.** No `TSCE` archive stores one and no Numbers
feature makes one: a "name" is a header cell's text, resolved at render time as
above. The `NamedReferenceManagerArchive` (4003) and the tracked-reference store
(4004) hold ASTs for the references the engine is *tracking*, keyed by formula
id — in this corpus they are the header-cell references, one per named row and
column.

## 10. Charts — `TSCH`

Charts are cross-app: `TSCH` type ids live in the shared registry, so a chart
made in Numbers, one dropped into a Pages report and one built by Keynote's
`add chart` are the same archives. What differs between the apps is not the
chart — it is whether anything stands behind it.

The corpus is 33 charts in four documents: **18** in `keynote-charts.key` (the
zoo, below), **12** in `numbers-charts.numbers` (Apple's `21_Simple_Charts`
template, eleven distinct types in one document), **2** in `numbers-rules` and
**1** in `pages-numbering`. Beyond that, all 901 bundled templates were scanned:
**69 have a chart**, 94 charts between them, and every claim below that says
"all" was checked against those too.

### The sandwich, and the message with no type id

A chart on the canvas is a `TSCH.ChartDrawableArchive` (**5021**) and it has
exactly two fields:

```
5021 TSCH.ChartDrawableArchive
├─ 1      TSD.DrawableArchive          geometry, parent, locked, title, caption
└─ 10000  TSCH.ChartArchive            the entire chart model
```

**`TSCH.ChartArchive` has no type id of its own.** It is a proto2 *extension* of
the drawable archive, declared inside the chart message itself, and the only
thing that says the bytes at field 10000 are a chart is that the shell is a
5021. A decoder driven by the registry alone sees `{1: bytes, 10000: bytes}` and
stops. All 33 chart drawables in the corpus have exactly those two fields and
nothing else — asserted, because an extension appearing at some other number is
how this would first go wrong.

The ten style archives **5022–5031** are the same shape: `{1: TSS.StyleArchive,
10000: TSCH.Generated.<matching>Archive}`, and the payload's schema is decided
by the *shell's* type id. That shape — `{1: varint-ish message, 10000: message}`
repeated — is also what fooled Phase 5's first AST walk; wire types plus the
stack check separate them.

`TSCH.ChartArchive`'s fields, all observed:

| # | Name | Notes |
|---|---|---|
| 1 | `chart_type` | `TSCH.ChartType`; absent means 0, `undefinedChartType` |
| 2 | `scatter_format` | 1 separate X, 2 shared X. Written on **every** chart; meaningful only on scatter and bubble |
| 3 | `legend_frame` | `{1: TSP.Point, 2: TSP.Size}` in chart-local coordinates |
| 4 | `preset` | → `TSCH.ChartStylePreset` (5020) in the theme |
| 5 | `series_direction` | 1 by row, 2 by column — **the only thing** that says which axis of the grid is a series |
| 6 | `contains_default_data` | true while the chart still shows Apple's placeholder numbers |
| 7 | `grid` | `TSCH.ChartGridArchive`, **inline**, not a reference |
| 8 | `mediator` | → `TN.ChartMediatorArchive` (12006). **Numbers only** |
| 9–16 | style slots | chart, legend and axis styles and non-styles |
| 17 | `series_theme_styles` | → 5028, one per theme series slot; six everywhere here |
| 18, 19 | `series_private_styles`, `series_non_styles` | `TSP.SparseReferenceArray` |
| 20 | `paragraph_styles` | the chart's own style table; every `…paragraphstyleindex` property indexes **this** |
| 21 | `multidataset_index` | which data set an interactive chart is showing |
| 22 | `needs_calc_engine_deferred_import_action` | 0 everywhere |
| 24 | `is_dirty` | |

Above 10000 sit the capability flags, and they are forward-compatibility
tripwires rather than settings: `supports_rounded_corners` (10026),
`supports_series_value_label_spacing` (10027),
`supports_series_error_bar_spacing` (10028), `supports_stacked_summary_labels`
(10029), `scene3d_settings_constant_depth` (10002) and `reference_lines`
(10005) are on every chart 15.3.1 writes. An older app that does not understand
one refuses to round-trip the document, so they are read, named and carried
verbatim; nothing here synthesises one.

### The grid — the chart's private copy of its data

`TSCH.ChartGridArchive` is written inline at field 7:

```
1  repeated string  row_name
2  repeated string  column_name
3  repeated GridRow grid_row          GridRow { 1: repeated GridValue value }
4  ChartGridRowColumnIdMap idMap      { 1: row entries, 2: column entries }
                                      Entry { 1: required string uuid, 2: required uint32 index }
```

`GridValue` is a union of four doubles decided by **which field is present**:
1 `numeric_value`, 2 `date_value_1_0` (the iWork-1.0 slot; only a document
written by Keynote 6 uses it), 3 `duration_value` in seconds, 4 `date_value` in
seconds from 2001-01-01 UTC.

**A blank cell is a present, zero-length `GridValue`** — the two bytes
`0A 00` inside a `GridRow`. This is the trap the domain is known for and it bit
here in a way worth recording: this crate's `decode_nested` deliberately refuses
empty bytes, so the first version of the grid reader *filtered the blank out* —
which does not merely lose a blank, it shifts every value after it one column to
the left. Empty is handled before the decode now, on both levels, and a unit
test builds a row of three cells with the middle one blank.

**The grid is stored rows first, always.** Whether a row or a column is a series
is `series_direction` and nothing else. In the corpus: 88 charts by row, 6 by
column.

Every grid in the corpus is rectangular, has one row name per row and one column
name per column, and its `idMap` indices are a permutation of the positions —
all four are `iwork check` invariants.

### The other copy: `TN.ChartMediatorArchive` and function 175

A **Numbers** chart is bound to its tables:

```
12006 TN.ChartMediatorArchive
├─ 1  TSCH.ChartMediatorArchive     { 1: → the chart, 2: local_series_indexes, 3: remote_series_indexes }
├─ 2  string entity_id              the mediator's identity to the calculation engine
├─ 3  TN.ChartMediatorFormulaStorage
│     ├─ 1  repeated TSCE.FormulaArchive  data_formulae      one per series
│     ├─ 3  repeated …                    row_label_formulae
│     ├─ 4  repeated …                    col_label_formulae
│     ├─ 5  int32                         direction
│     └─ 6–9                              error-bar formulas (none in this corpus)
└─ 4  bool columns_are_series
```

**Every one of those formulas ends in a `FUNCTION_NODE` of index 175**, which
Apple publishes no name for — one of the two holes in the function table, the
other being 337, the spill function Numbers itself prints as `(null)`. 915
references across the 69 chart-bearing bundles, and not one that is not wrapped.

It is **not** a one-argument wrapper, which is the correction this section
exists to make. Its arity is nought, one or three in the templates: a series fed
by three disjoint cells is `175(B7, D7, F7)` and a label list that names nothing
is `175()`. So the node is dropped and its *operands* are printed — one per
value the fragment leaves on the stack. A mediator formula is also not always a
reference: a bubble chart's row labels are string **literals** written into the
mediator, and a `#REF!` survives there like anywhere else.

References are printed with Phase 5's printer, against the table the AST names
by `base_owner_uid` — so `Fundraiser Results by Salesperson!Units Sold Andy` and
`Comparison of Units Sold by Year!B2:D2`. Printing them against no table at all
gives `Table::A2:Table::A10` for a range, which is why the target is resolved
first.

### Private copy versus live references — which is which

| | Pages | Keynote | Numbers |
|---|---|---|---|
| `ChartArchive.grid` | yes | yes | yes |
| `ChartArchive.mediator` | **no** | **no** | yes |
| what the chart draws | the grid | the grid | the grid |
| what the chart follows | nothing | nothing | the mediator's formulas |

The grid is what is drawn in all three. In Numbers it is a **cache** of what the
mediator's formulas last evaluated to; in Pages and Keynote it is the data
itself, and there is nothing else. A chart pasted from Numbers into Pages keeps
its numbers and loses its link, which is what the absent field 8 on the Pages
fixture's chart says. `iwork charts` prints both and labels them: the grid as a
small table, the references as `fed by …`, and `private data only — no mediator,
nothing to follow` where there is no second half.

Nothing in this crate recalculates a grid from its references, so a Numbers
chart whose table changed since the app last saved shows the cached numbers —
the same limitation, and the same honesty, as a formula cell's cached result
(§9).

### Chart types

`TSCH.ChartType` has 28 values in 15.3.1. **23 of them appear in this corpus**:
1–9, 11–20, 22, 25 and 27. Never seen: 0 (`undefinedChartType`), 10
(`mixedChartType2D` — one exists, in `28_GradeBook`, and no fixture takes it),
21, 23, 24 (the multi-data bar, scatter and bubble charts) and 26
(`donutChartType3D`).

The eight 3-D families are 12–19 plus 26. The four **interactive** families —
what Apple's chart picker calls the Interactive tab — are 20, 21, 23 and 24; the
corpus has one, a `multiDataColumnChartType2D` in `21_Simple_Charts`.

**Keynote's `add chart` is the only chart-creating command in any of the three
dictionaries**, and its `type` parameter is a *legacy* seventeen-value
enumeration that predates this one. The mapping was read off the documents the
fixture script writes:

| AppleScript | `ChartType` | | AppleScript | `ChartType` |
|---|--:|---|---|--:|
| `pie_2d` | 5 | | `stacked_horizontal_bar_3d` | 18 |
| `vertical_bar_2d` | **1** (column) | | `area_2d` | 4 |
| `stacked_vertical_bar_2d` | 6 | | `stacked_area_2d` | 8 |
| `horizontal_bar_2d` | **2** (bar) | | `line_2d` | 3 |
| `stacked_horizontal_bar_2d` | 7 | | `line_3d` | 14 |
| `pie_3d` | 16 | | `area_3d` | 15 |
| `vertical_bar_3d` | 12 | | `stacked_area_3d` | 19 |
| `stacked_vertical_bar_3d` | 17 | | `scatterplot_2d` | 9 |
| `horizontal_bar_3d` | 13 | | | |

"Vertical bar" is a column chart and "horizontal bar" is a bar chart — the one
place the two vocabularies disagree about a word.

### Series styles: sparse, and a three-level fallback

`series_private_styles` (18) and `series_non_styles` (19) are
`TSP.SparseReferenceArray`:

```
1  required uint32 count      the LOGICAL length — the number of series
2  repeated Entry entries     Entry { 1: index, 2: TSP.Reference }
```

**`count` is not `entries.len()`.** A series with no override simply has no
entry, so sizing a series vector from the entries drops every default-styled
series and mis-aligns every index after the first gap. In this corpus the two
are always equal — 66 arrays, no gaps, because Numbers writes an override for
every series — so the rule is asserted from the other side: every index is below
the count, the count is never smaller than the entry count, and the dense form
is `count` long. `iwork check` refuses an entry at or past the count.

Looking a style up for series *i* is a fallback, not a lookup:
`series_private_styles[i]` → `series_theme_styles[i mod len]` → the preset's
`series_styles`. And *within* a series style the property key depends on the
chart family (`tschchartseriesbarfill` for a bar chart,
`tschchartseriesdefaultfill` otherwise). This crate enumerates the overrides and
does not resolve them: nothing here draws a chart, and a fill it reported
without the family rule would be the wrong colour.

### Interactive charts

`multidataset_index` (21) is where an interactive chart's current data set is
kept, and it survives a save: the one in `numbers-charts` reads 0. The schema
offers a second home — `TSCH.ChartUIState`, reached from
`TN.UIStateArchive` field 23, with an `upgraded_to_ui_state` flag (extension
10021) that says the value moved there. **Neither is present**: the document's
view state has no field 23 and no chart carries 10021. So in 15.3.1 the index is
model state, not view state.

The control style (slider-with-buttons versus buttons-only) is a property of the
chart's non-style archive and is **not decoded** — see below.

### 3-D charts

Read-level: the type says a chart is 3-D, and eight of the corpus's charts are.
The scene itself — depth, lighting preset, bevel, rotation angles — lives in
`TSCH.Chart3D*` messages inside the *style* archives' extension 10000, and
`TSD.FillArchive` gains a `fill3d` at extension **100**, which is one of the
places a decoder that assumes "TSCH extensions start at 10000" loses data.
`scene3d_settings_constant_depth` (10002) is the one 3-D field on the chart
archive itself and is present on every chart, 3-D or not. None of the scene
payloads are decoded here; they round-trip byte for byte.

### A chart can carry a version patch, and that corrects §3

§3 records that 15.3.1 writes exactly one patched object per Numbers document —
the `TN.UIStateArchive` — and none in Pages or Keynote. Charts are the
counter-example. **A chart of a type that did not exist in an older release
carries a down-level `type == 0` patch**, and unlike the view state's it uses
`diff_field_path`:

| | donut | radar |
|---|---|---|
| base `MessageInfo.version` | `[2, 0, 25]` | `[2, 0, 25]` |
| `base_message_index` (7) | 0 | 0 |
| `diff_merge_version` (8) | `[2, 3, ∞]` | `[11, 1, ∞]` |
| `diff_field_path` (9) | `{1: [10000]}` | `{1: [10000]}` |
| `diff_read_version` (11) | `[2, 0, 25]` | `[2, 0, 25]` |
| payload | `{1: 5}` — pie | `{1: 2}` — bar |

The path `[10000]` reaches into `TSCH.ChartArchive`, and the whole patch is one
field: `chart_type`. Donut arrived in 10.2 and radar in 11.2, so what the patch
does is tell an older Numbers to draw a pie or a bar. No other chart in the
corpus has one, and no chart archive is ever rewritten here — the standing rule
that an object with patches must not have its first message rewritten applies
unchanged, because the patch would then describe a chart that is no longer
there.

### The oracle, and why it had to be built backwards

**No app will say what a chart contains.** `chart` is an element of a Keynote
slide, a Numbers sheet and a Pages document, and in all three dictionaries the
class is `<class name="chart" inherits="iWork item">` with **no properties of
its own** — position, size, rotation and opacity, and nothing about type, data
or series. There is no read-back.

So the oracle is the *input*. `keynote-charts.key` is eighteen charts built by
`add chart` from numbers chosen so that no two charts share one: chart *i* holds
`i×100 + 1, 2, 3` and `i×100 + 11, 12, 13`, with row names `Reihe A`/`Reihe B`
and columns `Q1 Q2 Q3`. `tests/charts.rs` asserts every one of those 108 values,
both row names and all three column names, for all seventeen types — plus the
by-column chart, where `Jan` is a series of `7001, 7011` rather than a category.
A decoder that read the grid transposed, or off by a row, or that dropped a
blank, cannot pass.

Three AppleScript findings came out of building it, each of which cost a run:

* the chart-type constants only resolve **inside** the `tell application` block;
* every slide must exist before any chart goes on, because interleaving `make
  new slide` with `add chart` made Keynote lose the document reference
  (-1728) halfway through;
* **`add chart` ignores the slide it is given.** Its direct parameter is
  documented as "the slide to add the chart to" and is not used: every chart
  lands on the document's *current slide*, so the first version of the fixture
  had eighteen charts stacked on one. Setting `current slide of doc` before each
  call is what places them.

`missing value` in the data list is refused with -1700, so **no fixture has a
blank grid cell**; the empty `GridValue` is exercised by a unit test built from
bytes, and its behaviour in a real document is Inferred.

### What is not decoded

* **Every `TSCH.Generated.*` property archive.** The six presets, and the axis,
  legend and series styles under them, are enumerated, counted and carried; not
  one property inside them is read. Titles, axis labels, number formats, error
  bars, trendlines, gap widths, corner radii, the interactive control style and
  the whole 3-D scene are all in there. This is the single largest thing this
  section does not do.
* **`TSCH.PreUFF.*` (5000–5017)** — the iWork '09/'13 chart model, still in the
  registry and still emitted for imported documents. Nothing in the corpus and
  nothing in the 901 bundles has one; the legacy grid's `repeated double` rows,
  which cannot express a blank at all, are written down from the schema and
  never decoded. A tripwire test asserts the absence.
* **Reference lines.** Extension 10005 is on every chart and holds an empty
  `ChartReferenceLinesArchive`; 5030 (the style) is in every document as part of
  the theme and **5031 (the non-style) is in none of the 901 bundles**, because
  no template has a reference line.
* **The chart's title and caption storages.** They are `TSD.DrawableArchive`
  fields 10 and 11 like any drawable's, reachable through `iwork drawables`, and
  are not surfaced on the chart.
* **`local_series_indexes` / `remote_series_indexes`.** Read and reported; what
  they mean when they disagree is unexercised — `numbers-rules` has a chart
  whose local index is `0xFFFFFFFF`.

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
5. **Remap attribute tables whenever the text changes**, and remap them
   according to what each one is anchored to. Clamping every index into the new
   length keeps the tables well formed and moves every style, hyperlink,
   anchored image and comment anchor onto the wrong characters. §Text has the
   three rules and the probes behind them.
6. **Keep run tables strictly increasing**, and free of entries that repeat the
   entry before them — an entry that draws no boundary is not what iWork writes,
   and editing accumulates them. A paragraph-anchored entry must additionally
   sit at a paragraph start, or at the end of the text; Keynote's own parser is
   documented as rendering a text box 2^16 points tall and then crashing on one
   that does not.
7. **Never leave a dangling reference.** Removing a style means removing every
   reference to it: the runs that use it, the stylesheet entries that list it,
   *and* its place in its parent's family entry.
8. **Declare every reference that leaves its component** in the referring
   component's `external_references`. An undeclared one does not always crash —
   it can simply make the edit invisible, which is worse.
9. **Previews go stale** — they are not regenerated by anything but iWork.
10. **Never rewrite an object that carries version patches** (§3). The patches
    are older encodings of what you just replaced, and leaving them makes the
    document say two different things depending on the app that opens it. In
    15.3.1's output that is one object per Numbers document and none anywhere
    else, so the cost of refusing outright is nil.
11. **Edit a record, do not rebuild one.** Every fixed-layout structure in this
    format — the cell record above all — carries fields nobody has decoded, and
    a rewrite that emits only the known ones drops them silently. Carry the
    bytes you did not change.
12. **Refcounted side tables are refcounted both ways.** Taking a reference to
    an interned entry raises its count; giving up the last one removes the
    entry. A writer that only ever adds leaves a list that grows on every save
    and disagrees with itself. The media registry is one of these.
13. **A `DataInfo`'s digest is a SHA-1 of the bytes it names**, and its
    `materialized_length` is their length. Writing new bytes under an old digest
    is accepted at open time and is a lie afterwards.
14. **A size that appears twice must be changed twice.** A shape's size is in
    its geometry *and* its path source; a media object's is in its geometry
    *and* its `originalSize`; a picture's is in its `naturalSize`, its traced
    outline *and* its `DataInfo`'s image attributes. The app maintains all of
    them, and a document with one of them changed opens and renders as though
    nothing had been done.
15. **Refuse an edit whose consequences are outside the file you were given.**
    Replacing an image's bytes cannot recompute the crop, the mask, the Instant
    Alpha path or the adjustments that were derived from the old pixels.
    Deleting the character an image is anchored to cannot remove the image from
    the drawable list, the z-order and the media registry. The only honest
    options are to refuse or to do the whole job; this crate refuses, by name.
16. **Do not merge two sections by deleting the break between them.** The
    `U+0004` is what makes the section; deleting it leaves two
    `TP.SectionArchive`s where one boundary is needed, and which of the two
    keeps its three section templates, its eighteen header and footer storages,
    its guides and its background is a question Pages will not answer for
    anyone —
    it refuses the edit from a script. §8 has the four ways that was checked.
17. **Never write a formula.** Reading one is settled (§9); writing one is
    not, and the gap is not the AST. A written formula has to be *evaluated*
    before it is saved, because the cell caches the result and the app trusts
    that cache until it recalculates; it has to be registered in the
    calculation engine's dependency graph, whose edge encoding nobody has
    decoded; and a reference into another table has to be written as that
    table's `base_owner_uid` and tracked. `set_cell` refuses a formula cell by
    name, and that refusal stands.
18. **A field you cannot place is not a field to skip.** A storage's attribute
    tables are told apart by field number and by nothing else, and an
    unrecognised one is far more likely to be a table than not. Refusing the
    edit is the safe answer; carrying it through unchanged while every other
    table moves is the unsafe one.
