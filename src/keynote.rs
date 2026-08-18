//! `KN` — the Keynote show: slides, layouts, placeholders, notes, transitions.
//!
//! Only Keynote writes these archives, and the numbering is **app-scoped**: a
//! Keynote type 5 is `KN.SlideArchive` and a Numbers type 5 is something else
//! entirely. [`crate::registry`] resolves them per document kind; nothing here
//! may be used on a document [`crate::Document::kind`] does not call Keynote.
//!
//! ```text
//!  KN.DocumentArchive (1)
//!    └── show (2) ─── KN.ShowArchive (2)
//!          ├── theme (2) ──── KN.ThemeArchive (10)
//!          │                    ├── super.name — "21_BasicWhite"
//!          │                    └── templates (2): the slide layouts, in order,
//!          │                        each a KN.SlideNodeArchive
//!          ├── slideTree (3) — inline KN.SlideTreeArchive
//!          │                    └── slides (2): the deck, in order, each a
//!          │                        KN.SlideNodeArchive
//!          ├── size (4), stylesheet (5), slideNumbersVisible (6)
//!          └── soundtrack (17)
//!
//!  KN.SlideNodeArchive (4)      one per slide *and* one per layout
//!    ├── slide (2) ──────────── KN.SlideArchive (5)
//!    ├── isSkipped (4), hasNote (8), hasTransition (7)
//!    ├── thumbnails (16) / thumbnailSizes (10) / thumbnailsAreDirty (14)
//!    └── template_slide_id (29): the *layout's* UUID, not the node's
//!
//!  KN.SlideArchive (5)          a slide and a layout are the same archive
//!    ├── style (1), transition (4), template_slide (17), note (27)
//!    ├── titlePlaceholder (5), bodyPlaceholder (6),
//!    │   slideNumberPlaceholder (20), objectPlaceholder (30)
//!    ├── owned_drawables (7), drawables_z_order (42)
//!    ├── builds (2), buildChunks (43)
//!    └── name (10) — only a layout has one
//! ```
//!
//! ## A slide is a component
//!
//! Each slide is its own `Index/Slide*.iwa` stream and its own component, whose
//! identifier is the `KN.SlideArchive`'s own; each layout is an
//! `Index/TemplateSlide-*.iwa`. The slide's *node* lives in `Index/Document.iwa`
//! with the show. That split is what makes duplicating a slide a package
//! operation rather than an object one — see [`duplicate_slide`].
//!
//! ## What "showing" means
//!
//! Keynote's `title showing` and `body showing` are not flags. A placeholder is
//! shown when the slide **owns** it — when it appears in `owned_drawables` (7) —
//! and the reference at field 5 or 6 stays whatever the layout gave it either
//! way. That is measured, not assumed: over the six slides of `keynote-deck`
//! the app's `title showing` / `body showing` agree with membership of field 7
//! twelve times out of twelve, including the "Statement" slide whose title
//! placeholder still holds text the app does not draw.
//!
//! ## Slide numbers
//!
//! A skipped slide has no number. The app answers `slide number` with **-1** for
//! it, and numbers the rest 1, 2, 3 … skipping over it — so the number is a
//! function of the deck, not a field on the slide. [`Slide::number`] is `None`
//! for a skipped slide for exactly that reason.

use std::collections::{BTreeMap, BTreeSet};

use crate::pb::{decode_nested, Message, Value};
use crate::style::reference_target;
use crate::Error;

/// `KN.DocumentArchive`.
pub const TYPE_DOCUMENT: u32 = 1;
/// `KN.ShowArchive`.
pub const TYPE_SHOW: u32 = 2;
/// `KN.UIStateArchive`.
pub const TYPE_UI_STATE: u32 = 3;
/// `KN.SlideNodeArchive` — one per slide and one per layout.
pub const TYPE_SLIDE_NODE: u32 = 4;
/// `KN.SlideArchive` — a slide *or* a layout.
pub const TYPE_SLIDE: u32 = 5;
/// `KN.PlaceholderArchive`.
pub const TYPE_PLACEHOLDER: u32 = 7;
/// `KN.BuildArchive` — one animation of one drawable.
pub const TYPE_BUILD: u32 = 8;
/// `KN.SlideStyleArchive`.
pub const TYPE_SLIDE_STYLE: u32 = 9;
/// `KN.ThemeArchive`.
pub const TYPE_THEME: u32 = 10;
/// `KN.NoteArchive` — the presenter notes wrapper; field 1 is the storage.
pub const TYPE_NOTE: u32 = 15;
/// `KN.RecordingArchive` — a recorded presentation. Read and passed through,
/// never authored (ground rule 8).
pub const TYPE_RECORDING: u32 = 16;
/// `KN.ClassicStylesheetRecordArchive`.
pub const TYPE_CLASSIC_STYLESHEET_RECORD: u32 = 19;
/// `KN.Soundtrack`.
pub const TYPE_SOUNDTRACK: u32 = 21;
/// `KN.SlideNumberAttachmentArchive`.
pub const TYPE_SLIDE_NUMBER_ATTACHMENT: u32 = 22;
/// `KN.MotionBackgroundStyleArchive`.
pub const TYPE_MOTION_BACKGROUND_STYLE: u32 = 26;
/// `KN.BuildChunkArchive` — one stage of a build.
pub const TYPE_BUILD_CHUNK: u32 = 153;

/// Field numbers of `KN.DocumentArchive`.
pub mod document_field {
    pub const SHOW: u32 = 2;
    /// `TSA.DocumentArchive`. **Field 3, not 15 or 8** — see
    /// [`crate::metadata::super_field`].
    pub const SUPER: u32 = 3;
    pub const CUSTOM_FORMAT_LIST: u32 = 4;
}

/// Field numbers of `KN.ShowArchive`.
pub mod show_field {
    pub const UI_STATE: u32 = 1;
    pub const THEME: u32 = 2;
    /// An inline `KN.SlideTreeArchive`, not a reference.
    pub const SLIDE_TREE: u32 = 3;
    pub const SIZE: u32 = 4;
    pub const STYLESHEET: u32 = 5;
    pub const SLIDE_NUMBERS_VISIBLE: u32 = 6;
    pub const RECORDING: u32 = 7;
    pub const LOOP: u32 = 8;
    pub const MODE: u32 = 9;
    pub const AUTOPLAY_TRANSITION_DELAY: u32 = 10;
    pub const AUTOPLAY_BUILD_DELAY: u32 = 11;
    pub const IDLE_TIMER_ACTIVE: u32 = 15;
    pub const IDLE_TIMER_DELAY: u32 = 16;
    pub const SOUNDTRACK: u32 = 17;
    pub const PLAYS_ON_OPEN: u32 = 18;
    pub const SLIDE_LIST: u32 = 19;
}

