//! Data structures for representing syntax definitions
//!
//! Everything here is public becaues I want this library to be useful in super integrated cases
//! like text editors and I have no idea what kind of monkeying you might want to do with the data.
//! Perhaps parsing your own syntax format into this data structure?

use super::regex::{Regex, Region};
use super::{scope::*, ParsingError};
use crate::parsing::syntax_set::SyntaxSet;
use regex_syntax::escape;
use serde::ser::{Serialize, Serializer};
use serde_derive::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::hash::Hash;

// Modifications Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
use super::execution::{ExecutionControl, Stage};

pub type CaptureMapping = Vec<(usize, Vec<Scope>)>;

/// An opaque ID for a [`Context`].
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub struct ContextId {
    /// Index into [`SyntaxSet::syntaxes`]
    pub(crate) syntax_index: usize,

    /// Index into [`crate::parsing::LazyContexts::contexts`] for the [`Self::syntax_index`] syntax
    pub(crate) context_index: usize,
}

/// The main data structure representing a syntax definition loaded from a
/// `.sublime-syntax` file
///
/// You'll probably only need these as references to be passed around to parsing code.
///
/// Some useful public fields are the `name` field which is a human readable name to display in
/// syntax lists, and the `hidden` field which means hide this syntax from any lists because it is
/// for internal use.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SyntaxDefinition {
    pub name: String,
    pub file_extensions: Vec<String>,
    pub scope: Scope,
    pub first_line_match: Option<String>,
    pub hidden: bool,
    #[serde(serialize_with = "ordered_map")]
    pub variables: HashMap<String, String>,
    #[serde(serialize_with = "ordered_map")]
    pub contexts: HashMap<String, Context>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Context {
    pub meta_scope: Vec<Scope>,
    pub meta_content_scope: Vec<Scope>,
    /// This being set false in the syntax file implies this field being set false,
    /// but it can also be set falso for contexts that don't include the prototype for other reasons
    pub meta_include_prototype: bool,
    pub clear_scopes: Option<ClearAmount>,
    /// This is filled in by the linker at link time
    /// for contexts that have `meta_include_prototype==true`
    /// and are not included from the prototype.
    pub prototype: Option<ContextId>,
    pub uses_backrefs: bool,

    pub patterns: Vec<Pattern>,
}

