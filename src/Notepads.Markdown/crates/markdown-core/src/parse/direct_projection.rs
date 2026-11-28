// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

//! Native grammar facts for projection, without constructing an accepted IR.
//!
//! Membership comes from the same final native tree as the direct compiler.
//! Envelopes use its prepared source witness and the same mapping, syntax,
//! non-Text accepted claims and fallback anchors as final NodeRows. These facts
//! cannot grant mapping/query authority; the projected model still passes the
//! complete independent validation/index/publication pipeline.

use super::*;
use crate::html::{HtmlProjection, front_matter, work::OwnedProjectionWork};
use crate::memory::{BuildValue, FrozenRows};
use crate::policy::Lease;
use crate::source::SourceSnapshot;

pub(crate) type ProjectionOutput = (HtmlProjection, String, Lease);

pub(crate) enum ProjectionBuild {
    Plain(CompactIr),
    Projected(ProjectionOutput),
    MaskedFallback,
}

enum NativeOutput<'id> {
    Plain(b::DirectBuilder<'id>),
    Projected(ProjectionOutput),
    MaskedFallback,
}

enum ProjectionState<'view, 'input, 'id> {
    Plain(CoreState<'view, 'input, 'id>),
    Projected(ProjectionOutput),
    MaskedFallback,
}

struct ProjectOrCompile<'source> {
    original: &'source SourceSnapshot,
    front: Option<front_matter::Prepared>,
    config: ParseConfig,
    inline_html_region: bool,
    directives: bool,
}

impl<'input, 'source, 'work, 'id, 'op>
    n::DirectCompiler<'input, PreparedBinding<'input, 'source, 'work, 'id, 'op>>
    for ProjectOrCompile<'_>
{
    type Output = NativeOutput<'id>;
    type State<'view> = ProjectionState<'view, 'input, 'id>;

    fn ast<'store, 'view, 'arena>(
        &mut self,
        reader: n::AstReader<'store, 'view, 'arena, 'input, ConstructionId<'id>, NativeLease<'id>>,
        host: &mut PreparedBinding<'input, 'source, 'work, 'id, 'op>,
    ) -> Result<Self::State<'view>, NativeFault<'input, 'id>> {
        host.native_parts(|source, work| {
            let native = AstNative::new(reader, source);
            let facts = collect(
                &native,
                self.config.extensions().contains(Extensions::HTML_BLOCKS),
                self.inline_html_region,
                self.front
                    .as_ref()
                    .is_some_and(|front| front.has_citations()),
                self.directives,
                work,
            )?;
            let masked = self.front.is_some();
            let projection = HtmlProjection::build(
                Some(&facts),
                self.config,
                self.original,
                self.front.take(),
                &mut OwnedProjectionWork::new(work, self.original.budget())?,
            )?;
            drop(facts);
            if let Some(projection) = projection {
                Ok(ProjectionState::Projected(projection))
            } else if masked {
                // A failed optional projection must never accept masked input.
                Ok(ProjectionState::MaskedFallback)
            } else {
                // Unsupported markup and incidental '<' reuse this exact native
                // tree and prepared witness, avoiding a second grammar parse.
                Ok(ProjectionState::Plain(compile_ast_for_config(
                    &native,
                    self.config,
                    work,
                )?))
            }
        })
    }

    fn provenance<'store, 'view>(
        &mut self,
        reader: n::ProvenanceReader<'store, 'view, 'input, ConstructionId<'id>, NativeLease<'id>>,
        state: Self::State<'view>,
        host: &mut PreparedBinding<'input, 'source, 'work, 'id, 'op>,
    ) -> Result<Self::Output, NativeFault<'input, 'id>> {
        host.native_parts(|source, work| match state {
            ProjectionState::Plain(state) => {
                compile_provenance(&Native { reader, source }, state, work).map(NativeOutput::Plain)
            }
            ProjectionState::Projected(projection) => Ok(NativeOutput::Projected(projection)),
            ProjectionState::MaskedFallback => Ok(NativeOutput::MaskedFallback),
        })
    }
}

