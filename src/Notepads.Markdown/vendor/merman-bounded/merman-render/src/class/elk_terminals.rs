//! Class terminal-label placement after ELK routes are clipped and straightened.
//!
//! Source: mermaid@12.1.0, rendering-util/layout-algorithms/elk/render.ts:
//! followMovedEndpoints, slideTerminalLabelsOffFrames, putTerminalLabelsOnTheirSide.

use crate::Result;
use crate::model::{LayoutEdge, LayoutLabel, LayoutNode, LayoutPoint as P};
use merman_layout_elk::TerminalLabelKey as Key;

#[derive(Default)]
pub(super) struct PlacedTerminals {
    pub(super) keys: Vec<Key>,
    pub(super) ports: Option<(P, P)>,
}

pub(super) fn label(edge: &LayoutEdge, key: Key) -> Option<&LayoutLabel> {
    match key {
        Key::StartLeft => edge.start_label_left.as_ref(),
        Key::StartRight => edge.start_label_right.as_ref(),
        Key::EndLeft => edge.end_label_left.as_ref(),
        Key::EndRight => edge.end_label_right.as_ref(),
    }
}

pub(super) fn label_slot(edge: &mut LayoutEdge, key: Key) -> &mut Option<LayoutLabel> {
    match key {
        Key::StartLeft => &mut edge.start_label_left,
        Key::StartRight => &mut edge.start_label_right,
        Key::EndLeft => &mut edge.end_label_left,
        Key::EndRight => &mut edge.end_label_right,
    }
}

pub(super) fn follow_moved_endpoints(edge: &mut LayoutEdge, placed: &PlacedTerminals) {
    let Some((original_start, original_end)) = &placed.ports else {
        return;
    };
    let (Some(start), Some(end)) = (edge.points.first(), edge.points.last()) else {
        return;
    };
    let start_delta = (start.x - original_start.x, start.y - original_start.y);
    let end_delta = (end.x - original_end.x, end.y - original_end.y);
    for &key in &placed.keys {
        let delta = if key.at_start() {
            start_delta
        } else {
            end_delta
        };
        if let Some(label) = label_slot(edge, key) {
            label.x += delta.0;
            label.y += delta.1;
        }
    }
}

#[derive(Clone, Copy)]
struct Box2 {
    x1: f64,
    y1: f64,
    x2: f64,
    y2: f64,
}

impl Box2 {
    fn centered(x: f64, y: f64, width: f64, height: f64) -> Self {
        Self {
            x1: x - width / 2.0,
            y1: y - height / 2.0,
            x2: x + width / 2.0,
            y2: y + height / 2.0,
        }
    }

    fn label(label: &LayoutLabel) -> Self {
        Self::centered(label.x, label.y, label.width, label.height)
    }

    fn overlaps(self, other: Self) -> bool {
        self.x1 < other.x2 && other.x1 < self.x2 && self.y1 < other.y2 && other.y1 < self.y2
    }

    fn padded(self, clearance: f64) -> Self {
        Self {
            x1: self.x1 - clearance,
            y1: self.y1 - clearance,
            x2: self.x2 + clearance,
            y2: self.y2 + clearance,
        }
    }
}

fn segment_hits_box(a: &P, b: &P, rect: Box2) -> bool {
    let mut t0: f64 = 0.0;
    let mut t1: f64 = 1.0;
    let dx = b.x - a.x;
    let dy = b.y - a.y;
    for (p, q) in [
        (-dx, a.x - rect.x1),
        (dx, rect.x2 - a.x),
        (-dy, a.y - rect.y1),
        (dy, rect.y2 - a.y),
    ] {
        if p == 0.0 {
            if q < 0.0 {
                return false;
            }
        } else if p < 0.0 {
            t0 = t0.max(q / p);
        } else {
            t1 = t1.min(q / p);
        }
    }
    t0 < t1
}

struct Obstacles {
    nodes: Vec<Box2>,
    frames: Vec<(P, P)>,
    center_labels: Vec<Box2>,
}