/// Field numbers of `KN.SlideTreeArchive`, which sits inline in the show.
pub mod slide_tree_field {
    /// `rootSlideNode`, deprecated and absent from every deck here.
    pub const ROOT: u32 = 1;
    /// The deck, in order. **This repeated field is the slide order** — see
    /// [`move_slide`].
    pub const SLIDES: u32 = 2;
}

/// Field numbers of `KN.SlideNodeArchive`.
pub mod node_field {
    pub const CHILDREN: u32 = 1;
    pub const SLIDE: u32 = 2;
    /// `isSkipped` — the whole of "skip this slide".
    pub const SKIPPED: u32 = 4;
    pub const HAS_BUILDS: u32 = 6;
    pub const HAS_TRANSITION: u32 = 7;
    pub const HAS_NOTE: u32 = 8;
    pub const THUMBNAIL_SIZES: u32 = 10;
    pub const COPY_FROM_SLIDE: u32 = 12;
    pub const THUMBNAILS_DIRTY: u32 = 14;
    pub const BUILD_EVENT_COUNT: u32 = 15;
    pub const THUMBNAILS: u32 = 16;
    pub const SLIDE_NUMBER_VISIBLE: u32 = 18;
    pub const HAS_EXPLICIT_BUILDS: u32 = 20;
    pub const DEPTH: u32 = 21;
    /// `template_slide_id` — a `TSP.UUID`, and the *layout's*, so two slides on
    /// one layout carry the same value.
    pub const TEMPLATE_SLIDE_ID: u32 = 29;
}

/// Field numbers of `KN.SlideArchive`.
pub mod slide_field {
    pub const STYLE: u32 = 1;
    pub const BUILDS: u32 = 2;
    pub const TRANSITION: u32 = 4;
    pub const TITLE_PLACEHOLDER: u32 = 5;
    pub const BODY_PLACEHOLDER: u32 = 6;
    /// `owned_drawables` — and, as measured, what "showing" means.
    pub const OWNED_DRAWABLES: u32 = 7;
    pub const NAME: u32 = 10;
    pub const TEMPLATE_SLIDE: u32 = 17;
    pub const IN_DOCUMENT: u32 = 19;
    pub const SLIDE_NUMBER_PLACEHOLDER: u32 = 20;
    pub const NOTE: u32 = 27;
    pub const SAGE_TAG_MAP: u32 = 28;
    pub const OBJECT_PLACEHOLDER: u32 = 30;
    pub const BODY_PARAGRAPH_STYLES: u32 = 31;
    pub const BODY_LIST_STYLES: u32 = 35;
    pub const GUIDE_STORAGE: u32 = 36;
    pub const DRAWABLES_Z_ORDER: u32 = 42;
    pub const BUILD_CHUNKS: u32 = 43;
    pub const INSTRUCTIONAL_TEXT: u32 = 45;
}

/// Field numbers of `KN.ThemeArchive`.
pub mod theme_field {
    /// `TSS.ThemeArchive`; its field 3 is the theme's stored name.
    pub const SUPER: u32 = 1;
    /// The slide layouts, in the order the app lists them, each a node.
    pub const TEMPLATES: u32 = 2;
    pub const UUID: u32 = 3;
    pub const DEFAULT_TEMPLATE_NODE: u32 = 5;
    pub const LIVE_VIDEO_SOURCES: u32 = 9;
    pub const MOTION_BACKGROUND_PRESETS: u32 = 10;
    /// Inside the `TSS.ThemeArchive` super: the theme's stored name.
    pub const SUPER_NAME: u32 = 3;
    /// Inside the super: the document stylesheet.
    pub const SUPER_STYLESHEET: u32 = 4;
}

/// Field numbers of `KN.PlaceholderArchive`.
pub mod placeholder_field {
    /// `TSWP.ShapeInfoArchive`.
    pub const SUPER: u32 = 1;
    pub const KIND: u32 = 2;
}

/// The chain from a slide to the four numbers a transition is made of:
/// `KN.TransitionArchive.attributes (2)` →
/// `KN.TransitionAttributesArchive.animationAttributes (8)` →
/// `KN.AnimationAttributesArchive`.
pub mod transition_field {
    pub const ATTRIBUTES: u32 = 2;
    pub const ANIMATION_ATTRIBUTES: u32 = 8;
    pub const ANIMATION_TYPE: u32 = 1;
    pub const EFFECT: u32 = 2;
    pub const DURATION: u32 = 3;
    pub const DIRECTION: u32 = 4;
    pub const DELAY: u32 = 5;
    pub const AUTOMATIC: u32 = 6;
    pub const SEED: u32 = 11;
}

/// What a `KN.PlaceholderArchive` stands in for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum PlaceholderKind {
    /// `kKindPlaceholder` — a placeholder with no special role.
    Generic,
    SlideNumber,
    Title,
    Body,
    /// `kKindObjectPlaceholder` — the "drag an image here" well.
    Object,
    /// A value the 15.3.1 schema does not name.
    Unknown(u64),
}

impl PlaceholderKind {
    pub fn of(value: u64) -> PlaceholderKind {
        match value {
            0 => PlaceholderKind::Generic,
            1 => PlaceholderKind::SlideNumber,
            2 => PlaceholderKind::Title,
            3 => PlaceholderKind::Body,
            4 => PlaceholderKind::Object,
            other => PlaceholderKind::Unknown(other),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            PlaceholderKind::Generic => "placeholder",
            PlaceholderKind::SlideNumber => "slide number",
            PlaceholderKind::Title => "title",
            PlaceholderKind::Body => "body",
            PlaceholderKind::Object => "object",
            PlaceholderKind::Unknown(_) => "unknown",
        }
    }
}

/// What a text storage on a slide is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Role {
    Title,
    Body,
    SlideNumber,
    ObjectPlaceholder,
    /// A placeholder the theme left unclassified.
    Placeholder,
    /// Presenter notes — a storage of kind 4, reached from `KN.NoteArchive`.
    Notes,
    /// A text item or shape the user put on the slide.
    TextBox,
}

impl Role {
    pub fn as_str(self) -> &'static str {
        match self {
            Role::Title => "title",
            Role::Body => "body",
            Role::SlideNumber => "slide number",
            Role::ObjectPlaceholder => "object placeholder",
            Role::Placeholder => "placeholder",
            Role::Notes => "notes",
            Role::TextBox => "text box",
        }
    }
}

/// One `KN.PlaceholderArchive` and the text it holds.
#[derive(Debug, Clone)]
pub struct Placeholder {
    pub identifier: u64,
    pub kind: PlaceholderKind,
    /// The `TSWP.StorageArchive` it lays out. This is what
    /// [`crate::Document::set_text`] takes.
    pub storage: Option<u64>,
    pub text: String,
    /// Whether the slide owns it — the app's `title showing` / `body showing`.
    pub shown: bool,
}

