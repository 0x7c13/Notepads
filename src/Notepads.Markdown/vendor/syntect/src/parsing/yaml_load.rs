use super::regex::{Regex, Region};
use super::scope::*;
use super::syntax_definition::*;
use std::collections::HashMap;
use std::error::Error;
use std::ops::DerefMut;
use std::path::Path;
use yaml_rust::yaml::Hash;
use yaml_rust::{ScanError, Yaml, YamlLoader};

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ParseSyntaxError {
    /// Invalid YAML file syntax, or at least something yaml_rust can't handle
    #[error("Invalid YAML file syntax: {0}")]
    InvalidYaml(#[from] ScanError),
    /// The file must contain at least one YAML document
    #[error("The file must contain at least one YAML document")]
    EmptyFile,
    /// Some keys are required for something to be a valid `.sublime-syntax`
    #[error("Missing mandatory key in YAML file: {0}")]
    MissingMandatoryKey(&'static str),
    /// Invalid regex
    #[error("Error while compiling regex '{0}': {1}")]
    RegexCompileError(String, #[source] Box<dyn Error + Send + Sync + 'static>),
    /// A scope that syntect's scope implementation can't handle
    #[error("Invalid scope: {0}")]
    InvalidScope(ParseScopeError),
    /// A reference to another file that is invalid
    #[error("Invalid file reference")]
    BadFileRef,
    /// Syntaxes must have a context named "main"
    #[error("Context 'main' is missing")]
    MainMissing,
    /// Some part of the YAML file is the wrong type (e.g a string but should be a list)
    /// Sorry this doesn't give you any way to narrow down where this is.
    /// Maybe use Sublime Text to figure it out.
    #[error("Type mismatch")]
    TypeMismatch,
}

fn get_key<'a, R, F: FnOnce(&'a Yaml) -> Option<R>>(
    map: &'a Hash,
    key: &'static str,
    f: F,
) -> Result<R, ParseSyntaxError> {
    map.get(&Yaml::String(key.to_owned()))
        .ok_or(ParseSyntaxError::MissingMandatoryKey(key))
        .and_then(|x| f(x).ok_or(ParseSyntaxError::TypeMismatch))
}

fn str_to_scopes(s: &str, repo: &mut ScopeRepository) -> Result<Vec<Scope>, ParseSyntaxError> {
    s.split_whitespace()
        .map(|scope| repo.build(scope).map_err(ParseSyntaxError::InvalidScope))
        .collect()
}

struct ParserState<'a> {
    scope_repo: &'a mut ScopeRepository,
    variables: HashMap<String, String>,
    variable_regex: Regex,
    backref_regex: Regex,
    lines_include_newline: bool,
}

// `__start` must not include prototypes from the actual syntax definition,
// otherwise it's possible that a prototype makes us pop out of `__start`.
static START_CONTEXT: &str = "
__start:
    - meta_include_prototype: false
    - match: ''
      push: __main
__main:
    - include: main
";

impl SyntaxDefinition {
    /// In case you want to create your own SyntaxDefinition's in memory from strings.
    ///
    /// Generally you should use a [`SyntaxSet`].
    ///
    /// `fallback_name` is an optional name to use when the YAML doesn't provide a `name` key.
    ///
    /// [`SyntaxSet`]: ../struct.SyntaxSet.html
    pub fn load_from_str(
        s: &str,
        lines_include_newline: bool,
        fallback_name: Option<&str>,
    ) -> Result<SyntaxDefinition, ParseSyntaxError> {
        let docs = match YamlLoader::load_from_str(s) {
            Ok(x) => x,
            Err(e) => return Err(ParseSyntaxError::InvalidYaml(e)),
        };
        if docs.is_empty() {
            return Err(ParseSyntaxError::EmptyFile);
        }
        let doc = &docs[0];
        let mut scope_repo = lock_global_scope_repo();
        SyntaxDefinition::parse_top_level(
            doc,
            scope_repo.deref_mut(),
            lines_include_newline,
            fallback_name,
        )
    }

    fn parse_top_level(
        doc: &Yaml,
        scope_repo: &mut ScopeRepository,
        lines_include_newline: bool,
        fallback_name: Option<&str>,
    ) -> Result<SyntaxDefinition, ParseSyntaxError> {
        let h = doc.as_hash().ok_or(ParseSyntaxError::TypeMismatch)?;

        let mut variables = HashMap::new();
        if let Ok(map) = get_key(h, "variables", |x| x.as_hash()) {
            for (key, value) in map.iter() {
                if let (Some(key_str), Some(val_str)) = (key.as_str(), value.as_str()) {
                    variables.insert(key_str.to_owned(), val_str.to_owned());
                }
            }
        }
        let contexts_hash = get_key(h, "contexts", |x| x.as_hash())?;
        let top_level_scope = scope_repo
            .build(get_key(h, "scope", |x| x.as_str())?)
            .map_err(ParseSyntaxError::InvalidScope)?;
        let mut state = ParserState {
            scope_repo,
            variables,
            variable_regex: Regex::new(r"\{\{([A-Za-z0-9_]+)\}\}".into()),
            backref_regex: Regex::new(r"\\\d".into()),
            lines_include_newline,
        };

        let mut contexts = SyntaxDefinition::parse_contexts(contexts_hash, &mut state)?;
        if !contexts.contains_key("main") {
            return Err(ParseSyntaxError::MainMissing);
        }

        SyntaxDefinition::add_initial_contexts(&mut contexts, &mut state, top_level_scope);

        let mut file_extensions = Vec::new();
        for extension_key in &["file_extensions", "hidden_file_extensions"] {
            if let Ok(v) = get_key(h, extension_key, |x| x.as_vec()) {
                file_extensions.extend(v.iter().filter_map(|y| y.as_str().map(|s| s.to_owned())))
            }
        }

        let defn = SyntaxDefinition {
            name: get_key(h, "name", |x| x.as_str())
                .unwrap_or_else(|_| fallback_name.unwrap_or("Unnamed"))
                .to_owned(),
            scope: top_level_scope,
            file_extensions,
            // TODO maybe cache a compiled version of this Regex
            first_line_match: get_key(h, "first_line_match", |x| x.as_str())
                .ok()
                .map(|s| s.to_owned()),
            hidden: get_key(h, "hidden", |x| x.as_bool()).unwrap_or(false),

            variables: state.variables,
            contexts,
        };
        Ok(defn)
    }

    fn parse_contexts(
        map: &Hash,
        state: &mut ParserState<'_>,
    ) -> Result<HashMap<String, Context>, ParseSyntaxError> {
        let mut contexts = HashMap::new();
        for (key, value) in map.iter() {
            if let (Some(name), Some(val_vec)) = (key.as_str(), value.as_vec()) {
                let is_prototype = name == "prototype";
                let mut namer = ContextNamer::new(name);
                SyntaxDefinition::parse_context(
                    val_vec,
                    state,
                    &mut contexts,
                    is_prototype,
                    &mut namer,
                )?;
            }
        }

        Ok(contexts)
    }

    fn parse_context(
        vec: &[Yaml],
        // TODO: Maybe just pass the scope repo if that's all that's needed?
        state: &mut ParserState<'_>,
        contexts: &mut HashMap<String, Context>,
        is_prototype: bool,
        namer: &mut ContextNamer,
    ) -> Result<String, ParseSyntaxError> {
        let mut context = Context::new(!is_prototype);
        let name = namer.next();

        for y in vec.iter() {
            let map = y.as_hash().ok_or(ParseSyntaxError::TypeMismatch)?;

            let mut is_special = false;
            if let Ok(x) = get_key(map, "meta_scope", |x| x.as_str()) {
                context.meta_scope = str_to_scopes(x, state.scope_repo)?;
                is_special = true;
            }
            if let Ok(x) = get_key(map, "meta_content_scope", |x| x.as_str()) {
                context.meta_content_scope = str_to_scopes(x, state.scope_repo)?;
                is_special = true;
            }
            if let Ok(x) = get_key(map, "meta_include_prototype", |x| x.as_bool()) {
                context.meta_include_prototype = x;
                is_special = true;
            }
            if let Ok(true) = get_key(map, "clear_scopes", |x| x.as_bool()) {
                context.clear_scopes = Some(ClearAmount::All);
                is_special = true;
            }
            if let Ok(x) = get_key(map, "clear_scopes", |x| x.as_i64()) {
                context.clear_scopes = Some(ClearAmount::TopN(x as usize));
                is_special = true;
            }
            if !is_special {
                if let Ok(x) = get_key(map, "include", Some) {
                    let reference =
                        SyntaxDefinition::parse_reference(x, state, contexts, namer, false)?;
                    context.patterns.push(Pattern::Include(reference));
                } else {
                    let pattern =
                        SyntaxDefinition::parse_match_pattern(map, state, contexts, namer)?;
                    if pattern.has_captures {
                        context.uses_backrefs = true;
                    }
                    context.patterns.push(Pattern::Match(pattern));
                }
            }
        }

        contexts.insert(name.clone(), context);
        Ok(name)
    }

    fn parse_reference(
        y: &Yaml,
        state: &mut ParserState<'_>,
        contexts: &mut HashMap<String, Context>,
        namer: &mut ContextNamer,
        with_escape: bool,
    ) -> Result<ContextReference, ParseSyntaxError> {
        if let Some(s) = y.as_str() {
            let parts: Vec<&str> = s.split('#').collect();
            let sub_context = if parts.len() > 1 {
                Some(parts[1].to_owned())
            } else {
                None
            };
            if parts[0].starts_with("scope:") {
                Ok(ContextReference::ByScope {
                    scope: state
                        .scope_repo
                        .build(&parts[0][6..])
                        .map_err(ParseSyntaxError::InvalidScope)?,
                    sub_context,
                    with_escape,
                })
            } else if parts[0].ends_with(".sublime-syntax") {
                let stem = Path::new(parts[0])
                    .file_stem()
                    .and_then(|x| x.to_str())
                    .ok_or(ParseSyntaxError::BadFileRef)?;
                Ok(ContextReference::File {
                    name: stem.to_owned(),
                    sub_context,
                    with_escape,
                })
            } else {
                Ok(ContextReference::Named(parts[0].to_owned()))
            }
        } else if let Some(v) = y.as_vec() {
            let subname = SyntaxDefinition::parse_context(v, state, contexts, false, namer)?;
            Ok(ContextReference::Inline(subname))
        } else {
            Err(ParseSyntaxError::TypeMismatch)
        }
    }

    fn parse_match_pattern(
        map: &Hash,
        state: &mut ParserState<'_>,
        contexts: &mut HashMap<String, Context>,
        namer: &mut ContextNamer,
    ) -> Result<MatchPattern, ParseSyntaxError> {
        let raw_regex = get_key(map, "match", |x| x.as_str())?;
        let regex_str = Self::parse_regex(raw_regex, state)?;
        // println!("{:?}", regex_str);

        let scope = get_key(map, "scope", |x| x.as_str())
            .ok()
            .map(|s| str_to_scopes(s, state.scope_repo))
            .unwrap_or_else(|| Ok(vec![]))?;

        let captures = if let Ok(map) = get_key(map, "captures", |x| x.as_hash()) {
            Some(Self::parse_captures(map, &regex_str, state)?)
        } else {
            None
        };

        let mut has_captures = false;
        let operation = if get_key(map, "pop", Some).is_ok() {
            // Thanks @wbond for letting me know this is the correct way to check for captures
            has_captures = state
                .backref_regex
                .search(&regex_str, 0, regex_str.len(), None);
            MatchOperation::Pop
        } else if let Ok(y) = get_key(map, "push", Some) {
            MatchOperation::Push(SyntaxDefinition::parse_pushargs(y, state, contexts, namer)?)
        } else if let Ok(y) = get_key(map, "set", Some) {
            MatchOperation::Set(SyntaxDefinition::parse_pushargs(y, state, contexts, namer)?)
        } else if let Ok(y) = get_key(map, "embed", Some) {
            // Same as push so we translate it to what it would be
            let mut embed_escape_context_yaml = vec![];
            let mut commands = Hash::new();
            commands.insert(
                Yaml::String("meta_include_prototype".to_string()),
                Yaml::Boolean(false),
            );
            embed_escape_context_yaml.push(Yaml::Hash(commands));
            if let Ok(s) = get_key(map, "embed_scope", Some) {
                commands = Hash::new();
                commands.insert(Yaml::String("meta_content_scope".to_string()), s.clone());
                embed_escape_context_yaml.push(Yaml::Hash(commands));
            }
            if let Ok(v) = get_key(map, "escape", Some) {
                let mut match_map = Hash::new();
                match_map.insert(Yaml::String("match".to_string()), v.clone());
                match_map.insert(Yaml::String("pop".to_string()), Yaml::Boolean(true));
                if let Ok(y) = get_key(map, "escape_captures", Some) {
                    match_map.insert(Yaml::String("captures".to_string()), y.clone());
                }
                embed_escape_context_yaml.push(Yaml::Hash(match_map));
                let escape_context = SyntaxDefinition::parse_context(
                    &embed_escape_context_yaml,
                    state,
                    contexts,
                    false,
                    namer,
                )?;
                MatchOperation::Push(vec![
                    ContextReference::Inline(escape_context),
                    SyntaxDefinition::parse_reference(y, state, contexts, namer, true)?,
                ])
            } else {
                return Err(ParseSyntaxError::MissingMandatoryKey("escape"));
            }
        } else {
            MatchOperation::None
        };

        let with_prototype = if let Ok(v) = get_key(map, "with_prototype", |x| x.as_vec()) {
            // should a with_prototype include the prototype? I don't think so.
            let subname = Self::parse_context(v, state, contexts, true, namer)?;
            Some(ContextReference::Inline(subname))
        } else if let Ok(v) = get_key(map, "escape", Some) {
            let subname = namer.next();

            let mut context = Context::new(false);
            let mut match_map = Hash::new();
            match_map.insert(
                Yaml::String("match".to_string()),
                Yaml::String(format!("(?={})", v.as_str().unwrap())),
            );
            match_map.insert(Yaml::String("pop".to_string()), Yaml::Boolean(true));
            let pattern =
                SyntaxDefinition::parse_match_pattern(&match_map, state, contexts, namer)?;
            if pattern.has_captures {
                context.uses_backrefs = true;
            }
            context.patterns.push(Pattern::Match(pattern));

            contexts.insert(subname.clone(), context);
            Some(ContextReference::Inline(subname))
        } else {
            None
        };

        let pattern = MatchPattern::new(
            has_captures,
            regex_str,
            scope,
            captures,
            operation,
            with_prototype,
        );

        Ok(pattern)
    }

    fn parse_pushargs(
        y: &Yaml,
        state: &mut ParserState<'_>,
        contexts: &mut HashMap<String, Context>,
        namer: &mut ContextNamer,
    ) -> Result<Vec<ContextReference>, ParseSyntaxError> {
        // check for a push of multiple items
        if y.as_vec().is_some_and(|v| {
            !v.is_empty()
                && (v[0].as_str().is_some()
                    || (v[0].as_vec().is_some() && v[0].as_vec().unwrap()[0].as_hash().is_some()))
        }) {
            // this works because Result implements FromIterator to handle the errors
            y.as_vec()
                .unwrap()
                .iter()
                .map(|x| SyntaxDefinition::parse_reference(x, state, contexts, namer, false))
                .collect()
        } else {
            let reference = SyntaxDefinition::parse_reference(y, state, contexts, namer, false)?;
            Ok(vec![reference])
        }
    }

    fn parse_regex(raw_regex: &str, state: &ParserState<'_>) -> Result<String, ParseSyntaxError> {
        let regex = Self::resolve_variables(raw_regex, state);
        let regex = replace_posix_char_classes(regex);
        let regex = if state.lines_include_newline {
            regex_for_newlines(regex)
        } else {
            // If the passed in strings don't include newlines (unlike Sublime) we can't match on
            // them using the original regex. So this tries to rewrite the regex in a way that
            // allows matching against lines without newlines (essentially replacing `\n` with `$`).
            regex_for_no_newlines(regex)
        };
        Self::try_compile_regex(&regex)?;
        Ok(regex)
    }

    fn resolve_variables(raw_regex: &str, state: &ParserState<'_>) -> String {
        let mut result = String::new();
        let mut index = 0;
        let mut region = Region::new();
        while state
            .variable_regex
            .search(raw_regex, index, raw_regex.len(), Some(&mut region))
        {
            let (begin, end) = region.pos(0).unwrap();

            result.push_str(&raw_regex[index..begin]);

            let var_pos = region.pos(1).unwrap();
            let var_name = &raw_regex[var_pos.0..var_pos.1];
            let var_raw = state
                .variables
                .get(var_name)
                .map(String::as_ref)
                .unwrap_or("");
            let var_resolved = Self::resolve_variables(var_raw, state);
            result.push_str(&var_resolved);

            index = end;
        }
        if index < raw_regex.len() {
            result.push_str(&raw_regex[index..]);
        }
        result
    }

    fn try_compile_regex(regex_str: &str) -> Result<(), ParseSyntaxError> {
        // Replace backreferences with a placeholder value that will also appear in errors
        let regex_str =
            substitute_backrefs_in_regex(regex_str, |i| Some(format!("<placeholder_{}>", i)));

        if let Some(error) = Regex::try_compile(&regex_str) {
            Err(ParseSyntaxError::RegexCompileError(regex_str, error))
        } else {
            Ok(())
        }
    }

    fn parse_captures(
        map: &Hash,
        regex_str: &str,
        state: &mut ParserState<'_>,
    ) -> Result<CaptureMapping, ParseSyntaxError> {
        let valid_indexes = get_consuming_capture_indexes(regex_str);
        let mut captures = Vec::new();
        for (key, value) in map.iter() {
            if let (Some(key_int), Some(val_str)) = (key.as_i64(), value.as_str()) {
                if valid_indexes.contains(&(key_int as usize)) {
                    captures.push((key_int as usize, str_to_scopes(val_str, state.scope_repo)?));
                }
            }
        }
        Ok(captures)
    }

    /// Sublime treats the top level context slightly differently from
    /// including the main context from other syntaxes. When main is popped
    /// it is immediately re-added and when it is `set` over the file level
    /// scope remains. This behaviour is emulated through some added contexts
    /// that are the actual top level contexts used in parsing.
    /// See <https://github.com/trishume/syntect/issues/58> for more.
    fn add_initial_contexts(
        contexts: &mut HashMap<String, Context>,
        state: &mut ParserState<'_>,
        top_level_scope: Scope,
    ) {
        let yaml_docs = YamlLoader::load_from_str(START_CONTEXT).unwrap();
        let yaml = &yaml_docs[0];

        let start_yaml: &[Yaml] = yaml["__start"].as_vec().unwrap();
        SyntaxDefinition::parse_context(
            start_yaml,
            state,
            contexts,
            false,
            &mut ContextNamer::new("__start"),
        )
        .unwrap();
        if let Some(start) = contexts.get_mut("__start") {
            start.meta_content_scope = vec![top_level_scope];
        }

        let main_yaml: &[Yaml] = yaml["__main"].as_vec().unwrap();
        SyntaxDefinition::parse_context(
            main_yaml,
            state,
            contexts,
            false,
            &mut ContextNamer::new("__main"),
        )
        .unwrap();

        let meta_include_prototype = contexts["main"].meta_include_prototype;
        let meta_scope = contexts["main"].meta_scope.clone();
        let meta_content_scope = contexts["main"].meta_content_scope.clone();

        if let Some(outer_main) = contexts.get_mut("__main") {
            outer_main.meta_include_prototype = meta_include_prototype;
            outer_main.meta_scope = meta_scope;
            outer_main.meta_content_scope = meta_content_scope;
        }

        // add the top_level_scope as a meta_content_scope to main so
        // pushes from other syntaxes add the file scope
        // TODO: this order is not quite correct if main also has a meta_scope
        if let Some(main) = contexts.get_mut("main") {
            main.meta_content_scope.insert(0, top_level_scope);
        }
    }
}

struct ContextNamer {
    name: String,
    anonymous_index: Option<usize>,
}

impl ContextNamer {
    fn new(name: &str) -> ContextNamer {
        ContextNamer {
            name: name.to_string(),
            anonymous_index: None,
        }
    }

    fn next(&mut self) -> String {
        let name = if let Some(index) = self.anonymous_index {
            format!("#anon_{}_{}", self.name, index)
        } else {
            self.name.clone()
        };

        self.anonymous_index = Some(self.anonymous_index.map(|i| i + 1).unwrap_or(0));
        name
    }
}

/// In fancy-regex, POSIX character classes only match ASCII characters.
///
/// Sublime's syntaxes expect them to match Unicode characters as well, so transform them to
/// corresponding Unicode character classes.
fn replace_posix_char_classes(regex: String) -> String {
    regex
        .replace("[:alpha:]", r"\p{L}")
        .replace("[:alnum:]", r"\p{L}\p{N}")
        .replace("[:lower:]", r"\p{Ll}")
        .replace("[:upper:]", r"\p{Lu}")
        .replace("[:digit:]", r"\p{Nd}")
}

/// Some of the regexes include `$` and expect it to match end of line,
/// e.g. *before* the `\n` in `test\n`.
///
/// In fancy-regex, `$` means end of text by default, so that would
/// match *after* `\n`. Using `(?m:$)` instead means it matches end of line.
///
/// Note that we don't want to add a `(?m)` in the beginning to change the
/// whole regex because that would also change the meaning of `^`. In
/// fancy-regex, that also matches at the end of e.g. `test\n` which is
/// different from onig. It would also change `.` to match more.
fn regex_for_newlines(regex: String) -> String {
    if !regex.contains('$') {
        return regex;
    }

    let rewriter = RegexRewriterForNewlines {
        parser: Parser::new(regex.as_bytes()),
    };
    rewriter.rewrite()
}

struct RegexRewriterForNewlines<'a> {
    parser: Parser<'a>,
}