impl Obstacles {
    fn new(
        edges: &[LayoutEdge],
        nodes: &[LayoutNode],
        charge: &mut impl FnMut(usize) -> Result<()>,
    ) -> Result<Self> {
        let mut boxes = Vec::new();
        let mut frames = Vec::new();
        for node in nodes {
            charge(4)?;
            let rect = Box2::centered(node.x, node.y, node.width, node.height);
            if node.is_cluster {
                let [a, b, c, d] = [
                    P {
                        x: rect.x1,
                        y: rect.y1,
                    },
                    P {
                        x: rect.x2,
                        y: rect.y1,
                    },
                    P {
                        x: rect.x2,
                        y: rect.y2,
                    },
                    P {
                        x: rect.x1,
                        y: rect.y2,
                    },
                ];
                frames.extend([
                    (a.clone(), b.clone()),
                    (b, c.clone()),
                    (c, d.clone()),
                    (d, a),
                ]);
            } else {
                boxes.push(rect);
            }
        }
        let mut center_labels = Vec::new();
        for edge in edges {
            charge(1)?;
            if let Some(label) = &edge.label
                && label.width != 0.0
                && label.height != 0.0
            {
                center_labels.push(Box2::label(label));
            }
        }
        Ok(Self {
            nodes: boxes,
            frames,
            center_labels,
        })
    }

    fn hits_frame(
        &self,
        rect: Box2,
        clearance: f64,
        charge: &mut impl FnMut(usize) -> Result<()>,
    ) -> Result<bool> {
        let rect = rect.padded(clearance);
        for (a, b) in &self.frames {
            charge(1)?;
            if segment_hits_box(a, b, rect) {
                return Ok(true);
            }
        }
        Ok(false)
    }

    #[allow(clippy::too_many_arguments)]
    fn blocked(
        &self,
        rect: Box2,
        edges: &[LayoutEdge],
        placed: &[PlacedTerminals],
        edge_index: usize,
        key: Key,
        segment: usize,
        clearance: f64,
        charge: &mut impl FnMut(usize) -> Result<()>,
    ) -> Result<bool> {
        for &other in self.nodes.iter().chain(&self.center_labels) {
            charge(1)?;
            if rect.overlaps(other) {
                return Ok(true);
            }
        }
        if self.hits_frame(rect, clearance, charge)? {
            return Ok(true);
        }
        for (other_index, edge) in edges.iter().enumerate() {
            charge(1)?;
            for (index, pair) in edge.points.windows(2).enumerate() {
                charge(1)?;
                if !(other_index == edge_index && index == segment)
                    && segment_hits_box(&pair[0], &pair[1], rect)
                {
                    return Ok(true);
                }
            }
        }
        for (other_index, (edge, placed)) in edges.iter().zip(placed).enumerate() {
            charge(1)?;
            for &other_key in &placed.keys {
                charge(1)?;
                if !(other_index == edge_index && other_key == key)
                    && let Some(other) = label(edge, other_key)
                    && rect.overlaps(Box2::label(other))
                {
                    return Ok(true);
                }
            }
        }
        Ok(false)
    }
}

fn terminal_segment(edge: &LayoutEdge, key: Key) -> Option<(usize, &P, &P, f64)> {
    if edge.points.len() < 2 {
        return None;
    }
    let index = if key.at_start() {
        0
    } else {
        edge.points.len() - 2
    };
    let from = &edge.points[index];
    let to = &edge.points[index + 1];
    let length = (to.x - from.x).hypot(to.y - from.y);
    (length > 0.0 && length.is_finite()).then_some((index, from, to, length))
}