/// One text storage on a slide, and what it is for.
#[derive(Debug, Clone)]
pub struct SlideText {
    pub storage: u64,
    pub role: Role,
    /// The drawable that lays it out, when one does. Notes have none.
    pub drawable: Option<u64>,
    pub text: String,
}

/// A slide's transition, as far as an *inventory* goes. The direction and the
/// two dozen `custom_*` parameters are phase 8b's.
#[derive(Debug, Clone, PartialEq)]
pub struct Transition {
    /// `AnimationAttributesArchive.effect` — `"none"`, or the identifier the
    /// app's dictionary lists: `"apple:dissolve"`, `"apple:magic-move…"`,
    /// `"com.apple.iWork.Keynote.BLTFadeThruColor"`.
    pub effect: String,
    /// `animation_type`, `"Transition"` on every one seen.
    pub animation_type: String,
    pub duration: f64,
    pub delay: f64,
    /// Advance without a click.
    pub automatic: bool,
    pub direction: u64,
    /// `random_number_seed` — different on every slide, and copied verbatim by
    /// the app's own duplicate.
    pub seed: u64,
}

impl Transition {
    /// Does the slide have a transition at all? `"none"` is the empty one.
    pub fn is_none(&self) -> bool {
        self.effect.is_empty() || self.effect == "none"
    }
}

impl Default for Transition {
    fn default() -> Transition {
        Transition {
            effect: "none".into(),
            animation_type: String::new(),
            duration: 0.0,
            delay: 0.0,
            automatic: false,
            direction: 0,
            seed: 0,
        }
    }
}

/// One slide of the deck.
#[derive(Debug, Clone)]
pub struct Slide {
    /// The `KN.SlideArchive`, which is also the component's identifier.
    pub identifier: u64,
    /// The `KN.SlideNodeArchive` that puts it in the deck.
    pub node: u64,
    /// `Index/Slide*.iwa`.
    pub stream: String,
    /// Position in the deck, from 0.
    pub index: usize,
    /// The number the app shows. `None` for a skipped slide, which the app
    /// answers `-1` for.
    pub number: Option<usize>,
    pub skipped: bool,
    /// The layout it is built on — a `KN.SlideArchive` in a
    /// `Index/TemplateSlide-*.iwa`.
    pub layout: Option<u64>,
    pub layout_name: String,
    pub title: Option<Placeholder>,
    pub body: Option<Placeholder>,
    pub slide_number: Option<Placeholder>,
    pub object: Option<Placeholder>,
    /// The `KN.NoteArchive`, when the slide has one.
    pub note: Option<u64>,
    /// The presenter-notes storage — what an edit takes.
    pub note_storage: Option<u64>,
    pub notes: String,
    /// `owned_drawables` (7).
    pub drawables: Vec<u64>,
    /// `drawables_z_order` (42), back to front.
    pub z_order: Vec<u64>,
    pub transition: Transition,
    /// How many `KN.BuildArchive` the slide lists (2).
    pub builds: usize,
    /// How many `KN.BuildChunkArchive` it lists (43).
    pub build_chunks: usize,
    pub style: Option<u64>,
    /// Every text storage on the slide, with its role.
    pub texts: Vec<SlideText>,
}

impl Slide {
    /// The title text, or the empty string.
    pub fn title_text(&self) -> &str {
        self.title.as_ref().map(|p| p.text.as_str()).unwrap_or("")
    }

    /// The body text, or the empty string.
    pub fn body_text(&self) -> &str {
        self.body.as_ref().map(|p| p.text.as_str()).unwrap_or("")
    }

    /// The app's `title showing`.
    pub fn title_showing(&self) -> bool {
        self.title.as_ref().is_some_and(|p| p.shown)
    }

    /// The app's `body showing`.
    pub fn body_showing(&self) -> bool {
        self.body.as_ref().is_some_and(|p| p.shown)
    }
}

/// One slide layout — what the app calls a `slide layout` and 2013 called a
/// master slide.
#[derive(Debug, Clone)]
pub struct Layout {
    pub identifier: u64,
    pub node: u64,
    pub stream: String,
    /// Position in the theme's list, from 0. The app numbers them from 1.
    pub index: usize,
    /// `KN.SlideArchive.name` (10) — "Title & Bullets", "Statement", "Blank".
    pub name: String,
    pub placeholders: Vec<Placeholder>,
    pub drawables: usize,
    /// `bodyParagraphStyles` (31): five style references, one per outline
    /// level. A **positional array** — an entry added to it shifts the mapping
    /// from level to style rather than listing another style.
    pub body_paragraph_styles: Vec<u64>,
    pub body_list_styles: Vec<u64>,
    pub transition: Transition,
}

/// `KN.Soundtrack`, as far as an inventory goes.
#[derive(Debug, Clone)]
pub struct Soundtrack {
    pub identifier: u64,
    pub volume: f64,
    /// 0 play once, 1 loop, 2 do not play.
    pub mode: u64,
    /// How many media entries it names.
    pub tracks: usize,
}

impl Soundtrack {
    pub fn mode_name(&self) -> &'static str {
        match self.mode {
            0 => "play once",
            1 => "loop",
            2 => "do not play",
            _ => "unknown",
        }
    }
}

/// Everything this module reads about one Keynote document.
#[derive(Debug, Clone)]
pub struct Show {
    pub identifier: u64,
    pub theme: Option<u64>,
    /// The theme's *stored* name — `"21_BasicWhite"`. The app shows a
    /// localised display name ("Basic White") that is nowhere in the document.
    pub theme_name: String,
    pub theme_uuid: String,
    /// Slide size in points. Keynote stores 16:9 as 1920 × 1080, which is the
    /// number the app's `width` and `height` report.
    pub width: f32,
    pub height: f32,
    pub slide_numbers_visible: bool,
    pub loop_presentation: bool,
    /// 0 normal, 1 auto-play, 2 hyperlinks only.
    pub mode: u64,
    pub autoplay_transition_delay: f64,
    pub autoplay_build_delay: f64,
    pub idle_timer_active: bool,
    pub idle_timer_delay: f64,
    pub plays_on_open: bool,
    pub stylesheet: Option<u64>,
    pub soundtrack: Option<Soundtrack>,
    /// A `KN.RecordingArchive`, when the deck carries a recorded presentation.
    /// Read and passed through; never authored.
    pub recording: Option<u64>,
    pub slides: Vec<Slide>,
    pub layouts: Vec<Layout>,
}

impl Show {
    pub fn mode_name(&self) -> &'static str {
        match self.mode {
            0 => "normal",
            1 => "auto-play",
            2 => "hyperlinks only",
            _ => "unknown",
        }
    }

    /// The slide with this identifier, whichever way it was named — by the
    /// slide archive or by its node.
    pub fn slide(&self, identifier: u64) -> Option<&Slide> {
        self.slides
            .iter()
            .find(|s| s.identifier == identifier || s.node == identifier)
    }

    /// The layout a slide is built on.
    pub fn layout_of(&self, slide: &Slide) -> Option<&Layout> {
        slide
            .layout
            .and_then(|id| self.layouts.iter().find(|l| l.identifier == id))
    }
}

