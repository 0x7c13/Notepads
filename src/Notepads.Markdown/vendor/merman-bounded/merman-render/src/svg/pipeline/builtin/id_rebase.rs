use crate::svg::pipeline::{
    SvgPostprocessContext, SvgPostprocessExecution, SvgPostprocessor, is_css_value_attribute,
    is_svg_idref_attribute, validate_well_formed_svg_with_controls,
};
use crate::{Error, Result};
use cssparser::{
    BasicParseErrorKind, CssStringWriter, Parser, SourcePosition, Token, serialize_identifier,
    serialize_name,
};
use quick_xml::XmlVersion;
use quick_xml::events::{BytesRef, BytesStart, BytesText, Event};
use quick_xml::reader::Reader;
use quick_xml::writer::Writer;
use std::borrow::Cow;
use std::collections::BTreeSet;
use std::fmt;
use std::io::Write as IoWrite;

#[derive(Debug, Clone)]
pub(crate) struct RebaseSvgIdsPostprocessor {
    prefix: String,
}

impl RebaseSvgIdsPostprocessor {
    pub(crate) fn new(prefix: impl Into<String>) -> Self {
        Self {
            prefix: super::super::super::sanitize_svg_id(&prefix.into()),
        }
    }
}

impl SvgPostprocessor for RebaseSvgIdsPostprocessor {
    fn name(&self) -> &'static str {
        "rebase-svg-ids"
    }

    fn process<'a>(
        &self,
        svg: Cow<'a, str>,
        ctx: &SvgPostprocessContext<'_>,
    ) -> Result<Cow<'a, str>> {
        let execution = ctx.execution();
        execution.checkpoint()?;
        execution.preflight_svg_byte_count(svg.len())?;
        validate_well_formed_svg_with_controls(
            &svg,
            &mut || execution.checkpoint(),
            &mut |elements, tree_depth| execution.preflight_svg_structure(elements, tree_depth),
        )?;
        let mut cadence = RebaseCadence::new(execution);
        let ids = collect_ids(&svg, &mut cadence)?;
        if ids.is_empty() {
            return Ok(svg);
        }

        let projected_bytes = projected_rebased_xml_bytes(&svg, &ids, &self.prefix, &mut cadence)?;
        execution.checkpoint()?;
        let Some(projected_bytes) = projected_bytes else {
            return Err(execution.svg_byte_count_overflow());
        };
        execution.preflight_svg_byte_count(projected_bytes)?;
        execution.checkpoint()?;

        rebase_xml(&svg, &ids, &self.prefix, projected_bytes, &mut cadence).map(Cow::Owned)
    }
}

const REBASE_CHECKPOINT_BATCH: usize = 64;
const REBASE_PREFIX_CHECKPOINT_BYTES: usize = 4096;

struct RebaseCadence<'a> {
    execution: SvgPostprocessExecution<'a>,
    iterations: usize,
}

impl<'a> RebaseCadence<'a> {
    const fn new(execution: SvgPostprocessExecution<'a>) -> Self {
        Self {
            execution,
            iterations: 0,
        }
    }

    fn checkpoint(&self) -> Result<()> {
        self.execution.checkpoint()
    }

    fn tick(&mut self) -> Result<()> {
        if self.iterations.is_multiple_of(REBASE_CHECKPOINT_BATCH) {
            self.checkpoint()?;
        }
        self.iterations = self.iterations.wrapping_add(1);
        Ok(())
    }
}

#[derive(Debug)]
struct CollectedIds {
    ids: BTreeSet<String>,
}

impl CollectedIds {
    fn is_empty(&self) -> bool {
        self.ids.is_empty()
    }

    fn contains_id(&self, id: &str) -> bool {
        self.ids.contains(id)
    }
}

fn collect_ids(svg: &str, cadence: &mut RebaseCadence<'_>) -> Result<CollectedIds> {
    let mut reader = Reader::from_str(svg);
    reader.config_mut().check_end_names = true;
    let mut ids = BTreeSet::new();

    loop {
        cadence.tick()?;
        let event = reader.read_event();
        cadence.checkpoint()?;
        let event = event.map_err(|error| rebase_error(format!("invalid SVG XML: {error}")))?;
        match event {
            Event::Start(start) | Event::Empty(start) => {
                collect_start_id(&start, &mut ids, cadence)?;
            }
            Event::Eof => break,
            _ => {}
        }
    }

    cadence.checkpoint()?;
    Ok(CollectedIds { ids })
}

fn collect_start_id(
    start: &BytesStart<'_>,
    ids: &mut BTreeSet<String>,
    cadence: &mut RebaseCadence<'_>,
) -> Result<()> {
    for attribute in start.attributes().with_checks(true) {
        cadence.tick()?;
        let attribute =
            attribute.map_err(|error| rebase_error(format!("invalid SVG attribute: {error}")))?;
        if attribute.key.as_ref() != "id" {
            continue;
        }
        let id = attribute
            .normalized_value(XmlVersion::Implicit1_0)
            .map_err(|error| rebase_error(format!("invalid SVG id: {error}")))?;
        if id.is_empty() {
            return Err(rebase_error("SVG id must not be empty"));
        }
        let id = id.into_owned();
        if ids.contains(id.as_str()) {
            return Err(rebase_error(format!("duplicate SVG id {id:?}")));
        }
        ids.insert(id);
    }
    Ok(())
}

#[derive(Default)]
struct ProjectedByteCounter {
    bytes: usize,
    overflowed: bool,
}

impl ProjectedByteCounter {
    fn add(&mut self, bytes: usize) {
        if !self.overflowed {
            match self.bytes.checked_add(bytes) {
                Some(bytes) => self.bytes = bytes,
                None => self.overflowed = true,
            }
        }
    }

    fn projected_bytes(self) -> Option<usize> {
        (!self.overflowed).then_some(self.bytes)
    }
}

impl IoWrite for ProjectedByteCounter {
    fn write(&mut self, buffer: &[u8]) -> std::io::Result<usize> {
        self.add(buffer.len());
        Ok(buffer.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

struct ProjectedTextCounter<'a> {
    bytes: &'a mut ProjectedByteCounter,
    attribute: bool,
}

impl<'a> ProjectedTextCounter<'a> {
    fn xml_text(bytes: &'a mut ProjectedByteCounter) -> Self {
        Self {
            bytes,
            attribute: false,
        }
    }

    fn xml_attribute(bytes: &'a mut ProjectedByteCounter) -> Self {
        Self {
            bytes,
            attribute: true,
        }
    }
}

impl fmt::Write for ProjectedTextCounter<'_> {
    fn write_str(&mut self, value: &str) -> fmt::Result {
        let mut plain_start = 0usize;
        for (index, byte) in value.bytes().enumerate() {
            let escaped_bytes = match byte {
                b'<' | b'>' => 4,
                b'&' | b'\r' => 5,
                b'\n' if self.attribute => 5,
                b'\t' if self.attribute => 4,
                b'\'' | b'"' => 6,
                _ => continue,
            };
            self.bytes.add(index - plain_start);
            self.bytes.add(escaped_bytes);
            plain_start = index + 1;
        }
        self.bytes.add(value.len() - plain_start);
        Ok(())
    }
}

#[derive(Default)]
struct LogicalStyleText {
    css: String,
}

impl LogicalStyleText {
    fn push_text(&mut self, text: &BytesText<'_>, cadence: &mut RebaseCadence<'_>) -> Result<()> {
        let text = text.xml10_content();
        self.push_str(&text, cadence)
    }

