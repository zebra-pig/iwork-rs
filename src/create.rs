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

/// `TSP.ComponentInfo.preferred_locator_version`, field 12.
///
/// 835 in every component of every document here, Pages, Numbers and Keynote
/// alike. What it counts is not known; what it is is the same number.
const COMPONENT_GENERATION: u64 = 835;

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

            info.push(varint(10, 0));
            info.push(varint(12, COMPONENT_GENERATION));
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
        fields.push(varint(8, COMPONENT_GENERATION));

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
