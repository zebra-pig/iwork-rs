//! `iwork` — inspect and edit Pages, Numbers and Keynote documents.

use std::collections::BTreeMap;
use std::process::ExitCode;

use iwork::pb::{self, Message, Value};
use iwork::{registry, style, Document, Error};

const USAGE: &str = "\
iwork — inspect and edit Apple iWork documents (.pages, .numbers, .key)

  iwork inspect   <file>                   package, components, media, object census
  iwork text      <file>                   every text storage, with its object id
  iwork set-text  <file> <id> <text> <out> replace one text storage
  iwork insert-text <file> <id> <at> <text> <out>
                                           insert text at a character index
  iwork delete-text <file> <id> <from> <to> <out>
                                           delete a range of characters
  iwork storages  <file>                   every text storage and the attribute
                                           tables it carries
  iwork links     <file>                   hyperlinks and smart fields, with the
                                           text each one covers
  iwork objects   <file> [type]            list objects, optionally of one message type
  iwork dump      <file> <id>              one object, field by field
  iwork check     <file>                   look for a broken object graph
  iwork extract   <file> <dir>             write embedded media to a directory
  iwork roundtrip <file> <out>             decode and re-encode every object

Character indices — <at>, <from>, <to> — are UTF-16 code units, the unit iWork
counts text in, so an emoji is two, and ranges are half-open. An edit that
would land inside a surrogate pair, or delete the character an image, a
footnote or a section break is anchored to, is refused by name rather than
performed badly.

drawables and media

  iwork drawables <file>                   every placed object: geometry, style,
                                           media and non-destructive edit state
  iwork media     <file>                   every media file, its digest and its users
  iwork set-geometry <file> <id> <x> <y> [<w> <h>] <out>
                                           move or resize one drawable
  iwork replace-media <file> <id> <image> <out>
                                           swap the bytes an image is drawn from

An <id> is an object identifier as printed by `iwork drawables`; for
replace-media it may also be a media identifier as printed by `iwork media`.
Positions and sizes are in points, and are the rectangle the app reports — for
a cropped image that is the mask's window, not the picture's own rectangle.

tables

  iwork tables    <file>                   every table: size, headers, geometry
  iwork cells     <file> <table> [--raw]   every cell of one table, with type and format
  iwork csv       <file> <table>           one table as CSV
  iwork organise  <file> [<table>]         sort rules, filters, categories,
                                           pivots, conditional highlighting,
                                           custom cell formats
  iwork formulas  <file> [<table>]         every formula, with its cell, its
                                           text and the value the app cached
  iwork set-cell  <file> <table> <cell> <value> <out>
  iwork set-cell  <file> <table> <row> <col> <value> <out>
                                           write one value into one cell

A <table> is an object id, as printed by `iwork tables`, or a table name. A
<cell> is an A1 reference; <row> and <col> are the 0-based indices the API
uses, so `B3` and `2 1` are the same cell.

A cell <value> is text unless it says otherwise: `n:` a number, `b:true` or
`b:false`, `d:2024-03-01T10:30:00Z` a date, `dur:5400` a duration in seconds,
`text:` text that would otherwise look like a prefix, and `empty:` to clear the
cell. A number is stored as a decimal rather than a float, so `n:0.1` is
exactly a tenth.

  iwork set-cell Budget.numbers Sales B3 n:43 out.numbers

Pages document structure

  iwork sections  <file>                   sections, their text ranges, page
                                           numbering and header/footer storages
  iwork structure <file>                   document mode, paper and margins,
                                           page templates, linked text-box
                                           threads, contents lists, footnotes,
                                           bookmarks, columns

Both are Pages-only; a Numbers or Keynote document has no `TP.DocumentArchive`
and they say so. A header or footer is an ordinary text storage, so editing one
is `iwork set-text <file> <storage> <text> <out>` with the id `iwork sections`
prints.

text styles

  iwork styles       <file>                            every text style, with its object id
  iwork style        <file> <id>                       one style: every field, and what uses it
  iwork new-style    <file> <template> <name> <out>    copy a style under a new name
  iwork set-style    <file> <id> <path=value> <out>    set or clear one field of a style
  iwork delete-style <file> <id> [<replacement>] <out> remove a style
  iwork apply-style  <file> <storage> <start> <end> <style> <out>
                                                       point a range of text at a style
  iwork paragraphs   <file> <storage>                  paragraph ranges, for apply-style
  iwork properties                                     every named style property
  iwork set-color    <file> <id> <r> <g> <b> <out>     text colour, everywhere the style keeps it

A <path> is a dotted list of protobuf field numbers, as printed by `iwork
style`, or one of the named properties — `iwork properties` lists them. A
<value> is varint:N, f32:N, f64:N, str:TEXT, hex:BYTES, or empty to remove the
field. Ranges are half-open and counted in UTF-16 code units, the unit iWork
indexes text in.

  iwork set-style Report.pages 3801 font-size=f32:18 out.pages
  iwork set-style Report.pages 3801 11.3=f32:18      out.pages   # the same field
";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let refs: Vec<&str> = args.iter().map(String::as_str).collect();

    let result = match refs.as_slice() {
        ["inspect", file] => inspect(file),
        ["text", file] => text(file),
        ["set-text", file, id, new_text, out] => match id.parse() {
            Ok(id) => set_text(file, id, new_text, out),
            Err(_) => Err(Error::Format(format!("'{id}' is not an object identifier"))),
        },
        ["insert-text", file, id, at, new_text, out] => identifier(id)
            .and_then(|id| index(at).map(|at| (id, at)))
            .and_then(|(id, at)| insert_text(file, id, at as u64, new_text, out)),
        ["delete-text", file, id, from, to, out] => identifier(id)
            .and_then(|id| index(from).map(|f| (id, f)))
            .and_then(|(id, f)| index(to).map(|t| (id, f, t)))
            .and_then(|(id, f, t)| delete_text(file, id, f as u64..t as u64, out)),
        ["storages", file] => storages(file),
        ["links", file] => links(file),
        ["objects", file] => objects(file, None),
        ["objects", file, message_type] => match message_type.parse() {
            Ok(t) => objects(file, Some(t)),
            Err(_) => Err(Error::Format(format!(
                "'{message_type}' is not a message type"
            ))),
        },
        ["dump", file, id] => identifier(id).and_then(|id| dump_object(file, id)),
        ["check", file] => check(file),
        ["extract", file, dir] => extract(file, dir),
        ["roundtrip", file, out] => roundtrip(file, out),
        ["styles", file] => styles(file),
        ["style", file, id] => identifier(id).and_then(|id| show_style(file, id)),
        ["new-style", file, template, name, out] => {
            identifier(template).and_then(|t| new_style(file, t, name, out))
        }
        ["set-style", file, id, assignment, out] => {
            identifier(id).and_then(|id| set_style(file, id, assignment, out))
        }
        ["delete-style", file, id, out] => {
            identifier(id).and_then(|id| delete_style(file, id, None, out))
        }
        ["delete-style", file, id, replacement, out] => identifier(id)
            .and_then(|id| Ok((id, identifier(replacement)?)))
            .and_then(|(id, replacement)| delete_style(file, id, Some(replacement), out)),
        ["apply-style", file, storage, start, end, style, out] => {
            apply_style(file, storage, start, end, style, out)
        }
        ["drawables", file] => drawables(file),
        ["media", file] => media(file),
        ["set-geometry", file, id, x, y, out] => set_geometry(file, id, x, y, None, out),
        ["set-geometry", file, id, x, y, w, h, out] => {
            set_geometry(file, id, x, y, Some((w, h)), out)
        }
        ["replace-media", file, target, image, out] => replace_media(file, target, image, out),
        ["tables", file] => tables(file),
        ["cells", file, table] => cells(file, table, false),
        ["cells", file, table, "--raw"] => cells(file, table, true),
        ["csv", file, table] => csv(file, table),
        ["organise", file] => organise(file, None),
        ["organise", file, table] => organise(file, Some(table)),
        ["formulas", file] => formulas(file, None),
        ["formulas", file, table] => formulas(file, Some(table)),
        ["set-cell", file, table, cell, value, out] => reference_position(cell)
            .and_then(|(row, column)| set_cell(file, table, row, column, value, out)),
        ["set-cell", file, table, row, column, value, out] => index(row)
            .and_then(|row| Ok((row, index(column)?)))
            .and_then(|(row, column)| set_cell(file, table, row, column, value, out)),
        ["sections", file] => sections(file),
        ["structure", file] => structure(file),
        ["properties"] => properties(),
        ["set-color", file, id, r, g, b, out] => set_color(file, id, r, g, b, out),
        ["paragraphs", file, storage] => {
            identifier(storage).and_then(|storage| paragraphs(file, storage))
        }
        _ => {
            eprint!("{USAGE}");
            return ExitCode::from(2);
        }
    };

    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

