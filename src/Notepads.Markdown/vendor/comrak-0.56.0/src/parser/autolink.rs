// Notepads Markdown modifications Copyright (c) 2026, Jiaqi (0x7c13) Liu.
// Original third-party licenses remain applicable; see the crate COPYING file.

macro_rules! native_some { ($label:lifetime, $value:expr) => { match $value { Some(value) => value, None => break $label None } }; }
use crate::native::Host;
use finl_unicode::categories::CharacterCategories;
use memchr_n::MemchrN;
use std::borrow::Cow;
use std::str;
use std::sync::OnceLock;

use crate::character_set::character_set;
use crate::ctype::{isalnum, isalpha, isspace};
use crate::nodes::{Node, NodeLink, NodeValue, Sourcepos, ValueText};
use crate::parser::Spx;
use crate::parser::inlines::{Subject, make_inline};

pub(crate) fn process_email_autolinks<'a, H: Host>(
    ctx: &mut crate::native::Context<'_, H>,
    arena: &'a crate::native::FallibleArena<crate::nodes::AstNode<'a>, H::Lease>,
    node: Node<'a>,
    contents: &mut ValueText,
    relaxed_autolinks: bool,
    sourcepos: &mut Sourcepos,
    spx: &mut Spx,
) -> Result<(), H::Error> {
    let contents = contents.owned_cow_mut().ok_or_else(|| {
        ctx.host
            .reject(crate::native::NativeViolation::InvalidProvenance)
    })?;
    'native: {
        let Some((mut post, mut before_len, mut skip)) =
            find_email_autolink(ctx, arena, contents, relaxed_autolinks)?
        else {
            break 'native;
        };

        let original = std::mem::take(contents);
        let mut offset = 0;
        let mut anchor = node;
        let mut current_sourcepos = *sourcepos;
        let mut first = true;

        loop {
            let initial_end_col = current_sourcepos.end.column;
            current_sourcepos.end.column = spx.consume(before_len);
            let nsp_end_col = spx.consume(skip);

            let before = ctx.string(&original[offset..offset + before_len])?;
            if first {
                *contents = before.into();
                *sourcepos = current_sourcepos;
                first = false;
            } else {
                let before = make_inline(
                    ctx,
                    arena,
                    NodeValue::Text(before.into()),
                    current_sourcepos,
                )?;
                anchor.insert_after(before);
                anchor = before;
            }

            let nsp: Sourcepos = (
                current_sourcepos.end.line,
                current_sourcepos.end.column + 1,
                current_sourcepos.end.line,
                nsp_end_col,
            )
                .into();
            post.data_mut().sourcepos = nsp;
            post.first_child().unwrap().data_mut().sourcepos = nsp;
            anchor.insert_after(post);
            anchor = post;

            offset += before_len + skip;
            if offset == original.len() {
                break;
            }

            let after_sourcepos: Sourcepos = (
                current_sourcepos.end.line,
                nsp.end.column + 1,
                current_sourcepos.end.line,
                initial_end_col,
            )
                .into();

            if let Some((next_post, next_before_len, next_skip)) =
                find_email_autolink(ctx, arena, &original[offset..], relaxed_autolinks)?
            {
                post = next_post;
                before_len = next_before_len;
                skip = next_skip;
                current_sourcepos = after_sourcepos;
            } else {
                let after = ctx.string(&original[offset..])?;
                let after =
                    make_inline(ctx, arena, NodeValue::Text(after.into()), after_sourcepos)?;
                anchor.insert_after(after);
                break;
            }
        }
        if let Cow::Owned(original) = original {
            ctx.discard_string(original)?;
        }
    };
    Ok(())
}

fn find_email_autolink<'a, H: Host>(
    ctx: &mut crate::native::Context<'_, H>,
    arena: &'a crate::native::FallibleArena<crate::nodes::AstNode<'a>, H::Lease>,
    contents: &str,
    relaxed_autolinks: bool,
) -> Result<Option<(Node<'a>, usize, usize)>, H::Error> {
    let result = 'native: {
        static MATCHER: OnceLock<MemchrN> = OnceLock::new();
        static RELAXED_MATCHER: OnceLock<MemchrN> = OnceLock::new();

        let matcher = if relaxed_autolinks {
            RELAXED_MATCHER.get_or_init(|| MemchrN::new(b"@"))
        } else {
            MATCHER.get_or_init(|| MemchrN::new(b"@[]"))
        };
        let bytes = contents.as_bytes();
        if !bytes.contains(&b'@') {
            break 'native None;
        }
        let mut bracket_opening = 0;

        for i in matcher.iter(bytes) {
            match bytes[i] {
                b'[' if !relaxed_autolinks => bracket_opening += 1,
                b']' if !relaxed_autolinks => bracket_opening -= 1,
                b'@' => {
                    if bracket_opening <= 0
                        && let Some((post, reverse, skip)) =
                            email_match(ctx, arena, contents, i, relaxed_autolinks)?
                    {
                        break 'native Some((post, i - reverse, skip));
                    }
                }
                _ => debug_assert!(false, "matcher will only report @, [, or ]"),
            }
        }

        None
    };
    Ok(result)
}

