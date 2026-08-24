//! A lean, schema-directed Kiwi decoder producing a minimal value tree.
//!
//! Compared to the generic `kiwi_schema::Value` decoder this:
//!   * stores byte arrays as one contiguous borrowed slice instead of one
//!     boxed value per byte,
//!   * represents objects as small ordered field vectors instead of hash maps,
//!   * skips (without allocating) every field whose name is not in the
//!     keep-set used by the document builder.
//!
//! Wire format reference: the `kiwi-schema` crate sources and evanw/kiwi.

use crate::error::ParseError;
use kiwi_schema::{
    DefKind, Schema, TYPE_BOOL, TYPE_BYTE, TYPE_FLOAT, TYPE_INT, TYPE_INT64, TYPE_STRING,
    TYPE_UINT, TYPE_UINT64,
};
use std::collections::HashSet;
use std::sync::OnceLock;

/// Field names the document builder actually consumes; everything else is
/// skipped during decoding rather than materialized.
const KEEP_FIELDS: &[&str] = &[
    // Root message
    "nodeChanges",
    "blobs",
    "bytes",
    // Node identity / hierarchy
    "guid",
    "sessionID",
    "localID",
    "parentIndex",
    "position",
    "type",
    "phase",
    "name",
    "visible",
    "opacity",
    "locked",
    // Shape basics
    "size",
    "x",
    "y",
    "transform",
    "m00",
    "m01",
    "m02",
    "m10",
    "m11",
    "m12",
    "cornerRadius",
    "rectangleTopLeftCornerRadius",
    "rectangleTopRightCornerRadius",
    "rectangleBottomRightCornerRadius",
    "rectangleBottomLeftCornerRadius",
    "resizeToFit",
    "frameMaskDisabled",
    "blendMode",
    // Strokes
    "strokeWeight",
    "strokeAlign",
    // Paints
    "fillPaints",
    "backgroundPaints",
    "strokePaints",
    "stops",
    "color",
    "r",
    "g",
    "b",
    "a",
    "image",
    "hash",
    // Effects
    "effects",
    "offset",
    "radius",
    "spread",
    // Baked geometry
    "fillGeometry",
    "strokeGeometry",
    "windingRule",
    "commandsBlob",
    "styleID",
    "vectorData",
    "vectorNetworkBlob",
    "normalizedSize",
    // Text
    "textData",
    "characters",
    "fontFamily",
    "fontWeight",
    "fontSize",
    "letterSpacing",
    "lineHeight",
    "fills",
];

fn keep_set() -> &'static HashSet<&'static str> {
    static SET: OnceLock<HashSet<&'static str>> = OnceLock::new();
    SET.get_or_init(|| KEEP_FIELDS.iter().copied().collect())
}

/// Minimal byte cursor mirroring `kiwi_schema`'s wire encodings.
struct ByteBuf<'m> {
    data: &'m [u8],
    index: usize,
}

impl<'m> ByteBuf<'m> {
    fn new(data: &'m [u8]) -> Self {
        Self { data, index: 0 }
    }

    fn read_byte(&mut self) -> Result<u8, ParseError> {
        self.data
            .get(self.index)
            .map(|b| {
                self.index += 1;
                *b
            })
            .ok_or_else(|| ParseError::SchemaDecode("unexpected end of kiwi message".into()))
    }

    fn read_bool(&mut self) -> Result<bool, ParseError> {
        match self.read_byte()? {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(ParseError::SchemaDecode("invalid bool byte".into())),
        }
    }

    fn read_var_uint(&mut self) -> Result<u32, ParseError> {
        let mut shift: u8 = 0;
        let mut result: u32 = 0;
        loop {
            let byte = self.read_byte()?;
            result |= ((byte & 127) as u32) << shift;
            shift += 7;
            if (byte & 128) == 0 || shift >= 35 {
                return Ok(result);
            }
        }
    }

