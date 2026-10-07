//! The things a document is made of, as values — and the handles that edit
//! them once they are in one.
//!
//! **Values to create, handles to edit.** A [`Shape`], a [`TextBox`], an
//! [`Image`] or a [`Chart`] here is a description: it borrows nothing, cannot
//! fail to build, and becomes part of a document when a slide, a sheet or a
//! page is told to [`add`](crate::document::SlideHandle::add) it. Where it
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
    style: Option<u64>,
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
    pub fn style(mut self, style: u64) -> TextBox {
        self.style = Some(style);
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
            text.style(0..length, style)?;
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
/// [`crate::chart::new_chart`] for where the look comes from otherwise.
#[derive(Debug, Clone, PartialEq)]
pub struct Chart {
    kind: ChartKind,
    categories: Vec<String>,
    series: Vec<(String, Vec<f64>)>,
    title: Option<String>,
    legend: bool,
    place: Place,
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

    /// The same data as the table [`crate::chart::set_chart_data`] takes.
    fn data(&self) -> Result<ChartData, Error> {
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
        let made = crate::chart::new_chart(
            document,
            container,
            self.kind,
            &data,
            self.place.position(),
            self.place.size.unwrap_or((800.0, 500.0)),
        )?;
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

/// A table: a name, and how many rows and columns. Its first row is a
/// header row.
///
/// What goes in the cells is written afterwards, through
/// [`Document::table_mut`] with the identifier `add` returns. A table has no
/// size of its own to set: it is as big as its rows and columns are.
#[derive(Debug, Clone, PartialEq)]
pub struct Table {
    name: String,
    rows: usize,
    columns: usize,
    position: (f32, f32),
}

impl Table {
    pub fn new(name: impl Into<String>, rows: usize, columns: usize) -> Table {
        Table {
            name: name.into(),
            rows,
            columns,
            position: (0.0, 0.0),
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
        document.add_table_at(
            container,
            &self.name,
            self.rows,
            self.columns,
            self.position,
        )
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
}

impl TextStyle {
    pub fn new(name: impl Into<String>) -> TextStyle {
        TextStyle {
            name: name.into(),
            look: TextLook::default(),
            align: None,
        }
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

    pub fn identifier(&self) -> u64 {
        self.element
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
        self.document
            .set_geometry(self.element, Some((x, y)), None)
            .map(|_| ())
    }

    pub fn resize(&mut self, width: f32, height: f32) -> Result<(), Error> {
        self.document
            .set_geometry(self.element, None, Some((width, height)))
            .map(|_| ())
    }

    /// Its text, to edit, style and format. Refused for an element with none.
    pub fn text(&mut self) -> Result<crate::document::TextHandle<'_>, Error> {
        let storage = self.storage()?;
        self.document.text_mut(storage)
    }

    /// The same, giving up the element handle for it.
    pub fn into_text(self) -> Result<crate::document::TextHandle<'a>, Error> {
        let storage = self.storage()?;
        self.document.text_mut(storage)
    }

    fn storage(&self) -> Result<u64, Error> {
        self.document
            .drawable(self.element)
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

    pub fn identifier(&self) -> u64 {
        self.chart
    }

    /// Replace the numbers it draws, keeping its kind and its look. The
    /// chart's own title and legend are left as they are.
    pub fn data(&mut self, chart: &Chart) -> Result<(), Error> {
        let data = chart.data()?;
        crate::chart::set_chart_data(self.document, self.chart, &data).map(|_| ())
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
    pub fn add(&mut self, element: impl Element) -> Result<u64, Error> {
        self.document.add_element(&self.container, &element)
    }
}
