//! Section `wellformed`: bodies RFC 8949 section 5.1 calls ill-formed, which
//! a decoder must refuse, and a server must answer 400.
//!
//! Each is decoded into `IgnoredAny` (the path that skips, where a lazy
//! decoder could accept without looking) and into `serde_cbor::Value` (the
//! path that reads everything, into a tree that holds any well-formed body,
//! integer keys and byte strings included, so the only fault a body has is
//! its form), through both arrivals. Accepting one, or refusing it with 422,
//! gates. The bodies are the harness's own and every example of RFC 8949
//! Appendix F.1, transcribed and checked against the RFC text. Trailing
//! bytes are the one case a library may leave to its caller by API design
//! ("decode the first item"); they gate too, because a body under
//! `application/cbor` is one item, and whether the library's entry point
//! checks is recorded beside it.

use codecs::Backend;
use serde_json::json;

use crate::cases::hex;
use crate::outcome::{ARRIVALS, AnyCbor, Ignored, Outcome, owned};
use crate::record::Section;

/// Ill-formed bodies, each with what makes it so.
pub const CASES: &[(&str, &str)] = &[
    ("", "empty body"),
    ("18", "truncated argument"),
    ("1b 0000", "truncated 8-byte argument"),
    ("62 61", "text shorter than declared"),
    ("44 0102", "bytes shorter than declared"),
    ("82 01", "array shorter than declared"),
    ("a1 61 61", "map missing a value"),
    ("1c", "additional information 28"),
    ("3d", "additional information 29 on a negative integer"),
    ("5e", "additional information 30 on a byte string"),
    ("fe", "additional information 30 on a simple value"),
    ("ff", "break outside an indefinite item"),
    ("82 01 ff", "break inside a definite array"),
    ("1f", "indefinite length on an integer"),
    ("3f", "indefinite length on a negative integer"),
    ("df 01", "indefinite length on a tag"),
    ("7f 01 ff", "integer chunk in an indefinite text string"),
    ("7f 41 00 ff", "byte-string chunk in an indefinite text string"),
    ("5f 61 00 ff", "text chunk in an indefinite byte string"),
    ("7f 7f ff ff", "nested indefinite chunk"),
    ("7f 61 61", "indefinite text string with no break"),
    ("9f 01", "indefinite array with no break"),
    ("bf 61 61 ff", "indefinite map with a key and no value"),
    ("f8 18", "two-byte simple value 24"),
    ("f8 1f", "two-byte simple value 31"),
    ("01 01", "a second item after the body"),
    ("a0 00", "bytes after an empty map"),
];

/// RFC 8949 Appendix F.1, every example in order: each list as the RFC
/// prints it, under the harness's name for it. Checked against the vendored
/// RFC text by [`appendix_f1`] (`tests::the_transcription_is_appendix_f1`).
pub const F1: &[(&str, &[&str])] = &[
    (
        "end of input in a head",
        &[
            "18",
            "19",
            "1a",
            "1b",
            "19 01",
            "1a 01 02",
            "1b 01 02 03 04 05 06 07",
            "38",
            "58",
            "78",
            "98",
            "9a 01 ff 00",
            "b8",
            "d8",
            "f8",
            "f9 00",
            "fa 00 00",
            "fb 00 00 00",
        ],
    ),
    (
        "definite-length string with short data",
        &[
            "41",
            "61",
            "5a ff ff ff ff 00",
            "5b ff ff ff ff ff ff ff ff 01 02 03",
            "7a ff ff ff ff 00",
            "7b 7f ff ff ff ff ff ff ff 01 02 03",
        ],
    ),
    (
        "definite-length map or array without enough items",
        &[
            "81",
            "81 81 81 81 81 81 81 81 81",
            "82 00",
            "a1",
            "a2 01 02",
            "a1 00",
            "a2 00 00 00",
        ],
    ),
    ("tag number without tag content", &["c0"]),
    ("indefinite-length string without a break", &["5f 41 00", "7f 61 00"]),
    (
        "indefinite-length map or array without a break",
        &[
            "9f",
            "9f 01 02",
            "bf",
            "bf 01 02 01 02",
            "81 9f",
            "9f 80 00",
            "9f 9f 9f 9f 9f ff ff ff ff",
            "9f 81 9f 81 9f 9f ff ff ff",
        ],
    ),
    (
        "reserved additional information",
        &[
            "1c", "1d", "1e", "3c", "3d", "3e", "5c", "5d", "5e", "7c", "7d", "7e", "9c", "9d", "9e", "bc", "bd", "be",
            "dc", "dd", "de", "fc", "fd", "fe",
        ],
    ),
    ("reserved two-byte simple value", &["f8 00", "f8 01", "f8 18", "f8 1f"]),
    (
        "indefinite-length string chunk of another type",
        &[
            "5f 00 ff",
            "5f 21 ff",
            "5f 61 00 ff",
            "5f 80 ff",
            "5f a0 ff",
            "5f c0 00 ff",
            "5f e0 ff",
            "7f 41 00 ff",
        ],
    ),
    (
        "indefinite-length string chunk of indefinite length",
        &["5f 5f 41 00 ff ff", "7f 7f 61 00 ff ff"],
    ),
    ("break outside an indefinite-length item", &["ff"]),
    (
        "break in a definite-length array, map or tag",
        &[
            "81 ff",
            "82 00 ff",
            "a1 ff",
            "a1 ff 00",
            "a1 00 ff",
            "a2 00 00 ff",
            "9f 81 ff",
            "9f 82 9f 81 9f 9f ff ff ff ff",
        ],
    ),
    ("break in a map's value position", &["bf 00 ff", "bf 00 00 00 ff"]),
    ("additional information 31 on major type 0, 1 or 6", &["1f", "3f", "df"]),
];

