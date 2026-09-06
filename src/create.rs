//! Documents from nothing.
//!
//! [`crate::Document::from_template`] answers "create a document" by copying one
//! that works, which is the rule the rest of this crate lives by — and which
//! needs Apple's software installed to have something to copy. This module owes
//! the other half of the README's promise: a caller with this crate and nothing
//! else gets [`crate::Document::new`], and the bytes come from here.
//!
//! # Why this is allowed to synthesise
//!
//! FORMAT.md's rule is *copy, don't synthesise*, and it is there because
//! inventing a message out of a schema crashed Pages every time it was tried.
//! Nothing below repeals it. What is written here was instead **measured**: a
//! document each app made was reduced — one field, one object, one stream
//! deleted at a time, the app asked after every deletion whether it still opened
//! the result and still read the text back — until nothing more could go. Every
//! object this module writes is one the app was watched insisting on, and
//! everything the app was watched not needing is not written. See FORMAT.md
//! §"The least document each app will open".
//!
//! # The shape of a document
//!
//! Four things, in this order:
//!
//! 1. **Objects**, each an `ArchiveInfo` and its payload, grouped into
//!    **components** — one `Index/<name>.iwa` stream apiece.
//! 2. **The component index**, `Index/Metadata.iwa`: a `TSP.PackageMetadata`
//!    naming every component and declaring every reference that leaves one.
//! 3. **The identity**, `Metadata/Properties.plist` and
//!    `Metadata/DocumentIdentifier`: the UUIDs that say which document this is.
//! 4. **Nothing else.** No previews (the app draws its own), no build history,
//!    no view state — all three apps open a document that has none.

use std::collections::BTreeMap;

use crate::iwa::{ArchiveMessage, ArchiveObject};
use crate::pb::{Field, Message, Value};
use crate::{Kind, Package};

/// The archive version every object this crate writes carries.
///
/// `MessageInfo.version`, and it is `[1, 0, 5]` on every object of every
/// document in the corpus — the apps write one number for the whole file.
pub(crate) const VERSION: [u32; 3] = [1, 0, 5];

/// `TSP.ComponentInfo.save_token` / `read_version` — `[2, 0, 0]`, packed.
const COMPONENT_VERSION: [u8; 3] = [2, 0, 0];

/// `TSP.ComponentInfo` field 12, and `TSP.PackageMetadata` field 8: the version
/// of the format the component was written in.
///
/// **It is per app, and that matters.** Pages writes 834 and 835, Numbers 535,
/// 536 and 537, Keynote 2385 and 2386 — the highest of each being what the
/// package itself claims in field 8. Writing Pages' number into a Numbers
/// document was watched being refused: everything else about the document was
/// right, every object had been grafted from one the app wrote, and Numbers
/// still would not open it. Nothing in this crate reads the number for meaning;
/// what it does is write back the one the app that has to read it uses.
fn component_generation(kind: Kind) -> u64 {
    match kind {
        Kind::Pages => 835,
        Kind::Numbers => 537,
        Kind::Keynote => 2386,
        Kind::Unknown => 0,
    }
}

/// The root object of a document, and the root of its metadata component.
///
/// Both are fixed by convention rather than by anything in the bytes: every
/// document in the corpus numbers its document archive 1 and its
/// `DocumentMetadata` 71, and a reader that has neither open has nothing to
/// resolve an identifier against ([`crate::Document::undeclared_references`]).
pub(crate) const ROOT: u64 = 1;
pub(crate) const DOCUMENT_METADATA: u64 = 71;
/// The `TSP.PackageMetadata` itself, which is object 2 in every document.
const PACKAGE_METADATA: u64 = 2;

/// Where the identifiers this module hands out begin.
///
/// Above the four the format fixes (1, 2, 71, and the data-metadata map), and
/// low enough that a document written here is readable: an object numbered
/// 1_732_539 is Pages counting from wherever its session had got to, not
/// something the format asks for.
const FIRST_IDENTIFIER: u64 = 1000;

// -- building blocks ----------------------------------------------------------

/// A protobuf message from its fields.
pub(crate) fn message(fields: Vec<Field>) -> Message {
    Message { fields }
}

pub(crate) fn varint(number: u32, value: u64) -> Field {
    Field {
        number,
        value: Value::Varint(value),
    }
}

pub(crate) fn string(number: u32, text: &str) -> Field {
    Field {
        number,
        value: Value::Bytes(text.as_bytes().to_vec()),
    }
}

pub(crate) fn float(number: u32, value: f32) -> Field {
    Field {
        number,
        value: Value::Fixed32(value.to_le_bytes()),
    }
}

#[allow(dead_code)]
pub(crate) fn double(number: u32, value: f64) -> Field {
    Field {
        number,
        value: Value::Fixed64(value.to_le_bytes()),
    }
}

#[allow(dead_code)]
/// Raw bytes as a length-delimited field.
pub(crate) fn bytes(number: u32, value: Vec<u8>) -> Field {
    Field {
        number,
        value: Value::Bytes(value),
    }
}

/// A nested message as a length-delimited field.
pub(crate) fn nested(number: u32, fields: Vec<Field>) -> Field {
    Field {
        number,
        value: Value::Bytes(message(fields).encode()),
    }
}

/// A `TSP.Reference` — `{1: identifier}`, the whole of it.
pub(crate) fn reference(number: u32, target: u64) -> Field {
    nested(number, vec![varint(1, target)])
}

/// An attribute table with a single entry covering the whole storage.
pub(crate) fn attribute_table(number: u32, target: u64) -> Field {
    Field {
        number,
        value: Value::Bytes(
            message(vec![nested(1, vec![varint(1, 0), reference(2, target)])]).encode(),
        ),
    }
}

// -- the blueprint ------------------------------------------------------------

/// A document being assembled: components, the objects in them, and the
/// identifiers handed out so far.
pub(crate) struct Blueprint {
    kind: Kind,
    components: Vec<Component>,
    next: u64,
}

/// One component — one `Index/*.iwa` stream, and the objects it holds.
struct Component {
    /// `ComponentInfo.identifier`, which is also its root object's.
    identifier: u64,
    /// `ComponentInfo.preferred_name`: "Document", "DocumentStylesheet".
    name: String,
    /// `ComponentInfo.locator`, when the stream is not named for the component
    /// — `CalculationEngine-1234` for a `CalculationEngine`.
    locator: Option<String>,
    objects: Vec<ArchiveObject>,
    /// Set on `DocumentMetadata` alone, in every document in the corpus.
    always_loaded: bool,
}

