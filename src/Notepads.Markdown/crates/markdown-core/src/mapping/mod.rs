// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

//! Grammar-independent, owner-relative bidirectional atom relations.

mod index;
mod storage;
mod types;

pub(crate) use index::MapIndex;
pub use types::*;

use crate::error::{CoreError, LimitKind};
use crate::html::{HtmlProjection, OriginMap};
use crate::identity::{
    LogicalBoundary, LogicalSpan, ModelIdentity, PayloadBoundary, PayloadSpan, SourceBoundary,
    SourceSpan,
};
use crate::ir::{AtomId, AtomKind, CompactIr, Rows, TargetSpan, TextTarget};
use crate::policy::Operation;
use crate::source::SourceSnapshot;
use index::{TargetIndex, atom_text, check_source, lower_bound};
use storage::{QueryScope, QueryVec};

/// Immutable flat owned storage. Row ranges in results refer only to these buffers.
pub struct QueryResponse<T: Copy> {
    results: QueryVec<T>,
    boundaries: QueryVec<PointResult>,
    selections: QueryVec<Selection>,
    spans: QueryVec<RelatedSpan>,
    anchors: QueryVec<Position>,
    contributors: QueryVec<Contributor>,
    contributions: QueryVec<Contribution>,
}

pub type PointResponse = QueryResponse<PointResult>;
pub type RangeResponse = QueryResponse<RangeResult>;

impl<T: Copy> QueryResponse<T> {
    pub fn results(&self) -> &[T] {
        self.results.as_slice()
    }
    pub fn boundaries(&self) -> &[PointResult] {
        self.boundaries.as_slice()
    }
    pub fn selections(&self) -> &[Selection] {
        self.selections.as_slice()
    }
    pub fn spans(&self) -> &[RelatedSpan] {
        self.spans.as_slice()
    }
    pub fn anchors(&self) -> &[Position] {
        self.anchors.as_slice()
    }
    pub fn contributors(&self) -> &[Contributor] {
        self.contributors.as_slice()
    }
    pub fn contributions(&self) -> &[Contribution] {
        self.contributions.as_slice()
    }

    fn original_content(
        &mut self,
        mapping: OriginMap<'_>,
        first_contributor: usize,
        first_contribution: usize,
        input: Option<RangeInput>,
        op: &mut Operation,
    ) -> Result<(bool, bool), CoreError> {
        let expansion = input.and_then(|input| mapping.expansion(input));
        let mut controls_changed = false;
        let mut synthetic_changed = false;
        for i in first_contributor..self.contributors.len() {
            let mut row = self.contributors.as_slice()[i];
            let synthetic = mapping.synthetic_span(row.source);
            let (source, exact, _) = mapping.original_content(row.source)?;
            row.source = source;
            if synthetic {
                row.kind = AtomKind::Synthetic;
                row.relation = Correspondence::Synthetic;
            } else if !exact && row.kind == AtomKind::Copy {
                row.kind = AtomKind::Transform;
                row.relation = Correspondence::Atomic;
            }
            self.contributors.replace(i, row, op)?;
        }
        let mut written = first_contribution;
        for i in first_contribution..self.contributions.len() {
            let mut row = self.contributions.as_slice()[i];
            let control_expansion =
                if row.kind == AtomKind::Object && mapping.control_lookup_work() != 0 {
                    op.events(mapping.control_lookup_work())?;
                    input.and_then(|input| {
                        mapping.control_expansion(row.atom, row.source, row.kind, input)
                    })
                } else {
                    None
                };
            let synthetic = mapping.synthetic_span(row.source);
            let (source, exact, expanded) = mapping.original_content(row.source)?;
            row.source = source;
            if synthetic {
                row.kind = AtomKind::Synthetic;
                row.expanded = false;
                synthetic_changed = true;
            } else if !exact && row.kind == AtomKind::Copy {
                row.kind = AtomKind::Transform;
            }
            if let Some(expanded) = control_expansion {
                row.expanded = expanded;
                controls_changed = true;
            } else {
                row.expanded |= !synthetic && expansion.unwrap_or(expanded);
            }
            if let Some(RangeInput::Source { span, .. }) = input {
                // Projection can expand an atomic source fragment before the
                // native query. Only the opposite side of the contribution
                // expands; preserve the original queried source intersection.
                // Generated edge syntax can map to an empty original anchor
                // outside that intersection. Like native synthetic content,
                // an anchor contributes only strictly inside a nonempty range.
                op.events(1)?;
                let intersects = if source.is_empty() {
                    span.start() < source.start() && source.start() < span.end()
                } else {
                    source.start() < span.end() && span.start() < source.end()
                };
                if !intersects {
                    continue;
                }
                row.expanded |= matches!(row.kind, AtomKind::Transform | AtomKind::Object)
                    && (source.start() < span.start() || source.end() > span.end());
                row.source = SourceSpan::new(
                    source.start().max(span.start()),
                    source.end().min(span.end()),
                )
                .map_err(|error| op.fail(error))?;
            }
            self.contributions.replace(written, row, op)?;
            written += 1;
        }
        self.contributions.truncate(written);
        Ok((controls_changed, synthetic_changed))
    }

    /// Only a verified projected HTML Task in Main can use this relation. Its
    /// marker's outer bytes also touch GFM container/gap atoms; those synthetic
    /// endpoint relations are not original input-tag content or ownership.
    fn original_control_point(
        &mut self,
        result: &mut PointResult,
        mapping: OriginMap<'_>,
        byte: u64,
        affinity: Affinity,
        op: &mut Operation,
    ) -> Result<bool, CoreError> {
        if mapping.control_lookup_work() == 0 {
            return Ok(false);
        }
        op.events(mapping.control_lookup_work())?;
        let Some(control) = mapping.control_point(byte, affinity) else {
            return Ok(false);
        };
        if result.direction != Direction::SourceToTarget
            || !matches!(result.target, TextTarget::Flow(_))
        {
            return Err(op.fail(CoreError::InvalidProvenance));
        }
        let first = self.contributors.len();
        for index in result.contributors.indices(first)? {
            op.events(1)?;
            let mut row = self.contributors.as_slice()[index];
            if row.atom != control.atom {
                continue;
            }
            if row.kind != AtomKind::Object
                || row.source != control.source
                || row.target.start != control.logical.start()
                || row.target.end != control.logical.end()
            {
                return Err(op.fail(CoreError::InvalidProvenance));
            }
            row.relation = if control.boundary {
                Correspondence::Exact
            } else {
                Correspondence::Object
            };
            self.contributors.push(row, op)?;
        }
        if self.contributors.len() != first + 1 {
            return Err(op.fail(CoreError::InvalidProvenance));
        }
        result.contributors = Rows::new(first, 1).map_err(|error| op.fail(error))?;
        let (spans, anchors) = (self.spans.len(), self.anchors.len());
        let (start, end) = if control.boundary {
            (control.chosen, control.chosen)
        } else {
            (control.logical.start(), control.logical.end())
        };
        self.add_related(result.target, result.direction, start, end, op)?;
        result.related = self.related(result.target, result.direction, spans, anchors, op)?;
        result.chosen = Position::Flow(LogicalBoundary::new(control.chosen));
        result.effects = if control.boundary {
            Effects::default()
        } else {
            Effects {
                atomic: true,
                object: true,
                ..Effects::default()
            }
        };
        result.correspondence = if control.boundary {
            Correspondence::Exact
        } else {
            Correspondence::Object
        };
        Ok(true)
    }

