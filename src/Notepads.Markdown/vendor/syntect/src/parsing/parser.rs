// Suppression of a false positive clippy lint. Upstream issue:
//
//   mutable_key_type false positive for raw pointers
//   https://github.com/rust-lang/rust-clippy/issues/6745
//
// We use `*const MatchPattern` as key in our `SearchCache` hash map.
// Clippy thinks this is a problem since `MatchPattern` has interior mutability
// via `MatchPattern::regex::regex` which is an `AtomicLazyCell`.
// But raw pointers are hashed via the pointer itself, not what is pointed to.
// See https://github.com/rust-lang/rust/blob/1.54.0/library/core/src/hash/mod.rs#L717-L725
#![allow(clippy::mutable_key_type)]

// Modifications Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
use super::execution::{ExecutionControl, ExecutionError, Stage};
use super::regex::Region;
use super::scope::*;
use super::syntax_definition::*;
use crate::parsing::syntax_definition::ContextId;
use crate::parsing::syntax_set::{SyntaxReference, SyntaxSet};
use fnv::FnvHasher;
use std::collections::HashMap;
use std::hash::BuildHasherDefault;

/// Errors that can occur while parsing.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ParsingError {
    #[error(transparent)]
    Execution(#[from] ExecutionError),
    #[error("Somehow main context was popped from the stack")]
    MissingMainContext,
    /// A context is missing. Usually caused by a syntax referencing a another
    /// syntax that is not known to syntect. See e.g. <https://github.com/trishume/syntect/issues/421>
    #[error("Missing context with ID '{0:?}'")]
    MissingContext(ContextId),
    #[error("Bad index to match_at: {0}")]
    BadMatchIndex(usize),
    #[error("Tried to use a ContextReference that has not bee resolved yet: {0:?}")]
    UnresolvedContextReference(ContextReference),
}

/// Keeps the current parser state (the internal syntax interpreter stack) between lines of parsing.
///
/// If you are parsing an entire file you create one of these at the start and use it
/// all the way to the end.
///
/// # Caching
///
/// One reason this is exposed is that since it implements `Clone` you can actually cache
/// these (probably along with a [`HighlightState`]) and only re-start parsing from the point of a change.
/// See the docs for [`HighlightState`] for more in-depth discussion of caching.
///
/// This state doesn't keep track of the current scope stack and parsing only returns changes to this stack
/// so if you want to construct scope stacks you'll need to keep track of that as well.
/// Note that [`HighlightState`] contains exactly this as a public field that you can use.
///
/// **Note:** Caching is for advanced users who have tons of time to maximize performance or want to do so eventually.
/// It is not recommended that you try caching the first time you implement highlighting.
///
/// [`HighlightState`]: ../highlighting/struct.HighlightState.html
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct ParseState {
    stack: Vec<StateLevel>,
    first_line: bool,
    // See issue #101. Contains indices of frames pushed by `with_prototype`s.
    // Doesn't look at `with_prototype`s below top of stack.
    proto_starts: Vec<usize>,
}

#[derive(Debug, Clone, Eq, PartialEq)]
struct StateLevel {
    context: ContextId,
    prototypes: Vec<ContextId>,
    captures: Option<(Region, String)>,
}

#[derive(Debug)]
struct RegexMatch<'a> {
    regions: Region,
    context: &'a Context,
    pat_index: usize,
    from_with_prototype: bool,
    would_loop: bool,
}

/// Maps the pattern to the start index, which is -1 if not found.
type SearchCache = HashMap<*const MatchPattern, Option<Region>, BuildHasherDefault<FnvHasher>>;

// To understand the implementation of this, here's an introduction to how
// Sublime Text syntax definitions work.
//
// Let's say we have the following made-up syntax definition:
//
//     contexts:
//       main:
//         - match: A
//           scope: scope.a.first
//           push: context-a
//         - match: b
//           scope: scope.b
//         - match: \w+
//           scope: scope.other
//       context-a:
//         - match: a+
//           scope: scope.a.rest
//         - match: (?=.)
//           pop: true
//
// There are two contexts, `main` and `context-a`. Each context contains a list
// of match rules with instructions for how to proceed.
//
// Let's say we have the input string " Aaaabxxx". We start at position 0 in
// the string. We keep a stack of contexts, which at the beginning is just main.
//
// So we start by looking at the top of the context stack (main), and look at
// the rules in order. The rule that wins is the first one that matches
// "earliest" in the input string. In our example:
//
// 1. The first one matches "A". Note that matches are not anchored, so this
//    matches at position 1.
// 2. The second one matches "b", so position 5. The first rule is winning.
// 3. The third one matches "\w+", so also position 1. But because the first
//    rule comes first, it wins.
//
// So now we execute the winning rule. Whenever we matched some text, we assign
// the scope (if there is one) to the matched text and advance our position to
// after the matched text. The scope is "scope.a.first" and our new position is
// after the "A", so 2. The "push" means that we should change our stack by
// pushing `context-a` on top of it.
//
// In the next step, we repeat the above, but now with the rules in `context-a`.
// The result is that we match "a+" and assign "scope.a.rest" to "aaa", and our
// new position is now after the "aaa". Note that there was no instruction for
// changing the stack, so we stay in that context.
//
// In the next step, the first rule doesn't match anymore, so we go to the next
// rule where "(?=.)" matches. The instruction is to "pop", which means we
// pop the top of our context stack, which means we're now back in main.
//
// This time in main, we match "b", and in the next step we match the rest with
// "\w+", and we're done.
//
//
// ## Preventing loops
//
// These are the basics of how matching works. Now, you saw that you can write
// patterns that result in an empty match and don't change the position. These
// are called non-consuming matches. The problem with them is that they could
// result in infinite loops. Let's look at a syntax where that is the case:
//
//     contexts:
//       main:
//         - match: (?=.)
//           push: test
//       test:
//         - match: \w+
//           scope: word
//         - match: (?=.)
//           pop: true
//
// This is a bit silly, but it's a minimal example for explaining how matching
// works in that case.
//
// Let's say we have the input string " hello". In `main`, our rule matches and
// we go into `test` and stay at position 0. Now, the best match is the rule
// with "pop". But if we used that rule, we'd pop back to `main` and would still
// be at the same position we started at! So this would be an infinite loop,
// which we don't want.
//
// So what Sublime Text does in case a looping rule "won":
//
// * If there's another rule that matches at the same position and does not
//   result in a loop, use that instead.
// * Otherwise, go to the next position and go through all the rules in the
//   current context again. Note that it means that the "pop" could again be the
//   winning rule, but that's ok as it wouldn't result in a loop anymore.
//
// So in our input string, we'd skip one character and try to match the rules
// again. This time, the "\w+" wins because it comes first.

impl ParseState {
    /// Creates a state from a syntax definition, keeping its own reference-counted point to the
    /// main context of the syntax
    pub fn new(syntax: &SyntaxReference) -> ParseState {
        let start_state = StateLevel {
            context: syntax.context_ids()["__start"],
            prototypes: Vec::new(),
            captures: None,
        };
        ParseState {
            stack: vec![start_state],
            first_line: true,
            proto_starts: Vec::new(),
        }
    }

    /// Parses a single line of the file. Because of the way regex engines work you unfortunately
    /// have to pass in a single line contiguous in memory. This can be bad for really long lines.
    /// Sublime Text avoids this by just not highlighting lines that are too long (thousands of characters).
    ///
    /// For efficiency reasons this returns only the changes to the current scope at each point in the line.
    /// You can use [`ScopeStack::apply`] on each operation in succession to get the stack for a given point.
    /// Look at the code in `highlighter.rs` for an example of doing this for highlighting purposes.
    ///
    /// The returned vector is in order both by index to apply at (the `usize`) and also by order to apply them at a
    /// given index (e.g popping old scopes before pushing new scopes).
    ///
    /// The [`SyntaxSet`] has to be the one that contained the syntax that was used to construct
    /// this [`ParseState`], or an extended version of it. Otherwise the parsing would return the
    /// wrong result or even panic. The reason for this is that contexts within the [`SyntaxSet`]
    /// are referenced via indexes.
    ///
    /// [`ScopeStack::apply`]: struct.ScopeStack.html#method.apply
    /// [`SyntaxSet`]: struct.SyntaxSet.html
    /// [`ParseState`]: struct.ParseState.html
    pub fn parse_line(
        &mut self,
        line: &str,
        syntax_set: &SyntaxSet,
    ) -> Result<Vec<(usize, ScopeStackOp)>, ParsingError> {
        self.parse_line_with_control(line, syntax_set, &mut ExecutionControl::uncontrolled())
    }

