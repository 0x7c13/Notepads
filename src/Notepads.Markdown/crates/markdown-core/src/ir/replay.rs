// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

//! Independent minimal transform replay. Grammar acceptance remains the native producer's duty.

use super::storage::owned_row;
use super::*;
use crate::error::LimitKind;
use crate::memory::BuildGrowingVec;
use crate::policy::{CHECKPOINT_BYTES, OwnedWork};
use crate::source::SourceSnapshot;

#[derive(Clone, Copy)]
struct Unit {
    start: usize,
    end: usize,
    component: usize,
}
#[derive(Clone, Copy)]
struct Component {
    parent: usize,
    rank: u32,
    operations: u16,
    stages: u32,
    source: SourceSpan,
}

const ENTITY: u16 = 1;
const ESCAPE: u16 = 2;
const BLOCK: u16 = 4;
const BREAK: u16 = 8;
const TRIM: u16 = 16;
const CODE_BREAK: u16 = 32;
const CODE_TRIM: u16 = 64;
const EMOJI: u16 = 128;
const MAX_EMOJI_STAGE_BYTES: usize = 66;
const MAX_STANDARD_STAGE_BYTES: usize = 34;

fn stage_byte_limit(kind: StageKind) -> usize {
    match kind {
        StageKind::EmojiShortcode => MAX_EMOJI_STAGE_BYTES,
        _ => MAX_STANDARD_STAGE_BYTES,
    }
}

#[derive(Clone, Copy)]
struct StageContext {
    claim: AuthoritySpec,
    host: HostRow,
    payload: Option<PayloadRole>,
    text_len: usize,
    math_syntax: Option<MathSyntax>,
}

pub(super) fn replay<'id>(
    ir: &CompactIr,
    source: &SourceSnapshot,
    atom_id: AtomId,
    payload: Option<PayloadRole>,
    op: &mut OwnedWork<'id, '_>,
) -> Result<(), CoreError> {
    let result = inner(ir, source, atom_id, payload, op);
    result.map_err(|error| op.fail(error))
}

fn require(value: bool) -> Result<(), CoreError> {
    if value {
        Ok(())
    } else {
        Err(CoreError::InvalidProvenance)
    }
}

