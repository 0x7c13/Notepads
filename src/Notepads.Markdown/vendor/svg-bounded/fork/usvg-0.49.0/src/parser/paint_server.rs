// Copyright 2018 the Resvg Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT
// Modifications Copyright (c) 2026, Jiaqi (0x7c13) Liu.
// Experimental request-owned error propagation; original license terms retained.

use crate::tree::owner::{PolicyValue, TreeVec, TryCopy};
use std::str::FromStr;

use strict_num::PositiveF32;
use svgtypes::{Length, LengthUnit as Unit};

use super::OptionLog;
use super::converter::{self, Cache, SvgColorExt};
use super::svgtree::{AId, EId, SvgNode};
use crate::*;

pub(crate) enum ServerOrColor {
    Server(Paint),
    Color { color: Color, opacity: Opacity },
}

pub(crate) fn convert(
    node: SvgNode,
    state: &converter::State,
    cache: &mut converter::Cache,
) -> Option<ServerOrColor> {
    cache.with_reference(node, state, |state, cache| {
        convert_inner(node, state, cache)
    })
}

fn convert_inner(
    node: SvgNode,
    state: &converter::State,
    cache: &mut converter::Cache,
) -> Option<ServerOrColor> {
    if state.opt.processing.failed() {
        return None;
    }
    // Check for existing.
    if let Some((paint, height)) = cache
        .paint
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
        let paint = paint.try_copy_in(&state.opt.processing).ok()?;
        cache.last_paint_proof = super::reference::PaintProof {
            root: node.reference_id(),
            span: 1 + height,
        };
        return Some(ServerOrColor::Server(paint));
    }

    if !validate_inheritance(node, state, cache) {
        return None;
    }

    // Unwrap is safe, because we already checked for is_paint_server().
    let paint = match node.tag_name().unwrap() {
        EId::LinearGradient => convert_linear(node, state),
        EId::RadialGradient => convert_radial(node, state),
        EId::Pattern => convert_pattern(node, state, cache),
        _ => unreachable!(),
    };

    if state.opt.processing.failed() {
        return None;
    }

    if let Some(ServerOrColor::Server(paint)) = &paint {
        cache
            .paint
            .insert(
                node.element_id(),
                paint.try_copy_in(&state.opt.processing).ok()?,
                &state.opt.processing,
                cache.references.height(),
            )
            .ok()?;
    }

    cache.last_paint_proof = super::reference::PaintProof {
        root: node.reference_id(),
        span: 1 + cache.references.height(),
    };
    paint
}

pub(super) fn validate_inheritance(
    node: SvgNode,
    state: &converter::State,
    cache: &mut converter::Cache,
) -> bool {
    if state.opt.processing.resources().is_none() {
        return true;
    }
    let links = match node.href_iter_in(&state.refs) {
        Ok(v) => v,
        Err(e) => {
            state.opt.processing.record_resource(e);
            return false;
        }
    };
    for target in links {
        let target = match target {
            Ok(v) => v,
            Err(e) => {
                state.opt.processing.record_resource(e);
                return false;
            }
        };
        if !cache.note_reference(&target.path, state) {
            return false;
        }
    }
    !state.opt.processing.failed()
}

#[inline(never)]
fn convert_linear(node: SvgNode, state: &converter::State) -> Option<ServerOrColor> {
    let id = NonEmptyString::try_new(node.element_id(), &state.opt.processing).ok()??;

    let stops = convert_stops(find_gradient_with_stops(node)?, &state.opt.processing).ok()?;
    if stops.len() < 2 {
        return stops_to_color(&stops);
    }

    let units = convert_units(node, AId::GradientUnits, Units::ObjectBoundingBox);
    let transform =
        resolve_attr(node, AId::GradientTransform).resolve_transform(AId::GradientTransform, state);

    let gradient = LinearGradient {
        x1: resolve_number(node, AId::X1, units, state, Length::zero()),
        y1: resolve_number(node, AId::Y1, units, state, Length::zero()),
        x2: resolve_number(
            node,
            AId::X2,
            units,
            state,
            Length::new(100.0, Unit::Percent),
        ),
        y2: resolve_number(node, AId::Y2, units, state, Length::zero()),
        base: BaseGradient {
            id,
            units,
            transform,
            spread_method: convert_spread_method(node),
            stops,
        },
    };

    Some(ServerOrColor::Server(Paint::LinearGradient(
        crate::SharedOwner::try_new(
            gradient,
            state.opt.processing.resources(),
            markdown_svg_budget::ResourceKind::Scratch,
        )
        .ok()?,
    )))
}

