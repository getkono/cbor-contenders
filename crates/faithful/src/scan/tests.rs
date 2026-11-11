//! One test per rule, on hand-built bytes, and the cases the profile must
//! accept.

use super::*;

fn hex(text: &str) -> Vec<u8> {
    let text: String = text.split_whitespace().collect();
    (0..text.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&text[i..i + 2], 16).expect("hex"))
        .collect()
}

fn refused(text: &str) -> Rule {
    scan(&hex(text)).expect_err(text).rule
}

#[test]
fn ill_formed_input_is_400() {
    for (bytes, rule) in [
        ("", Rule::Truncated),
        ("18", Rule::Truncated),
        ("62 61", Rule::Truncated),
        ("82 01", Rule::Truncated),
        ("a1 61 61", Rule::Truncated),
        ("1c", Rule::Reserved),
        ("3d", Rule::Reserved),
        ("fe", Rule::Reserved),
        ("ff", Rule::StrayBreak),
        ("82 01 ff", Rule::StrayBreak),
        ("1f", Rule::IndefiniteArgument),
        ("df 01", Rule::IndefiniteArgument),
        ("7f 01 ff", Rule::BadChunk),
        ("7f 7f ff ff", Rule::BadChunk),
        ("7f 41 00 ff", Rule::BadChunk),
        ("f8 18", Rule::SimpleTooShort),
        ("f8 1f", Rule::SimpleTooShort),
        ("01 01", Rule::Trailing),
        ("61 ff", Rule::InvalidUtf8),
        ("7f 61 c3 61 a9 ff", Rule::InvalidUtf8),
        ("bf 61 61 ff", Rule::StrayBreak),
    ] {
        let rule_got = refused(bytes);
        assert_eq!(rule_got, rule, "{bytes}");
        assert!(rule.is_syntax(), "{bytes}");
    }
}

#[test]
fn declared_lengths_past_the_body_are_refused_without_reading_them() {
    for bytes in [
        "5b ffffffffffffffff",
        "7b 7fffffffffffffff",
        "9b 00000001 00000000",
        "bb 7fffffffffffffff",
        "bb ffffffffffffffff",
    ] {
        let rule = refused(bytes);
        assert!(rule == Rule::Truncated || rule == Rule::ByteString, "{bytes}: {rule:?}");
    }
}

#[test]
fn values_without_a_json_equivalent_are_422() {
    for (bytes, rule) in [
        ("40", Rule::ByteString),
        ("5f 41 00 ff", Rule::ByteString),
        ("a1 61 61 41 00", Rule::ByteString),
        ("c0 74 323031332d30332d32315432303a30343a30305a", Rule::Tag),
        ("c1 1a 514b67b0", Rule::Tag),
        ("c2 49 010000000000000000", Rule::Tag),
        ("c3 49 010000000000000000", Rule::Tag),
        ("c4 82 21 19 6ab3", Rule::Tag),
        ("d9 d9f7 01", Rule::Tag),
        ("f7", Rule::Simple),
        ("f0", Rule::Simple),
        ("f8 ff", Rule::Simple),
        ("f9 7c00", Rule::NonFinite),
        ("f9 fc00", Rule::NonFinite),
        ("f9 7e00", Rule::NonFinite),
        ("fa 7fc00000", Rule::NonFinite),
        ("fb 7ff8000000000001", Rule::NonFinite),
        ("a1 01 02", Rule::NonTextKey),
        ("a1 f6 02", Rule::NonTextKey),
        ("a2 61 61 01 61 61 02", Rule::DuplicateKey),
        ("bf 61 61 01 7f 61 61 ff 02 ff", Rule::DuplicateKey),
    ] {
        let rule_got = refused(bytes);
        assert_eq!(rule_got, rule, "{bytes}");
        assert!(!rule.is_syntax(), "{bytes}");
    }
}

#[test]
fn transparent_encodings_are_accepted() {
    for bytes in [
        // Indefinite text, array and map.
        "7f 62 6869 62 6869 ff",
        "9f 01 02 ff",
        "bf 61 61 01 61 62 9f ff ff",
        // Non-preferred heads and floats.
        "19 0001",
        "1b 0000000000000001",
        "fb 3ff8000000000000",
        "f9 3e00",
        // Every 64-bit integer.
        "1b ffffffffffffffff",
        "3b ffffffffffffffff",
        // false, true, null, and nesting.
        "83 f4 f5 f6",
        "a1 61 61 a1 61 62 80",
        // -0 is a JSON number.
        "f9 8000",
    ] {
        assert_eq!(scan(&hex(bytes)), Ok(()), "{bytes}");
    }
}

#[test]
fn nesting_is_bounded_without_recursion() {
    let ok: Vec<u8> = std::iter::repeat_n(0x81, MAX_DEPTH).chain([0x00]).collect();
    assert_eq!(scan(&ok), Ok(()));
    let deep: Vec<u8> = std::iter::repeat_n(0x81, MAX_DEPTH + 1).chain([0x00]).collect();
    assert_eq!(scan(&deep).expect_err("too deep").rule, Rule::TooDeep);
    // A million levels is refused at the limit, on a small stack.
    let huge: Vec<u8> = std::iter::repeat_n(0x9f, 1_000_000).collect();
    let rule = std::thread::Builder::new()
        .stack_size(64 * 1024)
        .spawn(move || scan(&huge).map_err(|r| r.rule))
        .expect("spawn")
        .join()
        .expect("no overflow");
    assert_eq!(rule, Err(Rule::TooDeep));
}

#[test]
fn preferred_serialization_is_recognised() {
    for bytes in [
        "00",
        "17",
        "18 18",
        "19 0100",
        "f9 3e00",
        "fa 47c35000",
        "fb 3ff199999999999a",
        "82 01 02",
        "c1 1a 514b67b0",
        "40",
    ] {
        assert!(preferred(&hex(bytes)), "{bytes}");
    }
    for bytes in [
        "18 17",
        "19 00ff",
        "1a 0000ffff",
        "1b 00000000ffffffff",
        "fa 3fc00000",
        "fb 3ff8000000000000",
        "9f ff",
        "5f ff",
        "fb 7ff8000000000000",
    ] {
        assert!(!preferred(&hex(bytes)), "{bytes}");
    }
}

#[test]
fn refusals_name_no_input() {
    let refusal = scan(&hex("61 ff")).expect_err("invalid");
    assert_eq!(refusal.to_string(), "a CBOR text string is not UTF-8 (at byte 0)");
    assert_eq!(refusal.status(), 400);
    assert_eq!(scan(&hex("40")).expect_err("bytes").status(), 422);
}

#[test]
fn well_formed_accepts_what_the_profile_refuses_but_not_ill_formed_input() {
    for bytes in [
        "40",
        "5f 41 00 ff",
        "c1 1a 514b67b0",
        "f7",
        "f9 7e00",
        "a1 01 02",
        "a2 61 61 01 61 61 02",
    ] {
        assert_eq!(well_formed(&hex(bytes)), Ok(()), "{bytes}");
    }
    let deep: Vec<u8> = std::iter::repeat_n(0x81, 10_000).chain([0x00]).collect();
    assert_eq!(well_formed(&deep), Ok(()));
    for bytes in ["", "1c", "ff", "7f 01 ff", "f8 18", "01 01", "61 ff", "5f 61 00 ff"] {
        assert!(well_formed(&hex(bytes)).is_err(), "{bytes}");
    }
}
