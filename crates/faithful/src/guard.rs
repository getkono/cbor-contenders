//! The encode half: a `Serialize` wrapper that refuses, through whatever
//! serializer it is handed, every value the JSON data model has no place
//! for. It wraps the value, not the serializer, so it works with libraries
//! whose `Serializer` is private (ciborium's is).

use serde::ser::{
    self, Error as _, Impossible, Serialize, SerializeMap, SerializeSeq, SerializeStruct, SerializeStructVariant,
    SerializeTuple, SerializeTupleStruct, SerializeTupleVariant, Serializer,
};

/// `value`, serialized under the JSON-data-model profile (crate docs).
#[derive(Debug)]
pub struct Faithful<'a, T: ?Sized>(pub &'a T);

impl<T: Serialize + ?Sized> Serialize for Faithful<'_, T> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.0.serialize(Guard(serializer))
    }
}

const BYTES: &str = "a byte string has no JSON equivalent the schema could describe";
const NON_FINITE: &str = "NaN and infinities are not JSON numbers";
const WIDE: &str = "a 128-bit integer has no schema";
const KEY: &str = "a map key that is not a string has no JSON equivalent";

/// The wrapped serializer.
struct Guard<S>(S);

/// A compound serializer, its elements wrapped.
struct Each<C>(C);

macro_rules! pass {
    ($($method:ident($ty:ty)),* $(,)?) => {$(
        fn $method(self, v: $ty) -> Result<S::Ok, S::Error> {
            self.0.$method(v)
        }
    )*};
}

impl<S: Serializer> Serializer for Guard<S> {
    type Ok = S::Ok;
    type Error = S::Error;
    type SerializeSeq = Each<S::SerializeSeq>;
    type SerializeTuple = Each<S::SerializeTuple>;
    type SerializeTupleStruct = Each<S::SerializeTupleStruct>;
    type SerializeTupleVariant = Each<S::SerializeTupleVariant>;
    type SerializeMap = Each<S::SerializeMap>;
    type SerializeStruct = Each<S::SerializeStruct>;
    type SerializeStructVariant = Each<S::SerializeStructVariant>;

    pass!(
        serialize_bool(bool),
        serialize_i8(i8),
        serialize_i16(i16),
        serialize_i32(i32),
        serialize_i64(i64),
        serialize_u8(u8),
        serialize_u16(u16),
        serialize_u32(u32),
        serialize_u64(u64),
        serialize_char(char),
        serialize_str(&str),
    );

    fn serialize_i128(self, _: i128) -> Result<S::Ok, S::Error> {
        Err(S::Error::custom(WIDE))
    }

    fn serialize_u128(self, _: u128) -> Result<S::Ok, S::Error> {
        Err(S::Error::custom(WIDE))
    }

    fn serialize_f32(self, v: f32) -> Result<S::Ok, S::Error> {
        if v.is_finite() {
            self.0.serialize_f32(v)
        } else {
            Err(S::Error::custom(NON_FINITE))
        }
    }

    fn serialize_f64(self, v: f64) -> Result<S::Ok, S::Error> {
        if v.is_finite() {
            self.0.serialize_f64(v)
        } else {
            Err(S::Error::custom(NON_FINITE))
        }
    }

    fn serialize_bytes(self, _: &[u8]) -> Result<S::Ok, S::Error> {
        Err(S::Error::custom(BYTES))
    }

    fn serialize_none(self) -> Result<S::Ok, S::Error> {
        self.0.serialize_none()
    }

    fn serialize_some<T: Serialize + ?Sized>(self, value: &T) -> Result<S::Ok, S::Error> {
        self.0.serialize_some(&Faithful(value))
    }

    fn serialize_unit(self) -> Result<S::Ok, S::Error> {
        self.0.serialize_unit()
    }

    fn serialize_unit_struct(self, name: &'static str) -> Result<S::Ok, S::Error> {
        self.0.serialize_unit_struct(name)
    }

    fn serialize_unit_variant(self, name: &'static str, index: u32, variant: &'static str) -> Result<S::Ok, S::Error> {
        self.0.serialize_unit_variant(name, index, variant)
    }