fn inspect(path: &str) -> Result<(), Error> {
    let doc = Document::open(path)?;
    println!("{} document — {path}", doc.kind().as_str());

    // Pages has two kinds of document and they are not the same file at all:
    // one has a body storage the text flows through, the other has named page
    // templates and text only in boxes. Saying which, first, saves a reader the
    // question.
    if let Some(structure) = doc.structure() {
        println!(
            "{}, {} section(s), {:.0} × {:.0} pt{}",
            structure.mode.as_str(),
            structure.sections.len(),
            structure.setup.width,
            structure.setup.height,
            if structure.setup.facing_pages {
                ", facing pages"
            } else {
                ""
            }
        );
    }

    println!("\n== package entries ==");
    for (name, data) in &doc.package().entries {
        println!("  {name:<48} {:>10} bytes", data.len());
    }

    let components = doc.components();
    if !components.is_empty() {
        println!("\n== components ({}) ==", components.len());
        for c in &components {
            println!(
                "  id={:<8} {:<28} {:<34} refs={}",
                c.identifier,
                c.preferred_name,
                c.stream_name(),
                c.external_reference_count
            );
        }
    }

    let data_files = doc.data_files();
    if !data_files.is_empty() {
        println!("\n== media ({}) ==", data_files.len());
        for d in &data_files {
            match d.entry_name() {
                Some(entry) => println!("  id={:<5} {entry}", d.identifier),
                None => println!(
                    "  id={:<5} (theme asset, not stored) {} {}",
                    d.identifier, d.original_name, d.asset_path
                ),
            }
        }
    }

    if let Some(last) = doc.last_object_identifier() {
        println!("\nhighest allocated object identifier: {last}");
    }

    println!("\n== objects by message type ==");
    let mut census: BTreeMap<u32, usize> = BTreeMap::new();
    for (_, object) in doc.objects() {
        for message in &object.messages {
            *census.entry(message.message_type).or_default() += 1;
        }
    }
    for (message_type, count) in &census {
        println!(
            "  {message_type:>6} x{count:<5} {}",
            registry::describe_in(doc.kind(), *message_type)
        );
    }
    println!(
        "\n{} objects across {} streams; names ending in ? are inferred, ?? unverified",
        census.values().sum::<usize>(),
        doc.stream_names().count()
    );
    Ok(())
}

fn text(path: &str) -> Result<(), Error> {
    let doc = Document::open(path)?;
    for storage in doc.text_storages() {
        println!("--- {} storage {} ---", storage.stream, storage.identifier);
        println!("{}", storage.text);
    }
    Ok(())
}

fn set_text(path: &str, identifier: u64, new_text: &str, out: &str) -> Result<(), Error> {
    let mut doc = Document::open(path)?;
    report_edit(doc.set_text(identifier, new_text)?);
    save(&doc, out)
}

fn insert_text(
    path: &str,
    identifier: u64,
    at: u64,
    new_text: &str,
    out: &str,
) -> Result<(), Error> {
    let mut doc = Document::open(path)?;
    report_edit(doc.insert_text(identifier, at, new_text)?);
    save(&doc, out)
}

fn delete_text(
    path: &str,
    identifier: u64,
    range: std::ops::Range<u64>,
    out: &str,
) -> Result<(), Error> {
    let mut doc = Document::open(path)?;
    report_edit(doc.delete_text(identifier, range)?);
    save(&doc, out)
}

fn storages(path: &str) -> Result<(), Error> {
    let doc = Document::open(path)?;
    for storage in doc.storages() {
        println!(
            "  {:<9} {:<18} {:>6} unit(s), {} paragraph(s)   {}",
            storage.identifier,
            storage.kind_name(),
            storage.length,
            storage.paragraphs,
            storage.stream
        );
        for table in &storage.tables {
            println!(
                "      {:>2} {:<28} {:<10} {} entr(y|ies)",
                table.field,
                table.name,
                format!("{:?}", table.anchoring).to_lowercase(),
                table.entries
            );
        }
        if let Some(field) = storage.unknown_field {
            println!("      !! field {field} is not a table this crate knows — edits refused");
        }
    }
    Ok(())
}

fn links(path: &str) -> Result<(), Error> {
    let doc = Document::open(path)?;
    let fields = doc.smart_fields();
    if fields.is_empty() {
        println!("no smart fields");
        return Ok(());
    }
    for field in fields {
        println!(
            "  storage {:<9} {:>5}..{:<5} {:<34} {:?}",
            field.storage,
            field.range.start,
            field.range.end,
            registry::lookup(field.message_type)
                .map(|e| e.name)
                .unwrap_or("unknown"),
            field.text
        );
        if let Some(payload) = &field.payload {
            let arrow = if field.message_type == 2032 {
                "->"
            } else {
                " ="
            };
            println!("      {arrow} {payload}   (object {})", field.object);
        }
    }
    Ok(())
}

/// What the edit did to the things anchored into the storage. Printed because
/// it is the whole point: an edit that moves nothing has probably missed
/// something, and one that drops a great deal has probably taken too much.
fn report_edit(edit: iwork::TextEdit) {
    let tables: Vec<String> = edit
        .report
        .tables
        .iter()
        .map(|field| match iwork::text::table(*field) {
            Some(t) => format!("{field} {}", t.name),
            None => field.to_string(),
        })
        .collect();
    println!(
        "storage {}: {} unit(s) removed at {}, {} inserted; \
         {} run(s) moved, {} dropped, {} added",
        edit.storage,
        edit.edit.removed,
        edit.edit.at,
        edit.edit.inserted,
        edit.report.moved,
        edit.report.dropped,
        edit.report.added
    );
    if !tables.is_empty() {
        println!("  tables rewritten: {}", tables.join(", "));
    }
}

/// Write the document and say which streams that actually rewrote.
///
/// Worth printing: it is the difference between "I edited a style" and "I
/// rewrote the whole document", and only one of those is easy to review.
fn save(doc: &Document, out: &str) -> Result<(), Error> {
    let changed = doc.changed_streams();
    doc.save(out)?;
    match changed.len() {
        0 => println!("wrote {out} (no stream changed)"),
        n => println!(
            "wrote {out} ({n} of {} streams rewritten: {})",
            doc.stream_names().count(),
            changed.join(", ")
        ),
    }
    Ok(())
}

fn objects(path: &str, filter: Option<u32>) -> Result<(), Error> {
    let doc = Document::open(path)?;
    for (stream, object) in doc.objects() {
        let message_type = object.message_type();
        if filter.is_some_and(|t| t != message_type) {
            continue;
        }
        println!(
            "  id={:<8} type={:<6} {:<34} {:>7} bytes  {stream}",
            object.identifier,
            message_type,
            registry::describe_in(doc.kind(), message_type),
            object.payload().len()
        );
    }
    Ok(())
}

/// One line of preview text, with the characters that are not text named.
fn oneline(text: &str, width: usize) -> String {
    let mut out = String::new();
    for character in text.chars() {
        if out.chars().count() >= width {
            out.push('…');
            break;
        }
        match character {
            '\n' | '\r' | '\u{000C}' => out.push('⏎'),
            '\u{FFFC}' => out.push('▨'),
            '\u{0004}' => out.push('§'),
            '\u{0005}' => out.push('¶'),
            '\t' => out.push('→'),
            c => out.push(c),
        }
    }
    out
}

fn sections(path: &str) -> Result<(), Error> {
    let doc = Document::open(path)?;
    let Some(structure) = doc.structure() else {
        println!("not a Pages document — sections are a TP archive");
        return Ok(());
    };
    let body = structure
        .body_storage
        .map(|id| doc.storage_text(id).unwrap_or_default())
        .unwrap_or_default();

    println!(
        "{} document, {} section(s)",
        structure.mode.as_str(),
        structure.sections.len()
    );
    for section in &structure.sections {
        let name = if section.name.is_empty() {
            String::new()
        } else {
            format!(" \"{}\"", section.name)
        };
        println!(
            "\nsection {} id={}{} — [{}, {}), {} unit(s)",
            section.index + 1,
            section.identifier,
            name,
            section.start,
            section.end,
            section.length()
        );
        let text = slice_units(&body, section.start, section.end);
        if !text.is_empty() {
            println!("  text  {}", oneline(&text, 64));
        }
        println!("  pages {}", section.numbering());
        let mut switches = Vec::new();
        if section.inherits_header_footer {
            switches.push("inherits the previous header and footer");
        }
        if section.first_page_different {
            switches.push("first page different");
        }
        if section.even_odd_different {
            switches.push("even and odd pages different");
        }
        if section.hides_header_footer_on_first_page {
            switches.push("first page hides the header and footer");
        }
        if section.has_background {
            switches.push("has a background fill");
        }
        if section.hyperlink_uuid {
            switches.push("can be linked to");
        }
        if !switches.is_empty() {
            println!("  {}", switches.join(", "));
        }
        for (slot, page) in iwork::pages::TemplatePage::ALL.iter().enumerate() {
            let Some(template) = section.templates[slot] else {
                continue;
            };
            let zones: Vec<String> = structure
                .header_footers
                .iter()
                .filter(|hf| hf.section_template == template)
                .map(|hf| {
                    let text = if hf.text.is_empty() {
                        "empty".to_string()
                    } else {
                        format!("\"{}\"", oneline(&hf.text, 28))
                    };
                    // A lone ▨ is not much of a report; say what stands there.
                    let numbers: Vec<String> = hf
                        .numbers
                        .iter()
                        .map(|n| format!("{} as {}", n.kind_name(), n.format_name))
                        .collect();
                    format!(
                        "    {:<6} {:<6} id={:<8} {text}{}{}",
                        hf.kind(),
                        hf.zone.as_str(),
                        hf.storage,
                        if numbers.is_empty() { "" } else { " — " },
                        numbers.join(", ")
                    )
                })
                .collect();
            println!("  {} page template id={template}", page.as_str());
            for zone in zones {
                println!("{zone}");
            }
        }
    }
    Ok(())
}

