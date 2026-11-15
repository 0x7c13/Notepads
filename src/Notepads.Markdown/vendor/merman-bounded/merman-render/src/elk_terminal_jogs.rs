//! Mermaid 12 ELK terminal-channel straightening after endpoint clipping.
//!
//! Source: packages/mermaid/src/rendering-util/layout-algorithms/elk/render.ts,
//! straightenTerminalJogs, moved-run label projection and pair separation (mermaid@12.1.0).

use crate::model::LayoutPoint;

/// Paint routes may retain coordinates relative to separate parent groups.
pub(crate) trait TerminalRoute {
    fn points(&self) -> &[LayoutPoint];

    fn origin(&self) -> (f64, f64) {
        (0.0, 0.0)
    }

    /// Main labels share the route's coordinate system. Route-only adapters consume the
    /// returned change records instead, preserving ownership of their separate label caches.
    fn reproject_main_label(&mut self, _runs: &[StraightenedRun]) {}

    fn replace_terminal_points(
        &mut self,
        points: Vec<LayoutPoint>,
        source_changed: bool,
        target_changed: bool,
    );
}

impl TerminalRoute for Vec<LayoutPoint> {
    fn points(&self) -> &[LayoutPoint] {
        self
    }

    fn replace_terminal_points(&mut self, points: Vec<LayoutPoint>, _: bool, _: bool) {
        *self = points;
    }
}

impl TerminalRoute for crate::model::LayoutEdge {
    fn points(&self) -> &[LayoutPoint] {
        &self.points
    }

    fn reproject_main_label(&mut self, runs: &[StraightenedRun]) {
        if let Some(label) = self.label.as_mut() {
            reproject_label(label, runs);
        }
    }

    fn replace_terminal_points(&mut self, points: Vec<LayoutPoint>, _: bool, _: bool) {
        self.points = points;
    }
}

/// A displaced run in its original route coordinate system, including for a reversed end pass.
#[derive(Debug, Clone)]
pub(crate) struct StraightenedRun {
    pub(crate) old: StraightenedSegment,
    pub(crate) new: StraightenedSegment,
}

#[derive(Debug, Clone)]
pub(crate) struct StraightenedSegment {
    pub(crate) a: LayoutPoint,
    pub(crate) b: LayoutPoint,
}

#[derive(Debug)]
pub(crate) struct StraightenedRoute {
    pub(crate) route_index: usize,
    pub(crate) runs: Vec<StraightenedRun>,
}

struct StraightenedCandidate {
    points: Vec<LayoutPoint>,
    source_changed: bool,
    target_changed: bool,
    runs: Vec<StraightenedRun>,
}

/// Follow the nearest ORIGINAL moved run at the same clamped segment parameter. An unchanged
/// return leg may now lie closer, but it never owns the label whose original run moved.
pub(crate) fn project_label_onto_straightened_run(
    label: &LayoutPoint,
    runs: &[StraightenedRun],
) -> Option<LayoutPoint> {
    let mut best: Option<(f64, LayoutPoint)> = None;
    for run in runs {
        let dx = run.old.b.x - run.old.a.x;
        let dy = run.old.b.y - run.old.a.y;
        let length_squared = dx * dx + dy * dy;
        let parameter = if length_squared == 0.0 {
            0.0
        } else {
            (((label.x - run.old.a.x) * dx + (label.y - run.old.a.y) * dy) / length_squared)
                .clamp(0.0, 1.0)
        };
        let distance_x = run.old.a.x + parameter * dx - label.x;
        let distance_y = run.old.a.y + parameter * dy - label.y;
        let distance_squared = distance_x * distance_x + distance_y * distance_y;
        if best
            .as_ref()
            .is_none_or(|(distance, _)| distance_squared < *distance)
        {
            best = Some((
                distance_squared,
                LayoutPoint {
                    x: run.new.a.x + parameter * (run.new.b.x - run.new.a.x),
                    y: run.new.a.y + parameter * (run.new.b.y - run.new.a.y),
                },
            ));
        }
    }
    best.map(|(_, point)| point)
}

pub(crate) fn reproject_label(label: &mut crate::model::LayoutLabel, runs: &[StraightenedRun]) {
    if let Some(projected) = project_label_onto_straightened_run(
        &LayoutPoint {
            x: label.x,
            y: label.y,
        },
        runs,
    ) {
        label.x = projected.x;
        label.y = projected.y;
    }
}

