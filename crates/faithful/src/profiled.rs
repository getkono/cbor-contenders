//! The decode half's serde view: what `T` sees of a CBOR deserializer, made
//! to match what it sees of `Json<T>`'s.
//!
//! Two things differ between a CBOR library's deserializer and serde_json's
//! as Kynos drives it, and both change which value a body decodes to:
//!
//! - **Human-readable.** Every CBOR deserializer reports
//!   `is_human_readable() == false`, and the types whose schema Kynos
//!   describes as a formatted string switch on it: `Uuid` (`format: uuid`)
//!   reads 16 bytes instead of its text, `IpAddr`/`Ipv4Addr`/`Ipv6Addr`
//!   (`ipv4`, `ipv6`) read an octet array or an enum. Kynos's schema
//!   describes the human-readable form, the one `Json<T>` reads, so the view
//!   reports `true` at every position it hands out.
//! - **A struct is read only from an object.** serde's derive also reads a
//!   struct from an array of its fields. Kynos's `Json<T>` refuses that
//!   (`kynos-0.5.0/src/extract/strict_json.rs`), since the `type: object`
//!   the schema declares refuses an array, and a CBOR library may not: one
//!   that forwards `deserialize_struct` to `deserialize_any` (c2pa_cbor)
//!   hands the derive an array as readily as a map. Where `deserialize_struct`
//!   was asked for (a struct, or a struct variant), the view refuses an
//!   array as an invalid type, before any element is read.
//!
//! The view wraps every position: the deserializer, each visitor, each
//! `SeqAccess`, `MapAccess`, `EnumAccess` and `VariantAccess`, and each seed
//! they are handed, so a nested value sees what the top-level one does. It
//! holds one flag per visitor and allocates nothing.

use std::fmt;

use serde::de::{
    self, DeserializeSeed, Deserializer, EnumAccess, Error as _, MapAccess, SeqAccess, Unexpected, VariantAccess,
    Visitor,
};

/// `T`, decoded through the human-readable view (module docs).
///
/// ```
/// # use serde::Deserialize;
/// # fn decode<'de, D: serde::Deserializer<'de>>(d: D) -> Result<std::net::Ipv4Addr, D::Error> {
/// let address = faithful::Profiled::<std::net::Ipv4Addr>::deserialize(d)?.0;
/// # Ok(address)
/// # }
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Profiled<T>(pub T);

impl<'de, T: de::Deserialize<'de>> de::Deserialize<'de> for Profiled<T> {
    #[inline]
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        T::deserialize(View(deserializer)).map(Profiled)
    }
}

/// A deserializer, seen through the view.
struct View<D>(D);

/// A visitor whose nested positions are [`View`]s; `object` when it was
/// asked for a struct, which an array is not.
struct Visit<V> {
    inner: V,
    object: bool,
}

/// An access (sequence, map, enum or variant) whose positions are [`View`]s.
struct Access<A>(A);

/// A seed whose deserializer is a [`View`].
struct Seed<S>(S);

#[inline]
fn visit<V>(inner: V) -> Visit<V> {
    Visit { inner, object: false }
}

/// Forwards each `deserialize_*` method, with its extra arguments, its
/// visitor wrapped.
macro_rules! forward {
    ($($method:ident($($argument:ident: $kind:ty),*);)*) => {$(
        #[inline]
        fn $method<V: Visitor<'de>>(self, $($argument: $kind,)* visitor: V) -> Result<V::Value, D::Error> {
            self.0.$method($($argument,)* visit(visitor))
        }
    )*};
}

