// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

//! A bounded sequence parser and timeline producer. Source identifiers and
//! labels stay borrowed; all growable storage owns explicit memory receipts.
//! No third-party sequence tree, SVG, script, or platform object is created.

use crate::{
    DiagramError,
    labels::CompiledLabel,
    layout::{LayoutPolicy, ThemePolicy},
    native_common as common,
    rich_text::RichTextProvider,
    scene::{OwnedScene, Paint, PathCommand, PathSink, Point, Rect, SceneBuilder, shapes::Shape},
};
use markdown_memory::{Family, FundedString, FundedVec, MemoryScope};
use merman_core::OperationControl;
use std::fmt::Write;

const MAX_SOURCE: usize = 16 * 1024;
const MAX_PARTICIPANTS: usize = 128;
const MAX_EVENTS: usize = 256;
const MAX_DEPTH: usize = 8;
const UNFINISHED: usize = usize::MAX;

struct Participant<'a> {
    id: &'a str,
    raw: &'a str,
    actor: bool,
    declared: bool,
    label: Option<CompiledLabel>,
    x: f64,
    width: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Marker {
    None,
    Filled,
    Open,
    Cross,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Arrow {
    dashed: bool,
    marker: Marker,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Side {
    Over,
    Left,
    Right,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Block {
    Loop,
    Alt,
    Opt,
    Par,
    Critical,
    Break,
}

impl Block {
    fn name(self) -> &'static str {
        match self {
            Self::Loop => "loop",
            Self::Alt => "alt",
            Self::Opt => "opt",
            Self::Par => "par",
            Self::Critical => "critical",
            Self::Break => "break",
        }
    }
}

#[derive(Clone, Copy, Debug)]
enum EventKind {
    Message {
        from: usize,
        to: usize,
        arrow: Arrow,
        number: Option<u32>,
    },
    Note {
        from: usize,
        to: usize,
        side: Side,
    },
    Activation,
    Start(usize),
    Branch {
        frame: usize,
        name: &'static str,
    },
    End,
}

struct Event<'a> {
    kind: EventKind,
    raw: &'a str,
    label: Option<CompiledLabel>,
    y: f64,
    height: f64,
}

struct Frame {
    kind: Block,
    start: usize,
    end: usize,
    depth: usize,
}

struct Activation {
    participant: usize,
    depth: usize,
    start: usize,
    end: usize,
}

struct Model<'a> {
    participants: FundedVec<Participant<'a>>,
    events: FundedVec<Event<'a>>,
    frames: FundedVec<Frame>,
    activations: FundedVec<Activation>,
}

struct Parser<'a, 'c> {
    model: Model<'a>,
    control: &'c OperationControl,
    stack: [usize; MAX_DEPTH],
    depth: usize,
    active: [u8; MAX_PARTICIPANTS],
    number: Option<u32>,
    increment: u32,
}

impl<'a, 'c> Parser<'a, 'c> {
    fn new(scope: &MemoryScope, control: &'c OperationControl) -> Self {
        Self {
            model: Model {
                participants: FundedVec::new_in(scope.clone(), Family::Model),
                events: FundedVec::new_in(scope.clone(), Family::Model),
                frames: FundedVec::new_in(scope.clone(), Family::Model),
                activations: FundedVec::new_in(scope.clone(), Family::Model),
            },
            control,
            stack: [0; MAX_DEPTH],
            depth: 0,
            active: [0; MAX_PARTICIPANTS],
            number: None,
            increment: 1,
        }
    }

    fn parse(mut self, source: &'a str) -> Result<Model<'a>, DiagramError> {
        if source.len() > MAX_SOURCE {
            return Err(DiagramError::WorkLimited);
        }
        // Prepay the fixed number of allocation-free lexical passes below.
        common::check(
            self.control,
            source.len().saturating_mul(8).saturating_add(1),
        )?;
        let mut header = false;
        for line in source.trim_start_matches('\u{feff}').lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with("%%") && !line.starts_with("%%{") {
                continue;
            }
            // Semicolons are statement separators, except within quoted
            // strings or bounded Mermaid/HTML entity references.
            for statement in Statements::new(line) {
                let statement = statement.trim();
                if statement.is_empty() {
                    continue;
                }
                common::check(self.control, statement.len().saturating_add(1))?;
                if !header {
                    if statement != "sequenceDiagram" {
                        return Err(DiagramError::InvalidInput);
                    }
                    header = true;
                    continue;
                }
                self.statement(statement)?;
            }
        }
        if !header || self.depth != 0 || self.model.participants.is_empty() {
            return Err(DiagramError::InvalidInput);
        }
        Ok(self.model)
    }

