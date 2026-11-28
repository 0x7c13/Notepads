// Copyright 2016 the SimpleCSS Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT
// Modifications Copyright (c) 2026, Jiaqi (0x7c13) Liu.
use crate::{
    control::{event, Control},
    stream::Stream,
    Declaration, Error, Rule, Selector,
};
use alloc::vec::Vec;
pub(crate) trait SheetSink<'a> {
    type Selector;
    fn rule_count(&self) -> usize;
    fn selector(
        &mut self,
        text: &'a str,
        control: Option<&Control>,
    ) -> Result<(Option<Self::Selector>, usize), ()>;
    fn rule(&mut self, selector: Self::Selector) -> Result<(), ()>;
    fn begin_declarations(&mut self);
    fn declaration(&mut self, declaration: Declaration<'a>) -> Result<(), ()>;
    fn finish_group(&mut self, start: usize) -> Result<(), ()>;
}
pub(crate) struct PlainSheet<'a, 'r> {
    pub(crate) rules: &'r mut Vec<Rule<'a>>,
    pub(crate) declarations: Vec<Declaration<'a>>,
}
impl<'a> SheetSink<'a> for PlainSheet<'a, '_> {
    type Selector = Selector<'a>;
    fn rule_count(&self) -> usize {
        self.rules.len()
    }
    fn selector(
        &mut self,
        text: &'a str,
        _: Option<&Control>,
    ) -> Result<(Option<Self::Selector>, usize), ()> {
        Ok(crate::selector::parse(text))
    }
    fn rule(&mut self, selector: Self::Selector) -> Result<(), ()> {
        self.rules.push(Rule {
            selector,
            declarations: Vec::new(),
        });
        Ok(())
    }
    fn begin_declarations(&mut self) {
        self.declarations.clear();
    }
    fn declaration(&mut self, declaration: Declaration<'a>) -> Result<(), ()> {
        self.declarations.push(declaration);
        Ok(())
    }
    fn finish_group(&mut self, start: usize) -> Result<(), ()> {
        for rule in self.rules.iter_mut().skip(start) {
            rule.declarations = self.declarations.clone();
        }
        Ok(())
    }
}
pub(crate) fn parse_more<'a, S: SheetSink<'a>>(
    text: &'a str,
    sink: &mut S,
    control: Option<&Control>,
) -> Result<(), ()> {
    let mut s = match control {
        Some(c) => Stream::controlled(text, 0, c),
        None => Stream::new(text),
    };
    if s.skip_spaces_and_comments().is_err() {
        return control.map_or(Ok(()), Control::checked);
    }
    while !s.at_end() {
        event(control)?;
        if s.skip_spaces_and_comments().is_err() {
            break;
        }
        let result = if s.curr_byte() == Ok(b'@') {
            s.advance(1);
            crate::consume_at_rule(&mut s)
        } else {
            rule_set(&mut s, sink, control)
        };
        if let Some(c) = control {
            c.checked()?;
        }
        let _ = result;
    }
    if let Some(c) = control {
        c.checked()?;
    }
    if !s.at_end() {
        log::warn!("{} bytes were left.", s.slice_tail().len());
    }
    Ok(())
}
fn rule_set<'a, S: SheetSink<'a>>(
    s: &mut Stream<'a, '_>,
    sink: &mut S,
    control: Option<&Control>,
) -> Result<(), Error> {
    let start = sink.rule_count();
    while s.curr_byte()? == b',' || start == sink.rule_count() {
        if s.curr_byte()? == b',' {
            s.advance(1);
        }
        let (selector, offset) = sink
            .selector(s.slice_tail(), control)
            .map_err(|()| Error::UnexpectedEndOfStream)?;
        s.advance(offset);
        s.skip_spaces();
        if let Some(selector) = selector {
            sink.rule(selector)
                .map_err(|()| Error::UnexpectedEndOfStream)?;
        }
        match s.curr_byte()? {
            b'{' => break,
            b',' => {}
            _ => {
                s.skip_bytes(|c| c != b'{');
                break;
            }
        }
    }
    s.try_consume_byte(b'{');
    sink.begin_declarations();
    while !s.at_end() && s.curr_byte() != Ok(b'}') {
        match crate::consume_declaration(s) {
            Ok(d) => sink
                .declaration(d)
                .map_err(|()| Error::UnexpectedEndOfStream)?,
            Err(_) => {
                crate::consume_until_block_end(s);
                break;
            }
        }
    }
    if let Some(c) = control {
        c.checked().map_err(|()| Error::UnexpectedEndOfStream)?;
    }
    sink.finish_group(start)
        .map_err(|()| Error::UnexpectedEndOfStream)?;
    s.try_consume_byte(b'}');
    Ok(())
}