// -- reading -----------------------------------------------------------------

fn float(message: &Message, number: u32) -> f32 {
    match message.get(number) {
        Some(Value::Fixed32(bytes)) => f32::from_le_bytes(*bytes),
        Some(Value::Fixed64(bytes)) => f64::from_le_bytes(*bytes) as f32,
        _ => 0.0,
    }
}

fn double(message: &Message, number: u32, default: f64) -> f64 {
    match message.get(number) {
        Some(Value::Fixed64(bytes)) => f64::from_le_bytes(*bytes),
        Some(Value::Fixed32(bytes)) => f32::from_le_bytes(*bytes) as f64,
        _ => default,
    }
}

fn flag(message: &Message, number: u32, default: bool) -> bool {
    message.varint(number).map(|v| v != 0).unwrap_or(default)
}

fn text(message: &Message, number: u32) -> String {
    message
        .bytes(number)
        .map(|raw| String::from_utf8_lossy(raw).into_owned())
        .unwrap_or_default()
}

fn reference(message: &Message, number: u32) -> Option<u64> {
    decode_nested(message.bytes(number)?).and_then(|r| reference_target(&r))
}

fn references(message: &Message, number: u32) -> Vec<u64> {
    message
        .all(number)
        .filter_map(|value| match value {
            Value::Bytes(raw) => decode_nested(raw).and_then(|r| reference_target(&r)),
            _ => None,
        })
        .collect()
}

/// The transition a slide or a layout carries.
pub fn transition(slide: &Message) -> Transition {
    let animation = slide
        .bytes(slide_field::TRANSITION)
        .and_then(decode_nested)
        .and_then(|t| {
            t.bytes(transition_field::ATTRIBUTES)
                .and_then(decode_nested)
        })
        .and_then(|a| {
            a.bytes(transition_field::ANIMATION_ATTRIBUTES)
                .and_then(decode_nested)
        });
    let Some(animation) = animation else {
        return Transition::default();
    };
    Transition {
        effect: text(&animation, transition_field::EFFECT),
        animation_type: text(&animation, transition_field::ANIMATION_TYPE),
        duration: double(&animation, transition_field::DURATION, 0.0),
        delay: double(&animation, transition_field::DELAY, 0.0),
        automatic: flag(&animation, transition_field::AUTOMATIC, false),
        direction: animation.varint(transition_field::DIRECTION).unwrap_or(0),
        seed: animation.varint(transition_field::SEED).unwrap_or(0),
    }
}

/// The storage a `TSWP.ShapeInfoArchive` — or anything nesting one — lays out.
///
/// A placeholder is a `KN.PlaceholderArchive` whose field 1 is a shape info,
/// whose field 2 is the storage; a plain text item is the shape info itself.
/// One recursive look for field 2 covers both without knowing which it has.
///
/// **The target has to be a storage.** Field 2 of a `TSD.ImageArchive` is not
/// one, and a walk that trusts the field number alone reports an image slide as
/// carrying a text box whose "storage" is the slide itself — which is exactly
/// what it did before this check was added.
fn shape_storage(
    archives: &BTreeMap<u64, (u32, Message)>,
    archive: &Message,
    depth: usize,
) -> Option<u64> {
    if depth == 0 {
        return None;
    }
    if let Some(storage) = reference(archive, 2) {
        if archives
            .get(&storage)
            .is_some_and(|(message_type, _)| *message_type == crate::TYPE_STORAGE)
        {
            return Some(storage);
        }
    }
    let nested = archive.bytes(1).and_then(decode_nested)?;
    shape_storage(archives, &nested, depth - 1)
}

fn placeholder(
    archives: &BTreeMap<u64, (u32, Message)>,
    document: &crate::Document,
    identifier: u64,
    owned: &BTreeSet<u64>,
) -> Option<Placeholder> {
    let (message_type, archive) = archives.get(&identifier)?;
    let kind = if *message_type == TYPE_PLACEHOLDER {
        PlaceholderKind::of(archive.varint(placeholder_field::KIND).unwrap_or(0))
    } else {
        PlaceholderKind::Generic
    };
    let storage = shape_storage(archives, archive, 4);
    Some(Placeholder {
        identifier,
        kind,
        storage,
        text: storage
            .and_then(|id| document.storage_text(id).ok())
            .unwrap_or_default(),
        shown: owned.contains(&identifier),
    })
}

