use crate::common_db::LangiumCommonDbFields;
use crate::diagrams::langium_common::{
    LangiumCommonFacts, parse_langium_common, parse_langium_string,
    push_langium_common_editor_fact, push_langium_common_recovery, strip_langium_inline_comment,
};
use crate::{
    EditorExpectedSyntax, EditorExpectedSyntaxKind, EditorSemanticFacts, EditorSemanticKind,
    EditorSemanticSymbol, Error, MermaidConfig, ParseMetadata, Result, SourceSpan,
    editor::{editor_recovery_fallback_span, ensure_editor_recovery_from_error},
    family,
};
use serde_json::{Map, Value, json};

const MAX_PACKET_SIZE: usize = 10_000;
pub const DEFAULT_PACKET_BITS_PER_ROW: i64 = 32;

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("packet.bitsPerRow must be an integer greater than or equal to 1; received {value}")]
pub struct PacketBitsPerRowError {
    pub value: i64,
}

/// Validates the shared Packet row-width configuration before either semantic construction or
/// layout consumes it. Mermaid's configuration schema declares a minimum of one.
pub fn validate_packet_bits_per_row(
    value: Option<i64>,
) -> std::result::Result<i64, PacketBitsPerRowError> {
    let value = value.unwrap_or(DEFAULT_PACKET_BITS_PER_ROW);
    if value < 1 {
        return Err(PacketBitsPerRowError { value });
    }
    Ok(value)
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Default)]
pub struct PacketDiagramRenderModel {
    pub title: Option<String>,
    #[serde(rename = "accTitle")]
    pub acc_title: Option<String>,
    #[serde(rename = "accDescr")]
    pub acc_descr: Option<String>,
    pub packet: Vec<Vec<PacketRenderBlock>>,
    #[serde(skip)]
    compatibility_output: CompatibilityOutputState,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum CompatibilityOutputState {
    Empty,
    #[default]
    Model,
}

impl PacketDiagramRenderModel {
    fn empty_compatibility_output() -> Self {
        Self {
            compatibility_output: CompatibilityOutputState::Empty,
            ..Self::default()
        }
    }

