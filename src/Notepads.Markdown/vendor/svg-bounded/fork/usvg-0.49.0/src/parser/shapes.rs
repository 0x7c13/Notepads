// Copyright 2018 the Resvg Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT
// Modifications Copyright (c) 2026, Jiaqi (0x7c13) Liu.

use super::svgtree::{AId, EId, SvgNode};
use super::{converter, units};
use crate::geometry::SharedOwner;
use crate::{ApproxEqUlps, IsValidLength, ProcessingContext, Rect};
use markdown_svg_budget::{Error, ResourceKind};
use svgtypes::{Length, NumericParseError, SimplePathSegment};
use tiny_skia_path::{Path, PathBuilder};

pub(crate) fn checkpoint(processing: &ProcessingContext, work: u64) -> Result<(), Error> {
    if let Some(error) = processing.resource_failure() {
        return Err(error);
    }
    if let Some(resources) = processing.resources() {
        resources
            .checkpoint(work)
            .map_err(|error| processing.record_resource(error))?;
    }
    Ok(())
}

fn builder(processing: &ProcessingContext) -> PathBuilder {
    match processing.resources() {
        Some(resources) => PathBuilder::with_resources(resources, ResourceKind::Geometry),
        None => PathBuilder::new(),
    }
}

pub(crate) fn rect_to_path(rect: Rect, processing: &ProcessingContext) -> Result<Path, Error> {
    checkpoint(processing, 0)?;
    let path = match processing.resources() {
        Some(resources) => PathBuilder::try_from_rect(rect, resources, ResourceKind::Geometry)?,
        None => PathBuilder::from_rect(rect),
    };
    checkpoint(processing, 0)?;
    Ok(path)
}

pub(crate) fn rect_to_node_path(
    rect: Rect,
    processing: &ProcessingContext,
) -> Result<Option<crate::Path>, Error> {
    let candidate = {
        let path = rect_to_path(rect, processing)?;
        match processing.resources() {
            Some(resources) => crate::Path::try_new_simple(path, resources)?,
            None => crate::Path::new_simple(std::sync::Arc::new(path)),
        }
    };
    checkpoint(processing, 0)?;
    Ok(candidate)
}

pub(crate) fn convert(
    node: SvgNode,
    state: &converter::State,
) -> Result<Option<SharedOwner<Path>>, Error> {
    let processing = &state.opt.processing;
    checkpoint(processing, 0)?;
    let candidate = (|| {
        let Some(tag) = node.tag_name() else {
            return Ok(None);
        };
        let path = match tag {
            EId::Rect => convert_rect(node, state),
            EId::Circle => convert_circle(node, state),
            EId::Ellipse => convert_ellipse(node, state),
            EId::Line => convert_line(node, state),
            EId::Polyline => convert_polyline(node, state),
            EId::Polygon => convert_polygon(node, state),
            EId::Path => convert_path(node, state),
            _ => Ok(None),
        }?;
        path.map(|path| SharedOwner::try_new(path, processing.resources(), ResourceKind::Geometry))
            .transpose()
    })();
    // All parser queues and non-result builders have retired before this gate.
    checkpoint(processing, 0)?;
    candidate.map_err(|error| processing.record_resource(error))
}

fn syntax_or_control(processing: &ProcessingContext, error: NumericParseError<Error>) -> Error {
    match error {
        NumericParseError::Control(error) => processing.record_resource(error),
        NumericParseError::Syntax(_) => processing.record_resource(Error::new(
            markdown_svg_budget::FailureKind::Invariant,
            markdown_svg_budget::LimitKind::None,
        )),
    }
}

pub(crate) fn convert_path(node: SvgNode, state: &converter::State) -> Result<Option<Path>, Error> {
    let Some(value) = node.attribute::<&str>(AId::D) else {
        return Ok(None);
    };
    let processing = &state.opt.processing;
    let mut builder = builder(processing);
    {
        if let Some(resources) = processing.resources() {
            let mut parser = svgtypes::ControlledSimplifyingPathParser::new(
                value,
                |work| checkpoint(processing, work as u64),
                resources,
            )
            .map_err(|error| syntax_or_control(processing, error))?;
            append_segments(
                core::iter::from_fn(|| parser.try_next().transpose()),
                &mut builder,
                processing,
            )?;
        } else {
            append_segments(
                svgtypes::SimplifyingPathParser::from(value)
                    .map(|result| result.map_err(NumericParseError::Syntax)),
                &mut builder,
                processing,
            )?;
        }
    }
    checkpoint(processing, 0)?;
    builder.try_finish()
}

