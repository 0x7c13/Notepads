//! Iterators and data structures for transforming parsing information into styled text.

// Code based on https://github.com/defuz/sublimate/blob/master/src/core/syntax/highlighter.rs
// released under the MIT license by @defuz

use std::iter::Iterator;
use std::ops::Range;

use super::selector::ScopeSelector;
use super::style::{Color, FontStyle, Style, StyleModifier};
use super::theme::{Theme, ThemeItem};
use crate::parsing::{
    BasicScopeStackOp, MatchPower, Scope, ScopeStack, ScopeStackOp, ATOM_LEN_BITS,
};

/// Basically a wrapper around a [`Theme`] preparing it to be used for highlighting.
///
/// This is part of the API to preserve the possibility of caching matches of the
/// selectors of the theme on various scope paths or setting up some kind of
/// accelerator structure.
///
/// So for now this does very little but eventually if you keep it around between
/// highlighting runs it will preserve its cache.
///
/// [`Theme`]: struct.Theme.html
#[derive(Debug)]
pub struct Highlighter<'a> {
    theme: &'a Theme,
    /// Cache of the selectors in the theme that are only one scope
    /// In most themes this is the majority, hence the usefullness
    single_selectors: Vec<(Scope, StyleModifier)>,
    multi_selectors: Vec<(ScopeSelector, StyleModifier)>,
    // TODO single_cache: HashMap<Scope, StyleModifier, BuildHasherDefault<FnvHasher>>,
}

/// Keeps a stack of scopes and styles as state between highlighting different lines.
///
/// If you are highlighting an entire file you create one of these at the start and use it
/// all the way to the end.
///
/// # Caching
///
/// One reason this is exposed is that since it implements `Clone` you can actually cache these
/// (probably along with a [`ParseState`]) and only re-start highlighting from the point of a
/// change. You could also do something fancy like only highlight a bit past the end of a user's
/// screen and resume highlighting when they scroll down on large files.
///
/// Alternatively you can save space by caching only the `path` field of this struct then re-create
/// the `HighlightState` when needed by passing that stack as the `initial_stack` parameter to the
/// [`new`] method. This takes less space but a small amount of time to re-create the style stack.
///
/// **Note:** Caching is for advanced users who have tons of time to maximize performance or want to
/// do so eventually. It is not recommended that you try caching the first time you implement
/// highlighting.
///
/// [`ParseState`]: ../parsing/struct.ParseState.html
/// [`new`]: #method.new
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HighlightState {
    styles: Vec<Style>,
    single_caches: Vec<ScoredStyle>,
    pub path: ScopeStack,
}

/// Highlights a line of parsed code given a [`HighlightState`] and line of changes from the parser.
///
/// Yields the [`Style`], the text and well as the `Range` of the text in the source string.
///
/// It splits a line of text into different pieces each with a [`Style`]
///
/// [`HighlightState`]: struct.HighlightState.html
/// [`Style`]: struct.Style.html
#[derive(Debug)]
pub struct RangedHighlightIterator<'a, 'b> {
    index: usize,
    pos: usize,
    changes: &'a [(usize, ScopeStackOp)],
    text: &'b str,
    highlighter: &'a Highlighter<'a>,
    state: &'a mut HighlightState,
}

/// Highlights a line of parsed code given a [`HighlightState`] and line of changes from the parser.
///
/// This is a backwards compatible shim on top of the [`RangedHighlightIterator`] which only
/// yields the [`Style`] and the text of the token, not the range.
///
/// It splits a line of text into different pieces each with a [`Style`].
///
/// [`HighlightState`]: struct.HighlightState.html
/// [`RangedHighlightIterator`]: struct.RangedHighlightIterator.html
/// [`Style`]: struct.Style.html
#[derive(Debug)]
pub struct HighlightIterator<'a, 'b> {
    ranged_iterator: RangedHighlightIterator<'a, 'b>,
}