pub(crate) fn build_or_project<'id>(
    input: &ParseInput,
    config: ParseConfig,
    original: &SourceSnapshot,
    front: Option<front_matter::Prepared>,
    inline_html_region: bool,
    directives: bool,
    work: &mut OwnedWork<'id, '_>,
) -> Result<ProjectionBuild, CoreError> {
    work.checkpoint_now()?;
    work.ensure_budget(original.budget())?;
    let (options, policy) = native_configuration(config);
    let output = with_prepared(input, work, |native_input, mut binding| {
        let result = n::compile_with_inline_html_policy(
            native_input,
            &options,
            &mut binding,
            ProjectOrCompile {
                original,
                front,
                config,
                inline_html_region,
                directives,
            },
            policy,
            native_inline_html_policy(config),
        );
        binding.resolve(result)
    })?;
    work.checkpoint_now()?;
    match output {
        NativeOutput::Plain(builder) => builder.finish(work).map(ProjectionBuild::Plain),
        NativeOutput::Projected(projection) => Ok(ProjectionBuild::Projected(projection)),
        NativeOutput::MaskedFallback => Ok(ProjectionBuild::MaskedFallback),
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ProjectionBlock {
    pub kind: NodeKind,
    pub source: SourceSpan,
}
impl BuildValue for ProjectionBlock {}

#[derive(Clone, Copy)]
pub(crate) struct ProjectionText {
    pub span: SourceSpan,
    pub scope: u32,
    citation: bool,
    directive: bool,
    root_text: bool,
}
impl BuildValue for ProjectionText {}

pub(crate) struct ProjectionFacts {
    blocks: FrozenRows<ProjectionBlock>,
    text: Option<FrozenRows<ProjectionText>>,
    key: crate::identity::SourceKey,
    length: usize,
    budget: crate::policy::SourceBudget,
}
impl ProjectionFacts {
    pub(crate) fn blocks(&self) -> &[ProjectionBlock] {
        self.blocks.as_slice()
    }

    pub(crate) fn citation_text(&self) -> impl Iterator<Item = &ProjectionText> {
        self.text
            .iter()
            .flat_map(|rows| rows.as_slice())
            .filter(|row| row.citation)
    }

    pub(crate) fn directive_text(&self) -> impl Iterator<Item = (SourceSpan, bool)> + '_ {
        self.text
            .iter()
            .flat_map(|rows| rows.as_slice())
            .filter_map(|row| row.directive.then_some((row.span, row.root_text)))
    }

    pub(crate) fn matches_original(&self, source: &SourceSnapshot) -> bool {
        self.key == source.key()
            && self.length == source.text().len()
            && self.budget.same_root(source.budget())
    }

    #[cfg(test)]
    pub(crate) fn from_model(
        model: &crate::model::AcceptedModel,
        work: &mut OwnedWork<'_, '_>,
    ) -> Result<Self, CoreError> {
        work.ensure_budget(model.budget())?;
        let mut blocks = BuildGrowingVec::new(
            work.require_model_limits()?.model_records,
            LimitKind::ModelRecords,
            work,
        )?;
        let ir = model.ir();
        let root = ir.flows()[model.main_flow_id().index()].root;
        let mut next = ir.nodes()[root.index()].first_child;
        while let Some(id) = next {
            work.events(1)?;
            let node = ir.nodes()[id.index()];
            next = node.next_sibling;
            if matches!(node.kind, NodeKind::HtmlBlock | NodeKind::Paragraph) {
                blocks.push(
                    ProjectionBlock {
                        kind: node.kind,
                        source: node.source_envelope,
                    },
                    work,
                )?;
            }
        }
        Ok(Self {
            blocks: blocks.freeze(work)?,
            text: None,
            key: model.source().key(),
            length: model.source().text().len(),
            budget: model.budget().clone(),
        })
    }
}

#[derive(Clone, Copy)]
struct DraftBlock {
    kind: NodeKind,
    source: Option<SourceSpan>,
}
impl BuildValue for DraftBlock {}

#[derive(Clone, Copy)]
struct Slot {
    block: Option<u32>,
    non_text: bool,
    initial: Option<SourceSpan>,
    anchor: u64,
}

fn union(left: Option<SourceSpan>, right: SourceSpan) -> Result<SourceSpan, CoreError> {
    match left {
        Some(left) => SourceSpan::new(left.start().min(right.start()), left.end().max(right.end())),
        None => Ok(right),
    }
}

#[cfg(test)]
struct ProjectionCompiler;

#[cfg(test)]
impl<'input, 'source, 'work, 'id, 'op>
    n::DirectCompiler<'input, PreparedBinding<'input, 'source, 'work, 'id, 'op>>
    for ProjectionCompiler
{
    type Output = ProjectionFacts;
    type State<'view> = ProjectionFacts;

    fn ast<'store, 'view, 'arena>(
        &mut self,
        reader: n::AstReader<'store, 'view, 'arena, 'input, ConstructionId<'id>, NativeLease<'id>>,
        host: &mut PreparedBinding<'input, 'source, 'work, 'id, 'op>,
    ) -> Result<Self::State<'view>, NativeFault<'input, 'id>> {
        host.native_parts(|source, work| {
            collect(
                &AstNative::new(reader, source),
                true,
                false,
                false,
                false,
                work,
            )
        })
    }

    fn provenance<'store, 'view>(
        &mut self,
        _: n::ProvenanceReader<'store, 'view, 'input, ConstructionId<'id>, NativeLease<'id>>,
        facts: Self::State<'view>,
        host: &mut PreparedBinding<'input, 'source, 'work, 'id, 'op>,
    ) -> Result<Self::Output, NativeFault<'input, 'id>> {
        host.native_parts(|_, work| {
            work.checkpoint_now()?;
            Ok(facts)
        })
    }
}

