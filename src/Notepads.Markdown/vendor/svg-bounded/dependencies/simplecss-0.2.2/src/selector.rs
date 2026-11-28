// Copyright 2019 the SimpleCSS Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT
// Modifications Copyright (c) 2026, Jiaqi (0x7c13) Liu.

use crate::control::{event, Control};
use alloc::vec::Vec;
use core::fmt;

use log::warn;

use crate::stream::Stream;
use crate::Error;

/// An attribute selector operator.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum AttributeOperator<'a> {
    /// `[attr]`
    Exists,
    /// `[attr=value]`
    Matches(&'a str),
    /// `[attr~=value]`
    Contains(&'a str),
    /// `[attr|=value]`
    StartsWith(&'a str),
}

impl AttributeOperator<'_> {
    /// Checks that value is matching the operator.
    pub fn matches(&self, value: &str) -> bool {
        match *self {
            AttributeOperator::Exists => true,
            AttributeOperator::Matches(v) => value == v,
            AttributeOperator::Contains(v) => value.split(' ').any(|s| s == v),
            AttributeOperator::StartsWith(v) => {
                // exactly `v` or beginning with `v` immediately followed by `-`
                if value == v {
                    true
                } else if value.starts_with(v) {
                    value.get(v.len()..v.len() + 1) == Some("-")
                } else {
                    false
                }
            }
        }
    }
}

/// A pseudo-class.
#[derive(Clone, Copy, PartialEq, Debug)]
#[allow(missing_docs)]
pub enum PseudoClass<'a> {
    FirstChild,
    Link,
    Visited,
    Hover,
    Active,
    Focus,
    Lang(&'a str),
}

impl fmt::Display for PseudoClass<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PseudoClass::FirstChild => write!(f, "first-child"),
            PseudoClass::Link => write!(f, "link"),
            PseudoClass::Visited => write!(f, "visited"),
            PseudoClass::Hover => write!(f, "hover"),
            PseudoClass::Active => write!(f, "active"),
            PseudoClass::Focus => write!(f, "focus"),
            PseudoClass::Lang(lang) => write!(f, "lang({})", lang),
        }
    }
}

/// A trait to query an element node metadata.
pub trait Element: Sized {
    /// Returns a parent element.
    fn parent_element(&self) -> Option<Self>;

    /// Returns a previous sibling element.
    fn prev_sibling_element(&self) -> Option<Self>;

    /// Checks that the element has a specified local name.
    fn has_local_name(&self, name: &str) -> bool;

    /// Checks that the element has a specified attribute.
    fn attribute_matches(&self, local_name: &str, operator: AttributeOperator<'_>) -> bool;

    /// Checks that the element matches a specified pseudo-class.
    fn pseudo_class_matches(&self, class: PseudoClass<'_>) -> bool;
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub(crate) enum SimpleSelectorType<'a> {
    Type(&'a str),
    Universal,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub(crate) enum SubSelector<'a> {
    Attribute(&'a str, AttributeOperator<'a>),
    PseudoClass(PseudoClass<'a>),
}

#[derive(Clone, Debug)]
struct SimpleSelector<'a> {
    kind: SimpleSelectorType<'a>,
    subselectors: Vec<SubSelector<'a>>,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub(crate) enum Combinator {
    None,
    Descendant,
    Child,
    AdjacentSibling,
}

#[derive(Clone, Debug)]
struct Component<'a> {
    /// A combinator that precede the selector.
    combinator: Combinator,
    selector: SimpleSelector<'a>,
}

/// A selector.
#[derive(Clone, Debug)]
pub struct Selector<'a> {
    components: Vec<Component<'a>>,
}

impl<'a> Selector<'a> {
    /// Parses a selector from a string.
    ///
    /// Will log any errors as a warnings.
    ///
    /// Parsing will be stopped at EOF, `,` or `{`.
    pub fn parse(text: &'a str) -> Option<Self> {
        parse(text).0
    }

    /// Compute the selector's specificity.
    ///
    /// Cf. <https://www.w3.org/TR/selectors/#specificity>.
    pub fn specificity(&self) -> [u8; 3] {
        let mut spec = [0u8; 3];

        for selector in self.components.iter().map(|c| &c.selector) {
            if matches!(selector.kind, SimpleSelectorType::Type(_)) {
                spec[2] = spec[2].saturating_add(1);
            }

            for sub in &selector.subselectors {
                match sub {
                    SubSelector::Attribute("id", _) => spec[0] = spec[0].saturating_add(1),
                    _ => spec[1] = spec[1].saturating_add(1),
                }
            }
        }

        spec
    }

