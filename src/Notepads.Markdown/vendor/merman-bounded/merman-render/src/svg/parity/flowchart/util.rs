//! Flowchart-specific rendering helpers.

/// Flowchart HTML labels use fixed-size `foreignObject` boxes computed during headless layout.
/// Keep those boxes non-clipping so browser-specific font fallback does not drop terminal glyphs
/// when the actual display font is slightly wider than the headless measurement. Mermaid expresses
/// this through the inline style only; a duplicate presentation attribute changes the SVG DOM.
pub(super) const HTML_LABEL_FOREIGN_OBJECT_OVERFLOW_ATTR: &str = r#" style="overflow: visible;""#;

pub(super) struct OptionalStyleAttr<'a>(pub(super) &'a str);

impl std::fmt::Display for OptionalStyleAttr<'_> {
    #[inline]
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.0.trim().is_empty() {
            return Ok(());
        }
        write!(
            f,
            r#" style="{}""#,
            super::super::util::escape_attr_display(self.0)
        )
    }
}

pub(super) struct OptionalStyleXmlAttr<'a>(pub(super) &'a str);

impl std::fmt::Display for OptionalStyleXmlAttr<'_> {
    #[inline]
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = self.0.trim();
        if s.is_empty() {
            return Ok(());
        }
        write!(
            f,
            r#" style="{}""#,
            super::super::util::escape_xml_display(s)
        )
    }
}