impl RegexRewriterForNewlines<'_> {
    fn rewrite(mut self) -> String {
        let mut result = Vec::new();

        while let Some(c) = self.parser.peek() {
            match c {
                b'$' => {
                    self.parser.next();
                    result.extend_from_slice(br"(?m:$)");
                }
                b'\\' => {
                    self.parser.next();
                    result.push(c);
                    if let Some(c2) = self.parser.peek() {
                        self.parser.next();
                        result.push(c2);
                    }
                }
                b'[' => {
                    let (mut content, _) = self.parser.parse_character_class();
                    result.append(&mut content);
                }
                _ => {
                    self.parser.next();
                    result.push(c);
                }
            }
        }
        String::from_utf8(result).unwrap()
    }
}

/// Rewrite a regex that matches `\n` to one that matches `$` (end of line) instead.
/// That allows the regex to be used to match lines that don't include a trailing newline character.
///
/// The reason we're doing this is because the regexes in the syntax definitions assume that the
/// lines that are being matched on include a trailing newline.
///
/// Note that the rewrite is just an approximation and there's a couple of cases it can not handle,
/// due to `$` being an anchor whereas `\n` matches a character.
fn regex_for_no_newlines(regex: String) -> String {
    if !regex.contains(r"\n") {
        return regex;
    }

    // A special fix to rewrite a pattern from the `Rd` syntax that the RegexRewriter can not
    // handle properly.
    let regex = regex.replace("(?:\\n)?", "(?:$|)");

    let rewriter = RegexRewriterForNoNewlines {
        parser: Parser::new(regex.as_bytes()),
    };
    rewriter.rewrite()
}

