use pretty_assertions::assert_eq;

use super::*;

fn first_sourcepos(input: &str, sourcepos_chars: bool) -> Sourcepos {
    let arena = Arena::new();
    let mut options = Options::default();
    options.parse.sourcepos_chars = sourcepos_chars;
    let root = parse_document(&arena, input, &options);
    root.descendants()
        .find(|n| matches!(n.data().value, NodeValue::Paragraph))
        .map(|n| n.data().sourcepos)
        .expect("no matching node found")
}

#[test]
fn sourcepos_chars_one_byte() {
    const INPUT: &str = "a";
    let sp = first_sourcepos(INPUT, false);
    assert_eq!(sp, sourcepos!((1:1-1:1)));
    let spc = first_sourcepos(INPUT, true);
    assert_eq!(spc, sourcepos!((1:1-1:1)));
}

#[test]
fn sourcepos_chars_two_byte() {
    const INPUT: &str = "ɽ";
    let sp = first_sourcepos(INPUT, false);
    assert_eq!(sp, sourcepos!((1:1-1:2)));
    let spc = first_sourcepos(INPUT, true);
    assert_eq!(spc, sourcepos!((1:1-1:1)));
}

#[test]
fn sourcepos_chars_three_byte() {
    const INPUT: &str = "好";
    let sp = first_sourcepos(INPUT, false);
    assert_eq!(sp, sourcepos!((1:1-1:3)));
    let spc = first_sourcepos(INPUT, true);
    assert_eq!(spc, sourcepos!((1:1-1:1)));
}

#[test]
fn sourcepos_chars_four_byte() {
    const INPUT: &str = "𐀀";
    let sp = first_sourcepos(INPUT, false);
    assert_eq!(sp, sourcepos!((1:1-1:4)));
    let spc = first_sourcepos(INPUT, true);
    assert_eq!(spc, sourcepos!((1:1-1:1)));
}

#[test]
fn sourcepos_chars_multiple_one_byte_characters() {
    const INPUT: &str = "aaa";
    let sp = first_sourcepos(INPUT, false);
    assert_eq!(sp, sourcepos!((1:1-1:3)));
    let spc = first_sourcepos(INPUT, true);
    assert_eq!(spc, sourcepos!((1:1-1:3)));
}

#[test]
fn sourcepos_chars_multiple_two_byte_characters() {
    const INPUT: &str = "ɽɽɽ";
    let sp = first_sourcepos(INPUT, false);
    assert_eq!(sp, sourcepos!((1:1-1:6)));
    let spc = first_sourcepos(INPUT, true);
    assert_eq!(spc, sourcepos!((1:1-1:3)));
}

#[test]
fn sourcepos_chars_multiple_three_byte_characters() {
    const INPUT: &str = "好好好";
    let sp = first_sourcepos(INPUT, false);
    assert_eq!(sp, sourcepos!((1:1-1:9)));
    let spc = first_sourcepos(INPUT, true);
    assert_eq!(spc, sourcepos!((1:1-1:3)));
}

#[test]
fn sourcepos_chars_multiple_four_byte_characters() {
    const INPUT: &str = "𐀀𐀀𐀀";
    let sp = first_sourcepos(INPUT, false);
    assert_eq!(sp, sourcepos!((1:1-1:12)));
    let spc = first_sourcepos(INPUT, true);
    assert_eq!(spc, sourcepos!((1:1-1:3)));
}