impl<'de, D: Deserializer<'de>> Deserializer<'de> for View<D> {
    type Error = D::Error;

    forward! {
        deserialize_any();
        deserialize_bool();
        deserialize_i8();
        deserialize_i16();
        deserialize_i32();
        deserialize_i64();
        deserialize_i128();
        deserialize_u8();
        deserialize_u16();
        deserialize_u32();
        deserialize_u64();
        deserialize_u128();
        deserialize_f32();
        deserialize_f64();
        deserialize_char();
        deserialize_str();
        deserialize_string();
        deserialize_bytes();
        deserialize_byte_buf();
        deserialize_option();
        deserialize_unit();
        deserialize_unit_struct(name: &'static str);
        deserialize_newtype_struct(name: &'static str);
        deserialize_seq();
        deserialize_tuple(len: usize);
        deserialize_tuple_struct(name: &'static str, len: usize);
        deserialize_map();
        deserialize_enum(name: &'static str, variants: &'static [&'static str]);
        deserialize_identifier();
        deserialize_ignored_any();
    }

    #[inline]
    fn deserialize_struct<V: Visitor<'de>>(
        self,
        name: &'static str,
        fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, D::Error> {
        self.0.deserialize_struct(
            name,
            fields,
            Visit {
                inner: visitor,
                object: true,
            },
        )
    }

    #[inline]
    fn is_human_readable(&self) -> bool {
        true
    }
}

/// Forwards each scalar `visit_*` method unchanged.
macro_rules! scalar {
    ($($method:ident($kind:ty);)*) => {$(
        #[inline]
        fn $method<E: de::Error>(self, value: $kind) -> Result<Self::Value, E> {
            self.inner.$method(value)
        }
    )*};
}

impl<'de, V: Visitor<'de>> Visitor<'de> for Visit<V> {
    type Value = V::Value;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.inner.expecting(formatter)
    }

    scalar! {
        visit_bool(bool);
        visit_i8(i8);
        visit_i16(i16);
        visit_i32(i32);
        visit_i64(i64);
        visit_i128(i128);
        visit_u8(u8);
        visit_u16(u16);
        visit_u32(u32);
        visit_u64(u64);
        visit_u128(u128);
        visit_f32(f32);
        visit_f64(f64);
        visit_char(char);
        visit_str(&str);
        visit_borrowed_str(&'de str);
        visit_string(String);
        visit_bytes(&[u8]);
        visit_borrowed_bytes(&'de [u8]);
        visit_byte_buf(Vec<u8>);
    }

    #[inline]
    fn visit_none<E: de::Error>(self) -> Result<Self::Value, E> {
        self.inner.visit_none()
    }

    #[inline]
    fn visit_unit<E: de::Error>(self) -> Result<Self::Value, E> {
        self.inner.visit_unit()
    }

    #[inline]
    fn visit_some<D: Deserializer<'de>>(self, deserializer: D) -> Result<Self::Value, D::Error> {
        self.inner.visit_some(View(deserializer))
    }

    #[inline]
    fn visit_newtype_struct<D: Deserializer<'de>>(self, deserializer: D) -> Result<Self::Value, D::Error> {
        self.inner.visit_newtype_struct(View(deserializer))
    }

    /// Refused, before any element is read, where a struct was asked for.
    #[inline]
    fn visit_seq<A: SeqAccess<'de>>(self, seq: A) -> Result<Self::Value, A::Error> {
        if self.object {
            return Err(A::Error::invalid_type(Unexpected::Seq, &self.inner));
        }
        self.inner.visit_seq(Access(seq))
    }

    #[inline]
    fn visit_map<A: MapAccess<'de>>(self, map: A) -> Result<Self::Value, A::Error> {
        self.inner.visit_map(Access(map))
    }

    #[inline]
    fn visit_enum<A: EnumAccess<'de>>(self, data: A) -> Result<Self::Value, A::Error> {
        self.inner.visit_enum(Access(data))
    }

    // serde's derive calls this on a flattened `Option` field's visitor,
    // through its own buffered deserializer; forwarded so a wrapped visitor
    // keeps the answer `Option`'s visitor gives.
    #[inline]
    fn __private_visit_untagged_option<D: Deserializer<'de>>(self, deserializer: D) -> Result<Self::Value, ()> {
        self.inner.__private_visit_untagged_option(View(deserializer))
    }
}

impl<'de, S: DeserializeSeed<'de>> DeserializeSeed<'de> for Seed<S> {
    type Value = S::Value;

    #[inline]
    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<Self::Value, D::Error> {
        self.0.deserialize(View(deserializer))
    }
}

impl<'de, A: SeqAccess<'de>> SeqAccess<'de> for Access<A> {
    type Error = A::Error;

    #[inline]
    fn next_element_seed<T: DeserializeSeed<'de>>(&mut self, seed: T) -> Result<Option<T::Value>, A::Error> {
        self.0.next_element_seed(Seed(seed))
    }

    #[inline]
    fn size_hint(&self) -> Option<usize> {
        self.0.size_hint()
    }
}

impl<'de, A: MapAccess<'de>> MapAccess<'de> for Access<A> {
    type Error = A::Error;

    #[inline]
    fn next_key_seed<K: DeserializeSeed<'de>>(&mut self, seed: K) -> Result<Option<K::Value>, A::Error> {
        self.0.next_key_seed(Seed(seed))
    }

    #[inline]
    fn next_value_seed<T: DeserializeSeed<'de>>(&mut self, seed: T) -> Result<T::Value, A::Error> {
        self.0.next_value_seed(Seed(seed))
    }

    #[inline]
    fn next_entry_seed<K: DeserializeSeed<'de>, T: DeserializeSeed<'de>>(
        &mut self,
        key: K,
        value: T,
    ) -> Result<Option<(K::Value, T::Value)>, A::Error> {
        self.0.next_entry_seed(Seed(key), Seed(value))
    }

    #[inline]
    fn size_hint(&self) -> Option<usize> {
        self.0.size_hint()
    }
}

impl<'de, A: EnumAccess<'de>> EnumAccess<'de> for Access<A> {
    type Error = A::Error;
    type Variant = Access<A::Variant>;

    #[inline]
    fn variant_seed<T: DeserializeSeed<'de>>(self, seed: T) -> Result<(T::Value, Self::Variant), A::Error> {
        self.0
            .variant_seed(Seed(seed))
            .map(|(value, variant)| (value, Access(variant)))
    }
}

impl<'de, A: VariantAccess<'de>> VariantAccess<'de> for Access<A> {
    type Error = A::Error;

    #[inline]
    fn unit_variant(self) -> Result<(), A::Error> {
        self.0.unit_variant()
    }

    #[inline]
    fn newtype_variant_seed<T: DeserializeSeed<'de>>(self, seed: T) -> Result<T::Value, A::Error> {
        self.0.newtype_variant_seed(Seed(seed))
    }

    #[inline]
    fn tuple_variant<V: Visitor<'de>>(self, len: usize, visitor: V) -> Result<V::Value, A::Error> {
        self.0.tuple_variant(len, visit(visitor))
    }

    /// A struct variant is read only from an object, as a struct is.
    #[inline]
    fn struct_variant<V: Visitor<'de>>(
        self,
        fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, A::Error> {
        self.0.struct_variant(
            fields,
            Visit {
                inner: visitor,
                object: true,
            },
        )
    }
}
