//! Decoder for reaver/extras/mounts/goo/protocol/goo.proto.
//! Bounded borrowed submessages; no JSON text round trip on the network path.
use anyhow::{Result, bail, ensure};
use serde::Deserialize;
use serde::de::{self, DeserializeSeed, IntoDeserializer, Visitor};
use serde_json::Value;
#[path = "wire_schema.rs"]
mod schema;

pub const MIME: &str = "application/x-protobuf";
const MAX_BYTES: usize = 8 * 1024 * 1024;
struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
}
impl<'a> Reader<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, at: 0 }
    }
    fn varint(&mut self) -> Result<u64> {
        let mut n = 0;
        for i in 0..10 {
            let b = *self
                .bytes
                .get(self.at)
                .ok_or_else(|| anyhow::anyhow!("Truncated protobuf varint"))?;
            self.at += 1;
            ensure!(i != 9 || b <= 1, "Protobuf varint overflow");
            n |= u64::from(b & 127) << (7 * i);
            if b < 128 {
                return Ok(n);
            }
        }
        bail!("Protobuf varint overflow")
    }
    fn take(&mut self, n: usize) -> Result<&'a [u8]> {
        ensure!(n <= self.bytes.len() - self.at, "Truncated protobuf field");
        let v = &self.bytes[self.at..self.at + n];
        self.at += n;
        Ok(v)
    }
    fn blob(&mut self) -> Result<&'a [u8]> {
        let n = usize::try_from(self.varint()?)?;
        self.take(n)
    }
    fn tag(&mut self) -> Result<Option<(u32, u8)>> {
        if self.at == self.bytes.len() {
            return Ok(None);
        }
        let tag = self.varint()?;
        ensure!(
            (8..=(u64::from(u32::MAX))).contains(&tag),
            "Invalid protobuf tag"
        );
        Ok(Some(((tag >> 3) as u32, (tag & 7) as u8)))
    }
    fn skip(&mut self, wire: u8) -> Result<()> {
        match wire {
            0 => {
                self.varint()?;
            }
            1 => {
                self.take(8)?;
            }
            2 => {
                self.blob()?;
            }
            5 => {
                self.take(4)?;
            }
            _ => bail!("Unsupported protobuf wire type"),
        };
        Ok(())
    }
}
// The schema describes domain messages; these descriptors are generated from
// goo.proto. Deserialization visits borrowed fields directly into Rust models.
#[derive(Clone, Copy)]
pub(super) enum Kind {
    String,
    Bool,
    U32,
    U64,
    Double,
    Message(&'static Schema),
}
#[derive(Clone, Copy, PartialEq)]
pub(super) enum Mode {
    Required,
    Optional,
    Repeated,
}
pub(super) struct Field {
    name: &'static str,
    tag: u32,
    kind: Kind,
    mode: Mode,
}
pub(super) struct Schema {
    name: &'static str,
    fields: &'static [Field],
}
#[derive(Clone, Copy)]
enum Atom<'a> {
    Var(u64),
    Double(f64),
    Bytes(&'a [u8]),
}
#[derive(Clone, Copy)]
struct Item<'a> {
    kind: Kind,
    atom: Atom<'a>,
}
impl<'a> Reader<'a> {
    fn read_atom(&mut self, kind: Kind, wire: u8) -> Result<Atom<'a>> {
        Ok(match kind {
            Kind::String | Kind::Message(_) => {
                ensure!(wire == 2, "Wrong message/string wire type");
                Atom::Bytes(self.blob()?)
            }
            Kind::Double => {
                ensure!(wire == 1, "Wrong double wire type");
                let n = f64::from_le_bytes(self.take(8)?.try_into()?);
                ensure!(n.is_finite(), "Non-finite Goo geometry");
                Atom::Double(n)
            }
            Kind::Bool | Kind::U32 | Kind::U64 => {
                ensure!(wire == 0, "Wrong scalar wire type");
                let n = self.varint()?;
                match kind {
                    Kind::Bool => ensure!(n <= 1, "Invalid protobuf boolean"),
                    Kind::U32 => ensure!(n <= u32::MAX as u64, "uint32 overflow"),
                    _ => (),
                }
                Atom::Var(n)
            }
        })
    }
}
fn check(bytes: &[u8], schema: &'static Schema, depth: usize, budget: &mut usize) -> Result<()> {
    ensure!(
        depth <= 64 && *budget > 0,
        "Protobuf nesting/message limit exceeded"
    );
    *budget -= 1;
    let mut seen = 0u64;
    let mut r = Reader::new(bytes);
    let mut choices = 0;
    while let Some((tag, wire)) = r.tag()? {
        ensure!(*budget > 0, "Protobuf field limit exceeded");
        *budget -= 1;
        let Some(f) = schema.fields.iter().find(|f| f.tag == tag) else {
            r.skip(wire)?;
            continue;
        };
        ensure!(
            f.mode == Mode::Repeated || seen & (1 << tag) == 0,
            "Duplicate {}.{}",
            schema.name,
            f.name
        );
        seen |= 1 << tag;
        if schema.name == "Pail" || (schema.name == "Response" && tag != 1) {
            choices += 1;
        }
        let a = r.read_atom(f.kind, wire)?;
        match (f.kind, a) {
            (Kind::String, Atom::Bytes(b)) => {
                std::str::from_utf8(b)?;
            }
            (Kind::Message(s), Atom::Bytes(b)) => check(b, s, depth + 1, budget)?,
            _ => (),
        }
    }
    for f in schema.fields {
        ensure!(
            f.mode != Mode::Required || seen & (1 << f.tag) != 0,
            "Missing {}.{}",
            schema.name,
            f.name
        );
    }
    if schema.name == "Pail" || schema.name == "Response" {
        ensure!(choices == 1, "Expected exactly one {} variant", schema.name);
    }
    Ok(())
}
fn fields<'a>(bytes: &'a [u8], schema: &'static Schema) -> Result<Vec<(&'static Field, Item<'a>)>> {
    let mut out = Vec::with_capacity(schema.fields.len());
    let mut r = Reader::new(bytes);
    while let Some((tag, wire)) = r.tag()? {
        if let Some(f) = schema.fields.iter().find(|f| f.tag == tag) {
            out.push((
                f,
                Item {
                    kind: f.kind,
                    atom: r.read_atom(f.kind, wire)?,
                },
            ));
        } else {
            r.skip(wire)?;
        }
    }
    Ok(out)
}
fn payload(bytes: &[u8]) -> Result<Item<'_>> {
    ensure!(bytes.len() <= MAX_BYTES, "Protobuf response exceeds 8 MiB");
    check(bytes, &schema::RESPONSE, 0, &mut 250_000)?;
    let f = fields(bytes, &schema::RESPONSE)?;
    ensure!(
        matches!(
            f.iter().find(|(f, _)| f.tag == 1).unwrap().1.atom,
            Atom::Var(2)
        ),
        "Unsupported Goo protobuf version"
    );
    Ok(f.into_iter().find(|(f, _)| f.tag != 1).unwrap().1)
}
type Error = de::value::Error;
fn err(e: impl std::fmt::Display) -> Error {
    de::Error::custom(e)
}
struct Map<'a> {
    fields: Vec<(&'static Field, Item<'a>)>,
    schema: &'static Schema,
    index: usize,
    current: usize,
}
impl<'de> de::MapAccess<'de> for Map<'de> {
    type Error = Error;
    fn next_key_seed<K: DeserializeSeed<'de>>(
        &mut self,
        seed: K,
    ) -> std::result::Result<Option<K::Value>, Error> {
        while self.index < self.schema.fields.len() {
            let i = self.index;
            self.index += 1;
            let f = &self.schema.fields[i];
            if f.mode == Mode::Repeated || self.fields.iter().any(|(v, _)| v.tag == f.tag) {
                self.current = i;
                return seed.deserialize(f.name.into_deserializer()).map(Some);
            }
        }
        Ok(None)
    }
    fn next_value_seed<V: DeserializeSeed<'de>>(
        &mut self,
        seed: V,
    ) -> std::result::Result<V::Value, Error> {
        let f = &self.schema.fields[self.current];
        if f.mode == Mode::Repeated {
            seed.deserialize(Seq(self
                .fields
                .iter()
                .filter(|(v, _)| v.tag == f.tag)
                .map(|(_, i)| *i)
                .collect::<Vec<_>>()
                .into_iter()))
        } else {
            seed.deserialize(self.fields.iter().find(|(v, _)| v.tag == f.tag).unwrap().1)
        }
    }
}
struct Seq<'a>(std::vec::IntoIter<Item<'a>>);
impl<'de> de::SeqAccess<'de> for Seq<'de> {
    type Error = Error;
    fn next_element_seed<T: DeserializeSeed<'de>>(
        &mut self,
        seed: T,
    ) -> std::result::Result<Option<T::Value>, Error> {
        self.0.next().map(|item| seed.deserialize(item)).transpose()
    }
    fn size_hint(&self) -> Option<usize> {
        Some(self.0.len())
    }
}
impl<'de> de::Deserializer<'de> for Seq<'de> {
    type Error = Error;
    fn deserialize_any<V: Visitor<'de>>(self, v: V) -> std::result::Result<V::Value, Error> {
        v.visit_seq(self)
    }
    serde::forward_to_deserialize_any! {bool i8 i16 i32 i64 u8 u16 u32 u64 f32 f64 char str string bytes byte_buf option unit unit_struct newtype_struct seq tuple tuple_struct map struct enum identifier ignored_any}
}
struct Myth<'a> {
    entries: std::vec::IntoIter<Item<'a>>,
    current: Option<Item<'a>>,
    keys: std::collections::BTreeSet<&'a str>,
}
impl<'de> de::MapAccess<'de> for Myth<'de> {
    type Error = Error;
    fn next_key_seed<K: DeserializeSeed<'de>>(
        &mut self,
        seed: K,
    ) -> std::result::Result<Option<K::Value>, Error> {
        let Some(item) = self.entries.next() else {
            return Ok(None);
        };
        let Atom::Bytes(bytes) = item.atom else {
            unreachable!()
        };
        let f = fields(bytes, &schema::MYTHENTRY).map_err(err)?;
        let Atom::Bytes(path) = f.iter().find(|(f, _)| f.tag == 1).unwrap().1.atom else {
            unreachable!()
        };
        let path = std::str::from_utf8(path).map_err(err)?;
        if !self.keys.insert(path) {
            return Err(err("Duplicate myth path"));
        }
        self.current = Some(f.iter().find(|(f, _)| f.tag == 2).unwrap().1);
        seed.deserialize(de::value::BorrowedStrDeserializer::new(path))
            .map(Some)
    }
    fn next_value_seed<V: DeserializeSeed<'de>>(
        &mut self,
        seed: V,
    ) -> std::result::Result<V::Value, Error> {
        seed.deserialize(self.current.take().unwrap())
    }
}
// RunDescriptor's in-memory API still uses branded JSON pails. Only that small
// explicit sum is projected; the response and display never become Value trees.
fn pail(bytes: &[u8]) -> Result<Value> {
    let (f, item) = fields(bytes, &schema::PAIL)?.into_iter().next().unwrap();
    let kind = match f.tag {
        1 => "nat",
        2 => "bool",
        3 => "text",
        4 => "path",
        5 => "modifiers",
        6 => "weblink",
        7 => "unsupported",
        _ => unreachable!(),
    };
    let mut value = Value::deserialize(item)?;
    if f.tag == 5 {
        value = value["values"].take();
    }
    if f.tag == 7 {
        value = serde_json::json!({"brand":value});
    }
    Ok(serde_json::json!({"kind":kind,"value":value}))
}
impl<'de> de::Deserializer<'de> for Item<'de> {
    type Error = Error;
    fn deserialize_any<V: Visitor<'de>>(self, v: V) -> std::result::Result<V::Value, Error> {
        match (self.kind, self.atom) {
            (Kind::Bool, Atom::Var(n)) => v.visit_bool(n == 1),
            (Kind::U32 | Kind::U64, Atom::Var(n)) => v.visit_u64(n),
            (Kind::Double, Atom::Double(n)) => v.visit_f64(n),
            (Kind::String, Atom::Bytes(b)) => {
                v.visit_borrowed_str(std::str::from_utf8(b).map_err(err)?)
            }
            (Kind::Message(s), Atom::Bytes(b)) => {
                let f = fields(b, s).map_err(err)?;
                match s.name {
                    "TextRuns" | "RunFragments" | "Description" => v.visit_seq(Seq(f
                        .into_iter()
                        .map(|(_, i)| i)
                        .collect::<Vec<_>>()
                        .into_iter())),
                    "Myth" => v.visit_map(Myth {
                        entries: f
                            .into_iter()
                            .map(|(_, i)| i)
                            .collect::<Vec<_>>()
                            .into_iter(),
                        current: None,
                        keys: Default::default(),
                    }),
                    "Pail" => pail(b).map_err(err)?.deserialize_any(v).map_err(err),
                    _ => v.visit_map(Map {
                        fields: f,
                        schema: s,
                        index: 0,
                        current: 0,
                    }),
                }
            }
            _ => Err(err("Invalid typed protobuf field")),
        }
    }
    fn deserialize_option<V: Visitor<'de>>(self, v: V) -> std::result::Result<V::Value, Error> {
        v.visit_some(self)
    }
    fn deserialize_enum<V: Visitor<'de>>(
        self,
        _: &'static str,
        _: &'static [&'static str],
        v: V,
    ) -> std::result::Result<V::Value, Error> {
        let (Kind::Message(s), Atom::Bytes(b)) = (self.kind, self.atom) else {
            return Err(err("Expected language enum"));
        };
        if s.name != "Language" {
            return Err(err("Unsupported wire enum"));
        }
        let (_, item) = fields(b, s).map_err(err)?.into_iter().next().unwrap();
        v.visit_enum(Language(item))
    }
    fn deserialize_ignored_any<V: Visitor<'de>>(
        self,
        v: V,
    ) -> std::result::Result<V::Value, Error> {
        // The bounded schema pass already validated this entire submessage.
        // Ignored diagnostic fields must not allocate another object tree.
        v.visit_unit()
    }
    serde::forward_to_deserialize_any! {bool i8 i16 i32 i64 u8 u16 u32 u64 f32 f64 char str string bytes byte_buf unit unit_struct newtype_struct seq tuple tuple_struct map struct identifier}
}
struct Language<'a>(Item<'a>);
impl<'de> de::EnumAccess<'de> for Language<'de> {
    type Error = Error;
    type Variant = Self;
    fn variant_seed<V: DeserializeSeed<'de>>(
        self,
        seed: V,
    ) -> std::result::Result<(V::Value, Self), Error> {
        Ok((seed.deserialize("en".into_deserializer())?, self))
    }
}
impl<'de> de::VariantAccess<'de> for Language<'de> {
    type Error = Error;
    fn unit_variant(self) -> std::result::Result<(), Error> {
        Err(err("Expected English text"))
    }
    fn newtype_variant_seed<T: DeserializeSeed<'de>>(
        self,
        seed: T,
    ) -> std::result::Result<T::Value, Error> {
        seed.deserialize(self.0)
    }
    fn tuple_variant<V: Visitor<'de>>(
        self,
        _: usize,
        _: V,
    ) -> std::result::Result<V::Value, Error> {
        Err(err("Expected English text"))
    }
    fn struct_variant<V: Visitor<'de>>(
        self,
        _: &'static [&'static str],
        v: V,
    ) -> std::result::Result<V::Value, Error> {
        de::Deserializer::deserialize_any(self.0, v)
    }
}
/// Decode directly into a domain model. Diagnostic projections use decode().
pub fn deserialize<'a, T: Deserialize<'a>>(bytes: &'a [u8]) -> Result<T> {
    Ok(T::deserialize(payload(bytes)?)?)
}
pub fn decode(bytes: &[u8]) -> Result<Value> {
    deserialize(bytes)
}
pub fn decode_value(bytes: &[u8]) -> Result<Value> {
    if is_json(bytes) {
        Ok(serde_json::from_slice(bytes)?)
    } else {
        decode(bytes)
    }
}
pub fn is_json(bytes: &[u8]) -> bool {
    bytes
        .iter()
        .find(|b| !b.is_ascii_whitespace())
        .is_some_and(|b| *b == b'{' || *b == b'[')
}
