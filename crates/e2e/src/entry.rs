//! One Kynos service per entry, its handlers generated per body type.

use std::sync::{Arc, LazyLock};

use bytes::Bytes;
use codecs::Format;
use kynos::extract::body::binary::Binary;
use kynos::http::media::MediaType;
use kynos::prelude::*;
use kynos::router::service::Service;
use payloads::Workload;
use payloads::kynos::{JsonLarge, JsonSmall, checksum};

use crate::body::{Catalog, Echo, Item, Shared};

/// The response values every `GET` serves, built once.
pub static LARGE: LazyLock<Shared<Catalog>> = LazyLock::new(|| Shared(Arc::new(convert(&JsonLarge::value()))));
/// See [`LARGE`].
pub static SMALL: LazyLock<Shared<Item>> = LazyLock::new(|| Shared(Arc::new(convert(&JsonSmall::value()))));

/// A `payloads` value as this crate's copy of its type.
fn convert<T: serde::Serialize, U: serde::de::DeserializeOwned>(value: &T) -> U {
    serde_json::from_slice(&serde_json::to_vec(value).unwrap_or_default())
        .unwrap_or_else(|e| unreachable!("the copies are the originals' shape: {e}"))
}

/// An echo whose checksum does not match its payload is answered with the
/// checksum zeroed, which the verifier refuses: the payload must have been
/// read.
fn checked(mut echo: Echo) -> Echo {
    if checksum(echo.payload.as_bytes()) != echo.checksum {
        echo.checksum = 0;
    }
    echo
}

/// Declares a module serving the four routes with the body type `$body`,
/// built with `$wrap` and unwrapped with `$take`.
macro_rules! codec_entry {
    ($module:ident, $body:ident, $wrap:expr, $take:expr) => {
        mod $module {
            use super::*;

            #[kynos::post("/echo")]
            async fn echo(body: $body<Echo>) -> $body<Echo> {
                ($wrap)(checked(($take)(body)))
            }

            #[kynos::post("/json/large")]
            async fn post_large(body: $body<Catalog>) -> NoContent {
                drop(std::hint::black_box(($take)(body)));
                NoContent
            }

            #[kynos::get("/json/large")]
            async fn get_large() -> $body<Shared<Catalog>> {
                ($wrap)(LARGE.clone())
            }

            #[kynos::get("/json/small")]
            async fn get_small() -> $body<Shared<Item>> {
                ($wrap)(SMALL.clone())
            }

            pub fn service() -> kynos::Result<Service<()>> {
                Router::<()>::new()
                    .mount(kynos::routes![echo, post_large, get_large, get_small])
                    .build(())
            }
        }
    };
}

type JsonBody<T> = Json<T>;
codec_entry!(serde_json_entry, JsonBody, Json, |Json(v)| v);

type ProtoBody<T> = kynos::extract::body::protobuf::Protobuf<T>;
codec_entry!(
    prost_entry,
    ProtoBody,
    kynos::extract::body::protobuf::Protobuf,
    |kynos::extract::body::protobuf::Protobuf(v)| v
);

/// Declares a CBOR entry over backend `$backend` through the worked codec.
macro_rules! cbor_entry {
    ($feature:literal, $module:ident, $backend:ty) => {
        #[cfg(feature = $feature)]
        mod $module {
            use kynos::extract::body::codec::Encoded;

            use super::*;

            type CborBody<T> = Encoded<kynos_cbor::Cbor<$backend>, T>;
            codec_entry!(inner, CborBody, Encoded::new, Encoded::into_inner);

            pub use inner::service;
        }
    };
}

cbor_entry!("ciborium", ciborium_entry, codecs::backend::ciborium::Ciborium);
cbor_entry!(
    "minicbor-serde",
    minicbor_serde_entry,
    codecs::backend::minicbor_serde::MinicborSerde
);
cbor_entry!("cbor4ii", cbor4ii_entry, codecs::backend::cbor4ii::Cbor4ii);
cbor_entry!("cbor2", cbor2_entry, codecs::backend::cbor2::Cbor2);
cbor_entry!("c2pa_cbor", c2pa_cbor_entry, codecs::backend::c2pa_cbor::C2paCbor);

