// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

//! Actual API composition check, not the complete semantic compiler. This
//! exercises one real text value while the real target guard owns Work. No
//! incomplete model is published or passed off as independently validated.

use super::prepared::with_prepared;
use crate::error::{CoreError, LimitKind};
use crate::identity::{LocalUtf8Span, SourceKey};
use crate::ir::{AtomKind, ClaimKind, HostRole, NativePhase, TargetSpan, direct as b};
use crate::memory::BuildGrowingVec;
use crate::policy::{CancellationToken, SourceBudget, construction_scope};
use crate::source::{ParseInput, SourceDocument};
use comrak::native::direct as n;

use super::options;

#[test]
fn actual_reader_target_and_utf8_chunk_callbacks_compose() {
    let text = format!("{}中😀{}", "a".repeat(254), "文😀".repeat(90));
    let budget = SourceBudget::model_fixture();
    let mut operation = budget.begin(CancellationToken::new());
    let source = SourceDocument::new(
        SourceKey::new([73; 16], 1).unwrap(),
        text.as_bytes(),
        &mut operation,
    )
    .unwrap()
    .snapshot();
    let input = ParseInput::prepare(source, &mut operation).unwrap();
    let (copied, chunks, written) = construction_scope(&mut operation, |mut work| {
        let result = with_prepared(&input, &mut work, |native_input, mut binding| {
            let result = n::parse(native_input, &options::c0(), &mut binding, |mut view| {
                view.with_reader(|reader, host| {
                    host.native_parts(|witness, work| {
                        let value = witness.with_reads(work, |reads| {
                            reader.with_reads(reads, |access| {
                                let root = access.root()?;
                                let paragraph = access
                                    .node(root)?
                                    .first_child
                                    .ok_or(CoreError::InvalidProvenance)?;
                                let text = access
                                    .node(paragraph)?
                                    .first_child
                                    .ok_or(CoreError::InvalidProvenance)?;
                                match access.node(text)?.attributes {
                                    n::NodeAttributes::Text(value) => Ok(value),
                                    _ => Err(CoreError::InvalidProvenance),
                                }
                            })
                        })?;
                        let mut builder = b::DirectBuilder::new(work)?;
                        let flow = builder.reserve_flow(work)?;
                        let owner = builder.reserve_node(
                            b::DraftNodeSeed {
                                parent: None,
                                flow,
                                kind: crate::ir::NodeKind::Text,
                                detail: None,
                                initial: None,
                            },
                            work,
                        )?;
                        let mut order = BuildGrowingVec::new(1024, LimitKind::ModelRecords, work)?;
                        let mut target = builder.begin_target(work)?;

                        // These native reads occur after begin_target, through its
                        // actual short Work reborrow, not before a cached recipe.
                        let (value_len, mapping, atom_count) = target.with_read(|_, work| {
                            witness.with_reads(work, |reads| {
                                reader.with_reads(reads, |access| {
                                    let len = access.value_len(value)?;
                                    let mapping = access
                                        .value_mapping(value)?
                                        .ok_or(CoreError::InvalidProvenance)?;
                                    Ok((len, mapping, access.mapping_len(mapping)?))
                                })
                            })
                        })?;
                        let mut cursor = Some(target.with_read(|_, work| {
                            witness.with_reads(work, |reads| {
                                reader.with_reads(reads, |access| {
                                    access.value_cursor(value, 0, value_len)
                                })
                            })
                        })?);
                        let mut chunks = 0;
                        let copied_value = target.text_from_chunks(value_len, |offset, work| {
                            let current = cursor.take().ok_or(CoreError::InvalidProvenance)?;
                            if current.position() != offset {
                                return Err(CoreError::InvalidProvenance);
                            }
                            let (chunk, next) = witness.with_reads(work, |reads| {
                                reader.with_reads(reads, |access| {
                                    access.with_next_value_chunk(current, 256, |text, reads| {
                                        chunks += 1;
                                        reads.text_chunk(text)
                                    })
                                })
                            })?;
                            cursor = Some(next);
                            chunk.ok_or(CoreError::InvalidProvenance)
                        })?;
                        let mut end = 0_u64;
                        for index in 0..atom_count {
                            let atom = target.with_read(|_, work| {
                                witness.with_reads(work, |reads| {
                                    reader.with_reads(reads, |access| {
                                        access.mapping_atom(mapping, index)
                                    })
                                })
                            })?;
                            // This finite fixture is a real plain value. Full
                            // transform proofs belong to the forthcoming compiler.
                            if atom.proof.is_some() {
                                return Err(CoreError::InvalidProvenance);
                            }
                            let mut cursor = target.with_read(|_, work| {
                                witness.with_reads(work, |reads| {
                                    reader.with_reads(reads, |access| {
                                        access.source_cursor(atom.source)
                                    })
                                })
                            })?;
                            let mut source = None;
                            while let Some(piece) = target.with_read(|_, work| {
                                witness.with_reads(work, |reads| {
                                    reader
                                        .with_reads(reads, |access| access.next_source(&mut cursor))
                                })
                            })? {
                                target.with_read(|_, work| {
                                    witness.p_sources(piece.p, work, |part, _| {
                                        if source.replace(part).is_some() {
                                            return Err(CoreError::InvalidProvenance);
                                        }
                                        Ok(())
                                    })
                                })?;
                            }
                            let source = source.ok_or(CoreError::InvalidProvenance)?;
                            let sources = target
                                .with_rows(|builder, work| builder.source_parts(&[source], work))?;
                            let ticket = target.reserve_next()?;
                            let start = end;
                            end = end
                                .checked_add(atom.bytes as u64)
                                .ok_or(CoreError::ArithmeticOverflow)?;
                            let born = target.commit_owned(
                                ticket,
                                b::DraftOwnedAtom {
                                    owner,
                                    kind: AtomKind::Copy,
                                    target: TargetSpan { start, end },
                                    sources,
                                    proof: None,
                                    content: b::DraftAtomContent::Empty,
                                    primary: b::DraftPrimaryAuthority {
                                        phase: NativePhase::Inline,
                                        kind: ClaimKind::Block,
                                        consumed: LocalUtf8Span::new(start, end)?,
                                        host_role: HostRole::Inline,
                                        host_utf8_len: value_len as u64,
                                        host_domains: sources,
                                    },
                                },
                            )?;
                            target.with_read(|_, work| order.push(born, work))?;
                        }
                        let closed = target.close(order)?;
                        let _written_block = closed.into_block(work)?;
                        let first_chunk = builder.text_chunk(copied_value, 0, work)?;
                        if first_chunk.len() != 254 {
                            return Err(CoreError::InvalidProvenance);
                        }
                        // The unfinished graph is deliberately dropped. This test
                        // proves composition, not graph/proof acceptance.
                        drop(builder);
                        Ok((value_len, chunks, atom_count))
                    })
                })
            });
            binding.resolve(result)
        })?;
        work.finish()?;
        Ok::<_, CoreError>(result)
    })
    .unwrap();
    assert_eq!(copied, text.len());
    assert!(chunks >= 3);
    assert!(written > 0);
}