/// Which component an object goes in, handed back by [`Blueprint::component`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ComponentId(usize);

impl Blueprint {
    /// A blueprint with the two components every document has.
    pub(crate) fn new(kind: Kind) -> Blueprint {
        let mut blueprint = Blueprint {
            kind,
            components: Vec::new(),
            next: FIRST_IDENTIFIER,
        };
        blueprint.push_component("Document", ROOT, None, false);
        blueprint.push_component("DocumentMetadata", DOCUMENT_METADATA, None, true);
        blueprint
    }

    fn push_component(
        &mut self,
        name: &str,
        identifier: u64,
        locator: Option<String>,
        always_loaded: bool,
    ) -> ComponentId {
        self.components.push(Component {
            identifier,
            name: name.to_string(),
            locator,
            objects: Vec::new(),
            always_loaded,
        });
        ComponentId(self.components.len() - 1)
    }

    /// The `Document` component, which every blueprint has.
    pub(crate) fn document(&self) -> ComponentId {
        ComponentId(0)
    }

    fn metadata(&self) -> ComponentId {
        ComponentId(1)
    }

    /// A new component, rooted at a fresh identifier.
    ///
    /// `locator` is the stream name when it is not the component's name; iWork
    /// writes `CalculationEngine-1732610` for the calculation engine and the
    /// plain name for everything else.
    pub(crate) fn component(&mut self, name: &str, numbered: bool) -> (ComponentId, u64) {
        let identifier = self.allocate();
        let locator = numbered.then(|| format!("{name}-{identifier}"));
        (
            self.push_component(name, identifier, locator, false),
            identifier,
        )
    }

    #[allow(dead_code)]
    /// A component holding one object, which is its root.
    ///
    /// How Numbers stores a table: every tile, every interning list and every
    /// header bucket is a component of its own, named `Tables/Tile-1007` and
    /// the like, so the app can load one without loading the rest.
    pub(crate) fn in_own_component(
        &mut self,
        name: &str,
        message_type: u32,
        archive: Message,
    ) -> u64 {
        let (component, identifier) = self.component(name, true);
        self.put(component, identifier, message_type, archive);
        identifier
    }

    /// The next unused object identifier.
    pub(crate) fn allocate(&mut self) -> u64 {
        let identifier = self.next;
        self.next += 1;
        identifier
    }

    /// Put an object in a component under an identifier already allocated.
    pub(crate) fn put(
        &mut self,
        component: ComponentId,
        identifier: u64,
        message_type: u32,
        archive: Message,
    ) {
        self.components[component.0].objects.push(ArchiveObject {
            identifier,
            messages: vec![ArchiveMessage {
                message_type,
                version: VERSION.to_vec(),
                extra: Vec::new(),
                payload: archive.encode(),
            }],
            extra: Vec::new(),
        });
    }

    /// Allocate an identifier and put an object under it.
    pub(crate) fn add(
        &mut self,
        component: ComponentId,
        message_type: u32,
        archive: Message,
    ) -> u64 {
        let identifier = self.allocate();
        self.put(component, identifier, message_type, archive);
        identifier
    }

    /// Every object, whichever component it is in — for the reference walk that
    /// writes the component index.
    fn objects(&self) -> impl Iterator<Item = (&Component, &ArchiveObject)> {
        self.components
            .iter()
            .flat_map(|component| component.objects.iter().map(move |o| (component, o)))
    }

    /// Assemble the package: object streams, the component index, the identity.
    pub(crate) fn finish(mut self) -> Package {
        self.write_document_metadata();

        let mut package = Package {
            entries: Vec::new(),
            form: crate::package::Form::SingleFile,
        };
        for component in &self.components {
            package.set(
                &stream_name(component),
                crate::iwa::serialize(&component.objects),
            );
        }
        package.set(
            "Index/Metadata.iwa",
            crate::iwa::serialize(&[self.component_index()]),
        );
        crate::metadata::write_identity(&mut package, self.kind);
        package
    }

    /// `TSP.DocumentMetadata` — the object every reader opens second.
    ///
    /// Field 3 is a repeated save token; the app writes one per save, carrying a
    /// digest of what it wrote. A document that has never been saved by an app
    /// has none, and Pages, Numbers and Keynote all open one that way.
    fn write_document_metadata(&mut self) {
        let component = self.metadata();
        self.put(
            component,
            DOCUMENT_METADATA,
            TYPE_DOCUMENT_METADATA,
            message(vec![varint(1, 0)]),
        );
    }

