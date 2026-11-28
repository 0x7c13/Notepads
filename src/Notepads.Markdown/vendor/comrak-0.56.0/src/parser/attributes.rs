use std::mem;

use crate::nodes::Attributes;

#[derive(Default)]
enum Kind {
    #[default]
    Id,
    Class,
    Pair(String),
}

pub fn parse(input: &str) -> Option<(Attributes, usize)> {
    let mut ci = input.char_indices();
    if ci.next()?.1 != '{' {
        return None;
    }

    enum State {
        Betwixt,
        Value(Kind, String, Quote),
        Key(String),
        PostQuote,
    }

    enum Quote {
        Bare,
        Quoted,
        QuotedEscaped,
    }

    let mut state = State::Betwixt;
    let mut attrs = Attributes::default();

    // We permit ' ' | '\t' | '\r' | '\n' between/around attributes in the braces.
    // Is ASCII whitespace also '\v' | '\f'? Does anyone mind?

    loop {
        let (i, c) = ci.next()?;

        loop {
            match state {
                State::Betwixt => match c {
                    '#' => {
                        state = State::Value(Kind::Id, String::new(), Quote::Bare);
                        break;
                    }
                    '.' => {
                        state = State::Value(Kind::Class, String::new(), Quote::Bare);
                        break;
                    }
                    'a'..='z' | 'A'..='Z' => {
                        state = State::Key(c.to_string());
                        break;
                    }
                    '}' => return Some((attrs, i + 1)),
                    ' ' | '\t' | '\r' | '\n' => break,
                    _ => return None,
                },
                State::Value(ref mut kind, ref mut value, Quote::Bare) => match c {
                    '"' if value.is_empty() => {
                        state = State::Value(mem::take(kind), mem::take(value), Quote::Quoted);
                        break;
                    }
                    // An id can contain [literally anything] in HTML5, just so long as it's non-empty.
                    // Consider looking at how Pandoc parses this part.
                    // Per above, classes are also literally anything, *except* ASCII whitespace.
                    //
                    // [literally anything]: https://html.spec.whatwg.org/multipage/dom.html#the-id-attribute
                    //
                    // The below range of characters selected is arbitrary. Add to it if you like.
                    'a'..='z' | 'A'..='Z' | '0'..='9' | '-' | '_' | ':' | '.' | '%' => {
                        value.push(c);
                        break;
                    }
                    ' ' | '\t' | '\r' | '\n' | '}' => {
                        if value.is_empty() {
                            return None;
                        }
                        parse_attribute_value(kind, value, &mut attrs);
                        state = State::Betwixt;
                        // handle in loop
                    }
                    _ => return None,
                },
                State::Value(ref mut kind, ref mut value, Quote::Quoted) => match c {
                    '"' => {
                        parse_attribute_value(kind, value, &mut attrs);
                        state = State::PostQuote;
                        break;
                    }
                    '\\' => {
                        state =
                            State::Value(mem::take(kind), mem::take(value), Quote::QuotedEscaped);
                        break;
                    }
                    _ => {
                        value.push(c);
                        break;
                    }
                },
                State::Value(ref mut kind, ref mut value, Quote::QuotedEscaped) => {
                    value.push(c);
                    state = State::Value(mem::take(kind), mem::take(value), Quote::Quoted);
                    break;
                }
                State::Key(ref mut key) => match c {
                    // "Except where otherwise specified, attribute values on HTML elements may be any string value,
                    // including the empty string, and there is no restriction on what text can be specified in such
                    // attribute values."
                    // ¯\_(ツ)_/¯ I'm not driving
                    'a'..='z' | 'A'..='Z' | '0'..='9' | '-' | '_' | ':' | '.' => {
                        key.push(c);
                        break;
                    }
                    '=' => {
                        state =
                            State::Value(Kind::Pair(mem::take(key)), String::new(), Quote::Bare);
                        break;
                    }
                    _ => return None,
                },
                State::PostQuote => match c {
                    // Require a space after quote before anything else.
                    ' ' | '\t' | '\r' | '\n' | '}' => {
                        state = State::Betwixt;
                        // handle in loop
                    }
                    _ => return None,
                },
            }
        }
    }
}

fn parse_attribute_value(kind: &mut Kind, value: &mut String, attrs: &mut Attributes) {
    match kind {
        Kind::Id => attrs.id = Some(mem::take(value)),
        Kind::Class => attrs.classes.push(mem::take(value)),
        Kind::Pair(key) => attrs.pairs.push((mem::take(key), mem::take(value))),
    };
}

