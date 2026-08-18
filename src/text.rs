//! `TSWP.StorageArchive` — the text model shared by all three apps.
//!
//! Text and formatting are stored separately:
//!
//! | Field | Contents |
//! |-------|----------|
//! | 1     | what the storage is for — body, header, footnote, text box, cell … |
//! | 2     | reference to the owning stylesheet |
//! | 3     | the text, UTF-8, repeated — long text is split across several runs |
//! | 5     | **paragraph**-style table |
//! | 6     | packed per-paragraph data, a different shape |
//! | 7     | list-style table |
//! | 8     | **character**-style table |
//! | 9, 11, 12, 15, 16, 17 | further tables: attachments, smart fields, layout, bookmarks, footnotes, sections |
//!
//! Every attribute table has the same shape: repeated entries of
//! `{1: character_index, 2: reference to a style object}`, strictly increasing
//! by index. A run starts at its index and continues until the next entry.
//! Paragraphs are `\n` inside one storage; there is no per-paragraph object.
//!
//! That layout is why replacing text is not a string swap: every index past the
//! edit has to be brought back inside the new length, or the document addresses
//! characters that no longer exist.

use crate::pb::{Field, Message, Value};

/// Field numbers holding attribute tables — lists of
/// `{character_index, reference}` runs.
///
/// 5, 7 and 8 are the paragraph, list and character style tables (see
/// [`crate::style`]); the rest are the same shape pointing at things this crate
/// does not model, and are listed so that deleting a style cannot leave a
/// dangling reference behind in one of them.
///
/// Field 6 is a differently-shaped table of packed paragraph data, and field 10
/// is not a table at all — it is a lone boolean, and was in this list until the
/// storage archive's own field numbering was checked. Nothing broke, because
/// every operation here skips a field that is not length-delimited, but a run
/// table that was never there is not something to go looking for.
pub const ATTRIBUTE_TABLES: &[u32] = &[5, 7, 8, 9, 11, 12, 15, 16, 17];

/// Characters that end a paragraph.
///
/// `\n` is the obvious one. The others are not obvious and matter, because
/// the paragraph-style table puts a run immediately *after* each of them:
/// reading one as ordinary text splits the paragraphs one character wrong,
/// which is enough to make a paragraph style land in the wrong place.
///
/// **`\r` (`U+000D`) is the one this crate had wrong**, and it is the one the
/// apps write most often, because AppleScript's `return` is a carriage return
/// and every fixture built by script therefore has them. In `pages-styled`,
/// whose body reads `Überschrift\rEin roter Absatz…`, the paragraph table holds
/// `[0, 12, 74, 128]` — the characters after each `\r` — while
/// [`paragraph_ranges`] saw one paragraph of 171 characters. Four storages in
/// the corpus were affected, in Pages and in Keynote. It went unnoticed because
/// the test that would have caught it skips storages it believes have fewer
/// than two paragraphs, which is exactly what this bug made them look like.
///
/// `U+0005` appears where a Pages document changes layout mid-storage.
/// Verified in a Pages article at `…\n\n\u{5}Features\n`, where the run sits on
/// the `F`.
///
/// `U+0004` marks a section boundary. Verified in a document Pages built from
/// its "Project Proposal" template, whose body storage reads
/// `…123-4567\n\u{4}Company Name\n` at both of its section breaks and whose
/// paragraph table has a run on the `C`. It also turns up alone as the whole of
/// a body storage in a document whose text lives in shapes, which is the same
/// character doing the same job with nothing either side of it.
pub const PARAGRAPH_BREAKS: &[u16] = &[0x000A, 0x000D, 0x0005, 0x0004];

/// Length of a storage's text in UTF-16 code units — the unit run indices are
/// counted in.
pub fn length(text: &str) -> u64 {
    text.encode_utf16().count() as u64
}

/// Character ranges of the paragraphs in `text`, in UTF-16 code units.
///
/// A paragraph ends at `\n`, or at one of the break characters in
/// [`PARAGRAPH_BREAKS`]. The terminator belongs to the paragraph it ends, so the
/// ranges tile the text with no gaps. A paragraph style applies to whole
/// paragraphs, and this is how to name them. Empty text has no paragraphs.
pub fn paragraph_ranges(text: &str) -> Vec<std::ops::Range<u64>> {
    let mut out = Vec::new();
    let mut start = 0u64;
    let mut index = 0u64;
    for unit in text.encode_utf16() {
        index += 1;
        if PARAGRAPH_BREAKS.contains(&unit) {
            out.push(start..index);
            start = index;
        }
    }
    if start < index {
        out.push(start..index);
    }
    out
}