    fn serialize_newtype_struct<T: Serialize + ?Sized>(self, name: &'static str, value: &T) -> Result<S::Ok, S::Error> {
        self.0.serialize_newtype_struct(name, &Faithful(value))
    }

    fn serialize_newtype_variant<T: Serialize + ?Sized>(
        self,
        name: &'static str,
        index: u32,
        variant: &'static str,
        value: &T,
    ) -> Result<S::Ok, S::Error> {
        self.0.serialize_newtype_variant(name, index, variant, &Faithful(value))
    }

    fn serialize_seq(self, len: Option<usize>) -> Result<Self::SerializeSeq, S::Error> {
        self.0.serialize_seq(len).map(Each)
    }

    fn serialize_tuple(self, len: usize) -> Result<Self::SerializeTuple, S::Error> {
        self.0.serialize_tuple(len).map(Each)
    }

    fn serialize_tuple_struct(self, name: &'static str, len: usize) -> Result<Self::SerializeTupleStruct, S::Error> {
        self.0.serialize_tuple_struct(name, len).map(Each)
    }

    fn serialize_tuple_variant(
        self,
        name: &'static str,
        index: u32,
        variant: &'static str,
        len: usize,
    ) -> Result<Self::SerializeTupleVariant, S::Error> {
        self.0.serialize_tuple_variant(name, index, variant, len).map(Each)
    }

    fn serialize_map(self, len: Option<usize>) -> Result<Self::SerializeMap, S::Error> {
        self.0.serialize_map(len).map(Each)
    }

    fn serialize_struct(self, name: &'static str, len: usize) -> Result<Self::SerializeStruct, S::Error> {
        self.0.serialize_struct(name, len).map(Each)
    }

    fn serialize_struct_variant(
        self,
        name: &'static str,
        index: u32,
        variant: &'static str,
        len: usize,
    ) -> Result<Self::SerializeStructVariant, S::Error> {
        self.0.serialize_struct_variant(name, index, variant, len).map(Each)
    }

    fn is_human_readable(&self) -> bool {
        self.0.is_human_readable()
    }
}

macro_rules! each {
    ($($trait:ident :: $method:ident),* $(,)?) => {$(
        impl<C: $trait> $trait for Each<C> {
            type Ok = C::Ok;
            type Error = C::Error;

            fn $method<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<(), C::Error> {
                self.0.$method(&Faithful(value))
            }

            fn end(self) -> Result<C::Ok, C::Error> {
                self.0.end()
            }
        }
    )*};
}

each!(
    SerializeSeq::serialize_element,
    SerializeTuple::serialize_element,
    SerializeTupleStruct::serialize_field,
    SerializeTupleVariant::serialize_field,
);

impl<C: SerializeStruct> SerializeStruct for Each<C> {
    type Ok = C::Ok;
    type Error = C::Error;

    fn serialize_field<T: Serialize + ?Sized>(&mut self, key: &'static str, value: &T) -> Result<(), C::Error> {
        self.0.serialize_field(key, &Faithful(value))
    }

    fn skip_field(&mut self, key: &'static str) -> Result<(), C::Error> {
        self.0.skip_field(key)
    }

    fn end(self) -> Result<C::Ok, C::Error> {
        self.0.end()
    }
}

impl<C: SerializeStructVariant> SerializeStructVariant for Each<C> {
    type Ok = C::Ok;
    type Error = C::Error;

    fn serialize_field<T: Serialize + ?Sized>(&mut self, key: &'static str, value: &T) -> Result<(), C::Error> {
        self.0.serialize_field(key, &Faithful(value))
    }

    fn skip_field(&mut self, key: &'static str) -> Result<(), C::Error> {
        self.0.skip_field(key)
    }

    fn end(self) -> Result<C::Ok, C::Error> {
        self.0.end()
    }
}

impl<C: SerializeMap> SerializeMap for Each<C> {
    type Ok = C::Ok;
    type Error = C::Error;

