// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

use super::*;
use crate::config::Profile;
use crate::identity::SourceKey;
use crate::ir::NodeKind;
use crate::policy::{CancellationToken, ModelLimits, SourceLimits};

fn source(text: &str, budget: &SourceBudget) -> SourceSnapshot {
    SourceDocument::new(
        SourceKey::new([0x65; 16], 1).unwrap(),
        text.as_bytes(),
        &mut budget.begin(CancellationToken::new()),
    )
    .unwrap()
    .snapshot()
}

fn facts(
    source: SourceSnapshot,
    config: ParseConfig,
    op: &mut Operation,
) -> Result<crate::parse::ProjectionFacts, CoreError> {
    construction_scope(op, |mut work| {
        let input = ParseInput::prepare_owned(source, &mut work)?;
        let facts = crate::parse::projection_facts(&input, config, &mut work)?;
        drop(input);
        work.finish()?;
        Ok(facts)
    })
}

#[test]
fn prepared_projection_facts_equal_accepted_ir_for_every_eligible_block() {
    let cases = [
        "",
        "ordinary paragraph <span>x</span>\n\nother",
        "<div><p>中😀 &amp; **x**</p></div>\r\n\r\nafter",
        "before\r\n\r\n  <DIV class='x'>\r\n<p>😀\0中</p>\r\n</DIV>\r\n\r\nafter",
        "<label><input type=checkbox checked>中😀</label>\n\ntext",
        "  <LaBeL>before *bold* [link](dest) ![alt](image) after<input type=checkbox></LaBeL>\n",
        "<label>\n<input type=checkbox>\ntext\n</label>\n\n",
        "```html\n<div>fenced</div>\n```\n\n    <p>indented</p>\n\n> <div>nested</div>\n\n- <div>list</div>\n",
        "<!-- comment -->\n\n<?instruction ?>\n\n<!DOCTYPE html>\n\n<![CDATA[x]]>\n",
        "<script>\n<p>raw</p>\n</script>\n\ninline <i>italic</i>\n",
        "<label>![alt *x*](image 'title') [r] $x^2$ <code>a</code><input type=checkbox></label>\n\n[r]: <dest> 'title'\n",
        "a\\\nb  \r\nc\t尾\r\n\r\n<label> a &NotEqualTilde; \\* **b** <sup>😀</sup><input type=checkbox></label>\r\n",
        "| h | h |\n|---|---|\n| <label>no<input type=checkbox></label> | <p>x</p> |\n\n---\n",
        "[r]: /ref\n\n[r]\n\n<label><input type=checkbox>label [r]</label>\n",
        "\\<label><input type=checkbox>escaped opener</label>\n",
        "\\*<label><input type=checkbox>escaped punctuation</label>\n",
        "before\r\n\r\n<label>\\* **nested \\*emphasis\\*** \\_尾<input type=checkbox></label>\r\n",
        "before\n\n<label><code>\\*中😀</code><kbd>\\<x></kbd><samp>\\_tail</samp><input type=checkbox></label>\n",
        "before\n\n<label>![\\*escaped \\<label>](image)<input type=checkbox>中\0😀</label>\r\n",
        "  <LABEL\tclass=x>\\*中😀<input type=checkbox></LABEL>\r\n",
        "before\r\n\r\n<label><input type=checkbox disabled>\\* **nested \\*emphasis\\*** 中😀</label>\r\n",
        "<LaBeL><input TYPE=checkbox DISABLED><code>\\*中😀</code><kbd>\\<x></kbd><samp>\\_tail</samp></LaBeL>\r\n",
        "<label><input type=checkbox disabled><img src='a&amp;b' alt='\\*escaped'>中\0😀</label>\r\n",
    ];
    for profile in [Profile::C0, Profile::G3] {
        let config = ParseConfig::with_extensions(profile, Extensions::ALL);
        for text in cases {
            let budget = SourceBudget::preview();
            let source = source(text, &budget);
            let accepted = AcceptedModel::parse_once(
                source.clone(),
                config,
                1,
                &mut budget.begin(CancellationToken::new()),
                None,
            )
            .unwrap_or_else(|error| panic!("{profile:?}: {text:?}: {error:?}"));
            let facts = facts(
                source.clone(),
                config,
                &mut budget.begin(CancellationToken::new()),
            )
            .unwrap();
            let ir = accepted.ir();
            let root = ir.flows()[accepted.main_flow_id().index()].root;
            let mut next = ir.nodes()[root.index()].first_child;
            let mut expected = Vec::new();
            while let Some(id) = next {
                let node = ir.nodes()[id.index()];
                next = node.next_sibling;
                if matches!(node.kind, NodeKind::HtmlBlock | NodeKind::Paragraph) {
                    expected.push((node.kind, node.source_envelope));
                }
            }
            let actual: Vec<_> = facts.blocks().iter().map(|b| (b.kind, b.source)).collect();
            // The only omitted rows may be Paragraphs which the existing
            // lowerer's native-label prefix contract cannot admit. Every
            // retained row must still match the exact accepted-IR envelope,
            // order and kind; all native HtmlBlocks remain mandatory.
            let retained: Vec<_> = expected
                .iter()
                .copied()
                .filter(|row| actual.contains(row))
                .collect();
            assert_eq!(actual, retained, "{profile:?}: {text:?}");
            for (kind, span) in &expected {
                let raw = &text[span.start() as usize..span.end() as usize];
                let prefix = raw
                    .trim_start_matches([' ', '\t', '\r', '\n', '\x0c'])
                    .as_bytes();
                let label = crate::ir::inline_tag_prefix(&prefix[..prefix.len().min(7)])
                    .is_some_and(|(name, closing, _)| {
                        !closing && name.eq_ignore_ascii_case(b"label")
                    });
                if *kind == NodeKind::HtmlBlock || label {
                    assert!(
                        actual.contains(&(*kind, *span)),
                        "omitted {profile:?}: {text:?}"
                    );
                }
            }
            drop((facts, accepted, source));
            assert_eq!(budget.usage().source_capacity, 0);
            assert_eq!(budget.usage().auxiliary_capacity, 0);
            assert_eq!(budget.usage().source_owners, 0);
        }
    }
}

