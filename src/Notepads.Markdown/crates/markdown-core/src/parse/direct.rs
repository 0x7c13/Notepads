// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

//! Native-owned final values compile directly into the project-owned final IR.
//! Only accepted model validation can publish the returned immutable rows.

use super::prepared::{NativeFault, PreparedBinding, PreparedReads, ReadWitness, with_prepared};
use crate::config::{Extensions, ParseConfig, Profile};
use crate::error::{CoreError, LimitKind};
use crate::identity::{LocalUtf8Span, LogicalSpan, SourceSpan};
#[cfg(test)]
use crate::ir;
use crate::ir::{direct as b, *};
use crate::memory::{BuildGrowingVec, BuildRows};
use crate::policy::{ConstructionId, NativeLease, OwnedWork};
use crate::source::{InputPieceKind, ParseInput};
use comrak::native::{self as native, direct as n};

#[path = "direct_autolink.rs"]
mod autolink;

#[path = "direct_inline_html.rs"]
mod inline_html_regions;
#[path = "direct_projection.rs"]
pub(crate) mod projection;

#[path = "direct_capture.rs"]
pub(super) mod capture;
#[path = "direct_extensions.rs"]
pub(super) mod extensions;
#[path = "direct_footnotes.rs"]
mod footnotes;
#[path = "direct_graph.rs"]
pub(super) mod graph;
#[path = "direct_hidden.rs"]
pub(super) mod hidden;
#[path = "direct_metadata.rs"]
pub(super) mod metadata;
use super::options;
#[cfg(test)]
#[path = "direct_callouts.rs"]
mod callouts;
#[cfg(test)]
#[path = "direct_capture_integration.rs"]
mod capture_integration;
#[cfg(test)]
#[path = "direct_descriptions.rs"]
mod descriptions;
#[cfg(test)]
#[path = "direct_emoji.rs"]
mod emoji;
#[cfg(test)]
#[path = "direct_highlight.rs"]
mod highlight;
#[cfg(test)]
#[path = "direct_lifecycle.rs"]
mod lifecycle;
#[path = "direct_proof.rs"]
pub(super) mod proof;
#[cfg(test)]
#[path = "direct_regression.rs"]
mod regression;
#[cfg(test)]
#[path = "direct_scripts.rs"]
mod scripts;
#[path = "direct_target.rs"]
pub(super) mod target;
#[path = "direct_toc.rs"]
mod toc;
#[cfg(test)]
#[path = "direct_toc_tests.rs"]
mod toc_tests;
#[cfg(test)]
#[path = "direct_wiki_tests.rs"]
mod wiki_tests;

pub(super) struct Native<'store, 'view, 'input, 'source, 'id> {
    pub(super) reader:
        n::ProvenanceReader<'store, 'view, 'input, ConstructionId<'id>, NativeLease<'id>>,
    pub(super) source: ReadWitness<'input, 'source, 'id>,
}