    /// Cooperative block-scoped path. Any error terminates the control; discard the parser
    /// and all block tokens. Opaque regex/initialization calls cannot be preempted.
    pub fn parse_line_with_control(
        &mut self,
        line: &str,
        syntax_set: &SyntaxSet,
        control: &mut ExecutionControl<'_>,
    ) -> Result<Vec<(usize, ScopeStackOp)>, ParsingError> {
        control.checkpoint(Stage::Line, 0)?;
        let result = self.parse_line_inner(line, syntax_set, control);
        if result.is_err() {
            control.parsing_error();
        }
        result
    }

    fn parse_line_inner(
        &mut self,
        line: &str,
        syntax_set: &SyntaxSet,
        control: &mut ExecutionControl<'_>,
    ) -> Result<Vec<(usize, ScopeStackOp)>, ParsingError> {
        if self.stack.is_empty() {
            return Err(ParsingError::MissingMainContext);
        }
        let mut match_start = 0;
        let mut res = Vec::new();

        if self.first_line {
            let cur_level = &self.stack[self.stack.len() - 1];
            control.checkpoint(Stage::ContextVisit, 0)?;
            let context = syntax_set.get_context(&cur_level.context)?;
            control.checkpoint(Stage::ContextVisit, 0)?;
            if !context.meta_content_scope.is_empty() {
                control.checkpoint(Stage::ScopeOperation, 1)?;
                res.push((0, ScopeStackOp::Push(context.meta_content_scope[0])));
            }
            self.first_line = false;
        }

        let mut regions = Region::new();
        let fnv = BuildHasherDefault::<FnvHasher>::default();
        let mut search_cache: SearchCache = HashMap::with_capacity_and_hasher(128, fnv);
        // Used for detecting loops with push/pop, see long comment above.
        let mut non_consuming_push_at = (0, 0);

        while self.parse_next_token(
            line,
            syntax_set,
            &mut match_start,
            &mut search_cache,
            &mut regions,
            &mut non_consuming_push_at,
            &mut res,
            control,
        )? {}

        control.checkpoint(Stage::Line, 0)?;
        Ok(res)
    }

    #[allow(clippy::too_many_arguments)]
    fn parse_next_token(
        &mut self,
        line: &str,
        syntax_set: &SyntaxSet,
        start: &mut usize,
        search_cache: &mut SearchCache,
        regions: &mut Region,
        non_consuming_push_at: &mut (usize, usize),
        ops: &mut Vec<(usize, ScopeStackOp)>,
        control: &mut ExecutionControl<'_>,
    ) -> Result<bool, ParsingError> {
        control.checkpoint(Stage::Token, 1)?;
        let check_pop_loop = {
            let (pos, stack_depth) = *non_consuming_push_at;
            pos == *start && stack_depth == self.stack.len()
        };

        // Trim proto_starts that are no longer valid
        while self
            .proto_starts
            .last()
            .map(|start| *start >= self.stack.len())
            .unwrap_or(false)
        {
            control.checkpoint(Stage::ContextChange, 1)?;
            self.proto_starts.pop();
        }

        let best_match = self.find_best_match(
            line,
            *start,
            syntax_set,
            search_cache,
            regions,
            check_pop_loop,
            control,
        )?;

        if let Some(reg_match) = best_match {
            if reg_match.would_loop {
                // A push that doesn't consume anything (a regex that resulted
                // in an empty match at the current position) can not be
                // followed by a non-consuming pop. Otherwise we're back where
                // we started and would try the same sequence of matches again,
                // resulting in an infinite loop. In this case, Sublime Text
                // advances one character and tries again, thus preventing the
                // loop.

                // println!("pop_would_loop for match {:?}, start {}", reg_match, *start);

                // nth(1) gets the next character if there is one. Need to do
                // this instead of just += 1 because we have byte indices and
                // unicode characters can be more than 1 byte.
                if let Some((i, _)) = line[*start..].char_indices().nth(1) {
                    *start += i;
                    return Ok(true);
                } else {
                    // End of line, no character to advance and no point trying
                    // any more patterns.
                    return Ok(false);
                }
            }

            let match_end = reg_match.regions.pos(0).unwrap().1;

            let consuming = match_end > *start;
            if !consuming {
                // The match doesn't consume any characters. If this is a
                // "push", remember the position and stack size so that we can
                // check the next "pop" for loops. Otherwise leave the state,
                // e.g. non-consuming "set" could also result in a loop.
                let context = reg_match.context;
                let match_pattern = context.match_at(reg_match.pat_index)?;
                if let MatchOperation::Push(_) = match_pattern.operation {
                    *non_consuming_push_at = (match_end, self.stack.len() + 1);
                }
            }

            *start = match_end;

            // ignore `with_prototype`s below this if a context is pushed
            if reg_match.from_with_prototype {
                // use current height, since we're before the actual push
                self.proto_starts.push(self.stack.len());
            }

            let level_context = {
                let id = &self.stack[self.stack.len() - 1].context;
                syntax_set.get_context(id)?
            };
            self.exec_pattern(line, &reg_match, level_context, syntax_set, ops, control)?;

            Ok(true)
        } else {
            Ok(false)
        }
    }

