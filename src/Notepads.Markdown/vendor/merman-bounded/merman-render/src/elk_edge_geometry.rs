//! Shared shape projection of Mermaid 12 ELK's `geometry.ts` and `render.ts`.
//!
//! Provider routes remain immutable. These helpers operate on the separate paint projection.

use crate::model::{LayoutNode, LayoutPoint as P};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Outline {
    Rect,
    Ellipse,
    Diamond,
}

#[derive(Clone, Copy)]
pub(crate) struct Shape<'a> {
    pub node: &'a LayoutNode,
    pub outline: Outline,
    pub intersection: Option<&'a dyn Fn(&P) -> P>,
}

impl Shape<'_> {
    fn center(self) -> P {
        P {
            x: self.node.x,
            y: self.node.y,
        }
    }

    pub(crate) fn intersect(self, point: &P) -> P {
        if let Some(intersection) = self.intersection {
            return intersection(point);
        }
        let node = self.node;
        let dx = point.x - node.x;
        let dy = point.y - node.y;
        let w = node.width / 2.0;
        let h = node.height / 2.0;
        if self.outline == Outline::Ellipse {
            let det = (w * w * dy * dy + h * h * dx * dx).sqrt();
            return P {
                x: node.x + w * h * dx / det,
                y: node.y + w * h * dy / det,
            };
        }
        if self.outline == Outline::Diamond {
            // Mermaid 12 polygon intersection uses floating division without the old
            // half-pixel rounding bias. A diamond's center ray has this closed form.
            let scale = dx.abs() / w + dy.abs() / h;
            return if scale == 0.0 {
                self.center()
            } else {
                P {
                    x: node.x + dx / scale,
                    y: node.y + dy / scale,
                }
            };
        }
        if dy.abs() * w > dx.abs() * h {
            let h = if dy < 0.0 { -h } else { h };
            P {
                x: node.x + if dy == 0.0 { 0.0 } else { h * dx / dy },
                y: node.y + h,
            }
        } else {
            let w = if dx < 0.0 { -w } else { w };
            P {
                x: node.x + w,
                y: node.y + if dx == 0.0 { 0.0 } else { w * dy / dx },
            }
        }
    }

    fn outside(self, point: &P) -> bool {
        // Match Mermaid's outsideNode exactly: an inward-rounded port remains
        // inside, which can select a different departure and preserve a corner.
        (point.x - self.node.x).abs() >= self.node.width / 2.0
            || (point.y - self.node.y).abs() >= self.node.height / 2.0
    }

    fn border(self, point: &P, tolerance: f64) -> bool {
        let dx = (point.x - self.node.x).abs();
        let dy = (point.y - self.node.y).abs();
        ((dx - self.node.width / 2.0).abs() <= tolerance
            && dy <= self.node.height / 2.0 + tolerance)
            || ((dy - self.node.height / 2.0).abs() <= tolerance
                && dx <= self.node.width / 2.0 + tolerance)
    }

    fn inside_outline(self, point: &P) -> bool {
        let crossing = self.intersect(point);
        distance(point, &self.center()) <= distance(&crossing, &self.center()) + 1e-9
    }

    fn departure(self, port: &P, next: &P) -> Option<P> {
        if self.node.is_cluster {
            return None;
        }
        let dx = next.x - port.x;
        let dy = next.y - port.y;
        if (dx == 0.0 && dy == 0.0) || (dx.abs() > 1e-6 && dy.abs() > 1e-6) {
            return None;
        }
        let horizontal = dx.abs() > dy.abs();
        let along = |t| {
            if horizontal {
                P { x: t, y: port.y }
            } else {
                P { x: port.x, y: t }
            }
        };
        let mut inner = if horizontal { self.node.x } else { self.node.y };
        let mut outer = if horizontal { port.x } else { port.y };
        if !self.inside_outline(&along(inner)) {
            return None;
        }
        if self.inside_outline(&along(outer)) {
            return Some(port.clone());
        }
        for _ in 0..20 {
            let mid = (inner + outer) / 2.0;
            if self.inside_outline(&along(mid)) {
                inner = mid;
            } else {
                outer = mid;
            }
        }
        Some(along(inner))
    }

    fn compute_intersection(self, outside: &P, center: &P) -> P {
        let node = self.node;
        let mut outside = outside.clone();
        let dx = outside.x - node.x;
        let dy = outside.y - node.y;
        if (dx.abs() - node.width / 2.0).abs() < 1.0 || (dy.abs() - node.height / 2.0).abs() < 1.0 {
            let length = dx.hypot(dy);
            if length > 0.0 {
                outside = P {
                    x: node.x + dx / length * (length + 10.0),
                    y: node.y + dy / length * (length + 10.0),
                };
            }
        }
        let crossing = self.intersect(&outside);
        let wrong_side = (outside.x < node.x && crossing.x > node.x)
            || (outside.x > node.x && crossing.x < node.x);
        // Match JS comparisons for a degenerate ellipse's NaN intersection;
        // sanitize() owns the finite-point fallback after endpoint clipping.
        let rejected = wrong_side || distance(&outside, &crossing) <= 1.0;
        if !rejected {
            return crossing;
        }
        let inside = P {
            x: if (outside.x - node.x).abs() < 1.0 {
                outside.x
            } else if outside.x < node.x {
                node.x - node.width / 4.0
            } else {
                node.x + node.width / 4.0
            },
            y: if (outside.y - node.y).abs() < 1.0 {
                outside.y
            } else {
                center.y
            },
        };
        rect_segment_intersection(node, &outside, &inside)
    }
}

