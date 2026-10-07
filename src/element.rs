//! The things a document is made of, as values — and the handles that edit
//! them once they are in one.
//!
//! **Values to create, handles to edit.** A [`Shape`], a [`TextBox`], an
//! [`Image`] or a [`Chart`] here is a description: it borrows nothing, cannot
//! fail to build, and becomes part of a document when a slide, a sheet or a
//! page is told to [`add`](crate::document::SlideMut::add) it. Where it
//! goes and how big it is are properties of the value like any other —
//! `.at(x, y)`, `.size(w, h)`.
//!
//! ```
//! # std::env::set_current_dir(std::env::temp_dir()).unwrap();
//! use iwork::{Chart, ChartKind, Color, Document, Kind, Shape, TextBox, TextLook};
//!
//! let mut doc = Document::new(Kind::Keynote)?;
//! let mut slide = doc.slide_mut(0)?;
//! slide.add(Shape::rectangle().at(0.0, 0.0).size(60.0, 1080.0).fill(Color::hex("#122B4A")?).no_stroke())?;
//! slide.add(TextBox::new("Revenue").at(160.0, 120.0).look(TextLook::new().size(64.0).bold()))?;
//! slide.add(
//!     Chart::new(ChartKind::Column)
//!         .at(160.0, 260.0)
//!         .size(1500.0, 700.0)
//!         .categories(["Q1", "Q2", "Q3", "Q4"])
//!         .series("2025", [31.0, 35.5, 40.8, 42.0])
//!         .series("2026", [40.1, 44.5, 48.2, 51.0])
//!         .title("Revenue by quarter")
//!         .legend(),
//! )?;
//! doc.save("Element.key")?;
//! # Ok::<(), iwork::Error>(())
//! ```
//!
//! `API.md` in the repository is the rule book this module follows.

use std::ops::Range;

use crate::chart::{ChartData, ChartKind};
use crate::drawable::{Color, Fill, Frame, Outline, Shadow};
use crate::table::Align;
use crate::text::TextLook;
use crate::{Document, Error, Refusal};

mod sealed {
    pub trait Sealed {}
}