/// Use this only in contexts where there can *not* be any following text
/// and the attribute set still valid; i.e. blocks are OK, inlines are not.
pub fn parse_off(content: &mut String) -> Option<Attributes> {
    let mut ci = content.char_indices().rev();

    enum State {
        Pre,
        Within,
        Quoted,
        /// We've hit an opening quote, but there could be a backslash behind it;
        /// if there is, return to Quoted, otherwise go to Within and reparse.
        MaybeQuoteEnd,
    }

    let mut state = State::Pre;
    let mut end = None;

    loop {
        let (i, c) = ci.next()?;

        loop {
            match state {
                State::Pre => match c {
                    '}' => {
                        end = Some(i + 1);
                        state = State::Within;
                        break;
                    }
                    _ if c.is_whitespace() => break,
                    _ => return None,
                },
                State::Within => match c {
                    '"' => {
                        state = State::Quoted;
                        break;
                    }
                    '{' => {
                        let (attrs, j) = parse(&content[i..])?;

                        // We can't transition into Within without setting `end`.
                        // If these don't match up, something is Bad.
                        if i + j != end.unwrap() {
                            return None;
                        }

                        // Either there's nothing left (fine!) ORRRRR there's *at least*
                        // one (but possibly multiple) whitespace, which we truncate.
                        let Some((mut i, c)) = ci.next() else {
                            content.truncate(i);
                            return Some(attrs);
                        };

                        if !c.is_whitespace() {
                            return None;
                        }

                        for (i2, c) in ci {
                            if !c.is_whitespace() {
                                break;
                            }
                            i = i2;
                        }

                        content.truncate(i);
                        return Some(attrs);
                    }
                    _ => break,
                },
                State::Quoted => match c {
                    '"' => {
                        state = State::MaybeQuoteEnd;
                        break;
                    }
                    _ => break,
                },
                State::MaybeQuoteEnd => match c {
                    '\\' => {
                        state = State::Quoted;
                        break;
                    }
                    _ => {
                        state = State::Within;
                        // handle in loop
                    }
                },
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fundamentals() {
        assert_eq!(parse(""), None);

        assert_eq!(
            parse("{#henlo} there"),
            Some((
                Attributes {
                    id: Some("henlo".to_string()),
                    ..Default::default()
                },
                8
            ))
        );

        assert_eq!(
            parse("{.oken}"),
            Some((
                Attributes {
                    classes: vec!["oken".to_string()],
                    ..Default::default()
                },
                7
            ))
        );

        assert_eq!(
            parse("{data-thingy=ya}"),
            Some((
                Attributes {
                    pairs: vec![("data-thingy".to_string(), "ya".to_string())],
                    ..Default::default()
                },
                16
            ))
        );

        assert_eq!(
            parse("{.oken #yip m=26 .sure x=3% #yap} oh!"),
            Some((
                Attributes {
                    id: Some("yap".to_string()),
                    classes: vec!["oken".to_string(), "sure".to_string()],
                    pairs: vec![
                        ("m".to_string(), "26".to_string()),
                        ("x".to_string(), "3%".to_string())
                    ]
                },
                33
            ))
        );

        assert_eq!(
            parse("{#\"has space, will travel\" .\"ok\\\"en\" title=\"是非 \\\"not\\\"\"}"),
            Some((
                Attributes {
                    id: Some("has space, will travel".to_string()),
                    classes: vec!["ok\"en".to_string(),],
                    pairs: vec![("title".to_string(), "是非 \"not\"".to_string())],
                },
                60
            ))
        );

        assert_eq!(parse("{#}"), None);
        assert_eq!(parse("{}"), Some((Attributes::default(), 2)));
        assert_eq!(parse("{.}"), None);
        assert_eq!(parse("{uh"), None);

        assert_eq!(
            parse("{.why\n.not}"),
            Some((
                Attributes {
                    classes: vec!["why".to_string(), "not".to_string()],
                    ..Default::default()
                },
                11
            ))
        );

        // Consistent with Pandoc's commonmark+attributes.
        assert_eq!(parse("{hi}"), None);
        assert_eq!(parse("{hi=}"), None);
        assert_eq!(
            parse("{hi=\"\"}"),
            Some((
                Attributes {
                    pairs: vec![("hi".to_string(), String::new())],
                    ..Default::default()
                },
                7
            ))
        );
    }

    fn assert_parse_off(input: &str, expected_str: &str, expected_attrs: Option<Attributes>) {
        let mut s = input.to_string();
        let attrs = parse_off(&mut s);
        assert_eq!(s, expected_str);
        assert_eq!(attrs, expected_attrs);
    }

    #[test]
    fn parses_off() {
        assert_parse_off("hi", "hi", None);
        assert_parse_off(
            "hi  {#yay}",
            "hi",
            Some(Attributes {
                id: Some("yay".to_string()),
                ..Default::default()
            }),
        );

        assert_parse_off("Well!{#yay}", "Well!{#yay}", None);

        assert_parse_off(
            "Well! {#okay} {Not really.}",
            "Well! {#okay} {Not really.}",
            None,
        );

        assert_parse_off(
            "Well!! {cool=\"Lest \\\"{#confusion}\\\" sets in.\" } \t",
            "Well!!",
            Some(Attributes {
                pairs: vec![(
                    "cool".to_string(),
                    "Lest \"{#confusion}\" sets in.".to_string(),
                )],
                ..Default::default()
            }),
        );
    }
}