fn distance(a: &P, b: &P) -> f64 {
    (a.x - b.x).hypot(a.y - b.y)
}

fn rect_segment_intersection(node: &LayoutNode, outside: &P, inside: &P) -> P {
    let w = node.width / 2.0;
    let h = node.height / 2.0;
    let q_abs = (outside.y - inside.y).abs();
    let r_abs = (outside.x - inside.x).abs();
    let mut result;
    if (node.y - outside.y).abs() * w > (node.x - outside.x).abs() * h {
        let q = if inside.y < outside.y {
            outside.y - h - node.y
        } else {
            node.y - h - outside.y
        };
        let r = r_abs * q / q_abs;
        result = P {
            x: if inside.x < outside.x {
                inside.x + r
            } else {
                inside.x - r_abs + r
            },
            y: if inside.y < outside.y {
                inside.y + q_abs - q
            } else {
                inside.y - q_abs + q
            },
        };
    } else {
        let r = if inside.x < outside.x {
            outside.x - w - node.x
        } else {
            node.x - w - outside.x
        };
        let q = q_abs * r / r_abs;
        result = P {
            x: if inside.x < outside.x {
                inside.x + r_abs - r
            } else {
                inside.x - r_abs + r
            },
            y: if inside.y < outside.y {
                inside.y + q
            } else {
                inside.y - q
            },
        };
    }
    if r_abs == 0.0 {
        result.x = outside.x;
    }
    if q_abs == 0.0 {
        result.y = outside.y;
    }
    result
}

fn replace_endpoint(points: &mut Vec<P>, start: bool, value: P) {
    if points.is_empty() {
        return;
    }
    let index = if start { 0 } else { points.len() - 1 };
    if (points[index].x - value.x).abs() < 0.1 && (points[index].y - value.y).abs() < 0.1 {
        points.remove(index);
    } else {
        points[index] = value;
    }
}

