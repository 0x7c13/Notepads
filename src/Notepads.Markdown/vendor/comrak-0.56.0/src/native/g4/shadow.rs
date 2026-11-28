// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

//! Shadow-only accepted ownership. This store has no production mapping or
//! payload authority. Its indexed rows die with the child Context, before B.

#[cfg(test)]
#[path = "shadow_tests.rs"]
mod tests;

use super::{OwnerKind, ShadowOwner};
use crate::native::{
    AcceptedFact, DefinitionId, Host, NativeNodeId, NativeViolation, RSpan,
    definitions::DefinitionRecord,
    rows::{NativeRows, charge_host},
};
use crate::{
    arena_tree::NodeEdge,
    nodes::{Node, NodeValue},
    parser::inlines::{ByteTables, FootnoteDefs, RefMap, Subject},
};

pub(crate) struct Seed<'s, 'options> {
    pub(crate) raw: &'s str,
    pub(crate) offsets: &'s [usize],
    pub(crate) line: usize,
    pub(crate) in_paragraph: bool,
    pub(crate) in_table_cell: bool,
    pub(crate) options: &'s crate::Options<'options>,
    pub(crate) refmap: &'s RefMap,
    pub(crate) tables: &'s ByteTables,
}

/// This is the sole A producer. Its actual child storage is lexically gone
/// before ShadowReleased can be constructed or B can borrow the Host.
pub(crate) fn run<'leaf, H: Host>(
    leaf: super::LeafBrand<'leaf>,
    seed: Seed<'_, '_>,
    parent: &mut crate::native::Context<'_, H>,
) -> Result<super::ShadowReleased<'leaf, H::Lease>, H::Error> {
    run_with_note_depth(leaf, seed, parent, 0)
}

pub(crate) fn run_with_note_depth<'leaf, H: Host>(
    leaf: super::LeafBrand<'leaf>,
    seed: Seed<'_, '_>,
    parent: &mut crate::native::Context<'_, H>,
    inline_footnote_depth: usize,
) -> Result<super::ShadowReleased<'leaf, H::Lease>, H::Error> {
    parent.event()?;
    if leaf.bytes != seed.raw.len() || seed.options.extension.autolink || seed.offsets.is_empty() {
        return Err(parent.host.reject(NativeViolation::InvalidProvenance));
    }
    let owners = {
        // Declared first so error unwinding destroys both actual arenas and all
        // Subject values before the child's independent registry lease ledger.
        let mut child = parent.shadow_child();
        let arena = crate::native::FallibleArena::new();
        let delimiters = crate::native::FallibleArena::new();
        let mut footnotes = FootnoteDefs::new();
        let root = crate::parser::inlines::make_inline(
            &mut child,
            &arena,
            // The shadow root only owns inline children. Preserve the distinction
            // among paragraphs, headings and table cells. Wiki aliases use
            // the table's escaped-pipe grammar only in an actual table cell.
            if seed.in_paragraph {
                NodeValue::Paragraph
            } else if seed.in_table_cell {
                NodeValue::TableCell
            } else {
                NodeValue::Heading(crate::nodes::NodeHeading {
                    level: 1,
                    ..Default::default()
                })
            },
            (seed.line, 1, seed.line, 1).into(),
        )?;
        let raw_copy = child.string(seed.raw)?;
        let mut subject = Subject::new(
            &mut child,
            Some(leaf.buffer),
            leaf.base,
            &arena,
            seed.options,
            raw_copy,
            seed.line,
            seed.refmap,
            seed.tables,
            &mut footnotes,
            &delimiters,
            inline_footnote_depth,
        );
        while subject.parse_inline(root, seed.line, seed.offsets)? {}
        subject.process_emphasis(0)?;
        subject.clear_brackets()?;
        subject.release_owned()?;
        let owners = capture(leaf, seed.raw, root, &mut child)?;
        child.flush_scan()?;
        // Leaving this lexical scope destroys the self-referencing arenas in
        // place, then child. Neither an arena move nor detach/clear/reset-len
        // is used as evidence of actual freeing.
        owners
    };
    parent.event()?;
    Ok(super::ShadowReleased { leaf, owners })
}

