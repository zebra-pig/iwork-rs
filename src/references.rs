//! Which of an object's references it *owns*.
//!
//! Every `TSP.MessageInfo` lists, in `object_references` (5), the objects its
//! payload refers to **strongly** — the ones it keeps alive. A parent pointer,
//! a style's stylesheet, a stylesheet's flat list of styles are *weak* and are
//! left out. Nothing in the payload says which is which; the app knows from
//! its classes.
//!
//! Through 15.3 the apps did not mind the list being absent. Keynote 15.4
//! does: it resolves a strong reference only if the list declares it, and a
//! deck this crate made from nothing — no list anywhere — lost its theme, its
//! stylesheet and every placeholder's text, and was refused with "Keynote
//! couldn't read the file". Numbers and Pages 15.4 log the same assertion
//! (`TSPUnarchiver validateReferenceToObjectIdentifier… is not strongly
//! referenced from message`) and carry on.
//!
//! So the list is derived, at save, for every object. The rule is measured,
//! not reasoned: over the fixture corpus and all 990 templates in the three
//! 15.4 app bundles — 769,095 objects — "every reference in the payload,
//! except those at the paths in [`WEAK`]" reproduces the app's own list
//! exactly for all but ten, and those ten (`TN` type 12026) declare *more*
//! than their payload shows, which [`wanted`] keeps.

use crate::iwa::ArchiveObject;
use crate::pb::{Field, Message, Reader, Value};
use crate::style::reference_target;
use std::collections::BTreeSet;

/// `MessageInfo.object_references`.
const OBJECT_REFERENCES: u32 = 5;
/// `MessageInfo.data_references`.
const DATA_REFERENCES: u32 = 6;

/// (message type, field path to the reference) for every reference the apps
/// were seen leaving out of `object_references`. Sorted, for the search.
///
/// A path not listed is taken as strong, which is what four in five are.
const WEAK: &[(u32, &[u32])] = &[
    (5, &[42]),
    (7, &[1, 1, 1, 2]),
    (8, &[1]),
    (9, &[1, 5]),
    (10, &[1, 4]),
    (25, &[1]),
    (25, &[2]),
    (26, &[1, 5]),
    (153, &[1]),
    (213, &[1]),
    (401, &[1]),
    (401, &[5, 1]),
    (401, &[5, 2]),
    (401, &[8, 1]),
    (401, &[8, 3, 2]),
    (401, &[14, 1]),
    (401, &[14, 3, 2]),
    (633, &[1, 1, 1, 2]),
    (2001, &[2]),
    (2001, &[5, 1]),
    (2001, &[8, 1]),
    (2001, &[11, 1]),
    (2001, &[19, 1]),
    (2001, &[20, 1]),
    (2011, &[1, 1, 2]),
    (2011, &[3]),
    (2021, &[1, 5]),
    (2021, &[11]),
    (2022, &[1, 5]),
    (2022, &[11]),
    (2022, &[12]),
    (2023, &[1, 5]),
    (2024, &[1, 5]),
    (2024, &[11, 7, 1]),
    (2025, &[1, 1, 5]),
    (2025, &[11]),
    (2025, &[11, 4, 1]),
    (2026, &[1, 1, 5]),
    (2060, &[2]),
    (2240, &[1, 1, 1, 2]),
    (2410, &[2]),
    (3005, &[1, 2]),
    (3006, &[1, 2]),
    (3007, &[1, 2]),
    (3008, &[1, 2]),
    (3016, &[1, 5]),
    (3045, &[1]),
    (3061, &[2]),
    (4000, &[7, 1, 3, 1, 2, 1, 2, 1]),
    (4008, &[6, 1, 1, 2, 1]),
    (4008, &[7, 1, 2, 3]),
    (4008, &[8, 1, 2, 3]),
    (4008, &[11]),
    (4008, &[14, 1, 2, 1, 2, 1]),
    (4008, &[14, 2, 2, 2, 1, 2, 1]),
    (4010, &[2, 2, 2]),
    (4011, &[2, 1, 2, 1, 2, 1]),
    (4011, &[3, 1, 2, 1, 2, 1]),
    (4011, &[7, 1, 2, 1, 2, 1]),
    (5021, &[1, 2]),
    (5021, &[10000, 4]),
    (5022, &[1, 5]),
    (5024, &[1, 5]),
    (5026, &[1, 5]),
    (5028, &[1, 5]),
    (5030, &[1, 5]),
    (6000, &[1, 2]),
    (6001, &[47, 2, 3, 2, 1, 1, 40, 3]),
    (6001, &[47, 2, 3, 2, 1, 1, 40, 4]),
    (6001, &[48]),
    (6001, &[81, 2, 4, 2, 2, 6]),
    (6001, &[81, 2, 4, 2, 2, 7]),
    (6001, &[81, 2, 4, 2, 3, 2, 6]),
    (6001, &[81, 2, 4, 2, 3, 2, 7]),
    (6003, &[1, 5]),
    (6004, &[1, 5]),
    (6005, &[3, 5, 1, 1]),
    (6005, &[3, 5, 1, 1, 40, 1]),
    (6005, &[3, 5, 1, 1, 40, 3]),
    (6005, &[3, 5, 1, 1, 40, 4]),
    (6005, &[3, 6]),
    (6010, &[2, 1, 1, 1, 1]),
    (6010, &[3, 1, 1, 7, 1, 1]),
    (6030, &[9]),
    (6206, &[2]),
    (6316, &[1]),
    (6317, &[1]),
    (6318, &[1]),
    (6373, &[3, 3, 3, 3, 6, 4, 1]),
    (6373, &[3, 3, 3, 3, 8, 1]),
    (6373, &[3, 3, 3, 3, 9, 1]),
    (6373, &[3, 3, 3, 6, 4, 1]),
    (6373, &[3, 3, 3, 8, 1]),
    (6373, &[3, 3, 3, 9, 1]),
    (6373, &[3, 3, 6, 4, 1]),
    (6373, &[3, 3, 8, 1]),
    (6373, &[3, 3, 9, 1]),
    (6373, &[4, 2, 2, 6]),
    (6373, &[4, 2, 2, 7]),
    (6373, &[4, 2, 3, 2, 6]),
    (6373, &[4, 2, 3, 2, 7]),
    (6382, &[2, 2, 6]),
    (6382, &[2, 2, 7]),
    (6382, &[2, 3, 2, 6]),
    (6382, &[2, 3, 2, 7]),
    (6383, &[6, 4, 1]),
    (6383, &[8, 1]),
    (6383, &[9, 1]),
    (10001, &[1, 4]),
    (10024, &[1, 5]),
    (10131, &[5, 1, 14]),
    (10132, &[7]),
    (11006, &[3, 6]),
    (11008, &[2]),
    (12009, &[1, 4]),
    (12028, &[1]),
    (12050, &[1, 5]),
];