/// Concatenate the text runs of a storage archive.
pub fn read(storage: &Message) -> String {
    storage
        .all(3)
        .filter_map(|value| match value {
            Value::Bytes(bytes) => Some(String::from_utf8_lossy(bytes).into_owned()),
            _ => None,
        })
        .collect()
}

/// Does this storage hold anything a reader would call text?
///
/// Storages routinely exist only to anchor something else, and their contents
/// are placeholders rather than words:
///
/// - `U+FFFC OBJECT REPLACEMENT CHARACTER` stands in for an embedded drawable,
///   and is the entire contents of the storages attached to Numbers tables;
/// - `U+0004` appears alone in the Pages body storage of a document whose text
///   all lives in shapes.
pub fn has_content(text: &str) -> bool {
    text.chars()
        .any(|c| !c.is_whitespace() && !c.is_control() && c != '\u{FFFC}')
}

/// Replace the text of a storage archive and bring its attribute runs back
/// inside the new length.
///
/// Runs that start past the end of the new text collapse onto the last valid
/// index and are then deduplicated, so the tables stay strictly increasing.
/// Styling therefore survives at the start of the text and is truncated at the
/// end; this does not attempt to remap formatting onto the new wording, which
/// would need the paragraph and list structure to be interpreted rather than
/// preserved.
pub fn write(storage: &mut Message, new_text: &str) {
    // Collapse the possibly-multiple runs into a single one.
    let mut seen_text_field = false;
    storage.fields.retain(|field| {
        if field.number != 3 {
            return true;
        }
        let keep = !seen_text_field;
        seen_text_field = true;
        keep
    });
    // In field order: a storage that had no text at all would otherwise get its
    // text appended after every other field, which is not how iWork writes one.
    storage.set_in_order(3, Value::Bytes(new_text.as_bytes().to_vec()));

    // Run indices are character offsets, not byte offsets. Verified against a
    // German Pages document: in a storage reading
    // "Von Benjamin Keller\nVeröffentlicht am 07.09.2017\nim Magazin …" the
    // character-attribute table holds [0, 20, 49], which is exactly the
    // paragraph starts counted in characters; counted in UTF-8 bytes they
    // would be [0, 20, 50]. Three further storages agree.
    //
    // That sample is entirely BMP, so it cannot separate UTF-16 code units
    // from Unicode scalars. UTF-16 is used here because the text model is
    // NSString-backed, which makes astral characters count as two — if that
    // turns out to be wrong it only matters for text containing emoji or
    // similar, and only past the first such character.
    let limit = length(new_text);
    for field in &mut storage.fields {
        if !ATTRIBUTE_TABLES.contains(&field.number) {
            continue;
        }
        let Value::Bytes(raw) = &field.value else {
            continue;
        };
        let Ok(table) = Message::decode(raw) else {
            continue;
        };
        field.value = Value::Bytes(clamp_run_table(&table, limit).encode());
    }
}

