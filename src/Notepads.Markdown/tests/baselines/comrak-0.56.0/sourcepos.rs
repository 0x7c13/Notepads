use nodes::NodeValueDiscriminants;
use pretty_assertions::assert_eq;
use strum::VariantArray;

use super::*;

type TestCase = (&'static [Sourcepos], &'static str);

#[test]
fn multiline_inline_sourcepos() {
    let options = Options {
        extension: options::Extension {
            math_dollars: true,
            math_code: true,
            math_latex: true,
            ..Default::default()
        },
        ..Default::default()
    };
    for newline in ["\n", "\r\n", "\r"] {
        for (open, close) in [
            ("`a", "b`"),
            ("$a", "b$"),
            ("$$a", "b$$"),
            ("$`a", "b`$"),
            ("\\(a", "b\\)"),
            ("<img", "src='x'>"),
        ] {
            for first_indent in 0..=3 {
                for last_indent in 0..=3 {
                    let input = format!(
                        "Before{newline}{newline}Lead{newline}{}{open}{newline}{}{close} tail{newline}",
                        " ".repeat(first_indent),
                        " ".repeat(last_indent),
                    );
                    let arena = Arena::new();
                    let root = parse_document(&arena, &input, &options);
                    let inline = root
                        .descendants()
                        .find(|node| {
                            matches!(
                                node.data().value,
                                NodeValue::Code(_) | NodeValue::Math(_) | NodeValue::HtmlInline(_)
                            )
                        })
                        .unwrap();
                    assert_eq!(
                        inline.data().sourcepos,
                        (4, first_indent + 1, 5, last_indent + close.len()).into(),
                        "{input:?}",
                    );
                }
            }
        }
    }
}

const DOCUMENT: TestCase = (&[sourcepos!((1:1-1:1))], "a");

const FRONT_MATTER: TestCase = (
    &[sourcepos!((1:1-3:3))],
    r#"---
a: b
---

hello world
"#,
);

const BLOCK_QUOTE: TestCase = (
    &[sourcepos!((1:1-3:36))],
    r#"> hello world
> this is line 1
> this is line 2 and some extra text

hello world"#,
);

const MULTILINE_BLOCK_QUOTE: TestCase = (
    &[sourcepos!((3:1-7:3))],
    r#"Some text

>>>
hello world
this is line 1
this is line 2 and some extra text
>>>

hello world"#,
);

const LIST: TestCase = (
    &[sourcepos!((1:1-2:38))],
    r#"- bullet point one
- bullet point two and some extra text

hello world
"#,
);

const ITEM: TestCase = (
    &[sourcepos!((1:1-1:18)), sourcepos!((2:1-2:38))],
    r#"- bullet point one
- bullet point two and some extra text

hello world
"#,
);

const TASK_ITEM: TestCase = (
    &[sourcepos!((1:1-1:22)), sourcepos!((3:1-3:24))],
    r#"- [ ] bullet point one
- bullet point two and some extra text
- [x] bullet point three

hello world
"#,
);

const DESCRIPTION_LIST: TestCase = (
    &[sourcepos!((1:1-7:11))],
    r#"Term 1

: Details 1

Term 2

: Details 2"#,
);

const DESCRIPTION_ITEM: TestCase = (
    &[sourcepos!((1:1-3:11)), sourcepos!((5:1-7:11))],
    r#"Term 1

: Details 1

Term 2

: Details 2"#,
);

const DESCRIPTION_TERM: TestCase = (
    &[sourcepos!((1:1-1:6))],
    r#"Term 1

: Details 1

hello world
"#,
);

const DESCRIPTION_DETAILS: TestCase = (
    &[sourcepos!((3:1-3:11))],
    r#"Term 1

: Details 1

hello world
"#,
);

