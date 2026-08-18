//! `TST` — the table model, shared by Numbers, Pages and Keynote.
//!
//! A table is reached through five hops, none of which is optional:
//!
//! ```text
//! TST.TableInfoArchive  (6000)   the drawable on the sheet/page/slide
//!   └ 2 → TST.TableModelArchive  (6001)   size, names, header counts, styles
//!          └ 4  DataStore (inline)
//!               ├ 1  row headers    → HeaderStorage → HeaderStorageBucket (6006)
//!               ├ 2  column headers → one HeaderStorageBucket
//!               ├ 3  TileStorage    → TST.Tile (6002) per 256 rows
//!               └ 4…22 TableDataList (6005) side tables: strings, formats,
//!                      styles, formulas, rich text, control specs
//! ```
//!
//! The cells themselves are **not** protobuf. Each `Tile` holds one
//! `TileRowInfo` per non-empty row, and each of those holds a `bytes` field of
//! concatenated fixed-layout cell records plus an array of `int16` offsets into
//! it. [`decode_cell`] is the decoder for one record; [`row_cells`] slices a row.
//!
//! Everything a cell *is* — its text, its number format, its style, its formula
//! — is an integer key into one of the `TableDataList`s. The cell record holds
//! keys and one immediate value; nothing else.

use std::collections::BTreeMap;

use crate::pb::{decode_nested, Message, Value};

/// `TST.TableInfoArchive` — the drawable wrapper for a table.
pub const TYPE_TABLE_INFO: u32 = 6000;
/// `TST.TableModelArchive` — the table itself.
pub const TYPE_TABLE_MODEL: u32 = 6001;
/// `TST.Tile` — a block of at most `tile_size` rows of cell storage.
pub const TYPE_TILE: u32 = 6002;
/// `TST.TableDataList` — an interned side table. Also registered as 6201.
pub const TYPE_DATA_LIST: u32 = 6005;
/// `TST.HeaderStorageBucket` — row heights or column widths.
pub const TYPE_HEADER_BUCKET: u32 = 6006;
/// `TST.RichTextPayloadArchive` — a rich-text cell's text storage.
pub const TYPE_RICH_TEXT_PAYLOAD: u32 = 6218;

/// Seconds between the Unix epoch and Apple's, which dates are counted from.
pub const APPLE_EPOCH: f64 = 978_307_200.0;

/// Exponent bias of the cell record's decimal128 field.
const DECIMAL128_BIAS: i32 = 0x1820;

// -- decimal128 --------------------------------------------------------------

/// A number as the format stores it: a base-10 mantissa and an exponent.
///
/// Numbers keeps cell numbers in a 16-byte decimal, not a `double`, so that a
/// price typed as `1.10` is still `1.10` after a round trip. Converting to
/// `f64` is a lossy last step, not the representation — [`Decimal::to_f64`]
/// goes through the decimal text so the result is the correctly rounded one
/// rather than whatever `mantissa * 10f64.powi(exponent)` happens to give.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Decimal {
    pub mantissa: i128,
    pub exponent: i32,
}

impl Decimal {
    pub fn to_f64(self) -> f64 {
        format!("{}e{}", self.mantissa, self.exponent)
            .parse()
            .unwrap_or(f64::NAN)
    }

    /// Read a decimal from the digits the user typed, keeping them.
    ///
    /// `1.10` is kept as `110 × 10⁻²` rather than as the nearest `f64`, which is
    /// the reason cell numbers are not `double`s at all. Numbers itself normalises
    /// to fifteen significant digits when *it* writes a value — `3.14159`
    /// becomes `314159000000000 × 10⁻¹⁴` — but it reads the short form back
    /// as the same number, so there is nothing to gain by padding.
    pub fn parse(text: &str) -> Option<Decimal> {
        let text = text.trim();
        let (digits, exponent) = match text.split_once(['e', 'E']) {
            Some((mantissa, exponent)) => (mantissa, exponent.parse::<i32>().ok()?),
            None => (text, 0),
        };
        let (whole, fraction) = match digits.split_once('.') {
            Some((whole, fraction)) => (whole, fraction),
            None => (digits, ""),
        };
        if fraction.chars().any(|c| !c.is_ascii_digit()) {
            return None;
        }
        let joined = format!("{whole}{fraction}");
        // `-.5` and `+7` both have to parse, and an empty mantissa must not.
        if joined.is_empty() || joined == "-" || joined == "+" {
            return None;
        }
        Some(Decimal {
            mantissa: joined.parse().ok()?,
            exponent: exponent - fraction.len() as i32,
        })
    }
}

impl std::fmt::Display for Decimal {
    /// The exact value, with no exponent and no trailing zeroes — what the user
    /// typed, as far as the stored digits can say.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let sign = if self.mantissa < 0 { "-" } else { "" };
        let digits = self.mantissa.unsigned_abs().to_string();
        let text = match self.exponent {
            0 => digits,
            e if e > 0 => {
                // A large exponent would spell out a number nobody wants to
                // read; hand those to the float formatter instead.
                if e > 32 {
                    return write!(f, "{}", self.to_f64());
                }
                format!("{digits}{}", "0".repeat(e as usize))
            }
            e => {
                let shift = (-e) as usize;
                if shift > 40 {
                    return write!(f, "{}", self.to_f64());
                }
                let padded = format!("{:0>width$}", digits, width = shift + 1);
                let split = padded.len() - shift;
                let whole = &padded[..split];
                let frac = padded[split..].trim_end_matches('0');
                if frac.is_empty() {
                    whole.to_string()
                } else {
                    format!("{whole}.{frac}")
                }
            }
        };
        write!(f, "{sign}{text}")
    }
}

/// Decode the 16-byte decimal a number cell carries.
///
/// The layout is Apple's, not IEEE 754's: sign in the top bit of byte 15, a
/// 14-bit exponent split across bytes 15 and 14, and a 113-bit mantissa in
/// bytes 0..13 plus the low bit of byte 14. Reaching for a stock decimal128
/// reader gets a different number.
pub fn decode_decimal128(bytes: &[u8; 16]) -> Decimal {
    let exponent =
        ((((bytes[15] & 0x7f) as i32) << 7) | ((bytes[14] >> 1) as i32)) - DECIMAL128_BIAS;
    let mut mantissa = (bytes[14] & 1) as i128;
    for &byte in bytes[..14].iter().rev() {
        mantissa = (mantissa << 8) | byte as i128;
    }
    if bytes[15] & 0x80 != 0 {
        mantissa = -mantissa;
    }
    Decimal { mantissa, exponent }
}

/// Write the 16-byte decimal, the exact inverse of [`decode_decimal128`].
///
/// Fails rather than truncates: the mantissa is 113 bits and the biased
/// exponent 14, and a number outside either is a number this format cannot
/// hold — saying so is better than writing a different one.
pub fn encode_decimal128(value: Decimal) -> Result<[u8; 16], String> {
    let magnitude = value.mantissa.unsigned_abs();
    if magnitude >> 113 != 0 {
        return Err(format!(
            "{} needs more than the 113 mantissa bits a cell number has",
            value.mantissa
        ));
    }
    let biased = value.exponent + DECIMAL128_BIAS;
    if !(0..=0x3fff).contains(&biased) {
        return Err(format!(
            "exponent {} is outside the range a cell number can hold",
            value.exponent
        ));
    }
    let mut bytes = [0u8; 16];
    for (at, byte) in bytes[..14].iter_mut().enumerate() {
        *byte = (magnitude >> (8 * at)) as u8;
    }
    let biased = biased as u32;
    bytes[14] = (((biased & 0x7f) << 1) as u8) | ((magnitude >> 112) & 1) as u8;
    bytes[15] = ((biased >> 7) & 0x7f) as u8;
    if value.mantissa < 0 {
        bytes[15] |= 0x80;
    }
    Ok(bytes)
}

// -- the cell record ---------------------------------------------------------

/// Byte 1 of a cell record: `TST.CellType`.
///
/// Not `TST.CellValueType`, which the protobuf form of a cell uses and which
/// numbers almost every case differently. 10 is currency and is not a member of
/// either enum as published.
pub mod cell_type {
    pub const EMPTY: u8 = 0;
    /// A cell covered by a merge that begins elsewhere.
    pub const SPAN: u8 = 1;
    pub const NUMBER: u8 = 2;
    pub const TEXT: u8 = 3;
    pub const FORMULA: u8 = 4;
    pub const DATE: u8 = 5;
    pub const BOOL: u8 = 6;
    pub const DURATION: u8 = 7;
    pub const ERROR: u8 = 8;
    pub const RICH_TEXT: u8 = 9;
    pub const CURRENCY: u8 = 10;
}

/// One bit of the cell record's 32-bit flag word, with the width of the payload
/// it introduces.
///
/// The payloads follow the 12-byte header **in ascending bit order**, so a bit
/// whose meaning is of no interest still has to advance the cursor by exactly
/// its width or everything after it decodes as garbage.
const FLAGS: &[(u32, usize, &str)] = &[
    (0x0000_0001, 16, "decimal"),
    (0x0000_0002, 8, "double"),
    (0x0000_0004, 8, "seconds"),
    (0x0000_0008, 4, "string"),
    (0x0000_0010, 4, "rich text"),
    (0x0000_0020, 4, "cell style"),
    (0x0000_0040, 4, "text style"),
    (0x0000_0080, 4, "conditional style"),
    (0x0000_0100, 4, "conditional rule"),
    (0x0000_0200, 4, "formula"),
    (0x0000_0400, 4, "control"),
    (0x0000_0800, 4, "formula error"),
    (0x0000_1000, 4, "format kind"),
    (0x0000_2000, 4, "number format"),
    (0x0000_4000, 4, "currency format"),
    (0x0000_8000, 4, "date format"),
    (0x0001_0000, 4, "duration format"),
    (0x0002_0000, 4, "text format"),
    (0x0004_0000, 4, "boolean format"),
    (0x0008_0000, 4, "comment"),
    (0x0010_0000, 4, "import warning"),
];

/// A decoded cell record, as it stands before any side table is consulted.
///
/// Every `*_id` is a key into one of the `DataStore`'s `TableDataList`s; the
/// record itself holds no strings, no styles and no formulas.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CellRecord {
    pub version: u8,
    pub cell_type: u8,
    /// Bytes 2–5. Zero in everything written by the apps so far.
    pub reserved: [u8; 4],
    /// Bytes 6–7. The low byte says which format the user set on purpose —
    /// see [`EXPLICIT_FORMAT`] and [`CellRecord::explicit_format`]. The high
    /// byte is undecoded; `0x08` appears on currency cells.
    pub extras: u16,
    pub flags: u32,
    pub decimal: Option<Decimal>,
    /// The `0x2` slot: booleans (`> 0.0`) and durations (seconds) share it.
    pub double: Option<f64>,
    /// The `0x4` slot: seconds since 2001-01-01, timezone-naive.
    pub seconds: Option<f64>,
    pub string_id: Option<u32>,
    pub rich_id: Option<u32>,
    pub cell_style_id: Option<u32>,
    pub text_style_id: Option<u32>,
    pub conditional_style_id: Option<u32>,
    pub conditional_rule_id: Option<u32>,
    pub formula_id: Option<u32>,
    pub control_id: Option<u32>,
    pub formula_error_id: Option<u32>,
    /// Which of the six format slots is this cell's own — 1 number, 2
    /// currency, 3 date, 4 duration, 5 text, 6 boolean, in flag-word order.
    pub format_kind: Option<u32>,
    pub number_format_id: Option<u32>,
    pub currency_format_id: Option<u32>,
    pub date_format_id: Option<u32>,
    pub duration_format_id: Option<u32>,
    pub text_format_id: Option<u32>,
    pub boolean_format_id: Option<u32>,
    pub comment_id: Option<u32>,
    pub import_warning_id: Option<u32>,
    /// Bytes left over after the last payload named by the flag word. Empty in
    /// everything the apps have been seen to write, and carried rather than
    /// dropped so that a record this crate does not fully understand still
    /// re-encodes to the bytes it came from.
    pub tail: Vec<u8>,
}

/// Which of the six format slots a cell carries a key for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FormatSlot {
    Number,
    Currency,
    Duration,
    Date,
    Boolean,
    Text,
}

/// Byte 6, bit by bit: **which format the user set on purpose.**
///
/// Not a second copy of the flag word's presence bits, which is what the
/// distilled reference says it is — every cell carries a format key in some
/// slot, and byte 6 is zero on most of them. It says which of those keys the
/// app treats as chosen rather than inherited, and it is the only thing
/// separating a cell Numbers calls "automatic" from one it calls "number":
/// `numbers-formats.numbers` has both, and their records differ in exactly this
/// byte — flags `0x00003001`, number-format key 2, byte 6 `0x00` versus `0x01`.
///
/// The bit order is its own, and famously not the flag word's: duration is
/// `0x04` here and `0x10000` there, date is `0x08` here and `0x8000` there.
/// Nothing decodes *positions* from this byte — the payloads are consumed in
/// flag-word order — so the disagreement cannot shift a field; it can only make
/// a cell report the wrong format, which is what the app is asked about.
const EXPLICIT_FORMAT: &[(u8, FormatSlot)] = &[
    (0x01, FormatSlot::Number),
    (0x02, FormatSlot::Currency),
    (0x04, FormatSlot::Duration),
    (0x08, FormatSlot::Date),
    (0x20, FormatSlot::Boolean),
    (0x80, FormatSlot::Text),
];

impl CellRecord {
    /// The format slot the user set explicitly, if any.
    pub fn explicit_format(&self) -> Option<FormatSlot> {
        EXPLICIT_FORMAT
            .iter()
            .find(|&&(bit, _)| self.extras as u8 & bit != 0)
            .map(|&(_, slot)| slot)
    }

