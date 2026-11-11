//! Section `status`: every truncation of the three Kynos bodies must be
//! refused with 400, and `json-large` with one `id` written as text with
//! 422, through both arrivals, owned and borrowed.

use codecs::{Backend, Format};
use payloads::Workload;
use payloads::kynos::{EchoPost, JsonLarge, JsonSmall};
use serde_json::json;

use crate::outcome::{ARRIVALS, BorrowedForm, Outcome, Typed, borrowed, owned};
use crate::record::Section;

fn truncations<B: Backend, W: Workload>(section: &mut Section) {
    let bytes = W::bytes(Format::Cbor);
    let mut wrong = 0;
    for len in 0..bytes.len() {
        let cut = &bytes[..len];
        for arrival in ARRIVALS {
            for (path, outcome) in [
                ("owned", owned::<B, Typed<W::Owned>>(cut, arrival)),
                ("borrowed", borrowed::<B, BorrowedForm<W>>(cut, arrival)),
            ] {
                section.case();
                match outcome {
                    Outcome::Reject(failure) if failure.status() == 400 => {}
                    Outcome::Reject(failure) if failure.class == codecs::Class::Unsupported => {}
                    other => {
                        wrong += 1;
                        let case = format!("{} cut at {len}#{path}#{}", W::NAME, arrival.name());
                        section.differ(&case, json!("refused, 400"), other.describe());
                        section.fail(&case, "a truncated body not refused with 400");
                    }
                }
            }
        }
    }
    section.data(W::NAME, json!({ "truncations": bytes.len(), "wrong": wrong }));
}

/// Run the section.
#[must_use]
pub fn run<B: Backend>() -> Section {
    let mut section = Section::new("status");
    truncations::<B, JsonSmall>(&mut section);
    truncations::<B, EchoPost>(&mut section);
    truncations::<B, JsonLarge>(&mut section);
    let shape = payloads::reject::RejectShape::bytes(Format::Cbor);
    for arrival in ARRIVALS {
        for (path, outcome) in [
            ("owned", owned::<B, Typed<payloads::kynos::Catalog>>(&shape, arrival)),
            (
                "borrowed",
                borrowed::<B, BorrowedForm<payloads::reject::RejectShape>>(&shape, arrival),
            ),
        ] {
            section.case();
            let case = format!("an id as text#{path}#{}", arrival.name());
            match outcome {
                Outcome::Reject(failure) if failure.status() == 422 => {}
                Outcome::Reject(failure) if failure.class == codecs::Class::Unsupported => {}
                other => {
                    section.differ(&case, json!("refused, 422"), other.describe());
                    section.fail(&case, "a wrong shape not refused with 422");
                }
            }
        }
    }
    section
}