pub(super) fn slide_off_frames(
    edges: &mut [LayoutEdge],
    placed: &[PlacedTerminals],
    nodes: &[LayoutNode],
    mut charge: impl FnMut(usize) -> Result<()>,
) -> Result<()> {
    let obstacles = Obstacles::new(edges, nodes, &mut charge)?;
    for (edge_index, placed_edge) in placed.iter().enumerate() {
        charge(1)?;
        for &key in &placed_edge.keys {
            let edge = &edges[edge_index];
            let Some(center) = label(edge, key) else {
                continue;
            };
            if !obstacles.hits_frame(Box2::label(center), 0.0, &mut charge)? {
                continue;
            }
            let Some((segment, from, to, length)) = terminal_segment(edge, key) else {
                continue;
            };
            let (far, end) = if key.at_start() {
                (to, from)
            } else {
                (from, to)
            };
            let direction = ((end.x - far.x) / length, (end.y - far.y) / length);
            let half_extent =
                (direction.0.abs() * center.width + direction.1.abs() * center.height) / 2.0;
            let mut moved = None;
            let mut step = 1.0;
            while step <= length {
                charge(1)?;
                for shift in [step, -step] {
                    let x = center.x + shift * direction.0;
                    let y = center.y + shift * direction.1;
                    let along = (x - end.x) * direction.0 + (y - end.y) * direction.1;
                    if along + half_extent <= 0.0
                        && along - half_extent >= -length
                        && !obstacles.blocked(
                            Box2::centered(x, y, center.width, center.height),
                            edges,
                            placed,
                            edge_index,
                            key,
                            segment,
                            2.0,
                            &mut charge,
                        )?
                    {
                        moved = Some((x, y));
                        break;
                    }
                }
                if moved.is_some() {
                    break;
                }
                // Finite coordinates beyond exact integer precision must still terminate.
                let next = step + 1.0;
                if next == step {
                    break;
                }
                step = next;
            }
            if let Some((x, y)) = moved
                && let Some(label) = label_slot(&mut edges[edge_index], key)
            {
                label.x = x;
                label.y = y;
            }
        }
    }
    Ok(())
}