    fn participant(&mut self, id: &'a str) -> Result<usize, DiagramError> {
        valid_id(id)?;
        common::check(
            self.control,
            self.model
                .participants
                .len()
                .saturating_mul(id.len().saturating_add(1))
                .saturating_add(id.len()),
        )?;
        if let Some(index) = self.model.participants.iter().position(|p| p.id == id) {
            return Ok(index);
        }
        if self.model.participants.len() == MAX_PARTICIPANTS {
            return Err(DiagramError::WorkLimited);
        }
        let index = self.model.participants.len();
        self.model.participants.try_push(Participant {
            id,
            raw: id,
            actor: false,
            declared: false,
            label: None,
            x: 0.0,
            width: 0.0,
        })?;
        Ok(index)
    }

    fn push(&mut self, kind: EventKind, raw: &'a str) -> Result<usize, DiagramError> {
        if self.model.events.len() == MAX_EVENTS {
            return Err(DiagramError::WorkLimited);
        }
        label_syntax(raw)?;
        let index = self.model.events.len();
        self.model.events.try_push(Event {
            kind,
            raw,
            label: None,
            y: 0.0,
            height: 0.0,
        })?;
        Ok(index)
    }

    fn activation(
        &mut self,
        participant: usize,
        start: bool,
        event: usize,
    ) -> Result<(), DiagramError> {
        common::check(self.control, self.model.activations.len().saturating_add(1))?;
        let depth = self.active[participant] as usize;
        if start {
            if depth == MAX_DEPTH || self.model.activations.len() == MAX_EVENTS {
                return Err(DiagramError::WorkLimited);
            }
            self.model.activations.try_push(Activation {
                participant,
                depth,
                start: event,
                end: UNFINISHED,
            })?;
            self.active[participant] += 1;
        } else {
            if depth == 0 {
                return Err(DiagramError::InvalidInput);
            }
            let open = self
                .model
                .activations
                .iter_mut()
                .rev()
                .find(|a| a.participant == participant && a.end == UNFINISHED)
                .ok_or(DiagramError::InvalidInput)?;
            open.end = event;
            self.active[participant] -= 1;
        }
        Ok(())
    }

    fn statement(&mut self, source: &'a str) -> Result<(), DiagramError> {
        if let Some(body) = keyword(source, "participant") {
            return self.declaration(body, false);
        }
        if let Some(body) = keyword(source, "actor") {
            return self.declaration(body, true);
        }
        if let Some(body) = keyword(source, "autonumber") {
            let mut values = body.split_whitespace();
            let start = values.next().map_or(Ok(1), parse_number)?;
            let increment = values.next().map_or(Ok(1), parse_number)?;
            if values.next().is_some() || increment == 0 {
                return Err(DiagramError::InvalidInput);
            }
            self.number = Some(start);
            self.increment = increment;
            return Ok(());
        }
        for (name, start) in [("activate", true), ("deactivate", false)] {
            if let Some(body) = keyword(source, name) {
                let participant = self.participant(body)?;
                let event = self.push(EventKind::Activation, "")?;
                return self.activation(participant, start, event);
            }
        }
        if let Some(body) = keyword(source, "Note").or_else(|| keyword(source, "note")) {
            return self.note(body);
        }
        for (name, kind) in [
            ("loop", Block::Loop),
            ("alt", Block::Alt),
            ("opt", Block::Opt),
            ("par", Block::Par),
            ("critical", Block::Critical),
            ("break", Block::Break),
        ] {
            if let Some(body) = keyword(source, name) {
                if self.depth == MAX_DEPTH {
                    return Err(DiagramError::WorkLimited);
                }
                let index = self.model.frames.len();
                let start = self.push(EventKind::Start(index), body)?;
                self.model.frames.try_push(Frame {
                    kind,
                    start,
                    end: UNFINISHED,
                    depth: self.depth,
                })?;
                self.stack[self.depth] = index;
                self.depth += 1;
                return Ok(());
            }
        }
        for (name, expected) in [
            ("else", Block::Alt),
            ("and", Block::Par),
            ("option", Block::Critical),
        ] {
            if let Some(body) = keyword(source, name) {
                if self.depth == 0 {
                    return Err(DiagramError::InvalidInput);
                }
                let frame = self.stack[self.depth - 1];
                if self.model.frames[frame].kind != expected {
                    return Err(DiagramError::InvalidInput);
                }
                self.push(EventKind::Branch { frame, name }, body)?;
                return Ok(());
            }
        }
        if let Some(tail) = keyword(source, "end") {
            if !tail.is_empty() {
                return Err(DiagramError::InvalidInput);
            }
            if self.depth == 0 {
                return Err(DiagramError::InvalidInput);
            }
            self.depth -= 1;
            let frame = self.stack[self.depth];
            let end = self.push(EventKind::End, "")?;
            self.model.frames[frame].end = end;
            return Ok(());
        }
        // Recognized but unimplemented directives must never be mistaken for
        // messages merely because their arguments contain a hyphen.
        if [
            "box",
            "rect",
            "create",
            "destroy",
            "link",
            "links",
            "properties",
            "details",
            "title",
            "accTitle",
            "accDescr",
        ]
        .iter()
        .any(|word| keyword(source, word).is_some())
            || source.starts_with("%%{")
        {
            return Err(DiagramError::Unsupported);
        }
        if !source.contains(['-', '<']) {
            return Err(DiagramError::Unsupported);
        }
        self.message(source)
    }