// A test-only copy of the previous orchestration uses actual accepted IR as
// the classification oracle. Projection lowering, independent validation and
// source mappings run through their real implementations on both sides.
fn previous_parse(
    source: SourceSnapshot,
    config: ParseConfig,
    op: &mut Operation,
) -> Result<AcceptedModel, CoreError> {
    let front = crate::html::front_matter::prepare(&source, config, op)?;
    let has_front = front.is_some();
    let input = match &front {
        Some(front) => front.mask(&source, op)?,
        None => source.clone(),
    };
    let model = AcceptedModel::parse_once(input, config, 1, op, None)?;
    if front.is_none()
        && (!config.extensions().contains(Extensions::HTML_BLOCKS)
            || !html_probe::contains_marker(source.text(), op)?)
    {
        return Ok(model);
    }
    let facts = construction_scope(op, |mut work| {
        let facts = crate::parse::ProjectionFacts::from_model(&model, &mut work)?;
        work.finish()?;
        Ok::<_, CoreError>(facts)
    })?;
    let facts = config
        .extensions()
        .contains(Extensions::HTML_BLOCKS)
        .then_some(facts);
    let Some((mut html, text, reservation)) =
        HtmlProjection::build(facts.as_ref(), config, &source, front, op)?
    else {
        if has_front {
            drop(model);
            return AcceptedModel::parse_once(source, config, 1, op, None);
        }
        return Ok(model);
    };
    drop((model, facts));
    let projected = SourceDocument::new(source.key(), text.as_bytes(), op)?.snapshot();
    drop((text, reservation));
    let mut model =
        AcceptedModel::parse_with_projection_once(projected, config, 1, Some(&html), op, None)?;
    html.bind_static_checkboxes(&model, op)?;
    html.bind_tables(&model, op)?;
    html.bind_destinations(&model, op)?;
    html.finish_styles(&model, op)?;
    Arc::get_mut(&mut model.0).unwrap().html = Some(html);
    Ok(model)
}