/// A floor's media type marker.
macro_rules! marker {
    ($name:ident, $media:literal) => {
        /// A `Binary` body under this format's media type.
        #[derive(Debug)]
        pub struct $name;

        impl MediaType for $name {
            const MEDIA_TYPE: &'static str = $media;
        }
    };
}

marker!(JsonMedia, "application/json");
marker!(CborMedia, "application/cbor");
marker!(ProtobufMedia, "application/protobuf");

/// The floor's pre-encoded responses, in the format each floor serves.
static RESPONSES: LazyLock<[(Bytes, Bytes); 3]> = LazyLock::new(|| {
    [Format::Json, Format::Cbor, Format::Protobuf].map(|format| {
        (
            Bytes::from(JsonLarge::bytes(format)),
            Bytes::from(JsonSmall::bytes(format)),
        )
    })
});

/// Declares a floor module for one format.
macro_rules! floor_entry {
    ($module:ident, $media:ty, $index:literal) => {
        mod $module {
            use super::*;

            type Body = Binary<$media>;

            #[kynos::post("/echo")]
            async fn echo(body: Body) -> Body {
                let bytes = body.into_inner();
                std::hint::black_box(checksum(&bytes));
                Binary::new(bytes)
            }

            #[kynos::post("/json/large")]
            async fn post_large(body: Body) -> NoContent {
                drop(std::hint::black_box(body));
                NoContent
            }

            #[kynos::get("/json/large")]
            async fn get_large() -> Body {
                Binary::new(RESPONSES[$index].0.clone())
            }

            #[kynos::get("/json/small")]
            async fn get_small() -> Body {
                Binary::new(RESPONSES[$index].1.clone())
            }

            pub fn service() -> kynos::Result<Service<()>> {
                Router::<()>::new()
                    .mount(kynos::routes![echo, post_large, get_large, get_small])
                    .build(())
            }
        }
    };
}

floor_entry!(floor_json, JsonMedia, 0);
floor_entry!(floor_cbor, CborMedia, 1);
floor_entry!(floor_protobuf, ProtobufMedia, 2);

/// Every entry this build serves, floors last.
#[must_use]
#[allow(clippy::vec_init_then_push)]
pub fn compiled() -> Vec<&'static str> {
    let mut names = vec!["serde_json", "prost"];
    #[cfg(feature = "ciborium")]
    names.push("ciborium");
    #[cfg(feature = "minicbor-serde")]
    names.push("minicbor-serde");
    #[cfg(feature = "cbor4ii")]
    names.push("cbor4ii");
    #[cfg(feature = "cbor2")]
    names.push("cbor2");
    #[cfg(feature = "c2pa_cbor")]
    names.push("c2pa_cbor");
    names.extend(["floor-json", "floor-cbor", "floor-protobuf"]);
    names
}

/// The format `entry` reads and writes, if this build has it.
#[must_use]
pub fn format(entry: &str) -> Option<Format> {
    if !compiled().contains(&entry) {
        return None;
    }
    Some(match entry {
        "serde_json" | "floor-json" => Format::Json,
        "prost" | "floor-protobuf" => Format::Protobuf,
        _ => Format::Cbor,
    })
}

/// The service for `entry`, if this build has it.
///
/// The inner error is Kynos refusing to build the router, a bug here.
#[must_use]
pub fn service(entry: &str) -> Option<kynos::Result<Service<()>>> {
    Some(match entry {
        "serde_json" => serde_json_entry::service(),
        "prost" => prost_entry::service(),
        #[cfg(feature = "ciborium")]
        "ciborium" => ciborium_entry::service(),
        #[cfg(feature = "minicbor-serde")]
        "minicbor-serde" => minicbor_serde_entry::service(),
        #[cfg(feature = "cbor4ii")]
        "cbor4ii" => cbor4ii_entry::service(),
        #[cfg(feature = "cbor2")]
        "cbor2" => cbor2_entry::service(),
        #[cfg(feature = "c2pa_cbor")]
        "c2pa_cbor" => c2pa_cbor_entry::service(),
        "floor-json" => floor_json::service(),
        "floor-cbor" => floor_cbor::service(),
        "floor-protobuf" => floor_protobuf::service(),
        _ => return None,
    })
}
