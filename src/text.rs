//! `TSWP.StorageArchive` — the text model shared by all three apps.
//!
//! Text and formatting are stored separately:
//!
//! | Field | Contents |
//! |-------|----------|
//! | 2     | reference to the owning stylesheet |
//! | 3     | the text, UTF-8, repeated — long text is split across several runs |
//! | 5     | character-attribute table |
//! | 6     | packed paragraph/bidi flags |
//! | 7     | list-style table |
//! | 8     | paragraph-style table |
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

/// Field numbers holding attribute tables. Field 6 is packed flags rather than
/// a table and is deliberately excluded.
const ATTRIBUTE_TABLES: &[u32] = &[5, 7, 8, 9, 10, 11];

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
    storage.set(3, Value::Bytes(new_text.as_bytes().to_vec()));

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
    let limit = new_text.encode_utf16().count() as u64;
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

    /// Indices are UTF-16 code units, so astral characters count as two.
    #[test]
    fn limit_counts_utf16_code_units() {
        let mut s = storage("aaaaaa", &[0, 5]);
        write(&mut s, "\u{1F600}\u{1F600}"); // 2 chars, 4 UTF-16 units
        assert_eq!(run_starts(&s), vec![0, 4]);
    }
}
