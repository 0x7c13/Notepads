//! Convenient helper functions for common use cases:
//! * Printing to terminal
//! * Iterating lines with `\n`s
//! * Modifying ranges of highlighted output

use crate::highlighting::{Color, Style, StyleModifier};
#[cfg(feature = "parsing")]
use crate::parsing::ScopeStackOp;
use std::fmt::Write;
use std::ops::Range;

#[inline]
fn blend_fg_color(fg: Color, bg: Color) -> Color {
    if fg.a == 0xff {
        return fg;
    }
    let ratio = fg.a as u32;
    let r = (fg.r as u32 * ratio + bg.r as u32 * (255 - ratio)) / 255;
    let g = (fg.g as u32 * ratio + bg.g as u32 * (255 - ratio)) / 255;
    let b = (fg.b as u32 * ratio + bg.b as u32 * (255 - ratio)) / 255;
    Color {
        r: r as u8,
        g: g as u8,
        b: b as u8,
        a: 255,
    }
}

/// Formats the styled fragments using 24-bit color terminal escape codes.
/// Meant for debugging and testing.
///
/// This function is currently fairly inefficient in its use of escape codes.
///
/// Note that this does not currently ever un-set the color so that the end of a line will also get
/// highlighted with the background.  This means if you might want to use `println!("\x1b[0m");`
/// after to clear the coloring.
///
/// If `bg` is true then the background is also set
pub fn as_24_bit_terminal_escaped(v: &[(Style, &str)], bg: bool) -> String {
    let mut s: String = String::new();
    for &(ref style, text) in v.iter() {
        if bg {
            write!(
                s,
                "\x1b[48;2;{};{};{}m",
                style.background.r, style.background.g, style.background.b
            )
            .unwrap();
        }
        let fg = blend_fg_color(style.foreground, style.background);
        write!(s, "\x1b[38;2;{};{};{}m{}", fg.r, fg.g, fg.b, text).unwrap();
    }
    // s.push_str("\x1b[0m");
    s
}

const LATEX_REPLACE: [(&str, &str); 3] = [("\\", "\\\\"), ("{", "\\{"), ("}", "\\}")];

/// Formats the styled fragments using LaTeX textcolor directive.
///
/// Usage is similar to the `as_24_bit_terminal_escaped` function:
///
/// ```
/// use syntect::easy::HighlightLines;
/// use syntect::parsing::SyntaxSet;
/// use syntect::highlighting::{ThemeSet,Style};
/// use syntect::util::{as_latex_escaped,LinesWithEndings};
///
/// // Load these once at the start of your program
/// let ps = SyntaxSet::load_defaults_newlines();
/// let ts = ThemeSet::load_defaults();
///
/// let syntax = ps.find_syntax_by_extension("rs").unwrap();
/// let s = "pub struct Wow { hi: u64 }\nfn blah() -> u64 {}\n";
///
/// let mut h = HighlightLines::new(syntax, &ts.themes["InspiredGitHub"]);
/// for line in LinesWithEndings::from(s) { // LinesWithEndings enables use of newlines mode
///     let ranges: Vec<(Style, &str)> = h.highlight_line(line, &ps).unwrap();
///     let escaped = as_latex_escaped(&ranges[..]);
///     println!("{}", escaped);
/// }
/// ```
///
/// Returned content is intended to be placed inside a fancyvrb
/// Verbatim environment:
///
/// ```latex
/// \usepackage{fancyvrb}
/// \usepackage{xcolor}
/// % ...
/// % enable comma-separated arguments inside \textcolor
/// \makeatletter
/// \def\verbatim@nolig@list{\do\`\do\<\do\>\do\'\do\-}
/// \makeatother
/// % ...
/// \begin{Verbatim}[commandchars=\\\{\}]
/// % content goes here
/// \end{Verbatim}
/// ```
///
/// Background color is ignored.
pub fn as_latex_escaped(v: &[(Style, &str)]) -> String {
    let mut s: String = String::new();
    let mut prev_style: Option<Style> = None;
    let mut content: String;
    fn textcolor(style: &Style, first: bool) -> String {
        format!(
            "{}\\textcolor[RGB]{{{},{},{}}}{{",
            if first { "" } else { "}" },
            style.foreground.r,
            style.foreground.b,
            style.foreground.g
        )
    }
    for &(style, text) in v.iter() {
        if let Some(ps) = prev_style {
            match text {
                " " => {
                    s.push(' ');
                    continue;
                }
                "\n" => continue,
                _ => (),
            }
            if style != ps {
                write!(s, "{}", textcolor(&style, false)).unwrap();
            }
        } else {
            write!(s, "{}", textcolor(&style, true)).unwrap();
        }
        content = text.to_string();
        for &(old, new) in LATEX_REPLACE.iter() {
            content = content.replace(old, new);
        }
        write!(s, "{}", &content).unwrap();
        prev_style = Some(style);
    }
    s.push('}');
    s
}