    fn original_related(
        &mut self,
        related: Related,
        target: TextTarget,
        direction: Direction,
        mapping: OriginMap<'_>,
        op: &mut Operation,
    ) -> Result<Related, CoreError> {
        if direction == Direction::SourceToTarget {
            return Ok(related);
        }
        let mut promote = false;
        for index in related.anchors.indices(self.anchors.len())? {
            op.events(1)?;
            let byte = self.anchors.as_slice()[index].value();
            promote |= mapping.from_preview(byte, Affinity::Before)
                != mapping.from_preview(byte, Affinity::After);
        }
        let (first_span, first_anchor) = (self.spans.len(), self.anchors.len());
        for index in related.spans.indices(first_span)? {
            let value = original_related_span(self.spans.as_slice()[index], mapping)?;
            self.spans.replace(index, value, op)?;
            if promote {
                self.spans.push(value, op)?;
            }
        }
        for index in related.anchors.indices(first_anchor)? {
            let value = self.anchors.as_slice()[index];
            let before = original_position(value, mapping, Affinity::Before);
            let after = original_position(value, mapping, Affinity::After);
            self.anchors.replace(index, before, op)?;
            if promote {
                if before == after {
                    self.anchors.push(before, op)?;
                } else {
                    self.spans.push(
                        RelatedSpan::Source(SourceSpan::new(before.value(), after.value())?),
                        op,
                    )?;
                }
            }
        }
        if promote {
            // Preserve existing row indices when an anchor becomes a span.
            self.related(target, direction, first_span, first_anchor, op)
        } else {
            Ok(Related {
                envelope: related
                    .envelope
                    .map(|span| original_related_span(span, mapping))
                    .transpose()?,
                ..related
            })
        }
    }

    fn original_point(
        &mut self,
        result: &mut PointResult,
        mapping: OriginMap<'_>,
        affinity: Affinity,
        op: &mut Operation,
    ) -> Result<(), CoreError> {
        if let Position::Source(byte) = result.chosen {
            if mapping.synthetic_point(byte.get(), affinity) {
                result.effects = Effects {
                    synthetic: true,
                    ..Effects::default()
                };
                result.correspondence = Correspondence::Synthetic;
            } else if mapping.from_preview(byte.get(), Affinity::Before)
                != mapping.from_preview(byte.get(), Affinity::After)
            {
                result.effects.atomic = true;
                if result.correspondence == Correspondence::Exact {
                    result.correspondence = Correspondence::Atomic;
                }
            }
        }
        result.chosen = original_position(result.chosen, mapping, affinity);
        result.related =
            self.original_related(result.related, result.target, result.direction, mapping, op)?;
        Ok(())
    }

    fn new(op: &mut Operation) -> Result<Self, CoreError> {
        let scope = QueryScope::new(op)?;
        Ok(Self {
            results: QueryVec::new(&scope),
            boundaries: QueryVec::new(&scope),
            selections: QueryVec::new(&scope),
            spans: QueryVec::new(&scope),
            anchors: QueryVec::new(&scope),
            contributors: QueryVec::new(&scope),
            contributions: QueryVec::new(&scope),
        })
    }

    fn related(
        &mut self,
        target: TextTarget,
        direction: Direction,
        start_spans: usize,
        start_anchors: usize,
        op: &mut Operation,
    ) -> Result<Related, CoreError> {
        storage::sort(
            &mut self.spans,
            start_spans,
            |a, b| (a.start(), a.end()).cmp(&(b.start(), b.end())),
            op,
        )?;
        let mut out = start_spans;
        for index in start_spans..self.spans.len() {
            op.events(1)?;
            let value = self.spans.as_slice()[index];
            if out > start_spans {
                let prior = self.spans.as_slice()[out - 1];
                if value.start() <= prior.end() {
                    self.spans.replace(
                        out - 1,
                        span(
                            target,
                            direction,
                            prior.start(),
                            prior.end().max(value.end()),
                            op,
                        )?,
                        op,
                    )?;
                    continue;
                }
            }
            self.spans.replace(out, value, op)?;
            out += 1;
        }
        self.spans.truncate(out);
        storage::sort(
            &mut self.anchors,
            start_anchors,
            |a, b| a.value().cmp(&b.value()),
            op,
        )?;
        let mut out = start_anchors;
        for index in start_anchors..self.anchors.len() {
            op.events(1)?;
            let value = self.anchors.as_slice()[index];
            if out > start_anchors && self.anchors.as_slice()[out - 1] == value {
                continue;
            }
            self.anchors.replace(out, value, op)?;
            out += 1;
        }
        self.anchors.truncate(out);
        let mut bounds = None;
        for value in &self.spans.as_slice()[start_spans..] {
            op.events(1)?;
            include_bounds(&mut bounds, value.start(), value.end());
        }
        for value in &self.anchors.as_slice()[start_anchors..] {
            op.events(1)?;
            include_bounds(&mut bounds, value.value(), value.value());
        }
        Ok(Related {
            spans: Rows::new(start_spans, self.spans.len() - start_spans)
                .map_err(|e| op.fail(e))?,
            anchors: Rows::new(start_anchors, self.anchors.len() - start_anchors)
                .map_err(|e| op.fail(e))?,
            envelope: bounds
                .map(|(a, b)| span(target, direction, a, b, op))
                .transpose()?,
        })
    }

    fn add_related(
        &mut self,
        target: TextTarget,
        direction: Direction,
        start: u64,
        end: u64,
        op: &mut Operation,
    ) -> Result<(), CoreError> {
        if start == end {
            self.anchors.push(position(target, direction, start), op)
        } else {
            self.spans
                .push(span(target, direction, start, end, op)?, op)
        }
    }
}

