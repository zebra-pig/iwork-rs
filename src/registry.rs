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

pub struct Entry {
    pub message_type: u32,
    pub name: &'static str,
    pub confidence: Confidence,
}

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
    },
    Entry {
        message_type: 11014,
        name: "TSP.AnnotationAuthorArchive",
        confidence: Inferred,
    },
    Entry {
        message_type: 11015,
        name: "TSP.AnnotationAuthorStorageArchive",
        confidence: Inferred,
    },
    // -- word processing -----------------------------------------------------
    Entry {
        message_type: 2001,
        name: "TSWP.StorageArchive",
        confidence: Confirmed,
    },
    Entry {
        message_type: 2011,
        name: "TSWP.SelectionArchive",
        confidence: Inferred,
    },
    Entry {
        message_type: 2021,
        name: "TSWP.ParagraphStyleArchive",
        confidence: Inferred,
    },
    Entry {
        message_type: 2022,
        name: "TSWP.CharacterStyleArchive",
        confidence: Inferred,
    },
    Entry {
        message_type: 2023,
        name: "TSWP.ListStyleArchive",
        confidence: Inferred,
    },
    Entry {
        message_type: 2025,
        name: "TSWP.ShapeStyleArchive",
        confidence: Inferred,
    },
    Entry {
        message_type: 2026,
        name: "TSWP.TextStyleArchive",
        confidence: Inferred,
    },
    // -- drawables -----------------------------------------------------------
    Entry {
        message_type: 3005,
        name: "TSD.ImageArchive",
        confidence: Confirmed,
    },
    Entry {
        message_type: 3006,
        name: "TSD.MaskArchive",
        confidence: Inferred,
    },
    Entry {
        message_type: 3008,
        name: "TSD.GroupArchive",
        confidence: Inferred,
    },
    Entry {
        message_type: 3016,
        name: "TSD.ThemeArchive",
        confidence: Inferred,
    },
    Entry {
        message_type: 3047,
        name: "TSD.DrawableContentArchive",
        confidence: Inferred,
    },
    // -- stylesheets ---------------------------------------------------------
    Entry {
        message_type: 5020,
        name: "TSS.StylesheetArchive",
        confidence: Inferred,
    },
    Entry {
        message_type: 5026,
        name: "TSS.ThemeArchive",
        confidence: Inferred,
    },
    Entry {
        message_type: 5028,
        name: "TSS.StyleArchive",
        confidence: Inferred,
    },
    // -- tables (Numbers, and tables embedded in Pages/Keynote) --------------
    Entry {
        message_type: 6001,
        name: "TST.TableModelArchive",
        confidence: Unverified,
    },
    Entry {
        message_type: 6002,
        name: "TST.TileArchive",
        confidence: Unverified,
    },
    Entry {
        message_type: 6004,
        name: "TST.TableDataList",
        confidence: Inferred,
    },
    Entry {
        message_type: 6005,
        name: "TST.HeaderStorageBucket",
        confidence: Inferred,
    },
    // -- calculation engine (Numbers) ---------------------------------------
    Entry {
        message_type: 4008,
        name: "TSCE.CalculationEngineArchive",
        confidence: Inferred,
    },
    Entry {
        message_type: 4009,
        name: "TSCE.FormulaArchive",
        confidence: Unverified,
    },
    // -- Pages ---------------------------------------------------------------
    Entry {
        message_type: 10000,
        name: "TP.DocumentArchive",
        confidence: Confirmed,
    },
    Entry {
        message_type: 10011,
        name: "TP.SectionArchive",
        confidence: Inferred,
    },
    Entry {
        message_type: 10012,
        name: "TP.ThemeArchive",
        confidence: Inferred,
    },
    Entry {
        message_type: 10015,
        name: "TP.SettingsArchive",
        confidence: Inferred,
    },
    Entry {
        message_type: 10016,
        name: "TP.BodyStorageArchive",
        confidence: Inferred,
    },
    Entry {
        message_type: 10143,
        name: "TP.PageLayoutArchive",
        confidence: Inferred,
    },
    // -- Numbers -------------------------------------------------------------
    // In Numbers the root object (identifier 1) is type 1, not 10000; this is
    // what distinguishes a Numbers object graph from a Pages one.
    Entry {
        message_type: 1,
        name: "TN.DocumentArchive",
        confidence: Confirmed,
    },
    Entry {
        message_type: 2,
        name: "TN.SheetArchive",
        confidence: Unverified,
    },
    // -- Keynote -------------------------------------------------------------
    // No Keynote sample was available when this table was written. Keynote uses
    // the same container, the same IWA framing and the same TSWP/TSD/TSS
    // objects, so everything above applies; the KN.* document-level types are
    // deliberately absent rather than guessed. See README, "Keynote status".
];

pub fn lookup(message_type: u32) -> Option<&'static Entry> {
    ENTRIES.iter().find(|e| e.message_type == message_type)
}

/// Human-readable label for a message type, e.g.
/// `"TSWP.StorageArchive"` or `"TSWP #2042"` when the type is unknown.
pub fn describe(message_type: u32) -> String {
    match lookup(message_type) {
        Some(entry) => match entry.confidence {
            Confirmed => entry.name.to_string(),
            Inferred => format!("{}?", entry.name),
            Unverified => format!("{}??", entry.name),
        },
        None => format!("{} #{message_type}", framework(message_type)),
    }
}