#[test]
fn sourcepos_chars_multiple_mixed_characters() {
    const INPUT: &str = "aɽ好𐀀";
    let sp = first_sourcepos(INPUT, false);
    assert_eq!(sp, sourcepos!((1:1-1:10)));
    let spc = first_sourcepos(INPUT, true);
    assert_eq!(spc, sourcepos!((1:1-1:4)));

    const REVERSED: &str = "𐀀好ɽa";
    let sp2 = first_sourcepos(REVERSED, false);
    assert_eq!(sp2, sourcepos!((1:1-1:10)));
    let spc2 = first_sourcepos(REVERSED, true);
    assert_eq!(spc2, sourcepos!((1:1-1:4)));

    const MIXED: &str = "a𐀀ɽ好";
    let sp3 = first_sourcepos(MIXED, false);
    assert_eq!(sp3, sourcepos!((1:1-1:10)));
    let spc3 = first_sourcepos(MIXED, true);
    assert_eq!(spc3, sourcepos!((1:1-1:4)));

    const REVERSED_MIXED: &str = "好ɽ𐀀a";
    let sp4 = first_sourcepos(REVERSED_MIXED, false);
    assert_eq!(sp4, sourcepos!((1:1-1:10)));
    let spc4 = first_sourcepos(REVERSED_MIXED, true);
    assert_eq!(spc4, sourcepos!((1:1-1:4)));
}

#[test]
fn sourcepos_chars_character_and_combining_characters() {
    const INPUT: &str = "é";
    let sp = first_sourcepos(INPUT, false);
    assert_eq!(sp, sourcepos!((1:1-1:3)));
    let spc = first_sourcepos(INPUT, true);
    assert_eq!(spc, sourcepos!((1:1-1:2)));

    const THAI: &str = "ก้้้";
    let sp2 = first_sourcepos(THAI, false);
    assert_eq!(sp2, sourcepos!((1:1-1:12)));
    let spc2 = first_sourcepos(THAI, true);
    assert_eq!(spc2, sourcepos!((1:1-1:4)));
}

#[test]
fn sourcepos_chars_emoji() {
    // heart without variation selector
    const ONLY_HEART: &str = "❤";
    let sp = first_sourcepos(ONLY_HEART, false);
    assert_eq!(sp, sourcepos!((1:1-1:3)));
    let spc = first_sourcepos(ONLY_HEART, true);
    assert_eq!(spc, sourcepos!((1:1-1:1)));

    // heart + variation selector
    const HEART_WITH_VARIANT: &str = "❤️";
    let sp = first_sourcepos(HEART_WITH_VARIANT, false);
    assert_eq!(sp, sourcepos!((1:1-1:6)));
    let spc = first_sourcepos(HEART_WITH_VARIANT, true);
    assert_eq!(spc, sourcepos!((1:1-1:2)));

    // family emoji with multiple code points
    const FAMILY: &str = "👨‍👩‍👧‍👦";
    let sp = first_sourcepos(FAMILY, false);
    assert_eq!(sp, sourcepos!((1:1-1:25)));
    let spc = first_sourcepos(FAMILY, true);
    assert_eq!(spc, sourcepos!((1:1-1:7)));

    // family emoji with skin tone modifiers
    const FAMILY_WITH_SKIN_TONES: &str = "👩🏻‍👩‍👧‍👦";
    let sp = first_sourcepos(FAMILY_WITH_SKIN_TONES, false);
    assert_eq!(sp, sourcepos!((1:1-1:29)));
    let spc = first_sourcepos(FAMILY_WITH_SKIN_TONES, true);
    assert_eq!(spc, sourcepos!((1:1-1:8)));

    // flag emoji with regional indicator symbols
    const FLAG: &str = "🇺🇸";
    let sp = first_sourcepos(FLAG, false);
    assert_eq!(sp, sourcepos!((1:1-1:8)));
    let spc = first_sourcepos(FLAG, true);
    assert_eq!(spc, sourcepos!((1:1-1:2)));
}

#[test]
fn sourcepos_chars_confusables() {
    const INPUT: &str = "𝙰A𝐴ᗅ𝑨𝚨𝕬𝖠АΑ𝛢𝝖𝒜𝜜𖽀𝓐𝞐Ａ𝗔𝘈Ꭺꓮ𝐀𝔄𝔸𐊠𝘼";
    let sp = first_sourcepos(INPUT, false);
    assert_eq!(sp, sourcepos!((1:1-1:97)));
    let spc = first_sourcepos(INPUT, true);
    assert_eq!(spc, sourcepos!((1:1-1:27)));
}