/// Appendix F.1's lists as the RFC text prints them, in order: each starts
/// after the `:  ` ending its label, runs to the next blank line, and is
/// split at commas.
///
/// # Errors
/// If the text has no Appendix F.1.
pub fn appendix_f1(rfc: &str) -> Result<Vec<Vec<String>>, String> {
    let start = rfc
        .find("\nF.1.  Examples of CBOR Data Items That Are Not Well-Formed\n")
        .ok_or("no Appendix F.1")?;
    let end = rfc[start..].find("\nAppendix G.").ok_or("no Appendix G after F.1")? + start;
    let mut lists = Vec::new();
    let mut current: Option<String> = None;
    for line in rfc[start..end].lines() {
        if line.trim().is_empty() {
            lists.extend(current.take());
        } else if let Some(text) = current.as_mut() {
            text.push(' ');
            text.push_str(line.trim());
        } else if let Some((_, rest)) = line.split_once(":  ") {
            current = Some(rest.trim().to_owned());
        }
    }
    lists.extend(current);
    Ok(lists
        .into_iter()
        .map(|list| {
            list.split(',')
                .map(|item| item.split_whitespace().collect::<Vec<_>>().join(" "))
                .filter(|item| !item.is_empty())
                .collect()
        })
        .collect())
}

/// Run the section: the harness's cases, then Appendix F.1's.
#[must_use]
pub fn run<B: Backend>() -> Section {
    let mut section = Section::new("wellformed");
    let mut trailing = Vec::new();
    let f1: Vec<(String, &str)> = F1
        .iter()
        .flat_map(|(group, items)| items.iter().map(move |text| (format!("F.1 {group}"), *text)))
        .collect();
    let cases = CASES
        .iter()
        .map(|(text, what)| (*text, (*what).to_owned()))
        .chain(f1.iter().map(|(what, text)| (*text, what.clone())));
    for (text, what) in cases {
        let what = what.as_str();
        let bytes = hex(text);
        for (target, outcome) in [
            ("ignored", owned::<B, Ignored> as crate::outcome::Decode),
            ("value", owned::<B, AnyCbor>),
        ]
        .map(|(name, decode)| (name, ARRIVALS.map(|arrival| decode(&bytes, arrival))))
        {
            for (outcome, arrival) in outcome.iter().zip(ARRIVALS) {
                let case = format!("{what} ({text})#{target}#{}", arrival.name());
                section.case();
                match outcome {
                    Outcome::Reject(failure) if failure.status() == 400 => {}
                    other => {
                        section.differ(&case, json!("refused, 400"), other.describe());
                        let detail = match other {
                            Outcome::Accept(_) => "an ill-formed body accepted",
                            Outcome::Reject(_) => "an ill-formed body answered 422, not 400",
                            Outcome::Panic(_) => "panicked",
                        };
                        section.fail(&case, detail);
                    }
                }
                if what.contains("after") && arrival.name() == "unique" {
                    trailing.push(json!({ "case": text, "target": target, "outcome": outcome.describe() }));
                }
            }
        }
    }
    section.data("trailing", json!({ "library": B::TRAILING, "outcomes": trailing }));
    section
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every list of Appendix F.1, item by item, in the RFC's order. The
    /// RFC text is the vendored one, whose digest `appendix::load` checks.
    #[test]
    fn the_transcription_is_appendix_f1() {
        crate::appendix::load(&crate::appendix::default_dir()).expect("the vendored RFC verifies");
        let rfc = std::fs::read_to_string(crate::appendix::default_dir().join("rfc8949.txt")).expect("the RFC");
        let printed = appendix_f1(&rfc).expect("Appendix F.1");
        let transcribed: Vec<Vec<String>> = F1
            .iter()
            .map(|(_, items)| items.iter().map(|item| (*item).to_owned()).collect())
            .collect();
        assert_eq!(printed, transcribed);
        assert_eq!(transcribed.iter().map(Vec::len).sum::<usize>(), 94);
    }

    /// Every example is ill-formed by the harness's own walker, which shares
    /// no code with any library, and the oracle refuses each with 400.
    #[test]
    fn every_case_is_ill_formed_and_the_oracle_refuses_it() {
        for (_, items) in F1 {
            for text in *items {
                assert!(faithful::well_formed(&hex(text)).is_err(), "{text}");
            }
        }
        for (text, _) in CASES {
            assert!(faithful::well_formed(&hex(text)).is_err(), "{text}");
        }
        let section = run::<codecs::backend::serde_cbor::SerdeCbor>();
        let failures: Vec<_> = section.failures().to_vec();
        assert!(section.cases() > 400, "{}", section.cases());
        assert!(failures.is_empty(), "{failures:#?}");
    }
}