    fn read_var_uint64(&mut self) -> Result<u64, ParseError> {
        let mut shift: u8 = 0;
        let mut result: u64 = 0;
        loop {
            let byte = self.read_byte()?;
            result |= ((byte & 127) as u64) << shift;
            shift += 7;
            if (byte & 128) == 0 || shift >= 70 {
                return Ok(result);
            }
        }
    }

    fn read_var_int(&mut self) -> Result<i32, ParseError> {
        let v = self.read_var_uint()?;
        Ok(if v & 1 != 0 { !(v >> 1) } else { v >> 1 } as i32)
    }

    fn read_var_int64(&mut self) -> Result<i64, ParseError> {
        let v = self.read_var_uint64()?;
        Ok(if v & 1 != 0 { !(v >> 1) } else { v >> 1 } as i64)
    }

    /// Var-float: a zero byte means 0.0; otherwise low 23 bits are mantissa
    /// and the exponent is rotated back into place.
    fn read_var_float(&mut self) -> Result<f32, ParseError> {
        let first = self.read_byte()?;
        if first == 0 {
            return Ok(0.0);
        }
        if self.index + 3 > self.data.len() {
            return Err(ParseError::SchemaDecode("truncated float".into()));
        }
        let mut bits: u32 = (first as u32)
            | ((self.data[self.index] as u32) << 8)
            | ((self.data[self.index + 1] as u32) << 16)
            | ((self.data[self.index + 2] as u32) << 24);
        self.index += 3;
        bits = bits.rotate_left(23);
        Ok(f32::from_bits(bits))
    }

    /// Null-terminated UTF-8 string.
    fn read_string(&mut self) -> Result<String, ParseError> {
        let start = self.index;
        while self.index < self.data.len() {
            if self.data[self.index] == 0 {
                let s = String::from_utf8_lossy(&self.data[start..self.index]).into_owned();
                self.index += 1;
                return Ok(s);
            }
            self.index += 1;
        }
        Err(ParseError::SchemaDecode("unterminated string".into()))
    }

    fn take_bytes(&mut self, len: usize) -> Result<&'m [u8], ParseError> {
        if self.index + len > self.data.len() {
            return Err(ParseError::SchemaDecode("truncated byte array".into()));
        }
        let out = &self.data[self.index..self.index + len];
        self.index += len;
        Ok(out)
    }
}