    fn find_best_match<'a>(
        &self,
        line: &str,
        start: usize,
        syntax_set: &'a SyntaxSet,
        search_cache: &mut SearchCache,
        regions: &mut Region,
        check_pop_loop: bool,
        control: &mut ExecutionControl<'_>,
    ) -> Result<Option<RegexMatch<'a>>, ParsingError> {
        let cur_level = &self.stack[self.stack.len() - 1];
        control.checkpoint(Stage::ContextVisit, 0)?;
        let context = syntax_set.get_context(&cur_level.context)?;
        control.checkpoint(Stage::ContextVisit, 0)?;
        let prototype = if let Some(ref p) = context.prototype {
            Some(p)
        } else {
            None
        };

        // Build an iterator for the contexts we want to visit in order
        let context_chain = {
            let proto_start = self.proto_starts.last().cloned().unwrap_or(0);
            // Sublime applies with_prototypes from bottom to top
            let with_prototypes = self.stack[proto_start..].iter().flat_map(|lvl| {
                lvl.prototypes
                    .iter()
                    .map(move |ctx| (true, ctx, lvl.captures.as_ref()))
            });
            let cur_prototype = prototype.into_iter().map(|ctx| (false, ctx, None));
            let cur_context =
                Some((false, &cur_level.context, cur_level.captures.as_ref())).into_iter();
            with_prototypes.chain(cur_prototype).chain(cur_context)
        };

        // println!("{:#?}", cur_level);
        // println!("token at {} on {}", start, line.trim_right());

        let mut min_start = usize::MAX;
        let mut best_match: Option<RegexMatch<'_>> = None;
        let mut pop_would_loop = false;

        for (from_with_proto, ctx, captures) in context_chain {
            control.checkpoint(Stage::ContextVisit, 1)?;
            let mut patterns = context_iter(syntax_set, syntax_set.get_context(ctx)?);
            while let Some((pat_context, pat_index)) = patterns.try_next_with_control(control)? {
                control.checkpoint(Stage::PatternVisit, 1)?;
                let match_pat = pat_context.match_at(pat_index)?;

                if let Some(match_region) = self.search(
                    line,
                    start,
                    match_pat,
                    captures,
                    search_cache,
                    regions,
                    control,
                )? {
                    let (match_start, match_end) = match_region.pos(0).unwrap();

                    // println!("matched pattern {:?} at start {} end {} (pop would loop: {}, min start: {}, initial start: {}, check_pop_loop: {}, stack_len: {})", match_pat, match_start, match_end, pop_would_loop, min_start, start, check_pop_loop, self.stack.len());

                    if match_start < min_start || (match_start == min_start && pop_would_loop) {
                        // New match is earlier in text than old match,
                        // or old match was a looping pop at the same
                        // position.

                        // println!("setting as current match");

                        min_start = match_start;

                        let consuming = match_end > start;
                        pop_would_loop = check_pop_loop
                            && !consuming
                            && matches!(match_pat.operation, MatchOperation::Pop);

                        let push_too_deep = matches!(match_pat.operation, MatchOperation::Push(_))
                            && self.stack.len() >= 100;

                        if push_too_deep {
                            return Ok(None);
                        }

                        best_match = Some(RegexMatch {
                            regions: match_region,
                            context: pat_context,
                            pat_index,
                            from_with_prototype: from_with_proto,
                            would_loop: pop_would_loop,
                        });

                        if match_start == start && !pop_would_loop {
                            // We're not gonna find a better match after this,
                            // so as an optimization we can stop matching now.
                            return Ok(best_match);
                        }
                    }
                }
            }
        }
        Ok(best_match)
    }

    fn search(
        &self,
        line: &str,
        start: usize,
        match_pat: &MatchPattern,
        captures: Option<&(Region, String)>,
        search_cache: &mut SearchCache,
        regions: &mut Region,
        control: &mut ExecutionControl<'_>,
    ) -> Result<Option<Region>, ParsingError> {
        // println!("{} - {:?} - {:?}", match_pat.regex_str, match_pat.has_captures, cur_level.captures.is_some());
        let match_ptr = match_pat as *const MatchPattern;

        if let Some(maybe_region) = search_cache.get(&match_ptr) {
            control.checkpoint(Stage::CacheHit, 1)?;
            if let Some(ref region) = *maybe_region {
                let match_start = region.pos(0).unwrap().0;
                if match_start >= start {
                    // Cached match is valid, return it. Otherwise do another
                    // search below.
                    return Ok(Some(region.clone()));
                }
            } else {
                // Didn't find a match earlier, so no point trying to match it again
                return Ok(None);
            }
        }

        let dynamic;
        let (regex, can_cache) = match (match_pat.has_captures, captures) {
            (true, Some(captures)) => {
                control.checkpoint(Stage::Backreference, 1)?;
                let (region, s) = captures;
                dynamic = match_pat.regex_with_refs(region, s);
                control.checkpoint(Stage::Backreference, 0)?;
                (&dynamic, false)
            }
            _ => (match_pat.regex(), true),
        };
        // print!("  executing regex: {:?} at pos {} on line {}", regex.regex_str(), start, line);
        let matched =
            regex.try_search_with_control(line, start, line.len(), Some(regions), control)?;

        if matched {
            let (match_start, match_end) = regions.pos(0).unwrap();
            // this is necessary to avoid infinite looping on dumb patterns
            let does_something = match match_pat.operation {
                MatchOperation::None => match_start != match_end,
                MatchOperation::Push(_) => self.stack.len() < 100,
                _ => true,
            };
            if can_cache && does_something {
                control.checkpoint(Stage::CacheWrite, 1)?;
                search_cache.insert(match_pat, Some(regions.clone()));
            }
            if does_something {
                // print!("catch {} at {} on {}", match_pat.regex_str, match_start, line);
                return Ok(Some(regions.clone()));
            }
        } else if can_cache {
            control.checkpoint(Stage::CacheWrite, 1)?;
            search_cache.insert(match_pat, None);
        }
        Ok(None)
    }

    /// Returns true if the stack was changed
    fn exec_pattern<'a>(
        &mut self,
        line: &str,
        reg_match: &RegexMatch<'a>,
        level_context: &'a Context,
        syntax_set: &'a SyntaxSet,
        ops: &mut Vec<(usize, ScopeStackOp)>,
        control: &mut ExecutionControl<'_>,
    ) -> Result<bool, ParsingError> {
        let (match_start, match_end) = reg_match.regions.pos(0).unwrap();
        let context = reg_match.context;
        let pat = context.match_at(reg_match.pat_index)?;
        // println!("running pattern {:?} on '{}' at {}, operation {:?}", pat.regex_str, line, match_start, pat.operation);

        self.push_meta_ops(
            true,
            match_start,
            level_context,
            &pat.operation,
            syntax_set,
            ops,
            control,
        )?;
        for s in &pat.scope {
            // println!("pushing {:?} at {}", s, match_start);
            control.checkpoint(Stage::ScopeOperation, 1)?;
            ops.push((match_start, ScopeStackOp::Push(*s)));
        }
        if let Some(ref capture_map) = pat.captures {
            // captures could appear in an arbitrary order, have to produce ops in right order
            // ex: ((bob)|(hi))* could match hibob in wrong order, and outer has to push first
            // we don't have to handle a capture matching multiple times, Sublime doesn't
            let mut map: Vec<((usize, i32), ScopeStackOp)> = Vec::new();
            for &(cap_index, ref scopes) in capture_map.iter() {
                control.checkpoint(Stage::Capture, 1)?;
                if let Some((cap_start, cap_end)) = reg_match.regions.pos(cap_index) {
                    // marking up empty captures causes pops to be sorted wrong
                    if cap_start == cap_end {
                        continue;
                    }
                    // println!("capture {:?} at {:?}-{:?}", scopes[0], cap_start, cap_end);
                    for scope in scopes.iter() {
                        control.checkpoint(Stage::Capture, 1)?;
                        map.push((
                            (cap_start, -((cap_end - cap_start) as i32)),
                            ScopeStackOp::Push(*scope),
                        ));
                    }
                    map.push(((cap_end, i32::MIN), ScopeStackOp::Pop(scopes.len())));
                }
            }
            control.checkpoint(Stage::Capture, 0)?;
            map.sort_by(|a, b| a.0.cmp(&b.0));
            control.checkpoint(Stage::Capture, 0)?;
            for ((index, _), op) in map.into_iter() {
                control.checkpoint(Stage::ScopeOperation, 1)?;
                ops.push((index, op));
            }
        }
        if !pat.scope.is_empty() {
            // println!("popping at {}", match_end);
            control.checkpoint(Stage::ScopeOperation, 1)?;
            ops.push((match_end, ScopeStackOp::Pop(pat.scope.len())));
        }
        self.push_meta_ops(
            false,
            match_end,
            level_context,
            &pat.operation,
            syntax_set,
            ops,
            control,
        )?;

        self.perform_op(line, &reg_match.regions, pat, syntax_set, control)
    }

    fn push_meta_ops(
        &self,
        initial: bool,
        index: usize,
        cur_context: &Context,
        match_op: &MatchOperation,
        syntax_set: &SyntaxSet,
        ops: &mut Vec<(usize, ScopeStackOp)>,
        control: &mut ExecutionControl<'_>,
    ) -> Result<(), ParsingError> {
        // println!("metas ops for {:?}, initial: {}",
        //          match_op,
        //          initial);
        // println!("{:?}", cur_context.meta_scope);
        match *match_op {
            MatchOperation::Pop => {
                let v = if initial {
                    &cur_context.meta_content_scope
                } else {
                    &cur_context.meta_scope
                };
                if !v.is_empty() {
                    control.checkpoint(Stage::ScopeOperation, 1)?;
                    ops.push((index, ScopeStackOp::Pop(v.len())));
                }

                // cleared scopes are restored after the scopes from match pattern that invoked the pop are applied
                if !initial && cur_context.clear_scopes.is_some() {
                    control.checkpoint(Stage::ScopeOperation, 1)?;
                    ops.push((index, ScopeStackOp::Restore))
                }
            }
            // for some reason the ST3 behaviour of set is convoluted and is inconsistent with the docs and other ops
            // - the meta_content_scope of the current context is applied to the matched thing, unlike pop
            // - the clear_scopes are applied after the matched token, unlike push
            // - the interaction with meta scopes means that the token has the meta scopes of both the current scope and the new scope.
            MatchOperation::Push(ref context_refs) | MatchOperation::Set(ref context_refs) => {
                let is_set = matches!(*match_op, MatchOperation::Set(_));
                // a match pattern that "set"s keeps the meta_content_scope and meta_scope from the previous context
                if initial {
                    if is_set && cur_context.clear_scopes.is_some() {
                        // cleared scopes from the old context are restored immediately
                        control.checkpoint(Stage::ScopeOperation, 1)?;
                        ops.push((index, ScopeStackOp::Restore));
                    }
                    // add each context's meta scope
                    for r in context_refs.iter() {
                        control.checkpoint(Stage::ContextVisit, 1)?;
                        control.checkpoint(Stage::ContextVisit, 0)?;
                        let ctx = r.resolve(syntax_set)?;
                        control.checkpoint(Stage::ContextVisit, 0)?;

                        if !is_set {
                            if let Some(clear_amount) = ctx.clear_scopes {
                                control.checkpoint(Stage::ScopeOperation, 1)?;
                                ops.push((index, ScopeStackOp::Clear(clear_amount)));
                            }
                        }

                        for scope in ctx.meta_scope.iter() {
                            control.checkpoint(Stage::ScopeOperation, 1)?;
                            ops.push((index, ScopeStackOp::Push(*scope)));
                        }
                    }
                } else {
                    let mut repush = is_set
                        && (!cur_context.meta_scope.is_empty()
                            || !cur_context.meta_content_scope.is_empty());
                    if !repush {
                        for r in context_refs {
                            control.checkpoint(Stage::ContextVisit, 1)?;
                            control.checkpoint(Stage::ContextVisit, 0)?;
                            let ctx = r.resolve(syntax_set)?;
                            control.checkpoint(Stage::ContextVisit, 0)?;
                            if !ctx.meta_content_scope.is_empty()
                                || (ctx.clear_scopes.is_some() && is_set)
                            {
                                repush = true;
                                break;
                            }
                        }
                    }
                    if repush {
                        // remove previously pushed meta scopes, so that meta content scopes will be applied in the correct order
                        let mut num_to_pop = 0usize;
                        for r in context_refs {
                            control.checkpoint(Stage::ContextVisit, 1)?;
                            control.checkpoint(Stage::ContextVisit, 0)?;
                            let ctx = r.resolve(syntax_set)?;
                            control.checkpoint(Stage::ContextVisit, 0)?;
                            num_to_pop += ctx.meta_scope.len();
                        }

                        // also pop off the original context's meta scopes
                        if is_set {
                            num_to_pop +=
                                cur_context.meta_content_scope.len() + cur_context.meta_scope.len();
                        }

                        // do all the popping as one operation
                        if num_to_pop > 0 {
                            control.checkpoint(Stage::ScopeOperation, 1)?;
                            ops.push((index, ScopeStackOp::Pop(num_to_pop)));
                        }

                        // now we push meta scope and meta context scope for each context pushed
                        for r in context_refs {
                            control.checkpoint(Stage::ContextVisit, 1)?;
                            control.checkpoint(Stage::ContextVisit, 0)?;
                            let ctx = r.resolve(syntax_set)?;
                            control.checkpoint(Stage::ContextVisit, 0)?;

                            // for some reason, contrary to my reading of the docs, set does this after the token
                            if is_set {
                                if let Some(clear_amount) = ctx.clear_scopes {
                                    control.checkpoint(Stage::ScopeOperation, 1)?;
                                    ops.push((index, ScopeStackOp::Clear(clear_amount)));
                                }
                            }

                            for scope in ctx.meta_scope.iter() {
                                control.checkpoint(Stage::ScopeOperation, 1)?;
                                ops.push((index, ScopeStackOp::Push(*scope)));
                            }
                            for scope in ctx.meta_content_scope.iter() {
                                control.checkpoint(Stage::ScopeOperation, 1)?;
                                ops.push((index, ScopeStackOp::Push(*scope)));
                            }
                        }
                    }
                }
            }
            MatchOperation::None => (),
        }

        Ok(())
    }

    /// Returns true if the stack was changed
    fn perform_op(
        &mut self,
        line: &str,
        regions: &Region,
        pat: &MatchPattern,
        syntax_set: &SyntaxSet,
        control: &mut ExecutionControl<'_>,
    ) -> Result<bool, ParsingError> {
        control.checkpoint(Stage::ContextChange, 0)?;
        let (ctx_refs, old_proto_ids) = match pat.operation {
            MatchOperation::Push(ref ctx_refs) => (ctx_refs, None),
            MatchOperation::Set(ref ctx_refs) => {
                // a `with_prototype` stays active when the context is `set`
                // until the context layer in the stack (where the `with_prototype`
                // was initially applied) is popped off.
                (ctx_refs, self.stack.pop().map(|s| s.prototypes))
            }
            MatchOperation::Pop => {
                self.stack.pop();
                return Ok(true);
            }
            MatchOperation::None => return Ok(false),
        };
        for (i, r) in ctx_refs.iter().enumerate() {
            control.checkpoint(Stage::ContextChange, 1)?;
            let mut proto_ids = if i == 0 {
                // it is only necessary to preserve the old prototypes
                // at the first stack frame pushed
                old_proto_ids.clone().unwrap_or_else(Vec::new)
            } else {
                Vec::new()
            };
            if i == ctx_refs.len() - 1 {
                // if a with_prototype was specified, and multiple contexts were pushed,
                // then the with_prototype applies only to the last context pushed, i.e.
                // top most on the stack after all the contexts are pushed - this is also
                // referred to as the "target" of the push by sublimehq - see
                // https://forum.sublimetext.com/t/dev-build-3111/19240/17 for more info
                if let Some(ref p) = pat.with_prototype {
                    proto_ids.push(p.id()?);
                }
            }
            let context_id = r.id()?;
            control.checkpoint(Stage::ContextVisit, 0)?;
            let context = syntax_set.get_context(&context_id)?;
            control.checkpoint(Stage::ContextVisit, 0)?;
            let captures = {
                let mut uses_backrefs = context.uses_backrefs;
                if !proto_ids.is_empty() {
                    if !uses_backrefs {
                        for id in &proto_ids {
                            control.checkpoint(Stage::ContextVisit, 1)?;
                            control.checkpoint(Stage::ContextVisit, 0)?;
                            let uses = syntax_set.get_context(id)?.uses_backrefs;
                            control.checkpoint(Stage::ContextVisit, 0)?;
                            if uses {
                                uses_backrefs = true;
                                break;
                            }
                        }
                    }
                }
                if uses_backrefs {
                    control.checkpoint(Stage::Backreference, 1)?;
                    let captures = Some((regions.clone(), line.to_owned()));
                    control.checkpoint(Stage::Backreference, 0)?;
                    captures
                } else {
                    None
                }
            };
            self.stack.push(StateLevel {
                context: context_id,
                prototypes: proto_ids,
                captures,
            });
        }
        Ok(true)
    }
}

