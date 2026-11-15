/// Code based on <https://github.com/defuz/sublimate/blob/master/src/core/syntax/scope.rs>
/// released under the MIT license by @defuz
use crate::parsing::{MatchPower, ParseScopeError, Scope, ScopeStack};
use serde_derive::{Deserialize, Serialize};
use std::str::FromStr;

/// A single selector consisting of a stack to match and a possible stack to
/// exclude from being matched.
///
/// You probably want [`ScopeSelectors`] which is this but with union support.
///
/// [`ScopeSelectors`]: struct.ScopeSelectors.html
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct ScopeSelector {
    pub path: ScopeStack,
    pub excludes: Vec<ScopeStack>,
}

/// A selector set that matches anything matched by any of its component selectors.
///
/// See [The TextMate Docs](https://manual.macromates.com/en/scope_selectors) for how these work.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct ScopeSelectors {
    /// The selectors, if any of them match, that this matches
    pub selectors: Vec<ScopeSelector>,
}

impl ScopeSelector {
    /// Checks if this selector matches a given scope stack.
    ///
    /// See [`ScopeSelectors::does_match`] for more info.
    ///
    /// [`ScopeSelectors::does_match`]: struct.ScopeSelectors.html#method.does_match
    pub fn does_match(&self, stack: &[Scope]) -> Option<MatchPower> {
        // if there are any exclusions, and any one of them matches, then this selector doesn't match
        if self
            .excludes
            .iter()
            .any(|sel| sel.is_empty() || sel.does_match(stack).is_some())
        {
            return None;
        }
        if self.path.is_empty() {
            // an empty scope selector always matches with a score of 1
            Some(MatchPower(0o1u64 as f64))
        } else {
            self.path.does_match(stack)
        }
    }

    /// If this selector is really just a single scope, return it
    pub fn extract_single_scope(&self) -> Option<Scope> {
        if self.path.len() > 1 || !self.excludes.is_empty() || self.path.is_empty() {
            return None;
        }
        Some(self.path.as_slice()[0])
    }

    /// Extract all selectors for generating CSS
    pub fn extract_scopes(&self) -> Vec<Scope> {
        self.path.scopes.clone()
    }
}

impl FromStr for ScopeSelector {
    type Err = ParseScopeError;

    /// Parses a scope stack followed optionally by (one or more) " -" and then a scope stack to exclude
    fn from_str(s: &str) -> Result<ScopeSelector, ParseScopeError> {
        let mut excludes = Vec::new();
        let mut path_str: &str = "";
        for (i, selector) in s.split(" -").enumerate() {
            if i == 0 {
                path_str = selector;
            } else {
                excludes.push(ScopeStack::from_str(selector)?);
            }
        }
        Ok(ScopeSelector {
            path: ScopeStack::from_str(path_str)?,
            excludes,
        })
    }
}

impl ScopeSelectors {
    /// Checks if any of the given selectors match the given scope stack
    ///
    /// If so, it returns a match score. Higher match scores indicate stronger matches. Scores are
    /// ordered according to the rules found at [https://manual.macromates.com/en/scope_selectors](https://manual.macromates.com/en/scope_selectors).
    ///
    /// # Examples
    ///
    /// ```
    /// use syntect::parsing::{ScopeStack, MatchPower};
    /// use syntect::highlighting::ScopeSelectors;
    /// use std::str::FromStr;
    /// assert_eq!(ScopeSelectors::from_str("a.b, a e.f - c k, e.f - a.b").unwrap()
    ///     .does_match(ScopeStack::from_str("a.b c.d j e.f").unwrap().as_slice()),
    ///     Some(MatchPower(0o2001u64 as f64)));
    /// ```
    pub fn does_match(&self, stack: &[Scope]) -> Option<MatchPower> {
        self.selectors
            .iter()
            .filter_map(|sel| sel.does_match(stack))
            .max()
    }
}

impl FromStr for ScopeSelectors {
    type Err = ParseScopeError;