/// An identifier that says what it identifies.
///
/// Every object in a document has a number, and a number says nothing: a
/// style's and a shape's are both `u64`, and handing one where the other was
/// meant used to compile. `add` returns an [`ElementId`] and
/// [`Document::add_text_style`] a [`StyleId`]; what takes one says which.
///
/// A plain `u64` — what the snapshots (`doc.elements()`, `doc.text_styles()`)
/// and the low level speak — converts into either, and either converts back
/// with `.get()` or `u64::from`.
macro_rules! identifier {
    ($(#[$doc:meta])* $name:ident) => {
        $(#[$doc])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub struct $name(u64);

        impl $name {
            /// The object identifier itself.
            pub const fn get(self) -> u64 {
                self.0
            }
        }

        impl From<u64> for $name {
            fn from(identifier: u64) -> $name {
                $name(identifier)
            }
        }

        impl From<$name> for u64 {
            fn from(identifier: $name) -> u64 {
                identifier.0
            }
        }

        impl PartialEq<u64> for $name {
            fn eq(&self, other: &u64) -> bool {
                self.0 == *other
            }
        }

        impl PartialEq<$name> for u64 {
            fn eq(&self, other: &$name) -> bool {
                *self == other.0
            }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                self.0.fmt(f)
            }
        }
    };
}

identifier!(
    /// A shape, a text box, an image, a table or a chart in a document.
    ElementId
);
identifier!(
    /// A named text style in a document.
    StyleId
);

/// Something that can be put on a slide, a sheet or a page: [`Shape`],
/// [`TextBox`], [`Image`], [`Chart`] — and a reference to any of them, so a
/// value kept in a variable can be added more than once.
///
/// Sealed: the set of things a document can hold is the format's, not a
/// caller's.
pub trait Element: sealed::Sealed {
    /// Write the element into `container` and return its identifier.
    #[doc(hidden)]
    fn add_to(&self, document: &mut Document, container: &str) -> Result<u64, Error>;
}

impl<T: sealed::Sealed> sealed::Sealed for &T {}
impl<T: Element> Element for &T {
    fn add_to(&self, document: &mut Document, container: &str) -> Result<u64, Error> {
        (**self).add_to(document, container)
    }
}

/// Where an element goes and how big it is. Either may be left unsaid.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
struct Place {
    position: Option<(f32, f32)>,
    size: Option<(f32, f32)>,
}

impl Place {
    fn position(&self) -> (f32, f32) {
        self.position.unwrap_or((0.0, 0.0))
    }
}

/// The three methods every placeable value has.
macro_rules! placeable {
    ($name:ident) => {
        impl $name {
            /// Where its top-left corner goes, in points from the
            /// container's. Unsaid, that is the origin.
            pub fn at(mut self, x: f32, y: f32) -> Self {
                self.place.position = Some((x, y));
                self
            }

            /// Its width and height, in points.
            pub fn size(mut self, width: f32, height: f32) -> Self {
                self.place.size = Some((width, height));
                self
            }

            /// Position and size at once.
            pub fn frame(self, frame: Frame) -> Self {
                self.at(frame.x, frame.y).size(frame.width, frame.height)
            }
        }
    };
}

// -- shape -------------------------------------------------------------------

/// A rectangle, an ellipse or a line, with or without text in it.
///
/// Unsaid, it is 200 × 200 and looks like the document's own shapes.
#[derive(Debug, Clone, PartialEq)]
pub struct Shape {
    outline: Outline,
    text: String,
    place: Place,
    fill: Option<Fill>,
    /// `Some(None)` is "no outline".
    stroke: Option<Option<(Color, f32)>>,
    opacity: Option<f32>,
    shadow: Option<Shadow>,
}

placeable!(Shape);

impl Shape {
    pub fn new(outline: Outline) -> Shape {
        Shape {
            outline,
            text: String::new(),
            place: Place::default(),
            fill: None,
            stroke: None,
            opacity: None,
            shadow: None,
        }
    }

    pub fn rectangle() -> Shape {
        Shape::new(Outline::Rectangle)
    }

    pub fn ellipse() -> Shape {
        Shape::new(Outline::Ellipse)
    }

    /// A straight line from the top left of its box to the bottom right, so a
    /// height of zero is a horizontal rule.
    pub fn line() -> Shape {
        Shape::new(Outline::Line)
    }

    /// What it is filled with: a [`Color`], a
    /// [`Gradient`](crate::drawable::Gradient), [`Fill::image`], [`Fill::None`].
    pub fn fill(mut self, fill: impl Into<Fill>) -> Shape {
        self.fill = Some(fill.into());
        self
    }

    /// A solid outline, `width` points wide.
    pub fn stroke(mut self, colour: Color, width: f32) -> Shape {
        self.stroke = Some(Some((colour, width)));
        self
    }

    /// No outline.
    pub fn no_stroke(mut self) -> Shape {
        self.stroke = Some(None);
        self
    }

    /// `0.0..=1.0`.
    pub fn opacity(mut self, opacity: f32) -> Shape {
        self.opacity = Some(opacity);
        self
    }

    /// A drop shadow; `Shadow::default()` is a soft black one.
    pub fn shadow(mut self, shadow: Shadow) -> Shape {
        self.shadow = Some(shadow);
        self
    }

    /// Words inside the shape.
    pub fn text(mut self, text: impl Into<String>) -> Shape {
        self.text = text.into();
        self
    }
}

impl sealed::Sealed for Shape {}
impl Element for Shape {
    fn add_to(&self, document: &mut Document, container: &str) -> Result<u64, Error> {
        let size = self.place.size.unwrap_or((200.0, 200.0));
        let made = crate::drawable::add_shape(
            document,
            container,
            self.outline,
            &self.text,
            self.place.position(),
            size,
        )?;
        let mut shape = document.element_mut(made)?;
        if let Some(fill) = &self.fill {
            shape.fill(fill.clone())?;
        }
        match self.stroke {
            Some(Some((colour, width))) => shape.stroke(colour, width)?,
            Some(None) => shape.no_stroke()?,
            None => {}
        }
        if let Some(opacity) = self.opacity {
            shape.opacity(opacity)?;
        }
        if let Some(shadow) = &self.shadow {
            shape.shadow(shadow.clone())?;
        }
        Ok(made)
    }
}

// -- text box ----------------------------------------------------------------

/// Words in a box of their own.
///
/// Ranges are UTF-16 code units, as everywhere in this crate. Unsaid, the box
/// is 600 × 100 and the text is set in the document's body style.
#[derive(Debug, Clone, PartialEq)]
pub struct TextBox {
    text: String,
    place: Place,
    style: Option<StyleId>,
    looks: Vec<(Option<Range<u64>>, TextLook)>,
}

placeable!(TextBox);

impl TextBox {
    pub fn new(text: impl Into<String>) -> TextBox {
        TextBox {
            text: text.into(),
            place: Place::default(),
            style: None,
            looks: Vec::new(),
        }
    }

    /// Set every paragraph in a named style — one
    /// [`Document::add_text_style`] returned, or any the document has.
    pub fn style(mut self, style: impl Into<StyleId>) -> TextBox {
        self.style = Some(style.into());
        self
    }

    /// Give all of the text a look of its own, on top of its style.
    pub fn look(mut self, look: TextLook) -> TextBox {
        self.looks.push((None, look));
        self
    }

    /// Give a run of characters a look of its own: a bold word, a red figure.
    pub fn format(mut self, range: Range<u64>, look: TextLook) -> TextBox {
        self.looks.push((Some(range), look));
        self
    }
}

impl sealed::Sealed for TextBox {}
impl Element for TextBox {
    fn add_to(&self, document: &mut Document, container: &str) -> Result<u64, Error> {
        let size = self.place.size.unwrap_or((600.0, 100.0));
        let made = crate::drawable::add_text_box(
            document,
            container,
            &self.text,
            self.place.position(),
            size,
        )?;
        let length = self.text.encode_utf16().count() as u64;
        if length == 0 || (self.style.is_none() && self.looks.is_empty()) {
            return Ok(made);
        }
        let mut text = document.element_mut(made)?.into_text()?;
        if let Some(style) = self.style {
            text.style(0..length, style.get())?;
        }
        for (range, look) in &self.looks {
            text.format(range.clone().unwrap_or(0..length), look)?;
        }
        Ok(made)
    }
}

// -- image -------------------------------------------------------------------

/// A picture: PNG or JPEG bytes. Unsaid, it is drawn at its pixel size.
#[derive(Debug, Clone, PartialEq)]
pub struct Image {
    bytes: Vec<u8>,
    name: String,
    place: Place,
}

placeable!(Image);

impl Image {
    pub fn new(bytes: impl Into<Vec<u8>>) -> Image {
        Image {
            bytes: bytes.into(),
            name: "image".to_string(),
            place: Place::default(),
        }
    }

    /// The file name it is stored under, e.g. `"portrait.png"`.
    pub fn named(mut self, name: impl Into<String>) -> Image {
        self.name = name.into();
        self
    }
}

impl sealed::Sealed for Image {}
impl Element for Image {
    fn add_to(&self, document: &mut Document, container: &str) -> Result<u64, Error> {
        crate::drawable::add_image(
            document,
            container,
            &self.bytes,
            &self.name,
            self.place.position(),
            self.place.size,
        )
    }
}

// -- chart -------------------------------------------------------------------

/// A chart: a kind, categories along the axis, and one series of numbers per
/// thing compared.
///
/// Unsaid, it is 800 × 500, has no title and no legend. On a document made
/// from one of Apple's themes it takes the theme's look; see
/// [`crate::element::Chart`] for where the look comes from otherwise.
#[derive(Debug, Clone, PartialEq)]
pub struct Chart {
    kind: ChartKind,
    categories: Vec<String>,
    series: Vec<(String, Vec<f64>)>,
    title: Option<String>,
    legend: bool,
    place: Place,
    /// A chart of the document's to copy, in place of the crate's own.
    source: Option<ElementId>,
    /// The grid as given whole, in place of categories and series.
    grid: Option<ChartData>,
}

placeable!(Chart);

impl Chart {
    pub fn new(kind: ChartKind) -> Chart {
        Chart {
            kind,
            categories: Vec::new(),
            series: Vec::new(),
            title: None,
            legend: false,
            place: Place::default(),
            source: None,
            grid: None,
        }
    }

    /// The whole grid at once, for data that is not plain numbers — a blank
    /// cell, a date. It replaces whatever `categories` and `series` said.
    pub fn with_data(mut self, data: ChartData) -> Chart {
        self.grid = Some(data);
        self
    }

    /// A copy of a chart the document already has — its kind, its look, its
    /// own settings — drawing the data given here. Unsaid, it is the size of
    /// the chart it copies.
    ///
    /// What is shared and what is copied follows the archive's own division:
    /// the theme's styles are shared, as two charts made from one preset
    /// share them in the app, and everything the chart keeps to itself is
    /// copied, so editing one cannot change the other. A chart fed by a
    /// table is refused: its copy would claim to follow that table while
    /// holding numbers of its own.
    pub fn copy_of(chart: impl Into<ElementId>) -> Chart {
        Chart {
            source: Some(chart.into()),
            ..Chart::new(ChartKind::Column)
        }
    }

    /// The labels along the category axis — or, for a pie, of the wedges.
    pub fn categories<S: Into<String>>(mut self, names: impl IntoIterator<Item = S>) -> Chart {
        self.categories = names.into_iter().map(Into::into).collect();
        self
    }

    /// One series: its name, and a value for every category.
    pub fn series(
        mut self,
        name: impl Into<String>,
        values: impl IntoIterator<Item = f64>,
    ) -> Chart {
        self.series
            .push((name.into(), values.into_iter().collect()));
        self
    }

    /// A title above the chart.
    pub fn title(mut self, title: impl Into<String>) -> Chart {
        self.title = Some(title.into());
        self
    }

    /// Show the legend, which names each series.
    pub fn legend(mut self) -> Chart {
        self.legend = true;
        self
    }

    /// The categories and series as the grid a chart draws — what
    /// [`ChartMut::data`] takes.
    pub fn data(&self) -> Result<ChartData, Error> {
        if let Some(grid) = &self.grid {
            return Ok(grid.clone());
        }
        if self.categories.is_empty() || self.series.is_empty() {
            return Err(Error::refused(
                Refusal::UnwritableValue,
                "a chart needs at least one category and one series",
            ));
        }
        if let Some((name, values)) = self
            .series
            .iter()
            .find(|(_, values)| values.len() != self.categories.len())
        {
            return Err(Error::refused(
                Refusal::UnwritableValue,
                format!(
                    "series {name:?} has {} value(s) for {} categor(y|ies)",
                    values.len(),
                    self.categories.len()
                ),
            ));
        }
        Ok(ChartData {
            row_names: self.series.iter().map(|(name, _)| name.clone()).collect(),
            column_names: self.categories.clone(),
            rows: self
                .series
                .iter()
                .map(|(_, values)| {
                    values
                        .iter()
                        .copied()
                        .map(crate::chart::GridValue::Number)
                        .collect()
                })
                .collect(),
        })
    }
}

impl sealed::Sealed for Chart {}
impl Element for Chart {
    fn add_to(&self, document: &mut Document, container: &str) -> Result<u64, Error> {
        let data = self.data()?;
        let made = match self.source {
            Some(source) => {
                let size = self.place.size.or_else(|| {
                    document
                        .element(source)
                        .map(|found| (found.geometry.width, found.geometry.height))
                });
                crate::chart::add_chart(
                    document,
                    container,
                    source.get(),
                    &data,
                    self.place.position(),
                    size.unwrap_or((800.0, 500.0)),
                )?
            }
            None => crate::chart::new_chart(
                document,
                container,
                self.kind,
                &data,
                self.place.position(),
                self.place.size.unwrap_or((800.0, 500.0)),
            )?,
        };
        if let Some(title) = &self.title {
            crate::chart::set_title(document, made, Some(title))?;
        }
        if self.legend {
            crate::chart::set_legend(document, made, true)?;
        }
        Ok(made)
    }
}

// -- table -------------------------------------------------------------------

/// A table: a name, how many rows and columns, and what is in the cells.
///
/// Its first row is a header row. It has no size of its own to set: it is as
/// big as its rows and columns are (`doc.table_mut(id)?.column_width(…)`).
/// `add` returns the identifier [`Document::table_mut`] takes, for formats,
/// formulas, fills and everything else a table does.
#[derive(Debug, Clone, PartialEq)]
pub struct Table {
    name: String,
    rows: usize,
    columns: usize,
    position: (f32, f32),
    cells: Vec<Vec<crate::table::CellValue>>,
}

impl Table {
    /// An empty table of this size.
    pub fn new(name: impl Into<String>, rows: usize, columns: usize) -> Table {
        Table {
            name: name.into(),
            rows,
            columns,
            position: (0.0, 0.0),
            cells: Vec::new(),
        }
    }

    /// A table exactly as big as what is in it: one inner list per row, the
    /// first being the header row.
    pub fn with_rows<Row, Value>(
        name: impl Into<String>,
        rows: impl IntoIterator<Item = Row>,
    ) -> Table
    where
        Row: IntoIterator<Item = Value>,
        Value: Into<crate::table::CellValue>,
    {
        let cells: Vec<Vec<crate::table::CellValue>> = rows
            .into_iter()
            .map(|row| row.into_iter().map(Into::into).collect())
            .collect();
        Table {
            name: name.into(),
            rows: cells.len(),
            columns: cells.iter().map(Vec::len).max().unwrap_or(0),
            position: (0.0, 0.0),
            cells,
        }
    }

    /// Where its top-left corner goes, in points from the container's.
    pub fn at(mut self, x: f32, y: f32) -> Table {
        self.position = (x, y);
        self
    }
}

impl sealed::Sealed for Table {}
impl Element for Table {
    fn add_to(&self, document: &mut Document, container: &str) -> Result<u64, Error> {
        let made = document.add_table_at(
            container,
            &self.name,
            self.rows,
            self.columns,
            self.position,
        )?;
        if !self.cells.is_empty() {
            document.table_mut(made)?.set_block("A1", &self.cells)?;
        }
        Ok(made)
    }
}

// -- text style --------------------------------------------------------------

/// A named paragraph style: how its characters look, and how it is ranged.
///
/// Made from the document's body style with what is said here changed, and
/// offered in the app's style menu under its name. See
/// [`Document::add_text_style`].
#[derive(Debug, Clone, PartialEq)]
pub struct TextStyle {
    pub(crate) name: String,
    pub(crate) look: TextLook,
    pub(crate) align: Option<Align>,
    pub(crate) based_on: Option<StyleId>,
}

impl TextStyle {
    pub fn new(name: impl Into<String>) -> TextStyle {
        TextStyle {
            name: name.into(),
            look: TextLook::default(),
            align: None,
            based_on: None,
        }
    }

    /// Start from this style of the document's, rather than from its body
    /// style: everything it sets that this does not is kept.
    pub fn based_on(mut self, style: impl Into<StyleId>) -> TextStyle {
        self.based_on = Some(style.into());
        self
    }

    /// Font, size, colour, weight — see [`TextLook`].
    pub fn look(mut self, look: TextLook) -> TextStyle {
        self.look = look;
        self
    }

    /// How its paragraphs are ranged.
    pub fn align(mut self, align: Align) -> TextStyle {
        self.align = Some(align);
        self
    }
}

// -- handles -----------------------------------------------------------------

/// A shape, a text box, an image or a chart that is in a document, to change.
///
/// From [`Document::element_mut`]. Every method is one edit.
pub struct ElementMut<'a> {
    document: &'a mut Document,
    element: u64,
}