    /// `TSP.PackageMetadata`: every component, and every reference that leaves
    /// the component it is written in.
    fn component_index(&self) -> ArchiveObject {
        // Which component each object lives in, so a reference can be told from
        // one that stays at home.
        let mut home: BTreeMap<u64, u64> = BTreeMap::new();
        for (component, object) in self.objects() {
            home.insert(object.identifier, component.identifier);
        }

        // Field 1 is the high-water mark: the highest identifier ever handed
        // out in this package. iWork allocates above it rather than above what
        // it can see, so a document that understates it hands out an identifier
        // something already has. `Document::problems` checks it, and did so
        // against the first document this module wrote.
        let highest = self
            .objects()
            .map(|(_, object)| object.identifier)
            .max()
            .unwrap_or(DOCUMENT_METADATA)
            .max(self.next);
        let mut fields = vec![
            varint(1, highest),
            nested(2, vec![string(2, &crate::metadata::uuid()), varint(3, 0)]),
        ];
        for component in &self.components {
            let mut info = vec![varint(1, component.identifier), string(2, &component.name)];
            if let Some(locator) = &component.locator {
                info.push(string(3, locator));
            }
            info.push(Field {
                number: 4,
                value: Value::Bytes(COMPONENT_VERSION.to_vec()),
            });
            info.push(Field {
                number: 5,
                value: Value::Bytes(COMPONENT_VERSION.to_vec()),
            });

            // The declarations. `Document` and `DocumentMetadata` are open
            // before anything else is read, so a reference into either needs no
            // declaring; every other crossing does, root objects included.
            let mut declared: Vec<u64> = Vec::new();
            for object in &component.objects {
                let Ok(archive) = Message::decode(object.payload()) else {
                    continue;
                };
                for target in crate::style::references(&archive) {
                    let Some(&owner) = home.get(&target) else {
                        continue;
                    };
                    if owner == component.identifier
                        || target == ROOT
                        || target == DOCUMENT_METADATA
                    {
                        continue;
                    }
                    declared.push(target);
                }
            }
            declared.sort_unstable();
            declared.dedup();
            for target in declared {
                let owner = home[&target];
                // Two forms, and they are not interchangeable: `{component}`
                // alone declares that component's **root**, whose identifier is
                // the component's own, and `{component, object}` declares one
                // object inside it. iWork writes the short form for a root and
                // the long form for everything else.
                info.push(match target == owner {
                    true => nested(6, vec![varint(1, owner)]),
                    false => nested(6, vec![varint(1, owner), varint(2, target)]),
                });
            }

            // Every object in the component, given a UUID of its own.
            //
            // Pages does not need these — the reduction deleted all 2,450 of
            // them and the document still opened. Numbers does: nothing in its
            // component index could be deleted at all, this included, and a
            // document written without them is refused. FORMAT.md's rule 23
            // already said a *new* component needs fresh
            // `object_uuid_map_entries`; this says a new document does too.
            for object in &component.objects {
                let (lower, upper) = object_uuid();
                info.push(nested(
                    11,
                    vec![
                        varint(1, object.identifier),
                        nested(2, vec![varint(1, lower), varint(2, upper)]),
                    ],
                ));
            }

            info.push(varint(10, 0));
            info.push(varint(12, component_generation(self.kind)));
            if component.always_loaded {
                info.push(varint(21, 1));
            }
            fields.push(nested(3, info));
        }
        // The package's own version and generation, as every document carries
        // them. The per-component pair above is deletable and these are not:
        // a package written without field 5 is refused.
        fields.push(Field {
            number: 5,
            value: Value::Bytes(COMPONENT_VERSION.to_vec()),
        });
        fields.push(varint(8, component_generation(self.kind)));

        ArchiveObject {
            identifier: PACKAGE_METADATA,
            messages: vec![ArchiveMessage {
                message_type: TYPE_PACKAGE_METADATA,
                version: VERSION.to_vec(),
                extra: Vec::new(),
                payload: message(fields).encode(),
            }],
            extra: Vec::new(),
        }
    }
}

/// A `TSP.UUID` for one object, as two 64-bit halves.
///
/// Uniqueness inside the document is the whole requirement — two components
/// claiming the same object UUID is how a document that still opens ends up
/// corrupted (FORMAT.md rule 23) — so the entropy comes from the same place the
/// document's own UUIDs do.
fn object_uuid() -> (u64, u64) {
    let hex = crate::metadata::uuid().replace('-', "");
    let half = |range: std::ops::Range<usize>| u64::from_str_radix(&hex[range], 16).unwrap_or(0);
    (half(0..16), half(16..32))
}

fn stream_name(component: &Component) -> String {
    format!(
        "Index/{}.iwa",
        component.locator.as_deref().unwrap_or(&component.name)
    )
}

/// `TSP.PackageMetadata`.
const TYPE_PACKAGE_METADATA: u32 = 11006;
/// `TSP.DocumentMetadata`.
const TYPE_DOCUMENT_METADATA: u32 = 11011;

// -- Pages --------------------------------------------------------------------

/// `TSWP.StorageArchive`.
const TYPE_STORAGE: u32 = 2001;
/// `TSS.StylesheetArchive`.
const TYPE_STYLESHEET: u32 = 401;
/// `TSWP.ColumnStyleArchive`.
const TYPE_COLUMN_STYLE: u32 = 2024;
/// `TP.DocumentArchive`.
const TYPE_PAGES_DOCUMENT: u32 = 10000;

/// The paper a new Pages document starts on.
///
/// Pages ships its Blank template twice — `ISO.template` and
/// `Traditional.template` — and the only difference between the two documents
/// is this: the page, the margins and the paper's name. Both were read out of
/// those templates rather than looked up.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Paper {
    /// 595.28 × 841.89 pt, 56.69 pt margins. What Pages calls `iso-a4`.
    #[default]
    A4,
    /// 612 × 792 pt, 72 pt margins. What Pages calls `na-letter`.
    Letter,
}

impl Paper {
    /// Width, height, margin, header inset, footer inset — all in points.
    fn measurements(self) -> (f32, f32, f32, f32, f32) {
        match self {
            Paper::A4 => (595.28, 841.89, 56.692917, 35.43307, 42.519684),
            Paper::Letter => (612.0, 792.0, 72.0, 36.0, 43.2),
        }
    }

    fn name(self) -> &'static str {
        match self {
            Paper::A4 => "iso-a4",
            Paper::Letter => "na-letter",
        }
    }
}

/// A Pages document with one empty body.
///
/// Eleven objects, which is what the reduction left of the 569 in a document
/// Pages made from its own Blank template:
///
/// ```text
/// 1     TP.DocumentArchive      the page, the margins, the locale
///  └ body  TSWP.StorageArchive  the text, and the tables anchored into it
/// 401   TSS.StylesheetArchive   in its own component
///  ├ TSWP.ParagraphStyleArchive "Body"
///  ├ TSWP.ListStyleArchive      "None"
///  └ TSWP.ColumnStyleArchive    "None", and the variation the body points at
/// 71    TSP.DocumentMetadata
/// ```
///
/// What is *not* here is as measured as what is: no theme, no section, no
/// section templates, no headers or footers, no view state, no calculation
/// engine, no annotation authors, no previews, no build history. Pages opens
/// this and reads the text back out of it.
pub(crate) fn pages(paper: Paper) -> Blueprint {
    let mut blueprint = Blueprint::new(Kind::Pages);
    let document = blueprint.document();
    let (styles, stylesheet) = blueprint.component("DocumentStylesheet", false);

    // -- the stylesheet's styles, which have to exist before it lists them ----

    let list = blueprint.add(
        styles,
        crate::style::TYPE_LIST_STYLE,
        list_style(stylesheet),
    );
    let body_style = blueprint.add(
        styles,
        crate::style::TYPE_PARAGRAPH_STYLE,
        paragraph_style(stylesheet, list),
    );
    let column = blueprint.add(styles, TYPE_COLUMN_STYLE, column_style(stylesheet));
    let body_column = blueprint.add(
        styles,
        TYPE_COLUMN_STYLE,
        column_variation(stylesheet, column),
    );
    blueprint.put(
        styles,
        stylesheet,
        TYPE_STYLESHEET,
        stylesheet_archive(column, body_column, body_style),
    );

    // -- the body, and the document that points at it -------------------------

    let body = blueprint.add(
        document,
        TYPE_STORAGE,
        body_storage(stylesheet, body_style, list, body_column),
    );
    blueprint.put(
        document,
        ROOT,
        TYPE_PAGES_DOCUMENT,
        pages_document(stylesheet, body, paper),
    );
    blueprint
}

