// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

use super::*;
use router::ChannelGraph;

impl Layout<'_> {
    fn anchor_point(
        &self,
        anchor: Anchor,
        owner: usize,
        outgoing: bool,
        self_loop: bool,
        port_offset: f64,
    ) -> Result<(Point, Point), CanonicalError> {
        let clearance = self.policy.clearance;
        match anchor {
            Anchor::Portal(index) => {
                let p = self.portals[index];
                let sign = if p.group == owner { -1.0 } else { 1.0 };
                let n = normal(p.side);
                Ok((
                    p.point,
                    Point {
                        x: p.point.x + n.x * clearance * sign,
                        y: p.point.y + n.y * clearance * sign,
                    },
                ))
            }
            Anchor::Cell(index) => {
                let r = self.cells[index].bounds;
                let mut side = if outgoing {
                    self.cells[owner].dir.forward()
                } else {
                    opposite(self.cells[owner].dir.forward())
                };
                if self_loop {
                    side = if outgoing { 1 } else { 2 };
                }
                if index == owner {
                    side = if self_loop && !outgoing { 3 } else { 0 };
                }
                let n = normal(side);
                let c = center(r);
                self.checkpoint(160)?;
                let mut p = scene::shapes::port(
                    self.cells[index].shape,
                    r,
                    Point {
                        x: c.x + n.x * 1.0e5,
                        y: c.y + n.y * 1.0e5,
                    },
                )?;
                if side < 2 {
                    p.y += port_offset;
                } else {
                    p.x += port_offset;
                }
                let escape = if index == owner {
                    Point {
                        x: p.x - n.x * clearance,
                        y: p.y - n.y * clearance,
                    }
                } else {
                    match side {
                        0 => Point {
                            x: r.left - clearance,
                            y: p.y,
                        },
                        1 => Point {
                            x: r.right + clearance,
                            y: p.y,
                        },
                        2 => Point {
                            x: p.x,
                            y: r.top - clearance,
                        },
                        _ => Point {
                            x: p.x,
                            y: r.bottom + clearance,
                        },
                    }
                };
                Ok((p, escape))
            }
        }
    }
    fn append_route(&mut self, edge: usize, points: &[Point]) -> Result<(), CanonicalError> {
        for &point in points {
            self.checkpoint(1)?;
            if self.edges[edge].route.last().is_some_and(|p| *p == point) {
                continue;
            }
            if self.edges[edge].route.len() >= 257 {
                return Err(CanonicalError::Limited);
            }
            self.edges[edge].route.try_push(point)?;
        }
        Ok(())
    }
    pub(super) fn route(&mut self) -> Result<(), CanonicalError> {
        for index in 0..self.segments.len() {
            let segment = self.segments[index];
            let edge = &self.edges[segment.edge];
            let self_loop = edge.from == edge.to;
            let offsets = self
                .relationship_ports
                .get(segment.edge)
                .map_or([0.0; 2], |ports| ports.offsets);
            let (start, escape_start) =
                self.anchor_point(segment.from, segment.owner, true, self_loop, offsets[0])?;
            let (end, escape_end) =
                self.anchor_point(segment.to, segment.owner, false, self_loop, offsets[1])?;
            self.segments[index].start = start;
            self.segments[index].end = end;
            self.segments[index].escape_start = escape_start;
            self.segments[index].escape_end = escape_end;
        }
        let mut scene_states = 0;
        let mut routed =
            FundedVec::try_with_capacity(self.scope, Family::Graph, self.segments.len())?;
        for owner in 0..=self.root {
            if owner != self.root && self.cells[owner].group.is_none() {
                continue;
            }
            self.checkpoint(self.segments.len())?;
            let count = self.segments.iter().filter(|s| s.owner == owner).count();
            if count == 0 {
                continue;
            }
            let mut obstacles = FundedVec::try_with_capacity(
                self.scope,
                Family::Work,
                self.cells.len() + self.edges.len() + 1,
            )?;
            for index in 0..self.root {
                self.checkpoint(1)?;
                if self.cells[index].parent == owner {
                    obstacles.try_push(expand(self.cells[index].bounds, self.policy.clearance))?;
                }
            }
            if let Some(label) = &self.cells[owner].label {
                let r = self.cells[owner].bounds;
                obstacles.try_push(expand(
                    rect(
                        r.left + ((r.right - r.left) - label.width) / 2.0,
                        r.top + self.policy.padding + 3.0 * self.policy.clearance,
                        label.width,
                        label.height,
                    ),
                    self.policy.clearance,
                ))?;
            }
            for edge in &self.edges {
                self.checkpoint(1)?;
                if edge.owner == owner && edge.label.is_some() {
                    obstacles.try_push(expand(edge.label_rect, self.policy.clearance))?;
                }
            }
            let mut anchors = FundedVec::try_with_capacity(
                self.scope,
                Family::Work,
                count * 2 + self.edges.len() * 2,
            )?;
            let mut lanes =
                FundedVec::try_with_capacity(self.scope, Family::Work, self.edges.len())?;
            for s in &self.segments {
                self.checkpoint(1)?;
                if s.owner == owner {
                    anchors.try_push(s.escape_start)?;
                    anchors.try_push(s.escape_end)?;
                }
            }
            let bounds = self.cells[owner].bounds;
            let mut ordinal = 0;
            for edge in &self.edges {
                self.checkpoint(1)?;
                if edge.owner != owner {
                    continue;
                }
                if edge.label.is_some() {
                    let r = edge.label_rect;
                    let y = (r.top + r.bottom) / 2.0;
                    anchors.try_push(Point {
                        x: r.left - self.policy.clearance,
                        y,
                    })?;
                    anchors.try_push(Point {
                        x: r.right + self.policy.clearance,
                        y,
                    })?;
                }
                lanes.try_push(Point {
                    x: bounds.left
                        + self.policy.padding
                        + (ordinal + 1) as f64 * self.policy.lane_pitch,
                    y: bounds.bottom
                        - self.policy.padding
                        - (ordinal + 1) as f64 * self.policy.lane_pitch,
                })?;
                ordinal += 1;
            }
            let mut graph = ChannelGraph::new(
                self.scope,
                bounds,
                &obstacles,
                &anchors,
                &lanes,
                &mut scene_states,
            )?;
            for index in 0..self.segments.len() {
                let segment = self.segments[index];
                if segment.owner != owner {
                    continue;
                }
                let edge = segment.edge;
                // Store each segment's path separately until all layer routes
                // exist; source-order assembly below retains portal continuity.
                let mut points = FundedVec::new_in(self.scope.clone(), Family::Graph);
                points.try_push(segment.start)?;
                points.try_push(segment.escape_start)?;
                if self.edges[edge].owner == owner && self.edges[edge].label.is_some() {
                    let r = self.edges[edge].label_rect;
                    let y = (r.top + r.bottom) / 2.0;
                    let left = Point {
                        x: r.left - self.policy.clearance,
                        y,
                    };
                    let right = Point {
                        x: r.right + self.policy.clearance,
                        y,
                    };
                    let first = graph.route(segment.escape_start, left, 12.0, edge)?;
                    append(&mut points, &first, self.scope)?;
                    append(
                        &mut points,
                        &[Point { x: r.left, y }, Point { x: r.right, y }, right],
                        self.scope,
                    )?;
                    let second = graph.route(right, segment.escape_end, 12.0, edge)?;
                    append(&mut points, &second, self.scope)?;
                } else {
                    let mut parallel = 0;
                    for previous in 0..edge {
                        self.checkpoint(1)?;
                        if self.edges[previous].from == self.edges[edge].from
                            && self.edges[previous].to == self.edges[edge].to
                        {
                            parallel += 1;
                        }
                    }
                    if self.edges[edge].owner == owner
                        && (self.edges[edge].feedback
                            || self.edges[edge].from == self.edges[edge].to
                            || parallel > 0)
                    {
                        self.checkpoint(edge)?;
                        let lane_index = self.edges[..edge]
                            .iter()
                            .filter(|e| e.owner == owner)
                            .count();
                        let waypoint = lanes[lane_index];
                        let first = graph.route(segment.escape_start, waypoint, 12.0, edge)?;
                        append(&mut points, &first, self.scope)?;
                        let second = graph.route(waypoint, segment.escape_end, 12.0, edge)?;
                        append(&mut points, &second, self.scope)?;
                    } else {
                        let path =
                            graph.route(segment.escape_start, segment.escape_end, 12.0, edge)?;
                        append(&mut points, &path, self.scope)?;
                    }
                }
                if points.last().is_none_or(|point| *point != segment.end) {
                    if points.len() >= 257 {
                        return Err(CanonicalError::Limited);
                    }
                    points.try_push(segment.end)?;
                }
                self.validate_segment(segment, &points)?;
                // Segment order is edge source order, but layers are processed
                // bottom-up independently. Keep temporary segments in a paid
                // record array, then concatenate below without semantic guessing.
                routed.try_push((index, points))?;
            }
        }
        for segment in 0..self.segments.len() {
            self.checkpoint(routed.len())?;
            let points = routed
                .iter()
                .find(|(index, _)| *index == segment)
                .map(|(_, p)| p)
                .ok_or(CanonicalError::InvalidGeometry)?;
            self.append_route(self.segments[segment].edge, points)?;
        }
        self.validate_routes()
    }
    fn validate_segment(&self, segment: Segment, points: &[Point]) -> Result<(), CanonicalError> {
        let pad = self.edges[segment.edge].style.paint.stroke_width / 2.0;
        for (pair_index, pair) in points.windows(2).enumerate() {
            self.checkpoint(1)?;
            let (a, b) = (pair[0], pair[1]);
            if (a.x - b.x).abs() > 1e-6 && (a.y - b.y).abs() > 1e-6 {
                return Err(CanonicalError::InvalidGeometry);
            }
            for cell in 0..self.root {
                self.checkpoint(1)?;
                if self.cells[cell].parent != segment.owner {
                    continue;
                }
                let escape_from = pair_index == 0 && anchor_is(segment.from, cell, &self.portals);
                let escape_to =
                    pair_index + 2 == points.len() && anchor_is(segment.to, cell, &self.portals);
                if escape_from || escape_to {
                    continue;
                }
                if crosses(a, b, expand(self.cells[cell].bounds, pad)) {
                    return Err(CanonicalError::InvalidGeometry);
                }
            }
            if let Some(label) = &self.cells[segment.owner].label {
                let r = self.cells[segment.owner].bounds;
                let title = rect(
                    r.left + ((r.right - r.left) - label.width) / 2.0,
                    r.top + self.policy.padding + 3.0 * self.policy.clearance,
                    label.width,
                    label.height,
                );
                if crosses(a, b, expand(title, pad)) {
                    return Err(CanonicalError::InvalidGeometry);
                }
            }
            for (edge_index, edge) in self.edges.iter().enumerate() {
                self.checkpoint(1)?;
                if edge_index != segment.edge
                    && edge.owner == segment.owner
                    && edge.label.is_some()
                    && crosses(a, b, expand(edge.label_rect, 3.0 + pad))
                {
                    return Err(CanonicalError::InvalidGeometry);
                }
            }
        }
        Ok(())
    }
    fn validate_routes(&self) -> Result<(), CanonicalError> {
        for (edge_index, edge) in self.edges.iter().enumerate() {
            if edge.route.len() < 2 {
                return Err(CanonicalError::InvalidGeometry);
            }
            for point in &edge.route {
                self.checkpoint(1)?;
                if !scene::valid(point.x) || !scene::valid(point.y) {
                    return Err(CanonicalError::InvalidGeometry);
                }
            }
            if edge.label.is_some() {
                for index in 0..self.root {
                    self.checkpoint(1)?;
                    if self.cells[index].parent == edge.owner
                        && scene::intersects(edge.label_rect, self.cells[index].bounds)
                    {
                        return Err(CanonicalError::InvalidGeometry);
                    }
                }
                for previous in &self.edges[..edge_index] {
                    self.checkpoint(1)?;
                    if previous.owner == edge.owner
                        && previous.label.is_some()
                        && scene::intersects(previous.label_rect, edge.label_rect)
                    {
                        return Err(CanonicalError::InvalidGeometry);
                    }
                }
                if !contains(self.cells[edge.owner].bounds, edge.label_rect) {
                    return Err(CanonicalError::InvalidGeometry);
                }
            }
        }
        Ok(())
    }
}