/// UTF-16 code units `start..end` of a string, for printing a section.
fn slice_units(text: &str, start: u64, end: u64) -> String {
    let units: Vec<u16> = text.encode_utf16().collect();
    let from = (start as usize).min(units.len());
    let to = (end as usize).min(units.len());
    String::from_utf16_lossy(&units[from..to])
}

fn structure(path: &str) -> Result<(), Error> {
    let doc = Document::open(path)?;
    let Some(s) = doc.structure() else {
        println!("not a Pages document — the structure is a TP archive");
        return Ok(());
    };

    println!("mode      {}", s.mode.as_str());
    println!(
        "paper     {:.0} × {:.0} pt {} ({}), scale {}",
        s.setup.width,
        s.setup.height,
        if s.setup.portrait() {
            "portrait"
        } else {
            "landscape"
        },
        if s.setup.paper_id.is_empty() {
            "no paper id"
        } else {
            &s.setup.paper_id
        },
        s.setup.scale
    );
    println!(
        "margins   left {:.0} right {:.0} top {:.0} bottom {:.0}, header {:.0} footer {:.0}",
        s.setup.left_margin,
        s.setup.right_margin,
        s.setup.top_margin,
        s.setup.bottom_margin,
        s.setup.header_margin,
        s.setup.footer_margin
    );
    let mut switches = Vec::new();
    if s.setup.facing_pages {
        switches.push("facing pages".to_string());
    }
    if s.setup.single_header_footer {
        switches.push("one header and footer for the whole document".to_string());
    }
    if !s.setup.headers_shown {
        switches.push("headers hidden".to_string());
    }
    if !s.setup.footers_shown {
        switches.push("footers hidden".to_string());
    }
    if s.setup.rtl {
        switches.push("right to left".to_string());
    }
    if s.setup.body_vertical {
        switches.push("body laid out vertically".to_string());
    }
    if !s.setup.language.is_empty() {
        switches.push(format!("language {}", s.setup.language));
    }
    if !s.setup.template.is_empty() {
        switches.push(format!("from template {}", s.setup.template));
    }
    if !switches.is_empty() {
        println!("          {}", switches.join(", "));
    }

    println!("sections  {} — `iwork sections` for each", s.sections.len());
    let with_text = s.header_footers.iter().filter(|hf| !hf.text.is_empty());
    println!(
        "headers   {} storage(s), {} with text",
        s.header_footers.len(),
        with_text.count()
    );

    if s.page_templates.is_empty() {
        println!("templates none — a word-processing document has no page templates");
    } else {
        println!("templates {}", s.page_templates.len());
        for template in &s.page_templates {
            let mut notes = Vec::new();
            if template.matches_previous_page {
                notes.push("matches the previous page");
            }
            if template.hides_headers_footers {
                notes.push("hides headers and footers");
            }
            if template.has_background {
                notes.push("has a background fill");
            }
            println!(
                "    id={:<8} \"{}\" — {} drawable(s), {} placeholder(s){}{}",
                template.identifier,
                template.name,
                template.drawables,
                template.placeholders,
                if notes.is_empty() { "" } else { "; " },
                notes.join(", ")
            );
        }
    }

    if s.threads.is_empty() {
        println!("threads   none");
    } else {
        println!("threads   {}", s.threads.len());
        for thread in &s.threads {
            let boxes: Vec<String> = thread.boxes.iter().map(u64::to_string).collect();
            println!(
                "    id={:<8} number {} — storage {} through {} box(es): {}",
                thread.identifier,
                thread.number,
                thread
                    .storage
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| "none".into()),
                thread.boxes.len(),
                boxes.join(" → ")
            );
        }
    }

    if s.contents.is_empty() {
        println!("contents  none");
    } else {
        println!("contents  {} settings archive(s)", s.contents.len());
        for toc in &s.contents {
            let shown = toc.rules.iter().filter(|r| r.show).count();
            println!(
                "    id={:<8} \"{}\" scope {} — {} of {} paragraph style(s) included{}",
                toc.identifier,
                toc.name,
                toc.scope,
                shown,
                toc.rules.len(),
                match toc.placed_in {
                    Some(owner) => format!(", placed in {owner}"),
                    None => ", the document's own".to_string(),
                }
            );
            for (heading, page) in &toc.entries {
                println!("      p{page:<4} {heading}");
            }
        }
    }

    let notes = &s.footnote_settings;
    println!(
        "footnotes {} as {}, numbered {}, gap {} pt — {} in the text",
        notes.kind_name(),
        notes.format_name(),
        notes.numbering_name(),
        notes.gap,
        if s.footnotes.is_empty() {
            "none".to_string()
        } else {
            s.footnotes.len().to_string()
        }
    );
    for note in &s.footnotes {
        println!(
            "    storage {} at {} → {} {}",
            note.storage,
            note.index,
            note.body
                .map(|b| b.to_string())
                .unwrap_or_else(|| "no body".into()),
            oneline(&note.text, 48)
        );
    }

    println!(
        "bookmarks {}",
        if s.bookmarks.is_empty() {
            "none".to_string()
        } else {
            s.bookmarks.len().to_string()
        }
    );
    for (storage, index, object) in &s.bookmarks {
        println!(
            "    storage {storage} at {index} → {}",
            object
                .map(|o| o.to_string())
                .unwrap_or_else(|| "nothing".into())
        );
    }

    if let Some(body) = s.body_storage {
        let layouts = doc.column_layouts(body);
        if layouts.is_empty() {
            println!("columns   none recorded on the body storage");
        } else {
            println!("columns   {} layout(s) on the body storage", layouts.len());
            for layout in &layouts {
                // Widths and gaps are fractions of the text width, not
                // points, so they are printed as percentages.
                let shape = match (&layout.equal, &layout.unequal) {
                    (Some((count, gap)), _) => {
                        format!("{count} equal column(s), gap {:.1}%", gap * 100.0)
                    }
                    (_, Some(_)) => {
                        let parts: Vec<String> = layout
                            .fractions()
                            .iter()
                            .map(|f| format!("{:.1}%", f * 100.0))
                            .collect();
                        format!(
                            "{} unequal column(s): {}",
                            layout.count(),
                            parts.join(" | ")
                        )
                    }
                    _ if layout.none => "no columns".to_string(),
                    _ => "one column".to_string(),
                };
                println!(
                    "    [{}, {}) style {} — {shape}",
                    layout.start,
                    layout.end,
                    layout
                        .style
                        .map(|s| s.to_string())
                        .unwrap_or_else(|| "none".into())
                );
            }
        }
    }
    Ok(())
}