/// `TSS.StylesheetArchive`: the styles a document offers by name.
///
/// Field 5 is the pair of column styles the body's layout comes from; field 8 is
/// the list of named styles, as references and again as `{identifier, style}`
/// entries — the keyed form is what the app looks a style up by when it applies
/// one from the menu.
fn stylesheet_archive(column: u64, body_column: u64, body_style: u64) -> Message {
    message(vec![
        varint(4, 0),
        nested(5, vec![reference(1, column), reference(2, body_column)]),
        varint(6, 1),
        nested(
            8,
            vec![
                reference(1, body_style),
                nested(
                    2,
                    vec![string(1, BODY_IDENTIFIER), reference(2, body_style)],
                ),
            ],
        ),
    ])
}

/// The internal name of the one paragraph style a new document has.
///
/// The shape is the app's — `text-<n>-paragraphstyle-<name>` — and the number
/// is a theme's index, which a document with no theme does not have. 0 is what
/// the bundled themes number their first.
const BODY_IDENTIFIER: &str = "text-0-paragraphstyle-Body";

/// `TSWP.ParagraphStyleArchive` — "Body", 11 pt Helvetica Neue, black, ranged
/// left.
///
/// Field 11 is the character bag and field 12 the paragraph bag; the reduction
/// deleted **both** and Pages still opened the document, so everything here is
/// chosen rather than required. What is chosen is the smallest set that makes
/// the result a document rather than a shrug: a font, a size, a colour, an
/// alignment, and the list style the paragraph is not in.
fn paragraph_style(stylesheet: u64, list: u64) -> Message {
    use crate::style::property;
    message(vec![
        nested(
            1,
            vec![
                string(1, "Body"),
                string(2, BODY_IDENTIFIER),
                reference(5, stylesheet),
            ],
        ),
        nested(
            property::FONT_SIZE[0],
            vec![
                varint(property::BOLD[1], 0),
                varint(property::ITALIC[1], 0),
                float(property::FONT_SIZE[1], 11.0),
                string(property::FONT_NAME[1], "HelveticaNeue"),
                nested(property::FONT_COLOR[1], black()),
            ],
        ),
        nested(
            property::ALIGNMENT[0],
            vec![
                // 4 is "natural" — left in a left-to-right language, and what
                // every Body style in the corpus carries.
                varint(property::ALIGNMENT[1], 4),
                reference(property::LIST_STYLE[1], list),
            ],
        ),
    ])
}

/// `TSP.Color`, in the sRGB form every style in the corpus uses: a model, three
/// components, an alpha — and field 12, which is the colour space and is 1 on
/// every colour Pages 15.3.1 writes.
fn black() -> Vec<Field> {
    vec![
        varint(1, 1),
        float(3, 0.0),
        float(4, 0.0),
        float(5, 0.0),
        float(6, 1.0),
        varint(12, 1),
        float(13, 1.0),
    ]
}

/// `TSWP.ListStyleArchive` — "None", the list a paragraph is in when it is in
/// no list.
///
/// The nine repeated values are one per indent level: field 11 the label type
/// (0, none), field 12 the label indent and field 13 the text indent, 36 pt
/// apart. Nine is what Pages writes; nothing here has asked whether it is a
/// limit or a habit.
fn list_style(stylesheet: u64) -> Message {
    let mut fields = vec![
        nested(
            1,
            vec![
                string(1, "None"),
                string(2, "text-0-liststyle-None"),
                reference(5, stylesheet),
            ],
        ),
        varint(10, 4),
    ];
    for _ in 0..9 {
        fields.push(varint(11, 0));
    }
    for _ in 0..9 {
        fields.push(float(12, 0.0));
    }
    for level in 0..9 {
        fields.push(float(13, 36.0 * level as f32));
    }
    message(fields)
}

/// `TSWP.ColumnStyleArchive` — "None": one column, no gutter.
fn column_style(stylesheet: u64) -> Message {
    message(vec![
        nested(
            1,
            vec![
                string(1, "None"),
                string(2, "column-style-default"),
                reference(5, stylesheet),
            ],
        ),
        varint(10, 9),
        nested(
            11,
            vec![
                // One column, equal widths, no gap.
                varint(1, 1),
                varint(2, 0),
                varint(3, 0),
                float(4, 0.0),
                varint(5, 0),
                nested(7, vec![reference(1, ROOT)]),
            ],
        ),
    ])
}

/// The anonymous column style the body actually points at.
///
/// A *variation*: no name, a parent, and field 4 set — the archive iWork writes
/// when something is formatted directly rather than by picking a named style.
/// The body storage points at this one, and this one inherits everything from
/// the style above.
fn column_variation(stylesheet: u64, parent: u64) -> Message {
    message(vec![
        nested(
            1,
            vec![reference(3, parent), varint(4, 1), reference(5, stylesheet)],
        ),
        varint(10, 1),
    ])
}