impl HighlightState {
    /// Note that the [`Highlighter`] is not stored; it is used to construct the initial stack
    /// of styles.
    ///
    /// Most of the time you'll want to pass an empty stack as `initial_stack`, but see the docs for
    /// [`HighlightState`] for a discussion of advanced caching use cases.
    ///
    /// [`Highlighter`]: struct.Highlighter.html
    /// [`HighlightState`]: struct.HighlightState.html
    pub fn new(highlighter: &Highlighter<'_>, initial_stack: ScopeStack) -> HighlightState {
        let mut styles = vec![highlighter.get_default()];
        let mut single_caches = vec![ScoredStyle::from_style(styles[0])];
        for i in 0..initial_stack.len() {
            let prefix = initial_stack.bottom_n(i + 1);
            let new_cache = highlighter.update_single_cache_for_push(&single_caches[i], prefix);
            styles.push(highlighter.finalize_style_with_multis(&new_cache, prefix));
            single_caches.push(new_cache);
        }

        HighlightState {
            styles,
            single_caches,
            path: initial_stack,
        }
    }
}

impl<'a, 'b> RangedHighlightIterator<'a, 'b> {
    pub fn new(
        state: &'a mut HighlightState,
        changes: &'a [(usize, ScopeStackOp)],
        text: &'b str,
        highlighter: &'a Highlighter<'_>,
    ) -> RangedHighlightIterator<'a, 'b> {
        RangedHighlightIterator {
            index: 0,
            pos: 0,
            changes,
            text,
            highlighter,
            state,
        }
    }
}

impl<'b> Iterator for RangedHighlightIterator<'_, 'b> {
    type Item = (Style, &'b str, Range<usize>);

    /// Yields the next token of text and the associated `Style` to render that text with.
    /// the concatenation of the strings in each token will make the original string.
    fn next(&mut self) -> Option<(Style, &'b str, Range<usize>)> {
        if self.pos == self.text.len() && self.index >= self.changes.len() {
            return None;
        }
        let (end, command) = if self.index < self.changes.len() {
            self.changes[self.index].clone()
        } else {
            (self.text.len(), ScopeStackOp::Noop)
        };
        // println!("{} - {:?}   {}:{}", self.index, self.pos, self.state.path.len(), self.state.styles.len());
        let style = *self.state.styles.last().unwrap_or(&Style::default());
        let text = &self.text[self.pos..end];
        let range = Range {
            start: self.pos,
            end,
        };
        {
            // closures mess with the borrow checker's ability to see different struct fields
            let m_path = &mut self.state.path;
            let m_styles = &mut self.state.styles;
            let m_caches = &mut self.state.single_caches;
            let highlighter = &self.highlighter;
            m_path
                .apply_with_hook(&command, |op, cur_stack| {
                    // println!("{:?} - {:?}", op, cur_stack);
                    match op {
                        BasicScopeStackOp::Push(_) => {
                            // we can push multiple times so this might have changed
                            let new_cache = {
                                if let Some(prev_cache) = m_caches.last() {
                                    highlighter.update_single_cache_for_push(prev_cache, cur_stack)
                                } else {
                                    highlighter.update_single_cache_for_push(
                                        &ScoredStyle::from_style(highlighter.get_default()),
                                        cur_stack,
                                    )
                                }
                            };
                            m_styles.push(
                                highlighter.finalize_style_with_multis(&new_cache, cur_stack),
                            );
                            m_caches.push(new_cache);
                        }
                        BasicScopeStackOp::Pop => {
                            m_styles.pop();
                            m_caches.pop();
                        }
                    }
                })
                .ok()?;
        }
        self.pos = end;
        self.index += 1;
        if text.is_empty() {
            self.next()
        } else {
            Some((style, text, range))
        }
    }
}
impl<'a, 'b> HighlightIterator<'a, 'b> {
    pub fn new(
        state: &'a mut HighlightState,
        changes: &'a [(usize, ScopeStackOp)],
        text: &'b str,
        highlighter: &'a Highlighter<'_>,
    ) -> HighlightIterator<'a, 'b> {
        HighlightIterator {
            ranged_iterator: RangedHighlightIterator {
                index: 0,
                pos: 0,
                changes,
                text,
                highlighter,
                state,
            },
        }
    }
}