fn original_position(value: Position, mapping: OriginMap<'_>, affinity: Affinity) -> Position {
    match value {
        Position::Source(byte) => Position::Source(SourceBoundary::new(
            mapping.from_preview(byte.get(), affinity),
        )),
        _ => value,
    }
}

fn original_related_span(
    value: RelatedSpan,
    mapping: OriginMap<'_>,
) -> Result<RelatedSpan, CoreError> {
    match value {
        RelatedSpan::Source(span) => Ok(RelatedSpan::Source(mapping.original_span(span)?)),
        _ => Ok(value),
    }
}

fn include_projection_effects(result: &mut PointResult, effects: Effects) {
    result.effects.include(effects);
    if effects.atomic && result.correspondence != Correspondence::Object {
        result.correspondence = Correspondence::Atomic;
    } else if effects.hidden && result.correspondence == Correspondence::Exact {
        result.correspondence = Correspondence::Hidden;
    }
}

#[derive(Clone, Copy)]
struct Request {
    target: TextTarget,
    direction: Direction,
    start: u64,
    end: u64,
    index: TargetIndex,
}

#[derive(Clone, Copy)]
struct MappingInput<'a> {
    ir: &'a CompactIr,
    source: &'a SourceSnapshot,
}

#[derive(Clone, Copy, Eq, PartialEq)]
struct ContentCandidate {
    atom: AtomId,
    // Source queries retain the exact posting unless whole-atom expansion is required.
    part: Option<u32>,
}

impl MapIndex {
    #[cfg(test)]
    pub(crate) fn map_point(
        &self,
        ir: &CompactIr,
        source: &SourceSnapshot,
        query: &PointQuery,
        op: &mut Operation,
    ) -> Result<PointResponse, CoreError> {
        self.map_points(ir, source, std::slice::from_ref(query), op)
    }

    pub(crate) fn map_points(
        &self,
        ir: &CompactIr,
        source: &SourceSnapshot,
        queries: &[PointQuery],
        op: &mut Operation,
    ) -> Result<PointResponse, CoreError> {
        self.map_points_inner(
            ir,
            source,
            queries,
            None,
            op,
            #[cfg(test)]
            None,
        )
    }

    pub(crate) fn map_projected_points(
        &self,
        ir: &CompactIr,
        source: &SourceSnapshot,
        queries: &[PointQuery],
        html: &HtmlProjection,
        op: &mut Operation,
    ) -> Result<PointResponse, CoreError> {
        self.map_points_inner(
            ir,
            source,
            queries,
            Some(html),
            op,
            #[cfg(test)]
            None,
        )
    }

    fn map_points_inner(
        &self,
        ir: &CompactIr,
        source: &SourceSnapshot,
        queries: &[PointQuery],
        html: Option<&HtmlProjection>,
        op: &mut Operation,
        #[cfg(test)] before_publish: Option<&dyn Fn()>,
    ) -> Result<PointResponse, CoreError> {
        op.checkpoint()?;
        op.ensure_budget(&self.budget)?;
        op.ensure_budget(source.budget())?;
        let mut response = None;
        for (ordinal, query) in queries.iter().enumerate() {
            let original_input = query.input;
            let original_affinity = query.affinity;
            let target = match query.input {
                PointInput::Source { target, .. } => target,
                PointInput::Flow { flow, .. } => TextTarget::Flow(flow),
                PointInput::Payload { payload, .. } => TextTarget::Payload(payload),
            };
            let mapping = html.map(|html| html.mapping(target, op)).transpose()?;
            let effects =
                if let (Some(mapping), PointInput::Source { byte, .. }) = (mapping, query.input) {
                    mapping.source_effects(byte.get())
                } else {
                    Effects::default()
                };
            let query = if let Some(mapping) = mapping {
                mapping.point_query(*query).map_err(|e| op.fail(e))?
            } else {
                *query
            };
            let request = self.validate_point(ir, source, &query, op)?;
            self.request_quota(
                ordinal
                    .checked_add(1)
                    .ok_or_else(|| op.fail(CoreError::ArithmeticOverflow))?,
                op,
            )?;
            if response.is_none() {
                response = Some(QueryResponse::new(op)?);
            }
            let owner = response
                .as_mut()
                .ok_or_else(|| op.fail(CoreError::InternalInvariant))?;
            let first_contributor = owner.contributors.len();
            let first_contribution = owner.contributions.len();
            let mut result = self.point(
                MappingInput { ir, source },
                request,
                request.start,
                query.affinity,
                owner,
                op,
            )?;
            if let Some(mapping) = mapping {
                owner.original_content(mapping, first_contributor, first_contribution, None, op)?;
                owner.original_point(&mut result, mapping, query.affinity, op)?;
                include_projection_effects(&mut result, effects);
                if let PointInput::Source { byte, .. } = original_input {
                    owner.original_control_point(
                        &mut result,
                        mapping,
                        byte.get(),
                        original_affinity,
                        op,
                    )?;
                }
            }
            owner.results.push(result, op)?;
        }
        let response = match response {
            Some(response) => response,
            None => QueryResponse::new(op)?,
        };
        #[cfg(test)]
        if let Some(before_publish) = before_publish {
            before_publish();
        }
        op.checkpoint()?;
        Ok(response)
    }

    #[cfg(test)]
    pub(crate) fn map_range(
        &self,
        ir: &CompactIr,
        source: &SourceSnapshot,
        query: &RangeQuery,
        op: &mut Operation,
    ) -> Result<RangeResponse, CoreError> {
        self.map_ranges(ir, source, std::slice::from_ref(query), op)
    }

    pub(crate) fn map_ranges(
        &self,
        ir: &CompactIr,
        source: &SourceSnapshot,
        queries: &[RangeQuery],
        op: &mut Operation,
    ) -> Result<RangeResponse, CoreError> {
        self.map_ranges_inner(ir, source, queries, None, op)
    }

    pub(crate) fn map_projected_ranges(
        &self,
        ir: &CompactIr,
        source: &SourceSnapshot,
        queries: &[RangeQuery],
        html: &HtmlProjection,
        op: &mut Operation,
    ) -> Result<RangeResponse, CoreError> {
        self.map_ranges_inner(ir, source, queries, Some(html), op)
    }