/// `TSWP.StorageArchive` for the body: no text, and one entry in every
/// attribute table.
///
/// A table entry is `{1: character index, 2: value}` and the first is always at
/// 0 ([`crate::style`]). An empty storage still needs them: they are what says
/// which paragraph style the first paragraph is in, and Pages writes exactly
/// this — one entry each — for the empty body of a new document.
fn body_storage(stylesheet: u64, paragraph: u64, list: u64, column: u64) -> Message {
    message(vec![
        // kind 0: the body of a word-processing document.
        varint(1, 0),
        reference(2, stylesheet),
        attribute_table(5, paragraph),
        // table_para_data — `{index, first, second}`, the list level and its
        // number, both zero for a paragraph in no list.
        nested(
            6,
            vec![nested(1, vec![varint(1, 0), varint(2, 0), varint(3, 0)])],
        ),
        attribute_table(7, list),
        varint(10, 1),
        attribute_table(12, column),
        // table_para_starts and table_para_bidi: one entry, zero.
        nested(
            14,
            vec![nested(1, vec![varint(1, 0), varint(2, 0), varint(3, 0)])],
        ),
        nested(
            24,
            vec![nested(1, vec![varint(1, 0), varint(2, 0), varint(3, 0)])],
        ),
    ])
}

/// `TP.DocumentArchive`: the page, and what is on it.
///
/// Field 15 is the `TSA.DocumentArchive` every app's document archive is built
/// on — the locale, the language, and which template this came from. A document
/// this crate made came from no template, so it says so by leaving field 9 out.
fn pages_document(stylesheet: u64, body: u64, paper: Paper) -> Message {
    let (width, height, margin, header, footer) = paper.measurements();
    message(vec![
        reference(2, stylesheet),
        reference(4, body),
        nested(
            15,
            vec![nested(
                1,
                vec![
                    // TSK.DocumentArchive: the two locales, both unset, and the
                    // flags that say they were never chosen by a user.
                    varint(10, 1),
                    varint(12, 0),
                    varint(15, 1),
                    varint(16, 1),
                ],
            )],
        ),
        float(30, width),
        float(31, height),
        float(32, margin),
        float(33, margin),
        float(34, margin),
        float(35, margin),
        float(36, header),
        float(37, footer),
        float(38, 1.0),
        varint(39, 0),
        varint(40, 0),
        varint(42, 0),
        // The printer this was laid out for, and the paper in it. A space is
        // what the bundled templates carry — the app fills in a real one the
        // first time the document meets a printer.
        string(43, " "),
        string(44, paper.name()),
    ])
}

// -- Numbers ------------------------------------------------------------------
//
// Everything below is `allow(dead_code)` because `Document::new` does not offer
// it: Numbers refuses what it writes, and a document the app refuses is worse
// than no document at all. It is kept, and unit-tested for everything that can
// be checked without the app, because the measurements behind it are the
// expensive part — see [`numbers`].

/// `TN.DocumentArchive` — the root of a Numbers document, and object 1.
#[allow(dead_code)]
const TYPE_NUMBERS_DOCUMENT: u32 = 1;
/// `TN.SheetArchive`.
#[allow(dead_code)]
const TYPE_SHEET: u32 = 2;
/// `TST.TableInfoArchive` — the drawable a table hangs off.
#[allow(dead_code)]
const TYPE_TABLE_INFO: u32 = 6000;
/// `TST.TableModelArchive` — the table itself.
#[allow(dead_code)]
const TYPE_TABLE_MODEL: u32 = 6001;
/// `TST.Tile` — the cells, 256 rows at a time.
#[allow(dead_code)]
const TYPE_TILE: u32 = 6002;
/// `TST.TableDataList` — the interning tables cells refer into.
#[allow(dead_code)]
const TYPE_DATA_LIST: u32 = 6005;
/// `TST.HeaderStorageBucket` — per-row and per-column sizes and cell counts.
#[allow(dead_code)]
const TYPE_HEADER_BUCKET: u32 = 6006;

/// A tile covers this many rows, and `TileStorage.tile_size` says so.
#[allow(dead_code)]
const TILE_SIZE: u64 = 256;
/// What Numbers gives a new table, in points.
#[allow(dead_code)]
const DEFAULT_ROW_HEIGHT: f64 = 19.929931640625;
#[allow(dead_code)]
const DEFAULT_COLUMN_WIDTH: f64 = 98.0;