fn email_match<'a, H: Host>(
    ctx: &mut crate::native::Context<'_, H>,
    arena: &'a crate::native::FallibleArena<crate::nodes::AstNode<'a>, H::Lease>,
    contents: &str,
    i: usize,
    relaxed_autolinks: bool,
) -> Result<Option<(Node<'a>, usize, usize)>, H::Error> {
    let result = 'native: {
        const EMAIL_OK_SET: [bool; 256] = character_set!(b".+-_");

        let size = contents.len();
        let bytes = contents.as_bytes();

        let mut auto_mailto = true;
        let mut is_xmpp = false;
        let mut rewind = 0;

        while rewind < i {
            let c = bytes[i - rewind - 1];

            if isalnum(c) || EMAIL_OK_SET[c as usize] {
                rewind += 1;
                continue;
            }

            if c == b':' {
                if validate_protocol("mailto", contents, i - rewind - 1) {
                    auto_mailto = false;
                    rewind += 1;
                    continue;
                }

                if validate_protocol("xmpp", contents, i - rewind - 1) {
                    is_xmpp = true;
                    auto_mailto = false;
                    rewind += 1;
                    continue;
                }
            }

            break;
        }

        if rewind == 0 {
            break 'native None;
        }

        let mut link_end = 1;
        let mut np = 0;

        while link_end < size - i {
            let c = bytes[i + link_end];

            if isalnum(c) {
                // empty
            } else if c == b'@' {
                break 'native None;
            } else if c == b'.' && link_end < size - i - 1 && isalnum(bytes[i + link_end + 1]) {
                np += 1;
            } else if c == b'/' && is_xmpp {
                // xmpp allows a `/` in the url
            } else if c != b'-' && c != b'_' {
                break;
            }

            link_end += 1;
        }

        if link_end < 2
            || np == 0
            || (!isalpha(bytes[i + link_end - 1]) && bytes[i + link_end - 1] != b'.')
        {
            break 'native None;
        }

        link_end = autolink_delim(&contents[i..], link_end, relaxed_autolinks);
        if link_end == 0 {
            break 'native None;
        }

        let text = &contents[i - rewind..link_end + i];
        let url = if auto_mailto {
            let mut url = ctx.string("mailto:")?;
            ctx.append(&mut url, text)?;
            url
        } else {
            ctx.string(text)?
        };
        let title = ctx.empty_value();
        let link = ctx.boxed(NodeLink {
            url: url.into(),
            title,
        })?;

        let inl = make_inline(ctx, arena, NodeValue::Link(link), (0, 1, 0, 1).into())?;

        let label = ctx.string(text)?;
        inl.append(make_inline(
            ctx,
            arena,
            NodeValue::Text(label.into()),
            (0, 1, 0, 1).into(),
        )?);
        Some((inl, rewind, rewind + link_end))
    };
    Ok(result)
}

fn validate_protocol(protocol: &str, contents: &str, cursor: usize) -> bool {
    let size = contents.len();
    let bytes = contents.as_bytes();
    let mut rewind = 0;

    while rewind < cursor && isalpha(bytes[cursor - rewind - 1]) {
        rewind += 1;
    }

    size - cursor + rewind >= protocol.len() && &contents[cursor - rewind..cursor] == protocol
}

