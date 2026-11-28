//! Mermaid-like Markdown tokenization helpers.

use super::{is_ecmascript_whitespace, trim_ecmascript_whitespace};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MermaidMarkdownWordType {
    Normal,
    Strong,
    Em,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MermaidMarkdownAnalysis {
    pub(crate) lines: Vec<Vec<(String, MermaidMarkdownWordType)>>,
    pub(crate) has_styled_runs: bool,
    pub(crate) line_count: usize,
}

impl MermaidMarkdownAnalysis {
    pub(crate) fn all_runs_normal(&self) -> bool {
        !self.has_styled_runs
    }
}

pub(crate) fn analyze_mermaid_markdown(
    markdown: &str,
    markdown_auto_wrap: bool,
) -> MermaidMarkdownAnalysis {
    let lines = mermaid_markdown_to_lines(markdown, markdown_auto_wrap);
    let has_styled_runs = lines
        .iter()
        .flatten()
        .any(|(_, word_type)| !matches!(word_type, MermaidMarkdownWordType::Normal));
    let line_count = lines.len();
    MermaidMarkdownAnalysis {
        lines,
        has_styled_runs,
        line_count,
    }
}

/// Minimal, deterministic subset of Mermaid's `markdownToLines(...)` output.
///
/// This aims to match Mermaid's token boundaries for emphasis/strong delimiters (including `_`
/// behavior) well enough to reproduce upstream SVG-label layout and baseline DOM.
pub(crate) fn mermaid_markdown_to_lines(
    markdown: &str,
    markdown_auto_wrap: bool,
) -> Vec<Vec<(String, MermaidMarkdownWordType)>> {
    fn preprocess_mermaid_markdown(markdown: &str, markdown_auto_wrap: bool) -> String {
        let markdown = markdown.replace("\r\n", "\n");

        // Mermaid preprocessing:
        // - Replace `<br/>` with `\n`
        // - Replace multiple newlines with a single newline
        // - Dedent common indentation
        let mut s = markdown
            .replace("<br/>", "\n")
            .replace("<br />", "\n")
            .replace("<br>", "\n")
            .replace("</br>", "\n")
            .replace("</br/>", "\n")
            .replace("</br />", "\n")
            .replace("</br >", "\n");

        // Collapse multiple consecutive newlines to a single `\n`.
        let mut collapsed = String::with_capacity(s.len());
        let mut prev_nl = false;
        for ch in s.chars() {
            if ch == '\n' {
                if prev_nl {
                    continue;
                }
                prev_nl = true;
                collapsed.push('\n');
            } else {
                prev_nl = false;
                collapsed.push(ch);
            }
        }
        s = collapsed;

        // Dedent: remove the smallest common leading indentation of non-empty lines.
        let lines: Vec<&str> = s.split('\n').collect();
        let mut min_indent: Option<usize> = None;
        for l in &lines {
            if trim_ecmascript_whitespace(l).is_empty() {
                continue;
            }
            let indent = l
                .chars()
                .take_while(|c| *c == ' ' || *c == '\t')
                .map(|c| if c == '\t' { 4 } else { 1 })
                .sum::<usize>();
            min_indent = Some(min_indent.map_or(indent, |m| m.min(indent)));
        }
        let min_indent = min_indent.unwrap_or(0);
        if min_indent > 0 {
            let mut dedented = String::with_capacity(s.len());
            for (idx, l) in lines.iter().enumerate() {
                if idx > 0 {
                    dedented.push('\n');
                }
                let mut remaining = min_indent;
                let mut it = l.chars().peekable();
                while remaining > 0 {
                    match it.peek().copied() {
                        Some(' ') => {
                            let _ = it.next();
                            remaining = remaining.saturating_sub(1);
                        }
                        Some('\t') => {
                            let _ = it.next();
                            remaining = remaining.saturating_sub(4);
                        }
                        _ => break,
                    }
                }
                for ch in it {
                    dedented.push(ch);
                }
            }
            s = dedented;
        }

        if !markdown_auto_wrap {
            s = s.replace(' ', "&nbsp;");
        }
        s
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum DelimKind {
        Strong,
        Em,
    }

    fn is_punctuation(ch: char) -> bool {
        !is_ecmascript_whitespace(ch) && !ch.is_alphanumeric()
    }

    fn mermaid_delim_can_open_close(
        ch: char,
        prev: Option<char>,
        next: Option<char>,
    ) -> (bool, bool) {
        let prev_is_ws = prev.is_none_or(is_ecmascript_whitespace);
        let next_is_ws = next.is_none_or(is_ecmascript_whitespace);
        let prev_is_punct = prev.is_some_and(is_punctuation);
        let next_is_punct = next.is_some_and(is_punctuation);

        let left_flanking = !next_is_ws && (!next_is_punct || prev_is_ws || prev_is_punct);
        let right_flanking = !prev_is_ws && (!prev_is_punct || next_is_ws || next_is_punct);

        if ch == '_' {
            let can_open = left_flanking && (!right_flanking || prev_is_ws || prev_is_punct);
            let can_close = right_flanking && (!left_flanking || next_is_ws || next_is_punct);
            (can_open, can_close)
        } else {
            (left_flanking, right_flanking)
        }
    }

    fn mermaid_delim_has_closer(
        chars: &[char],
        mut i: usize,
        delimiter: char,
        run_len: usize,
    ) -> bool {
        let mut in_code_span = false;
        while i < chars.len() {
            if chars[i] == '<'
                && let Some(end) = chars[i..].iter().position(|ch| *ch == '>')
            {
                i += end + 1;
                continue;
            }
            if chars[i] == '`' {
                in_code_span = !in_code_span;
                i += 1;
                continue;
            }
            if !in_code_span && chars[i] == delimiter {
                let candidate_len = if i + 1 < chars.len() && chars[i + 1] == delimiter {
                    2
                } else {
                    1
                };
                let prev = i.checked_sub(1).map(|index| chars[index]);
                let next = chars.get(i + candidate_len).copied();
                let (_, can_close) = mermaid_delim_can_open_close(delimiter, prev, next);
                if candidate_len == run_len && can_close {
                    return true;
                }
                i += candidate_len;
                continue;
            }
            i += 1;
        }
        false
    }

    // Mermaid wraps SVG-label Markdown strings in single backticks; strip to avoid inline-code
    // suppressing `**`/`_` formatting.
    let markdown = if markdown.len() >= 2
        && markdown.starts_with('`')
        && markdown.ends_with('`')
        && !markdown.starts_with("``")
        && !markdown.ends_with("``")
    {
        &markdown[1..markdown.len() - 1]
    } else {
        markdown
    };

    let pre = preprocess_mermaid_markdown(markdown, markdown_auto_wrap);

    fn needs_full_delimiter_resolution(markdown: &str) -> bool {
        let chars: Vec<char> = markdown.chars().collect();
        if chars
            .windows(2)
            .any(|pair| (pair[0] == '*' && pair[1] == '_') || (pair[0] == '_' && pair[1] == '*'))
        {
            return true;
        }

        ['*', '_'].into_iter().any(|delimiter| {
            let mut has_single = false;
            let mut has_double = false;
            let mut index = 0;
            while index < chars.len() {
                if chars[index] != delimiter {
                    index += 1;
                    continue;
                }
                let run_len = chars[index..]
                    .iter()
                    .take_while(|candidate| **candidate == delimiter)
                    .count();
                match run_len {
                    1 => has_single = true,
                    2 => has_double = true,
                    _ => return true,
                }
                if has_single && has_double {
                    return true;
                }
                index += run_len;
            }
            false
        })
    }

    fn resolve_full_delimiter_runs(markdown: &str) -> Vec<Vec<(String, MermaidMarkdownWordType)>> {
        fn line_mut(
            out: &mut Vec<Vec<(String, MermaidMarkdownWordType)>>,
            line_idx: usize,
        ) -> &mut Vec<(String, MermaidMarkdownWordType)> {
            if out.len() <= line_idx {
                out.resize_with(line_idx + 1, Vec::new);
            }
            &mut out[line_idx]
        }

        fn append_text(
            out: &mut Vec<Vec<(String, MermaidMarkdownWordType)>>,
            line_idx: &mut usize,
            text: &str,
            word_type: MermaidMarkdownWordType,
            join_first_word: bool,
        ) {
            for (part_index, part) in text.split('\n').enumerate() {
                if part_index != 0 {
                    *line_idx += 1;
                    out.push(Vec::new());
                }
                for (word_index, word) in part.split(' ').enumerate() {
                    if word.is_empty() {
                        continue;
                    }
                    let word = word.replace("&#39;", "'");
                    let can_join = join_first_word && part_index == 0 && word_index == 0;
                    let line = line_mut(out, *line_idx);
                    if can_join
                        && let Some((previous, previous_type)) = line.last_mut()
                        && *previous_type == word_type
                    {
                        previous.push_str(&word);
                    } else {
                        line.push((word, word_type));
                    }
                }
            }
        }

        let parser = pulldown_cmark::Parser::new_ext(
            markdown,
            pulldown_cmark::Options::ENABLE_TABLES
                | pulldown_cmark::Options::ENABLE_STRIKETHROUGH
                | pulldown_cmark::Options::ENABLE_TASKLISTS,
        )
        .into_offset_iter();
        let mut out = vec![Vec::new()];
        let mut line_idx = 0;
        let mut style_stack = Vec::new();
        let mut in_paragraph = false;
        let mut skipped_inline_depth = 0usize;
        let mut skipped_block_depth = 0usize;
        let mut previous_text = None;

        for (event, range) in parser {
            if skipped_block_depth > 0 {
                match event {
                    pulldown_cmark::Event::Start(_) => skipped_block_depth += 1,
                    pulldown_cmark::Event::End(_) => skipped_block_depth -= 1,
                    _ => {}
                }
                continue;
            }

            match event {
                pulldown_cmark::Event::Start(pulldown_cmark::Tag::Paragraph) => {
                    in_paragraph = true;
                }
                pulldown_cmark::Event::Start(tag) if in_paragraph => {
                    if skipped_inline_depth > 0 {
                        skipped_inline_depth += 1;
                    } else {
                        match tag {
                            pulldown_cmark::Tag::Strong => {
                                style_stack.push(MermaidMarkdownWordType::Strong);
                            }
                            pulldown_cmark::Tag::Emphasis => {
                                style_stack.push(MermaidMarkdownWordType::Em);
                            }
                            _ => {
                                skipped_inline_depth = 1;
                                previous_text = None;
                            }
                        }
                    }
                }
                pulldown_cmark::Event::Start(_) => {
                    if let Some(raw) = markdown.get(range) {
                        line_mut(&mut out, line_idx)
                            .push((raw.to_string(), MermaidMarkdownWordType::Normal));
                    }
                    skipped_block_depth = 1;
                    previous_text = None;
                }
                pulldown_cmark::Event::End(pulldown_cmark::TagEnd::Paragraph) => {
                    in_paragraph = false;
                    skipped_inline_depth = 0;
                    style_stack.clear();
                    previous_text = None;
                }
                pulldown_cmark::Event::End(tag) if in_paragraph => {
                    if skipped_inline_depth > 0 {
                        skipped_inline_depth -= 1;
                        if skipped_inline_depth == 0 {
                            previous_text = None;
                        }
                    } else if matches!(
                        tag,
                        pulldown_cmark::TagEnd::Strong | pulldown_cmark::TagEnd::Emphasis
                    ) {
                        let _ = style_stack.pop();
                    }
                }
                pulldown_cmark::Event::Text(text) if in_paragraph && skipped_inline_depth == 0 => {
                    // pulldown-cmark eagerly synthesizes Unicode for HTML entities, while Marked
                    // keeps entity spellings in `node.text`. Preserve the source only when its
                    // full HTML-entity decode is exactly the emitted event; unrelated synthesized
                    // text (escapes, autolinks, and other extensions) keeps pulldown's payload.
                    let source_text = markdown
                        .get(range.clone())
                        .filter(|raw| {
                            raw.contains('&')
                                && merman_core::entities::decode_html_entities_to_unicode(raw)
                                    .as_ref()
                                    == text.as_ref()
                        })
                        .unwrap_or(text.as_ref());
                    let word_type = style_stack
                        .last()
                        .copied()
                        .unwrap_or(MermaidMarkdownWordType::Normal);
                    let join_first_word =
                        previous_text.is_some_and(|(end, can_join): (usize, bool)| {
                            end == range.start && can_join
                        }) && source_text
                            .chars()
                            .next()
                            .is_some_and(|character| !is_ecmascript_whitespace(character));
                    append_text(
                        &mut out,
                        &mut line_idx,
                        source_text,
                        word_type,
                        join_first_word,
                    );
                    previous_text = Some((
                        range.end,
                        source_text
                            .chars()
                            .last()
                            .is_some_and(|character| !is_ecmascript_whitespace(character)),
                    ));
                }
                pulldown_cmark::Event::Code(_) if in_paragraph && skipped_inline_depth == 0 => {
                    previous_text = None;
                }
                pulldown_cmark::Event::Html(html) | pulldown_cmark::Event::InlineHtml(html)
                    if skipped_inline_depth == 0 =>
                {
                    let raw = markdown.get(range).unwrap_or(html.as_ref());
                    line_mut(&mut out, line_idx)
                        .push((raw.to_string(), MermaidMarkdownWordType::Normal));
                    previous_text = None;
                }
                pulldown_cmark::Event::SoftBreak | pulldown_cmark::Event::HardBreak
                    if in_paragraph && skipped_inline_depth == 0 =>
                {
                    line_idx += 1;
                    out.push(Vec::new());
                    previous_text = None;
                }
                pulldown_cmark::Event::Rule if skipped_inline_depth == 0 => {
                    if let Some(raw) = markdown.get(range) {
                        line_mut(&mut out, line_idx)
                            .push((raw.to_string(), MermaidMarkdownWordType::Normal));
                    }
                    previous_text = None;
                }
                _ => {}
            }
        }

        while out.last().is_some_and(|line| line.is_empty()) && out.len() > 1 {
            out.pop();
        }
        out
    }

    if needs_full_delimiter_resolution(&pre) {
        return resolve_full_delimiter_runs(&pre);
    }

    let chars: Vec<char> = pre.chars().collect();

    let mut out: Vec<Vec<(String, MermaidMarkdownWordType)>> = vec![Vec::new()];
    let mut line_idx: usize = 0;

    let mut stack: Vec<MermaidMarkdownWordType> = vec![MermaidMarkdownWordType::Normal];
    let mut word = String::new();
    let mut word_ty = MermaidMarkdownWordType::Normal;
    let mut in_code_span = false;

    fn line_mut(
        out: &mut Vec<Vec<(String, MermaidMarkdownWordType)>>,
        line_idx: usize,
    ) -> &mut Vec<(String, MermaidMarkdownWordType)> {
        if out.len() <= line_idx {
            out.resize_with(line_idx + 1, Vec::new);
        }
        &mut out[line_idx]
    }

    let flush_word = |out: &mut Vec<Vec<(String, MermaidMarkdownWordType)>>,
                      line_idx: &mut usize,
                      word: &mut String,
                      word_ty: MermaidMarkdownWordType| {
        if word.is_empty() {
            return;
        }
        let mut w = std::mem::take(word);
        if w.contains("&#39;") {
            w = w.replace("&#39;", "'");
        }
        line_mut(out, *line_idx).push((w, word_ty));
    };

    let mut i = 0usize;
    while i < chars.len() {
        let ch = chars[i];

        if ch == '\n' {
            flush_word(&mut out, &mut line_idx, &mut word, word_ty);
            word_ty = *stack.last().unwrap_or(&MermaidMarkdownWordType::Normal);
            line_idx += 1;
            out.push(Vec::new());
            i += 1;
            continue;
        }
        if ch == ' ' {
            flush_word(&mut out, &mut line_idx, &mut word, word_ty);
            word_ty = *stack.last().unwrap_or(&MermaidMarkdownWordType::Normal);
            i += 1;
            continue;
        }

        if ch == '<'
            && let Some(end) = chars[i..].iter().position(|c| *c == '>')
        {
            let end = i + end;
            let html: String = chars[i..=end].iter().collect();
            flush_word(&mut out, &mut line_idx, &mut word, word_ty);
            if html.eq_ignore_ascii_case("<br>")
                || html.eq_ignore_ascii_case("<br/>")
                || html.eq_ignore_ascii_case("<br />")
                || html.eq_ignore_ascii_case("</br>")
                || html.eq_ignore_ascii_case("</br/>")
                || html.eq_ignore_ascii_case("</br />")
                || html.eq_ignore_ascii_case("</br >")
            {
                word_ty = *stack.last().unwrap_or(&MermaidMarkdownWordType::Normal);
                line_idx += 1;
                out.push(Vec::new());
            } else {
                line_mut(&mut out, line_idx).push((html, MermaidMarkdownWordType::Normal));
                word_ty = *stack.last().unwrap_or(&MermaidMarkdownWordType::Normal);
            }
            i = end + 1;
            continue;
        }

        if ch == '`' {
            if word.is_empty() {
                word_ty = *stack.last().unwrap_or(&MermaidMarkdownWordType::Normal);
            }
            word.push(ch);
            in_code_span = !in_code_span;
            i += 1;
            continue;
        }

        if ch == '*' || ch == '_' {
            if in_code_span {
                if word.is_empty() {
                    word_ty = *stack.last().unwrap_or(&MermaidMarkdownWordType::Normal);
                }
                word.push(ch);
                i += 1;
                continue;
            }

            let prev = i.checked_sub(1).map(|index| chars[index]);
            let run_len = if i + 1 < chars.len() && chars[i + 1] == ch {
                2
            } else {
                1
            };
            let kind = if run_len == 2 {
                DelimKind::Strong
            } else {
                DelimKind::Em
            };
            let next = chars.get(i + run_len).copied();
            let (can_open, can_close) = mermaid_delim_can_open_close(ch, prev, next);

            let want_ty = match kind {
                DelimKind::Strong => MermaidMarkdownWordType::Strong,
                DelimKind::Em => MermaidMarkdownWordType::Em,
            };
            let cur_ty = *stack.last().unwrap_or(&MermaidMarkdownWordType::Normal);

            if can_close && cur_ty == want_ty {
                flush_word(&mut out, &mut line_idx, &mut word, word_ty);
                stack.pop();
                word_ty = *stack.last().unwrap_or(&MermaidMarkdownWordType::Normal);
                i += run_len;
                continue;
            }
            if can_open && mermaid_delim_has_closer(&chars, i + run_len, ch, run_len) {
                flush_word(&mut out, &mut line_idx, &mut word, word_ty);
                stack.push(want_ty);
                word_ty = *stack.last().unwrap_or(&MermaidMarkdownWordType::Normal);
                i += run_len;
                continue;
            }

            // Treat the delimiter run as literal if it can't open/close. Mermaid's upstream
            // behavior does not reinterpret a failed `__` run as two separate `_` runs (e.g.
            // `a__b` must remain literal, not split into `a_` + `_b_`).
            if word.is_empty() {
                word_ty = *stack.last().unwrap_or(&MermaidMarkdownWordType::Normal);
            }
            for _ in 0..run_len {
                word.push(ch);
            }
            i += run_len;
            continue;
        }

        if word.is_empty() {
            word_ty = *stack.last().unwrap_or(&MermaidMarkdownWordType::Normal);
        }
        word.push(ch);
        i += 1;
    }

    flush_word(&mut out, &mut line_idx, &mut word, word_ty);

    if out.is_empty() {
        out.push(Vec::new());
    }
    while out.last().is_some_and(|l| l.is_empty()) && out.len() > 1 {
        out.pop();
    }
    out
}

pub(crate) fn mermaid_markdown_contains_html_tags(markdown: &str) -> bool {
    pulldown_cmark::Parser::new_ext(
        markdown,
        pulldown_cmark::Options::ENABLE_TABLES
            | pulldown_cmark::Options::ENABLE_STRIKETHROUGH
            | pulldown_cmark::Options::ENABLE_TASKLISTS,
    )
    .any(|ev| {
        matches!(
            ev,
            pulldown_cmark::Event::Html(_) | pulldown_cmark::Event::InlineHtml(_)
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn underscore_delimiters_match_mermaid() {
        use MermaidMarkdownWordType::*;

        assert_eq!(
            mermaid_markdown_to_lines("`a__b`", true),
            vec![vec![("a__b".to_string(), Normal)]]
        );
        assert_eq!(
            mermaid_markdown_to_lines("`_a_b_`", true),
            vec![vec![("a_b".to_string(), Em)]]
        );
        assert_eq!(
            mermaid_markdown_to_lines("`_a__b_`", true),
            vec![vec![("a__b".to_string(), Em)]]
        );
        assert_eq!(
            mermaid_markdown_to_lines("`__a__`", true),
            vec![vec![("a".to_string(), Strong)]]
        );
        // Marked 16.3 tokenizes this as three nested `em` nodes. Mermaid passes
        // the innermost parent type to the text token, so the effective word is `Em`.
        assert_eq!(
            mermaid_markdown_to_lines("`*_*word*_*`", true),
            vec![vec![("word".to_string(), Em)]]
        );
        assert_eq!(
            mermaid_markdown_to_lines("`***word***`", true),
            vec![vec![("word".to_string(), Strong)]]
        );
        assert_eq!(
            mermaid_markdown_to_lines("`___word___`", true),
            vec![vec![("word".to_string(), Strong)]]
        );
        assert_eq!(
            mermaid_markdown_to_lines("`**_word_**`", true),
            vec![vec![("word".to_string(), Em)]]
        );
        assert_eq!(
            mermaid_markdown_to_lines("`_*word*_`", true),
            vec![vec![("word".to_string(), Em)]]
        );
        assert_eq!(
            mermaid_markdown_to_lines("`***foo* bar**`", true),
            vec![vec![("foo".to_string(), Em), ("bar".to_string(), Strong),]]
        );
        assert_eq!(
            mermaid_markdown_to_lines("`***foo** bar*`", true),
            vec![vec![("foo".to_string(), Strong), ("bar".to_string(), Em),]]
        );
        assert_eq!(
            mermaid_markdown_to_lines("`**foo***`", true),
            vec![vec![("foo".to_string(), Strong), ("*".to_string(), Normal),]]
        );
        assert_eq!(
            mermaid_markdown_to_lines("`___foo_ bar__`", true),
            vec![vec![("foo".to_string(), Em), ("bar".to_string(), Strong),]]
        );
        assert_eq!(
            mermaid_markdown_to_lines("`**foo*`", true),
            vec![vec![("*".to_string(), Normal), ("foo".to_string(), Em),]]
        );
        assert_eq!(
            mermaid_markdown_to_lines("`*foo**`", true),
            vec![vec![("foo".to_string(), Em), ("*".to_string(), Normal),]]
        );
        assert_eq!(
            mermaid_markdown_to_lines("`__foo_`", true),
            vec![vec![("_".to_string(), Normal), ("foo".to_string(), Em),]]
        );
        assert_eq!(
            mermaid_markdown_to_lines("`_foo__`", true),
            vec![vec![("foo".to_string(), Em), ("_".to_string(), Normal),]]
        );
        assert_eq!(
            mermaid_markdown_to_lines("`**CoreResult<T>**`", true),
            vec![vec![
                ("CoreResult".to_string(), Strong),
                ("<T>".to_string(), Normal),
            ]]
        );
        assert_eq!(
            mermaid_markdown_to_lines("`**CoreResult&lt;T&gt;**`", true),
            vec![vec![("CoreResult&lt;T&gt;".to_string(), Strong)]]
        );
        assert_eq!(
            mermaid_markdown_to_lines(r"`**_styled_ \#tag &amp; value**`", true),
            vec![vec![
                ("styled".to_string(), Em),
                ("#tag".to_string(), Strong),
                ("&amp;".to_string(), Strong),
                ("value".to_string(), Strong),
            ]]
        );
        assert_eq!(
            mermaid_markdown_to_lines("`**CoreResult~T~**`", true),
            vec![vec![("CoreResult~T~".to_string(), Strong)]]
        );
    }

    #[test]
    fn full_delimiter_fallback_preserves_marked_entity_spelling() {
        use MermaidMarkdownWordType::*;

        assert_eq!(
            mermaid_markdown_to_lines("***&copy;***", true),
            vec![vec![("&copy;".to_string(), Strong)]],
        );
        assert_eq!(
            mermaid_markdown_to_lines("**_x_ &nbsp; y**", true),
            vec![vec![
                ("x".to_string(), Em),
                ("&nbsp;".to_string(), Strong),
                ("y".to_string(), Strong),
            ]],
        );
        assert_eq!(
            mermaid_markdown_to_lines("***&#160;***", true),
            vec![vec![("&#160;".to_string(), Strong)]],
        );
        assert_eq!(
            mermaid_markdown_to_lines("***x&nbsp;y***", true),
            vec![vec![("x&nbsp;y".to_string(), Strong)]],
        );
        assert_eq!(
            mermaid_markdown_to_lines("***&NotEqualTilde;***", true),
            vec![vec![("&NotEqualTilde;".to_string(), Strong)]],
        );
        assert_eq!(
            mermaid_markdown_to_lines("***&#39;***", true),
            vec![vec![("'".to_string(), Strong)]],
        );
    }

    #[test]
    fn inline_code_suppresses_emphasis_delimiters() {
        use MermaidMarkdownWordType::*;

        // Mermaid CLI baselines (class diagram HTML labels) preserve backticks and do not
        // interpret `**...**` inside them as strong/emphasis.
        assert_eq!(
            mermaid_markdown_to_lines("inline: `**not bold**`", true),
            vec![vec![
                ("inline:".to_string(), Normal),
                ("`**not".to_string(), Normal),
                ("bold**`".to_string(), Normal),
            ]]
        );
    }

    #[test]
    fn full_delimiter_fallback_matches_mermaid_inline_token_allowlist() {
        use MermaidMarkdownWordType::*;

        assert_eq!(
            mermaid_markdown_to_lines("***foo*** [bar](u)", true),
            vec![vec![("foo".to_string(), Strong)]],
        );
        assert_eq!(
            mermaid_markdown_to_lines("***foo*** ~~bar~~ baz", true),
            vec![vec![
                ("foo".to_string(), Strong),
                ("baz".to_string(), Normal),
            ]],
        );
        assert_eq!(
            mermaid_markdown_to_lines("***foo*** [bar **baz** `qux` <i>zap</i>](u) tail", true,),
            vec![vec![
                ("foo".to_string(), Strong),
                ("tail".to_string(), Normal),
            ]],
        );
        assert_eq!(
            mermaid_markdown_to_lines("***foo*** `bar` <i>baz</i>", true),
            vec![vec![
                ("foo".to_string(), Strong),
                ("<i>".to_string(), Normal),
                ("baz".to_string(), Normal),
                ("</i>".to_string(), Normal),
            ]],
        );
    }

    #[test]
    fn full_delimiter_fallback_preserves_unsupported_block_raw_text() {
        use MermaidMarkdownWordType::*;

        for (markdown, raw) in [
            ("# ***foo***", "# ***foo***"),
            ("- ***foo***\n- bar", "- ***foo***\n- bar"),
            ("> ***foo***", "> ***foo***"),
            ("```text\n***foo***\n```", "```text\n***foo***\n```"),
            ("<div>\n***foo***\n</div>", "<div>\n***foo***\n</div>"),
            (
                "| label |\n| --- |\n| ***foo*** |",
                "| label |\n| --- |\n| ***foo*** |",
            ),
        ] {
            assert_eq!(
                mermaid_markdown_to_lines(markdown, true),
                vec![vec![(raw.to_string(), Normal)]],
                "markdown={markdown:?}",
            );
        }
    }

    #[test]
    fn full_delimiter_fallback_preserves_rule_as_one_top_level_raw_token() {
        use MermaidMarkdownWordType::*;

        assert_eq!(
            mermaid_markdown_to_lines("***foo***\n- - -", true),
            vec![vec![
                ("foo".to_string(), Strong),
                ("- - -".to_string(), Normal),
            ]],
        );
        assert_eq!(
            mermaid_markdown_to_lines("# ***foo***\n- - -", true),
            vec![vec![
                ("# ***foo***\n".to_string(), Normal),
                ("- - -".to_string(), Normal),
            ]],
        );
    }

    #[test]
    fn full_delimiter_fallback_keeps_nested_inline_styles_inside_paragraphs() {
        use MermaidMarkdownWordType::*;

        assert_eq!(
            mermaid_markdown_to_lines("***foo _bar_*** baz", true),
            vec![vec![
                ("foo".to_string(), Strong),
                ("bar".to_string(), Em),
                ("baz".to_string(), Normal),
            ]],
        );
        assert_eq!(
            mermaid_markdown_to_lines("# ***foo***\n***bar*** baz", true),
            vec![vec![
                ("# ***foo***\n".to_string(), Normal),
                ("bar".to_string(), Strong),
                ("baz".to_string(), Normal),
            ]],
        );
    }

    #[test]
    fn html_tags_after_newline_stay_on_current_markdown_line() {
        use MermaidMarkdownWordType::*;

        assert_eq!(
            mermaid_markdown_to_lines("alpha\n<strong>bravo</strong>", true),
            vec![
                vec![("alpha".to_string(), Normal)],
                vec![
                    ("<strong>".to_string(), Normal),
                    ("bravo".to_string(), Normal),
                    ("</strong>".to_string(), Normal),
                ],
            ]
        );
    }

    #[test]
    fn svg_analysis_distinguishes_literal_markers_from_styled_runs() {
        for literal in [
            "driver_license",
            "*unclosed",
            "literal ` backtick",
            "`code`",
        ] {
            let analysis = analyze_mermaid_markdown(literal, true);
            assert!(
                analysis.all_runs_normal(),
                "literal={literal:?}, analysis={analysis:?}"
            );
            assert_eq!(analysis.line_count, 1, "literal={literal:?}");
        }

        for styled in ["*emphasis*", "__strong__"] {
            let analysis = analyze_mermaid_markdown(styled, true);
            assert!(
                analysis.has_styled_runs,
                "styled={styled:?}, analysis={analysis:?}"
            );
            assert_eq!(analysis.line_count, 1, "styled={styled:?}");
        }

        let multiline = analyze_mermaid_markdown("first<br/>second", true);
        assert_eq!(multiline.line_count, 2);
        assert!(multiline.all_runs_normal());
    }
}
