// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

use super::*;

impl Layout<'_> {
    #[cfg(test)]
    pub(super) fn paint(
        self,
        provider: &dyn RichTextProvider,
    ) -> Result<OwnedScene, CanonicalError> {
        self.paint_with_relationship_style(provider, RelationshipStyle::default())
    }

    pub(super) fn paint_with_relationship_style(
        self,
        provider: &dyn RichTextProvider,
        relationships: RelationshipStyle<'_>,
    ) -> Result<OwnedScene, CanonicalError> {
        let mut scene = SceneBuilder::new(self.scope, 4 * 1024 * 1024, 65_536);
        // Ancestor backgrounds precede descendant backgrounds and all edges.
        for depth in 1..=8 {
            for cell in &self.cells {
                self.checkpoint(1)?;
                if cell.group.is_none() || cell.depth != depth {
                    continue;
                }
                scene.begin_path(cell.style.paint, Point::default())?;
                scene::shapes::outline(Shape::Rectangle, cell.bounds, &mut scene)?;
                scene.end_path()?;
                if let Some(label) = &cell.label {
                    text(
                        &mut scene,
                        provider,
                        label,
                        cell.style.color,
                        cell.style.paint.opacity,
                        cell.bounds.left + (cell.width - label.width) / 2.0,
                        cell.bounds.top + self.policy.padding + 3.0 * self.policy.clearance,
                    )?;
                }
            }
        }
        for (index, edge) in self.edges.iter().enumerate() {
            self.checkpoint(1)?;
            if self.model.edges[index].visibility == FlowEdgeVisibility::Invisible {
                continue;
            }
            if edge.route.len() < 2 {
                return Err(CanonicalError::InvalidGeometry);
            }
            let mut paint = edge.style.paint;
            paint.fill = None;
            scene.begin_path(paint, Point::default())?;
            scene.push(PathCommand::move_to(edge.route[0].x, edge.route[0].y))?;
            for p in &edge.route[1..] {
                scene.push(PathCommand::line_to(p.x, p.y))?;
            }
            scene.end_path()?;
            let last = edge.route.len() - 1;
            if let Some(relationship) = relationships.edges.get(index) {
                for (terminal, tip, inside) in [
                    (relationship.ends.start, 0, 1),
                    (relationship.ends.end, last, last - 1),
                ] {
                    relationship_terminal(
                        &mut scene,
                        terminal,
                        edge.route[tip],
                        edge.route[inside],
                        paint,
                        self.theme.background,
                    )?;
                }
            } else {
                marker(
                    &mut scene,
                    self.model.edges[index].start_marker,
                    edge.route[0],
                    edge.route[1],
                    paint,
                )?;
                marker(
                    &mut scene,
                    self.model.edges[index].end_marker,
                    edge.route[last],
                    edge.route[last - 1],
                    paint,
                )?;
            }
            if let Some(label) = &edge.label {
                let r = expand(edge.label_rect, 3.0);
                scene.begin_path(
                    Paint {
                        fill: Some(self.theme.background),
                        stroke: None,
                        opacity: edge.style.paint.opacity,
                        ..Paint::default()
                    },
                    Point::default(),
                )?;
                scene::shapes::outline(Shape::Rectangle, r, &mut scene)?;
                scene.end_path()?;
                text(
                    &mut scene,
                    provider,
                    label,
                    edge.style.color,
                    edge.style.paint.opacity,
                    edge.label_rect.left,
                    edge.label_rect.top,
                )?;
            }
        }
        for (edge, ports) in self.edges.iter().zip(self.relationship_ports.iter()) {
            self.checkpoint(2)?;
            let last = edge.route.len() - 1;
            for (label, tip, inside) in [
                (&ports.multiplicities[0], 0, 1),
                (&ports.multiplicities[1], last, last - 1),
            ] {
                if let Some(label) = label {
                    let (x, y) = multiplicity_origin(edge.route[tip], edge.route[inside], label)?;
                    text(
                        &mut scene,
                        provider,
                        label,
                        edge.style.color,
                        edge.style.paint.opacity,
                        x,
                        y,
                    )?;
                }
            }
        }
        for cell in &self.cells {
            self.checkpoint(1)?;
            let Some(node) = cell.node else {
                continue;
            };
            let final_state = cell.state_marker == Some(StateMarkerKind::Final);
            scene.begin_path(cell.style.paint, Point::default())?;
            scene::shapes::outline(
                if final_state {
                    Shape::Circle
                } else {
                    cell.shape
                },
                cell.bounds,
                &mut scene,
            )?;
            scene.end_path()?;
            if final_state {
                scene.begin_path(
                    Paint {
                        fill: Some(self.theme.text),
                        stroke: None,
                        ..Paint::default()
                    },
                    Point::default(),
                )?;
                scene::shapes::outline(Shape::Circle, expand(cell.bounds, -4.5), &mut scene)?;
                scene.end_path()?;
            }
            if let Some(label) = &cell.label {
                let label_top = cell.bounds.top + (cell.height - label.height) / 2.0;
                if let Some(Some(paragraph)) = relationships.header_ends.get(node)
                    && let Some(offset) = label.paragraph_end(*paragraph)?
                {
                    let y = label_top + offset;
                    let mut paint = cell.style.paint;
                    paint.fill = None;
                    scene.begin_path(paint, Point::default())?;
                    scene.push(PathCommand::move_to(cell.bounds.left, y))?;
                    scene.push(PathCommand::line_to(cell.bounds.right, y))?;
                    scene.end_path()?;
                }
                text(
                    &mut scene,
                    provider,
                    label,
                    cell.style.color,
                    cell.style.paint.opacity,
                    cell.bounds.left + (cell.width - label.width) / 2.0,
                    label_top,
                )?;
            }
        }
        scene.finish(self.policy.margin)
    }
}