/// Print out the various push and pop operations in a vector
/// with visual alignment to the line. Obviously for debugging.
#[cfg(feature = "parsing")]
pub fn debug_print_ops(line: &str, ops: &[(usize, ScopeStackOp)]) {
    for &(i, ref op) in ops.iter() {
        println!("{}", line.trim_end());
        print!("{: <1$}", "", i);
        match *op {
            ScopeStackOp::Push(s) => {
                println!("^ +{}", s);
            }
            ScopeStackOp::Pop(count) => {
                println!("^ pop {}", count);
            }
            ScopeStackOp::Clear(amount) => {
                println!("^ clear {:?}", amount);
            }
            ScopeStackOp::Restore => println!("^ restore"),
            ScopeStackOp::Noop => println!("noop"),
        }
    }
}

/// An iterator over the lines of a string, including the line endings.
///
/// This is similar to the standard library's `lines` method on `str`, except
/// that the yielded lines include the trailing newline character(s).
///
/// You can use it if you're parsing/highlighting some text that you have as a
/// string. With this, you can use the "newlines" variant of syntax definitions,
/// which is recommended.
///
/// # Examples
///
/// ```
/// use syntect::util::LinesWithEndings;
///
/// let mut lines = LinesWithEndings::from("foo\nbar\nbaz");
///
/// assert_eq!(Some("foo\n"), lines.next());
/// assert_eq!(Some("bar\n"), lines.next());
/// assert_eq!(Some("baz"), lines.next());
///
/// assert_eq!(None, lines.next());
/// ```
pub struct LinesWithEndings<'a> {
    input: &'a str,
}

impl<'a> LinesWithEndings<'a> {
    pub fn from(input: &'a str) -> LinesWithEndings<'a> {
        LinesWithEndings { input }
    }
}

impl<'a> Iterator for LinesWithEndings<'a> {
    type Item = &'a str;

    #[inline]
    fn next(&mut self) -> Option<&'a str> {
        if self.input.is_empty() {
            return None;
        }
        let split = self
            .input
            .find('\n')
            .map(|i| i + 1)
            .unwrap_or_else(|| self.input.len());
        let (line, rest) = self.input.split_at(split);
        self.input = rest;
        Some(line)
    }
}