/// The named style properties, with how far each name can be trusted.
fn tables(path: &str) -> Result<(), Error> {
    let doc = Document::open(path)?;
    let tables = doc.tables();
    if tables.is_empty() {
        println!("no tables");
        return Ok(());
    }
    for table in &tables {
        let where_ = match &table.sheet {
            Some(sheet) => format!("sheet {sheet}"),
            None => table.stream.clone(),
        };
        println!(
            "table {:<8} {:<24} {}×{} in {where_}",
            table.identifier, table.name, table.rows, table.columns
        );
        println!(
            "  headers: {} row(s){}, {} column(s){}, {} footer row(s)",
            table.header_rows,
            if table.header_rows_frozen {
                " frozen"
            } else {
                ""
            },
            table.header_columns,
            if table.header_columns_frozen {
                " frozen"
            } else {
                ""
            },
            table.footer_rows
        );
        let hidden_rows = hidden_list(&table.row_extents);
        let hidden_columns = hidden_list(&table.column_extents);
        println!(
            "  hidden: {} row(s) ({} by the user, {} by a filter), {} column(s) ({} by the user)",
            table.hidden_rows,
            table.user_hidden_rows,
            table.filtered_rows,
            table.hidden_columns,
            table.user_hidden_columns
        );
        if !hidden_rows.is_empty() {
            println!("  hidden rows: {}", hidden_rows.join(", "));
        }
        if !hidden_columns.is_empty() {
            println!("  hidden columns: {}", hidden_columns.join(", "));
        }
        println!(
            "  default size: {} × {} pt; {} stored cell(s)",
            table.default_column_width,
            table.default_row_height,
            table.cells().len()
        );
        let sized_rows = table
            .row_extents
            .iter()
            .filter(|e| e.size.is_some())
            .count();
        let sized_columns = table
            .column_extents
            .iter()
            .filter(|e| e.size.is_some())
            .count();
        if sized_rows + sized_columns > 0 {
            println!("  explicitly sized: {sized_rows} row(s), {sized_columns} column(s)");
        }
        for merge in &table.merges {
            println!(
                "  merged: {} spanning {} row(s) × {} column(s)",
                reference_name(merge.row, merge.column),
                merge.rows,
                merge.columns
            );
        }
        organisation_summary(table, "  ");
        for problem in &table.problems {
            println!("  problem: {problem}");
        }
    }
    Ok(())
}

fn drawables(path: &str) -> Result<(), Error> {
    let doc = Document::open(path)?;
    let all = doc.drawables();
    if all.is_empty() {
        println!("no drawables");
        return Ok(());
    }
    let by_id: BTreeMap<u64, &iwork::Drawable> = all.iter().map(|d| (d.identifier, d)).collect();
    let mut place = String::new();

    for drawable in &all {
        let where_ = drawable.placement.as_str();
        if where_ != place {
            println!("== {where_} ==");
            place = where_;
        }
        let mask = drawable.mask().and_then(|id| by_id.get(&id)).copied();
        let frame = drawable.frame(mask);
        let mut what = drawable.kind.as_str().to_string();
        if drawable
            .path_source
            .as_ref()
            .is_some_and(|p| p.looks_like_a_line())
        {
            what = "line".to_string();
        }
        println!(
            "  {:<8} z{:<3} {:<12} {:>8.1},{:<8.1} {:>8.1} × {:<8.1}{}",
            drawable.identifier,
            drawable.z,
            what,
            frame.x,
            frame.y,
            frame.width,
            frame.height,
            match drawable.geometry.angle {
                angle if angle != 0.0 => format!("  {angle:.1}°"),
                _ => String::new(),
            }
        );
        let mut notes: Vec<String> = Vec::new();
        if drawable.locked {
            notes.push("locked".into());
        }
        if drawable.aspect_ratio_locked {
            notes.push("aspect locked".into());
        }
        if drawable.geometry.flags != 3 {
            notes.push(format!("geometry flags {}", drawable.geometry.flags));
        }
        if let Some(parent) = drawable.parent {
            notes.push(format!("parent {parent}"));
        }
        if let Some(text) = drawable.text {
            notes.push(format!("text storage {text}"));
        }
        if let Some(comment) = drawable.comment {
            notes.push(format!("comment {comment}"));
        }
        if !drawable.pencil_annotations.is_empty() {
            notes.push(format!(
                "{} pencil annotation(s)",
                drawable.pencil_annotations.len()
            ));
        }
        if !notes.is_empty() {
            println!("      {}", notes.join(", "));
        }
        if let Some(link) = &drawable.hyperlink {
            println!("      link {link}");
        }
        if let Some(description) = &drawable.description {
            println!("      description {description:?}");
        }
        if !drawable.children.is_empty() {
            let list: Vec<String> = drawable.children.iter().map(u64::to_string).collect();
            println!("      children (back to front): {}", list.join(", "));
        }
        if let Some(source) = &drawable.path_source {
            println!(
                "      path: {:?}{}{}",
                source.kind,
                match source.natural_size {
                    Some((w, h)) => format!(" natural {w:.1} × {h:.1}"),
                    None => String::new(),
                },
                match source.elements {
                    0 => String::new(),
                    n => format!(", {n} element(s)"),
                }
            );
        }
        if let Some(media) = &drawable.media {
            let name = media
                .data
                .and_then(|id| doc.data_files().into_iter().find(|d| d.identifier == id))
                .map(|d| {
                    if d.stored_name.is_empty() {
                        format!("{} (theme asset)", d.original_name)
                    } else {
                        format!("Data/{}", d.stored_name)
                    }
                })
                .unwrap_or_else(|| "none".to_string());
            println!(
                "      media: {name}{}{}{}",
                match media.natural_size {
                    Some((w, h)) => format!(", natural {w:.0} × {h:.0}"),
                    None => String::new(),
                },
                if media.is_placeholder() {
                    ", placeholder"
                } else {
                    ""
                },
                if media.was_replaced() {
                    ", replaced"
                } else {
                    ""
                }
            );
            let mut playback: Vec<String> = Vec::new();
            if let Some((start, end)) = media.trim {
                playback.push(format!("trimmed {start:.2}s–{end:.2}s"));
            }
            if let Some(time) = media.poster_time {
                playback.push(format!("poster at {time:.2}s"));
            }
            if let Some(volume) = media.volume {
                playback.push(format!("volume {:.0}%", volume * 100.0));
            }
            if let Some(option) = media.loop_option {
                playback.push(format!("loop {option}"));
            }
            if media.audio_only {
                playback.push("audio only".into());
            }
            if media.live_video {
                playback.push("live video source".into());
            }
            if let Some(url) = &media.remote_url {
                playback.push(format!("streams from {url}"));
            }
            if !playback.is_empty() {
                println!("      playback: {}", playback.join(", "));
            }
        }
        if !drawable.extensions.is_empty() {
            println!("      carries: {}", drawable.extensions.join(", "));
        }
        if let Some(state) = &drawable.edit_state {
            if let Some(mask) = state.mask {
                println!(
                    "      mask {mask}{}",
                    if state.crops {
                        " (crops)"
                    } else {
                        " (identity)"
                    }
                );
            }
            for objection in state.objections() {
                println!("      edit state: {objection}");
            }
        }
        if let Some(style) = drawable.style.and_then(|id| doc.object_style(id)) {
            let mut parts: Vec<String> = Vec::new();
            if let Some(name) = &style.name {
                parts.push(format!("{name:?}"));
            }
            if let Some(fill) = &style.fill {
                parts.push(match fill {
                    iwork::drawable::Fill::Color(colour) => format!("fill {colour}"),
                    other => format!("fill {}", other.as_str()),
                });
            }
            if let Some(stroke) = &style.stroke {
                parts.push(format!(
                    "stroke {:?} {:.1}pt{}",
                    stroke.pattern,
                    stroke.width,
                    match stroke.color {
                        Some(colour) => format!(" {colour}"),
                        None => String::new(),
                    }
                ));
            }
            if let Some(opacity) = style.opacity {
                parts.push(format!("opacity {:.0}%", opacity * 100.0));
            }
            if let Some(shadow) = &style.shadow {
                parts.push(format!(
                    "shadow {}{:.0}° {:.0}pt r{}",
                    if shadow.enabled { "" } else { "off " },
                    shadow.angle,
                    shadow.offset,
                    shadow.radius
                ));
            }
            if let Some(reflection) = style.reflection {
                parts.push(format!("reflection {:.0}%", reflection * 100.0));
            }
            println!(
                "      style {}{}: {}",
                style.identifier,
                match style.override_count {
                    Some(n) if n > 0 => format!(" (+{n} override(s))"),
                    _ => String::new(),
                },
                if parts.is_empty() {
                    "nothing this crate reads".to_string()
                } else {
                    parts.join(", ")
                }
            );
        }
    }
    println!("\n{} drawable(s)", all.len());
    Ok(())
}

/// Every media file the package registers, and what points at it.
fn media(path: &str) -> Result<(), Error> {
    let doc = Document::open(path)?;
    let files = doc.data_files();
    if files.is_empty() {
        println!("no media");
        return Ok(());
    }
    let drawables = doc.drawables();
    for file in &files {
        let users: Vec<String> = drawables
            .iter()
            .filter(|d| d.media.as_ref().and_then(|m| m.data) == Some(file.identifier))
            .map(|d| format!("{} ({})", d.identifier, d.kind.as_str()))
            .collect();
        let posters: Vec<String> = drawables
            .iter()
            .filter(|d| d.media.as_ref().and_then(|m| m.poster) == Some(file.identifier))
            .map(|d| format!("{} poster", d.identifier))
            .collect();
        let stored = match file.entry_name() {
            Some(entry) => match doc.package().get(&entry) {
                Some(bytes) => format!("{entry} ({} bytes)", bytes.len()),
                None => format!("{entry} — MISSING from the package"),
            },
            None => format!(
                "{} in {} (theme asset)",
                file.original_name, file.asset_path
            ),
        };
        println!("  {:<6} {stored}", file.identifier);
        println!(
            "         digest {} {}",
            file.digest
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>(),
            match file
                .entry_name()
                .and_then(|e| doc.package().get(&e).map(iwork::media::sha1))
            {
                Some(actual) if actual.as_slice() == file.digest => "(matches the bytes)",
                Some(_) => "(DOES NOT match the bytes)",
                None => "",
            }
        );
        let used = [users, posters].concat();
        if !used.is_empty() {
            println!("         used by {}", used.join(", "));
        }
    }
    println!("\n{} media file(s)", files.len());
    Ok(())
}