pub fn www_match<'a, H: Host>(
    subject: &mut Subject<'a, '_, '_, '_, '_, '_, '_, H>,
) -> Result<Option<(Node<'a>, usize, usize)>, H::Error> {
    let result = 'native: {
        const WWW_DELIMS: [bool; 256] = character_set!(b"*_~([");
        let i = subject.scanner.pos;
        let relaxed_autolinks = subject.options.parse.relaxed_autolinks;
        let bytes = subject.input.as_bytes();

        if i < subject.relaxed_autolink_dead_zone_end {
            break 'native None;
        }

        if i > 0 && !isspace(bytes[i - 1]) && !WWW_DELIMS[bytes[i - 1] as usize] {
            break 'native None;
        }

        if !subject.input[i..].starts_with("www.") {
            break 'native None;
        }

        // Skip over "www." for domain validation, but account for its length in the overall link
        let mut link_end =
            native_some!('native, check_domain(&subject.input[i + 4..], relaxed_autolinks)) + 4;

        while i + link_end < subject.input.len() && !isspace(bytes[i + link_end]) {
            // basic test to detect whether we're in a normal markdown link - not exhaustive
            if relaxed_autolinks && bytes[i + link_end - 1] == b']' && bytes[i + link_end] == b'(' {
                subject.relaxed_autolink_dead_zone_end = i + link_end;
                break 'native None;
            }
            link_end += 1;
        }

        link_end = autolink_delim(&subject.input[i..], link_end, relaxed_autolinks);

        let mut url = subject.ctx.string("http://")?;
        subject
            .ctx
            .append(&mut url, &subject.input[i..link_end + i])?;
        let title = subject.ctx.empty_value();
        let link = subject.ctx.boxed(NodeLink {
            url: url.into(),
            title,
        })?;

        let inl = make_inline(
            subject.ctx,
            subject.arena,
            NodeValue::Link(link),
            (0, 1, 0, 1).into(),
        )?;

        let label = subject.ctx.string(&subject.input[i..link_end + i])?;
        inl.append(make_inline(
            subject.ctx,
            subject.arena,
            NodeValue::Text(label.into()),
            (0, 1, 0, 1).into(),
        )?);
        Some((inl, 0, link_end))
    };
    Ok(result)
}

const MAX_DOMAIN_DOTS: usize = 10;

struct DomainScan {
    end: usize,
    dots: usize,
    uscored_host: bool,
}

fn scan_domain(data: &str) -> DomainScan {
    let mut end = 0;
    let mut dots = 0;
    let mut uscores = 0;
    let mut prev_uscores = 0;

    for (i, c) in data.char_indices() {
        match c {
            '\\' if i + 1 < data.len() => {}
            '_' => uscores += 1,
            '.' => {
                prev_uscores = uscores;
                uscores = 0;
                dots += 1;
            }
            c if is_valid_hostchar(c) || c == '-' => {}
            _ => {
                end = i;
                break;
            }
        }
        end = i + c.len_utf8();
    }

    DomainScan {
        end,
        dots,
        uscored_host: uscores > 0 || prev_uscores > 0,
    }
}

fn check_domain(data: &str, allow_short: bool) -> Option<usize> {
    let scan = scan_domain(data);

    if scan.dots > MAX_DOMAIN_DOTS {
        return Some(scan.end);
    }

    if scan.uscored_host {
        return None;
    }

    (allow_short || scan.dots > 0).then_some(scan.end)
}

fn is_valid_hostchar(ch: char) -> bool {
    !(ch.is_whitespace() || ch.is_punctuation() || ch.is_symbol())
}

fn autolink_delim(data: &str, mut link_end: usize, relaxed_autolinks: bool) -> usize {
    const LINK_END_ASSORTMENT: [bool; 256] = character_set!(b"?!.,:*_~'\"");
    // \u{2069} (Pop Directional Isolate)
    const LINK_END_UNICODE: [u8; 3] = [0xe2, 0x81, 0xa9];

    let bytes = data.as_bytes();
    let mut parentheses = (0, 0);
    let mut brackets = (0, 0);
    let mut braces = (0, 0);
    for (i, &b) in bytes.iter().enumerate().take(link_end) {
        if b == b'<' {
            link_end = i;
            break;
        }

        match b {
            b'(' => parentheses.0 += 1,
            b')' => parentheses.1 += 1,
            b'[' => brackets.0 += 1,
            b']' => brackets.1 += 1,
            b'{' => braces.0 += 1,
            b'}' => braces.1 += 1,
            _ => (),
        }
    }

    while link_end > 0 {
        let cclose = bytes[link_end - 1];
        let counts = match cclose {
            b')' => Some(parentheses),
            b']' if relaxed_autolinks => Some(brackets),
            b'}' if relaxed_autolinks => Some(braces),
            _ => None,
        };

        if LINK_END_ASSORTMENT[cclose as usize] {
            link_end -= 1;
        } else if cclose == b';' {
            let mut new_end = link_end - 2;

            while new_end > 0 && isalpha(bytes[new_end]) {
                new_end -= 1;
            }

            if new_end < link_end - 2 && bytes[new_end] == b'&' {
                link_end = new_end;
            } else {
                link_end -= 1;
            }
        } else if let Some((opening, closing)) = counts {
            if closing <= opening {
                break;
            }

            match cclose {
                b')' => parentheses.1 -= 1,
                b']' => brackets.1 -= 1,
                b'}' => braces.1 -= 1,
                _ => unreachable!(),
            }
            link_end -= 1;
        } else if cclose == LINK_END_UNICODE[2] {
            let slice = &bytes[link_end - LINK_END_UNICODE.len()..link_end];

            if slice == LINK_END_UNICODE {
                link_end -= LINK_END_UNICODE.len();
                break;
            }

            break;
        } else {
            break;
        }
    }

    link_end
}