#[test]
fn projected_and_inert_models_match_previous_acceptance_and_original_coordinates() {
    use crate::identity::SourceBoundary;
    use crate::ir::TextTarget;
    use crate::mapping::PointInput;
    let config = ParseConfig::with_extensions(Profile::G3, Extensions::ALL);
    for text in [
        "---\r\ntitle: 中😀\r\n---\r\n\r\nparagraph **strong**\r\n",
        "---\r\ncontact: alice@example.com\r\n---\r\n\r\nbody&#64;example.net\r\n",
        "<p>alice@example.com</p>\n\nbody&#64;example.net\n",
        "---\na: b\n---\n\n<div><p>中😀 &amp; <a href='x&amp;y'>link</a></p></div>\n\ntext\n",
        "before\r\n\r\n<div><p>中😀 &amp; text</p></div>\r\n\r\nafter",
        "<label>中😀<input checked type=checkbox></label>\n\nend",
        "<unknown unsupported=x>inert</unknown>\n\ntext < y",
        "ordinary < y and inline `<div>code</div>`\n\nnext",
        "<table><tr><th>中😀</th></tr><tr><td>x &amp; y</td></tr></table>\n",
        "\\<label><input type=checkbox>escaped opener</label>\n",
        "\\*<label><input type=checkbox>escaped punctuation</label>\n",
        "before\r\n\r\n<label>\\* **nested \\*emphasis\\*** \\_尾<input type=checkbox></label>\r\n",
        "before\n\n<label><code>\\*中😀</code><kbd>\\<x></kbd><samp>\\_tail</samp><input type=checkbox></label>\n",
        "before\n\n<label>![\\*escaped \\<label>](image)<input type=checkbox>中\0😀</label>\r\n",
        "before\r\n\r\n<label><input type=checkbox disabled>\\* **nested \\*emphasis\\*** 中😀</label>\r\n",
        "<LaBeL><input TYPE=checkbox DISABLED><code>\\*中😀</code><kbd>\\<x></kbd><samp>\\_tail</samp></LaBeL>\r\n",
        "<label><input type=checkbox disabled><img src='a&amp;b' alt='\\*escaped'>中\0😀</label>\r\n",
    ] {
        let budget = SourceBudget::preview();
        let source = source(text, &budget);
        let old = previous_parse(
            source.clone(),
            config,
            &mut budget.begin(CancellationToken::new()),
        )
        .unwrap();
        let new = AcceptedModel::parse(
            source.clone(),
            config,
            1,
            &mut budget.begin(CancellationToken::new()),
        )
        .unwrap();
        assert_eq!(new.source().text(), old.source().text(), "{text:?}");
        assert_eq!(new.original_source().text(), text);
        assert_eq!(new.ir().nodes().len(), old.ir().nodes().len());
        assert_eq!(new.ir().atoms_len(), old.ir().atoms_len());
        for (byte, _) in text
            .char_indices()
            .chain(std::iter::once((text.len(), '\0')))
        {
            for affinity in [Affinity::Before, Affinity::After] {
                let query = |model: &AcceptedModel| PointQuery {
                    model: model.identity(),
                    input: PointInput::Source {
                        target: TextTarget::Flow(model.main_flow_id()),
                        byte: SourceBoundary::new(byte as u64),
                    },
                    affinity,
                };
                let a = old
                    .map_points(&[query(&old)], &mut budget.begin(CancellationToken::new()))
                    .unwrap();
                let b = new
                    .map_points(&[query(&new)], &mut budget.begin(CancellationToken::new()))
                    .unwrap();
                assert_eq!(
                    a.results(),
                    b.results(),
                    "byte {byte}, {affinity:?}: {text:?}"
                );
            }
        }
        drop((old, new, source));
        assert_eq!(budget.usage().source_capacity, 0);
        assert_eq!(budget.usage().auxiliary_capacity, 0);
    }
}