    /// Parses a series of selectors separated by commas or pipes
    fn from_str(s: &str) -> Result<ScopeSelectors, ParseScopeError> {
        let mut selectors = Vec::new();
        for selector in s.split(&[',', '|'][..]) {
            selectors.push(ScopeSelector::from_str(selector)?)
        }
        Ok(ScopeSelectors { selectors })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn selectors_work() {
        use std::str::FromStr;
        let sels = ScopeSelectors::from_str(
            "source.php meta.preprocessor - string.quoted, \
                                             source string",
        )
        .unwrap();
        assert_eq!(sels.selectors.len(), 2);
        let first_sel = &sels.selectors[0];
        assert_eq!(format!("{:?}", first_sel),
                   "ScopeSelector { path: ScopeStack { clear_stack: [], scopes: [<source.php>, <meta.preprocessor>] }, excludes: [ScopeStack { clear_stack: [], scopes: [<string.quoted>] }] }");

        let sels = ScopeSelectors::from_str(
            "source.php meta.preprocessor -string.quoted|\
                                             source string",
        )
        .unwrap();
        assert_eq!(sels.selectors.len(), 2);
        let first_sel = &sels.selectors[0];
        assert_eq!(format!("{:?}", first_sel),
                   "ScopeSelector { path: ScopeStack { clear_stack: [], scopes: [<source.php>, <meta.preprocessor>] }, excludes: [ScopeStack { clear_stack: [], scopes: [<string.quoted>] }] }");

        let sels = ScopeSelectors::from_str(
            "text.xml meta.tag.preprocessor.xml punctuation.separator.key-value.xml",
        )
        .unwrap();
        assert_eq!(sels.selectors.len(), 1);
        let first_sel = &sels.selectors[0];
        assert_eq!(format!("{:?}", first_sel),
                   "ScopeSelector { path: ScopeStack { clear_stack: [], scopes: [<text.xml>, <meta.tag.preprocessor.xml>, <punctuation.separator.key-value.xml>] }, excludes: [] }");

        let sels = ScopeSelectors::from_str("text.xml meta.tag.preprocessor.xml punctuation.separator.key-value.xml - text.html - string")
            .unwrap();
        assert_eq!(sels.selectors.len(), 1);
        let first_sel = &sels.selectors[0];
        assert_eq!(format!("{:?}", first_sel),
                   "ScopeSelector { path: ScopeStack { clear_stack: [], scopes: [<text.xml>, <meta.tag.preprocessor.xml>, <punctuation.separator.key-value.xml>] }, excludes: [ScopeStack { clear_stack: [], scopes: [<text.html>] }, ScopeStack { clear_stack: [], scopes: [<string>] }] }");

        let sels = ScopeSelectors::from_str("text.xml meta.tag.preprocessor.xml punctuation.separator.key-value.xml - text.html - string, source - comment")
            .unwrap();
        assert_eq!(sels.selectors.len(), 2);
        let first_sel = &sels.selectors[0];
        assert_eq!(format!("{:?}", first_sel),
                   "ScopeSelector { path: ScopeStack { clear_stack: [], scopes: [<text.xml>, <meta.tag.preprocessor.xml>, <punctuation.separator.key-value.xml>] }, excludes: [ScopeStack { clear_stack: [], scopes: [<text.html>] }, ScopeStack { clear_stack: [], scopes: [<string>] }] }");
        let second_sel = &sels.selectors[1];
        assert_eq!(format!("{:?}", second_sel),
                   "ScopeSelector { path: ScopeStack { clear_stack: [], scopes: [<source>] }, excludes: [ScopeStack { clear_stack: [], scopes: [<comment>] }] }");

        let sels = ScopeSelectors::from_str(" -a.b|j.g").unwrap();
        assert_eq!(sels.selectors.len(), 2);
        let first_sel = &sels.selectors[0];
        assert_eq!(format!("{:?}", first_sel),
                   "ScopeSelector { path: ScopeStack { clear_stack: [], scopes: [] }, excludes: [ScopeStack { clear_stack: [], scopes: [<a.b>] }] }");
        let second_sel = &sels.selectors[1];
        assert_eq!(
            format!("{:?}", second_sel),
            "ScopeSelector { path: ScopeStack { clear_stack: [], scopes: [<j.g>] }, excludes: [] }"
        );
    }
    #[test]
    fn matching_works() {
        use crate::parsing::{MatchPower, ScopeStack};
        use std::str::FromStr;
        assert_eq!(
            ScopeSelectors::from_str("a.b, a e, e.f")
                .unwrap()
                .does_match(ScopeStack::from_str("a.b e.f").unwrap().as_slice()),
            Some(MatchPower(0o20u64 as f64))
        );
        assert_eq!(
            ScopeSelectors::from_str("a.b, a e.f, e.f")
                .unwrap()
                .does_match(ScopeStack::from_str("a.b e.f").unwrap().as_slice()),
            Some(MatchPower(0o21u64 as f64))
        );
        assert_eq!(
            ScopeSelectors::from_str("a.b, a e.f - c j, e.f")
                .unwrap()
                .does_match(ScopeStack::from_str("a.b c.d j e.f").unwrap().as_slice()),
            Some(MatchPower(0o2000u64 as f64))
        );
        assert_eq!(
            ScopeSelectors::from_str("a.b, a e.f - c j, e.f - a.b")
                .unwrap()
                .does_match(ScopeStack::from_str("a.b c.d j e.f").unwrap().as_slice()),
            Some(MatchPower(0o2u64 as f64))
        );
        assert_eq!(
            ScopeSelectors::from_str("a.b, a e.f - c k, e.f - a.b")
                .unwrap()
                .does_match(ScopeStack::from_str("a.b c.d j e.f").unwrap().as_slice()),
            Some(MatchPower(0o2001u64 as f64))
        );
        assert_eq!(
            ScopeSelectors::from_str("a.b|a e.f -d, e.f -a.b")
                .unwrap()
                .does_match(ScopeStack::from_str("a.b c.d e.f").unwrap().as_slice()),
            Some(MatchPower(0o201u64 as f64))
        );
    }

    #[test]
    fn empty_stack_matching_works() {
        use crate::parsing::{MatchPower, ScopeStack};
        use std::str::FromStr;
        assert_eq!(
            ScopeSelector::from_str(" - a.b")
                .unwrap()
                .does_match(ScopeStack::from_str("a.b c.d j e.f").unwrap().as_slice()),
            None
        );
        assert_eq!(
            ScopeSelector::from_str("")
                .unwrap()
                .does_match(ScopeStack::from_str("a.b c.d j e.f").unwrap().as_slice()),
            Some(MatchPower(0o1u64 as f64))
        );
        assert_eq!(
            ScopeSelector::from_str("")
                .unwrap()
                .does_match(ScopeStack::from_str("").unwrap().as_slice()),
            Some(MatchPower(0o1u64 as f64))
        );
        assert_eq!(
            ScopeSelector::from_str(" - a.b")
                .unwrap()
                .does_match(ScopeStack::from_str("").unwrap().as_slice()),
            Some(MatchPower(0o1u64 as f64))
        );
        assert_eq!(
            ScopeSelector::from_str("a.b - ")
                .unwrap()
                .does_match(ScopeStack::from_str("").unwrap().as_slice()),
            None
        );
        assert_eq!(
            ScopeSelector::from_str("a.b - ")
                .unwrap()
                .does_match(ScopeStack::from_str("a.b c.d j e.f").unwrap().as_slice()),
            None
        );
        assert_eq!(
            ScopeSelector::from_str(" - ")
                .unwrap()
                .does_match(ScopeStack::from_str("a.b c.d j e.f").unwrap().as_slice()),
            None
        );
        assert_eq!(
            ScopeSelector::from_str(" - a.b")
                .unwrap()
                .does_match(ScopeStack::from_str("a.b c.d j e.f").unwrap().as_slice()),
            None
        );
        assert_eq!(
            ScopeSelector::from_str(" - g.h")
                .unwrap()
                .does_match(ScopeStack::from_str("a.b c.d j e.f").unwrap().as_slice()),
            Some(MatchPower(0o1u64 as f64))
        );

        assert_eq!(
            ScopeSelector::from_str(" -a.b")
                .unwrap()
                .does_match(ScopeStack::from_str("a.b c.d j e.f").unwrap().as_slice()),
            None
        );
        assert_eq!(
            ScopeSelector::from_str("")
                .unwrap()
                .does_match(ScopeStack::from_str("a.b c.d j e.f").unwrap().as_slice()),
            Some(MatchPower(0o1u64 as f64))
        );
        assert_eq!(
            ScopeSelector::from_str(" -a.b")
                .unwrap()
                .does_match(ScopeStack::from_str("").unwrap().as_slice()),
            Some(MatchPower(0o1u64 as f64))
        );
        assert_eq!(
            ScopeSelector::from_str("a.b -")
                .unwrap()
                .does_match(ScopeStack::from_str("").unwrap().as_slice()),
            None
        );
        assert_eq!(
            ScopeSelector::from_str("a.b -")
                .unwrap()
                .does_match(ScopeStack::from_str("a.b c.d j e.f").unwrap().as_slice()),
            None
        );
        assert_eq!(
            ScopeSelector::from_str(" -")
                .unwrap()
                .does_match(ScopeStack::from_str("a.b c.d j e.f").unwrap().as_slice()),
            None
        );
        assert_eq!(
            ScopeSelector::from_str(" -a.b")
                .unwrap()
                .does_match(ScopeStack::from_str("a.b c.d j e.f").unwrap().as_slice()),
            None
        );
        assert_eq!(
            ScopeSelector::from_str(" -g.h")
                .unwrap()
                .does_match(ScopeStack::from_str("a.b c.d j e.f").unwrap().as_slice()),
            Some(MatchPower(0o1u64 as f64))
        );
    }

    #[test]
    fn multiple_excludes_matching_works() {
        use crate::parsing::{MatchPower, ScopeStack};
        use std::str::FromStr;
        assert_eq!(
            ScopeSelector::from_str(" - a.b - c.d")
                .unwrap()
                .does_match(ScopeStack::from_str("a.b c.d j e.f").unwrap().as_slice()),
            None
        );
        assert_eq!(
            ScopeSelector::from_str(" - a.b - c.d")
                .unwrap()
                .does_match(ScopeStack::from_str("").unwrap().as_slice()),
            Some(MatchPower(0o1u64 as f64))
        );
        assert_eq!(
            ScopeSelector::from_str("a.b - c.d -e.f")
                .unwrap()
                .does_match(ScopeStack::from_str("").unwrap().as_slice()),
            None
        );
        assert_eq!(
            ScopeSelector::from_str("a.b - c.d -")
                .unwrap()
                .does_match(ScopeStack::from_str("a.b c.d j e.f").unwrap().as_slice()),
            None
        );
        assert_eq!(
            ScopeSelector::from_str(" -g.h - h.i")
                .unwrap()
                .does_match(ScopeStack::from_str("a.b c.d j e.f").unwrap().as_slice()),
            Some(MatchPower(0o1u64 as f64))
        );
        assert_eq!(
            ScopeSelector::from_str("a.b")
                .unwrap()
                .does_match(ScopeStack::from_str("a.b c.d j e.f").unwrap().as_slice()),
            Some(MatchPower(0o2u64 as f64))
        );
        assert_eq!(
            ScopeSelector::from_str("a.b -g.h - h.i")
                .unwrap()
                .does_match(ScopeStack::from_str("a.b c.d j e.f").unwrap().as_slice()),
            Some(MatchPower(0o2u64 as f64))
        );
        assert_eq!(
            ScopeSelector::from_str("c.d")
                .unwrap()
                .does_match(ScopeStack::from_str("a.b c.d j e.f").unwrap().as_slice()),
            Some(MatchPower(0o20u64 as f64))
        );
        assert_eq!(
            ScopeSelector::from_str("c.d - j.g - h.i")
                .unwrap()
                .does_match(ScopeStack::from_str("a.b c.d j e.f").unwrap().as_slice()),
            Some(MatchPower(0o20u64 as f64))
        );
        assert_eq!(
            ScopeSelectors::from_str("j.g| -a.b")
                .unwrap()
                .does_match(ScopeStack::from_str("a.b c.d j e.f").unwrap().as_slice()),
            None
        );
        assert_eq!(
            ScopeSelectors::from_str(" -a.b|j.g")
                .unwrap()
                .does_match(ScopeStack::from_str("a.b c.d j e.f").unwrap().as_slice()),
            None
        );
        assert_eq!(
            ScopeSelectors::from_str(" -a.b,c.d - j.g - h.i")
                .unwrap()
                .does_match(ScopeStack::from_str("a.b c.d j e.f").unwrap().as_slice()),
            Some(MatchPower(0o20u64 as f64))
        );
        assert_eq!(
            ScopeSelectors::from_str(" -a.b, -d.c -f.e")
                .unwrap()
                .does_match(ScopeStack::from_str("a.b c.d j e.f").unwrap().as_slice()),
            Some(MatchPower(0o01u64 as f64))
        );
    }
}
