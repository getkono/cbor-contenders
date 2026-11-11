//! prost encodes as Kynos's `Protobuf<T>` does: one buffer of exactly the
//! encoded length, never reallocated.

use std::sync::Mutex;

use alloc_count::{Counting, Snapshot};
use bytes::Bytes;
use codecs::native::{prost::Prost, support};
use codecs::{Failure, Native, NativeType};

#[global_allocator]
static COUNTING: Counting = Counting;

static SERIAL: Mutex<()> = Mutex::new(());

#[derive(Clone, PartialEq, ::prost::Message)]
struct Record {
    #[prost(uint64, tag = "1")]
    id: u64,
    #[prost(string, repeated, tag = "2")]
    names: Vec<String>,
}

impl NativeType for Record {
    fn proto_decode(body: Bytes) -> Result<Self, Failure> {
        support::proto_decode(body)
    }
    fn proto_encode(&self, out: &mut Vec<u8>) -> Result<(), Failure> {
        support::proto_encode(self, out)
    }
    fn proto_to_vec(&self) -> Result<Vec<u8>, Failure> {
        Ok(support::proto_to_vec(self))
    }
    fn mini_decode(_: &[u8]) -> Result<Self, Failure> {
        Err(Failure::unsupported())
    }
    fn mini_encode(&self, _: &mut Vec<u8>) -> Result<(), Failure> {
        Err(Failure::unsupported())
    }
}

fn record() -> Record {
    Record {
        id: 7,
        names: (0..200).map(|i| format!("name {i}")).collect(),
    }
}

#[test]
fn encode_and_the_server_encode_are_one_exact_allocation() {
    let _serial = SERIAL.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let value = record();
    let expected = ::prost::Message::encode_to_vec(&value);
    for encode in [Prost::encode::<Record>, Prost::encode_server::<Record>] {
        let before = Snapshot::now();
        let out = encode(&value).expect("encodes");
        let after = Snapshot::now();
        assert_eq!(out, expected);
        assert_eq!(after.allocations - before.allocations, 1);
        assert_eq!(after.reallocations, before.reallocations);
        assert_eq!(after.bytes - before.bytes, expected.len() as u64);
    }
}

#[test]
fn encode_into_reserves_once_and_reuses_capacity() {
    let _serial = SERIAL.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let value = record();
    let expected = ::prost::Message::encode_to_vec(&value);
    let mut out = Vec::new();
    let before = Snapshot::now();
    Prost::encode_into(&value, &mut out).expect("encodes");
    let after = Snapshot::now();
    assert_eq!(out, expected);
    assert_eq!(after.allocations - before.allocations, 1);
    assert_eq!(after.reallocations, before.reallocations);
    // A second encode into the same buffer allocates nothing.
    let before = Snapshot::now();
    Prost::encode_into(&value, &mut out).expect("encodes");
    let after = Snapshot::now();
    assert_eq!(out, expected);
    assert_eq!(
        (after.allocations, after.reallocations),
        (before.allocations, before.reallocations)
    );
}