#[cfg(feature = "yaml-load")]
#[cfg(test)]
mod tests {
    use super::*;
    use crate::parsing::ScopeStackOp::{Clear, Pop, Push, Restore};
    use crate::parsing::{Scope, ScopeStack, SyntaxSet, SyntaxSetBuilder};
    use crate::util::debug_print_ops;
    use crate::utils::testdata;

    const TEST_SYNTAX: &str = include_str!("../../testdata/parser_tests.sublime-syntax");
    #[test]
    fn can_parse_simple() {
        let ss = &*testdata::PACKAGES_SYN_SET;
        let mut state = {
            let syntax = ss.find_syntax_by_name("Ruby on Rails").unwrap();
            ParseState::new(syntax)
        };

        let ops1 = ops(&mut state, "module Bob::Wow::Troll::Five; 5; end", &ss);
        let test_ops1 = vec![
            (0, Push(Scope::new("source.ruby.rails").unwrap())),
            (0, Push(Scope::new("meta.module.ruby").unwrap())),
            (0, Push(Scope::new("keyword.control.module.ruby").unwrap())),
            (6, Pop(2)),
            (6, Push(Scope::new("meta.module.ruby").unwrap())),
            (7, Pop(1)),
            (7, Push(Scope::new("meta.module.ruby").unwrap())),
            (7, Push(Scope::new("entity.name.module.ruby").unwrap())),
            (7, Push(Scope::new("support.other.namespace.ruby").unwrap())),
            (10, Pop(1)),
            (10, Push(Scope::new("punctuation.accessor.ruby").unwrap())),
        ];
        assert_eq!(&ops1[0..test_ops1.len()], &test_ops1[..]);

        let ops2 = ops(&mut state, "def lol(wow = 5)", &ss);
        let test_ops2 = vec![
            (0, Push(Scope::new("meta.function.ruby").unwrap())),
            (0, Push(Scope::new("keyword.control.def.ruby").unwrap())),
            (3, Pop(2)),
            (3, Push(Scope::new("meta.function.ruby").unwrap())),
            (4, Push(Scope::new("entity.name.function.ruby").unwrap())),
            (7, Pop(1)),
        ];
        assert_eq!(&ops2[0..test_ops2.len()], &test_ops2[..]);
    }

    #[test]
    fn can_parse_yaml() {
        let ps = &*testdata::PACKAGES_SYN_SET;
        let mut state = {
            let syntax = ps.find_syntax_by_name("YAML").unwrap();
            ParseState::new(syntax)
        };

        assert_eq!(
            ops(&mut state, "key: value\n", &ps),
            vec![
                (0, Push(Scope::new("source.yaml").unwrap())),
                (
                    0,
                    Push(Scope::new("string.unquoted.plain.out.yaml").unwrap())
                ),
                (0, Push(Scope::new("entity.name.tag.yaml").unwrap())),
                (3, Pop(2)),
                (
                    3,
                    Push(Scope::new("punctuation.separator.key-value.mapping.yaml").unwrap())
                ),
                (4, Pop(1)),
                (
                    5,
                    Push(Scope::new("string.unquoted.plain.out.yaml").unwrap())
                ),
                (10, Pop(1)),
            ]
        );
    }