fn append_segments(
    segments: impl Iterator<Item = Result<SimplePathSegment, NumericParseError<Error>>>,
    builder: &mut PathBuilder,
    processing: &ProcessingContext,
) -> Result<(), Error> {
    for segment in segments {
        let segment = match segment {
            Ok(value) => value,
            Err(NumericParseError::Syntax(error)) => {
                log::warn!("Error during path parsing: {error}");
                break;
            }
            Err(NumericParseError::Control(error)) => return Err(processing.record_resource(error)),
        };
        checkpoint(processing, 1)?;
        match segment {
            SimplePathSegment::MoveTo { x, y } => builder.try_move_to(x as f32, y as f32)?,
            SimplePathSegment::LineTo { x, y } => builder.try_line_to(x as f32, y as f32)?,
            SimplePathSegment::Quadratic { x1, y1, x, y } => {
                builder.try_quad_to(x1 as f32, y1 as f32, x as f32, y as f32)?
            }
            SimplePathSegment::CurveTo {
                x1,
                y1,
                x2,
                y2,
                x,
                y,
            } => builder.try_cubic_to(
                x1 as f32, y1 as f32, x2 as f32, y2 as f32, x as f32, y as f32,
            )?,
            SimplePathSegment::ClosePath => builder.try_close()?,
        }
    }
    Ok(())
}

fn convert_rect(node: SvgNode, state: &converter::State) -> Result<Option<Path>, Error> {
    let width = node.convert_user_length(AId::Width, state, Length::zero());
    let height = node.convert_user_length(AId::Height, state, Length::zero());
    if !width.is_valid_length() {
        log::warn!(
            "Rect '{}' has an invalid 'width' value. Skipped.",
            node.element_id()
        );
        return Ok(None);
    }
    if !height.is_valid_length() {
        log::warn!(
            "Rect '{}' has an invalid 'height' value. Skipped.",
            node.element_id()
        );
        return Ok(None);
    }
    let x = node.convert_user_length(AId::X, state, Length::zero());
    let y = node.convert_user_length(AId::Y, state, Length::zero());
    let (mut rx, mut ry) = resolve_rx_ry(node, state);
    if rx > width / 2.0 {
        rx = width / 2.0;
    }
    if ry > height / 2.0 {
        ry = height / 2.0;
    }
    if rx.approx_eq_ulps(&0.0, 4) {
        let Some(rect) = Rect::from_xywh(x, y, width, height) else {
            return Ok(None);
        };
        return rect_to_path(rect, &state.opt.processing).map(Some);
    }
    let processing = &state.opt.processing;
    let mut builder = builder(processing);
    builder.try_move_to(x + rx, y)?;
    builder.try_line_to(x + width - rx, y)?;
    builder.try_arc_to(rx, ry, 0.0, false, true, x + width, y + ry, processing)?;
    builder.try_line_to(x + width, y + height - ry)?;
    builder.try_arc_to(
        rx,
        ry,
        0.0,
        false,
        true,
        x + width - rx,
        y + height,
        processing,
    )?;
    builder.try_line_to(x + rx, y + height)?;
    builder.try_arc_to(rx, ry, 0.0, false, true, x, y + height - ry, processing)?;
    builder.try_line_to(x, y + ry)?;
    builder.try_arc_to(rx, ry, 0.0, false, true, x + rx, y, processing)?;
    builder.try_close()?;
    builder.try_finish()
}

