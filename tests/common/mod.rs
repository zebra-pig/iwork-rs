//! The call shapes these tests were written in, on top of the public API.
//!
//! Up to 0.3 `Document` had a flat function for every edit — `set_cell`,
//! `add_shape`, `set_object_fill`. 0.4 removed them: an edit goes through the
//! handle of the thing edited. The tests written against the flat functions
//! are worth more than their spelling, so this keeps the spelling and routes
//! every call through `slide_mut`, `table_mut`, `text_mut`, `element_mut` and
//! `canvas_mut` — which makes each of them a test of the handle too.

#![allow(dead_code)]

use std::ops::Range;

use iwork::chart::{ChartData, ChartKind, GridValue};
use iwork::drawable::{
    Color, Fill, Frame, Gradient, ImageFill, ImageFit, ImageSource, Outline, Shadow,
};
use iwork::keynote::{SlideCopy, SlideRef, Transition, TransitionEdit};
use iwork::pb::Value;
use iwork::style::StyleDeletion;
use iwork::table::{CellValue, Format, SortRule};
use iwork::text::TextLook;
use iwork::{Chart, Document, Error, Image, Shape, Table, TextBox, TextEdit};

fn slide(identifier: u64) -> SlideRef {
    SlideRef::Identifier(identifier)
}

pub trait Flat {
    fn doc(&mut self) -> &mut Document;

    // -- text ---------------------------------------------------------------
    fn set_text(&mut self, storage: u64, text: &str) -> Result<TextEdit, Error> {
        self.doc().text_mut(storage)?.set(text)
    }
    fn append_paragraph(&mut self, text: &str) -> Result<TextEdit, Error> {
        self.doc().body_mut()?.append(text)
    }
    fn insert_text(&mut self, storage: u64, at: u64, text: &str) -> Result<TextEdit, Error> {
        self.doc().text_mut(storage)?.insert(at, text)
    }
    fn delete_text(&mut self, storage: u64, range: Range<u64>) -> Result<TextEdit, Error> {
        self.doc().text_mut(storage)?.delete(range)
    }
    fn replace_text(
        &mut self,
        storage: u64,
        range: Range<u64>,
        text: &str,
    ) -> Result<TextEdit, Error> {
        self.doc().text_mut(storage)?.replace(range, text)
    }
    fn apply_text_style(
        &mut self,
        storage: u64,
        range: Range<u64>,
        style: u64,
    ) -> Result<(), Error> {
        self.doc().text_mut(storage)?.style(range, style)
    }
    fn format_text(
        &mut self,
        storage: u64,
        range: Range<u64>,
        look: &TextLook,
    ) -> Result<(), Error> {
        self.doc().text_mut(storage)?.format(range, look)
    }

    // -- text styles --------------------------------------------------------
    fn rename_text_style(&mut self, style: u64, name: &str) -> Result<(), Error> {
        self.doc().text_style_mut(style)?.rename(name)
    }
    fn set_text_style_property(
        &mut self,
        style: u64,
        path: &[u32],
        value: Option<Value>,
    ) -> Result<(), Error> {
        self.doc().text_style_mut(style)?.property(path, value)
    }
    fn set_text_style_color(
        &mut self,
        style: u64,
        red: f32,
        green: f32,
        blue: f32,
        alpha: f32,
    ) -> Result<(), Error> {
        let colour = Color::rgb(red, green, blue).with_alpha(alpha);
        self.doc()
            .text_style_mut(style)?
            .look(&TextLook::new().colour(colour))
    }
    fn delete_text_style(
        &mut self,
        style: u64,
        replace_with: Option<u64>,
    ) -> Result<StyleDeletion, Error> {
        self.doc().text_style_mut(style)?.delete(replace_with)
    }

