//! A Pages document with real typography, written from Rust — no Apple
//! software anywhere in the picture.
//!
//! ```text
//! cargo run --example report -- Report.pages
//! open Report.pages
//! ```
//!
//! The shape worth noticing is that **a blank Pages document has almost no
//! stylesheet**: one paragraph style called `Body`, a list style called
//! `None`, and the seven table styles. There is no Title, no Heading, no
//! bullet, not one character style. So every style used below is *made* —
//! copied from `Body` with [`Document::create_text_style`] and then given its
//! own font, size, colour and spacing by path. That is the rule the rest of
//! this crate follows for whole documents: copy something that works, change
//! the fields that must differ, invent nothing.
//!
//! What that buys, and what it does not:
//!
//! * fonts, sizes and faces are **drawn** — `scripts/paragraph-oracle.sh` asks
//!   Pages for the size and font of every paragraph of this document and gets
//!   back the kicker at 9 pt, the title at 27, the body in Charter at 11;
//! * colour, alignment, indents, space before and after, tracking and small
//!   caps are a `set_text_style_property` away and are written the way a
//!   document Pages made carries them. Colour needed one more thing than the
//!   others: a style keeps its text colour in up to four places and the app
//!   paints with the *fill*, so `set_text_style_color` writes all of them;
//! * **line spacing is not**, because `Body` has no `12.13` container to put
//!   the multiple in and this crate refuses to invent one;
//! * **inline emphasis is not**, because character styles are made by copying
//!   and a blank document has none to copy;
//! * **bullets are not**, for the same reason — the only list style is `None`.
//!
//! For a long time none of it was drawn at all, and the reason was not in the
//! styles. A storage's paragraph-style table has an entry for *every*
//! paragraph; written run-length, as it was, Pages throws it away and sets the
//! whole document in its default. See `style::apply_to_paragraphs`.
//!
//! The dashes in "What we are doing about it" are therefore typed characters
//! in a hanging-indent paragraph style, not a Pages list.

use iwork::pb::Value;
use iwork::style::property as prop;
use iwork::{Document, Kind};

/// Points, as the format stores them.
/// A style to make: its name, its text colour, and the properties that differ
/// from `Body` as (path, value) pairs.
type Recipe<'a> = (&'a str, u32, &'a [(&'a [u32], Value)]);

fn pt(x: f32) -> Value {
    Value::Fixed32(x.to_le_bytes())
}

fn on() -> Value {
    Value::Varint(1)
}

/// `#RRGGBB` as the four channels `set_text_style_color` wants.
fn rgb(hex: u32) -> (f32, f32, f32) {
    let c = |shift: u32| ((hex >> shift) & 0xff) as f32 / 255.0;
    (c(16), c(8), c(0))
}

const INK: u32 = 0x1B2A41; // headings and title
const BODY_INK: u32 = 0x2E2E2E; // running text
const ACCENT: u32 = 0x9E2B25; // the kicker and the section numbers
const QUIET: u32 = 0x7A7F87; // deck, captions, colophon

/// One paragraph of the report, and the style it is set in.
struct Para(&'static str, &'static str);

const REPORT: &[Para] = &[
    Para("kicker", "Betriebsbericht · Q3 2026"),
    Para(
        "title",
        "Rolling stock availability on the Rhaetian network",
    ),
    Para(
        "deck",
        "Three quarters of measured data, one fleet-wide conclusion: the \
         Allegra sets are carrying the timetable, and the Ge 4/4 III are the \
         reason the reserve keeps being called on.",
    ),
    Para("heading", "Where the quarter stands"),
    Para(
        "lead",
        "Availability across the whole fleet finished the quarter at 91.4 per \
         cent, four tenths of a point below the same quarter last year and \
         the first year-on-year decline since 2023.",
    ),
    Para(
        "body",
        "The decline is not evenly spread. The ABe 8/12 Allegra sets held \
         94.8 per cent and have now run eleven consecutive quarters above \
         their target. The Ge 4/4 III locomotives, which are the older half \
         of the mainline fleet, finished at 86.2 — their weakest quarter on \
         record — and account for the entire fleet-wide shortfall on their \
         own.",
    ),
    Para(
        "body",
        "Unplanned workshop time at Landquart rose from 1 840 hours to 2 310, \
         and CHF 1.24 million of the CHF 4.9 million maintenance budget went \
         on work that was not in the plan at the start of the quarter. Nothing \
         in the figures suggests a single cause; the pattern is the ordinary \
         one of a class approaching its second heavy overhaul.",
    ),
    Para("heading", "What the reserve is costing"),
    Para(
        "body",
        "The reserve was called on 47 times, against 29 in the comparable \
         quarter. Each call costs roughly CHF 3 100 in crew and pathing, so \
         the additional eighteen calls are worth about CHF 56 000 — small \
         against the budget, and a poor guide to the real cost, which is that \
         a reserve set standing at Samedan is a set not available at Chur.",
    ),
    Para(
        "pull",
        "A reserve that is used every other day is not a reserve. It is an \
         undeclared part of the timetable.",
    ),
    Para("heading", "What we are doing about it"),
    Para(
        "item",
        "—  Bringing the Ge 4/4 III overhaul programme forward by two \
         quarters, beginning with 651 and 652 in January.",
    ),
    Para(
        "item",
        "—  Fitting bogie temperature telemetry to the whole class, so that \
         the failures now found at Landquart are found at Chur instead.",
    ),
    Para(
        "item",
        "—  Holding the Allegra maintenance interval where it is. Nothing in \
         three quarters of data argues for changing what is working.",
    ),
    Para("heading", "What we are asking for"),
    Para(
        "body",
        "CHF 2.1 million drawn forward from the 2027 overhaul budget into the \
         first half of 2026, and a decision by the end of November — the \
         Landquart works cannot hold two sets in January if the slot is \
         booked in December.",
    ),
    Para(
        "colophon",
        "Prepared by the Rolling Stock Office, Chur · Figures are provisional \
         until the year-end audit.",
    ),
];

