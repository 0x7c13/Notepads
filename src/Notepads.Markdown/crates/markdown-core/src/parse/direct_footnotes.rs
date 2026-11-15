// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

use super::*;
use crate::memory::BuildValue;

#[derive(Clone, Copy)]
struct Definition<'view, 'arena, 'input> {
    token: n::NodeToken<'view, 'arena, 'input>,
    id: u32,
    ordinal: u32,
    occurrences: u32,
    first: Option<u32>,
    last: Option<u32>,
}

impl BuildValue for Definition<'_, '_, '_> {}

#[derive(Clone, Copy)]
pub(super) struct Reference<'view, 'input> {
    pub lineage: n::LineageToken<'view, 'input>,
    pub occurrence: u32,
    pub next: Option<u32>,
}

impl BuildValue for Reference<'_, '_> {}

#[derive(Clone, Copy)]
pub(super) enum Note {
    Definition { ordinal: u32 },
    Reference { ordinal: u32, occurrence: u32 },
}

impl BuildValue for Note {}

/// Keep the native tree in authored order and give Core an independent reading order.
pub(super) struct NotePlan<'view, 'arena, 'input, 'id> {
    definitions: BuildRows<'id, Definition<'view, 'arena, 'input>>,
    order: BuildRows<'id, u32>,
    references: BuildRows<'id, Reference<'view, 'input>>,
}

impl<'view, 'arena, 'input, 'id> NotePlan<'view, 'arena, 'input, 'id> {
    pub(super) fn prepare(
        native: &AstNative<'_, 'view, 'arena, 'input, '_, 'id>,
        root: n::NodeToken<'view, 'arena, 'input>,
        work: &mut OwnedWork<'id, '_>,
    ) -> Result<Option<Self>, CoreError> {
        let limit = work.require_model_limits()?.model_records;
        let mut plan = Self {
            definitions: BuildRows::new(limit, LimitKind::ModelRecords, work)?,
            order: BuildRows::new(limit, LimitKind::ModelRecords, work)?,
            references: BuildRows::new(limit, LimitKind::ModelRecords, work)?,
        };
        let first = native.read(work, |access| access.node(root))?.first_child;
        let mut child = first;
        let mut last = 0;
        while let Some(token) = child {
            let row = native.read(work, |access| access.node(token))?;
            child = row.next_sibling;
            if row.kind == native::NativeKind::FootnoteDefinition {
                let id = row.id.get();
                if id <= last {
                    return Err(work.fail(CoreError::InvalidProvenance));
                }
                last = id;
                plan.definitions.push(
                    Definition {
                        token,
                        id,
                        ordinal: 0,
                        occurrences: 0,
                        first: None,
                        last: None,
                    },
                    work,
                )?;
            }
        }
        if plan.definitions.len() == 0 {
            return Ok(None);
        }

        child = first;
        while let Some(token) = child {
            let row = native.read(work, |access| access.node(token))?;
            child = row.next_sibling;
            if row.kind != native::NativeKind::FootnoteDefinition {
                plan.visit(native, row, work)?;
            }
        }
        let mut index = 0;
        while let Some(token) = plan.at(index, work)? {
            let row = native.read(work, |access| access.node(token))?;
            plan.visit(native, row, work)?;
            index += 1;
        }
        Ok(Some(plan))
    }