/// A Numbers document with one sheet and one empty table.
///
/// **Numbers does not open this yet, and [`crate::Document::new`] refuses to
/// hand it out.** It is kept because most of it is measured rather than
/// guessed, and the measurements are the expensive part:
///
/// * A Numbers root needs four things a Pages root does not — a stylesheet, a
///   theme, a `TSK` 205 (empty, and required all the same) and a calculation
///   engine. Deleting any one of them from a document Numbers wrote makes
///   Numbers refuse it.
/// * `ComponentInfo` field 12 and `PackageMetadata` field 8 are **per app**:
///   Pages writes 835, Numbers 537, Keynote 2386.
/// * Nothing in a Numbers component index is deletable — twenty-five probes,
///   not one accepted — where a Pages one gives up 2,450 object-UUID entries at
///   once. So the UUID map is written for every document now.
/// * Every tile, interning list and header bucket is a component of its own.
///
/// What is still missing is not known. The app's own object graph, grafted
/// wholesale into a package this crate wrote, is refused too, which says the
/// remaining difference is in the package rather than the objects — and the
/// machine's Numbers stopped opening *any* document before that could be run
/// down. The reduction to continue from is `REDUCE_OBJECTS=<theme>` against a
/// document Numbers wrote.
#[allow(dead_code)]
pub(crate) fn numbers(
    sheet_name: &str,
    table_name: &str,
    rows: usize,
    columns: usize,
) -> Blueprint {
    let mut blueprint = Blueprint::new(Kind::Numbers);
    let document = blueprint.document();

    // The interning tables every cell refers into. A new table has interned
    // nothing, so each is a list type and a next key of 1 — keys start at 1
    // because a stored key of 0 means "none".
    let strings = blueprint.in_own_component("Tables/DataList", TYPE_DATA_LIST, data_list(1));
    // The format list starts with one entry rather than none: the automatic
    // format, which every slot can point at. `set_cell` gives a cell its format
    // by borrowing one the table already uses, and a table with no cells has
    // none to borrow — so a new table would be one no value could be written
    // into. Its refcount is 0 until a cell takes it.
    let formats =
        blueprint.in_own_component("Tables/DataList", TYPE_DATA_LIST, automatic_format_list());
    let styles = blueprint.in_own_component("Tables/DataList", TYPE_DATA_LIST, data_list(4));
    // The list style every paragraph style needs to say it is in no list.
    let (style_component, stylesheet) = blueprint.component("DocumentStylesheet", false);
    let list = blueprint.add(
        style_component,
        crate::style::TYPE_LIST_STYLE,
        list_style(stylesheet),
    );

    // One entry per row and per column: the default size (a literal 0 means
    // "still the table's default"), visible, and no cells yet.
    let row_bucket = blueprint.in_own_component(
        "Tables/HeaderStorageBucket",
        TYPE_HEADER_BUCKET,
        header_bucket(rows),
    );
    let column_bucket = blueprint.in_own_component(
        "Tables/HeaderStorageBucket",
        TYPE_HEADER_BUCKET,
        header_bucket(columns),
    );
    // One tile per 256 rows, each with a `TileRowInfo` per row.
    //
    // Numbers writes no `TileRowInfo` for a row with no cells, and a table made
    // that way is one no caller can then write into: `set_cell` finds a cell by
    // its row's entry, and giving a row its first entry is a job this crate has
    // not taken on. Writing the empty entries costs 522 bytes a row and makes
    // every cell of a new table writable, which is the whole point of making
    // one.
    let tiles: Vec<(u64, u64)> = (0..rows.div_ceil(TILE_SIZE as usize))
        .map(|index| {
            let first = index * TILE_SIZE as usize;
            let count = (rows - first).min(TILE_SIZE as usize);
            (
                index as u64,
                blueprint.in_own_component("Tables/Tile", TYPE_TILE, tile(count, columns)),
            )
        })
        .collect();

    // What the reduction found a Numbers root cannot do without, beyond its
    // sheets: a stylesheet, a theme, a `TSK` 205 (which is *empty* in the
    // document it was measured in), and a calculation engine.
    blueprint.put(
        style_component,
        stylesheet,
        TYPE_STYLESHEET,
        message(Vec::new()),
    );
    let sheet_style = blueprint.add(style_component, TYPE_SHEET_STYLE, sheet_style(stylesheet));
    let theme = blueprint.add(document, TYPE_NUMBERS_THEME, numbers_theme(stylesheet));
    let support = blueprint.add(document, TYPE_DOCUMENT_SUPPORT, message(Vec::new()));
    let engine = blueprint.in_own_component(
        "CalculationEngine",
        TYPE_CALCULATION_ENGINE,
        message(vec![nested(2, Vec::new())]),
    );

    // The styles the table model names. A `TST.TableModelArchive` points at a
    // table style, four cell styles (body, header row, header column, footer),
    // the text styles for the same four areas, and a shape style — and Numbers,
    // unlike Pages, does not open a document whose table names none of them.
    let table_style = blueprint.add(
        style_component,
        TYPE_TABLE_STYLE,
        named_style("table-0-tableStyle", stylesheet),
    );
    let cell_styles: Vec<u64> = ["body", "headerRow", "headerColumn", "footerRow"]
        .iter()
        .map(|area| {
            blueprint.add(
                style_component,
                TYPE_CELL_STYLE,
                cell_style(&format!("tableCell-0-{area}Style"), stylesheet),
            )
        })
        .collect();
    let cell_text = blueprint.add(
        style_component,
        crate::style::TYPE_PARAGRAPH_STYLE,
        paragraph_style(stylesheet, list),
    );
    let shape_style = blueprint.add(
        style_component,
        TYPE_SHAPE_STYLE,
        named_style("shape-0-tableStyle", stylesheet),
    );

    let sheet = blueprint.allocate();
    let model = blueprint.allocate();
    let info = blueprint.add(
        document,
        TYPE_TABLE_INFO,
        table_info(sheet, model, rows, columns),
    );
    blueprint.put(
        document,
        model,
        TYPE_TABLE_MODEL,
        table_model(TableParts {
            name: table_name,
            rows,
            columns,
            tiles: &tiles,
            row_bucket,
            column_bucket,
            strings,
            formats,
            styles,
            table_style,
            cell_styles: &cell_styles,
            cell_text,
            shape_style,
        }),
    );
    blueprint.put(
        document,
        sheet,
        TYPE_SHEET,
        sheet_archive(sheet_name, info, sheet_style),
    );
    blueprint.put(
        document,
        ROOT,
        TYPE_NUMBERS_DOCUMENT,
        numbers_document(sheet, stylesheet, support, theme, engine),
    );
    blueprint
}

/// The `FORMAT` list of a new table: one entry, the automatic format.
///
/// A `ListEntry` is `{1: key, 2: refcount}` and a payload field, which for a
/// format list is field 6 — a `TSK.FormatStructArchive`, whose field 1 is the
/// format type. 260 is automatic, and it is what Numbers writes for a cell
/// nobody has formatted, text cells included.
#[allow(dead_code)]
fn automatic_format_list() -> Message {
    message(vec![
        varint(1, 2),
        varint(2, 2),
        nested(
            3,
            vec![varint(1, 1), varint(2, 0), nested(6, vec![varint(1, 260)])],
        ),
    ])
}

/// `TST.TableDataList` — `{1: listType, 2: nextListID}` and no entries.
#[allow(dead_code)]
fn data_list(list_type: u64) -> Message {
    message(vec![varint(1, list_type), varint(2, 1)])
}

/// `TST.HeaderStorageBucket` — one entry per row, or per column.
///
/// `{1: index, 2: size, 3: hidingState, 4: numberOfCells}`, and a size of a
/// literal `0` is what a row still at the table's default height carries. The
/// entry for a row with no cells is optional in documents Numbers writes; it is
/// written here because a row that exists and says nothing about itself is
/// harder to reason about than one that does.
#[allow(dead_code)]
fn header_bucket(count: usize) -> Message {
    let mut fields = vec![varint(1, 1)];
    for index in 0..count {
        fields.push(nested(
            2,
            vec![
                varint(1, index as u64),
                float(2, 0.0),
                varint(3, 0),
                varint(4, 0),
            ],
        ));
    }
    message(fields)
}

