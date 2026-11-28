// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

use super::*;
use crate::policy::{CancellationToken, SourceBudget, SourceLimits};

fn parse(source: &str) -> Result<Vec<Entry>, String> {
    let budget = SourceBudget::new(SourceLimits::default()).unwrap();
    let mut op = budget.begin(CancellationToken::new());
    parse_entries(source, &mut op).unwrap()
}
fn text(key: &str, text: &str) -> Entry {
    Entry {
        key: key.into(),
        value: DisplayValue::Text(text.into()),
    }
}
fn code(key: &str, text: &str) -> Entry {
    Entry {
        key: key.into(),
        value: DisplayValue::Code(text.into()),
    }
}

#[test]
fn mapping_scalars_and_nulls() {
    assert_eq!(
        parse("title: Hello\ncount: 3\nflag: true").unwrap(),
        vec![
            text("title", "Hello"),
            text("count", "3"),
            text("flag", "true")
        ]
    );
    assert_eq!(
        parse("a: null\nb: ~\nc:\nd: []\ne: {}").unwrap(),
        vec![
            text("a", ""),
            text("b", ""),
            text("c", ""),
            Entry {
                key: "d".into(),
                value: DisplayValue::List(vec![])
            },
            code("e", "{}")
        ]
    );
    for empty in [
        "",
        "# comment",
        "null",
        "~",
        "!!set {a: null}",
        "!!timestamp 2001-12-15",
        "!!omap [{a: b}]",
    ] {
        assert_eq!(parse(empty).unwrap(), vec![], "{empty}");
    }
    for (source, result) in [("Hello", "Hello"), ("false", "false"), ("3.14", "3.14")] {
        assert_eq!(parse(source).unwrap(), vec![text("", result)]);
    }
}

#[test]
fn nested_lists_preserve_hierarchy_and_objects_use_code() {
    assert_eq!(
        parse("meta:\n  x: 1\n  y: yes\nitems:\n  - a\n  - [b, c]\n  - {d: e}").unwrap(),
        vec![
            code("meta", "x: 1\ny: yes"),
            Entry {
                key: "items".into(),
                value: DisplayValue::List(vec![
                    DisplayValue::Text("a".into()),
                    DisplayValue::List(vec![
                        DisplayValue::Text("b".into()),
                        DisplayValue::Text("c".into())
                    ]),
                    DisplayValue::Code("d: e".into())
                ])
            }
        ]
    );
}

#[test]
fn core_schema_numbers_have_js_precision_and_display() {
    let entries = parse("yes: yes\nno: no\non: on\noff: off\nTrue: True\nFalse: FALSE\nleading: 012\nhex: 0x10\noctal: 0o10\ninf: .inf\nnan: .NaN\nnegativezero: -0\nlarge: 9007199254740993").unwrap();
    assert_eq!(
        entries,
        vec![
            text("yes", "yes"),
            text("no", "no"),
            text("on", "on"),
            text("off", "off"),
            text("true", "true"),
            text("false", "false"),
            text("leading", "12"),
            text("hex", "16"),
            text("octal", "8"),
            text("inf", "Infinity"),
            text("nan", "NaN"),
            text("negativezero", "0"),
            text("large", "9007199254740992")
        ]
    );
    assert_eq!(
        parse("a: 1e+21\nb: 1e-7\nc: 0x20000000000001\nd: -0o10\ne: .iNf").unwrap(),
        vec![
            text("a", "1e+21"),
            text("b", "1e-7"),
            text("c", "9007199254740992"),
            text("d", "-0o10"),
            text("e", ".iNf")
        ]
    );
}

#[test]
fn dates_are_plain_strings_without_explicit_tag() {
    assert_eq!(
        parse("date: 2001-12-15\ntime: 2001-12-15T02:59:43.1Z").unwrap(),
        vec![
            text("date", "2001-12-15"),
            text("time", "2001-12-15T02:59:43.1Z")
        ]
    );
    // Date.toISOString() uses exactly three fractional digits.
    assert_eq!(
        parse("a: !!timestamp 2001-12-15T02:59:43.1Z").unwrap(),
        vec![code("a", "2001-12-15T02:59:43.100Z")]
    );
    assert_eq!(
        parse("a: !!timestamp 2001-12-15T02:00:00+02:30").unwrap(),
        vec![code("a", "2001-12-14T23:30:00.000Z")]
    );
}