    /// The slot [`CellRecord::format_kind`] names.
    pub fn current_format(&self) -> Option<FormatSlot> {
        match self.format_kind? {
            1 => Some(FormatSlot::Number),
            2 => Some(FormatSlot::Currency),
            3 => Some(FormatSlot::Date),
            4 => Some(FormatSlot::Duration),
            5 => Some(FormatSlot::Text),
            6 => Some(FormatSlot::Boolean),
            _ => None,
        }
    }

    /// The explicit format slot, but only when it is the cell's current one.
    ///
    /// A cell keeps every format it has ever been given. The header of a column
    /// formatted as currency is a *text* cell carrying a currency format key, a
    /// text one, and the currency-explicit bit — and Numbers draws plain text
    /// and calls the format automatic. `format_kind` is the tie-breaker: it
    /// names the slot the value actually uses, and a format in any other slot
    /// is inert rather than wrong.
    pub fn applicable_format(&self) -> Option<FormatSlot> {
        let slot = self.explicit_format()?;
        match self.current_format() {
            Some(current) => (current == slot).then_some(slot),
            // No `format_kind` — fall back to what the value type can use.
            None => {
                let applies = match self.cell_type {
                    cell_type::TEXT | cell_type::RICH_TEXT => slot == FormatSlot::Text,
                    cell_type::NUMBER | cell_type::CURRENCY => {
                        matches!(slot, FormatSlot::Number | FormatSlot::Currency)
                    }
                    cell_type::DATE => slot == FormatSlot::Date,
                    cell_type::DURATION => slot == FormatSlot::Duration,
                    cell_type::BOOL => slot == FormatSlot::Boolean,
                    _ => false,
                };
                applies.then_some(slot)
            }
        }
    }

    /// The format key held in one slot.
    pub fn format_id_in(&self, slot: FormatSlot) -> Option<u32> {
        match slot {
            FormatSlot::Number => self.number_format_id,
            FormatSlot::Currency => self.currency_format_id,
            FormatSlot::Duration => self.duration_format_id,
            FormatSlot::Date => self.date_format_id,
            FormatSlot::Boolean => self.boolean_format_id,
            FormatSlot::Text => self.text_format_id,
        }
    }

    /// Key of the format this cell carries, whichever slot it is in.
    ///
    /// A cell names at most one of the six, and which one follows from its
    /// value type rather than from the format.
    pub fn format_id(&self) -> Option<u32> {
        self.number_format_id
            .or(self.currency_format_id)
            .or(self.date_format_id)
            .or(self.duration_format_id)
            .or(self.text_format_id)
            .or(self.boolean_format_id)
    }

    /// Bytes past the last payload the flag word names. Zero everywhere in the
    /// corpus, and the check that pins the flag word's widths and order.
    pub fn trailing(&self) -> usize {
        self.tail.len()
    }

    /// The key held in one flag-word slot, by bit.
    fn key_at(&self, bit: u32) -> Option<u32> {
        match bit {
            0x0000_0008 => self.string_id,
            0x0000_0010 => self.rich_id,
            0x0000_0020 => self.cell_style_id,
            0x0000_0040 => self.text_style_id,
            0x0000_0080 => self.conditional_style_id,
            0x0000_0100 => self.conditional_rule_id,
            0x0000_0200 => self.formula_id,
            0x0000_0400 => self.control_id,
            0x0000_0800 => self.formula_error_id,
            0x0000_1000 => self.format_kind,
            0x0000_2000 => self.number_format_id,
            0x0000_4000 => self.currency_format_id,
            0x0000_8000 => self.date_format_id,
            0x0001_0000 => self.duration_format_id,
            0x0002_0000 => self.text_format_id,
            0x0004_0000 => self.boolean_format_id,
            0x0008_0000 => self.comment_id,
            0x0010_0000 => self.import_warning_id,
            _ => None,
        }
    }

    /// The flag word this record's contents call for.
    ///
    /// Derived rather than remembered: the flag word is a presence mask over
    /// the fields below it, so a record that has been edited would otherwise
    /// have to keep the two in step by hand, and one missed bit shifts every
    /// payload after it.
    pub fn derived_flags(&self) -> u32 {
        let mut flags = 0;
        if self.decimal.is_some() {
            flags |= 0x0000_0001;
        }
        if self.double.is_some() {
            flags |= 0x0000_0002;
        }
        if self.seconds.is_some() {
            flags |= 0x0000_0004;
        }
        for &(bit, _, _) in &FLAGS[3..] {
            if self.key_at(bit).is_some() {
                flags |= bit;
            }
        }
        flags
    }

    /// Re-encode the record as a tile's storage buffer holds it.
    ///
    /// Exactly inverts [`decode_cell`] — `encode(decode(bytes)) == bytes` for
    /// every record in the corpus, which is what `tests/cells.rs` asserts.
    /// Fields this crate does not model are not re-synthesised: bytes 2–5,
    /// byte 6's chosen-format bits, byte 7 and any trailing bytes are carried
    /// through as they arrived, and so are the conditional-style and
    /// conditional-rule keys that a cell's highlighting hangs off.
    pub fn encode(&self) -> Result<Vec<u8>, String> {
        let known = FLAGS.iter().fold(0u32, |all, &(bit, _, _)| all | bit);
        if self.flags & !known != 0 {
            return Err(format!(
                "cell record flags {:#010x} carry a bit this crate cannot place a payload for",
                self.flags
            ));
        }
        let flags = self.derived_flags();
        let mut out = Vec::with_capacity(12 + 4 * flags.count_ones() as usize);
        out.push(self.version);
        out.push(self.cell_type);
        out.extend_from_slice(&self.reserved);
        out.extend_from_slice(&self.extras.to_le_bytes());
        out.extend_from_slice(&flags.to_le_bytes());
        for &(bit, _, _) in FLAGS {
            if flags & bit == 0 {
                continue;
            }
            match bit {
                0x0000_0001 => {
                    out.extend_from_slice(&encode_decimal128(self.decimal.expect("bit is set"))?)
                }
                0x0000_0002 => {
                    out.extend_from_slice(&self.double.expect("bit is set").to_le_bytes())
                }
                0x0000_0004 => {
                    out.extend_from_slice(&self.seconds.expect("bit is set").to_le_bytes())
                }
                other => {
                    out.extend_from_slice(&self.key_at(other).expect("bit is set").to_le_bytes())
                }
            }
        }
        out.extend_from_slice(&self.tail);
        Ok(out)
    }
}

/// Decode one cell record from a tile's storage buffer.
///
/// Fails on a version byte other than 5: versions 1–4 predate the "BNC" storage
/// engine, have a different layout, and no public decoder for them exists —
/// decoding one as version 5 would produce numbers rather than an error.
pub fn decode_cell(bytes: &[u8]) -> Result<CellRecord, String> {
    if bytes.len() < 12 {
        return Err(format!("cell record is {} bytes, need 12", bytes.len()));
    }
    let version = bytes[0];
    if version != 5 {
        return Err(format!("cell storage version {version} is not supported"));
    }
    let mut cell = CellRecord {
        version,
        cell_type: bytes[1],
        reserved: [bytes[2], bytes[3], bytes[4], bytes[5]],
        extras: u16::from_le_bytes([bytes[6], bytes[7]]),
        flags: u32::from_le_bytes([bytes[8], bytes[9], bytes[10], bytes[11]]),
        ..CellRecord::default()
    };

    let mut at = 12usize;
    for &(bit, width, name) in FLAGS {
        if cell.flags & bit == 0 {
            continue;
        }
        let end = at + width;
        if end > bytes.len() {
            return Err(format!(
                "cell record ends inside its {name} field ({} bytes, wanted {end})",
                bytes.len()
            ));
        }
        let payload = &bytes[at..end];
        let key = || u32::from_le_bytes(payload.try_into().expect("4-byte payload"));
        match bit {
            0x0000_0001 => cell.decimal = Some(decode_decimal128(payload.try_into().unwrap())),
            0x0000_0002 => cell.double = Some(f64::from_le_bytes(payload.try_into().unwrap())),
            0x0000_0004 => cell.seconds = Some(f64::from_le_bytes(payload.try_into().unwrap())),
            0x0000_0008 => cell.string_id = Some(key()),
            0x0000_0010 => cell.rich_id = Some(key()),
            0x0000_0020 => cell.cell_style_id = Some(key()),
            0x0000_0040 => cell.text_style_id = Some(key()),
            0x0000_0080 => cell.conditional_style_id = Some(key()),
            0x0000_0100 => cell.conditional_rule_id = Some(key()),
            0x0000_0200 => cell.formula_id = Some(key()),
            0x0000_0400 => cell.control_id = Some(key()),
            0x0000_0800 => cell.formula_error_id = Some(key()),
            0x0000_1000 => cell.format_kind = Some(key()),
            0x0000_2000 => cell.number_format_id = Some(key()),
            0x0000_4000 => cell.currency_format_id = Some(key()),
            0x0000_8000 => cell.date_format_id = Some(key()),
            0x0001_0000 => cell.duration_format_id = Some(key()),
            0x0002_0000 => cell.text_format_id = Some(key()),
            0x0004_0000 => cell.boolean_format_id = Some(key()),
            0x0008_0000 => cell.comment_id = Some(key()),
            0x0010_0000 => cell.import_warning_id = Some(key()),
            _ => unreachable!("FLAGS covers every bit it lists"),
        }
        at = end;
    }
    cell.tail = bytes[at..].to_vec();
    Ok(cell)
}

/// Slice one `TileRowInfo`'s storage buffer into per-column records.
///
/// `offsets` is the raw `cell_offsets` field: little-endian **signed** 16-bit
/// entries, one per column, `-1` for a column with no cell. With
/// `has_wide_offsets` each entry counts groups of four bytes. A record runs to
/// the next *non-negative* offset — the array is not dense, so stepping to the
/// next entry rather than the next present one shifts every cell after a gap.
pub fn row_cells<'a>(
    buffer: &'a [u8],
    offsets: &[u8],
    wide: bool,
) -> Vec<Option<(usize, &'a [u8])>> {
    let scale = if wide { 4usize } else { 1 };
    let starts: Vec<i32> = offsets
        .chunks_exact(2)
        .map(|pair| i16::from_le_bytes([pair[0], pair[1]]) as i32)
        .collect();
    let mut out = Vec::with_capacity(starts.len());
    for (column, &start) in starts.iter().enumerate() {
        if start < 0 {
            out.push(None);
            continue;
        }
        let begin = start as usize * scale;
        let end = starts[column + 1..]
            .iter()
            .find(|&&next| next >= 0)
            .map(|&next| next as usize * scale)
            .unwrap_or(buffer.len());
        if begin > buffer.len() || end > buffer.len() || end < begin {
            out.push(None);
            continue;
        }
        out.push(Some((column, &buffer[begin..end])));
    }
    out
}

/// Lay records back out into a `TileRowInfo`'s buffer and `cell_offsets`.
///
/// The inverse of [`row_cells`], and the reason a rewritten row is not simply
/// a patched buffer: every record after the edited one moves, so every offset
/// after it has to move too. The array keeps the length it arrived with —
/// Numbers pads it to 255 entries whatever the table's width — because that
/// padding is what a reader steps through.
pub fn encode_row(records: &[Option<Vec<u8>>], wide: bool) -> Result<(Vec<u8>, Vec<u8>), String> {
    let scale = if wide { 4usize } else { 1 };
    let mut buffer = Vec::new();
    let mut offsets = Vec::with_capacity(records.len() * 2);
    for (column, record) in records.iter().enumerate() {
        let Some(record) = record else {
            offsets.extend_from_slice(&(-1i16).to_le_bytes());
            continue;
        };
        if buffer.len() % scale != 0 {
            return Err(format!(
                "column {column} would start at byte {}, which is not a multiple of the \
                 {scale}-byte unit this row's offsets count in",
                buffer.len()
            ));
        }
        let start = buffer.len() / scale;
        let start = i16::try_from(start).map_err(|_| {
            format!(
                "row storage reached {} bytes, past what an offset can name",
                buffer.len()
            )
        })?;
        offsets.extend_from_slice(&start.to_le_bytes());
        buffer.extend_from_slice(record);
    }
    Ok((buffer, offsets))
}

// -- side tables -------------------------------------------------------------

/// One entry of a `TST.TableDataList`, kept in whichever shape it arrived in.
#[derive(Debug, Clone, Default)]
pub struct ListEntry {
    pub key: u32,
    pub refcount: u32,
    /// Field 3 — `STRING` lists.
    pub string: Option<String>,
    /// Field 4 — `STYLE` and `CONDITIONAL_STYLE` lists.
    pub reference: Option<u64>,
    /// Field 6 — `FORMAT`, as a `TSK.FormatStructArchive`.
    pub format: Option<Message>,
    /// Field 9 — `RICH_TEXT_PAYLOAD`.
    pub rich_text: Option<u64>,
    /// Field 12 — `CONTROL_CELL_SPEC`, as a `TST.CellSpecArchive`.
    pub cell_spec: Option<Message>,
    /// Field 5 — `FORMULA`. Kept undecoded; formulas are Phase 5's job.
    pub has_formula: bool,
}

/// A decoded `TST.TableDataList`, keyed the way cells address it.
///
/// Keys start at 1 and a stored key of 0 means "absent", so this is a map and
/// not a vector; entries also arrive in no guaranteed order.
#[derive(Debug, Clone, Default)]
pub struct DataList {
    pub list_type: u32,
    pub entries: BTreeMap<u32, ListEntry>,
}