fn inner<'id>(
    ir: &CompactIr,
    source: &SourceSnapshot,
    atom_id: AtomId,
    payload: Option<PayloadRole>,
    op: &mut OwnedWork<'id, '_>,
) -> Result<(), CoreError> {
    op.check()?;
    let atom = ir.owned_atom_at(atom_id.index(), op)?;
    let math_syntax = if payload == Some(PayloadRole::MathInput) {
        let claim = ir.owned_read_authority(atom.claim, op)?;
        let ClaimKind::Math(syntax) = claim.kind else {
            return Err(op.fail(CoreError::InvalidProvenance));
        };
        Some(syntax)
    } else {
        None
    };
    let proof_id = atom.proof.ok_or(CoreError::InvalidProvenance)?;
    let proof = *ir
        .proofs()
        .get(proof_id.index())
        .ok_or(CoreError::InvalidProvenance)?;
    require(
        proof.atom == atom_id
            && match atom.kind {
                AtomKind::Transform => {
                    atom.content == AtomContent::Pool(proof.value) && !proof.value.is_empty()
                }
                AtomKind::Hidden => atom.content == AtomContent::Empty && proof.value.is_empty(),
                _ => false,
            },
    )?;
    let primitives = proof.primitives.slice(ir.primitives())?;
    let stages = proof.stages.slice(ir.stages())?;
    require(!primitives.is_empty())?;
    if let Some(stage) = stages.first().filter(|stage| {
        matches!(
            stage.kind,
            StageKind::HtmlBreakToLineFeed
                | StageKind::HtmlEmphasisDelimiter
                | StageKind::HtmlCodeDelimiter
        )
    }) {
        let is_break = stage.kind == StageKind::HtmlBreakToLineFeed;
        let sources = atom.sources.slice(ir.sources())?;
        require(primitives.len() == 1 && sources.len() == 1)?;
        let primitive = primitives[0];
        let span = sources[0];
        let claim = ir.owned_authority(atom.claim, op)?;
        require(
            stages.len() == 1
                && if is_break {
                    atom.kind == AtomKind::Transform && proof.value.len() == 1
                } else {
                    atom.kind == AtomKind::Hidden && proof.value.is_empty()
                }
                && payload.is_none()
                && primitive.kind == PrimitiveKind::Copy
                && primitive.source == span
                && atom.claim == ClaimId::primary(atom_id)
                && stage.accepted_by == atom.claim
                && (!is_break || stage.parameter == 0)
                && stage.input.start() == 0
                && stage.input.end() == span.len()
                && stage.output == proof.value
                && claim.kind == ClaimKind::Html
                && claim.phase == NativePhase::Inline
                && claim.host.role == HostRole::Inline
                && claim.owner == atom.owner
                && claim.host.owner == atom.owner
                && claim.consumed.end() - claim.consumed.start() == span.len()
                && claim.sources.slice(ir.sources())? == sources,
        )?;
        source.validate_span(span)?;
        let start = usize::try_from(span.start()).map_err(|_| CoreError::ArithmeticOverflow)?;
        let end = usize::try_from(span.end()).map_err(|_| CoreError::ArithmeticOverflow)?;
        let raw = source
            .text()
            .as_bytes()
            .get(start..end)
            .ok_or(CoreError::InvalidProvenance)?;
        let maximum = if stage.kind == StageKind::HtmlEmphasisDelimiter {
            HtmlEmphasis::MAX_DELIMITER_BYTES
        } else {
            34
        };
        require((3..=maximum).contains(&raw.len()))?;
        op.events(2)?;
        op.scan(raw.len() as u64)?;
        if is_break {
            require(is_inline_html_break(raw))?;
            equal_bytes(ir.text_owned(proof.value, op)?.as_bytes(), b"\n", op)?;
        } else {
            let parameter = if stage.kind == StageKind::HtmlCodeDelimiter {
                HtmlCodeDelimiter::parse(raw).map(HtmlCodeDelimiter::parameter)
            } else {
                HtmlEmphasis::delimiter(raw).map(HtmlEmphasis::parameter)
            };
            require(
                parameter == Some(stage.parameter) && ir.text_owned(proof.value, op)?.is_empty(),
            )?;
        }
        return op.check();
    }
    let limits = op
        .model_limits()
        .ok_or(CoreError::UnsupportedConfiguration)?;
    let sources = atom.sources.slice(ir.sources())?;
    if primitives.len() == 1
        && stages.len() == 1
        && sources.len() == 1
        && primitives[0].kind == PrimitiveKind::Copy
        && stages[0].kind != StageKind::AsciiTrim
        && limits.proof_records >= stage_byte_limit(stages[0].kind) as u64
        && limits.decoded_bytes >= stage_byte_limit(stages[0].kind) as u64
    {
        let primitive = primitives[0];
        let stage = stages[0];
        source.validate_span(primitive.source)?;
        let start =
            usize::try_from(primitive.source.start()).map_err(|_| CoreError::ArithmeticOverflow)?;
        let end =
            usize::try_from(primitive.source.end()).map_err(|_| CoreError::ArithmeticOverflow)?;
        let raw = source
            .text()
            .get(start..end)
            .ok_or(CoreError::InvalidProvenance)?;
        let output = ir.text_owned(stage.output, op)?;
        if primitive.source == sources[0]
            && stage.input.start() == 0
            && stage.input.end() == raw.len() as u64
            && raw.len() <= stage_byte_limit(stage.kind)
            && output.len() <= stage_byte_limit(stage.kind)
        {
            op.events(2)?;
            require(!raw.is_empty())?;
            for scalar in raw.chars() {
                op.scan(scalar.len_utf8() as u64)?;
                op.events(1)?;
                require(scalar != '\r' && scalar != '\0')?;
            }
            require(
                !(raw.starts_with('\n')
                    && start > 0
                    && source.text().as_bytes().get(start - 1) == Some(&b'\r')),
            )?;
            // Keep the bounded stage preparation and independent decoder checks.
            // No scalar Component/Unit allocation is needed: this one whole
            // stage touches and connects every scalar of the one primitive.
            op.scan(raw.len() as u64)?;
            let input =
                std::str::from_utf8(raw.as_bytes()).map_err(|_| CoreError::InvalidProvenance)?;
            op.scan(output.len() as u64)?;
            let output =
                std::str::from_utf8(output.as_bytes()).map_err(|_| CoreError::InvalidProvenance)?;
            let claim = ir.owned_authority(stage.accepted_by, op)?;
            let host = claim.host;
            let metadata = matches!(
                host.role,
                HostRole::Destination
                    | HostRole::Title
                    | HostRole::FenceInfo
                    | HostRole::ReferenceLabel
            );
            let operation = validate_stage(
                stage,
                input,
                input.as_bytes(),
                input.len(),
                output,
                StageContext {
                    claim,
                    host,
                    payload,
                    text_len: raw.len(),
                    math_syntax,
                },
                op,
            )?;
            validate_order(0, operation, metadata, host.role)?;
            let authority = claim.sources.slice(ir.sources())?;
            require(!authority.is_empty() && covered(primitive.source, authority, op)?)?;
            equal_bytes(output.as_bytes(), pool(ir, proof.value, op)?, op)?;
            return op.check();
        }
    }
    let mut text = BuildGrowingVec::<u8>::new(limits.decoded_bytes, LimitKind::DecodedBytes, op)?;
    let mut units =
        BuildGrowingVec::<Unit>::new(limits.proof_records, LimitKind::ProofRecords, op)?;
    let mut components =
        BuildGrowingVec::<Component>::new(limits.proof_records, LimitKind::ProofRecords, op)?;
    let mut source_index = 0;
    let mut cursor = sources.first().ok_or(CoreError::InvalidProvenance)?.start();
    let mut changed = false;
    for primitive in primitives {
        op.events(1)?;
        source.validate_span(primitive.source)?;
        let current = sources
            .get(source_index)
            .ok_or(CoreError::InvalidProvenance)?;
        require(
            !primitive.source.is_empty()
                && primitive.source.start() == cursor
                && primitive.source.end() <= current.end(),
        )?;
        let raw = &source.text()[usize::try_from(primitive.source.start())
            .map_err(|_| CoreError::ArithmeticOverflow)?
            ..usize::try_from(primitive.source.end())
                .map_err(|_| CoreError::ArithmeticOverflow)?];
        match primitive.kind {
            PrimitiveKind::Copy => {
                for (offset, scalar) in raw.char_indices() {
                    op.scan(scalar.len_utf8() as u64)?;
                    require(scalar != '\r' && scalar != '\0')?;
                    let index = components.len();
                    let begin = primitive
                        .source
                        .start()
                        .checked_add(offset as u64)
                        .ok_or(CoreError::ArithmeticOverflow)?;
                    let end = begin
                        .checked_add(scalar.len_utf8() as u64)
                        .ok_or(CoreError::ArithmeticOverflow)?;
                    components.push(
                        Component {
                            parent: index,
                            rank: 0,
                            operations: 0,
                            stages: 0,
                            source: SourceSpan::new(begin, end)?,
                        },
                        op,
                    )?;
                    let mut buffer = [0; 4];
                    let value = scalar.encode_utf8(&mut buffer);
                    push_unit(&mut text, &mut units, value.as_bytes(), index, op)?;
                }
                // A Copy cannot claim the second half of a CRLF primitive.
                require(
                    !(raw.starts_with('\n')
                        && primitive.source.start() > 0
                        && source
                            .text()
                            .as_bytes()
                            .get(primitive.source.start() as usize - 1)
                            == Some(&b'\r')),
                )?;
            }
            kind => {
                require(raw.len() <= 3)?;
                op.scan(raw.len() as u64)?;
                let value = match kind {
                    PrimitiveKind::CrLf if raw == "\r\n" => "\n",
                    PrimitiveKind::Cr
                        if raw == "\r"
                            && source
                                .text()
                                .as_bytes()
                                .get(primitive.source.end() as usize)
                                != Some(&b'\n') =>
                    {
                        "\n"
                    }
                    PrimitiveKind::Nul if raw == "\0" => "\u{fffd}",
                    _ => return Err(CoreError::InvalidProvenance),
                };
                changed = true;
                let index = components.len();
                components.push(
                    Component {
                        parent: index,
                        rank: 0,
                        operations: 0,
                        stages: 0,
                        source: primitive.source,
                    },
                    op,
                )?;
                push_unit(&mut text, &mut units, value.as_bytes(), index, op)?;
            }
        }
        cursor = primitive.source.end();
        if cursor == current.end() {
            source_index += 1;
            if let Some(next) = sources.get(source_index) {
                cursor = next.start();
            }
        }
    }
    require(source_index == sources.len())?;

    let mut inline_started = false;
    for stage in stages {
        op.events(1)?;
        let start =
            usize::try_from(stage.input.start()).map_err(|_| CoreError::ArithmeticOverflow)?;
        let end = usize::try_from(stage.input.end()).map_err(|_| CoreError::ArithmeticOverflow)?;
        require(start < end && start <= text.len() && end <= text.len())?;
        let output = pool(ir, stage.output, op)?;
        let maximum = stage_byte_limit(stage.kind);
        require(output.len() <= maximum)?;
        let input_len = end - start;
        let mut input_bytes = [0_u8; MAX_EMOJI_STAGE_BYTES];
        let input = if stage.kind == StageKind::AsciiTrim {
            require(output.is_empty())?;
            for offset in start..end {
                let value = owned_row(&text, offset, op)?;
                op.scan(1)?;
                require(matches!(value, b' ' | b'\t' | b'\n' | b'\r'))?;
            }
            &input_bytes[..0]
        } else {
            require(input_len <= maximum)?;
            for (offset, slot) in input_bytes[..input_len].iter_mut().enumerate() {
                *slot = owned_row(&text, start + offset, op)?;
            }
            op.scan(input_len as u64)?;
            &input_bytes[..input_len]
        };
        let input_str = std::str::from_utf8(input).map_err(|_| CoreError::InvalidProvenance)?;
        op.scan(output.len() as u64)?;
        let output_str = std::str::from_utf8(output).map_err(|_| CoreError::InvalidProvenance)?;
        let claim = ir.owned_authority(stage.accepted_by, op)?;
        let host = claim.host;
        let metadata = matches!(
            host.role,
            HostRole::Destination
                | HostRole::Title
                | HostRole::FenceInfo
                | HostRole::ReferenceLabel
        );
        let operation = validate_stage(
            *stage,
            input_str,
            input,
            input_len,
            output_str,
            StageContext {
                claim,
                host,
                payload,
                text_len: text.len(),
                math_syntax,
            },
            op,
        )?;
        if matches!(
            stage.kind,
            StageKind::PartialTab | StageKind::TableEscapedPipe
        ) {
            require(!inline_started)?;
        } else {
            inline_started = true;
        }
        let mut joined = None;
        for unit_index in 0..units.len() {
            let unit = owned_row(&units, unit_index, op)?;
            op.events(1)?;
            if unit.start < end && start < unit.end {
                require(start <= unit.start && unit.end <= end)?;
                let root = find(&mut components, unit.component, op)?;
                let state = owned_row(&components, root, op)?;
                validate_order(state.operations, operation, metadata, host.role)?;
                joined = Some(match joined {
                    None => root,
                    Some(previous) => unite(&mut components, previous, root, op)?,
                });
            }
        }
        let root = joined.ok_or(CoreError::InvalidProvenance)?;
        let mut component = owned_row(&components, root, op)?;
        component.operations |= operation;
        component.stages = component
            .stages
            .checked_add(1)
            .ok_or(CoreError::ArithmeticOverflow)?;
        components.replace(root, component, op)?;

        // Every stage is bound to the atom's actual native source authority.
        let claimed = claim.sources.slice(ir.sources())?;
        require(!claimed.is_empty())?;
        for index in 0..components.len() {
            op.events(1)?;
            if find(&mut components, index, op)? == root {
                require(covered(
                    owned_row(&components, index, op)?.source,
                    claimed,
                    op,
                )?)?;
            }
        }
        let length = text
            .len()
            .checked_sub(end - start)
            .and_then(|n| n.checked_add(output.len()))
            .ok_or(CoreError::ArithmeticOverflow)?;
        op.check_limit(LimitKind::DecodedBytes, length as u64, limits.decoded_bytes)?;
        let mut next_text =
            BuildGrowingVec::<u8>::new(limits.decoded_bytes, LimitKind::DecodedBytes, op)?;
        let mut next_units =
            BuildGrowingVec::<Unit>::new(limits.proof_records, LimitKind::ProofRecords, op)?;
        next_text.extend_from_rows(&text, 0, start, op)?;
        next_text.extend_from_slice(output, op)?;
        next_text.extend_from_rows(&text, end, text.len(), op)?;
        for unit_index in 0..units.len() {
            let unit = owned_row(&units, unit_index, op)?;
            op.events(1)?;
            if unit.end <= start {
                next_units.push(unit, op)?;
            }
        }
        for (offset, scalar) in output_str.char_indices() {
            op.scan(scalar.len_utf8() as u64)?;
            next_units.push(
                Unit {
                    start: start + offset,
                    end: start + offset + scalar.len_utf8(),
                    component: root,
                },
                op,
            )?;
        }
        for unit_index in 0..units.len() {
            let unit = owned_row(&units, unit_index, op)?;
            op.events(1)?;
            if unit.start >= end {
                let begin = unit
                    .start
                    .checked_sub(end)
                    .and_then(|n| n.checked_add(start + output.len()))
                    .ok_or(CoreError::ArithmeticOverflow)?;
                let finish = begin
                    .checked_add(unit.end - unit.start)
                    .ok_or(CoreError::ArithmeticOverflow)?;
                next_units.push(
                    Unit {
                        start: begin,
                        end: finish,
                        component: unit.component,
                    },
                    op,
                )?;
            }
        }
        require(next_text.len() == length)?;
        text = next_text;
        units = next_units;
        changed = true;
    }
    require(changed && (atom.kind == AtomKind::Hidden || text.len() != 0))?;
    let expected = pool(ir, proof.value, op)?;
    require(text.len() == expected.len())?;
    for (index, byte) in expected.iter().enumerate() {
        let actual = owned_row(&text, index, op)?;
        op.scan(2)?;
        require(actual == *byte)?;
    }
    let root = find(&mut components, 0, op)?;
    for index in 0..components.len() {
        op.events(1)?;
        require(find(&mut components, index, op)? == root)?;
    }
    for unit_index in 0..units.len() {
        let unit = owned_row(&units, unit_index, op)?;
        op.events(1)?;
        require(find(&mut components, unit.component, op)? == root)?;
    }
    op.check()
}

