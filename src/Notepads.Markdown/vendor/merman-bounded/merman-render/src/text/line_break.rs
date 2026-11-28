//! Unicode line-breaking primitives for browser-like HTML labels.

/// Replaces tags matched by Mermaid's `common.lineBreakRegex` (`/<\/?br\s*\/?>/gi`).
/// ELK applies this before Markdown parsing, including for State and Class edge labels.
pub(crate) fn mermaid_html_breaks_to_newlines(text: &str) -> std::borrow::Cow<'_, str> {
    let mut output: Option<String> = None;
    let mut copied_until = 0;
    for (start, _) in text.match_indices('<') {
        let rest = &text[start + 1..];
        let rest = rest.strip_prefix('/').unwrap_or(rest);
        if !rest
            .get(..2)
            .is_some_and(|name| name.eq_ignore_ascii_case("br"))
        {
            continue;
        }
        let rest = rest[2..].trim_start_matches(super::is_ecmascript_whitespace);
        let rest = rest.strip_prefix('/').unwrap_or(rest);
        let Some(after_tag) = rest.strip_prefix('>') else {
            continue;
        };
        let output = output.get_or_insert_with(|| String::with_capacity(text.len()));
        output.push_str(&text[copied_until..start]);
        output.push('\n');
        copied_until = text.len() - after_tag.len();
    }
    match output {
        Some(mut output) => {
            output.push_str(&text[copied_until..]);
            std::borrow::Cow::Owned(output)
        }
        None => std::borrow::Cow::Borrowed(text),
    }
}

/// Returns the atomic line-box segments for Mermaid's wrapped HTML labels.
///
/// `unicode_linebreak` supplies the UAX #14 soft and mandatory opportunities. Chromium's CSS
/// line breaker suppresses the library's soft opportunity immediately after `/`; this keeps path
/// and URI components together while retaining opportunities after characters such as `-` and
/// `?`. CSS `white-space: break-spaces` additionally permits a break after every preserved U+0020,
/// so runs of spaces are divided without discarding the spaces themselves.
pub(crate) fn html_break_spaces_segments(text: &str) -> Vec<&str> {
    match html_break_spaces_segments_with_checkpoints(text, &mut || {
        Ok::<(), std::convert::Infallible>(())
    }) {
        Ok(segments) => segments,
        Err(error) => match error {},
    }
}

/// Shares browser break opportunities with cancellable SVG label conversion.
pub(crate) fn html_break_spaces_segments_with_checkpoints<'a, E>(
    text: &'a str,
    checkpoint: &mut impl FnMut() -> Result<(), E>,
) -> Result<Vec<&'a str>, E> {
    html_break_spaces_segments_with_metadata_and_checkpoints(text, checkpoint)
        .map(|segments| segments.into_iter().map(|segment| segment.text).collect())
}

/// The action available at the end of an atomic HTML line-box segment.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum HtmlBreakKind {
    /// An ordinary discretionary break without an inserted glyph.
    Ordinary,
    /// A U+00AD opportunity that displays a hyphen only when the break is selected.
    SoftHyphen,
    /// A hard break or the synthetic end-of-text opportunity.
    Mandatory,
}

/// A borrowed atomic segment together with its line break action.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct HtmlBreakSegment<'a> {
    pub(crate) text: &'a str,
    pub(crate) end: HtmlBreakKind,
}

/// Shares browser break opportunities and conditional glyphs with cancellable wrapping.
pub(crate) fn html_break_spaces_segments_with_metadata_and_checkpoints<'a, E>(
    text: &'a str,
    checkpoint: &mut impl FnMut() -> Result<(), E>,
) -> Result<Vec<HtmlBreakSegment<'a>>, E> {
    checkpoint()?;
    if text.is_empty() {
        return Ok(vec![HtmlBreakSegment {
            text,
            end: HtmlBreakKind::Mandatory,
        }]);
    }

    let mut segments = Vec::new();
    let mut segment_start = 0usize;
    let checked_chars = CheckpointChars {
        chars: text.char_indices(),
        checkpoint,
        processed: 0,
        error: None,
    };
    let mut opportunities = unicode_linebreak::linebreaks_iter(checked_chars, text.len());
    while let Some((segment_end, opportunity)) = opportunities.next() {
        let input = opportunities.get_mut();
        // Early input termination creates a synthetic EOT break. It must never turn a cancelled
        // scan into a successful segment or expose offsets beyond the characters actually read.
        if let Some(error) = input.error.take() {
            return Err(error);
        }
        (input.checkpoint)()?;
        if opportunity == unicode_linebreak::BreakOpportunity::Allowed
            && text[..segment_end].ends_with('/')
        {
            continue;
        }
        let end = match opportunity {
            unicode_linebreak::BreakOpportunity::Mandatory => HtmlBreakKind::Mandatory,
            unicode_linebreak::BreakOpportunity::Allowed
                if text[..segment_end].ends_with('\u{00ad}') =>
            {
                HtmlBreakKind::SoftHyphen
            }
            unicode_linebreak::BreakOpportunity::Allowed => HtmlBreakKind::Ordinary,
        };
        push_break_spaces_segments(
            text,
            segment_start,
            segment_end,
            end,
            &mut segments,
            input.checkpoint,
        )?;
        segment_start = segment_end;
    }

    let input = opportunities.get_mut();
    if let Some(error) = input.error.take() {
        return Err(error);
    }

    // UAX #14 emits a mandatory opportunity at the end of non-empty text. Keep the remainder as
    // a defensive fallback in case the dependency changes that iterator contract.
    if segment_start < text.len() {
        push_break_spaces_segments(
            text,
            segment_start,
            text.len(),
            HtmlBreakKind::Mandatory,
            &mut segments,
            input.checkpoint,
        )?;
    }

    (input.checkpoint)()?;
    if segments.is_empty() {
        Ok(vec![HtmlBreakSegment {
            text,
            end: HtmlBreakKind::Mandatory,
        }])
    } else {
        Ok(segments)
    }
}