    // -- cells --------------------------------------------------------------
    fn set_cell(
        &mut self,
        table: &str,
        row: usize,
        column: usize,
        value: CellValue,
    ) -> Result<CellValue, Error> {
        self.doc().table_mut(table)?.set((row, column), value)
    }
    fn set_formula(
        &mut self,
        table: &str,
        row: usize,
        column: usize,
        text: &str,
        value: CellValue,
    ) -> Result<(), Error> {
        self.doc()
            .table_mut(table)?
            .formula((row, column), text, value)
    }
    fn set_format(
        &mut self,
        table: &str,
        cells: impl IntoIterator<Item = (usize, usize)>,
        format: &Format,
    ) -> Result<usize, Error> {
        let mut handle = self.doc().table_mut(table)?;
        let mut changed = 0;
        for cell in cells {
            changed += handle.format(cell, format)?;
        }
        Ok(changed)
    }
    fn set_cell_fill(
        &mut self,
        table: &str,
        cells: impl IntoIterator<Item = (usize, usize)>,
        colour: Option<Color>,
    ) -> Result<usize, Error> {
        let mut handle = self.doc().table_mut(table)?;
        let mut changed = 0;
        for cell in cells {
            changed += handle.fill(cell, colour)?;
        }
        Ok(changed)
    }
    fn set_column_width(
        &mut self,
        table: &str,
        column: usize,
        points: Option<f32>,
    ) -> Result<(), Error> {
        self.doc().table_mut(table)?.column_width(column, points)
    }
    fn set_row_height(
        &mut self,
        table: &str,
        row: usize,
        points: Option<f32>,
    ) -> Result<(), Error> {
        self.doc().table_mut(table)?.row_height(row, points)
    }
    fn set_sort_rules(&mut self, table: &str, rules: &[SortRule]) -> Result<(), Error> {
        self.doc().table_mut(table)?.sort_by(rules)
    }
    fn merge_cells(
        &mut self,
        table: &str,
        row: usize,
        column: usize,
        rows: usize,
        columns: usize,
    ) -> Result<(), Error> {
        self.doc().table_mut(table)?.merge(iwork::table::CellRange {
            start: (row, column).into(),
            end: (
                row + rows.saturating_sub(1),
                column + columns.saturating_sub(1),
            )
                .into(),
        })
    }
    fn unmerge_cells(&mut self, table: &str, row: usize, column: usize) -> Result<(), Error> {
        self.doc().table_mut(table)?.unmerge((row, column))
    }

    // -- things on a slide, a sheet or a page -------------------------------
    fn add_table_at(
        &mut self,
        container: &str,
        name: &str,
        rows: usize,
        columns: usize,
        position: (f32, f32),
    ) -> Result<u64, Error> {
        let table = Table::new(name, rows, columns).at(position.0, position.1);
        self.doc().canvas_mut(container)?.add(table)
    }
    fn add_text_box(
        &mut self,
        container: &str,
        text: &str,
        position: (f32, f32),
        size: (f32, f32),
    ) -> Result<u64, Error> {
        let text_box = TextBox::new(text)
            .at(position.0, position.1)
            .size(size.0, size.1);
        self.doc().canvas_mut(container)?.add(text_box)
    }
    fn add_shape(
        &mut self,
        container: &str,
        outline: Outline,
        text: &str,
        position: (f32, f32),
        size: (f32, f32),
    ) -> Result<u64, Error> {
        let shape = Shape::new(outline)
            .text(text)
            .at(position.0, position.1)
            .size(size.0, size.1);
        self.doc().canvas_mut(container)?.add(shape)
    }
    fn add_image(
        &mut self,
        container: &str,
        bytes: &[u8],
        name: &str,
        position: (f32, f32),
        size: Option<(f32, f32)>,
    ) -> Result<u64, Error> {
        let mut image = Image::new(bytes).named(name).at(position.0, position.1);
        if let Some((width, height)) = size {
            image = image.size(width, height);
        }
        self.doc().canvas_mut(container)?.add(image)
    }
    fn new_chart(
        &mut self,
        container: &str,
        kind: ChartKind,
        data: &ChartData,
        position: (f32, f32),
        size: (f32, f32),
    ) -> Result<u64, Error> {
        let mut chart = Chart::new(kind)
            .at(position.0, position.1)
            .size(size.0, size.1)
            .categories(data.column_names.iter().cloned());
        for (name, row) in data.row_names.iter().zip(&data.rows) {
            let values = row.iter().map(|value| match value {
                GridValue::Number(number) => *number,
                other => panic!("the chart value is numbers only, not {other:?}"),
            });
            chart = chart.series(name.clone(), values);
        }
        self.doc().canvas_mut(container)?.add(chart)
    }
    fn add_chart(
        &mut self,
        container: &str,
        from: u64,
        data: &ChartData,
        position: (f32, f32),
        size: (f32, f32),
    ) -> Result<u64, Error> {
        self.doc().copy_chart(container, from, data, position, size)
    }