fn push_unit<'id>(
    text: &mut BuildGrowingVec<'id, u8>,
    units: &mut BuildGrowingVec<'id, Unit>,
    value: &[u8],
    component: usize,
    op: &mut OwnedWork<'id, '_>,
) -> Result<(), CoreError> {
    let start = text.len();
    text.extend_from_slice(value, op)?;
    units.push(
        Unit {
            start,
            end: text.len(),
            component,
        },
        op,
    )
}

fn find<'id>(
    components: &mut BuildGrowingVec<'id, Component>,
    mut index: usize,
    op: &mut OwnedWork<'id, '_>,
) -> Result<usize, CoreError> {
    let mut visits = 0;
    loop {
        op.events(1)?;
        let value = owned_row(components, index, op)?;
        if value.parent == index {
            return Ok(index);
        }
        index = value.parent;
        visits += 1;
        require(visits <= components.len())?;
    }
}

fn unite<'id>(
    components: &mut BuildGrowingVec<'id, Component>,
    left: usize,
    right: usize,
    op: &mut OwnedWork<'id, '_>,
) -> Result<usize, CoreError> {
    let mut left = find(components, left, op)?;
    let mut right = find(components, right, op)?;
    if left == right {
        return Ok(left);
    }
    if owned_row(components, left, op)?.rank < owned_row(components, right, op)?.rank {
        std::mem::swap(&mut left, &mut right);
    }
    let mut a = owned_row(components, left, op)?;
    let mut b = owned_row(components, right, op)?;
    b.parent = left;
    if a.rank == b.rank {
        a.rank = a.rank.checked_add(1).ok_or(CoreError::ArithmeticOverflow)?;
    }
    a.operations |= b.operations;
    a.stages = a
        .stages
        .checked_add(b.stages)
        .ok_or(CoreError::ArithmeticOverflow)?;
    components.replace(left, a, op)?;
    components.replace(right, b, op)?;
    Ok(left)
}