    fn push_cdata(
        &mut self,
        text: &quick_xml::events::BytesCData<'_>,
        cadence: &mut RebaseCadence<'_>,
    ) -> Result<()> {
        let text = text.xml10_content();
        self.push_str(&text, cadence)
    }

    fn push_reference(
        &mut self,
        reference: &BytesRef<'_>,
        cadence: &mut RebaseCadence<'_>,
    ) -> Result<()> {
        cadence.checkpoint()?;
        self.css.push(resolve_style_reference(reference)?);
        cadence.checkpoint()
    }

    fn push_str(&mut self, value: &str, cadence: &mut RebaseCadence<'_>) -> Result<()> {
        cadence.checkpoint()?;
        self.css.try_reserve(value.len()).map_err(|error| {
            rebase_error(format!("failed to allocate logical style text: {error}"))
        })?;
        self.css.push_str(value);
        cadence.checkpoint()
    }

    fn rewrite_to<W: fmt::Write>(
        &self,
        ids: &CollectedIds,
        prefix: &str,
        output: &mut W,
        cadence: &mut RebaseCadence<'_>,
    ) -> Result<()> {
        rewrite_stylesheet_to(&self.css, ids, prefix, output, cadence)
    }
}

fn resolve_style_reference(reference: &BytesRef<'_>) -> Result<char> {
    if let Some(value) = reference
        .resolve_char_ref()
        .map_err(|error| rebase_error(format!("invalid style character reference: {error}")))?
    {
        return Ok(value);
    }

    let name = reference.as_ref();
    match name {
        "amp" => Ok('&'),
        "apos" => Ok('\''),
        "gt" => Ok('>'),
        "lt" => Ok('<'),
        "quot" => Ok('"'),
        _ => Err(rebase_error(format!(
            "invalid style entity reference: unknown entity &{name};"
        ))),
    }
}

fn nested_style_xml_error() -> Error {
    rebase_error("a <style> element contains nested XML elements")
}

fn projected_rebased_xml_bytes(
    svg: &str,
    ids: &CollectedIds,
    prefix: &str,
    cadence: &mut RebaseCadence<'_>,
) -> Result<Option<usize>> {
    let mut writer = Writer::new(ProjectedByteCounter::default());
    let mut reader = Reader::from_str(svg);
    reader.config_mut().check_end_names = true;
    let mut style_text = None::<LogicalStyleText>;

    loop {
        cadence.tick()?;
        let event = reader.read_event();
        cadence.checkpoint()?;
        let event = event.map_err(|error| rebase_error(format!("invalid SVG XML: {error}")))?;
        match event {
            Event::Start(start) => {
                if style_text.is_some() {
                    return Err(nested_style_xml_error());
                }
                let is_style = start.local_name().as_ref().eq_ignore_ascii_case("style");
                project_start(&start, ids, prefix, writer.get_mut(), false, cadence)?;
                if is_style {
                    style_text = Some(LogicalStyleText::default());
                }
            }
            Event::Empty(start) => {
                if style_text.is_some() {
                    return Err(nested_style_xml_error());
                }
                project_start(&start, ids, prefix, writer.get_mut(), true, cadence)?;
            }
            Event::End(end) => {
                if let Some(style) = style_text.take() {
                    if !end.local_name().as_ref().eq_ignore_ascii_case("style") {
                        return Err(nested_style_xml_error());
                    }
                    let mut output = ProjectedTextCounter::xml_text(writer.get_mut());
                    style.rewrite_to(ids, prefix, &mut output, cadence)?;
                }
                writer.write_event(Event::End(end)).map_err(write_error)?;
            }
            Event::Text(text) if style_text.is_some() => {
                style_text
                    .as_mut()
                    .expect("guarded above")
                    .push_text(&text, cadence)?;
            }
            Event::CData(text) if style_text.is_some() => {
                style_text
                    .as_mut()
                    .expect("guarded above")
                    .push_cdata(&text, cadence)?;
            }
            Event::GeneralRef(reference) if style_text.is_some() => {
                style_text
                    .as_mut()
                    .expect("guarded above")
                    .push_reference(&reference, cadence)?;
            }
            Event::Comment(_) if style_text.is_some() => {
                // XML comments are not part of the effective CSS text. Waiting until </style>
                // lets selectors and url() tokens span Text/CDATA/comment boundaries exactly as
                // they do in the browser.
            }
            Event::Eof => break,
            other => writer.write_event(other).map_err(write_error)?,
        }
    }
    Ok(writer.into_inner().projected_bytes())
}

fn project_start(
    start: &BytesStart<'_>,
    ids: &CollectedIds,
    prefix: &str,
    output: &mut ProjectedByteCounter,
    empty: bool,
    cadence: &mut RebaseCadence<'_>,
) -> Result<()> {
    let start_name = start.name();
    let name = start_name.as_ref();
    output.add(1);
    output.add(name.len());
    for attribute in start.attributes().with_checks(true) {
        cadence.tick()?;
        let attribute =
            attribute.map_err(|error| rebase_error(format!("invalid SVG attribute: {error}")))?;
        let key = attribute.key.as_ref();
        let value = attribute
            .normalized_value(XmlVersion::Implicit1_0)
            .map_err(|error| rebase_error(format!("invalid attribute value: {error}")))?;
        output.add(1 + key.len() + 2);
        let mut escaped = ProjectedTextCounter::xml_attribute(output);
        rewrite_attribute_to(key, &value, ids, prefix, &mut escaped, cadence)?;
        output.add(1);
    }
    output.add(if empty { 2 } else { 1 });
    Ok(())
}

fn rebase_xml(
    svg: &str,
    ids: &CollectedIds,
    prefix: &str,
    projected_bytes: usize,
    cadence: &mut RebaseCadence<'_>,
) -> Result<String> {
    let mut output = Vec::new();
    output
        .try_reserve_exact(projected_bytes)
        .map_err(|error| rebase_error(format!("failed to allocate rebased SVG: {error}")))?;
    let mut writer = Writer::new(output);
    write_rebased_xml(svg, ids, prefix, &mut writer, cadence)?;
    cadence.checkpoint()?;
    let output = writer.into_inner();
    if output.len() != projected_bytes {
        return Err(rebase_error(
            "rebased SVG byte projection changed during materialization",
        ));
    }
    String::from_utf8(output)
        .map_err(|error| rebase_error(format!("rebased SVG is not UTF-8: {error}")))
}

fn write_rebased_xml<W: IoWrite>(
    svg: &str,
    ids: &CollectedIds,
    prefix: &str,
    writer: &mut Writer<W>,
    cadence: &mut RebaseCadence<'_>,
) -> Result<()> {
    let mut reader = Reader::from_str(svg);
    reader.config_mut().check_end_names = true;
    let mut style_text = None::<LogicalStyleText>;

    loop {
        cadence.tick()?;
        let event = reader.read_event();
        cadence.checkpoint()?;
        let event = event.map_err(|error| rebase_error(format!("invalid SVG XML: {error}")))?;
        match event {
            Event::Start(start) => {
                if style_text.is_some() {
                    return Err(nested_style_xml_error());
                }
                let is_style = start.local_name().as_ref().eq_ignore_ascii_case("style");
                let rewritten = rewrite_start(start, ids, prefix, cadence)?;
                writer
                    .write_event(Event::Start(rewritten))
                    .map_err(write_error)?;
                cadence.checkpoint()?;
                if is_style {
                    style_text = Some(LogicalStyleText::default());
                }
            }
            Event::Empty(start) => {
                if style_text.is_some() {
                    return Err(nested_style_xml_error());
                }
                let rewritten = rewrite_start(start, ids, prefix, cadence)?;
                writer
                    .write_event(Event::Empty(rewritten))
                    .map_err(write_error)?;
                cadence.checkpoint()?;
            }
            Event::End(end) => {
                if let Some(style) = style_text.take() {
                    if !end.local_name().as_ref().eq_ignore_ascii_case("style") {
                        return Err(nested_style_xml_error());
                    }
                    let mut css = String::new();
                    style.rewrite_to(ids, prefix, &mut css, cadence)?;
                    writer
                        .write_event(Event::Text(BytesText::new(&css)))
                        .map_err(write_error)?;
                    cadence.checkpoint()?;
                }
                writer.write_event(Event::End(end)).map_err(write_error)?;
            }
            Event::Text(text) if style_text.is_some() => {
                style_text
                    .as_mut()
                    .expect("guarded above")
                    .push_text(&text, cadence)?;
            }
            Event::CData(text) if style_text.is_some() => {
                style_text
                    .as_mut()
                    .expect("guarded above")
                    .push_cdata(&text, cadence)?;
            }
            Event::GeneralRef(reference) if style_text.is_some() => {
                style_text
                    .as_mut()
                    .expect("guarded above")
                    .push_reference(&reference, cadence)?;
            }
            Event::Comment(_) if style_text.is_some() => {
                // See the projection pass: comments do not contribute to style text.
            }
            Event::Eof => break,
            other => writer.write_event(other).map_err(write_error)?,
        }
    }
    Ok(())
}

fn rewrite_start(
    start: BytesStart<'_>,
    ids: &CollectedIds,
    prefix: &str,
    cadence: &mut RebaseCadence<'_>,
) -> Result<BytesStart<'static>> {
    let name = start.name().as_ref().to_owned();
    let mut rewritten = BytesStart::new(name);
    for attribute in start.attributes().with_checks(true) {
        cadence.tick()?;
        let attribute =
            attribute.map_err(|error| rebase_error(format!("invalid SVG attribute: {error}")))?;
        let key = attribute.key.as_ref();
        let value = attribute
            .normalized_value(XmlVersion::Implicit1_0)
            .map_err(|error| rebase_error(format!("invalid attribute value: {error}")))?;
        let value = rewrite_attribute(key, &value, ids, prefix, cadence)?;
        rewritten.push_attribute((key, value.as_str()));
    }
    Ok(rewritten)
}