#[test]
fn explicit_timestamp_timezone_matches_fixed_yaml_290() {
    for (source, expected) in [
        ("2001-12-15T02:59:43+00:15", "2001-12-14T11:59:43.000Z"),
        ("2001-12-15T02:59:43+00:01", "2001-12-15T01:59:43.000Z"),
        ("2001-12-15T02:59:43-00:20", "2001-12-15T22:59:43.000Z"),
        ("2001-12-15T02:59:43+01:20", "2001-12-15T01:39:43.000Z"),
    ] {
        assert_eq!(
            parse(&format!("a: !!timestamp {source}")).unwrap(),
            vec![code("a", expected)]
        );
    }
    assert!(parse("a: !!timestamp 2001-12-15T02:59:43+01:2").is_err());
}

#[test]
fn explicit_scalar_tags_and_unknown_tags_follow_yaml() {
    assert_eq!(
        parse("a: !!str 123\nb: !custom text\nc: !!int nope\nd: !custom true\ne: !!bool 'true'")
            .unwrap(),
        vec![
            text("a", "123"),
            text("b", "text"),
            text("c", "nope"),
            text("d", "true"),
            text("e", "true")
        ]
    );
    assert_eq!(parse("a: !custom {x: 1}").unwrap(), vec![code("a", "x: 1")]);
    assert!(parse("a: !!timestamp nope").is_err());
}

#[test]
fn binary_is_node_buffer_and_decoding_is_lenient() {
    let expected = "type: Buffer\ndata:\n  - 72\n  - 101\n  - 108\n  - 108\n  - 111";
    assert_eq!(
        parse("a: !!binary SGVsbG8=").unwrap(),
        vec![code("a", expected)]
    );
    assert_eq!(
        parse("a: !!binary SGVs!!bG8").unwrap(),
        vec![code("a", expected)]
    );
    assert_eq!(
        parse("!!binary SGk=").unwrap(),
        vec![text("0", "72"), text("1", "105")]
    );
}

#[test]
fn set_omap_and_pairs_keep_their_collection_tags() {
    assert_eq!(
        parse("a: !!set\n  red:\n  blue:").unwrap(),
        vec![code("a", "- red\n- blue")]
    );
    assert!(parse("a: !!set {red: 1}").is_err());
    assert_eq!(
        parse("a: !!omap [{x: 1}, {y: 2}]").unwrap(),
        vec![code("a", "x: 1\ny: 2")]
    );
    assert!(parse("a: !!omap [{x: 1}, {x: 2}]").is_err());
    assert_eq!(
        parse("a: !!pairs [{x: 1}, {y: 2}]").unwrap(),
        vec![Entry {
            key: "a".into(),
            value: DisplayValue::List(vec![
                DisplayValue::Code("x: 1".into()),
                DisplayValue::Code("y: 2".into())
            ])
        }]
    );
}

#[test]
fn direct_scalar_duplicate_keys_use_typed_values() {
    for source in [
        "a: 1\na: 2",
        "a: 1\n\"a\": 2",
        "1: one\n1.0: two",
        "True: one\ntrue: two",
        "-0: one\n0: two",
    ] {
        assert!(
            parse(source)
                .unwrap_err()
                .starts_with("Map keys must be unique"),
            "{source}"
        );
    }
    assert_eq!(
        parse("1: numeric\n\"1\": string").unwrap(),
        vec![text("1", "string")]
    );
    assert_eq!(
        parse("true: boolean\n\"true\": string").unwrap(),
        vec![text("true", "string")]
    );
    assert_eq!(
        parse("? [a,b]\n: first\n? [a,b]\n: second").unwrap(),
        vec![text("[ a, b ]", "second")]
    );
    assert_eq!(
        parse(".nan: first\n.NaN: second").unwrap(),
        vec![text("NaN", "second")]
    );
}

#[test]
fn duplicate_error_has_source_context() {
    assert_eq!(
        parse("a: 1\na: 2").unwrap_err(),
        "Map keys must be unique at line 2, column 1:\n\na: 1\na: 2\n^\n"
    );
}

#[test]
fn js_integer_keys_sort_before_insertion_order_keys() {
    assert_eq!(
        parse("z: first\n10: ten\n2: two\na: last").unwrap(),
        vec![
            text("2", "two"),
            text("10", "ten"),
            text("z", "first"),
            text("a", "last")
        ]
    );
    assert_eq!(
        parse("\"4294967295\": max\n\"01\": leading\n\"4294967294\": index\n\"0\": zero").unwrap(),
        vec![
            text("0", "zero"),
            text("4294967294", "index"),
            text("4294967295", "max"),
            text("01", "leading")
        ]
    );
    for key in ["+1", "-0", "00", "1 ", " 1", "١", "4294967296", ""] {
        assert_eq!(array_index(key), None, "{key:?}");
    }
    assert_eq!(
        parse("__proto__: value\nconstructor: x\ntoString: y").unwrap(),
        vec![
            text("__proto__", "value"),
            text("constructor", "x"),
            text("toString", "y")
        ]
    );
}