#[inline(never)]
fn convert_radial(node: SvgNode, state: &converter::State) -> Option<ServerOrColor> {
    let id = NonEmptyString::try_new(node.element_id(), &state.opt.processing).ok()??;

    let stops = convert_stops(find_gradient_with_stops(node)?, &state.opt.processing).ok()?;
    if stops.len() < 2 {
        return stops_to_color(&stops);
    }

    let units = convert_units(node, AId::GradientUnits, Units::ObjectBoundingBox);
    let r = resolve_number(node, AId::R, units, state, Length::new(50.0, Unit::Percent));
    let fr = resolve_number(node, AId::Fr, units, state, Length::zero());

    // 'A value of zero will cause the area to be painted as a single color
    // using the color and opacity of the last gradient stop.'
    //
    // https://www.w3.org/TR/SVG11/pservers.html#RadialGradientElementRAttribute
    if !r.is_valid_length() {
        let stop = stops.last().unwrap();
        return Some(ServerOrColor::Color {
            color: stop.color,
            opacity: stop.opacity,
        });
    }

    let spread_method = convert_spread_method(node);
    let cx = resolve_number(
        node,
        AId::Cx,
        units,
        state,
        Length::new(50.0, Unit::Percent),
    );
    let cy = resolve_number(
        node,
        AId::Cy,
        units,
        state,
        Length::new(50.0, Unit::Percent),
    );
    let fx = resolve_number(node, AId::Fx, units, state, Length::new_number(cx as f64));
    let fy = resolve_number(node, AId::Fy, units, state, Length::new_number(cy as f64));
    let transform =
        resolve_attr(node, AId::GradientTransform).resolve_transform(AId::GradientTransform, state);

    let gradient = RadialGradient {
        cx,
        cy,
        r: PositiveF32::new(r).unwrap(),
        fx,
        fy,
        fr: PositiveF32::new(fr).unwrap_or(PositiveF32::ZERO),
        base: BaseGradient {
            id,
            units,
            transform,
            spread_method,
            stops,
        },
    };

    Some(ServerOrColor::Server(Paint::RadialGradient(
        crate::SharedOwner::try_new(
            gradient,
            state.opt.processing.resources(),
            markdown_svg_budget::ResourceKind::Scratch,
        )
        .ok()?,
    )))
}

#[inline(never)]
fn convert_pattern(
    node: SvgNode,
    state: &converter::State,
    cache: &mut converter::Cache,
) -> Option<ServerOrColor> {
    let selected = find_pattern_with_children(node, state, cache)?;
    let node_with_children = selected.node;
    let mut body_state = state.try_clone().ok()?;
    body_state.refs = selected.path;

    let id = NonEmptyString::try_new(node.element_id(), &state.opt.processing).ok()??;

    let view_box = {
        let n1 = resolve_attr(node, AId::ViewBox);
        let n2 = resolve_attr(node, AId::PreserveAspectRatio);
        n1.parse_viewbox().map(|vb| ViewBox {
            rect: vb,
            aspect: n2.attribute(AId::PreserveAspectRatio).unwrap_or_default(),
        })
    };

    let units = convert_units(node, AId::PatternUnits, Units::ObjectBoundingBox);
    let content_units = convert_units(node, AId::PatternContentUnits, Units::UserSpaceOnUse);

    let transform =
        resolve_attr(node, AId::PatternTransform).resolve_transform(AId::PatternTransform, state);

    let rect = NonZeroRect::from_xywh(
        resolve_number(node, AId::X, units, state, Length::zero()),
        resolve_number(node, AId::Y, units, state, Length::zero()),
        resolve_number(node, AId::Width, units, state, Length::zero()),
        resolve_number(node, AId::Height, units, state, Length::zero()),
    );
    let rect = rect.log_none(|| {
        log::warn!(
            "Pattern '{}' has an invalid size. Skipped.",
            node.element_id()
        );
    })?;

    let mut patt = Pattern {
        id,
        units,
        content_units,
        transform,
        rect,
        view_box,
        root: crate::scene::root(&state.opt.processing, || {
            Group::empty_in(&state.opt.processing)
        })
        .ok()?,
    };

    // We can apply viewbox transform only for user space coordinates.
    // Otherwise we need a bounding box, which is unknown at this point.
    if patt.view_box.is_some()
        && patt.units == Units::UserSpaceOnUse
        && patt.content_units == Units::UserSpaceOnUse
    {
        let mut g = Group::empty_in(&state.opt.processing);
        g.transform = view_box.unwrap().to_transform(rect.size());
        g.abs_transform = g.transform;

        converter::convert_children(node_with_children, &body_state, cache, &mut g);
        if state.opt.processing.failed() {
            return None;
        }
        if !g.has_children() {
            return None;
        }

        g.calculate_bounding_boxes();
        let group =
            crate::scene::boxed_owned(&state.opt.processing, crate::scene::SceneCost::NODE, g)
                .ok()?;
        patt.root
            .children
            .try_push(Node::Group(group), &state.opt.processing)
            .ok()?;
    } else {
        converter::convert_children(node_with_children, &body_state, cache, &mut patt.root);
        if state.opt.processing.failed() {
            return None;
        }
        if !patt.root.has_children() {
            return None;
        }
    }

    patt.root.calculate_bounding_boxes();
    update_paint_servers(&mut patt.root, Transform::default(), None, None, cache);
    if state.opt.processing.failed() {
        return None;
    }

    Some(ServerOrColor::Server(Paint::Pattern(
        crate::SharedOwner::try_new(
            patt,
            state.opt.processing.resources(),
            markdown_svg_budget::ResourceKind::Scratch,
        )
        .ok()?,
    )))
}