fn validate_order(
    old: u16,
    operation: u16,
    metadata: bool,
    role: HostRole,
) -> Result<(), CoreError> {
    if operation == EMOJI || old & EMOJI != 0 {
        return require(operation == EMOJI && old == 0 && !metadata);
    }
    match operation {
        ENTITY => require(old & (ENTITY | ESCAPE | BREAK | TRIM | CODE_BREAK | CODE_TRIM) == 0),
        ESCAPE => require(
            old & (ESCAPE | BREAK | TRIM | CODE_BREAK | CODE_TRIM) == 0
                && (metadata || old & ENTITY == 0),
        ),
        BLOCK => require(old & (ENTITY | ESCAPE | BREAK | TRIM | CODE_BREAK | CODE_TRIM) == 0),
        BREAK | CODE_BREAK => {
            require(old & (ENTITY | ESCAPE | BREAK | TRIM | CODE_BREAK | CODE_TRIM) == 0)
        }
        CODE_TRIM => require(!metadata && old & (ENTITY | ESCAPE | BREAK | TRIM | CODE_TRIM) == 0),
        TRIM => require(
            old & (ENTITY | ESCAPE | BREAK | TRIM | CODE_BREAK | CODE_TRIM) == 0
                || metadata
                    && role == HostRole::FenceInfo
                    && old & (ESCAPE | BREAK | TRIM | CODE_BREAK | CODE_TRIM) == 0,
        ),
        _ => Err(CoreError::InvalidProvenance),
    }
}

