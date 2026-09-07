//! Write a Numbers spreadsheet from a slice of Rust data, with no Apple
//! software anywhere in the picture.
//!
//! ```text
//! cargo run --example spreadsheet -- Sales.numbers
//! open Sales.numbers
//! ```

use iwork::table::{CellValue, Decimal};
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

    // One header row and one row per region, three columns.
    let mut doc = Document::new_spreadsheet("Sales", "Q1", SALES.len() + 1, 3)?;

    for (column, heading) in ["Region", "Units", "Revenue"].iter().enumerate() {
        doc.set_cell("Q1", 0, column, CellValue::Text((*heading).into()))?;
    }
    for (row, region) in SALES.iter().enumerate() {
        let number = |value: String| {
            Decimal::parse(&value)
                .map(CellValue::Number)
                .unwrap_or(CellValue::Empty)
        };
        doc.set_cell("Q1", row + 1, 0, CellValue::Text(region.name.into()))?;
        doc.set_cell("Q1", row + 1, 1, number(region.units.to_string()))?;
        doc.set_cell("Q1", row + 1, 2, number(format!("{:.2}", region.revenue)))?;
    }

    doc.save(&out)?;
    println!("wrote {out}");
    Ok(())
}