#[derive(Clone, Copy)]
struct OpenOwner {
    node: NativeNodeId,
    ordinal: usize,
    end: usize,
}

fn stack_top<H: Host>(
    stack: &NativeRows<OpenOwner, H::Lease>,
    depth: usize,
    ctx: &mut crate::native::Context<'_, H>,
) -> Result<Option<OpenOwner>, H::Error> {
    let Some(index) = depth.checked_sub(1) else {
        return Ok(None);
    };
    charge_host(ctx.host, std::mem::size_of::<OpenOwner>())?;
    stack
        .get(index, |bytes| charge_host(ctx.host, bytes))?
        .copied()
        .map(Some)
        .ok_or_else(|| ctx.host.reject(NativeViolation::InvalidProvenance))
}

fn capture<'leaf, H: Host>(
    leaf: super::LeafBrand<'leaf>,
    raw: &str,
    root: Node<'_>,
    ctx: &mut crate::native::Context<'_, H>,
) -> Result<NativeRows<ShadowOwner<'leaf>, H::Lease>, H::Error> {
    let mut owners: NativeRows<ShadowOwner<'leaf>, H::Lease> = NativeRows::default();
    let mut stack = NativeRows::default();
    let mut depth = 0usize;
    let mut previous = None;
    for edge in root.traverse() {
        ctx.event()?;
        let (enter, node) = match edge {
            NodeEdge::Start(n) => (true, n),
            NodeEdge::End(n) => (false, n),
        };
        let node = node
            .data()
            .native_id
            .ok_or_else(|| ctx.host.reject(NativeViolation::InvalidProvenance))?;
        let accepted = ctx
            .shadow
            .as_ref()
            .ok_or_else(|| ctx.host.reject(NativeViolation::InvalidProvenance))?
            .owner(node, ctx.host)?;
        let Some(accepted) = accepted else { continue };
        let top = stack_top(&stack, depth, ctx)?;
        if enter {
            let ordinal = owners.len();
            let row = owner_row(leaf, accepted, top.map(|p| p.ordinal), ordinal, ctx.host)?;
            ctx.scan(2)?;
            if !raw.is_char_boundary(row.start)
                || !raw.is_char_boundary(row.end)
                || top.is_some_and(|p| row.end > p.end)
                || previous.is_some_and(|p| p >= row.opening_slot)
            {
                return Err(ctx.host.reject(NativeViolation::InvalidProvenance));
            }
            previous = Some(row.opening_slot);
            ctx.host.admit(crate::native::NativeAdmission::Record(1))?;
            charge_host(ctx.host, std::mem::size_of::<ShadowOwner<'_>>())?;
            owners.push(row, ctx.host)?;
            let open = OpenOwner {
                node,
                ordinal,
                end: row.end,
            };
            ctx.host.admit(crate::native::NativeAdmission::Record(1))?;
            charge_host(ctx.host, std::mem::size_of::<OpenOwner>())?;
            if depth == stack.len() {
                stack.push(open, ctx.host)?;
            } else {
                *stack
                    .get_mut(depth, |bytes| charge_host(ctx.host, bytes))?
                    .ok_or_else(|| ctx.host.reject(NativeViolation::InvalidProvenance))? = open;
            }
            depth = depth
                .checked_add(1)
                .ok_or_else(|| ctx.host.reject(NativeViolation::ArithmeticOverflow))?;
        } else {
            let top = top
                .filter(|p| p.node == node)
                .ok_or_else(|| ctx.host.reject(NativeViolation::InvalidProvenance))?;
            let end = owners.len();
            charge_host(ctx.host, std::mem::size_of::<ShadowOwner<'_>>())?;
            owners
                .get_mut(top.ordinal, |bytes| charge_host(ctx.host, bytes))?
                .ok_or_else(|| ctx.host.reject(NativeViolation::InvalidProvenance))?
                .subtree_end = end;
            depth = depth
                .checked_sub(1)
                .ok_or_else(|| ctx.host.reject(NativeViolation::InvalidProvenance))?;
        }
    }
    if depth != 0 {
        return Err(ctx.host.reject(NativeViolation::InvalidProvenance));
    }
    Ok(owners)
}

#[derive(Clone, Copy)]
pub(in crate::native) struct AcceptedOwner {
    kind: OwnerKind,
    span: RSpan,
}

pub(in crate::native) struct ShadowState<'defs, L> {
    definitions: &'defs NativeRows<DefinitionRecord, L>,
    owners: NativeRows<Option<AcceptedOwner>, L>,
}