/// Read the whole `KN` structure of a document.
///
/// Returns `None` for anything that is not a Keynote document: a Pages file
/// handed here has no `KN.ShowArchive` and says so rather than guessing.
pub fn show(document: &crate::Document) -> Option<Show> {
    if document.kind() != crate::Kind::Keynote {
        return None;
    }
    let mut archives: BTreeMap<u64, (u32, Message)> = BTreeMap::new();
    let mut stream_of: BTreeMap<u64, String> = BTreeMap::new();
    for (stream, object) in document.objects() {
        if let Ok(message) = Message::decode(object.payload()) {
            archives.insert(object.identifier, (object.message_type(), message));
            stream_of.insert(object.identifier, stream.to_string());
        }
    }

    let (identifier, show_archive) = archives
        .iter()
        .find(|(_, (message_type, _))| *message_type == TYPE_SHOW)
        .map(|(id, (_, m))| (*id, m.clone()))?;

    // -- the theme, and the layouts it lists ---------------------------------
    let theme = reference(&show_archive, show_field::THEME);
    let theme_archive = theme
        .and_then(|id| archives.get(&id))
        .map(|(_, m)| m.clone());
    let theme_super = theme_archive
        .as_ref()
        .and_then(|t| t.bytes(theme_field::SUPER).and_then(decode_nested));
    let layout_nodes = theme_archive
        .as_ref()
        .map(|t| references(t, theme_field::TEMPLATES))
        .unwrap_or_default();

    let mut layouts = Vec::new();
    for (index, node) in layout_nodes.iter().enumerate() {
        let Some((_, node_archive)) = archives.get(node) else {
            continue;
        };
        let Some(identifier) = reference(node_archive, node_field::SLIDE) else {
            continue;
        };
        let Some((_, archive)) = archives.get(&identifier) else {
            continue;
        };
        let owned: BTreeSet<u64> = references(archive, slide_field::OWNED_DRAWABLES)
            .into_iter()
            .collect();
        let mut placeholders = Vec::new();
        for field in [
            slide_field::TITLE_PLACEHOLDER,
            slide_field::BODY_PLACEHOLDER,
            slide_field::SLIDE_NUMBER_PLACEHOLDER,
            slide_field::OBJECT_PLACEHOLDER,
        ] {
            if let Some(id) = reference(archive, field) {
                if let Some(p) = placeholder(&archives, document, id, &owned) {
                    placeholders.push(p);
                }
            }
        }
        layouts.push(Layout {
            identifier,
            node: *node,
            stream: stream_of.get(&identifier).cloned().unwrap_or_default(),
            index,
            name: text(archive, slide_field::NAME),
            placeholders,
            drawables: owned.len(),
            body_paragraph_styles: references(archive, slide_field::BODY_PARAGRAPH_STYLES),
            body_list_styles: references(archive, slide_field::BODY_LIST_STYLES),
            transition: transition(archive),
        });
    }
    let layout_name: BTreeMap<u64, String> = layouts
        .iter()
        .map(|l| (l.identifier, l.name.clone()))
        .collect();

    // -- the deck ------------------------------------------------------------
    let slide_nodes = show_archive
        .bytes(show_field::SLIDE_TREE)
        .and_then(decode_nested)
        .map(|tree| references(&tree, slide_tree_field::SLIDES))
        .unwrap_or_default();

    let mut slides = Vec::new();
    let mut number = 0usize;
    for (index, node) in slide_nodes.iter().enumerate() {
        let Some((_, node_archive)) = archives.get(node) else {
            continue;
        };
        let Some(identifier) = reference(node_archive, node_field::SLIDE) else {
            continue;
        };
        let Some((_, archive)) = archives.get(&identifier) else {
            continue;
        };
        let skipped = flag(node_archive, node_field::SKIPPED, false);
        // The app numbers the slides it will show, and answers -1 for the rest.
        let visible = if skipped {
            None
        } else {
            number += 1;
            Some(number)
        };

        let drawables = references(archive, slide_field::OWNED_DRAWABLES);
        let owned: BTreeSet<u64> = drawables.iter().copied().collect();
        let named = |field: u32| {
            reference(archive, field).and_then(|id| placeholder(&archives, document, id, &owned))
        };
        let title = named(slide_field::TITLE_PLACEHOLDER);
        let body = named(slide_field::BODY_PLACEHOLDER);
        let slide_number = named(slide_field::SLIDE_NUMBER_PLACEHOLDER);
        let object = named(slide_field::OBJECT_PLACEHOLDER);

        let note = reference(archive, slide_field::NOTE);
        let note_storage = note
            .and_then(|id| archives.get(&id))
            .and_then(|(_, n)| reference(n, 1));

        // Every storage on the slide, named by what points at it.
        let mut texts = Vec::new();
        let mut claimed: BTreeMap<u64, Role> = BTreeMap::new();
        for (place, role) in [
            (&title, Role::Title),
            (&body, Role::Body),
            (&slide_number, Role::SlideNumber),
            (&object, Role::ObjectPlaceholder),
        ] {
            if let Some(p) = place {
                if let Some(storage) = p.storage {
                    claimed.insert(storage, role);
                    texts.push(SlideText {
                        storage,
                        role,
                        drawable: Some(p.identifier),
                        text: p.text.clone(),
                    });
                }
            }
        }
        if let Some(storage) = note_storage {
            claimed.insert(storage, Role::Notes);
            texts.push(SlideText {
                storage,
                role: Role::Notes,
                drawable: None,
                text: document.storage_text(storage).unwrap_or_default(),
            });
        }
        for drawable in &drawables {
            let Some((message_type, info)) = archives.get(drawable) else {
                continue;
            };
            let Some(storage) = shape_storage(&archives, info, 4) else {
                continue;
            };
            if claimed.contains_key(&storage) {
                continue;
            }
            let role = if *message_type == TYPE_PLACEHOLDER {
                Role::Placeholder
            } else {
                Role::TextBox
            };
            claimed.insert(storage, role);
            texts.push(SlideText {
                storage,
                role,
                drawable: Some(*drawable),
                text: document.storage_text(storage).unwrap_or_default(),
            });
        }

        slides.push(Slide {
            identifier,
            node: *node,
            stream: stream_of.get(&identifier).cloned().unwrap_or_default(),
            index,
            number: visible,
            skipped,
            layout: reference(archive, slide_field::TEMPLATE_SLIDE),
            layout_name: reference(archive, slide_field::TEMPLATE_SLIDE)
                .and_then(|id| layout_name.get(&id).cloned())
                .unwrap_or_default(),
            notes: note_storage
                .and_then(|id| document.storage_text(id).ok())
                .unwrap_or_default(),
            title,
            body,
            slide_number,
            object,
            note,
            note_storage,
            drawables,
            z_order: references(archive, slide_field::DRAWABLES_Z_ORDER),
            transition: transition(archive),
            builds: archive.all(slide_field::BUILDS).count(),
            build_chunks: archive.all(slide_field::BUILD_CHUNKS).count(),
            style: reference(archive, slide_field::STYLE),
            texts,
        });
    }

    let size = show_archive
        .bytes(show_field::SIZE)
        .and_then(decode_nested)
        .unwrap_or_default();

    let soundtrack = reference(&show_archive, show_field::SOUNDTRACK)
        .and_then(|id| archives.get(&id).map(|(_, m)| (id, m)))
        .map(|(identifier, m)| Soundtrack {
            identifier,
            volume: double(m, 1, 0.0),
            mode: m.varint(2).unwrap_or(0),
            tracks: m.all(3).count(),
        });

    Some(Show {
        identifier,
        theme,
        theme_name: theme_super
            .as_ref()
            .map(|s| text(s, theme_field::SUPER_NAME))
            .unwrap_or_default(),
        theme_uuid: theme_archive
            .as_ref()
            .map(|t| text(t, theme_field::UUID))
            .unwrap_or_default(),
        width: float(&size, 1),
        height: float(&size, 2),
        slide_numbers_visible: flag(&show_archive, show_field::SLIDE_NUMBERS_VISIBLE, false),
        loop_presentation: flag(&show_archive, show_field::LOOP, false),
        mode: show_archive.varint(show_field::MODE).unwrap_or(0),
        autoplay_transition_delay: double(
            &show_archive,
            show_field::AUTOPLAY_TRANSITION_DELAY,
            5.0,
        ),
        autoplay_build_delay: double(&show_archive, show_field::AUTOPLAY_BUILD_DELAY, 2.0),
        idle_timer_active: flag(&show_archive, show_field::IDLE_TIMER_ACTIVE, false),
        idle_timer_delay: double(&show_archive, show_field::IDLE_TIMER_DELAY, 900.0),
        plays_on_open: flag(&show_archive, show_field::PLAYS_ON_OPEN, false),
        stylesheet: reference(&show_archive, show_field::STYLESHEET),
        soundtrack,
        recording: reference(&show_archive, show_field::RECORDING),
        slides,
        layouts,
    })
}

// -- writing -----------------------------------------------------------------