impl DataList {
    pub fn decode(archive: &Message) -> DataList {
        let mut list = DataList {
            list_type: archive.varint(1).unwrap_or(0) as u32,
            entries: BTreeMap::new(),
        };
        for value in archive.all(3) {
            let Value::Bytes(raw) = value else { continue };
            let Some(entry) = decode_nested(raw) else {
                continue;
            };
            let key = entry.varint(1).unwrap_or(0) as u32;
            list.entries.insert(
                key,
                ListEntry {
                    key,
                    refcount: entry.varint(2).unwrap_or(0) as u32,
                    string: entry
                        .bytes(3)
                        .map(|b| String::from_utf8_lossy(b).into_owned()),
                    reference: entry.bytes(4).and_then(reference),
                    format: entry.bytes(6).and_then(decode_nested),
                    rich_text: entry.bytes(9).and_then(reference),
                    cell_spec: entry.bytes(12).and_then(decode_nested),
                    has_formula: entry.get(5).is_some(),
                },
            );
        }
        list
    }

    pub fn string(&self, key: u32) -> Option<&str> {
        self.entries.get(&key)?.string.as_deref()
    }
}

/// `TSP.Reference` — a message whose field 1 is the target's object identifier.
pub fn reference(bytes: &[u8]) -> Option<u64> {
    decode_nested(bytes)?.varint(1).filter(|&id| id != 0)
}

// -- formats and controls ----------------------------------------------------

/// How a cell's value is displayed — the same list the app's own inspector
/// shows, and the same names AppleScript reports for `format of cell`.
///
/// Three things decide it, in this order: a control definition wins outright,
/// then byte 6 of the cell record says whether any format was chosen at all,
/// then `TSK.FormatStructArchive.format_type` says which. See
/// [`CellFormat::of`] and `FORMAT.md` §Tables.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CellFormat {
    Automatic,
    Number,
    Currency,
    Percentage,
    Scientific,
    Fraction,
    NumeralSystem,
    Text,
    DateTime,
    Duration,
    Checkbox,
    Rating,
    Slider,
    Stepper,
    PopUpMenu,
    /// A `format_type` this crate has not seen a document use, kept as its raw
    /// code rather than guessed at.
    Other(u32),
}

impl CellFormat {
    /// `TSK.FormatStructArchive.format_type`.
    ///
    /// The values are Apple's, start at 256, and were read off cells whose
    /// format the app itself named through AppleScript. 264, 265 and 266 sit
    /// unclaimed between `checkbox` and `rating` — three gaps, and exactly the
    /// three controls (pop-up menu, stepper, slider) whose cells carry a
    /// *number* format plus a control definition instead — but nothing here
    /// observed them, so they stay [`CellFormat::Other`].
    pub fn from_format_type(code: u32) -> CellFormat {
        match code {
            256 => CellFormat::Number,
            257 => CellFormat::Currency,
            258 => CellFormat::Percentage,
            259 => CellFormat::Scientific,
            260 => CellFormat::Automatic,
            261 => CellFormat::DateTime,
            262 => CellFormat::Fraction,
            263 => CellFormat::Checkbox,
            267 => CellFormat::Rating,
            268 => CellFormat::Duration,
            269 => CellFormat::NumeralSystem,
            other => CellFormat::Other(other),
        }
    }

    /// The format the app would report for a cell.
    ///
    /// * A control definition decides it: a slider cell carries the plain
    ///   number format `256` and Numbers still calls it a slider.
    /// * Otherwise byte 6 of the record has to name a slot, and that slot has
    ///   to be one the value can use ([`CellRecord::applicable_format`]). Every
    ///   cell carries a format key — a plain text cell points at `format_type`
    ///   260 — and without that byte there is no difference between a cell the
    ///   user made a number and one that merely holds a number.
    /// * The text slot is its own answer. Numbers writes `format_type` 260
    ///   there, which everywhere else means "automatic", and calls the cell
    ///   text.
    pub fn of(
        record: &CellRecord,
        control: Option<CellControl>,
        format_type: Option<u32>,
    ) -> CellFormat {
        if let Some(control) = control {
            if let Some(format) = control.as_format() {
                return format;
            }
        }
        match record.applicable_format() {
            None => CellFormat::Automatic,
            Some(FormatSlot::Text) => CellFormat::Text,
            Some(_) => format_type
                .map(CellFormat::from_format_type)
                .unwrap_or(CellFormat::Automatic),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            CellFormat::Automatic => "automatic",
            CellFormat::Number => "number",
            CellFormat::Currency => "currency",
            CellFormat::Percentage => "percent",
            CellFormat::Scientific => "scientific",
            CellFormat::Fraction => "fraction",
            CellFormat::NumeralSystem => "numeral system",
            CellFormat::Text => "text",
            CellFormat::DateTime => "date and time",
            CellFormat::Duration => "duration",
            CellFormat::Checkbox => "checkbox",
            CellFormat::Rating => "rating",
            CellFormat::Slider => "slider",
            CellFormat::Stepper => "stepper",
            CellFormat::PopUpMenu => "pop up menu",
            CellFormat::Other(_) => "other",
        }
    }
}

impl std::fmt::Display for CellFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CellFormat::Other(code) => write!(f, "format {code}"),
            known => write!(f, "{}", known.as_str()),
        }
    }
}

/// `TST.CellSpecArchive.interaction_type` — what a control cell offers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CellControl {
    ValueEditing,
    FormulaEditing,
    Stock,
    CategorySummary,
    Stepper,
    Slider,
    Rating,
    PopUpMenu,
    Checkbox,
    Other(u32),
}

impl CellControl {
    pub fn from_code(code: u32) -> CellControl {
        match code {
            0 => CellControl::ValueEditing,
            1 => CellControl::FormulaEditing,
            2 => CellControl::Stock,
            3 => CellControl::CategorySummary,
            4 => CellControl::Stepper,
            5 => CellControl::Slider,
            6 => CellControl::Rating,
            7 => CellControl::PopUpMenu,
            8 => CellControl::Checkbox,
            other => CellControl::Other(other),
        }
    }

    /// The format name the app reports for a cell carrying this control, or
    /// `None` for the two that are not controls at all.
    pub fn as_format(self) -> Option<CellFormat> {
        match self {
            CellControl::Stepper => Some(CellFormat::Stepper),
            CellControl::Slider => Some(CellFormat::Slider),
            CellControl::Rating => Some(CellFormat::Rating),
            CellControl::PopUpMenu => Some(CellFormat::PopUpMenu),
            CellControl::Checkbox => Some(CellFormat::Checkbox),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            CellControl::ValueEditing => "value editing",
            CellControl::FormulaEditing => "formula editing",
            CellControl::Stock => "stock",
            CellControl::CategorySummary => "category summary",
            CellControl::Stepper => "stepper",
            CellControl::Slider => "slider",
            CellControl::Rating => "rating",
            CellControl::PopUpMenu => "pop-up menu",
            CellControl::Checkbox => "checkbox",
            CellControl::Other(_) => "other",
        }
    }
}

// -- the reader's view -------------------------------------------------------

/// What a cell holds, once its keys have been resolved.
#[derive(Debug, Clone, PartialEq)]
pub enum CellValue {
    Empty,
    /// Covered by a merge that starts in another cell.
    Span,
    Text(String),
    Number(Decimal),
    Currency(Decimal),
    Bool(bool),
    /// Seconds since 2001-01-01, timezone-naive, as the format stores it.
    Date(f64),
    /// Seconds.
    Duration(f64),
    /// A cell whose formula failed. The record carries only a key into the
    /// formula-error list; the message itself is not decoded here.
    Error,
    /// A cell whose text is a `TSWP.StorageArchive` rather than a table string.
    RichText(String),
    /// A record whose type byte this crate does not know, kept as the raw code.
    Unknown(u8),
}

impl CellValue {
    pub fn is_empty(&self) -> bool {
        matches!(self, CellValue::Empty)
    }

    /// The value as text, in the shape a CSV or a diff wants: exact for
    /// numbers, ISO-8601 for dates, seconds for durations.
    pub fn to_text(&self) -> String {
        match self {
            CellValue::Empty | CellValue::Span => String::new(),
            CellValue::Text(t) | CellValue::RichText(t) => t.clone(),
            CellValue::Number(d) | CellValue::Currency(d) => d.to_string(),
            CellValue::Bool(b) => b.to_string(),
            CellValue::Date(seconds) => format_date(*seconds),
            CellValue::Duration(seconds) => format!("{seconds}s"),
            CellValue::Error => "#ERROR".to_string(),
            CellValue::Unknown(code) => format!("#TYPE{code}"),
        }
    }

    pub fn kind(&self) -> &'static str {
        match self {
            CellValue::Empty => "empty",
            CellValue::Span => "span",
            CellValue::Text(_) => "text",
            CellValue::Number(_) => "number",
            CellValue::Currency(_) => "currency",
            CellValue::Bool(_) => "bool",
            CellValue::Date(_) => "date",
            CellValue::Duration(_) => "duration",
            CellValue::Error => "error",
            CellValue::RichText(_) => "rich text",
            CellValue::Unknown(_) => "unknown",
        }
    }
}

/// One cell, resolved.
#[derive(Debug, Clone)]
pub struct Cell {
    pub row: usize,
    pub column: usize,
    pub value: CellValue,
    /// The format the app would report — see [`CellFormat::of`].
    pub format: CellFormat,
    pub control: Option<CellControl>,
    /// Set when the cell carries a formula. The formula itself is a `TSCE`
    /// archive in the table's formula list; reading it is Phase 5.
    pub has_formula: bool,
    /// The record as it was on the wire, for callers that want more than this
    /// crate models yet.
    pub record: CellRecord,
}

/// A row or column's persisted geometry and hidden state.
#[derive(Debug, Clone, Default)]
pub struct Extent {
    /// Height in points for a row, width for a column — **only when the row or
    /// column has been given one by hand**. Numbers writes a literal `0` for
    /// every row and column that is still at the table's default, and that is
    /// the usual case; a row with no header entry at all — one holding no
    /// cells — reads the same way.
    pub size: Option<f32>,
    /// `hidingState`: 0 is visible. Numbers keeps *why* a row is hidden — the
    /// user hid it, or a filter did — and this is the only per-row trace of it.
    /// See [`Extent::hiding`] for the codes.
    pub hiding_state: u32,
    pub cell_count: u32,
}

/// Why a row or column is not on screen.
///
/// The codes were read off `numbers-rules.numbers`, where the *same document*
/// carries both: three columns the template's author hid by hand come back as
/// `1`, and the nine rows a filter rule excludes come back as `2`. Nothing else
/// in the corpus is hidden at all, which is why Phase 1 could only report the
/// number.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hiding {
    Visible,
    /// The user hid it — Table ▸ Hide Rows, or the row's own menu.
    User,
    /// A filter rule excluded it.
    Filter,
    /// A `hidingState` this crate has not seen, kept as its code.
    Other(u32),
}

impl Hiding {
    pub fn as_str(self) -> &'static str {
        match self {
            Hiding::Visible => "visible",
            Hiding::User => "hidden by the user",
            Hiding::Filter => "hidden by a filter",
            Hiding::Other(_) => "hidden",
        }
    }
}

impl Extent {
    pub fn hidden(&self) -> bool {
        self.hiding_state != 0
    }

    pub fn hiding(&self) -> Hiding {
        match self.hiding_state {
            0 => Hiding::Visible,
            1 => Hiding::User,
            2 => Hiding::Filter,
            other => Hiding::Other(other),
        }
    }
}

/// A merged range: the cell at `row`/`column` spans `rows` × `columns`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Merge {
    pub row: usize,
    pub column: usize,
    pub rows: usize,
    pub columns: usize,
}

// -- how a table is organised ------------------------------------------------
//
// Everything from here down is layered *on top of* the cells: the sort rules
// and filters that decide which rows are on screen and in what order, the
// categories that group them, the conditional-highlighting rules that recolour
// them, and the pivot tables that summarise them.
//
// One thing separates this half of the format from the cell half. Cells are
// addressed by index. **Organisation is addressed by UUID** — every row, every
// column, every group and every owner carries one — because a sort or a filter
// moves indexes and a UUID survives it. Reading any of it therefore starts by
// building the index the table keeps for exactly that purpose,
// `TST.ColumnRowUIDMapArchive`.

/// A `TSP.UUID`: two 64-bit halves, `{1: lower, 2: upper}`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Uuid {
    pub lower: u64,
    pub upper: u64,
}

impl Uuid {
    pub fn decode(message: &Message) -> Uuid {
        Uuid {
            lower: message.varint(1).unwrap_or(0),
            upper: message.varint(2).unwrap_or(0),
        }
    }

    fn read(raw: &[u8]) -> Option<Uuid> {
        decode_nested(raw).map(|m| Uuid::decode(&m))
    }
}

impl std::fmt::Display for Uuid {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:016x}{:016x}", self.upper, self.lower)
    }
}

/// `TST.ColumnRowUIDMapArchive` (6267) — the join between UUIDs and indexes.
///
/// The trap is in the field names, and the mined references do not carry them:
/// field 1 is `sorted_column_uids`, **sorted by UUID and not by position**, and
/// field 2 is `column_index_for_uid`, the index each of those is at. Reading
/// field 1 as "the UUID of column *i*" gives the right answer for every column
/// that happens to be a fixed point of that permutation — which in a five-column
/// table is most of them, so the mistake survives a casual check.
#[derive(Debug, Clone, Default)]
pub struct UidMap {
    columns: BTreeMap<Uuid, usize>,
    rows: BTreeMap<Uuid, usize>,
}

