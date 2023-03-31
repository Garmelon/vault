use std::{error, fmt, str::Utf8Error};

use rusqlite::{
    types::{FromSqlError, ValueRef},
    Row,
};
use serde::{
    de::{
        self, value::BorrowedStrDeserializer, DeserializeSeed, Deserializer, MapAccess, SeqAccess,
        Visitor,
    },
    forward_to_deserialize_any, Deserialize,
};

#[derive(Debug)]
enum Error {
    ExpectedTupleLikeBaseType,
    ExpectedStructLikeBaseType,
    Utf8(Utf8Error),
    Rusqlite(rusqlite::Error),
    Custom(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ExpectedTupleLikeBaseType => write!(f, "expected tuple-like base type"),
            Self::ExpectedStructLikeBaseType => write!(f, "expected struct-like base type"),
            Self::Utf8(err) => err.fmt(f),
            Self::Rusqlite(err) => err.fmt(f),
            Self::Custom(msg) => msg.fmt(f),
        }
    }
}

impl error::Error for Error {}

impl de::Error for Error {
    fn custom<T: fmt::Display>(msg: T) -> Self {
        Self::Custom(msg.to_string())
    }
}

impl From<Utf8Error> for Error {
    fn from(value: Utf8Error) -> Self {
        Self::Utf8(value)
    }
}

impl From<rusqlite::Error> for Error {
    fn from(value: rusqlite::Error) -> Self {
        Self::Rusqlite(value)
    }
}

struct ValueRefDeserializer<'de> {
    value: ValueRef<'de>,
}

impl<'de> Deserializer<'de> for ValueRefDeserializer<'de> {
    type Error = Error;

    forward_to_deserialize_any! {
        i8 i16 i32 i64 u8 u16 u32 u64 f32 f64 char str string bytes byte_buf
        unit unit_struct seq tuple tuple_struct map struct identifier
        ignored_any
    }

    fn deserialize_any<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Self::Error> {
        match self.value {
            ValueRef::Null => visitor.visit_unit(),
            ValueRef::Integer(v) => visitor.visit_i64(v),
            ValueRef::Real(v) => visitor.visit_f64(v),
            ValueRef::Text(v) => visitor.visit_borrowed_str(std::str::from_utf8(v)?),
            ValueRef::Blob(v) => visitor.visit_borrowed_bytes(v),
        }
    }

    fn deserialize_bool<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Self::Error> {
        match self.value {
            ValueRef::Integer(0) => visitor.visit_bool(false),
            ValueRef::Integer(_) => visitor.visit_bool(true),
            _ => self.deserialize_any(visitor),
        }
    }

    fn deserialize_option<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Self::Error> {
        match self.value {
            ValueRef::Null => visitor.visit_none(),
            _ => visitor.visit_some(self),
        }
    }

    fn deserialize_newtype_struct<V: Visitor<'de>>(
        self,
        _name: &'static str,
        visitor: V,
    ) -> Result<V::Value, Self::Error> {
        visitor.visit_newtype_struct(self)
    }

    fn deserialize_enum<V: Visitor<'de>>(
        self,
        name: &'static str,
        variants: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Self::Error> {
        match self.value {
            ValueRef::Text(v) => {
                let v = BorrowedStrDeserializer::new(std::str::from_utf8(v)?);
                v.deserialize_enum(name, variants, visitor)
            }
            _ => self.deserialize_any(visitor),
        }
    }
}

struct IndexedRowDeserializer<'de, 'stmt> {
    row: &'de Row<'stmt>,
}

impl<'de> Deserializer<'de> for IndexedRowDeserializer<'de, '_> {
    type Error = Error;

    forward_to_deserialize_any! {
        bool i8 i16 i32 i64 u8 u16 u32 u64 f32 f64 char str string bytes
        byte_buf option unit unit_struct map enum identifier ignored_any
    }

    fn deserialize_any<V: Visitor<'de>>(self, _visitor: V) -> Result<V::Value, Self::Error> {
        Err(Error::ExpectedTupleLikeBaseType)
    }

    fn deserialize_newtype_struct<V: Visitor<'de>>(
        self,
        _name: &'static str,
        visitor: V,
    ) -> Result<V::Value, Self::Error> {
        visitor.visit_newtype_struct(self)
    }

    fn deserialize_seq<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Self::Error> {
        visitor.visit_seq(IndexedRowSeq::new(self.row))
    }

    fn deserialize_tuple<V: Visitor<'de>>(
        self,
        _len: usize,
        visitor: V,
    ) -> Result<V::Value, Self::Error> {
        self.deserialize_seq(visitor)
    }

    fn deserialize_tuple_struct<V: Visitor<'de>>(
        self,
        _name: &'static str,
        _len: usize,
        visitor: V,
    ) -> Result<V::Value, Self::Error> {
        self.deserialize_seq(visitor)
    }

    fn deserialize_struct<V: Visitor<'de>>(
        self,
        _name: &'static str,
        fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Self::Error> {
        visitor.visit_map(IndexedRowMap::new(self.row, fields))
    }
}

