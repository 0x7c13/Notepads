// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

//! Final email pass over the coalesced ordinary inline tree.
//! No scanner, entity decoder, sourcepos search or reference resolver runs here.

use super::*;
use crate::native::value::{NodeValueRole, ValueText};
use crate::native::{GeneratedPrefix, PayloadKind};
use crate::nodes::NodeLink;

impl<'a: 'd, 'o, 'c, 'd, 'n, 'h, H: Host> Parser<'a, 'o, 'c, 'd, 'n, 'h, H> {
    pub(super) fn finish_g4_email(&mut self, root: Node<'a>) -> Result<(), H::Error> {
        let mut stack = Vec::new();
        self.ctx.push(&mut stack, root)?;
        while let Some(parent) = stack.pop() {
            self.ctx.event()?;
            let mut child = parent.first_child();
            while let Some(node) = child {
                self.ctx.event()?;
                child = node.next_sibling();
                let value = node.data();
                match &value.value {
                    NodeValue::Text(_) => {
                        drop(value);
                        self.finish_g4_email_run(node)?;
                    }
                    NodeValue::Link(_)
                    | NodeValue::Image(_)
                    | NodeValue::Code(_)
                    | NodeValue::HtmlInline(_)
                    | NodeValue::SoftBreak
                    | NodeValue::LineBreak => {}
                    _ => self.ctx.push(&mut stack, node)?,
                }
            }
        }
        self.ctx.discard_vec(stack)
    }

    fn email_node(&mut self, value: NodeValue, sourcepos: Sourcepos) -> Result<Node<'a>, H::Error> {
        let kind = native::kind(&value)
            .ok_or_else(|| self.ctx.host.reject(NativeViolation::InvalidProvenance))?;
        let id = self.ctx.node(None, kind)?;
        let mut ast = Ast::new_with_sourcepos(value, sourcepos);
        ast.native_id = Some(id);
        ast.open = false;
        self.arena.try_alloc(ast.into(), self.ctx)
    }

    fn finish_g4_email_run(&mut self, node: Node<'a>) -> Result<(), H::Error> {
        let (id, sourcepos, text) = {
            let ast = node.data();
            let id = ast
                .native_id
                .ok_or_else(|| self.ctx.host.reject(NativeViolation::InvalidProvenance))?;
            let NodeValue::Text(value) = &ast.value else {
                return Err(self.ctx.host.reject(NativeViolation::InvalidProvenance));
            };
            let Some(text) = self.ctx.g4_email_text(id, value)? else {
                return Ok(());
            };
            (id, ast.sourcepos, text)
        };
        let result = native::g4::email::with_run(id, text.len(), |run| {
            let mut selected = native::g4::email::select(&text, run, self.ctx)?;
            let Some(first) = selected.next(id, text.len(), self.ctx)? else {
                return Ok(false);
            };
            let mut atoms = self.ctx.email_atoms(id, text.len())?;
            let mut current = Some(first);
            let mut at = 0;
            while let Some(email) = current {
                self.ctx.event()?;
                self.ctx.scan(3)?;
                if email.start() < at
                    || email.start() >= email.at()
                    || email.at() >= email.end()
                    || email.end() > text.len()
                    || text.as_bytes()[email.at()] != b'@'
                    || !text.is_char_boundary(email.start())
                    || !text.is_char_boundary(email.end())
                {
                    return Err(self.ctx.host.reject(NativeViolation::InvalidProvenance));
                }
                if at != email.start() {
                    let fragment = atoms.take(email.start(), self.ctx)?;
                    let literal = self.email_value(&text[at..email.start()])?;
                    // Legacy sourcepos is diagnostic only. Exact provenance is
                    // transported below from each actual accepted atom.
                    let literal = self.email_node(NodeValue::Text(literal), sourcepos)?;
                    self.ctx
                        .email_bind_literal(literal.data().native_id.unwrap(), fragment)?;
                    self.finish_email_value(literal)?;
                    node.insert_before(literal);
                }
                let fragment = atoms.take(email.end(), self.ctx)?;
                let raw = fragment.source(self.ctx)?;
                // Email starts in a real copied source byte, including the
                // opening '&' when that byte belongs to a decoded entity.
                let first = self.ctx.accepted_copy_piece(RSpan {
                    end: raw.start + 1,
                    ..raw
                })?;
                let last = self.ctx.accepted_copy_piece(RSpan {
                    start: raw.end - 1,
                    ..raw
                })?;
                if !self.ctx.host.email_autolink_allowed(PSpan {
                    start: first.start,
                    end: last.end,
                })? {
                    let literal = self.email_value(&text[email.start()..email.end()])?;
                    let literal = self.email_node(NodeValue::Text(literal), sourcepos)?;
                    self.ctx
                        .email_bind_literal(literal.data().native_id.unwrap(), fragment)?;
                    self.finish_email_value(literal)?;
                    node.insert_before(literal);
                    at = email.end();
                    current = selected.next(id, text.len(), self.ctx)?;
                    continue;
                }
                let payload = self.ctx.payload(PayloadKind::Destination, raw)?;
                let mut destination = self.ctx.decoded_string(GeneratedPrefix::Mailto.text())?;
                self.ctx.host.admit(NativeAdmission::DecodedBytes(
                    (email.end() - email.start()) as u64,
                ))?;
                self.ctx
                    .append(&mut destination, &text[email.start()..email.end()])?;
                let label = self.email_value(&text[email.start()..email.end()])?;
                let label = self.email_node(NodeValue::Text(label), sourcepos)?;
                let label_id = label.data().native_id.unwrap();
                self.ctx.email_bind_label(label_id, payload, fragment)?;
                self.finish_email_value(label)?;
                self.ctx.fact(AcceptedFact::PayloadPrefix {
                    payload,
                    kind: GeneratedPrefix::Mailto,
                    at: RSpan {
                        end: raw.start,
                        ..raw
                    },
                    value: "mailto:",
                })?;
                self.ctx.fact(AcceptedFact::PayloadEnd {
                    payload,
                    value: &destination,
                })?;
                let destination = self.ctx.finish_payload_value(
                    destination,
                    Some(payload),
                    PayloadKind::Destination,
                )?;
                let link = self.ctx.boxed(NodeLink {
                    url: destination,
                    title: self.ctx.empty_value(),
                })?;
                let link = self.email_node(NodeValue::Link(link), sourcepos)?;
                let link_id = link.data().native_id.unwrap();
                link.append(label);
                self.ctx.fact(AcceptedFact::Link {
                    node: link_id,
                    syntax: raw,
                    label: raw,
                    image: false,
                    wiki: None,
                    destination: Some(payload),
                    title: None,
                    definition: None,
                })?;
                self.ctx
                    .fact(AcceptedFact::Extended(native::g4::ExtendedAccepted {
                        node: link_id,
                        label: label_id,
                        kind: native::g4::ExtendedKind::Email,
                        raw,
                        destination: payload,
                    }))?;
                node.insert_before(link);
                at = email.end();
                current = selected.next(id, text.len(), self.ctx)?;
            }
            if at < text.len() {
                let fragment = atoms.take(text.len(), self.ctx)?;
                let literal = self.email_value(&text[at..])?;
                let literal = self.email_node(NodeValue::Text(literal), sourcepos)?;
                self.ctx
                    .email_bind_literal(literal.data().native_id.unwrap(), fragment)?;
                self.finish_email_value(literal)?;
                node.insert_before(literal);
            }
            atoms.finish(self.ctx)?;
            node.detach();
            Ok(true)
        });
        match result {
            Ok(_) => self.ctx.discard_string(text),
            Err(error) => {
                // Free the actual temporary now. A latched Host may reject a
                // ledger walk; the failed Context immediately owns its cleanup.
                drop(text);
                self.ctx.fail_grammar();
                Err(error)
            }
        }
    }

    // Preserve the old per-output DecodedBytes admissions, including logical
    // UTF-8 chunk boundaries, while the actual accepted fragment owns the value.
    fn email_value(&mut self, value: &str) -> Result<ValueText, H::Error> {
        let mut at = 0;
        while at < value.len() {
            let mut end = (at + 65_536).min(value.len());
            while !value.is_char_boundary(end) {
                end -= 1;
            }
            self.ctx
                .host
                .admit(NativeAdmission::DecodedBytes((end - at) as u64))?;
            at = end;
        }
        Ok(ValueText::pending())
    }

    fn finish_email_value(&mut self, node: Node<'a>) -> Result<(), H::Error> {
        let mut ast = node.data_mut();
        let id = ast.native_id;
        let NodeValue::Text(value) = &mut ast.value else {
            return Err(self.ctx.host.reject(NativeViolation::InvalidProvenance));
        };
        *value = self
            .ctx
            .finish_node_value(mem::take(value), id, NodeValueRole::Text)?;
        Ok(())
    }
}