    fn declaration(&mut self, source: &'a str, actor: bool) -> Result<(), DiagramError> {
        if source.contains("@{") {
            return Err(DiagramError::Unsupported);
        }
        let at = source.find(char::is_whitespace).unwrap_or(source.len());
        let id = &source[..at];
        let rest = source[at..].trim();
        let raw = if rest.is_empty() {
            id
        } else {
            let alias = keyword(rest, "as").ok_or(DiagramError::InvalidInput)?;
            if alias.is_empty() {
                return Err(DiagramError::InvalidInput);
            }
            alias
        };
        label_syntax(raw)?;
        let participant = self.participant(id)?;
        let p = &mut self.model.participants[participant];
        if p.declared {
            return Err(DiagramError::InvalidInput);
        }
        p.declared = true;
        p.raw = raw;
        p.actor = actor;
        Ok(())
    }

    fn note(&mut self, source: &'a str) -> Result<(), DiagramError> {
        let (placement, raw) = source.split_once(':').ok_or(DiagramError::InvalidInput)?;
        let (side, ids) = if let Some(ids) = keyword(placement.trim(), "over") {
            (Side::Over, ids)
        } else if let Some(ids) = keyword(placement.trim(), "left of") {
            (Side::Left, ids)
        } else if let Some(ids) = keyword(placement.trim(), "right of") {
            (Side::Right, ids)
        } else {
            return Err(DiagramError::InvalidInput);
        };
        let mut ids = ids.split(',');
        let from = self.participant(ids.next().ok_or(DiagramError::InvalidInput)?.trim())?;
        let to = if let Some(id) = ids.next() {
            if side != Side::Over {
                return Err(DiagramError::InvalidInput);
            }
            self.participant(id.trim())?
        } else {
            from
        };
        if ids.next().is_some() {
            return Err(DiagramError::InvalidInput);
        }
        self.push(EventKind::Note { from, to, side }, raw.trim())?;
        Ok(())
    }

    fn message(&mut self, source: &'a str) -> Result<(), DiagramError> {
        let (route, raw) = source.split_once(':').ok_or(DiagramError::InvalidInput)?;
        let at = route.find(['-', '<']).ok_or(DiagramError::InvalidInput)?;
        if route.as_bytes()[at] == b'<' {
            return Err(DiagramError::Unsupported);
        }
        let from = self.participant(route[..at].trim())?;
        let tail = &route[at..];
        let (length, arrow) = [
            ("-->>", true, Marker::Filled),
            ("->>", false, Marker::Filled),
            ("-->", true, Marker::None),
            ("->", false, Marker::None),
            ("--x", true, Marker::Cross),
            ("-x", false, Marker::Cross),
            ("--)", true, Marker::Open),
            ("-)", false, Marker::Open),
        ]
        .into_iter()
        .find(|(token, _, _)| tail.starts_with(token))
        .map(|(token, dashed, marker)| (token.len(), Arrow { dashed, marker }))
        .ok_or(DiagramError::Unsupported)?;
        let tail = tail[length..].trim();
        let (activation, target) = if let Some(id) = tail.strip_prefix('+') {
            (1, id.trim())
        } else if let Some(id) = tail.strip_prefix('-') {
            (-1, id.trim())
        } else {
            (0, tail)
        };
        let to = self.participant(target)?;
        let number = self.number;
        if let Some(value) = number {
            self.number = Some(
                value
                    .checked_add(self.increment)
                    .ok_or(DiagramError::WorkLimited)?,
            );
        }
        let event = self.push(
            EventKind::Message {
                from,
                to,
                arrow,
                number,
            },
            raw.trim(),
        )?;
        match activation {
            1 => self.activation(to, true, event)?,
            -1 => self.activation(from, false, event)?,
            _ => {}
        }
        Ok(())
    }
}