    #[test]
    fn can_parse_includes() {
        let ss = &*testdata::PACKAGES_SYN_SET;
        let mut state = {
            let syntax = ss.find_syntax_by_name("HTML (Rails)").unwrap();
            ParseState::new(syntax)
        };

        let ops = ops(&mut state, "<script>var lol = '<% def wow(", &ss);

        let mut test_stack = ScopeStack::new();
        test_stack.push(Scope::new("text.html.ruby").unwrap());
        test_stack.push(Scope::new("text.html.basic").unwrap());
        test_stack.push(Scope::new("source.js.embedded.html").unwrap());
        test_stack.push(Scope::new("source.js").unwrap());
        test_stack.push(Scope::new("string.quoted.single.js").unwrap());
        test_stack.push(Scope::new("source.ruby.rails.embedded.html").unwrap());
        test_stack.push(Scope::new("meta.function.parameters.ruby").unwrap());

        let mut stack = ScopeStack::new();
        for (_, op) in ops.iter() {
            stack.apply(op).expect("#[cfg(test)]");
        }
        assert_eq!(stack, test_stack);
    }

    #[test]
    fn can_parse_backrefs() {
        let ss = &*testdata::PACKAGES_SYN_SET;
        let mut state = {
            let syntax = ss.find_syntax_by_name("Ruby on Rails").unwrap();
            ParseState::new(syntax)
        };

        // For parsing HEREDOC, the "SQL" is captured at the beginning and then used in another
        // regex with a backref, to match the end of the HEREDOC. Note that there can be code
        // after the marker (`.strip`) here.
        assert_eq!(
            ops(&mut state, "lol = <<-SQL.strip", &ss),
            vec![
                (0, Push(Scope::new("source.ruby.rails").unwrap())),
                (
                    4,
                    Push(Scope::new("keyword.operator.assignment.ruby").unwrap())
                ),
                (5, Pop(1)),
                (
                    6,
                    Push(Scope::new("string.unquoted.embedded.sql.ruby").unwrap())
                ),
                (
                    6,
                    Push(Scope::new("punctuation.definition.string.begin.ruby").unwrap())
                ),
                (12, Pop(1)),
                (12, Pop(1)),
                (
                    12,
                    Push(Scope::new("string.unquoted.embedded.sql.ruby").unwrap())
                ),
                (12, Push(Scope::new("text.sql.embedded.ruby").unwrap())),
                (12, Clear(ClearAmount::TopN(2))),
                (12, Push(Scope::new("punctuation.accessor.ruby").unwrap())),
                (13, Pop(1)),
                (18, Restore),
            ]
        );

        assert_eq!(ops(&mut state, "wow", &ss), vec![]);

        assert_eq!(
            ops(&mut state, "SQL", &ss),
            vec![
                (0, Pop(1)),
                (
                    0,
                    Push(Scope::new("punctuation.definition.string.end.ruby").unwrap())
                ),
                (3, Pop(1)),
                (3, Pop(1)),
            ]
        );
    }

    #[test]
    fn can_parse_preprocessor_rules() {
        let ss = &*testdata::PACKAGES_SYN_SET;
        let mut state = {
            let syntax = ss.find_syntax_by_name("C").unwrap();
            ParseState::new(syntax)
        };

        assert_eq!(
            ops(&mut state, "#ifdef FOO", &ss),
            vec![
                (0, Push(Scope::new("source.c").unwrap())),
                (0, Push(Scope::new("meta.preprocessor.c").unwrap())),
                (0, Push(Scope::new("keyword.control.import.c").unwrap())),
                (6, Pop(1)),
                (10, Pop(1)),
            ]
        );
        assert_eq!(
            ops(&mut state, "{", &ss),
            vec![
                (0, Push(Scope::new("meta.block.c").unwrap())),
                (
                    0,
                    Push(Scope::new("punctuation.section.block.begin.c").unwrap())
                ),
                (1, Pop(1)),
            ]
        );
        assert_eq!(
            ops(&mut state, "#else", &ss),
            vec![
                (0, Push(Scope::new("meta.preprocessor.c").unwrap())),
                (0, Push(Scope::new("keyword.control.import.c").unwrap())),
                (5, Pop(1)),
                (5, Pop(1)),
            ]
        );
        assert_eq!(
            ops(&mut state, "{", &ss),
            vec![
                (0, Push(Scope::new("meta.block.c").unwrap())),
                (
                    0,
                    Push(Scope::new("punctuation.section.block.begin.c").unwrap())
                ),
                (1, Pop(1)),
            ]
        );
        assert_eq!(
            ops(&mut state, "#endif", &ss),
            vec![
                (0, Pop(1)),
                (0, Push(Scope::new("meta.block.c").unwrap())),
                (0, Push(Scope::new("meta.preprocessor.c").unwrap())),
                (0, Push(Scope::new("keyword.control.import.c").unwrap())),
                (6, Pop(2)),
                (6, Pop(2)),
                (6, Push(Scope::new("meta.block.c").unwrap())),
            ]
        );
        assert_eq!(
            ops(&mut state, "    foo;", &ss),
            vec![
                (7, Push(Scope::new("punctuation.terminator.c").unwrap())),
                (8, Pop(1)),
            ]
        );
        assert_eq!(
            ops(&mut state, "}", &ss),
            vec![
                (
                    0,
                    Push(Scope::new("punctuation.section.block.end.c").unwrap())
                ),
                (1, Pop(1)),
                (1, Pop(1)),
            ]
        );
    }

    #[test]
    fn can_parse_issue25() {
        let ss = &*testdata::PACKAGES_SYN_SET;
        let mut state = {
            let syntax = ss.find_syntax_by_name("C").unwrap();
            ParseState::new(syntax)
        };

        // test fix for issue #25
        assert_eq!(ops(&mut state, "struct{estruct", &ss).len(), 10);
    }

    #[test]
    fn can_compare_parse_states() {
        let ss = &*testdata::PACKAGES_SYN_SET;
        let syntax = ss.find_syntax_by_name("Java").unwrap();
        let mut state1 = ParseState::new(syntax);
        let mut state2 = ParseState::new(syntax);

        assert_eq!(ops(&mut state1, "class Foo {", &ss).len(), 11);
        assert_eq!(ops(&mut state2, "class Fooo {", &ss).len(), 11);

        assert_eq!(state1, state2);
        ops(&mut state1, "}", &ss);
        assert_ne!(state1, state2);
    }

    #[test]
    fn can_parse_non_nested_clear_scopes() {
        let line = "'hello #simple_cleared_scopes_test world test \\n '";
        let expect = [
            "<source.test>, <example.meta-scope.after-clear-scopes.example>, <example.pushes-clear-scopes.example>",
            "<source.test>, <example.meta-scope.after-clear-scopes.example>, <example.pops-clear-scopes.example>",
            "<source.test>, <string.quoted.single.example>, <constant.character.escape.example>",
        ];
        expect_scope_stacks(line, &expect, TEST_SYNTAX);
    }

    #[test]
    fn can_parse_non_nested_too_many_clear_scopes() {
        let line = "'hello #too_many_cleared_scopes_test world test \\n '";
        let expect = [
            "<example.meta-scope.after-clear-scopes.example>, <example.pushes-clear-scopes.example>",
            "<example.meta-scope.after-clear-scopes.example>, <example.pops-clear-scopes.example>",
            "<source.test>, <string.quoted.single.example>, <constant.character.escape.example>",
        ];
        expect_scope_stacks(line, &expect, TEST_SYNTAX);
    }

    #[test]
    fn can_parse_nested_clear_scopes() {
        let line = "'hello #nested_clear_scopes_test world foo bar test \\n '";
        let expect = [
            "<source.test>, <example.meta-scope.after-clear-scopes.example>, <example.pushes-clear-scopes.example>",
            "<source.test>, <example.meta-scope.cleared-previous-meta-scope.example>, <foo>",
            "<source.test>, <example.meta-scope.after-clear-scopes.example>, <example.pops-clear-scopes.example>",
            "<source.test>, <string.quoted.single.example>, <constant.character.escape.example>",
        ];
        expect_scope_stacks(line, &expect, TEST_SYNTAX);
    }

    #[test]
    fn can_parse_infinite_loop() {
        let line = "#infinite_loop_test 123";
        let expect = ["<source.test>, <constant.numeric.test>"];
        expect_scope_stacks(line, &expect, TEST_SYNTAX);
    }

    #[test]
    fn can_parse_infinite_seeming_loop() {
        // See https://github.com/SublimeTextIssues/Core/issues/1190 for an
        // explanation.
        let line = "#infinite_seeming_loop_test hello";
        let expect = [
            "<source.test>, <keyword.test>",
            "<source.test>, <test>, <string.unquoted.test>",
            "<source.test>, <test>, <keyword.control.test>",
        ];
        expect_scope_stacks(line, &expect, TEST_SYNTAX);
    }

