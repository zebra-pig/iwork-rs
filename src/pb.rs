//! Minimal schema-less protobuf wire codec.
//!
//! iWork's `.proto` files are not published, so everything here works at the
//! wire level: fields are kept as (number, wire value) pairs and re-encoded in
//! the order they were read. That is enough to inspect and rewrite documents
//! without knowing the message definitions.

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Varint(u64),
    Fixed64([u8; 8]),
    Bytes(Vec<u8>),
    Fixed32([u8; 4]),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Field {
    pub number: u32,
    pub value: Value,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Message {
    pub fields: Vec<Field>,
}

pub struct Reader<'a> {
    pub buf: &'a [u8],
    pub pos: usize,
}

impl<'a> Reader<'a> {
    pub fn new(buf: &'a [u8]) -> Self {
        Reader { buf, pos: 0 }
    }

    pub fn done(&self) -> bool {
        self.pos >= self.buf.len()
    }

    pub fn varint(&mut self) -> Result<u64, String> {
        let mut value = 0u64;
        let mut shift = 0;
        loop {
            let byte = *self
                .buf
                .get(self.pos)
                .ok_or_else(|| "truncated varint".to_string())?;
            self.pos += 1;
            value |= u64::from(byte & 0x7f) << shift;
            if byte & 0x80 == 0 {
                return Ok(value);
            }
            shift += 7;
            if shift > 63 {
                return Err("varint too long".into());
            }
        }
    }

    pub fn take(&mut self, n: usize) -> Result<&'a [u8], String> {
        let end = self.pos.checked_add(n).ok_or("length overflow")?;
        if end > self.buf.len() {
            return Err("truncated field".into());
        }
        let slice = &self.buf[self.pos..end];
        self.pos = end;
        Ok(slice)
    }

    pub fn field(&mut self) -> Result<Field, String> {
        let key = self.varint()?;
        let number = (key >> 3) as u32;
        let value = match key & 7 {
            0 => Value::Varint(self.varint()?),
            1 => Value::Fixed64(self.take(8)?.try_into().unwrap()),
            2 => {
                let len = self.varint()? as usize;
                Value::Bytes(self.take(len)?.to_vec())
            }
            5 => Value::Fixed32(self.take(4)?.try_into().unwrap()),
            other => return Err(format!("unsupported wire type {other}")),
        };
        Ok(Field { number, value })
    }
}

impl Message {
    pub fn decode(buf: &[u8]) -> Result<Message, String> {
        let mut reader = Reader::new(buf);
        let mut fields = Vec::new();
        while !reader.done() {
            fields.push(reader.field()?);
        }
        Ok(Message { fields })
    }

    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::new();
        for field in &self.fields {
            let wire = match &field.value {
                Value::Varint(_) => 0,
                Value::Fixed64(_) => 1,
                Value::Bytes(_) => 2,
                Value::Fixed32(_) => 5,
            };
            write_varint(&mut out, (u64::from(field.number) << 3) | wire);
            match &field.value {
                Value::Varint(v) => write_varint(&mut out, *v),
                Value::Fixed64(b) => out.extend_from_slice(b),
                Value::Fixed32(b) => out.extend_from_slice(b),
                Value::Bytes(b) => {
                    write_varint(&mut out, b.len() as u64);
                    out.extend_from_slice(b);
                }
            }
        }
        out
    }

    pub fn get(&self, number: u32) -> Option<&Value> {
        self.fields
            .iter()
            .find(|f| f.number == number)
            .map(|f| &f.value)
    }

    pub fn all(&self, number: u32) -> impl Iterator<Item = &Value> {
        self.fields
            .iter()
            .filter(move |f| f.number == number)
            .map(|f| &f.value)
    }

    pub fn varint(&self, number: u32) -> Option<u64> {
        match self.get(number) {
            Some(Value::Varint(v)) => Some(*v),
            _ => None,
        }
    }

    pub fn bytes(&self, number: u32) -> Option<&[u8]> {
        match self.get(number) {
            Some(Value::Bytes(b)) => Some(b),
            _ => None,
        }
    }

    /// Replace the first occurrence of `number`, or append it if absent.
    pub fn set(&mut self, number: u32, value: Value) {
        match self.fields.iter_mut().find(|f| f.number == number) {
            Some(field) => field.value = value,
            None => self.fields.push(Field { number, value }),
        }
    }
}

pub fn write_varint(out: &mut Vec<u8>, mut value: u64) {
    loop {
        let byte = (value & 0x7f) as u8;
        value >>= 7;
        if value == 0 {
            out.push(byte);
            return;
        }
        out.push(byte | 0x80);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn varints_roundtrip() {
        for value in [0u64, 1, 127, 128, 300, 65535, 129448, u64::MAX] {
            let mut buf = Vec::new();
            write_varint(&mut buf, value);
            assert_eq!(Reader::new(&buf).varint().unwrap(), value);
        }
    }

    #[test]
    fn message_roundtrips_every_wire_type() {
        let mut message = Message::default();
        message.fields.push(Field {
            number: 1,
            value: Value::Varint(129448),
        });
        message.fields.push(Field {
            number: 2,
            value: Value::Bytes(b"Document".to_vec()),
        });
        message.fields.push(Field {
            number: 30,
            value: Value::Fixed32(595.28f32.to_le_bytes()),
        });
        message.fields.push(Field {
            number: 31,
            value: Value::Fixed64(841.89f64.to_le_bytes()),
        });

        let decoded = Message::decode(&message.encode()).unwrap();
        assert_eq!(decoded, message);
        assert_eq!(decoded.varint(1), Some(129448));
        assert_eq!(decoded.bytes(2), Some(b"Document".as_slice()));
    }

    /// Repeated fields must survive in order — attribute-run tables are
    /// repeated fields and their order is their meaning.
    #[test]
    fn repeated_fields_keep_their_order() {
        let mut message = Message::default();
        for index in [0u64, 6, 57, 58] {
            message.fields.push(Field {
                number: 1,
                value: Value::Varint(index),
            });
        }
        let decoded = Message::decode(&message.encode()).unwrap();
        let values: Vec<_> = decoded
            .all(1)
            .map(|v| match v {
                Value::Varint(n) => *n,
                _ => unreachable!(),
            })
            .collect();
        assert_eq!(values, vec![0, 6, 57, 58]);
    }

    #[test]
    fn set_replaces_in_place_and_appends_when_absent() {
        let mut message = Message::default();
        message.set(3, Value::Bytes(b"old".to_vec()));
        message.set(5, Value::Varint(1));
        message.set(3, Value::Bytes(b"new".to_vec()));
        assert_eq!(message.fields.len(), 2);
        assert_eq!(message.bytes(3), Some(b"new".as_slice()));
        assert_eq!(message.fields[0].number, 3, "field order is preserved");
    }

    #[test]
    fn truncated_input_is_an_error() {
        assert!(Message::decode(&[0x0a, 0x05, b'a']).is_err());
        assert!(Message::decode(&[0x80]).is_err());
    }

    /// Group wire types (3 and 4) never appear in iWork archives; rejecting
    /// them keeps a desynced parse from silently producing garbage.
    #[test]
    fn group_wire_types_are_rejected() {
        assert!(Message::decode(&[0x0b]).is_err());
    }
}