#[cfg(test)]
#[path = "scene_pattern_tests.rs"]
mod scene_pattern_tests;

fn convert_spread_method(node: SvgNode) -> SpreadMethod {
    let node = resolve_attr(node, AId::SpreadMethod);
    node.attribute(AId::SpreadMethod).unwrap_or_default()
}

pub(crate) fn convert_units(node: SvgNode, name: AId, def: Units) -> Units {
    let node = resolve_attr(node, name);
    node.attribute(name).unwrap_or(def)
}

fn find_gradient_with_stops<'a, 'input: 'a>(
    node: SvgNode<'a, 'input>,
) -> Option<SvgNode<'a, 'input>> {
    for link in node.href_iter() {
        if !link.tag_name().unwrap().is_gradient() {
            log::warn!(
                "Gradient '{}' cannot reference '{}' via 'xlink:href'.",
                node.element_id(),
                link.tag_name().unwrap()
            );
            return None;
        }

        if link.children().any(|n| n.tag_name() == Some(EId::Stop)) {
            return Some(link);
        }
    }

    None
}

fn find_pattern_with_children<'a, 'input: 'a>(
    node: SvgNode<'a, 'input>,
    state: &converter::State,
    cache: &mut converter::Cache,
) -> Option<super::svgtree::RefTarget<'a, 'input>> {
    for target in node.href_iter_in(&state.refs).ok()? {
        let target = target.ok()?;
        if !cache.note_reference(&target.path, state) {
            return None;
        }
        let link = target.node;
        if link.tag_name() != Some(EId::Pattern) {
            log::warn!(
                "Pattern '{}' cannot reference '{}' via 'xlink:href'.",
                node.element_id(),
                link.tag_name().unwrap()
            );
            return None;
        }

        if link.has_children() {
            return Some(target);
        }
    }

    None
}

fn convert_stops(
    grad: SvgNode,
    p: &crate::ProcessingContext,
) -> crate::tree::owner::Result<TreeVec<Stop>> {
    let mut stops = TreeVec::new(p);

    {
        let mut prev_offset = Length::zero();
        for stop in grad.children() {
            crate::tree::owner::check(p, 1)?;
            if stop.tag_name() != Some(EId::Stop) {
                log::warn!("Invalid gradient child: '{:?}'.", stop.tag_name().unwrap());
                continue;
            }

            // `number` can be either a number or a percentage.
            let offset = stop.attribute(AId::Offset).unwrap_or(prev_offset);
            let offset = match offset.unit {
                Unit::None => offset.number,
                Unit::Percent => offset.number / 100.0,
                _ => prev_offset.number,
            };
            prev_offset = Length::new_number(offset);
            let offset = crate::f32_bound(0.0, offset as f32, 1.0);

            let (color, opacity) = match stop.attribute(AId::StopColor) {
                Some("currentColor") => stop
                    .find_attribute(AId::Color)
                    .unwrap_or_else(svgtypes::Color::black),
                Some(value) => {
                    if let Ok(c) = svgtypes::Color::from_str(value) {
                        c
                    } else {
                        log::warn!("Failed to parse stop-color value: '{}'.", value);
                        svgtypes::Color::black()
                    }
                }
                _ => svgtypes::Color::black(),
            }
            .split_alpha();

            let stop_opacity = stop
                .attribute::<Opacity>(AId::StopOpacity)
                .unwrap_or(Opacity::ONE);
            stops.try_push(
                Stop {
                    offset: StopOffset::new_clamped(offset),
                    color,
                    opacity: opacity * stop_opacity,
                },
                p,
            )?;
        }
    }

    // Remove stops with equal offset.
    //
    // Example:
    // offset="0.5"
    // offset="0.7"
    // offset="0.7" <-- this one should be removed
    // offset="0.7"
    // offset="0.9"
    if stops.len() >= 3 {
        let mut i = 0;
        while i < stops.len() - 2 {
            crate::tree::owner::check(p, 1)?;
            let offset1 = stops[i + 0].offset.get();
            let offset2 = stops[i + 1].offset.get();
            let offset3 = stops[i + 2].offset.get();

            if offset1.approx_eq_ulps(&offset2, 4) && offset2.approx_eq_ulps(&offset3, 4) {
                // Remove offset in the middle.
                stops.try_remove(i + 1, p)?;
            } else {
                i += 1;
            }
        }
    }

    // Remove zeros.
    //
    // From:
    // offset="0.0"
    // offset="0.0"
    // offset="0.7"
    //
    // To:
    // offset="0.0"
    // offset="0.00000001"
    // offset="0.7"
    if stops.len() >= 2 {
        let mut i = 0;
        while i < stops.len() - 1 {
            crate::tree::owner::check(p, 1)?;
            let offset1 = stops[i + 0].offset.get();
            let offset2 = stops[i + 1].offset.get();

            if offset1.approx_eq_ulps(&0.0, 4) && offset2.approx_eq_ulps(&0.0, 4) {
                stops[i + 1].offset = StopOffset::new_clamped(offset1 + f32::EPSILON);
            }

            i += 1;
        }
    }

    // Shift equal offsets.
    //
    // From:
    // offset="0.5"
    // offset="0.7"
    // offset="0.7"
    //
    // To:
    // offset="0.5"
    // offset="0.699999999"
    // offset="0.7"
    {
        let mut i = 1;
        while i < stops.len() {
            crate::tree::owner::check(p, 1)?;
            let offset1 = stops[i - 1].offset.get();
            let offset2 = stops[i - 0].offset.get();

            // Next offset must be smaller then previous.
            if offset1 > offset2 || offset1.approx_eq_ulps(&offset2, 4) {
                // Make previous offset a bit smaller.
                let new_offset = offset1 - f32::EPSILON;
                stops[i - 1].offset = StopOffset::new_clamped(new_offset);
                stops[i - 0].offset = StopOffset::new_clamped(offset1);
            }

            i += 1;
        }
    }

    crate::tree::owner::finish(p, Ok(stops))
}