#[cfg(test)]
pub(crate) fn projection_facts<'id>(
    input: &ParseInput,
    config: ParseConfig,
    work: &mut OwnedWork<'id, '_>,
) -> Result<ProjectionFacts, CoreError> {
    work.checkpoint_now()?;
    let (options, policy) = native_configuration(config);
    with_prepared(input, work, |native_input, mut binding| {
        let result = n::compile_with_inline_html_policy(
            native_input,
            &options,
            &mut binding,
            ProjectionCompiler,
            policy,
            native_inline_html_policy(config),
        );
        binding.resolve(result)
    })
}

fn collect<'id>(
    native: &AstNative<'_, '_, '_, '_, '_, 'id>,
    html_blocks: bool,
    inline_html_region: bool,
    citations: bool,
    directives: bool,
    work: &mut OwnedWork<'id, '_>,
) -> Result<ProjectionFacts, CoreError> {
    let limits = work
        .model_limits()
        .ok_or(CoreError::UnsupportedConfiguration)?;
    // A standalone HTML label or anchor can belong to a Markdown paragraph.
    // The conservative probe admits their containing envelopes for validation.
    let paragraphs =
        html_blocks && contains_paragraph_marker(native.source.input().source().text(), work)?;
    let count = native.read(work, |a| Ok(a.created_nodes()))? as usize;
    let mut slots = BuildRows::new(limits.native_records, LimitKind::NativeRecords, work)?;
    let mut blocks = BuildGrowingVec::new(limits.model_records, LimitKind::ModelRecords, work)?;
    let root = native.read(work, |a| a.root())?;
    let citations = citations && accepts_bibliography(native, root, work)?;
    let notes = if citations || directives || inline_html_region {
        footnotes::NotePlan::prepare(native, root, work)?
    } else {
        None
    };
    let text = if citations || directives {
        Some(collect_projection_text(
            native,
            root,
            citations,
            directives,
            notes.as_ref(),
            work,
        )?)
    } else {
        None
    };
    let mut next = if html_blocks {
        native.read(work, |a| a.node(root))?.first_child
    } else {
        None
    };
    while let Some(token) = next {
        let top = native.read(work, |a| a.node(token))?;
        next = top.next_sibling;
        if top.kind != native::NativeKind::HtmlBlock
            && !(paragraphs && top.kind == native::NativeKind::Paragraph)
        {
            continue;
        }
        if slots.len() == 0 {
            for _ in 0..count {
                slots.push(
                    Slot {
                        block: None,
                        non_text: false,
                        initial: None,
                        anchor: 0,
                    },
                    work,
                )?;
            }
        }
        let block =
            u32::try_from(blocks.len()).map_err(|_| work.fail(CoreError::ArithmeticOverflow))?;
        blocks.push(
            DraftBlock {
                kind: graph::kind(top.kind)?,
                source: None,
            },
            work,
        )?;
        let mut current = token;
        loop {
            let mut row = native.read(work, |a| a.node(current))?;
            let lineage = native.read(work, |a| a.lineage(current))?;
            let binding = native.read(work, |a| a.binding(lineage))?;
            let mut initial = None;
            let mappings = native.read(work, |a| a.mapping_len(binding.value))?;
            for index in 0..mappings {
                let atom = native.read(work, |a| a.mapping_atom(binding.value, index))?;
                native.visit_sources(atom.source, work, |source, _| {
                    initial = Some(union(initial, source)?);
                    Ok(())
                })?;
            }
            if let Some(syntax) = binding.syntax {
                native.visit_sources(syntax, work, |source, _| {
                    initial = Some(union(initial, source)?);
                    Ok(())
                })?;
            }
            let anchor = native.source.p_anchor(binding.start_p.unwrap_or(0), work)?;
            slots.replace(
                row.id.get() as usize - 1,
                Slot {
                    block: Some(block),
                    non_text: row.kind != native::NativeKind::Text,
                    initial,
                    anchor,
                },
                work,
            )?;
            // Images own their label as a separate Alt flow in the final IR.
            // Its descendants therefore cannot extend a Main block envelope.
            if row.kind != native::NativeKind::Image
                && let Some(child) = row.first_child
            {
                current = child;
                continue;
            }
            loop {
                if row.id == top.id {
                    break;
                }
                if let Some(sibling) = row.next_sibling {
                    current = sibling;
                    break;
                }
                let parent = row
                    .parent
                    .ok_or_else(|| work.fail(CoreError::InvalidProvenance))?;
                row = native.read(work, |a| a.node(parent))?;
            }
            if row.id == top.id {
                break;
            }
        }
    }
    let mut output = BuildGrowingVec::new(limits.model_records, LimitKind::ModelRecords, work)?;
    if inline_html_region {
        inline_html_regions::collect(native, notes.as_ref(), &mut output, work)?;
    }
    let inline_rows = output.len();
    if blocks.len() == 0 {
        if inline_rows > 1 {
            sort_html_projection_blocks(&mut output, work)?;
        }
        return Ok(ProjectionFacts {
            blocks: output.freeze(work)?,
            text,
            key: native.source.input().source().key(),
            length: native.source.input().source().text().len(),
            budget: native.source.input().source().budget().clone(),
        });
    }
    let claims = native.read(work, |a| Ok(a.claim_count()))?;
    for index in 0..claims {
        let claim = native.read(work, |a| a.claim(index))?;
        let owner = native.read(work, |a| a.binding(claim.owner))?;
        let at = owner.id.get() as usize - 1;
        let mut slot = slots.get(at, work)?;
        if slot.block.is_some() && slot.non_text {
            native.visit_sources(claim.source, work, |source, _| {
                slot.initial = Some(union(slot.initial, source)?);
                Ok(())
            })?;
            slots.replace(at, slot, work)?;
        }
    }
    for index in 0..slots.len() {
        let slot = slots.get(index, work)?;
        if let Some(block) = slot.block {
            let at = block as usize;
            let mut row = target::growing(&blocks, at, work)?;
            let source = match slot.initial {
                Some(source) => source,
                None => SourceSpan::new(slot.anchor, slot.anchor)?,
            };
            row.source = Some(union(row.source, source)?);
            blocks.replace(at, row, work)?;
        }
    }
    drop(slots);
    for index in 0..blocks.len() {
        let block = target::growing(&blocks, index, work)?;
        output.push(
            ProjectionBlock {
                kind: block.kind,
                source: block
                    .source
                    .ok_or_else(|| work.fail(CoreError::InvalidProvenance))?,
            },
            work,
        )?;
    }
    if inline_rows != 0 {
        sort_html_projection_blocks(&mut output, work)?;
    }
    Ok(ProjectionFacts {
        blocks: output.freeze(work)?,
        text,
        key: native.source.input().source().key(),
        length: native.source.input().source().text().len(),
        budget: native.source.input().source().budget().clone(),
    })
}