    fn map_ranges_inner(
        &self,
        ir: &CompactIr,
        source: &SourceSnapshot,
        queries: &[RangeQuery],
        html: Option<&HtmlProjection>,
        op: &mut Operation,
    ) -> Result<RangeResponse, CoreError> {
        op.checkpoint()?;
        op.ensure_budget(&self.budget)?;
        op.ensure_budget(source.budget())?;
        let mut response = None;
        for (ordinal, query) in queries.iter().enumerate() {
            let target = match query.input {
                RangeInput::Source { target, .. } => target,
                RangeInput::Flow { flow, .. } => TextTarget::Flow(flow),
                RangeInput::Payload { payload, .. } => TextTarget::Payload(payload),
            };
            let mapping = html.map(|html| html.mapping(target, op)).transpose()?;
            let original_input = query.input;
            let original_empty_affinity = query.empty_affinity;
            let (start_effects, end_effects) =
                if let (Some(mapping), RangeInput::Source { span, .. }) = (mapping, query.input) {
                    (
                        mapping.source_effects(span.start()),
                        mapping.source_effects(span.end()),
                    )
                } else {
                    (Effects::default(), Effects::default())
                };
            let query = if let Some(mapping) = mapping {
                mapping.range_query(*query).map_err(|e| op.fail(e))?
            } else {
                *query
            };
            let request = self.validate_range(ir, source, &query, op)?;
            self.request_quota(
                ordinal
                    .checked_add(1)
                    .ok_or_else(|| op.fail(CoreError::ArithmeticOverflow))?,
                op,
            )?;
            if response.is_none() {
                response = Some(QueryResponse::new(op)?);
            }
            let owner = response
                .as_mut()
                .ok_or_else(|| op.fail(CoreError::InternalInvariant))?;
            let first_contributor = owner.contributors.len();
            let first_contribution = owner.contributions.len();
            let result = if request.start == request.end {
                let mut point = self.point(
                    MappingInput { ir, source },
                    request,
                    request.start,
                    query.empty_affinity,
                    owner,
                    op,
                )?;
                if let Some(mapping) = mapping {
                    owner.original_content(
                        mapping,
                        first_contributor,
                        first_contribution,
                        Some(original_input),
                        op,
                    )?;
                    owner.original_point(&mut point, mapping, query.empty_affinity, op)?;
                    include_projection_effects(&mut point, start_effects);
                    if let RangeInput::Source { span, .. } = original_input {
                        owner.original_control_point(
                            &mut point,
                            mapping,
                            span.start(),
                            original_empty_affinity,
                            op,
                        )?;
                    }
                }
                let row = Rows::new(owner.boundaries.len(), 1).map_err(|e| op.fail(e))?;
                owner.boundaries.push(point, op)?;
                RangeResult::Boundary(row)
            } else {
                let mut selection = self.selection(ir, source, request, owner, op)?;
                if let Some(mapping) = mapping {
                    // A label-only range starts on the label side of an input
                    // seam. The ordinary After relation retains its real gap/
                    // copy facts; an input-only range still ends on its Before
                    // side. No other source point or contribution is changed.
                    let shared_start = matches!(original_input, RangeInput::Source { span, .. }
                        if mapping.control_end(span.start()));
                    if shared_start {
                        selection.start = self.point(
                            MappingInput { ir, source },
                            request,
                            request.start,
                            Affinity::After,
                            owner,
                            op,
                        )?;
                    }
                    let (controls_changed, synthetic_changed) = owner.original_content(
                        mapping,
                        first_contributor,
                        first_contribution,
                        Some(original_input),
                        op,
                    )?;
                    let retained = owner.contributions.len() - first_contribution;
                    let filtered = retained != selection.contributions.len();
                    selection.contributions =
                        Rows::new(first_contribution, retained).map_err(|error| op.fail(error))?;
                    let at_bibliography = |point: &PointResult, source_byte| {
                        let byte = if request.direction == Direction::SourceToTarget {
                            source_byte
                        } else {
                            point.chosen.value()
                        };
                        mapping.bibliography_boundary(byte)
                    };
                    let start_bibliography = at_bibliography(&selection.start, request.start);
                    let end_bibliography = at_bibliography(&selection.end, request.end);
                    owner.original_point(&mut selection.start, mapping, Affinity::Before, op)?;
                    owner.original_point(&mut selection.end, mapping, Affinity::After, op)?;
                    include_projection_effects(&mut selection.start, start_effects);
                    include_projection_effects(&mut selection.end, end_effects);
                    let mut control_boundary = false;
                    if let RangeInput::Source { span, .. } = original_input {
                        control_boundary |= owner.original_control_point(
                            &mut selection.start,
                            mapping,
                            span.start(),
                            if shared_start {
                                Affinity::After
                            } else {
                                Affinity::Before
                            },
                            op,
                        )?;
                        control_boundary |= owner.original_control_point(
                            &mut selection.end,
                            mapping,
                            span.end(),
                            if mapping.control_end(span.end()) {
                                Affinity::Before
                            } else {
                                Affinity::After
                            },
                            op,
                        )?;
                    }
                    if synthetic_changed {
                        selection.effects = Effects::default();
                        selection.expanded = false;
                    } else if controls_changed || control_boundary || filtered {
                        // Rebuild from corrected per-atom contributions below;
                        // unrelated transform/image expansion remains intact.
                        selection.expanded = start_effects.atomic || end_effects.atomic;
                    } else {
                        selection.expanded |= start_effects.atomic || end_effects.atomic;
                    }
                    selection.related = if filtered {
                        // Only source queries filter content. Rebuild from the
                        // retained opposite-side intersections, leaving endpoint
                        // Contributor/Related rows and earlier queries intact.
                        let (spans, anchors) = (owner.spans.len(), owner.anchors.len());
                        for index in selection.contributions.indices(owner.contributions.len())? {
                            op.events(1)?;
                            let row = owner.contributions.as_slice()[index];
                            owner.add_related(
                                selection.target,
                                selection.direction,
                                row.target.start,
                                row.target.end,
                                op,
                            )?;
                        }
                        selection.effects = Effects::default();
                        owner.related(selection.target, selection.direction, spans, anchors, op)?
                    } else {
                        owner.original_related(
                            selection.related,
                            selection.target,
                            selection.direction,
                            mapping,
                            op,
                        )?
                    };
                    for (point, bibliography) in [
                        (selection.start, start_bibliography),
                        (selection.end, end_bibliography),
                    ] {
                        let mut effects = point.effects;
                        // The generated EOF separator is an endpoint relation,
                        // not content selected from the adjacent original text.
                        if bibliography {
                            effects.synthetic = false;
                        }
                        selection.effects.include(effects);
                    }
                    for index in selection.contributions.indices(owner.contributions.len())? {
                        op.events(1)?;
                        let row = owner.contributions.as_slice()[index];
                        selection.effects.include(content_effects(row.kind));
                        selection.expanded |= row.expanded;
                    }
                }
                let row = Rows::new(owner.selections.len(), 1).map_err(|e| op.fail(e))?;
                owner.selections.push(selection, op)?;
                RangeResult::Selection(row)
            };
            owner.results.push(result, op)?;
        }
        let response = match response {
            Some(response) => response,
            None => QueryResponse::new(op)?,
        };
        op.checkpoint()?;
        Ok(response)
    }

