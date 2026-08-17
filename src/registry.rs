//! Message-type registry.
//!
//! Every object payload in an iWork document is an unlabelled protobuf message;
//! the only thing identifying its schema is `MessageInfo.type`, an index into a
//! registry that lives inside the iWork binaries and is not published.
//!
//! This table was built by observation, so each entry carries how much it can
//! be trusted. Nothing in this crate *depends* on the table — it is used for
//! human-readable output only. Parsing and rewriting work at the wire level and
//! are unaffected by a wrong or missing name.

/// How much a registry entry is worth.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Confidence {
    /// The payload was decoded and behaves as the name says — e.g. type 2001
    /// really does hold the UTF-8 text of the document.
    Confirmed,
    /// Name inferred from position in the object graph, payload shape and the
    /// framework's number range. Plausible, not proven.
    Inferred,
    /// Carried over from public prior art (numbers-parser, keynote-parser and
    /// friends) without a local sample to check it against.
    Unverified,
}

/// Which app an entry applies to.
///
/// Most message types mean the same thing everywhere — the frameworks are
/// shared, and `TSWP.StorageArchive` is 2001 in all three apps. The app-level
/// archives are not: **Numbers and Keynote both number their document archive
/// 1**, so a table keyed on the number alone has to be wrong for one of them.
/// A Keynote deck was reported as holding a `TN.DocumentArchive` until this
/// existed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum App {
    /// The message type means the same thing in all three apps.
    Any,
    Pages,
    Numbers,
    Keynote,
}

impl App {
    /// The app-specific entry a document of this kind should prefer.
    fn of(kind: Kind) -> Option<App> {
        match kind {
            Kind::Pages => Some(App::Pages),
            Kind::Numbers => Some(App::Numbers),
            Kind::Keynote => Some(App::Keynote),
            Kind::Unknown => None,
        }
    }
}

pub struct Entry {
    pub message_type: u32,
    pub name: &'static str,
    pub confidence: Confidence,
    /// App this entry is for. Entries with a specific app are only chosen for
    /// documents of that kind.
    pub app: App,
}

use crate::document::Kind;
use App::{Keynote as InKeynote, Numbers as InNumbers, Pages as InPages};
use Confidence::*;

/// Framework prefixes, by number range. These ranges are stable across the
/// three apps and are the most reliable thing in this module.
pub fn framework(message_type: u32) -> &'static str {
    match message_type {
        1..=999 => "TSP/TSK",
        1000..=1999 => "TSA",
        2000..=2999 => "TSWP",
        3000..=3999 => "TSD",
        4000..=4999 => "TSCE",
        5000..=5999 => "TSS",
        6000..=6999 => "TST",
        10000..=10999 => "app",
        11000..=11999 => "TSP package",
        12000..=12999 => "TSA/chart",
        _ => "?",
    }
}