#[test]
fn label_paragraphs_split_by_probe_windows_keep_previous_projection() {
    let config = ParseConfig::with_extensions(Profile::G3, Extensions::ALL);
    for padding in [4086, 4087, 4088, 4089, 4090, 4091, 8182, 8183, 8184, 8185] {
        for ending in ["\n\n", "\r\n\r\n"] {
            // Emoji before the separator exercises byte windows independently
            // of UTF-8 boundaries. Several label openers straddle byte 4096 or
            // 8192; missing the five-byte overlap would silently omit them.
            let text = format!(
                "{}😀{ending}<LaBeL><input type=checkbox disabled>中😀</LaBeL>\n",
                "a".repeat(padding)
            );
            let budget = SourceBudget::preview();
            let source = source(&text, &budget);
            let old = previous_parse(
                source.clone(),
                config,
                &mut budget.begin(CancellationToken::new()),
            )
            .unwrap();
            let new = AcceptedModel::parse(
                source.clone(),
                config,
                1,
                &mut budget.begin(CancellationToken::new()),
            )
            .unwrap();
            assert_ne!(old.source().text(), text);
            assert_eq!(
                new.source().text(),
                old.source().text(),
                "padding {padding}, {ending:?}"
            );
            assert_eq!(
                new.ir().flows()[new.main_flow_id().index()].logical_utf16,
                old.ir().flows()[old.main_flow_id().index()].logical_utf16
            );
            drop((new, old, source));
            assert_eq!(budget.usage().auxiliary_capacity, 0);
            assert_eq!(budget.usage().source_capacity, 0);
        }
    }
}

#[test]
fn projection_facts_and_original_acceptance_reject_same_native_faults() {
    let text = "<div>中😀</div>\n\nordinary **strong** [link](dest)\n";
    let config = ParseConfig::with_extensions(Profile::G3, Extensions::ALL);
    for limits in [
        ModelLimits {
            native_records: 1,
            ..ModelLimits::default()
        },
        ModelLimits {
            depth: 1,
            ..ModelLimits::default()
        },
        ModelLimits {
            decoded_bytes: 1,
            ..ModelLimits::default()
        },
    ] {
        let budget = SourceBudget::new_with_model_limits(SourceLimits::default(), limits).unwrap();
        let source = source(text, &budget);
        let mut old = budget.begin(CancellationToken::new());
        let error = AcceptedModel::parse_once(source.clone(), config, 1, &mut old, None)
            .err()
            .unwrap();
        let mut new = budget.begin(CancellationToken::new());
        let actual = facts(source.clone(), config, &mut new).err().unwrap();
        assert_eq!(actual, error);
        assert_eq!(new.first_fault(), old.first_fault());
        drop(source);
        assert_eq!(budget.usage().auxiliary_capacity, 0);
        assert_eq!(budget.usage().source_capacity, 0);
    }
    let budget = SourceBudget::preview();
    let source = source(text, &budget);
    let cancel = CancellationToken::new();
    cancel.cancel();
    for original in [false, true] {
        let mut op = budget.begin(cancel.clone());
        let error = if original {
            AcceptedModel::parse_once(source.clone(), config, 1, &mut op, None).err()
        } else {
            facts(source.clone(), config, &mut op).err()
        };
        assert_eq!(error, Some(CoreError::Cancelled));
        assert_eq!(op.first_fault(), Some(CoreError::Cancelled));
    }
}

#[test]
fn html_preparation_rejects_wrong_root_before_scanning_and_preserves_deadline_faults() {
    for text in ["<div>中😀</div>", "plain < y", "---\na: b\n---\n\nbody"] {
        let budget = SourceBudget::task_fixture();
        let source = source(text, &budget);
        let config = ParseConfig::with_extensions(Profile::G3, Extensions::ALL);
        let other = SourceBudget::task_fixture();
        let mut wrong = other.begin(CancellationToken::new());
        assert_eq!(
            AcceptedModel::parse(source.clone(), config, 1, &mut wrong).err(),
            Some(CoreError::WrongBudget)
        );
        assert_eq!(wrong.stats(), None);
        let retained = budget.usage();
        let mut expired = budget.begin(CancellationToken::new());
        expired.set_clock_samples_for_test(&[500_000_000]);
        let error = AcceptedModel::parse(source.clone(), config, 1, &mut expired)
            .err()
            .unwrap();
        assert!(matches!(error, CoreError::DeadlineExceeded { .. }));
        assert_eq!(expired.first_fault(), Some(error));
        assert_eq!(
            AcceptedModel::parse(source.clone(), config, 1, &mut expired).err(),
            Some(error)
        );
        assert_eq!(budget.usage().source_capacity, retained.source_capacity);
        assert_eq!(
            budget.usage().auxiliary_capacity,
            retained.auxiliary_capacity
        );
        drop(source);
        assert_eq!(budget.usage().source_capacity, 0);
        assert_eq!(budget.usage().auxiliary_capacity, 0);
    }
}