    /// Checks that the provided element matches the current selector.
    pub fn matches<E: Element>(&self, element: &E) -> bool {
        assert!(!self.components.is_empty(), "selector must not be empty");
        assert_eq!(
            self.components[0].combinator,
            Combinator::None,
            "the first component must not have a combinator"
        );

        crate::matcher::matches(self, element, None).unwrap_or(false)
    }
}
impl<'a> crate::matcher::View<'a> for Selector<'a> {
    fn len(&self) -> usize {
        self.components.len()
    }
    fn kind(&self, index: usize) -> SimpleSelectorType<'a> {
        self.components[index].selector.kind
    }
    fn combinator(&self, index: usize) -> Combinator {
        self.components[index].combinator
    }
    fn subselectors(&self, index: usize) -> &[SubSelector<'a>] {
        &self.components[index].selector.subselectors
    }
}

pub(crate) trait SelectorSink<'a>: Sized {
    type Output;
    fn len(&self) -> usize;
    fn component(&mut self, kind: SimpleSelectorType<'a>, combinator: Combinator)
        -> Result<(), ()>;
    fn sub(&mut self, value: SubSelector<'a>, combinator: Combinator) -> Result<(), ()>;
    fn finish(self) -> Self::Output;
}
struct PlainSelector<'a> {
    components: Vec<Component<'a>>,
}
impl<'a> SelectorSink<'a> for PlainSelector<'a> {
    type Output = Selector<'a>;
    fn len(&self) -> usize {
        self.components.len()
    }
    fn component(
        &mut self,
        kind: SimpleSelectorType<'a>,
        combinator: Combinator,
    ) -> Result<(), ()> {
        self.components.push(Component {
            combinator,
            selector: SimpleSelector {
                kind,
                subselectors: Vec::new(),
            },
        });
        Ok(())
    }
    fn sub(&mut self, value: SubSelector<'a>, combinator: Combinator) -> Result<(), ()> {
        if combinator != Combinator::None || self.components.is_empty() {
            self.component(SimpleSelectorType::Universal, combinator)?;
        }
        self.components
            .last_mut()
            .unwrap()
            .selector
            .subselectors
            .push(value);
        Ok(())
    }
    fn finish(self) -> Self::Output {
        Selector {
            components: self.components,
        }
    }
}
pub(crate) fn parse(text: &str) -> (Option<Selector<'_>>, usize) {
    parse_into(
        text,
        None,
        PlainSelector {
            components: Vec::new(),
        },
    )
    .unwrap_or((None, 0))
}
pub(crate) fn parse_into<'a, S: SelectorSink<'a>>(
    text: &'a str,
    control: Option<&Control>,
    mut sink: S,
) -> Result<(Option<S::Output>, usize), ()> {
    let mut combinator = Combinator::None;
    let stream = match control {
        Some(c) => Stream::controlled(text, 0, c),
        None => Stream::new(text),
    };
    let mut tokenizer = Tokenizer {
        stream,
        after_combinator: true,
        finished: false,
    };
    for token in &mut tokenizer {
        event(control)?;
        let token = match token {
            Ok(t) => t,
            Err(e) => {
                if let Some(c) = control {
                    c.checked()?;
                }
                warn!("Selector parsing failed cause {}.", e);
                return Ok((None, tokenizer.stream.pos()));
            }
        };
        match token {
            SelectorToken::UniversalSelector => {
                sink.component(SimpleSelectorType::Universal, combinator)?;
                combinator = Combinator::None;
            }
            SelectorToken::TypeSelector(name) => {
                sink.component(SimpleSelectorType::Type(name), combinator)?;
                combinator = Combinator::None;
            }
            SelectorToken::ClassSelector(v) => {
                sink.sub(
                    SubSelector::Attribute("class", AttributeOperator::Contains(v)),
                    combinator,
                )?;
                combinator = Combinator::None;
            }
            SelectorToken::IdSelector(v) => {
                sink.sub(
                    SubSelector::Attribute("id", AttributeOperator::Matches(v)),
                    combinator,
                )?;
                combinator = Combinator::None;
            }
            SelectorToken::AttributeSelector(n, v) => {
                sink.sub(SubSelector::Attribute(n, v), combinator)?;
                combinator = Combinator::None;
            }
            SelectorToken::PseudoClass(ident) => {
                let class = match ident {
                    "first-child" => PseudoClass::FirstChild,
                    "link" => PseudoClass::Link,
                    "visited" => PseudoClass::Visited,
                    "hover" => PseudoClass::Hover,
                    "active" => PseudoClass::Active,
                    "focus" => PseudoClass::Focus,
                    _ => {
                        warn!("':{}' is not supported. Selector skipped.", ident);
                        return Ok((None, tokenizer.stream.pos()));
                    }
                };
                sink.sub(SubSelector::PseudoClass(class), combinator)?;
                combinator = Combinator::None;
            }
            SelectorToken::LangPseudoClass(v) => {
                sink.sub(SubSelector::PseudoClass(PseudoClass::Lang(v)), combinator)?;
                combinator = Combinator::None;
            }
            SelectorToken::DescendantCombinator => combinator = Combinator::Descendant,
            SelectorToken::ChildCombinator => combinator = Combinator::Child,
            SelectorToken::AdjacentCombinator => combinator = Combinator::AdjacentSibling,
        }
    }
    if let Some(c) = control {
        c.checked()?;
    }
    let offset = tokenizer.stream.pos();
    if sink.len() == 0 {
        Ok((None, offset))
    } else {
        Ok((Some(sink.finish()), offset))
    }
}