    fn request_quota(&self, count: usize, op: &mut Operation) -> Result<(), CoreError> {
        let limits = op
            .budget()
            .model_limits()
            .copied()
            .ok_or_else(|| op.fail(CoreError::UnsupportedConfiguration))?;
        op.check_limit(
            LimitKind::QueryRequests,
            count as u64,
            limits.query_requests,
        )
    }

    fn identity(
        &self,
        source: &SourceSnapshot,
        actual: ModelIdentity,
        op: &mut Operation,
    ) -> Result<(), CoreError> {
        op.checkpoint()?;
        op.ensure_budget(&self.budget)?;
        op.ensure_budget(source.budget())?;
        check_source(self.identity, source, op)?;
        let expected = self.identity.source();
        if actual.source().document() != expected.document() {
            return Err(op.fail(CoreError::WrongDocument));
        }
        if actual.source().revision() != expected.revision() {
            return Err(op.fail(CoreError::WrongRevision {
                expected: expected.revision(),
                actual: actual.source().revision(),
            }));
        }
        if actual.generation() != self.identity.generation() {
            return Err(op.fail(CoreError::WrongGeneration {
                expected: self.identity.generation(),
                actual: actual.generation(),
            }));
        }
        if actual.config() != self.identity.config() {
            return Err(op.fail(CoreError::UnsupportedConfiguration));
        }
        Ok(())
    }

    fn validate_point(
        &self,
        ir: &CompactIr,
        source: &SourceSnapshot,
        query: &PointQuery,
        op: &mut Operation,
    ) -> Result<Request, CoreError> {
        self.identity(source, query.model, op)?;
        let (target, direction, value) = match query.input {
            PointInput::Source { target, byte } => (target, Direction::SourceToTarget, byte.get()),
            PointInput::Flow { flow, utf16 } => (
                TextTarget::Flow(flow),
                Direction::TargetToSource,
                utf16.get(),
            ),
            PointInput::Payload { payload, utf8 } => (
                TextTarget::Payload(payload),
                Direction::TargetToSource,
                utf8.get(),
            ),
        };
        let index = self.get(target, op)?;
        let request = Request {
            target,
            direction,
            start: value,
            end: value,
            index,
        };
        self.validate_domain(ir, source, request, false, op)?;
        Ok(request)
    }

    fn validate_range(
        &self,
        ir: &CompactIr,
        source: &SourceSnapshot,
        query: &RangeQuery,
        op: &mut Operation,
    ) -> Result<Request, CoreError> {
        self.identity(source, query.model, op)?;
        let (target, direction, start, end) = match query.input {
            RangeInput::Source { target, span } => {
                (target, Direction::SourceToTarget, span.start(), span.end())
            }
            RangeInput::Flow { flow, span } => (
                TextTarget::Flow(flow),
                Direction::TargetToSource,
                span.start(),
                span.end(),
            ),
            RangeInput::Payload { payload, span } => (
                TextTarget::Payload(payload),
                Direction::TargetToSource,
                span.start(),
                span.end(),
            ),
        };
        let index = self.get(target, op)?;
        let request = Request {
            target,
            direction,
            start,
            end,
            index,
        };
        self.validate_domain(ir, source, request, true, op)?;
        Ok(request)
    }

    fn validate_domain(
        &self,
        ir: &CompactIr,
        source: &SourceSnapshot,
        request: Request,
        is_range: bool,
        op: &mut Operation,
    ) -> Result<(), CoreError> {
        let max = match request.direction {
            Direction::SourceToTarget => source.text().len() as u64,
            Direction::TargetToSource => request.index.length,
        };
        if request.start > request.end || request.end > max {
            return Err(op.fail(if is_range {
                CoreError::InvalidRange
            } else {
                CoreError::InvalidBoundary
            }));
        }
        if request.direction == Direction::SourceToTarget {
            let domains = request
                .index
                .domains
                .slice(ir.sources())
                .map_err(|e| op.fail(e))?;
            if request.start == request.end {
                let index = index::lower_bound_view(domains, |row| row.end() >= request.start, op)?;
                if !domains
                    .get(index)
                    .is_some_and(|row| row.start() <= request.start)
                {
                    return Err(op.fail(CoreError::OutsideScope));
                }
            } else {
                let mut cursor = request.start;
                let first = index::lower_bound_view(domains, |row| row.end() > request.start, op)?;
                for domain in domains
                    .range(first, domains.len() - first)
                    .ok_or_else(|| op.fail(CoreError::InvalidProvenance))?
                {
                    op.events(1)?;
                    if domain.start() > cursor {
                        break;
                    }
                    cursor = cursor.max(domain.end());
                    if cursor >= request.end {
                        break;
                    }
                }
                if cursor < request.end {
                    return Err(op.fail(CoreError::OutsideScope));
                }
            }
            op.events(1)?;
            op.scan(u64::from(request.start < max) + u64::from(request.end < max))?;
            source
                .validate_span(SourceSpan::new(request.start, request.end).map_err(|e| op.fail(e))?)
                .map_err(|e| op.fail(e))?;
        } else {
            self.target_scalar(ir, source, request, request.start, op)?;
            if request.end != request.start {
                self.target_scalar(ir, source, request, request.end, op)?;
            }
        }
        Ok(())
    }

    fn target_scalar(
        &self,
        ir: &CompactIr,
        source: &SourceSnapshot,
        request: Request,
        value: u64,
        op: &mut Operation,
    ) -> Result<(), CoreError> {
        if value == 0 || value == request.index.length {
            return Ok(());
        }
        let rows = request
            .index
            .target
            .slice(self.target.as_slice())
            .map_err(|e| op.fail(e))?;
        let first = lower_bound(rows, |row| row.end >= value, op)?;
        let row = rows
            .get(first)
            .ok_or_else(|| op.fail(CoreError::InvalidProvenance))?;
        if row.start > value {
            return Err(op.fail(CoreError::InvalidProvenance));
        }
        if value == row.start || value == row.end {
            return Ok(());
        }
        let atom = ir.atom_at(row.atom.index(), op)?;
        match request.target {
            TextTarget::Flow(_) => {
                self.utf16_to_byte(ir, source, row.atom, value - row.start, op)?;
            }
            TextTarget::Payload(_) => {
                op.events(1)?;
                let byte = usize::try_from(value - row.start)
                    .map_err(|_| op.fail(CoreError::ArithmeticOverflow))?;
                op.scan(1)?;
                if !atom_text(ir, source, &atom, op)?.is_char_boundary(byte) {
                    return Err(op.fail(CoreError::InvalidBoundary));
                }
            }
        }
        Ok(())
    }