fn resolve_rx_ry(node: SvgNode, state: &converter::State) -> (f32, f32) {
    let mut rx_opt = node.attribute::<Length>(AId::Rx);
    let mut ry_opt = node.attribute::<Length>(AId::Ry);

    // Remove negative values first.
    if let Some(v) = rx_opt {
        if v.number.is_sign_negative() {
            rx_opt = None;
        }
    }
    if let Some(v) = ry_opt {
        if v.number.is_sign_negative() {
            ry_opt = None;
        }
    }

    // Resolve.
    match (rx_opt, ry_opt) {
        (None, None) => (0.0, 0.0),
        (Some(rx), None) => {
            let rx = units::convert_user_length(rx, node, AId::Rx, state);
            (rx, rx)
        }
        (None, Some(ry)) => {
            let ry = units::convert_user_length(ry, node, AId::Ry, state);
            (ry, ry)
        }
        (Some(rx), Some(ry)) => {
            let rx = units::convert_user_length(rx, node, AId::Rx, state);
            let ry = units::convert_user_length(ry, node, AId::Ry, state);
            (rx, ry)
        }
    }
}

fn convert_line(node: SvgNode, state: &converter::State) -> Result<Option<Path>, Error> {
    let x1 = node.convert_user_length(AId::X1, state, Length::zero());
    let y1 = node.convert_user_length(AId::Y1, state, Length::zero());
    let x2 = node.convert_user_length(AId::X2, state, Length::zero());
    let y2 = node.convert_user_length(AId::Y2, state, Length::zero());
    let mut builder = builder(&state.opt.processing);
    builder.try_move_to(x1, y1)?;
    builder.try_line_to(x2, y2)?;
    builder.try_finish()
}

fn convert_polyline(node: SvgNode, state: &converter::State) -> Result<Option<Path>, Error> {
    let Some(builder) = points_to_path(node, "Polyline", state)? else {
        return Ok(None);
    };
    builder.try_finish()
}
fn convert_polygon(node: SvgNode, state: &converter::State) -> Result<Option<Path>, Error> {
    let Some(mut builder) = points_to_path(node, "Polygon", state)? else {
        return Ok(None);
    };
    builder.try_close()?;
    builder.try_finish()
}
fn points_to_path(
    node: SvgNode,
    eid: &str,
    state: &converter::State,
) -> Result<Option<PathBuilder>, Error> {
    let Some(text) = node.attribute::<&str>(AId::Points) else {
        log::warn!(
            "{} '{}' has an invalid 'points' value. Skipped.",
            eid,
            node.element_id()
        );
        return Ok(None);
    };
    let processing = &state.opt.processing;
    let mut builder = builder(processing);
    if processing.resources().is_some() {
        let mut parser =
            svgtypes::ControlledPointsParser::new(text, |work| checkpoint(processing, work as u64))
                .map_err(|error| syntax_or_control(processing, error))?;
        append_points(
            core::iter::from_fn(|| parser.try_next().transpose()),
            &mut builder,
            processing,
        )?;
    } else {
        append_points(
            svgtypes::PointsParser::from(text).map(Ok),
            &mut builder,
            processing,
        )?;
    }
    checkpoint(processing, 0)?;
    if builder.len() < 2 {
        log::warn!(
            "{} '{}' has less than 2 points. Skipped.",
            eid,
            node.element_id()
        );
        return Ok(None);
    }
    Ok(Some(builder))
}
fn append_points(
    points: impl Iterator<Item = Result<(f64, f64), NumericParseError<Error>>>,
    builder: &mut PathBuilder,
    processing: &ProcessingContext,
) -> Result<(), Error> {
    for point in points {
        let (x, y) = point.map_err(|error| syntax_or_control(processing, error))?;
        checkpoint(processing, 1)?;
        if builder.is_empty() {
            builder.try_move_to(x as f32, y as f32)?;
        } else {
            builder.try_line_to(x as f32, y as f32)?;
        }
    }
    Ok(())
}

