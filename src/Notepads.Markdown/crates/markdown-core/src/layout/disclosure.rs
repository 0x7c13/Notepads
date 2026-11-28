// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

use super::section::Section;
use super::storage::LayoutRows;
use crate::error::CoreError;
use crate::identity::LogicalSpan;
use crate::ir::NodeId;
use crate::model::{AcceptedModel, DisclosureRegion};
use crate::policy::Operation;

/// An ordered, model-local input row. A node is accepted only if it has a disclosure.
#[repr(C, align(8))]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DisclosureOverride {
    pub node: u32,
    pub expanded: u32,
}

/// Logical coordinates remain in the original Main flow even when hidden.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Disclosure {
    pub node: NodeId,
    pub summary: NodeId,
    pub first_body_node: u32,
    pub next_node: u32,
    pub summary_start: u64,
    pub body: LogicalSpan,
    pub expanded: bool,
}

pub(super) struct Visibility {
    disclosures: LayoutRows<Disclosure>,
    hidden: LayoutRows<LogicalSpan>,
    hidden_nodes: LayoutRows<(u32, u32)>,
}

impl Visibility {
    pub fn build(
        model: &AcceptedModel,
        overrides: &[DisclosureOverride],
        section: Option<&Section>,
        op: &mut Operation,
    ) -> Result<Self, CoreError> {
        let regions = model.disclosure_regions();
        let count = regions.len();
        if overrides.len() > count {
            return Err(op.fail(CoreError::InvalidRange));
        }
        let mut disclosures = LayoutRows::new(count, op)?;
        let mut hidden: LayoutRows<LogicalSpan> = LayoutRows::new(count, op)?;
        let mut hidden_nodes: LayoutRows<(u32, u32)> = LayoutRows::new(count, op)?;
        let mut next = 0;
        for &DisclosureRegion {
            node,
            summary,
            first_body_node,
            next_node,
            summary_start,
            body,
            initially_expanded,
        } in regions
        {
            op.events(1)?;
            op.scan(size_of::<DisclosureRegion>() as u64)?;
            let expanded = if let Some(state) = overrides.get(next) {
                op.scan(size_of::<DisclosureOverride>() as u64)?;
                match state.node.cmp(&node.value()) {
                    std::cmp::Ordering::Less => return Err(op.fail(CoreError::InvalidRange)),
                    std::cmp::Ordering::Equal => {
                        next += 1;
                        match state.expanded {
                            0 => false,
                            1 => true,
                            _ => return Err(op.fail(CoreError::InvalidRange)),
                        }
                    }
                    std::cmp::Ordering::Greater => initially_expanded,
                }
            } else {
                initially_expanded
            };
            // The enclosing disclosure of a selected heading supplies structural
            // context only. Its summary is outside the excerpt and its body is open.
            if let Some(section) = section
                && ((first_body_node..next_node).contains(&section.heading.value())
                    || !section.contains(summary, op)?)
            {
                continue;
            }
            disclosures.push(
                Disclosure {
                    node,
                    summary,
                    first_body_node,
                    next_node,
                    summary_start,
                    body,
                    expanded,
                },
                op,
            )?;
            if expanded {
                continue;
            }
            if first_body_node < next_node {
                if let Some(previous) = hidden_nodes.values.last_mut()
                    && previous.1 >= first_body_node
                {
                    if first_body_node < previous.0 {
                        return Err(op.fail(CoreError::InvalidProvenance));
                    }
                    op.copy(size_of::<(u32, u32)>() as u64)?;
                    previous.1 = previous.1.max(next_node);
                } else {
                    hidden_nodes.push((first_body_node, next_node), op)?;
                }
            }
            if body.is_empty() {
                continue;
            }
            if let Some(previous) = hidden.values.last_mut()
                && previous.end() >= body.start()
            {
                if previous.start() > body.start() {
                    return Err(op.fail(CoreError::InvalidProvenance));
                }
                op.copy(size_of::<LogicalSpan>() as u64)?;
                *previous = LogicalSpan::new(previous.start(), previous.end().max(body.end()))
                    .map_err(|error| op.fail(error))?;
            } else {
                hidden.push(body, op)?;
            }
        }
        if next != overrides.len() {
            return Err(op.fail(CoreError::InvalidRange));
        }
        Ok(Self {
            disclosures,
            hidden,
            hidden_nodes,
        })
    }