impl<'b> Iterator for HighlightIterator<'_, 'b> {
    type Item = (Style, &'b str);

    /// Yields the next token of text and the associated `Style` to render that text with.
    /// the concatenation of the strings in each token will make the original string.
    fn next(&mut self) -> Option<(Style, &'b str)> {
        self.ranged_iterator.next().map(|e| (e.0, e.1))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScoredStyle {
    pub foreground: (MatchPower, Color),
    pub background: (MatchPower, Color),
    pub font_style: (MatchPower, FontStyle),
}

#[inline]
fn update_scored<T: Clone>(scored: &mut (MatchPower, T), update: &Option<T>, score: MatchPower) {
    if score > scored.0 {
        if let Some(u) = update {
            scored.0 = score;
            scored.1 = u.clone();
        }
    }
}

impl ScoredStyle {
    fn apply(&mut self, other: &StyleModifier, score: MatchPower) {
        update_scored(&mut self.foreground, &other.foreground, score);
        update_scored(&mut self.background, &other.background, score);
        update_scored(&mut self.font_style, &other.font_style, score);
    }

    fn to_style(&self) -> Style {
        Style {
            foreground: self.foreground.1,
            background: self.background.1,
            font_style: self.font_style.1,
        }
    }

    fn from_style(style: Style) -> ScoredStyle {
        ScoredStyle {
            foreground: (MatchPower(-1.0), style.foreground),
            background: (MatchPower(-1.0), style.background),
            font_style: (MatchPower(-1.0), style.font_style),
        }
    }
}

impl<'a> Highlighter<'a> {
    pub fn new(theme: &'a Theme) -> Highlighter<'a> {
        let mut single_selectors = Vec::new();
        let mut multi_selectors = Vec::new();
        for item in &theme.scopes {
            for sel in &item.scope.selectors {
                if let Some(scope) = sel.extract_single_scope() {
                    single_selectors.push((scope, item.style));
                } else {
                    multi_selectors.push((sel.clone(), item.style));
                }
            }
        }
        // So that deeper matching selectors get checked first
        single_selectors.sort_by(|a, b| b.0.len().cmp(&a.0.len()));

        Highlighter {
            theme,
            single_selectors,
            multi_selectors,
        }
    }

    /// The default style in the absence of any matched rules.
    /// Basically what plain text gets highlighted as.
    pub fn get_default(&self) -> Style {
        Style {
            foreground: self.theme.settings.foreground.unwrap_or(Color::BLACK),
            background: self.theme.settings.background.unwrap_or(Color::WHITE),
            font_style: FontStyle::empty(),
        }
    }

    fn update_single_cache_for_push(&self, cur: &ScoredStyle, path: &[Scope]) -> ScoredStyle {
        let mut new_style = cur.clone();

        let last_scope = path[path.len() - 1];
        for &(scope, ref modif) in self
            .single_selectors
            .iter()
            .filter(|a| a.0.is_prefix_of(last_scope))
        {
            let single_score = f64::from(scope.len())
                * f64::from(ATOM_LEN_BITS * ((path.len() - 1) as u16)).exp2();
            new_style.apply(modif, MatchPower(single_score));
        }

        new_style
    }

    fn finalize_style_with_multis(&self, cur: &ScoredStyle, path: &[Scope]) -> Style {
        let mut new_style = cur.clone();

        let mult_iter = self
            .multi_selectors
            .iter()
            .filter_map(|(sel, style)| sel.does_match(path).map(|score| (score, style)));
        for (score, modif) in mult_iter {
            new_style.apply(modif, score);
        }

        new_style.to_style()
    }

    /// Returns the fully resolved style for the given stack.
    ///
    /// This operation is convenient but expensive. For reasonable performance,
    /// the caller should be caching results.
    pub fn style_for_stack(&self, stack: &[Scope]) -> Style {
        let mut single_cache = ScoredStyle::from_style(self.get_default());
        for i in 0..stack.len() {
            single_cache = self.update_single_cache_for_push(&single_cache, &stack[0..i + 1]);
        }
        self.finalize_style_with_multis(&single_cache, stack)
    }

    /// Returns a [`StyleModifier`] which, if applied to the default style,
    /// would generate the fully resolved style for this stack.
    ///
    /// This is made available to applications that are using syntect styles
    /// in combination with style information from other sources.
    ///
    /// This operation is convenient but expensive. For reasonable performance,
    /// the caller should be caching results. It's likely slower than [`style_for_stack`].
    ///
    /// [`StyleModifier`]: struct.StyleModifier.html
    /// [`style_for_stack`]: #method.style_for_stack
    pub fn style_mod_for_stack(&self, path: &[Scope]) -> StyleModifier {
        let mut matching_items: Vec<(MatchPower, &ThemeItem)> = self
            .theme
            .scopes
            .iter()
            .filter_map(|item| item.scope.does_match(path).map(|score| (score, item)))
            .collect();
        matching_items.sort_by_key(|&(score, _)| score);
        let sorted = matching_items.iter().map(|(_, item)| item);

        let mut modifier = StyleModifier {
            background: None,
            foreground: None,
            font_style: None,
        };
        for item in sorted {
            modifier = modifier.apply(item.style);
        }
        modifier
    }
}

#[cfg(all(feature = "default-syntaxes", feature = "default-themes"))]
#[cfg(test)]
mod tests {
    use super::*;
    use crate::highlighting::{Color, FontStyle, Style, ThemeSet};
    use crate::parsing::{ParseState, ScopeStack};
    use crate::utils::testdata;

    #[test]
    fn can_parse() {
        let ps = &*testdata::PACKAGES_SYN_SET;
        let mut state = {
            let syntax = ps.find_syntax_by_name("Ruby on Rails").unwrap();
            ParseState::new(syntax)
        };
        let ts = ThemeSet::load_defaults();
        let highlighter = Highlighter::new(&ts.themes["base16-ocean.dark"]);

        let mut highlight_state = HighlightState::new(&highlighter, ScopeStack::new());
        let line = "module Bob::Wow::Troll::Five; 5; end";
        let ops = state.parse_line(line, &ps).expect("#[cfg(test)]");
        let iter = HighlightIterator::new(&mut highlight_state, &ops[..], line, &highlighter);
        let regions: Vec<(Style, &str)> = iter.collect();
        // println!("{:#?}", regions);
        assert_eq!(
            regions[11],
            (
                Style {
                    foreground: Color {
                        r: 208,
                        g: 135,
                        b: 112,
                        a: 0xFF,
                    },
                    background: Color {
                        r: 43,
                        g: 48,
                        b: 59,
                        a: 0xFF,
                    },
                    font_style: FontStyle::empty(),
                },
                "5"
            )
        );
    }

    #[test]
    fn can_parse_with_highlight_state_from_cache() {
        let ps = &*testdata::PACKAGES_SYN_SET;
        let mut state = {
            let syntax = ps
                .find_syntax_by_scope(Scope::new("source.python").unwrap())
                .unwrap();
            ParseState::new(syntax)
        };
        let ts = ThemeSet::load_defaults();
        let highlighter = Highlighter::new(&ts.themes["base16-ocean.dark"]);

        // We start by parsing a python multiline-comment: """
        let mut highlight_state = HighlightState::new(&highlighter, ScopeStack::new());
        let line = r#"""""#;
        let ops = state.parse_line(line, &ps).expect("#[cfg(test)]");
        let iter = HighlightIterator::new(&mut highlight_state, &ops[..], line, &highlighter);
        assert_eq!(1, iter.count());
        let path = highlight_state.path;

        // We then parse the next line with a highlight state built from the previous state
        let mut highlight_state = HighlightState::new(&highlighter, path);
        let line = "multiline comment";
        let ops = state.parse_line(line, &ps).expect("#[cfg(test)]");
        let iter = HighlightIterator::new(&mut highlight_state, &ops[..], line, &highlighter);
        let regions: Vec<(Style, &str)> = iter.collect();

        // We expect the line to be styled as a comment.
        assert_eq!(
            regions[0],
            (
                Style {
                    foreground: Color {
                        // (Comment: #65737E)
                        r: 101,
                        g: 115,
                        b: 126,
                        a: 0xFF,
                    },
                    background: Color {
                        r: 43,
                        g: 48,
                        b: 59,
                        a: 0xFF,
                    },
                    font_style: FontStyle::empty(),
                },
                "multiline comment"
            )
        );
    }

    // see issues #133 and #203, this test tests the fixes for those issues
    #[test]
    fn tricky_cases() {
        use crate::highlighting::{ScopeSelectors, ThemeSettings};
        use crate::parsing::ScopeStack;
        use std::str::FromStr;
        let c1 = Color {
            r: 1,
            g: 1,
            b: 1,
            a: 255,
        };
        let c2 = Color {
            r: 2,
            g: 2,
            b: 2,
            a: 255,
        };
        let def_bg = Color {
            r: 255,
            g: 255,
            b: 255,
            a: 255,
        };
        let test_color_scheme = Theme {
            name: None,
            author: None,
            settings: ThemeSettings::default(),
            scopes: vec![
                ThemeItem {
                    scope: ScopeSelectors::from_str("comment.line").unwrap(),
                    style: StyleModifier {
                        foreground: Some(c1),
                        background: None,
                        font_style: None,
                    },
                },
                ThemeItem {
                    scope: ScopeSelectors::from_str("comment").unwrap(),
                    style: StyleModifier {
                        foreground: Some(c2),
                        background: None,
                        font_style: Some(FontStyle::ITALIC),
                    },
                },
                ThemeItem {
                    scope: ScopeSelectors::from_str("comment.line.rs - keyword").unwrap(),
                    style: StyleModifier {
                        foreground: None,
                        background: Some(c1),
                        font_style: None,
                    },
                },
                ThemeItem {
                    scope: ScopeSelectors::from_str("no.match").unwrap(),
                    style: StyleModifier {
                        foreground: None,
                        background: Some(c2),
                        font_style: Some(FontStyle::UNDERLINE),
                    },
                },
            ],
        };
        let highlighter = Highlighter::new(&test_color_scheme);

        use crate::parsing::ScopeStackOp::*;
        let ops = [
            // three rules apply at once here, two singles and one multi
            (0, Push(Scope::new("comment.line.rs").unwrap())),
            // multi un-applies
            (1, Push(Scope::new("keyword.control.rs").unwrap())),
            (2, Pop(1)),
        ];

        let mut highlight_state = HighlightState::new(&highlighter, ScopeStack::new());
        let iter = HighlightIterator::new(&mut highlight_state, &ops[..], "abcdef", &highlighter);
        let regions: Vec<Style> = iter.map(|(s, _)| s).collect();

        // println!("{:#?}", regions);
        assert_eq!(
            regions,
            vec![
                Style {
                    foreground: c1,
                    background: c1,
                    font_style: FontStyle::ITALIC
                },
                Style {
                    foreground: c1,
                    background: def_bg,
                    font_style: FontStyle::ITALIC
                },
                Style {
                    foreground: c1,
                    background: c1,
                    font_style: FontStyle::ITALIC
                },
            ]
        );

        let full_stack = ScopeStack::from_str("comment.line.rs keyword.control.rs").unwrap();
        let full_style = highlighter.style_for_stack(full_stack.as_slice());
        assert_eq!(
            full_style,
            Style {
                foreground: c1,
                background: def_bg,
                font_style: FontStyle::ITALIC
            }
        );
        let full_mod = highlighter.style_mod_for_stack(full_stack.as_slice());
        assert_eq!(
            full_mod,
            StyleModifier {
                foreground: Some(c1),
                background: None,
                font_style: Some(FontStyle::ITALIC)
            }
        );
    }

    #[test]
    fn test_ranges() {
        let ps = &*testdata::PACKAGES_SYN_SET;
        let mut state = {
            let syntax = ps.find_syntax_by_name("Ruby on Rails").unwrap();
            ParseState::new(syntax)
        };
        let ts = ThemeSet::load_defaults();
        let highlighter = Highlighter::new(&ts.themes["base16-ocean.dark"]);

        let mut highlight_state = HighlightState::new(&highlighter, ScopeStack::new());
        let line = "module Bob::Wow::Troll::Five; 5; end";
        let ops = state.parse_line(line, &ps).expect("#[cfg(test)]");
        let iter = RangedHighlightIterator::new(&mut highlight_state, &ops[..], line, &highlighter);
        let regions: Vec<(Style, &str, Range<usize>)> = iter.collect();
        // println!("{:#?}", regions);
        assert_eq!(
            regions[11],
            (
                Style {
                    foreground: Color {
                        r: 208,
                        g: 135,
                        b: 112,
                        a: 0xFF,
                    },
                    background: Color {
                        r: 43,
                        g: 48,
                        b: 59,
                        a: 0xFF,
                    },
                    font_style: FontStyle::empty(),
                },
                "5",
                Range { start: 30, end: 31 }
            )
        );
    }
}
