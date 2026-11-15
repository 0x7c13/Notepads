//! Flowchart route ownership around the shared ELK terminal-channel pass.

use super::ClippedEdgeRoute;
use crate::elk_terminal_jogs::TerminalRoute;
use crate::model::LayoutPoint;
use crate::resources::OperationWorkMeter;

impl TerminalRoute for ClippedEdgeRoute {
    fn points(&self) -> &[LayoutPoint] {
        &self.points
    }

    fn origin(&self) -> (f64, f64) {
        (self.origin_x, self.origin_y)
    }

    fn reproject_main_label(&mut self, runs: &[crate::elk_terminal_jogs::StraightenedRun]) {
        if let Some(label) = self.label.as_mut() {
            crate::elk_terminal_jogs::reproject_label(label, runs);
        }
    }

    fn replace_terminal_points(
        &mut self,
        points: Vec<LayoutPoint>,
        source_changed: bool,
        target_changed: bool,
    ) {
        self.points = points;
        // Connector removal invalidates only its own rounded-corner adapter mask.
        if source_changed {
            self.elk_endpoint_adapters.source = false;
        }
        if target_changed {
            self.elk_endpoint_adapters.target = false;
        }
    }
}

pub(in crate::svg::parity::flowchart) fn straighten_edge_terminals(
    routes: &mut [ClippedEdgeRoute],
    work: &OperationWorkMeter,
) -> crate::Result<()> {
    crate::elk_terminal_jogs::straighten_edge_terminals(routes, |units| {
        work.charge(units).map_err(Into::into)
    })
}

/// Compare labels in absolute coordinates, then retain each owning route's local origin.
pub(in crate::svg::parity::flowchart) fn separate_edge_labels<'a>(
    routes: &mut [ClippedEdgeRoute],
    endpoints: impl Iterator<Item = (&'a str, &'a str, bool)>,
    work: &OperationWorkMeter,
) -> crate::Result<()> {
    work.charge(routes.len().saturating_mul(2))?;
    for route in routes.iter_mut() {
        if let Some(label) = route.label.as_mut() {
            label.x += route.origin_x;
            label.y += route.origin_y;
        }
    }
    let result = crate::elk_terminal_jogs::separate_opposite_edge_labels(
        endpoints
            .zip(routes.iter_mut())
            .map(|((from, to, has_label), route)| {
                (from, to, route.label.as_mut().filter(|_| has_label))
            }),
        |units| work.charge(units).map_err(Into::into),
    );
    for route in routes {
        if let Some(label) = route.label.as_mut() {
            label.x -= route.origin_x;
            label.y -= route.origin_y;
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::LayoutLabel;
    use crate::resources::{RenderResourcePolicy, ResourceLimitId};

    fn route(points: &[(f64, f64)], label: (f64, f64), origin: (f64, f64)) -> ClippedEdgeRoute {
        let points: Vec<_> = points.iter().map(|&(x, y)| LayoutPoint { x, y }).collect();
        ClippedEdgeRoute {
            base_points: points.clone(),
            points,
            origin_x: origin.0,
            origin_y: origin.1,
            label: Some(LayoutLabel {
                x: label.0,
                y: label.1,
                width: 20.0,
                height: 10.0,
            }),
            elk_endpoint_adapters: super::super::ElkEndpointAdapterCorners {
                source: true,
                target: true,
            },
        }
    }

    fn coords(points: &[LayoutPoint]) -> Vec<(f64, f64)> {
        points.iter().map(|point| (point.x, point.y)).collect()
    }

    #[test]
    fn terminal_projection_follows_the_local_run_and_preserves_its_group_origin() {
        for vertical in [false, true] {
            for reverse in [false, true] {
                let mut original = vec![
                    (0.0, 0.0),
                    (20.0, 0.0),
                    (20.0, 10.0),
                    (100.0, 10.0),
                    (100.0, 100.0),
                ];
                let mut label = (60.0, 10.0);
                let mut expected = (60.0, 0.0);
                if vertical {
                    for (x, y) in &mut original {
                        std::mem::swap(x, y);
                    }
                    std::mem::swap(&mut label.0, &mut label.1);
                    std::mem::swap(&mut expected.0, &mut expected.1);
                }
                if reverse {
                    original.reverse();
                }
                let mut routes = [route(&original, label, (700.0, 300.0))];
                let work =
                    OperationWorkMeter::new(RenderResourcePolicy::unbounded_for_trusted_input());
                straighten_edge_terminals(&mut routes, &work).unwrap();
                let updated = &routes[0];
                let label = updated.label.as_ref().unwrap();
                assert_eq!((label.x, label.y), expected);
                assert_eq!(updated.origin(), (700.0, 300.0));
                assert_eq!(coords(&updated.base_points), original);
                assert_eq!(updated.points.len(), 3);
                assert_eq!(updated.elk_endpoint_adapters.source, reverse);
                assert_eq!(updated.elk_endpoint_adapters.target, !reverse);
            }
        }
    }

    #[test]
    fn rejected_crossing_candidate_keeps_the_provider_label() {
        let original = [
            (0.0, 0.0),
            (20.0, 0.0),
            (20.0, 10.0),
            (100.0, 10.0),
            (100.0, 100.0),
        ];
        let mut routes = [
            route(&original, (60.0, 10.0), (0.0, 0.0)),
            route(&[(60.0, -5.0), (60.0, 5.0)], (60.0, 0.0), (0.0, 0.0)),
        ];
        let work = OperationWorkMeter::new(RenderResourcePolicy::unbounded_for_trusted_input());
        straighten_edge_terminals(&mut routes, &work).unwrap();
        assert_eq!(coords(&routes[0].points), original);
        let label = routes[0].label.as_ref().unwrap();
        assert_eq!((label.x, label.y), (60.0, 10.0));
    }

    #[test]
    fn paired_label_separation_compares_absolute_centres_and_restores_local_origins() {
        for limit in [None, Some(4)] {
            let mut routes = [
                route(&[(0.0, 0.0), (10.0, 0.0)], (0.0, 0.0), (100.0, 50.0)),
                route(&[(0.0, 0.0), (10.0, 0.0)], (-95.0, -30.0), (200.0, 80.0)),
            ];
            let mut policy = RenderResourcePolicy::unbounded_for_trusted_input();
            if let Some(limit) = limit {
                policy = policy
                    .with_limit(ResourceLimitId::MaxLayoutWorkUnits, limit)
                    .unwrap();
            }
            let work = OperationWorkMeter::new(policy);
            let result = separate_edge_labels(
                &mut routes,
                [("A", "B", true), ("B", "A", true)].into_iter(),
                &work,
            );
            let labels: Vec<_> = routes
                .iter()
                .map(|route| {
                    let label = route.label.as_ref().unwrap();
                    (label.x, label.y)
                })
                .collect();
            if limit.is_some() {
                assert!(result.is_err());
                assert_eq!(labels, [(0.0, 0.0), (-95.0, -30.0)]);
            } else {
                result.unwrap();
                assert_eq!(labels, [(-9.5, 0.0), (-85.5, -30.0)]);
            }
            assert_eq!(routes[0].origin(), (100.0, 50.0));
            assert_eq!(routes[1].origin(), (200.0, 80.0));
            for route in routes {
                assert_eq!(coords(&route.points), [(0.0, 0.0), (10.0, 0.0)]);
            }
        }
    }
}