fn validate_stage<'id>(
    stage: StageRow,
    input: &str,
    raw_input: &[u8],
    raw_input_len: usize,
    output: &str,
    context: StageContext,
    op: &mut OwnedWork<'id, '_>,
) -> Result<u16, CoreError> {
    let StageContext {
        claim,
        host,
        payload,
        text_len,
        math_syntax,
    } = context;
    let metadata = matches!(
        host.role,
        HostRole::Destination | HostRole::Title | HostRole::FenceInfo | HostRole::ReferenceLabel
    );
    let inline =
        claim.phase == NativePhase::Inline && host.role == HostRole::Inline && payload.is_none();
    let role_matches = match payload {
        Some(PayloadRole::Destination) => host.role == HostRole::Destination,
        Some(PayloadRole::Title) => host.role == HostRole::Title,
        Some(PayloadRole::FenceInfo) => host.role == HostRole::FenceInfo,
        Some(PayloadRole::ReferenceLabel) => host.role == HostRole::ReferenceLabel,
        Some(PayloadRole::MathInput) => host.role == HostRole::MathInput,
        Some(PayloadRole::FenceInput) => host.role == HostRole::FenceInput,
        None => false,
    };
    let clean = claim.phase == NativePhase::Metadata && metadata && role_matches;
    let normalized_math = payload == Some(PayloadRole::MathInput)
        && math_syntax.is_some_and(|syntax| {
            matches!(syntax, MathSyntax::DollarInline | MathSyntax::CodeInline)
                && claim.kind == ClaimKind::Math(syntax)
        })
        && claim.phase == NativePhase::Inline
        && host.role == HostRole::MathInput
        && claim.owner == host.owner;
    let zero_parameter = stage.parameter == 0;
    let valid = match stage.kind {
        StageKind::NamedEntity => {
            require((inline || clean) && claim.kind == ClaimKind::Entity && zero_parameter)?;
            // The fixed lookup meters the bytes actually compared.
            super::entities::named_metered(input, |bytes| op.scan(bytes))? == Some(output)
        }
        StageKind::NumericEntity => {
            require((inline || clean) && claim.kind == ClaimKind::Entity && zero_parameter)?;
            op.scan(input.len() as u64)?;
            super::entities::numeric(input).is_some_and(|scalar| {
                let mut bytes = [0; 4];
                scalar.encode_utf8(&mut bytes) == output
            })
        }
        StageKind::EmojiShortcode => {
            require(inline && claim.kind == ClaimKind::EmojiShortcode && zero_parameter)?;
            require(
                (3..=MAX_EMOJI_STAGE_BYTES).contains(&input.len()) && raw_input == input.as_bytes(),
            )?;
            op.scan((2 * input.len() + output.len()) as u64)?;
            input
                .strip_prefix(':')
                .and_then(|tail| tail.strip_suffix(':'))
                .filter(|alias| {
                    alias
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'+' | b'-'))
                })
                .and_then(emojis::get_by_shortcode)
                .is_some_and(|emoji| emoji.as_str() == output)
        }
        StageKind::AsciiEscape => {
            require((inline || clean) && claim.kind == ClaimKind::Escape && zero_parameter)?;
            input.len() == 2
                && input.as_bytes()[0] == b'\\'
                && input.as_bytes()[1].is_ascii_punctuation()
                && &input[1..] == output
        }
        StageKind::PartialTab => {
            require(
                claim.phase == NativePhase::Block
                    && host.role == HostRole::Block
                    && claim.kind == ClaimKind::ContainerPrefix,
            )?;
            let count = 4 - stage.parameter % 4;
            input == "\t" && output.len() == count as usize && output.bytes().all(|b| b == b' ')
        }
        StageKind::TableEscapedPipe => {
            require(
                claim.phase == NativePhase::Block
                    && host.role == HostRole::Block
                    && claim.kind == ClaimKind::Table
                    && zero_parameter,
            )?;
            input == "\\|" && output == "|"
        }
        StageKind::SoftBreakToSpace => {
            require(
                inline && claim.kind == ClaimKind::Break && payload.is_none() && zero_parameter,
            )?;
            input == "\n" && output == " "
        }
        StageKind::CodeLineEndingToSpace => {
            require(
                (inline && claim.kind == ClaimKind::Code || normalized_math) && zero_parameter,
            )?;
            input == "\n" && output == " "
        }
        StageKind::CodeEdgeTrim => {
            require(
                (inline && claim.kind == ClaimKind::Code || normalized_math) && zero_parameter,
            )?;
            output.is_empty()
                && input == " "
                && (stage.input.start() == 0 || stage.input.end() as usize == text_len)
        }
        StageKind::HtmlBreakToLineFeed
        | StageKind::HtmlEmphasisDelimiter
        | StageKind::HtmlCodeDelimiter => {
            return Err(CoreError::InvalidProvenance);
        }
        StageKind::AsciiTrim => {
            require(
                clean
                    && host.role != HostRole::Title
                    && claim.kind == ClaimKind::Payload
                    && zero_parameter,
            )?;
            output.is_empty()
                && raw_input_len != 0
                && (stage.input.start() == 0 || stage.input.end() as usize == text_len)
        }
    };
    require(valid && (raw_input_len != output.len() || raw_input != output.as_bytes()))?;
    Ok(match stage.kind {
        StageKind::NamedEntity | StageKind::NumericEntity => ENTITY,
        StageKind::EmojiShortcode => EMOJI,
        StageKind::AsciiEscape => ESCAPE,
        StageKind::PartialTab | StageKind::TableEscapedPipe => BLOCK,
        StageKind::SoftBreakToSpace => BREAK,
        StageKind::CodeLineEndingToSpace => CODE_BREAK,
        StageKind::CodeEdgeTrim => CODE_TRIM,
        StageKind::AsciiTrim => TRIM,
        StageKind::HtmlBreakToLineFeed
        | StageKind::HtmlEmphasisDelimiter
        | StageKind::HtmlCodeDelimiter => {
            return Err(CoreError::InvalidProvenance);
        }
    })
}