fn keyword<'a>(source: &'a str, word: &str) -> Option<&'a str> {
    source
        .strip_prefix(word)
        .filter(|rest| rest.is_empty() || rest.starts_with(char::is_whitespace))
        .map(str::trim)
}

fn valid_id(id: &str) -> Result<(), DiagramError> {
    if id.is_empty()
        || id.chars().any(|c| {
            c.is_whitespace()
                || c.is_control()
                || matches!(
                    c,
                    ':' | ','
                        | ';'
                        | '-'
                        | '+'
                        | '<'
                        | '>'
                        | '"'
                        | '('
                        | ')'
                        | '['
                        | ']'
                        | '{'
                        | '}'
                )
        })
    {
        return Err(DiagramError::InvalidInput);
    }
    Ok(())
}

fn label_syntax(raw: &str) -> Result<(), DiagramError> {
    if raw.contains('\0') {
        return Err(DiagramError::InvalidInput);
    }
    let mut quoted = false;
    let mut chars = raw.chars();
    while let Some(character) = chars.next() {
        if character == '\\' {
            chars.next();
        } else if character == '"' {
            quoted = !quoted;
        }
    }
    if quoted {
        return Err(DiagramError::InvalidInput);
    }
    Ok(())
}

fn parse_number(raw: &str) -> Result<u32, DiagramError> {
    if raw == "off" || raw == "on" {
        return Err(DiagramError::Unsupported);
    }
    raw.parse().map_err(|_| DiagramError::InvalidInput)
}

/// Allocation-free statement scanner. The source bound also bounds every scan.
struct Statements<'a> {
    rest: &'a str,
}
impl<'a> Statements<'a> {
    fn new(rest: &'a str) -> Self {
        Self { rest }
    }
}
impl<'a> Iterator for Statements<'a> {
    type Item = &'a str;
    fn next(&mut self) -> Option<Self::Item> {
        if self.rest.is_empty() {
            return None;
        }
        let source = self.rest;
        let bytes = source.as_bytes();
        let mut at = 0;
        let mut quoted = false;
        while at < bytes.len() {
            match bytes[at] {
                b'\\' => at = (at + 2).min(bytes.len()),
                b'"' => {
                    quoted = !quoted;
                    at += 1;
                }
                b'#' | b'&' if !quoted => {
                    let start = at;
                    at += 1;
                    while at < bytes.len()
                        && at - start <= 66
                        && (bytes[at].is_ascii_alphanumeric() || matches!(bytes[at], b'#' | b'+'))
                    {
                        at += 1;
                    }
                    if at > start + 1 && bytes.get(at) == Some(&b';') {
                        at += 1;
                    }
                }
                b';' if !quoted => {
                    self.rest = &source[at + 1..];
                    return Some(&source[..at]);
                }
                _ => at += 1,
            }
        }
        self.rest = "";
        Some(source)
    }
}

struct Timeline<'a> {
    model: Model<'a>,
    policy: LayoutPolicy,
    top: f64,
    bottom: f64,
    header_height: f64,
    left: f64,
    right: f64,
}

