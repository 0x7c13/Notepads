// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

//! Move an already accepted D-run once, preserving original decoder ancestors.
//! Numeric email matches are not allowed to mint or reconstruct source facts.

use super::*;

pub(crate) struct EmailAtoms {
    value: MappedValue,
    index: usize,
    offset: usize,
    position: usize,
}

pub(crate) struct EmailFragment {
    value: MappedValue,
}

impl EmailAtoms {
    pub(crate) fn take<H: Host>(
        &mut self,
        end: usize,
        ctx: &mut Context<'_, H>,
    ) -> Result<EmailFragment, H::Error> {
        ctx.require_grammar()?;
        let ctx = &mut ctx.mapping_access();
        if end < self.position || end > self.value.bytes {
            return Err(ctx.host.reject(NativeViolation::InvalidProvenance));
        }
        let mut value = MappedValue::default();
        while let Some(atom) = self.value.atoms.get(self.index) {
            ctx.event()?;
            ctx.scan(std::mem::size_of::<Atom>())?;
            let atom = *atom;
            if atom.bytes == 0 {
                // Zero-output facts belong to the following literal fragment;
                // enabled email label atoms must all contribute actual bytes.
                if self.position >= end && end != self.value.bytes {
                    break;
                }
                value.atoms.push(atom, ctx)?;
                self.index += 1;
                continue;
            }
            if self.position == end {
                break;
            }
            let count = (end - self.position).min(atom.bytes - self.offset);
            if count == 0 || atom.hidden_syntax {
                return Err(ctx.host.reject(NativeViolation::InvalidProvenance));
            }
            let mut part = atom;
            if self.offset != 0 || count != atom.bytes {
                if atom.proof.is_some()
                    || atom.source.end.checked_sub(atom.source.start) != Some(atom.bytes)
                {
                    return Err(ctx.host.reject(NativeViolation::InvalidProvenance));
                }
                part.source.start = atom
                    .source
                    .start
                    .checked_add(self.offset)
                    .ok_or_else(|| ctx.host.reject(NativeViolation::ArithmeticOverflow))?;
                part.source.end = part
                    .source
                    .start
                    .checked_add(count)
                    .ok_or_else(|| ctx.host.reject(NativeViolation::ArithmeticOverflow))?;
                part.bytes = count;
            }
            value.atoms.push(part, ctx)?;
            value.bytes = value
                .bytes
                .checked_add(count)
                .ok_or_else(|| ctx.host.reject(NativeViolation::ArithmeticOverflow))?;
            self.position = self
                .position
                .checked_add(count)
                .ok_or_else(|| ctx.host.reject(NativeViolation::ArithmeticOverflow))?;
            self.offset += count;
            if self.offset == atom.bytes {
                self.index += 1;
                self.offset = 0;
            }
        }
        if self.position != end {
            return Err(ctx.host.reject(NativeViolation::InvalidProvenance));
        }
        Ok(EmailFragment { value })
    }

    pub(crate) fn finish<H: Host>(self, ctx: &mut Context<'_, H>) -> Result<(), H::Error> {
        ctx.require_grammar()?;
        let ctx = &mut ctx.mapping_access();
        if self.position != self.value.bytes
            || self.index != self.value.atoms.len()
            || self.offset != 0
        {
            return Err(ctx.host.reject(NativeViolation::InvalidProvenance));
        }
        self.value.atoms.discard(ctx)
    }
}

impl EmailFragment {
    /// Verify the selected ordered atom union before representing it as R.
    /// R-to-P transport may still contain container holes; no P envelope is used.
    pub(crate) fn source<H: Host>(&self, ctx: &mut Context<'_, H>) -> Result<RSpan, H::Error> {
        ctx.require_grammar()?;
        self.source_access(&mut ctx.mapping_access())
    }