    fn point<T: Copy>(
        &self,
        input: MappingInput<'_>,
        request: Request,
        value: u64,
        affinity: Affinity,
        response: &mut QueryResponse<T>,
        op: &mut Operation,
    ) -> Result<PointResult, CoreError> {
        let (s, a, c) = (
            response.spans.len(),
            response.anchors.len(),
            response.contributors.len(),
        );
        let mut effects = Effects::default();
        if let Some(anchor) = request.index.empty_anchor {
            let opposite = match request.direction {
                Direction::SourceToTarget => 0,
                Direction::TargetToSource => anchor,
            };
            response.add_related(request.target, request.direction, opposite, opposite, op)?;
        } else if request.direction == Direction::SourceToTarget {
            let rows = request
                .index
                .source
                .slice(self.source.as_slice())
                .map_err(|e| op.fail(e))?;
            let first = lower_bound(rows, |row| row.span.end() >= value, op)?;
            for posting in &rows[first..] {
                op.events(1)?;
                if posting.span.start() > value {
                    break;
                }
                effects.include(self.source_point(input, request, value, *posting, response, op)?);
            }
            let rows = request
                .index
                .synthetic
                .slice(self.synthetic.as_slice())
                .map_err(|e| op.fail(e))?;
            let first = lower_bound(rows, |row| row.span.start() >= value, op)?;
            for posting in &rows[first..] {
                op.events(1)?;
                if posting.span.start() != value {
                    break;
                }
                effects.include(self.source_point(input, request, value, *posting, response, op)?);
            }
        } else {
            let rows = request
                .index
                .target
                .slice(self.target.as_slice())
                .map_err(|e| op.fail(e))?;
            let first = lower_bound(rows, |row| row.end >= value, op)?;
            for posting in &rows[first..] {
                op.events(1)?;
                if posting.start > value {
                    break;
                }
                effects.include(self.target_point(
                    input,
                    request,
                    value,
                    posting.atom,
                    response,
                    op,
                )?);
            }
            let rows = request
                .index
                .hidden
                .slice(self.hidden.as_slice())
                .map_err(|e| op.fail(e))?;
            let first = lower_bound(rows, |row| row.start >= value, op)?;
            for posting in &rows[first..] {
                op.events(1)?;
                if posting.start != value {
                    break;
                }
                effects.include(self.target_point(
                    input,
                    request,
                    value,
                    posting.atom,
                    response,
                    op,
                )?);
            }
        }
        // Endpoint contributors remain complete even when their opposite anchors coincide.
        storage::sort(
            &mut response.contributors,
            c,
            |a, b| (a.atom, a.source_part).cmp(&(b.atom, b.source_part)),
            op,
        )?;
        let related = response.related(request.target, request.direction, s, a, op)?;
        let envelope = related
            .envelope
            .ok_or_else(|| op.fail(CoreError::InvalidProvenance))?;
        let chosen = match affinity {
            Affinity::Before => envelope.start(),
            Affinity::After => envelope.end(),
        };
        Ok(PointResult {
            model: self.identity,
            target: request.target,
            direction: request.direction,
            chosen: position(request.target, request.direction, chosen),
            related,
            effects,
            correspondence: correspondence(effects),
            contributors: Rows::new(c, response.contributors.len() - c).map_err(|e| op.fail(e))?,
        })
    }

    fn source_point<T: Copy>(
        &self,
        input: MappingInput<'_>,
        request: Request,
        value: u64,
        posting: index::Posting,
        response: &mut QueryResponse<T>,
        op: &mut Operation,
    ) -> Result<Effects, CoreError> {
        let MappingInput { ir, source } = input;
        let atom = ir.atom_at(posting.atom.index(), op)?;
        let parts = atom.sources.slice(ir.sources()).map_err(|e| op.fail(e))?;
        let mut effects = Effects::default();
        let (start, end) = match atom.kind {
            AtomKind::Copy => {
                let relative = sub(value, posting.span.start(), op)?;
                let relative = match request.target {
                    TextTarget::Flow(_) => {
                        self.byte_to_utf16(ir, source, posting.atom, relative, op)?
                    }
                    TextTarget::Payload(_) => relative,
                };
                let value = add(atom.target.start, relative, op)?;
                (value, value)
            }
            AtomKind::Hidden => {
                effects.hidden = value != posting.span.start() && value != posting.span.end();
                (atom.target.start, atom.target.start)
            }
            AtomKind::Synthetic => {
                effects.synthetic = true;
                (atom.target.start, atom.target.end)
            }
            AtomKind::Transform | AtomKind::Object => {
                let first = parts
                    .first()
                    .ok_or_else(|| op.fail(CoreError::InvalidProvenance))?;
                let last = parts
                    .last()
                    .ok_or_else(|| op.fail(CoreError::InvalidProvenance))?;
                if value == first.start() {
                    (atom.target.start, atom.target.start)
                } else if value == last.end() {
                    (atom.target.end, atom.target.end)
                } else {
                    effects.atomic = atom.kind == AtomKind::Transform;
                    effects.object = atom.kind == AtomKind::Object;
                    (atom.target.start, atom.target.end)
                }
            }
        };
        response.add_related(request.target, request.direction, start, end, op)?;
        response.contributors.push(
            Contributor {
                atom: posting.atom,
                source_part: posting.part,
                kind: atom.kind,
                source: posting.span,
                target: atom.target,
                relation: correspondence(effects),
            },
            op,
        )?;
        Ok(effects)
    }