#[inline(never)]
pub(crate) fn resolve_number(
    node: SvgNode,
    name: AId,
    units: Units,
    state: &converter::State,
    def: Length,
) -> f32 {
    resolve_attr(node, name).convert_length(name, units, state, def)
}

fn resolve_attr<'a, 'input: 'a>(node: SvgNode<'a, 'input>, name: AId) -> SvgNode<'a, 'input> {
    if node.has_attribute(name) {
        return node;
    }

    match node.tag_name().unwrap() {
        EId::LinearGradient => resolve_lg_attr(node, name),
        EId::RadialGradient => resolve_rg_attr(node, name),
        EId::Pattern => resolve_pattern_attr(node, name),
        EId::Filter => resolve_filter_attr(node, name),
        _ => node,
    }
}

fn resolve_lg_attr<'a, 'input: 'a>(node: SvgNode<'a, 'input>, name: AId) -> SvgNode<'a, 'input> {
    for link in node.href_iter() {
        let tag_name = match link.tag_name() {
            Some(v) => v,
            None => return node,
        };

        match (name, tag_name) {
            // Coordinates can be resolved only from
            // ref element with the same type.
              (AId::X1, EId::LinearGradient)
            | (AId::Y1, EId::LinearGradient)
            | (AId::X2, EId::LinearGradient)
            | (AId::Y2, EId::LinearGradient)
            // Other attributes can be resolved
            // from any kind of gradient.
            | (AId::GradientUnits, EId::LinearGradient)
            | (AId::GradientUnits, EId::RadialGradient)
            | (AId::SpreadMethod, EId::LinearGradient)
            | (AId::SpreadMethod, EId::RadialGradient)
            | (AId::GradientTransform, EId::LinearGradient)
            | (AId::GradientTransform, EId::RadialGradient) => {
                if link.has_attribute(name) {
                    return link;
                }
            }
            _ => break,
        }
    }

    node
}

fn resolve_rg_attr<'a, 'input>(node: SvgNode<'a, 'input>, name: AId) -> SvgNode<'a, 'input> {
    for link in node.href_iter() {
        let tag_name = match link.tag_name() {
            Some(v) => v,
            None => return node,
        };

        match (name, tag_name) {
            // Coordinates can be resolved only from
            // ref element with the same type.
              (AId::Cx, EId::RadialGradient)
            | (AId::Cy, EId::RadialGradient)
            | (AId::R,  EId::RadialGradient)
            | (AId::Fx, EId::RadialGradient)
            | (AId::Fy, EId::RadialGradient)
            | (AId::Fr, EId::RadialGradient)
            // Other attributes can be resolved
            // from any kind of gradient.
            | (AId::GradientUnits, EId::LinearGradient)
            | (AId::GradientUnits, EId::RadialGradient)
            | (AId::SpreadMethod, EId::LinearGradient)
            | (AId::SpreadMethod, EId::RadialGradient)
            | (AId::GradientTransform, EId::LinearGradient)
            | (AId::GradientTransform, EId::RadialGradient) => {
                if link.has_attribute(name) {
                    return link;
                }
            }
            _ => break,
        }
    }

    node
}