fn group_clip(points: &mut Vec<P>, shape: Shape<'_>, start: bool) -> bool {
    let outside = if start {
        points.iter().position(|p| shape.outside(p))
    } else {
        points.iter().rposition(|p| shape.outside(p))
    };
    let Some(index) = outside else {
        return false;
    };
    if (start && index == 0) || (!start && index == points.len() - 1) {
        return false;
    }
    let outside = &points[index];
    let inside = &points[if start { index - 1 } else { index + 1 }];
    let dx = outside.x - inside.x;
    let dy = outside.y - inside.y;
    let tx = if dx == 0.0 {
        f64::INFINITY
    } else {
        (shape.node.x + dx.signum() * shape.node.width / 2.0 - inside.x) / dx
    };
    let ty = if dy == 0.0 {
        f64::INFINITY
    } else {
        (shape.node.y + dy.signum() * shape.node.height / 2.0 - inside.y) / dy
    };
    let t = tx.min(ty);
    let crossing = P {
        x: inside.x + t * dx,
        y: inside.y + t * dy,
    };
    if start {
        points.splice(..index, [crossing]);
    } else {
        points.splice(index + 1.., [crossing]);
    }
    true
}

fn apply_intersection(points: &mut Vec<P>, shape: Shape<'_>, start: bool) {
    let outside = if start {
        points.iter().find(|p| shape.outside(p))
    } else {
        points.iter().rfind(|p| shape.outside(p))
    };
    let center = if start { points.first() } else { points.last() };
    if let (Some(outside), Some(center)) = (outside, center) {
        let crossing = shape.compute_intersection(outside, center);
        replace_endpoint(points, start, crossing);
    }
}

fn cutter(points: &[P], start: Shape<'_>, end: Shape<'_>) -> Vec<P> {
    let mut out = points.to_vec();
    let (Some(start_center), Some(end_center)) = (points.first(), points.last()) else {
        return out;
    };
    if let Some(index) = out.iter().position(|p| start.outside(p)) {
        let intersection = out
            .get(index + 1)
            .and_then(|next| start.departure(&out[index], next))
            .unwrap_or_else(|| start.compute_intersection(&out[index], start_center));
        replace_endpoint(&mut out, true, intersection);
    }
    let outside = out
        .iter()
        .rposition(|p| end.outside(p))
        .or_else(|| out.len().checked_sub(2));
    if let Some(index) = outside {
        let intersection = index
            .checked_sub(1)
            .and_then(|next| end.departure(&out[index], &out[next]))
            .unwrap_or_else(|| end.compute_intersection(&out[index], end_center));
        replace_endpoint(&mut out, false, intersection);
    }
    if out.len() > 1 && distance(&out[out.len() - 1], &out[out.len() - 2]) < 2.0 {
        out.pop();
    }
    out
}

pub(crate) fn sanitize(points: &[P], start: Shape<'_>, end: Shape<'_>) -> Vec<P> {
    if points.is_empty() {
        return Vec::new();
    }
    let mut previous = points.to_vec();
    let center_approx = |point: &P, shape: Shape<'_>| {
        (point.x - shape.node.x).abs() < 1e-6 && (point.y - shape.node.y).abs() < 1e-6
    };
    let start_center = center_approx(&previous[0], start);
    let end_center = center_approx(&previous[previous.len() - 1], end);
    let start_candidate = if start_center && previous.len() > 1 {
        &previous[1]
    } else {
        &previous[0]
    };
    let end_candidate = if end_center && previous.len() > 1 {
        &previous[previous.len() - 2]
    } else {
        &previous[previous.len() - 1]
    };
    let mut skip_start = start.node.is_cluster && start.border(start_candidate, 0.5);
    let mut skip_end = end.node.is_cluster && end.border(end_candidate, 0.5);
    if skip_start && start_center {
        previous.remove(0);
    }
    if skip_end && end_center {
        previous.pop();
    }
    if start.node.is_cluster && !skip_start {
        skip_start = group_clip(&mut previous, start, true);
    }
    if end.node.is_cluster && !skip_end {
        skip_end = group_clip(&mut previous, end, false);
    }
    let mut clipped = if skip_start || skip_end {
        if !skip_start {
            apply_intersection(&mut previous, start, true);
        }
        if !skip_end {
            apply_intersection(&mut previous, end, false);
        }
        previous.clone()
    } else {
        cutter(&previous, start, end)
    };
    if clipped.len() < 2 || clipped.iter().any(|p| !p.x.is_finite() || !p.y.is_finite()) {
        let cleaned: Vec<_> = previous
            .iter()
            .filter(|p| p.x.is_finite() && p.y.is_finite())
            .cloned()
            .collect();
        clipped = if cleaned.len() >= 2 {
            cleaned
        } else {
            previous
        };
    }
    // Source compares with the original predecessor, not the previously retained point.
    clipped
        .iter()
        .enumerate()
        .filter(|(index, point)| {
            *index == 0
                || (point.x - clipped[index - 1].x).abs() > 1e-6
                || (point.y - clipped[index - 1].y).abs() > 1e-6
        })
        .map(|(_, p)| p.clone())
        .collect()
}

