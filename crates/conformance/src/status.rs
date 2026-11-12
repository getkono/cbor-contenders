//! Section `status`: every truncation of the three Kynos bodies must be
//! refused with 400, and `json-large` with one `id` written as text with
//! 422, through both arrivals, owned and borrowed. The borrowed form is
//! judged by [`on_borrowed`]: a library that refuses the borrowed form of the
//! whole valid body does not borrow it, and its refusals of the damaged
//! bodies are recorded as not borrowed.

use codecs::{Backend, Format};
use payloads::Workload;
use payloads::kynos::{EchoPost, JsonLarge, JsonSmall};
use serde_json::json;

use crate::outcome::{ARRIVALS, BorrowedForm, OnBorrowed, Outcome, Typed, borrowed, borrows, on_borrowed, owned};
use crate::record::Section;

fn truncations<B: Backend, W: Workload>(section: &mut Section) {
    let bytes = W::bytes(Format::Cbor);
    let borrowing = borrows::<B, BorrowedForm<W>>(&bytes);
    let mut wrong = 0;
    for len in 0..bytes.len() {
        let cut = &bytes[..len];
        for arrival in ARRIVALS {
            for (path, outcome) in [
                ("owned", owned::<B, Typed<W::Owned>>(cut, arrival)),
                ("borrowed", borrowed::<B, BorrowedForm<W>>(cut, arrival)),
            ] {
                let case = format!("{} cut at {len}#{path}#{}", W::NAME, arrival.name());
                let right = matches!(&outcome, Outcome::Reject(failure) if failure.status() == 400);
                match judge(path, &outcome, borrowing, right) {
                    OnBorrowed::Right => section.case(),
                    OnBorrowed::NotBorrowed => section.not_borrowed(&case, outcome.describe()),
                    OnBorrowed::Wrong => {
                        section.case();
                        wrong += 1;
                        section.differ(&case, json!("refused, 400"), outcome.describe());
                        section.fail(&case, "a truncated body not refused with 400");
                    }
                }
            }
        }
    }
    section.data(
        W::NAME,
        json!({ "truncations": bytes.len(), "wrong": wrong, "borrowed_form_borrows": borrowing }),
    );
}

/// The owned path gates every case; the borrowed one by [`on_borrowed`].
fn judge(path: &str, outcome: &Outcome, borrows: bool, right: bool) -> OnBorrowed {
    match path {
        "borrowed" => on_borrowed(outcome, borrows, right),
        _ if right => OnBorrowed::Right,
        _ => OnBorrowed::Wrong,
    }
}

/// Run the section.
#[must_use]
pub fn run<B: Backend>() -> Section {
    let mut section = Section::new("status");
    truncations::<B, JsonSmall>(&mut section);
    truncations::<B, EchoPost>(&mut section);
    truncations::<B, JsonLarge>(&mut section);
    wrong_shape::<B>(&mut section);
    section
}

/// The section over `json-small` and the wrong shape only: a slice small
/// enough for Miri.
#[must_use]
pub fn small<B: Backend>() -> Section {
    let mut section = Section::new("status");
    truncations::<B, JsonSmall>(&mut section);
    wrong_shape::<B>(&mut section);
    section
}

fn wrong_shape<B: Backend>(section: &mut Section) {
    let shape = payloads::reject::RejectShape::bytes(Format::Cbor);
    let borrowing = borrows::<B, BorrowedForm<JsonLarge>>(&JsonLarge::bytes(Format::Cbor));
    for arrival in ARRIVALS {
        for (path, outcome) in [
            ("owned", owned::<B, Typed<payloads::kynos::Catalog>>(&shape, arrival)),
            (
                "borrowed",
                borrowed::<B, BorrowedForm<payloads::reject::RejectShape>>(&shape, arrival),
            ),
        ] {
            let case = format!("an id as text#{path}#{}", arrival.name());
            let right = matches!(&outcome, Outcome::Reject(failure) if failure.status() == 422);
            match judge(path, &outcome, borrowing, right) {
                OnBorrowed::Right => section.case(),
                OnBorrowed::NotBorrowed => section.not_borrowed(&case, outcome.describe()),
                OnBorrowed::Wrong => {
                    section.case();
                    section.differ(&case, json!("refused, 422"), outcome.describe());
                    section.fail(&case, "a wrong shape not refused with 422");
                }
            }
        }
    }
}