fn text(
    scene: &mut SceneBuilder,
    provider: &dyn RichTextProvider,
    label: &CompiledLabel,
    color: Color,
    opacity: f64,
    x: f64,
    y: f64,
) -> Result<(), CanonicalError> {
    scene.begin_path(
        Paint {
            fill: Some(color),
            stroke: None,
            opacity,
            ..Paint::default()
        },
        Point::default(),
    )?;
    label.outline_at(provider, x, y, scene)?;
    scene.end_path()
}

fn multiplicity_origin(
    tip: Point,
    inside: Point,
    label: &CompiledLabel,
) -> Result<(f64, f64), CanonicalError> {
    let (dx, dy) = (inside.x - tip.x, inside.y - tip.y);
    if dx.hypot(dy) < 1e-8 {
        return Err(CanonicalError::InvalidGeometry);
    }
    // Put cardinality labels beside their endpoint without painting over the
    // terminal or the center relationship label.
    Ok(if dx.abs() > dy.abs() {
        (
            tip.x + if dx > 0.0 { 6.0 } else { -6.0 - label.width },
            tip.y - 8.0 - label.height,
        )
    } else {
        (
            tip.x + 8.0,
            tip.y + if dy > 0.0 { 6.0 } else { -6.0 - label.height },
        )
    })
}

fn relationship_terminal(
    scene: &mut SceneBuilder,
    terminal: RelationshipTerminal,
    tip: Point,
    previous: Point,
    edge: Paint,
    background: Color,
) -> Result<(), CanonicalError> {
    use RelationshipTerminal::*;
    let Some(color) = edge.stroke.filter(|_| terminal != None) else {
        return Ok(());
    };
    let (dx, dy) = (tip.x - previous.x, tip.y - previous.y);
    let length = dx.hypot(dy);
    if length < 1e-8 {
        return Err(CanonicalError::InvalidGeometry);
    }
    let point = |depth: f64, width: f64| Point {
        x: tip.x - (dx * depth + dy * width) / length,
        y: tip.y - (dy * depth - dx * width) / length,
    };
    let mut paint = Paint {
        stroke: Some(color),
        stroke_width: edge.stroke_width.min(2.0),
        opacity: edge.opacity,
        ..Paint::default()
    };
    if matches!(terminal, Arrow | Triangle | Diamond | OpenDiamond) {
        paint.fill = match terminal {
            Triangle | OpenDiamond => Some(background),
            Diamond => Some(color),
            _ => Option::None,
        };
        let closed = terminal != Arrow;
        scene.begin_path(paint, Point::default())?;
        let start = point(8.0, -4.0);
        scene.push(PathCommand::move_to(start.x, start.y))?;
        scene.push(PathCommand::line_to(tip.x, tip.y))?;
        let right = point(8.0, 4.0);
        scene.push(PathCommand::line_to(right.x, right.y))?;
        if matches!(terminal, Diamond | OpenDiamond) {
            let end = point(16.0, 0.0);
            scene.push(PathCommand::line_to(end.x, end.y))?;
        }
        if closed {
            scene.push(PathCommand::close())?;
        }
        return scene.end_path();
    }
    scene.begin_path(paint, Point::default())?;
    let line = |scene: &mut SceneBuilder, a: Point, b: Point| {
        scene.push(PathCommand::move_to(a.x, a.y))?;
        scene.push(PathCommand::line_to(b.x, b.y))
    };
    if matches!(terminal, AtLeastOne | OptionalMany) {
        let center = point(10.0, 0.0);
        for width in [-5.0, 5.0] {
            line(scene, center, point(1.0, width))?;
        }
    } else {
        line(scene, point(3.0, -5.0), point(3.0, 5.0))?;
    }
    if matches!(terminal, ExactlyOne | AtLeastOne) {
        line(scene, point(13.0, -5.0), point(13.0, 5.0))?;
    }
    scene.end_path()?;
    if matches!(terminal, OptionalOne | OptionalMany) {
        let center = point(15.0, 0.0);
        paint.fill = Some(background);
        scene.begin_path(paint, Point::default())?;
        scene::shapes::outline(
            Shape::Circle,
            rect(center.x - 3.5, center.y - 3.5, 7.0, 7.0),
            scene,
        )?;
        scene.end_path()?;
    }
    Ok(())
}