fn set_geometry(
    path: &str,
    id: &str,
    x: &str,
    y: &str,
    size: Option<(&str, &str)>,
    out: &str,
) -> Result<(), Error> {
    let identifier = identifier(id)?;
    let number = |text: &str| -> Result<f32, Error> {
        text.parse::<f32>()
            .map_err(|_| Error::Format(format!("'{text}' is not a number of points")))
    };
    let position = Some((number(x)?, number(y)?));
    let size = match size {
        Some((w, h)) => Some((number(w)?, number(h)?)),
        None => None,
    };

    let mut doc = Document::open(path)?;
    let before = doc
        .drawable(identifier)
        .ok_or(Error::NoSuchObject(identifier))?;
    if before.locked {
        println!("note: drawable {identifier} is locked, so the app will not let a user move it");
    }
    let change = doc.set_geometry(identifier, position, size)?;
    println!(
        "{} {}: {:.1},{:.1} {:.1} × {:.1} -> {:.1},{:.1} {:.1} × {:.1}",
        before.kind.as_str(),
        change.drawable,
        change.before.x,
        change.before.y,
        change.before.width,
        change.before.height,
        change.after.x,
        change.after.y,
        change.after.width,
        change.after.height
    );
    if let Some(mask) = change.mask {
        println!("  mask {mask} scaled with it");
    }
    save(&doc, out)
}

fn replace_media(path: &str, target: &str, image: &str, out: &str) -> Result<(), Error> {
    let target = identifier(target)?;
    let bytes = std::fs::read(image)?;
    let name = std::path::Path::new(image)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("replacement");

    let mut doc = Document::open(path)?;
    let replacement = doc.replace_media(target, &bytes, name, None)?;
    println!(
        "media {}: {} -> {} ({} bytes, {:.0} × {:.0})",
        replacement.data,
        replacement.was,
        replacement.now,
        replacement.bytes,
        replacement.new_pixel_size.0,
        replacement.new_pixel_size.1
    );
    println!(
        "  digest {}",
        replacement
            .digest
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>()
    );
    if !replacement.drawables.is_empty() {
        let list: Vec<String> = replacement.drawables.iter().map(u64::to_string).collect();
        println!("  drawable(s) brought into step: {}", list.join(", "));
    }
    if let Some(note) = replacement.aspect_note() {
        println!("  warning: {note}");
    }
    save(&doc, out)
}

/// Hidden rows or columns, each with the reason the model gives.
fn hidden_list(extents: &[iwork::table::Extent]) -> Vec<String> {
    extents
        .iter()
        .enumerate()
        .filter(|(_, e)| e.hidden())
        .map(|(i, e)| format!("{i} ({})", e.hiding().as_str()))
        .collect()
}

/// The one-line-per-feature view `iwork tables` shows; `iwork organise` prints
/// the same things in full.
fn organisation_summary(table: &iwork::Table, indent: &str) {
    if !table.sort_rules.is_empty() {
        let rules: Vec<String> = table
            .sort_rules
            .iter()
            .map(|rule| {
                format!(
                    "{} {}",
                    column_name(rule.column),
                    if rule.descending { "desc" } else { "asc" }
                )
            })
            .collect();
        println!("{indent}sorted by: {}", rules.join(", "));
    }
    if let Some(filter) = &table.filter {
        println!(
            "{indent}filter: {} rule(s), match {}, {}",
            filter.rules.len(),
            if filter.match_any { "any" } else { "all" },
            if filter.enabled { "on" } else { "off" }
        );
    }
    for category in &table.categories {
        let columns: Vec<String> = category
            .columns
            .iter()
            .map(|c| column_name(c.column.unwrap_or(usize::MAX)))
            .collect();
        println!(
            "{indent}category: by {}, {} group(s), {} summary assignment(s), {}",
            columns.join(" then "),
            category.groups().len(),
            category.summaries.len(),
            if category.enabled { "on" } else { "off" }
        );
    }
    if let Some(pivot) = &table.pivot {
        println!(
            "{indent}pivot of {:?}: {} row field(s), {} column field(s), {} value(s){}",
            pivot.source_name,
            pivot.rows.len(),
            pivot.columns.len(),
            pivot.values.len(),
            if pivot.empty { ", empty" } else { "" }
        );
    }
    let conditional: usize = table
        .conditional_styles
        .iter()
        .map(|set| set.rules.len())
        .sum();
    if conditional > 0 {
        println!(
            "{indent}conditional highlighting: {conditional} rule(s) in {} set(s)",
            table.conditional_styles.len()
        );
    }
}

/// A column index as the letter the app shows, or `?` when a UUID did not
/// resolve to one.
fn column_name(column: usize) -> String {
    if column == usize::MAX {
        return "?".to_string();
    }
    let mut name = String::new();
    let mut n = column + 1;
    while n > 0 {
        let digit = (n - 1) % 26;
        name.insert(0, (b'A' + digit as u8) as char);
        n = (n - 1) / 26;
    }
    name
}

/// Everything a table is organised by, in full.
fn organise(path: &str, wanted: Option<&str>) -> Result<(), Error> {
    let doc = Document::open(path)?;
    let formats = doc.custom_formats();
    if !formats.is_empty() {
        println!("== custom cell formats ({}) ==", formats.len());
        for format in &formats {
            println!(
                "  {:<24} format {} {:?}{}",
                format.name,
                format.format_type,
                format.format_string,
                match format.conditions {
                    0 => String::new(),
                    n => format!(", {n} conditional sub-rule(s)"),
                }
            );
        }
        println!();
    }

    // Every table, always: a rule's condition is a formula, and a formula
    // resolves names against the whole document.
    let tables = doc.tables();
    let index = Some(iwork::table::names(&tables));
    let chosen: Vec<usize> = tables
        .iter()
        .enumerate()
        .filter(|(_, table)| match wanted {
            Some(name) => table.name == name || table.identifier.to_string() == name,
            None => true,
        })
        .map(|(at, _)| at)
        .collect();
    if let (Some(name), true) = (wanted, chosen.is_empty()) {
        return Err(Error::Format(format!("no table called '{name}' in {path}")));
    }
    if chosen.is_empty() {
        println!("no tables");
        return Ok(());
    }

    for position in chosen {
        let table = &tables[position];
        println!("== table {} {} ==", table.identifier, table.name);
        let hidden_rows = hidden_list(&table.row_extents);
        let hidden_columns = hidden_list(&table.column_extents);
        if !hidden_rows.is_empty() {
            println!("  hidden rows: {}", hidden_rows.join(", "));
        }
        if !hidden_columns.is_empty() {
            println!("  hidden columns: {}", hidden_columns.join(", "));
        }
        if !table.row_states.user_hidden.is_empty() || !table.row_states.filtered.is_empty() {
            println!(
                "  row hidden-state extent: {} user-hidden, {} filtered",
                table.row_states.user_hidden.len(),
                table.row_states.filtered.len()
            );
        }
        if !table.column_states.user_hidden.is_empty() || !table.column_states.filtered.is_empty() {
            println!(
                "  column hidden-state extent: user-hidden {:?}, filtered {:?}",
                table.column_states.user_hidden, table.column_states.filtered
            );
        }
        for rule in &table.sort_rules {
            println!(
                "  sort: column {} {}",
                column_name(rule.column),
                if rule.descending {
                    "descending"
                } else {
                    "ascending"
                }
            );
        }
        if let Some(filter) = &table.filter {
            println!(
                "  filter set {} — match {}, {}",
                filter.identifier,
                if filter.match_any { "any" } else { "all" },
                if filter.enabled {
                    "filters on"
                } else {
                    "filters off"
                }
            );
            for (at, rule) in filter.rules.iter().enumerate() {
                println!(
                    "    rule {at}: column {}, {}, {}",
                    rule.column.map(column_name).unwrap_or("?".into()),
                    if rule.enabled { "enabled" } else { "disabled" },
                    describe_in(&rule.predicate, index.as_ref(), position, rule.column)
                );
            }
        }
        for category in &table.categories {
            println!(
                "  category {} — {}",
                category.identifier,
                if category.enabled { "on" } else { "off" }
            );
            for column in &category.columns {
                println!(
                    "    grouped by column {} (grouping type {}{})",
                    column.column.map(column_name).unwrap_or("?".into()),
                    column.grouping_type,
                    if column.has_functor { ", bucketed" } else { "" }
                );
            }
            for summary in &category.summaries {
                println!(
                    "    summary: column {} at level {} = {}",
                    summary.column.map(column_name).unwrap_or("?".into()),
                    summary.level,
                    summary.function
                );
            }
            print_groups(category, 0);
        }
        if let Some(pivot) = &table.pivot {
            println!(
                "  pivot {} of table {:?}{}",
                pivot.identifier,
                pivot.source_name,
                if pivot.empty { " — empty" } else { "" }
            );
            for (label, fields) in [("rows", &pivot.rows), ("columns", &pivot.columns)] {
                for field in fields {
                    println!(
                        "    {label}: column {} (grouping type {}{})",
                        field.column.map(column_name).unwrap_or("?".into()),
                        field.grouping_type,
                        if field.has_functor { ", bucketed" } else { "" }
                    );
                }
            }
            for value in &pivot.values {
                println!(
                    "    value: column {} = {}{}",
                    value.column.map(column_name).unwrap_or("?".into()),
                    value.function,
                    match value.show_as {
                        0 => String::new(),
                        n => format!(" shown as {n}"),
                    }
                );
            }
            println!(
                "    grand totals: rows {}, columns {}",
                if pivot.hide_grand_total_rows {
                    "hidden"
                } else {
                    "shown"
                },
                if pivot.hide_grand_total_columns {
                    "hidden"
                } else {
                    "shown"
                }
            );
        }
        for set in &table.conditional_styles {
            println!(
                "  conditional highlighting set {} (key {})",
                set.identifier,
                set.key.map(|k| k.to_string()).unwrap_or("—".into())
            );
            for rule in &set.rules {
                println!(
                    "    {} -> cell style {:?}, text style {:?}",
                    describe_in(&rule.predicate, index.as_ref(), position, None),
                    rule.cell_style,
                    rule.text_style
                );
            }
        }
    }
    Ok(())
}