/// `TST.Tile` holding `rows` rows, every cell of them empty.
///
/// `TileRowInfo` fields 3 and 4 are the pre-BNC buffer and its offsets: dead
/// weight in this storage version, `required` in the schema, and present on
/// every row of every tile in the corpus. They are written because a `required`
/// field a parser cannot find is a parse error, not a default.
#[allow(dead_code)]
fn tile(rows: usize, columns: usize) -> Message {
    let mut fields = vec![
        varint(1, 0),
        varint(2, 0),
        varint(3, 0),
        varint(4, rows as u64),
    ];
    for index in 0..rows {
        fields.push(nested(
            5,
            vec![
                varint(1, index as u64),
                varint(2, 0),
                bytes(3, Vec::new()),
                bytes(4, empty_offsets(columns)),
                varint(5, 5),
                bytes(6, Vec::new()),
                bytes(7, empty_offsets(columns)),
            ],
        ));
    }
    // storage_version 5 and last_saved_in_BNC — every tile in the corpus
    // carries both, and a reader is entitled to refuse one that does not.
    fields.push(varint(6, 5));
    fields.push(varint(7, 1));
    message(fields)
}

/// A cell-offset array with every column empty.
///
/// `int16[]`, little-endian and signed, `-1` for a column with no cell. Numbers
/// pads the array well past the table's width — 255 entries for a five-column
/// table — and the padding is what leaves room for a column to be given a cell
/// later, so it is written the same way here.
#[allow(dead_code)]
fn empty_offsets(columns: usize) -> Vec<u8> {
    let slots = columns.max(OFFSET_SLOTS);
    (-1i16).to_le_bytes().repeat(slots)
}

/// How many offsets a row carries whatever its width, as Numbers writes them.
#[allow(dead_code)]
const OFFSET_SLOTS: usize = 255;

/// `TST.TableInfoArchive` — the drawable the table is drawn as.
///
/// Field 1 is the `TSD.DrawableArchive` every placed object begins with: a
/// geometry and the thing it hangs off, which for a Numbers table is the sheet.
#[allow(dead_code)]
fn table_info(sheet: u64, model: u64, rows: usize, columns: usize) -> Message {
    let width = columns as f32 * DEFAULT_COLUMN_WIDTH as f32;
    let height = rows as f32 * DEFAULT_ROW_HEIGHT as f32;
    message(vec![
        nested(
            1,
            vec![
                nested(
                    1,
                    vec![
                        nested(1, vec![float(1, 0.0), float(2, 0.0)]),
                        nested(2, vec![float(1, width), float(2, height)]),
                        varint(3, 3),
                        float(4, 0.0),
                    ],
                ),
                reference(2, sheet),
            ],
        ),
        reference(2, model),
    ])
}

/// Everything [`table_model`] needs, so its signature stays readable.
#[allow(dead_code)]
struct TableParts<'a> {
    name: &'a str,
    rows: usize,
    columns: usize,
    tiles: &'a [(u64, u64)],
    row_bucket: u64,
    column_bucket: u64,
    strings: u64,
    formats: u64,
    styles: u64,
    table_style: u64,
    /// Body, header row, header column, footer row — in that order.
    cell_styles: &'a [u64],
    cell_text: u64,
    shape_style: u64,
}

/// `TST.TableModelArchive` — the table, with its `TST.DataStore` inline.
///
/// The row/column asymmetry at the data store is Apple's: rows get a *list* of
/// bucket references and columns get a single one.
#[allow(dead_code)]
fn table_model(parts: TableParts) -> Message {
    message(vec![
        string(1, &table_id()),
        nested(
            4,
            vec![
                nested(1, vec![varint(1, 1), reference(2, parts.row_bucket)]),
                reference(2, parts.column_bucket),
                nested(3, {
                    let mut storage: Vec<Field> = parts
                        .tiles
                        .iter()
                        .map(|(index, tile)| {
                            nested(1, vec![varint(1, *index), reference(2, *tile)])
                        })
                        .collect();
                    storage.push(varint(2, TILE_SIZE));
                    storage
                }),
                reference(4, parts.strings),
                reference(5, parts.styles),
                reference(22, parts.formats),
            ],
        ),
        varint(6, parts.rows as u64),
        varint(7, parts.columns as u64),
        string(8, parts.name),
        // One header row and one header column, which is what Numbers gives a
        // new table, and both frozen.
        varint(9, 1),
        varint(10, 1),
        varint(11, 0),
        varint(12, 1),
        varint(13, 1),
        double(16, DEFAULT_ROW_HEIGHT),
        double(17, DEFAULT_COLUMN_WIDTH),
        reference(3, parts.table_style),
        reference(18, parts.cell_styles[0]),
        reference(19, parts.cell_styles[1]),
        reference(20, parts.cell_styles[2]),
        reference(21, parts.cell_styles[3]),
        reference(24, parts.cell_text),
        reference(25, parts.cell_text),
        reference(26, parts.cell_text),
        reference(27, parts.cell_text),
        reference(30, parts.cell_text),
        reference(36, parts.shape_style),
    ])
}

/// A style that is nothing but a name and the stylesheet it belongs to.
#[allow(dead_code)]
fn named_style(identifier: &str, stylesheet: u64) -> Message {
    message(vec![nested(
        1,
        vec![string(2, identifier), reference(5, stylesheet)],
    )])
}

/// `TST.CellStyleArchive` — a cell's padding and its fill.
#[allow(dead_code)]
fn cell_style(identifier: &str, stylesheet: u64) -> Message {
    message(vec![
        nested(1, vec![string(2, identifier), reference(5, stylesheet)]),
        nested(
            11,
            vec![
                bytes(1, Vec::new()),
                varint(3, 1),
                varint(8, 0),
                // The four insets Numbers gives a cell, in points.
                nested(
                    9,
                    vec![float(1, 4.0), float(2, 4.0), float(3, 4.0), float(4, 4.0)],
                ),
            ],
        ),
    ])
}

/// `TST.TableStyleArchive`.
#[allow(dead_code)]
const TYPE_TABLE_STYLE: u32 = 6003;
/// `TST.CellStyleArchive`.
#[allow(dead_code)]
const TYPE_CELL_STYLE: u32 = 6004;
/// `TSWP.ShapeStyleArchive`.
#[allow(dead_code)]
const TYPE_SHAPE_STYLE: u32 = 2025;

/// `TST.TableModelArchive.table_id` — an uppercase UUID, as Numbers writes it.
#[allow(dead_code)]
fn table_id() -> String {
    crate::metadata::uuid()
}

