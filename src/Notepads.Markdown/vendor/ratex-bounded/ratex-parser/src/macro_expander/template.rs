// Notepads Markdown execution-control modifications: Copyright (c) 2026, Jiaqi (0x7c13) Liu.
//! Borrowed macro templates: count copies before creating output tokens.
use crate::ParseResult;
use crate::expansion_budget::{Cost, ExpansionBudget};
use ratex_lexer::token::SourceLocation;
use ratex_lexer::{Lexer, Token};
use ratex_types::execution::{ExecutionControl, Phase};
use ratex_types::storage::{BoundedClone, StorageFamily};

#[derive(Clone, Copy)]
pub(super) enum Template<'a> {
    Text(&'a str),
    BuiltinText(&'a str),
    Tokens(&'a [Token]),
}
#[derive(Clone)]
struct View<'a> {
    is_builtin_literal: bool,
    text: &'a str,
    loc: SourceLocation,
    noexpand: bool,
    treat_as_relax: bool,
}
impl View<'_> {
    fn owned(self, execution: Option<&ExecutionControl<'_>>) -> ParseResult<Token> {
        Ok(Token {
            is_builtin_literal: self.is_builtin_literal,
            text: match execution.and_then(ExecutionControl::storage) {
                Some(storage) => storage
                    .try_string(Phase::Macro, self.text)
                    .map_err(crate::ParseError::storage)?,
                None => self.text.to_owned(),
            },
            loc: self.loc,
            noexpand: self.noexpand,
            treat_as_relax: self.treat_as_relax,
        })
    }
}
enum Iter<'a> {
    Text(Lexer<'a>, bool),
    Tokens(std::iter::Rev<std::slice::Iter<'a, Token>>),
}
impl<'a> Iterator for Iter<'a> {
    type Item = View<'a>;
    fn next(&mut self) -> Option<Self::Item> {
        Some(match self {
            Self::Text(lexer, builtin) => {
                let s = lexer.next_span();
                if s.is_eof {
                    return None;
                }
                View {
                    is_builtin_literal: *builtin,
                    text: s.text,
                    loc: s.loc,
                    noexpand: false,
                    treat_as_relax: false,
                }
            }
            Self::Tokens(tokens) => {
                let t = tokens.next()?;
                View {
                    is_builtin_literal: t.is_builtin_literal,
                    text: &t.text,
                    loc: t.loc.clone(),
                    noexpand: t.noexpand,
                    treat_as_relax: t.treat_as_relax,
                }
            }
        })
    }
}
enum Item<'a> {
    Literal(View<'a>),
    Arg(usize),
}
struct Items<'a> {
    iter: std::iter::Peekable<Iter<'a>>,
    num_args: usize,
}
impl<'a> Iterator for Items<'a> {
    type Item = Item<'a>;
    fn next(&mut self) -> Option<Self::Item> {
        let t = self.iter.next()?;
        if self.num_args > 0 && t.text == "#" {
            if let Some(next) = self.iter.peek() {
                if next.text == "#" {
                    // Preserve the second hash's location: mhchem uses source
                    // gaps to reconstruct whitespace after macro substitution.
                    return self.iter.next().map(Item::Literal);
                } else if let Ok(n) = next.text.parse::<usize>() {
                    if n > 0 && n <= self.num_args {
                        self.iter.next();
                        return Some(Item::Arg(n - 1));
                    }
                }
            }
        }
        Some(Item::Literal(t))
    }
}
impl<'a> Template<'a> {
    fn iter<'b>(self, execution: Option<&'b ExecutionControl<'b>>) -> Iter<'b>
    where
        'a: 'b,
    {
        match self {
            Self::Text(s) | Self::BuiltinText(s) => {
                let builtin = matches!(self, Self::BuiltinText(_));
                let mut lexer = Lexer::new(s);
                if let Some(execution) = execution {
                    lexer.set_execution(execution);
                }
                Iter::Text(lexer, builtin)
            }
            Self::Tokens(t) => Iter::Tokens(t.iter().rev()),
        }
    }
    fn items<'b>(self, num_args: usize, execution: Option<&'b ExecutionControl<'b>>) -> Items<'b>
    where
        'a: 'b,
    {
        Items {
            iter: self.iter(execution).peekable(),
            num_args,
        }
    }