    // -- how an element looks -----------------------------------------------
    fn set_object_fill(&mut self, element: u64, colour: Option<Color>) -> Result<(), Error> {
        self.doc().element_mut(element)?.fill(colour)
    }
    fn set_object_gradient(&mut self, element: u64, gradient: &Gradient) -> Result<(), Error> {
        self.doc().element_mut(element)?.fill(gradient.clone())
    }
    fn set_object_image_fill(
        &mut self,
        element: u64,
        bytes: &[u8],
        name: &str,
        fit: ImageFit,
    ) -> Result<(), Error> {
        let fill = Fill::Image(ImageFill {
            source: ImageSource::Bytes {
                bytes: bytes.to_vec(),
                name: name.to_string(),
            },
            fit,
        });
        self.doc().element_mut(element)?.fill(fill)
    }
    fn set_object_shadow(&mut self, element: u64, shadow: Option<Shadow>) -> Result<(), Error> {
        let mut handle = self.doc().element_mut(element)?;
        match shadow {
            Some(shadow) => handle.shadow(shadow),
            None => handle.no_shadow(),
        }
    }
    fn set_object_stroke(&mut self, element: u64, colour: Color, width: f32) -> Result<(), Error> {
        self.doc().element_mut(element)?.stroke(colour, width)
    }
    fn set_object_opacity(&mut self, element: u64, opacity: f32) -> Result<(), Error> {
        self.doc().element_mut(element)?.opacity(opacity)
    }

    // -- slides -------------------------------------------------------------
    fn set_slide_skipped(&mut self, identifier: u64, skipped: bool) -> Result<bool, Error> {
        self.doc().slide_mut(slide(identifier))?.skip(skipped)
    }
    fn move_slide(&mut self, identifier: u64, to: usize) -> Result<usize, Error> {
        self.doc().slide_mut(slide(identifier))?.move_to(to)
    }
    fn duplicate_slide(&mut self, identifier: u64) -> Result<SlideCopy, Error> {
        self.doc().slide_mut(slide(identifier))?.duplicate()
    }
    fn set_transition(
        &mut self,
        identifier: u64,
        edit: &TransitionEdit,
    ) -> Result<Transition, Error> {
        self.doc()
            .slide_mut(slide(identifier))?
            .transition_with(edit)
    }
    fn set_slide_background(
        &mut self,
        identifier: u64,
        colour: Option<Color>,
    ) -> Result<(), Error> {
        self.doc().slide_mut(slide(identifier))?.background(colour)
    }
    fn set_presenter_notes(&mut self, identifier: u64, text: &str) -> Result<TextEdit, Error> {
        self.doc().slide_mut(slide(identifier))?.notes(text)
    }
}

impl Flat for Document {
    fn doc(&mut self) -> &mut Document {
        self
    }
}

/// The same for the slide handle's own shortcuts.
pub trait FlatSlide {
    fn add_text_box(&mut self, text: &str, frame: Frame) -> Result<u64, Error>;
    fn add_image(&mut self, bytes: &[u8], name: &str, frame: Frame) -> Result<u64, Error>;
    fn background_gradient(&mut self, gradient: &Gradient) -> Result<(), Error>;
}

impl FlatSlide for iwork::document::SlideMut<'_> {
    fn add_text_box(&mut self, text: &str, frame: Frame) -> Result<u64, Error> {
        self.add(TextBox::new(text).frame(frame))
    }
    fn add_image(&mut self, bytes: &[u8], name: &str, frame: Frame) -> Result<u64, Error> {
        self.add(Image::new(bytes).named(name).frame(frame))
    }
    fn background_gradient(&mut self, gradient: &Gradient) -> Result<(), Error> {
        self.background(gradient.clone())
    }
}

/// What 0.3 called a cell's text look, alignment included; 0.4 has
/// `TextLook` for the characters and `align` for the paragraph.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CellText {
    pub bold: Option<bool>,
    pub italic: Option<bool>,
    pub size: Option<f32>,
    pub font: Option<String>,
    pub colour: Option<Color>,
    pub align: Option<iwork::Align>,
}

impl CellText {
    pub fn bold() -> CellText {
        CellText {
            bold: Some(true),
            ..CellText::default()
        }
    }
    pub fn coloured(colour: Color) -> CellText {
        CellText {
            colour: Some(colour),
            ..CellText::default()
        }
    }
}

pub trait FlatTable {
    fn text_look(
        &mut self,
        cells: impl Into<iwork::table::CellRange>,
        look: &CellText,
    ) -> Result<usize, Error>;
}

impl FlatTable for iwork::document::TableMut<'_> {
    fn text_look(
        &mut self,
        cells: impl Into<iwork::table::CellRange>,
        look: &CellText,
    ) -> Result<usize, Error> {
        let cells = cells.into();
        let characters = TextLook {
            bold: look.bold,
            italic: look.italic,
            size: look.size,
            font: look.font.clone(),
            colour: look.colour,
            ..TextLook::default()
        };
        let mut changed = 0;
        if characters != TextLook::default() {
            changed = self.look(cells.clone(), &characters)?;
        }
        if let Some(align) = look.align {
            changed = changed.max(self.align(cells, align)?);
        }
        Ok(changed)
    }
}