const ENTRIES: &[Entry] = &[
    // -- package level -------------------------------------------------------
    Entry {
        message_type: 11006,
        name: "TSP.PackageMetadata",
        confidence: Confirmed,
        app: App::Any,
    },
    Entry {
        message_type: 11014,
        name: "TSP.AnnotationAuthorArchive",
        confidence: Inferred,
        app: App::Any,
    },
    Entry {
        message_type: 11015,
        name: "TSP.AnnotationAuthorStorageArchive",
        confidence: Inferred,
        app: App::Any,
    },
    // -- word processing -----------------------------------------------------
    Entry {
        message_type: 2001,
        name: "TSWP.StorageArchive",
        confidence: Confirmed,
        app: App::Any,
    },
    Entry {
        message_type: 2011,
        name: "TSWP.SelectionArchive",
        confidence: Inferred,
        app: App::Any,
    },
    // 2021 and 2022 are the other way round from what public prior art says.
    // Settled by asking the styles: across six documents, every type 2021 has an
    // internal identifier of the form `character-style-…` (12 of them) and every
    // type 2022 `…-paragraphstyle-…` (229). A type 2022 also carries paragraph
    // properties — alignment, indents, tab stops — which a character style has
    // no use for.
    Entry {
        message_type: 2021,
        name: "TSWP.CharacterStyleArchive",
        confidence: Confirmed,
        app: App::Any,
    },
    Entry {
        message_type: 2022,
        name: "TSWP.ParagraphStyleArchive",
        confidence: Confirmed,
        app: App::Any,
    },
    Entry {
        message_type: 2023,
        name: "TSWP.ListStyleArchive",
        confidence: Confirmed,
        app: App::Any,
    },
    Entry {
        message_type: 2025,
        name: "TSWP.ShapeStyleArchive",
        confidence: Inferred,
        app: App::Any,
    },
    Entry {
        message_type: 2026,
        name: "TSWP.TextStyleArchive",
        confidence: Inferred,
        app: App::Any,
    },
    // -- drawables -----------------------------------------------------------
    Entry {
        message_type: 3005,
        name: "TSD.ImageArchive",
        confidence: Confirmed,
        app: App::Any,
    },
    Entry {
        message_type: 3006,
        name: "TSD.MaskArchive",
        confidence: Inferred,
        app: App::Any,
    },
    Entry {
        message_type: 3008,
        name: "TSD.GroupArchive",
        confidence: Inferred,
        app: App::Any,
    },
    Entry {
        message_type: 3016,
        name: "TSD.ThemeArchive",
        confidence: Inferred,
        app: App::Any,
    },
    Entry {
        message_type: 3047,
        name: "TSD.DrawableContentArchive",
        confidence: Inferred,
        app: App::Any,
    },
    // -- stylesheets ---------------------------------------------------------
    Entry {
        message_type: 5020,
        name: "TSS.StylesheetArchive",
        confidence: Inferred,
        app: App::Any,
    },
    Entry {
        message_type: 5026,
        name: "TSS.ThemeArchive",
        confidence: Inferred,
        app: App::Any,
    },
    Entry {
        message_type: 5028,
        name: "TSS.StyleArchive",
        confidence: Inferred,
        app: App::Any,
    },
    // -- tables (Numbers, and tables embedded in Pages/Keynote) --------------
    Entry {
        message_type: 6001,
        name: "TST.TableModelArchive",
        confidence: Unverified,
        app: App::Any,
    },
    Entry {
        message_type: 6002,
        name: "TST.TileArchive",
        confidence: Unverified,
        app: App::Any,
    },
    Entry {
        message_type: 6004,
        name: "TST.TableDataList",
        confidence: Inferred,
        app: App::Any,
    },
    Entry {
        message_type: 6005,
        name: "TST.HeaderStorageBucket",
        confidence: Inferred,
        app: App::Any,
    },
    // -- calculation engine (Numbers) ---------------------------------------
    Entry {
        message_type: 4008,
        name: "TSCE.CalculationEngineArchive",
        confidence: Inferred,
        app: App::Any,
    },
    Entry {
        message_type: 4009,
        name: "TSCE.FormulaArchive",
        confidence: Unverified,
        app: App::Any,
    },
    // The stylesheet a document's styles are actually listed in. Not in the TSS
    // range, despite the name — verified in the Pages and Keynote samples, where
    // it holds hundreds of style references (see `style::clone_registrations`).
    Entry {
        message_type: 401,
        name: "TSS.StylesheetArchive",
        confidence: Confirmed,
        app: App::Any,
    },
    // -- Pages ---------------------------------------------------------------
    Entry {
        message_type: 10000,
        name: "TP.DocumentArchive",
        confidence: Confirmed,
        app: InPages,
    },
    Entry {
        message_type: 10011,
        name: "TP.SectionArchive",
        confidence: Inferred,
        app: InPages,
    },
    Entry {
        message_type: 10012,
        name: "TP.ThemeArchive",
        confidence: Inferred,
        app: InPages,
    },
    Entry {
        message_type: 10015,
        name: "TP.SettingsArchive",
        confidence: Inferred,
        app: InPages,
    },
    Entry {
        message_type: 10016,
        name: "TP.BodyStorageArchive",
        confidence: Inferred,
        app: InPages,
    },
    Entry {
        message_type: 10143,
        name: "TP.PageLayoutArchive",
        confidence: Inferred,
        app: InPages,
    },
    // -- Numbers -------------------------------------------------------------
    // The root object (identifier 1) is type 1, not 10000. That distinguishes a
    // Numbers object graph from a Pages one — but *not* from a Keynote one,
    // which numbers its own document archive 1 as well. See `App`.
    Entry {
        message_type: 1,
        name: "TN.DocumentArchive",
        confidence: Confirmed,
        app: InNumbers,
    },
    Entry {
        message_type: 2,
        name: "TN.SheetArchive",
        confidence: Unverified,
        app: InNumbers,
    },
    // -- Keynote -------------------------------------------------------------
    // Derived from one deck: 1204 objects, 19 masters, 5 slides, 30 streams.
    // The app-level numbering starts at 1 and collides with Numbers throughout,
    // which is what `App` exists to keep straight.
    Entry {
        message_type: 1,
        name: "KN.DocumentArchive",
        confidence: Confirmed,
        app: InKeynote,
    },
    // Object 1 field 2 points at it, and it is the presentation: field 2 the
    // theme, field 3 the slide tree, field 4 the slide size (1920 × 1080 in the
    // sample), field 5 the document stylesheet.
    Entry {
        message_type: 2,
        name: "KN.ShowArchive",
        confidence: Confirmed,
        app: InKeynote,
    },
    // One per slide, in the show's slide tree; field 2 points at the slide.
    Entry {
        message_type: 4,
        name: "KN.SlideNodeArchive",
        confidence: Confirmed,
        app: InKeynote,
    },
    // The slide. Carries its transition — `{"Transition", "none", 1.0, 0.5}` —
    // and a reference to the master it is built on, which is what makes this a
    // presentation object rather than anything shared with the other two apps.
    Entry {
        message_type: 5,
        name: "KN.SlideArchive",
        confidence: Confirmed,
        app: InKeynote,
    },
    // One per `Index/TemplateSlide-*.iwa`, and what a slide's field 1 points at.
    // `KN.MasterSlideArchive` was the obvious guess and is not a message that
    // exists; the role is clear from the document, the name is not, so it does
    // not get one.
    Entry {
        message_type: 9,
        name: "KN slide-template archive",
        confidence: Unverified,
        app: InKeynote,
    },
    // Names itself: field 1.3 is the theme name, `"58_Startup_Simple_PM"`.
    Entry {
        message_type: 10,
        name: "KN.ThemeArchive",
        confidence: Confirmed,
        app: InKeynote,
    },
    // Seven of them, every one identifying itself as `dropcap-style-N` or
    // `drop-cap-style-default` in the TSS base at field 1.2. The archive is a
    // TSWP one rather than a KN one, so the number being in the app range is the
    // app registering a framework message rather than defining its own.
    Entry {
        message_type: 10024,
        name: "TSWP.DropCapStyleArchive",
        confidence: Inferred,
        app: InKeynote,
    },
];