struct RegexRewriterForNoNewlines<'a> {
    parser: Parser<'a>,
}

impl RegexRewriterForNoNewlines<'_> {
    fn rewrite(mut self) -> String {
        let mut result = Vec::new();
        while let Some(c) = self.parser.peek() {
            match c {
                b'\\' => {
                    self.parser.next();
                    if let Some(c2) = self.parser.peek() {
                        self.parser.next();
                        // Replacing `\n` with `$` in `\n?` or `\n+` would make parsing later fail
                        // with "target of repeat operator is invalid"
                        let c3 = self.parser.peek();
                        if c2 == b'n' && c3 != Some(b'?') && c3 != Some(b'+') && c3 != Some(b'*') {
                            result.extend_from_slice(b"$");
                        } else {
                            result.push(c);
                            result.push(c2);
                        }
                    } else {
                        result.push(c);
                    }
                }
                b'[' => {
                    let (mut content, matches_newline) = self.parser.parse_character_class();
                    if matches_newline && self.parser.peek() != Some(b'?') {
                        result.extend_from_slice(b"(?:");
                        result.append(&mut content);
                        result.extend_from_slice(br"|$)");
                    } else {
                        result.append(&mut content);
                    }
                }
                _ => {
                    self.parser.next();
                    result.push(c);
                }
            }
        }
        String::from_utf8(result).unwrap()
    }
}