/// The node of a slide, whichever way the slide was named.
fn node_of(document: &crate::Document, slide: u64) -> Result<(u64, Message), Error> {
    let mut by_slide = None;
    for (_, object) in document.objects() {
        if object.message_type() != TYPE_SLIDE_NODE {
            continue;
        }
        let Ok(archive) = Message::decode(object.payload()) else {
            continue;
        };
        if object.identifier == slide {
            return Ok((object.identifier, archive));
        }
        if reference(&archive, node_field::SLIDE) == Some(slide) {
            by_slide = Some((object.identifier, archive));
        }
    }
    by_slide.ok_or(Error::NoSuchObject(slide))
}

/// Skip a slide, or stop skipping it.
///
/// The whole of the feature is `KN.SlideNodeArchive.isSkipped` (4), which the
/// app's `skipped` property reads and writes. A skipped slide keeps its
/// number's *place* in nothing: the app renumbers the deck around it and
/// answers `slide number` with -1.
///
/// Returns whether the flag changed.
pub fn set_slide_skipped(
    document: &mut crate::Document,
    slide: u64,
    skipped: bool,
) -> Result<bool, Error> {
    let (node, mut archive) = node_of(document, slide)?;
    let was = flag(&archive, node_field::SKIPPED, false);
    if was == skipped {
        return Ok(false);
    }
    // `isSkipped` is a *required* field of the message and every node in the
    // corpus writes it, false included — so this sets it rather than removing
    // it when it goes back to false.
    archive.set(node_field::SKIPPED, Value::Varint(u64::from(skipped)));
    document.set_archive_of(node, &archive)?;
    Ok(true)
}

/// Move a slide to another position in the deck.
///
/// `to` is the index the slide ends up at, counting the deck **after** the
/// slide has been taken out — which is what `move slide n to after slide m`
/// means to the app. Reordering is exactly a permutation of
/// `KN.ShowArchive.slideTree.slides`: Keynote's own `move` was watched doing
/// that and touching nothing else, every node and every slide component byte
/// for byte where it was.
pub fn move_slide(document: &mut crate::Document, slide: u64, to: usize) -> Result<usize, Error> {
    let (node, _) = node_of(document, slide)?;
    let (show_id, show_archive) = document
        .objects()
        .find(|(_, o)| o.message_type() == TYPE_SHOW)
        .map(|(_, o)| (o.identifier, Message::decode(o.payload())))
        .ok_or_else(|| Error::Format("no KN.ShowArchive: not a Keynote document".into()))?;
    let mut show_archive =
        show_archive.map_err(|e| Error::Format(format!("KN.ShowArchive: {e}")))?;

    let Some(tree_raw) = show_archive.bytes(show_field::SLIDE_TREE) else {
        return Err(Error::Format("KN.ShowArchive has no slide tree".into()));
    };
    let mut tree = decode_nested(tree_raw)
        .ok_or_else(|| Error::Format("KN.ShowArchive slide tree does not decode".into()))?;
    let mut order = references(&tree, slide_tree_field::SLIDES);
    let from = order
        .iter()
        .position(|id| *id == node)
        .ok_or_else(|| Error::Format(format!("slide {slide} is not in the deck")))?;
    let to = to.min(order.len().saturating_sub(1));
    if from == to {
        return Ok(from);
    }
    let moved = order.remove(from);
    order.insert(to, moved);

    // Rewrite the repeated field in place: the entries keep the encoding they
    // had, only their order changes.
    let mut rebuilt = Message::default();
    for field in &tree.fields {
        if field.number != slide_tree_field::SLIDES {
            rebuilt.fields.push(field.clone());
        }
    }
    for id in &order {
        let mut entry = Message::default();
        entry.set(1, Value::Varint(*id));
        rebuilt.append_in_order(slide_tree_field::SLIDES, Value::Bytes(entry.encode()));
    }
    tree = rebuilt;
    show_archive.set(show_field::SLIDE_TREE, Value::Bytes(tree.encode()));
    document.set_archive_of(show_id, &show_archive)?;
    Ok(to)
}

/// What [`duplicate_slide`] made.
#[derive(Debug, Clone)]
pub struct SlideCopy {
    /// The new `KN.SlideArchive`, which is also the new component's identifier.
    pub identifier: u64,
    /// The new `KN.SlideNodeArchive`.
    pub node: u64,
    /// The slide it was copied from.
    pub source: u64,
    /// The new `Index/Slide-<id>.iwa`.
    pub stream: String,
    /// Position in the deck, from 0.
    pub index: usize,
    /// How many objects were copied.
    pub objects: usize,
    /// How many `Data/` entries the copy now shares with the original.
    pub media: usize,
    /// Declarations added by [`crate::Document::declare_external_references`].
    pub declarations: usize,
}

/// Copy a slide, and put the copy straight after it.
///
/// **Copy, don't synthesise, taken all the way.** Keynote's own `duplicate` was
/// watched first, by saving before and after: it writes a new
/// `Index/Slide-<id>.iwa` holding the same objects as the source stream, in the
/// same order, at the same sizes, with every reference *inside* the stream
/// remapped and every reference *out* of it — the layout, the stylesheet, the
/// slide style — left pointing where it did; a new `KN.SlideNodeArchive` beside
/// the original in `Index/Document.iwa`, identical to it but for the slide it
/// names and `thumbnailsAreDirty`; a new `TSP.ComponentInfo` carrying the same
/// external references, the same data references with the *user* object
/// remapped, and fresh object UUIDs; and one more entry in the show's slide
/// tree. This does that, and nothing else.
///
/// What it deliberately does not do: invent a thumbnail (the node's
/// `thumbnails` are copied and marked dirty, which is what the app itself
/// leaves behind), or copy the `Data/` bytes (a duplicated image slide shares
/// the original's media — the app shares it too, and the registry is
/// refcounted per component, not per package).
pub fn duplicate_slide(document: &mut crate::Document, slide: u64) -> Result<SlideCopy, Error> {
    // -- find the slide, its node, its stream -------------------------------
    let (stream, source_id) = {
        let (stream, object) = document.object(slide).ok_or(Error::NoSuchObject(slide))?;
        if object.message_type() != TYPE_SLIDE {
            // Naming the node is the friendlier call and costs one lookup.
            let (node, node_archive) = node_of(document, slide)?;
            let target = reference(&node_archive, node_field::SLIDE)
                .ok_or_else(|| Error::Format(format!("slide node {node} names no slide")))?;
            let (stream, _) = document.object(target).ok_or(Error::NoSuchObject(target))?;
            (stream.to_string(), target)
        } else {
            (stream.to_string(), slide)
        }
    };
    if stream.starts_with("Index/TemplateSlide") {
        return Err(Error::Format(format!(
            "object {source_id} is a slide layout, not a slide; \
             Keynote's own dictionary will not copy one either"
        )));
    }
    let (node_id, node_archive) = node_of(document, source_id)?;

    // -- allocate identifiers for every object in the stream ----------------
    let source_objects: Vec<u64> = document
        .objects()
        .filter(|(name, _)| *name == stream)
        .map(|(_, object)| object.identifier)
        .collect();
    if source_objects.is_empty() {
        return Err(Error::Format(format!("slide {source_id} has no stream")));
    }
    let mut next = document.next_object_identifier();
    let mut map: BTreeMap<u64, u64> = BTreeMap::new();
    // The slide archive first, so the copy's component identifier is its slide
    // archive's — the convention every component in a deck follows.
    map.insert(source_id, next);
    next += 1;
    for id in &source_objects {
        if *id == source_id {
            continue;
        }
        map.insert(*id, next);
        next += 1;
    }
    let new_slide = map[&source_id];
    let new_node = next;
    next += 1;
    document.set_last_object_identifier(next - 1)?;

    // -- the stream ----------------------------------------------------------
    let mut copied = document.stream_objects(&stream);
    for object in &mut copied {
        object.identifier = map[&object.identifier];
        for message in &mut object.messages {
            if let Ok(mut archive) = Message::decode(&message.payload) {
                if remap_references(&mut archive, &map, MAX_DEPTH) {
                    message.payload = archive.encode();
                }
            }
        }
    }
    let new_stream = format!("Index/Slide-{new_slide}.iwa");
    let objects = copied.len();
    document.add_stream(&new_stream, copied)?;

    // -- the node ------------------------------------------------------------
    let mut node = node_archive.clone();
    let mut slide_ref = Message::default();
    slide_ref.set(1, Value::Varint(new_slide));
    node.set(node_field::SLIDE, Value::Bytes(slide_ref.encode()));
    // The copy's thumbnails are the original's until the app redraws them,
    // which is exactly what Keynote's own duplicate leaves behind.
    node.set(node_field::THUMBNAILS_DIRTY, Value::Varint(1));
    document.add_object_after(node_id, new_node, TYPE_SLIDE_NODE, &node)?;

    // -- the slide tree ------------------------------------------------------
    let index = insert_into_slide_tree(document, node_id, new_node)?;

    // -- the component -------------------------------------------------------
    let media = clone_component(document, source_id, new_slide, &new_stream, &map)?;
    let declarations = document.declare_external_references();

    Ok(SlideCopy {
        identifier: new_slide,
        node: new_node,
        source: source_id,
        stream: new_stream,
        index,
        objects,
        media,
        declarations,
    })
}

