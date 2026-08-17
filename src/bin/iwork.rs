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

tables

  iwork tables    <file>                   every table: size, headers, geometry
  iwork cells     <file> <table> [--raw]   every cell of one table, with type and format
  iwork csv       <file> <table>           one table as CSV
  iwork organise  <file> [<table>]         sort rules, filters, categories,
                                           pivots, conditional highlighting,
                                           custom cell formats

A <table> is an object id, as printed by `iwork tables`, or a table name.

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
        ["tables", file] => tables(file),
        ["cells", file, table] => cells(file, table, false),
        ["cells", file, table, "--raw"] => cells(file, table, true),
        ["csv", file, table] => csv(file, table),
        ["organise", file] => organise(file, None),
        ["organise", file, table] => organise(file, Some(table)),
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

    let tables: Vec<iwork::Table> = match wanted {
        Some(name) => vec![doc
            .table(name)
            .ok_or_else(|| Error::Format(format!("no table called '{name}' in {path}")))?],
        None => doc.tables(),
    };
    if tables.is_empty() {
        println!("no tables");
        return Ok(());
    }

    for table in &tables {
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
                    describe(&rule.predicate)
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
                    describe(&rule.predicate),
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

/// A predicate in one line: its code, and whatever it compares against.
fn describe(predicate: &iwork::table::Predicate) -> String {
    let mut text = format!("predicate {}", predicate.kind);
    if predicate.qualifiers != (0, 0) {
        text.push_str(&format!(" {:?}", predicate.qualifiers));
    }
    for value in &predicate.values {
        text.push_str(&format!(" {:?}", value.to_text()));
    }
    if predicate.has_formula {
        text.push_str(" (against a formula)");
    }
    if predicate.pre_pivot {
        text.push_str(" [pre-pivot form]");
    }
    text
}

fn find_table(path: &str, wanted: &str) -> Result<iwork::Table, Error> {
    let doc = Document::open(path)?;
    doc.table(wanted)
        .ok_or_else(|| Error::Format(format!("no table called '{wanted}' in {path}")))
}

fn cells(path: &str, wanted: &str, raw: bool) -> Result<(), Error> {
    let table = find_table(path, wanted)?;
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
        if cell.has_formula {
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
                 keys{}{}{}{}{}{}{}{}{}",
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
                key(" formula", r.formula_id),
                key(" control", r.control_id),
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