fn rewrite_attribute(
    qualified_name: &str,
    value: &str,
    ids: &CollectedIds,
    prefix: &str,
    cadence: &mut RebaseCadence<'_>,
) -> Result<String> {
    let mut output = String::new();
    rewrite_attribute_to(qualified_name, value, ids, prefix, &mut output, cadence)?;
    Ok(output)
}

fn rewrite_attribute_to<W: fmt::Write>(
    qualified_name: &str,
    value: &str,
    ids: &CollectedIds,
    prefix: &str,
    output: &mut W,
    cadence: &mut RebaseCadence<'_>,
) -> Result<()> {
    let name = qualified_name.rsplit(':').next().unwrap_or(qualified_name);
    let normalized_name = name.to_ascii_lowercase();
    if is_css_value_attribute(name) {
        return rewrite_component_values_to(value, ids, prefix, false, output, cadence);
    }
    match normalized_name.as_str() {
        "id" => {
            if ids.contains_id(value) {
                write_rebased_reference(prefix, value, output, cadence)?;
            } else {
                write_rebased(output, value)?;
            }
        }
        "href" if value.starts_with('#') => {
            write_rebased(output, "#")?;
            write_rebased_reference(prefix, &value[1..], output, cadence)?;
        }
        name if is_svg_idref_attribute(name) => {
            for (index, id) in value.split_ascii_whitespace().enumerate() {
                cadence.tick()?;
                if index != 0 {
                    write_rebased(output, " ")?;
                }
                write_rebased_reference(prefix, id, output, cadence)?;
            }
        }
        "begin" | "end" => rewrite_smil_timing_to(value, ids, prefix, output, cadence)?,
        _ => write_rebased(output, value)?,
    }
    Ok(())
}

fn write_rebased_reference<W: fmt::Write>(
    prefix: &str,
    id: &str,
    output: &mut W,
    cadence: &mut RebaseCadence<'_>,
) -> Result<()> {
    write_sanitized_prefix(prefix, output, cadence)?;
    write_rebased(output, "-")?;
    cadence.checkpoint()?;
    let result = write_rebased(output, id);
    cadence.checkpoint()?;
    result
}

fn write_sanitized_prefix<W: fmt::Write>(
    prefix: &str,
    output: &mut W,
    cadence: &mut RebaseCadence<'_>,
) -> Result<()> {
    for chunk in prefix.as_bytes().chunks(REBASE_PREFIX_CHECKPOINT_BYTES) {
        cadence.checkpoint()?;
        let chunk = std::str::from_utf8(chunk)
            .map_err(|_| rebase_error("sanitized SVG ID prefix is not ASCII"))?;
        write_rebased(output, chunk)?;
    }
    Ok(())
}

fn serialize_sanitized_prefix<W: fmt::Write>(
    prefix: &str,
    output: &mut W,
    cadence: &mut RebaseCadence<'_>,
    context: &'static str,
) -> Result<()> {
    for chunk in prefix.as_bytes().chunks(REBASE_PREFIX_CHECKPOINT_BYTES) {
        cadence.checkpoint()?;
        let chunk = std::str::from_utf8(chunk)
            .map_err(|_| rebase_error("sanitized SVG ID prefix is not ASCII"))?;
        serialize_name(chunk, output)
            .map_err(|_| rebase_error(format!("failed to serialize rebased {context}")))?;
    }
    Ok(())
}