impl<'a> Timeline<'a> {
    fn prepare(
        mut model: Model<'a>,
        policy: LayoutPolicy,
        provider: &dyn RichTextProvider,
        scope: &MemoryScope,
        control: &OperationControl,
    ) -> Result<Self, DiagramError> {
        for value in [
            policy.wrap_width,
            policy.node_gap,
            policy.rank_gap,
            policy.padding,
            policy.lane_pitch,
            policy.clearance,
            policy.margin,
        ] {
            if !value.is_finite() || !(0.0..=4096.0).contains(&value) {
                return Err(DiagramError::InvalidGeometry);
            }
        }
        if policy.wrap_width == 0.0 {
            return Err(DiagramError::InvalidGeometry);
        }
        let padding = policy.padding.max(8.0);
        let mut header_height = 48.0_f64;
        for p in &mut model.participants {
            let decoded = common::decode_label(p.raw, scope)?;
            let label = common::label(&decoded, policy, provider, control)?;
            p.width = (label.width + 2.0 * padding).max(80.0);
            header_height =
                header_height.max(label.height + 2.0 * padding + if p.actor { 56.0 } else { 0.0 });
            p.label = Some(label);
        }
        for event in &mut model.events {
            common::check(control, 1)?;
            let decoded = common::decode_label(event.raw, scope)?;
            let mut authored = FundedString::new_in(scope.clone(), Family::Text);
            match event.kind {
                EventKind::Message {
                    number: Some(n), ..
                } => {
                    write!(authored, "{n}. ").map_err(|_| {
                        DiagramError::Memory(
                            scope
                                .failure()
                                .unwrap_or(markdown_memory::MemoryError::InvalidState),
                        )
                    })?;
                }
                EventKind::Start(frame) => {
                    authored.try_push_str(model.frames[frame].kind.name())?;
                    authored.try_push_str(" [")?;
                }
                EventKind::Branch { name, .. } => {
                    authored.try_push_str(name)?;
                    authored.try_push_str(" [")?;
                }
                EventKind::Activation | EventKind::End => continue,
                _ => {}
            }
            authored.try_push_str(&decoded)?;
            if matches!(event.kind, EventKind::Start(_) | EventKind::Branch { .. }) {
                authored.try_push(']')?;
            }
            event.label = Some(common::label(&authored, policy, provider, control)?);
        }

        let count = model.participants.len();
        let mut gaps = FundedVec::try_with_capacity(scope, Family::Work, count.saturating_sub(1))?;
        for pair in model.participants.windows(2) {
            gaps.try_push((pair[0].width + pair[1].width) / 2.0 + policy.node_gap.max(48.0))?;
        }
        for event in &model.events {
            common::check(control, count.saturating_mul(3).saturating_add(1))?;
            let width = event.label.as_ref().map_or(0.0, |label| label.width) + 2.0 * padding;
            match event.kind {
                EventKind::Message { from, to, .. } if from != to => {
                    span(&mut gaps, from, to, width + 24.0);
                }
                EventKind::Message { from, .. } => {
                    if from + 1 < count {
                        span(
                            &mut gaps,
                            from,
                            from + 1,
                            width + model.participants[from + 1].width / 2.0 + 60.0,
                        );
                    }
                }
                EventKind::Note {
                    from,
                    to,
                    side: Side::Over,
                } if from != to => {
                    span(&mut gaps, from, to, width);
                }
                EventKind::Note { from, side, .. } => {
                    if from > 0 && side != Side::Right {
                        let need = if side == Side::Left {
                            width + 32.0
                        } else {
                            width / 2.0 + 16.0
                        };
                        span(
                            &mut gaps,
                            from - 1,
                            from,
                            need + model.participants[from - 1].width / 2.0,
                        );
                    }
                    if from + 1 < count && side != Side::Left {
                        let need = if side == Side::Right {
                            width + 32.0
                        } else {
                            width / 2.0 + 16.0
                        };
                        span(
                            &mut gaps,
                            from,
                            from + 1,
                            need + model.participants[from + 1].width / 2.0,
                        );
                    }
                }
                _ => {}
            }
        }
        let mut x = 0.0;
        for (index, participant) in model.participants.iter_mut().enumerate() {
            participant.x = x;
            if let Some(gap) = gaps.get(index) {
                x += gap;
            }
        }
        let top = header_height + 24.0;
        let mut y = top + 12.0;
        for event in &mut model.events {
            let label_height = event.label.as_ref().map_or(0.0, |label| label.height);
            event.y = y;
            event.height = match event.kind {
                EventKind::Message { from, to, .. } => {
                    label_height + if from == to { 66.0 } else { 38.0 }
                }
                EventKind::Note { .. } => label_height + 2.0 * padding + 16.0,
                EventKind::Start(_) | EventKind::Branch { .. } => label_height + 24.0,
                EventKind::End => 18.0,
                EventKind::Activation => 12.0,
            };
            y += event.height;
        }
        let bottom = y + 20.0;
        let mut left = model.participants[0].x
            - model.participants[0].width / 2.0
            - 28.0
            - MAX_DEPTH as f64 * 12.0;
        let last = &model.participants[count - 1];
        let mut right = last.x + last.width / 2.0 + 28.0 + MAX_DEPTH as f64 * 12.0;
        for event in &model.events {
            common::check(control, 1)?;
            if let EventKind::Note { from, to, side } = event.kind {
                let r = note_rect(&model, event, from, to, side, padding);
                left = left.min(r.left - 12.0);
                right = right.max(r.right + 12.0);
            } else if let EventKind::Message { from, to, .. } = event.kind {
                if from == to {
                    let width = event.label.as_ref().map_or(0.0, |label| label.width);
                    right = right.max(model.participants[from].x + width + 72.0);
                }
            }
        }
        // A deepest nested frame still has room for its condition label,
        // including when the diagram contains just one short participant.
        for event in &model.events {
            common::check(control, 1)?;
            let frame = match event.kind {
                EventKind::Start(frame) | EventKind::Branch { frame, .. } => frame,
                _ => continue,
            };
            let width = event.label.as_ref().map_or(0.0, |label| label.width);
            let required = width + 40.0 + 24.0 * model.frames[frame].depth as f64;
            right = right.max(left + required);
        }
        Ok(Self {
            model,
            policy,
            top,
            bottom,
            header_height,
            left,
            right,
        })
    }

