//! The borrowed forms borrow: every string and byte string a borrowed decode
//! yields points into the input.

use codecs::Format;
use serde::Serialize;

use super::on_a_big_stack;
use crate::{ALL, Expect, Visit, Workload, blob, dispatch, enums, families, kynos, sweep, world};

/// Every string and byte string a value serializes, as address ranges.
///
/// A borrowed form serializes each `&str` and `&[u8]` field as the slice it
/// holds, so the ranges a serializer is handed are the ranges the fields
/// point at. Empty ones are dropped: an empty slice points nowhere in
/// particular (`#[serde(default)]` fills an absent `&str` with a static `""`).
mod slices {
    use serde::ser::{self, Serialize};

    /// A range: start and end address, and the text (or "a byte string").
    pub type Range = (usize, usize, String);

    #[derive(Debug)]
    pub struct Collect {
        pub ranges: Vec<Range>,
    }

    #[derive(Debug)]
    pub struct Never;

    impl std::fmt::Display for Never {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("unreachable")
        }
    }

    impl std::error::Error for Never {}

    impl ser::Error for Never {
        fn custom<T: std::fmt::Display>(_: T) -> Self {
            Self
        }
    }

    impl Collect {
        fn push(&mut self, bytes: &[u8], what: &str) {
            if !bytes.is_empty() {
                let start = bytes.as_ptr() as usize;
                self.ranges.push((start, start + bytes.len(), what.to_owned()));
            }
        }
    }

    macro_rules! nothing {
        ($($name:ident: $ty:ty),* $(,)?) => {
            $(fn $name(self, _: $ty) -> Result<(), Never> { Ok(()) })*
        };
    }

    impl ser::Serializer for &mut Collect {
        type Ok = ();
        type Error = Never;
        type SerializeSeq = Self;
        type SerializeTuple = Self;
        type SerializeTupleStruct = Self;
        type SerializeTupleVariant = Self;
        type SerializeMap = Self;
        type SerializeStruct = Self;
        type SerializeStructVariant = Self;

        /// Not human-readable, so bytes are written as bytes (`blob::wire`).
        fn is_human_readable(&self) -> bool {
            false
        }

        nothing!(
            serialize_bool: bool,
            serialize_i8: i8,
            serialize_i16: i16,
            serialize_i32: i32,
            serialize_i64: i64,
            serialize_i128: i128,
            serialize_u8: u8,
            serialize_u16: u16,
            serialize_u32: u32,
            serialize_u64: u64,
            serialize_u128: u128,
            serialize_f32: f32,
            serialize_f64: f64,
            serialize_char: char,
            serialize_unit_struct: &'static str,
        );

        fn serialize_str(self, v: &str) -> Result<(), Never> {
            self.push(v.as_bytes(), v);
            Ok(())
        }
        fn serialize_bytes(self, v: &[u8]) -> Result<(), Never> {
            self.push(v, "a byte string");
            Ok(())
        }
        fn serialize_none(self) -> Result<(), Never> {
            Ok(())
        }
        fn serialize_some<T: Serialize + ?Sized>(self, v: &T) -> Result<(), Never> {
            v.serialize(self)
        }
        fn serialize_unit(self) -> Result<(), Never> {
            Ok(())
        }
        fn serialize_unit_variant(self, _: &'static str, _: u32, _: &'static str) -> Result<(), Never> {
            Ok(())
        }
        fn serialize_newtype_struct<T: Serialize + ?Sized>(self, _: &'static str, v: &T) -> Result<(), Never> {
            v.serialize(self)
        }
        fn serialize_newtype_variant<T: Serialize + ?Sized>(
            self,
            _: &'static str,
            _: u32,
            _: &'static str,
            v: &T,
        ) -> Result<(), Never> {
            v.serialize(self)
        }
        fn serialize_seq(self, _: Option<usize>) -> Result<Self, Never> {
            Ok(self)
        }
        fn serialize_tuple(self, _: usize) -> Result<Self, Never> {
            Ok(self)
        }
        fn serialize_tuple_struct(self, _: &'static str, _: usize) -> Result<Self, Never> {
            Ok(self)
        }
        fn serialize_tuple_variant(self, _: &'static str, _: u32, _: &'static str, _: usize) -> Result<Self, Never> {
            Ok(self)
        }
        fn serialize_map(self, _: Option<usize>) -> Result<Self, Never> {
            Ok(self)
        }
        fn serialize_struct(self, _: &'static str, _: usize) -> Result<Self, Never> {
            Ok(self)
        }
        fn serialize_struct_variant(self, _: &'static str, _: u32, _: &'static str, _: usize) -> Result<Self, Never> {
            Ok(self)
        }
    }

    macro_rules! compound {
        ($($trait:ident: $method:ident),* $(,)?) => {
            $(impl ser::$trait for &mut Collect {
                type Ok = ();
                type Error = Never;
                fn $method<T: Serialize + ?Sized>(&mut self, v: &T) -> Result<(), Never> {
                    v.serialize(&mut **self)
                }
                fn end(self) -> Result<(), Never> {
                    Ok(())
                }
            })*
        };
    }

    compound!(
        SerializeSeq: serialize_element,
        SerializeTuple: serialize_element,
        SerializeTupleStruct: serialize_field,
        SerializeTupleVariant: serialize_field,
    );

    impl ser::SerializeMap for &mut Collect {
        type Ok = ();
        type Error = Never;
        fn serialize_key<T: Serialize + ?Sized>(&mut self, k: &T) -> Result<(), Never> {
            k.serialize(&mut **self)
        }
        fn serialize_value<T: Serialize + ?Sized>(&mut self, v: &T) -> Result<(), Never> {
            v.serialize(&mut **self)
        }
        fn end(self) -> Result<(), Never> {
            Ok(())
        }
    }

    impl ser::SerializeStruct for &mut Collect {
        type Ok = ();
        type Error = Never;
        fn serialize_field<T: Serialize + ?Sized>(&mut self, _: &'static str, v: &T) -> Result<(), Never> {
            v.serialize(&mut **self)
        }
        fn end(self) -> Result<(), Never> {
            Ok(())
        }
    }

    impl ser::SerializeStructVariant for &mut Collect {
        type Ok = ();
        type Error = Never;
        fn serialize_field<T: Serialize + ?Sized>(&mut self, _: &'static str, v: &T) -> Result<(), Never> {
            v.serialize(&mut **self)
        }
        fn end(self) -> Result<(), Never> {
            Ok(())
        }
    }

    /// The non-empty string and byte-string ranges `value` serializes.
    pub fn of<T: Serialize>(value: &T) -> Vec<Range> {
        let mut collect = Collect { ranges: Vec::new() };
        let _ = value.serialize(&mut collect);
        collect.ranges
    }
}