    pub fn disclosures(&self) -> &[Disclosure] {
        &self.disclosures.values
    }

    pub fn hidden(&self) -> &[LogicalSpan] {
        &self.hidden.values
    }

    pub fn hides(&self, node: NodeId, op: &mut Operation) -> Result<bool, CoreError> {
        let entries = &self.hidden_nodes.values;
        let node = node.value();
        let (mut lo, mut hi) = (0, entries.len());
        while lo < hi {
            op.events(1)?;
            op.scan(size_of::<(u32, u32)>() as u64)?;
            let mid = lo + (hi - lo) / 2;
            let (first, end) = entries[mid];
            if node < first {
                hi = mid;
            } else if node >= end {
                lo = mid + 1;
            } else {
                return Ok(true);
            }
        }
        Ok(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{Extensions, ParseConfig, Profile};
    use crate::identity::SourceKey;
    use crate::layout::{LayoutDraft, LayoutKey, LayoutStyle};
    use crate::policy::{CancellationToken, SourceBudget};
    use crate::source::SourceDocument;

    #[test]
    fn collapsed_children_skip_measurement_and_keep_main_coordinates() {
        let source = "> [!NOTE]- Outer\n> Body before inner.\n>\n\
            > > [!TIP]+ Inner\n> > Inner body.\n>\n> Body after inner.\n\nTail.\n";
        let budget = SourceBudget::preview();
        let cancel = CancellationToken::new();
        let mut op = budget.begin(cancel.clone());
        let source = SourceDocument::new(
            SourceKey::new([117; 16], 1).unwrap(),
            source.as_bytes(),
            &mut op,
        )
        .unwrap()
        .snapshot();
        let model = AcceptedModel::parse(
            source,
            ParseConfig::with_extensions(Profile::G3, Extensions::CALLOUTS),
            1,
            &mut op,
        )
        .unwrap();
        let key = |generation| {
            LayoutKey::new(
                model.identity(),
                generation,
                1,
                1,
                400.0,
                LayoutStyle::markdown(16.0, 8.0).unwrap(),
            )
            .unwrap()
        };
        let mut closed =
            LayoutDraft::with_disclosures(model.clone(), key(1), &[], &mut op).unwrap();
        let [outer, inner] = closed.disclosures() else {
            panic!("two nested disclosures expected")
        };
        let (outer, inner) = (*outer, *inner);
        assert!(!outer.expanded && inner.expanded);
        assert_eq!(closed.hidden_ranges(), [outer.body]);
        let requests = closed.measurement_batch(256, &mut op).unwrap().unwrap();
        assert_eq!(requests.requests.len(), 2);
        assert_eq!(requests.requests[0].node, outer.summary);
        assert!(requests.requests[1].logical.start() >= outer.body.end());

        let switches = [
            DisclosureOverride {
                node: outer.node.value(),
                expanded: 1,
            },
            DisclosureOverride {
                node: inner.node.value(),
                expanded: 0,
            },
        ];
        let mut partial =
            LayoutDraft::with_disclosures(model.clone(), key(2), &switches, &mut op).unwrap();
        assert_eq!(partial.model().identity(), closed.model().identity());
        assert_eq!(partial.disclosures()[0].body, outer.body);
        assert_eq!(partial.hidden_ranges(), [inner.body]);
        let requests = partial.measurement_batch(256, &mut op).unwrap().unwrap();
        assert_eq!(requests.requests.len(), 5);
        assert_eq!(requests.requests[2].node, inner.summary);
        assert!(requests.requests.iter().all(|request| {
            request.logical.end() <= inner.body.start()
                || request.logical.start() >= inner.body.end()
        }));

        let mut invalid = budget.begin(CancellationToken::new());
        let duplicate = [switches[0], switches[0]];
        let invalid_key = key(3);
        assert_eq!(
            LayoutDraft::with_disclosures(model, invalid_key, &duplicate, &mut invalid).err(),
            Some(CoreError::InvalidRange)
        );
    }

    #[test]
    fn html_details_fold_native_markdown_between_original_markers() {
        let source = "<details>\r\n\r\n<summary>HTML <em>summary</em></summary>\r\n\r\n\
            ## Body heading\r\n\r\n- **Body item**\r\n\r\n</details>\r\n\r\n\
            <details open=\"\">\r\n\r\nInitially visible.\r\n\r\n</details>\r\n\r\n\
            <details open><summary>Compact standard HTML</summary><details><summary>Nested summary</summary>\
            <p>Nested hidden body.</p></details><p>Visible after nested.</p></details>\r\n\r\nTail.\r\n";
        let budget = SourceBudget::preview();
        let cancel = CancellationToken::new();
        let mut op = budget.begin(cancel);
        let original = SourceDocument::new(
            SourceKey::new([119; 16], 1).unwrap(),
            source.as_bytes(),
            &mut op,
        )
        .unwrap()
        .snapshot();
        let model = AcceptedModel::parse(
            original,
            ParseConfig::with_extensions(Profile::G3, Extensions::HTML_BLOCKS),
            1,
            &mut op,
        )
        .unwrap();
        assert_eq!(model.original_source().text(), source);
        let key = LayoutKey::new(
            model.identity(),
            1,
            1,
            1,
            400.0,
            LayoutStyle::markdown(16.0, 8.0).unwrap(),
        )
        .unwrap();
        let mut layout = LayoutDraft::new(model, key, &mut op).unwrap();
        let [closed, open, compact, nested] = *layout.disclosures() else {
            panic!("expected four HTML disclosures: {:?}", layout.disclosures());
        };
        assert_eq!(closed.node, closed.summary);
        let title = layout.model().ir().nodes()[closed.summary.index()].source_envelope;
        assert_eq!(
            layout.model().source().text()[title.start() as usize..title.end() as usize].trim(),
            "HTML summary"
        );
        assert!(!closed.expanded && open.expanded && compact.expanded && !nested.expanded);
        assert_eq!(layout.hidden_ranges(), [closed.body, nested.body]);
        assert!(
            compact.first_body_node <= nested.summary.value()
                && nested.next_node < compact.next_node
        );
        let batch = layout.measurement_batch(256, &mut op).unwrap().unwrap();
        assert!(
            batch
                .requests
                .iter()
                .all(|r| r.logical.end() <= closed.body.start()
                    || r.logical.start() >= closed.body.end())
        );
        assert!(batch.requests.iter().any(|r| r.node == open.summary));
    }

    #[test]
    fn unsupported_nested_html_does_not_close_the_outer_disclosure() {
        let source = "<details><summary>Outer</summary>\n\nBefore inner.\n\n\
            <div><details><summary>Inner</summary>\n\nInner body.\n\n\
            </details>\n\n</div>\n\nOuter trailing body.\n\n</details>\n";
        let large = source.replace(
            "<summary>Inner</summary>\n",
            &format!("<summary>Inner</summary>\n{}\n", "a".repeat(65_536)),
        );
        for source in [source, &large] {
            let budget = SourceBudget::preview();
            let mut op = budget.begin(CancellationToken::new());
            let original = SourceDocument::new(
                SourceKey::new([120; 16], 1).unwrap(),
                source.as_bytes(),
                &mut op,
            )
            .unwrap()
            .snapshot();
            let model = AcceptedModel::parse(
                original,
                ParseConfig::with_extensions(Profile::G3, Extensions::HTML_BLOCKS),
                1,
                &mut op,
            )
            .unwrap();
            assert!(model.disclosure_regions().is_empty());
            assert_eq!(model.original_source().text(), source);
        }
    }
}