/// Checks before the Unicode iterator consumes a batch, including when no break is yielded.
struct CheckpointChars<'a, 'c, C, E> {
    chars: std::str::CharIndices<'a>,
    checkpoint: &'c mut C,
    processed: usize,
    error: Option<E>,
}

impl<C: FnMut() -> Result<(), E>, E> Iterator for CheckpointChars<'_, '_, C, E> {
    type Item = (usize, char);

    fn next(&mut self) -> Option<Self::Item> {
        if self.error.is_some() {
            return None;
        }
        if self.processed.is_multiple_of(64)
            && let Err(error) = (self.checkpoint)()
        {
            self.error = Some(error);
            return None;
        }
        let item = self.chars.next()?;
        self.processed += 1;
        Some(item)
    }
}

/// Reports whether browser-like `white-space: break-spaces` layout has an internal soft break.
///
/// Deriving this from the same atomic segments used by wrapping and min-content sizing keeps
/// callers from guessing breakability from ASCII whitespace alone. A single segment means that
/// shrinking the containing block cannot reflow the text without an additional CSS breaking rule.
pub(crate) fn html_has_soft_break_opportunity(text: &str) -> bool {
    html_break_spaces_segments(text).len() > 1
}

fn push_break_spaces_segments<'a, E>(
    text: &'a str,
    start: usize,
    end: usize,
    end_kind: HtmlBreakKind,
    segments: &mut Vec<HtmlBreakSegment<'a>>,
    checkpoint: &mut impl FnMut() -> Result<(), E>,
) -> Result<(), E> {
    if start >= end {
        return Ok(());
    }

    let mut part_start = start;
    for (iteration, (offset, ch)) in text[start..end].char_indices().enumerate() {
        if iteration.is_multiple_of(64) {
            checkpoint()?;
        }
        if ch != ' ' {
            continue;
        }
        let part_end = start + offset + ch.len_utf8();
        segments.push(HtmlBreakSegment {
            text: &text[part_start..part_end],
            end: if part_end == end {
                end_kind
            } else {
                HtmlBreakKind::Ordinary
            },
        });
        part_start = part_end;
    }
    if part_start < end {
        segments.push(HtmlBreakSegment {
            text: &text[part_start..end],
            end: end_kind,
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        CheckpointChars, HtmlBreakKind, HtmlBreakSegment, html_break_spaces_segments,
        html_break_spaces_segments_with_checkpoints,
        html_break_spaces_segments_with_metadata_and_checkpoints, html_has_soft_break_opportunity,
        mermaid_html_breaks_to_newlines,
    };

    #[test]
    fn mermaid_html_breaks_match_source_tags_and_javascript_whitespace() {
        assert_eq!(
            mermaid_html_breaks_to_newlines("a<BR />b</br>c<br\u{feff}/>d"),
            "a\nb\nc\nd"
        );
        let unchanged = "a<br class=x>b<br\u{0085}>c<br / >d<break>e\\n";
        assert!(
            matches!(mermaid_html_breaks_to_newlines(unchanged), std::borrow::Cow::Borrowed(value) if value == unchanged)
        );
        assert_eq!(
            mermaid_html_breaks_to_newlines("<broken<br/>中文</BR >"),
            "<broken\n中文\n"
        );
    }

    #[test]
    fn follows_browser_line_breaking_for_prose_cjk_and_url_boundaries() {
        assert_eq!(html_break_spaces_segments("alpha-beta"), ["alpha-", "beta"]);
        assert_eq!(
            html_break_spaces_segments("alpha\u{200b}beta"),
            ["alpha\u{200b}", "beta"]
        );
        assert_eq!(
            html_break_spaces_segments("负责人审批"),
            ["负", "责", "人", "审", "批"]
        );
        assert_eq!(
            html_break_spaces_segments("https://x.test/(alpha)/z"),
            ["https://x.test/(alpha)/z"]
        );
        assert_eq!(
            html_break_spaces_segments(
                "https://example.com/api/v1/some(very-long)/resource-name?query=foo_bar&baz=qux"
            ),
            [
                "https://example.com/api/v1/some(very-",
                "long)/resource-",
                "name?",
                "query=foo_bar&baz=qux"
            ]
        );
    }

    #[test]
    fn reports_soft_breaks_from_the_same_browser_segments() {
        assert!(!html_has_soft_break_opportunity(""));
        assert!(!html_has_soft_break_opportunity("unbroken"));
        assert!(!html_has_soft_break_opportunity("https://x.test/(alpha)/z"));
        assert!(html_has_soft_break_opportunity("alpha beta"));
        assert!(html_has_soft_break_opportunity("alpha-beta"));
        assert!(html_has_soft_break_opportunity("负责人审批"));
        assert!(html_has_soft_break_opportunity(
            "https://example.com/api/v1/some(very-long)/resource-name?query=foo_bar&baz=qux"
        ));
    }

    #[test]
    fn break_spaces_preserves_each_space_before_its_break() {
        assert_eq!(html_break_spaces_segments("a  b"), ["a ", " ", "b"]);
        assert_eq!(html_break_spaces_segments("  "), [" ", " "]);
    }

    #[test]
    fn controlled_segmentation_propagates_cancellation() {
        let text = "unbreakable".repeat(1024);
        let mut calls = 0;
        let result = html_break_spaces_segments_with_checkpoints(&text, &mut || {
            calls += 1;
            if calls == 4 { Err("cancelled") } else { Ok(()) }
        });
        assert_eq!(result, Err("cancelled"));
        assert_eq!(calls, 4);
    }

    #[test]
    fn first_unicode_scan_stops_before_consuming_the_65th_character() {
        // These include contexts that cannot be restarted safely at a fixed chunk boundary.
        for text in [
            "unbreakable".repeat(1024),
            format!("a{}b", "\u{0301}".repeat(1024)),
            "\u{200d}".repeat(1024),
            "🇦".repeat(1024),
            " ".repeat(1024),
        ] {
            let mut calls = 0;
            let mut checkpoint = || {
                calls += 1;
                if calls == 2 { Err("cancelled") } else { Ok(()) }
            };
            let chars = CheckpointChars {
                chars: text.char_indices(),
                checkpoint: &mut checkpoint,
                processed: 0,
                error: None,
            };
            let mut opportunities = unicode_linebreak::linebreaks_iter(chars, text.len());
            while opportunities.next().is_some() {}
            let input = opportunities.get_mut();
            assert_eq!(input.processed, 64, "{text:?}");
            assert_eq!(input.error, Some("cancelled"));
            let unread = text.char_indices().nth(64).unwrap().0;
            assert_eq!(input.chars.as_str(), &text[unread..]);
        }
    }

    #[test]
    fn checked_unicode_scan_keeps_context_across_checkpoint_boundaries() {
        for text in [
            format!("a{}b c", "\u{0301}".repeat(129)),
            format!("a{}b", " ".repeat(129)),
            "🇦".repeat(129),
            format!("👩{}👩", "\u{200d}".repeat(129)),
            "각나\r\n中文".repeat(32),
        ] {
            let mut checkpoint = || Ok::<(), &'static str>(());
            let chars = CheckpointChars {
                chars: text.char_indices(),
                checkpoint: &mut checkpoint,
                processed: 0,
                error: None,
            };
            assert!(
                unicode_linebreak::linebreaks_iter(chars, text.len())
                    .eq(unicode_linebreak::linebreaks(&text)),
                "{text:?}"
            );
        }
    }

    #[test]
    fn segment_metadata_distinguishes_conditional_glyphs_and_hard_breaks() {
        let mut checkpoint = || Ok::<(), &'static str>(());
        assert_eq!(
            html_break_spaces_segments_with_metadata_and_checkpoints(
                "alpha\u{00ad}beta\r\n中文",
                &mut checkpoint,
            )
            .unwrap(),
            [
                HtmlBreakSegment {
                    text: "alpha\u{00ad}",
                    end: HtmlBreakKind::SoftHyphen,
                },
                HtmlBreakSegment {
                    text: "beta\r\n",
                    end: HtmlBreakKind::Mandatory,
                },
                HtmlBreakSegment {
                    text: "中",
                    end: HtmlBreakKind::Ordinary,
                },
                HtmlBreakSegment {
                    text: "文",
                    end: HtmlBreakKind::Mandatory,
                },
            ]
        );
        assert_eq!(
            html_break_spaces_segments_with_metadata_and_checkpoints("a  ", &mut checkpoint)
                .unwrap(),
            [
                HtmlBreakSegment {
                    text: "a ",
                    end: HtmlBreakKind::Ordinary,
                },
                HtmlBreakSegment {
                    text: " ",
                    end: HtmlBreakKind::Mandatory,
                },
            ]
        );
        assert_eq!(
            html_break_spaces_segments_with_metadata_and_checkpoints(
                "alpha\u{00ad}",
                &mut checkpoint
            )
            .unwrap(),
            [HtmlBreakSegment {
                text: "alpha\u{00ad}",
                end: HtmlBreakKind::Mandatory,
            }]
        );
    }
}