    #[test]
    fn can_parse_prototype_that_pops_main() {
        let syntax = r#"
name: test
scope: source.test
contexts:
  prototype:
    # This causes us to pop out of the main context. Sublime Text handles that
    # by pushing main back automatically.
    - match: (?=!)
      pop: true
  main:
    - match: foo
      scope: test.good
"#;

        let line = "foo!";
        let expect = ["<source.test>, <test.good>"];
        expect_scope_stacks(line, &expect, syntax);
    }

    #[test]
    fn can_parse_syntax_with_newline_in_character_class() {
        let syntax = r#"
name: test
scope: source.test
contexts:
  main:
    - match: foo[\n]
      scope: foo.end
    - match: foo
      scope: foo.any
"#;

        let line = "foo";
        let expect = ["<source.test>, <foo.end>"];
        expect_scope_stacks(line, &expect, syntax);

        let line = "foofoofoo";
        let expect = [
            "<source.test>, <foo.any>",
            "<source.test>, <foo.any>",
            "<source.test>, <foo.end>",
        ];
        expect_scope_stacks(line, &expect, syntax);
    }

    #[test]
    fn can_parse_issue120() {
        let syntax = SyntaxDefinition::load_from_str(
            include_str!("../../testdata/embed_escape_test.sublime-syntax"),
            false,
            None,
        )
        .unwrap();

        let line1 = "\"abctest\" foobar";
        let expect1 = [
            "<meta.attribute-with-value.style.html>, <string.quoted.double>, <punctuation.definition.string.begin.html>",
            "<meta.attribute-with-value.style.html>, <source.css>",
            "<meta.attribute-with-value.style.html>, <string.quoted.double>, <punctuation.definition.string.end.html>",
            "<meta.attribute-with-value.style.html>, <source.css>, <test.embedded>",
            "<top-level.test>",
        ];

        expect_scope_stacks_with_syntax(line1, &expect1, syntax.clone());

        let line2 = ">abctest</style>foobar";
        let expect2 = [
            "<meta.tag.style.begin.html>, <punctuation.definition.tag.end.html>",
            "<source.css.embedded.html>, <test.embedded>",
            "<top-level.test>",
        ];
        expect_scope_stacks_with_syntax(line2, &expect2, syntax);
    }

    #[test]
    fn can_parse_non_consuming_pop_that_would_loop() {
        // See https://github.com/trishume/syntect/issues/127
        let syntax = r#"
name: test
scope: source.test
contexts:
  main:
    # This makes us go into "test" without consuming any characters
    - match: (?=hello)
      push: test
  test:
    # If we used this match, we'd go back to "main" without consuming anything,
    # and then back into "test", infinitely looping. ST detects this at this
    # point and ignores this match until at least one character matched.
    - match: (?!world)
      pop: true
    - match: \w+
      scope: test.matched
"#;

        let line = "hello";
        let expect = ["<source.test>, <test.matched>"];
        expect_scope_stacks(line, &expect, syntax);
    }

    #[test]
    fn can_parse_non_consuming_set_and_pop_that_would_loop() {
        let syntax = r#"
name: test
scope: source.test
contexts:
  main:
    # This makes us go into "a" without advancing
    - match: (?=test)
      push: a
  a:
    # This makes us go into "b" without advancing
    - match: (?=t)
      set: b
  b:
    # If we used this match, we'd go back to "main" without having advanced,
    # which means we'd have an infinite loop like with the previous test.
    # So even for a "set", we have to check if we're advancing or not.
    - match: (?=t)
      pop: true
    - match: \w+
      scope: test.matched
"#;

        let line = "test";
        let expect = ["<source.test>, <test.matched>"];
        expect_scope_stacks(line, &expect, syntax);
    }

    #[test]
    fn can_parse_non_consuming_set_after_consuming_push_that_does_not_loop() {
        let syntax = r#"
name: test
scope: source.test
contexts:
  main:
    # This makes us go into "a", but we consumed a character
    - match: t
      push: a
    - match: \w+
      scope: test.matched
  a:
    # This makes us go into "b" without consuming
    - match: (?=e)
      set: b
  b:
    # This match does not result in an infinite loop because we already consumed
    # a character to get into "a", so it's ok to pop back into "main".
    - match: (?=e)
      pop: true
"#;

        let line = "test";
        let expect = ["<source.test>, <test.matched>"];
        expect_scope_stacks(line, &expect, syntax);
    }

    #[test]
    fn can_parse_non_consuming_set_after_consuming_set_that_does_not_loop() {
        let syntax = r#"
name: test
scope: source.test
contexts:
  main:
    - match: (?=hello)
      push: a
    - match: \w+
      scope: test.matched
  a:
    - match: h
      set: b
  b:
    - match: (?=e)
      set: c
  c:
    # This is not an infinite loop because "a" consumed a character, so we can
    # actually pop back into main and then match the rest of the input.
    - match: (?=e)
      pop: true
"#;

        let line = "hello";
        let expect = ["<source.test>, <test.matched>"];
        expect_scope_stacks(line, &expect, syntax);
    }

    #[test]
    fn can_parse_non_consuming_pop_that_would_loop_at_end_of_line() {
        let syntax = r#"
name: test
scope: source.test
contexts:
  main:
    # This makes us go into "test" without consuming, even at the end of line
    - match: ""
      push: test
  test:
    - match: ""
      pop: true
    - match: \w+
      scope: test.matched
"#;

        let line = "hello";
        let expect = ["<source.test>, <test.matched>"];
        expect_scope_stacks(line, &expect, syntax);
    }

    #[test]
    fn can_parse_empty_but_consuming_set_that_does_not_loop() {
        let syntax = r#"
name: test
scope: source.test
contexts:
  main:
    - match: (?=hello)
      push: a
    - match: ello
      scope: test.good
  a:
    # This is an empty match, but it consumed a character (the "h")
    - match: (?=e)
      set: b
  b:
    # .. so it's ok to pop back to main from here
    - match: ""
      pop: true
    - match: ello
      scope: test.bad
"#;

        let line = "hello";
        let expect = ["<source.test>, <test.good>"];
        expect_scope_stacks(line, &expect, syntax);
    }

    #[test]
    fn can_parse_non_consuming_pop_that_does_not_loop() {
        let syntax = r#"
name: test
scope: source.test
contexts:
  main:
    # This is a non-consuming push, so "b" will need to check for a
    # non-consuming pop
    - match: (?=hello)
      push: [b, a]
    - match: ello
      scope: test.good
  a:
    # This pop is ok, it consumed "h"
    - match: (?=e)
      pop: true
  b:
    # This is non-consuming, and we set to "c"
    - match: (?=e)
      set: c
  c:
    # It's ok to pop back to "main" here because we consumed a character in the
    # meantime.
    - match: ""
      pop: true
    - match: ello
      scope: test.bad
"#;

        let line = "hello";
        let expect = ["<source.test>, <test.good>"];
        expect_scope_stacks(line, &expect, syntax);
    }

    #[test]
    fn can_parse_non_consuming_pop_with_multi_push_that_does_not_loop() {
        let syntax = r#"
name: test
scope: source.test
contexts:
  main:
    - match: (?=hello)
      push: [b, a]
    - match: ello
      scope: test.good
  a:
    # This pop is ok, as we're not popping back to "main" yet (which would loop),
    # we're popping to "b"
    - match: ""
      pop: true
    - match: \w+
      scope: test.bad
  b:
    - match: \w+
      scope: test.good
"#;

        let line = "hello";
        let expect = ["<source.test>, <test.good>"];
        expect_scope_stacks(line, &expect, syntax);
    }

    #[test]
    fn can_parse_non_consuming_pop_of_recursive_context_that_does_not_loop() {
        let syntax = r#"
name: test
scope: source.test
contexts:
  main:
    - match: xxx
      scope: test.good
    - include: basic-identifiers

  basic-identifiers:
    - match: '\w+::'
      scope: test.matched
      push: no-type-names

  no-type-names:
      - include: basic-identifiers
      - match: \w+
        scope: test.matched.inside
      # This is a tricky one because when this is the best match,
      # we have two instances of "no-type-names" on the stack, so we're popping
      # back from "no-type-names" to another "no-type-names".
      - match: ''
        pop: true
"#;

        let line = "foo::bar::* xxx";
        let expect = ["<source.test>, <test.good>"];
        expect_scope_stacks(line, &expect, syntax);
    }