fn print_groups(category: &iwork::table::Category, _depth: usize) {
    let Some(root) = &category.root else { return };
    fn walk(group: &iwork::table::Group, depth: usize) {
        for child in &group.children {
            println!(
                "    {}group {:?}: {} row(s){}",
                "  ".repeat(depth),
                child
                    .value
                    .as_ref()
                    .map(|v| v.to_text())
                    .unwrap_or_default(),
                child.rows.len(),
                if child.collapsed { ", collapsed" } else { "" }
            );
            walk(child, depth + 1);
        }
    }
    walk(root, 0);
}

/// A predicate in one line: its code, whatever it compares against, and — when
/// the condition is a formula — the formula itself.
fn describe_in(
    predicate: &iwork::table::Predicate,
    index: Option<&iwork::formula::Names>,
    table: usize,
    column: Option<usize>,
) -> String {
    let mut text = format!("predicate {}", predicate.kind);
    if predicate.qualifiers != (0, 0) {
        text.push_str(&format!(" {:?}", predicate.qualifiers));
    }
    for value in &predicate.values {
        text.push_str(&format!(" {:?}", value.to_text()));
    }
    match index.and_then(|index| {
        iwork::table::predicate_text(predicate, index, table, column.unwrap_or(0))
    }) {
        Some(formula) => text.push_str(&format!(" {formula}")),
        None if predicate.has_formula => text.push_str(" (against a formula)"),
        None => {}
    }
    if predicate.pre_pivot {
        text.push_str(" [pre-pivot form]");
    }
    text
}

/// Every formula in the document: where it is, what it says, what it cached.
fn formulas(path: &str, wanted: Option<&str>) -> Result<(), Error> {
    let doc = Document::open(path)?;
    let tables = doc.tables();
    let all = iwork::table::formulas(&tables);
    let listed: Vec<&iwork::table::FormulaCell> = all
        .iter()
        .filter(|f| match wanted {
            Some(name) => f.table_name == name || f.table.to_string() == name,
            None => true,
        })
        .collect();
    if listed.is_empty() {
        println!("no formulas");
        return Ok(());
    }
    let mut last = None;
    for formula in listed {
        if last != Some(formula.table) {
            match &formula.sheet {
                Some(sheet) => println!(
                    "== table {} {} (sheet {sheet}) ==",
                    formula.table, formula.table_name
                ),
                None => println!("== table {} {} ==", formula.table, formula.table_name),
            }
            last = Some(formula.table);
        }
        println!(
            "{:<6} {:<48} = {}",
            formula.reference,
            formula.text,
            formula.value.to_text()
        );
    }
    Ok(())
}

fn find_table(path: &str, wanted: &str) -> Result<iwork::Table, Error> {
    let doc = Document::open(path)?;
    doc.table(wanted)
        .ok_or_else(|| Error::Format(format!("no table called '{wanted}' in {path}")))
}

fn cells(path: &str, wanted: &str, raw: bool) -> Result<(), Error> {
    // The name index is document-wide: a formula in this table may name a
    // column of another one, and which table owns a shared name depends on all
    // of them.
    let doc = Document::open(path)?;
    let tables = doc.tables();
    let index = iwork::table::names(&tables);
    let position = tables
        .iter()
        .position(|t| t.name == wanted || t.identifier.to_string() == wanted)
        .ok_or_else(|| Error::Format(format!("no table called '{wanted}' in {path}")))?;
    let table = &tables[position];
    println!(
        "table {} — {} — {}×{}",
        table.identifier, table.name, table.rows, table.columns
    );
    for cell in table.cells() {
        if cell.value.is_empty() && !raw {
            continue;
        }
        let mut notes = Vec::new();
        notes.push(cell.format.to_string());
        if let Some(control) = cell.control {
            notes.push(format!("control: {}", control.as_str()));
        }
        if let Some(formula) = table.formula(cell.row, cell.column) {
            let at = iwork::formula::Site::new(
                &index,
                Some(position),
                (cell.column as i64, cell.row as i64),
            );
            notes.push(formula.text(at));
        } else if cell.has_formula {
            notes.push("formula".to_string());
        }
        if let Some(merge) = table.merge_at(cell.row, cell.column) {
            notes.push(format!("merged {}×{}", merge.rows, merge.columns));
        }
        let suffix = if notes.is_empty() {
            String::new()
        } else {
            format!("  [{}]", notes.join(", "))
        };
        println!(
            "{:<6} {:<10} {}{suffix}",
            reference_name(cell.row, cell.column),
            cell.value.kind(),
            cell.value.to_text()
        );
        if raw {
            let r = &cell.record;
            println!(
                "       type {:<3} reserved {:02x}{:02x}{:02x}{:02x} extras {:04x} flags {:08x} \
                 keys{}{}{}{}{}{}{}{}{}{}{}{}",
                r.cell_type,
                r.reserved[0],
                r.reserved[1],
                r.reserved[2],
                r.reserved[3],
                r.extras,
                r.flags,
                key(" string", r.string_id),
                key(" rich", r.rich_id),
                key(" cell-style", r.cell_style_id),
                key(" text-style", r.text_style_id),
                // The two an edit must carry rather than re-synthesise: a cell
                // that loses them loses its highlighting and nothing says so.
                key(" conditional-style", r.conditional_style_id),
                key(" conditional-rule", r.conditional_rule_id),
                key(" formula", r.formula_id),
                key(" control", r.control_id),
                key(" comment", r.comment_id),
                key(" format-kind", r.format_kind),
                key(" number-format", r.number_format_id),
                key(
                    " other-format",
                    r.format_id().filter(|_| r.number_format_id.is_none())
                ),
            );
        }
    }
    Ok(())
}

fn set_cell(
    path: &str,
    table: &str,
    row: usize,
    column: usize,
    value: &str,
    out: &str,
) -> Result<(), Error> {
    let value = parse_cell_value(value)?;
    let mut doc = Document::open(path)?;
    let previous = doc.set_cell(table, row, column, value.clone())?;
    println!(
        "{} of table {table}: {} -> {}",
        reference_name(row, column),
        describe_value(&previous),
        describe_value(&value)
    );
    save(&doc, out)
}

/// A cell value as the command line spells it — text unless it says otherwise.
fn parse_cell_value(text: &str) -> Result<iwork::table::CellValue, Error> {
    use iwork::table::CellValue;
    let bad = |what: &str, value: &str| Error::Format(format!("'{value}' is not {what}"));
    let Some((kind, rest)) = text.split_once(':') else {
        return Ok(CellValue::Text(text.to_string()));
    };
    Ok(match kind {
        "text" => CellValue::Text(rest.to_string()),
        "n" => CellValue::Number(
            iwork::table::Decimal::parse(rest).ok_or_else(|| bad("a number", rest))?,
        ),
        "b" => match rest {
            "true" | "yes" | "1" => CellValue::Bool(true),
            "false" | "no" | "0" => CellValue::Bool(false),
            other => return Err(bad("true or false", other)),
        },
        "d" => CellValue::Date(
            iwork::table::parse_date(rest)
                .ok_or_else(|| bad("a date, as 2024-03-01 or 2024-03-01T10:30:00Z", rest))?,
        ),
        "dur" => CellValue::Duration(rest.parse().map_err(|_| bad("a number of seconds", rest))?),
        "empty" => CellValue::Empty,
        // A colon is ordinary in a cell — "Total: 40" is text, not a prefix
        // this crate failed to understand.
        _ => CellValue::Text(text.to_string()),
    })
}