fn serialize_rebased_identifier<W: fmt::Write>(
    prefix: &str,
    id: &str,
    output: &mut W,
    context: &'static str,
    cadence: &mut RebaseCadence<'_>,
) -> Result<()> {
    // `sanitize_svg_id` guarantees an ASCII alphabetic first prefix byte. Therefore the
    // serialization of `prefix-id` is exactly the CSS name serialization of these three pieces,
    // without constructing the potentially much larger joined identifier.
    serialize_sanitized_prefix(prefix, output, cadence, context)?;
    output
        .write_char('-')
        .map_err(|_| rebase_error(format!("failed to serialize rebased {context}")))?;
    serialize_name_with_checkpoints(id, output, cadence, context)
}

fn serialize_identifier_with_checkpoints<W: fmt::Write>(
    value: &str,
    output: &mut W,
    cadence: &mut RebaseCadence<'_>,
    context: &'static str,
) -> Result<()> {
    cadence.checkpoint()?;
    let result = serialize_identifier(value, output);
    cadence.checkpoint()?;
    result.map_err(|_| rebase_error(format!("failed to serialize rebased {context}")))
}

fn serialize_name_with_checkpoints<W: fmt::Write>(
    value: &str,
    output: &mut W,
    cadence: &mut RebaseCadence<'_>,
    context: &'static str,
) -> Result<()> {
    cadence.checkpoint()?;
    let result = serialize_name(value, output);
    cadence.checkpoint()?;
    result.map_err(|_| rebase_error(format!("failed to serialize rebased {context}")))
}

fn write_rebased(output: &mut (impl fmt::Write + ?Sized), value: &str) -> Result<()> {
    output
        .write_str(value)
        .map_err(|_| rebase_error("failed to write rebased SVG component"))
}

fn rewrite_smil_timing_to<W: fmt::Write>(
    value: &str,
    ids: &CollectedIds,
    prefix: &str,
    output: &mut W,
    cadence: &mut RebaseCadence<'_>,
) -> Result<()> {
    for (part_index, part) in value.split(';').enumerate() {
        cadence.tick()?;
        if part_index != 0 {
            write_rebased(output, ";")?;
        }
        let trimmed = part.trim();
        let Some(index) = trimmed.find('.') else {
            write_rebased(output, trimmed)?;
            continue;
        };
        let eventbase = &trimmed[..index];
        if !ids.contains_id(eventbase)
            || !trimmed[index + 1..]
                .chars()
                .next()
                .is_some_and(|character| character.is_ascii_alphabetic())
        {
            write_rebased(output, trimmed)?;
            continue;
        }
        write_rebased_reference(prefix, eventbase, output, cadence)?;
        write_rebased(output, &trimmed[index..])?;
    }
    Ok(())
}

fn rewrite_stylesheet_to<W: fmt::Write>(
    css: &str,
    ids: &CollectedIds,
    prefix: &str,
    output: &mut W,
    cadence: &mut RebaseCadence<'_>,
) -> Result<()> {
    rewrite_component_values_to(css, ids, prefix, true, output, cadence)
}

fn rewrite_component_values_to<W: fmt::Write>(
    css: &str,
    ids: &CollectedIds,
    prefix: &str,
    rewrite_hashes: bool,
    output: &mut W,
    cadence: &mut RebaseCadence<'_>,
) -> Result<()> {
    let mut parser = Parser::new(css);
    rewrite_parser(&mut parser, ids, prefix, rewrite_hashes, output, cadence)
}

fn rewrite_parser<W: fmt::Write>(
    input: &mut Parser<'_>,
    ids: &CollectedIds,
    prefix: &str,
    rewrite_hashes: bool,
    output: &mut W,
    cadence: &mut RebaseCadence<'_>,
) -> Result<()> {
    let mut group_at_rule = None::<String>;
    loop {
        cadence.tick()?;
        let start = input.position();
        let token = input.next_including_whitespace_and_comments();
        cadence.checkpoint()?;
        let token = match token {
            Ok(token) => token.clone(),
            Err(error) if matches!(error.kind, BasicParseErrorKind::EndOfInput) => {
                write_rebased(output, input.slice_from(start))?;
                break;
            }
            Err(error) => return Err(rebase_error(format!("invalid CSS: {error:?}"))),
        };
        let end = input.position();
        match token {
            Token::AtKeyword(name) => {
                let name = name.to_ascii_lowercase();
                group_at_rule = matches!(
                    name.as_str(),
                    "container" | "document" | "layer" | "media" | "scope" | "supports"
                )
                .then_some(name);
                write_rebased(output, input.slice(start..end))?;
            }
            Token::IDHash(id) | Token::Hash(id)
                if rewrite_hashes && group_at_rule.is_none() && ids.contains_id(id.as_ref()) =>
            {
                output
                    .write_char('#')
                    .map_err(|_| rebase_error("failed to write rebased CSS selector prefix"))?;
                serialize_rebased_identifier(prefix, id.as_ref(), output, "CSS selector", cadence)?;
            }
            Token::UnquotedUrl(url) if url.starts_with('#') => {
                write_rebased(output, "url(#")?;
                write_rebased_reference(prefix, &url[1..], output, cadence)?;
                output
                    .write_char(')')
                    .map_err(|_| rebase_error("failed to write rebased CSS URL terminator"))?;
            }
            Token::Function(name) => {
                write_rebased(output, input.slice(start..end))?;
                let is_url = name.eq_ignore_ascii_case("url");
                let nested_hashes = name.eq_ignore_ascii_case("selector")
                    || (rewrite_hashes && group_at_rule.is_none());
                let mut rewrite_error = None;
                let nested = input.parse_nested_block(|nested| {
                    match rewrite_url_or_nested_to(
                        nested,
                        ids,
                        prefix,
                        nested_hashes,
                        is_url,
                        output,
                        cadence,
                    ) {
                        Ok(()) => Ok(()),
                        Err(error) => {
                            rewrite_error = Some(error);
                            Err(cssparser::ParseError::<()>::custom(()))
                        }
                    }
                });
                cadence.checkpoint()?;
                if let Some(error) = rewrite_error {
                    return Err(error);
                }
                nested.map_err(|_| rebase_error("invalid CSS function"))?;
                output
                    .write_char(')')
                    .map_err(|_| rebase_error("failed to write rebased CSS function terminator"))?;
            }
            Token::ParenthesisBlock | Token::SquareBracketBlock | Token::CurlyBracketBlock => {
                write_rebased(output, input.slice(start..end))?;
                let nested_hashes = if matches!(token, Token::CurlyBracketBlock) {
                    group_at_rule.is_some()
                } else if matches!(token, Token::ParenthesisBlock)
                    && group_at_rule.as_deref() == Some("scope")
                {
                    true
                } else {
                    rewrite_hashes && group_at_rule.is_none()
                };
                let mut rewrite_error = None;
                let nested = input.parse_nested_block(|nested| {
                    let result = if matches!(token, Token::SquareBracketBlock) && nested_hashes {
                        rewrite_attribute_selector_to(nested, prefix, output, cadence)
                    } else {
                        rewrite_parser(nested, ids, prefix, nested_hashes, output, cadence)
                    };
                    match result {
                        Ok(()) => Ok(()),
                        Err(error) => {
                            rewrite_error = Some(error);
                            Err(cssparser::ParseError::<()>::custom(()))
                        }
                    }
                });
                cadence.checkpoint()?;
                if let Some(error) = rewrite_error {
                    return Err(error);
                }
                nested.map_err(|_| rebase_error("invalid nested CSS block"))?;
                output
                    .write_char(match token {
                        Token::ParenthesisBlock => ')',
                        Token::SquareBracketBlock => ']',
                        Token::CurlyBracketBlock => '}',
                        _ => unreachable!(),
                    })
                    .map_err(|_| rebase_error("failed to write rebased CSS block terminator"))?;
                if matches!(token, Token::CurlyBracketBlock) {
                    group_at_rule = None;
                }
            }
            Token::Semicolon => {
                group_at_rule = None;
                write_rebased(output, input.slice(start..end))?;
            }
            Token::BadUrl(_) | Token::BadString(_) => {
                return Err(rebase_error("invalid CSS token"));
            }
            _ => write_rebased(output, input.slice(start..end))?,
        }
    }
    Ok(())
}