impl<'defs, L> ShadowState<'defs, L> {
    pub(in crate::native) fn definitions(&self) -> &'defs NativeRows<DefinitionRecord, L> {
        self.definitions
    }

    pub(in crate::native) fn new(definitions: &'defs NativeRows<DefinitionRecord, L>) -> Self {
        Self {
            definitions,
            owners: NativeRows::default(),
        }
    }

    pub(crate) fn node<H: Host<Lease = L>>(
        &mut self,
        node: NativeNodeId,
        host: &mut H,
    ) -> Result<(), H::Error> {
        if node.get() as usize != self.owners.len() + 1 {
            return Err(host.reject(NativeViolation::InvalidProvenance));
        }
        host.admit(crate::native::NativeAdmission::Record(1))?;
        charge_host(host, std::mem::size_of::<Option<AcceptedOwner>>())?;
        self.owners.push(None, host)
    }

    pub(crate) fn fact<H: Host<Lease = L>>(
        &mut self,
        fact: AcceptedFact<'_>,
        host: &mut H,
    ) -> Result<(), H::Error> {
        let (node, owner) = match fact {
            AcceptedFact::Code { node, syntax, .. }
            | AcceptedFact::HtmlCode { node, syntax, .. } => (
                node,
                AcceptedOwner {
                    kind: OwnerKind::Code,
                    span: syntax,
                },
            ),
            AcceptedFact::Math { node, syntax, .. } => (
                node,
                AcceptedOwner {
                    kind: OwnerKind::Math,
                    span: syntax,
                },
            ),
            AcceptedFact::FootnoteReference { node, syntax, .. } => (
                node,
                AcceptedOwner {
                    kind: OwnerKind::Footnote,
                    span: syntax,
                },
            ),
            AcceptedFact::InlineFootnote { node, body, .. } => {
                let start = body
                    .start
                    .checked_sub(2)
                    .ok_or_else(|| host.reject(NativeViolation::InvalidProvenance))?;
                let end = body
                    .end
                    .checked_add(1)
                    .ok_or_else(|| host.reject(NativeViolation::ArithmeticOverflow))?;
                (
                    node,
                    AcceptedOwner {
                        kind: OwnerKind::Footnote,
                        span: RSpan { start, end, ..body },
                    },
                )
            }
            AcceptedFact::RawHtml {
                node,
                span,
                block: false,
            } => (
                node,
                AcceptedOwner {
                    kind: OwnerKind::Html,
                    span,
                },
            ),
            AcceptedFact::Link {
                node,
                syntax,
                image,
                ..
            } => (
                node,
                AcceptedOwner {
                    kind: if image {
                        OwnerKind::Image
                    } else {
                        OwnerKind::Link
                    },
                    span: syntax,
                },
            ),
            _ => return Ok(()),
        };
        self.record_owner(node, owner, host)
    }

    pub(crate) fn delimited<H: Host<Lease = L>>(
        &mut self,
        node: NativeNodeId,
        span: RSpan,
        marker: u8,
        width: u8,
        host: &mut H,
    ) -> Result<(), H::Error> {
        self.record_owner(
            node,
            AcceptedOwner {
                kind: OwnerKind::Delimited { marker, width },
                span,
            },
            host,
        )
    }

    fn record_owner<H: Host<Lease = L>>(
        &mut self,
        node: NativeNodeId,
        owner: AcceptedOwner,
        host: &mut H,
    ) -> Result<(), H::Error> {
        charge_host(host, std::mem::size_of::<Option<AcceptedOwner>>())?;
        let row = self
            .owners
            .get_mut(node.get() as usize - 1, |bytes| charge_host(host, bytes))?
            .ok_or_else(|| host.reject(NativeViolation::InvalidProvenance))?;
        if row.is_some() {
            return Err(host.reject(NativeViolation::InvalidProvenance));
        }
        *row = Some(owner);
        Ok(())
    }

    pub(crate) fn owner<H: Host<Lease = L>>(
        &self,
        node: NativeNodeId,
        host: &mut H,
    ) -> Result<Option<AcceptedOwner>, H::Error> {
        charge_host(host, std::mem::size_of::<Option<AcceptedOwner>>())?;
        self.owners
            .get(node.get() as usize - 1, |bytes| charge_host(host, bytes))?
            .copied()
            .ok_or_else(|| host.reject(NativeViolation::InvalidProvenance))
    }

    /// Borrow the parent's sole normalized key, independently of this child.
    /// The lifetime is the immutable parent store, not the shadow owner ledger.
    pub(in crate::native) fn definition_key<H: Host<Lease = L>>(
        &self,
        id: DefinitionId,
        host: &mut H,
    ) -> Result<&'defs str, H::Error> {
        let entry = self
            .definitions
            .get(id.get() as usize - 1, |bytes| charge_host(host, bytes))?
            .ok_or_else(|| host.reject(NativeViolation::InvalidProvenance))?;
        if !entry.winner || entry.metadata.id != id {
            return Err(host.reject(NativeViolation::InvalidProvenance));
        }
        Ok(entry.key.as_str())
    }

    /// Reads the same immutable winner row. No payload clone, ReferenceBytes,
    /// legacy reference fuel, or second normalized-label index is involved.
    pub(crate) fn winner<H: Host<Lease = L>>(
        &self,
        id: DefinitionId,
        host: &mut H,
    ) -> Result<crate::native::ReferenceIdentity, H::Error> {
        host.work(crate::native::WorkDelta::Events(1))?;
        charge_host(host, std::mem::size_of::<DefinitionRecord>())?;
        let entry = self
            .definitions
            .get(id.get() as usize - 1, |bytes| charge_host(host, bytes))?
            .ok_or_else(|| host.reject(NativeViolation::InvalidProvenance))?;
        let identity = entry
            .value
            .native
            .ok_or_else(|| host.reject(NativeViolation::InvalidProvenance))?;
        if !entry.winner || entry.metadata.id != id || identity.definition != id {
            return Err(host.reject(NativeViolation::InvalidProvenance));
        }
        Ok(identity)
    }
}