/// Split a highlighted line at a byte index in the line into a before and
/// after component.
///
/// This is just a helper that does the somewhat tricky logic including splitting
/// a span if the index lies on a boundary.
///
/// This can be used to extract a chunk of the line out for special treatment
/// like wrapping it in an HTML tag for extra styling.
///
/// Generic for testing purposes and fancier use cases, but intended for use with
/// the `Vec<(Style, &str)>` returned by `highlight` methods. Look at the source
/// code for `modify_range` for an example usage.
#[allow(clippy::type_complexity)]
pub fn split_at<'a, A: Clone>(
    v: &[(A, &'a str)],
    split_i: usize,
) -> (Vec<(A, &'a str)>, Vec<(A, &'a str)>) {
    // This function works by gradually reducing the problem into smaller sub-problems from the front
    let mut rest = v;
    let mut rest_split_i = split_i;

    // Consume all tokens before the split
    let mut before = Vec::new();
    for tok in rest {
        // Use for instead of a while to avoid bounds checks
        if tok.1.len() > rest_split_i {
            break;
        }
        before.push(tok.clone());
        rest_split_i -= tok.1.len();
    }
    rest = &rest[before.len()..];

    let mut after = Vec::new();
    // If necessary, split the token the split falls inside
    if !rest.is_empty() && rest_split_i > 0 {
        let mut rest_split_index = rest_split_i;
        // Splitting in the middle of a multibyte character causes panic,
        // so if index is in the middle of such a character,
        // reduce the index by 1.
        while !rest[0].1.is_char_boundary(rest_split_index) && rest_split_index > 0 {
            rest_split_index -= 1;
        }
        let (sa, sb) = rest[0].1.split_at(rest_split_index);
        before.push((rest[0].0.clone(), sa));
        after.push((rest[0].0.clone(), sb));
        rest = &rest[1..];
    }

    after.extend_from_slice(rest);

    (before, after)
}

/// Modify part of a highlighted line using a style modifier, useful for highlighting sections of a line.
///
/// # Examples
///
/// ```
/// use syntect::util::modify_range;
/// use syntect::highlighting::{Style, StyleModifier, FontStyle};
///
/// let plain = Style::default();
/// let boldmod = StyleModifier { foreground: None, background: None, font_style: Some(FontStyle::BOLD) };
/// let bold = plain.apply(boldmod);
///
/// let l = &[(plain, "abc"), (plain, "def"), (plain, "ghi")];
/// let l2 = modify_range(l, 1..6, boldmod);
/// assert_eq!(l2, &[(plain, "a"), (bold, "bc"), (bold, "def"), (plain, "ghi")]);
/// ```
pub fn modify_range<'a>(
    v: &[(Style, &'a str)],
    r: Range<usize>,
    modifier: StyleModifier,
) -> Vec<(Style, &'a str)> {
    let (mut result, in_and_after) = split_at(v, r.start);
    let (inside, mut after) = split_at(&in_and_after, r.end - r.start);

    result.extend(inside.iter().map(|(style, s)| (style.apply(modifier), *s)));
    result.append(&mut after);
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::highlighting::FontStyle;

    #[test]
    fn test_lines_with_endings() {
        fn lines(s: &str) -> Vec<&str> {
            LinesWithEndings::from(s).collect()
        }

        assert!(lines("").is_empty());
        assert_eq!(lines("f"), vec!["f"]);
        assert_eq!(lines("foo"), vec!["foo"]);
        assert_eq!(lines("foo\n"), vec!["foo\n"]);
        assert_eq!(lines("foo\nbar"), vec!["foo\n", "bar"]);
        assert_eq!(lines("foo\nbar\n"), vec!["foo\n", "bar\n"]);
        assert_eq!(lines("foo\r\nbar"), vec!["foo\r\n", "bar"]);
        assert_eq!(lines("foo\r\nbar\r\n"), vec!["foo\r\n", "bar\r\n"]);
        assert_eq!(lines("\nfoo"), vec!["\n", "foo"]);
        assert_eq!(lines("\n\n\n"), vec!["\n", "\n", "\n"]);
    }

    #[test]
    fn test_split_at() {
        let l: &[(u8, &str)] = &[];
        let (before, after) = split_at(l, 0); // empty
        assert_eq!((&before[..], &after[..]), (&[][..], &[][..]));

        let l = &[(0u8, "abc"), (1u8, "def"), (2u8, "ghi")];

        let (before, after) = split_at(l, 0); // at start
        assert_eq!(
            (&before[..], &after[..]),
            (&[][..], &[(0u8, "abc"), (1u8, "def"), (2u8, "ghi")][..])
        );

        let (before, after) = split_at(l, 4); // inside token
        assert_eq!(
            (&before[..], &after[..]),
            (
                &[(0u8, "abc"), (1u8, "d")][..],
                &[(1u8, "ef"), (2u8, "ghi")][..]
            )
        );

        let (before, after) = split_at(l, 3); // between tokens
        assert_eq!(
            (&before[..], &after[..]),
            (&[(0u8, "abc")][..], &[(1u8, "def"), (2u8, "ghi")][..])
        );

        let (before, after) = split_at(l, 9); // just after last token
        assert_eq!(
            (&before[..], &after[..]),
            (&[(0u8, "abc"), (1u8, "def"), (2u8, "ghi")][..], &[][..])
        );

        let (before, after) = split_at(l, 10); // out of bounds
        assert_eq!(
            (&before[..], &after[..]),
            (&[(0u8, "abc"), (1u8, "def"), (2u8, "ghi")][..], &[][..])
        );

        let l = &[(0u8, "こんにちは"), (1u8, "世界"), (2u8, "！")];

        let (before, after) = split_at(l, 3);

        assert_eq!(
            (&before[..], &after[..]),
            (
                &[(0u8, "こ")][..],
                &[(0u8, "んにちは"), (1u8, "世界"), (2u8, "！")][..]
            )
        );

        //Splitting inside a multibyte character could cause panic,
        //so if index is inside such a character,
        //index is decreased by 1.
        let (before, after) = split_at(l, 4);

        assert_eq!(
            (&before[..], &after[..]),
            (
                &[(0u8, "こ")][..],
                &[(0u8, "んにちは"), (1u8, "世界"), (2u8, "！")][..]
            )
        );
    }

    #[test]
    fn test_as_24_bit_terminal_escaped() {
        let style = Style {
            foreground: Color::WHITE,
            background: Color::BLACK,
            font_style: FontStyle::default(),
        };

        // With background
        let s = as_24_bit_terminal_escaped(&[(style, "hello")], true);
        assert_eq!(s, "\x1b[48;2;0;0;0m\x1b[38;2;255;255;255mhello");

        // Without background
        let s = as_24_bit_terminal_escaped(&[(style, "hello")], false);
        assert_eq!(s, "\x1b[38;2;255;255;255mhello");

        // Blend alpha
        let mut foreground = Color::WHITE;
        foreground.a = 128;
        let style = Style {
            foreground,
            background: Color::BLACK,
            font_style: FontStyle::default(),
        };
        let s = as_24_bit_terminal_escaped(&[(style, "hello")], true);
        assert_eq!(s, "\x1b[48;2;0;0;0m\x1b[38;2;128;128;128mhello");
    }
}