fn rewrite_attribute_selector_to<W: fmt::Write>(
    input: &mut Parser<'_>,
    prefix: &str,
    output: &mut W,
    cadence: &mut RebaseCadence<'_>,
) -> Result<()> {
    let mut attribute_name = None::<String>;
    let mut operator = None::<AttributeMatchOperator>;
    loop {
        cadence.tick()?;
        let start = input.position();
        let token = input.next_including_whitespace_and_comments();
        cadence.checkpoint()?;
        let token = match token {
            Ok(token) => token.clone(),
            Err(error) if matches!(error.kind, BasicParseErrorKind::EndOfInput) => {
                write_rebased(output, input.slice_from(start))?;
                break;
            }
            Err(error) => {
                return Err(rebase_error(format!(
                    "invalid CSS attribute selector: {error:?}"
                )));
            }
        };
        let end = input.position();
        match token {
            Token::Ident(name) if operator.is_none() => {
                attribute_name = Some(name.to_ascii_lowercase());
                write_rebased(output, input.slice(start..end))?;
            }
            Token::Delim('=') => {
                operator = Some(AttributeMatchOperator::Exact);
                write_rebased(output, input.slice(start..end))?;
            }
            Token::IncludeMatch => {
                operator = Some(AttributeMatchOperator::Includes);
                write_rebased(output, input.slice(start..end))?;
            }
            Token::DashMatch => {
                operator = Some(AttributeMatchOperator::Dash);
                write_rebased(output, input.slice(start..end))?;
            }
            Token::PrefixMatch => {
                operator = Some(AttributeMatchOperator::Prefix);
                write_rebased(output, input.slice(start..end))?;
            }
            Token::SuffixMatch => {
                operator = Some(AttributeMatchOperator::Suffix);
                write_rebased(output, input.slice(start..end))?;
            }
            Token::SubstringMatch => {
                operator = Some(AttributeMatchOperator::Substring);
                write_rebased(output, input.slice(start..end))?;
            }
            Token::QuotedString(value) if operator.is_some() => {
                output.write_char('"').map_err(|_| {
                    rebase_error("failed to write rebased CSS attribute selector quote")
                })?;
                {
                    let mut escaped = CssStringWriter::new(output);
                    rewrite_selector_attribute_value_to(
                        attribute_name.as_deref(),
                        operator.expect("guarded above"),
                        &value,
                        prefix,
                        &mut escaped,
                        cadence,
                    )?;
                }
                output.write_char('"').map_err(|_| {
                    rebase_error("failed to write rebased CSS attribute selector quote")
                })?;
            }
            Token::Ident(value) if operator.is_some() => {
                serialize_selector_attribute_ident_to(
                    attribute_name.as_deref(),
                    operator.expect("guarded above"),
                    &value,
                    prefix,
                    output,
                    cadence,
                )?;
            }
            Token::IDHash(value) if operator.is_some() => {
                output.write_char('#').map_err(|_| {
                    rebase_error("failed to write rebased CSS attribute selector prefix")
                })?;
                serialize_selector_attribute_id_hash_to(
                    attribute_name.as_deref(),
                    operator.expect("guarded above"),
                    &value,
                    prefix,
                    output,
                    cadence,
                )?;
            }
            Token::BadUrl(_) | Token::BadString(_) => {
                return Err(rebase_error("invalid CSS token"));
            }
            _ => write_rebased(output, input.slice(start..end))?,
        }
    }
    Ok(())
}

#[derive(Clone, Copy)]
enum AttributeMatchOperator {
    Exact,
    Includes,
    Dash,
    Prefix,
    Suffix,
    Substring,
}

impl AttributeMatchOperator {
    fn preserves_id_value(self) -> bool {
        matches!(self, Self::Suffix | Self::Substring)
    }
}

fn serialize_selector_attribute_ident_to<W: fmt::Write>(
    attribute_name: Option<&str>,
    operator: AttributeMatchOperator,
    value: &str,
    prefix: &str,
    output: &mut W,
    cadence: &mut RebaseCadence<'_>,
) -> Result<()> {
    let context = "CSS attribute selector";
    match attribute_name {
        Some("id") if operator.preserves_id_value() => {
            serialize_identifier_with_checkpoints(value, output, cadence, context)
        }
        Some("id") => serialize_rebased_identifier(prefix, value, output, context, cadence),
        Some("href" | "xlink:href") if value.starts_with('#') => {
            write_rebased(output, r"\#")?;
            serialize_rebased_identifier(prefix, &value[1..], output, context, cadence)
        }
        Some(name) if is_svg_idref_attribute(name) => {
            for (index, id) in value.split_ascii_whitespace().enumerate() {
                cadence.tick()?;
                if index != 0 {
                    write_rebased(output, r"\ ")?;
                }
                serialize_rebased_identifier(prefix, id, output, context, cadence)?;
            }
            Ok(())
        }
        _ => serialize_identifier_with_checkpoints(value, output, cadence, context),
    }
}

fn serialize_selector_attribute_id_hash_to<W: fmt::Write>(
    attribute_name: Option<&str>,
    operator: AttributeMatchOperator,
    value: &str,
    prefix: &str,
    output: &mut W,
    cadence: &mut RebaseCadence<'_>,
) -> Result<()> {
    let context = "CSS attribute selector";
    match attribute_name {
        Some("id") if operator.preserves_id_value() => {
            serialize_identifier_with_checkpoints(value, output, cadence, context)
        }
        Some("id") => {
            serialize_sanitized_prefix(prefix, output, cadence, context)?;
            write_rebased(output, r"-\#")?;
            serialize_name_with_checkpoints(value, output, cadence, context)
        }
        Some("href" | "xlink:href") => {
            serialize_rebased_identifier(prefix, value, output, context, cadence)
        }
        Some(name) if is_svg_idref_attribute(name) => {
            serialize_sanitized_prefix(prefix, output, cadence, context)?;
            write_rebased(output, r"-\#")?;
            serialize_name_with_checkpoints(value, output, cadence, context)
        }
        _ => serialize_identifier_with_checkpoints(value, output, cadence, context),
    }
}