/// The variant names `enums`' internally tagged `Event` writes as its tag:
/// static strings serde's derive serializes itself, which no field holds.
const INTERNAL_TAGS: &[&str] = &["created", "deleted", "moved", "heartbeat"];

/// Whether every range lies inside `input`, tags excepted.
fn all_inside(input: &[u8], ranges: &[slices::Range]) -> Result<(), String> {
    let span = input.as_ptr() as usize..input.as_ptr() as usize + input.len();
    match ranges.iter().find(|(start, end, what)| {
        (!span.contains(start) || *end > span.end) && !INTERNAL_TAGS.contains(&what.as_str())
    }) {
        Some((_, _, what)) => Err(format!("{what:?} does not point into the input")),
        None => Ok(()),
    }
}

/// The borrowed form of every workload whose bytes the oracle decodes,
/// decoded by the oracle (`serde_cbor::from_slice`, which hands out a slice
/// of the input for every definite-length string): the result is the
/// workload's value, and every string and byte string of it points into the
/// input. Returns how many slices it holds.
struct Borrows;

impl Visit for Borrows {
    type Output = Option<usize>;

    fn visit<W: Workload>(self) -> Option<usize> {
        if W::EXPECT != Expect::Value {
            return None;
        }
        let bytes = W::bytes(Format::Cbor);
        let decoded = match serde_cbor::from_slice::<W::Borrowed<'_>>(&bytes) {
            Ok(decoded) => decoded,
            // Past the oracle's 128 levels; `Node` holds no string anyway.
            Err(_) if W::NAME.starts_with("nested-") => return None,
            Err(error) => panic!("{}: the oracle refuses the borrowed form: {error}", W::NAME),
        };
        assert_eq!(
            serde_json::to_value(&decoded).expect("a JSON value"),
            serde_json::to_value(W::value()).expect("a JSON value"),
            "{}: the borrowed form is another value",
            W::NAME
        );
        let ranges = slices::of(&decoded);
        if let Err(why) = all_inside(&bytes, &ranges) {
            panic!("{}: {why}", W::NAME);
        }
        Some(ranges.len())
    }
}