const MAX_DEPTH: usize = 24;

/// Rewrite every `TSP.Reference` whose target is in `map`, at any depth.
///
/// The detector is [`crate::style::reference_target`]'s — a message that is a
/// bare `{1: n}` and nothing else — and only targets *in the map* are touched,
/// so a reference out of the stream, a `TSP.DataReference` and any one-field
/// message that is not a reference at all are all left exactly as written.
/// Returns whether anything changed, so an untouched object keeps its bytes.
fn remap_references(message: &mut Message, map: &BTreeMap<u64, u64>, depth: usize) -> bool {
    if depth == 0 {
        return false;
    }
    let mut changed = false;
    for field in &mut message.fields {
        let Value::Bytes(raw) = &field.value else {
            continue;
        };
        let Some(mut nested) = decode_nested(raw) else {
            continue;
        };
        match reference_target(&nested) {
            Some(target) => {
                if let Some(replacement) = map.get(&target) {
                    nested.set(1, Value::Varint(*replacement));
                    field.value = Value::Bytes(nested.encode());
                    changed = true;
                }
            }
            None => {
                if remap_references(&mut nested, map, depth - 1) {
                    field.value = Value::Bytes(nested.encode());
                    changed = true;
                }
            }
        }
    }
    changed
}

/// Put `new_node` straight after `after` in the show's slide tree, and say
/// where it landed.
fn insert_into_slide_tree(
    document: &mut crate::Document,
    after: u64,
    new_node: u64,
) -> Result<usize, Error> {
    let (show_id, show_archive) = document
        .objects()
        .find(|(_, o)| o.message_type() == TYPE_SHOW)
        .map(|(_, o)| (o.identifier, Message::decode(o.payload())))
        .ok_or_else(|| Error::Format("no KN.ShowArchive: not a Keynote document".into()))?;
    let mut show_archive =
        show_archive.map_err(|e| Error::Format(format!("KN.ShowArchive: {e}")))?;
    let tree_raw = show_archive
        .bytes(show_field::SLIDE_TREE)
        .ok_or_else(|| Error::Format("KN.ShowArchive has no slide tree".into()))?;
    let tree = decode_nested(tree_raw)
        .ok_or_else(|| Error::Format("KN.ShowArchive slide tree does not decode".into()))?;
    let mut order = references(&tree, slide_tree_field::SLIDES);
    let index = order
        .iter()
        .position(|id| *id == after)
        .map_or(order.len(), |position| position + 1);
    order.insert(index, new_node);

    let mut rebuilt = Message::default();
    for field in &tree.fields {
        if field.number != slide_tree_field::SLIDES {
            rebuilt.fields.push(field.clone());
        }
    }
    for id in &order {
        let mut entry = Message::default();
        entry.set(1, Value::Varint(*id));
        rebuilt.append_in_order(slide_tree_field::SLIDES, Value::Bytes(entry.encode()));
    }
    show_archive.set(show_field::SLIDE_TREE, Value::Bytes(rebuilt.encode()));
    document.set_archive_of(show_id, &show_archive)?;
    Ok(index)
}