pub(super) fn put_on_own_side(
    edges: &mut [LayoutEdge],
    placed: &[PlacedTerminals],
    nodes: &[LayoutNode],
    mut charge: impl FnMut(usize) -> Result<()>,
) -> Result<()> {
    let obstacles = Obstacles::new(edges, nodes, &mut charge)?;
    for (edge_index, placed_edge) in placed.iter().enumerate() {
        charge(1)?;
        for &key in &placed_edge.keys {
            let edge = &edges[edge_index];
            let Some(center) = label(edge, key) else {
                continue;
            };
            let Some((segment, from, to, length)) = terminal_segment(edge, key) else {
                continue;
            };
            let normal = (-(to.y - from.y) / length, (to.x - from.x) / length);
            let offset = (center.x - from.x) * normal.0 + (center.y - from.y) * normal.1;
            if offset == 0.0 || (offset > 0.0) == key.on_right() {
                continue;
            }
            let x = center.x - 2.0 * offset * normal.0;
            let y = center.y - 2.0 * offset * normal.1;
            if !obstacles.blocked(
                Box2::centered(x, y, center.width, center.height),
                edges,
                placed,
                edge_index,
                key,
                segment,
                0.0,
                &mut charge,
            )? && let Some(label) = label_slot(&mut edges[edge_index], key)
            {
                label.x = x;
                label.y = y;
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn edge(points: &[(f64, f64)]) -> LayoutEdge {
        LayoutEdge {
            id: "edge".into(),
            from: "A".into(),
            to: "B".into(),
            from_cluster: None,
            to_cluster: None,
            points: points.iter().map(|&(x, y)| P { x, y }).collect(),
            label: None,
            start_label_left: None,
            start_label_right: None,
            end_label_left: None,
            end_label_right: None,
            start_marker: None,
            end_marker: None,
            stroke_dasharray: None,
        }
    }

    fn node(id: &str, x: f64, y: f64, width: f64, height: f64, is_cluster: bool) -> LayoutNode {
        LayoutNode {
            id: id.into(),
            x,
            y,
            width,
            height,
            is_cluster,
            label_width: None,
            label_height: None,
        }
    }

    #[test]
    fn placed_terminal_follows_only_its_moved_provider_port() {
        let mut edge = edge(&[(100.0, 58.0), (140.0, 58.0), (140.0, 20.0)]);
        edge.start_label_right = Some(LayoutLabel {
            x: 105.0,
            y: 70.0,
            width: 6.0,
            height: 16.5,
        });
        edge.end_label_left = Some(LayoutLabel {
            x: 150.0,
            y: 30.0,
            width: 6.0,
            height: 16.5,
        });
        follow_moved_endpoints(
            &mut edge,
            &PlacedTerminals {
                keys: vec![Key::StartRight, Key::EndLeft],
                ports: Some((P { x: 100.0, y: 50.0 }, P { x: 140.0, y: 20.0 })),
            },
        );
        assert_eq!(
            edge.start_label_right
                .as_ref()
                .map(|label| (label.x, label.y)),
            Some((105.0, 78.0))
        );
        assert_eq!(
            edge.end_label_left.as_ref().map(|label| (label.x, label.y)),
            Some((150.0, 30.0))
        );
    }

    #[test]
    fn frame_avoidance_never_slides_a_label_past_its_terminal_segment() {
        let mut edge = edge(&[(80.0, -40.0), (80.0, 0.0), (100.0, 0.0)]);
        edge.end_label_left = Some(LayoutLabel {
            x: 92.0,
            y: 12.0,
            width: 16.0,
            height: 16.0,
        });
        let mut edges = [edge];
        let placed = [PlacedTerminals {
            keys: vec![Key::EndLeft],
            ..Default::default()
        }];
        let nodes = [
            node("G", 145.0, 0.0, 110.0, 200.0, true),
            node("N", 130.0, 0.0, 60.0, 40.0, false),
        ];
        slide_off_frames(&mut edges, &placed, &nodes, |_| Ok(())).unwrap();
        assert_eq!(
            edges[0]
                .end_label_left
                .as_ref()
                .map(|label| (label.x, label.y)),
            Some((92.0, 12.0))
        );
    }

    #[test]
    fn frame_avoidance_uses_the_closest_free_integer_shift_with_source_clearance() {
        let mut edge = edge(&[(0.0, 0.0), (100.0, 0.0)]);
        edge.end_label_left = Some(LayoutLabel {
            x: 55.0,
            y: -10.0,
            width: 20.0,
            height: 10.0,
        });
        let mut edges = [edge];
        let placed = [PlacedTerminals {
            keys: vec![Key::EndLeft],
            ..Default::default()
        }];
        let nodes = [node("G", 50.0, 0.0, 20.0, 100.0, true)];
        slide_off_frames(&mut edges, &placed, &nodes, |_| Ok(())).unwrap();
        let label = edges[0].end_label_left.as_ref().unwrap();
        assert_eq!((label.x, label.y), (73.0, -10.0));
    }

    #[test]
    fn side_correction_preserves_an_adjacent_ports_reserved_channel() {
        for blocked in [false, true] {
            let mut first = edge(&[(0.0, 0.0), (100.0, 0.0)]);
            first.start_label_right = Some(LayoutLabel {
                x: 30.0,
                y: -12.0,
                width: 10.0,
                height: 10.0,
            });
            let mut edges = vec![first];
            let mut placed = vec![PlacedTerminals {
                keys: vec![Key::StartRight],
                ..Default::default()
            }];
            if blocked {
                edges.push(edge(&[(0.0, 12.0), (100.0, 12.0)]));
                placed.push(PlacedTerminals::default());
            }
            put_on_own_side(&mut edges, &placed, &[], |_| Ok(())).unwrap();
            assert_eq!(
                edges[0].start_label_right.as_ref().unwrap().y,
                if blocked { -12.0 } else { 12.0 }
            );
        }
    }

    #[test]
    fn frame_search_charges_candidate_steps_and_stops_on_cancellation() {
        let mut edge = edge(&[(0.0, 0.0), (1_000_000.0, 0.0)]);
        edge.end_label_left = Some(LayoutLabel {
            x: 50.0,
            y: -10.0,
            width: 20.0,
            height: 10.0,
        });
        let mut edges = [edge];
        let placed = [PlacedTerminals {
            keys: vec![Key::EndLeft],
            ..Default::default()
        }];
        let nodes = [node("frame", 50.0, 0.0, 20.0, 100.0, true)];
        let mut remaining = 12usize;
        let result = slide_off_frames(&mut edges, &placed, &nodes, |units| {
            remaining = remaining
                .checked_sub(units)
                .ok_or_else(|| crate::Error::InvalidModel {
                    message: "cancelled".into(),
                })?;
            Ok(())
        });
        assert!(result.is_err());
        assert_eq!(edges[0].end_label_left.as_ref().unwrap().x, 50.0);
    }
}