fn resolve_pattern_attr<'a, 'input: 'a>(
    node: SvgNode<'a, 'input>,
    name: AId,
) -> SvgNode<'a, 'input> {
    for link in node.href_iter() {
        let tag_name = match link.tag_name() {
            Some(v) => v,
            None => return node,
        };

        if tag_name != EId::Pattern {
            break;
        }

        if link.has_attribute(name) {
            return link;
        }
    }

    node
}

fn resolve_filter_attr<'a, 'input: 'a>(node: SvgNode<'a, 'input>, aid: AId) -> SvgNode<'a, 'input> {
    for link in node.href_iter() {
        let tag_name = match link.tag_name() {
            Some(v) => v,
            None => return node,
        };

        if tag_name != EId::Filter {
            break;
        }

        if link.has_attribute(aid) {
            return link;
        }
    }

    node
}

fn stops_to_color(stops: &[Stop]) -> Option<ServerOrColor> {
    if stops.is_empty() {
        None
    } else {
        Some(ServerOrColor::Color {
            color: stops[0].color,
            opacity: stops[0].opacity,
        })
    }
}

// Update paints servers by doing the following:
// 1. Replace context fills/strokes that are linked to
// a use node with their actual values.
// 2. Convert all object units to UserSpaceOnUse
pub fn update_paint_servers(
    group: &mut Group,
    context_transform: Transform,
    context_bbox: Option<Rect>,
    text_bbox: Option<Rect>,
    cache: &mut Cache,
) {
    if cache.processing.failed() {
        return;
    }
    for child in &mut group.children {
        if cache.processing.failed() {
            return;
        }
        // Set context transform and bbox if applicable if the
        // current group is a use node.
        let (context_transform, context_bbox) = if group.is_context_element {
            (group.abs_transform, Some(group.bounding_box))
        } else {
            (context_transform, context_bbox)
        };

        node_to_user_coordinates(child, context_transform, context_bbox, text_bbox, cache);
        if cache.processing.failed() {
            return;
        }
    }
}

// When parsing clipPaths, masks and filters we already know group's bounding box.
// But with gradients and patterns we don't, because we have to know text bounding box
// before we even parsed it. Which is impossible.
// Therefore our only choice is to parse gradients and patterns preserving their units
// and then replace them with `userSpaceOnUse` after the whole tree parsing is finished.
// So while gradients and patterns do still store their units,
// they are not exposed in the public API and for the caller they are always `userSpaceOnUse`.
fn node_to_user_coordinates(
    node: &mut Node,
    context_transform: Transform,
    context_bbox: Option<Rect>,
    text_bbox: Option<Rect>,
    cache: &mut Cache,
) {
    if cache.processing.failed() {
        return;
    }
    match node {
        Node::Group(g) => {
            update_paint_servers(
                g.as_mut(),
                context_transform,
                context_bbox,
                text_bbox,
                cache,
            );
        }
        Node::Path(path) => {
            let path = path.as_mut();
            // Paths inside `Text::flattened` are special and must use text's bounding box
            // instead of their own.
            let bbox = text_bbox.unwrap_or(path.bounding_box);

            process_fill(
                &mut path.fill,
                path.abs_transform,
                context_transform,
                context_bbox,
                bbox,
                cache,
            );
            if cache.processing.failed() {
                return;
            }
            process_stroke(
                &mut path.stroke,
                path.abs_transform,
                context_transform,
                context_bbox,
                bbox,
                cache,
            );
        }
        Node::Image(image) => {
            if let ImageKind::SVG(tree) = &mut image.as_mut().kind {
                update_paint_servers(&mut tree.root, context_transform, context_bbox, None, cache);
            }
        }
        Node::Text(text) => {
            let text = text.as_mut();
            // By the SVG spec, `tspan` doesn't have a bbox and uses the parent `text` bbox.
            // Therefore we have to use text's bbox when converting tspan and flatted text
            // paint servers.
            let bbox = text.bounding_box;

            // We need to update three things:
            // 1. The fills/strokes of the original elements in the usvg tree.
            // 2. The fills/strokes of the layouted elements of the text.
            // 3. The fills/strokes of the outlined text.

            // 1.
            for chunk in &mut text.chunks {
                if cache.processing.failed() {
                    return;
                }
                for span in &mut chunk.spans {
                    if cache.processing.failed() {
                        return;
                    }
                    process_fill(
                        &mut span.fill,
                        text.abs_transform,
                        context_transform,
                        context_bbox,
                        bbox,
                        cache,
                    );
                    if cache.processing.failed() {
                        return;
                    }
                    process_stroke(
                        &mut span.stroke,
                        text.abs_transform,
                        context_transform,
                        context_bbox,
                        bbox,
                        cache,
                    );
                    if cache.processing.failed() {
                        return;
                    }
                    process_text_decoration(&mut span.decoration.underline, bbox, cache);
                    if cache.processing.failed() {
                        return;
                    }
                    process_text_decoration(&mut span.decoration.overline, bbox, cache);
                    if cache.processing.failed() {
                        return;
                    }
                    process_text_decoration(&mut span.decoration.line_through, bbox, cache);
                    if cache.processing.failed() {
                        return;
                    }
                }
            }

            // 2.
            #[cfg(feature = "text")]
            for span in &mut text.layouted {
                if cache.processing.failed() {
                    return;
                }
                process_fill(
                    &mut span.fill,
                    text.abs_transform,
                    context_transform,
                    context_bbox,
                    bbox,
                    cache,
                );
                if cache.processing.failed() {
                    return;
                }
                process_stroke(
                    &mut span.stroke,
                    text.abs_transform,
                    context_transform,
                    context_bbox,
                    bbox,
                    cache,
                );

                if cache.processing.failed() {
                    return;
                }
                let mut process_decoration = |path: &mut Path| {
                    if cache.processing.failed() {
                        return false;
                    }
                    process_fill(
                        &mut path.fill,
                        text.abs_transform,
                        context_transform,
                        context_bbox,
                        bbox,
                        cache,
                    );
                    if cache.processing.failed() {
                        return false;
                    }
                    process_stroke(
                        &mut path.stroke,
                        text.abs_transform,
                        context_transform,
                        context_bbox,
                        bbox,
                        cache,
                    );

                    !cache.processing.failed()
                };

                if let Some(path) = &mut span.overline {
                    if !process_decoration(path) {
                        return;
                    }
                }

                if let Some(path) = &mut span.underline {
                    if !process_decoration(path) {
                        return;
                    }
                }

                if let Some(path) = &mut span.line_through {
                    if !process_decoration(path) {
                        return;
                    }
                }
            }

            // 3.
            update_paint_servers(
                text.flattened.as_mut(),
                context_transform,
                context_bbox,
                Some(bbox),
                cache,
            );
        }
    }
}