fn main() -> Result<(), iwork::Error> {
    let out = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "Report.pages".to_string());

    let mut doc = Document::new(Kind::Pages)?;
    let body = doc
        .text_styles()
        .into_iter()
        .find(|s| s.name.as_deref() == Some("Body"))
        .expect("a blank Pages document has a Body style")
        .identifier;

    // Every style is a copy of Body with the fields that must differ changed.
    // `properties` is (path, value); the colour goes in separately because a
    // style keeps its text colour in up to four places and Pages expects them
    // to agree — `set_text_style_color` writes every one the style has.
    let recipes: &[Recipe] = &[
        (
            "kicker",
            ACCENT,
            &[
                (prop::FONT_NAME, font("AvenirNext-DemiBold")),
                (prop::FONT_SIZE, pt(9.0)),
                (prop::CAPITALISATION, Value::Varint(1)),
                (prop::TRACKING, pt(0.12)),
                (prop::SPACE_AFTER, pt(6.0)),
                (prop::KEEP_WITH_NEXT, on()),
            ],
        ),
        (
            "title",
            INK,
            &[
                (prop::FONT_NAME, font("AvenirNext-Bold")),
                (prop::FONT_SIZE, pt(27.0)),
                (prop::TRACKING, pt(-0.015)),
                (prop::SPACE_AFTER, pt(10.0)),
                (prop::KEEP_WITH_NEXT, on()),
            ],
        ),
        (
            "deck",
            QUIET,
            &[
                (prop::FONT_NAME, font("HelveticaNeue-Light")),
                (prop::FONT_SIZE, pt(13.0)),
                (prop::SPACE_AFTER, pt(26.0)),
                (prop::RIGHT_INDENT, pt(48.0)),
            ],
        ),
        (
            "heading",
            INK,
            &[
                (prop::FONT_NAME, font("AvenirNext-DemiBold")),
                (prop::FONT_SIZE, pt(13.0)),
                (prop::SPACE_BEFORE, pt(22.0)),
                (prop::SPACE_AFTER, pt(7.0)),
                (prop::KEEP_WITH_NEXT, on()),
                (prop::OUTLINE_LEVEL, Value::Varint(1)),
            ],
        ),
        (
            "lead",
            BODY_INK,
            &[
                (prop::FONT_NAME, font("Charter-Roman")),
                (prop::FONT_SIZE, pt(12.5)),
                (prop::SPACE_AFTER, pt(10.0)),
                (prop::WIDOW_CONTROL, on()),
            ],
        ),
        (
            "body",
            BODY_INK,
            &[
                (prop::FONT_NAME, font("Charter-Roman")),
                (prop::FONT_SIZE, pt(11.0)),
                (prop::SPACE_AFTER, pt(10.0)),
                (prop::HYPHENATE, on()),
                (prop::WIDOW_CONTROL, on()),
            ],
        ),
        (
            "pull",
            ACCENT,
            &[
                (prop::FONT_NAME, font("Charter-Italic")),
                (prop::FONT_SIZE, pt(15.0)),
                (prop::LEFT_INDENT, pt(36.0)),
                (prop::RIGHT_INDENT, pt(36.0)),
                (prop::SPACE_BEFORE, pt(14.0)),
                (prop::SPACE_AFTER, pt(18.0)),
                (prop::WIDOW_CONTROL, on()),
            ],
        ),
        (
            // No bullets: a blank document's only list style is `None`, and a
            // list style is made by copying one that already has bullets.
            // A hanging indent and a typed em dash is the honest substitute.
            "item",
            BODY_INK,
            &[
                (prop::FONT_NAME, font("Charter-Roman")),
                (prop::FONT_SIZE, pt(11.0)),
                (prop::LEFT_INDENT, pt(22.0)),
                (prop::FIRST_LINE_INDENT, pt(-22.0)),
                (prop::SPACE_AFTER, pt(7.0)),
                (prop::WIDOW_CONTROL, on()),
            ],
        ),
        (
            "colophon",
            QUIET,
            &[
                (prop::FONT_NAME, font("HelveticaNeue")),
                (prop::FONT_SIZE, pt(8.5)),
                (prop::SPACE_BEFORE, pt(28.0)),
                (prop::TRACKING, pt(0.02)),
            ],
        ),
    ];

    let mut styles = std::collections::HashMap::new();
    for (name, colour, properties) in recipes.iter() {
        let id = doc.add_text_style(&iwork::TextStyle::new(*name).based_on(body))?;
        let mut style = doc.text_style_mut(id)?;
        // What `TextLook` does not cover — spacing, tracking — goes in by its
        // path in the archive.
        for (path, value) in *properties {
            style.property(path, Some(value.clone()))?;
        }
        let (r, g, b) = rgb(*colour);
        style.look(&iwork::TextLook::new().colour(iwork::Color::rgb(r, g, b)))?;
        styles.insert(*name, id);
    }

    // The text first, then the styles over it: applying to a paragraph needs
    // the paragraph to exist.
    let storage = doc.body_storage().expect("a Pages document has a body");
    for Para(_, text) in REPORT {
        doc.text_mut(storage)?.append(text)?;
    }
    let ranges = doc.paragraph_ranges(storage)?;
    for (Para(style, _), range) in REPORT.iter().zip(ranges) {
        doc.text_mut(storage)?.style(range, styles[style])?;
    }

    doc.save(&out)?;
    println!("wrote {out}");
    Ok(())
}

fn font(name: &str) -> Value {
    Value::Bytes(name.as_bytes().to_vec())
}
