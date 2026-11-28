// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

//! The complete C0 manifest from M1-CONTRACT 1.2; no upstream default controls parsing.
//! Attribute and HEEx fields are absent with the selected Comrak features.

pub(super) fn c0() -> comrak::Options<'static> {
    use comrak::options::{AlertStyleType, Extension, ListStyleType, Parse, Render};
    comrak::Options {
        extension: Extension {
            strikethrough: false,
            table: false,
            autolink: false,
            tasklist: false,
            superscript: false,
            header_id_prefix: None,
            header_id_prefix_in_href: false,
            footnotes: false,
            inline_footnotes: false,
            description_lists: false,
            front_matter_delimiter: None,
            multiline_block_quotes: false,
            alerts: false,
            native_heading_ids: false,
            native_table_of_contents: false,
            native_wiki_links: false,
            native_note_embeds: false,
            math_dollars: false,
            math_latex: false,
            math_code: false,
            shortcodes: false,
            wikilinks_title_after_pipe: false,
            wikilinks_title_before_pipe: false,
            underline: false,
            subscript: false,
            spoiler: false,
            greentext: false,
            image_url_rewriter: None,
            link_url_rewriter: None,
            cjk_friendly_emphasis: false,
            subtext: false,
            highlight: false,
            insert: false,
            block_directive: false,
        },
        parse: Parse {
            smart: false,
            default_info_string: None,
            relaxed_tasklist_matching: false,
            tasklist_in_table: false,
            relaxed_autolinks: false,
            latex_display_standalone: false,
            ignore_setext: false,
            broken_link_callback: None,
            leave_footnote_definitions: false,
            escaped_char_spans: false,
            sourcepos_chars: false,
        },
        render: Render {
            hardbreaks: false,
            github_pre_lang: false,
            full_info_string: false,
            width: 0,
            r#unsafe: false,
            escape: false,
            list_style: ListStyleType::Dash,
            sourcepos: false,
            escaped_char_spans: false,
            ignore_empty_links: false,
            gfm_quirks: false,
            prefer_fenced: false,
            figure_with_caption: false,
            tasklist_classes: false,
            alert_style: AlertStyleType::Specific,
            ol_width: 0,
            experimental_minimize_commonmark: false,
            compact_html: false,
        },
    }
}

pub(super) fn g3() -> comrak::Options<'static> {
    let mut options = c0();
    options.extension.strikethrough = true;
    options.extension.table = true;
    options.extension.tasklist = true;
    options
}
