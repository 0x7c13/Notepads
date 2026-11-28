// Copyright 2018 the Resvg Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT
// Modifications Copyright (c) 2026, Jiaqi (0x7c13) Liu.
// Experimental request-owned error propagation; original license terms retained.

use crate::tree::owner::TreeString;
use std::str::FromStr;

use super::converter;
use super::svgtree::{AId, EId, SvgNode};
use crate::{ClipPath, Group, NonEmptyString, NonZeroRect, Transform, Units};

pub(crate) fn convert(
    node: SvgNode,
    state: &converter::State,
    object_bbox: Option<NonZeroRect>,
    cache: &mut converter::Cache,
) -> Option<crate::SharedOwner<ClipPath>> {
    cache.with_reference(node, state, |state, cache| {
        convert_inner(node, state, object_bbox, cache)
    })
}

fn convert_inner(
    node: SvgNode,
    state: &converter::State,
    object_bbox: Option<NonZeroRect>,
    cache: &mut converter::Cache,
) -> Option<crate::SharedOwner<ClipPath>> {
    if state.opt.processing.failed() {
        return None;
    }
    // A `clip-path` attribute must reference a `clipPath` element.
    if node.tag_name() != Some(EId::ClipPath) {
        return None;
    }

    // The whole clip path should be ignored when a transform is invalid.
    let mut transform = resolve_clip_path_transform(node, state)?;

    let units = node
        .attribute(AId::ClipPathUnits)
        .unwrap_or(Units::UserSpaceOnUse);

    // Check if this element was already converted.
    //
    // Only `userSpaceOnUse` clipPaths can be shared,
    // because `objectBoundingBox` one will be converted into user one
    // and will become node-specific.
    let cacheable = units == Units::UserSpaceOnUse;
    if cacheable {
        if let Some((clip, height)) = cache
            .clip_paths
            .get(node.element_id(), &state.opt.processing)
            .ok()?
        {
            let peak = state
                .refs
                .check_height(height, state.opt.processing.resources())
                .ok()?;
            cache
                .references
                .note(peak, state.opt.processing.resources())
                .ok()?;
            let value = clip.try_alias_in(&state.opt.processing).ok()?;
            return Some(value);
        }
    }

    if units == Units::ObjectBoundingBox {
        let object_bbox = match object_bbox {
            Some(v) => v,
            None => {
                log::warn!("Clipping of zero-sized shapes is not allowed.");
                return None;
            }
        };

        let ts = Transform::from_bbox(object_bbox);
        transform = transform.pre_concat(ts);
    }

    // Resolve linked clip path.
    let mut clip_path = None;
    if let Some(link) = node.attribute::<SvgNode>(AId::ClipPath) {
        clip_path = convert(link, state, object_bbox, cache);
        if state.opt.processing.failed() {
            return None;
        }

        // Linked `clipPath` must be valid.
        if clip_path.is_none() {
            return None;
        }
    }

    let mut id = NonEmptyString::try_new(node.element_id(), &state.opt.processing).ok()??;
    // Generate ID only when we're parsing `objectBoundingBox` clip for the second time.
    if !cacheable
        && cache
            .clip_paths
            .contains_key(id.get(), &state.opt.processing)
            .ok()?
    {
        id = cache.gen_clip_path_id().ok()?;
    }
    let id_copy = TreeString::try_from_str(id.get(), &state.opt.processing).ok()?;

    let mut clip = ClipPath {
        id,
        transform,
        clip_path,
        root: crate::scene::root(&state.opt.processing, || {
            Group::empty_in(&state.opt.processing)
        })
        .ok()?,
    };

    let mut clip_state = state.try_clone().ok()?;
    clip_state.parent_clip_path = Some(node);
    converter::convert_clip_path_elements(node, &clip_state, cache, &mut clip.root);
    if state.opt.processing.failed() {
        return None;
    }

    if clip.root.has_children() {
        clip.root.calculate_bounding_boxes();
        let clip = crate::SharedOwner::try_new(
            clip,
            state.opt.processing.resources(),
            markdown_svg_budget::ResourceKind::Scratch,
        )
        .ok()?;
        cache
            .clip_paths
            .insert(
                &id_copy,
                clip.try_alias_in(&state.opt.processing).ok()?,
                &state.opt.processing,
                cache.references.height(),
            )
            .ok()?;
        Some(clip)
    } else {
        // A clip path without children is invalid.
        None
    }
}

fn resolve_clip_path_transform(node: SvgNode, state: &converter::State) -> Option<Transform> {
    // Do not use Node::attribute::<Transform>, because it will always
    // return a valid transform.

    let value: &str = match node.attribute(AId::Transform) {
        Some(v) => v,
        None => return Some(Transform::default()),
    };

    let ts = match svgtypes::Transform::from_str(value) {
        Ok(v) => v,
        Err(_) => {
            log::warn!("Failed to parse {} value: '{}'.", AId::Transform, value);
            return None;
        }
    };

    let ts = Transform::from_row(
        ts.a as f32,
        ts.b as f32,
        ts.c as f32,
        ts.d as f32,
        ts.e as f32,
        ts.f as f32,
    );

    if ts.is_valid() {
        Some(node.resolve_transform(AId::Transform, state))
    } else {
        None
    }
}