#[test]
fn sourcepos_chars_rtl() {
    const INPUT: &str = "שלום";
    let sp = first_sourcepos(INPUT, false);
    assert_eq!(sp, sourcepos!((1:1-1:8)));
    let spc = first_sourcepos(INPUT, true);
    assert_eq!(spc, sourcepos!((1:1-1:4)));
}

#[test]
fn sourcepos_chars_rtl_ltr_isolates() {
    const LTR_ISOLATE: &str = "A\u{2066}שלום\u{2069}B";
    let sp = first_sourcepos(LTR_ISOLATE, false);
    assert_eq!(sp, sourcepos!((1:1-1:16)));
    let spc = first_sourcepos(LTR_ISOLATE, true);
    assert_eq!(spc, sourcepos!((1:1-1:8)));

    const RTL_ISOLATE: &str = "A\u{2067}שלום\u{2069}B";
    let sp2 = first_sourcepos(RTL_ISOLATE, false);
    assert_eq!(sp2, sourcepos!((1:1-1:16)));
    let spc2 = first_sourcepos(RTL_ISOLATE, true);
    assert_eq!(spc2, sourcepos!((1:1-1:8)));
}

#[test]
fn sourcepos_chars_one_byte_strong() {
    // Byte layout of "a**a**a" (7 bytes): a=1, **=2-3, a=4, **=5-6, a=7
    assert_ast_match!(
        [],
        "a**a**a",
        (document (1:1-1:7) [
            (paragraph (1:1-1:7) [
                (text (1:1-1:1) "a")
                (strong (1:2-1:6) [
                    (text (1:4-1:4) "a")
                ])
                (text (1:7-1:7) "a")
            ])
        ])
    );

    // Char layout of "a**a**a" (7 chars): a=1, **=2-3, a=4, **=5-6, a=7
    assert_ast_match!(
        [parse.sourcepos_chars],
        "a**a**a",
        (document (1:1-1:7) [
            (paragraph (1:1-1:7) [
                (text (1:1-1:1) "a")
                (strong (1:2-1:6) [
                    (text (1:4-1:4) "a")
                ])
                (text (1:7-1:7) "a")
            ])
        ])
    );
}

#[test]
fn sourcepos_chars_two_byte_strong() {
    // Byte layout of "ɽ**ɽ**ɽ" (10 bytes): ɽ=1-2, **=3-4, ɽ=5-6, **=7-8, ɽ=9-10
    assert_ast_match!(
        [],
        "ɽ**ɽ**ɽ",
        (document (1:1-1:10) [
            (paragraph (1:1-1:10) [
                (text (1:1-1:2) "ɽ")
                (strong (1:3-1:8) [
                    (text (1:5-1:6) "ɽ")
                ])
                (text (1:9-1:10) "ɽ")
            ])
        ])
    );

    // Char layout of "ɽ**ɽ**ɽ" (7 chars): ɽ=1, **=2-3, ɽ=4, **=5-6, ɽ=7
    assert_ast_match!(
        [parse.sourcepos_chars],
        "ɽ**ɽ**ɽ",
        (document (1:1-1:7) [
            (paragraph (1:1-1:7) [
                (text (1:1-1:1) "ɽ")
                (strong (1:2-1:6) [
                    (text (1:4-1:4) "ɽ")
                ])
                (text (1:7-1:7) "ɽ")
            ])
        ])
    );
}

#[test]
fn sourcepos_chars_three_byte_strong() {
    // Byte layout of "好**好**好" (13 bytes): 好=1-3, **=4-5, 好=6-8, **=9-10, 好=11-13
    assert_ast_match!(
        [],
        "好**好**好",
        (document (1:1-1:13) [
            (paragraph (1:1-1:13) [
                (text (1:1-1:3) "好")
                (strong (1:4-1:10) [
                    (text (1:6-1:8) "好")
                ])
                (text (1:11-1:13) "好")
            ])
        ])
    );

    // Char layout of "好**好**好" (7 chars): 好=1, **=2-3, 好=4, **=5-6, 好=7
    assert_ast_match!(
        [parse.sourcepos_chars],
        "好**好**好",
        (document (1:1-1:7) [
            (paragraph (1:1-1:7) [
                (text (1:1-1:1) "好")
                (strong (1:2-1:6) [
                    (text (1:4-1:4) "好")
                ])
                (text (1:7-1:7) "好")
            ])
        ])
    );
}