impl<'a> ElementMut<'a> {
    pub(crate) fn new(document: &'a mut Document, element: u64) -> ElementMut<'a> {
        ElementMut { document, element }
    }

    pub fn identifier(&self) -> ElementId {
        ElementId(self.element)
    }

    /// What it is filled with. The first paint gives it a style of its own,
    /// so nothing else that shared its style changes.
    pub fn fill(&mut self, fill: impl Into<Fill>) -> Result<(), Error> {
        crate::drawable::set_fill(self.document, self.element, &fill.into())
    }

    /// A solid outline, `width` points wide.
    pub fn stroke(&mut self, colour: Color, width: f32) -> Result<(), Error> {
        crate::drawable::set_stroke(self.document, self.element, colour, width)
    }

    /// No outline.
    pub fn no_stroke(&mut self) -> Result<(), Error> {
        crate::drawable::set_stroke(self.document, self.element, Color::BLACK, 0.0)
    }

    /// `0.0..=1.0`.
    pub fn opacity(&mut self, opacity: f32) -> Result<(), Error> {
        crate::drawable::set_opacity(self.document, self.element, opacity)
    }

    pub fn shadow(&mut self, shadow: Shadow) -> Result<(), Error> {
        crate::drawable::set_shadow(self.document, self.element, Some(shadow))
    }

    pub fn no_shadow(&mut self) -> Result<(), Error> {
        crate::drawable::set_shadow(self.document, self.element, None)
    }

    /// Move its top-left corner.
    pub fn move_to(&mut self, x: f32, y: f32) -> Result<(), Error> {
        self.set_geometry(Some((x, y)), None).map(|_| ())
    }

    pub fn resize(&mut self, width: f32, height: f32) -> Result<(), Error> {
        self.set_geometry(None, Some((width, height))).map(|_| ())
    }

    /// Move it, resize it, or both at once — and be told what changed with
    /// it.
    ///
    /// Move or resize a drawable.
    ///
    /// The rectangle is the one the **app** reports, which for a masked image
    /// is the mask's window and not the picture's own rectangle. Give `None`
    /// for either half to leave it alone.
    ///
    /// What travels with it, because the app maintains it and a document that
    /// does not is inconsistent with every other document:
    ///
    /// * an **unmasked** media drawable's `originalSize`, which Keynote and
    ///   Pages both rewrote to the new size when a script resized an image. A
    ///   masked image's `originalSize` is left alone: the app fills it with the
    ///   mask window rather than the picture there, and not even consistently
    ///   (see the body of this method), so there is no size to rewrite it to;
    /// * a mask's geometry and its path source's natural size. Resizing a
    ///   masked image scales the whole assembly by one factor: Pages, asked to
    ///   make a 475-point-wide masked photo 300 wide, multiplied the picture's
    ///   own size, the mask's offset, the mask's size and the mask path's
    ///   natural size by 300/475 and moved the picture so that
    ///   `image.position + mask.position` still landed on the frame's corner.
    ///   That is what this reproduces, with the horizontal and vertical factors
    ///   taken separately — which reduces to what was observed whenever the
    ///   resize is proportional, and is **unverified** when it is not, because
    ///   the app would not perform one: every image in the corpus has its
    ///   aspect ratio locked.
    ///
    /// Rotation, the geometry flags and everything else in the archive are left
    /// exactly as they were.
    ///
    /// Refused by name: an object carrying version patches (see
    /// [`Document::patched_objects`]), and resizing something whose current
    /// width or height is zero, where the scale factor is not a number. A
    /// **locked** drawable is not refused — the lock is a rule the app's UI
    /// keeps, not one the format keeps — but it is reported, because the app
    /// will not let a user undo the move by hand.
    pub fn set_geometry(
        &mut self,
        position: Option<(f32, f32)>,
        size: Option<(f32, f32)>,
    ) -> Result<crate::drawable::GeometryChange, Error> {
        self.document.set_geometry(self.element, position, size)
    }

    /// Its text, to edit, style and format. Refused for an element with none.
    pub fn text(&mut self) -> Result<crate::document::TextMut<'_>, Error> {
        let storage = self.storage()?;
        self.document.text_mut(storage)
    }