fn convert_circle(node: SvgNode, state: &converter::State) -> Result<Option<Path>, Error> {
    let cx = node.convert_user_length(AId::Cx, state, Length::zero());
    let cy = node.convert_user_length(AId::Cy, state, Length::zero());
    let r = node.convert_user_length(AId::R, state, Length::zero());
    if !r.is_valid_length() {
        log::warn!(
            "Circle '{}' has an invalid 'r' value. Skipped.",
            node.element_id()
        );
        return Ok(None);
    }
    ellipse_to_path(cx, cy, r, r, &state.opt.processing)
}
fn convert_ellipse(node: SvgNode, state: &converter::State) -> Result<Option<Path>, Error> {
    let cx = node.convert_user_length(AId::Cx, state, Length::zero());
    let cy = node.convert_user_length(AId::Cy, state, Length::zero());
    let (rx, ry) = resolve_rx_ry(node, state);
    if !rx.is_valid_length() {
        log::warn!(
            "Ellipse '{}' has an invalid 'rx' value. Skipped.",
            node.element_id()
        );
        return Ok(None);
    }
    if !ry.is_valid_length() {
        log::warn!(
            "Ellipse '{}' has an invalid 'ry' value. Skipped.",
            node.element_id()
        );
        return Ok(None);
    }
    ellipse_to_path(cx, cy, rx, ry, &state.opt.processing)
}
fn ellipse_to_path(
    cx: f32,
    cy: f32,
    rx: f32,
    ry: f32,
    processing: &ProcessingContext,
) -> Result<Option<Path>, Error> {
    let mut builder = builder(processing);
    builder.try_move_to(cx + rx, cy)?;
    builder.try_arc_to(rx, ry, 0.0, false, true, cx, cy + ry, processing)?;
    builder.try_arc_to(rx, ry, 0.0, false, true, cx - rx, cy, processing)?;
    builder.try_arc_to(rx, ry, 0.0, false, true, cx, cy - ry, processing)?;
    builder.try_arc_to(rx, ry, 0.0, false, true, cx + rx, cy, processing)?;
    builder.try_close()?;
    builder.try_finish()
}
trait PathBuilderExt {
    #[allow(clippy::too_many_arguments)]
    fn try_arc_to(
        &mut self,
        rx: f32,
        ry: f32,
        x_axis_rotation: f32,
        large_arc: bool,
        sweep: bool,
        x: f32,
        y: f32,
        processing: &ProcessingContext,
    ) -> Result<(), Error>;
}
impl PathBuilderExt for PathBuilder {
    fn try_arc_to(
        &mut self,
        rx: f32,
        ry: f32,
        x_axis_rotation: f32,
        large_arc: bool,
        sweep: bool,
        x: f32,
        y: f32,
        processing: &ProcessingContext,
    ) -> Result<(), Error> {
        checkpoint(processing, 1)?;
        let Some(prev) = self.last_point() else {
            return Ok(());
        };
        let svg_arc = kurbo::SvgArc {
            from: kurbo::Point::new(prev.x as f64, prev.y as f64),
            to: kurbo::Point::new(x as f64, y as f64),
            radii: kurbo::Vec2::new(rx as f64, ry as f64),
            x_rotation: (x_axis_rotation as f64).to_radians(),
            large_arc,
            sweep,
        };
        if let Some(arc) = kurbo::Arc::from_svg_arc(&svg_arc) {
            let mut segments = arc.append_iter(0.1);
            loop {
                checkpoint(processing, 1)?;
                let Some(kurbo::PathEl::CurveTo(p1, p2, p)) = segments.next() else {
                    break;
                };
                self.try_cubic_to(
                    p1.x as f32,
                    p1.y as f32,
                    p2.x as f32,
                    p2.y as f32,
                    p.x as f32,
                    p.y as f32,
                )?;
            }
        } else {
            self.try_line_to(x, y)?;
        }
        checkpoint(processing, 0)
    }
}