    fn serialize_key<T: Serialize + ?Sized>(&mut self, key: &T) -> Result<(), C::Error> {
        // Checked by serializing it into a probe that accepts only strings;
        // the probe writes nothing, then the real serializer writes the key.
        key.serialize(KeyProbe::<C::Error>(std::marker::PhantomData))?;
        self.0.serialize_key(&Faithful(key))
    }

    fn serialize_value<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<(), C::Error> {
        self.0.serialize_value(&Faithful(value))
    }

    fn end(self) -> Result<C::Ok, C::Error> {
        self.0.end()
    }
}

/// A serializer that succeeds, writing nothing, only for a string: what a
/// map key must be.
struct KeyProbe<E>(std::marker::PhantomData<E>);

macro_rules! refuse {
    ($($method:ident($($ty:ty)?)),* $(,)?) => {$(
        fn $method(self $(, _: $ty)?) -> Result<(), E> {
            Err(E::custom(KEY))
        }
    )*};
}

impl<E: ser::Error> Serializer for KeyProbe<E> {
    type Ok = ();
    type Error = E;
    type SerializeSeq = Impossible<(), E>;
    type SerializeTuple = Impossible<(), E>;
    type SerializeTupleStruct = Impossible<(), E>;
    type SerializeTupleVariant = Impossible<(), E>;
    type SerializeMap = Impossible<(), E>;
    type SerializeStruct = Impossible<(), E>;
    type SerializeStructVariant = Impossible<(), E>;

    refuse!(
        serialize_bool(bool),
        serialize_i8(i8),
        serialize_i16(i16),
        serialize_i32(i32),
        serialize_i64(i64),
        serialize_i128(i128),
        serialize_u8(u8),
        serialize_u16(u16),
        serialize_u32(u32),
        serialize_u64(u64),
        serialize_u128(u128),
        serialize_f32(f32),
        serialize_f64(f64),
        serialize_bytes(&[u8]),
        serialize_none(),
        serialize_unit(),
    );

    fn serialize_char(self, _: char) -> Result<(), E> {
        Ok(())
    }

    fn serialize_str(self, _: &str) -> Result<(), E> {
        Ok(())
    }

    fn serialize_some<T: Serialize + ?Sized>(self, _: &T) -> Result<(), E> {
        Err(E::custom(KEY))
    }

    fn serialize_unit_struct(self, _: &'static str) -> Result<(), E> {
        Err(E::custom(KEY))
    }

    /// A unit variant is written as its name.
    fn serialize_unit_variant(self, _: &'static str, _: u32, _: &'static str) -> Result<(), E> {
        Ok(())
    }

    fn serialize_newtype_struct<T: Serialize + ?Sized>(self, _: &'static str, value: &T) -> Result<(), E> {
        value.serialize(self)
    }

    fn serialize_newtype_variant<T: Serialize + ?Sized>(
        self,
        _: &'static str,
        _: u32,
        _: &'static str,
        _: &T,
    ) -> Result<(), E> {
        Err(E::custom(KEY))
    }

    fn serialize_seq(self, _: Option<usize>) -> Result<Self::SerializeSeq, E> {
        Err(E::custom(KEY))
    }

    fn serialize_tuple(self, _: usize) -> Result<Self::SerializeTuple, E> {
        Err(E::custom(KEY))
    }

    fn serialize_tuple_struct(self, _: &'static str, _: usize) -> Result<Self::SerializeTupleStruct, E> {
        Err(E::custom(KEY))
    }

    fn serialize_tuple_variant(
        self,
        _: &'static str,
        _: u32,
        _: &'static str,
        _: usize,
    ) -> Result<Self::SerializeTupleVariant, E> {
        Err(E::custom(KEY))
    }

    fn serialize_map(self, _: Option<usize>) -> Result<Self::SerializeMap, E> {
        Err(E::custom(KEY))
    }

    fn serialize_struct(self, _: &'static str, _: usize) -> Result<Self::SerializeStruct, E> {
        Err(E::custom(KEY))
    }

    fn serialize_struct_variant(
        self,
        _: &'static str,
        _: u32,
        _: &'static str,
        _: usize,
    ) -> Result<Self::SerializeStructVariant, E> {
        Err(E::custom(KEY))
    }

    fn is_human_readable(&self) -> bool {
        false
    }
}