    fn target_point<T: Copy>(
        &self,
        input: MappingInput<'_>,
        request: Request,
        value: u64,
        id: AtomId,
        response: &mut QueryResponse<T>,
        op: &mut Operation,
    ) -> Result<Effects, CoreError> {
        let MappingInput { ir, source } = input;
        let atom = ir.atom_at(id.index(), op)?;
        let parts = atom.sources.slice(ir.sources()).map_err(|e| op.fail(e))?;
        let mut effects = Effects::default();
        match atom.kind {
            AtomKind::Copy => {
                let relative = sub(value, atom.target.start, op)?;
                let byte = match request.target {
                    TextTarget::Flow(_) => self.utf16_to_byte(ir, source, id, relative, op)?,
                    TextTarget::Payload(_) => relative,
                };
                let part = parts
                    .first()
                    .ok_or_else(|| op.fail(CoreError::InvalidProvenance))?;
                let mapped = add(part.start(), byte, op)?;
                response.add_related(request.target, request.direction, mapped, mapped, op)?;
            }
            AtomKind::Synthetic => {
                effects.synthetic = value != atom.target.start && value != atom.target.end;
                for part in parts {
                    response.add_related(
                        request.target,
                        request.direction,
                        part.start(),
                        part.start(),
                        op,
                    )?;
                }
            }
            AtomKind::Hidden => {
                effects.hidden = true;
                for part in parts {
                    response.add_related(
                        request.target,
                        request.direction,
                        part.start(),
                        part.end(),
                        op,
                    )?;
                }
            }
            AtomKind::Transform | AtomKind::Object => {
                if value == atom.target.start {
                    let part = parts
                        .first()
                        .ok_or_else(|| op.fail(CoreError::InvalidProvenance))?;
                    response.add_related(
                        request.target,
                        request.direction,
                        part.start(),
                        part.start(),
                        op,
                    )?;
                } else if value == atom.target.end {
                    let part = parts
                        .last()
                        .ok_or_else(|| op.fail(CoreError::InvalidProvenance))?;
                    response.add_related(
                        request.target,
                        request.direction,
                        part.end(),
                        part.end(),
                        op,
                    )?;
                } else {
                    effects.atomic = atom.kind == AtomKind::Transform;
                    effects.object = atom.kind == AtomKind::Object;
                    for part in parts {
                        response.add_related(
                            request.target,
                            request.direction,
                            part.start(),
                            part.end(),
                            op,
                        )?;
                    }
                }
            }
        }
        for (part, source) in parts.iter().copied().enumerate() {
            response.contributors.push(
                Contributor {
                    atom: id,
                    source_part: u32::try_from(part)
                        .map_err(|_| op.fail(CoreError::ArithmeticOverflow))?,
                    kind: atom.kind,
                    source,
                    target: atom.target,
                    relation: correspondence(effects),
                },
                op,
            )?;
        }
        Ok(effects)
    }

    fn selection(
        &self,
        ir: &CompactIr,
        source: &SourceSnapshot,
        request: Request,
        response: &mut RangeResponse,
        op: &mut Operation,
    ) -> Result<Selection, CoreError> {
        let input = MappingInput { ir, source };
        let start = self.point(
            input,
            request,
            request.start,
            Affinity::Before,
            response,
            op,
        )?;
        let end = self.point(input, request, request.end, Affinity::After, response, op)?;
        let (s, a, c) = (
            response.spans.len(),
            response.anchors.len(),
            response.contributions.len(),
        );
        let mut candidates = QueryVec::new(response.results.scope());
        if request.direction == Direction::SourceToTarget {
            let rows = request
                .index
                .source
                .slice(self.source.as_slice())
                .map_err(|e| op.fail(e))?;
            let first = lower_bound(rows, |row| row.span.end() > request.start, op)?;
            for row in &rows[first..] {
                op.events(1)?;
                if row.span.start() >= request.end {
                    break;
                }
                let atom = ir.atom_at(row.atom.index(), op)?;
                candidates.push(
                    ContentCandidate {
                        atom: row.atom,
                        part: (!matches!(atom.kind, AtomKind::Transform | AtomKind::Object))
                            .then_some(row.part),
                    },
                    op,
                )?;
            }
            let rows = request
                .index
                .synthetic
                .slice(self.synthetic.as_slice())
                .map_err(|e| op.fail(e))?;
            let first = lower_bound(rows, |row| row.span.start() > request.start, op)?;
            for row in &rows[first..] {
                op.events(1)?;
                if row.span.start() >= request.end {
                    break;
                }
                candidates.push(
                    ContentCandidate {
                        atom: row.atom,
                        part: Some(row.part),
                    },
                    op,
                )?;
            }
        } else {
            let rows = request
                .index
                .target
                .slice(self.target.as_slice())
                .map_err(|e| op.fail(e))?;
            let first = lower_bound(rows, |row| row.end > request.start, op)?;
            for row in &rows[first..] {
                op.events(1)?;
                if row.start >= request.end {
                    break;
                }
                candidates.push(
                    ContentCandidate {
                        atom: row.atom,
                        part: None,
                    },
                    op,
                )?;
            }
            let rows = request
                .index
                .hidden
                .slice(self.hidden.as_slice())
                .map_err(|e| op.fail(e))?;
            let first = lower_bound(rows, |row| row.start > request.start, op)?;
            for row in &rows[first..] {
                op.events(1)?;
                if row.start >= request.end {
                    break;
                }
                candidates.push(
                    ContentCandidate {
                        atom: row.atom,
                        part: None,
                    },
                    op,
                )?;
            }
        }
        storage::sort(
            &mut candidates,
            0,
            |a, b| (a.atom.value(), a.part).cmp(&(b.atom.value(), b.part)),
            op,
        )?;
        let mut previous = None;
        let mut effects = Effects::default();
        let mut expanded = false;
        for candidate in candidates.as_slice() {
            op.events(1)?;
            if previous == Some(*candidate) {
                continue;
            }
            previous = Some(*candidate);
            let atom = ir.atom_at(candidate.atom.index(), op)?;
            effects.include(content_effects(atom.kind));
            expanded |= self.content(input, request, *candidate, response, op)?;
        }
        drop(candidates);
        let related = response.related(request.target, request.direction, s, a, op)?;
        Ok(Selection {
            model: self.identity,
            target: request.target,
            direction: request.direction,
            start,
            end,
            contributions: Rows::new(c, response.contributions.len() - c)
                .map_err(|e| op.fail(e))?,
            related,
            effects,
            expanded,
        })
    }