/// Source's two-label pair correction. Supply None for absent/empty semantic label text.
/// This runs after terminal straightening even when that optional pass is disabled.
pub(crate) fn separate_opposite_edge_labels<'a>(
    edges: impl Iterator<Item = (&'a str, &'a str, Option<&'a mut crate::model::LayoutLabel>)>,
    mut charge: impl FnMut(usize) -> crate::Result<()>,
) -> crate::Result<()> {
    let mut pairs: std::collections::HashMap<_, Vec<&mut crate::model::LayoutLabel>> =
        std::collections::HashMap::new();
    for (source, target, label) in edges {
        charge(1)?;
        let Some(label) = label.filter(|label| label.width != 0.0) else {
            continue;
        };
        let key = if source <= target {
            (source, target)
        } else {
            (target, source)
        };
        pairs.entry(key).or_default().push(label);
    }
    // Reject before moving any label. Each pair is considered once, not against every edge.
    charge(pairs.len())?;
    for mut labels in pairs.into_values() {
        if labels.len() != 2 {
            continue;
        }
        let b = labels.pop().expect("two-label bucket has a second label");
        let a = labels.pop().expect("two-label bucket has a first label");
        if (a.y - b.y).abs() > a.height.max(b.height) {
            continue;
        }
        let required = a.width / 2.0 + b.width / 2.0 + 4.0;
        let current = b.x - a.x;
        let deficit = required - current.abs();
        if deficit <= 0.0 {
            continue;
        }
        let sign = if current == 0.0 {
            1.0
        } else {
            current.signum()
        };
        a.x -= sign * deficit / 2.0;
        b.x += sign * deficit / 2.0;
    }
    Ok(())
}

const JOG_EPS: f64 = 0.01;
const TERMINAL_JOG_MAX: f64 = 16.0;
const TERMINAL_RUN_MAX: f64 = 30.0;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Axis {
    Horizontal,
    Vertical,
}

fn axis(a: &LayoutPoint, b: &LayoutPoint) -> Option<Axis> {
    let dx = (a.x - b.x).abs();
    let dy = (a.y - b.y).abs();
    if dx > JOG_EPS && dy <= JOG_EPS {
        Some(Axis::Horizontal)
    } else if dy > JOG_EPS && dx <= JOG_EPS {
        Some(Axis::Vertical)
    } else {
        None
    }
}

fn straighten_front(points: &mut Vec<LayoutPoint>) -> Option<StraightenedRun> {
    if points.len() < 5 {
        return None;
    }
    let [p0, p1, p2, p3] = [&points[0], &points[1], &points[2], &points[3]];
    let direction = axis(p0, p1)?;
    let perpendicular = match direction {
        Axis::Horizontal => Axis::Vertical,
        Axis::Vertical => Axis::Horizontal,
    };
    if axis(p2, p3) != Some(direction)
        || axis(p1, p2) != Some(perpendicular)
        || (p1.x - p0.x).hypot(p1.y - p0.y) > TERMINAL_RUN_MAX
    {
        return None;
    }
    let (jog, forward) = match direction {
        Axis::Horizontal => (
            (p2.y - p1.y).abs(),
            (p1.x - p0.x).signum() == (p3.x - p2.x).signum(),
        ),
        Axis::Vertical => (
            (p2.x - p1.x).abs(),
            (p1.y - p0.y).signum() == (p3.y - p2.y).signum(),
        ),
    };
    if !(JOG_EPS..=TERMINAL_JOG_MAX).contains(&jog) || !forward {
        return None;
    }
    let mut last = 3;
    while last + 1 < points.len() && axis(&points[last], &points[last + 1]) == Some(direction) {
        last += 1;
    }
    if last == points.len() - 1 {
        return None;
    }
    let (x, y) = (p0.x, p0.y);
    let old = StraightenedSegment {
        a: points[2].clone(),
        b: points[last].clone(),
    };
    for point in &mut points[2..=last] {
        match direction {
            Axis::Horizontal => point.y = y,
            Axis::Vertical => point.x = x,
        }
    }
    let new = StraightenedSegment {
        a: points[2].clone(),
        b: points[last].clone(),
    };
    points.drain(1..3);
    Some(StraightenedRun { old, new })
}

