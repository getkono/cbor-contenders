//! One module per serde backend crate. A module compiles only when its
//! crate's feature is on; the baseline and the oracle are always compiled.

pub mod serde_cbor;
pub mod serde_json;

#[cfg(feature = "c2pa_cbor")]
pub mod c2pa_cbor;
#[cfg(feature = "cbor2")]
pub mod cbor2;
#[cfg(feature = "cbor4ii")]
pub mod cbor4ii;
#[cfg(feature = "ciborium")]
pub mod ciborium;
#[cfg(feature = "minicbor-serde")]
pub mod minicbor_serde;