/// Pull every run start down to `limit` and drop runs that collapse onto an
/// earlier one, keeping the table strictly increasing.
fn clamp_run_table(table: &Message, limit: u64) -> Message {
    let mut out = Message::default();
    let mut previous: Option<u64> = None;
    for field in &table.fields {
        let Value::Bytes(raw) = &field.value else {
            out.fields.push(field.clone());
            continue;
        };
        let Ok(mut entry) = Message::decode(raw) else {
            out.fields.push(field.clone());
            continue;
        };
        let Some(index) = entry.varint(1) else {
            out.fields.push(field.clone());
            continue;
        };
        let clamped = index.min(limit);
        if previous == Some(clamped) {
            continue;
        }
        previous = Some(clamped);
        entry.set(1, Value::Varint(clamped));
        out.fields.push(Field {
            number: field.number,
            value: Value::Bytes(entry.encode()),
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a storage archive with one text run and one attribute table.
    fn storage(text: &str, run_starts: &[u64]) -> Message {
        let mut table = Message::default();
        for start in run_starts {
            let mut entry = Message::default();
            entry.set(1, Value::Varint(*start));
            entry.set(2, Value::Varint(4242));
            table.fields.push(Field {
                number: 1,
                value: Value::Bytes(entry.encode()),
            });
        }
        let mut storage = Message::default();
        storage.set(3, Value::Bytes(text.as_bytes().to_vec()));
        storage.set(5, Value::Bytes(table.encode()));
        storage
    }

    fn run_starts(storage: &Message) -> Vec<u64> {
        let table = Message::decode(storage.bytes(5).unwrap()).unwrap();
        table
            .fields
            .iter()
            .map(|f| match &f.value {
                Value::Bytes(raw) => Message::decode(raw).unwrap().varint(1).unwrap(),
                _ => unreachable!(),
            })
            .collect()
    }

    #[test]
    fn placeholder_only_storages_are_not_content() {
        assert!(!has_content("\u{FFFC}"));
        assert!(!has_content("\u{4}"));
        assert!(!has_content("  \n\t "));
        assert!(has_content("Bossard"));
        assert!(has_content("\u{FFFC} Abbildung 1"));
    }

    #[test]
    fn reads_concatenated_runs() {
        let mut message = Message::default();
        message.fields.push(Field {
            number: 3,
            value: Value::Bytes(b"Hello, ".to_vec()),
        });
        message.fields.push(Field {
            number: 3,
            value: Value::Bytes(b"world".to_vec()),
        });
        assert_eq!(read(&message), "Hello, world");
    }

    #[test]
    fn shortening_clamps_and_deduplicates_runs() {
        let mut s = storage("0123456789", &[0, 3, 7, 9]);
        write(&mut s, "0123");
        assert_eq!(read(&s), "0123");
        // 7 and 9 both clamp to 4, and the duplicate is dropped.
        assert_eq!(run_starts(&s), vec![0, 3, 4]);
    }

    #[test]
    fn lengthening_leaves_runs_alone() {
        let mut s = storage("0123", &[0, 3]);
        write(&mut s, "0123456789");
        assert_eq!(run_starts(&s), vec![0, 3]);
    }

    /// Text goes where iWork puts it even when the storage had none — an empty
    /// placeholder in a Keynote slide is exactly that case.
    #[test]
    fn text_added_to_an_empty_storage_lands_in_field_order() {
        let mut s = Message::default();
        s.set(2, Value::Varint(1));
        s.set(5, Value::Bytes(Vec::new()));
        s.set(24, Value::Varint(1));
        write(&mut s, "Neu");
        let numbers: Vec<u32> = s.fields.iter().map(|f| f.number).collect();
        assert_eq!(numbers, vec![2, 3, 5, 24]);
        assert_eq!(read(&s), "Neu");
    }

    #[test]
    fn multiple_text_runs_collapse_to_one() {
        let mut s = storage("abc", &[0]);
        s.fields.push(Field {
            number: 3,
            value: Value::Bytes(b"def".to_vec()),
        });
        write(&mut s, "xyz");
        assert_eq!(s.all(3).count(), 1);
        assert_eq!(read(&s), "xyz");
    }

    #[test]
    fn a_layout_break_ends_a_paragraph_too() {
        // "…ende\n\n\u{5}Features" — the next paragraph starts on the F.
        assert_eq!(
            paragraph_ranges("ab\n\n\u{5}Fe"),
            vec![0..3, 3..4, 4..5, 5..7]
        );
    }

    /// The break the apps actually write: AppleScript's `return` is a carriage
    /// return, and Pages and Keynote store it as the paragraph separator.
    #[test]
    fn a_carriage_return_ends_a_paragraph() {
        assert_eq!(paragraph_ranges("ab\rcd\r"), vec![0..3, 3..6]);
        assert_eq!(paragraph_ranges("Überschrift\rEin"), vec![0..12, 12..15]);
    }

    #[test]
    fn a_section_break_ends_a_paragraph_too() {
        // "…123-4567\n\u{4}Company Name\n" — the next paragraph starts on the C.
        assert_eq!(paragraph_ranges("67\n\u{4}Co\n"), vec![0..3, 3..4, 4..7]);
    }

    #[test]
    fn paragraphs_tile_the_text_and_keep_their_newline() {
        assert_eq!(paragraph_ranges("one\ntwo\nthree"), vec![0..4, 4..8, 8..13]);
        assert_eq!(paragraph_ranges("one\ntwo\n"), vec![0..4, 4..8]);
        assert_eq!(paragraph_ranges("no newline"), vec![0..10]);
        assert_eq!(paragraph_ranges("\n"), vec![0..1]);
        assert!(paragraph_ranges("").is_empty());
    }

    /// Paragraph ranges are counted the same way run indices are.
    #[test]
    fn paragraphs_count_utf16_code_units() {
        assert_eq!(paragraph_ranges("\u{1F600}\nx"), vec![0..3, 3..4]);
        assert_eq!(length("\u{1F600}\nx"), 4);
    }

    /// Indices are UTF-16 code units, so astral characters count as two.
    #[test]
    fn limit_counts_utf16_code_units() {
        let mut s = storage("aaaaaa", &[0, 5]);
        write(&mut s, "\u{1F600}\u{1F600}"); // 2 chars, 4 UTF-16 units
        assert_eq!(run_starts(&s), vec![0, 4]);
    }
}