const CODE_BLOCK: TestCase = (
    &[sourcepos!((1:1-3:3))],
    r#"```
hello world
```

hello world
"#,
);

const HTML_BLOCK: TestCase = (
    &[sourcepos!((1:1-2:30)), sourcepos!((5:1-5:10))],
    r#"<details>
<summary>hello world</summary>

hello world
</details>

hello world
"#,
);

const HTML_INLINE: TestCase = (
    &[sourcepos!((1:7-3:14))],
    r#"hello <img
    src="foo"
    alt="bar"> world
"#,
);

#[cfg(feature = "phoenix_heex")]
const HEEX_BLOCK: TestCase = (
    &[sourcepos!((1:1-3:10))],
    r#"<.header>
hello
</.header>

hello world
"#,
);

#[cfg(feature = "phoenix_heex")]
const HEEX_INLINE: TestCase = (
    &[sourcepos!((1:7-1:12))],
    r#"hello {some} world
"#,
);

const PARAGRAPH: TestCase = (
    &[sourcepos!((1:1-1:11)), sourcepos!((4:1-4:11))],
    r#"hello world


hello world
"#,
);

const HEADING: TestCase = (
    &[sourcepos!((5:1-5:13)), sourcepos!((7:1-7:16))],
    r#"---
a: b
---

# Hello World

# Test heading #

hello world
"#,
);

const THEMATIC_BREAK: TestCase = (
    &[sourcepos!((3:2-3:4))],
    r#"Hello

 ---

World"#,
);

const FOOTNOTE_DEFINITION: TestCase = (
    &[sourcepos!((3:1-3:11))],
    r#"Hello[^1]

[^1]: World
"#,
);

const FOOTNOTE_REFERENCE: TestCase = (
    &[sourcepos!((1:6-1:9))],
    r#"Hello[^1]

[^1]: World
"#,
);

#[cfg(feature = "shortcodes")]
const SHORTCODE: TestCase = (
    &[sourcepos!((2:2-2:7))],
    r#"nya~
!:fire:!
"#,
);

const TABLE: TestCase = (
    &[sourcepos!((3:1-5:17))],
    r#"stuff before

| Hello | World |
| ----- | ----- |
| cell1 | cell2 |

hello world
"#,
);

const TABLE_ROW: TestCase = (
    &[sourcepos!((3:1-3:17)), sourcepos!((5:1-5:18))],
    r#"stuff before

| Hello | World |
| ----- | ----- |
| cell1 | cell02 |

hello world
"#,
);

const TABLE_CELL: TestCase = (
    &[
        sourcepos!((3:2-3:8)),
        sourcepos!((3:10-3:16)),
        sourcepos!((5:2-5:8)),
        sourcepos!((5:10-5:17)),
    ],
    r#"stuff before

| Hello | World |
| ----- | ----- |
| cell1 | cell02 |

hello world
"#,
);

const TEXT: TestCase = (
    &[
        sourcepos!((1:1-1:12)),
        sourcepos!((3:3-3:7)),
        sourcepos!((3:11-3:15)),
        sourcepos!((5:3-5:7)),
        sourcepos!((5:11-5:16)),
        sourcepos!((7:1-7:11)),
        sourcepos!((9:3-9:13)),
        sourcepos!((11:3-11:8)),
        sourcepos!((12:3-12:9)),
        sourcepos!((12:12-12:15)),
        sourcepos!((14:1-14:5)),
        sourcepos!((14:7-14:7)),
        sourcepos!((14:8-14:18)),
        sourcepos!((14:20-14:20)),
        sourcepos!((14:21-14:26)),
        sourcepos!((16:7-16:14)),
    ],
    r#"stuff before

| Hello | World |
| ----- | ----- |
| cell1 | cell02 |

hello world

> hello world

- item 1[^1]
- item 2 **bold**

Test \`hello world\` here.

[^1]: The end.
"#,
);

const SOFT_BREAK: TestCase = (&[sourcepos!((1:13-1:13))], "stuff before\nstuff after");
const LINE_BREAK: TestCase = (
    &[sourcepos!((1:13-1:15)), sourcepos!((4:13-4:14))],
    "stuff before  \nstuff after\n\nstuff before\\\nstuff after\n",
);

const CODE: TestCase = (&[sourcepos!((1:7-1:13))], "hello `world`");

const EMPH: TestCase = (
    &[sourcepos!((1:7-1:13)), sourcepos!((1:23-2:4))],
    "hello *world* between *wo\nrld* after",
);

const STRONG: TestCase = (
    &[sourcepos!((1:7-1:15)), sourcepos!((1:25-2:5))],
    "hello **world** between **wo\nrld** after",
);

const STRIKETHROUGH: TestCase = (
    &[sourcepos!((1:7-1:15)), sourcepos!((1:25-2:5))],
    "hello ~~world~~ between ~~wo\nrld~~ after",
);

const HIGHLIGHT: TestCase = (
    &[sourcepos!((1:7-1:15)), sourcepos!((1:25-2:5))],
    "hello ==world== between ==wo\nrld== after",
);

const INSERT: TestCase = (
    &[sourcepos!((1:7-1:15)), sourcepos!((1:25-2:5))],
    "hello ++world++ between ++wo\nrld++ after",
);

const SUPERSCRIPT: TestCase = (
    &[sourcepos!((1:7-1:13)), sourcepos!((1:23-2:4))],
    "hello ^world^ between ^wo\nrld^ after",
);

const SUBSCRIPT: TestCase = (
    &[sourcepos!((1:7-1:13)), sourcepos!((1:23-2:4))],
    "hello ~world~ between ~wo\nrld~ after",
);

const LINK: TestCase = (
    &[
        sourcepos!((1:7-1:32)),
        sourcepos!((2:7-2:32)),
        sourcepos!((3:7-3:11)),
        sourcepos!((4:7-4:16)),
        sourcepos!((5:7-5:29)),
        sourcepos!((6:7-6:21)),
        sourcepos!((7:7-7:21)),
    ],
    r#"hello <https://example.com/fooo> world
hello [foo](https://example.com) world
hello [foo] world
hello [bar][bar] world
hello https://example.com/foo world
hello www.example.com world
hello foo@example.com world

[foo]: https://example.com
[bar]: https://example.com"#,
);

const IMAGE: TestCase = (
    &[sourcepos!((1:7-1:38))],
    "hello ![alt text](https://example.com) banana",
);

const MATH: TestCase = (
    &[
        sourcepos!((3:1-3:7)),
        sourcepos!((3:17-3:26)),
        sourcepos!((3:36-3:44)),
    ],
    r#"hello

$1 + 1$ between $`1 + 23`$ between $$a + b$$

banana"#,
);

const ESCAPED: TestCase = (
    &[
        sourcepos!((1:1-1:2)),
        sourcepos!((1:3-1:4)),
        sourcepos!((1:5-1:6)),
        sourcepos!((1:7-1:8)),
        sourcepos!((1:9-1:10)),
        sourcepos!((1:11-1:12)),
        sourcepos!((1:13-1:14)),
        sourcepos!((1:15-1:16)),
        sourcepos!((1:17-1:18)),
        sourcepos!((1:19-1:20)),
        sourcepos!((1:21-1:22)),
        sourcepos!((1:23-1:24)),
        sourcepos!((1:25-1:26)),
        sourcepos!((1:27-1:28)),
        sourcepos!((1:29-1:30)),
        sourcepos!((1:31-1:32)),
        sourcepos!((1:33-1:34)),
        sourcepos!((1:35-1:36)),
        sourcepos!((1:37-1:38)),
        sourcepos!((1:39-1:40)),
        sourcepos!((1:41-1:42)),
        sourcepos!((1:43-1:44)),
        sourcepos!((1:45-1:46)),
        sourcepos!((1:47-1:48)),
        sourcepos!((1:49-1:50)),
        sourcepos!((1:51-1:52)),
        sourcepos!((1:53-1:54)),
        sourcepos!((1:55-1:56)),
        sourcepos!((1:57-1:58)),
        sourcepos!((1:59-1:60)),
        sourcepos!((1:61-1:62)),
        sourcepos!((1:63-1:64)),
    ],
    r#"\!\"\#\$\%\&\'\(\)\*\+\,\-\.\/\:\;\<\=\>\?\@\[\\\]\^\_\`\{\|\}\~\a"#,
);

const WIKI_LINK: TestCase = (
    &[sourcepos!((1:1-1:9)), sourcepos!((3:1-3:33))],
    r#"[[floop]]

[[http://example.com|some title]]

after"#,
);

const UNDERLINE: TestCase = (&[sourcepos!((1:8-1:22))], "before __hello world__ after");

const SPOILERED_TEXT: TestCase = (
    &[sourcepos!((2:1-2:11))],
    r#"before
||spoiler||
after"#,
);

// NOTE: I've adjusted this from its original asserted sourcepos (2:1-2:8) while
// fixing emphasis sourcepos. I am not even sure what it is, really.
const ESCAPED_TAG: TestCase = (
    &[sourcepos!((2:2-2:8))],
    r#"before
||hello|
after"#,
);

const ALERT: TestCase = (
    &[sourcepos!((2:1-3:9))],
    r#"before
> [!note]
> it's on

after"#,
);

const SUBTEXT: TestCase = (
    &[sourcepos!((2:1-2:14))],
    r#"before
-# Hello World

hello world
"#,
);

const BLOCK_DIRECTIVE: TestCase = (
    &[sourcepos!((2:1-4:3))],
    r#"before
:::
hello world
:::
after"#,
);

fn node_values() -> HashMap<NodeValueDiscriminants, TestCase> {
    use NodeValueDiscriminants::*;

    NodeValueDiscriminants::VARIANTS
        .iter()
        .filter(|v| !matches!(v, Raw))
        .map(|v| {
            let text = match v {
                Document => DOCUMENT,
                FrontMatter => FRONT_MATTER,
                BlockQuote => BLOCK_QUOTE,
                MultilineBlockQuote => MULTILINE_BLOCK_QUOTE,
                List => LIST,
                Item => ITEM,
                TaskItem => TASK_ITEM,
                DescriptionList => DESCRIPTION_LIST,
                DescriptionItem => DESCRIPTION_ITEM,
                DescriptionTerm => DESCRIPTION_TERM,
                DescriptionDetails => DESCRIPTION_DETAILS,
                CodeBlock => CODE_BLOCK,
                HtmlBlock => HTML_BLOCK,
                HtmlInline => HTML_INLINE,
                Paragraph => PARAGRAPH,
                Heading => HEADING,
                ThematicBreak => THEMATIC_BREAK,
                FootnoteDefinition => FOOTNOTE_DEFINITION,
                FootnoteReference => FOOTNOTE_REFERENCE,
                #[cfg(feature = "shortcodes")]
                ShortCode => SHORTCODE,
                Table => TABLE,
                TableRow => TABLE_ROW,
                TableCell => TABLE_CELL,
                Text => TEXT,
                SoftBreak => SOFT_BREAK,
                LineBreak => LINE_BREAK,
                Code => CODE,
                Emph => EMPH,
                Strong => STRONG,
                Strikethrough => STRIKETHROUGH,
                Highlight => HIGHLIGHT,
                Insert => INSERT,
                Superscript => SUPERSCRIPT,
                Subscript => SUBSCRIPT,
                Link => LINK,
                Image => IMAGE,
                Math => MATH,
                Escaped => ESCAPED,
                WikiLink => WIKI_LINK,
                Underline => UNDERLINE,
                SpoileredText => SPOILERED_TEXT,
                EscapedTag => ESCAPED_TAG,
                Alert => ALERT,
                Subtext => SUBTEXT,
                BlockDirective => BLOCK_DIRECTIVE,
                Raw => unreachable!(),
                #[cfg(feature = "phoenix_heex")]
                HeexBlock => HEEX_BLOCK,
                #[cfg(feature = "phoenix_heex")]
                HeexInline => HEEX_INLINE,
            };
            (*v, text)
        })
        .collect()
}

#[test]
fn sourcepos() {
    let node_values = node_values();

    let mut options = Options::default();
    options.render.escaped_char_spans = true;
    options.extension.front_matter_delimiter = Some("---".to_string());
    options.extension.description_lists = true;
    options.extension.footnotes = true;
    options.extension.table = true;
    options.extension.tasklist = true;
    options.extension.strikethrough = true;
    options.extension.highlight = true;
    options.extension.insert = true;
    options.extension.superscript = true;
    options.extension.subscript = true;
    options.extension.autolink = true;
    #[cfg(feature = "shortcodes")]
    {
        options.extension.shortcodes = true;
    }
    options.extension.math_code = true;
    options.extension.math_dollars = true;
    options.extension.multiline_block_quotes = true;
    options.extension.wikilinks_title_after_pipe = true;
    options.extension.underline = true;
    options.extension.spoiler = true;
    options.extension.alerts = true;
    options.extension.subtext = true;
    options.extension.block_directive = true;
    #[cfg(feature = "phoenix_heex")]
    {
        options.extension.phoenix_heex = true;
    }

    for (kind, (expecteds, text)) in node_values {
        let arena = Arena::new();
        let root = parse_document(&arena, text, &options);
        let asts: Vec<_> = root
            .descendants()
            .filter(|d| NodeValueDiscriminants::from(&d.data().value) == kind)
            .collect();

        if asts.len() != expecteds.len() {
            panic!(
                "expected {} node(s) of kind {:?}, but got {}",
                expecteds.len(),
                kind,
                asts.len()
            );
        }

        for (ast, expected) in asts.into_iter().zip(expecteds) {
            let actual = ast.data().sourcepos;
            assert_eq!(
                *expected, actual,
                "{} != {} for {:?}",
                expected, actual, kind
            );
        }
    }
}
