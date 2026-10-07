//! Regenerate `src/chart_kit.iwa` — what a chart is made of, cut out of a
//! deck Keynote wrote.
//!
//! ```text
//! cargo run --example chart_kit -- tests/fixtures/generated/keynote-charts.key src/chart_kit.iwa
//! ```
//!
//! A chart is not something this crate can invent: its model names a preset,
//! a chart style, a legend style, three axis styles, six series styles and
//! twenty-five paragraph styles, and their properties are a theme's. So the
//! crate carries one — the column chart `scripts/applescript/keynote-charts`
//! had Keynote make, with everything it reaches — and `Document::new_chart`
//! copies from that. This cuts it out: the chart, what it owns, its preset and
//! the preset's styles, renumbered from 100, the stylesheet they belong to
//! replaced by the placeholder identifier 1.

use iwork::pb::{Field, Message, Value};
use std::collections::BTreeMap;

/// The chart archives (`TSCH` 5020–5031), paragraph styles, caption stand-ins.
fn belongs(message_type: u32) -> bool {
    (5020..=5031).contains(&message_type) || matches!(message_type, 2022 | 3097)
}

fn remap(message: &mut Message, map: &BTreeMap<u64, u64>, depth: usize) {
    if depth == 0 {
        return;
    }
    for Field { value, .. } in &mut message.fields {
        let Value::Bytes(raw) = value else { continue };
        let Some(mut nested) = iwork::pb::decode_nested(raw) else {
            continue;
        };
        match iwork::style::reference_target(&nested) {
            Some(target) => {
                if let Some(new) = map.get(&target) {
                    *raw = iwork::style::reference(*new).encode();
                }
            }
            None => {
                remap(&mut nested, map, depth - 1);
                *raw = nested.encode();
            }
        }
    }
}

fn main() {
    let mut args = std::env::args().skip(1);
    let (from, to) = (
        args.next().expect("a deck"),
        args.next().expect("an output"),
    );
    let doc = iwork::Document::open(&from).unwrap();
    let chart = doc
        .charts()
        .into_iter()
        .find(|chart| chart.chart_type == 1 && chart.mediator.is_none())
        .expect("a column chart");

    let mut kit = vec![chart.identifier];
    let mut map = BTreeMap::new();
    let mut at = 0;
    while at < kit.len() {
        let (_, object) = doc.object(kit[at]).unwrap();
        assert!(
            object.messages.len() == 1 && !object.messages[0].extra.iter().any(|f| f.number == 6),
            "object {} is a patch or names media, and the kit carries neither",
            kit[at]
        );
        let archive = Message::decode(object.payload()).unwrap();
        for target in iwork::style::references(&archive) {
            let Some((_, other)) = doc.object(target) else {
                continue;
            };
            if other.message_type() == 401 {
                map.insert(target, 1);
            } else if belongs(other.message_type()) && !kit.contains(&target) {
                kit.push(target);
            }
        }
        at += 1;
    }
    kit[1..].sort_unstable();
    for (index, identifier) in kit.iter().enumerate() {
        map.insert(*identifier, 100 + index as u64);
    }

    let objects: Vec<iwork::iwa::ArchiveObject> = kit
        .iter()
        .map(|identifier| {
            let mut object = doc.object(*identifier).unwrap().1.clone();
            let mut archive = Message::decode(object.payload()).unwrap();
            remap(&mut archive, &map, 32);
            object.identifier = map[identifier];
            object.extra.clear();
            object.messages[0].extra.clear();
            object.messages[0].payload = archive.encode();
            object
        })
        .collect();
    std::fs::write(&to, iwork::iwa::serialize(&objects)).unwrap();
    println!(
        "{to}: {} objects from chart {}",
        objects.len(),
        chart.identifier
    );
}