#[test]
fn sourcepos_chars_four_byte_strong() {
    // Byte layout of "𐀀**𐀀**𐀀" (16 bytes): 𐀀=1-4, **=5-6, 𐀀=7-10, **=11-12, 𐀀=13-16
    assert_ast_match!(
        [],
        "𐀀**𐀀**𐀀",
        (document (1:1-1:16) [
            (paragraph (1:1-1:16) [
                (text (1:1-1:4) "𐀀")
                (strong (1:5-1:12) [
                    (text (1:7-1:10) "𐀀")
                ])
                (text (1:13-1:16) "𐀀")
            ])
        ])
    );

    // Char layout of "𐀀**𐀀**𐀀" (7 chars): 𐀀=1, **=2-3, 𐀀=4, **=5-6, 𐀀=7
    assert_ast_match!(
        [parse.sourcepos_chars],
        "𐀀**𐀀**𐀀",
        (document (1:1-1:7) [
            (paragraph (1:1-1:7) [
                (text (1:1-1:1) "𐀀")
                (strong (1:2-1:6) [
                    (text (1:4-1:4) "𐀀")
                ])
                (text (1:7-1:7) "𐀀")
            ])
        ])
    );
}

#[test]
fn sourcepos_chars_alerts() {
    assert_ast_match!(
        [extension.alerts, extension.multiline_block_quotes],
        ">>> [!note]\n"
        "𐀀\n"
        ">>>\n",
        (document (1:1-3:3) [
            (alert (1:1-2:4) [
                (paragraph (2:1-2:4) [
                    (text (2:1-2:4) "𐀀")
                ])
            ])
        ])
    );

    assert_ast_match!(
        [parse.sourcepos_chars, extension.alerts, extension.multiline_block_quotes],
        ">>> [!note]\n"
        "𐀀\n"
        ">>>\n",
        (document (1:1-3:3) [
            (alert (1:1-2:1) [
                (paragraph (2:1-2:1) [
                    (text (2:1-2:1) "𐀀")
                ])
            ])
        ])
    );
}

#[test]
fn sourcepos_chars_cjk_friendly_emphasis() {
    assert_ast_match!(
        [extension.cjk_friendly_emphasis],
        "스크립트**안녕**하세요",
        (document (1:1-1:31) [
            (paragraph (1:1-1:31) [
                (text (1:1-1:12) "스크립트")
                (strong (1:13-1:22) [
                    (text (1:15-1:20) "안녕")
                ])
                (text (1:23-1:31) "하세요")
            ])
        ])
    );

    assert_ast_match!(
        [parse.sourcepos_chars, extension.cjk_friendly_emphasis],
        "스크립트**안녕**하세요",
        (document (1:1-1:13) [
            (paragraph (1:1-1:13) [
                (text (1:1-1:4) "스크립트")
                (strong (1:5-1:10) [
                    (text (1:7-1:8) "안녕")
                ])
                (text (1:11-1:13) "하세요")
            ])
        ])
    );
}

#[test]
fn sourcepos_chars_code() {
    // Inline code
    assert_ast_match!(
        [],
        "`𐀀`",
        (document (1:1-1:6) [
            (paragraph (1:1-1:6) [
                (code (1:1-1:6) "𐀀")
            ])
        ])
    );

    assert_ast_match!(
        [parse.sourcepos_chars],
        "`𐀀`",
        (document (1:1-1:3) [
            (paragraph (1:1-1:3) [
                (code (1:1-1:3) "𐀀")
            ])
        ])
    );

    // Code block
    assert_ast_match!(
        [],
        "```\n𐀀\n```\n",
        (document (1:1-3:3) [
            (code_block (1:1-3:3) "𐀀\n")
        ])
    );

    assert_ast_match!(
        [parse.sourcepos_chars],
        "```\n𐀀\n```\n",
        (document (1:1-3:3) [
            (code_block (1:1-3:3) "𐀀\n")
        ])
    );
}

