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
//! operation rather than an object one.
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
    /// The deck, in order. **This repeated field is the slide order.**
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

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn placeholder_kinds_are_the_schemas_five() {
        assert_eq!(PlaceholderKind::of(0), PlaceholderKind::Generic);
        assert_eq!(PlaceholderKind::of(1), PlaceholderKind::SlideNumber);
        assert_eq!(PlaceholderKind::of(2), PlaceholderKind::Title);
        assert_eq!(PlaceholderKind::of(3), PlaceholderKind::Body);
        assert_eq!(PlaceholderKind::of(4), PlaceholderKind::Object);
        assert_eq!(PlaceholderKind::of(9), PlaceholderKind::Unknown(9));
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