    fn event_y(&self, index: usize) -> f64 {
        if index == UNFINISHED {
            return self.bottom;
        }
        let event = &self.model.events[index];
        match event.kind {
            EventKind::Message { from, to, .. } => {
                event.y
                    + event.label.as_ref().map_or(0.0, |label| label.height)
                    + if from == to { 40.0 } else { 14.0 }
            }
            _ => event.y + 6.0,
        }
    }

    fn frame_sides(&self, frame: usize) -> (f64, f64) {
        let inset = self.model.frames[frame].depth as f64 * 12.0;
        (self.left + inset + 8.0, self.right - inset - 8.0)
    }

    fn endpoint(&self, participant: usize, event: usize, right: bool, outgoing: bool) -> f64 {
        let mut x = self.model.participants[participant].x;
        let mut active = None;
        for activation in &self.model.activations {
            if activation.participant == participant
                && activation.start <= event
                && !(outgoing && activation.start == event)
                && (activation.end == UNFINISHED || activation.end >= event)
            {
                active = Some(active.map_or(activation.depth, |d: usize| d.max(activation.depth)));
            }
        }
        if let Some(depth) = active {
            x += depth as f64 * 5.0 + if right { 6.0 } else { -6.0 };
        }
        x
    }

    fn paint(
        &self,
        theme: ThemePolicy,
        provider: &dyn RichTextProvider,
        scope: &MemoryScope,
        control: &OperationControl,
    ) -> Result<OwnedScene, DiagramError> {
        let mut scene = SceneBuilder::new(scope, 4 * 1024 * 1024, 65_536);
        common::shape(
            &mut scene,
            Shape::Rectangle,
            rect(
                self.left - 8.0,
                -8.0,
                self.right - self.left + 16.0,
                self.bottom + self.header_height + 44.0,
            ),
            Paint {
                fill: Some(theme.background),
                stroke: None,
                ..Paint::default()
            },
        )?;
        // Borders are painted before timeline details; transparent frame
        // interiors preserve nested messages and lifelines.
        for (index, frame) in self.model.frames.iter().enumerate() {
            common::check(control, 1)?;
            let (left, right) = self.frame_sides(index);
            let y = self.model.events[frame.start].y - 4.0;
            let bottom = self.model.events[frame.end].y + 12.0;
            common::shape(
                &mut scene,
                Shape::Rectangle,
                rect(left, y, right - left, bottom - y),
                Paint {
                    fill: None,
                    ..theme.group
                },
            )?;
        }
        let mut lifeline = theme.edge;
        lifeline.fill = None;
        lifeline.dash = [5.0, 5.0, 0.0, 0.0];
        lifeline.dash_len = 2;
        for participant in &self.model.participants {
            common::check(control, 1)?;
            common::line(
                &mut scene,
                point(participant.x, self.top),
                point(participant.x, self.bottom),
                lifeline,
            )?;
        }
        for activation in &self.model.activations {
            common::check(control, 1)?;
            let y = self.event_y(activation.start);
            let end = self.event_y(activation.end).max(y + 10.0);
            let x =
                self.model.participants[activation.participant].x + activation.depth as f64 * 5.0;
            common::shape(
                &mut scene,
                Shape::Rectangle,
                rect(x - 6.0, y, 12.0, end - y),
                theme.node,
            )?;
        }
        for (index, event) in self.model.events.iter().enumerate() {
            common::check(
                control,
                self.model
                    .activations
                    .len()
                    .saturating_mul(3)
                    .saturating_add(1),
            )?;
            let Some(label) = &event.label else {
                continue;
            };
            match event.kind {
                EventKind::Message {
                    from, to, arrow, ..
                } => {
                    let right = to >= from;
                    let x1 = self.endpoint(from, index, right, true);
                    let x2 = self.endpoint(to, index, !right, false);
                    let y = self.event_y(index);
                    let mut paint = theme.edge;
                    paint.fill = None;
                    paint.dash_len = if arrow.dashed { 2 } else { 0 };
                    paint.dash = [5.0, 4.0, 0.0, 0.0];
                    if from == to {
                        let target_x = self.endpoint(to, index, true, false);
                        let loop_x = x1 + label.width.max(44.0) + 24.0;
                        let upper = y - 26.0;
                        path(
                            &mut scene,
                            &[
                                point(x1, upper),
                                point(loop_x, upper),
                                point(loop_x, y),
                                point(target_x, y),
                            ],
                            paint,
                            control,
                        )?;
                        marker(
                            &mut scene,
                            point(target_x, y),
                            -1.0,
                            arrow.marker,
                            theme.edge,
                            control,
                        )?;
                        common::text(&mut scene, provider, label, theme.text, x1 + 14.0, event.y)?;
                    } else {
                        common::line(&mut scene, point(x1, y), point(x2, y), paint)?;
                        marker(
                            &mut scene,
                            point(x2, y),
                            if right { 1.0 } else { -1.0 },
                            arrow.marker,
                            theme.edge,
                            control,
                        )?;
                        common::text(
                            &mut scene,
                            provider,
                            label,
                            theme.text,
                            (x1 + x2 - label.width) / 2.0,
                            event.y,
                        )?;
                    }
                }
                EventKind::Note { from, to, side } => {
                    let padding = self.policy.padding.max(8.0);
                    let r = note_rect(&self.model, event, from, to, side, padding);
                    common::shape(&mut scene, Shape::Rectangle, r, theme.group)?;
                    common::text(
                        &mut scene,
                        provider,
                        label,
                        theme.text,
                        (r.left + r.right - label.width) / 2.0,
                        r.top + padding,
                    )?;
                }
                EventKind::Start(frame) => {
                    let (left, _) = self.frame_sides(frame);
                    common::shape(
                        &mut scene,
                        Shape::Rectangle,
                        rect(left + 2.0, event.y, label.width + 18.0, label.height + 12.0),
                        Paint {
                            fill: Some(theme.background),
                            stroke: None,
                            ..Paint::default()
                        },
                    )?;
                    common::text(
                        &mut scene,
                        provider,
                        label,
                        theme.text,
                        left + 10.0,
                        event.y + 4.0,
                    )?;
                }
                EventKind::Branch { frame, .. } => {
                    let (left, right) = self.frame_sides(frame);
                    let mut paint = theme.edge;
                    paint.dash = [6.0, 4.0, 0.0, 0.0];
                    paint.dash_len = 2;
                    common::line(
                        &mut scene,
                        point(left, event.y - 4.0),
                        point(right, event.y - 4.0),
                        paint,
                    )?;
                    common::shape(
                        &mut scene,
                        Shape::Rectangle,
                        rect(left + 2.0, event.y, label.width + 18.0, label.height + 12.0),
                        Paint {
                            fill: Some(theme.background),
                            stroke: None,
                            ..Paint::default()
                        },
                    )?;
                    common::text(
                        &mut scene,
                        provider,
                        label,
                        theme.text,
                        left + 10.0,
                        event.y + 4.0,
                    )?;
                }
                EventKind::Activation | EventKind::End => {}
            }
        }
        for participant in &self.model.participants {
            common::check(control, 1)?;
            self.draw_participant(&mut scene, participant, 0.0, theme, provider, control)?;
            self.draw_participant(
                &mut scene,
                participant,
                self.bottom + 16.0,
                theme,
                provider,
                control,
            )?;
        }
        common::check(control, 1)?;
        scene
            .finish(self.policy.margin)
            .map_err(|e| common::canonical(e, control))
    }

