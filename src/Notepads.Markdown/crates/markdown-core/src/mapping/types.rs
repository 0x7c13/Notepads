// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

//! Three coordinate domains stay typed at the query and response boundary.

use crate::identity::{
    LogicalBoundary, LogicalSpan, ModelIdentity, PayloadBoundary, PayloadSpan, SourceBoundary,
    SourceSpan,
};
use crate::ir::{AtomId, AtomKind, FlowId, PayloadId, Rows, TargetSpan, TextTarget};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Affinity {
    Before,
    After,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PointInput {
    Source {
        target: TextTarget,
        byte: SourceBoundary,
    },
    Flow {
        flow: FlowId,
        utf16: LogicalBoundary,
    },
    Payload {
        payload: PayloadId,
        utf8: PayloadBoundary,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RangeInput {
    Source {
        target: TextTarget,
        span: SourceSpan,
    },
    Flow {
        flow: FlowId,
        span: LogicalSpan,
    },
    Payload {
        payload: PayloadId,
        span: PayloadSpan,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PointQuery {
    pub model: ModelIdentity,
    pub input: PointInput,
    pub affinity: Affinity,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RangeQuery {
    pub model: ModelIdentity,
    pub input: RangeInput,
    pub empty_affinity: Affinity,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Direction {
    SourceToTarget,
    TargetToSource,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Position {
    Source(SourceBoundary),
    Flow(LogicalBoundary),
    Payload(PayloadBoundary),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RelatedSpan {
    Source(SourceSpan),
    Flow(LogicalSpan),
    Payload(PayloadSpan),
}

impl Position {
    pub fn value(self) -> u64 {
        match self {
            Self::Source(x) => x.get(),
            Self::Flow(x) => x.get(),
            Self::Payload(x) => x.get(),
        }
    }
}

impl RelatedSpan {
    pub fn start(self) -> u64 {
        match self {
            Self::Source(x) => x.start(),
            Self::Flow(x) => x.start(),
            Self::Payload(x) => x.start(),
        }
    }
    pub fn end(self) -> u64 {
        match self {
            Self::Source(x) => x.end(),
            Self::Flow(x) => x.end(),
            Self::Payload(x) => x.end(),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Effects {
    pub atomic: bool,
    pub hidden: bool,
    pub synthetic: bool,
    pub object: bool,
}

impl Effects {
    pub(crate) fn include(&mut self, other: Self) {
        self.atomic |= other.atomic;
        self.hidden |= other.hidden;
        self.synthetic |= other.synthetic;
        self.object |= other.object;
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Correspondence {
    Exact,
    Atomic,
    Hidden,
    Synthetic,
    Object,
}

/// A diagnostic envelope never grants permission to its interior.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Related {
    pub spans: Rows<RelatedSpan>,
    pub anchors: Rows<Position>,
    pub envelope: Option<RelatedSpan>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Contributor {
    pub atom: AtomId,
    pub source_part: u32,
    pub kind: AtomKind,
    pub source: SourceSpan,
    pub target: TargetSpan,
    pub relation: Correspondence,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PointResult {
    pub model: ModelIdentity,
    pub target: TextTarget,
    pub direction: Direction,
    pub chosen: Position,
    pub related: Related,
    pub effects: Effects,
    pub correspondence: Correspondence,
    pub contributors: Rows<Contributor>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Contribution {
    pub atom: AtomId,
    pub source_part: u32,
    pub kind: AtomKind,
    pub source: SourceSpan,
    pub target: TargetSpan,
    pub expanded: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Selection {
    pub model: ModelIdentity,
    pub target: TextTarget,
    pub direction: Direction,
    pub start: PointResult,
    pub end: PointResult,
    pub contributions: Rows<Contribution>,
    pub related: Related,
    pub effects: Effects,
    pub expanded: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RangeResult {
    Boundary(Rows<PointResult>),
    Selection(Rows<Selection>),
}
