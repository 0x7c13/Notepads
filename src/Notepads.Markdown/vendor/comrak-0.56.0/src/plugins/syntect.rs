//! Adapter for the Syntect syntax highlighter plugin.

use std::borrow::Cow;
use std::collections::{HashMap, hash_map};
use std::fmt::{self, Write};
use syntect::Error;
use syntect::easy::HighlightLines;
use syntect::highlighting::{Color, ThemeSet};
use syntect::html::{
    ClassStyle, ClassedHTMLGenerator, IncludeBackground, append_highlighted_html_for_styled_line,
};
use syntect::parsing::{SyntaxReference, SyntaxSet};
use syntect::util::LinesWithEndings;

use crate::adapters::SyntaxHighlighterAdapter;
use crate::html;

#[derive(Debug)]
/// Syntect syntax highlighter plugin.
pub struct SyntectAdapter {
    theme: Option<String>,
    syntax_set: SyntaxSet,
    theme_set: ThemeSet,
    css_class_prefix: Option<&'static str>, // syntect::html::ClassStyle::SpacePrefixed requires prefix to be &'static str
}

impl SyntectAdapter {
    /// Construct a new `SyntectAdapter` object and set the syntax highlighting theme.
    /// If `None` is specified, apply CSS classes instead.
    pub fn new(theme: Option<&str>) -> Self {
        SyntectAdapter {
            theme: theme.map(String::from),
            syntax_set: SyntaxSet::load_defaults_newlines(),
            theme_set: ThemeSet::load_defaults(),
            css_class_prefix: None,
        }
    }

    fn highlight_html(&self, code: &str, syntax: &SyntaxReference) -> Result<String, Error> {
        match &self.theme {
            Some(theme) => {
                // syntect::html::highlighted_html_for_string, without the opening/closing <pre>.
                let theme = &self.theme_set.themes[theme];
                let mut highlighter = HighlightLines::new(syntax, theme);

                let bg = theme.settings.background.unwrap_or(Color::WHITE);

                let mut output = String::new();
                for line in LinesWithEndings::from(code) {
                    let regions = highlighter.highlight_line(line, &self.syntax_set)?;
                    append_highlighted_html_for_styled_line(
                        &regions[..],
                        IncludeBackground::IfDifferent(bg),
                        &mut output,
                    )?;
                }
                Ok(output)
            }
            None => {
                // fall back to HTML classes.
                let class_style = match &self.css_class_prefix {
                    None => ClassStyle::Spaced,
                    Some(prefix) => ClassStyle::SpacedPrefixed { prefix },
                };
                let mut html_generator = ClassedHTMLGenerator::new_with_class_style(
                    syntax,
                    &self.syntax_set,
                    class_style,
                );
                for line in LinesWithEndings::from(code) {
                    html_generator.parse_html_for_line_which_includes_newline(line)?;
                }
                Ok(html_generator.finalize())
            }
        }
    }
}

impl SyntaxHighlighterAdapter for SyntectAdapter {
    fn write_highlighted(
        &self,
        output: &mut dyn Write,
        lang: Option<&str>,
        code: &str,
    ) -> fmt::Result {
        let fallback_syntax = "Plain Text";

        let lang: &str = match lang {
            Some(l) if !l.is_empty() => l.split_once(',').map(|(left, _)| left).unwrap_or(l),
            _ => fallback_syntax,
        };

        let syntax = self
            .syntax_set
            .find_syntax_by_token(lang)
            .unwrap_or_else(|| {
                self.syntax_set
                    .find_syntax_by_first_line(code)
                    .unwrap_or_else(|| self.syntax_set.find_syntax_plain_text())
            });

        match self.highlight_html(code, syntax) {
            Ok(highlighted_code) => output.write_str(&highlighted_code),
            Err(_) => output.write_str(code),
        }
    }