fn describe_value(value: &iwork::table::CellValue) -> String {
    match value {
        iwork::table::CellValue::Empty => "(empty)".to_string(),
        other => format!("{} {:?}", other.kind(), other.to_text()),
    }
}

fn csv(path: &str, wanted: &str) -> Result<(), Error> {
    let table = find_table(path, wanted)?;
    for row in table.to_rows() {
        let fields: Vec<String> = row.iter().map(|field| csv_field(field)).collect();
        println!("{}", fields.join(","));
    }
    Ok(())
}

fn csv_field(text: &str) -> String {
    if text.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", text.replace('"', "\"\""))
    } else {
        text.to_string()
    }
}

fn key(label: &str, value: Option<u32>) -> String {
    value.map(|v| format!("{label}={v}")).unwrap_or_default()
}

/// `A1`-style name for a cell, as the app writes it.
fn reference_name(row: usize, column: usize) -> String {
    let mut letters = String::new();
    let mut n = column + 1;
    while n > 0 {
        let digit = (n - 1) % 26;
        letters.insert(0, (b'A' + digit as u8) as char);
        n = (n - 1) / 26;
    }
    format!("{letters}{}", row + 1)
}

fn properties() -> Result<(), Error> {
    println!("  {:<24} {:<12} evidence", "name", "path");
    for (name, path, confidence) in style::property::BY_NAME {
        let dotted = path
            .iter()
            .map(u32::to_string)
            .collect::<Vec<_>>()
            .join(".");
        println!(
            "  {name:<24} {dotted:<12} {}",
            match confidence {
                registry::Confidence::Confirmed => "measured in an imported document",
                registry::Confidence::Inferred => "observed changing alongside a measured one",
                registry::Confidence::Unverified => "name only, not observed here",
            }
        );
    }
    Ok(())
}

fn check(path: &str) -> Result<(), Error> {
    let doc = Document::open(path)?;

    // Not a problem — a state. An object can carry older encodings of itself as
    // patches, and this crate reads the first message and rewrites none of
    // them; saying so is the difference between "nothing here" and "nothing
    // here that this tool would touch".
    let patched = doc.patched_objects();
    if !patched.is_empty() {
        let listed: Vec<String> = patched
            .iter()
            .map(|(id, patches)| format!("{id} ({patches})"))
            .collect();
        println!(
            "  note: {} object(s) carry version patches, read-only here: {}",
            patched.len(),
            listed.join(", ")
        );
    }

    let problems = doc.problems();
    if problems.is_empty() {
        println!(
            "{path}: no problems found in {} objects",
            doc.objects().count()
        );
        return Ok(());
    }
    for problem in &problems {
        println!("  {problem}");
    }
    Err(Error::Format(format!("{} problem(s)", problems.len())))
}

fn extract(path: &str, dir: &str) -> Result<(), Error> {
    let doc = Document::open(path)?;
    std::fs::create_dir_all(dir)?;
    let mut written = 0;
    for name in doc.package().data_names() {
        let bytes = doc
            .package()
            .get(&name)
            .expect("name came from the package");
        let file = name.trim_start_matches("Data/");
        let target = std::path::Path::new(dir).join(file);
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&target, bytes)?;
        println!("  {} ({} bytes)", target.display(), bytes.len());
        written += 1;
    }
    if written == 0 {
        println!("no embedded media");
    }
    Ok(())
}

// -- text styles -------------------------------------------------------------

fn styles(path: &str) -> Result<(), Error> {
    let doc = Document::open(path)?;
    let styles = doc.text_styles();
    if styles.is_empty() {
        println!("no text styles in {path}");
        return Ok(());
    }
    for style in &styles {
        println!(
            "  id={:<8} {:<10} {:<34} {:>4} run(s)  {}",
            style.identifier,
            style.kind.as_str(),
            style.label().unwrap_or("(unnamed variation)"),
            doc.text_style_usage(style.identifier).len(),
            style.stream,
        );
    }
    println!("\n{} text styles", styles.len());
    Ok(())
}

fn show_style(path: &str, id: u64) -> Result<(), Error> {
    let doc = Document::open(path)?;
    let style = doc.text_style(id).ok_or(Error::NoSuchStyle(id))?;
    let message_type = style.kind.message_type();
    println!(
        "{} style {id} — type {message_type} {}, in {}",
        style.kind.as_str(),
        registry::describe_in(doc.kind(), message_type),
        style.stream
    );
    match &style.name {
        Some(name) => println!("name: {name:?}"),
        None => println!("name: none — this is a variation style"),
    }
    if let Some(identifier) = &style.style_identifier {
        println!("style identifier: {identifier:?}");
    }
    if let Some(parent) = style.parent {
        let inherited = doc
            .text_style(parent)
            .and_then(|s| s.label().map(str::to_string));
        println!(
            "inherits from: {parent} {}",
            inherited.unwrap_or_else(|| "(unnamed)".into())
        );
    }

    println!("\n== fields ==");
    dump(&style.archive, "");

    let uses = doc.text_style_usage(id);
    println!("\n== used by {} run(s) ==", uses.len());
    for used in &uses {
        println!(
            "  storage {:<8} field {:<3} chars {}..{}  {}",
            used.storage, used.table, used.range.start, used.range.end, used.stream
        );
    }
    Ok(())
}

/// Print any object's fields, whatever it is.
///
/// The registry names a fraction of the message types and this crate models
/// fewer still, so the way to find out what an unknown archive holds is to look
/// at it. Reference-shaped fields are marked, which is usually enough to walk
/// the graph by hand.
fn dump_object(path: &str, id: u64) -> Result<(), Error> {
    let doc = Document::open(path)?;
    let (stream, object) = doc.object(id).ok_or(Error::NoSuchObject(id))?;
    println!(
        "object {id} — type {} {}, in {stream}",
        object.message_type(),
        registry::describe_in(doc.kind(), object.message_type())
    );
    for (index, message) in object.messages.iter().enumerate() {
        if object.messages.len() > 1 {
            println!("\n-- message {index}, type {} --", message.message_type);
        }
        println!(
            "version {:?}, {} bytes\n",
            message.version,
            message.payload.len()
        );
        match Message::decode(&message.payload) {
            Ok(archive) => dump(&archive, ""),
            Err(e) => println!("  could not decode: {e}"),
        }
    }
    Ok(())
}

/// Print an archive as addressable field paths, so `set-style` has something to
/// aim at. Nothing here claims to know what a field means.
fn dump(message: &Message, prefix: &str) {
    for field in &message.fields {
        let path = match prefix.is_empty() {
            true => field.number.to_string(),
            false => format!("{prefix}.{}", field.number),
        };
        match &field.value {
            Value::Varint(v) => println!("  {path:<14} varint  {v}"),
            Value::Fixed32(b) => println!("  {path:<14} f32     {}", f32::from_le_bytes(*b)),
            Value::Fixed64(b) => println!("  {path:<14} f64     {}", f64::from_le_bytes(*b)),
            Value::Bytes(raw) => match pb::decode_nested(raw) {
                Some(nested) => {
                    match iwork::style::reference_target(&nested) {
                        Some(target) => println!("  {path:<14} ref     -> object {target}"),
                        None => println!("  {path:<14} message {} bytes", raw.len()),
                    }
                    dump(&nested, &path);
                }
                None => match std::str::from_utf8(raw) {
                    Ok(text) if !text.is_empty() && text.chars().all(|c| !c.is_control()) => {
                        println!("  {path:<14} str     {text:?}")
                    }
                    _ => println!("  {path:<14} bytes   {} bytes", raw.len()),
                },
            },
        }
    }
}

fn new_style(path: &str, template: u64, name: &str, out: &str) -> Result<(), Error> {
    let mut doc = Document::open(path)?;
    let created = doc.create_text_style(template, name)?;
    println!(
        "created style {} from {} in {} ({} stylesheet entries cloned)",
        created.identifier, created.template, created.stream, created.registrations_cloned
    );
    let result = save(&doc, out);
    if created.name.is_none() {
        println!(
            "note: style {} is a variation, so the copy is anonymous and keeps no name.\n      \
             Naming it would make Pages refuse the document — see create_text_style.",
            created.template
        );
    }
    result
}