fn straighten_terminal_jogs(points: &[LayoutPoint]) -> Option<StraightenedCandidate> {
    if points.len() < 5 {
        return None;
    }
    let mut candidate = points.to_vec();
    let front = straighten_front(&mut candidate);
    candidate.reverse();
    let back = straighten_front(&mut candidate);
    candidate.reverse();
    if front.is_none() && back.is_none() {
        return None;
    }
    Some(StraightenedCandidate {
        points: candidate,
        source_changed: front.is_some(),
        target_changed: back.is_some(),
        runs: front.into_iter().chain(back).collect(),
    })
}

fn crosses(a1: &LayoutPoint, a2: &LayoutPoint, b1: &LayoutPoint, b2: &LayoutPoint) -> bool {
    let side = |o: &LayoutPoint, p: &LayoutPoint, q: &LayoutPoint| {
        (p.x - o.x) * (q.y - o.y) - (p.y - o.y) * (q.x - o.x)
    };
    let opposite = |a: f64, b: f64| (a > 0.0 && b < 0.0) || (a < 0.0 && b > 0.0);
    opposite(side(b1, b2, a1), side(b1, b2, a2)) && opposite(side(a1, a2, b1), side(a1, a2, b2))
}

fn crossing_count(
    a: &[LayoutPoint],
    b: &impl TerminalRoute,
    origin_x: f64,
    origin_y: f64,
) -> usize {
    let (other_x, other_y) = b.origin();
    let dx = other_x - origin_x;
    let dy = other_y - origin_y;
    let mut count = 0;
    for pair in b.points().windows(2) {
        let b1 = LayoutPoint {
            x: pair[0].x + dx,
            y: pair[0].y + dy,
        };
        let b2 = LayoutPoint {
            x: pair[1].x + dx,
            y: pair[1].y + dy,
        };
        for segment in a.windows(2) {
            count += usize::from(crosses(&segment[0], &segment[1], &b1, &b2));
        }
    }
    count
}

/// Accept each candidate against the current routes, in source edge order. Ports never move.
/// Origin offsets put routes in the same coordinate frame without rewriting their stored points.
pub(crate) fn straighten_edge_terminals<R: TerminalRoute>(
    routes: &mut [R],
    charge: impl FnMut(usize) -> crate::Result<()>,
) -> crate::Result<()> {
    straighten_edge_terminals_with_runs(routes, charge).map(|_| ())
}