/// `TN.SheetArchive` — a name, what is on it, and how it is laid out.
///
/// The layout fields are the ones a sheet Numbers made carries, kept because a
/// sheet is a *page* as well as a container: field 7 is its zoom, 13 and 14 its
/// print margins, 22 the style it is drawn with.
#[allow(dead_code)]
fn sheet_archive(name: &str, table: u64, style: u64) -> Message {
    message(vec![
        string(1, name),
        reference(2, table),
        varint(3, 1),
        varint(5, 1),
        varint(6, 0),
        float(7, 0.72),
        varint(8, 0),
        varint(11, 0),
        varint(12, 1),
        float(13, 20.0),
        float(14, 20.0),
        varint(20, 0),
        varint(21, 0),
        reference(22, style),
        varint(23, 1),
        varint(24, 0),
    ])
}

/// `TN.SheetStyleArchive` — a white sheet.
#[allow(dead_code)]
fn sheet_style(stylesheet: u64) -> Message {
    message(vec![
        nested(
            1,
            vec![string(2, "sheet-0-sheetStyle"), reference(5, stylesheet)],
        ),
        varint(2, 2),
        nested(
            3,
            vec![nested(1, vec![nested(1, white())]), bytes(2, Vec::new())],
        ),
    ])
}

/// `TSP.Color`, white and opaque.
#[allow(dead_code)]
fn white() -> Vec<Field> {
    vec![
        varint(1, 1),
        float(3, 1.0),
        float(4, 1.0),
        float(5, 1.0),
        float(6, 1.0),
        varint(12, 1),
        float(13, 1.0),
    ]
}

/// `TN.SheetStyleArchive`.
#[allow(dead_code)]
const TYPE_SHEET_STYLE: u32 = 12050;

/// `TN.DocumentArchive` — object 1, and type 1.
///
/// Every reference here is one the reduction could not delete: a Numbers
/// document with no stylesheet, no theme, no `TSK` 205 or no calculation
/// engine is refused, where a Pages document needs none of the four.
#[allow(dead_code)]
fn numbers_document(sheet: u64, stylesheet: u64, support: u64, theme: u64, engine: u64) -> Message {
    message(vec![
        reference(1, sheet),
        reference(4, stylesheet),
        reference(5, support),
        reference(6, theme),
        nested(8, vec![nested(1, Vec::new()), reference(4, engine)]),
    ])
}

/// The theme, which for Numbers is little more than a name and a stylesheet.
#[allow(dead_code)]
fn numbers_theme(stylesheet: u64) -> Message {
    message(vec![nested(
        1,
        vec![string(3, "Blank"), reference(4, stylesheet)],
    )])
}

/// `TN.ThemeArchive`.
#[allow(dead_code)]
const TYPE_NUMBERS_THEME: u32 = 12009;
/// `TSK` 205 — empty in every document measured, and required all the same.
#[allow(dead_code)]
const TYPE_DOCUMENT_SUPPORT: u32 = 205;
/// `TSCE.CalculationEngineArchive`.
#[allow(dead_code)]
const TYPE_CALCULATION_ENGINE: u32 = 4000;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::table::CellValue;
    use crate::Document;

    fn document(blueprint: Blueprint) -> Document {
        let mut document =
            Document::from_package(blueprint.finish()).expect("a blueprint has to parse");
        document.declare_external_references();
        document
    }

    /// The Pages blueprint, from the inside: the objects the format fixes are
    /// where the format fixes them, and nothing dangles.
    #[test]
    fn a_pages_blueprint_is_a_document() {
        let doc = document(pages(Paper::A4));
        assert_eq!(doc.object(ROOT).expect("root").1.message_type(), 10000);
        assert!(doc.problems().is_empty(), "{:?}", doc.problems());
        assert!(doc.undeclared_references().is_empty());
    }

    /// The Numbers blueprint is not offered by [`crate::Document::new`], so
    /// this is what stands in for it: everything about the table that can be
    /// checked without Numbers in the room.
    ///
    /// It is worth keeping sharp. When the app finally accepts one of these,
    /// the difference between this and that is the answer.
    #[test]
    fn a_numbers_blueprint_holds_a_table_that_can_be_written_into() {
        let mut doc = document(numbers("Sheet 1", "Budget", 12, 3));
        assert!(doc.problems().is_empty(), "{:?}", doc.problems());
        assert!(doc.undeclared_references().is_empty());

        let table = doc.table("Budget").expect("no table");
        assert_eq!((table.rows, table.columns), (12, 3));
        assert_eq!(table.cells().len(), 0, "a new table has no cells");

        // Every cell of it is writable, which is the reason the tile carries a
        // row entry per row rather than none: `set_cell` finds a cell through
        // its row's entry, and Numbers writes no entry for an empty row.
        doc.set_cell("Budget", 0, 0, CellValue::Text("Region".into()))
            .expect("A1");
        doc.set_cell(
            "Budget",
            11,
            2,
            CellValue::Number(crate::table::Decimal::parse("42").expect("a number")),
        )
        .expect("C12");
        let table = doc.table("Budget").expect("no table");
        assert_eq!(table.value(0, 0).to_text(), "Region");
        assert_eq!(table.value(11, 2).to_text(), "42");
        assert!(doc.problems().is_empty(), "{:?}", doc.problems());
    }

    /// A table wider than one tile gets more than one, and the rows in the
    /// second tile are writable too.
    #[test]
    fn a_table_taller_than_a_tile_gets_more_than_one() {
        let mut doc = document(numbers("Sheet 1", "Long", 300, 2));
        let tiles = doc
            .objects()
            .filter(|(_, object)| object.message_type() == TYPE_TILE)
            .count();
        assert_eq!(tiles, 2, "300 rows is two tiles of 256");
        doc.set_cell("Long", 299, 0, CellValue::Text("last".into()))
            .expect("the last row");
        assert_eq!(doc.table("Long").unwrap().value(299, 0).to_text(), "last");
    }

    /// The version number in a component index is the app's, not this crate's.
    #[test]
    fn the_component_generation_is_the_apps_own() {
        assert_eq!(component_generation(Kind::Pages), 835);
        assert_eq!(component_generation(Kind::Numbers), 537);
        assert_eq!(component_generation(Kind::Keynote), 2386);
    }
}