#[test]
fn aliases_preserve_shared_objects_and_merge_is_disabled_by_default() {
    assert_eq!(
        parse("a: &base {x: 1, y: [a,b]}\nb: *base").unwrap(),
        vec![
            code("a", "x: 1\ny:\n  - a\n  - b"),
            code("b", "x: 1\ny:\n  - a\n  - b")
        ]
    );
    assert_eq!(
        parse("defaults: &d {x: 1}\nmerged: {<<: *d, y: 2}").unwrap(),
        vec![
            code("defaults", "x: 1"),
            code("merged", "<<:\n  x: 1\ny: 2")
        ]
    );
    assert_eq!(
        parse("x: {a: &a {x: 1}, b: *a}").unwrap(),
        vec![code("x", "a: &a1\n  x: 1\nb: *a1")]
    );
}

#[test]
fn aliases_fail_visibly_when_missing_cyclic_or_excessive() {
    for source in ["a: *missing", "a: &a {self: *a}", "a: &a [*a]", "&a [*a]"] {
        assert!(parse(source).is_err(), "{source}");
    }
    let bomb = "a: &a [x,x,x,x,x,x,x,x,x,x,x]\nb: &b [*a,*a,*a,*a,*a,*a,*a,*a,*a,*a,*a]\nc: [*b,*b,*b,*b,*b,*b,*b,*b,*b,*b,*b]";
    assert!(parse(bomb).unwrap_err().contains("Excessive alias count"));
    let aliases = format!("a: &a x\nb: [{}]", vec!["*a"; 101].join(","));
    assert!(parse(&aliases).unwrap_err().contains("100 alias"));
}

#[test]
fn yaml_merges_admit_expanded_entries_and_key_bytes_before_copying() {
    let source = "%YAML 1.1\n---\na: &a {k: v}\nb: {<<: *a}";
    let budget = SourceBudget::preview();
    let mut op = budget.begin(CancellationToken::new());
    let ast = parse_ast(source, &mut op).unwrap();
    let Kind::Mapping(root) = &ast.nodes[ast.root.unwrap()].kind else {
        panic!("expected root mapping");
    };
    let anchor = root[0].1;
    let merged = root[1].1;
    for byte_limit in [false, true] {
        let mut converter = Converter::new(&ast, source);
        converter.convert(anchor, 0, &mut op).unwrap();
        if byte_limit {
            converter.merged_key_bytes = MAX_OUTPUT - 1;
        } else {
            // The map, alias and cached target consume the remaining three
            // nodes; its first merged entry must be rejected separately.
            converter.work = MAX_NODES - 3;
        }
        let error = converter.convert(merged, 0, &mut op).unwrap_err();
        let Failure::Yaml(message) = error else {
            panic!("merge limits must use the visible metadata fallback");
        };
        assert!(
            message.contains(if byte_limit {
                "1 MiB merged key"
            } else {
                "16,384"
            }),
            "{message}"
        );
        assert!(converter.cache[merged].is_none());
    }
    assert_eq!(op.first_fault(), None);
}

#[test]
fn multiline_and_markup_stay_literal_text() {
    assert_eq!(
        parse("a: |\n  hello\n  world").unwrap(),
        vec![text("a", "hello\nworld\n")]
    );
    assert_eq!(
        parse("a: >\n  hello\n  world").unwrap(),
        vec![text("a", "hello world\n")]
    );
    assert_eq!(
        parse("a: |+\n  hello\n\n").unwrap(),
        vec![text("a", "hello\n\n")]
    );
    assert_eq!(
        parse("a: \"hello\n  world\"").unwrap(),
        vec![text("a", "hello world")]
    );
    assert_eq!(
        parse("a: '**bold** [link](x) ![image](x) <script>x</script>'\nb: \"x\0y\"").unwrap(),
        vec![
            text("a", "**bold** [link](x) ![image](x) <script>x</script>"),
            text("b", "x\u{fffd}y")
        ]
    );
}

#[test]
fn yaml11_directive_and_multiple_documents_are_distinct() {
    assert_eq!(parse("%YAML 1.1\n--- x").unwrap(), vec![text("", "x")]);
    assert_eq!(
        parse("%YAML 1.1\n---\na: yes\nb: 012").unwrap(),
        vec![text("a", "true"), text("b", "10")]
    );
    assert!(parse("a: 1\n---\nb: 2").is_err());
    assert!(parse("title: [unclosed").is_err());
}

#[test]
fn visible_input_depth_node_and_output_limits() {
    assert!(
        parse(&"x".repeat(MAX_BYTES + 1))
            .unwrap_err()
            .contains("256 KiB")
    );
    assert!(
        parse(&format!("{}x{}", "[".repeat(65), "]".repeat(65)))
            .unwrap_err()
            .contains("64 level")
    );
    assert!(
        parse(&format!("[{}]", vec!["x"; MAX_NODES].join(",")))
            .unwrap_err()
            .contains("16,384")
    );
    let source = format!(
        "a: &a {}\nb: [{}]",
        "x".repeat(20_000),
        vec!["*a"; 70].join(",")
    );
    assert!(parse(&source).unwrap_err().contains("1 MiB"));
}