    pub(super) fn at(
        &self,
        index: usize,
        work: &mut OwnedWork<'id, '_>,
    ) -> Result<Option<n::NodeToken<'view, 'arena, 'input>>, CoreError> {
        if index >= self.order.len() {
            return Ok(None);
        }
        let definition = self.order.get(index, work)? as usize;
        Ok(Some(self.definitions.get(definition, work)?.token))
    }

    pub(super) fn has_visible_definitions(&self) -> bool {
        self.order.len() != 0
    }

    pub(super) fn is_visible_definition(
        &self,
        id: u32,
        work: &mut OwnedWork<'id, '_>,
    ) -> Result<bool, CoreError> {
        let index = self.definition_index(id, work)?;
        Ok(self.definitions.get(index, work)?.ordinal != 0)
    }

    fn include(
        &mut self,
        id: u32,
        reference: n::LineageToken<'view, 'input>,
        work: &mut OwnedWork<'id, '_>,
    ) -> Result<(), CoreError> {
        let index = self.definition_index(id, work)?;
        let mut definition = self.definitions.get(index, work)?;
        if definition.ordinal == 0 {
            let offset =
                u32::try_from(index).map_err(|_| work.fail(CoreError::ArithmeticOverflow))?;
            definition.ordinal = u32::try_from(self.order.len())
                .ok()
                .and_then(|length| length.checked_add(1))
                .ok_or_else(|| work.fail(CoreError::ArithmeticOverflow))?;
            self.order.push(offset, work)?;
        }
        let last = definition
            .last
            .map(|last| self.references.get(last as usize, work))
            .transpose()?;
        let occurrence = last
            .map_or(Some(1), |row| row.occurrence.checked_add(1))
            .ok_or_else(|| work.fail(CoreError::ArithmeticOverflow))?;
        let at = self.references.push(
            Reference {
                lineage: reference,
                occurrence,
                next: None,
            },
            work,
        )?;
        let at = u32::try_from(at).map_err(|_| work.fail(CoreError::ArithmeticOverflow))?;
        if let Some(mut last) = last {
            last.next = Some(at);
            self.references
                .replace(definition.last.unwrap() as usize, last, work)?;
        } else {
            definition.first = Some(at);
        }
        definition.last = Some(at);
        self.definitions.replace(index, definition, work)?;
        Ok(())
    }

    pub(super) fn first_reference(
        &self,
        id: u32,
        work: &mut OwnedWork<'id, '_>,
    ) -> Result<(u32, u32), CoreError> {
        let index = self.definition_index(id, work)?;
        let definition = self.definitions.get(index, work)?;
        Ok((
            definition.first.ok_or(CoreError::InvalidProvenance)?,
            definition.ordinal,
        ))
    }

    pub(super) fn reference(
        &self,
        index: u32,
        work: &mut OwnedWork<'id, '_>,
    ) -> Result<Reference<'view, 'input>, CoreError> {
        self.references.get(index as usize, work)
    }

    fn definition_index(&self, id: u32, work: &mut OwnedWork<'id, '_>) -> Result<usize, CoreError> {
        let mut start = 0;
        let mut end = self.definitions.len();
        while start < end {
            let mid = start + (end - start) / 2;
            if self.definitions.get(mid, work)?.id < id {
                start = mid + 1;
            } else {
                end = mid;
            }
        }
        if start == self.definitions.len() || self.definitions.get(start, work)?.id != id {
            return Err(work.fail(CoreError::InvalidProvenance));
        }
        Ok(start)
    }

    pub(super) fn metadata(
        &mut self,
        native: &AstNative<'_, 'view, 'arena, 'input, '_, 'id>,
        kind: native::NativeKind,
        binding: n::BindingInfo<'view, 'input>,
        work: &mut OwnedWork<'id, '_>,
    ) -> Result<Option<Note>, CoreError> {
        let note = match kind {
            native::NativeKind::FootnoteDefinition => {
                let index = self.definition_index(binding.id.get(), work)?;
                let ordinal = self.definitions.get(index, work)?.ordinal;
                if ordinal == 0 {
                    return Err(work.fail(CoreError::InvalidProvenance));
                }
                Note::Definition { ordinal }
            }
            native::NativeKind::FootnoteReference => {
                let definition = binding
                    .footnote_definition
                    .ok_or(CoreError::InvalidProvenance)?;
                let owner = native.read(work, |reader| reader.binding(definition))?.id;
                let index = self.definition_index(owner.get(), work)?;
                let mut entry = self.definitions.get(index, work)?;
                if entry.ordinal == 0 {
                    return Err(work.fail(CoreError::InvalidProvenance));
                }
                entry.occurrences = entry
                    .occurrences
                    .checked_add(1)
                    .ok_or_else(|| work.fail(CoreError::ArithmeticOverflow))?;
                self.definitions.replace(index, entry, work)?;
                Note::Reference {
                    ordinal: entry.ordinal,
                    occurrence: entry.occurrences,
                }
            }
            _ => return Ok(None),
        };
        Ok(Some(note))
    }

    fn visit(
        &mut self,
        native: &AstNative<'_, 'view, 'arena, 'input, '_, 'id>,
        root: n::NodeInfo<'view, 'arena, 'input>,
        work: &mut OwnedWork<'id, '_>,
    ) -> Result<(), CoreError> {
        let Some(mut next) = root.first_child else {
            return Ok(());
        };
        loop {
            let mut row = native.read(work, |access| access.node(next))?;
            if row.kind == native::NativeKind::FootnoteReference {
                let (definition, lineage) = native.read(work, |access| {
                    let lineage = access.lineage(next)?;
                    let binding = access.binding(lineage)?;
                    let definition = binding
                        .footnote_definition
                        .ok_or(CoreError::InvalidProvenance)?;
                    Ok((access.binding(definition)?.id.get(), lineage))
                })?;
                self.include(definition, lineage, work)?;
            }
            if row.kind != native::NativeKind::Image
                && let Some(child) = row.first_child
            {
                next = child;
                continue;
            }
            loop {
                if let Some(sibling) = row.next_sibling {
                    next = sibling;
                    break;
                }
                let parent = row.parent.ok_or(CoreError::InvalidProvenance)?;
                row = native.read(work, |access| access.node(parent))?;
                if row.id == root.id {
                    return Ok(());
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::SourceKey;
    use crate::policy::{CancellationToken, SourceBudget, construction_scope};
    use crate::source::SourceDocument;

    struct CheckReadingOrder;

    impl<'input, 'source, 'work, 'id, 'op>
        n::DirectCompiler<'input, PreparedBinding<'input, 'source, 'work, 'id, 'op>>
        for CheckReadingOrder
    {
        type Output = ();
        type State<'view> = ();

        fn ast<'store, 'view, 'arena>(
            &mut self,
            reader: n::AstReader<
                'store,
                'view,
                'arena,
                'input,
                ConstructionId<'id>,
                NativeLease<'id>,
            >,
            host: &mut PreparedBinding<'input, 'source, 'work, 'id, 'op>,
        ) -> Result<(), NativeFault<'input, 'id>> {
            host.native_parts(|source, work| {
                let native = AstNative::new(reader, source);
                let config = ParseConfig::with_extensions(Profile::G3, Extensions::FOOTNOTES);
                let state = compile_ast_for_config(&native, config, work)?;
                let graph = &state.graph;
                let builder = &state.builder;
                let body = graph.first(0, work)?.unwrap();
                assert_eq!(
                    builder
                        .graph_node(graph.nodes.get(body, work)?.node, work)?
                        .kind,
                    NodeKind::Paragraph
                );
                let mut next = graph.next(body, builder, work)?;
                for expected in ["[^b]:", "[^c]:", "[^a]:"] {
                    let index = next.unwrap();
                    let member = graph.nodes.get(index, work)?;
                    assert_eq!(
                        builder.graph_node(member.node, work)?.kind,
                        NodeKind::FootnoteDefinition
                    );
                    let binding = native.read(work, |a| a.binding(member.lineage))?;
                    let mut text = String::new();
                    native.visit_sources(binding.marker.unwrap(), work, |span, _| {
                        let input = native.source.input().source().text();
                        text.push_str(&input[span.start() as usize..span.end() as usize]);
                        Ok(())
                    })?;
                    assert_eq!(text, expected);
                    next = graph.next(index, builder, work)?;
                }
                assert!(next.is_none());
                Ok(())
            })
        }

        fn provenance<'store, 'view>(
            &mut self,
            _: n::ProvenanceReader<'store, 'view, 'input, ConstructionId<'id>, NativeLease<'id>>,
            (): Self::State<'view>,
            _: &mut PreparedBinding<'input, 'source, 'work, 'id, 'op>,
        ) -> Result<(), NativeFault<'input, 'id>> {
            Ok(())
        }
    }

    #[test]
    fn body_references_then_transitive_notes_determine_visited_definition_order() {
        let source = "[^c]: C points to [^a].\n\n[^unused]: Invisible.\n\n\
            First [^b], then [^c].\n\n[^a]: A returns to [^b].\n\n[^b]: B repeats [^c].\n";
        let budget = SourceBudget::preview();
        let mut op = budget.begin(CancellationToken::new());
        let original = SourceDocument::new(
            SourceKey::new([111; 16], 1).unwrap(),
            source.as_bytes(),
            &mut op,
        )
        .unwrap()
        .snapshot();
        construction_scope(&mut op, |mut work| {
            let prepared = ParseInput::prepare_owned(original, &mut work)?;
            let (options, policy) = native_configuration(ParseConfig::with_extensions(
                Profile::G3,
                Extensions::FOOTNOTES,
            ));
            with_prepared(&prepared, &mut work, |input, mut host| {
                let result = n::compile_with_inline_policy(
                    input,
                    &options,
                    &mut host,
                    CheckReadingOrder,
                    policy,
                );
                host.resolve(result)
            })?;
            work.finish()
        })
        .unwrap();
    }

    #[test]
    fn accepted_note_numbers_and_bodies_share_read_order_with_original_mapping() {
        let source = "[^a]: A body.\r\n\r\nFirst [^b], repeat [^b], then [^a].\r\n\r\n\
            [^unused]: Hidden.\r\n[^b]: B body.\r\n";
        let budget = SourceBudget::preview();
        let mut op = budget.begin(CancellationToken::new());
        let original = SourceDocument::new(
            SourceKey::new([112; 16], 1).unwrap(),
            source.as_bytes(),
            &mut op,
        )
        .unwrap()
        .snapshot();
        let mut progress = crate::model::ParseProgress::default();
        let model = crate::model::AcceptedModel::experimental_parse_with_progress(
            original,
            ParseConfig::with_extensions(Profile::G3, Extensions::FOOTNOTES),
            1,
            &mut op,
            &mut progress,
        )
        .unwrap_or_else(|error| panic!("{error:?} at {:?}", progress.stage()));
        let flow = model.main_flow_id();
        let end = model.ir().flows()[flow.index()].logical_utf16;
        let mut visible = String::new();
        crate::reader::text::visit(
            &model,
            flow,
            LogicalSpan::new(0, end).unwrap(),
            &mut op,
            |text, _| {
                visible.push_str(text);
                Ok(())
            },
        )
        .unwrap();
        assert_eq!(
            visible,
            "First 1, repeat 1, then 2.\n1.\nB body.\n↩ ↩2\n2.\nA body.\n↩"
        );
        assert_eq!(model.original_source().text(), source);
        for node in model
            .ir()
            .nodes()
            .iter()
            .filter(|row| row.kind == NodeKind::FootnoteReference)
        {
            let style = model
                .inline_script_style(node.logical.start(), &mut op)
                .unwrap();
            assert!(style.size_scale < 1.0 && style.relative_em < 0.0);
        }
        let byte = source.find("A body.").unwrap() as u64;
        let query = crate::mapping::PointQuery {
            model: model.identity(),
            input: crate::mapping::PointInput::Source {
                target: TextTarget::Flow(flow),
                byte: crate::identity::SourceBoundary::new(byte),
            },
            affinity: crate::mapping::Affinity::After,
        };
        let response = model.map_points(&[query], &mut op).unwrap();
        let crate::mapping::Position::Flow(paragraph) = response.results()[0].chosen else {
            unreachable!();
        };
        let position = |needle| {
            visible[..visible.find(needle).unwrap()]
                .encode_utf16()
                .count() as u64
        };
        assert_eq!(paragraph.get(), position("A body."));
        let marker_input = crate::mapping::PointInput::Source {
            target: TextTarget::Flow(flow),
            byte: crate::identity::SourceBoundary::new(0),
        };
        for affinity in [
            crate::mapping::Affinity::Before,
            crate::mapping::Affinity::After,
        ] {
            let response = model
                .map_points(
                    &[crate::mapping::PointQuery {
                        model: model.identity(),
                        input: marker_input,
                        affinity,
                    }],
                    &mut op,
                )
                .unwrap();
            let crate::mapping::Position::Flow(marker) = response.results()[0].chosen else {
                unreachable!();
            };
            assert!((position("\n2.\n")..=paragraph.get()).contains(&marker.get()));
        }

        use crate::layout::{LayoutDraft, LayoutKey, LayoutStyle};
        use crate::presentation::PresentationBatch;
        let key = LayoutKey::new(
            model.identity(),
            1,
            1,
            1,
            720.0,
            LayoutStyle::markdown(16.0, 8.0).unwrap(),
        )
        .unwrap();
        let mut layout = LayoutDraft::new(model.clone(), key, &mut op).unwrap();
        let measurements = layout.measurement_batch(256, &mut op).unwrap().unwrap();
        let batch = PresentationBatch::build(model, &measurements, &mut op).unwrap();
        assert_eq!(batch.blocks().len(), 7);
        let note1 = position("\n1.\n") + 1;
        let note2 = position("\n2.\n") + 1;
        let links: Vec<_> = batch
            .links()
            .iter()
            .map(|link| {
                let start = link.title.start as usize;
                let end = start + link.title.len as usize;
                assert_eq!(link.destination.len, 0);
                (
                    link.local_position,
                    std::str::from_utf8(&batch.blob()[start..end]).unwrap(),
                )
            })
            .collect();
        assert_eq!(
            links,
            [
                (Some(note1), "Footnote 1"),
                (Some(note1), "Footnote 1"),
                (Some(note2), "Footnote 2"),
                (
                    Some(position("1, repeat")),
                    "Back to reference 1 of footnote 1"
                ),
                (
                    Some(position("1, then")),
                    "Back to reference 2 of footnote 1"
                ),
                (
                    Some(position("2.\n1.")),
                    "Back to reference 1 of footnote 2"
                ),
            ]
        );
        assert_eq!(batch.script_runs().len(), 3);
    }

    #[test]
    fn inline_notes_move_only_the_body_and_share_reference_order() {
        let source =
            "First^[outer ^[inner] and [^named]]. Tail [^named].\r\n\r\n[^named]: Named body.\r\n";
        let budget = SourceBudget::preview();
        let mut op = budget.begin(CancellationToken::new());
        let original = SourceDocument::new(
            SourceKey::new([114; 16], 1).unwrap(),
            source.as_bytes(),
            &mut op,
        )
        .unwrap()
        .snapshot();
        let mut progress = crate::model::ParseProgress::default();
        let model = crate::model::AcceptedModel::experimental_parse_with_progress(
            original,
            ParseConfig::with_extensions(Profile::G3, Extensions::FOOTNOTES),
            1,
            &mut op,
            &mut progress,
        )
        .unwrap_or_else(|error| panic!("{error:?} at {:?}", progress.stage()));
        assert_eq!(model.original_source().text(), source);
        let flow = model.main_flow_id();
        let length = model.ir().flows()[flow.index()].logical_utf16;
        let mut text = String::new();
        crate::reader::text::visit(
            &model,
            flow,
            LogicalSpan::new(0, length).unwrap(),
            &mut op,
            |piece, _| {
                text.push_str(piece);
                Ok(())
            },
        )
        .unwrap();
        assert_eq!(
            text,
            "First1. Tail 2.\n1.\nouter 3 and 2\n↩\n2.\nNamed body.\n↩ ↩2\n3.\ninner\n↩"
        );
        for (raw, shown) in [
            ("outer", "outer"),
            ("inner", "inner"),
            ("^[", "1. Tail"),
            ("]. Tail", "1. Tail"),
        ] {
            let mut offset = source.find(raw).unwrap() as u64;
            if raw.starts_with(']') {
                offset += 1;
            }
            let result = model
                .map_points(
                    &[crate::mapping::PointQuery {
                        model: model.identity(),
                        input: crate::mapping::PointInput::Source {
                            target: TextTarget::Flow(flow),
                            byte: crate::identity::SourceBoundary::new(offset),
                        },
                        affinity: crate::mapping::Affinity::After,
                    }],
                    &mut op,
                )
                .unwrap();
            let crate::mapping::Position::Flow(mapped) = result.results()[0].chosen else {
                unreachable!()
            };
            let start = text.find(shown).unwrap();
            let expected = text[..start].encode_utf16().count() as u64;
            assert!(
                (expected..=expected + u64::from(shown == "1. Tail")).contains(&mapped.get()),
                "{raw}: {}",
                mapped.get()
            );
        }
    }

    #[test]
    fn unused_footnote_with_a_lazy_continuation_keeps_the_document_available() {
        for (profile, newline) in [(Profile::C0, "\n"), (Profile::G3, "\r\n")] {
            let mut op = SourceBudget::preview().begin(CancellationToken::new());
            let source = format!("[^n]: a{newline}b{newline}c{newline}{newline}Visible prose.");
            let document = SourceDocument::new(
                SourceKey::new([132; 16], 1).unwrap(),
                source.as_bytes(),
                &mut op,
            )
            .unwrap()
            .snapshot();
            let mut progress = crate::model::ParseProgress::default();
            let model = crate::model::AcceptedModel::experimental_parse_with_progress(
                document,
                ParseConfig::with_extensions(profile, Extensions::FOOTNOTES),
                1,
                &mut op,
                &mut progress,
            )
            .unwrap_or_else(|error| panic!("{error:?} at {:?}", progress.stage()));
            let mut reader = crate::reader::MainReader::new(model, op).unwrap();
            let mut output = [0; 128];
            let read = reader.read_into(&mut output).unwrap();
            assert!(read.done);
            assert_eq!(&output[..read.written], b"Visible prose.");
        }
    }

    #[test]
    fn unused_footnote_bodies_keep_hidden_source_without_reading_nodes() {
        for (body, kind) in [
            (" $x$", NodeKind::Math),
            (" [TOC]", NodeKind::TableOfContents),
            ("\n    Term\n\n    : Definition", NodeKind::DescriptionList),
        ] {
            for reference in ["", "[^n]"] {
                let source = format!("# Visible\n\nProse.{reference}\n\n[^n]:{body}");
                let mut op = SourceBudget::preview().begin(CancellationToken::new());
                let document = SourceDocument::new(
                    SourceKey::new([132; 16], 1).unwrap(),
                    source.as_bytes(),
                    &mut op,
                )
                .unwrap()
                .snapshot();
                let model = crate::model::AcceptedModel::parse(
                    document,
                    ParseConfig::with_extensions(Profile::G3, Extensions::ALL),
                    1,
                    &mut op,
                )
                .unwrap_or_else(|error| panic!("{source:?}: {error:?}"));
                assert_eq!(model.original_source().text(), source);
                assert_eq!(
                    model.ir().nodes().iter().any(|node| node.kind == kind),
                    !reference.is_empty(),
                    "{source}"
                );
                if reference.is_empty() {
                    let mut reader = crate::reader::MainReader::new(model, op).unwrap();
                    let mut output = [0; 128];
                    let read = reader.read_into(&mut output).unwrap();
                    assert!(read.done);
                    assert_eq!(&output[..read.written], b"Visible\nProse.");
                }
            }
        }
    }

    #[test]
    fn footnote_reference_alone_can_be_nested_inside_a_markdown_subscript() {
        for profile in [Profile::C0, Profile::G3] {
            let mut op = SourceBudget::preview().begin(CancellationToken::new());
            let source = "~[^n]~\n[^n]: a";
            let document = SourceDocument::new(
                SourceKey::new([133; 16], 1).unwrap(),
                source.as_bytes(),
                &mut op,
            )
            .unwrap()
            .snapshot();
            let extensions = Extensions::from_bits(
                Extensions::FOOTNOTES.bits() | Extensions::INLINE_SCRIPTS.bits(),
            )
            .unwrap();
            let mut progress = crate::model::ParseProgress::default();
            let model = crate::model::AcceptedModel::experimental_parse_with_progress(
                document,
                ParseConfig::with_extensions(profile, extensions),
                1,
                &mut op,
                &mut progress,
            )
            .unwrap_or_else(|error| panic!("{error:?} at {:?}", progress.stage()));
            assert_eq!(
                model.inline_script_style(0, &mut op).unwrap(),
                crate::presentation::ScriptStyle::default()
                    .nested(ScriptKind::Subscript)
                    .nested(ScriptKind::Superscript)
            );
            let mut reader = crate::reader::MainReader::new(model, op).unwrap();
            let mut output = [0; 128];
            let read = reader.read_into(&mut output).unwrap();
            assert!(read.done);
            assert_eq!(&output[..read.written], "1\n1.\na\n↩".as_bytes());
        }
    }

    #[test]
    fn footnote_footer_follows_the_final_envelope_of_a_descendant_table() {
        for source in ["[^n]\n[^n]:|\n|", "[^n]\n\n[^n]: a |\n|-|\n|b|"] {
            let budget = SourceBudget::preview();
            let mut op = budget.begin(CancellationToken::new());
            let document = SourceDocument::new(
                SourceKey::new([128; 16], 1).unwrap(),
                source.as_bytes(),
                &mut op,
            )
            .unwrap()
            .snapshot();
            let model = crate::model::AcceptedModel::parse(
                document,
                ParseConfig::with_extensions(Profile::G3, Extensions::FOOTNOTES),
                1,
                &mut op,
            )
            .unwrap();
            let nodes = model.ir().nodes();
            let end = source.len() as u64;
            assert!(
                nodes
                    .iter()
                    .any(|row| row.kind == NodeKind::FootnoteDefinition
                        && row.source_envelope.end() == end)
            );
            for kind in [NodeKind::FootnoteFooter, NodeKind::FootnoteBacklink] {
                let row = nodes.iter().find(|row| row.kind == kind).unwrap();
                assert_eq!(row.source_envelope, SourceSpan::new(end, end).unwrap());
            }
        }
    }

    #[test]
    fn relocated_note_resets_closed_html_endpoints_and_nests_superscript() {
        let source = "[^a]: Earlier *body* <b>bold</b>.\n\nBefore <sup>marker[^a]</sup> and <code>later</code>.\n";
        let budget = SourceBudget::preview();
        let mut op = budget.begin(CancellationToken::new());
        let original = SourceDocument::new(
            SourceKey::new([113; 16], 1).unwrap(),
            source.as_bytes(),
            &mut op,
        )
        .unwrap()
        .snapshot();
        let mut progress = crate::model::ParseProgress::default();
        let model = crate::model::AcceptedModel::experimental_parse_with_progress(
            original,
            ParseConfig::with_extensions(Profile::G3, Extensions::ALL),
            1,
            &mut op,
            &mut progress,
        )
        .unwrap_or_else(|error| panic!("{error:?} at {:?}", progress.stage()));
        let flow = model.main_flow_id();
        let length = model.ir().flows()[flow.index()].logical_utf16;
        let mut visible = String::new();
        crate::reader::text::visit(
            &model,
            flow,
            LogicalSpan::new(0, length).unwrap(),
            &mut op,
            |text, _| {
                visible.push_str(text);
                Ok(())
            },
        )
        .unwrap();
        assert_eq!(
            visible,
            "Before marker1 and later.\n1.\nEarlier body bold.\n↩"
        );
        let number = visible.find("marker1").unwrap() as u64 + 6;
        let outer = model.inline_script_style(number - 1, &mut op).unwrap();
        let nested = model.inline_script_style(number, &mut op).unwrap();
        assert!(nested.size_scale < outer.size_scale && nested.relative_em < outer.relative_em);
    }
}
