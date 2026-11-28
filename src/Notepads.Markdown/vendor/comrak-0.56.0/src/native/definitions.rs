// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

//! Actual ownership of every accepted reference definition. RefMap owns only
//! definition IDs on the direct path; loser and unused identities live here.

use super::value::{AcceptedValueRef, ValueStorage, ValueText};
use super::{Context, DefinitionId, Host, NativeViolation, PayloadId, PayloadKind, RSpan};
use crate::parser::ResolvedReference;

#[derive(Clone, Copy, Debug)]
pub(super) struct PendingDefinition {
    pub id: DefinitionId,
    pub syntax: RSpan,
    pub label: RSpan,
}

#[derive(Debug)]
pub(super) struct DefinitionRecord {
    pub metadata: PendingDefinition,
    pub key: String,
    pub value: ResolvedReference,
    pub winner: bool,
}

pub(super) fn mark_retained<H: Host>(
    definitions: &super::NativeRows<DefinitionRecord, H::Lease>,
    registry: &mut super::allocation::Registry<H::Lease>,
    host: &mut H,
) -> Result<(), H::Error> {
    definitions.visit(host, |entry, host| {
        let DefinitionRecord {
            metadata:
                PendingDefinition {
                    id: _,
                    syntax: _,
                    label: _,
                },
            key,
            value,
            winner: _,
        } = entry;
        validate_value(value, host)?;
        registry.mark_string(key, host)
    })
}

fn matches_payload(value: &ValueText, payload: Option<PayloadId>, role: PayloadKind) -> bool {
    match (&value.repr, payload) {
        (ValueStorage::Empty, None) => true,
        (
            ValueStorage::Accepted(AcceptedValueRef::Payload {
                payload: actual,
                role: actual_role,
            }),
            Some(expected),
        ) => *actual == expected && *actual_role == role,
        _ => false,
    }
}

fn validate_value<H: Host>(value: &ResolvedReference, host: &mut H) -> Result<(), H::Error> {
    let identity = value
        .native
        .ok_or_else(|| host.reject(NativeViolation::InvalidProvenance))?;
    if !matches_payload(
        &value.url,
        Some(identity.destination),
        PayloadKind::Destination,
    ) || !matches_payload(&value.title, identity.title, PayloadKind::Title)
    {
        return Err(host.reject(NativeViolation::InvalidProvenance));
    }
    Ok(())
}

impl<H: Host> Context<'_, H> {
    /// Moves the original grammar-owned key and cleaned values, before RefMap
    /// can discard a losing insertion. No normalized text is reconstructed.
    pub(crate) fn preserve_definition(
        &mut self,
        key: String,
        value: ResolvedReference,
        winner: bool,
    ) -> Result<DefinitionId, H::Error> {
        let metadata = self
            .pending_definition
            .take()
            .ok_or_else(|| self.host.reject(NativeViolation::InvalidProvenance))?;
        let identity = value
            .native
            .ok_or_else(|| self.host.reject(NativeViolation::InvalidProvenance))?;
        if identity.definition != metadata.id
            || metadata.id.get() as usize != self.definitions.len() + 1
        {
            return Err(self.host.reject(NativeViolation::InvalidProvenance));
        }
        validate_value(&value, self.host)?;
        let id = metadata.id;
        let mut definitions = std::mem::take(&mut self.definitions);
        let result = definitions.push(
            DefinitionRecord {
                metadata,
                key,
                value,
                winner,
            },
            self.host,
        );
        // Restore the actual owner even when growth/admission fails, before
        // the caller unwinds. The Context drops buffers before ledger leases.
        self.definitions = definitions;
        result?;
        Ok(id)
    }

    /// Borrow the sole actual normalized key. The owner is restored before
    /// every ordinary return, including an error from lookup/hash work.
    pub(crate) fn with_definition_key<T>(
        &mut self,
        id: DefinitionId,
        body: impl FnOnce(&str, &mut Self) -> Result<T, H::Error>,
    ) -> Result<T, H::Error> {
        self.require_grammar()?;
        if let Some(shadow) = &self.shadow {
            let key = shadow.definition_key(id, self.host)?;
            return body(key, self);
        }
        let definitions = std::mem::take(&mut self.definitions);
        let result = (|| {
            let entry = definitions
                .get(id.get() as usize - 1, |bytes| {
                    super::rows::charge_host(self.host, bytes)
                })?
                .ok_or_else(|| self.host.reject(NativeViolation::InvalidProvenance))?;
            if entry.metadata.id != id || !entry.winner {
                return Err(self.host.reject(NativeViolation::InvalidProvenance));
            }
            body(&entry.key, self)
        })();
        self.definitions = definitions;
        result
    }

    pub(crate) fn definition_bytes(&mut self, id: DefinitionId) -> Result<usize, H::Error> {
        self.event()?;
        let definitions = std::mem::take(&mut self.definitions);
        let result = (|| {
            let entry = definitions
                .get(id.get() as usize - 1, |bytes| {
                    super::rows::charge_host(self.host, bytes)
                })?
                .ok_or_else(|| self.host.reject(NativeViolation::InvalidProvenance))?;
            let url = self.value_len(&entry.value.url)?;
            let title = self.value_len(&entry.value.title)?;
            url.checked_add(title)
                .ok_or_else(|| self.host.reject(NativeViolation::ArithmeticOverflow))
        })();
        self.definitions = definitions;
        result
    }

    /// Each use keeps its original semantic admissions, in URL/title order.
    /// Actual accepted payload identities are shared; no string is copied.
    pub(crate) fn materialize_definition(
        &mut self,
        id: DefinitionId,
    ) -> Result<ResolvedReference, H::Error> {
        let definitions = std::mem::take(&mut self.definitions);
        let result = (|| {
            let entry = definitions
                .get(id.get() as usize - 1, |bytes| {
                    super::rows::charge_host(self.host, bytes)
                })?
                .ok_or_else(|| self.host.reject(NativeViolation::InvalidProvenance))?;
            if !entry.winner || entry.metadata.id != id {
                return Err(self.host.reject(NativeViolation::InvalidProvenance));
            }
            validate_value(&entry.value, self.host)?;
            let url = self.share_reference_value(&entry.value.url)?;
            let title = self.share_reference_value(&entry.value.title)?;
            Ok(ResolvedReference {
                url,
                title,
                native: entry.value.native,
            })
        })();
        self.definitions = definitions;
        result
    }
}