pub fn url_match<'a, H: Host>(
    subject: &mut Subject<'a, '_, '_, '_, '_, '_, '_, H>,
) -> Result<Option<(Node<'a>, usize, usize)>, H::Error> {
    let result = 'native: {
        const SCHEMES: [&str; 3] = ["http", "https", "ftp"];

        let i = subject.scanner.pos;
        let relaxed_autolinks = subject.options.parse.relaxed_autolinks;
        let bytes = subject.input.as_bytes();
        let size = subject.input.len();

        if i < subject.relaxed_autolink_dead_zone_end {
            break 'native None;
        }

        if size - i < 4 || bytes[i + 1] != b'/' || bytes[i + 2] != b'/' {
            break 'native None;
        }

        let mut rewind = 0;
        while rewind < i && isalpha(bytes[i - rewind - 1]) {
            rewind += 1;
        }

        if !relaxed_autolinks {
            let scheme = &subject.input[i - rewind..i];
            let cond = |s: &&str| size - i + rewind >= s.len() && scheme.eq_ignore_ascii_case(s);
            if !SCHEMES.iter().any(cond) {
                break 'native None;
            }

            if !subject.input[i + 3..]
                .chars()
                .next()
                .is_some_and(is_valid_hostchar)
            {
                break 'native None;
            }
        }

        let mut link_end = native_some!('native, check_domain(&subject.input[i + 3..], true)) + 3;

        while link_end < size - i && !isspace(bytes[i + link_end]) {
            // basic test to detect whether we're in a normal markdown link - not exhaustive
            if relaxed_autolinks
                && link_end > 0
                && bytes[i + link_end - 1] == b']'
                && bytes[i + link_end] == b'('
            {
                subject.relaxed_autolink_dead_zone_end = i + link_end;
                break 'native None;
            }
            link_end += 1;
        }

        link_end = autolink_delim(&subject.input[i..], link_end, relaxed_autolinks);

        let url = &subject.input[i - rewind..i + link_end];
        let destination = subject.ctx.string(url)?;
        let title = subject.ctx.empty_value();
        let link = subject.ctx.boxed(NodeLink {
            url: destination.into(),
            title,
        })?;
        let inl = make_inline(
            subject.ctx,
            subject.arena,
            NodeValue::Link(link),
            (0, 1, 0, 1).into(),
        )?;

        let label = subject.ctx.string(url)?;
        inl.append(make_inline(
            subject.ctx,
            subject.arena,
            NodeValue::Text(label.into()),
            (0, 1, 0, 1).into(),
        )?);
        Some((inl, rewind, rewind + link_end))
    };
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::find_email_autolink;
    use crate::{Arena, native};

    #[test]
    fn email_autolink_bracket_depth() {
        let cases = [
            ("foo@example.com", false, Some((0, 15))),
            ("☃ foo@example.com", false, Some((4, 15))),
            ("[foo@example.com]", false, None),
            ("[[foo@example.com]] bar@example.com", false, Some((20, 15))),
            ("[foo@example.com]", true, Some((1, 15))),
            ("]][[foo@example.com", false, Some((4, 15))),
            ("[foo@example.com", false, None),
            ("[foo@example.com", true, Some((1, 15))),
            ("no email or brackets", false, None),
        ];

        for (contents, relaxed, expected) in cases {
            let arena = Arena::new();
            let mut host = native::LegacyHost;
            let mut ctx = native::Context::new(&mut host, contents.len());
            ctx.capture = false;
            let actual = find_email_autolink(&mut ctx, &arena, contents, relaxed)
                .unwrap()
                .map(|(_, before_len, skip)| (before_len, skip));
            assert_eq!(actual, expected, "{contents:?}, relaxed: {relaxed}");
        }
    }
}