impl UidMap {
    pub fn decode(archive: &Message) -> UidMap {
        UidMap {
            columns: uid_index(archive, 1, 2),
            rows: uid_index(archive, 4, 5),
        }
    }

    /// Index of the column with this UUID.
    pub fn column(&self, uid: Uuid) -> Option<usize> {
        self.columns.get(&uid).copied()
    }

    /// Index of the row with this UUID.
    pub fn row(&self, uid: Uuid) -> Option<usize> {
        self.rows.get(&uid).copied()
    }

    /// UUID of the column at an index — the lookup the other way round.
    pub fn column_uid(&self, index: usize) -> Option<Uuid> {
        self.columns
            .iter()
            .find(|(_, &at)| at == index)
            .map(|(&uid, _)| uid)
    }

    /// UUID of the row at an index.
    pub fn row_uid(&self, index: usize) -> Option<Uuid> {
        self.rows
            .iter()
            .find(|(_, &at)| at == index)
            .map(|(&uid, _)| uid)
    }

    pub fn is_empty(&self) -> bool {
        self.columns.is_empty() && self.rows.is_empty()
    }
}

/// Zip a repeated-UUID field against the repeated index field beside it.
fn uid_index(archive: &Message, uid_field: u32, index_field: u32) -> BTreeMap<Uuid, usize> {
    let uids = archive.all(uid_field).filter_map(|value| match value {
        Value::Bytes(raw) => Uuid::read(raw),
        _ => None,
    });
    uids.zip(varints(archive, index_field))
        .map(|(uid, index)| (uid, index as usize))
        .collect()
}

/// A `repeated` scalar field, however it was written.
///
/// proto2 leaves `repeated uint32` unpacked by default and Numbers writes it
/// that way, but the wire format allows either and a packed field arrives as
/// one `bytes` value. Reading only the unpacked form silently yields an empty
/// list, which reads as "no rules" rather than as an error.
fn varints(message: &Message, number: u32) -> Vec<u64> {
    let mut out = Vec::new();
    for value in message.all(number) {
        match value {
            Value::Varint(n) => out.push(*n),
            Value::Bytes(raw) => {
                let mut at = 0usize;
                while at < raw.len() {
                    let mut shift = 0u32;
                    let mut acc = 0u64;
                    loop {
                        let Some(&byte) = raw.get(at) else { return out };
                        at += 1;
                        acc |= ((byte & 0x7f) as u64) << shift;
                        if byte & 0x80 == 0 {
                            break;
                        }
                        shift += 7;
                        if shift > 63 {
                            return out;
                        }
                    }
                    out.push(acc);
                }
            }
            _ => {}
        }
    }
    out
}

/// A `TSCE.IndexSetArchive` — inclusive `{range_begin, range_end?}` runs.
fn index_set(message: &Message) -> Vec<usize> {
    let mut out = Vec::new();
    for value in message.all(1) {
        let Value::Bytes(raw) = value else { continue };
        let Some(entry) = decode_nested(raw) else {
            continue;
        };
        let Some(begin) = entry.varint(1) else {
            continue;
        };
        let end = entry.varint(2).unwrap_or(begin);
        if end < begin || end - begin > 1_000_000 {
            continue;
        }
        out.extend(begin as usize..=end as usize);
    }
    out
}

/// A `TSCE.CellValueArchive`, which is how a category group names the value it
/// groups on and how a predicate names the value it compares against.
fn cell_value(message: &Message) -> Option<CellValue> {
    match message.varint(1)? {
        1 => Some(CellValue::Empty),
        2 => Some(CellValue::Bool(
            message
                .bytes(2)
                .and_then(decode_nested)
                .and_then(|b| b.varint(1))
                .unwrap_or(0)
                != 0,
        )),
        3 => Some(CellValue::Date(
            message
                .bytes(3)
                .and_then(decode_nested)
                .map(|d| double_field(&d, 1))
                .unwrap_or(0.0),
        )),
        4 => {
            let number = message.bytes(4).and_then(decode_nested)?;
            Some(CellValue::Number(Decimal {
                mantissa: double_field(&number, 1) as i128,
                exponent: 0,
            }))
        }
        5 => Some(CellValue::Text(
            message
                .bytes(5)
                .and_then(decode_nested)
                .map(|s| string_field(&s, 1))
                .unwrap_or_default(),
        )),
        _ => None,
    }
}

/// One rule of `TST.TableSortOrderArchive` — a column and a direction.
///
/// `TableModelArchive` field 44. The archive's own `type` field says whether the
/// rules cover the whole table or a row range; every one observed here is
/// `entire_table`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SortRule {
    pub column: usize,
    pub descending: bool,
}

/// What a filter or a conditional-highlighting rule tests.
///
/// The condition itself is a `predicate_type` code plus up to three arguments,
/// and the arguments are either immediate values or a `TSCE` formula. This
/// crate reads the code and the immediate values and leaves the formula to
/// Phase 5 — a rule that compares against another cell says so through
/// [`Predicate::has_formula`] rather than being reported as having no argument.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Predicate {
    /// `predicate_type`. Apple publishes no names for these; the code is
    /// reported as it stands rather than guessed at.
    pub kind: i64,
    pub qualifiers: (i64, i64),
    pub values: Vec<CellValue>,
    pub has_formula: bool,
    /// True for the older `…PrePivot` shape, whose fields are numbered
    /// differently at the same numbers — see `FORMAT.md`.
    pub pre_pivot: bool,
}

impl Predicate {
    /// `TST.FormulaPredicateArchive` — the current shape, `predicate_type` first.
    fn decode(message: &Message) -> Predicate {
        let mut values = Vec::new();
        for number in 4..=6 {
            let Some(argument) = message.bytes(number).and_then(decode_nested) else {
                continue;
            };
            if let Some(value) = argument
                .bytes(2)
                .and_then(decode_nested)
                .and_then(|data| predicate_value(&data))
            {
                values.push(value);
            }
        }
        Predicate {
            kind: message.varint(1).unwrap_or(0) as i64,
            qualifiers: (
                message.varint(2).unwrap_or(0) as i64,
                message.varint(3).unwrap_or(0) as i64,
            ),
            values,
            has_formula: message.get(7).is_some(),
            pre_pivot: false,
        }
    }

    /// `TST.FormulaPredicatePrePivotArchive` — the older shape, whose field 1
    /// is the *formula* and whose `predicate_type` is field 2. The two are
    /// wire-incompatible at the same field numbers and are told apart by that:
    /// a length-delimited field 1 is the old one, a varint the new one.
    fn decode_pre_pivot(message: &Message) -> Predicate {
        Predicate {
            kind: message.varint(2).unwrap_or(0) as i64,
            qualifiers: (
                message.varint(3).unwrap_or(0) as i64,
                message.varint(4).unwrap_or(0) as i64,
            ),
            values: Vec::new(),
            has_formula: message.get(1).is_some(),
            pre_pivot: true,
        }
    }

    /// Read whichever shape this is.
    fn decode_either(message: &Message) -> Predicate {
        match message.get(1) {
            Some(Value::Bytes(_)) => Predicate::decode_pre_pivot(message),
            _ => Predicate::decode(message),
        }
    }
}

/// A `TST.FormulaPredArgDataArchive` — one immediate argument of a predicate.
fn predicate_value(data: &Message) -> Option<CellValue> {
    if let Some(text) = data.bytes(4) {
        return Some(CellValue::Text(String::from_utf8_lossy(text).into_owned()));
    }
    if let Some(Value::Varint(flag)) = data.get(8) {
        return Some(CellValue::Bool(*flag != 0));
    }
    if let Some(Value::Fixed64(bits)) = data.get(5) {
        return Some(CellValue::Date(f64::from_le_bytes(*bits)));
    }
    if let Some(Value::Fixed64(bits)) = data.get(6) {
        return Some(CellValue::Duration(f64::from_le_bytes(*bits)));
    }
    if let Some(Value::Fixed64(bits)) = data.get(1) {
        return Some(CellValue::Number(Decimal {
            mantissa: f64::from_le_bytes(*bits) as i128,
            exponent: 0,
        }));
    }
    None
}

/// One rule of a filter set.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct FilterRule {
    /// Column the rule tests, from `FilterSetArchive.filter_offsets`.
    pub column: Option<usize>,
    pub enabled: bool,
    pub predicate: Predicate,
}

/// `TST.FilterSetArchive` (6220) — a table's whole filter.
///
/// Every table has one whether or not it filters anything: a document with
/// three tables carries seven of these, all eight bytes long and all empty, and
/// only the ones with rules mean anything.
#[derive(Debug, Clone, Default)]
pub struct FilterSet {
    pub identifier: u64,
    /// `false` is "All" — every rule must match; `true` is "Any".
    pub match_any: bool,
    /// The Organise pane's filter switch. Defaults to **true** when the field
    /// is absent, which is what an empty archive means.
    pub enabled: bool,
    pub rules: Vec<FilterRule>,
}

impl FilterSet {
    pub fn decode(identifier: u64, archive: &Message) -> FilterSet {
        let offsets = varints(archive, 5);
        let per_rule = varints(archive, 6);
        let mut rules = Vec::new();
        // Field 3 is the pre-pivot slot and field 7 the current one. Numbers
        // 15.3.1 writes the **pre-pivot** one for filters — the reverse of what
        // the published references say — so the current slot is preferred where
        // it exists and the older one is what is actually read here.
        let current = archive.all(7).count() > 0;
        for (slot, number) in [(true, 3u32), (false, 7)] {
            if slot == current {
                continue;
            }
            for value in archive.all(number) {
                let Value::Bytes(raw) = value else { continue };
                let Some(rule) = decode_nested(raw) else {
                    continue;
                };
                let Some(predicate) = rule.bytes(1).and_then(decode_nested) else {
                    continue;
                };
                let at = rules.len();
                rules.push(FilterRule {
                    column: offsets.get(at).map(|&n| n as usize),
                    // `disabled` is the old slot's own field; the new slot
                    // keeps the flag in the set's parallel array instead.
                    enabled: if slot {
                        rule.varint(2).unwrap_or(0) == 0
                    } else {
                        per_rule.get(at).copied().unwrap_or(1) != 0
                    },
                    predicate: Predicate::decode_either(&predicate),
                });
            }
        }
        FilterSet {
            identifier,
            match_any: archive.varint(1).unwrap_or(0) != 0,
            enabled: archive.varint(2).unwrap_or(1) != 0,
            rules,
        }
    }
}

/// How a category or a pivot summarises one column.
///
/// `agg_type` is Apple's code. **2 is Sum**, proven twice over: the pivot
/// fixture labels the column Numbers renders from it "Units (Sum)", and the
/// category fixture's accumulator for the same code holds 275, which is the sum
/// of its ten values and not their count, mean, minimum or maximum. The others
/// are reported as codes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Aggregate {
    Sum,
    Other(u32),
}

impl Aggregate {
    pub fn from_code(code: u32) -> Aggregate {
        match code {
            2 => Aggregate::Sum,
            other => Aggregate::Other(other),
        }
    }
}

impl std::fmt::Display for Aggregate {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Aggregate::Sum => write!(f, "sum"),
            Aggregate::Other(code) => write!(f, "aggregate {code}"),
        }
    }
}

/// `TST.ColumnAggregateArchive` — one summary assignment.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ColumnAggregate {
    pub column: Option<usize>,
    pub column_uid: Uuid,
    /// Which nesting level of the category this summary belongs to; a pivot
    /// value uses 0.
    pub level: u32,
    pub function: Aggregate,
    /// `show_as_type` — the pivot's "Show As" (running total, % of total…).
    /// 0 everywhere in this corpus.
    pub show_as: u32,
}

impl ColumnAggregate {
    fn decode(message: &Message, uids: &UidMap) -> ColumnAggregate {
        let column_uid = message.bytes(1).and_then(Uuid::read).unwrap_or_default();
        ColumnAggregate {
            column: uids.column(column_uid),
            column_uid,
            level: message.varint(2).unwrap_or(0) as u32,
            function: Aggregate::from_code(message.varint(3).unwrap_or(0) as u32),
            show_as: message.varint(4).unwrap_or(0) as u32,
        }
    }
}

/// `TST.GroupColumnArchive` — a column rows are grouped by.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GroupColumn {
    pub column: Option<usize>,
    pub column_uid: Uuid,
    /// How values are bucketed. 0 is "every distinct value"; the date and
    /// number buckets carry a `TSCE.FunctorArchive` this crate does not read,
    /// and the pivot fixture's date-by-month column is `7`.
    pub grouping_type: u32,
    /// True when a functor is present — i.e. the values are bucketed rather
    /// than taken as they are.
    pub has_functor: bool,
}

impl GroupColumn {
    fn decode(message: &Message, uids: &UidMap) -> GroupColumn {
        let column_uid = message.bytes(1).and_then(Uuid::read).unwrap_or_default();
        GroupColumn {
            column: uids.column(column_uid),
            column_uid,
            grouping_type: message.varint(2).unwrap_or(0) as u32,
            has_functor: message.get(3).is_some(),
        }
    }
}

/// One node of a category's group tree — `TST.GroupByArchive.GroupNodeArchive`
/// (6383).
///
/// **Field 2 does not exist**; the children are field 3 inline or field 10 by
/// reference, and 15.3.1 writes both forms in the same document — the tree
/// inline on the `GroupByArchive` and the same tree again as free-standing
/// objects.
#[derive(Debug, Clone, Default)]
pub struct Group {
    pub uid: Uuid,
    /// The value every row in this group shares. Absent on the root.
    pub value: Option<CellValue>,
    /// Rows in the group, as the node records them.
    pub rows: Vec<usize>,
    /// Whether the group is collapsed, from the row hidden-state extent's
    /// `collapsed_group_uids`.
    pub collapsed: bool,
    pub children: Vec<Group>,
}