fn process_fill(
    fill: &mut Option<Fill>,
    path_transform: Transform,
    context_transform: Transform,
    context_bbox: Option<Rect>,
    bbox: Rect,
    cache: &mut Cache,
) {
    if cache.processing.failed() {
        return;
    }
    let mut ok = false;
    if let Some(fill) = fill.as_mut() {
        // Path context elements (i.e. for  markers) have already been resolved,
        // so we only care about use nodes.
        ok = process_paint(
            &mut fill.paint,
            matches!(fill.context_element, Some(ContextElement::UseNode)),
            context_transform,
            context_bbox,
            path_transform,
            bbox,
            cache,
        );
    }
    if cache.processing.failed() {
        return;
    }
    if !ok {
        *fill = None;
    }
}

fn process_stroke(
    stroke: &mut Option<Stroke>,
    path_transform: Transform,
    context_transform: Transform,
    context_bbox: Option<Rect>,
    bbox: Rect,
    cache: &mut Cache,
) {
    if cache.processing.failed() {
        return;
    }
    let mut ok = false;
    if let Some(stroke) = stroke.as_mut() {
        // Path context elements (i.e. for  markers) have already been resolved,
        // so we only care about use nodes.
        ok = process_paint(
            &mut stroke.paint,
            matches!(stroke.context_element, Some(ContextElement::UseNode)),
            context_transform,
            context_bbox,
            path_transform,
            bbox,
            cache,
        );
    }
    if cache.processing.failed() {
        return;
    }
    if !ok {
        *stroke = None;
    }
}