pub(super) fn pool<'a>(
    ir: &'a CompactIr,
    value: TextSlice,
    op: &mut OwnedWork<'_, '_>,
) -> Result<&'a [u8], CoreError> {
    ir.text_owned(value, op).map(str::as_bytes)
}

pub(super) fn equal_bytes<'id>(
    left: &[u8],
    right: &[u8],
    op: &mut OwnedWork<'id, '_>,
) -> Result<(), CoreError> {
    require(left.len() == right.len())?;
    for (a, b) in left
        .chunks(CHECKPOINT_BYTES as usize / 2)
        .zip(right.chunks(CHECKPOINT_BYTES as usize / 2))
    {
        op.scan((a.len() + b.len()) as u64)?;
        require(a == b)?;
    }
    Ok(())
}

pub(super) fn covered<'id>(
    span: SourceSpan,
    spans: crate::ir::RowView<'_, SourceSpan>,
    op: &mut OwnedWork<'id, '_>,
) -> Result<bool, CoreError> {
    if span.is_empty() {
        for candidate in spans {
            op.events(1)?;
            if candidate.start() > span.start() {
                return Ok(false);
            }
            if span.start() <= candidate.end() {
                return Ok(true);
            }
        }
        return Ok(false);
    }
    let mut cursor = span.start();
    for candidate in spans {
        op.events(1)?;
        if candidate.start() > cursor {
            return Ok(false);
        }
        if candidate.end() >= cursor {
            cursor = candidate.end().min(span.end());
        }
        if cursor == span.end() {
            return Ok(true);
        }
    }
    Ok(false)
}

// Scalar coordinates and local IDs; no borrowed or writable capability.
impl crate::memory::BuildValue for Unit {}
impl crate::memory::BuildValue for Component {}