fn accepts_bibliography<'view, 'arena, 'input, 'id>(
    native: &AstNative<'_, 'view, 'arena, 'input, '_, 'id>,
    root: n::NodeToken<'view, 'arena, 'input>,
    work: &mut OwnedWork<'id, '_>,
) -> Result<bool, CoreError> {
    use native::NativeKind;
    let mut last = native.read(work, |a| a.node(root))?.last_child;
    let (token, row) = loop {
        let Some(token) = last else { return Ok(true) };
        let row = native.read(work, |a| a.node(token))?;
        if row.kind != NativeKind::FootnoteDefinition {
            break (token, row);
        }
        last = row.previous_sibling;
    };
    if !matches!(row.kind, NativeKind::CodeBlock | NativeKind::HtmlBlock) {
        return Ok(true);
    }
    let lineage = native.read(work, |a| a.lineage(token))?;
    let binding = native.read(work, |a| a.binding(lineage))?;
    if row.kind == NativeKind::CodeBlock {
        // An unindented new block ends indented code, but an unclosed fence
        // takes every subsequent line, including the generated bibliography.
        return Ok(
            !matches!(row.attributes, n::NodeAttributes::Code { fenced: true, .. })
                || binding.fence_closing.is_some(),
        );
    }
    let start = native.source.p_anchor(binding.start_p.unwrap_or(0), work)?;
    let source = native.source.input().source().text();
    let bytes = source
        .as_bytes()
        .get(start as usize..)
        .ok_or_else(|| work.fail(CoreError::InvalidProvenance))?;
    let start = bytes.iter().take(3).take_while(|&&b| b == b' ').count();
    let bytes = &bytes[start..];
    // CommonMark HTML blocks 1–5 need their closing token. Types 6 and 7
    // stop at the blank lines that separate the appended section.
    const TAGS: &[&[u8]] = &[b"script", b"pre", b"textarea", b"style"];
    let ends: &[&[u8]] = if bytes.starts_with(b"<!--") {
        &[b"-->"]
    } else if bytes.starts_with(b"<?") {
        &[b"?>"]
    } else if bytes.starts_with(b"<![CDATA[") {
        &[b"]]>"]
    } else if bytes.starts_with(b"<!") && bytes.get(2).is_some_and(u8::is_ascii_alphabetic) {
        &[b">"]
    } else if let Some((name, false, _)) =
        crate::ir::inline_tag_prefix(&bytes[..bytes.len().min(16)])
        && TAGS.iter().any(|tag| name.eq_ignore_ascii_case(tag))
    {
        &[b"</script>", b"</pre>", b"</textarea>", b"</style>"]
    } else {
        return Ok(true);
    };
    let overlap = ends.iter().map(|token| token.len()).max().unwrap() - 1;
    for start in (0..bytes.len()).step_by(4096) {
        let window = &bytes[start..(start + 4096 + overlap).min(bytes.len())];
        work.scan(window.len() as u64)?;
        if ends.iter().any(|token| {
            window
                .windows(token.len())
                .any(|candidate| candidate.eq_ignore_ascii_case(token))
        }) {
            return Ok(true);
        }
    }
    Ok(false)
}

