// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
//! New conforming font fixtures. Original no-glyphN fixtures/assertions remain unchanged.
use super::font_replay_color_tests::{mock_data, sfnt};
use super::*;
use crate::parser::source_kind::{SourceError, SourceKind};
fn options(glyph: u32, upm: u16, color: svgtypes::Color) -> Options<'static> {
    Options {
        source_kind: SourceKind::FontGlyph {
            glyph,
            units_per_em: upm,
            current_color: color,
        },
        ..Options::default()
    }
}
fn parse(source: &str, opt: &Options) -> Result<Tree, crate::Error> {
    Tree::from_data_in_request(source.as_bytes(), opt)
}
fn font(data: &[u8], doc: &[u8], first: u16, last: u16, upm: u16) -> Vec<u8> {
    let count = u16::from_be_bytes(data[4..6].try_into().unwrap()) as usize;
    let mut tables = Vec::new();
    for index in 0..count {
        let at = 12 + index * 16;
        let tag: [u8; 4] = data[at..at + 4].try_into().unwrap();
        let offset = u32::from_be_bytes(data[at + 8..at + 12].try_into().unwrap()) as usize;
        let len = u32::from_be_bytes(data[at + 12..at + 16].try_into().unwrap()) as usize;
        let mut value = data[offset..offset + len].to_vec();
        if tag == *b"SVG " {
            value = vec![0, 0, 0, 0, 0, 10, 0, 0, 0, 0, 0, 1];
            value.extend(first.to_be_bytes());
            value.extend(last.to_be_bytes());
            value.extend(14u32.to_be_bytes());
            value.extend((doc.len() as u32).to_be_bytes());
            value.extend(doc);
        }
        if tag == *b"head" {
            value[18..20].copy_from_slice(&upm.to_be_bytes());
        }
        tables.push((tag, value));
    }
    let mut result = data[..12].to_vec();
    let mut offset = 12 + count * 16;
    for (tag, value) in &tables {
        result.extend(tag);
        result.extend(0u32.to_be_bytes());
        result.extend((offset as u32).to_be_bytes());
        result.extend((value.len() as u32).to_be_bytes());
        offset += (value.len() + 3) & !3;
    }
    for (_, value) in tables {
        result.extend(value);
        while result.len() % 4 != 0 {
            result.push(0);
        }
    }
    result
}
fn paths(group: &Group) -> Vec<&Path> {
    let mut output = Vec::new();
    for node in group.children() {
        match node {
            Node::Path(path) => output.push(path.as_ref()),
            Node::Group(g) => output.extend(paths(g)),
            _ => (),
        }
    }
    output
}
#[test]
fn font_source_selected_use_preserves_source_css_defs_and_excludes_sibling_geometry() {
    let source = r##"<svg xmlns="http://www.w3.org/2000/svg" width="1000" height="1000"><style>svg > g > path {fill:red}</style><defs><path id="common" d="M1 2h10v20h-10z"/></defs><g id="glyph1" transform="translate(7,9)"><use href="#common"/></g><g id="glyph2"><path d="M0 0h900v900h-900z"/></g></svg>"##;
    let opt = options(1, 1000, svgtypes::Color::black());
    let tree = parse(source, &opt).unwrap();
    let ps = paths(tree.root());
    assert_eq!(ps.len(), 1);
    let bbox = tree.root().abs_bounding_box();
    assert_eq!(
        (bbox.x(), bbox.y(), bbox.width(), bbox.height()),
        (8.0, 11.0, 10.0, 20.0)
    );
    // Separate target's real raw structural selector is resolved before cloning.
    let css = r#"<svg xmlns="http://www.w3.org/2000/svg"><style>svg > g > path {fill:red}</style><g id="glyph1"><path d="M0 0h20v30z"/></g></svg>"#;
    let tree = parse(css, &opt).unwrap();
    assert!(
        matches!(paths(tree.root())[0].fill().unwrap().paint(),Paint::Color(c) if *c==Color::new_rgb(255,0,0))
    );
}
#[test]
fn font_source_root_target_viewport_and_no_em_square_clip_are_actual() {
    let opt = options(1, 2048, svgtypes::Color::black());
    let tree=parse(r#"<svg xmlns="http://www.w3.org/2000/svg" id="glyph1" viewBox="0 0 1024 1024" overflow="hidden"><path d="M-10 0h2058v10h-2058z"/></svg>"#,&opt).unwrap();
    assert_eq!(
        (tree.size().width(), tree.size().height()),
        (2048.0, 2048.0)
    );
    let b = tree.root().abs_bounding_box();
    assert_eq!((b.x(), b.width()), (-20.0, 4116.0));
    let tree=parse(r#"<svg xmlns="http://www.w3.org/2000/svg" width="200" height="400" viewBox="0 0 100 100" preserveAspectRatio="none"><path id="glyph1" d="M0 0h100v100H0z"/></svg>"#,&opt).unwrap();
    let b = tree.root().abs_bounding_box();
    assert_eq!((b.width(), b.height()), (200.0, 400.0));
}
#[test]
fn font_source_every_record_needs_one_exact_target_and_raw_security_runs_before_ignore() {
    for (source, expected) in [
        (
            r#"<svg xmlns="http://www.w3.org/2000/svg"><path d="M0 0h1v1z"/></svg>"#,
            SourceError::MissingGlyph,
        ),
        (
            r#"<svg xmlns="http://www.w3.org/2000/svg"><g id="glyph1"/><g id="glyph1"/></svg>"#,
            SourceError::DuplicateGlyph,
        ),
    ] {
        let opt = options(1, 1000, svgtypes::Color::black());
        assert!(matches!(parse(source,&opt),Err(crate::Error::SourcePolicy(r)) if r==expected));
    }
    for bad in [
        r#"<script/>"#,
        r#"<path onclick="bad()"/>"#,
        r#"<image href="https://invalid.invalid/a.png"/>"#,
        r#"<style>.bad{fill:url(https://invalid.invalid/x)}</style>"#,
        r#"<style>@import 'https://invalid.invalid/x';</style>"#,
    ] {
        let source = format!(
            r#"<svg xmlns="http://www.w3.org/2000/svg"><g id="glyph1"><path d="M0 0h2v2z"/></g><text display="none">{bad}</text></svg>"#
        );
        let opt = options(1, 1000, svgtypes::Color::black());
        assert!(parse(&source, &opt).is_err(), "{bad}");
    }
}
#[test]
fn font_source_text_is_ignored_ordinary_data_svg_uses_same_existing_font_replay() {
    let source = r#"<svg xmlns="http://www.w3.org/2000/svg" width="100" height="100" id="glyph1"><text x="0" y="50" font-family="Mock">A</text><path d="M0 0h3v4z"/></svg>"#;
    let (cache, owner, _) = mock_data(sfnt(false, false, false), false);
    let mut opt = options(1, 1000, svgtypes::Color::black());
    opt.fontdb = cache.fontdb;
    opt.font_replay = Some(owner.clone());
    let tree = parse(source, &opt).unwrap();
    assert!(!tree.has_text_nodes());
    assert_eq!(paths(tree.root()).len(), 1);
    opt.source_kind = SourceKind::GeneralImage;
    let tree = parse(source, &opt).unwrap();
    assert!(tree.has_text_nodes());
    assert!(tree.root().abs_bounding_box().height() > 4.0);
}
#[test]
fn font_source_actual_font_record_extracts_only_target_and_source_color_not_fill() {
    let doc=br#"<svg xmlns="http://www.w3.org/2000/svg" width="1000" height="1000"><path id="glyph1" d="M0 0h500v700H0z" fill="currentColor"/><path id="glyph2" d="M0 0h900v900H0z"/></svg>"#;
    for last in [1, 2] {
        let bytes = font(&sfnt(false, true, false), doc, 1, last, 1000);
        let (cache, owner, _) = mock_data(bytes, false);
        let opt = Options {
            fontdb: cache.fontdb,
            font_replay: Some(owner),
            ..Options::default()
        };
        let source = r#"<svg xmlns="http://www.w3.org/2000/svg" width="100" height="100"><text x="0" y="80" font-family="Mock" font-size="20" color="red" fill="blue">A</text><text x="40" y="80" font-family="Mock" font-size="20" color="blue" fill="red">B</text></svg>"#;
        let tree = parse(source, &opt).unwrap();
        let mut colors = Vec::new();
        fn collect(g: &Group, out: &mut Vec<Color>) {
            for node in g.children() {
                match node {
                    Node::Text(t) => collect(t.flattened(), out),
                    Node::Group(g) => collect(g, out),
                    Node::Path(p) => {
                        if let Some(fill) = p.fill() {
                            if let Paint::Color(c) = fill.paint() {
                                out.push(*c)
                            }
                        }
                    }
                    _ => (),
                }
            }
        }
        collect(tree.root(), &mut colors);
        assert_eq!(
            colors,
            vec![Color::new_rgb(255, 0, 0), Color::new_rgb(0, 0, 255)]
        );
    }
}
#[test]
fn font_source_local_color_overrides_outer_and_general_svgz_stays_closed() {
    let opt = options(1, 1000, svgtypes::Color::red());
    let tree=parse(r#"<svg xmlns="http://www.w3.org/2000/svg" color="green"><path id="glyph1" fill="currentColor" d="M0 0h10v20z"/></svg>"#,&opt).unwrap();
    assert!(
        matches!(paths(tree.root())[0].fill().unwrap().paint(),Paint::Color(c) if *c==Color::new_rgb(0,128,0))
    );
    assert!(matches!(
        Tree::from_data_in_request(&[0x1f, 0x8b], &Options::default()),
        Err(crate::Error::SourcePolicy(
            SourceError::GeneralSvgzUnsupported
        ))
    ));
}

#[test]
fn compliant_font_source_cache_fixture_retains_actual_selected_svg_and_existing_priority() {
    let doc=br#"<svg xmlns="http://www.w3.org/2000/svg" width="1000" height="1000" id="glyph1"><path d="M0 0h500v700H0z" fill="red"/></svg>"#;
    let data = font(&sfnt(true, true, true), doc, 1, 1, 1000);
    let (mut cache, _owner, id) = mock_data(data, false);
    for _ in 0..2 {
        assert!(cache
            .fontdb_colr(id, GlyphId(1), &[], svgtypes::Color::black())
            .is_some());
        assert!(cache.fontdb_svg(id, GlyphId(1)).is_some());
        assert!(cache.fontdb_raster(id, GlyphId(1)).is_some());
        assert!(!cache.processing.failed());
    }
}

#[test]
fn old_nonconforming_font_source_actual_bytes_without_glyph_id_are_rejected() {
    let old_doc=br#"<svg xmlns="http://www.w3.org/2000/svg" width="1000" height="1000"><path d="M0 0H500V700H0Z" fill="red"/></svg>"#;
    let bytes = font(&sfnt(false, true, false), old_doc, 1, 1, 1000);
    let (mut cache, _owner, id) = mock_data(bytes, false);
    assert!(cache.fontdb_svg(id, GlyphId(1)).is_none());
    assert_eq!(
        cache.processing.resource_failure().unwrap().kind,
        markdown_svg_budget::FailureKind::Decode
    );
    assert!(cache.cache_svg.is_empty());
}