impl<'store, 'view, 'input, 'id> Native<'store, 'view, 'input, '_, 'id> {
    #[cfg(test)]
    pub(super) fn mapping_width(
        &self,
        mapping: n::MappingToken<'view, 'input>,
        work: &mut OwnedWork<'id, '_>,
    ) -> Result<u64, CoreError> {
        let count = self.read(work, |access| access.mapping_len(mapping))?;
        let mut width = 0_u64;
        for index in 0..count {
            let atom = self.read(work, |access| access.mapping_atom(mapping, index))?;
            if atom.hidden_syntax {
                continue;
            }
            if let Some(proof) = atom.proof {
                let info = self.read(work, |access| access.proof(proof))?;
                let mut at = 0;
                while at < info.output_len {
                    let (bytes, units) = self.read(work, |access| {
                        access.with_proof_chunk_next(proof, at, info.output_len, 256, |text, _| {
                            Ok((
                                text.len(),
                                text.chars().map(|c| c.len_utf16() as u64).sum::<u64>(),
                            ))
                        })
                    })?;
                    if bytes == 0 {
                        return Err(work.fail(CoreError::InvalidProvenance));
                    }
                    at += bytes;
                    width = width
                        .checked_add(units)
                        .ok_or_else(|| work.fail(CoreError::ArithmeticOverflow))?;
                }
            } else {
                let mut cursor = self.read(work, |access| access.source_cursor(atom.source))?;
                while let Some(piece) = self.read(work, |access| access.next_source(&mut cursor))? {
                    let units = match piece.kind {
                        native::AppendKind::PartialTab { .. }
                        | native::AppendKind::TableEscapedPipe => {
                            (piece.r_end - piece.r_start) as u64
                        }
                        native::AppendKind::Copy => {
                            let text = self
                                .source
                                .input()
                                .text()
                                .get(piece.p.start..piece.p.end)
                                .ok_or_else(|| work.fail(CoreError::InvalidProvenance))?;
                            let mut units = 0_u64;
                            for scalar in text.chars() {
                                work.scan(scalar.len_utf8() as u64)?;
                                units = units
                                    .checked_add(scalar.len_utf16() as u64)
                                    .ok_or_else(|| work.fail(CoreError::ArithmeticOverflow))?;
                            }
                            units
                        }
                        native::AppendKind::CodeTerminal => {
                            return Err(work.fail(CoreError::InvalidProvenance));
                        }
                    };
                    width = width
                        .checked_add(units)
                        .ok_or_else(|| work.fail(CoreError::ArithmeticOverflow))?;
                }
            }
        }
        Ok(width)
    }

    pub(super) fn read<T>(
        &self,
        work: &mut OwnedWork<'id, '_>,
        action: impl FnOnce(
            &mut n::ProvenanceReadAccess<
                '_,
                'store,
                'view,
                'input,
                PreparedReads<'_, 'input, 'id, '_>,
                NativeLease<'id>,
            >,
        ) -> Result<T, CoreError>,
    ) -> Result<T, CoreError> {
        self.source
            .with_reads(work, |reads| self.reader.with_reads(reads, action))
    }

    pub(super) fn inline_html_break(
        &self,
        mapping: n::MappingToken<'view, 'input>,
        work: &mut OwnedWork<'id, '_>,
    ) -> Result<
        Option<(
            n::SourceToken<'view, 'input>,
            SourceSpan,
            std::ops::Range<usize>,
        )>,
        CoreError,
    > {
        let Some((token, source, range)) =
            self.inline_html_copy(mapping, INLINE_HTML_BREAK_BYTES, work)?
        else {
            return Ok(None);
        };
        Ok(is_inline_html_break(self.html_source(source, work)?).then_some((token, source, range)))
    }

    pub(super) fn html_source(
        &self,
        source: SourceSpan,
        work: &mut OwnedWork<'id, '_>,
    ) -> Result<&[u8], CoreError> {
        let raw = self
            .source
            .input()
            .source()
            .text()
            .as_bytes()
            .get(source.start() as usize..source.end() as usize)
            .ok_or(CoreError::InvalidProvenance)?;
        work.scan(raw.len() as u64)?;
        Ok(raw)
    }

    pub(super) fn inline_html_copy(
        &self,
        mapping: n::MappingToken<'view, 'input>,
        length: std::ops::RangeInclusive<usize>,
        work: &mut OwnedWork<'id, '_>,
    ) -> Result<
        Option<(
            n::SourceToken<'view, 'input>,
            SourceSpan,
            std::ops::Range<usize>,
        )>,
        CoreError,
    > {
        if self.read(work, |access| access.mapping_len(mapping))? != 1 {
            return Ok(None);
        }
        let atom = self.read(work, |access| access.mapping_atom(mapping, 0))?;
        if atom.proof.is_some() || atom.hidden_syntax || !length.contains(&atom.bytes) {
            return Ok(None);
        }
        Ok(self
            .inline_html_source_copy(atom.source, length, work)?
            .filter(|(_, range)| range.len() == atom.bytes)
            .map(|(span, range)| (atom.source, span, range)))
    }

    pub(super) fn inline_html_source_copy(
        &self,
        source: n::SourceToken<'view, 'input>,
        length: std::ops::RangeInclusive<usize>,
        work: &mut OwnedWork<'id, '_>,
    ) -> Result<Option<(SourceSpan, std::ops::Range<usize>)>, CoreError> {
        let mut cursor = self.read(work, |access| access.source_cursor(source))?;
        let Some(piece) = self.read(work, |access| access.next_source(&mut cursor))? else {
            return Ok(None);
        };
        let Some(bytes) = piece.r_end.checked_sub(piece.r_start) else {
            return Ok(None);
        };
        if !matches!(piece.kind, native::AppendKind::Copy)
            || !length.contains(&bytes)
            || piece.p.end.checked_sub(piece.p.start) != Some(bytes)
            || self
                .read(work, |access| access.next_source(&mut cursor))?
                .is_some()
        {
            return Ok(None);
        }
        let mut pieces = self.source.piece_cursor(piece.p, work)?;
        let Some(row) = pieces.next(work)? else {
            return Ok(None);
        };
        if row.kind != InputPieceKind::Copy || pieces.next(work)?.is_some() {
            return Ok(None);
        }
        Ok(Some((row.source, piece.r_start..piece.r_end)))
    }

    pub(super) fn sources(
        &self,
        token: n::SourceToken<'view, 'input>,
        builder: &mut b::DirectBuilder<'id>,
        work: &mut OwnedWork<'id, '_>,
    ) -> Result<(b::SourceBlockRef, usize, usize), CoreError> {
        let start = builder.sources_len();
        let (raw_start, raw_end) = self.visit_sources(token, work, |source, work| {
            builder.source_parts(&[source], work).map(|_| ())
        })?;
        let block = builder.written_source_block(start, builder.sources_len() - start, work)?;
        Ok((block, raw_start, raw_end))
    }

    pub(super) fn visit_sources(
        &self,
        token: n::SourceToken<'view, 'input>,
        work: &mut OwnedWork<'id, '_>,
        mut emit: impl FnMut(SourceSpan, &mut OwnedWork<'id, '_>) -> Result<(), CoreError>,
    ) -> Result<(usize, usize), CoreError> {
        let mut cursor = self.read(work, |access| access.source_cursor(token))?;
        let mut pending: Option<SourceSpan> = None;
        let mut raw_start = None;
        let mut raw_end = 0;
        while let Some(piece) = self.read(work, |access| access.next_source(&mut cursor))? {
            raw_start.get_or_insert(piece.r_start);
            raw_end = piece.r_end;
            self.source.p_sources(piece.p, work, |source, work| {
                if let Some(previous) = pending {
                    work.events(1)?;
                    if previous.end() == source.start() {
                        pending = Some(SourceSpan::new(previous.start(), source.end())?);
                        return Ok(());
                    }
                    if previous.end() > source.start() {
                        return Err(work.fail(CoreError::InvalidProvenance));
                    }
                    emit(previous, work)?;
                }
                pending = Some(source);
                Ok(())
            })?;
        }
        emit(
            pending.ok_or_else(|| work.fail(CoreError::InvalidProvenance))?,
            work,
        )?;
        Ok((
            raw_start.ok_or_else(|| work.fail(CoreError::InvalidProvenance))?,
            raw_end,
        ))
    }
}

