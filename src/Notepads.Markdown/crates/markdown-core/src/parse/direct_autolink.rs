// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

//! Consume actual native extended-link acceptance; no lexical reconstruction.

use super::*;

pub(super) fn syntax<'view, 'input, 'id>(
    native: &Native<'_, 'view, 'input, '_, 'id>,
    graph: &graph::Graph<'view, 'input, 'id>,
    builder: &b::DirectBuilder<'id>,
    info: &capture::NodeRecord<'view, 'input>,
    work: &mut OwnedWork<'id, '_>,
) -> Result<Option<n::SourceToken<'view, 'input>>, CoreError> {
    if info.kind != native::NativeKind::Link {
        return Ok(None);
    }
    let token = native.read(work, |access| access.extended_for_lineage(info.lineage))?;
    let Some(token) = token else { return Ok(None) };
    let accepted = native.read(work, |access| access.extended(token))?;
    let owner = graph
        .member_for_lineage(info.lineage, native, work)?
        .ok_or_else(|| work.fail(CoreError::InvalidProvenance))?;
    let label = graph
        .member_for_lineage(accepted.label_lineage, native, work)?
        .ok_or_else(|| work.fail(CoreError::InvalidProvenance))?;
    let owner_member = graph.nodes.get(owner, work)?;
    let label_member = graph.nodes.get(label, work)?;
    let owner_node = builder.graph_node(info.node, work)?;
    let label_node = builder.graph_node(label_member.node, work)?;
    if owner_member.node != info.node
        || owner.checked_add(1) != Some(label)
        || label.checked_add(1) != Some(owner_member.end())
        || label_node.parent != Some(owner_node.index)
        || label_node.kind != NodeKind::Text
    {
        return Err(work.fail(CoreError::InvalidProvenance));
    }
    let prefix = native.read(work, |access| access.mapping_prefix(accepted.destination))?;
    let expected = match accepted.kind {
        native::g4::ExtendedKind::Www => Some(native::GeneratedPrefix::Http),
        native::g4::ExtendedKind::Url => None,
        native::g4::ExtendedKind::Email => Some(native::GeneratedPrefix::Mailto),
    };
    if prefix.map(|p| p.kind) != expected {
        return Err(work.fail(CoreError::InvalidProvenance));
    }
    // The native record is issued only after checking the real label's atoms,
    // destination mapping and exact syntax union. Core reads these real mapped
    // values separately, and its independent validator still grants authority.
    Ok(Some(accepted.syntax))
}