impl fmt::Display for Selector<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for component in &self.components {
            match component.combinator {
                Combinator::Descendant => write!(f, " ")?,
                Combinator::Child => write!(f, " > ")?,
                Combinator::AdjacentSibling => write!(f, " + ")?,
                Combinator::None => {}
            }

            match component.selector.kind {
                SimpleSelectorType::Universal => write!(f, "*")?,
                SimpleSelectorType::Type(ident) => write!(f, "{}", ident)?,
            };

            for sel in &component.selector.subselectors {
                match sel {
                    SubSelector::Attribute(name, operator) => {
                        match operator {
                            AttributeOperator::Exists => {
                                write!(f, "[{}]", name)?;
                            }
                            AttributeOperator::Matches(value) => {
                                write!(f, "[{}='{}']", name, value)?;
                            }
                            AttributeOperator::Contains(value) => {
                                write!(f, "[{}~='{}']", name, value)?;
                            }
                            AttributeOperator::StartsWith(value) => {
                                write!(f, "[{}|='{}']", name, value)?;
                            }
                        };
                    }
                    SubSelector::PseudoClass(class) => write!(f, ":{}", class)?,
                }
            }
        }

        Ok(())
    }
}

/// A selector token.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum SelectorToken<'a> {
    /// `*`
    UniversalSelector,

    /// `div`
    TypeSelector(&'a str),

    /// `.class`
    ClassSelector(&'a str),

    /// `#id`
    IdSelector(&'a str),

    /// `[color=red]`
    AttributeSelector(&'a str, AttributeOperator<'a>),

    /// `:first-child`
    PseudoClass(&'a str),

    /// `:lang(en)`
    LangPseudoClass(&'a str),

    /// `a b`
    DescendantCombinator,

    /// `a > b`
    ChildCombinator,

    /// `a + b`
    AdjacentCombinator,
}

/// A selector tokenizer.
///
/// # Example
///
/// ```
/// use simplecss::{SelectorTokenizer, SelectorToken};
///
/// let mut t = SelectorTokenizer::from("div > p:first-child");
/// assert_eq!(t.next().unwrap().unwrap(), SelectorToken::TypeSelector("div"));
/// assert_eq!(t.next().unwrap().unwrap(), SelectorToken::ChildCombinator);
/// assert_eq!(t.next().unwrap().unwrap(), SelectorToken::TypeSelector("p"));
/// assert_eq!(t.next().unwrap().unwrap(), SelectorToken::PseudoClass("first-child"));
/// assert!(t.next().is_none());
/// ```
pub(crate) struct Tokenizer<'a, 'r> {
    stream: Stream<'a, 'r>,
    after_combinator: bool,
    finished: bool,
}

impl<'a> From<&'a str> for Tokenizer<'a, 'static> {
    fn from(text: &'a str) -> Self {
        Tokenizer {
            stream: Stream::from(text),
            after_combinator: true,
            finished: false,
        }
    }
}

impl<'a> Iterator for Tokenizer<'a, '_> {
    type Item = Result<SelectorToken<'a>, Error>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if event(self.stream.control()).is_err() {
                self.finished = true;
                return Some(Err(Error::UnexpectedEndOfStream));
            }
            if self.finished || self.stream.at_end() {
                if self.after_combinator {
                    self.after_combinator = false;
                    return Some(Err(Error::SelectorMissing));
                }

                return None;
            }

            macro_rules! try2 {
                ($e:expr) => {
                    match $e {
                        Ok(v) => v,
                        Err(e) => {
                            self.finished = true;
                            return Some(Err(e));
                        }
                    }
                };
            }

            return match self.stream.curr_byte_unchecked() {
                b'*' => {
                    if !self.after_combinator {
                        self.finished = true;
                        return Some(Err(Error::UnexpectedSelector));
                    }

                    self.after_combinator = false;
                    self.stream.advance(1);
                    Some(Ok(SelectorToken::UniversalSelector))
                }
                b'#' => {
                    self.after_combinator = false;
                    self.stream.advance(1);
                    let ident = try2!(self.stream.consume_ident());
                    Some(Ok(SelectorToken::IdSelector(ident)))
                }
                b'.' => {
                    self.after_combinator = false;
                    self.stream.advance(1);
                    let ident = try2!(self.stream.consume_ident());
                    Some(Ok(SelectorToken::ClassSelector(ident)))
                }
                b'[' => {
                    self.after_combinator = false;
                    self.stream.advance(1);
                    let ident = try2!(self.stream.consume_ident());

                    let op = match try2!(self.stream.curr_byte()) {
                        b']' => AttributeOperator::Exists,
                        b'=' => {
                            self.stream.advance(1);
                            let value = try2!(self.stream.consume_string());
                            AttributeOperator::Matches(value)
                        }
                        b'~' => {
                            self.stream.advance(1);
                            try2!(self.stream.consume_byte(b'='));
                            let value = try2!(self.stream.consume_string());
                            AttributeOperator::Contains(value)
                        }
                        b'|' => {
                            self.stream.advance(1);
                            try2!(self.stream.consume_byte(b'='));
                            let value = try2!(self.stream.consume_string());
                            AttributeOperator::StartsWith(value)
                        }
                        _ => {
                            self.finished = true;
                            return Some(Err(Error::InvalidAttributeSelector));
                        }
                    };

                    try2!(self.stream.consume_byte(b']'));

                    Some(Ok(SelectorToken::AttributeSelector(ident, op)))
                }
                b':' => {
                    self.after_combinator = false;
                    self.stream.advance(1);
                    let ident = try2!(self.stream.consume_ident());

                    if ident == "lang" {
                        try2!(self.stream.consume_byte(b'('));
                        let raw_lang = self.stream.consume_bytes(|c| c != b')');
                        let lang = crate::stream::trim(raw_lang, self.stream.control());
                        try2!(self.stream.consume_byte(b')'));

                        if lang.is_empty() {
                            self.finished = true;
                            return Some(Err(Error::InvalidLanguagePseudoClass));
                        }

                        Some(Ok(SelectorToken::LangPseudoClass(lang)))
                    } else {
                        Some(Ok(SelectorToken::PseudoClass(ident)))
                    }
                }
                b'>' => {
                    if self.after_combinator {
                        self.after_combinator = false;
                        self.finished = true;
                        return Some(Err(Error::UnexpectedCombinator));
                    }

                    self.stream.advance(1);
                    self.after_combinator = true;
                    Some(Ok(SelectorToken::ChildCombinator))
                }
                b'+' => {
                    if self.after_combinator {
                        self.after_combinator = false;
                        self.finished = true;
                        return Some(Err(Error::UnexpectedCombinator));
                    }

                    self.stream.advance(1);
                    self.after_combinator = true;
                    Some(Ok(SelectorToken::AdjacentCombinator))
                }
                b' ' | b'\t' | b'\n' | b'\r' | b'\x0C' => {
                    self.stream.skip_spaces();

                    if self.after_combinator {
                        continue;
                    }

                    while self.stream.curr_byte() == Ok(b'/') {
                        try2!(self.stream.skip_comment());
                        self.stream.skip_spaces();
                    }

                    match self.stream.curr_byte() {
                        Ok(b'>') | Ok(b'+') | Ok(b',') | Ok(b'{') | Err(_) => continue,
                        _ => {
                            if self.after_combinator {
                                self.after_combinator = false;
                                self.finished = true;
                                return Some(Err(Error::UnexpectedSelector));
                            }

                            self.after_combinator = true;
                            Some(Ok(SelectorToken::DescendantCombinator))
                        }
                    }
                }
                b'/' => {
                    if self.stream.next_byte() == Ok(b'*') {
                        try2!(self.stream.skip_comment());
                    } else {
                        self.finished = true;
                    }

                    continue;
                }
                b',' | b'{' => {
                    self.finished = true;
                    continue;
                }
                _ => {
                    let ident = try2!(self.stream.consume_ident());

                    if !self.after_combinator {
                        self.finished = true;
                        return Some(Err(Error::UnexpectedSelector));
                    }

                    self.after_combinator = false;
                    Some(Ok(SelectorToken::TypeSelector(ident)))
                }
            };
        }
    }
}

/// A zero-allocation selector tokenizer retaining the original public lifetime.
pub struct SelectorTokenizer<'a>(Tokenizer<'a, 'static>);
impl<'a> From<&'a str> for SelectorTokenizer<'a> {
    fn from(text: &'a str) -> Self {
        Self(Tokenizer::from(text))
    }
}
impl<'a> Iterator for SelectorTokenizer<'a> {
    type Item = Result<SelectorToken<'a>, Error>;
    fn next(&mut self) -> Option<Self::Item> {
        self.0.next()
    }
}