impl Group {
    fn decode(document: &crate::Document, message: &Message, collapsed: &[Uuid]) -> Group {
        let uid = message.bytes(1).and_then(Uuid::read).unwrap_or_default();
        let mut children = Vec::new();
        for value in message.all(3) {
            let Value::Bytes(raw) = value else { continue };
            if let Some(child) = decode_nested(raw) {
                children.push(Group::decode(document, &child, collapsed));
            }
        }
        for value in message.all(10) {
            let Value::Bytes(raw) = value else { continue };
            if let Some(child) = reference(raw).and_then(|id| archive(document, id)) {
                children.push(Group::decode(document, &child, collapsed));
            }
        }
        // Field 8 is the index set of rows and field 9 the lookup positions;
        // 15.3.1 writes only 9, whose ranges are row indexes in every document
        // read here.
        let rows = message
            .bytes(8)
            .or_else(|| message.bytes(9))
            .and_then(decode_nested)
            .map(|set| index_set(&set))
            .unwrap_or_default();
        Group {
            uid,
            value: message
                .bytes(7)
                .and_then(decode_nested)
                .and_then(|v| cell_value(&v)),
            rows,
            collapsed: collapsed.contains(&uid),
            children,
        }
    }

    /// Every group under this one, this one included, depth first.
    pub fn walk(&self) -> Vec<&Group> {
        let mut out = vec![self];
        for child in &self.children {
            out.extend(child.walk());
        }
        out
    }
}

/// A category — one `TST.GroupByArchive` (6373).
#[derive(Debug, Clone, Default)]
pub struct Category {
    pub identifier: u64,
    pub uid: Uuid,
    /// The Organise pane's Categories switch.
    pub enabled: bool,
    /// Source columns, outermost group first.
    pub columns: Vec<GroupColumn>,
    /// Summary-row assignments, one per (column, level).
    pub summaries: Vec<ColumnAggregate>,
    /// The group tree's root. Its children are the top-level groups.
    pub root: Option<Group>,
}

impl Category {
    fn decode(
        document: &crate::Document,
        identifier: u64,
        archive_: &Message,
        uids: &UidMap,
        collapsed: &[Uuid],
    ) -> Category {
        let root = archive_
            .bytes(3)
            .and_then(decode_nested)
            .or_else(|| {
                archive_
                    .bytes(18)
                    .and_then(reference)
                    .and_then(|id| archive(document, id))
            })
            .map(|node| Group::decode(document, &node, collapsed));
        Category {
            identifier,
            uid: archive_.bytes(1).and_then(Uuid::read).unwrap_or_default(),
            enabled: archive_.varint(6).unwrap_or(0) != 0,
            columns: archive_
                .all(2)
                .filter_map(|value| match value {
                    Value::Bytes(raw) => decode_nested(raw),
                    _ => None,
                })
                .map(|column| GroupColumn::decode(&column, uids))
                .collect(),
            summaries: archive_
                .all(5)
                .filter_map(|value| match value {
                    Value::Bytes(raw) => decode_nested(raw),
                    _ => None,
                })
                .map(|aggregate| ColumnAggregate::decode(&aggregate, uids))
                .collect(),
            root,
        }
    }

    /// Every group, depth first, the root excluded.
    pub fn groups(&self) -> Vec<&Group> {
        match &self.root {
            Some(root) => root.children.iter().flat_map(Group::walk).collect(),
            None => Vec::new(),
        }
    }
}

/// A pivot table's rules — `TST.PivotOwnerArchive` (6370).
///
/// **The archive has no field 1.** Rows and columns are two lists of the same
/// `GroupColumnArchive` type, values are `ColumnAggregateArchive`s, and the two
/// grand-total switches are separate booleans well past them.
#[derive(Debug, Clone, Default)]
pub struct Pivot {
    pub identifier: u64,
    pub uid: Uuid,
    /// Name of the table the pivot summarises, as the pivot itself records it.
    pub source_name: String,
    pub source_uid: Uuid,
    /// Fields dropped in the Rows well.
    pub rows: Vec<GroupColumn>,
    /// Fields dropped in the Columns well.
    pub columns: Vec<GroupColumn>,
    /// Fields dropped in the Values well, with their summary functions.
    pub values: Vec<ColumnAggregate>,
    pub hide_grand_total_rows: bool,
    pub hide_grand_total_columns: bool,
    /// A pivot with nothing assigned yet. The fixture has one on purpose.
    pub empty: bool,
}

impl Pivot {
    fn decode(identifier: u64, archive_: &Message, uids: &UidMap) -> Pivot {
        let group_columns = |number: u32| -> Vec<GroupColumn> {
            archive_
                .bytes(number)
                .and_then(decode_nested)
                .map(|list| {
                    list.all(1)
                        .filter_map(|value| match value {
                            Value::Bytes(raw) => decode_nested(raw),
                            _ => None,
                        })
                        .map(|column| GroupColumn::decode(&column, uids))
                        .collect()
                })
                .unwrap_or_default()
        };
        Pivot {
            identifier,
            uid: archive_.bytes(2).and_then(Uuid::read).unwrap_or_default(),
            source_name: string_field(archive_, 12),
            source_uid: archive_.bytes(8).and_then(Uuid::read).unwrap_or_default(),
            rows: group_columns(3),
            columns: group_columns(4),
            values: archive_
                .bytes(5)
                .and_then(decode_nested)
                .map(|list| {
                    list.all(1)
                        .filter_map(|value| match value {
                            Value::Bytes(raw) => decode_nested(raw),
                            _ => None,
                        })
                        .map(|aggregate| ColumnAggregate::decode(&aggregate, uids))
                        .collect()
                })
                .unwrap_or_default(),
            hide_grand_total_rows: archive_.varint(11).unwrap_or(0) != 0,
            hide_grand_total_columns: archive_.varint(13).unwrap_or(0) != 0,
            empty: archive_.varint(7).unwrap_or(0) != 0,
        }
    }
}

/// One conditional-highlighting rule.
#[derive(Debug, Clone, Default)]
pub struct ConditionalRule {
    pub predicate: Predicate,
    /// The cell style applied when the rule matches.
    pub cell_style: Option<u64>,
    pub text_style: Option<u64>,
}

/// `TST.ConditionalStyleSetArchive` (6010) — the ordered rules for one range.
///
/// Cells point at one of these through the CONDITIONAL_STYLE data list, which
/// is why a set is reported with the key that reaches it: a whole column of
/// highlighted cells shares one set and one key.
#[derive(Debug, Clone, Default)]
pub struct ConditionalStyles {
    pub identifier: u64,
    /// Key in the table's CONDITIONAL_STYLE `TableDataList`, where one reaches
    /// this set.
    pub key: Option<u32>,
    pub rules: Vec<ConditionalRule>,
}

impl ConditionalStyles {
    fn decode(identifier: u64, key: Option<u32>, archive_: &Message) -> ConditionalStyles {
        let mut rules = Vec::new();
        // Numbers 15.3.1 writes **both** slots, with the same rules in each:
        // field 2 in the pre-pivot shape and field 3 wrapping the current one.
        // They are not equivalent — only the current shape carries the value
        // the rule compares against, because the pre-pivot shape keeps it
        // inside a formula — so the current slot wins outright and the older
        // one is the fallback for a document that has only it.
        let current: Vec<Message> = archive_
            .bytes(3)
            .and_then(decode_nested)
            .map(|wrapper| {
                wrapper
                    .all(1)
                    .filter_map(|value| match value {
                        Value::Bytes(raw) => decode_nested(raw),
                        _ => None,
                    })
                    .collect()
            })
            .unwrap_or_default();
        let raws = if current.is_empty() {
            archive_
                .all(2)
                .filter_map(|value| match value {
                    Value::Bytes(raw) => decode_nested(raw),
                    _ => None,
                })
                .collect()
        } else {
            current
        };
        for rule in raws {
            rules.push(ConditionalRule {
                predicate: rule
                    .bytes(1)
                    .and_then(decode_nested)
                    .map(|p| Predicate::decode_either(&p))
                    .unwrap_or_default(),
                cell_style: rule.bytes(2).and_then(reference),
                text_style: rule.bytes(3).and_then(reference),
            });
        }
        ConditionalStyles {
            identifier,
            key,
            rules,
        }
    }
}

/// A named cell format the user defined — `TSK.CustomFormatArchive`.
///
/// Custom formats are **document-scoped**, not table-scoped: they live in one
/// `TSK.CustomFormatListArchive` (222) that every table's cells reach into by
/// `custom_uid`.
#[derive(Debug, Clone, Default)]
pub struct CustomFormat {
    pub uid: Uuid,
    pub name: String,
    /// `format_type`, the same code space as a cell's data format.
    pub format_type: u32,
    /// The format string the user typed, where the format has one.
    pub format_string: String,
    /// How many conditional sub-rules the format carries — a custom format may
    /// switch representation on the value's sign or size.
    pub conditions: usize,
}

/// A table's row or column hidden-state record —
/// `TST.HiddenStateExtentArchive`, reached through `TableModelArchive` field 70.
#[derive(Debug, Clone, Default)]
pub struct HiddenStates {
    /// UUIDs of the groups that are collapsed. This is the *only* place a
    /// collapsed category group is recorded.
    pub collapsed_groups: Vec<Uuid>,
    /// Rows or columns the user hid, by index where the UUID resolves.
    pub user_hidden: Vec<usize>,
    /// Rows or columns a filter excluded, as the extent records them. **Not
    /// the same set as the rows whose `hidingState` is 2** — see `FORMAT.md`.
    pub filtered: Vec<usize>,
    /// The filter set this extent is driven by.
    pub filter_set: Option<u64>,
}

impl HiddenStates {
    fn decode(extent: &Message, uids: &UidMap, rows: bool) -> HiddenStates {
        let mut state = HiddenStates {
            collapsed_groups: extent
                .all(7)
                .filter_map(|value| match value {
                    Value::Bytes(raw) => Uuid::read(raw),
                    _ => None,
                })
                .collect(),
            filter_set: extent.bytes(8).and_then(reference),
            ..HiddenStates::default()
        };
        for value in extent.all(2) {
            let Value::Bytes(raw) = value else { continue };
            let Some(entry) = decode_nested(raw) else {
                continue;
            };
            let Some(uid) = entry.bytes(1).and_then(Uuid::read) else {
                continue;
            };
            let Some(index) = (if rows {
                uids.row(uid)
            } else {
                uids.column(uid)
            }) else {
                continue;
            };
            if entry.varint(2).unwrap_or(0) != 0 {
                state.user_hidden.push(index);
            }
            if entry.varint(3).unwrap_or(0) != 0 {
                state.filtered.push(index);
            }
        }
        state.user_hidden.sort_unstable();
        state.filtered.sort_unstable();
        state
    }
}

/// A table, read.
#[derive(Debug, Clone)]
pub struct Table {
    /// `TST.TableInfoArchive` object identifier — the handle callers use.
    pub identifier: u64,
    /// `TST.TableModelArchive` object identifier.
    pub model: u64,
    pub stream: String,
    pub name: String,
    /// Sheet the table sits on, in Numbers. `None` in Pages and Keynote, where
    /// a table is parented by a page or a slide instead.
    pub sheet: Option<String>,
    /// `super.parent` of the table's drawable — the sheet in Numbers, the
    /// containing drawable in Pages and Keynote. The upward half of a
    /// containment that iWork stores in both directions.
    pub parent: Option<u64>,
    /// `table_id`, the uppercase UUID string the document identifies it by.
    pub table_id: String,
    pub rows: usize,
    pub columns: usize,
    pub header_rows: u32,
    pub header_columns: u32,
    pub footer_rows: u32,
    pub header_rows_frozen: bool,
    pub header_columns_frozen: bool,
    /// Counts as the model records them: total hidden, then the user-hidden
    /// and filtered breakdown Numbers keeps separately.
    pub hidden_rows: u32,
    pub hidden_columns: u32,
    pub filtered_rows: u32,
    pub user_hidden_rows: u32,
    pub user_hidden_columns: u32,
    pub default_row_height: f64,
    pub default_column_width: f64,
    pub row_extents: Vec<Extent>,
    pub column_extents: Vec<Extent>,
    pub merges: Vec<Merge>,
    /// UUID ↔ index for this table's rows and columns; everything below is
    /// addressed through it.
    pub uids: UidMap,
    /// `TSCE.HauntedOwnerArchive.owner_uid` — the table's identity for
    /// everything that refers to it from outside, a pivot's source reference
    /// included.
    pub haunted_uid: Uuid,
    /// Sort rules, in the order they are applied.
    pub sort_rules: Vec<SortRule>,
    /// The table's filter, when it has one with rules.
    pub filter: Option<FilterSet>,
    /// Categories. A table has at most one active `GroupByArchive`, but the
    /// field is repeated and a pivot's source carries several.
    pub categories: Vec<Category>,
    /// Set when this table *is* a pivot table, with the rules that build it.
    pub pivot: Option<Pivot>,
    /// Conditional-highlighting rule sets used by this table's cells.
    pub conditional_styles: Vec<ConditionalStyles>,
    /// Row hidden state, including which category groups are collapsed.
    pub row_states: HiddenStates,
    /// Column hidden state.
    pub column_states: HiddenStates,
    cells: Vec<Cell>,
    /// Rows whose storage could not be read, with the reason — so that a
    /// partially decoded table says so instead of reporting empty cells.
    pub problems: Vec<String>,
}