/// AST handles and values are accessible only during the first consumer phase.
/// Common source/mapping methods use the original provenance reader and witness.
pub(super) struct AstNative<'store, 'view, 'arena, 'input, 'source, 'id> {
    reader: n::AstReader<'store, 'view, 'arena, 'input, ConstructionId<'id>, NativeLease<'id>>,
    common: Native<'store, 'view, 'input, 'source, 'id>,
}

impl<'store, 'view, 'arena, 'input, 'source, 'id>
    AstNative<'store, 'view, 'arena, 'input, 'source, 'id>
{
    fn new(
        reader: n::AstReader<'store, 'view, 'arena, 'input, ConstructionId<'id>, NativeLease<'id>>,
        source: ReadWitness<'input, 'source, 'id>,
    ) -> Self {
        let provenance = reader.provenance_reader();
        Self {
            reader,
            common: Native {
                reader: provenance,
                source,
            },
        }
    }

    fn read<T>(
        &self,
        work: &mut OwnedWork<'id, '_>,
        action: impl FnOnce(
            &mut n::AstReadAccess<
                '_,
                'store,
                'view,
                'arena,
                'input,
                PreparedReads<'_, 'input, 'id, '_>,
                NativeLease<'id>,
            >,
        ) -> Result<T, CoreError>,
    ) -> Result<T, CoreError> {
        self.source
            .with_reads(work, |reads| self.reader.with_reads(reads, action))
    }
}

impl<'store, 'view, 'arena, 'input, 'source, 'id> std::ops::Deref
    for AstNative<'store, 'view, 'arena, 'input, 'source, 'id>
{
    type Target = Native<'store, 'view, 'input, 'source, 'id>;
    fn deref(&self) -> &Self::Target {
        &self.common
    }
}