    #[test]
    fn can_parse_non_consuming_pop_order() {
        let syntax = r#"
name: test
scope: source.test
contexts:
  main:
    - match: (?=hello)
      push: test
  test:
    # This matches first
    - match: (?=e)
      push: good
    # But this (looping) match replaces it, because it's an earlier match
    - match: (?=h)
      pop: true
    # And this should not replace it, as it's a later match (only matches at
    # the same position can replace looping pops).
    - match: (?=o)
      push: bad
  good:
    - match: \w+
      scope: test.good
  bad:
    - match: \w+
      scope: test.bad
"#;

        let line = "hello";
        let expect = ["<source.test>, <test.good>"];
        expect_scope_stacks(line, &expect, syntax);
    }

    #[test]
    fn can_parse_prototype_with_embed() {
        let syntax = r#"
name: Javadoc
scope: text.html.javadoc
contexts:
  prototype:
    - match: \*
      scope: punctuation.definition.comment.javadoc

  main:
    - meta_include_prototype: false
    - match: /\*\*
      scope: comment.block.documentation.javadoc punctuation.definition.comment.begin.javadoc
      embed: contents
      embed_scope: comment.block.documentation.javadoc text.html.javadoc
      escape: \*/
      escape_captures:
        0: comment.block.documentation.javadoc punctuation.definition.comment.end.javadoc

  contents:
    - match: ''
"#;

        let syntax = SyntaxDefinition::load_from_str(syntax, true, None).unwrap();
        expect_scope_stacks_with_syntax("/** * */", &["<comment.block.documentation.javadoc>, <punctuation.definition.comment.begin.javadoc>", "<comment.block.documentation.javadoc>, <text.html.javadoc>, <punctuation.definition.comment.javadoc>", "<comment.block.documentation.javadoc>, <punctuation.definition.comment.end.javadoc>"], syntax);
    }

    #[test]
    fn can_parse_context_included_in_prototype_via_named_reference() {
        let syntax = r#"
scope: source.test
contexts:
  prototype:
    - match: a
      push: a
    - match: b
      scope: test.bad
  main:
    - match: unused
  # This context is included in the prototype (see `push: a`).
  # Because of that, ST doesn't apply the prototype to this context, so if
  # we're in here the "b" shouldn't match.
  a:
    - match: a
      scope: test.good
"#;

        let stack_states = stack_states(parse("aa b", syntax));
        assert_eq!(
            stack_states,
            vec![
                "<source.test>",
                "<source.test>, <test.good>",
                "<source.test>",
            ],
            "Expected test.bad to not match"
        );
    }

    #[test]
    fn can_parse_with_prototype_set() {
        let syntax = r#"%YAML 1.2
---
scope: source.test-set-with-proto
contexts:
  main:
    - match: a
      scope: a
      set: next1
      with_prototype:
        - match: '1'
          scope: '1'
        - match: '2'
          scope: '2'
        - match: '3'
          scope: '3'
        - match: '4'
          scope: '4'
    - match: '5'
      scope: '5'
      set: [next3, next2]
      with_prototype:
        - match: c
          scope: cwith
  next1:
    - match: b
      scope: b
      set: next2
  next2:
    - match: c
      scope: c
      push: next3
    - match: e
      scope: e
      pop: true
    - match: f
      scope: f
      set: [next1, next2]
  next3:
    - match: d
      scope: d
    - match: (?=e)
      pop: true
    - match: c
      scope: cwithout
"#;

        expect_scope_stacks_with_syntax(
            "a1b2c3d4e5",
            &[
                "<a>", "<1>", "<b>", "<2>", "<c>", "<3>", "<d>", "<4>", "<e>", "<5>",
            ],
            SyntaxDefinition::load_from_str(syntax, true, None).unwrap(),
        );
        expect_scope_stacks_with_syntax(
            "5cfcecbedcdea",
            &[
                "<5>",
                "<cwith>",
                "<f>",
                "<e>",
                "<b>",
                "<d>",
                "<cwithout>",
                "<a>",
            ],
            SyntaxDefinition::load_from_str(syntax, true, None).unwrap(),
        );
    }

    #[test]
    fn can_parse_issue176() {
        let syntax = r#"
scope: source.dummy
contexts:
  main:
    - match: (test)(?=(foo))(f)
      captures:
        1: test
        2: ignored
        3: f
      push:
        - match: (oo)
          captures:
            1: keyword
"#;

        let syntax = SyntaxDefinition::load_from_str(syntax, true, None).unwrap();
        expect_scope_stacks_with_syntax(
            "testfoo",
            &["<test>", /*"<ignored>",*/ "<f>", "<keyword>"],
            syntax,
        );
    }

    #[test]
    fn can_parse_two_with_prototypes_at_same_stack_level() {
        let syntax_yamlstr = r#"
%YAML 1.2
---
# See http://www.sublimetext.com/docs/3/syntax.html
scope: source.example-wp
contexts:
  main:
    - match: a
      scope: a
      push:
        - match: b
          scope: b
          set:
            - match: c
              scope: c
          with_prototype:
            - match: '2'
              scope: '2'
      with_prototype:
        - match: '1'
          scope: '1'
"#;

        let syntax = SyntaxDefinition::load_from_str(syntax_yamlstr, true, None).unwrap();
        expect_scope_stacks_with_syntax("abc12", &["<1>", "<2>"], syntax);
    }

    #[test]
    fn can_parse_two_with_prototypes_at_same_stack_level_set_multiple() {
        let syntax_yamlstr = r#"
%YAML 1.2
---
# See http://www.sublimetext.com/docs/3/syntax.html
scope: source.example-wp
contexts:
  main:
    - match: a
      scope: a
      push:
        - match: b
          scope: b
          set: [context1, context2, context3]
          with_prototype:
            - match: '2'
              scope: '2'
      with_prototype:
        - match: '1'
          scope: '1'
    - match: '1'
      scope: digit1
    - match: '2'
      scope: digit2
  context1:
    - match: e
      scope: e
      pop: true
    - match: '2'
      scope: digit2
  context2:
    - match: d
      scope: d
      pop: true
    - match: '2'
      scope: digit2
  context3:
    - match: c
      scope: c
      pop: true
"#;

        let syntax = SyntaxDefinition::load_from_str(syntax_yamlstr, true, None).unwrap();
        expect_scope_stacks_with_syntax("ab12", &["<1>", "<2>"], syntax.clone());
        expect_scope_stacks_with_syntax("abc12", &["<1>", "<digit2>"], syntax.clone());
        expect_scope_stacks_with_syntax("abcd12", &["<1>", "<digit2>"], syntax.clone());
        expect_scope_stacks_with_syntax("abcde12", &["<digit1>", "<digit2>"], syntax);
    }

    #[test]
    fn can_parse_two_with_prototypes_at_same_stack_level_updated_captures() {
        let syntax_yamlstr = r#"
%YAML 1.2
---
# See http://www.sublimetext.com/docs/3/syntax.html
scope: source.example-wp
contexts:
  main:
    - match: (a)
      scope: a
      push:
        - match: (b)
          scope: b
          set:
            - match: c
              scope: c
          with_prototype:
            - match: d
              scope: d
      with_prototype:
        - match: \1
          scope: '1'
          pop: true
"#;

        let syntax = SyntaxDefinition::load_from_str(syntax_yamlstr, true, None).unwrap();
        expect_scope_stacks_with_syntax("aa", &["<a>", "<1>"], syntax.clone());
        expect_scope_stacks_with_syntax("abcdb", &["<a>", "<b>", "<c>", "<d>", "<1>"], syntax);
    }

    #[test]
    fn can_parse_two_with_prototypes_at_same_stack_level_updated_captures_ignore_unexisting() {
        let syntax_yamlstr = r#"
%YAML 1.2
---
# See http://www.sublimetext.com/docs/3/syntax.html
scope: source.example-wp
contexts:
  main:
    - match: (a)(-)
      scope: a
      push:
        - match: (b)
          scope: b
          set:
            - match: c
              scope: c
          with_prototype:
            - match: d
              scope: d
      with_prototype:
        - match: \2
          scope: '2'
          pop: true
        - match: \1
          scope: '1'
          pop: true
"#;

        let syntax = SyntaxDefinition::load_from_str(syntax_yamlstr, true, None).unwrap();
        expect_scope_stacks_with_syntax("a--", &["<a>", "<2>"], syntax.clone());
        // it seems that when ST encounters a non existing pop backreference, it just pops back to the with_prototype's original parent context - i.e. cdb is unscoped
        // TODO: it would be useful to have syntest functionality available here for easier testing and clarity
        expect_scope_stacks_with_syntax("a-bcdba-", &["<a>", "<b>"], syntax);
    }

    #[test]
    fn can_parse_syntax_with_eol_and_newline() {
        let syntax = r#"
name: test
scope: source.test
contexts:
  main:
    - match: foo$\n
      scope: foo.newline
"#;

        let line = "foo";
        let expect = ["<source.test>, <foo.newline>"];
        expect_scope_stacks(line, &expect, syntax);
    }