fn rewrite_selector_attribute_value_to<W: fmt::Write>(
    attribute_name: Option<&str>,
    operator: AttributeMatchOperator,
    value: &str,
    prefix: &str,
    output: &mut W,
    cadence: &mut RebaseCadence<'_>,
) -> Result<()> {
    match attribute_name {
        Some("id") if operator.preserves_id_value() => write_rebased(output, value)?,
        Some("id") => write_rebased_reference(prefix, value, output, cadence)?,
        Some("href" | "xlink:href") if value.starts_with('#') => {
            write_rebased(output, "#")?;
            write_rebased_reference(prefix, &value[1..], output, cadence)?;
        }
        Some(name) if is_svg_idref_attribute(name) => {
            for (index, id) in value.split_ascii_whitespace().enumerate() {
                cadence.tick()?;
                if index != 0 {
                    write_rebased(output, " ")?;
                }
                write_rebased_reference(prefix, id, output, cadence)?;
            }
        }
        _ => write_rebased(output, value)?,
    }
    Ok(())
}

fn rewrite_url_or_nested_to<W: fmt::Write>(
    input: &mut Parser<'_>,
    ids: &CollectedIds,
    prefix: &str,
    rewrite_hashes: bool,
    is_url: bool,
    output: &mut W,
    cadence: &mut RebaseCadence<'_>,
) -> Result<()> {
    if !is_url {
        return rewrite_parser(input, ids, prefix, rewrite_hashes, output, cadence);
    }
    let body_start = input.position();
    let mut payload = None::<(SourcePosition, SourcePosition, Token<'_>)>;
    let mut has_multiple_payload_tokens = false;
    let body_end = loop {
        cadence.tick()?;
        let token_start = input.position();
        let token = input.next_including_whitespace_and_comments();
        cadence.checkpoint()?;
        let token = match token {
            Ok(token) => token.clone(),
            Err(error) if matches!(error.kind, BasicParseErrorKind::EndOfInput) => {
                break input.position();
            }
            Err(error) => return Err(rebase_error(format!("invalid CSS URL: {error:?}"))),
        };
        let token_end = input.position();
        match token {
            Token::WhiteSpace(_) | Token::Comment(_) => {}
            Token::BadUrl(_) | Token::BadString(_) => {
                return Err(rebase_error("invalid CSS URL token"));
            }
            token if payload.is_none() => payload = Some((token_start, token_end, token)),
            _ => has_multiple_payload_tokens = true,
        }
    };

    if has_multiple_payload_tokens {
        return write_rebased(output, input.slice(body_start..body_end));
    }
    let Some((token_start, token_end, token)) = payload else {
        return write_rebased(output, input.slice(body_start..body_end));
    };

    write_rebased(output, input.slice(body_start..token_start))?;
    match token {
        Token::QuotedString(value) if value.starts_with('#') => {
            output
                .write_char('"')
                .map_err(|_| rebase_error("failed to write rebased CSS URL quote"))?;
            {
                let mut escaped = CssStringWriter::new(output);
                write_rebased(&mut escaped, "#")?;
                write_rebased_reference(prefix, &value[1..], &mut escaped, cadence)?;
            }
            output
                .write_char('"')
                .map_err(|_| rebase_error("failed to write rebased CSS URL quote"))?;
        }
        Token::IDHash(value) => {
            output
                .write_char('#')
                .map_err(|_| rebase_error("failed to write rebased CSS URL prefix"))?;
            serialize_rebased_identifier(prefix, value.as_ref(), output, "CSS URL", cadence)?;
        }
        _ => write_rebased(output, input.slice(token_start..token_end))?,
    }
    write_rebased(output, input.slice(token_end..body_end))
}

fn rebase_error(message: impl Into<String>) -> Error {
    Error::svg_postprocess("rebase-svg-ids", message)
}

fn write_error(error: std::io::Error) -> Error {
    rebase_error(format!("failed to write rebased SVG: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::environment::RenderEnvironment;
    use crate::resources::{
        RenderResourcePolicy, ResourceLimitCause, ResourceLimitId, ResourceLimitPhase,
    };
    use crate::svg::pipeline::{SvgPipeline, SvgPipelinePreset, SvgPostprocessMetadata};
    use merman_core::{CancelReason, OperationControl, OperationPhase};

    fn rebase(svg: &str) -> String {
        let session = RenderEnvironment::deterministic().begin_session().unwrap();
        SvgPipeline::parity()
            .with_postprocessor(RebaseSvgIdsPostprocessor::new("fragment-light"))
            .process_to_string(svg, &session)
            .unwrap()
    }

    #[test]
    fn rebases_ids_and_every_supported_local_reference_shape() {
        let svg = r##"<svg id="root" aria-labelledby="title desc" xmlns:xlink="http://www.w3.org/1999/xlink"><style>#root #shape{fill:url(#paint)}#fff{stroke:#fff}.optional{fill:url(#missing-gradient)}</style><title id="title">T</title><desc id="desc">D</desc><defs><linearGradient id="paint"/><path id="shape"/><path id="fff"/></defs><use href="#shape" xlink:href="#shape"/><animate id="pulse" begin="shape.begin+1s; pulse.end"/><path style="fill:url('#paint')" marker-end="url(#shape)"/></svg>"##;
        let output = rebase(svg);

        for expected in [
            r#"id="fragment-light-root""#,
            r#"aria-labelledby="fragment-light-title fragment-light-desc""#,
            "#fragment-light-root #fragment-light-shape",
            "#fragment-light-fff{stroke:#fff}",
            "url(#fragment-light-paint)",
            "url(#fragment-light-missing-gradient)",
            r##"href="#fragment-light-shape""##,
            "fragment-light-shape.begin+1s;fragment-light-pulse.end",
        ] {
            assert!(output.contains(expected), "missing {expected:?}: {output}");
        }
        roxmltree::Document::parse(&output).expect("rebased SVG XML");
    }

    #[test]
    fn rebases_quoted_urls_without_discarding_whitespace_or_comments() {
        let svg = r##"<svg><style>.spaced{fill:url( "#paint" )}.commented{stroke:u\72l( "#paint" /* trailing */)}</style><defs><linearGradient id="paint"/></defs></svg>"##;
        let output = rebase(svg);

        assert!(
            output.contains("url( &quot;#fragment-light-paint&quot; )"),
            "{output}"
        );
        assert!(
            output.contains("u\\72l( &quot;#fragment-light-paint&quot; /* trailing */)"),
            "{output}"
        );
    }

    #[test]
    fn rebases_css_tokens_across_xml_text_cdata_and_comment_boundaries() {
        let svg = r##"<svg><style>#no<![CDATA[de]]><!-- not CSS text -->{fill:url(#pa<![CDATA[int]]>)}</style><defs><linearGradient id="paint"/></defs><path id="node"/></svg>"##;
        let output = rebase(svg);

        assert!(
            output.contains("#fragment-light-node{fill:url(#fragment-light-paint)}"),
            "{output}"
        );
        assert!(!output.contains("not CSS text"), "{output}");
        assert!(!output.contains("<![CDATA["), "{output}");
    }

    #[test]
    fn preserves_referenced_whitespace_in_attributes_and_style_text() {
        let svg = r##"<svg id="root" data-note="&#13;&#10;&#9;"><style>#root{fill:red}&#13;&#10;&#9;</style></svg>"##;
        let output = rebase(svg);
        let document = roxmltree::Document::parse(&output).expect("rebased SVG XML");

        assert_eq!(
            document.root_element().attribute("data-note"),
            Some("\r\n\t")
        );
        assert_eq!(
            document
                .root_element()
                .first_element_child()
                .unwrap()
                .text(),
            Some("#fragment-light-root{fill:red}\r\n\t")
        );
    }

    #[test]
    fn duplicate_ids_fail_closed() {
        let session = RenderEnvironment::deterministic().begin_session().unwrap();
        let error = SvgPipeline::parity()
            .with_postprocessor(RebaseSvgIdsPostprocessor::new("fragment"))
            .process_to_string(r#"<svg><g id="same"/><path id="same"/></svg>"#, &session)
            .unwrap_err();

        assert!(error.to_string().contains("duplicate SVG id"), "{error}");
    }

    #[test]
    fn projected_n_minus_one_returns_the_exact_svg_byte_limit_error() {
        let svg = r##"<svg id="root" aria-labelledby="paint shape" data-note="&#13;&#10;&#9;"><style>#root[aria-labelledby="paint shape"] #shape{fill:url(#paint);content:"&lt;&amp;"}&#13;&#10;&#9;</style><style><![CDATA[#shape{stroke:url(#paint)}]]></style><defs><linearGradient id="paint"/></defs><path id="shape" fill="url(#paint)"/><use href="#shape"/></svg>"##;
        let processor = RebaseSvgIdsPostprocessor::new("fragment-with-a-long-scope");
        let unbounded_session = RenderEnvironment::deterministic()
            .with_resource_policy(RenderResourcePolicy::unbounded_for_trusted_input())
            .begin_session()
            .unwrap();
        let metadata = SvgPostprocessMetadata::from_svg(svg);
        let unbounded_context = SvgPostprocessContext::new(
            SvgPipelinePreset::Parity,
            0,
            "rebase-svg-ids",
            &metadata,
            &unbounded_session,
        );
        let projected_bytes = processor
            .process(Cow::Borrowed(svg), &unbounded_context)
            .expect("unbounded ID rebase should materialize")
            .len();
        assert!(projected_bytes > svg.len());

        let exact_policy = RenderResourcePolicy::unbounded_for_trusted_input()
            .with_limit(ResourceLimitId::MaxSvgBytes, projected_bytes)
            .unwrap();
        let exact_session = RenderEnvironment::deterministic()
            .with_resource_policy(exact_policy)
            .begin_session()
            .unwrap();
        let exact_context = SvgPostprocessContext::new(
            SvgPipelinePreset::Parity,
            0,
            "rebase-svg-ids",
            &metadata,
            &exact_session,
        );
        let exact = processor
            .process(Cow::Borrowed(svg), &exact_context)
            .expect("the exact SVG byte limit should admit ID rebasing");
        assert_eq!(exact.len(), projected_bytes);

        let policy = RenderResourcePolicy::unbounded_for_trusted_input()
            .with_limit(ResourceLimitId::MaxSvgBytes, projected_bytes - 1)
            .unwrap();
        let session = RenderEnvironment::deterministic()
            .with_resource_policy(policy)
            .begin_session()
            .unwrap();
        let context = SvgPostprocessContext::new(
            SvgPipelinePreset::Parity,
            0,
            "rebase-svg-ids",
            &metadata,
            &session,
        );
        let error = processor.process(Cow::Borrowed(svg), &context).unwrap_err();

        let Error::ResourceLimitExceeded(details) = error else {
            panic!("expected SVG byte resource rejection, got {error}");
        };
        assert_eq!(details.cause, ResourceLimitCause::Ceiling);
        assert_eq!(details.phase, ResourceLimitPhase::SvgPostprocess);
        assert_eq!(details.limit, "max_svg_bytes");
        assert_eq!(details.actual, projected_bytes);
        assert_eq!(details.max, projected_bytes - 1);
    }

    #[test]
    fn large_prefix_style_projection_matches_output_and_rejects_n_minus_one() {
        let prefix = "p".repeat(REBASE_PREFIX_CHECKPOINT_BYTES * 64);
        let svg = r##"<svg><style>#node{fill:url(#paint)}</style><style><![CDATA[#node{stroke:url(#paint)}]]></style><defs><linearGradient id="paint"/></defs><path id="node"/></svg>"##;
        let processor = RebaseSvgIdsPostprocessor::new(prefix.clone());
        let metadata = SvgPostprocessMetadata::from_svg(svg);
        let unbounded_session = RenderEnvironment::deterministic()
            .with_resource_policy(RenderResourcePolicy::unbounded_for_trusted_input())
            .begin_session()
            .unwrap();
        let unbounded_context = SvgPostprocessContext::new(
            SvgPipelinePreset::Parity,
            0,
            "rebase-svg-ids",
            &metadata,
            &unbounded_session,
        );
        let expected = processor
            .process(Cow::Borrowed(svg), &unbounded_context)
            .expect("unbounded ID rebase should materialize");
        let projected_bytes = expected.len();
        assert!(projected_bytes > 1024 * 1024);

        let exact_policy = RenderResourcePolicy::unbounded_for_trusted_input()
            .with_limit(ResourceLimitId::MaxSvgBytes, projected_bytes)
            .unwrap();
        let exact_session = RenderEnvironment::deterministic()
            .with_resource_policy(exact_policy)
            .begin_session()
            .unwrap();
        let exact_context = SvgPostprocessContext::new(
            SvgPipelinePreset::Parity,
            0,
            "rebase-svg-ids",
            &metadata,
            &exact_session,
        );
        let exact = processor
            .process(Cow::Borrowed(svg), &exact_context)
            .expect("the exact SVG byte limit should admit the expanded styles");
        assert_eq!(exact, expected);

        let limited_policy = RenderResourcePolicy::unbounded_for_trusted_input()
            .with_limit(ResourceLimitId::MaxSvgBytes, projected_bytes - 1)
            .unwrap();
        let limited_session = RenderEnvironment::deterministic()
            .with_resource_policy(limited_policy)
            .begin_session()
            .unwrap();
        let limited_context = SvgPostprocessContext::new(
            SvgPipelinePreset::Parity,
            0,
            "rebase-svg-ids",
            &metadata,
            &limited_session,
        );
        let error = processor
            .process(Cow::Borrowed(svg), &limited_context)
            .expect_err("N - 1 bytes must reject before rebased SVG materialization");

        let Error::ResourceLimitExceeded(details) = error else {
            panic!("expected SVG byte resource rejection, got {error}");
        };
        assert_eq!(details.cause, ResourceLimitCause::Ceiling);
        assert_eq!(details.phase, ResourceLimitPhase::SvgPostprocess);
        assert_eq!(details.limit, "max_svg_bytes");
        assert_eq!(details.actual, projected_bytes);
        assert_eq!(details.max, projected_bytes - 1);
    }

    #[test]
    fn streaming_element_limit_preserves_exact_and_n_minus_one_boundaries() {
        let svg = r#"<svg id="root"><g id="first"/><g id="second"/></svg>"#;
        let processor = RebaseSvgIdsPostprocessor::new("fragment-with-a-long-scope");
        let metadata = SvgPostprocessMetadata::from_svg(svg);

        let exact_policy = RenderResourcePolicy::unbounded_for_trusted_input()
            .with_limit(ResourceLimitId::MaxSvgElements, 3)
            .unwrap();
        let exact_session = RenderEnvironment::deterministic()
            .with_resource_policy(exact_policy)
            .begin_session()
            .unwrap();
        let exact_context = SvgPostprocessContext::new(
            SvgPipelinePreset::Parity,
            0,
            "rebase-svg-ids",
            &metadata,
            &exact_session,
        );
        processor
            .process(Cow::Borrowed(svg), &exact_context)
            .expect("the exact element limit should admit ID rebasing");

        let limited_policy = RenderResourcePolicy::unbounded_for_trusted_input()
            .with_limit(ResourceLimitId::MaxSvgElements, 2)
            .unwrap();
        let limited_session = RenderEnvironment::deterministic()
            .with_resource_policy(limited_policy)
            .begin_session()
            .unwrap();
        let limited_context = SvgPostprocessContext::new(
            SvgPipelinePreset::Parity,
            0,
            "rebase-svg-ids",
            &metadata,
            &limited_session,
        );
        let error = processor
            .process(Cow::Borrowed(svg), &limited_context)
            .expect_err("N - 1 elements must fail during streaming collection");

        let Error::ResourceLimitExceeded(details) = error else {
            panic!("expected SVG element resource rejection, got {error}");
        };
        assert_eq!(details.cause, ResourceLimitCause::Ceiling);
        assert_eq!(details.phase, ResourceLimitPhase::SvgPostprocess);
        assert_eq!(details.limit, "max_svg_elements");
        assert_eq!(details.actual, 3);
        assert_eq!(details.max, 2);
    }

    #[test]
    fn id_collection_observes_mid_stream_cancellation() {
        let svg = r#"<svg id="root"><g id="first"/><g id="second"/></svg>"#;
        let control = OperationControl::new();
        let session = RenderEnvironment::deterministic()
            .with_resource_policy(RenderResourcePolicy::unbounded_for_trusted_input())
            .begin_session_with_control(control.clone())
            .unwrap();
        let metadata = SvgPostprocessMetadata::from_svg(svg);
        let context = SvgPostprocessContext::new(
            SvgPipelinePreset::Parity,
            0,
            "rebase-svg-ids",
            &metadata,
            &session,
        );
        let mut cadence = RebaseCadence::new(context.execution());
        control.cancel_after_checkpoints(2);

        let error = collect_ids(svg, &mut cadence)
            .expect_err("the ID scan must observe cancellation between XML events");

        let Error::Cancelled(cancelled) = error else {
            panic!("expected structured cancellation, got {error}");
        };
        assert_eq!(cancelled.phase, OperationPhase::Postprocess);
        assert_eq!(cancelled.reason, CancelReason::Requested);
    }

    #[test]
    fn rebases_group_rule_selectors_without_rewriting_prelude_or_value_colors() {
        let svg = r##"<svg><style>@media (prefers-color-scheme:dark){#root{fill:#fff}}@supports (color:#fff){#root{stroke:#fff}}</style><g id="root"/><g id="fff"/></svg>"##;
        let output = rebase(svg);

        assert!(
            output.contains("@media (prefers-color-scheme:dark){#fragment-light-root{fill:#fff}}"),
            "{output}"
        );
        assert!(
            output.contains("@supports (color:#fff){#fragment-light-root{stroke:#fff}}"),
            "{output}"
        );
    }

    #[test]
    fn rebases_escaped_urls_selector_preludes_attribute_selectors_and_smil_events() {
        let svg = r##"<svg><style>@scope (#root){[href="#shape"],[id="shape"]{fill:u\72l("#paint")}}@supports selector(#root){#root{stroke:#fff}}</style><defs><linearGradient id="paint"/><path id="shape"/><g id="root"/><g id="fff"/></defs><path fill="u\72l(#paint)"/><animate begin="shape.click;shape.repeat(2);shape.begin"/></svg>"##;
        let output = rebase(svg);

        for expected in [
            "@scope (#fragment-light-root)",
            "@supports selector(#fragment-light-root)",
            "[href=&quot;#fragment-light-shape&quot;]",
            "[id=&quot;fragment-light-shape&quot;]",
            "#fragment-light-root{stroke:#fff}",
            "#fragment-light-paint",
            "fragment-light-shape.click;fragment-light-shape.repeat(2);fragment-light-shape.begin",
        ] {
            assert!(output.contains(expected), "missing {expected:?}: {output}");
        }
        assert!(!output.contains("#fragment-light-fff}"), "{output}");
        roxmltree::Document::parse(&output).expect("rebased SVG XML");
    }

    #[test]
    fn leaves_non_css_prose_attributes_opaque() {
        let svg = r#"<svg id="root" aria-label="prose https://example.test/url(foo&amp;bar)"><g id="shape"/></svg>"#;
        let output = rebase(svg);

        assert!(
            output.contains(r#"aria-label="prose https://example.test/url(foo&amp;bar)""#),
            "{output}"
        );
    }

    #[test]
    fn rebases_all_aria_idrefs_in_attributes_and_selectors() {
        let svg = r##"<svg id="root" aria-activedescendant="shape" aria-controls="shape" aria-details="shape" aria-errormessage="shape" aria-flowto="shape" aria-owns="shape"><style>#root [aria-controls="shape"],#root [aria-owns="shape"],#root [id$="-arrowhead"],#root [id*="arrow"],#root [id^="shape"]{fill:red}</style><path id="shape"/><path id="shape-arrowhead"/></svg>"##;
        let output = rebase(svg);

        for attribute in [
            "aria-activedescendant",
            "aria-controls",
            "aria-details",
            "aria-errormessage",
            "aria-flowto",
            "aria-owns",
        ] {
            assert!(
                output.contains(&format!(r#"{attribute}="fragment-light-shape""#)),
                "{attribute}: {output}"
            );
        }
        assert!(
            output.contains("[aria-controls=&quot;fragment-light-shape&quot;]"),
            "{output}"
        );
        assert!(
            output.contains("[aria-owns=&quot;fragment-light-shape&quot;]"),
            "{output}"
        );
        assert!(output.contains("[id$=&quot;-arrowhead&quot;]"), "{output}");
        assert!(output.contains("[id*=&quot;arrow&quot;]"), "{output}");
        assert!(
            output.contains("[id^=&quot;fragment-light-shape&quot;]"),
            "{output}"
        );
    }
}