#[test]
fn operation_faults_remain_outer_and_scratch_is_released() {
    let budget = SourceBudget::new(SourceLimits::default()).unwrap();
    let cancel = CancellationToken::new();
    cancel.cancel();
    let mut op = budget.begin(cancel);
    assert_eq!(parse_entries("a: 1", &mut op), Err(CoreError::Cancelled));
    assert_eq!(budget.usage().auxiliary_capacity, 0);
    let limits = SourceLimits {
        max_work: 128,
        ..SourceLimits::default()
    };
    let budget = SourceBudget::new(limits).unwrap();
    let mut op = budget.begin(CancellationToken::new());
    assert!(matches!(
        parse_entries(&format!("# {}", "x".repeat(8192)), &mut op),
        Err(CoreError::ResourceLimit { .. })
    ));
    assert_eq!(budget.usage().auxiliary_capacity, 0);
    let budget = SourceBudget::new(SourceLimits::default()).unwrap();
    let mut op = budget.begin(CancellationToken::new());
    assert!(parse_entries("a: [unclosed", &mut op).unwrap().is_err());
    assert_eq!(budget.usage().auxiliary_capacity, 0);
}

#[test]
fn complex_key_exponents_and_fraction_digits_are_ast_formats() {
    assert_eq!(
        parse("? [1e3, 1E-7, -0e+0, 12.3400, -0.00]\n: value").unwrap(),
        vec![text("[ 1e+3, 1e-7, 0e+0, 12.3400, -0.00 ]", "value")]
    );
    assert_eq!(
        parse("? [1000000000000000100e0, 1.20e2, 0.00000010]\n: value").unwrap(),
        vec![text("[ 1.0000000000000001e+18, 1.2e+2, 1e-7 ]", "value")]
    );
    // Direct scalar keys keep JS property coercion, rather than YAML spelling.
    assert_eq!(
        parse("1e3: numeric\n\"1000\": string").unwrap(),
        vec![text("1000", "string")]
    );
    assert!(
        parse("1e3: first\n1000.0: second")
            .unwrap_err()
            .contains("Map keys must be unique")
    );
    assert_eq!(
        parse("%YAML 1.1\n---\n? [1_0.20_, -0.00_]\n: value").unwrap(),
        vec![text("[ 10.20, -0.00 ]", "value")]
    );
}

#[test]
fn complex_key_radices_are_normalized_after_js_rounding() {
    assert_eq!(
        parse("? [0x000A, 0o0017, 0x20000000000001, 0x10000000000000001]\n: value").unwrap(),
        vec![text(
            "[ 0xa, 0o17, 0x20000000000000, 0x10000000000000000 ]",
            "value"
        )]
    );
    assert_eq!(
        parse("%YAML 1.1\n---\n? [-0x00AF, +0b0011, 00017]\n: value").unwrap(),
        vec![text("[ -0xaf, 0b11, 017 ]", "value")]
    );
    assert!(
        parse("0x000A: first\n10: second")
            .unwrap_err()
            .contains("Map keys must be unique")
    );
}

#[test]
fn key_numeric_formats_preserve_quoted_nested_and_alias_keys() {
    assert_eq!(
        parse("? {\"1e3\": 1e3, '1.20': [1.20, 0x00A]}\n: value").unwrap(),
        vec![text("{ \"1e3\": 1e+3, '1.20': [ 1.20, 0xa ] }", "value")]
    );
    assert_eq!(
        parse("? &key [1e3, 1.20, \"0x000A\"]\n: first\n? *key\n: second").unwrap(),
        vec![
            text("[ 1e+3, 1.20, \"0x000A\" ]", "first"),
            text("*key", "second")
        ]
    );
}

#[test]
fn valid_large_values_respect_operation_batch_limits() {
    let scalar = "x".repeat(70_000);
    assert_eq!(
        parse(&format!("a: {scalar}")).unwrap(),
        vec![text("a", &scalar)]
    );
    let source = format!("# {}\na: &a value\nb: *a", "x".repeat(70_000));
    assert_eq!(
        parse(&source).unwrap(),
        vec![text("a", "value"), text("b", "value")]
    );
    let source = format!("!!binary {}", "AAAA".repeat(200));
    let rows = parse(&source).unwrap();
    assert_eq!(rows.len(), 600);
    assert_eq!(rows.first(), Some(&text("0", "0")));
    assert_eq!(rows.last(), Some(&text("599", "0")));
}