/// Return accepted moved runs for adapters whose labels live outside the route objects.
/// The indices follow input order and run coordinates retain each route's original origin.
pub(crate) fn straighten_edge_terminals_with_runs<R: TerminalRoute>(
    routes: &mut [R],
    mut charge: impl FnMut(usize) -> crate::Result<()>,
) -> crate::Result<Vec<StraightenedRoute>> {
    let mut changed = Vec::new();
    for index in 0..routes.len() {
        // Two run records at most, plus the candidate's point clone and traversal.
        charge(routes[index].points().len().saturating_add(3))?;
        let Some(StraightenedCandidate {
            points: candidate,
            source_changed,
            target_changed,
            runs,
        }) = straighten_terminal_jogs(routes[index].points())
        else {
            continue;
        };
        let original = &routes[index];
        let (origin_x, origin_y) = original.origin();
        let mut before = 0usize;
        let mut after = 0usize;
        // Visiting short or empty routes still costs work even though they have no crossings.
        charge(routes.len())?;
        for (other_index, other) in routes.iter().enumerate() {
            if other_index == index || other.points().len() < 2 {
                continue;
            }
            charge(
                original
                    .points()
                    .len()
                    .saturating_sub(1)
                    .saturating_add(candidate.len().saturating_sub(1))
                    .saturating_mul(other.points().len() - 1),
            )?;
            before =
                before.saturating_add(crossing_count(original.points(), other, origin_x, origin_y));
            after = after.saturating_add(crossing_count(&candidate, other, origin_x, origin_y));
        }
        if after <= before {
            charge(runs.len().saturating_add(1))?;
            routes[index].replace_terminal_points(candidate, source_changed, target_changed);
            routes[index].reproject_main_label(&runs);
            changed.push(StraightenedRoute {
                route_index: index,
                runs,
            });
        }
    }
    Ok(changed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::resources::OperationWorkMeter;
    use crate::{RenderResourcePolicy, ResourceLimitId};

    struct Route {
        points: Vec<LayoutPoint>,
        origin: (f64, f64),
        changed: (bool, bool),
    }

    impl TerminalRoute for Route {
        fn points(&self) -> &[LayoutPoint] {
            &self.points
        }
        fn origin(&self) -> (f64, f64) {
            self.origin
        }
        fn replace_terminal_points(
            &mut self,
            points: Vec<LayoutPoint>,
            source: bool,
            target: bool,
        ) {
            self.points = points;
            self.changed = (source, target);
        }
    }

    fn points(input: &[(f64, f64)]) -> Vec<LayoutPoint> {
        input.iter().map(|&(x, y)| LayoutPoint { x, y }).collect()
    }
    fn route(input: &[(f64, f64)], offset: (f64, f64)) -> Route {
        Route {
            points: points(input),
            origin: offset,
            changed: (false, false),
        }
    }
    fn coords(points: &[LayoutPoint]) -> Vec<(f64, f64)> {
        points.iter().map(|p| (p.x, p.y)).collect()
    }
    fn meter() -> OperationWorkMeter {
        OperationWorkMeter::new(RenderResourcePolicy::unbounded_for_trusted_input())
    }

    #[test]
    fn upstream_terminal_jog_cases_keep_ports_and_move_the_whole_run() {
        // Pinned geometry.spec.ts cases, also reflected into the vertical/end orientations.
        for (input, expected) in [
            (
                vec![
                    (193., 116.25),
                    (218., 116.25),
                    (218., 119.5),
                    (400., 119.5),
                    (400., 300.),
                ],
                vec![(193., 116.25), (400., 116.25), (400., 300.)],
            ),
            (
                vec![
                    (193., 116.25),
                    (218., 116.25),
                    (218., 119.5),
                    (300., 119.5),
                    (400., 119.5),
                    (400., 300.),
                ],
                vec![(193., 116.25), (300., 116.25), (400., 116.25), (400., 300.)],
            ),
            (
                vec![
                    (193., 116.5),
                    (218., 116.5),
                    (218., 117.358),
                    (400., 117.358),
                    (400., 300.),
                ],
                vec![(193., 116.5), (400., 116.5), (400., 300.)],
            ),
        ] {
            for vertical in [false, true] {
                for reverse in [false, true] {
                    let transform = |input: &[(f64, f64)]| {
                        let mut result = points(input);
                        if vertical {
                            for p in &mut result {
                                std::mem::swap(&mut p.x, &mut p.y);
                            }
                        }
                        if reverse {
                            result.reverse();
                        }
                        result
                    };
                    let input = transform(&input);
                    let expected = transform(&expected);
                    assert_eq!(
                        coords(&straighten_terminal_jogs(&input).unwrap().points),
                        coords(&expected)
                    );
                    assert_eq!(coords(&input).first(), coords(&expected).first());
                    assert_eq!(coords(&input).last(), coords(&expected).last());
                }
            }
        }
    }

    #[test]
    fn upstream_real_turns_and_far_ports_are_not_connectors() {
        for input in [
            vec![
                (193., 116.25),
                (218., 116.25),
                (218., 119.5),
                (300., 119.5),
                (400., 119.5),
            ],
            vec![
                (193., 100.),
                (218., 100.),
                (218., 140.),
                (400., 140.),
                (400., 300.),
            ],
            vec![
                (193., 116.25),
                (313., 116.25),
                (313., 119.5),
                (415., 119.5),
                (415., 300.),
            ],
            vec![
                (193., 116.),
                (218., 116.),
                (218., 119.),
                (100., 119.),
                (100., 300.),
            ],
            vec![(193., 120.), (315., 120.)],
        ] {
            assert!(straighten_terminal_jogs(&points(&input)).is_none());
        }
    }

    #[test]
    fn graph_pass_rejects_new_crossings_across_different_route_origins() {
        let original = [(0., 0.), (20., 0.), (20., 5.), (100., 5.), (100., 50.)];
        // The other route is globally x=60, y=-1..1, so moving the channel onto y=0 crosses it.
        let mut routes = [
            route(&original, (0., 0.)),
            route(&[(10., 9.), (10., 11.)], (50., -10.)),
        ];
        let work = meter();
        straighten_edge_terminals(&mut routes, |units| work.charge(units).map_err(Into::into))
            .unwrap();
        assert_eq!(coords(&routes[0].points), original);
        // A contact at the other route's endpoint is not a strict crossing.
        routes[1].points[0].y = 10.;
        let work = meter();
        straighten_edge_terminals(&mut routes, |units| work.charge(units).map_err(Into::into))
            .unwrap();
        assert_eq!(
            coords(&routes[0].points),
            [(0., 0.), (100., 0.), (100., 50.)]
        );
    }

    #[test]
    fn graph_pass_budget_rejection_does_not_accept_a_candidate() {
        let original = [(0., 0.), (20., 0.), (20., 5.), (100., 5.), (100., 50.)];
        let mut routes = [
            route(&original, (0., 0.)),
            route(&[(60., -1.), (60., 1.)], (0., 0.)),
        ];
        let work = OperationWorkMeter::new(
            RenderResourcePolicy::unbounded_for_trusted_input()
                .with_limit(ResourceLimitId::MaxLayoutWorkUnits, 6)
                .unwrap(),
        );
        assert!(matches!(
            straighten_edge_terminals(&mut routes, |units| work.charge(units).map_err(Into::into)),
            Err(crate::Error::ResourceLimitExceeded(_))
        ));
        assert_eq!(coords(&routes[0].points), original);
    }

    #[test]
    fn empty_comparison_routes_are_metered_before_candidate_acceptance() {
        let original = points(&[
            (0.0, 0.0),
            (20.0, 0.0),
            (20.0, 5.0),
            (100.0, 5.0),
            (100.0, 50.0),
        ]);
        let mut routes = vec![original.clone()];
        routes.resize_with(129, Vec::new);
        let work = OperationWorkMeter::new(
            RenderResourcePolicy::unbounded_for_trusted_input()
                .with_limit(ResourceLimitId::MaxLayoutWorkUnits, 64)
                .unwrap(),
        );
        assert!(matches!(
            straighten_edge_terminals(&mut routes, |units| work.charge(units).map_err(Into::into)),
            Err(crate::Error::ResourceLimitExceeded(_))
        ));
        assert_eq!(coords(&routes[0]), coords(&original));
    }

    fn layout_edge(
        id: &str,
        from: &str,
        to: &str,
        points: Vec<LayoutPoint>,
        label: Option<crate::model::LayoutLabel>,
    ) -> crate::model::LayoutEdge {
        crate::model::LayoutEdge {
            id: id.to_owned(),
            from: from.to_owned(),
            to: to.to_owned(),
            points,
            label,
            from_cluster: None,
            to_cluster: None,
            start_label_left: None,
            start_label_right: None,
            end_label_left: None,
            end_label_right: None,
            start_marker: None,
            end_marker: None,
            stroke_dasharray: None,
        }
    }

    fn label(x: f64, y: f64, width: f64, height: f64) -> crate::model::LayoutLabel {
        crate::model::LayoutLabel {
            x,
            y,
            width,
            height,
        }
    }

    fn label_jog() -> Vec<LayoutPoint> {
        points(&[
            (193.0, 116.25),
            (218.0, 116.25),
            (218.0, 119.5),
            (300.0, 119.5),
            (400.0, 119.5),
            (400.0, 300.0),
        ])
    }

    #[test]
    fn moved_run_reports_match_upstream_and_layout_edges_project_their_main_labels() {
        let mut edges = [layout_edge(
            "e",
            "A",
            "B",
            label_jog(),
            Some(label(300.0, 119.5, 40.0, 20.0)),
        )];
        let work = meter();
        let changed = straighten_edge_terminals_with_runs(&mut edges, |units| {
            work.charge(units).map_err(Into::into)
        })
        .unwrap();
        assert_eq!(changed.len(), 1);
        assert_eq!(changed[0].route_index, 0);
        assert_eq!(changed[0].runs.len(), 1);
        let run = &changed[0].runs[0];
        assert_eq!(
            coords(&[run.old.a.clone(), run.old.b.clone()]),
            [(218.0, 119.5), (400.0, 119.5)]
        );
        assert_eq!(
            coords(&[run.new.a.clone(), run.new.b.clone()]),
            [(218.0, 116.25), (400.0, 116.25)]
        );
        let label = edges[0].label.as_ref().unwrap();
        assert_eq!(
            (label.x, label.y, label.width, label.height),
            (300.0, 116.25, 40.0, 20.0)
        );
        assert_eq!(
            coords(&edges[0].points),
            [
                (193.0, 116.25),
                (300.0, 116.25),
                (400.0, 116.25),
                (400.0, 300.0)
            ]
        );
    }

    #[test]
    fn projection_does_not_snap_to_a_nearer_unchanged_return_leg() {
        let mut route = label_jog();
        route.extend(points(&[(350.0, 300.0), (250.0, 118.0), (350.0, 118.0)]));
        let mut routes = [route];
        let work = meter();
        let changed = straighten_edge_terminals_with_runs(&mut routes, |units| {
            work.charge(units).map_err(Into::into)
        })
        .unwrap();
        let projected = project_label_onto_straightened_run(
            &LayoutPoint { x: 300.0, y: 119.5 },
            &changed[0].runs,
        )
        .unwrap();
        assert_eq!((projected.x, projected.y), (300.0, 116.25));
        assert_eq!(
            coords(&routes[0][routes[0].len() - 2..]),
            [(250.0, 118.0), (350.0, 118.0)]
        );
    }

    #[test]
    fn moved_run_projection_handles_both_ends_axes_and_route_origins() {
        for vertical in [false, true] {
            for reverse in [false, true] {
                let mut input = label_jog();
                let mut old_label = LayoutPoint { x: 300.0, y: 119.5 };
                let mut expected = LayoutPoint {
                    x: 300.0,
                    y: 116.25,
                };
                if vertical {
                    for point in &mut input {
                        std::mem::swap(&mut point.x, &mut point.y);
                    }
                    std::mem::swap(&mut old_label.x, &mut old_label.y);
                    std::mem::swap(&mut expected.x, &mut expected.y);
                }
                if reverse {
                    input.reverse();
                }
                let mut routes = [Route {
                    points: input,
                    origin: (500.0, -100.0),
                    changed: (false, false),
                }];
                let work = meter();
                let changed = straighten_edge_terminals_with_runs(&mut routes, |units| {
                    work.charge(units).map_err(Into::into)
                })
                .unwrap();
                let projected =
                    project_label_onto_straightened_run(&old_label, &changed[0].runs).unwrap();
                assert_eq!((projected.x, projected.y), (expected.x, expected.y));
                assert_eq!(routes[0].origin, (500.0, -100.0));
                assert_eq!(routes[0].changed, (!reverse, reverse));
            }
        }
    }

    #[test]
    fn label_projection_clamps_to_the_old_segment_and_selects_the_nearest_old_run() {
        let horizontal = StraightenedRun {
            old: StraightenedSegment {
                a: LayoutPoint { x: 10.0, y: 5.0 },
                b: LayoutPoint { x: 100.0, y: 5.0 },
            },
            new: StraightenedSegment {
                a: LayoutPoint { x: 10.0, y: 0.0 },
                b: LayoutPoint { x: 100.0, y: 0.0 },
            },
        };
        let vertical = StraightenedRun {
            old: StraightenedSegment {
                a: LayoutPoint { x: 105.0, y: 20.0 },
                b: LayoutPoint { x: 105.0, y: 100.0 },
            },
            new: StraightenedSegment {
                a: LayoutPoint { x: 100.0, y: 20.0 },
                b: LayoutPoint { x: 100.0, y: 100.0 },
            },
        };
        let runs = [horizontal, vertical];
        for (input, expected) in [
            ((0.0, 5.0), (10.0, 0.0)),
            ((50.0, 5.0), (50.0, 0.0)),
            ((105.0, 60.0), (100.0, 60.0)),
            ((105.0, 150.0), (100.0, 100.0)),
        ] {
            let projected = project_label_onto_straightened_run(
                &LayoutPoint {
                    x: input.0,
                    y: input.1,
                },
                &runs,
            )
            .unwrap();
            assert_eq!((projected.x, projected.y), expected);
        }
        assert!(
            project_label_onto_straightened_run(&LayoutPoint { x: 0.0, y: 0.0 }, &[]).is_none()
        );
    }

    #[test]
    fn rejected_jogs_and_budget_failures_do_not_move_main_labels() {
        let original = points(&[
            (0.0, 0.0),
            (20.0, 0.0),
            (20.0, 5.0),
            (100.0, 5.0),
            (100.0, 50.0),
        ]);
        let mut edges = [
            layout_edge(
                "jog",
                "A",
                "B",
                original.clone(),
                Some(label(50.0, 5.0, 20.0, 10.0)),
            ),
            layout_edge(
                "blocker",
                "C",
                "D",
                points(&[(60.0, -1.0), (60.0, 1.0)]),
                None,
            ),
        ];
        let work = meter();
        let changed = straighten_edge_terminals_with_runs(&mut edges, |units| {
            work.charge(units).map_err(Into::into)
        })
        .unwrap();
        assert!(changed.is_empty());
        assert_eq!(coords(&edges[0].points), coords(&original));
        assert_eq!(edges[0].label.as_ref().unwrap().y, 5.0);
        let work = OperationWorkMeter::new(
            RenderResourcePolicy::unbounded_for_trusted_input()
                .with_limit(ResourceLimitId::MaxLayoutWorkUnits, 1)
                .unwrap(),
        );
        assert!(
            straighten_edge_terminals(&mut edges, |units| work.charge(units).map_err(Into::into))
                .is_err()
        );
        assert_eq!(edges[0].label.as_ref().unwrap().y, 5.0);
    }

    fn separate(labels: &mut [crate::model::LayoutLabel]) {
        let work = meter();
        separate_opposite_edge_labels(
            labels.iter_mut().enumerate().map(|(index, label)| {
                if index % 2 == 0 {
                    ("A", "B", Some(label))
                } else {
                    ("B", "A", Some(label))
                }
            }),
            |units| work.charge(units).map_err(Into::into),
        )
        .unwrap();
    }

    #[test]
    fn opposite_main_labels_separate_symmetrically_with_the_upstream_four_pixel_gap() {
        let mut labels = [
            label(492.933, 489.5, 34.25, 21.0),
            label(531.067, 489.5, 48.266, 21.0),
        ];
        let midpoint = (labels[0].x + labels[1].x) / 2.0;
        separate(&mut labels);
        assert!(((labels[1].x - labels[0].x) - (34.25 / 2.0 + 48.266 / 2.0 + 4.0)).abs() < 1e-9);
        assert!(((labels[0].x + labels[1].x) / 2.0 - midpoint).abs() < 1e-9);
        let mut coincident = [label(100.0, 0.0, 20.0, 10.0), label(100.0, 0.0, 40.0, 10.0)];
        separate(&mut coincident);
        assert_eq!((coincident[0].x, coincident[1].x), (83.0, 117.0));
    }

    #[test]
    fn opposite_label_separation_leaves_clear_multi_edge_and_different_height_pairs_unchanged() {
        for mut labels in [
            vec![
                label(284.933, 489.5, 25.6875, 21.0),
                label(323.067, 489.5, 38.15625, 21.0),
            ],
            vec![
                label(100.0, 0.0, 80.0, 21.0),
                label(110.0, 200.0, 80.0, 21.0),
            ],
            vec![
                label(100.0, 0.0, 80.0, 21.0),
                label(110.0, 0.0, 80.0, 21.0),
                label(105.0, 0.0, 80.0, 21.0),
            ],
            vec![label(100.0, 0.0, 0.0, 21.0), label(110.0, 0.0, 80.0, 21.0)],
        ] {
            let before: Vec<_> = labels.iter().map(|label| (label.x, label.y)).collect();
            separate(&mut labels);
            assert_eq!(
                labels
                    .iter()
                    .map(|label| (label.x, label.y))
                    .collect::<Vec<_>>(),
                before
            );
        }
    }

    #[test]
    fn opposite_label_separation_rejects_before_moving_any_pair() {
        let mut labels = [label(100.0, 0.0, 80.0, 21.0), label(110.0, 0.0, 80.0, 21.0)];
        let work = OperationWorkMeter::new(
            RenderResourcePolicy::unbounded_for_trusted_input()
                .with_limit(ResourceLimitId::MaxLayoutWorkUnits, 2)
                .unwrap(),
        );
        assert!(
            separate_opposite_edge_labels(
                labels.iter_mut().map(|label| ("A", "B", Some(label))),
                |units| work.charge(units).map_err(Into::into)
            )
            .is_err()
        );
        assert_eq!((labels[0].x, labels[1].x), (100.0, 110.0));
    }
}
