use once_cell::sync::OnceCell;
use serde::de::{Deserialize, Deserializer};
use serde::ser::{Serialize, Serializer};
use std::error::Error;

// Modifications Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
#[cfg(feature = "parsing")]
use super::execution::{ExecutionControl, ExecutionError, RegexPhase, Stage};

/// An abstraction for regex patterns.
///
/// * Allows swapping out the regex implementation because it's only in this module.
/// * Makes regexes serializable and deserializable using just the pattern string.
/// * Lazily compiles regexes on first use to improve initialization time.
#[derive(Debug)]
pub struct Regex {
    regex_str: String,
    regex: OnceCell<regex_impl::Regex>,
}

/// A region contains text positions for capture groups in a match result.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Region {
    region: regex_impl::Region,
}

impl Regex {
    /// Create a new regex from the pattern string.
    ///
    /// Note that the regex compilation happens on first use, which is why this method does not
    /// return a result.
    pub fn new(regex_str: String) -> Self {
        Self {
            regex_str,
            regex: OnceCell::new(),
        }
    }

    /// Check whether the pattern compiles as a valid regex or not.
    pub fn try_compile(regex_str: &str) -> Option<Box<dyn Error + Send + Sync + 'static>> {
        regex_impl::Regex::new(regex_str).err()
    }

    /// Return the regex pattern.
    pub fn regex_str(&self) -> &str {
        &self.regex_str
    }

    /// Check if the regex matches the given text.
    pub fn is_match(&self, text: &str) -> bool {
        self.regex().is_match(text)
    }

    /// Search for the pattern in the given text from begin/end positions.
    ///
    /// If a region is passed, it is used for storing match group positions. The argument allows
    /// the [`Region`] to be reused between searches, which makes a significant performance
    /// difference.
    ///
    /// [`Region`]: struct.Region.html
    pub fn search(
        &self,
        text: &str,
        begin: usize,
        end: usize,
        region: Option<&mut Region>,
    ) -> bool {
        self.regex()
            .search(text, begin, end, region.map(|r| &mut r.region))
    }

    /// Controlled parser path: backend errors are errors, never negative matches.
    /// Successful shared compilation retains no control/monitor reference.
    #[cfg(feature = "parsing")]
    pub fn try_search_with_control(
        &self,
        text: &str,
        begin: usize,
        end: usize,
        region: Option<&mut Region>,
        control: &mut ExecutionControl<'_>,
    ) -> Result<bool, ExecutionError> {
        control.checkpoint(Stage::RegexCompile, 0)?;
        if !control.is_controlled() {
            return Ok(self.search(text, begin, end, region));
        }
        let compiled = self.regex.get_or_try_init(|| {
            control.checkpoint(Stage::RegexCompile, 1)?;
            regex_impl::Regex::new(&self.regex_str)
                .map_err(|source| control.regex_error(RegexPhase::Compile, source))
        })?;
        control.checkpoint(Stage::RegexCompile, 0)?;
        control.checkpoint(Stage::RegexSearch, 1)?;
        let matched =
            compiled.try_search(text, begin, end, region.map(|r| &mut r.region), control)?;
        control.checkpoint(Stage::RegexSearch, 0)?;
        Ok(matched)
    }

    fn regex(&self) -> &regex_impl::Regex {
        self.regex.get_or_init(|| {
            regex_impl::Regex::new(&self.regex_str).expect("regex string should be pre-tested")
        })
    }
}

impl Clone for Regex {
    fn clone(&self) -> Self {
        Regex {
            regex_str: self.regex_str.clone(),
            regex: OnceCell::new(),
        }
    }
}

impl PartialEq for Regex {
    fn eq(&self, other: &Regex) -> bool {
        self.regex_str == other.regex_str
    }
}

impl Eq for Regex {}

impl Serialize for Regex {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.regex_str)
    }
}

impl<'de> Deserialize<'de> for Regex {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let regex_str = String::deserialize(deserializer)?;
        Ok(Regex::new(regex_str))
    }
}

impl Region {
    pub fn new() -> Self {
        Self {
            region: regex_impl::new_region(),
        }
    }

    /// Get the start/end positions of the capture group with given index.
    ///
    /// If there is no match for that group or the index does not correspond to a group, `None` is
    /// returned. The index 0 returns the whole match.
    pub fn pos(&self, index: usize) -> Option<(usize, usize)> {
        self.region.pos(index)
    }
}

impl Default for Region {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(feature = "regex-onig")]
mod regex_impl {
    pub use onig::Region;
    use onig::{MatchParam, RegexOptions, SearchOptions, Syntax};
    use std::error::Error;

    #[derive(Debug)]
    pub struct Regex {
        regex: onig::Regex,
    }

    pub fn new_region() -> Region {
        Region::with_capacity(8)
    }

    impl Regex {
        pub fn new(regex_str: &str) -> Result<Regex, Box<dyn Error + Send + Sync + 'static>> {
            let result = onig::Regex::with_options(
                regex_str,
                RegexOptions::REGEX_OPTION_CAPTURE_GROUP,
                Syntax::default(),
            );
            match result {
                Ok(regex) => Ok(Regex { regex }),
                Err(error) => Err(Box::new(error)),
            }
        }