#[cfg(test)]
mod geometry_control_tests {
    use super::*;
    use markdown_svg_budget::{Backend, FirstFailureRef, Limits, ResourcesRef, Ticket};
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    };

    #[derive(Default)]
    struct Counts {
        next: AtomicUsize,
        live: AtomicUsize,
        fail_reserve: usize,
    }
    struct TestBackend(Arc<Counts>);
    impl Backend for TestBackend {
        fn reserve(&self, _: ResourceKind, bytes: usize) -> Result<Ticket, Error> {
            let slot = self.0.next.fetch_add(1, Ordering::Relaxed) + 1;
            if slot == self.0.fail_reserve {
                return Err(Error::native(-951));
            }
            self.0.live.fetch_add(bytes, Ordering::Relaxed);
            Ticket::new(bytes as u64, slot as u32, 1)
        }
        fn allocation_committed(&self, _: &Ticket, _: usize) -> Result<(), Error> {
            Ok(())
        }
        fn release_after_free(&self, ticket: Ticket) -> Result<(), Error> {
            self.0
                .live
                .fetch_sub(ticket.account_nonce() as usize, Ordering::Relaxed);
            Ok(())
        }
        fn checkpoint(&self, _: u64) -> Result<(), Error> {
            Ok(())
        }
    }
    fn resources(fail_reserve: usize) -> (ResourcesRef, Arc<Counts>) {
        let counts = Arc::new(Counts {
            fail_reserve,
            ..Counts::default()
        });
        let resources = ResourcesRef::new_prepaid(
            TestBackend(counts.clone()),
            FirstFailureRef::new_prepaid(),
            Limits::default(),
        );
        (resources, counts)
    }
    fn record_rect(rect: Rect, out: &mut Vec<u32>) {
        out.extend([
            rect.left().to_bits(),
            rect.top().to_bits(),
            rect.right().to_bits(),
            rect.bottom().to_bits(),
        ]);
    }
    fn snapshot(group: &crate::Group, resources: Option<&ResourcesRef>, out: &mut Vec<u32>) {
        if let Some(clip) = group.clip_path() {
            out.push(9);
            snapshot(clip.root(), resources, out);
        }
        for node in group.children() {
            match node {
                crate::Node::Group(group) => {
                    out.push(0);
                    snapshot(group, resources, out);
                }
                crate::Node::Path(path) => {
                    out.push(1);
                    let data = path.data();
                    if let Some(resources) = resources {
                        assert!(data.belongs_to(resources));
                        assert!(path.data.is_paid());
                    }
                    out.push(data.verbs().len() as u32);
                    out.extend(data.verbs().iter().map(|verb| *verb as u32));
                    out.push(data.points().len() as u32);
                    for point in data.points() {
                        out.extend([point.x.to_bits(), point.y.to_bits()]);
                    }
                    record_rect(data.bounds(), out);
                    record_rect(path.bounding_box(), out);
                    record_rect(path.abs_bounding_box(), out);
                    record_rect(path.stroke_bounding_box(), out);
                    record_rect(path.abs_stroke_bounding_box(), out);
                }
                crate::Node::Image(image) => {
                    out.push(2);
                    if let crate::ImageKind::SVG(tree) = image.kind() {
                        snapshot(tree.root(), resources, out);
                    }
                }
                crate::Node::Text(_) => panic!("fontless fixture unexpectedly contains text"),
            }
        }
    }
    const SHAPES: &str = r#"<svg xmlns='http://www.w3.org/2000/svg' width='320' height='200'>
        <rect x='2' y='3' width='30' height='20'/><rect x='40' y='3' width='30' height='20' rx='3' ry='5'/>
        <circle cx='90' cy='10' r='7'/><ellipse cx='115' cy='10' rx='9' ry='4'/>
        <path d='M1 40L10 45Q20 50 30 40C35 30 40 50 50 40A8 5 20 0 1 70 40z' stroke='black' stroke-width='2'/>
        <polyline points='1 70 15 80 25 70 99' stroke='black' fill='none'/><polygon points='40 70 55 80 65 70'/>
        <path d='M1 100L20 110L30' stroke='black'/><polyline points='40 100 50 110 bad 60 120' stroke='black'/>
        <path d='M1 130zzzH20V140z'/><rect x='1' y='1' width='-2' height='2'/>
    </svg>"#;
    const REFERENCES: &str = r#"<svg xmlns='http://www.w3.org/2000/svg' width='100' height='100'>
        <defs><symbol id='s' viewBox='0 0 20 10'><path d='M0 0L20 0L20 10z'/></symbol>
        <marker id='m' markerWidth='4' markerHeight='4' viewBox='0 0 4 4' orient='auto'><path d='M0 0L4 2L0 4z'/></marker></defs>
        <use href='#s' x='2' y='3' width='30' height='20'/>
        <svg x='40' y='20' width='20' height='30' viewBox='0 0 40 40' overflow='hidden'><circle cx='20' cy='20' r='18'/></svg>
        <path d='M10 50L20 55Q25 60 30 50z' fill='none' stroke='black' marker-start='url(#m)' marker-mid='url(#m)' marker-end='url(#m)'/>
    </svg>"#;
    const IMAGE: &str = r#"<svg xmlns='http://www.w3.org/2000/svg' width='100' height='100'>
        <image width='30' height='20' preserveAspectRatio='xMidYMid slice' href='data:image/svg+xml,%3Csvg%20xmlns=%22http://www.w3.org/2000/svg%22%20width=%2220%22%20height=%2210%22%3E%3Crect%20width=%2220%22%20height=%2210%22/%3E%3C/svg%3E'/>
    </svg>"#;

    #[test]
    fn real_trees_keep_all_geometry_fields_and_paid_provenance_for_shapes_references_and_image_clips(
    ) {
        for source in [SHAPES, REFERENCES, IMAGE] {
            let ordinary =
                crate::Tree::from_data(source.as_bytes(), &crate::Options::default()).unwrap();
            let mut expected = Vec::new();
            snapshot(ordinary.root(), None, &mut expected);
            assert!(!expected.is_empty());
            let (resources, counts) = resources(0);
            {
                let opt = crate::Options {
                    processing: Arc::new(ProcessingContext::with_resources(resources.clone())),
                    ..crate::Options::default()
                };
                let tree = crate::Tree::from_data_controlled(source.as_bytes(), &opt).unwrap();
                let mut actual = Vec::new();
                snapshot(tree.root(), Some(&resources), &mut actual);
                assert_eq!(actual, expected);
                assert!(counts.live.load(Ordering::Relaxed) > 0);
            }
            resources.checkpoint(0).unwrap();
            assert_eq!(counts.live.load(Ordering::Relaxed), 0);
            resources.validate_retired().unwrap();
        }
    }

    #[test]
    fn real_shape_tree_every_actual_request_reserve_failure_remains_typed_and_retires() {
        let (baseline, counts) = resources(0);
        {
            let opt = crate::Options {
                processing: Arc::new(ProcessingContext::with_resources(baseline.clone())),
                ..crate::Options::default()
            };
            let tree = crate::Tree::from_data_controlled(SHAPES.as_bytes(), &opt).unwrap();
            drop(tree);
        }
        let total = counts.next.load(Ordering::Relaxed);
        assert!(total > 20);
        baseline.validate_retired().unwrap();
        let mut wrappers = [0usize; 3];
        for reject in 1..=total {
            let (resources, counts) = resources(reject);
            {
                let opt = crate::Options {
                    processing: Arc::new(ProcessingContext::with_resources(resources.clone())),
                    ..crate::Options::default()
                };
                let result = crate::Tree::from_data_controlled(SHAPES.as_bytes(), &opt);
                let typed = match &result {
                    Err(crate::Error::Resource(error)) => {
                        wrappers[0] += 1;
                        Some(*error)
                    }
                    Err(crate::Error::ParsingFailed(roxmltree::Error::Resource(error))) => {
                        wrappers[1] += 1;
                        Some(*error)
                    }
                    Err(crate::Error::ProcessingFailure(error)) => {
                        wrappers[2] += 1;
                        Some(error.typed_failure)
                    }
                    _ => None,
                };
                assert_eq!(
                    typed,
                    Some(Error::native(-951)),
                    "reserve {reject}/{total}: {result:?}"
                );
                assert_eq!(resources.failure(), Some(Error::native(-951)));
            }
            assert_eq!(counts.next.load(Ordering::Relaxed), reject);
            assert_eq!(
                counts.live.load(Ordering::Relaxed),
                0,
                "reserve {reject}/{total}"
            );
            resources.validate_retired().unwrap();
        }
        eprintln!(
            "geometry-reserve-matrix total={total} resource={} xmlResource={} processing={}",
            wrappers[0], wrappers[1], wrappers[2]
        );
    }
}
