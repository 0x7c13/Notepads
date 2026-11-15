use ratex_parser::{parse, ParseNode};

fn symbols(input: &str) -> String {
    parse(input)
        .unwrap()
        .iter()
        .map(|node| node.symbol_text().expect("expected a symbol"))
        .collect()
}

fn numeric_forms(value: u128) -> [String; 3] {
    [
        format!(r"\char{value}"),
        format!(r#"\char"{value:X}"#),
        format!(r"\char'{value:o}"),
    ]
}

#[test]
fn oversized_char_literals_return_errors_in_every_base() {
    let mut inputs = vec![
        r"\char9999999999999999999".to_owned(),
        r#"\char"FFFFFFFFFFFFFFFF"#.to_owned(),
        r"\char'7777777777777777777777".to_owned(),
    ];
    for value in [
        i64::MAX as u128 + 1, // Includes overflow in addition, not just multiplication.
        (1u128 << 64) + 65,   // Previously wrapped to 'A' in release builds.
        (1u128 << 64) + 115,  // Previously wrapped to 's'.
    ] {
        inputs.extend(numeric_forms(value));
    }
    for prefix in [r"\char", r#"\char""#, r"\char'"] {
        inputs.push(format!("{prefix}{}", "7".repeat(300)));
    }
    for input in inputs {
        let error = parse(&input).expect_err(&input);
        assert!(
            error.message.contains(r"\char number out of range"),
            "{input}: {error}"
        );
        assert!(parse(r"\char65").is_ok());
    }
}

#[test]
fn char_preserves_valid_codepoints_and_leading_zeroes() {
    for value in [0, 65, 0xD7FF, 0xE000, 0x1F600, 0x10FFFF] {
        for input in numeric_forms(value) {
            let nodes = parse(&input).unwrap();
            assert!(
                matches!(
                    nodes.as_slice(),
                    [ParseNode::TextOrd { text, .. }]
                        if text == &char::from_u32(value as u32).unwrap().to_string()
                ),
                "{input}: {nodes:?}"
            );
        }
    }
    for (prefix, digits) in [(r"\char", "65"), (r#"\char""#, "41"), (r"\char'", "101")] {
        let input = format!("{prefix}{}{digits}", "0".repeat(300));
        assert_eq!(symbols(&input), "A");
    }
}

#[test]
fn char_keeps_unicode_range_checks_and_backtick_syntax() {
    for value in [0xD800, 0xDFFF, 0x110000, i64::MAX as u128] {
        for input in numeric_forms(value) {
            let error = parse(&input).expect_err(&input);
            // Representable i64 values still reach the existing \@char validation.
            assert!(error.message.contains(r"\@char"), "{input}: {error}");
        }
    }
    for input in [r"\char`A+1", r"\char65+1", r#"\char"41+1"#, r"\char'101+1"] {
        assert_eq!(symbols(input), "A+1");
    }
    assert_eq!(symbols(r"\char`\%+1"), "%+1");
}