fn collect_projection_text<'view, 'arena, 'input, 'id>(
    native: &AstNative<'_, 'view, 'arena, 'input, '_, 'id>,
    root: n::NodeToken<'view, 'arena, 'input>,
    citations: bool,
    directives: bool,
    notes: Option<&footnotes::NotePlan<'view, 'arena, 'input, 'id>>,
    work: &mut OwnedWork<'id, '_>,
) -> Result<FrozenRows<ProjectionText>, CoreError> {
    use native::NativeKind;
    let mut text = BuildGrowingVec::new(
        work.require_model_limits()?.model_records,
        LimitKind::ModelRecords,
        work,
    )?;
    let input = native.source.input().source().text();
    let mut current = root;
    let mut scope = 0;
    let mut depth = 0;
    let mut root_text = false;
    let mut opaque = inline_html_regions::OpaqueTags::default();
    'tree: loop {
        let mut row = native.read(work, |a| a.node(current))?;
        if depth == 1 {
            root_text =
                directives && matches!(row.kind, NativeKind::Paragraph | NativeKind::Heading);
        }
        if matches!(
            row.kind,
            NativeKind::Paragraph
                | NativeKind::Heading
                | NativeKind::TableCell
                | NativeKind::DescriptionTerm
                | NativeKind::FootnoteDefinition
        ) {
            scope = row.id.get();
            opaque = inline_html_regions::OpaqueTags::default();
        }
        if row.kind == NativeKind::HtmlInline {
            if let Some(span) = inline_html_regions::tag_span(native, current, work)? {
                let raw = native.html_source(span, work)?;
                opaque.observe(raw, false);
            } else {
                opaque.unknown();
            }
        }
        if row.kind == NativeKind::Text && !opaque.active() {
            let lineage = native.read(work, |a| a.lineage(current))?;
            let binding = native.read(work, |a| a.binding(lineage))?;
            let count = native.read(work, |a| a.mapping_len(binding.value))?;
            for index in 0..count {
                let atom = native.read(work, |a| a.mapping_atom(binding.value, index))?;
                if atom.proof.is_some() || atom.hidden_syntax {
                    continue;
                }
                native.visit_sources(atom.source, work, |source, work| {
                    let start = source.start() as usize;
                    let end = source.end() as usize;
                    let bytes = input
                        .as_bytes()
                        .get(start..end)
                        .ok_or_else(|| work.fail(CoreError::InvalidProvenance))?;
                    let mut citation = false;
                    let mut directive = false;
                    for start in (0..bytes.len()).step_by(4096) {
                        let end = (start + 4096).min(bytes.len());
                        let overlap = if directives && !directive {
                            (end + 2).min(bytes.len())
                        } else {
                            end
                        };
                        work.scan((overlap - start) as u64)?;
                        citation |= citations && !citation && bytes[start..end].contains(&b'@');
                        directive |= directives
                            && !directive
                            && bytes[start..overlap]
                                .windows(3)
                                .any(|value| value == b":::");
                        if (!citations || citation) && (!directives || directive) {
                            break;
                        }
                    }
                    if citation || directive {
                        text.push(
                            ProjectionText {
                                span: source,
                                scope,
                                citation,
                                directive,
                                root_text,
                            },
                            work,
                        )?;
                    }
                    Ok(())
                })?;
            }
        }
        let hidden_note = match notes {
            Some(notes) if row.kind == NativeKind::FootnoteDefinition => {
                !notes.is_visible_definition(row.id.get(), work)?
            }
            _ => false,
        };
        if !hidden_note
            && !matches!(
                row.kind,
                NativeKind::Link | NativeKind::Image | NativeKind::NoteEmbed | NativeKind::Escaped
            )
            && let Some(child) = row.first_child
        {
            current = child;
            depth += 1;
            continue;
        }
        loop {
            if row.parent.is_none() {
                break 'tree;
            }
            if let Some(sibling) = row.next_sibling {
                current = sibling;
                break;
            }
            current = row.parent.unwrap();
            depth -= 1;
            row = native.read(work, |a| a.node(current))?;
        }
    }
    crate::memory::sort_build_rows_by(
        &mut text,
        0,
        |a, b, _| Ok(a.span.start().cmp(&b.span.start())),
        work,
    )?;
    text.freeze(work)
}

