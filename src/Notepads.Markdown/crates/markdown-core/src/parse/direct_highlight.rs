// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

use super::*;
use crate::identity::SourceKey;
use crate::layout::{LayoutDraft, LayoutKey, LayoutStyle};
use crate::model::{AcceptedModel, ParseProgress};
use crate::policy::{CancellationToken, SourceBudget};
use crate::presentation::{PresentationBatch, RUN_BOLD, RUN_CODE, RUN_HIGHLIGHT};
use crate::source::SourceDocument;

#[test]
fn inline_highlight_bounds_strict_autolinks_only_inside_accepted_pairs() {
    let source = "==https://example.test/a?x==!\n\n\
                  ==visit https://example.test/b?x=1==.\n\n\
                  https://example.test/c?a==b==";
    let mut op = SourceBudget::preview().begin(CancellationToken::new());
    let document = SourceDocument::new(
        SourceKey::new([125; 16], 1).unwrap(),
        source.as_bytes(),
        &mut op,
    )
    .unwrap()
    .snapshot();
    let mut progress = ParseProgress::default();
    let model = AcceptedModel::experimental_parse_with_progress(
        document,
        ParseConfig::with_extensions(Profile::G3, Extensions::HIGHLIGHT),
        1,
        &mut op,
        &mut progress,
    )
    .unwrap_or_else(|error| panic!("{error:?} at {:?}", progress.stage()));
    let ir = model.ir();
    assert_eq!(
        ir.nodes()
            .iter()
            .filter(|node| node.kind == NodeKind::Highlight)
            .count(),
        2,
    );
    let key = LayoutKey::new(
        model.identity(),
        1,
        1,
        1,
        400.0,
        LayoutStyle::markdown(18.0, 8.0).unwrap(),
    )
    .unwrap();
    let mut layout = LayoutDraft::new(model.clone(), key, &mut op).unwrap();
    let requests = layout.measurement_batch(8, &mut op).unwrap().unwrap();
    let batch = PresentationBatch::build(model, &requests, &mut op).unwrap();
    let targets = batch
        .links()
        .iter()
        .map(|link| {
            let start = link.destination.start as usize;
            std::str::from_utf8(&batch.blob()[start..start + link.destination.len as usize])
                .unwrap()
        })
        .collect::<Vec<_>>();
    assert_eq!(
        targets,
        [
            "https://example.test/a?x",
            "https://example.test/b?x=1",
            "https://example.test/c?a==b==",
        ]
    );
}

#[test]
fn inline_highlight_composes_with_markdown_html_and_links_without_changing_main() {
    let source = "😀 ==one **strong** [link](/work)== and <mark>two &amp; <sup>raised</sup></mark>.\r\n\r\n\
                  `==literal==` \\==escaped== ===triple=== ==unfinished\r\n\r\n\
                  <table><tr><td><mark>cell</mark></td></tr></table>";
    let mut op = SourceBudget::preview().begin(CancellationToken::new());
    let document = SourceDocument::new(
        SourceKey::new([124; 16], 1).unwrap(),
        source.as_bytes(),
        &mut op,
    )
    .unwrap()
    .snapshot();
    let mut progress = ParseProgress::default();
    let html_bits = Extensions::HTML_INLINE.bits() | Extensions::HTML_BLOCKS.bits();
    let model = AcceptedModel::experimental_parse_with_progress(
        document.clone(),
        ParseConfig::with_extensions(
            Profile::G3,
            Extensions::from_bits(html_bits | Extensions::HIGHLIGHT.bits()).unwrap(),
        ),
        1,
        &mut op,
        &mut progress,
    )
    .unwrap_or_else(|error| panic!("{error:?} at {:?}", progress.stage()));
    let ir = model.ir();
    assert_eq!(
        ir.nodes()
            .iter()
            .filter(|node| node.kind == NodeKind::Highlight)
            .count(),
        1
    );
    let mut delimiters: Vec<_> = (0..ir.atoms_len())
        .map(|index| {
            let atom = ir.atom_at(index, &mut op).unwrap();
            ir.authority(atom.claim, &mut op).unwrap()
        })
        .chain(ir.extra_authorities.view().iter().map(|row| row.value))
        .filter(|claim| ir.nodes()[claim.owner.index()].kind == NodeKind::Highlight)
        .filter(|claim| claim.kind == ClaimKind::Delimiter)
        .flat_map(|claim| claim.sources.slice(ir.sources()).unwrap())
        .map(|span| span.start() as usize)
        .collect();
    delimiters.sort_unstable();
    delimiters.dedup();
    assert_eq!(
        delimiters,
        [
            source.find("==one").unwrap(),
            source.find("== and").unwrap()
        ]
    );
    let key = LayoutKey::new(
        model.identity(),
        1,
        1,
        1,
        280.0,
        LayoutStyle::markdown(18.0, 8.0).unwrap(),
    )
    .unwrap();
    let mut layout = LayoutDraft::new(model.clone(), key, &mut op).unwrap();
    let requests = layout.measurement_batch(32, &mut op).unwrap().unwrap();
    let batch = PresentationBatch::build(model, &requests, &mut op).unwrap();
    let mut rows = Vec::new();
    for text in batch.texts() {
        let start = text.value.start as usize;
        let utf16 = std::str::from_utf8(&batch.blob()[start..start + text.value.len as usize])
            .unwrap()
            .encode_utf16()
            .collect::<Vec<_>>();
        let mut marked = String::new();
        let runs = text.runs.indices(batch.runs().len()).unwrap();
        for run in &batch.runs()[runs] {
            let piece = String::from_utf16(&utf16[run.start as usize..run.end as usize]).unwrap();
            if run.flags & RUN_HIGHLIGHT != 0 {
                marked.push_str(&piece);
                if piece == "strong" {
                    assert_ne!(run.flags & RUN_BOLD, 0);
                }
                if piece == "link" {
                    assert!(run.link.is_some());
                }
            }
            if piece == "==literal==" {
                assert_eq!(run.flags & (RUN_HIGHLIGHT | RUN_CODE), RUN_CODE);
            }
        }
        rows.push((String::from_utf16(&utf16).unwrap(), marked));
    }
    assert!(
        rows.contains(&(
            "😀 one strong link and two & raised.".into(),
            "one strong linktwo & raised".into()
        )),
        "{rows:?}"
    );
    assert!(
        rows.contains(&(
            "==literal== ==escaped== ===triple=== ==unfinished".into(),
            String::new()
        )),
        "{rows:?}"
    );
    assert!(rows.contains(&("cell".into(), "cell".into())), "{rows:?}");
    let plain = AcceptedModel::parse(
        document,
        ParseConfig::with_extensions(Profile::G3, Extensions::from_bits(html_bits).unwrap()),
        2,
        &mut op,
    )
    .unwrap();
    assert!(
        plain
            .ir()
            .nodes()
            .iter()
            .all(|node| node.kind != NodeKind::Highlight)
    );
}
