use crate::Result;
use std::borrow::Cow;

use super::util::{find_tag_end, next_svg_quoted_attr};
use crate::svg::pipeline::{SvgPostprocessContext, SvgPostprocessor};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CssOverridePolicy {
    #[default]
    Preserve,
    StripExistingImportant,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct CssOverridePostprocessor {
    policy: CssOverridePolicy,
}

impl CssOverridePostprocessor {
    pub fn new(policy: CssOverridePolicy) -> Self {
        Self { policy }
    }

    pub fn strip_existing_important() -> Self {
        Self::new(CssOverridePolicy::StripExistingImportant)
    }

    pub fn policy(&self) -> CssOverridePolicy {
        self.policy
    }
}

impl SvgPostprocessor for CssOverridePostprocessor {
    fn name(&self) -> &'static str {
        "css-override"
    }

    fn process<'a>(
        &self,
        svg: Cow<'a, str>,
        _ctx: &SvgPostprocessContext<'_>,
    ) -> Result<Cow<'a, str>> {
        match self.policy {
            CssOverridePolicy::Preserve => Ok(svg),
            CssOverridePolicy::StripExistingImportant => {
                Ok(Cow::Owned(strip_css_important(svg.as_ref())))
            }
        }
    }
}

pub(crate) fn strip_css_important(svg: &str) -> String {
    if !svg.contains('!') {
        return svg.to_string();
    }

    let svg = strip_css_important_in_style_elements(svg);
    strip_css_important_in_style_attrs(&svg)
}

fn strip_css_important_in_style_elements(svg: &str) -> String {
    let mut out = String::with_capacity(svg.len());
    let mut cursor = 0;
    let mut saw_style = false;

    while let Some(rel_start) = svg[cursor..].find("<style") {
        saw_style = true;
        let start = cursor + rel_start;
        out.push_str(&svg[cursor..start]);

        let Some(open_end) = find_tag_end(svg, start) else {
            out.push_str(&svg[start..]);
            return out;
        };

        let content_start = open_end + 1;
        let Some(rel_close_start) = svg[content_start..].find("</style") else {
            out.push_str(&svg[start..]);
            return out;
        };
        let close_start = content_start + rel_close_start;
        let Some(close_end) = find_tag_end(svg, close_start) else {
            out.push_str(&svg[start..]);
            return out;
        };

        out.push_str(&svg[start..=open_end]);
        out.push_str(&strip_css_important_from_css(
            &svg[content_start..close_start],
        ));
        out.push_str(&svg[close_start..=close_end]);
        cursor = close_end + 1;
    }

    if !saw_style {
        return svg.to_string();
    }

    out.push_str(&svg[cursor..]);
    out
}

fn strip_css_important_in_style_attrs(svg: &str) -> String {
    let mut out = String::with_capacity(svg.len());
    let mut cursor = 0;
    let mut saw_tag = false;

    while let Some(rel_start) = svg[cursor..].find('<') {
        saw_tag = true;
        let start = cursor + rel_start;
        out.push_str(&svg[cursor..start]);

        let Some(end) = find_tag_end(svg, start) else {
            out.push_str(&svg[start..]);
            return out;
        };

        out.push_str(&strip_css_important_in_tag_style_attrs(&svg[start..=end]));
        cursor = end + 1;
    }

    if !saw_tag {
        return svg.to_string();
    }

    out.push_str(&svg[cursor..]);
    out
}

fn strip_css_important_in_tag_style_attrs(tag: &str) -> Cow<'_, str> {
    if tag.starts_with("</")
        || tag.starts_with("<!--")
        || tag.starts_with("<!")
        || tag.starts_with("<?")
    {
        return Cow::Borrowed(tag);
    }

    let mut out = String::new();
    let mut copied_until = 0usize;
    let mut cursor = 0usize;
    let mut changed = false;

    while let Some(attr) = next_svg_quoted_attr(tag, cursor) {
        let name = &tag[attr.name_start..attr.name_end];
        if !name.eq_ignore_ascii_case("style") {
            cursor = attr.full_end;
            continue;
        }

        let value = &tag[attr.value_start..attr.value_end];
        let stripped = strip_css_important_from_css(value);
        if stripped == value {
            cursor = attr.full_end;
            continue;
        }

        if !changed {
            out = String::with_capacity(tag.len());
            changed = true;
        }
        out.push_str(&tag[copied_until..attr.value_start]);
        out.push_str(&stripped);
        copied_until = attr.value_end;
        cursor = attr.full_end;
    }

    if changed {
        out.push_str(&tag[copied_until..]);
        Cow::Owned(out)
    } else {
        Cow::Borrowed(tag)
    }
}