/// Checked numeric transport after actual acceptance. No source token is made.
fn owner_row<'leaf, H: Host>(
    leaf: super::LeafBrand<'leaf>,
    accepted: AcceptedOwner,
    parent: Option<usize>,
    ordinal: usize,
    host: &mut H,
) -> Result<ShadowOwner<'leaf>, H::Error> {
    let start = accepted
        .span
        .start
        .checked_sub(leaf.base)
        .ok_or_else(|| host.reject(NativeViolation::InvalidProvenance))?;
    let end = accepted
        .span
        .end
        .checked_sub(leaf.base)
        .ok_or_else(|| host.reject(NativeViolation::InvalidProvenance))?;
    if accepted.span.buffer != leaf.buffer || start >= end || end > leaf.bytes {
        return Err(host.reject(NativeViolation::InvalidProvenance));
    }
    let opening_slot = u64::try_from(start)
        .ok()
        .and_then(|s| s.checked_add(1))
        .ok_or_else(|| host.reject(NativeViolation::ArithmeticOverflow))?;
    Ok(ShadowOwner {
        leaf,
        kind: accepted.kind,
        opening_slot,
        start,
        end,
        parent,
        subtree_end: ordinal
            .checked_add(1)
            .ok_or_else(|| host.reject(NativeViolation::ArithmeticOverflow))?,
    })
}