    // Preserve the Text API's historical raw-text inference (including hashes
    // in comments/control words), without allocating text.replace("##", "").
    pub(super) fn num_args(self) -> usize {
        let text = match self {
            Self::Text(text) | Self::BuiltinText(text) => text,
            Self::Tokens(_) => unreachable!("token templates carry an explicit arity"),
        };
        if !text.contains('#') {
            return 0;
        }
        let mut num_args = 0usize;
        loop {
            let Some(next) = num_args.checked_add(1) else {
                return num_args;
            };
            let mut buffer = [0u8; 22];
            let mut start = buffer.len();
            let mut value = next;
            loop {
                start -= 1;
                buffer[start] = b'0' + (value % 10) as u8;
                value /= 10;
                if value == 0 {
                    break;
                }
            }
            start -= 1;
            buffer[start] = b'#';
            let needle = &buffer[start..];
            let mut matched = 0;
            let found = text.split("##").flat_map(str::bytes).any(|byte| {
                if byte == needle[matched] {
                    matched += 1;
                } else {
                    matched = usize::from(byte == b'#');
                }
                matched == needle.len()
            });
            if !found {
                return num_args;
            }
            num_args = next;
        }
    }

    pub(super) fn expand(
        self,
        mut args: Vec<Vec<Token>>,
        budget: &ExpansionBudget,
        execution: Option<&ExecutionControl<'_>>,
    ) -> ParseResult<Vec<Token>> {
        let mut small = [0usize; 9];
        let mut legacy = Vec::new();
        let remaining: &mut [usize] = if args.len() <= 9 {
            &mut small[..args.len()]
        } else {
            if execution.is_some_and(|control| control.storage().is_some()) {
                return Err(crate::ParseError::msg("Unsupported macro arity"));
            }
            legacy.resize(args.len(), 0);
            &mut legacy
        };
        let mut cost = Some(Cost::default());
        let mut output_len = Some(0usize);
        for item in self.items(args.len(), execution) {
            if let Some(control) = execution {
                control
                    .checkpoint(Phase::Macro)
                    .map_err(crate::ParseError::control)?;
            }
            match item {
                Item::Literal(t) => {
                    cost = cost.and_then(|c| c.checked_add(Cost::token(t.text)));
                    output_len = output_len.and_then(|len| len.checked_add(1));
                }
                Item::Arg(n) => {
                    output_len = output_len.and_then(|len| len.checked_add(args[n].len()));
                    remaining[n] = match remaining[n].checked_add(1) {
                        Some(n) => n,
                        None => {
                            budget.charge(None, "parameter references")?;
                            unreachable!()
                        }
                    };
                }
            }
        }
        for (n, arg) in args.iter().enumerate() {
            let copies = remaining[n].saturating_sub(1);
            let arg_cost = token_cost(arg).and_then(|c| c.checked_mul(copies));
            cost = cost.and_then(|c| arg_cost.and_then(|a| c.checked_add(a)));
        }
        if let Some(control) = execution.filter(|control| control.storage().is_some()) {
            control
                .consume(
                    Phase::Macro,
                    ratex_types::execution::Resource::Tokens,
                    cost.map_or(u64::MAX, |cost| cost.tokens as u64),
                )
                .map_err(crate::ParseError::control)?;
            control
                .consume(
                    Phase::Macro,
                    ratex_types::execution::Resource::GeneratedBytes,
                    cost.map_or(u64::MAX, |cost| cost.bytes as u64),
                )
                .map_err(crate::ParseError::control)?;
        }
        budget.charge(cost, "macro replacement")?;
        let mut out = if let Some(storage) = execution.and_then(ExecutionControl::storage) {
            let Some(length) = output_len else {
                return Err(crate::ParseError::storage(
                    storage.fail(ratex_types::storage::StorageError::Arithmetic),
                ));
            };
            storage
                .try_vec_with_capacity(Phase::Macro, StorageFamily::Tokens, length)
                .map_err(crate::ParseError::storage)?
        } else {
            Vec::new()
        };
        for item in self.items(args.len(), execution) {
            if let Some(control) = execution {
                control
                    .checkpoint(Phase::Macro)
                    .map_err(crate::ParseError::control)?;
            }
            match item {
                Item::Literal(t) => out.push(t.owned(execution)?),
                Item::Arg(n) => {
                    remaining[n] -= 1;
                    if remaining[n] == 0 {
                        for token in std::mem::take(&mut args[n]).into_iter().rev() {
                            if let Some(control) = execution {
                                control
                                    .checkpoint(Phase::Macro)
                                    .map_err(crate::ParseError::control)?;
                            }
                            out.push(token);
                        }
                    } else {
                        for token in args[n].iter().rev() {
                            let token = if let Some(storage) =
                                execution.and_then(ExecutionControl::storage)
                            {
                                token
                                    .bounded_clone(&storage, Phase::Macro, StorageFamily::Tokens)
                                    .map_err(crate::ParseError::storage)?
                            } else {
                                token.clone()
                            };
                            out.push(token);
                        }
                    }
                }
            }
        }
        out.reverse();
        Ok(out)
    }
}
pub(super) fn token_cost(tokens: &[Token]) -> Option<Cost> {
    tokens
        .iter()
        .try_fold(Cost::default(), |c, t| c.checked_add(Cost::token(&t.text)))
}