impl Table {
    /// Every non-empty cell, in row-major order.
    pub fn cells(&self) -> &[Cell] {
        &self.cells
    }

    /// One cell by position. Cells with no stored record read as empty.
    pub fn cell(&self, row: usize, column: usize) -> Option<&Cell> {
        self.cells
            .binary_search_by_key(&(row, column), |c| (c.row, c.column))
            .ok()
            .map(|at| &self.cells[at])
    }

    /// The value at a position, empty where nothing is stored.
    pub fn value(&self, row: usize, column: usize) -> CellValue {
        self.cell(row, column)
            .map(|c| c.value.clone())
            .unwrap_or(CellValue::Empty)
    }

    /// Height of a row in points, falling back to the table's default.
    pub fn row_height(&self, row: usize) -> f64 {
        self.row_extents
            .get(row)
            .and_then(|e| e.size)
            .map(f64::from)
            .unwrap_or(self.default_row_height)
    }

    /// Width of a column in points, falling back to the table's default.
    pub fn column_width(&self, column: usize) -> f64 {
        self.column_extents
            .get(column)
            .and_then(|e| e.size)
            .map(f64::from)
            .unwrap_or(self.default_column_width)
    }

    /// The UUID a column is known by outside the cell grid.
    pub fn column_uid(&self, column: usize) -> Option<Uuid> {
        self.uids.column_uid(column)
    }

    /// The UUID a row is known by outside the cell grid.
    pub fn row_uid(&self, row: usize) -> Option<Uuid> {
        self.uids.row_uid(row)
    }

    /// The merge starting at a position, if one does.
    pub fn merge_at(&self, row: usize, column: usize) -> Option<Merge> {
        self.merges
            .iter()
            .copied()
            .find(|m| m.row == row && m.column == column)
    }

    /// The table as rows of text, for CSV and for eyeballing.
    pub fn to_rows(&self) -> Vec<Vec<String>> {
        (0..self.rows)
            .map(|row| {
                (0..self.columns)
                    .map(|column| self.value(row, column).to_text())
                    .collect()
            })
            .collect()
    }
}

/// Seconds since 2001-01-01 as an ISO-8601 timestamp in UTC.
///
/// Written out here rather than pulled in: the only calendar arithmetic this
/// crate needs is one civil-from-days conversion, and a dependency for it would
/// be a dependency in every document that has no dates.
pub fn format_date(seconds: f64) -> String {
    let total = seconds.floor() as i64;
    let days = total.div_euclid(86_400);
    let rest = total.rem_euclid(86_400);
    let (year, month, day) = civil_from_days(days + 11_323); // 2001-01-01 from 1970-01-01
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        rest / 3600,
        (rest % 3600) / 60,
        rest % 60
    )
}

/// Howard Hinnant's civil-from-days, with the era shifted to 0000-03-01.
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

// -- reading a document ------------------------------------------------------

/// Everything hanging off one table's `DataStore`, resolved once.
struct SideTables {
    strings: DataList,
    formats: DataList,
    controls: DataList,
    formulas: DataList,
}

/// Read every table in a document.
pub fn tables(document: &crate::Document) -> Vec<Table> {
    let sheets = sheet_names(document);
    let mut out = Vec::new();
    for (stream, object) in document.objects() {
        if object.message_type() != TYPE_TABLE_INFO {
            continue;
        }
        let Ok(info) = Message::decode(object.payload()) else {
            continue;
        };
        let Some(model_id) = info.bytes(2).and_then(reference) else {
            continue;
        };
        let Some(model) = archive(document, model_id) else {
            continue;
        };
        let parent = info
            .bytes(1)
            .and_then(decode_nested)
            .and_then(|drawable| drawable.bytes(2).and_then(reference));
        if let Some(table) = read_table(
            document,
            object.identifier,
            model_id,
            stream,
            &info,
            &model,
            sheets.get(&object.identifier).cloned(),
            parent,
        ) {
            out.push(table);
        }
    }
    out.sort_by_key(|t| t.identifier);
    resolve_pivot_sources(&mut out);
    out
}

/// Point a pivot's fields at columns of the table it summarises.
///
/// A pivot's `GroupColumnArchive`s name columns of the **source** table, so
/// resolving them against the pivot's own UUID map answers nothing. The join is
/// `PivotOwnerArchive.source_table_uid` against the source's
/// `HauntedOwnerArchive.owner_uid` — and the two are *not equal*: the lower
/// halves differ by a small constant (35 in the fixture), because every owner a
/// table has is a numbered offset from one base UUID. The **upper half is the
/// table's identity** and is what matches; `source_table_name` breaks the tie
/// if two tables ever share one, and is all there is when nothing matches.
///
/// This has to happen after every table is read, which is why it is a second
/// pass rather than part of reading one.
fn resolve_pivot_sources(tables: &mut [Table]) {
    let sources: Vec<(Uuid, String, UidMap)> = tables
        .iter()
        .map(|t| (t.haunted_uid, t.name.clone(), t.uids.clone()))
        .collect();
    for table in tables.iter_mut() {
        let Some(pivot) = &mut table.pivot else {
            continue;
        };
        let mut matches = sources
            .iter()
            .filter(|(uid, _, _)| uid.upper == pivot.source_uid.upper && uid.upper != 0);
        let source = match (matches.next(), matches.next()) {
            (Some(only), None) => Some(only),
            _ => sources
                .iter()
                .find(|(_, name, _)| *name == pivot.source_name),
        };
        let Some((_, _, uids)) = source else { continue };
        for field in pivot.rows.iter_mut().chain(pivot.columns.iter_mut()) {
            field.column = uids.column(field.column_uid);
        }
        for value in &mut pivot.values {
            value.column = uids.column(value.column_uid);
        }
    }
}

/// Sheet name per table, from `TN.SheetArchive` — Numbers only.
///
/// A sheet lists its drawables at field 2 and its name at field 1. Containment
/// is stored twice in Numbers, downward here and upward in each drawable's
/// `parent`; this reads the downward one, which is the one that carries the
/// order the sidebar shows.
fn sheet_names(document: &crate::Document) -> BTreeMap<u64, String> {
    let mut out = BTreeMap::new();
    if document.kind() != crate::Kind::Numbers {
        return out;
    }
    for (_, object) in document.objects() {
        if object.message_type() != 2 {
            continue;
        }
        let Ok(sheet) = Message::decode(object.payload()) else {
            continue;
        };
        let Some(name) = sheet
            .bytes(1)
            .map(|b| String::from_utf8_lossy(b).into_owned())
        else {
            continue;
        };
        for value in sheet.all(2) {
            let Value::Bytes(raw) = value else { continue };
            if let Some(target) = reference(raw) {
                out.insert(target, name.clone());
            }
        }
    }
    out
}

fn archive(document: &crate::Document, identifier: u64) -> Option<Message> {
    let (_, object) = document.object(identifier)?;
    Message::decode(object.payload()).ok()
}

fn data_list(document: &crate::Document, store: &Message, field: u32) -> DataList {
    store
        .bytes(field)
        .and_then(reference)
        .and_then(|id| archive(document, id))
        .map(|a| DataList::decode(&a))
        .unwrap_or_default()
}

#[allow(clippy::too_many_arguments)]
fn read_table(
    document: &crate::Document,
    identifier: u64,
    model_id: u64,
    stream: &str,
    info: &Message,
    model: &Message,
    sheet: Option<String>,
    parent: Option<u64>,
) -> Option<Table> {
    let store = model.bytes(4).and_then(decode_nested)?;
    let rows = model.varint(6).unwrap_or(0) as usize;
    let columns = model.varint(7).unwrap_or(0) as usize;

    let side = SideTables {
        strings: data_list(document, &store, 4),
        formats: data_list(document, &store, 22),
        controls: data_list(document, &store, 21),
        formulas: data_list(document, &store, 6),
    };

    // The UUID index first: nothing below can name a row or a column without
    // it. `base_column_row_uids` is the unsorted map; the *view* map hanging
    // off the table info follows the category label rows and is a different
    // length, so it is not a substitute.
    let uids = model
        .bytes(46)
        .and_then(reference)
        .and_then(|id| archive(document, id))
        .map(|map| UidMap::decode(&map))
        .unwrap_or_default();
    let (column_states, row_states) = hidden_states(model, &uids);

    let mut table = Table {
        identifier,
        model: model_id,
        stream: stream.to_string(),
        name: string_field(model, 8),
        sheet,
        parent,
        table_id: string_field(model, 1),
        rows,
        columns,
        header_rows: model.varint(9).unwrap_or(0) as u32,
        header_columns: model.varint(10).unwrap_or(0) as u32,
        footer_rows: model.varint(11).unwrap_or(0) as u32,
        header_rows_frozen: model.varint(12).unwrap_or(0) != 0,
        header_columns_frozen: model.varint(13).unwrap_or(0) != 0,
        hidden_rows: model.varint(14).unwrap_or(0) as u32,
        hidden_columns: model.varint(15).unwrap_or(0) as u32,
        filtered_rows: model.varint(40).unwrap_or(0) as u32,
        user_hidden_rows: model.varint(41).unwrap_or(0) as u32,
        user_hidden_columns: model.varint(42).unwrap_or(0) as u32,
        default_row_height: double_field(model, 16),
        default_column_width: double_field(model, 17),
        row_extents: row_extents(document, &store, rows),
        column_extents: column_extents(document, &store, columns),
        merges: merges(document, model, &store),
        sort_rules: sort_rules(model),
        filter: filter_set(document, model, &row_states),
        categories: categories(document, model, &uids, &row_states.collapsed_groups),
        pivot: pivot(document, info, model, &uids),
        conditional_styles: conditional_styles(document, &store),
        haunted_uid: model
            .bytes(84)
            .and_then(decode_nested)
            .and_then(|owner| owner.bytes(1).and_then(Uuid::read))
            .unwrap_or_default(),
        uids,
        row_states,
        column_states,
        cells: Vec::new(),
        problems: Vec::new(),
    };

    read_cells(document, &store, &side, &mut table);
    Some(table)
}

/// `TableModelArchive` field 70 — the inline `HiddenStatesOwnerArchive`.
///
/// It holds a list of `HiddenStatesArchive`, each with a column extent (2) and
/// a row extent (3). Every document read here has exactly one.
fn hidden_states(model: &Message, uids: &UidMap) -> (HiddenStates, HiddenStates) {
    let Some(states) = model
        .bytes(70)
        .and_then(decode_nested)
        .and_then(|owner| owner.bytes(2).and_then(decode_nested))
    else {
        return (HiddenStates::default(), HiddenStates::default());
    };
    let extent = |number: u32, rows: bool| {
        states
            .bytes(number)
            .and_then(decode_nested)
            .map(|e| HiddenStates::decode(&e, uids, rows))
            .unwrap_or_default()
    };
    (extent(2, false), extent(3, true))
}

/// `TableModelArchive` field 44 — `TST.TableSortOrderArchive`.
fn sort_rules(model: &Message) -> Vec<SortRule> {
    let Some(order) = model.bytes(44).and_then(decode_nested) else {
        return Vec::new();
    };
    order
        .all(2)
        .filter_map(|value| match value {
            Value::Bytes(raw) => decode_nested(raw),
            _ => None,
        })
        .map(|rule| SortRule {
            column: rule.varint(1).unwrap_or(0) as usize,
            descending: rule.varint(2).unwrap_or(0) != 0,
        })
        .collect()
}

/// The table's filter set.
///
/// Two references reach one: `TableModelArchive` field 38, and the row hidden
/// state extent's own field 8. They point at *different* archives in the same
/// document — the extent's is the one carrying the rules — so both are read and
/// the one that filters anything wins.
fn filter_set(
    document: &crate::Document,
    model: &Message,
    row_states: &HiddenStates,
) -> Option<FilterSet> {
    let mut best: Option<FilterSet> = None;
    let candidates = [model.bytes(38).and_then(reference), row_states.filter_set];
    for identifier in candidates.into_iter().flatten() {
        let Some(set) = archive(document, identifier).map(|a| FilterSet::decode(identifier, &a))
        else {
            continue;
        };
        let better = match &best {
            Some(current) => set.rules.len() > current.rules.len(),
            None => true,
        };
        if better {
            best = Some(set);
        }
    }
    best.filter(|set| !set.rules.is_empty())
}

/// The table's categories.
///
/// 15.3.1 writes them **twice**: inline at `TableModelArchive` field 81, the
/// field the schema marks `category_owner_deprecated`, and again by reference
/// at field 86 through a `CategoryOwnerRefArchive` (6372). The referenced form
/// is preferred; the inline one is the fallback for a document that has only
/// it.
fn categories(
    document: &crate::Document,
    model: &Message,
    uids: &UidMap,
    collapsed: &[Uuid],
) -> Vec<Category> {
    let mut out = Vec::new();
    if let Some(owner) = model
        .bytes(86)
        .and_then(reference)
        .and_then(|id| archive(document, id))
    {
        for value in owner.all(1) {
            let Value::Bytes(raw) = value else { continue };
            let Some(identifier) = reference(raw) else {
                continue;
            };
            let Some(group_by) = archive(document, identifier) else {
                continue;
            };
            out.push(Category::decode(
                document, identifier, &group_by, uids, collapsed,
            ));
        }
    }
    if out.is_empty() {
        if let Some(owner) = model.bytes(81).and_then(decode_nested) {
            for value in owner.all(2) {
                let Value::Bytes(raw) = value else { continue };
                let Some(group_by) = decode_nested(raw) else {
                    continue;
                };
                out.push(Category::decode(document, 0, &group_by, uids, collapsed));
            }
        }
    }
    out.retain(|category| !category.columns.is_empty());
    out
}