fn process_context_paint(
    paint: &mut Paint,
    context_transform: Transform,
    path_transform: Transform,
    cache: &mut Cache,
) -> Option<()> {
    if cache.processing.failed() {
        return None;
    }
    // The idea is the following: We have a certain context element that has
    // a transform A, and further below in the tree we have for example a path
    // whose paint has a transform C. In order to get from A to C, there is some
    // transformation matrix B such that A x B = C. We now need to figure out
    // a way to get from C back to A, so that the transformation of the paint
    // matches the one from the context element, even if B was applied. How
    // do we do that? We calculate CxB^(-1), which will overall then have
    // the same effect as A. How do we calculate B^(-1)?
    // --> (A^(-1)xC)^(-1)
    let rev_transform = context_transform
        .invert()?
        .pre_concat(path_transform)
        .invert()?;

    match paint {
        Paint::Color(_) => {}
        Paint::LinearGradient(lg) => {
            let transform = lg.transform.post_concat(rev_transform);
            *paint = Paint::LinearGradient(
                crate::SharedOwner::try_new(
                    LinearGradient {
                        x1: lg.x1,
                        y1: lg.y1,
                        x2: lg.x2,
                        y2: lg.y2,
                        base: BaseGradient {
                            id: cache.gen_linear_gradient_id().ok()?,
                            units: lg.units,
                            transform,
                            spread_method: lg.spread_method,
                            stops: lg.stops.try_copy_in(&cache.processing).ok()?,
                        },
                    },
                    cache.processing.resources(),
                    markdown_svg_budget::ResourceKind::Scratch,
                )
                .ok()?,
            );
        }
        Paint::RadialGradient(rg) => {
            let transform = rg.transform.post_concat(rev_transform);
            *paint = Paint::RadialGradient(
                crate::SharedOwner::try_new(
                    RadialGradient {
                        cx: rg.cx,
                        cy: rg.cy,
                        r: rg.r,
                        fx: rg.fx,
                        fy: rg.fy,
                        fr: rg.fr,
                        base: BaseGradient {
                            id: cache.gen_radial_gradient_id().ok()?,
                            units: rg.units,
                            transform,
                            spread_method: rg.spread_method,
                            stops: rg.stops.try_copy_in(&cache.processing).ok()?,
                        },
                    },
                    cache.processing.resources(),
                    markdown_svg_budget::ResourceKind::Scratch,
                )
                .ok()?,
            );
        }
        Paint::Pattern(pat) => {
            let transform = pat.transform.post_concat(rev_transform);
            let root = crate::scene::clone_owned(&cache.processing, &pat.root).ok()?;
            *paint = Paint::Pattern(
                crate::SharedOwner::try_new(
                    Pattern {
                        id: cache.gen_pattern_id().ok()?,
                        units: pat.units,
                        content_units: pat.content_units,
                        transform,
                        rect: pat.rect,
                        view_box: pat.view_box,
                        root,
                    },
                    cache.processing.resources(),
                    markdown_svg_budget::ResourceKind::Scratch,
                )
                .ok()?,
            );
        }
    }

    crate::tree::owner::check(&cache.processing, 0).ok()?;
    Some(())
}

pub(crate) fn process_paint(
    paint: &mut Paint,
    has_context: bool,
    context_transform: Transform,
    context_bbox: Option<Rect>,
    path_transform: Transform,
    bbox: Rect,
    cache: &mut Cache,
) -> bool {
    if cache.processing.failed() {
        return false;
    }
    if paint.units() == Units::ObjectBoundingBox
        || paint.content_units() == Units::ObjectBoundingBox
    {
        let bbox = if has_context {
            let Some(bbox) = context_bbox else {
                return false;
            };
            bbox
        } else {
            bbox
        };

        if paint.to_user_coordinates(bbox, cache).is_none() {
            return false;
        }
    }

    if has_context {
        process_context_paint(paint, context_transform, path_transform, cache);
    }

    !cache.processing.failed()
}

fn process_text_decoration(style: &mut Option<TextDecorationStyle>, bbox: Rect, cache: &mut Cache) {
    if cache.processing.failed() {
        return;
    }
    if let Some(style) = style.as_mut() {
        process_fill(
            &mut style.fill,
            Transform::default(),
            Transform::default(),
            None,
            bbox,
            cache,
        );
        if cache.processing.failed() {
            return;
        }
        process_stroke(
            &mut style.stroke,
            Transform::default(),
            Transform::default(),
            None,
            bbox,
            cache,
        );
    }
}