struct CoreCompiler<'id>(
    std::marker::PhantomData<fn(&'id ()) -> &'id ()>,
    ParseConfig,
);

struct CoreState<'view, 'input, 'id> {
    graph: graph::Graph<'view, 'input, 'id>,
    captured: capture::Captured<'view, 'input, 'id>,
    builder: b::DirectBuilder<'id>,
}

impl<'input, 'source, 'work, 'id, 'op>
    n::DirectCompiler<'input, PreparedBinding<'input, 'source, 'work, 'id, 'op>>
    for CoreCompiler<'id>
{
    type Output = b::DirectBuilder<'id>;
    type State<'view> = CoreState<'view, 'input, 'id>;

    fn ast<'store, 'view, 'arena>(
        &mut self,
        reader: n::AstReader<'store, 'view, 'arena, 'input, ConstructionId<'id>, NativeLease<'id>>,
        host: &mut PreparedBinding<'input, 'source, 'work, 'id, 'op>,
    ) -> Result<Self::State<'view>, NativeFault<'input, 'id>> {
        host.native_parts(|source, work| {
            compile_ast_for_config(&AstNative::new(reader, source), self.1, work)
        })
    }

    fn provenance<'store, 'view>(
        &mut self,
        reader: n::ProvenanceReader<'store, 'view, 'input, ConstructionId<'id>, NativeLease<'id>>,
        state: Self::State<'view>,
        host: &mut PreparedBinding<'input, 'source, 'work, 'id, 'op>,
    ) -> Result<Self::Output, NativeFault<'input, 'id>> {
        host.native_parts(|source, work| {
            compile_provenance(&Native { reader, source }, state, work)
        })
    }
}

pub(crate) fn build<'id>(
    input: &ParseInput,
    config: ParseConfig,
    work: &mut OwnedWork<'id, '_>,
) -> Result<CompactIr, CoreError> {
    build_with_projection(input, config, None, work)
}

pub(crate) fn build_with_projection<'id>(
    input: &ParseInput,
    config: ParseConfig,
    projection: Option<&crate::html::HtmlProjection>,
    work: &mut OwnedWork<'id, '_>,
) -> Result<CompactIr, CoreError> {
    work.checkpoint_now()?;
    let (options, policy) = native_configuration(config);
    let builder = super::prepared::with_prepared_projection(
        input,
        projection,
        work,
        |native_input, mut binding| {
            let result = n::compile_with_inline_html_policy(
                native_input,
                &options,
                &mut binding,
                CoreCompiler(std::marker::PhantomData, config),
                policy,
                native_inline_html_policy(config),
            );
            binding.resolve(result)
        },
    )?;
    // Only owned final storage crosses the native boundary. Node pages completed
    // after AST retirement; the remaining tables freeze after the native tail.
    work.checkpoint_now()?;
    builder.finish(work)
}

fn native_configuration(config: ParseConfig) -> (comrak::Options<'static>, native::InlinePolicy) {
    let (mut options, policy) = match config.profile() {
        Profile::C0 => (options::c0(), native::InlinePolicy::Ordinary),
        Profile::G3 => (options::g3(), native::InlinePolicy::StrictG4),
    };
    if config.extensions().contains(Extensions::MATH) {
        options.extension.math_dollars = true;
        options.extension.math_code = true;
        options.extension.math_latex = true;
        options.parse.latex_display_standalone = true;
    }
    options.extension.native_heading_ids = config.extensions().contains(Extensions::HEADING_IDS);
    options.extension.footnotes = config.extensions().contains(Extensions::FOOTNOTES);
    options.extension.inline_footnotes = options.extension.footnotes;
    options.extension.alerts = config.extensions().contains(Extensions::CALLOUTS);
    options.extension.highlight = config.extensions().contains(Extensions::HIGHLIGHT);
    let scripts = config.extensions().contains(Extensions::INLINE_SCRIPTS);
    options.extension.superscript = scripts;
    options.extension.subscript = scripts;
    options.extension.shortcodes = config.extensions().contains(Extensions::EMOJI_SHORTCODES);
    options.extension.native_table_of_contents =
        config.extensions().contains(Extensions::TABLE_OF_CONTENTS);
    options.extension.native_wiki_links =
        config.extensions().contains(Extensions::LOCAL_WIKI_LINKS);
    options.extension.native_note_embeds = config.extensions().contains(Extensions::NOTE_EMBEDS);
    options.extension.description_lists =
        config.extensions().contains(Extensions::DESCRIPTION_LISTS);

    (options, policy)
}