#[test]
fn sourcepos_chars_frontmatter() {
    assert_ast_match!(
        [extension.front_matter_delimiter = Some("---".to_owned())],
        "---\ntitle: 𐀀\n---\n\n# 𐀀\n\n𐀀",
        (document (1:1-7:4) [
            (frontmatter (1:1-3:3) "---\ntitle: 𐀀\n---\n\n")
            (heading (5:1-5:6) [
                (text (5:3-5:6) "𐀀")
            ])
            (paragraph (7:1-7:4) [
                (text (7:1-7:4) "𐀀")
            ])
        ])
    );

    assert_ast_match!(
        [parse.sourcepos_chars, extension.front_matter_delimiter = Some("---".to_owned())],
        "---\ntitle: 𐀀\n---\n\n# 𐀀\n\n𐀀",
        (document (1:1-7:1) [
            (frontmatter (1:1-3:3) "---\ntitle: 𐀀\n---\n\n")
            (heading (5:1-5:3) [
                (text (5:3-5:3) "𐀀")
            ])
            (paragraph (7:1-7:1) [
                (text (7:1-7:1) "𐀀")
            ])
        ])
    );
}

#[test]
fn sourcepos_chars_link() {
    assert_ast_match!(
        [],
        "[𐀀](𐀁)",
        (document (1:1-1:12) [
            (paragraph (1:1-1:12) [
                (link (1:1-1:12) "𐀁" [
                    (text (1:2-1:5) "𐀀")
                ])
            ])
        ])
    );

    assert_ast_match!(
        [parse.sourcepos_chars],
        "[𐀀](𐀁)",
        (document (1:1-1:6) [
            (paragraph (1:1-1:6) [
                (link (1:1-1:6) "𐀁" [
                    (text (1:2-1:2) "𐀀")
                ])
            ])
        ])
    );

    // Link with a title
    assert_ast_match!(
        [],
        "[𐀀](𐀁 \"𐀂\")",
        (document (1:1-1:19) [
            (paragraph (1:1-1:19) [
                (link (1:1-1:19) "𐀁" [
                    (text (1:2-1:5) "𐀀")
                ])
            ])
        ])
    );

    assert_ast_match!(
        [parse.sourcepos_chars],
        "[𐀀](𐀁 \"𐀂\")",
        (document (1:1-1:10) [
            (paragraph (1:1-1:10) [
                (link (1:1-1:10) "𐀁" [
                    (text (1:2-1:2) "𐀀")
                ])
            ])
        ])
    );
}

#[test]
fn sourcepos_chars_lists() {
    // Unordered list
    assert_ast_match!(
        [],
        "- 𐀀\n- 𐀁\n",
        (document (1:1-2:6) [
            (list (1:1-2:6) [
                (item (1:1-1:6) [
                    (paragraph (1:3-1:6) [
                        (text (1:3-1:6) "𐀀")
                    ])
                ])
                (item (2:1-2:6) [
                    (paragraph (2:3-2:6) [
                        (text (2:3-2:6) "𐀁")
                    ])
                ])
            ])
        ])
    );

    assert_ast_match!(
        [parse.sourcepos_chars],
        "- 𐀀\n- 𐀁\n",
        (document (1:1-2:3) [
            (list (1:1-2:3) [
                (item (1:1-1:3) [
                    (paragraph (1:3-1:3) [
                        (text (1:3-1:3) "𐀀")
                    ])
                ])
                (item (2:1-2:3) [
                    (paragraph (2:3-2:3) [
                        (text (2:3-2:3) "𐀁")
                    ])
                ])
            ])
        ])
    );

    // Ordered list
    assert_ast_match!(
        [],
        "1. 𐀀\n2. 𐀁\n",
        (document (1:1-2:7) [
            (list (1:1-2:7) [
                (item (1:1-1:7) [
                    (paragraph (1:4-1:7) [
                        (text (1:4-1:7) "𐀀")
                    ])
                ])
                (item (2:1-2:7) [
                    (paragraph (2:4-2:7) [
                        (text (2:4-2:7) "𐀁")
                    ])
                ])
            ])
        ])
    );

    assert_ast_match!(
        [parse.sourcepos_chars],
        "1. 𐀀\n2. 𐀁\n",
        (document (1:1-2:4) [
            (list (1:1-2:4) [
                (item (1:1-1:4) [
                    (paragraph (1:4-1:4) [
                        (text (1:4-1:4) "𐀀")
                    ])
                ])
                (item (2:1-2:4) [
                    (paragraph (2:4-2:4) [
                        (text (2:4-2:4) "𐀁")
                    ])
                ])
            ])
        ])
    );
}