    fn content(
        &self,
        input: MappingInput<'_>,
        request: Request,
        candidate: ContentCandidate,
        response: &mut RangeResponse,
        op: &mut Operation,
    ) -> Result<bool, CoreError> {
        let MappingInput { ir, source } = input;
        let id = candidate.atom;
        let atom = ir.atom_at(id.index(), op)?;
        let parts = atom.sources.slice(ir.sources()).map_err(|e| op.fail(e))?;
        let atomic = matches!(atom.kind, AtomKind::Transform | AtomKind::Object);
        let mut expanded = atomic
            && request.direction == Direction::TargetToSource
            && (atom.target.start < request.start || atom.target.end > request.end);
        if atomic && request.direction == Direction::SourceToTarget {
            for part in parts {
                op.events(1)?;
                expanded |= part.start() < request.start || part.end() > request.end;
            }
        }
        let mut target_related = false;
        let (first, last) = match candidate.part {
            Some(part) if !atomic && request.direction == Direction::SourceToTarget => {
                let part = part as usize;
                if part >= parts.len() {
                    return Err(op.fail(CoreError::InvalidProvenance));
                }
                (part, part + 1)
            }
            _ => (0, parts.len()),
        };
        for (relative, part) in parts
            .range(first, last - first)
            .ok_or_else(|| op.fail(CoreError::InvalidProvenance))?
            .iter()
            .copied()
            .enumerate()
        {
            let part_index = first + relative;
            op.events(1)?;
            if request.direction == Direction::SourceToTarget {
                let intersects = if part.is_empty() {
                    request.start < part.start() && part.start() < request.end
                } else {
                    part.start() < request.end && part.end() > request.start
                };
                if !intersects {
                    continue;
                }
            }
            let (source_part, target_part) = if atomic {
                // Atomic correspondence expands the opposite side. A content
                // contribution still records the actual queried intersection.
                match request.direction {
                    Direction::SourceToTarget => (
                        SourceSpan::new(
                            part.start().max(request.start),
                            part.end().min(request.end),
                        )
                        .map_err(|e| op.fail(e))?,
                        atom.target,
                    ),
                    Direction::TargetToSource => (
                        part,
                        TargetSpan {
                            start: atom.target.start.max(request.start),
                            end: atom.target.end.min(request.end),
                        },
                    ),
                }
            } else if request.direction == Direction::SourceToTarget {
                let source_part =
                    SourceSpan::new(part.start().max(request.start), part.end().min(request.end))
                        .map_err(|e| op.fail(e))?;
                if atom.kind == AtomKind::Copy {
                    let offset = |byte, op: &mut Operation| {
                        let relative = sub(byte, part.start(), op)?;
                        match request.target {
                            TextTarget::Flow(_) => self.byte_to_utf16(ir, source, id, relative, op),
                            TextTarget::Payload(_) => Ok(relative),
                        }
                    };
                    (
                        source_part,
                        TargetSpan {
                            start: add(atom.target.start, offset(source_part.start(), op)?, op)?,
                            end: add(atom.target.start, offset(source_part.end(), op)?, op)?,
                        },
                    )
                } else {
                    (source_part, atom.target)
                }
            } else {
                let target_part = TargetSpan {
                    start: atom.target.start.max(request.start),
                    end: atom.target.end.min(request.end),
                };
                if atom.kind == AtomKind::Copy {
                    let offset = |position, op: &mut Operation| {
                        let relative = sub(position, atom.target.start, op)?;
                        match request.target {
                            TextTarget::Flow(_) => self.utf16_to_byte(ir, source, id, relative, op),
                            TextTarget::Payload(_) => Ok(relative),
                        }
                    };
                    (
                        SourceSpan::new(
                            add(part.start(), offset(target_part.start, op)?, op)?,
                            add(part.start(), offset(target_part.end, op)?, op)?,
                        )
                        .map_err(|e| op.fail(e))?,
                        target_part,
                    )
                } else {
                    (part, target_part)
                }
            };
            response.contributions.push(
                Contribution {
                    atom: id,
                    source_part: u32::try_from(part_index)
                        .map_err(|_| op.fail(CoreError::ArithmeticOverflow))?,
                    kind: atom.kind,
                    source: source_part,
                    target: target_part,
                    expanded,
                },
                op,
            )?;
            match request.direction {
                Direction::SourceToTarget if !target_related => {
                    response.add_related(
                        request.target,
                        request.direction,
                        target_part.start,
                        target_part.end,
                        op,
                    )?;
                    target_related = true;
                }
                Direction::SourceToTarget => {}
                Direction::TargetToSource => response.add_related(
                    request.target,
                    request.direction,
                    source_part.start(),
                    source_part.end(),
                    op,
                )?,
            }
        }
        Ok(expanded)
    }
}

fn position(target: TextTarget, direction: Direction, value: u64) -> Position {
    match direction {
        Direction::TargetToSource => Position::Source(SourceBoundary::new(value)),
        Direction::SourceToTarget => match target {
            TextTarget::Flow(_) => Position::Flow(LogicalBoundary::new(value)),
            TextTarget::Payload(_) => Position::Payload(PayloadBoundary::new(value)),
        },
    }
}

fn span(
    target: TextTarget,
    direction: Direction,
    start: u64,
    end: u64,
    op: &mut Operation,
) -> Result<RelatedSpan, CoreError> {
    match direction {
        Direction::TargetToSource => SourceSpan::new(start, end).map(RelatedSpan::Source),
        Direction::SourceToTarget => match target {
            TextTarget::Flow(_) => LogicalSpan::new(start, end).map(RelatedSpan::Flow),
            TextTarget::Payload(_) => PayloadSpan::new(start, end).map(RelatedSpan::Payload),
        },
    }
    .map_err(|e| op.fail(e))
}

fn include_bounds(bounds: &mut Option<(u64, u64)>, start: u64, end: u64) {
    *bounds = Some(match *bounds {
        Some((a, b)) => (a.min(start), b.max(end)),
        None => (start, end),
    });
}

fn sub(left: u64, right: u64, op: &mut Operation) -> Result<u64, CoreError> {
    left.checked_sub(right)
        .ok_or_else(|| op.fail(CoreError::InvalidProvenance))
}

fn add(left: u64, right: u64, op: &mut Operation) -> Result<u64, CoreError> {
    left.checked_add(right)
        .ok_or_else(|| op.fail(CoreError::ArithmeticOverflow))
}

fn correspondence(effects: Effects) -> Correspondence {
    if effects.hidden {
        Correspondence::Hidden
    } else if effects.synthetic {
        Correspondence::Synthetic
    } else if effects.object {
        Correspondence::Object
    } else if effects.atomic {
        Correspondence::Atomic
    } else {
        Correspondence::Exact
    }
}

fn content_effects(kind: AtomKind) -> Effects {
    Effects {
        atomic: kind == AtomKind::Transform,
        hidden: kind == AtomKind::Hidden,
        synthetic: kind == AtomKind::Synthetic,
        object: kind == AtomKind::Object,
    }
}

#[cfg(test)]
mod tests;