    fn draw_participant(
        &self,
        scene: &mut SceneBuilder,
        participant: &Participant<'_>,
        y: f64,
        theme: ThemePolicy,
        provider: &dyn RichTextProvider,
        control: &OperationControl,
    ) -> Result<(), DiagramError> {
        let label = participant
            .label
            .as_ref()
            .ok_or(DiagramError::InvalidInput)?;
        let x = participant.x;
        if participant.actor {
            common::shape(
                scene,
                Shape::Circle,
                rect(x - 9.0, y + 2.0, 18.0, 18.0),
                theme.node,
            )?;
            path(
                scene,
                &[
                    point(x, y + 20.0),
                    point(x, y + 39.0),
                    point(x - 14.0, y + 53.0),
                ],
                theme.edge,
                control,
            )?;
            common::line(
                scene,
                point(x, y + 39.0),
                point(x + 14.0, y + 53.0),
                theme.edge,
            )?;
            common::line(
                scene,
                point(x - 18.0, y + 29.0),
                point(x + 18.0, y + 29.0),
                theme.edge,
            )?;
            common::text(
                scene,
                provider,
                label,
                theme.text,
                x - label.width / 2.0,
                y + 58.0,
            )?;
        } else {
            common::shape(
                scene,
                Shape::Rounded,
                rect(
                    x - participant.width / 2.0,
                    y,
                    participant.width,
                    self.header_height,
                ),
                theme.node,
            )?;
            common::text(
                scene,
                provider,
                label,
                theme.text,
                x - label.width / 2.0,
                y + (self.header_height - label.height) / 2.0,
            )?;
        }
        Ok(())
    }
}