    fn write_pre_tag(
        &self,
        output: &mut dyn Write,
        attributes: HashMap<&'static str, Cow<'_, str>>,
    ) -> fmt::Result {
        match &self.theme {
            Some(theme) => {
                let theme = &self.theme_set.themes[theme];
                let colour = theme.settings.background.unwrap_or(Color::WHITE);

                let style = format!(
                    "background-color:#{:02x}{:02x}{:02x};",
                    colour.r, colour.g, colour.b
                );

                let mut pre_attributes = SyntectPreAttributes::new(attributes, &style);
                html::write_opening_tag(output, "pre", pre_attributes.iter_mut())
            }
            None => html::write_opening_tag(output, "pre", vec![("class", "syntax-highlighting")]),
        }
    }

    fn write_code_tag(
        &self,
        output: &mut dyn Write,
        attributes: HashMap<&'static str, Cow<'_, str>>,
    ) -> fmt::Result {
        html::write_opening_tag(output, "code", attributes)
    }
}

struct SyntectPreAttributes<'s> {
    syntect_style: String,
    attributes: HashMap<&'static str, Cow<'s, str>>,
}

impl<'s> SyntectPreAttributes<'s> {
    fn new(attributes: HashMap<&'static str, Cow<'s, str>>, syntect_style: &str) -> Self {
        Self {
            syntect_style: syntect_style.into(),
            attributes,
        }
    }

    fn iter_mut(&mut self) -> SyntectPreAttributesIter<'_, 's> {
        SyntectPreAttributesIter {
            iter_mut: self.attributes.iter_mut(),
            syntect_style: &self.syntect_style,
            style_written: false,
        }
    }
}

struct SyntectPreAttributesIter<'a, 's> {
    iter_mut: hash_map::IterMut<'a, &'static str, Cow<'s, str>>,
    syntect_style: &'a str,
    style_written: bool,
}

impl<'a> Iterator for SyntectPreAttributesIter<'a, '_> {
    type Item = (&'a str, &'a str);

    fn next(&mut self) -> Option<Self::Item> {
        match self.iter_mut.next() {
            Some((k, v)) if *k == "style" && !self.style_written => {
                self.style_written = true;
                v.to_mut().insert_str(0, self.syntect_style);
                Some((k, v))
            }
            Some((k, v)) => Some((k, v)),
            None if !self.style_written => {
                self.style_written = true;
                Some(("style", self.syntect_style))
            }
            None => None,
        }
    }
}

#[derive(Debug, Default)]
/// A builder for [`SyntectAdapter`].
///
/// Allows customization of `Theme`, [`ThemeSet`], and [`SyntaxSet`].
pub struct SyntectAdapterBuilder {
    theme: Option<String>,
    syntax_set: Option<SyntaxSet>,
    theme_set: Option<ThemeSet>,
    css_class_prefix: Option<&'static str>,
}

impl SyntectAdapterBuilder {
    /// Create a new empty [`SyntectAdapterBuilder`].
    pub fn new() -> Self {
        Default::default()
    }

    /// Set the theme.
    pub fn theme(mut self, s: &str) -> Self {
        self.theme.replace(s.into());
        self
    }

    /// Uses CSS classes instead of a Syntect theme.
    pub fn css(mut self) -> Self {
        self.theme = None;
        self
    }

    /// Uses CSS classes with the specified prefix instead of a Syntect theme.
    pub fn css_with_class_prefix(self, prefix: &'static str) -> Self {
        let mut builder = self.css();
        builder.css_class_prefix = Some(prefix);
        builder
    }

    /// Set the syntax set.
    pub fn syntax_set(mut self, s: SyntaxSet) -> Self {
        self.syntax_set.replace(s);
        self
    }

    /// Set the theme set.
    pub fn theme_set(mut self, s: ThemeSet) -> Self {
        self.theme_set.replace(s);
        self
    }

    /// Builds the [`SyntectAdapter`]. Default values:
    /// - `theme`: `None` (uses CSS classes)
    /// - `syntax_set`: [`SyntaxSet::load_defaults_newlines()`]
    /// - `theme_set`: [`ThemeSet::load_defaults()`]
    pub fn build(self) -> SyntectAdapter {
        SyntectAdapter {
            theme: self.theme,
            syntax_set: self
                .syntax_set
                .unwrap_or_else(SyntaxSet::load_defaults_newlines),
            theme_set: self.theme_set.unwrap_or_else(ThemeSet::load_defaults),
            css_class_prefix: self.css_class_prefix,
        }
    }
}