#[test]
fn sourcepos_chars_softbreak() {
    // Single-byte characters
    assert_ast_match!(
        [],
        "a\n"
        "b\n",
        (document (1:1-2:1) [
            (paragraph (1:1-2:1) [
                (text (1:1-1:1) "a")
                (softbreak (1:2-1:2))
                (text (2:1-2:1) "b")
            ])
        ])
    );

    assert_ast_match!(
        [parse.sourcepos_chars],
        "a\n"
        "b\n",
        (document (1:1-2:1) [
            (paragraph (1:1-2:1) [
                (text (1:1-1:1) "a")
                (softbreak (1:2-1:2))
                (text (2:1-2:1) "b")
            ])
        ])
    );

    // Two-byte characters
    assert_ast_match!(
        [],
        "ɽ\n"
        "ɾ\n",
        (document (1:1-2:2) [
            (paragraph (1:1-2:2) [
                (text (1:1-1:2) "ɽ")
                (softbreak (1:3-1:3))
                (text (2:1-2:2) "ɾ")
            ])
        ])
    );

    assert_ast_match!(
        [parse.sourcepos_chars],
        "ɽ\n"
        "ɾ\n",
        (document (1:1-2:1) [
            (paragraph (1:1-2:1) [
                (text (1:1-1:1) "ɽ")
                (softbreak (1:2-1:2))
                (text (2:1-2:1) "ɾ")
            ])
        ])
    );

    // Three-byte characters
    assert_ast_match!(
        [],
        "好\n"
        "奾\n",
        (document (1:1-2:3) [
            (paragraph (1:1-2:3) [
                (text (1:1-1:3) "好")
                (softbreak (1:4-1:4))
                (text (2:1-2:3) "奾")
            ])
        ])
    );

    assert_ast_match!(
        [parse.sourcepos_chars],
        "好\n"
        "奾\n",
        (document (1:1-2:1) [
            (paragraph (1:1-2:1) [
                (text (1:1-1:1) "好")
                (softbreak (1:2-1:2))
                (text (2:1-2:1) "奾")
            ])
        ])
    );

    // Four-byte characters
    assert_ast_match!(
        [],
        "𐀀\n"
        "𐀁\n",
        (document (1:1-2:4) [
            (paragraph (1:1-2:4) [
                (text (1:1-1:4) "𐀀")
                (softbreak (1:5-1:5))
                (text (2:1-2:4) "𐀁")
            ])
        ])
    );

    assert_ast_match!(
        [parse.sourcepos_chars],
        "𐀀\n"
        "𐀁\n",
        (document (1:1-2:1) [
            (paragraph (1:1-2:1) [
                (text (1:1-1:1) "𐀀")
                (softbreak (1:2-1:2))
                (text (2:1-2:1) "𐀁")
            ])
        ])
    );
}