impl Context {
    pub fn new(meta_include_prototype: bool) -> Context {
        Context {
            meta_scope: Vec::new(),
            meta_content_scope: Vec::new(),
            meta_include_prototype,
            clear_scopes: None,
            uses_backrefs: false,
            patterns: Vec::new(),
            prototype: None,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum Pattern {
    Match(MatchPattern),
    Include(ContextReference),
}

/// Used to iterate over all the match patterns in a context
///
/// Basically walks the tree of patterns and include directives in the correct order.
#[derive(Debug)]
pub struct MatchIter<'a> {
    syntax_set: &'a SyntaxSet,
    ctx_stack: Vec<&'a Context>,
    index_stack: Vec<usize>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct MatchPattern {
    pub has_captures: bool,
    pub regex: Regex,
    pub scope: Vec<Scope>,
    pub captures: Option<CaptureMapping>,
    pub operation: MatchOperation,
    pub with_prototype: Option<ContextReference>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum ContextReference {
    #[non_exhaustive]
    Named(String),
    #[non_exhaustive]
    ByScope {
        scope: Scope,
        sub_context: Option<String>,
        /// `true` if this reference by scope is part of an `embed` for which
        /// there is an `escape`. In other words a reference for a context for
        /// which there "always is a way out". Enables falling back to `Plain
        /// Text` syntax in case the referenced scope is missing.
        with_escape: bool,
    },
    #[non_exhaustive]
    File {
        name: String,
        sub_context: Option<String>,
        /// Same semantics as for [`Self::ByScope::with_escape`].
        with_escape: bool,
    },
    #[non_exhaustive]
    Inline(String),
    #[non_exhaustive]
    Direct(ContextId),
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum MatchOperation {
    Push(Vec<ContextReference>),
    Set(Vec<ContextReference>),
    Pop,
    None,
}

impl<'a> Iterator for MatchIter<'a> {
    type Item = (&'a Context, usize);

    fn next(&mut self) -> Option<(&'a Context, usize)> {
        loop {
            if self.ctx_stack.is_empty() {
                return None;
            }
            // uncomment for debugging infinite recursion
            // println!("{:?}", self.index_stack);
            // use std::thread::sleep_ms;
            // sleep_ms(500);
            let last_index = self.ctx_stack.len() - 1;
            let context = self.ctx_stack[last_index];
            let index = self.index_stack[last_index];
            self.index_stack[last_index] = index + 1;
            if index < context.patterns.len() {
                match context.patterns[index] {
                    Pattern::Match(_) => {
                        return Some((context, index));
                    }
                    Pattern::Include(ref ctx_ref) => {
                        let ctx_ptr = match *ctx_ref {
                            ContextReference::Direct(ref context_id) => {
                                self.syntax_set.get_context(context_id).unwrap()
                            }
                            _ => return self.next(), // skip this and move onto the next one
                        };
                        self.ctx_stack.push(ctx_ptr);
                        self.index_stack.push(0);
                    }
                }
            } else {
                self.ctx_stack.pop();
                self.index_stack.pop();
            }
        }
    }
}

impl<'a> MatchIter<'a> {
    /// Controlled traversal checks inside includes, even when no match is ever yielded.
    /// A terminal error poisons this block's control; discard this iterator and parser.
    pub fn try_next_with_control(
        &mut self,
        control: &mut ExecutionControl<'_>,
    ) -> Result<Option<(&'a Context, usize)>, ParsingError> {
        let result = self.next_controlled(control);
        if result.is_err() {
            control.parsing_error();
        }
        result
    }

    fn next_controlled(
        &mut self,
        control: &mut ExecutionControl<'_>,
    ) -> Result<Option<(&'a Context, usize)>, ParsingError> {
        loop {
            control.checkpoint(Stage::IncludeStep, 1)?;
            if self.ctx_stack.is_empty() {
                return Ok(None);
            }
            let last_index = self.ctx_stack.len() - 1;
            let context = self.ctx_stack[last_index];
            let index = self.index_stack[last_index];
            self.index_stack[last_index] = index + 1;
            if index < context.patterns.len() {
                match &context.patterns[index] {
                    Pattern::Match(_) => return Ok(Some((context, index))),
                    Pattern::Include(ContextReference::Direct(context_id)) => {
                        let included = self.syntax_set.get_context(context_id)?;
                        control.checkpoint(Stage::IncludeStep, 0)?;
                        self.ctx_stack.push(included);
                        self.index_stack.push(0);
                    }
                    Pattern::Include(_) => continue,
                }
            } else {
                self.ctx_stack.pop();
                self.index_stack.pop();
            }
        }
    }
}

/// Returns an iterator over all the match patterns in this context.
///
/// It recursively follows include directives. Can only be run on contexts that have already been
/// linked up.
pub fn context_iter<'a>(syntax_set: &'a SyntaxSet, context: &'a Context) -> MatchIter<'a> {
    MatchIter {
        syntax_set,
        ctx_stack: vec![context],
        index_stack: vec![0],
    }
}

impl Context {
    /// Returns the match pattern at an index
    pub fn match_at(&self, index: usize) -> Result<&MatchPattern, ParsingError> {
        match self.patterns[index] {
            Pattern::Match(ref match_pat) => Ok(match_pat),
            _ => Err(ParsingError::BadMatchIndex(index)),
        }
    }
}

impl ContextReference {
    /// find the pointed to context
    pub fn resolve<'a>(&self, syntax_set: &'a SyntaxSet) -> Result<&'a Context, ParsingError> {
        match *self {
            ContextReference::Direct(ref context_id) => syntax_set.get_context(context_id),
            _ => Err(ParsingError::UnresolvedContextReference(self.clone())),
        }
    }

    /// get the context ID this reference points to
    pub fn id(&self) -> Result<ContextId, ParsingError> {
        match *self {
            ContextReference::Direct(ref context_id) => Ok(*context_id),
            _ => Err(ParsingError::UnresolvedContextReference(self.clone())),
        }
    }
}

pub(crate) fn substitute_backrefs_in_regex<F>(regex_str: &str, substituter: F) -> String
where
    F: Fn(usize) -> Option<String>,
{
    let mut reg_str = String::with_capacity(regex_str.len());

    let mut last_was_escape = false;
    for c in regex_str.chars() {
        if last_was_escape && c.is_ascii_digit() {
            let val = c.to_digit(10).unwrap() as usize;
            if let Some(sub) = substituter(val) {
                reg_str.push_str(&sub);
            }
        } else if last_was_escape {
            reg_str.push('\\');
            reg_str.push(c);
        } else if c != '\\' {
            reg_str.push(c);
        }

        last_was_escape = c == '\\' && !last_was_escape;
    }
    if last_was_escape {
        reg_str.push('\\');
    }
    reg_str
}

impl MatchPattern {
    pub fn new(
        has_captures: bool,
        regex_str: String,
        scope: Vec<Scope>,
        captures: Option<CaptureMapping>,
        operation: MatchOperation,
        with_prototype: Option<ContextReference>,
    ) -> MatchPattern {
        MatchPattern {
            has_captures,
            regex: Regex::new(regex_str),
            scope,
            captures,
            operation,
            with_prototype,
        }
    }

    /// Used by the parser to compile a regex which needs to reference
    /// regions from another matched pattern.
    pub fn regex_with_refs(&self, region: &Region, text: &str) -> Regex {
        let new_regex = substitute_backrefs_in_regex(self.regex.regex_str(), |i| {
            region.pos(i).map(|(start, end)| escape(&text[start..end]))
        });

        Regex::new(new_regex)
    }

    pub fn regex(&self) -> &Regex {
        &self.regex
    }
}

/// Serialize the provided map in natural key order, so that it's deterministic when dumping.
pub(crate) fn ordered_map<K, V, S>(map: &HashMap<K, V>, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
    K: Eq + Hash + Ord + Serialize,
    V: Serialize,
{
    let ordered: BTreeMap<_, _> = map.iter().collect();
    ordered.serialize(serializer)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn can_compile_refs() {
        let pat = MatchPattern {
            has_captures: true,
            regex: Regex::new(r"lol \\ \2 \1 '\9' \wz".into()),
            scope: vec![],
            captures: None,
            operation: MatchOperation::None,
            with_prototype: None,
        };
        let r = Regex::new(r"(\\\[\]\(\))(b)(c)(d)(e)".into());
        let s = r"\[]()bcde";
        let mut region = Region::new();
        let matched = r.search(s, 0, s.len(), Some(&mut region));
        assert!(matched);

        let regex_with_refs = pat.regex_with_refs(&region, s);
        assert_eq!(regex_with_refs.regex_str(), r"lol \\ b \\\[\]\(\) '' \wz");
    }
}