    fn source_access<H: Host>(&self, ctx: &mut MappingAccess<'_, H>) -> Result<RSpan, H::Error> {
        let mut result: Option<RSpan> = None;
        let mut bytes = 0usize;
        for atom in &self.value.atoms {
            ctx.event()?;
            ctx.scan(std::mem::size_of::<Atom>())?;
            if atom.bytes == 0 || atom.hidden_syntax || atom.source.start >= atom.source.end {
                return Err(ctx.host.reject(NativeViolation::InvalidProvenance));
            }
            bytes = bytes
                .checked_add(atom.bytes)
                .ok_or_else(|| ctx.host.reject(NativeViolation::ArithmeticOverflow))?;
            match &mut result {
                Some(r) if r.buffer == atom.source.buffer && r.end == atom.source.start => {
                    r.end = atom.source.end
                }
                None => result = Some(atom.source),
                _ => return Err(ctx.host.reject(NativeViolation::InvalidProvenance)),
            }
        }
        if bytes != self.value.bytes {
            return Err(ctx.host.reject(NativeViolation::InvalidProvenance));
        }
        result.ok_or_else(|| ctx.host.reject(NativeViolation::InvalidProvenance))
    }
}

impl<H: Host> Context<'_, H> {
    fn email_mutation<T>(
        &mut self,
        body: impl FnOnce(&mut Mappings<H::Lease>, &mut MappingAccess<'_, H>) -> Result<T, H::Error>,
    ) -> Result<T, H::Error> {
        self.require_grammar()?;
        if !self.direct
            || self.is_shadow()
            || self.inline_policy != super::super::InlinePolicy::StrictG4
        {
            return Err(self.host.reject(NativeViolation::InvalidProvenance));
        }
        self.event()?;
        self.mutate_mappings(body)
    }

    pub(crate) fn email_atoms(
        &mut self,
        node: NativeNodeId,
        bytes: usize,
    ) -> Result<EmailAtoms, H::Error> {
        self.email_mutation(|mappings, ctx| {
            let value = mappings.value_mut(node, ctx)?;
            if value.bytes != bytes {
                return Err(ctx.host.reject(NativeViolation::InvalidProvenance));
            }
            Ok(EmailAtoms {
                value: std::mem::take(value),
                index: 0,
                offset: 0,
                position: 0,
            })
        })
    }

    pub(crate) fn email_bind_literal(
        &mut self,
        node: NativeNodeId,
        fragment: EmailFragment,
    ) -> Result<(), H::Error> {
        self.email_mutation(|mappings, ctx| {
            let slot = mappings.value_mut(node, ctx)?;
            if slot.bytes != 0 || !slot.atoms.is_empty() {
                return Err(ctx.host.reject(NativeViolation::InvalidProvenance));
            }
            *slot = fragment.value;
            Ok(())
        })
    }

    pub(crate) fn email_bind_label(
        &mut self,
        node: NativeNodeId,
        payload: PayloadId,
        fragment: EmailFragment,
    ) -> Result<(), H::Error> {
        self.email_mutation(|mappings, ctx| {
            let raw = fragment.source_access(ctx)?;
            let mut destination = MappedValue::default();
            for atom in &fragment.value.atoms {
                destination.atoms.push(*atom, ctx)?;
            }
            destination.bytes = fragment.value.bytes;
            let row = mappings
                .payloads
                .get_mut(payload.get() as usize - 1, |n| charge_host(ctx.host, n))?
                .ok_or_else(|| ctx.host.reject(NativeViolation::InvalidProvenance))?;
            if row.role != PayloadKind::Destination
                || row.raw != raw
                || row.stage != 0
                || row.prefix.is_some()
            {
                return Err(ctx.host.reject(NativeViolation::InvalidProvenance));
            }
            let old = std::mem::replace(&mut row.value, destination);
            row.stage_len = row.value.bytes;
            old.atoms.discard(ctx)?;
            let slot = mappings.value_mut(node, ctx)?;
            if slot.bytes != 0 || !slot.atoms.is_empty() {
                return Err(ctx.host.reject(NativeViolation::InvalidProvenance));
            }
            *slot = fragment.value;
            Ok(())
        })
    }
}