impl Paint {
    fn to_user_coordinates(&mut self, bbox: Rect, cache: &mut Cache) -> Option<()> {
        if cache.processing.failed() {
            return None;
        }
        let name = if matches!(self, Paint::Pattern(_)) {
            "Pattern"
        } else {
            "Gradient"
        };
        let bbox = bbox
            .to_non_zero_rect()
            .log_none(|| log::warn!("{} on zero-sized shapes is not allowed.", name))?;

        // `Arc::get_mut()` allow us to modify some paint servers in-place.
        // This reduces the amount of cloning and preserves the original ID as well.
        match self {
            Paint::Color(_) => {} // unreachable
            Paint::LinearGradient(lg) => {
                let transform = lg.transform.post_concat(Transform::from_bbox(bbox));
                if let Some(lg) = lg.try_get_mut_if_unique(&cache.processing).ok()? {
                    lg.base.transform = transform;
                    lg.base.units = Units::UserSpaceOnUse;
                } else {
                    *lg = crate::SharedOwner::try_new(
                        LinearGradient {
                            x1: lg.x1,
                            y1: lg.y1,
                            x2: lg.x2,
                            y2: lg.y2,
                            base: BaseGradient {
                                id: cache.gen_linear_gradient_id().ok()?,
                                units: Units::UserSpaceOnUse,
                                transform,
                                spread_method: lg.spread_method,
                                stops: lg.stops.try_copy_in(&cache.processing).ok()?,
                            },
                        },
                        cache.processing.resources(),
                        markdown_svg_budget::ResourceKind::Scratch,
                    )
                    .ok()?;
                }
            }
            Paint::RadialGradient(rg) => {
                let transform = rg.transform.post_concat(Transform::from_bbox(bbox));
                if let Some(rg) = rg.try_get_mut_if_unique(&cache.processing).ok()? {
                    rg.base.transform = transform;
                    rg.base.units = Units::UserSpaceOnUse;
                } else {
                    *rg = crate::SharedOwner::try_new(
                        RadialGradient {
                            cx: rg.cx,
                            cy: rg.cy,
                            r: rg.r,
                            fx: rg.fx,
                            fy: rg.fy,
                            fr: rg.fr,
                            base: BaseGradient {
                                id: cache.gen_radial_gradient_id().ok()?,
                                units: Units::UserSpaceOnUse,
                                transform,
                                spread_method: rg.spread_method,
                                stops: rg.stops.try_copy_in(&cache.processing).ok()?,
                            },
                        },
                        cache.processing.resources(),
                        markdown_svg_budget::ResourceKind::Scratch,
                    )
                    .ok()?;
                }
            }
            Paint::Pattern(patt) => {
                let rect = if patt.units == Units::ObjectBoundingBox {
                    patt.rect.bbox_transform(bbox)
                } else {
                    patt.rect
                };

                if let Some(patt) = patt.try_get_mut_if_unique(&cache.processing).ok()? {
                    patt.rect = rect;
                    patt.units = Units::UserSpaceOnUse;

                    if patt.content_units == Units::ObjectBoundingBox && patt.view_box.is_none() {
                        // No need to shift patterns.
                        let transform = Transform::from_scale(bbox.width(), bbox.height());
                        push_pattern_transform(&mut patt.root, transform, &cache.processing)?;
                    }

                    if let Some(view_box) = patt.view_box {
                        push_pattern_transform(
                            &mut patt.root,
                            view_box.to_transform(rect.size()),
                            &cache.processing,
                        )?;
                    }

                    patt.content_units = Units::UserSpaceOnUse;
                } else {
                    let mut root = if patt.content_units == Units::ObjectBoundingBox
                        && patt.view_box.is_none()
                    {
                        // No need to shift patterns.
                        let transform = Transform::from_scale(bbox.width(), bbox.height());

                        let mut g =
                            crate::scene::clone_owned(&cache.processing, &patt.root).ok()?;
                        push_pattern_transform(&mut g, transform, &cache.processing)?;
                        g
                    } else {
                        crate::scene::clone_owned(&cache.processing, &patt.root).ok()?
                    };

                    if let Some(view_box) = patt.view_box {
                        push_pattern_transform(
                            &mut root,
                            view_box.to_transform(rect.size()),
                            &cache.processing,
                        )?;
                    }

                    *patt = crate::SharedOwner::try_new(
                        Pattern {
                            id: cache.gen_pattern_id().ok()?,
                            units: Units::UserSpaceOnUse,
                            content_units: Units::UserSpaceOnUse,
                            transform: patt.transform,
                            rect,
                            view_box: patt.view_box,
                            root,
                        },
                        cache.processing.resources(),
                        markdown_svg_budget::ResourceKind::Scratch,
                    )
                    .ok()?;
                }
            }
        }

        self.validate_policy(&cache.processing).ok()?;
        crate::tree::owner::check(&cache.processing, 0).ok()?;
        Some(())
    }
}

fn push_pattern_transform(
    root: &mut Group,
    transform: Transform,
    processing: &crate::ProcessingContext,
) -> Option<()> {
    // TODO: we should update abs_transform in all descendants as well
    let outer = crate::scene::root(processing, || Group::empty_in(processing)).ok()?;
    let mut group = std::mem::replace(root, outer);
    group.transform = transform;
    group.abs_transform = transform;

    // The old root was admitted by pattern conversion's scene::root or the
    // clone_owned branch above. This wrapper moves it and adds only outer.
    let group = crate::scene::moved_root(processing, group).ok()?;
    root.children
        .try_push(Node::Group(group), processing)
        .ok()?;
    root.calculate_bounding_boxes();
    Some(())
}

impl Paint {
    #[inline]
    pub(crate) fn units(&self) -> Units {
        match self {
            Self::Color(_) => Units::UserSpaceOnUse,
            Self::LinearGradient(lg) => lg.units,
            Self::RadialGradient(rg) => rg.units,
            Self::Pattern(patt) => patt.units,
        }
    }

    #[inline]
    pub(crate) fn content_units(&self) -> Units {
        match self {
            Self::Pattern(patt) => patt.content_units,
            _ => Units::UserSpaceOnUse,
        }
    }
}
