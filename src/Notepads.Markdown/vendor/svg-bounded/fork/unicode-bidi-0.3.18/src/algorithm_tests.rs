// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
//! Fixed ordinary API facts beyond the single-paragraph Unicode data files.

use super::*;

#[cfg(feature = "hardcoded-data")]
#[test]
fn whole_text_splits_paragraphs_and_resolves_fsi_without_changing_input() {
    let text = "a\n\u{2068}אב\u{2069}\n😀z";
    let initial = InitialInfo::new(text, None);
    let info = BidiInfo::new(text, None);
    assert_eq!(info.paragraphs.len(), 3);
    assert_eq!(initial.paragraphs, info.paragraphs);
    assert_eq!(initial.original_classes, info.original_classes);
    assert_eq!(info.paragraphs[0].range, 0..2);
    assert_eq!(info.paragraphs[1].range, 2..13);
    assert_eq!(info.paragraphs[2].range, 13..18);
    assert!(info.paragraphs.iter().all(|para| para.level == LTR_LEVEL));
    assert_eq!(&info.original_classes[2..5], &[RLI; 3]);
    assert_eq!(info.text, text);

    let text16: Vec<u16> = text.encode_utf16().collect();
    let info16 = utf16::BidiInfo::new(&text16, None);
    assert_eq!(info16.paragraphs.len(), 3);
    assert_eq!(info16.paragraphs[0].range, 0..2);
    assert_eq!(info16.paragraphs[1].range, 2..7);
    assert_eq!(info16.paragraphs[2].range, 7..10);
    let utf8_character_levels: Vec<_> = text.char_indices().map(|(i, _)| info.levels[i]).collect();
    let utf16_character_levels: Vec<_> = text16
        .char_indices()
        .map(|(i, _)| info16.levels[i])
        .collect();
    assert_eq!(utf8_character_levels, utf16_character_levels);
}

#[cfg(feature = "hardcoded-data")]
#[test]
fn single_paragraph_keeps_separator_and_its_original_base() {
    let text = "אב\nabc";
    let split = BidiInfo::new(text, None);
    let single = ParagraphBidiInfo::new(text, None);
    assert_eq!(split.paragraphs.len(), 2);
    assert_eq!(split.paragraphs[0].level, RTL_LEVEL);
    assert_eq!(split.paragraphs[1].level, LTR_LEVEL);
    assert_eq!(single.paragraph_level, RTL_LEVEL);
    assert_eq!(single.levels.len(), text.len());
    assert_eq!(single.original_classes[4], B);
    let text16: Vec<u16> = text.encode_utf16().collect();
    let single16 = utf16::ParagraphBidiInfo::new(&text16, None);
    assert_eq!(single16.paragraph_level, RTL_LEVEL);
    assert_eq!(single16.original_classes[2], B);
    assert_eq!(
        text.char_indices()
            .map(|(i, _)| single.levels[i])
            .collect::<Vec<_>>(),
        text16
            .char_indices()
            .map(|(i, _)| single16.levels[i])
            .collect::<Vec<_>>()
    );
}

#[cfg(feature = "hardcoded-data")]
#[test]
fn l1_keeps_full_levels_and_unselected_paragraphs() {
    let text = "אב cd  \nxyz  ";
    let info = BidiInfo::new(text, None);
    let para = &info.paragraphs[0];
    let line = 0..para.range.end - 1;
    let reordered = info.reordered_levels(para, line.clone());
    assert_eq!(reordered.len(), info.levels.len());
    assert_eq!(&reordered[line.end..], &info.levels[line.end..]);
    assert_eq!(&reordered[line.end - 2..line.end], &[RTL_LEVEL; 2]);
    assert_eq!(
        info.reordered_levels(&info.paragraphs[1], text.len()..text.len()),
        info.levels
    );
}

#[test]
fn l2_permutation_preserves_nested_even_and_odd_runs() {
    let levels = Level::vec(&[0, 1, 1, 2, 2, 1, 0]);
    assert_eq!(BidiInfo::reorder_visual(&levels), &[0, 5, 3, 4, 2, 1, 6]);
    assert_eq!(
        utf16::BidiInfo::reorder_visual(&levels),
        &[0, 5, 3, 4, 2, 1, 6]
    );
    assert!(BidiInfo::reorder_visual(&[]).is_empty());
}

#[cfg(feature = "hardcoded-data")]
#[test]
fn visual_runs_preserve_nonterminal_empty_line_and_full_level_result() {
    let info = BidiInfo::new("abc", None);
    let (levels, runs) = info.visual_runs(&info.paragraphs[0], 1..1);
    assert_eq!(levels, &[LTR_LEVEL; 3]);
    assert_eq!(runs, &[1..1]);
    let mixed = BidiInfo::new("aאבb", None);
    assert_eq!(mixed.reorder_line(&mixed.paragraphs[0], 0..6), "aבאb");
    let (levels, runs) = mixed.visual_runs(&mixed.paragraphs[0], 0..6);
    assert_eq!(levels.len(), mixed.text.len());
    assert_eq!(runs, &[0..1, 1..5, 5..6]);
}

#[test]
fn sequence_iteration_retains_stock_order_across_previous_runs() {
    // The stock predecessor helper reverses run order, while retaining the
    // forward order inside earlier ranges. The storage migration preserves it.
    let sequence = prepare::Sequence::<NoControl> {
        runs: vec![0..2, 4..6],
        sos: L,
        eos: R,
    };
    assert_eq!(
        sequence.iter_forwards_from(1, 0).collect::<Vec<_>>(),
        &[1, 4, 5]
    );
    assert_eq!(
        sequence.iter_backwards_from(6, 1).collect::<Vec<_>>(),
        &[5, 4, 0, 1]
    );
}