    /// The same, giving up the element handle for it.
    pub fn into_text(self) -> Result<crate::document::TextMut<'a>, Error> {
        let storage = self.storage()?;
        self.document.text_mut(storage)
    }

    fn storage(&self) -> Result<u64, Error> {
        self.document
            .element(self.element)
            .and_then(|drawable| drawable.text)
            .ok_or_else(|| {
                Error::refused(
                    Refusal::WrongSlot,
                    format!("element {} holds no text", self.element),
                )
            })
    }
}

/// A chart that is in a document, to change. From [`Document::chart_mut`].
pub struct ChartMut<'a> {
    document: &'a mut Document,
    chart: u64,
}

impl<'a> ChartMut<'a> {
    pub(crate) fn new(document: &'a mut Document, chart: u64) -> ChartMut<'a> {
        ChartMut { document, chart }
    }

    pub fn identifier(&self) -> ElementId {
        ElementId(self.chart)
    }

    /// Replace the numbers it draws, keeping its kind and its look. The
    /// chart's own title and legend are left as they are.
    ///
    /// `Chart::new(kind).categories(…).series(…).data()?` builds one, and so
    /// does [`ChartData::numbers`]. See [`crate::element::ChartMut::data`].
    pub fn data(&mut self, data: &ChartData) -> Result<(), Error> {
        crate::chart::set_chart_data(self.document, self.chart, data).map(|_| ())
    }

    /// Make the chart follow a table.
    ///
    /// Make a chart follow a table — the *mediator*, which is what turns a
    /// picture of some numbers into a chart of a table.
    ///
    /// The grid the chart draws stays what it was: in Numbers it is a **cache**
    /// of what the mediator's formulas last evaluated to. What this adds is the
    /// formulas — one per series and per label, each a reference to the table
    /// wrapped in function 175 — and the owner that makes the calculation
    /// engine know about them.
    ///
    /// ```no_run
    /// # fn main() -> Result<(), iwork::Error> {
    /// # let mut doc = iwork::Document::open("Budget.numbers")?;
    /// use iwork::chart::ChartBinding;
    /// doc.chart_mut(905245)?.bind(
    ///     "Umsatz",
    ///     &ChartBinding {
    ///         series: vec!["B2:B13".into(), "C2:C13".into()],
    ///         row_labels: vec!["A2:A13".into()],
    ///         column_labels: vec!["B1".into(), "C1".into()],
    ///         series_by_row: false,
    ///     },
    /// )?;
    /// # Ok(()) }
    /// ```
    pub fn bind(
        &mut self,
        table: &str,
        binding: &crate::chart::ChartBinding,
    ) -> Result<u64, Error> {
        self.document.bind_chart(self.chart, table, binding)
    }

    pub fn title(&mut self, title: impl AsRef<str>) -> Result<(), Error> {
        crate::chart::set_title(self.document, self.chart, Some(title.as_ref()))
    }

    pub fn no_title(&mut self) -> Result<(), Error> {
        crate::chart::set_title(self.document, self.chart, None)
    }

    pub fn legend(&mut self) -> Result<(), Error> {
        crate::chart::set_legend(self.document, self.chart, true)
    }

    pub fn no_legend(&mut self) -> Result<(), Error> {
        crate::chart::set_legend(self.document, self.chart, false)
    }

    /// The chart as an element: to move, to resize.
    pub fn element(&mut self) -> ElementMut<'_> {
        ElementMut::new(self.document, self.chart)
    }
}