#[test]
fn sourcepos_chars_table() {
    assert_ast_match!(
        [extension.table],
        "| 𐀀 | 𐀁 |\n"
        "| - | - |\n"
        "| 𐀂 | 𐀃 |\n"
        ,
        (document (1:1-3:15) [
            (table (1:1-3:15) [
                (table_row (1:1-1:15) [
                    (table_cell (1:2-1:7) [
                        (text (1:3-1:6) "𐀀")
                    ])
                    (table_cell (1:9-1:14) [
                        (text (1:10-1:13) "𐀁")
                    ])
                ])
                (table_row (3:1-3:15) [
                    (table_cell (3:2-3:7) [
                        (text (3:3-3:6) "𐀂")
                    ])
                    (table_cell (3:9-3:14) [
                        (text (3:10-3:13) "𐀃")
                    ])
                ])
            ])
        ])
    );

    assert_ast_match!(
        [parse.sourcepos_chars, extension.table],
        "| 𐀀 | 𐀁 |\n"
        "| - | - |\n"
        "| 𐀂 | 𐀃 |\n"
        ,
        (document (1:1-3:9) [
            (table (1:1-3:9) [
                (table_row (1:1-1:9) [
                    (table_cell (1:2-1:4) [
                        (text (1:3-1:3) "𐀀")
                    ])
                    (table_cell (1:6-1:8) [
                        (text (1:7-1:7) "𐀁")
                    ])
                ])
                (table_row (3:1-3:9) [
                    (table_cell (3:2-3:4) [
                        (text (3:3-3:3) "𐀂")
                    ])
                    (table_cell (3:6-3:8) [
                        (text (3:7-3:7) "𐀃")
                    ])
                ])
            ])
        ])
    );
}

#[test]
fn sourcepos_chars_tasklist() {
    assert_ast_match!(
        [extension.tasklist],
        "- [x] 𐀀\n"
        "  - [ ] 𐀁\n"
        "  - [x] 𐀂\n"
        "- [ ] 𐀃\n",
        (document (1:1-4:10) [
            (list (1:1-4:10) [
                (taskitem (1:1-3:12) [
                    (paragraph (1:7-1:10) [
                        (text (1:7-1:10) "𐀀")
                    ])
                    (list (2:3-3:12) [
                        (taskitem (2:3-2:12) [
                            (paragraph (2:9-2:12) [
                                (text (2:9-2:12) "𐀁")
                            ])
                        ])
                        (taskitem (3:3-3:12) [
                            (paragraph (3:9-3:12) [
                                (text (3:9-3:12) "𐀂")
                            ])
                        ])
                    ])
                ])
                (taskitem (4:1-4:10) [
                    (paragraph (4:7-4:10) [
                        (text (4:7-4:10) "𐀃")
                    ])
                ])
            ])
        ])
    );

    assert_ast_match!(
        [parse.sourcepos_chars, extension.tasklist],
        "- [x] 𐀀\n"
        "  - [ ] 𐀁\n"
        "  - [x] 𐀂\n"
        "- [ ] 𐀃\n",
        (document (1:1-4:7) [
            (list (1:1-4:7) [
                (taskitem (1:1-3:9) [
                    (paragraph (1:7-1:7) [
                        (text (1:7-1:7) "𐀀")
                    ])
                    (list (2:3-3:9) [
                        (taskitem (2:3-2:9) [
                            (paragraph (2:9-2:9) [
                                (text (2:9-2:9) "𐀁")
                            ])
                        ])
                        (taskitem (3:3-3:9) [
                            (paragraph (3:9-3:9) [
                                (text (3:9-3:9) "𐀂")
                            ])
                        ])
                    ])
                ])
                (taskitem (4:1-4:7) [
                    (paragraph (4:7-4:7) [
                        (text (4:7-4:7) "𐀃")
                    ])
                ])
            ])
        ])
    );
}

#[test]
fn sourcepos_chars_thematic_break() {
    assert_ast_match!(
        [],
        "𐀀\n\n"
        "---\n\n"
        "𐀁\n",
        (document (1:1-5:4) [
            (paragraph (1:1-1:4) [
                (text (1:1-1:4) "𐀀")
            ])
            (thematic_break (3:1-3:3))
            (paragraph (5:1-5:4) [
                (text (5:1-5:4) "𐀁")
            ])
        ])
    );

    assert_ast_match!(
        [parse.sourcepos_chars],
        "𐀀\n\n"
        "---\n\n"
        "𐀁\n",
        (document (1:1-5:1) [
            (paragraph (1:1-1:1) [
                (text (1:1-1:1) "𐀀")
            ])
            (thematic_break (3:1-3:3))
            (paragraph (5:1-5:1) [
                (text (5:1-5:1) "𐀁")
            ])
        ])
    );
}