pub(crate) fn marker_segment(
    points: &mut Vec<P>,
    shape: Shape<'_>,
    marker: Option<&str>,
    start: bool,
) {
    let offset: f64 = match marker {
        Some("arrow_point") => 4.0,
        Some("arrow_barb_neo") => 5.5,
        Some("extension") => 17.25,
        Some("arrow_cross" | "arrow_circle") => 12.5,
        _ => 0.0,
    };
    if offset <= 0.0 || points.len() < 3 {
        return;
    }
    let (terminal, adjacent) = if start {
        (0, 1)
    } else {
        (points.len() - 1, points.len() - 2)
    };
    if distance(&points[terminal], &points[adjacent]) < 8.0_f64.max(offset * 2.0)
        && shape.border(&points[adjacent], 1.0)
    {
        points.remove(adjacent);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout_work::OperationLayoutWorkControl;
    use crate::resources::{OperationWorkMeter, RenderResourcePolicy};
    use std::sync::Arc;

    fn node(x: f64, y: f64, width: f64, height: f64, group: bool) -> LayoutNode {
        LayoutNode {
            id: "node".into(),
            x,
            y,
            width,
            height,
            is_cluster: group,
            label_width: None,
            label_height: None,
        }
    }

    fn points(values: &[(f64, f64)]) -> Vec<P> {
        values.iter().map(|&(x, y)| P { x, y }).collect()
    }

    fn coordinates(points: &[P]) -> Vec<(f64, f64)> {
        points.iter().map(|point| (point.x, point.y)).collect()
    }

    #[test]
    fn repeated_endpoint_is_removed_even_from_a_two_point_route() {
        // Mermaid 12 geometry.spec.ts: replacement removes an already matching endpoint.
        for (start, replacement, expected) in [
            (true, (0.0, 0.0), (1.0, 1.0)),
            (false, (1.0, 1.0), (0.0, 0.0)),
        ] {
            let mut route = points(&[(0.0, 0.0), (1.0, 1.0)]);
            replace_endpoint(
                &mut route,
                start,
                P {
                    x: replacement.0,
                    y: replacement.1,
                },
            );
            assert_eq!(coordinates(&route), [expected]);
        }
    }

    #[test]
    fn boundary_classification_preserves_the_source_comparison_at_one_ulp() {
        let terminal = node(1000.0, -2000.0, 144.0, 80.0, false);
        let shape = Shape {
            intersection: None,
            node: &terminal,
            outline: Outline::Rect,
        };
        for (x, y) in [
            (terminal.x + terminal.width / 2.0, terminal.y),
            (terminal.x - terminal.width / 2.0, terminal.y),
            (terminal.x, terminal.y + terminal.height / 2.0),
            (terminal.x, terminal.y - terminal.height / 2.0),
        ] {
            assert!(shape.outside(&P { x, y }));
            let toward_center = |value: f64, center: f64| {
                if value > center {
                    value.next_down()
                } else if value < center {
                    value.next_up()
                } else {
                    value
                }
            };
            assert!(!shape.outside(&P {
                x: toward_center(x, terminal.x),
                y: toward_center(y, terminal.y),
            }));
        }
    }

    #[test]
    fn ellipse_near_intersection_uses_the_shared_segment_fallback() {
        let node = node(0.0, 0.0, 100.0, 60.0, false);
        let shape = Shape {
            intersection: None,
            node: &node,
            outline: Outline::Ellipse,
        };
        let crossing = shape.compute_intersection(&P { x: 40.0, y: 18.0 }, &shape.center());
        assert_eq!((crossing.x, crossing.y), (50.0, -12.0));
    }

    #[test]
    fn offset_ellipse_ports_keep_the_provider_departure_axis() {
        let start_node = node(0.0, 0.0, 100.0, 60.0, false);
        let end_node = node(200.0, 15.0, 60.0, 60.0, false);
        let start = Shape {
            intersection: None,
            node: &start_node,
            outline: Outline::Ellipse,
        };
        let end = Shape {
            intersection: None,
            node: &end_node,
            outline: Outline::Rect,
        };
        let raw = points(&[
            (0.0, 0.0),
            (50.0, 15.0),
            (100.0, 15.0),
            (170.0, 15.0),
            (200.0, 15.0),
        ]);
        let clipped = sanitize(&raw, start, end);
        assert!((clipped[0].x - 50.0 * 0.75_f64.sqrt()).abs() < 0.0001);
        assert_eq!(clipped[0].y, 15.0);
        assert_eq!(clipped[1].y, 15.0);
        assert_eq!(coordinates(&raw)[1], (50.0, 15.0));
        let reversed = sanitize(&raw.iter().rev().cloned().collect::<Vec<_>>(), end, start);
        assert!((reversed.last().unwrap().x - clipped[0].x).abs() < 0.0001);
        assert_eq!(reversed.last().unwrap().y, 15.0);
    }

    #[test]
    fn changed_group_frame_uses_actual_crossing_segment_in_both_directions() {
        // Pinned render.spec.ts: crossing lies on the frame at y=130, not on its centre ray.
        let source_node = node(-100.0, 130.0, 40.0, 40.0, true);
        let target_node = node(100.0, 100.0, 100.0, 120.0, true);
        let source = Shape {
            intersection: None,
            node: &source_node,
            outline: Outline::Rect,
        };
        let target = Shape {
            intersection: None,
            node: &target_node,
            outline: Outline::Rect,
        };
        let raw = points(&[
            (-100.0, 130.0),
            (-80.0, 130.0),
            (20.0, 130.0),
            (53.0, 130.0),
            (60.0, 130.0),
            (100.0, 100.0),
        ]);
        let expected = [(-80.0, 130.0), (20.0, 130.0), (50.0, 130.0)];
        assert_eq!(coordinates(&sanitize(&raw, source, target)), expected);
        let reversed = raw.into_iter().rev().collect::<Vec<_>>();
        assert_eq!(
            coordinates(&sanitize(&reversed, target, source)),
            expected.into_iter().rev().collect::<Vec<_>>()
        );
    }

    #[test]
    fn terminal_jog_is_rejected_when_it_adds_a_crossing() {
        let raw = points(&[
            (193.0, 116.25),
            (218.0, 116.25),
            (218.0, 119.5),
            (400.0, 119.5),
            (400.0, 300.0),
        ]);
        let blocker = points(&[(300.0, 115.0), (300.0, 118.0)]);
        let mut routes = vec![raw.clone(), blocker];
        let mut work = OperationLayoutWorkControl::new(Arc::new(OperationWorkMeter::new(
            RenderResourcePolicy::default(),
        )));
        crate::elk_terminal_jogs::straighten_edge_terminals(&mut routes, |units| {
            work.charge_adapter(units)
        })
        .unwrap();
        assert_eq!(coordinates(&routes[0]), coordinates(&raw));
    }

    #[test]
    fn short_marker_stubs_are_removed_without_changing_ports() {
        let terminal = node(100.0, 100.0, 100.0, 100.0, false);
        let shape = Shape {
            intersection: None,
            node: &terminal,
            outline: Outline::Rect,
        };
        let mut route = points(&[(0.0, 90.0), (50.0, 90.0), (50.0, 100.0)]);
        marker_segment(&mut route, shape, Some("extension"), false);
        assert_eq!(coordinates(&route), [(0.0, 90.0), (50.0, 100.0)]);

        let mut neo_route = points(&[(0.0, 90.0), (50.0, 90.0), (50.0, 100.0)]);
        marker_segment(&mut neo_route, shape, Some("arrow_barb_neo"), false);
        assert_eq!(coordinates(&neo_route), [(0.0, 90.0), (50.0, 100.0)]);
    }

    #[test]
    fn endpoint_replacement_keeps_the_exact_source_threshold() {
        let mut route = points(&[(0.0, 0.0), (10.0, 0.0)]);
        replace_endpoint(&mut route, true, P { x: 0.1, y: 0.0 });
        assert_eq!(coordinates(&route), [(0.1, 0.0), (10.0, 0.0)]);
    }

    #[test]
    fn group_border_removes_auto_centers_and_deduplicates_original_neighbors() {
        let source_node = node(0.0, 0.0, 20.0, 20.0, true);
        let target_node = node(100.0, 0.0, 20.0, 20.0, true);
        let source = Shape {
            node: &source_node,
            outline: Outline::Rect,
            intersection: None,
        };
        let target = Shape {
            node: &target_node,
            outline: Outline::Rect,
            intersection: None,
        };
        let raw = points(&[
            (0.0, 0.0),
            (10.0, 0.0),
            (10.00000075, 0.0),
            (10.0000015, 0.0),
            (90.0, 0.0),
            (100.0, 0.0),
        ]);
        assert_eq!(
            coordinates(&sanitize(&raw, source, target)),
            [(10.0, 0.0), (90.0, 0.0)]
        );
        assert_eq!(raw.len(), 6, "provider route remains unchanged");
    }

    #[test]
    fn family_outline_callback_drives_departure_and_endpoint_intersections() {
        let source_node = node(0.0, 0.0, 100.0, 60.0, false);
        let target_node = node(200.0, 15.0, 60.0, 60.0, false);
        let ellipse = Shape {
            node: &source_node,
            outline: Outline::Ellipse,
            intersection: None,
        };
        let callback = |point: &P| ellipse.intersect(point);
        let source = Shape {
            node: &source_node,
            outline: Outline::Rect,
            intersection: Some(&callback),
        };
        let target = Shape {
            node: &target_node,
            outline: Outline::Rect,
            intersection: None,
        };
        let raw = points(&[
            (0.0, 0.0),
            (50.0, 15.0),
            (100.0, 15.0),
            (170.0, 15.0),
            (200.0, 15.0),
        ]);
        let actual = sanitize(&raw, source, target);
        assert_eq!(
            coordinates(&actual),
            coordinates(&sanitize(&raw, ellipse, target))
        );
        assert!((actual[0].x - 50.0 * 0.75_f64.sqrt()).abs() < 0.0001);
        assert_eq!(actual[0].y, 15.0);
    }

    #[test]
    fn invalid_family_intersections_restore_finite_provider_points() {
        let source_node = node(0.0, 0.0, 20.0, 20.0, false);
        let target_node = node(100.0, 0.0, 20.0, 20.0, false);
        let invalid = |_: &P| P {
            x: f64::NAN,
            y: f64::NAN,
        };
        let source = Shape {
            node: &source_node,
            outline: Outline::Rect,
            intersection: Some(&invalid),
        };
        let target = Shape {
            node: &target_node,
            outline: Outline::Rect,
            intersection: Some(&invalid),
        };
        let raw = points(&[(0.0, 0.0), (f64::NAN, 0.0), (50.0, 0.0), (100.0, 0.0)]);
        assert_eq!(
            coordinates(&sanitize(&raw, source, target)),
            [(0.0, 0.0), (50.0, 0.0), (100.0, 0.0)]
        );
        assert!(raw[1].x.is_nan());
    }
}