/// A Numbers sheet or a Pages page, to add things to. From
/// [`Document::sheet_mut`] and [`Document::page_mut`].
pub struct CanvasMut<'a> {
    document: &'a mut Document,
    container: String,
}

impl<'a> CanvasMut<'a> {
    pub(crate) fn new(document: &'a mut Document, container: String) -> CanvasMut<'a> {
        CanvasMut {
            document,
            container,
        }
    }

    /// Put a [`Shape`], a [`TextBox`], an [`Image`] or a [`Chart`] here, and
    /// get its identifier. A refused `add` leaves the document as it was.
    pub fn add(&mut self, element: impl Element) -> Result<ElementId, Error> {
        self.document
            .add_element(&self.container, &element)
            .map(ElementId)
    }
}

/// A named text style that is in a document, to change. From
/// [`Document::text_style_mut`].
pub struct TextStyleMut<'a> {
    document: &'a mut Document,
    style: u64,
}

impl<'a> TextStyleMut<'a> {
    pub(crate) fn new(document: &'a mut Document, style: u64) -> TextStyleMut<'a> {
        TextStyleMut { document, style }
    }

    pub fn identifier(&self) -> StyleId {
        StyleId(self.style)
    }

    /// Change what the [`TextLook`] sets, and leave the rest of the style.
    pub fn look(&mut self, look: &TextLook) -> Result<(), Error> {
        self.document.apply_style_look(self.style, look, None)
    }

    /// How its paragraphs are ranged.
    pub fn align(&mut self, align: Align) -> Result<(), Error> {
        self.document
            .apply_style_look(self.style, &TextLook::default(), Some(align))
    }

    pub fn rename(&mut self, name: &str) -> Result<(), Error> {
        self.document.rename_text_style(self.style, name)
    }

    /// One property by its path in the archive (`iwork::style::property`),
    /// for what [`TextLook`] does not cover; `None` clears it.
    pub fn property(&mut self, path: &[u32], value: Option<crate::pb::Value>) -> Result<(), Error> {
        self.document
            .set_text_style_property(self.style, path, value)
    }

    /// Delete the style, pointing the text that used it at `replace_with` or
    /// at nothing.
    pub fn delete(
        self,
        replace_with: Option<StyleId>,
    ) -> Result<crate::style::StyleDeletion, Error> {
        self.document
            .delete_text_style(self.style, replace_with.map(StyleId::get))
    }
}

impl TextStyleMut<'_> {
    /// Copy this style under a new name, and be told what was made. Any kind
    /// of text style can be copied — paragraph, character, list.
    ///
    /// Copy a style, giving the copy a new name and a new object identifier.
    ///
    /// Styles are created by copying rather than by synthesis, for the reason
    /// `FORMAT.md` gives for whole documents: the style graph is large, iWork is
    /// unforgiving about dangling references, and a style that already works is
    /// a far better starting point than a guess at the schema. The copy lands in
    /// the template's stream, right after it, and is listed in every stylesheet
    /// that listed the template by plain reference.
    ///
    /// The identifier comes from above `TSP.PackageMetadata` field 1, which is
    /// then bumped, so iWork will not later hand the same number to something
    /// else.
    ///
    /// **A copy of a variation style does not get the name.** Named styles and
    /// variations are different things: a named style carries a name at
    /// [`crate::style::NAME`] and an identifier at [`crate::style::STYLE_IDENTIFIER`], while a
    /// variation carries neither, a parent, and a flag saying it is one. Naming
    /// the copy of a variation produces an object that claims to be a variation,
    /// has a name, has no identifier, and is listed among the named styles —
    /// and Pages crashes on opening the document. It was doing exactly that
    /// until a document that crashed showed up.
    ///
    /// The copy is still made, still listed, and still usable — it is simply
    /// anonymous, which is what a variation is. `CreatedStyle::name` reports
    /// whether the name was applied. Turning a variation into a named style
    /// properly would mean synthesising a style identifier and clearing the
    /// variation flag, which is more invention than this crate is willing to do
    /// without a document to check it against.
    pub fn copy(&mut self, name: &str) -> Result<crate::style::CreatedStyle, Error> {
        self.document.create_text_style(self.style, name)
    }