fn get_consuming_capture_indexes(regex: &str) -> Vec<usize> {
    let parser = ConsumingCaptureIndexParser {
        parser: Parser::new(regex.as_bytes()),
    };
    parser.get_consuming_capture_indexes()
}

struct ConsumingCaptureIndexParser<'a> {
    parser: Parser<'a>,
}

impl ConsumingCaptureIndexParser<'_> {
    /// Find capture groups which are not inside lookarounds.
    ///
    /// If, in a YAML syntax definition, a scope stack is applied to a capture group inside a
    /// lookaround, (i.e. "captures:\n x: scope.stack goes.here", where "x" is the number of a
    /// capture group in a lookahead/behind), those those scopes are not applied, so no need to
    /// even parse them.
    fn get_consuming_capture_indexes(mut self) -> Vec<usize> {
        let mut result = Vec::new();
        let mut stack = Vec::new();
        let mut cap_num = 0;
        let mut in_lookaround = false;
        stack.push(in_lookaround);
        result.push(cap_num);

        while let Some(c) = self.parser.peek() {
            match c {
                b'\\' => {
                    self.parser.next();
                    self.parser.next();
                }
                b'[' => {
                    self.parser.parse_character_class();
                }
                b'(' => {
                    self.parser.next();
                    // add the current lookaround state to the stack so we can just pop at a closing paren
                    stack.push(in_lookaround);
                    if let Some(c2) = self.parser.peek() {
                        if c2 != b'?' {
                            // simple numbered capture group
                            cap_num += 1;
                            // if we are not currently in a lookaround,
                            // add this capture group number to the valid ones
                            if !in_lookaround {
                                result.push(cap_num);
                            }
                        } else {
                            self.parser.next();
                            if let Some(c3) = self.parser.peek() {
                                self.parser.next();
                                if c3 == b'=' || c3 == b'!' {
                                    // lookahead
                                    in_lookaround = true;
                                } else if c3 == b'<' {
                                    if let Some(c4) = self.parser.peek() {
                                        if c4 == b'=' || c4 == b'!' {
                                            self.parser.next();
                                            // lookbehind
                                            in_lookaround = true;
                                        }
                                    }
                                } else if c3 == b'P' {
                                    if let Some(c4) = self.parser.peek() {
                                        if c4 == b'<' {
                                            // named capture group
                                            cap_num += 1;
                                            // if we are not currently in a lookaround,
                                            // add this capture group number to the valid ones
                                            if !in_lookaround {
                                                result.push(cap_num);
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                b')' => {
                    if let Some(value) = stack.pop() {
                        in_lookaround = value;
                    }
                    self.parser.next();
                }
                _ => {
                    self.parser.next();
                }
            }
        }
        result
    }
}

struct Parser<'a> {
    bytes: &'a [u8],
    index: usize,
}

impl Parser<'_> {
    fn new(bytes: &[u8]) -> Parser {
        Parser { bytes, index: 0 }
    }

    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.index).copied()
    }

    fn next(&mut self) {
        self.index += 1;
    }

    fn parse_character_class(&mut self) -> (Vec<u8>, bool) {
        let mut content = Vec::new();
        let mut negated = false;
        let mut nesting = 0;
        let mut matches_newline = false;

        self.next();
        content.push(b'[');
        if let Some(b'^') = self.peek() {
            self.next();
            content.push(b'^');
            negated = true;
        }

        // An unescaped `]` is allowed after `[` or `[^` and doesn't mean the end of the class.
        if let Some(b']') = self.peek() {
            self.next();
            content.push(b']');
        }

        while let Some(c) = self.peek() {
            match c {
                b'\\' => {
                    self.next();
                    content.push(c);
                    if let Some(c2) = self.peek() {
                        self.next();
                        if c2 == b'n' && !negated && nesting == 0 {
                            matches_newline = true;
                        }
                        content.push(c2);
                    }
                }
                b'[' => {
                    self.next();
                    content.push(b'[');
                    nesting += 1;
                }
                b']' => {
                    self.next();
                    content.push(b']');
                    if nesting == 0 {
                        break;
                    }
                    nesting -= 1;
                }
                _ => {
                    self.next();
                    content.push(c);
                }
            }
        }

        (content, matches_newline)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parsing::Scope;

    #[test]
    fn can_parse() {
        let defn: SyntaxDefinition = SyntaxDefinition::load_from_str(
            "name: C\nscope: source.c\ncontexts: {main: []}",
            false,
            None,
        )
        .unwrap();
        assert_eq!(defn.name, "C");
        assert_eq!(defn.scope, Scope::new("source.c").unwrap());
        let exts_empty: Vec<String> = Vec::new();
        assert_eq!(defn.file_extensions, exts_empty);
        assert!(!defn.hidden);
        assert!(defn.variables.is_empty());
        let defn2: SyntaxDefinition = SyntaxDefinition::load_from_str(
            "
        name: C
        scope: source.c
        file_extensions: [c, h]
        hidden_file_extensions: [k, l]
        hidden: true
        variables:
          ident: '[QY]+'
        contexts:
          prototype:
            - match: lol
              scope: source.php
          main:
            - match: \\b(if|else|for|while|{{ident}})\\b
              scope: keyword.control.c keyword.looping.c
              captures:
                  1: meta.preprocessor.c++
                  2: keyword.control.include.c++
              push: [string, 'scope:source.c#main', 'CSS.sublime-syntax#rule-list-body']
              with_prototype:
                - match: wow
                  pop: true
            - match: '\"'
              push: string
          string:
            - meta_scope: string.quoted.double.c
            - meta_include_prototype: false
            - match: \\\\.
              scope: constant.character.escape.c
            - match: '\"'
              pop: true
        ",
            false,
            None,
        )
        .unwrap();
        assert_eq!(defn2.name, "C");
        let top_level_scope = Scope::new("source.c").unwrap();
        assert_eq!(defn2.scope, top_level_scope);
        let exts: Vec<String> = vec!["c", "h", "k", "l"]
            .into_iter()
            .map(String::from)
            .collect();
        assert_eq!(defn2.file_extensions, exts);
        assert!(defn2.hidden);
        assert_eq!(defn2.variables.get("ident").unwrap(), "[QY]+");

        let n: Vec<Scope> = Vec::new();
        println!("{:?}", defn2);
        // unreachable!();
        let main = &defn2.contexts["main"];
        assert_eq!(main.meta_content_scope, vec![top_level_scope]);
        assert_eq!(main.meta_scope, n);
        assert!(main.meta_include_prototype);

        assert_eq!(defn2.contexts["__main"].meta_content_scope, n);
        assert_eq!(
            defn2.contexts["__start"].meta_content_scope,
            vec![top_level_scope]
        );

        assert_eq!(
            defn2.contexts["string"].meta_scope,
            vec![Scope::new("string.quoted.double.c").unwrap()]
        );
        let first_pattern: &Pattern = &main.patterns[0];
        match *first_pattern {
            Pattern::Match(ref match_pat) => {
                let m: &CaptureMapping = match_pat.captures.as_ref().expect("test failed");
                assert_eq!(
                    &m[0],
                    &(1, vec![Scope::new("meta.preprocessor.c++").unwrap()])
                );
                use crate::parsing::syntax_definition::ContextReference::*;

                // this is sadly necessary because Context is not Eq because of the Regex
                let expected = MatchOperation::Push(vec![
                    Named("string".to_owned()),
                    ByScope {
                        scope: Scope::new("source.c").unwrap(),
                        sub_context: Some("main".to_owned()),
                        with_escape: false,
                    },
                    File {
                        name: "CSS".to_owned(),
                        sub_context: Some("rule-list-body".to_owned()),
                        with_escape: false,
                    },
                ]);
                assert_eq!(
                    format!("{:?}", match_pat.operation),
                    format!("{:?}", expected)
                );

                assert_eq!(
                    match_pat.scope,
                    vec![
                        Scope::new("keyword.control.c").unwrap(),
                        Scope::new("keyword.looping.c").unwrap()
                    ]
                );

                assert!(match_pat.with_prototype.is_some());
            }
            _ => unreachable!(),
        }
    }

    #[test]
    fn can_parse_embed_as_with_prototypes() {
        let old_def = SyntaxDefinition::load_from_str(r#"
        name: C
        scope: source.c
        file_extensions: [c, h]
        variables:
          ident: '[QY]+'
        contexts:
          main:
            - match: '(>)\s*'
              captures:
                1: meta.tag.style.begin.html punctuation.definition.tag.end.html
              push:
                - [{ meta_include_prototype: false }, { meta_content_scope: 'source.css.embedded.html' }, { match: '(?i)(?=</style)', pop: true }]
                - scope:source.css
              with_prototype:
                - match: (?=(?i)(?=</style))
                  pop: true
        "#,false, None).unwrap();

        let mut def_with_embed = SyntaxDefinition::load_from_str(
            r#"
        name: C
        scope: source.c
        file_extensions: [c, h]
        variables:
          ident: '[QY]+'
        contexts:
          main:
            - match: '(>)\s*'
              captures:
                1: meta.tag.style.begin.html punctuation.definition.tag.end.html
              embed: scope:source.css
              embed_scope: source.css.embedded.html
              escape: (?i)(?=</style)
        "#,
            false,
            None,
        )
        .unwrap();

        // We will soon do an `assert_eq!()`. But there is one difference we must expect, namely
        // that for `def_with_embed`, the value of `ContextReference::ByScope::with_escape` will be
        // `true`, whereas for `old_def` it will be `false`. So manually adjust `with_escape` to
        // `false` so that `assert_eq!()` will work.
        let def_with_embed_context = def_with_embed.contexts.get_mut("main").unwrap();
        if let Pattern::Match(ref mut match_pattern) = def_with_embed_context.patterns[0] {
            if let MatchOperation::Push(ref mut context_references) = match_pattern.operation {
                if let ContextReference::ByScope {
                    ref mut with_escape,
                    ..
                } = context_references[1]
                {
                    *with_escape = false;
                }
            }
        }

        assert_eq!(old_def.contexts["main"], def_with_embed.contexts["main"]);
    }

    #[test]
    fn errors_on_embed_without_escape() {
        let def = SyntaxDefinition::load_from_str(
            r#"
        name: C
        scope: source.c
        file_extensions: [c, h]
        variables:
          ident: '[QY]+'
        contexts:
          main:
            - match: '(>)\s*'
              captures:
                1: meta.tag.style.begin.html punctuation.definition.tag.end.html
              embed: scope:source.css
              embed_scope: source.css.embedded.html
        "#,
            false,
            None,
        );
        assert!(def.is_err());
        match def.unwrap_err() {
            ParseSyntaxError::MissingMandatoryKey(key) => assert_eq!(key, "escape"),
            _ => unreachable!("Got unexpected ParseSyntaxError"),
        }
    }

    #[test]
    fn errors_on_regex_compile_error() {
        let def = SyntaxDefinition::load_from_str(
            r#"
        name: C
        scope: source.c
        file_extensions: [test]
        contexts:
          main:
            - match: '[a'
              scope: keyword.name
        "#,
            false,
            None,
        );
        assert!(def.is_err());
        match def.unwrap_err() {
            ParseSyntaxError::RegexCompileError(ref regex, _) => assert_eq!("[a", regex),
            _ => unreachable!("Got unexpected ParseSyntaxError"),
        }
    }

    #[test]
    fn can_parse_ugly_yaml() {
        let defn: SyntaxDefinition = SyntaxDefinition::load_from_str(
            "
        name: LaTeX
        scope: text.tex.latex
        contexts:
          main:
            - match: '((\\\\)(?:framebox|makebox))\\b'
              captures:
                1: support.function.box.latex
                2: punctuation.definition.backslash.latex
              push:
                - [{meta_scope: meta.function.box.latex}, {match: '', pop: true}]
                - argument
                - optional-arguments
          argument:
            - match: '\\{'
              scope: punctuation.definition.group.brace.begin.latex
            - match: '(?=\\S)'
              pop: true
          optional-arguments:
            - match: '(?=\\S)'
              pop: true
        ",
            false,
            None,
        )
        .unwrap();
        assert_eq!(defn.name, "LaTeX");
        let top_level_scope = Scope::new("text.tex.latex").unwrap();
        assert_eq!(defn.scope, top_level_scope);

        let first_pattern: &Pattern = &defn.contexts["main"].patterns[0];
        match *first_pattern {
            Pattern::Match(ref match_pat) => {
                let m: &CaptureMapping = match_pat.captures.as_ref().expect("test failed");
                assert_eq!(
                    &m[0],
                    &(1, vec![Scope::new("support.function.box.latex").unwrap()])
                );

                //use parsing::syntax_definition::ContextReference::*;
                // TODO: check the first pushed reference is Inline(...) and has a meta_scope of meta.function.box.latex
                // TODO: check the second pushed reference is Named("argument".to_owned())
                // TODO: check the third pushed reference is Named("optional-arguments".to_owned())

                assert!(match_pat.with_prototype.is_none());
            }
            _ => unreachable!(),
        }
    }

    #[test]
    fn names_anonymous_contexts() {
        let def = SyntaxDefinition::load_from_str(
            r#"
            scope: source.c
            contexts:
              main:
                - match: a
                  push: a
              a:
                - meta_scope: a
                - match: x
                  push:
                    - meta_scope: anonymous_x
                    - match: anything
                      push:
                        - meta_scope: anonymous_x_2
                - match: y
                  push:
                    - meta_scope: anonymous_y
                - match: z
                  escape: 'test'
            "#,
            false,
            None,
        )
        .unwrap();

        assert_eq!(def.contexts["a"].meta_scope, vec![Scope::new("a").unwrap()]);
        assert_eq!(
            def.contexts["#anon_a_0"].meta_scope,
            vec![Scope::new("anonymous_x").unwrap()]
        );
        assert_eq!(
            def.contexts["#anon_a_1"].meta_scope,
            vec![Scope::new("anonymous_x_2").unwrap()]
        );
        assert_eq!(
            def.contexts["#anon_a_2"].meta_scope,
            vec![Scope::new("anonymous_y").unwrap()]
        );
        assert_eq!(def.contexts["#anon_a_3"].patterns.len(), 1); // escape
    }

    #[test]
    fn can_use_fallback_name() {
        let def = SyntaxDefinition::load_from_str(
            r#"
        scope: source.c
        contexts:
          main:
            - match: ''
        "#,
            false,
            Some("C"),
        );
        assert_eq!(def.unwrap().name, "C");
    }

    #[test]
    fn can_rewrite_regex_for_newlines() {
        fn rewrite(s: &str) -> String {
            regex_for_newlines(s.to_string())
        }

        assert_eq!(&rewrite(r"a"), r"a");
        assert_eq!(&rewrite(r"\b"), r"\b");
        assert_eq!(&rewrite(r"(a)"), r"(a)");
        assert_eq!(&rewrite(r"[a]"), r"[a]");
        assert_eq!(&rewrite(r"[^a]"), r"[^a]");
        assert_eq!(&rewrite(r"[]a]"), r"[]a]");
        assert_eq!(&rewrite(r"[[a]]"), r"[[a]]");

        assert_eq!(&rewrite(r"^"), r"^");
        assert_eq!(&rewrite(r"$"), r"(?m:$)");
        assert_eq!(&rewrite(r"^ab$"), r"^ab(?m:$)");
        assert_eq!(&rewrite(r"\^ab\$"), r"\^ab\$");
        assert_eq!(&rewrite(r"(//).*$"), r"(//).*(?m:$)");

        // Do not rewrite this `$` because it's in a char class and doesn't mean end of line
        assert_eq!(&rewrite(r"[a$]"), r"[a$]");
    }

    #[test]
    fn can_rewrite_regex_for_no_newlines() {
        fn rewrite(s: &str) -> String {
            regex_for_no_newlines(s.to_string())
        }

        assert_eq!(&rewrite(r"a"), r"a");
        assert_eq!(&rewrite(r"\b"), r"\b");
        assert_eq!(&rewrite(r"(a)"), r"(a)");
        assert_eq!(&rewrite(r"[a]"), r"[a]");
        assert_eq!(&rewrite(r"[^a]"), r"[^a]");
        assert_eq!(&rewrite(r"[]a]"), r"[]a]");
        assert_eq!(&rewrite(r"[[a]]"), r"[[a]]");

        assert_eq!(&rewrite(r"\n"), r"$");
        assert_eq!(&rewrite(r"\[\n"), r"\[$");
        assert_eq!(&rewrite(r"a\n?"), r"a\n?");
        assert_eq!(&rewrite(r"a\n+"), r"a\n+");
        assert_eq!(&rewrite(r"a\n*"), r"a\n*");
        assert_eq!(&rewrite(r"[abc\n]"), r"(?:[abc\n]|$)");
        assert_eq!(&rewrite(r"[^\n]"), r"[^\n]");
        assert_eq!(&rewrite(r"[^]\n]"), r"[^]\n]");
        assert_eq!(&rewrite(r"[\n]?"), r"[\n]?");
        // Removing the `\n` might result in an empty character class, so we should leave it.
        assert_eq!(&rewrite(r"[\n]"), r"(?:[\n]|$)");
        assert_eq!(&rewrite(r"[]\n]"), r"(?:[]\n]|$)");
        // In order to properly understand nesting, we'd have to have a full parser, so ignore it.
        assert_eq!(&rewrite(r"[[a]&&[\n]]"), r"[[a]&&[\n]]");

        assert_eq!(&rewrite(r"ab(?:\n)?"), r"ab(?:$|)");
        assert_eq!(&rewrite(r"(?<!\n)ab"), r"(?<!$)ab");
        assert_eq!(&rewrite(r"(?<=\n)ab"), r"(?<=$)ab");
    }

    #[test]
    fn can_get_valid_captures_from_regex() {
        let regex = "hello(test)(?=(world))(foo(?P<named>bar))";
        println!("{:?}", regex);
        let valid_indexes = get_consuming_capture_indexes(regex);
        println!("{:?}", valid_indexes);
        assert_eq!(valid_indexes, [0, 1, 3, 4]);
    }

    #[test]
    fn can_get_valid_captures_from_regex2() {
        let regex = "hello(test)[(?=tricked](foo(bar))";
        println!("{:?}", regex);
        let valid_indexes = get_consuming_capture_indexes(regex);
        println!("{:?}", valid_indexes);
        assert_eq!(valid_indexes, [0, 1, 2, 3]);
    }

    #[test]
    fn can_get_valid_captures_from_nested_regex() {
        let regex = "hello(test)(?=(world(?!(te(?<=(st))))))(foo(bar))";
        println!("{:?}", regex);
        let valid_indexes = get_consuming_capture_indexes(regex);
        println!("{:?}", valid_indexes);
        assert_eq!(valid_indexes, [0, 1, 5, 6]);
    }

    #[test]
    fn error_loading_syntax_with_unescaped_backslash() {
        let load_err = SyntaxDefinition::load_from_str(
            r#"
            name: Unescaped Backslash
            scope: source.c
            file_extensions: [test]
            contexts:
              main:
                - match: '\'
            "#,
            false,
            None,
        )
        .unwrap_err();
        match load_err {
            ParseSyntaxError::RegexCompileError(bad_regex, _) => assert_eq!(bad_regex, r"\"),
            _ => panic!("Unexpected error: {load_err}"),
        }
    }
}