struct IndexedRowSeq<'de, 'stmt> {
    row: &'de Row<'stmt>,
    next_index: usize,
}

impl<'de, 'stmt> IndexedRowSeq<'de, 'stmt> {
    fn new(row: &'de Row<'stmt>) -> Self {
        Self { row, next_index: 0 }
    }
}

impl<'de> SeqAccess<'de> for IndexedRowSeq<'de, '_> {
    type Error = Error;

    fn next_element_seed<T>(&mut self, seed: T) -> Result<Option<T::Value>, Self::Error>
    where
        T: DeserializeSeed<'de>,
    {
        match self.row.get_ref(self.next_index) {
            Ok(value) => {
                self.next_index += 1;
                seed.deserialize(ValueRefDeserializer { value }).map(Some)
            }
            Err(rusqlite::Error::InvalidColumnIndex(_)) => Ok(None),
            Err(err) => Err(err)?,
        }
    }
}

struct IndexedRowMap<'de, 'stmt> {
    row: &'de Row<'stmt>,
    fields: &'static [&'static str],
    next_index: usize,
}

impl<'de, 'stmt> IndexedRowMap<'de, 'stmt> {
    fn new(row: &'de Row<'stmt>, fields: &'static [&'static str]) -> Self {
        Self {
            row,
            fields,
            next_index: 0,
        }
    }
}

impl<'de> MapAccess<'de> for IndexedRowMap<'de, '_> {
    type Error = Error;

    fn next_key_seed<K>(&mut self, seed: K) -> Result<Option<K::Value>, Self::Error>
    where
        K: DeserializeSeed<'de>,
    {
        if let Some(key) = self.fields.get(self.next_index) {
            self.next_index += 1;
            seed.deserialize(BorrowedStrDeserializer::new(key))
                .map(Some)
        } else {
            Ok(None)
        }
    }

    fn next_value_seed<V>(&mut self, seed: V) -> Result<V::Value, Self::Error>
    where
        V: DeserializeSeed<'de>,
    {
        let value = self.row.get_ref(self.next_index - 1)?;
        seed.deserialize(ValueRefDeserializer { value })
    }
}

pub fn from_row_via_index<'de, T>(row: &'de Row<'_>) -> rusqlite::Result<T>
where
    T: Deserialize<'de>,
{
    T::deserialize(IndexedRowDeserializer { row })
        .map_err(|err| FromSqlError::Other(Box::new(err)).into())
}

struct NamedRowDeserializer<'de, 'stmt> {
    row: &'de Row<'stmt>,
}

impl<'de> Deserializer<'de> for NamedRowDeserializer<'de, '_> {
    type Error = Error;

    forward_to_deserialize_any! {
        bool i8 i16 i32 i64 u8 u16 u32 u64 f32 f64 char str string bytes
        byte_buf option unit unit_struct newtype_struct seq tuple tuple_struct
        map enum identifier ignored_any
    }

    fn deserialize_any<V: Visitor<'de>>(self, _visitor: V) -> Result<V::Value, Self::Error> {
        Err(Error::ExpectedStructLikeBaseType)
    }

    fn deserialize_struct<V: Visitor<'de>>(
        self,
        _name: &'static str,
        fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Self::Error> {
        visitor.visit_map(NamedRowMap::new(self.row, fields))
    }
}

struct NamedRowMap<'de, 'stmt> {
    row: &'de Row<'stmt>,
    fields: &'static [&'static str],
    next_index: usize,
}

impl<'de, 'stmt> NamedRowMap<'de, 'stmt> {
    fn new(row: &'de Row<'stmt>, fields: &'static [&'static str]) -> Self {
        Self {
            row,
            fields,
            next_index: 0,
        }
    }
}

impl<'de> MapAccess<'de> for NamedRowMap<'de, '_> {
    type Error = Error;

    fn next_key_seed<K>(&mut self, seed: K) -> Result<Option<K::Value>, Self::Error>
    where
        K: DeserializeSeed<'de>,
    {
        if let Some(key) = self.fields.get(self.next_index) {
            self.next_index += 1;
            seed.deserialize(BorrowedStrDeserializer::new(key))
                .map(Some)
        } else {
            Ok(None)
        }
    }

    fn next_value_seed<V>(&mut self, seed: V) -> Result<V::Value, Self::Error>
    where
        V: DeserializeSeed<'de>,
    {
        let value = self.row.get_ref(self.next_index - 1)?;
        seed.deserialize(ValueRefDeserializer { value })
    }
}

pub fn from_row_via_name<'de, T>(row: &'de Row<'_>) -> rusqlite::Result<T>
where
    T: Deserialize<'de>,
{
    T::deserialize(NamedRowDeserializer { row })
        .map_err(|err| FromSqlError::Other(Box::new(err)).into())
}