    #[test]
    fn can_parse_syntax_with_eol_only() {
        let syntax = r#"
name: test
scope: source.test
contexts:
  main:
    - match: foo$
      scope: foo.newline
"#;

        let line = "foo";
        let expect = ["<source.test>, <foo.newline>"];
        expect_scope_stacks(line, &expect, syntax);
    }

    #[test]
    fn can_parse_syntax_with_beginning_of_line() {
        let syntax = r#"
name: test
scope: source.test
contexts:
  main:
    - match: \w+
      scope: word
      push:
        # this should not match at the end of the line
        - match: ^\s*$
          pop: true
        - match: =+
          scope: heading
          pop: true
    - match: .*
      scope: other
"#;

        let syntax_newlines = SyntaxDefinition::load_from_str(syntax, true, None).unwrap();
        let syntax_set = link(syntax_newlines);

        let mut state = ParseState::new(&syntax_set.syntaxes()[0]);
        assert_eq!(
            ops(&mut state, "foo\n", &syntax_set),
            vec![
                (0, Push(Scope::new("source.test").unwrap())),
                (0, Push(Scope::new("word").unwrap())),
                (3, Pop(1))
            ]
        );
        assert_eq!(
            ops(&mut state, "===\n", &syntax_set),
            vec![(0, Push(Scope::new("heading").unwrap())), (3, Pop(1))]
        );

        assert_eq!(
            ops(&mut state, "bar\n", &syntax_set),
            vec![(0, Push(Scope::new("word").unwrap())), (3, Pop(1))]
        );
        // This should result in popping out of the context
        assert_eq!(ops(&mut state, "\n", &syntax_set), vec![]);
        // So now this matches other
        assert_eq!(
            ops(&mut state, "====\n", &syntax_set),
            vec![(0, Push(Scope::new("other").unwrap())), (4, Pop(1))]
        );
    }

    #[test]
    fn can_parse_syntax_with_comment_and_eol() {
        let syntax = r#"
name: test
scope: source.test
contexts:
  main:
    - match: (//).*$
      scope: comment.line.double-slash
"#;

        let syntax_newlines = SyntaxDefinition::load_from_str(syntax, true, None).unwrap();
        let syntax_set = link(syntax_newlines);

        let mut state = ParseState::new(&syntax_set.syntaxes()[0]);
        assert_eq!(
            ops(&mut state, "// foo\n", &syntax_set),
            vec![
                (0, Push(Scope::new("source.test").unwrap())),
                (0, Push(Scope::new("comment.line.double-slash").unwrap())),
                // 6 is important here, should not be 7. The pattern should *not* consume the newline,
                // but instead match before it. This is important for whitespace-sensitive syntaxes
                // where newlines terminate statements such as Scala.
                (6, Pop(1))
            ]
        );
    }

    #[test]
    fn can_parse_text_with_unicode_to_skip() {
        let syntax = r#"
name: test
scope: source.test
contexts:
  main:
    - match: (?=.)
      push: test
  test:
    - match: (?=.)
      pop: true
    - match: x
      scope: test.good
"#;

        // U+03C0 GREEK SMALL LETTER PI, 2 bytes in UTF-8
        expect_scope_stacks("\u{03C0}x", &["<source.test>, <test.good>"], syntax);
        // U+0800 SAMARITAN LETTER ALAF, 3 bytes in UTF-8
        expect_scope_stacks("\u{0800}x", &["<source.test>, <test.good>"], syntax);
        // U+1F600 GRINNING FACE, 4 bytes in UTF-8
        expect_scope_stacks("\u{1F600}x", &["<source.test>, <test.good>"], syntax);
    }

    #[test]
    fn can_include_backrefs() {
        let syntax = SyntaxDefinition::load_from_str(
            r#"
                name: Backref Include Test
                scope: source.backrefinc
                contexts:
                  main:
                    - match: (a)
                      scope: a
                      push: context1
                  context1:
                    - include: context2
                  context2:
                    - match: \1
                      scope: b
                      pop: true
                "#,
            true,
            None,
        )
        .unwrap();

        expect_scope_stacks_with_syntax("aa", &["<a>", "<b>"], syntax);
    }

    #[test]
    fn can_include_nested_backrefs() {
        let syntax = SyntaxDefinition::load_from_str(
            r#"
                name: Backref Include Test
                scope: source.backrefinc
                contexts:
                  main:
                    - match: (a)
                      scope: a
                      push: context1
                  context1:
                    - include: context3
                  context3:
                    - include: context2
                  context2:
                    - match: \1
                      scope: b
                      pop: true
                "#,
            true,
            None,
        )
        .unwrap();

        expect_scope_stacks_with_syntax("aa", &["<a>", "<b>"], syntax);
    }

    #[test]
    fn can_avoid_infinite_stack_depth() {
        let syntax = SyntaxDefinition::load_from_str(
            r#"
                name: Stack Depth Test
                scope: source.stack_depth
                contexts:
                  main:
                    - match: (a)
                      scope: a
                      push: context1

                    
                  context1:
                    - match: b
                      scope: b
                    - match: ''
                      push: context1
                    - match: ''
                      pop: 1
                    - match: c
                      scope: c
                "#,
            true,
            None,
        )
        .unwrap();

        let syntax_set = link(syntax);
        let mut state = ParseState::new(&syntax_set.syntaxes()[0]);
        expect_scope_stacks_for_ops(ops(&mut state, "a bc\n", &syntax_set), &["<a>"]);
        expect_scope_stacks_for_ops(ops(&mut state, "bc\n", &syntax_set), &["<b>"]);
    }

    fn expect_scope_stacks(line_without_newline: &str, expect: &[&str], syntax: &str) {
        println!("Parsing with newlines");
        let line_with_newline = format!("{}\n", line_without_newline);
        let syntax_newlines = SyntaxDefinition::load_from_str(syntax, true, None).unwrap();
        expect_scope_stacks_with_syntax(&line_with_newline, expect, syntax_newlines);

        println!("Parsing without newlines");
        let syntax_nonewlines = SyntaxDefinition::load_from_str(syntax, false, None).unwrap();
        expect_scope_stacks_with_syntax(line_without_newline, expect, syntax_nonewlines);
    }

    fn expect_scope_stacks_with_syntax(line: &str, expect: &[&str], syntax: SyntaxDefinition) {
        // check that each expected scope stack appears at least once while parsing the given test line

        let syntax_set = link(syntax);
        let mut state = ParseState::new(&syntax_set.syntaxes()[0]);
        let ops = ops(&mut state, line, &syntax_set);
        expect_scope_stacks_for_ops(ops, expect);
    }

    fn expect_scope_stacks_for_ops(ops: Vec<(usize, ScopeStackOp)>, expect: &[&str]) {
        let mut criteria_met = Vec::new();
        for stack_str in stack_states(ops) {
            println!("{}", stack_str);
            for expectation in expect.iter() {
                if stack_str.contains(expectation) {
                    criteria_met.push(expectation);
                }
            }
        }
        if let Some(missing) = expect.iter().find(|e| !criteria_met.contains(e)) {
            panic!("expected scope stack '{}' missing", missing);
        }
    }

    fn parse(line: &str, syntax: &str) -> Vec<(usize, ScopeStackOp)> {
        let syntax = SyntaxDefinition::load_from_str(syntax, true, None).unwrap();
        let syntax_set = link(syntax);

        let mut state = ParseState::new(&syntax_set.syntaxes()[0]);
        ops(&mut state, line, &syntax_set)
    }

    fn link(syntax: SyntaxDefinition) -> SyntaxSet {
        let mut builder = SyntaxSetBuilder::new();
        builder.add(syntax);
        builder.build()
    }

    fn ops(
        state: &mut ParseState,
        line: &str,
        syntax_set: &SyntaxSet,
    ) -> Vec<(usize, ScopeStackOp)> {
        let ops = state.parse_line(line, syntax_set).expect("#[cfg(test)]");
        debug_print_ops(line, &ops);
        ops
    }

    fn stack_states(ops: Vec<(usize, ScopeStackOp)>) -> Vec<String> {
        let mut states = Vec::new();
        let mut stack = ScopeStack::new();
        for (_, op) in ops.iter() {
            stack.apply(op).expect("#[cfg(test)]");
            let scopes: Vec<String> = stack
                .as_slice()
                .iter()
                .map(|s| format!("{:?}", s))
                .collect();
            let stack_str = scopes.join(", ");
            states.push(stack_str);
        }
        states
    }
}