fn strip_css_important_from_css(css: &str) -> String {
    let mut out = String::with_capacity(css.len());
    let mut copied_until = 0usize;
    let mut search_from = 0usize;
    let mut stripped = false;

    while let Some(rel) = css[search_from..].find('!') {
        let bang = search_from + rel;
        if let Some((start, end)) = css_important_match_bounds_at_bang(css, bang) {
            out.push_str(&css[copied_until..start]);
            copied_until = end;
            search_from = end;
            stripped = true;
            continue;
        }

        search_from = bang + 1;
    }

    if !stripped {
        return css.to_string();
    }

    out.push_str(&css[copied_until..]);
    out
}

fn css_important_match_bounds_at_bang(svg: &str, bang: usize) -> Option<(usize, usize)> {
    let marker_end = bang + "!important".len();
    if !svg
        .get(bang..marker_end)?
        .eq_ignore_ascii_case("!important")
    {
        return None;
    }

    if let Some(next) = svg.get(marker_end..).and_then(|tail| tail.chars().next())
        && is_css_regex_word_char(next)
    {
        return None;
    }

    let start = svg[..bang]
        .char_indices()
        .rev()
        .find(|(_, ch)| !ch.is_whitespace())
        .map(|(idx, ch)| idx + ch.len_utf8())
        .unwrap_or(0);

    Some((start, marker_end))
}

fn is_css_regex_word_char(ch: char) -> bool {
    ch == '_' || ch.is_alphanumeric()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::svg::pipeline::SvgPipeline;

    fn render_session() -> crate::environment::RenderSession {
        crate::environment::RenderEnvironment::deterministic()
            .begin_session()
            .unwrap()
    }

    #[test]
    fn css_override_strips_important_only_when_requested() {
        let svg = r#"<svg><style>.node{fill:red !important;}</style></svg>"#;
        let session = render_session();

        let preserve = SvgPipeline::parity()
            .with_postprocessor(CssOverridePostprocessor::new(CssOverridePolicy::Preserve))
            .process_to_string(svg, &session)
            .unwrap();
        let strip = SvgPipeline::parity()
            .with_postprocessor(CssOverridePostprocessor::strip_existing_important())
            .process_to_string(svg, &session)
            .unwrap();

        assert!(preserve.contains("!important"));
        assert!(!strip.contains("!important"));
    }

    #[test]
    fn css_override_important_scanner_preserves_regex_boundaries() {
        assert_eq!(
            strip_css_important_from_css(
                ".a{fill:red\t!important;stroke:blue !IMPORTANT;color:green !importantfoo;}"
            ),
            ".a{fill:red;stroke:blue;color:green !importantfoo;}"
        );
        assert_eq!(
            strip_css_important_from_css(".a{fill:red!important-border;color:blue !importanté;}"),
            ".a{fill:red-border;color:blue !importanté;}"
        );
    }

    #[test]
    fn css_override_strips_important_only_from_css_contexts() {
        let svg = r#"<svg><style>.node{fill:red !important;}</style><text>keep !important</text><path data-note="keep !important" style="stroke: blue !important; fill: green"/></svg>"#;
        let out = strip_css_important(svg);

        assert!(out.contains(".node{fill:red;}"), "got: {out}");
        assert!(
            out.contains("style=\"stroke: blue; fill: green\""),
            "got: {out}"
        );
        assert!(out.contains("<text>keep !important</text>"), "got: {out}");
        assert!(out.contains("data-note=\"keep !important\""), "got: {out}");
    }

    #[test]
    fn css_override_handles_single_quoted_style_attributes() {
        let svg = r#"<svg><path style='stroke: blue !important; fill: green'/></svg>"#;
        let out = strip_css_important(svg);

        assert!(
            out.contains("style='stroke: blue; fill: green'"),
            "got: {out}"
        );
    }
}