    /// Edit the style's archive directly.
    ///
    /// Edit a style archive directly.
    ///
    /// The escape hatch for everything this crate does not model: the archive
    /// arrives decoded into wire fields and is re-encoded in place afterwards.
    pub fn update(&mut self, edit: impl FnOnce(&mut crate::pb::Message)) -> Result<(), Error> {
        self.document.update_text_style(self.style, edit)
    }

    /// Take one property from another style.
    ///
    /// Copy one property subtree from another style.
    ///
    /// The way to give a style a property whose container it does not have.
    /// [`crate::element::TextStyleMut::property`] will not invent a container,
    /// because a container it invents holds only the fields it was asked for —
    /// a colour written as `{r, g, b}`, with no model and no alpha, crashes
    /// Pages on opening. Lifting a whole working subtree across avoids the
    /// question: the copy is a colour that a real document already contains.
    ///
    /// Take the colour from a style that has one, then change the channels:
    ///
    /// ```no_run
    /// # fn main() -> Result<(), iwork::Error> {
    /// # let mut doc = iwork::Document::open("Report.pages")?;
    /// use iwork::style::property;
    /// let mut style = doc.text_style_mut(3801)?;
    /// style.copy_property(3712, property::FONT_COLOR)?;
    /// style.property(property::RED,
    ///     Some(iwork::pb::Value::Fixed32(0.85f32.to_le_bytes())))?;
    /// # Ok(()) }
    /// ```
    pub fn copy_property(&mut self, from: impl Into<StyleId>, path: &[u32]) -> Result<(), Error> {
        self.document
            .copy_text_style_property(from.into().get(), self.style, path)
    }
}

/// A cell style that is in a document, to repaint. From
/// [`Document::cell_style_mut`].
pub struct CellStyleMut<'a> {
    document: &'a mut Document,
    style: u64,
}

impl<'a> CellStyleMut<'a> {
    pub(crate) fn new(document: &'a mut Document, style: u64) -> CellStyleMut<'a> {
        CellStyleMut { document, style }
    }

    /// A colour, or no fill.
    ///
    /// Paint a cell style, or with `None` stop it painting.
    ///
    /// See [`crate::element::CellStyleMut::fill`]: this repaints **every**
    /// table whose style names the cell style, because that is what a shared
    /// style is. The fill is the only property of a cell style this crate
    /// writes; the rest of the archive is left exactly as it was.
    pub fn fill(&mut self, fill: impl Into<Fill>) -> Result<(), Error> {
        let colour = match fill.into() {
            Fill::None => None,
            Fill::Color(colour) => Some(colour),
            Fill::Gradient(_) | Fill::Image(_) => {
                return Err(Error::refused(
                    Refusal::UnwritableValue,
                    "a cell style takes a colour or no fill",
                ))
            }
        };
        self.document.set_cell_style_fill(self.style, colour)
    }
}
