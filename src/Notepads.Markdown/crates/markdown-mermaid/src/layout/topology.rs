// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

use super::*;

#[derive(Clone, Copy, Default)]
struct RankSlot {
    cross: f64,
    major: f64,
    offset: f64,
    cursor: f64,
}

impl Layout<'_> {
    pub(super) fn topology(&mut self) -> Result<(), CanonicalError> {
        for owner in 0..=self.root {
            if owner != self.root && self.cells[owner].group.is_none() {
                continue;
            }
            let mut projected =
                FundedVec::try_with_capacity(self.scope, Family::Work, self.edges.len())?;
            for (index, edge) in self.edges.iter().enumerate() {
                self.checkpoint(1)?;
                if edge.owner != owner {
                    continue;
                }
                if let (Some(a), Some(b)) = (
                    self.projected(edge.from, owner)?,
                    self.projected(edge.to, owner)?,
                ) {
                    projected.try_push((a, b, index))?;
                }
            }
            let mut colors =
                FundedVec::try_with_capacity(self.scope, Family::Work, self.cells.len())?;
            self.checkpoint(self.cells.len())?;
            colors.try_resize(self.cells.len(), 0u8)?;
            let mut stack =
                FundedVec::try_with_capacity(self.scope, Family::Work, self.cells.len())?;
            let mut finished =
                FundedVec::try_with_capacity(self.scope, Family::Work, self.cells.len())?;
            for child in 0..self.root {
                self.checkpoint(1)?;
                if self.cells[child].parent != owner || colors[child] != 0 {
                    continue;
                }
                colors[child] = 1;
                stack.try_push((child, 0usize))?;
                while !stack.is_empty() {
                    self.checkpoint(1)?;
                    let last = stack.len() - 1;
                    let (vertex, next) = stack[last];
                    if next == projected.len() {
                        colors[vertex] = 2;
                        finished.try_push(vertex)?;
                        stack.pop();
                        continue;
                    }
                    stack[last].1 += 1;
                    let (from, to, edge) = projected[next];
                    if from != vertex {
                        continue;
                    }
                    if colors[to] == 1 {
                        self.edges[edge].feedback = true;
                        continue;
                    }
                    if colors[to] == 0 {
                        colors[to] = 1;
                        stack.try_push((to, 0))?;
                    }
                }
            }
            for &child in &finished {
                self.cells[child].rank = 0;
                self.cells[child].order = child;
            }
            for &vertex in finished.iter().rev() {
                for &(from, to, edge) in &projected {
                    self.checkpoint(1)?;
                    if from != vertex || self.edges[edge].feedback {
                        continue;
                    }
                    let length = self.model.edges[edge].length.max(1);
                    let rank = self.cells[from]
                        .rank
                        .checked_add(length)
                        .ok_or(CanonicalError::Limited)?;
                    if rank > 512 {
                        return Err(CanonicalError::Limited);
                    }
                    self.cells[to].rank = self.cells[to].rank.max(rank);
                }
            }
            // Stable barycenters, two bounded passes. The insertion sort never
            // allocates and retains source order when the scores tie.
            let mut scores =
                FundedVec::try_with_capacity(self.scope, Family::Work, finished.len())?;
            for _ in 0..2 {
                scores.clear();
                for &child in &finished {
                    let (mut sum, mut count) = (0.0, 0usize);
                    for &(from, to, edge) in &projected {
                        self.checkpoint(1)?;
                        if to == child && !self.edges[edge].feedback {
                            sum += self.cells[from].order as f64;
                            count += 1;
                        }
                    }
                    scores.try_push((
                        child,
                        if count == 0 {
                            self.cells[child].order as f64
                        } else {
                            sum / count as f64
                        },
                    ))?;
                }
                for read in 1..scores.len() {
                    let mut position = read;
                    while position > 0 {
                        self.checkpoint(1)?;
                        let a = scores[position - 1];
                        let b = scores[position];
                        let comparison = self.cells[a.0]
                            .rank
                            .cmp(&self.cells[b.0].rank)
                            .then(a.1.total_cmp(&b.1))
                            .then(a.0.cmp(&b.0));
                        if !comparison.is_gt() {
                            break;
                        }
                        scores.swap(position - 1, position);
                        position -= 1;
                    }
                }
                for (order, &(child, _)) in scores.iter().enumerate() {
                    self.cells[child].order = order;
                }
            }
        }
        Ok(())
    }
    fn portal(&mut self, group: usize, side: usize) -> Result<Anchor, CanonicalError> {
        self.checkpoint(1)?;
        if self.portals.len() >= 4096 {
            return Err(CanonicalError::Limited);
        }
        let ordinal = self.cells[group].portals[side];
        self.cells[group].portals[side] += 1;
        let index = self.portals.len();
        self.portals.try_push(Portal {
            group,
            side,
            ordinal,
            point: Point::default(),
        })?;
        Ok(Anchor::Portal(index))
    }
    pub(super) fn plan_segments(&mut self) -> Result<(), CanonicalError> {
        for edge in 0..self.edges.len() {
            self.checkpoint(1)?;
            let (from, to, owner, feedback) = (
                self.edges[edge].from,
                self.edges[edge].to,
                self.edges[edge].owner,
                self.edges[edge].feedback,
            );
            let mut source = Anchor::Cell(from);
            let mut current = from;
            while current != owner && self.cells[current].parent != owner {
                let group = self.cells[current].parent;
                let dir = self.cells[self.cells[group].parent].dir;
                let side = if feedback {
                    if dir.horizontal() { 2 } else { 0 }
                } else {
                    dir.forward()
                };
                let portal = self.portal(group, side)?;
                self.segments
                    .try_push(blank_segment(edge, group, source, portal))?;
                source = portal;
                current = group;
            }
            let mut destination = Anchor::Cell(to);
            let mut current = to;
            let mut tail: [Option<Segment>; 8] = [None; 8];
            let mut count = 0;
            while current != owner && self.cells[current].parent != owner {
                if count == 8 {
                    return Err(CanonicalError::Limited);
                }
                let group = self.cells[current].parent;
                let dir = self.cells[self.cells[group].parent].dir;
                let side = if feedback {
                    if dir.horizontal() { 2 } else { 0 }
                } else {
                    opposite(dir.forward())
                };
                let portal = self.portal(group, side)?;
                tail[count] = Some(blank_segment(edge, group, portal, destination));
                count += 1;
                destination = portal;
                current = group;
            }
            self.segments
                .try_push(blank_segment(edge, owner, source, destination))?;
            for slot in tail[..count].iter().rev() {
                self.segments
                    .try_push(slot.ok_or(CanonicalError::InvalidGeometry)?)?;
            }
        }
        Ok(())
    }
    pub(super) fn place(&mut self) -> Result<(), CanonicalError> {
        for depth in (0..=8).rev() {
            for owner in 0..=self.root {
                if self.cells[owner].depth != depth
                    || owner != self.root && self.cells[owner].group.is_none()
                {
                    continue;
                }
                self.place_container(owner)?;
            }
        }
        self.cells[self.root].bounds = rect(
            0.0,
            0.0,
            self.cells[self.root].width,
            self.cells[self.root].height,
        );
        for depth in 1..=MAX_VERTEX_DEPTH {
            for index in 0..self.root {
                self.checkpoint(1)?;
                if self.cells[index].depth != depth {
                    continue;
                }
                let parent = self.cells[index].parent;
                let p = self.cells[parent].bounds;
                self.cells[index].bounds = rect(
                    p.left + self.cells[index].local.x,
                    p.top + self.cells[index].local.y,
                    self.cells[index].width,
                    self.cells[index].height,
                );
            }
        }
        for edge in 0..self.edges.len() {
            let owner = self.cells[self.edges[edge].owner].bounds;
            self.edges[edge].label_rect = scene::translated(
                self.edges[edge].label_rect,
                Point {
                    x: owner.left,
                    y: owner.top,
                },
            );
        }
        for index in 0..self.portals.len() {
            self.checkpoint(1)?;
            let portal = self.portals[index];
            let r = self.cells[portal.group].bounds;
            let title = self.cells[portal.group].label.as_ref().map_or(0.0, |l| {
                l.height + self.policy.padding + 3.0 * self.policy.clearance
            });
            let along = self.policy.padding + (portal.ordinal + 1) as f64 * self.policy.lane_pitch;
            self.portals[index].point = match portal.side {
                0 => Point {
                    x: r.left,
                    y: r.top + title + along,
                },
                1 => Point {
                    x: r.right,
                    y: r.top + title + along,
                },
                2 => Point {
                    x: r.left + along,
                    y: r.top,
                },
                _ => Point {
                    x: r.left + along,
                    y: r.bottom,
                },
            };
        }
        Ok(())
    }
    fn place_container(&mut self, owner: usize) -> Result<(), CanonicalError> {
        let mut children =
            FundedVec::try_with_capacity(self.scope, Family::Work, self.cells.len())?;
        let mut max_rank = 0;
        for index in 0..self.root {
            self.checkpoint(1)?;
            if self.cells[index].parent == owner {
                max_rank = max_rank.max(self.cells[index].rank);
                children.try_push(index)?;
            }
        }
        for i in 1..children.len() {
            let mut j = i;
            while j > 0 {
                self.checkpoint(1)?;
                let a = &self.cells[children[j - 1]];
                let b = &self.cells[children[j]];
                if (a.rank, a.order) <= (b.rank, b.order) {
                    break;
                }
                children.swap(j - 1, j);
                j -= 1;
            }
        }
        let mut ranks = FundedVec::try_with_capacity(self.scope, Family::Work, max_rank + 1)?;
        self.checkpoint(max_rank + 1)?;
        ranks.try_resize(max_rank + 1, RankSlot::default())?;
        let horizontal = self.cells[owner].dir.horizontal();
        for &index in &children {
            self.checkpoint(1)?;
            let c = &self.cells[index];
            let r = &mut ranks[c.rank];
            r.cross += if horizontal { c.height } else { c.width };
            r.cross += self.policy.node_gap;
            r.major = r.major.max(if horizontal { c.width } else { c.height });
        }
        let mut edge_count = 0;
        let mut label_major = 0.0f64;
        let mut label_cross = 0.0f64;
        let mut label_stack = 0.0;
        for edge in &self.edges {
            self.checkpoint(1)?;
            if edge.owner == owner {
                edge_count += 1;
                if let Some(label) = &edge.label {
                    label_major = label_major.max(if horizontal {
                        label.width
                    } else {
                        label.height
                    });
                    label_cross = label_cross.max(label.width);
                    // Expanded node and label obstacles must leave a channel
                    // for the node's clearance escape and neighboring labels.
                    label_stack +=
                        label.height + 2.0 * self.policy.clearance + self.policy.lane_pitch;
                }
            }
        }
        let ring = self.policy.padding
            + self.policy.clearance
            + (edge_count + 1) as f64 * self.policy.lane_pitch;
        let gap =
            self.policy.rank_gap + (edge_count + 1) as f64 * self.policy.lane_pitch + label_major;
        let title = self.cells[owner].label.as_ref().map_or(0.0, |l| {
            l.height + self.policy.padding + 3.0 * self.policy.clearance
        });
        let title_width = self.cells[owner].label.as_ref().map_or(0.0, |l| l.width);
        let mut cross = 0.0f64;
        let mut major = 0.0;
        for rank in &mut ranks {
            self.scope.charge_work(1)?;
            if rank.cross > 0.0 {
                rank.cross -= self.policy.node_gap;
            }
            cross = cross.max(rank.cross);
            rank.offset = major;
            major += rank.major + gap;
        }
        if !ranks.is_empty() {
            major -= gap;
        }
        let (mut width, mut height) = if horizontal {
            (major + 2.0 * ring, cross + 2.0 * ring + title + label_stack)
        } else {
            (cross + 2.0 * ring, major + 2.0 * ring + title + label_stack)
        };
        width = width
            .max(title_width + 2.0 * ring)
            .max(label_cross + 2.0 * ring)
            .max(2.0 * ring + 40.0);
        let counts = self.cells[owner].portals;
        width = width.max(
            (counts[2].max(counts[3]) + 2) as f64 * self.policy.lane_pitch
                + 2.0 * self.policy.padding,
        );
        height = height.max(
            (counts[0].max(counts[1]) + 2) as f64 * self.policy.lane_pitch
                + title
                + 2.0 * self.policy.padding,
        );
        for &index in &children {
            self.checkpoint(1)?;
            let c = &self.cells[index];
            let rank = &mut ranks[c.rank];
            let c_cross = if horizontal { c.height } else { c.width };
            let c_major = if horizontal { c.width } else { c.height };
            let cross_position = (cross - rank.cross) / 2.0 + rank.cursor;
            rank.cursor += c_cross + self.policy.node_gap;
            let mut major_position = rank.offset + (rank.major - c_major) / 2.0;
            if matches!(
                self.cells[owner].dir,
                Direction::BottomTop | Direction::RightLeft
            ) {
                major_position = major - major_position - c_major;
            }
            self.cells[index].local = if horizontal {
                Point {
                    x: ring + major_position,
                    y: ring + title + cross_position,
                }
            } else {
                Point {
                    x: ring + cross_position,
                    y: ring + title + major_position,
                }
            };
        }
        let content_bottom = if horizontal {
            ring + title + cross
        } else {
            ring + title + major
        };
        let label_gap = 2.0 * self.policy.clearance + self.policy.lane_pitch;
        let mut label_y = content_bottom + label_gap;
        for edge in &mut self.edges {
            self.scope.charge_work(1)?;
            if edge.owner == owner
                && let Some(label) = &edge.label
            {
                edge.label_rect = rect(
                    (width - label.width) / 2.0,
                    label_y,
                    label.width,
                    label.height,
                );
                label_y += label.height + label_gap;
            }
        }
        self.cells[owner].width = width;
        self.cells[owner].height = height;
        Ok(())
    }
}

fn blank_segment(edge: usize, owner: usize, from: Anchor, to: Anchor) -> Segment {
    Segment {
        edge,
        owner,
        from,
        to,
        start: Point::default(),
        end: Point::default(),
        escape_start: Point::default(),
        escape_end: Point::default(),
    }
}