fn sort_html_projection_blocks<'id>(
    rows: &mut BuildGrowingVec<'id, ProjectionBlock>,
    work: &mut OwnedWork<'id, '_>,
) -> Result<(), CoreError> {
    crate::memory::sort_build_rows_by(
        rows,
        0,
        |a, b, _| {
            Ok(a.source
                .start()
                .cmp(&b.source.start())
                .then(b.source.end().cmp(&a.source.end())))
        },
        work,
    )
}

fn contains_paragraph_marker(text: &str, work: &mut OwnedWork<'_, '_>) -> Result<bool, CoreError> {
    const WINDOW: usize = 4096;
    const PREFIX: &[u8] = b"<label";
    work.checkpoint_now()?;
    let bytes = text.as_bytes();
    for start in (0..bytes.len()).step_by(WINDOW) {
        // Five overlap bytes cover a prefix split by a window or UTF-8 scalar.
        let end = (start + WINDOW + PREFIX.len() - 1).min(bytes.len());
        let window = &bytes[start..end];
        work.scan(window.len() as u64)?;
        if window
            .windows(PREFIX.len())
            .any(|part| part.eq_ignore_ascii_case(PREFIX))
            || window
                .windows(3)
                .any(|part| part[..2].eq_ignore_ascii_case(b"<a") && part[2].is_ascii_whitespace())
        {
            return Ok(true);
        }
    }
    Ok(false)
}

#[cfg(test)]
mod label_probe_tests {
    use super::*;
    use crate::policy::{CancellationToken, SourceBudget, construction_scope};

    #[test]
    fn conservative_label_probe_covers_case_tag_space_and_utf8_window_seams() {
        for prefix in [
            "<label>",
            "<LaBeL class=x>",
            "<LABEL\t>",
            "<label\x0b>",
            "<label\x0c>",
            "<label\r\n>",
            "<label/>",
        ] {
            for offset in [0, 4093, 4095, 4096, 8191] {
                let text = format!("{}😀{prefix}中", "a".repeat(offset));
                let budget = SourceBudget::preview();
                let mut op = budget.begin(CancellationToken::new());
                construction_scope(&mut op, |mut work| {
                    assert!(contains_paragraph_marker(&text, &mut work).unwrap());
                    work.finish().unwrap();
                });
            }
        }
        for text in [
            "ordinary < y",
            "<div>HTML remains native</div>",
            "<la bel>",
            "<labe>",
        ] {
            let budget = SourceBudget::preview();
            let mut op = budget.begin(CancellationToken::new());
            construction_scope(&mut op, |mut work| {
                assert!(!contains_paragraph_marker(text, &mut work).unwrap());
                work.finish().unwrap();
            });
        }
    }
}