/// A style keeps its text colour in up to four places and they must agree, so
/// setting one of them by path is usually the wrong thing to do.
fn set_color(path: &str, id: &str, r: &str, g: &str, b: &str, out: &str) -> Result<(), Error> {
    let id = identifier(id)?;
    let channel = |text: &str| -> Result<f32, Error> {
        text.parse::<f32>()
            .map_err(|_| Error::Format(format!("'{text}' is not a channel value in 0.0..=1.0")))
    };
    let mut doc = Document::open(path)?;
    let set = doc.set_text_style_color(id, channel(r)?, channel(g)?, channel(b)?, 1.0)?;
    if set == 0 {
        return Err(Error::Format(format!(
            "style {id} keeps no colour of its own, and one invented here would \
             make Pages refuse the document — copy a style that has one"
        )));
    }
    println!("set {set} colour field(s) on style {id}");
    save(&doc, out)
}

fn set_style(path: &str, id: u64, assignment: &str, out: &str) -> Result<(), Error> {
    let (field, value) = assignment
        .split_once('=')
        .ok_or_else(|| Error::Format(format!("'{assignment}' is not <path>=<value>")))?;

    let mut doc = Document::open(path)?;
    if field == "name" {
        doc.rename_text_style(id, value)?;
        println!("renamed style {id} to {value:?}");
    } else {
        let field_path = parse_path(field)?;
        let value = parse_value(value)?;
        let cleared = value.is_none();
        doc.set_text_style_property(id, &field_path, value)?;
        match cleared {
            true => println!("cleared field {field} of style {id}"),
            false => println!("set field {field} of style {id}"),
        }
    }
    save(&doc, out)
}

fn delete_style(path: &str, id: u64, replacement: Option<u64>, out: &str) -> Result<(), Error> {
    let mut doc = Document::open(path)?;
    let deleted = doc.delete_text_style(id, replacement)?;
    println!(
        "deleted style {id}: {} run(s) repointed, {} dropped, {} stylesheet entries removed",
        deleted.runs_repointed, deleted.runs_dropped, deleted.registrations_removed
    );
    save(&doc, out)
}

fn apply_style(
    path: &str,
    storage: &str,
    start: &str,
    end: &str,
    style: &str,
    out: &str,
) -> Result<(), Error> {
    let storage = identifier(storage)?;
    let start = identifier(start)?;
    let end = identifier(end)?;
    let style = identifier(style)?;

    let mut doc = Document::open(path)?;
    doc.apply_text_style(storage, start..end, style)?;
    println!("storage {storage} chars {start}..{end} now use style {style}");
    save(&doc, out)
}

fn paragraphs(path: &str, storage: u64) -> Result<(), Error> {
    let doc = Document::open(path)?;
    let text: Vec<u16> = doc.storage_text(storage)?.encode_utf16().collect();
    let list = doc.list_paragraphs(storage)?;
    for (index, paragraph) in list.iter().enumerate() {
        let slice = &text[paragraph.range.start as usize..paragraph.range.end as usize];
        let style = doc
            .style_of_run(storage, paragraph.range.start, style::StyleKind::Paragraph)?
            .map(|resolved| {
                let name = resolved
                    .name
                    .clone()
                    .unwrap_or_else(|| format!("style {}", resolved.style));
                match (resolved.is_variation, resolved.overrides.len()) {
                    (true, 0) => format!("{name} (variation)"),
                    (true, n) => format!("{name} + {n} override(s)"),
                    (false, _) => name,
                }
            })
            .unwrap_or_else(|| "-".to_string());
        let bullet = match doc.text_style(paragraph.style.unwrap_or(0)) {
            Some(list_style) => list_style
                .name
                .unwrap_or_else(|| format!("list {}", list_style.identifier)),
            None => "-".to_string(),
        };
        println!(
            "  {index:<4} {:>7}..{:<7} L{}  {:<28} {:<14} {}",
            paragraph.range.start,
            paragraph.range.end,
            paragraph.level,
            style,
            bullet,
            snippet(slice)
        );
    }
    println!("\n{} paragraph(s), {} characters", list.len(), text.len());
    Ok(())
}

/// One line of a paragraph's text, short enough to scan in a listing.
fn snippet(units: &[u16]) -> String {
    let text = String::from_utf16_lossy(units);
    let flat: String = text
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect();
    let trimmed = flat.trim();
    match trimmed.char_indices().nth(64) {
        Some((cut, _)) => format!("{}…", &trimmed[..cut]),
        None => trimmed.to_string(),
    }
}

// -- argument parsing --------------------------------------------------------

fn identifier(text: &str) -> Result<u64, Error> {
    text.parse()
        .map_err(|_| Error::Format(format!("'{text}' is not an object identifier")))
}

fn index(text: &str) -> Result<usize, Error> {
    text.parse()
        .map_err(|_| Error::Format(format!("'{text}' is not a row or column index")))
}

/// `B3` to the `(row, column)` pair the API takes — the inverse of
/// [`reference_name`].
fn reference_position(text: &str) -> Result<(usize, usize), Error> {
    let bad = || Error::Format(format!("'{text}' is not a cell reference like B3"));
    let split = text
        .find(|c: char| c.is_ascii_digit())
        .filter(|&at| at > 0)
        .ok_or_else(bad)?;
    let (letters, digits) = text.split_at(split);
    let mut column = 0usize;
    for letter in letters.chars() {
        let value = letter.to_ascii_uppercase() as u32;
        if !(b'A' as u32..=b'Z' as u32).contains(&value) {
            return Err(bad());
        }
        column = column * 26 + (value - b'A' as u32 + 1) as usize;
    }
    let row: usize = digits.parse().map_err(|_| bad())?;
    if row == 0 {
        return Err(bad());
    }
    Ok((row - 1, column - 1))
}

/// A dotted path of field numbers, or one of the names in
/// [`iwork::style::property`].
fn parse_path(text: &str) -> Result<Vec<u32>, Error> {
    if text.is_empty() {
        return Err(Error::Format("empty field path".into()));
    }
    if let Some(known) = style::property::path(text) {
        return Ok(known.to_vec());
    }
    text.split('.')
        .map(|part| {
            part.parse().map_err(|_| {
                let names: Vec<&str> = style::property::BY_NAME
                    .iter()
                    .map(|(n, _, _)| *n)
                    .collect();
                Error::Format(format!(
                    "'{part}' is not a field number; known names are {}",
                    names.join(", ")
                ))
            })
        })
        .collect()
}

/// `varint:1`, `f32:18`, `f64:1.5`, `str:Body`, `hex:0a0b`, or empty to remove
/// the field.
fn parse_value(text: &str) -> Result<Option<Value>, Error> {
    if text.is_empty() {
        return Ok(None);
    }
    let (kind, rest) = text
        .split_once(':')
        .ok_or_else(|| Error::Format(format!("'{text}' is not <type>:<value>")))?;
    let bad = |what: &str| Error::Format(format!("'{rest}' is not {what}"));
    let value = match kind {
        "varint" => Value::Varint(rest.parse().map_err(|_| bad("an unsigned integer"))?),
        "f32" => Value::Fixed32(
            rest.parse::<f32>()
                .map_err(|_| bad("a number"))?
                .to_le_bytes(),
        ),
        "f64" => Value::Fixed64(
            rest.parse::<f64>()
                .map_err(|_| bad("a number"))?
                .to_le_bytes(),
        ),
        "str" => Value::Bytes(rest.as_bytes().to_vec()),
        "hex" => Value::Bytes(parse_hex(rest)?),
        other => {
            return Err(Error::Format(format!(
                "unknown value type '{other}' (varint, f32, f64, str, hex)"
            )))
        }
    };
    Ok(Some(value))
}

fn parse_hex(text: &str) -> Result<Vec<u8>, Error> {
    let digits: Vec<char> = text.chars().filter(|c| !c.is_whitespace()).collect();
    if digits.len() % 2 != 0 {
        return Err(Error::Format("hex needs an even number of digits".into()));
    }
    digits
        .chunks(2)
        .map(|pair| {
            u8::from_str_radix(&pair.iter().collect::<String>(), 16).map_err(|_| {
                Error::Format(format!("'{}' is not hex", pair.iter().collect::<String>()))
            })
        })
        .collect()
}

fn roundtrip(path: &str, out: &str) -> Result<(), Error> {
    let doc = Document::open(path)?;
    let count = doc.objects().count();
    save(&doc, out)?;

    // Re-open and compare object streams, so the check covers the file that was
    // actually written rather than the in-memory graph.
    let reopened = Document::open(out)?;
    let before: Vec<_> = doc.objects().map(|(_, o)| o.identifier).collect();
    let after: Vec<_> = reopened.objects().map(|(_, o)| o.identifier).collect();
    if before != after {
        return Err(Error::Format(
            "object identifiers changed on re-encode".into(),
        ));
    }
    println!("decoded and checked {count} objects");
    Ok(())
}