/// Clone a component entry in `TSP.PackageMetadata` for the copied stream.
///
/// Everything comes from the source component: the external references it
/// already declares, its data references with the *using object* remapped, its
/// version fields, its save token. Only three things differ — the identifier,
/// the locator, and the object UUIDs, which have to be new because two
/// components claiming the same object UUID is the one thing a copy must not
/// do. Returns how many `Data/` entries the copy shares with the original.
fn clone_component(
    document: &mut crate::Document,
    source: u64,
    copy: u64,
    stream: &str,
    map: &BTreeMap<u64, u64>,
) -> Result<usize, Error> {
    let locator = stream
        .strip_prefix("Index/")
        .and_then(|name| name.strip_suffix(".iwa"))
        .unwrap_or(stream)
        .to_string();
    let mut media = 0usize;
    document.update_package_metadata(|metadata| {
        let Some(entry) = metadata
            .all(3)
            .filter_map(|value| match value {
                Value::Bytes(raw) => Message::decode(raw).ok(),
                _ => None,
            })
            .find(|info| info.varint(1) == Some(source))
        else {
            return;
        };
        let mut info = Message::default();
        for field in &entry.fields {
            match field.number {
                // identifier and locator, the two things that must differ
                1 => info.set(1, Value::Varint(copy)),
                2 => info.fields.push(field.clone()),
                3 => {}
                // data references: same media, the user object remapped
                7 => {
                    let Value::Bytes(raw) = &field.value else {
                        continue;
                    };
                    let Some(mut reference) = decode_nested(raw) else {
                        continue;
                    };
                    let mut users = Vec::new();
                    for value in reference.all(2) {
                        let Value::Bytes(raw) = value else { continue };
                        let Some(mut user) = decode_nested(raw) else {
                            continue;
                        };
                        if let Some(object) = user.varint(1) {
                            if let Some(replacement) = map.get(&object) {
                                user.set(1, Value::Varint(*replacement));
                            }
                        }
                        users.push(user.encode());
                    }
                    reference.fields.retain(|f| f.number != 2);
                    for user in users {
                        reference.append_in_order(2, Value::Bytes(user));
                    }
                    media += 1;
                    info.append_in_order(7, Value::Bytes(reference.encode()));
                }
                // object UUIDs: the same shape, new values
                11 => {
                    let Value::Bytes(raw) = &field.value else {
                        continue;
                    };
                    let Some(mut uuid_entry) = decode_nested(raw) else {
                        continue;
                    };
                    let Some(object) = uuid_entry.varint(1) else {
                        continue;
                    };
                    let Some(replacement) = map.get(&object) else {
                        continue;
                    };
                    let (lower, upper) = derived_uuid(&uuid_entry, *replacement);
                    let mut uuid = Message::default();
                    uuid.set(1, Value::Varint(lower));
                    uuid.set(2, Value::Varint(upper));
                    uuid_entry.set(1, Value::Varint(*replacement));
                    uuid_entry.set(2, Value::Bytes(uuid.encode()));
                    info.append_in_order(11, Value::Bytes(uuid_entry.encode()));
                }
                _ => info.fields.push(field.clone()),
            }
        }
        // The locator is only written when it differs from the preferred one,
        // which for a copy it always does.
        info.set(3, Value::Bytes(locator.as_bytes().to_vec()));
        metadata.append_in_order(3, Value::Bytes(info.encode()));
    })?;
    Ok(media)
}

/// A fresh object UUID, derived rather than drawn.
///
/// A copy must not claim the original's object UUIDs — two components saying
/// the same object is the same object is the one way this operation could
/// corrupt a document that still opens. The value is a SHA-1 of the original
/// UUID and the new identifier, which makes the whole duplicate reproducible
/// (the same input gives the same file, so byte-identity tests still mean
/// something) without a random-number dependency.
fn derived_uuid(entry: &Message, identifier: u64) -> (u64, u64) {
    let mut seed = Vec::with_capacity(32);
    if let Some(raw) = entry.bytes(2) {
        seed.extend_from_slice(raw);
    }
    seed.extend_from_slice(b"iwork-rs duplicate-slide");
    seed.extend_from_slice(&identifier.to_le_bytes());
    let digest = crate::media::sha1(&seed);
    let lower = u64::from_le_bytes(digest[0..8].try_into().expect("sha1 is 20 bytes"));
    let upper = u64::from_le_bytes(digest[8..16].try_into().expect("sha1 is 20 bytes"));
    (lower, upper)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pb::Field;

    fn message(fields: Vec<(u32, Value)>) -> Message {
        Message {
            fields: fields
                .into_iter()
                .map(|(number, value)| Field { number, value })
                .collect(),
        }
    }

    fn reference_bytes(id: u64) -> Value {
        let mut r = Message::default();
        r.set(1, Value::Varint(id));
        Value::Bytes(r.encode())
    }

    #[test]
    fn placeholder_kinds_are_the_schemas_five() {
        assert_eq!(PlaceholderKind::of(0), PlaceholderKind::Generic);
        assert_eq!(PlaceholderKind::of(1), PlaceholderKind::SlideNumber);
        assert_eq!(PlaceholderKind::of(2), PlaceholderKind::Title);
        assert_eq!(PlaceholderKind::of(3), PlaceholderKind::Body);
        assert_eq!(PlaceholderKind::of(4), PlaceholderKind::Object);
        assert_eq!(PlaceholderKind::of(9), PlaceholderKind::Unknown(9));
    }

    /// Only the targets in the map move. A reference out of the stream is the
    /// whole point of a slide component — the layout, the stylesheet, the
    /// slide style all live elsewhere — and remapping one would break it.
    #[test]
    fn remapping_leaves_references_out_of_the_map_alone() {
        let mut archive = message(vec![
            (1, reference_bytes(100)),
            (5, reference_bytes(200)),
            (7, reference_bytes(300)),
        ]);
        let map = BTreeMap::from([(200u64, 999u64)]);
        assert!(remap_references(&mut archive, &map, MAX_DEPTH));
        let target = |number: u32| {
            decode_nested(archive.bytes(number).unwrap())
                .and_then(|r| reference_target(&r))
                .unwrap()
        };
        assert_eq!(target(1), 100, "outside the map, untouched");
        assert_eq!(target(5), 999, "inside the map, moved");
        assert_eq!(target(7), 300);
    }

    /// An object with nothing to remap keeps its bytes, which is what lets a
    /// duplicate leave every untouched stream byte-identical.
    #[test]
    fn remapping_nothing_reports_nothing() {
        let mut archive = message(vec![(1, reference_bytes(100))]);
        let before = archive.encode();
        assert!(!remap_references(&mut archive, &BTreeMap::new(), MAX_DEPTH));
        assert_eq!(archive.encode(), before);
    }

    /// References nest: a slide's transition holds its attributes three levels
    /// down, and a shape's storage sits under its super.
    #[test]
    fn remapping_reaches_into_nested_messages() {
        let inner = message(vec![(2, reference_bytes(7))]);
        let mut archive = message(vec![(1, Value::Bytes(inner.encode()))]);
        let map = BTreeMap::from([(7u64, 70u64)]);
        assert!(remap_references(&mut archive, &map, MAX_DEPTH));
        let nested = decode_nested(archive.bytes(1).unwrap()).unwrap();
        assert_eq!(
            decode_nested(nested.bytes(2).unwrap()).and_then(|r| reference_target(&r)),
            Some(70)
        );
    }

    /// The same copy twice gives the same UUID, so a duplicate is reproducible
    /// and the byte-identity tests still say something.
    #[test]
    fn a_derived_uuid_is_a_function_of_its_inputs() {
        let mut uuid = Message::default();
        uuid.set(1, Value::Varint(11));
        uuid.set(2, Value::Varint(22));
        let entry = message(vec![
            (1, Value::Varint(5)),
            (2, Value::Bytes(uuid.encode())),
        ]);
        let once = derived_uuid(&entry, 4242);
        assert_eq!(once, derived_uuid(&entry, 4242));
        assert_ne!(once, derived_uuid(&entry, 4243), "and it depends on the id");
        assert_ne!(once, (11, 22), "and it is not the one it came from");
    }

    #[test]
    fn a_transition_with_no_attributes_is_none() {
        assert!(transition(&Message::default()).is_none());
        let t = Transition {
            effect: "apple:dissolve".into(),
            ..Transition::default()
        };
        assert!(!t.is_none());
    }
}