fn marker(
    scene: &mut SceneBuilder,
    marker: FlowEdgeMarker,
    tip: Point,
    previous: Point,
    edge: Paint,
) -> Result<(), CanonicalError> {
    if marker == FlowEdgeMarker::None {
        return Ok(());
    }
    let (dx, dy) = (tip.x - previous.x, tip.y - previous.y);
    let length = dx.hypot(dy);
    if length < 1e-8 {
        return Err(CanonicalError::InvalidGeometry);
    }
    let (d, n) = (
        Point {
            x: dx / length,
            y: dy / length,
        },
        Point {
            x: -dy / length,
            y: dx / length,
        },
    );
    let Some(color) = edge.stroke else {
        return Ok(());
    };
    let mut paint = Paint {
        fill: Some(color),
        stroke: Some(color),
        stroke_width: edge.stroke_width.min(2.0),
        opacity: edge.opacity,
        ..Paint::default()
    };
    match marker {
        FlowEdgeMarker::Point => {
            let size = 8.0 + edge.stroke_width;
            scene.begin_path(paint, Point::default())?;
            scene.push(PathCommand::move_to(tip.x, tip.y))?;
            scene.push(PathCommand::line_to(
                tip.x - d.x * size + n.x * size / 2.0,
                tip.y - d.y * size + n.y * size / 2.0,
            ))?;
            scene.push(PathCommand::line_to(
                tip.x - d.x * size - n.x * size / 2.0,
                tip.y - d.y * size - n.y * size / 2.0,
            ))?;
            scene.push(PathCommand::close())?;
            scene.end_path()
        }
        FlowEdgeMarker::Circle => {
            let radius = 3.0 + edge.stroke_width / 2.0;
            let c = Point {
                x: tip.x - d.x * radius,
                y: tip.y - d.y * radius,
            };
            scene.begin_path(paint, Point::default())?;
            scene::shapes::outline(
                Shape::Circle,
                rect(c.x - radius, c.y - radius, 2.0 * radius, 2.0 * radius),
                scene,
            )?;
            scene.end_path()
        }
        FlowEdgeMarker::Cross => {
            paint.fill = None;
            paint.stroke_width = edge.stroke_width.max(1.5);
            let size = 4.0 + edge.stroke_width / 2.0;
            let c = Point {
                x: tip.x - d.x * size,
                y: tip.y - d.y * size,
            };
            scene.begin_path(paint, Point::default())?;
            for sign in [-1.0, 1.0] {
                scene.push(PathCommand::move_to(
                    c.x - d.x * size + n.x * size * sign,
                    c.y - d.y * size + n.y * size * sign,
                ))?;
                scene.push(PathCommand::line_to(
                    c.x + d.x * size - n.x * size * sign,
                    c.y + d.y * size - n.y * size * sign,
                ))?;
            }
            scene.end_path()
        }
        FlowEdgeMarker::None => Ok(()),
    }
}
