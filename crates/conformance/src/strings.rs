//! Section `strings`: text strings must be UTF-8 (RFC 8949 section 5.3.1:
//! invalid UTF-8 makes the item invalid), into `String`, `&str` and `Cow`,
//! definite and indefinite, and in a member a struct skips. Accepting one
//! gates; so does refusing it with 422, since an invalid item is the
//! client's syntax.

use std::borrow::Cow;

use codecs::Backend;
use serde::Deserialize;
use serde_json::json;

use crate::cases::hex;
use crate::outcome::{
    ARRIVALS, Decode, OnBorrowed, Outcome, Str, Target, Typed, borrowed, borrows, on_borrowed, owned,
};
use crate::record::Section;

/// A `Cow<str>` field that borrows where it can.
#[derive(Debug, Deserialize)]
pub struct CowText<'a> {
    /// The text.
    #[serde(borrow)]
    pub t: Cow<'a, str>,
}

/// [`CowText`] as a decode target.
#[derive(Debug)]
pub struct AsCow;

impl Target for AsCow {
    type Out<'de> = CowText<'de>;
    fn view(out: &CowText<'_>) -> serde_json::Value {
        json!(out.t)
    }
}

/// A struct that declares one member: the other is skipped.
#[derive(Debug, serde::Serialize, Deserialize)]
pub struct One {
    /// The declared member.
    pub a: u8,
}

/// Invalid text, each in the shape it is decoded from.
const INVALID: &[(&str, &str, &str)] = &[
    ("lone continuation byte", "61 80", "text"),
    ("truncated sequence", "62 c3 28", "text"),
    ("overlong encoding", "62 c0 af", "text"),
    ("surrogate code point", "63 ed a0 80", "text"),
    ("beyond U+10FFFF", "64 f4 90 80 80", "text"),
    ("invalid in a chunk", "7f 61 61 61 ff ff", "text"),
    ("split sequence across chunks", "7f 61 c3 61 a9 ff", "text"),
    ("in a member the struct skips", "a2 61 61 01 61 62 61 ff", "struct"),
    ("in a skipped member's key", "a2 61 61 01 61 ff 01", "struct"),
];

/// Run the section.
///
/// The borrowing targets (`&str`, and a `Cow` field, which a library that
/// does not borrow fills owned) are judged by [`on_borrowed`]: a library
/// that cannot produce one from valid text of the same shape does not
/// borrow it, and its refusals of that target are recorded as not borrowed,
/// neither passed nor gated. The owned targets gate every case.
#[must_use]
pub fn run<B: Backend>() -> Section {
    let mut section = Section::new("strings");
    let str_borrows = borrows::<B, Str>(&hex("61 61"));
    let cow_borrows = borrows::<B, AsCow>(&hex("a1 61 74 61 61"));
    section.data("borrows", json!({ "&str": str_borrows, "Cow": cow_borrows }));
    for (what, text, shape) in INVALID {
        let bytes = hex(text);
        let wrapped = [hex("a1 61 74"), bytes.clone()].concat();
        // Each path, and whether it is a borrowing target the library
        // borrows (`None` for an owned target).
        let paths: Vec<(&str, Decode, Vec<u8>, Option<bool>)> = if *shape == "struct" {
            vec![("struct", owned::<B, Typed<One>>, bytes.clone(), None)]
        } else {
            vec![
                ("String", owned::<B, Typed<String>>, bytes.clone(), None),
                ("&str", borrowed::<B, Str>, bytes.clone(), Some(str_borrows)),
                ("Cow", borrowed::<B, AsCow>, wrapped, Some(cow_borrows)),
            ]
        };
        for (target, decode, input, borrowing) in paths {
            for arrival in ARRIVALS {
                let case = format!("{what} ({text})#{target}#{}", arrival.name());
                let outcome = decode(&input, arrival);
                let right = matches!(&outcome, Outcome::Reject(failure) if failure.status() == 400);
                let judged = borrowing.map_or(if right { OnBorrowed::Right } else { OnBorrowed::Wrong }, |borrows| {
                    on_borrowed(&outcome, borrows, right)
                });
                match judged {
                    OnBorrowed::NotBorrowed => section.not_borrowed(&case, outcome.describe()),
                    OnBorrowed::Right => section.case(),
                    OnBorrowed::Wrong => {
                        section.case();
                        section.differ(&case, json!("refused, 400"), outcome.describe());
                        section.fail(
                            &case,
                            match outcome {
                                Outcome::Accept(_) => "invalid UTF-8 accepted",
                                Outcome::Reject(_) => "invalid UTF-8 answered 422, not 400",
                                Outcome::Panic(_) => "panicked",
                            },
                        );
                    }
                }
            }
        }
    }
    // Valid text still decodes, borrowed where the library borrows.
    for (target, decode, borrowing) in [
        ("&str", borrowed::<B, Str> as Decode, Some(str_borrows)),
        ("String", owned::<B, Typed<String>>, None),
    ] {
        let case = format!("valid two-byte character#{target}");
        let outcome = decode(&hex("62 c3 bc"), ARRIVALS[0]);
        let right = matches!(&outcome, Outcome::Accept(value) if value == "\u{fc}");
        let judged = borrowing.map_or(if right { OnBorrowed::Right } else { OnBorrowed::Wrong }, |borrows| {
            on_borrowed(&outcome, borrows, right)
        });
        match judged {
            OnBorrowed::NotBorrowed => section.not_borrowed(&case, outcome.describe()),
            OnBorrowed::Right => section.case(),
            OnBorrowed::Wrong => {
                section.case();
                section.differ(&case, json!("\u{fc}"), outcome.describe());
                section.fail(&case, "valid text not decoded");
            }
        }
    }
    section
}
