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
  iwork objects   <file> [type]            list objects, optionally of one message type
  iwork dump      <file> <id>              one object, field by field
  iwork check     <file>                   look for a broken object graph
  iwork extract   <file> <dir>             write embedded media to a directory
  iwork roundtrip <file> <out>             decode and re-encode every object

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
    doc.set_text(identifier, new_text)?;
    save(&doc, out)
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

/// The named style properties, with how far each name can be trusted.
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
    let ranges = doc.paragraph_ranges(storage)?;
    for (index, range) in ranges.iter().enumerate() {
        let slice = &text[range.start as usize..range.end as usize];
        println!(
            "  {index:<4} {:>7}..{:<7} {}",
            range.start,
            range.end,
            snippet(slice)
        );
    }
    println!("\n{} paragraph(s), {} characters", ranges.len(), text.len());
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