fn span(gaps: &mut [f64], from: usize, to: usize, required: f64) {
    let (left, right) = (from.min(to), from.max(to));
    if left == right {
        return;
    }
    let current = gaps[left..right].iter().sum::<f64>();
    let addition = ((required - current) / (right - left) as f64).max(0.0);
    for gap in &mut gaps[left..right] {
        *gap += addition;
    }
}

fn note_rect(
    model: &Model<'_>,
    event: &Event<'_>,
    from: usize,
    to: usize,
    side: Side,
    padding: f64,
) -> Rect {
    let label = event.label.as_ref();
    let width = label.map_or(0.0, |label| label.width) + 2.0 * padding;
    let height = label.map_or(0.0, |label| label.height) + 2.0 * padding;
    let x1 = model.participants[from].x;
    let x2 = model.participants[to].x;
    let (left, right) = match side {
        Side::Left => (x1 - 22.0 - width, x1 - 22.0),
        Side::Right => (x1 + 22.0, x1 + 22.0 + width),
        Side::Over => {
            let width = width.max((x2 - x1).abs() + 2.0 * padding);
            ((x1 + x2 - width) / 2.0, (x1 + x2 + width) / 2.0)
        }
    };
    rect(left, event.y + 6.0, right - left, height)
}

fn point(x: f64, y: f64) -> Point {
    Point { x, y }
}
fn rect(x: f64, y: f64, width: f64, height: f64) -> Rect {
    Rect {
        left: x,
        top: y,
        right: x + width,
        bottom: y + height,
    }
}

fn path(
    scene: &mut SceneBuilder,
    points: &[Point],
    mut paint: Paint,
    control: &OperationControl,
) -> Result<(), DiagramError> {
    paint.fill = None;
    scene
        .begin_path(paint, Point::default())
        .map_err(|e| common::canonical(e, control))?;
    for (index, p) in points.iter().enumerate() {
        let c = if index == 0 {
            PathCommand::move_to(p.x, p.y)
        } else {
            PathCommand::line_to(p.x, p.y)
        };
        scene.push(c).map_err(|e| common::canonical(e, control))?;
    }
    scene.end_path().map_err(|e| common::canonical(e, control))
}

fn marker(
    scene: &mut SceneBuilder,
    at: Point,
    direction: f64,
    marker: Marker,
    mut paint: Paint,
    control: &OperationControl,
) -> Result<(), DiagramError> {
    paint.dash_len = 0;
    paint.fill = None;
    match marker {
        Marker::None => Ok(()),
        Marker::Open => path(
            scene,
            &[
                point(at.x - direction * 10.0, at.y - 5.0),
                at,
                point(at.x - direction * 10.0, at.y + 5.0),
            ],
            paint,
            control,
        ),
        Marker::Cross => {
            path(
                scene,
                &[point(at.x - 5.0, at.y - 5.0), point(at.x + 5.0, at.y + 5.0)],
                paint,
                control,
            )?;
            path(
                scene,
                &[point(at.x - 5.0, at.y + 5.0), point(at.x + 5.0, at.y - 5.0)],
                paint,
                control,
            )
        }
        Marker::Filled => {
            paint.fill = paint.stroke;
            scene
                .begin_path(paint, Point::default())
                .map_err(|e| common::canonical(e, control))?;
            for c in [
                PathCommand::move_to(at.x, at.y),
                PathCommand::line_to(at.x - direction * 10.0, at.y - 5.0),
                PathCommand::line_to(at.x - direction * 10.0, at.y + 5.0),
                PathCommand::close(),
            ] {
                scene.push(c).map_err(|e| common::canonical(e, control))?;
            }
            scene.end_path().map_err(|e| common::canonical(e, control))
        }
    }
}

pub fn render_sequence_native_controlled(
    source: &str,
    policy: LayoutPolicy,
    theme: ThemePolicy,
    provider: &dyn RichTextProvider,
    control: &OperationControl,
) -> Result<OwnedScene, DiagramError> {
    let result = (|| {
        let scope = common::scope(control)?;
        let model = Parser::new(&scope, control).parse(source)?;
        Timeline::prepare(model, policy, provider, &scope, control)?
            .paint(theme, provider, &scope, control)
    })();
    result.map_err(|error| {
        // Shared scene helpers carry cancellation without its control reason.
        // Resolve it once at this boundary, preserving funded failure priority
        // and distinguishing an expired deadline from requested cancellation.
        if error == DiagramError::Cancelled {
            common::check(control, 0).err().unwrap_or(error)
        } else {
            error
        }
    })
}

#[cfg(test)]
#[path = "sequence/tests.rs"]
mod tests;