    pub(crate) fn sanitize_common_db_fields(&mut self, config: &crate::MermaidConfig) {
        crate::common_db::sanitize_optional_title(&mut self.title, config);
        crate::common_db::sanitize_optional_acc_title(&mut self.acc_title, config);
        crate::common_db::sanitize_optional_acc_descr(&mut self.acc_descr, config);
    }
}

pub(crate) fn render_model_to_compat_json(
    model: &PacketDiagramRenderModel,
    meta: &ParseMetadata,
) -> Result<Value> {
    if model.compatibility_output == CompatibilityOutputState::Empty {
        return Ok(json!({}));
    }
    let mut out = Map::with_capacity(6);
    out.insert("type".to_string(), Value::String(meta.diagram_type.clone()));
    out.insert("title".to_string(), json!(&model.title));
    out.insert("accTitle".to_string(), json!(&model.acc_title));
    out.insert("accDescr".to_string(), json!(&model.acc_descr));
    out.insert("packet".to_string(), json!(&model.packet));
    out.insert(
        "config".to_string(),
        crate::config::clone_value_nonrecursive(meta.effective_config.as_value()),
    );
    Ok(Value::Object(out))
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PacketRenderBlock {
    pub start: i64,
    pub end: i64,
    pub bits: i64,
    pub label: String,
}

#[derive(Debug, Clone)]
struct PacketBlock {
    start: Option<i64>,
    end: Option<i64>,
    bits: Option<i64>,
    label: String,
    numeric_span: SourceSpan,
}

enum PacketParseOutput {
    Empty,
    Model(PacketDiagramRenderModel),
}

struct PacketSemanticSource {
    output: PacketParseOutput,
    editor_facts: EditorSemanticFacts,
}

type PacketWord = Vec<PacketRenderBlock>;

pub(crate) fn parse_packet(code: &str, meta: &ParseMetadata) -> Result<Value> {
    packet_output_into_json(parse_packet_semantic_source(code, meta)?.output, meta)
}

pub(crate) fn parse_packet_json_and_editor_facts(
    code: &str,
    meta: &ParseMetadata,
    control: &crate::OperationControl,
) -> crate::OperationControlResult<family::CombinedSemanticParse> {
    control.checkpoint()?;
    let parsed = family::CombinedSemanticParse::from_construction(
        construct_packet_semantic_source_controlled(code, meta, control)?,
        |source| {
            (
                packet_output_into_json(source.output, meta),
                source.editor_facts,
            )
        },
        family::CombinedSemanticFailure::into_parts,
    );
    control.checkpoint()?;
    Ok(parsed)
}

fn packet_output_into_json(output: PacketParseOutput, meta: &ParseMetadata) -> Result<Value> {
    match output {
        PacketParseOutput::Empty => Ok(json!({})),
        PacketParseOutput::Model(model) => render_model_to_compat_json(&model, meta),
    }
}

#[cfg(test)]
pub(crate) fn parse_packet_model_for_render(
    code: &str,
    meta: &ParseMetadata,
) -> Result<PacketDiagramRenderModel> {
    match parse_packet_semantic_source(code, meta)?.output {
        PacketParseOutput::Empty => Ok(PacketDiagramRenderModel::empty_compatibility_output()),
        PacketParseOutput::Model(model) => Ok(model),
    }
}

pub(crate) fn parse_packet_model_for_render_controlled(
    code: &str,
    meta: &ParseMetadata,
    control: &crate::OperationControl,
) -> crate::OperationControlResult<Result<PacketDiagramRenderModel>> {
    let construction = construct_packet_semantic_source_controlled(code, meta, control)?;
    let source = match construction {
        Ok(source) => source,
        Err(failure) => return Ok(Err(failure.into_error())),
    };
    let model = match source.output {
        PacketParseOutput::Empty => PacketDiagramRenderModel::empty_compatibility_output(),
        PacketParseOutput::Model(model) => model,
    };
    Ok(Ok(model))
}

fn parse_packet_semantic_source(code: &str, meta: &ParseMetadata) -> Result<PacketSemanticSource> {
    construct_packet_semantic_source(code, meta)
        .map_err(family::CombinedSemanticFailure::into_error)
}

fn construct_packet_semantic_source(
    code: &str,
    meta: &ParseMetadata,
) -> std::result::Result<PacketSemanticSource, family::CombinedSemanticFailure> {
    construct_packet_semantic_source_controlled(code, meta, &crate::OperationControl::new())
        .expect("a private parse control cannot be cancelled")
}

fn construct_packet_semantic_source_controlled(
    code: &str,
    meta: &ParseMetadata,
    control: &crate::OperationControl,
) -> crate::OperationControlResult<
    std::result::Result<PacketSemanticSource, family::CombinedSemanticFailure>,
> {
    control.checkpoint()?;
    #[cfg(test)]
    crate::diagrams::langium_common::record_family_syntax_construction("packet");

    let body = match packet_body_start_controlled(code, control)? {
        Ok(Some(body)) => body,
        Ok(None) => {
            return Ok(Ok(PacketSemanticSource {
                output: PacketParseOutput::Empty,
                editor_facts: EditorSemanticFacts::new(),
            }));
        }
        Err(error) => {
            return Ok(Err(packet_parse_failure(
                error,
                EditorSemanticFacts::new(),
                code,
            )));
        }
    };
    let mut offset = body.offset;
    let mut common = LangiumCommonFacts::default();
    let mut blocks: Vec<PacketBlock> = Vec::new();
    let mut editor_facts = EditorSemanticFacts::new();
    let mut first_error = None;

    while offset < code.len() {
        control.checkpoint()?;
        if let Some(parsed) = parse_langium_common(code, offset) {
            if let Some(diagnostic) = &parsed.diagnostic {
                push_langium_common_recovery(&mut editor_facts, diagnostic);
                first_error.get_or_insert_with(|| {
                    Error::diagram_parse_insertion_point(
                        meta.diagram_type.clone(),
                        diagnostic.message.clone(),
                        diagnostic.span.start,
                    )
                });
            }
            push_langium_common_editor_fact(&mut editor_facts, &parsed.fact, "packet");
            common.push(parsed.fact);
            offset += parsed.consumed;
            continue;
        }

        let line_start = offset;
        let (line, next_offset) = physical_line(code, offset);
        offset = next_offset;
        let visible = strip_inline_comment(line);
        let t = visible.trim();
        if t.is_empty() {
            continue;
        }

        if let Some(block) = parse_packet_block_spanned(line, line_start) {
            push_packet_block_editor_fact(&mut editor_facts, &block);
            blocks.push(block.semantic);
            continue;
        }

        let diagnostic = if line_start < body.header_line_end {
            "expected packet".to_string()
        } else {
            format!("unexpected packet statement: {t}")
        };
        let start = line_start + visible.find(t).unwrap_or_default();
        editor_facts.mark_recovered_from_parse_error(
            diagnostic.clone(),
            Some(SourceSpan::new(start, start + t.len())),
        );
        first_error.get_or_insert_with(|| {
            Error::diagram_parse_fallback(meta.diagram_type.clone(), diagnostic)
        });
    }

    let packet_result =
        match validate_packet_bits_per_row(config_i64(&meta.effective_config, "packet.bitsPerRow"))
        {
            Ok(bits_per_row) => populate_packet_controlled(blocks, bits_per_row, control)?,
            Err(error) => Err(Error::diagram_parse_fallback(
                meta.diagram_type.clone(),
                error.to_string(),
            )),
        };
    let packet = match packet_result {
        Ok(packet) => packet,
        Err(error) => {
            editor_facts = ensure_editor_recovery_from_error(
                editor_facts,
                &error,
                editor_recovery_fallback_span(code),
            );
            if first_error.is_none() {
                first_error = Some(error);
            }
            Vec::new()
        }
    };
    let common = LangiumCommonDbFields::from_facts(&common);

    if let Some(error) = first_error {
        return Ok(Err(family::CombinedSemanticFailure::new(
            error,
            editor_facts,
        )));
    }

    control.checkpoint()?;
    Ok(Ok(PacketSemanticSource {
        output: PacketParseOutput::Model(PacketDiagramRenderModel {
            title: common.title,
            acc_title: common.acc_title,
            acc_descr: common.acc_descr,
            packet,
            compatibility_output: CompatibilityOutputState::Model,
        }),
        editor_facts,
    }))
}

fn packet_parse_failure(
    error: Error,
    editor_facts: EditorSemanticFacts,
    code: &str,
) -> family::CombinedSemanticFailure {
    let editor_facts = ensure_editor_recovery_from_error(
        editor_facts,
        &error,
        editor_recovery_fallback_span(code),
    );
    family::CombinedSemanticFailure::new(error, editor_facts)
}

fn push_packet_block_editor_fact(facts: &mut EditorSemanticFacts, block: &PacketBlockSpanned) {
    facts.push_expected_syntax(EditorExpectedSyntax::new(
        EditorExpectedSyntaxKind::Payload,
        block.semantic.numeric_span,
    ));
    facts.push_symbol(EditorSemanticSymbol::payload(
        block.label.text.to_string(),
        Some("packet block".to_string()),
        EditorSemanticKind::String,
        block.label.span,
        block.label.span,
    ));
}

fn populate_packet_controlled(
    blocks: Vec<PacketBlock>,
    bits_per_row: i64,
    control: &crate::OperationControl,
) -> crate::OperationControlResult<Result<Vec<PacketWord>>> {
    let mut packet: Vec<PacketWord> = Vec::new();
    let mut last_bit: i64 = -1;
    let mut word: PacketWord = Vec::new();
    let mut row: i128 = 1;

    for (block_index, mut block) in blocks.into_iter().enumerate() {
        if block_index % 128 == 0 {
            control.checkpoint()?;
        }
        if let (Some(start), Some(end)) = (block.start, block.end)
            && end < start
        {
            return Ok(Err(Error::diagram_parse_exact(
                "packet".to_string(),
                format!("Packet block {start} - {end} is invalid. End must be greater than start."),
                block.numeric_span,
            )));
        }

        let expected_start = match last_bit.checked_add(1) {
            Some(start) => start,
            None => {
                return Ok(Err(Error::diagram_parse_exact(
                    "packet".to_string(),
                    "Packet bit range exceeds the supported integer range.".to_string(),
                    block.numeric_span,
                )));
            }
        };
        let start = block.start.unwrap_or(expected_start);
        let end_for_msg = block.end.unwrap_or(start);
        if start != expected_start {
            return Ok(Err(Error::diagram_parse_exact(
                "packet".to_string(),
                format!(
                    "Packet block {start} - {end_for_msg} is not contiguous. It should start from {}.",
                    expected_start
                ),
                block.numeric_span,
            )));
        }

        if block.bits == Some(0) {
            return Ok(Err(Error::diagram_parse_exact(
                "packet".to_string(),
                format!("Packet block {start} is invalid. Cannot have a zero bit field."),
                block.numeric_span,
            )));
        }

        let end = match block.end {
            Some(end) => end,
            None => {
                let bits = block.bits.unwrap_or(1);
                let Some(width) = bits.checked_sub(1) else {
                    return Ok(Err(Error::diagram_parse_exact(
                        "packet".to_string(),
                        "Packet block width must be positive.".to_string(),
                        block.numeric_span,
                    )));
                };
                let Some(end) = start.checked_add(width) else {
                    return Ok(Err(Error::diagram_parse_exact(
                        "packet".to_string(),
                        "Packet bit range exceeds the supported integer range.".to_string(),
                        block.numeric_span,
                    )));
                };
                end
            }
        };
        let Some(bits) = end
            .checked_sub(start)
            .and_then(|value| value.checked_add(1))
        else {
            return Ok(Err(Error::diagram_parse_exact(
                "packet".to_string(),
                "Packet bit range arithmetic overflowed.".to_string(),
                block.numeric_span,
            )));
        };
        last_bit = end;

        let mut cur = PacketRenderBlock {
            start,
            end,
            bits,
            label: std::mem::take(&mut block.label),
        };

        let mut split_count = 0usize;
        loop {
            if split_count.is_multiple_of(128) {
                control.checkpoint()?;
            }
            split_count = split_count.saturating_add(1);
            let (fitting, next) = match get_next_fitting_block(cur, row, bits_per_row) {
                Ok(next) => next,
                Err(error) => return Ok(Err(error)),
            };
            let Some(row_boundary) = row.checked_mul(i128::from(bits_per_row)) else {
                return Ok(Err(Error::diagram_parse_exact(
                    "packet".to_string(),
                    "Packet row arithmetic exceeds the supported integer range.".to_string(),
                    block.numeric_span,
                )));
            };
            let reached_row_end = i128::from(fitting.end) + 1 == row_boundary;
            word.push(fitting);
            if reached_row_end {
                if !word.is_empty() {
                    if packet.len() >= MAX_PACKET_SIZE {
                        return Ok(Err(Error::diagram_parse_exact(
                            "packet".to_string(),
                            format!("Packet exceeds the {MAX_PACKET_SIZE}-row safety limit."),
                            block.numeric_span,
                        )));
                    }
                    packet.push(std::mem::take(&mut word));
                }
                let Some(next_row) = row.checked_add(1) else {
                    return Ok(Err(Error::diagram_parse_exact(
                        "packet".to_string(),
                        "Packet row arithmetic exceeds the supported integer range.".to_string(),
                        block.numeric_span,
                    )));
                };
                row = next_row;
            }
            let Some(next) = next else {
                break;
            };
            cur = next;
        }
    }

    if !word.is_empty() {
        packet.push(word);
    }

    control.checkpoint()?;
    Ok(Ok(packet))
}

fn get_next_fitting_block(
    block: PacketRenderBlock,
    row: i128,
    bits_per_row: i64,
) -> Result<(PacketRenderBlock, Option<PacketRenderBlock>)> {
    if block.start > block.end {
        return Err(Error::diagram_parse_fallback(
            "packet".to_string(),
            format!(
                "Block start {} is greater than block end {}.",
                block.start, block.end
            ),
        ));
    }

    let row_start = row.checked_mul(i128::from(bits_per_row)).ok_or_else(|| {
        Error::diagram_parse_fallback(
            "packet".to_string(),
            "Packet row arithmetic exceeds the supported integer range.".to_string(),
        )
    })?;

    if i128::from(block.end) < row_start {
        return Ok((block, None));
    }

    let row_end = row_start.checked_sub(1).ok_or_else(|| {
        Error::diagram_parse_fallback(
            "packet".to_string(),
            "Packet row arithmetic underflowed.".to_string(),
        )
    })?;
    let row_end = i64::try_from(row_end).map_err(|_| {
        Error::diagram_parse_fallback(
            "packet".to_string(),
            "Packet row boundary exceeds the supported integer range.".to_string(),
        )
    })?;
    let row_start = i64::try_from(row_start).map_err(|_| {
        Error::diagram_parse_fallback(
            "packet".to_string(),
            "Packet row boundary exceeds the supported integer range.".to_string(),
        )
    })?;
    let first_bits = row_end
        .checked_sub(block.start)
        .and_then(|value| value.checked_add(1))
        .ok_or_else(|| {
            Error::diagram_parse_fallback(
                "packet".to_string(),
                "Packet block width arithmetic overflowed.".to_string(),
            )
        })?;
    let second_bits = block
        .end
        .checked_sub(row_start)
        .and_then(|value| value.checked_add(1))
        .ok_or_else(|| {
            Error::diagram_parse_fallback(
                "packet".to_string(),
                "Packet block width arithmetic overflowed.".to_string(),
            )
        })?;
    Ok((
        PacketRenderBlock {
            start: block.start,
            end: row_end,
            label: block.label.clone(),
            bits: first_bits,
        },
        Some(PacketRenderBlock {
            start: row_start,
            end: block.end,
            label: block.label,
            bits: second_bits,
        }),
    ))
}

fn strip_inline_comment(line: &str) -> &str {
    strip_langium_inline_comment(line)
}

fn parse_packet_block_spanned(line: &str, line_start: usize) -> Option<PacketBlockSpanned> {
    let stripped = strip_inline_comment(line);
    let trimmed = stripped.trim_start();
    if trimmed.is_empty() {
        return None;
    }
    let leading = stripped.len() - trimmed.len();
    let base = line_start + leading;
    let bytes = trimmed.as_bytes();
    let mut idx = 0usize;

    let (start, end, bits, numeric_span) = if bytes.first() == Some(&b'+') {
        idx = 1;
        while idx < bytes.len() && matches!(bytes[idx], b' ' | b'\t') {
            idx += 1;
        }
        let digits_start = idx;
        while idx < bytes.len() && bytes[idx].is_ascii_digit() {
            idx += 1;
        }
        if idx == digits_start {
            return None;
        }
        let bits = parse_packet_integer(&trimmed[digits_start..idx])?;
        let bits_span = SourceSpan::new(base + digits_start, base + idx);
        (None, None, Some(bits), bits_span)
    } else {
        let digits_start = idx;
        while idx < bytes.len() && bytes[idx].is_ascii_digit() {
            idx += 1;
        }
        if idx == digits_start {
            return None;
        }
        let start = parse_packet_integer(&trimmed[digits_start..idx])?;
        let start_span = SourceSpan::new(base + digits_start, base + idx);
        while idx < bytes.len() && matches!(bytes[idx], b' ' | b'\t') {
            idx += 1;
        }
        if idx < bytes.len() && bytes[idx] == b'-' {
            idx += 1;
            while idx < bytes.len() && matches!(bytes[idx], b' ' | b'\t') {
                idx += 1;
            }
            let end_digits_start = idx;
            while idx < bytes.len() && bytes[idx].is_ascii_digit() {
                idx += 1;
            }
            if idx == end_digits_start {
                return None;
            }
            let end = parse_packet_integer(&trimmed[end_digits_start..idx])?;
            (
                Some(start),
                Some(end),
                None,
                SourceSpan::new(start_span.start, base + idx),
            )
        } else {
            (Some(start), None, None, start_span)
        }
    };

    let mut rest = &trimmed[idx..];
    let rest_trimmed = rest.trim_start();
    let ws1 = rest.len() - rest_trimmed.len();
    rest = rest_trimmed;
    if !rest.starts_with(':') {
        return None;
    }
    let after_colon_base = base + idx + ws1 + 1;
    rest = &rest[1..];
    let rest_trimmed = rest.trim_start();
    let ws2 = rest.len() - rest_trimmed.len();
    rest = rest_trimmed;
    let label_base = after_colon_base + ws2;
    let (label, decoded_label, tail) = parse_quoted_string_spanned(rest, label_base)?;
    if !tail.trim().is_empty() {
        return None;
    }

    Some(PacketBlockSpanned {
        semantic: PacketBlock {
            start,
            end,
            bits,
            label: decoded_label,
            numeric_span,
        },
        label,
    })
}

fn parse_packet_integer(token: &str) -> Option<i64> {
    if token.len() > 1 && token.starts_with('0') {
        return None;
    }
    token.parse().ok()
}

fn parse_quoted_string_spanned(
    input: &str,
    base_offset: usize,
) -> Option<(SpannedText, String, &str)> {
    let parsed = parse_langium_string(input, base_offset)?;
    let tail = &input[parsed.consumed..];
    Some((
        SpannedText {
            text: parsed.value.clone(),
            span: parsed.value_span,
        },
        parsed.value,
        tail,
    ))
}

fn packet_header_token_len(line: &str) -> Option<usize> {
    if let Some(rest) = line.strip_prefix("packet-beta")
        && keyword_boundary(rest)
    {
        return Some("packet-beta".len());
    }
    if let Some(rest) = line.strip_prefix("packet")
        && keyword_boundary(rest)
    {
        return Some("packet".len());
    }
    None
}

fn keyword_boundary(rest: &str) -> bool {
    rest.is_empty()
        || rest.starts_with("%%")
        || rest.chars().next().is_some_and(char::is_whitespace)
}

#[derive(Debug, Clone, Copy)]
struct PacketBodyStart {
    offset: usize,
    header_line_end: usize,
}

fn packet_body_start_controlled(
    code: &str,
    control: &crate::OperationControl,
) -> crate::OperationControlResult<Result<Option<PacketBodyStart>>> {
    let mut offset = 0usize;
    while offset < code.len() {
        control.checkpoint()?;
        let (line, next_offset) = physical_line(code, offset);
        let visible = strip_inline_comment(line);
        let trimmed = visible.trim_start();
        if trimmed.trim().is_empty() {
            offset = next_offset;
            continue;
        }

        let leading = visible.len() - trimmed.len();
        let Some(header_len) = packet_header_token_len(trimmed) else {
            return Ok(Err(Error::diagram_parse_fallback(
                "packet".to_string(),
                "expected packet".to_string(),
            )));
        };
        let body_start = offset + leading + header_len;
        return Ok(Ok(Some(PacketBodyStart {
            offset: body_start,
            header_line_end: next_offset,
        })));
    }
    Ok(Ok(None))
}

fn physical_line(source: &str, offset: usize) -> (&str, usize) {
    let rest = &source[offset..];
    if let Some(newline) = rest.find('\n') {
        let line = rest[..newline]
            .strip_suffix('\r')
            .unwrap_or(&rest[..newline]);
        (line, offset + newline + 1)
    } else {
        (rest, source.len())
    }
}

fn config_i64(config: &MermaidConfig, dotted_path: &str) -> Option<i64> {
    let mut cur = config.as_value();
    for segment in dotted_path.split('.') {
        cur = cur.as_object()?.get(segment)?;
    }
    match cur {
        Value::Number(n) => n
            .as_i64()
            .or_else(|| n.as_u64().and_then(|v| i64::try_from(v).ok())),
        _ => None,
    }
}

#[derive(Debug, Clone)]
struct SpannedText {
    text: String,
    span: SourceSpan,
}

#[derive(Debug, Clone)]
struct PacketBlockSpanned {
    semantic: PacketBlock,
    label: SpannedText,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Engine, ParseOptions};
    use futures::executor::block_on;
    use serde_json::json;

    fn parse(text: &str) -> crate::ManagedSemanticJson {
        let engine = Engine::new();
        block_on(engine.parse_diagram(text, ParseOptions::default()))
            .unwrap()
            .unwrap()
            .model
    }

    fn parse_err(text: &str) -> String {
        let engine = Engine::new();
        match block_on(engine.parse_diagram(text, ParseOptions::default())).unwrap_err() {
            Error::DiagramParse { diagnostic, .. } => diagnostic.message().to_string(),
            other => other.to_string(),
        }
    }

    #[test]
    fn packet_parser_can_cancel_between_blocks() {
        let mut text = String::from("packet\n");
        for index in 0..512 {
            text.push_str(&format!("{index}: \"bit-{index}\"\n"));
        }
        let control = crate::OperationControl::new();
        control.cancel_after_checkpoints(4);
        let meta = ParseMetadata {
            diagram_type: "packet".to_string(),
            config: MermaidConfig::empty_object(),
            effective_config: MermaidConfig::empty_object(),
            title: None,
        };

        assert!(matches!(
            construct_packet_semantic_source_controlled(&text, &meta, &control),
            Err(crate::OperationCancelled { .. })
        ));
    }

    #[test]
    fn packet_typed_projection_matches_complete_compatibility_json() {
        let text = "packet\ntitle Header\n0-7: \"byte\"\n";
        let effective_config = MermaidConfig::from_value(json!({
            "packet": { "bitsPerRow": 16 },
            "theme": "forest"
        }));
        let meta = ParseMetadata {
            diagram_type: "packet".to_string(),
            config: MermaidConfig::empty_object(),
            effective_config,
            title: None,
        };
        let compat = parse_packet(text, &meta).unwrap();
        let typed = parse_packet_model_for_render(text, &meta).unwrap();

        assert_eq!(render_model_to_compat_json(&typed, &meta).unwrap(), compat);
        assert_eq!(compat["config"]["packet"]["bitsPerRow"], 16);
        assert!(compat["accTitle"].is_null());
    }

    #[test]
    fn packet_bit_order_preserves_typed_model_and_semantic_ranges() {
        let source = "packet\n+12: \"wide\"\n+1: \"flag\"\n";
        let mut packets = Vec::new();
        for bit_order in ["ascending", "descending"] {
            let meta = ParseMetadata {
                diagram_type: "packet".to_string(),
                config: MermaidConfig::empty_object(),
                effective_config: MermaidConfig::from_value(json!({
                    "packet": { "bitsPerRow": 8, "bitOrder": bit_order }
                })),
                title: None,
            };
            let typed = parse_packet_model_for_render(source, &meta).unwrap();
            let compat = render_model_to_compat_json(&typed, &meta).unwrap();
            assert_eq!(compat, parse_packet(source, &meta).unwrap());
            assert_eq!(compat["config"]["packet"]["bitOrder"], bit_order);
            packets.push(compat["packet"].clone());
            assert!(parse_packet("packet\n7-0: \"reversed\"", &meta).is_err());
        }
        assert_eq!(packets[0], packets[1]);
        assert_eq!(packets[1][1][0]["start"], 8);
        assert_eq!(packets[1][1][0]["end"], 11);
        assert_eq!(packets[1][1][1]["start"], 12);
    }

    #[test]
    fn packet_typed_projection_preserves_empty_and_header_only_output_states() {
        let meta = ParseMetadata {
            diagram_type: "packet".to_string(),
            config: MermaidConfig::empty_object(),
            effective_config: MermaidConfig::empty_object(),
            title: None,
        };
        for source in ["", "packet"] {
            let compat = parse_packet(source, &meta).unwrap();
            let typed = parse_packet_model_for_render(source, &meta).unwrap();

            assert_eq!(
                render_model_to_compat_json(&typed, &meta).unwrap(),
                compat,
                "projection drift for {source:?}"
            );
        }
    }

    #[test]
    fn packet_beta_header_is_accepted() {
        let model = parse("packet-beta");
        assert_eq!(model["packet"], json!([]));
    }

    #[test]
    fn packet_header_is_accepted() {
        let model = parse("packet");
        assert_eq!(model["packet"], json!([]));
    }

    #[test]
    fn packet_rejects_a_nonpositive_bits_per_row_before_expanding_rows() {
        let error = parse_err(
            r#"---
config:
  packet:
    bitsPerRow: 0
---
packet
0-7: "byte"
"#,
        );

        assert_eq!(
            error,
            "packet.bitsPerRow must be an integer greater than or equal to 1; received 0"
        );
    }

    #[test]
    fn packet_rejects_extreme_ranges_without_integer_overflow() {
        let error = parse_err(
            r#"packet
0-9223372036854775807: "too wide"
"#,
        );

        assert_eq!(error, "Packet bit range arithmetic overflowed.");
    }

    #[test]
    fn packet_header_does_not_accept_trailing_text() {
        assert_eq!(parse_err("packet diagrams"), "expected packet");
    }

    #[test]
    fn packet_data_and_title_are_parsed() {
        let model = parse(
            r#"packet
title Packet diagram
accTitle: Packet accTitle
accDescr: Packet accDescription
0-10: "test"
"#,
        );
        assert_eq!(model["title"], json!("Packet diagram"));
        assert_eq!(model["accTitle"], json!("Packet accTitle"));
        assert_eq!(model["accDescr"], json!("Packet accDescription"));
        assert_eq!(
            model["packet"],
            json!([
              [
                {
                  "bits": 11,
                  "end": 10,
                  "label": "test",
                  "start": 0
                }
              ]
            ])
        );
    }

    #[test]
    fn packet_multiline_accessibility_description_matches_common_langium() {
        let model = parse(
            "packet\r\naccDescr {\r\n  First   line\r\n\r\n\tSecond line  \r\n}\r\n0-7: \"byte\"\r\n",
        );

        assert_eq!(model["accDescr"], json!("First line\nSecond line"));
        assert_eq!(model["packet"][0][0]["label"], json!("byte"));
    }

    #[test]
    fn packet_single_bits_are_supported() {
        let model = parse(
            r#"packet
0-10: "test"
11: "single"
"#,
        );
        assert_eq!(
            model["packet"],
            json!([
              [
                {
                  "bits": 11,
                  "end": 10,
                  "label": "test",
                  "start": 0
                },
                {
                  "bits": 1,
                  "end": 11,
                  "label": "single",
                  "start": 11
                }
              ]
            ])
        );
    }

    #[test]
    fn packet_uses_langium_string_escapes_and_quote_aware_inline_comments() {
        let source = r#"packet-beta
0-7: "A\n100%% complete" %% outside comment
8-15: 'B\tlabel'
"#;
        let model = parse(source);

        assert_eq!(model["packet"][0][0]["label"], "An100%% complete");
        assert_eq!(model["packet"][0][1]["label"], "Btlabel");

        let facts = Engine::new()
            .parse_editor_semantic_facts_with_type_sync("packet", source)
            .unwrap()
            .unwrap();
        let escaped = "A\\n100%% complete";
        let escaped_start = source.find(escaped).unwrap();
        assert!(facts.symbols.iter().any(|symbol| {
            symbol.name == "An100%% complete"
                && symbol.selection == SourceSpan::new(escaped_start, escaped_start + escaped.len())
        }));
    }

    #[test]
    fn packet_bit_counts_are_supported() {
        let model = parse(
            r#"packet
+8: "byte"
+16: "word"
"#,
        );
        assert_eq!(
            model["packet"],
            json!([
              [
                {
                  "bits": 8,
                  "end": 7,
                  "label": "byte",
                  "start": 0
                },
                {
                  "bits": 16,
                  "end": 23,
                  "label": "word",
                  "start": 8
                }
              ]
            ])
        );
    }

    #[test]
    fn packet_editor_facts_expose_parser_backed_spans() {
        let engine = crate::Engine::new();
        let text = r#"packet
title Packet diagram
accTitle: Packet accTitle
accDescr: Packet accDescription
0-10: "test"
11: "single"
"#;
        let facts = engine
            .parse_editor_semantic_facts_with_type_sync("packet", text)
            .unwrap()
            .unwrap();

        assert!(facts.directive_prefixes.iter().any(|p| p == "title"));
        assert!(facts.directive_prefixes.iter().any(|p| p == "accTitle"));
        assert!(facts.directive_prefixes.iter().any(|p| p == "accDescr"));
        assert!(
            facts
                .symbols
                .iter()
                .any(|symbol| symbol.name == "test" && symbol.kind == EditorSemanticKind::String)
        );
        assert!(
            facts
                .symbols
                .iter()
                .any(|symbol| symbol.name == "single" && symbol.kind == EditorSemanticKind::String)
        );

        let start = text.find("0-10").unwrap();
        let single_start = text.find("11").unwrap();
        assert!(facts.expected_syntax.iter().any(|expected| {
            expected.kind == EditorExpectedSyntaxKind::Payload
                && expected.span == SourceSpan::new(start, start + "0-10".len())
        }));
        assert!(facts.expected_syntax.iter().any(|expected| {
            expected.kind == EditorExpectedSyntaxKind::Payload
                && expected.span == SourceSpan::new(single_start, single_start + "11".len())
        }));
    }

    #[test]
    fn packet_editor_recovery_reports_invalid_statement_with_exact_span() {
        let engine = crate::Engine::new();
        let text = concat!(
            "packet\n",
            "0-7: \"valid\"\n",
            "  invalid packet statement  %% hidden\n",
            "8-15: \"next\"\n",
        );
        let facts = engine
            .parse_editor_semantic_facts_with_type_sync("packet", text)
            .unwrap()
            .unwrap();

        assert_eq!(
            facts.completeness,
            crate::EditorSemanticCompleteness::Recovered
        );
        assert!(facts.symbols.iter().any(|symbol| symbol.name == "valid"));
        assert!(facts.symbols.iter().any(|symbol| symbol.name == "next"));

        let invalid = "invalid packet statement";
        let start = text.find(invalid).unwrap();
        assert!(facts.diagnostics.iter().any(|diagnostic| {
            diagnostic.message == format!("unexpected packet statement: {invalid}")
                && diagnostic.span == Some(SourceSpan::new(start, start + invalid.len()))
                && diagnostic.kind == crate::EditorSemanticDiagnosticKind::ParserRecovery
        }));
    }

    #[test]
    fn packet_editor_recovery_reports_post_parse_validation_errors() {
        let text = concat!(
            "packet\r\n",
            "0-7: \"valid\"\r\n",
            "  9-15: \"not contiguous\"  \r\n",
        );
        let invalid = "9-15";
        let start = text.find(invalid).unwrap();
        let expected_span = SourceSpan::new(start, start + invalid.len());
        let engine = Engine::new();

        let error = engine
            .parse_diagram_sync(text, ParseOptions::strict())
            .expect_err("a non-contiguous packet block must fail strict parsing");
        let Error::DiagramParse { diagnostic, .. } = error else {
            panic!("expected packet parse diagnostic");
        };
        assert_eq!(diagnostic.span(), Some(expected_span));

        let facts = engine
            .parse_editor_semantic_facts_with_type_sync("packet", text)
            .unwrap()
            .expect("packet editor recovery facts");
        assert_eq!(
            facts.completeness,
            crate::EditorSemanticCompleteness::Recovered
        );
        assert!(facts.symbols.iter().any(|symbol| symbol.name == "valid"));
        assert!(
            facts
                .symbols
                .iter()
                .any(|symbol| symbol.name == "not contiguous")
        );
        assert!(facts.diagnostics.iter().any(|diagnostic| {
            diagnostic.kind == crate::EditorSemanticDiagnosticKind::ParserRecovery
                && diagnostic.span == Some(expected_span)
                && diagnostic.message.contains("is not contiguous")
        }));
    }

    #[test]
    fn packet_splits_into_multiple_rows() {
        let model = parse(
            r#"packet
0-10: "test"
11-90: "multiple"
"#,
        );
        assert_eq!(
            model["packet"],
            json!([
              [
                {
                  "bits": 11,
                  "end": 10,
                  "label": "test",
                  "start": 0
                },
                {
                  "bits": 21,
                  "end": 31,
                  "label": "multiple",
                  "start": 11
                }
              ],
              [
                {
                  "bits": 32,
                  "end": 63,
                  "label": "multiple",
                  "start": 32
                }
              ],
              [
                {
                  "bits": 27,
                  "end": 90,
                  "label": "multiple",
                  "start": 64
                }
              ]
            ])
        );
    }

    #[test]
    fn packet_splits_into_multiple_rows_at_exact_length() {
        let model = parse(
            r#"packet
0-16: "test"
17-63: "multiple"
"#,
        );
        assert_eq!(
            model["packet"],
            json!([
              [
                {
                  "bits": 17,
                  "end": 16,
                  "label": "test",
                  "start": 0
                },
                {
                  "bits": 15,
                  "end": 31,
                  "label": "multiple",
                  "start": 17
                }
              ],
              [
                {
                  "bits": 32,
                  "end": 63,
                  "label": "multiple",
                  "start": 32
                }
              ]
            ])
        );
    }

    #[test]
    fn packet_errors_if_numbers_are_not_continuous() {
        let err = parse_err(
            r#"packet
0-16: "test"
18-20: "error"
"#,
        );
        assert_eq!(
            err,
            "Packet block 18 - 20 is not contiguous. It should start from 17."
        );
    }

    #[test]
    fn packet_errors_if_numbers_are_not_continuous_with_bit_counts() {
        let err = parse_err(
            r#"packet
+16: "test"
18-20: "error"
"#,
        );
        assert_eq!(
            err,
            "Packet block 18 - 20 is not contiguous. It should start from 16."
        );
    }

    #[test]
    fn packet_errors_if_single_number_is_not_continuous() {
        let err = parse_err(
            r#"packet
0-16: "test"
18: "error"
"#,
        );
        assert_eq!(
            err,
            "Packet block 18 - 18 is not contiguous. It should start from 17."
        );
    }

    #[test]
    fn packet_errors_if_single_number_is_not_continuous_with_bit_counts() {
        let err = parse_err(
            r#"packet
+16: "test"
18: "error"
"#,
        );
        assert_eq!(
            err,
            "Packet block 18 - 18 is not contiguous. It should start from 16."
        );
    }

    #[test]
    fn packet_errors_if_single_number_is_not_continuous_2() {
        let err = parse_err(
            r#"packet
0-16: "test"
17: "good"
19: "error"
"#,
        );
        assert_eq!(
            err,
            "Packet block 19 - 19 is not contiguous. It should start from 18."
        );
    }

    #[test]
    fn packet_errors_if_end_is_less_than_start() {
        let err = parse_err(
            r#"packet
0-16: "test"
25-20: "error"
"#,
        );
        assert_eq!(
            err,
            "Packet block 25 - 20 is invalid. End must be greater than start."
        );
    }

    #[test]
    fn packet_errors_if_bit_count_is_zero() {
        let err = parse_err(
            r#"packet
+0: "test"
"#,
        );
        assert_eq!(
            err,
            "Packet block 0 is invalid. Cannot have a zero bit field."
        );
    }
}