/// The pivot rules, when this table is a pivot table.
///
/// `TableInfoArchive.is_a_pivot_table` (16) is the flag; the rules hang off the
/// *model* at field 85. A pivot's source table carries a `PivotOwnerArchive`
/// too — the same object, reached from the other end — so the flag is what
/// decides which of the two tables reports it.
fn pivot(
    document: &crate::Document,
    info: &Message,
    model: &Message,
    uids: &UidMap,
) -> Option<Pivot> {
    if info.varint(16).unwrap_or(0) == 0 {
        return None;
    }
    let identifier = model.bytes(85).and_then(reference)?;
    let owner = archive(document, identifier)?;
    Some(Pivot::decode(identifier, &owner, uids))
}

/// Conditional-highlighting rule sets, from `DataStore.conditionalstyletable`.
fn conditional_styles(document: &crate::Document, store: &Message) -> Vec<ConditionalStyles> {
    let list = data_list(document, store, 18);
    let mut out = Vec::new();
    for (key, entry) in &list.entries {
        let Some(identifier) = entry.reference else {
            continue;
        };
        let Some(set) = archive(document, identifier) else {
            continue;
        };
        out.push(ConditionalStyles::decode(identifier, Some(*key), &set));
    }
    out
}

fn string_field(message: &Message, number: u32) -> String {
    message
        .bytes(number)
        .map(|b| String::from_utf8_lossy(b).into_owned())
        .unwrap_or_default()
}

fn double_field(message: &Message, number: u32) -> f64 {
    match message.get(number) {
        Some(Value::Fixed64(b)) => f64::from_le_bytes(*b),
        _ => 0.0,
    }
}

/// Row heights: `DataStore.rowHeaders` is a `HeaderStorage` — a list of bucket
/// references — while the column side is a single bucket. The asymmetry is
/// Apple's and both halves have to be read their own way.
fn row_extents(document: &crate::Document, store: &Message, rows: usize) -> Vec<Extent> {
    let mut out = vec![Extent::default(); rows];
    let Some(storage) = store.bytes(1).and_then(decode_nested) else {
        return out;
    };
    for value in storage.all(2) {
        let Value::Bytes(raw) = value else { continue };
        let Some(bucket) = reference(raw).and_then(|id| archive(document, id)) else {
            continue;
        };
        read_bucket(&bucket, &mut out);
    }
    out
}

fn column_extents(document: &crate::Document, store: &Message, columns: usize) -> Vec<Extent> {
    let mut out = vec![Extent::default(); columns];
    if let Some(bucket) = store
        .bytes(2)
        .and_then(reference)
        .and_then(|id| archive(document, id))
    {
        read_bucket(&bucket, &mut out);
    }
    out
}

fn read_bucket(bucket: &Message, into: &mut [Extent]) {
    for value in bucket.all(2) {
        let Value::Bytes(raw) = value else { continue };
        let Some(header) = decode_nested(raw) else {
            continue;
        };
        let index = header.varint(1).unwrap_or(0) as usize;
        if index >= into.len() {
            continue;
        }
        into[index] = Extent {
            size: match header.get(2) {
                Some(Value::Fixed32(b)) => Some(f32::from_le_bytes(*b)).filter(|&s| s > 0.0),
                _ => None,
            },
            hiding_state: header.varint(3).unwrap_or(0) as u32,
            cell_count: header.varint(4).unwrap_or(0) as u32,
        };
    }
}

/// Merge ranges.
///
/// A merge is the one thing about a table that is stored nowhere near the cells
/// it covers. Numbers 15.3.1 writes it as a *formula*: the table's
/// `merge_owner` (`TableModelArchive` field 47) owns a formula store, and each
/// formula in it is a single reference naming one merged range. The two
/// documented alternatives — `DataStore.merge_region_map`, and back-dependency
/// ranges on the merge owner's `TSCE.FormulaOwnerDependenciesArchive` — are
/// both absent from every document the apps have written here, so the formula
/// store is read first and the region map only as a fallback.
///
/// The covered cells leave no trace of their own: they have no cell record, not
/// even a `spanCellType` one, and their offsets are the plain `-1` of an empty
/// cell.
fn merges(document: &crate::Document, model: &Message, store: &Message) -> Vec<Merge> {
    let mut out = merges_from_owner(model);
    if out.is_empty() {
        out = merges_from_region_map(document, store);
    }
    out.sort_by_key(|m| (m.row, m.column));
    out
}

/// The merge owner's formula store: one formula per merged range.
///
/// Two node shapes appear. A `COLON_TRACT_NODE` (67) carries the rectangle in
/// `AST_colon_tract`'s absolute column and row ranges, where an absent
/// `range_end` means "the same as `range_begin`". A `CELL_REFERENCE_NODE` (36)
/// carries a single cell in `AST_column`/`AST_row`, **zigzag-encoded** — a
/// merge one cell wide and one cell tall, which Numbers really does write when
/// a merge is whittled down to nothing.
fn merges_from_owner(model: &Message) -> Vec<Merge> {
    let Some(store) = model
        .bytes(47)
        .and_then(decode_nested)
        .and_then(|owner| owner.bytes(2).and_then(decode_nested))
    else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for value in store.all(3) {
        let Value::Bytes(raw) = value else { continue };
        let Some(nodes) = decode_nested(raw)
            .and_then(|pair| pair.bytes(2).and_then(decode_nested))
            .and_then(|formula| formula.bytes(1).and_then(decode_nested))
        else {
            continue;
        };
        for value in nodes.all(1) {
            let Value::Bytes(raw) = value else { continue };
            let Some(node) = decode_nested(raw) else {
                continue;
            };
            match node.varint(1) {
                Some(67) => {
                    let Some(tract) = node.bytes(40).and_then(decode_nested) else {
                        continue;
                    };
                    let (Some(columns), Some(rows)) = (span(&tract, 3), span(&tract, 4)) else {
                        continue;
                    };
                    out.push(Merge {
                        row: rows.0,
                        column: columns.0,
                        rows: rows.1 - rows.0 + 1,
                        columns: columns.1 - columns.0 + 1,
                    });
                }
                Some(36) => {
                    let column = node
                        .bytes(26)
                        .and_then(decode_nested)
                        .and_then(|c| c.varint(1));
                    let row = node
                        .bytes(27)
                        .and_then(decode_nested)
                        .and_then(|r| r.varint(1));
                    let (Some(column), Some(row)) = (column, row) else {
                        continue;
                    };
                    out.push(Merge {
                        row: unzigzag(row),
                        column: unzigzag(column),
                        rows: 1,
                        columns: 1,
                    });
                }
                _ => continue,
            }
            break;
        }
    }
    out
}

/// First `{range_begin, range_end?}` entry of a colon tract's axis.
fn span(tract: &Message, number: u32) -> Option<(usize, usize)> {
    let Value::Bytes(raw) = tract.get(number)? else {
        return None;
    };
    let range = decode_nested(raw)?;
    let begin = range.varint(1)? as usize;
    Some((begin, range.varint(2).map(|e| e as usize).unwrap_or(begin)))
}

/// Protobuf zigzag decoding, which `AST_column` and `AST_row` use and the
/// colon tract's absolute ranges do not.
fn unzigzag(value: u64) -> usize {
    ((value >> 1) ^ (0u64.wrapping_sub(value & 1))) as i64 as usize
}

/// Merge ranges from `DataStore.merge_region_map`, the documented form.
///
/// `TST.CellID.packedData` puts the **column in the high half** and the row in
/// the low half, and `TST.TableSize.packedData` does the same with the column
/// count. Guessing the other way round gives ranges that look reasonable and
/// are transposed. Unverified here: no document this repository can produce
/// writes the region map.
fn merges_from_region_map(document: &crate::Document, store: &Message) -> Vec<Merge> {
    let Some(map) = store
        .bytes(13)
        .and_then(reference)
        .and_then(|id| archive(document, id))
    else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for value in map.all(1) {
        let Value::Bytes(raw) = value else { continue };
        let Some(range) = decode_nested(raw) else {
            continue;
        };
        let Some(origin) = range.bytes(1).and_then(decode_nested) else {
            continue;
        };
        let Some(size) = range.bytes(2).and_then(decode_nested) else {
            continue;
        };
        let (Some(Value::Fixed32(o)), Some(Value::Fixed32(s))) = (origin.get(1), size.get(1))
        else {
            continue;
        };
        let packed_origin = u32::from_le_bytes(*o);
        let packed_size = u32::from_le_bytes(*s);
        out.push(Merge {
            row: (packed_origin & 0xffff) as usize,
            column: (packed_origin >> 16) as usize,
            rows: (packed_size & 0xffff) as usize,
            columns: (packed_size >> 16) as usize,
        });
    }
    out
}

/// Walk every tile and turn its rows into cells.
///
/// The absolute row of a `TileRowInfo` is `tileid * tile_size + tile_row_index`.
/// The alternative — counting `TileRowInfo`s against the row-header buckets —
/// only works while the two stay in lockstep, and nothing enforces that.
fn read_cells(document: &crate::Document, store: &Message, side: &SideTables, table: &mut Table) {
    let Some(tiles) = store.bytes(3).and_then(decode_nested) else {
        return;
    };
    let tile_size = tiles.varint(2).unwrap_or(256) as usize;
    for value in tiles.all(1) {
        let Value::Bytes(raw) = value else { continue };
        let Some(entry) = decode_nested(raw) else {
            continue;
        };
        let tile_id = entry.varint(1).unwrap_or(0) as usize;
        let Some(tile) = entry
            .bytes(2)
            .and_then(reference)
            .and_then(|id| archive(document, id))
        else {
            continue;
        };
        let tile_wide = tile.varint(8).unwrap_or(0) != 0;
        for value in tile.all(5) {
            let Value::Bytes(raw) = value else { continue };
            let Some(info) = decode_nested(raw) else {
                continue;
            };
            let row = tile_id * tile_size + info.varint(1).unwrap_or(0) as usize;
            let (Some(buffer), Some(offsets)) = (info.bytes(6), info.bytes(7)) else {
                // Fields 3 and 4 are the pre-BNC pair. They are `required`, so
                // they are always present and always meaningless in a modern
                // document; a row that has only those is storage this crate
                // does not decode, not an empty row.
                table
                    .problems
                    .push(format!("row {row}: no version-5 cell storage"));
                continue;
            };
            let wide = tile_wide || info.varint(8).unwrap_or(0) != 0;
            for slot in row_cells(buffer, offsets, wide) {
                let Some((column, bytes)) = slot else {
                    continue;
                };
                if column >= table.columns {
                    continue;
                }
                match decode_cell(bytes) {
                    Ok(record) => table.cells.push(resolve(row, column, record, side)),
                    Err(reason) => table
                        .problems
                        .push(format!("row {row} column {column}: {reason}",)),
                }
            }
        }
    }
    table.cells.sort_by_key(|c| (c.row, c.column));
}

/// Turn a decoded record into a cell, resolving its keys against the side
/// tables.
fn resolve(row: usize, column: usize, record: CellRecord, side: &SideTables) -> Cell {
    let value = match record.cell_type {
        cell_type::EMPTY => CellValue::Empty,
        cell_type::SPAN => CellValue::Span,
        cell_type::NUMBER => record
            .decimal
            .map(CellValue::Number)
            .unwrap_or(CellValue::Empty),
        cell_type::CURRENCY => record
            .decimal
            .map(CellValue::Currency)
            .unwrap_or(CellValue::Empty),
        cell_type::TEXT => CellValue::Text(
            record
                .string_id
                .and_then(|key| side.strings.string(key))
                .unwrap_or_default()
                .to_string(),
        ),
        cell_type::BOOL => CellValue::Bool(record.double.unwrap_or(0.0) > 0.0),
        cell_type::DURATION => CellValue::Duration(record.double.unwrap_or(0.0)),
        cell_type::DATE => CellValue::Date(record.seconds.unwrap_or(0.0)),
        cell_type::ERROR => CellValue::Error,
        cell_type::RICH_TEXT => CellValue::RichText(String::new()),
        other => CellValue::Unknown(other),
    };

    let format_type = record
        .applicable_format()
        .and_then(|slot| record.format_id_in(slot))
        .and_then(|key| side.formats.entries.get(&key))
        .and_then(|entry| entry.format.as_ref())
        .and_then(|f| f.varint(1))
        .map(|code| code as u32);

    let control = record
        .control_id
        .and_then(|key| side.controls.entries.get(&key))
        .and_then(|entry| entry.cell_spec.as_ref())
        .map(|spec| CellControl::from_code(spec.varint(1).unwrap_or(0) as u32));

    let format = CellFormat::of(&record, control, format_type);

    let has_formula = record
        .formula_id
        .is_some_and(|key| side.formulas.entries.contains_key(&key));

    Cell {
        row,
        column,
        value,
        format,
        control,
        has_formula,
        record,
    }
}

/// `TSK.CustomFormatListArchive` — every named cell format in the document.
pub const TYPE_CUSTOM_FORMAT_LIST: u32 = 222;