fn native_inline_html_policy(config: ParseConfig) -> native::InlineHtmlPolicy {
    if !config.extensions().contains(Extensions::HTML_INLINE) {
        return native::InlineHtmlPolicy::Disabled;
    }
    match config.profile() {
        Profile::C0 => native::InlineHtmlPolicy::RawText,
        Profile::G3 => native::InlineHtmlPolicy::Tagfilter,
    }
}

#[cfg(test)]
fn compile_ast<'view, 'arena, 'input, 'id>(
    native: &AstNative<'_, 'view, 'arena, 'input, '_, 'id>,
    work: &mut OwnedWork<'id, '_>,
) -> Result<CoreState<'view, 'input, 'id>, CoreError> {
    compile_ast_for_config(native, ParseConfig::new(Profile::C0), work)
}

fn compile_ast_for_config<'view, 'arena, 'input, 'id>(
    native: &AstNative<'_, 'view, 'arena, 'input, '_, 'id>,
    config: ParseConfig,
    work: &mut OwnedWork<'id, '_>,
) -> Result<CoreState<'view, 'input, 'id>, CoreError> {
    let mut builder = b::DirectBuilder::new(work)?;
    let mut captures = capture::CaptureBuilder::new(native, &mut builder, work)?;
    let graph =
        graph::Graph::prepare_for_config(native, config, &mut builder, &mut captures, work)?;
    let captured = captures.finish(graph.metadata_len(), work)?;
    Ok(CoreState {
        graph,
        captured,
        builder,
    })
}

fn compile_provenance<'view, 'input, 'id>(
    native: &Native<'_, 'view, 'input, '_, 'id>,
    state: CoreState<'view, 'input, 'id>,
    work: &mut OwnedWork<'id, '_>,
) -> Result<b::DirectBuilder<'id>, CoreError> {
    let CoreState {
        mut graph,
        captured,
        mut builder,
    } = state;
    let images = metadata::emit(native, &mut graph, &mut builder, captured, work)?;
    target::flows(native, &mut graph, &mut builder, &images, work)?;
    builder.complete_nodes(work)?;
    drop(images);
    drop(graph);
    Ok(builder)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::{ModelIdentity, SourceKey};
    use crate::mapping::MapIndex;
    use crate::policy::{CancellationToken, SourceBudget, construction_scope};
    use crate::source::SourceDocument;

    #[test]
    fn actual_direct_final_rows_pass_independent_validation_and_index() {
        for text in ["", "hello 中😀 world"] {
            for verified in [false, true] {
                let budget = if verified {
                    SourceBudget::task_fixture()
                } else {
                    SourceBudget::model_fixture()
                };
                let mut operation = budget.begin(CancellationToken::new());
                let source = SourceDocument::new(
                    SourceKey::new([75; 16], 1).unwrap(),
                    text.as_bytes(),
                    &mut operation,
                )
                .unwrap()
                .snapshot();
                let config = ParseConfig::new(Profile::C0);
                construction_scope(&mut operation, |mut work| {
                    let input = ParseInput::prepare_owned(source.clone(), &mut work)?;
                    let ir = build(&input, config, &mut work).unwrap_or_else(|e| {
                        panic!("direct build {text:?} verified={verified}: {e:?}")
                    });
                    let identity = ModelIdentity::new(source.key(), 1, config)?;
                    if verified {
                        let facts = ir::validate::validate_for_index_owned(
                            &ir, &source, identity, &mut work,
                        )
                        .unwrap_or_else(|e| panic!("direct validate/facts {text:?}: {e:?}"));
                        let (index, _) = MapIndex::build_verified(facts)?;
                        drop(index);
                    } else {
                        ir::validate::validate_owned(&ir, &source, &mut work)
                            .unwrap_or_else(|e| panic!("direct validate {text:?}: {e:?}"));
                        drop(MapIndex::build_owned(&ir, &source, identity, &mut work)?);
                    }
                    assert_eq!(
                        ir.flows()[0].logical_utf16,
                        text.encode_utf16().count() as u64
                    );
                    drop(ir);
                    drop(input);
                    work.finish()
                })
                .unwrap();
            }
        }
    }
}