        pub fn is_match(&self, text: &str) -> bool {
            self.regex
                .match_with_options(text, 0, SearchOptions::SEARCH_OPTION_NONE, None)
                .is_some()
        }

        pub fn search(
            &self,
            text: &str,
            begin: usize,
            end: usize,
            region: Option<&mut Region>,
        ) -> bool {
            let matched = self.regex.search_with_param(
                text,
                begin,
                end,
                SearchOptions::SEARCH_OPTION_NONE,
                region,
                MatchParam::default(),
            );

            // If there's an error during search, treat it as non-matching.
            // For example, in case of catastrophic backtracking, onig should
            // fail with a "retry-limit-in-match over" error eventually.
            matches!(matched, Ok(Some(_)))
        }

        #[cfg(feature = "parsing")]
        pub fn try_search(
            &self,
            text: &str,
            begin: usize,
            end: usize,
            region: Option<&mut Region>,
            control: &mut super::ExecutionControl<'_>,
        ) -> Result<bool, super::ExecutionError> {
            self.regex
                .search_with_param(
                    text,
                    begin,
                    end,
                    SearchOptions::SEARCH_OPTION_NONE,
                    region,
                    MatchParam::default(),
                )
                .map(|matched| matched.is_some())
                .map_err(|error| control.regex_error(super::RegexPhase::Search, Box::new(error)))
        }
    }
}

// If both regex-fancy and regex-onig are requested, this condition makes regex-onig win.
#[cfg(all(feature = "regex-fancy", not(feature = "regex-onig")))]
mod regex_impl {
    use std::error::Error;

    #[derive(Debug)]
    pub struct Regex {
        regex: fancy_regex::Regex,
    }

    #[derive(Clone, Debug, Eq, PartialEq)]
    pub struct Region {
        positions: Vec<Option<(usize, usize)>>,
    }

    pub fn new_region() -> Region {
        Region {
            positions: Vec::with_capacity(8),
        }
    }

    impl Regex {
        pub fn new(regex_str: &str) -> Result<Regex, Box<dyn Error + Send + Sync + 'static>> {
            let result = fancy_regex::RegexBuilder::new(regex_str)
                .oniguruma_mode(true)
                .build();
            match result {
                Ok(regex) => Ok(Regex { regex }),
                Err(error) => Err(Box::new(error)),
            }
        }

        pub fn is_match(&self, text: &str) -> bool {
            // Errors are treated as non-matches
            self.regex.is_match(text).unwrap_or(false)
        }

        pub fn search(
            &self,
            text: &str,
            begin: usize,
            end: usize,
            region: Option<&mut Region>,
        ) -> bool {
            // If there's an error during search, treat it as non-matching.
            // For example, in case of catastrophic backtracking, fancy-regex should
            // fail with an error eventually.
            if let Ok(Some(captures)) = self.regex.captures_from_pos(&text[..end], begin) {
                if let Some(region) = region {
                    region.init_from_captures(&captures);
                }
                true
            } else {
                false
            }
        }

        #[cfg(feature = "parsing")]
        pub fn try_search(
            &self,
            text: &str,
            begin: usize,
            end: usize,
            region: Option<&mut Region>,
            control: &mut super::ExecutionControl<'_>,
        ) -> Result<bool, super::ExecutionError> {
            match self.regex.captures_from_pos(&text[..end], begin) {
                Ok(Some(captures)) => {
                    if let Some(region) = region {
                        region.positions.clear();
                        for i in 0..captures.len() {
                            control.checkpoint(super::Stage::Capture, 1)?;
                            region
                                .positions
                                .push(captures.get(i).map(|m| (m.start(), m.end())));
                        }
                    }
                    Ok(true)
                }
                Ok(None) => Ok(false),
                Err(error) => Err(control.regex_error(super::RegexPhase::Search, Box::new(error))),
            }
        }
    }

    impl Region {
        fn init_from_captures(&mut self, captures: &fancy_regex::Captures) {
            self.positions.clear();
            for i in 0..captures.len() {
                let pos = captures.get(i).map(|m| (m.start(), m.end()));
                self.positions.push(pos);
            }
        }

        pub fn pos(&self, i: usize) -> Option<(usize, usize)> {
            if i < self.positions.len() {
                self.positions[i]
            } else {
                None
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn caches_compiled_regex() {
        let regex = Regex::new(String::from(r"\w+"));

        assert!(regex.regex.get().is_none());
        assert!(regex.is_match("test"));
        assert!(regex.regex.get().is_some());
    }

    #[test]
    fn serde_as_string() {
        let pattern: Regex = serde_json::from_str("\"just a string\"").unwrap();
        assert_eq!(pattern.regex_str(), "just a string");
        let back_to_str = serde_json::to_string(&pattern).unwrap();
        assert_eq!(back_to_str, "\"just a string\"");
    }
}