fn append(
    points: &mut FundedVec<Point>,
    extra: &[Point],
    scope: &MemoryScope,
) -> Result<(), CanonicalError> {
    for &p in extra {
        scope.charge_work(1)?;
        if points.last().is_some_and(|x| *x == p) {
            continue;
        }
        if points.len() >= 257 {
            return Err(CanonicalError::Limited);
        }
        points.try_push(p)?;
    }
    Ok(())
}
fn contains(outer: Rect, inner: Rect) -> bool {
    inner.left >= outer.left
        && inner.right <= outer.right
        && inner.top >= outer.top
        && inner.bottom <= outer.bottom
}
fn anchor_is(anchor: Anchor, cell: usize, portals: &[Portal]) -> bool {
    match anchor {
        Anchor::Cell(index) => index == cell,
        Anchor::Portal(index) => portals[index].group == cell,
    }
}
fn crosses(a: Point, b: Point, r: Rect) -> bool {
    if (a.x - b.x).abs() < 1e-6 {
        a.x > r.left + 1e-7
            && a.x < r.right - 1e-7
            && a.y.min(b.y) < r.bottom - 1e-7
            && a.y.max(b.y) > r.top + 1e-7
    } else {
        a.y > r.top + 1e-7
            && a.y < r.bottom - 1e-7
            && a.x.min(b.x) < r.right - 1e-7
            && a.x.max(b.x) > r.left + 1e-7
    }
}
