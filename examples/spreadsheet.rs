//! Write a Numbers spreadsheet from a slice of Rust data, with no Apple
//! software anywhere in the picture.
//!
//! ```text
//! cargo run --example spreadsheet -- Sales.numbers
//! open Sales.numbers
//! ```

use iwork::table::Format;
use iwork::Document;

struct Region {
    name: &'static str,
    units: u32,
    revenue: f64,
}

const SALES: &[Region] = &[
    Region {
        name: "Zürich",
        units: 1_240,
        revenue: 184_300.0,
    },
    Region {
        name: "Genève",
        units: 980,
        revenue: 151_900.0,
    },
    Region {
        name: "Lugano",
        units: 415,
        revenue: 62_250.0,
    },
];

fn main() -> Result<(), iwork::Error> {
    let out = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "Sales.numbers".to_string());

    // A header row, one row per region, and a total row under them.
    let mut doc = Document::new_spreadsheet("Sales", "Q1", SALES.len() + 2, 3)?;
    let mut q1 = doc.table_mut("Q1")?;

    q1.set_block("A1", &[vec!["Region", "Units", "Revenue"]])?;
    for (index, region) in SALES.iter().enumerate() {
        // A1 row numbers, because that is what the app shows and what the
        // formula below has to agree with: the header is row 1.
        let row = index + 2;
        q1.set(format!("A{row}"), region.name)?;
        q1.set(format!("B{row}"), region.units)?;
        q1.set(format!("C{row}"), region.revenue)?;
        // Two decimals, not a currency: a *currency cell* is a value type this
        // crate does not write yet, and a currency format on a plain number
        // would sit in the file and never be drawn — which `set_format` says
        // rather than accepting.
        q1.format(format!("C{row}"), &Format::Number { decimals: Some(2) })?;
    }

    // The total is a real formula: Numbers recalculates it when a figure above
    // it changes. The value is what the cell shows until it does — this crate
    // writes formulas and does not evaluate them.
    let last = SALES.len() + 1;
    let total = last + 1;
    q1.set(format!("A{total}"), "Total")?;
    q1.formula(
        format!("B{total}"),
        &format!("=SUM(B2:B{last})"),
        SALES.iter().map(|region| region.units).sum::<u32>(),
    )?;
    q1.formula(
        format!("C{total}"),
        &format!("=SUM(C2:C{last})"),
        SALES.iter().map(|region| region.revenue).sum::<f64>(),
    )?;
    q1.format(format!("C{total}"), &Format::Number { decimals: Some(2) })?;
    q1.width(0, Some(140.0))?;

    doc.save(&out)?;
    println!("wrote {out}");
    Ok(())
}