/// A packed list of varints from every message of `object`.
fn packed(object: &ArchiveObject, number: u32) -> BTreeSet<u64> {
    let mut out = BTreeSet::new();
    for field in object
        .messages
        .iter()
        .flat_map(|message| message.extra.iter())
        .filter(|field| field.number == number)
    {
        let Value::Bytes(raw) = &field.value else {
            continue;
        };
        let mut reader = Reader::new(raw);
        while let Ok(value) = reader.varint() {
            out.insert(value);
            if reader.done() {
                break;
            }
        }
    }
    out
}

fn walk(
    message_type: u32,
    message: &Message,
    path: &mut Vec<u32>,
    strong: &mut BTreeSet<u64>,
    weak: &mut BTreeSet<u64>,
    depth: usize,
) {
    if depth == 0 {
        return;
    }
    for field in &message.fields {
        let Value::Bytes(raw) = &field.value else {
            continue;
        };
        let Some(nested) = crate::pb::decode_nested(raw) else {
            continue;
        };
        path.push(field.number);
        match reference_target(&nested) {
            Some(target) => {
                if WEAK.binary_search(&(message_type, path.as_slice())).is_ok() {
                    weak.insert(target);
                } else {
                    strong.insert(target);
                }
            }
            None => walk(message_type, &nested, path, strong, weak, depth - 1),
        }
        path.pop();
    }
}

/// What `object` should declare, or `None` when it already declares exactly
/// that — or is a kind of object this cannot speak for.
///
/// `exists` says whether an identifier is an object of the document: a `{1: n}`
/// that names nothing is a number, not a reference.
pub(crate) fn wanted(object: &ArchiveObject, exists: impl Fn(u64) -> bool) -> Option<Vec<u64>> {
    // An object with more than one message is a base and its version patches,
    // and each message's list is about that message alone. Only the apps write
    // those; they are left as written.
    let [message] = object.messages.as_slice() else {
        return None;
    };
    let archive = Message::decode(&message.payload).ok()?;
    let (mut strong, mut weak) = (BTreeSet::new(), BTreeSet::new());
    walk(
        message.message_type,
        &archive,
        &mut Vec::new(),
        &mut strong,
        &mut weak,
        24,
    );
    let declared = packed(object, OBJECT_REFERENCES);
    let data = packed(object, DATA_REFERENCES);
    // A `TSP.DataReference` is `{1: n}` too; the object's own data list says
    // which of these are media. See `Document::undeclared_references`.
    let mut want: BTreeSet<u64> = strong
        .iter()
        .copied()
        .filter(|target| exists(*target) && !data.contains(target))
        .collect();
    // What the app declared and the payload does not show as a reference at
    // all is the app knowing something this does not. Keep it.
    want.extend(
        declared
            .iter()
            .copied()
            .filter(|target| !strong.contains(target) && !weak.contains(target) && exists(*target)),
    );
    (want != declared).then(|| want.into_iter().collect())
}

/// Make `object` declare exactly `references`.
pub(crate) fn declare(object: &mut ArchiveObject, references: &[u64]) {
    let Some(message) = object.messages.first_mut() else {
        return;
    };
    message
        .extra
        .retain(|field| field.number != OBJECT_REFERENCES);
    if references.is_empty() {
        return;
    }
    let mut packed = Vec::new();
    for reference in references {
        crate::pb::write_varint(&mut packed, *reference);
    }
    // Before `data_references` (6) and the rest: fields go out in the order
    // they are held.
    let at = message
        .extra
        .iter()
        .position(|field| field.number > OBJECT_REFERENCES)
        .unwrap_or(message.extra.len());
    message.extra.insert(
        at,
        Field {
            number: OBJECT_REFERENCES,
            value: Value::Bytes(packed),
        },
    );
}

#[cfg(test)]
mod tests {
    use super::WEAK;

    #[test]
    fn the_weak_table_is_sorted_for_the_binary_search() {
        assert!(WEAK.windows(2).all(|pair| pair[0] < pair[1]));
    }
}