/// Every custom cell format the document defines.
///
/// The list is document-scoped and there is exactly one archive per document,
/// empty in most of them. Its two repeated fields are parallel: `uuids` (1) and
/// `custom_formats` (2), and a cell reaches a format by the UUID rather than by
/// position.
pub fn custom_formats(document: &crate::Document) -> Vec<CustomFormat> {
    let mut out = Vec::new();
    for (_, object) in document.objects() {
        if object.message_type() != TYPE_CUSTOM_FORMAT_LIST {
            continue;
        }
        let Ok(list) = Message::decode(object.payload()) else {
            continue;
        };
        let uids: Vec<Uuid> = list
            .all(1)
            .filter_map(|value| match value {
                Value::Bytes(raw) => Uuid::read(raw),
                _ => None,
            })
            .collect();
        for (at, value) in list.all(2).enumerate() {
            let Value::Bytes(raw) = value else { continue };
            let Some(format) = decode_nested(raw) else {
                continue;
            };
            let default = format.bytes(3).and_then(decode_nested);
            out.push(CustomFormat {
                uid: uids.get(at).copied().unwrap_or_default(),
                name: string_field(&format, 1),
                format_type: format.varint(5).or_else(|| format.varint(2)).unwrap_or(0) as u32,
                format_string: default
                    .as_ref()
                    .map(|f| string_field(f, 18))
                    .unwrap_or_default(),
                conditions: format.all(4).count(),
            });
        }
    }
    out
}

/// Fill in the text of the rich-text cells of a table, which lives in
/// `TSWP.StorageArchive`s outside the table's own storage.
pub(crate) fn resolve_rich_text(document: &crate::Document, table: &mut Table) {
    let Some(model) = archive(document, table.model) else {
        return;
    };
    let Some(store) = model.bytes(4).and_then(decode_nested) else {
        return;
    };
    let payloads = data_list(document, &store, 17);
    for cell in &mut table.cells {
        if !matches!(cell.value, CellValue::RichText(_)) {
            continue;
        }
        let text = cell
            .record
            .rich_id
            .and_then(|key| payloads.entries.get(&key))
            .and_then(|entry| entry.rich_text)
            .and_then(|id| archive(document, id))
            .and_then(|payload| payload.bytes(1).and_then(reference))
            .and_then(|id| archive(document, id))
            .map(|storage| crate::text::read(&storage))
            .unwrap_or_default();
        cell.value = CellValue::RichText(text);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The empty cell: twelve bytes, version 5, no flags. This is the byte
    /// string a fresh table is made of.
    #[test]
    fn an_empty_record_is_twelve_bytes_of_header() {
        let record = decode_cell(&[5, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]).unwrap();
        assert_eq!(record.cell_type, cell_type::EMPTY);
        assert_eq!(record.flags, 0);
        assert_eq!(record.trailing(), 0);
        assert_eq!(record.explicit_format(), None);
    }

    fn record(cell_type: u8, extras: u16, flags: u32, payload: &[u8]) -> Vec<u8> {
        let mut bytes = vec![5, cell_type, 0, 0, 0, 0];
        bytes.extend_from_slice(&extras.to_le_bytes());
        bytes.extend_from_slice(&flags.to_le_bytes());
        bytes.extend_from_slice(payload);
        bytes
    }

    #[test]
    fn a_text_cell_carries_a_key_into_the_string_table() {
        let bytes = record(cell_type::TEXT, 0x0000, 0x0000_0008, &7u32.to_le_bytes());
        let cell = decode_cell(&bytes).unwrap();
        assert_eq!(cell.string_id, Some(7));
        assert_eq!(cell.trailing(), 0);
    }

    /// **The two bit orders.** Payloads are consumed in ascending *flag-word*
    /// bit order — date (`0x8000`) before duration (`0x10000`) — while byte 6
    /// orders the same two the other way round: duration `0x04`, date `0x08`.
    /// A decoder that took its cue from byte 6 would hand back the two keys
    /// swapped, and every length would still add up, so nothing would complain.
    #[test]
    fn format_keys_are_consumed_in_flag_word_order_not_byte_six_order() {
        let mut payload = Vec::new();
        payload.extend_from_slice(&11u32.to_le_bytes()); // 0x2000  number format
        payload.extend_from_slice(&22u32.to_le_bytes()); // 0x8000  date format
        payload.extend_from_slice(&33u32.to_le_bytes()); // 0x10000 duration format
        let bytes = record(
            cell_type::DATE,
            0x0008, // byte 6: the date format is the one that was chosen
            0x0000_2000 | 0x0000_8000 | 0x0001_0000,
            &payload,
        );
        let cell = decode_cell(&bytes).unwrap();
        assert_eq!(cell.number_format_id, Some(11));
        assert_eq!(cell.date_format_id, Some(22));
        assert_eq!(cell.duration_format_id, Some(33));
        assert_eq!(cell.explicit_format(), Some(FormatSlot::Date));
        assert_eq!(cell.format_id_in(FormatSlot::Date), Some(22));
    }

    /// Byte 6 is not a copy of the flag word's presence bits: a cell carries a
    /// format key in a slot without that byte saying anything, and then the app
    /// calls the cell automatic. This is the difference between a number cell
    /// and a cell the user made a number, and there is nothing else to tell
    /// them apart.
    #[test]
    fn byte_six_says_which_format_was_chosen_not_which_is_present() {
        let inherited = decode_cell(&record(
            cell_type::NUMBER,
            0x0000,
            0x0000_2000,
            &2u32.to_le_bytes(),
        ))
        .unwrap();
        let chosen = decode_cell(&record(
            cell_type::NUMBER,
            0x0001,
            0x0000_2000,
            &2u32.to_le_bytes(),
        ))
        .unwrap();
        assert_eq!(inherited.number_format_id, chosen.number_format_id);
        assert_eq!(inherited.explicit_format(), None);
        assert_eq!(chosen.explicit_format(), Some(FormatSlot::Number));
        assert_eq!(
            CellFormat::of(&inherited, None, Some(256)),
            CellFormat::Automatic
        );
        assert_eq!(CellFormat::of(&chosen, None, Some(256)), CellFormat::Number);
    }

    /// A format that is not the cell's current one is inert. A column formatted
    /// as currency gives its text header a currency key and sets the bit; the
    /// header's `format_kind` still says text, and Numbers draws plain text and
    /// calls the format automatic.
    #[test]
    fn a_format_that_is_not_the_cells_own_is_ignored() {
        let header = decode_cell(&record(
            cell_type::TEXT,
            0x0002, // currency, chosen
            0x0000_0008 | 0x0000_1000 | 0x0000_4000 | 0x0002_0000,
            &[
                1, 0, 0, 0, // string
                5, 0, 0, 0, // format kind: text
                3, 0, 0, 0, // currency format
                1, 0, 0, 0, // text format
            ],
        ))
        .unwrap();
        assert_eq!(header.explicit_format(), Some(FormatSlot::Currency));
        assert_eq!(header.current_format(), Some(FormatSlot::Text));
        assert_eq!(header.applicable_format(), None);
        assert_eq!(
            CellFormat::of(&header, None, Some(257)),
            CellFormat::Automatic
        );
    }

    /// `format_kind` numbers the six format slots in flag-word order, which is
    /// a second, independent statement of that order — the same order the
    /// payloads are consumed in, and not byte 6's.
    #[test]
    fn format_kind_numbers_the_slots_in_flag_word_order() {
        for (kind, slot, flag) in [
            (1, FormatSlot::Number, 0x0000_2000u32),
            (2, FormatSlot::Currency, 0x0000_4000),
            (3, FormatSlot::Date, 0x0000_8000),
            (4, FormatSlot::Duration, 0x0001_0000),
            (5, FormatSlot::Text, 0x0002_0000),
            (6, FormatSlot::Boolean, 0x0004_0000),
        ] {
            let mut payload = (kind as u32).to_le_bytes().to_vec();
            payload.extend_from_slice(&9u32.to_le_bytes());
            let cell =
                decode_cell(&record(cell_type::NUMBER, 0, 0x0000_1000 | flag, &payload)).unwrap();
            assert_eq!(cell.current_format(), Some(slot));
            assert_eq!(cell.format_id_in(slot), Some(9), "slot {kind}");
        }
    }

    /// A control wins over the format the cell also carries: a slider cell is
    /// stored as a plain number with `format_type` 256 beside it.
    #[test]
    fn a_control_decides_the_format_it_sits_on() {
        let slider = decode_cell(&record(
            cell_type::NUMBER,
            0x0001,
            0x0000_0400 | 0x0000_2000,
            &[3, 0, 0, 0, 2, 0, 0, 0],
        ))
        .unwrap();
        assert_eq!(slider.control_id, Some(3));
        assert_eq!(slider.number_format_id, Some(2));
        assert_eq!(
            CellFormat::of(&slider, Some(CellControl::Slider), Some(256)),
            CellFormat::Slider
        );
    }

    /// Bits nothing here reads still have to advance the cursor, or the fields
    /// after them are read from the wrong offsets.
    #[test]
    fn skipped_bits_still_consume_their_payload() {
        let mut payload = Vec::new();
        payload.extend_from_slice(&1u32.to_le_bytes()); // 0x80  conditional style
        payload.extend_from_slice(&2u32.to_le_bytes()); // 0x100 conditional rule
        payload.extend_from_slice(&99u32.to_le_bytes()); // 0x200 formula
        let bytes = record(
            cell_type::NUMBER,
            0,
            0x0000_0080 | 0x0000_0100 | 0x0000_0200,
            &payload,
        );
        let cell = decode_cell(&bytes).unwrap();
        assert_eq!(cell.formula_id, Some(99));
        assert_eq!(cell.trailing(), 0);
    }

    #[test]
    fn a_short_record_is_an_error_rather_than_a_guess() {
        let bytes = record(cell_type::TEXT, 0x0080, 0x0000_0008, &[1, 2]);
        assert!(decode_cell(&bytes).is_err());
        assert!(decode_cell(&[5, 0, 0]).is_err());
    }

    #[test]
    fn storage_versions_other_than_five_are_refused() {
        let mut bytes = record(cell_type::TEXT, 0, 0, &[]);
        bytes[0] = 4;
        assert!(decode_cell(&bytes).is_err());
    }

    /// The exponent is biased by 0x1820 and split across two bytes; the
    /// mantissa is little-endian with its top bit in byte 14. These are the
    /// numbers a document actually contains.
    #[test]
    fn decimal128_decodes_apples_layout() {
        // 42 = 42 × 10^0.
        let mut bytes = [0u8; 16];
        bytes[0] = 42;
        let exponent = DECIMAL128_BIAS;
        bytes[15] = (exponent >> 7) as u8;
        bytes[14] = ((exponent & 0x7f) << 1) as u8;
        let value = decode_decimal128(&bytes);
        assert_eq!(value.mantissa, 42);
        assert_eq!(value.exponent, 0);
        assert_eq!(value.to_f64(), 42.0);
        assert_eq!(value.to_string(), "42");

        // -1.5 = -15 × 10^-1.
        let mut bytes = [0u8; 16];
        bytes[0] = 15;
        let exponent = DECIMAL128_BIAS - 1;
        bytes[15] = 0x80 | (exponent >> 7) as u8;
        bytes[14] = ((exponent & 0x7f) << 1) as u8;
        let value = decode_decimal128(&bytes);
        assert_eq!(value.to_f64(), -1.5);
        assert_eq!(value.to_string(), "-1.5");
    }

    /// A mantissa bit lives in byte 14 alongside the exponent's low bits.
    #[test]
    fn the_top_mantissa_bit_shares_a_byte_with_the_exponent() {
        let mut bytes = [0u8; 16];
        let exponent = DECIMAL128_BIAS;
        bytes[15] = (exponent >> 7) as u8;
        bytes[14] = (((exponent & 0x7f) << 1) | 1) as u8;
        let value = decode_decimal128(&bytes);
        assert_eq!(value.mantissa, 1i128 << 112);
        assert_eq!(value.exponent, 0);
    }

    #[test]
    fn decimal_prints_exactly_where_a_double_would_not() {
        let tenth = Decimal {
            mantissa: 1,
            exponent: -1,
        };
        assert_eq!(tenth.to_string(), "0.1");
        assert_eq!(
            Decimal {
                mantissa: 110,
                exponent: -2
            }
            .to_string(),
            "1.1"
        );
        assert_eq!(
            Decimal {
                mantissa: 0,
                exponent: -16
            }
            .to_string(),
            "0"
        );
    }

    /// `-1` is an empty column and a record runs to the next *non-negative*
    /// offset, not the next one. A row with a gap proves the difference.
    #[test]
    fn offsets_skip_gaps_to_find_the_end_of_a_record() {
        let buffer: Vec<u8> = (0..24u8).collect();
        let mut offsets = Vec::new();
        for entry in [0i16, -1, 12, -1] {
            offsets.extend_from_slice(&entry.to_le_bytes());
        }
        let cells = row_cells(&buffer, &offsets, false);
        assert_eq!(cells.len(), 4);
        assert_eq!(cells[0].unwrap().1.len(), 12);
        assert!(cells[1].is_none());
        assert_eq!(cells[2].unwrap().1.len(), 12);
        assert!(cells[3].is_none());
    }

    #[test]
    fn wide_offsets_count_in_groups_of_four_bytes() {
        let buffer: Vec<u8> = (0..32u8).collect();
        let mut offsets = Vec::new();
        for entry in [0i16, 3] {
            offsets.extend_from_slice(&entry.to_le_bytes());
        }
        let cells = row_cells(&buffer, &offsets, true);
        assert_eq!(cells[0].unwrap().1.len(), 12);
        assert_eq!(cells[1].unwrap().1, &buffer[12..]);
    }

    #[test]
    fn dates_count_from_2001() {
        assert_eq!(format_date(0.0), "2001-01-01T00:00:00Z");
        assert_eq!(format_date(86_400.0), "2001-01-02T00:00:00Z");
        // 2024-02-29T12:34:56Z, a leap day, counted from the Apple epoch.
        let seconds = 730_000_000.0 + 39_296.0;
        assert_eq!(&format_date(seconds)[..4], "2024");
    }
}