/// The entry for a message type in a document of `kind`.
///
/// An app-specific entry wins over a shared one, so Keynote's type 1 resolves
/// to `KN.DocumentArchive` and Numbers' to `TN.DocumentArchive`.
pub fn lookup_in(kind: Kind, message_type: u32) -> Option<&'static Entry> {
    let matching = |app: App| {
        ENTRIES
            .iter()
            .find(|e| e.message_type == message_type && e.app == app)
    };
    App::of(kind)
        .and_then(matching)
        .or_else(|| matching(App::Any))
}

/// The entry for a message type when the document's kind is not known.
///
/// Only entries that mean the same thing in all three apps can be resolved this
/// way; an app-specific number resolves to nothing rather than to whichever app
/// happens to be listed first.
pub fn lookup(message_type: u32) -> Option<&'static Entry> {
    ENTRIES
        .iter()
        .find(|e| e.message_type == message_type && e.app == App::Any)
}

/// Human-readable label for a message type, e.g.
/// `"TSWP.StorageArchive"` or `"TSWP #2042"` when the type is unknown.
pub fn describe(message_type: u32) -> String {
    label(lookup(message_type), message_type)
}

/// Human-readable label for a message type in a document of a known kind.
///
/// Prefer this: without a kind, every app-level archive is unnameable.
pub fn describe_in(kind: Kind, message_type: u32) -> String {
    label(lookup_in(kind, message_type), message_type)
}

fn label(entry: Option<&'static Entry>, message_type: u32) -> String {
    match entry {
        Some(entry) => match entry.confidence {
            Confirmed => entry.name.to_string(),
            Inferred => format!("{}?", entry.name),
            Unverified => format!("{}??", entry.name),
        },
        None => format!("{} #{message_type}", framework(message_type)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The reason `App` exists: one number, two apps, two meanings.
    #[test]
    fn type_one_depends_on_the_app() {
        assert_eq!(describe_in(Kind::Numbers, 1), "TN.DocumentArchive");
        assert_eq!(describe_in(Kind::Keynote, 1), "KN.DocumentArchive");
        // With no kind to go on, saying nothing beats saying the wrong one.
        assert_eq!(describe(1), "TSP/TSK #1");
        assert_eq!(describe_in(Kind::Unknown, 1), "TSP/TSK #1");
    }

    #[test]
    fn shared_types_resolve_for_every_app() {
        for kind in [Kind::Pages, Kind::Numbers, Kind::Keynote, Kind::Unknown] {
            assert_eq!(describe_in(kind, 2001), "TSWP.StorageArchive");
        }
        assert_eq!(describe(2001), "TSWP.StorageArchive");
    }

    #[test]
    fn confidence_shows_in_the_label() {
        assert_eq!(describe_in(Kind::Keynote, 5), "KN.SlideArchive");
        assert_eq!(describe_in(Kind::Keynote, 9), "KN slide-template archive??");
        assert_eq!(describe_in(Kind::Numbers, 2), "TN.SheetArchive??");
        assert_eq!(describe(999999), "? #999999");
    }

    /// A Pages type must not be offered for a Keynote document, and vice versa.
    #[test]
    fn app_specific_entries_do_not_leak_across_apps() {
        assert_eq!(describe_in(Kind::Keynote, 10000), "app #10000");
        assert_eq!(describe_in(Kind::Pages, 10000), "TP.DocumentArchive");
        assert_eq!(describe_in(Kind::Pages, 10024), "app #10024");
        assert_eq!(
            describe_in(Kind::Keynote, 10024),
            "TSWP.DropCapStyleArchive?"
        );
    }
}