#[test]
fn every_borrowed_form_borrows_every_string_from_the_input() {
    let mut borrowed = Vec::new();
    for name in ALL {
        let name = *name;
        if let Some(count) = on_a_big_stack(move || dispatch(name, Borrows).expect("dispatches")) {
            borrowed.push((name, count));
        }
    }
    // The workloads that hold strings or bytes really hold some.
    for name in [
        kynos::JsonSmall::NAME,
        kynos::EchoPost::NAME,
        kynos::JsonLarge::NAME,
        families::Unicode::NAME,
        families::Wide::NAME,
        families::LongString::NAME,
        families::Sparse::NAME,
        families::Defaults::NAME,
        families::KeysLongInt::NAME,
        blob::ByteStrings::NAME,
        enums::Enums::NAME,
        world::TwitterLike::NAME,
        world::CitmLike::NAME,
        world::CanadaLike::NAME,
        world::SkipUnknown::NAME,
        sweep::Sweep::<1024>::NAME,
    ] {
        let count = borrowed.iter().find(|(n, _)| *n == name).map(|(_, c)| *c);
        assert!(count.is_some_and(|c| c > 0), "{name}: {count:?} borrowed slices");
    }
}

/// An owned decode's strings are copies, and the check says so: it is not
/// vacuous.
#[test]
fn a_copied_string_is_caught() {
    #[derive(serde::Deserialize, Serialize)]
    struct Tags<'a> {
        #[serde(borrow)]
        tags: Vec<std::borrow::Cow<'a, str>>,
    }
    let bytes = kynos::JsonSmall::bytes(Format::Cbor);
    let owned: kynos::Item = serde_cbor::from_slice(&bytes).expect("decodes");
    let ranges = slices::of(&owned);
    assert_eq!(ranges.len(), 4, "{ranges:?}");
    assert!(all_inside(&bytes, &ranges).is_err());
    // And a `Cow` inside a `Vec` copies, which is why no form uses one.
    let bytes = crate::cbor(&serde_json::json!({ "tags": ["a", "b"] }));
    let tags: Tags<'_> = serde_cbor::from_slice(&bytes).expect("decodes");
    assert!(all_inside(&bytes, &slices::of(&tags)).is_err());
}

/// serde_json hands out a slice for a string without escapes and refuses a
/// `&str` for one with any: `twitter-like`'s `source` holds quotes, which
/// JSON escapes, and `byte-strings` holds base64 text. Those two borrowed
/// forms are refused from JSON (it does not borrow them), and every other
/// one decodes; a change to the list is a change to what the JSON baseline's
/// borrowed decode measures.
#[test]
fn json_borrows_every_workload_but_escaped_text_and_base64() {
    struct Json;
    impl Visit for Json {
        type Output = Option<bool>;
        fn visit<W: Workload>(self) -> Option<bool> {
            if W::EXPECT != Expect::Value || W::NAME.starts_with("nested-") && W::NAME != "nested-100" {
                return None;
            }
            let bytes = W::bytes(Format::Json);
            Some(serde_json::from_slice::<W::Borrowed<'_>>(&bytes).is_ok())
        }
    }
    let refused: Vec<&str> = ALL
        .iter()
        .copied()
        .filter(|&name| on_a_big_stack(move || dispatch(name, Json).expect("dispatches")) == Some(false))
        .collect();
    assert_eq!(refused, [blob::ByteStrings::NAME, world::TwitterLike::NAME]);
}