/// A decoded kiwi value, keeping only what the parser needs.
pub enum FastVal<'s, 'm> {
    Bool(bool),
    Int(i32),
    UInt(u32),
    Float(f32),
    Int64(i64),
    UInt64(u64),
    Str(String),
    /// Enum variant name, borrowed from the schema.
    Enum(&'s str),
    /// Contiguous byte array (e.g. geometry blobs), borrowed from the message.
    Bytes(&'m [u8]),
    Arr(Vec<FastVal<'s, 'm>>),
    /// Ordered (field name, value) pairs; names borrow from the schema.
    Obj(Vec<(&'s str, FastVal<'s, 'm>)>),
}

impl<'s, 'm> FastVal<'s, 'm> {
    pub fn get(&self, name: &str) -> Option<&FastVal<'s, 'm>> {
        match self {
            FastVal::Obj(fields) => fields.iter().find(|(k, _)| *k == name).map(|(_, v)| v),
            _ => None,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            FastVal::Str(s) => Some(s.as_str()),
            FastVal::Enum(v) => Some(v),
            _ => None,
        }
    }

    pub fn as_f64(&self) -> Option<f64> {
        match self {
            FastVal::Int(i) => Some(*i as f64),
            FastVal::UInt(u) => Some(*u as f64),
            FastVal::Float(f) => Some(*f as f64),
            FastVal::Int64(i) => Some(*i as f64),
            FastVal::UInt64(u) => Some(*u as f64),
            _ => None,
        }
    }

    pub fn as_u64(&self) -> Option<u64> {
        match self {
            FastVal::UInt(u) => Some(*u as u64),
            FastVal::Int(i) => (*i >= 0).then_some(*i as u64),
            FastVal::Int64(i) => (*i >= 0).then_some(*i as u64),
            FastVal::UInt64(u) => Some(*u),
            _ => None,
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            FastVal::Bool(b) => Some(*b),
            _ => None,
        }
    }

    pub fn as_array(&self) -> Option<&Vec<FastVal<'s, 'm>>> {
        match self {
            FastVal::Arr(a) => Some(a),
            _ => None,
        }
    }

    pub fn as_bytes(&self) -> Option<&'m [u8]> {
        match self {
            FastVal::Bytes(b) => Some(b),
            _ => None,
        }
    }
}

fn read_primitive<'s, 'm>(
    type_id: i32,
    bb: &mut ByteBuf<'m>,
) -> Result<FastVal<'s, 'm>, ParseError> {
    Ok(match type_id {
        TYPE_BOOL => FastVal::Bool(bb.read_bool()?),
        TYPE_BYTE => FastVal::Int(bb.read_byte()? as i32),
        TYPE_INT => FastVal::Int(bb.read_var_int()?),
        TYPE_UINT => FastVal::UInt(bb.read_var_uint()?),
        TYPE_FLOAT => FastVal::Float(bb.read_var_float()?),
        TYPE_STRING => FastVal::Str(bb.read_string()?),
        TYPE_INT64 => FastVal::Int64(bb.read_var_int64()?),
        TYPE_UINT64 => FastVal::UInt64(bb.read_var_uint64()?),
        other => {
            return Err(ParseError::SchemaDecode(format!(
                "read_primitive called with non-primitive id {}",
                other
            )))
        }
    })
}

fn skip_primitive(type_id: i32, is_array: bool, bb: &mut ByteBuf) -> Result<(), ParseError> {
    if !is_array {
        match type_id {
            TYPE_BOOL | TYPE_BYTE => bb.read_byte().map(|_| ())?,
            TYPE_INT | TYPE_UINT => bb.read_var_uint().map(|_| ())?,
            TYPE_FLOAT => bb.read_var_float().map(|_| ())?,
            TYPE_STRING => bb.read_string().map(|_| ())?,
            TYPE_INT64 | TYPE_UINT64 => bb.read_var_uint64().map(|_| ())?,
            other => {
                return Err(ParseError::SchemaDecode(format!(
                    "skip_primitive: bad id {}",
                    other
                )))
            }
        }
        return Ok(());
    }
    let len = bb.read_var_uint()? as usize;
    match type_id {
        TYPE_BOOL | TYPE_BYTE => bb.take_bytes(len).map(|_| ())?,
        TYPE_INT | TYPE_UINT => {
            for _ in 0..len {
                bb.read_var_uint()?;
            }
        }
        TYPE_FLOAT => {
            for _ in 0..len {
                bb.read_var_float()?;
            }
        }
        TYPE_STRING => {
            for _ in 0..len {
                bb.read_string()?;
            }
        }
        TYPE_INT64 | TYPE_UINT64 => {
            for _ in 0..len {
                bb.read_var_uint64()?;
            }
        }
        other => {
            return Err(ParseError::SchemaDecode(format!(
                "skip_primitive array: bad id {}",
                other
            )))
        }
    }
    Ok(())
}

/// Advance past a value of `type_id` without building anything.
fn skip_value(
    schema: &Schema,
    type_id: i32,
    is_array: bool,
    bb: &mut ByteBuf,
) -> Result<(), ParseError> {
    if type_id >= 0 {
        let def = &schema.defs[type_id as usize];
        let count = if is_array {
            bb.read_var_uint()? as usize
        } else {
            1
        };
        for _ in 0..count {
            match def.kind {
                DefKind::Enum => {
                    bb.read_var_uint()?;
                }
                DefKind::Struct => {
                    // Fixed layout: every declared field must be consumed.
                    for field in &def.fields {
                        skip_value(schema, field.type_id, field.is_array, bb)?;
                    }
                }
                DefKind::Message => loop {
                    let id = bb.read_var_uint()?;
                    if id == 0 {
                        break;
                    }
                    match def.field_value_to_index.get(&id) {
                        Some(index) => {
                            let field = &def.fields[*index];
                            skip_value(schema, field.type_id, field.is_array, bb)?;
                        }
                        None => {
                            return Err(ParseError::SchemaDecode(format!(
                                "unknown field id {} while skipping message {}",
                                id, def.name
                            )));
                        }
                    }
                },
            }
        }
        return Ok(());
    }
    skip_primitive(type_id, is_array, bb)
}

fn decode_field<'s, 'm>(
    schema: &'s Schema,
    name: &'s str,
    type_id: i32,
    is_array: bool,
    bb: &mut ByteBuf<'m>,
) -> Result<Option<FastVal<'s, 'm>>, ParseError> {
    let kept = keep_set().contains(name);
    if !kept {
        skip_value(schema, type_id, is_array, bb)?;
        return Ok(None);
    }

    if !is_array && type_id >= 0 {
        // User-defined single value.
        return decode_defined(schema, type_id, bb).map(Some);
    }

    if !is_array {
        return read_primitive(type_id, bb).map(Some);
    }

    // Arrays.
    if type_id == TYPE_BYTE {
        let len = bb.read_var_uint()? as usize;
        return Ok(Some(FastVal::Bytes(bb.take_bytes(len)?)));
    }
    let len = bb.read_var_uint()? as usize;
    let mut arr = Vec::with_capacity(len.min(1 << 20));
    if type_id >= 0 {
        for _ in 0..len {
            arr.push(decode_defined(schema, type_id, bb)?);
        }
    } else {
        for _ in 0..len {
            arr.push(read_primitive(type_id, bb)?);
        }
    }
    Ok(Some(FastVal::Arr(arr)))
}

fn decode_defined<'s, 'm>(
    schema: &'s Schema,
    type_id: i32,
    bb: &mut ByteBuf<'m>,
) -> Result<FastVal<'s, 'm>, ParseError> {
    debug_assert!(type_id >= 0);
    let def = &schema.defs[type_id as usize];
    match def.kind {
        DefKind::Enum => {
            let raw = bb.read_var_uint()?;
            match def.field_value_to_index.get(&raw) {
                Some(index) => Ok(FastVal::Enum(def.fields[*index].name.as_str())),
                None => Err(ParseError::SchemaDecode(format!(
                    "unknown enum value {} for {}",
                    raw, def.name
                ))),
            }
        }
        DefKind::Struct => {
            // Fixed layout: declaration order decides reads; keep-filter drops
            // uninteresting fields but they are always consumed.
            let mut fields = Vec::new();
            for field in &def.fields {
                if let Some(v) = decode_field(
                    schema,
                    field.name.as_str(),
                    field.type_id,
                    field.is_array,
                    bb,
                )? {
                    fields.push((field.name.as_str(), v));
                }
            }
            Ok(FastVal::Obj(fields))
        }
        DefKind::Message => {
            let mut fields = Vec::new();
            loop {
                let id = bb.read_var_uint()?;
                if id == 0 {
                    return Ok(FastVal::Obj(fields));
                }
                match def.field_value_to_index.get(&id) {
                    Some(index) => {
                        let field = &def.fields[*index];
                        if let Some(v) = decode_field(
                            schema,
                            field.name.as_str(),
                            field.type_id,
                            field.is_array,
                            bb,
                        )? {
                            fields.push((field.name.as_str(), v));
                        }
                    }
                    None => {
                        return Err(ParseError::SchemaDecode(format!(
                            "unknown field id {} in message {}",
                            id, def.name
                        )));
                    }
                }
            }
        }
    }
}

/// Decode `message_bytes` against `schema`, returning the root value.
pub fn decode_root<'s, 'm>(
    schema: &'s Schema,
    message_bytes: &'m [u8],
) -> Result<FastVal<'s, 'm>, ParseError> {
    let root_id = if let Some(def) = schema.def("Message") {
        def.index
    } else {
        schema
            .defs
            .first()
            .map(|d| d.index)
            .ok_or_else(|| ParseError::SchemaDecode("No root type found in schema".into()))?
    };
    let mut bb = ByteBuf::new(message_bytes);
    decode_defined(schema, root_id, &mut bb)
}
