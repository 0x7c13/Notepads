// Copyright 2018 Evgeniy Reizner
// Modifications Copyright (c) 2026, Jiaqi (0x7c13) Liu.
// SPDX-License-Identifier: MIT
//! Original XML state machine shared by ordinary and request storage modes.
use crate::storage::{plain, Bytes, Depths, Plain, Policy};
use crate::{DepthData, Indent, Options, State};
use std::fmt;

pub(crate) struct WriterCore<P: Policy> {
    pub(crate) buf: P::Bytes,
    policy: P,
    state: State,
    preserve_whitespaces: bool,
    depth_stack: P::Depths,
    opt: Options,
}

impl<P: Policy> WriterCore<P> {
    pub(crate) fn new(policy: P, buf: P::Bytes, opt: Options) -> Self {
        let depth_stack = policy.depths();
        Self {
            buf,
            policy,
            state: State::Empty,
            preserve_whitespaces: false,
            depth_stack,
            opt,
        }
    }
    pub(crate) fn write_declaration(&mut self) -> Result<(), P::Error> {
        self.policy.check(0)?;
        if self.state != State::Empty {
            return Err(self.policy.invalid_state("declaration was already written"));
        }
        self.state = State::Attributes;
        self.push_str("<?xml")?;
        self.write_attribute_fmt("version", format_args!("{}", "1.0"))?;
        self.write_attribute_fmt("encoding", format_args!("{}", "UTF-8"))?;
        self.write_attribute_fmt("standalone", format_args!("{}", "no"))?;
        self.push_str("?>")?;
        self.state = State::Document;
        self.policy.check(0)
    }
    pub(crate) fn write_comment_fmt(&mut self, args: fmt::Arguments<'_>) -> Result<(), P::Error> {
        self.policy.check(0)?;
        if self.state == State::Attributes {
            self.write_open_element()?;
        }
        if self.state != State::Empty {
            self.write_new_line()?;
        }
        self.write_node_indent()?;
        self.push_str("<!--")?;
        self.buf.format(args)?;
        self.push_str("-->")?;
        if self.state == State::Attributes {
            self.depth_stack.push(DepthData {
                range: 0..0,
                has_children: false,
            })?;
        }
        self.state = State::Document;
        self.policy.check(0)
    }
    pub(crate) fn start_element(&mut self, name: &str) -> Result<(), P::Error> {
        self.policy.check(0)?;
        if self.state == State::Attributes {
            self.write_open_element()?;
        }
        if self.state != State::Empty {
            self.write_new_line()?;
        }
        if !self.preserve_whitespaces {
            self.write_node_indent()?;
        }
        self.push_byte(b'<')?;
        let start = self.buf.as_slice().len();
        self.push_str(name)?;
        self.depth_stack.push(DepthData {
            range: start..self.buf.as_slice().len(),
            has_children: false,
        })?;
        self.state = State::Attributes;
        self.policy.check(0)
    }
    pub(crate) fn write_attribute_fmt(
        &mut self,
        name: &str,
        args: fmt::Arguments<'_>,
    ) -> Result<(), P::Error> {
        self.policy.check(0)?;
        if self.state != State::Attributes {
            return Err(self
                .policy
                .invalid_state("must be called after start_element()"));
        }
        self.write_attribute_prefix(name)?;
        let start = self.buf.as_slice().len();
        self.buf.format(args)?;
        self.escape(start, true)?;
        self.write_quote()?;
        self.policy.check(0)
    }
    fn write_attribute_prefix(&mut self, name: &str) -> Result<(), P::Error> {
        if self.opt.attributes_indent == Indent::None {
            self.push_byte(b' ')?;
        } else {
            self.push_byte(b'\n')?;
            let depth = self.depth_stack.len();
            if depth > 0 {
                self.write_indent(depth - 1, self.opt.indent)?;
            }
            self.write_indent(1, self.opt.attributes_indent)?;
        }
        self.push_str(name)?;
        self.push_byte(b'=')?;
        self.write_quote()
    }
    fn escape(&mut self, mut start: usize, attribute: bool) -> Result<(), P::Error> {
        let needle = if attribute { self.quote() } else { b'<' };
        let replacement: &[u8] = if !attribute {
            b"&lt;"
        } else if self.opt.use_single_quote {
            b"&apos;"
        } else {
            b"&quot;"
        };
        loop {
            let mut found = None;
            while start < self.buf.as_slice().len() {
                let end = start + (self.buf.as_slice().len() - start).min(256);
                self.policy.check(end - start)?;
                if let Some(offset) = self.buf.as_slice()[start..end]
                    .iter()
                    .position(|value| *value == needle)
                {
                    found = Some(start + offset);
                    break;
                }
                start = end;
            }
            let Some(index) = found else {
                break;
            };
            self.buf.replace(index..index + 1, replacement)?;
            start = index + replacement.len();
        }
        self.policy.check(0)
    }
    pub(crate) fn set_preserve_whitespaces(&mut self, preserve: bool) {
        self.preserve_whitespaces = preserve;
    }
    pub(crate) fn write_text_fmt(&mut self, args: fmt::Arguments<'_>) -> Result<(), P::Error> {
        self.policy.check(0)?;
        if self.state == State::Empty || self.depth_stack.len() == 0 {
            return Err(self
                .policy
                .invalid_state("must be called after start_element()"));
        }
        if self.state == State::Attributes {
            self.write_open_element()?;
        }
        if self.state != State::Empty {
            self.write_new_line()?;
        }
        self.write_node_indent()?;
        let start = self.buf.as_slice().len();
        self.buf.format(args)?;
        self.escape(start, false)?;
        if self.state == State::Attributes {
            self.depth_stack.push(DepthData {
                range: 0..0,
                has_children: false,
            })?;
        }
        self.state = State::Document;
        self.policy.check(0)
    }
    pub(crate) fn end_element(&mut self) -> Result<(), P::Error> {
        self.policy.check(1)?;
        if let Some(depth) = self.depth_stack.pop() {
            if depth.has_children {
                if !self.preserve_whitespaces {
                    self.write_new_line()?;
                    self.write_node_indent()?;
                }
                self.push_str("</")?;
                self.buf.append_existing(depth.range)?;
                self.push_byte(b'>')?;
            } else {
                self.push_str("/>")?;
            }
        }
        self.state = State::Document;
        self.policy.check(0)
    }
    pub(crate) fn finish(&mut self) -> Result<(), P::Error> {
        while self.depth_stack.len() != 0 {
            self.end_element()?;
        }
        self.write_new_line()?;
        self.policy.check(0)
    }
    fn push_byte(&mut self, value: u8) -> Result<(), P::Error> {
        self.buf.push(value)
    }
    fn push_str(&mut self, text: &str) -> Result<(), P::Error> {
        self.buf.extend(text.as_bytes())
    }
    fn quote(&self) -> u8 {
        if self.opt.use_single_quote {
            b'\''
        } else {
            b'"'
        }
    }
    fn write_quote(&mut self) -> Result<(), P::Error> {
        self.push_byte(self.quote())
    }
    fn write_open_element(&mut self) -> Result<(), P::Error> {
        if let Some(depth) = self.depth_stack.last_mut() {
            depth.has_children = true;
            self.push_byte(b'>')?;
            self.state = State::Document;
        }
        Ok(())
    }
    fn write_node_indent(&mut self) -> Result<(), P::Error> {
        self.write_indent(self.depth_stack.len(), self.opt.indent)
    }
    fn write_indent(&mut self, depth: usize, indent: Indent) -> Result<(), P::Error> {
        if indent == Indent::None || self.preserve_whitespaces {
            return Ok(());
        }
        for _ in 0..depth {
            self.policy.check(1)?;
            match indent {
                Indent::None => {}
                Indent::Spaces(count) => {
                    for _ in 0..count {
                        self.push_byte(b' ')?;
                    }
                }
                Indent::Tabs => self.push_byte(b'\t')?,
            }
        }
        Ok(())
    }
    fn write_new_line(&mut self) -> Result<(), P::Error> {
        if self.opt.indent != Indent::None && !self.preserve_whitespaces {
            self.push_byte(b'\n')?;
        }
        Ok(())
    }
}

impl WriterCore<Plain> {
    pub(crate) fn write_attribute_raw(&mut self, name: &str, write: impl FnOnce(&mut Vec<u8>)) {
        if self.state != State::Attributes {
            panic!("must be called after start_element()");
        }
        plain(self.write_attribute_prefix(name));
        let start = self.buf.0.len();
        write(&mut self.buf.0);
        plain(self.escape(start, true));
        plain(self.write_quote());
    }
}
