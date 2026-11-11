//! A Kynos service that reads and writes CBOR through the worked codec.
//!
//! `POST /items` takes an item as `application/cbor` and answers with it in
//! the same format; `GET /items/{id}` offers JSON and CBOR and serves what the
//! client's `Accept` prefers. The OpenAPI document it prints states the item's
//! schema under both media types, and the codec keeps the CBOR side true to
//! it: a byte string, a tag, `undefined`, NaN or a non-text key in a request
//! is a 422, ill-formed CBOR a 400.
//!
//! ```text
//! cargo run -p kynos-cbor --features ciborium --example serve
//! ```

use std::net::Ipv4Addr;

use kynos::{
    error::rejection::NegotiationRejection,
    extract::body::codec::Encoded,
    prelude::*,
    response::negotiate::{Accept, Negotiated},
};
use kynos_cbor::CiboriumCbor;
use serde::{Deserialize, Serialize};

/// A catalogue record: the `json-small` shape of kynos-bench.
#[derive(Clone, Schema, Serialize, Deserialize)]
struct Item {
    id: u64,
    name: String,
    tags: Vec<String>,
    active: bool,
    score: f64,
}

/// What `/items/{id}` captures.
#[allow(dead_code)]
#[derive(Schema, PathParams)]
struct ItemPath {
    id: u64,
}

/// The two ways an item is served, JSON first when `Accept` ranks them equal.
type Formats = (Json<Item>, Encoded<CiboriumCbor, Item>);

#[kynos::post("/items")]
async fn create_item(item: Encoded<CiboriumCbor, Item>) -> Encoded<CiboriumCbor, Item> {
    item
}

#[kynos::get("/items/{id}")]
async fn get_item(
    Path(path): Path<ItemPath>,
    accept: Accept<Formats>,
) -> Result<Negotiated<Formats>, NegotiationRejection> {
    let item = Item {
        id: path.id,
        name: format!("item-{:06}", path.id),
        tags: vec!["tag-1".into()],
        active: true,
        score: 1.0,
    };
    accept.respond_with(
        &item,
        (
            |item: &Item| Json(item.clone()),
            |item: &Item| Encoded::new(item.clone()),
        ),
    )
}

#[tokio::main]
async fn main() -> kynos::Result<()> {
    let router = Router::<()>::new().mount(kynos::routes![create_item, get_item]);
    println!("{}", router.openapi()?.to_json()?);
    Server::new(router.build(())?)
        .bind((Ipv4Addr::LOCALHOST, 3000))
        .serve()
        .await
}
