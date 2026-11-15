// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

//! Replay the fixed specification examples through the public model and presentation APIs.
//! The HTML oracle is original upstream data, with provenance in the fixture manifest.

use lol_html::{HtmlRewriter, Settings, doc_text, element, end_tag};
use markdown_core::config::{ParseConfig, Profile};
use markdown_core::identity::SourceKey;
use markdown_core::ir::{
    Alignment, AtomContent, AtomKind, ClaimKind, DetailRow, FlowId, ListMarker, NodeId, NodeKind,
    ObjectRow, PayloadId, Rows, SyntheticKind,
};
use markdown_core::layout::{
    BlockKind, LayoutDraft, LayoutKey, LayoutStyle, MAX_MEASUREMENT_BATCH,
};
use markdown_core::model::AcceptedModel;
use markdown_core::policy::{CancellationToken, Operation, SourceBudget};
use markdown_core::presentation::{
    BlobSlice, DecorationKind, PresentationBatch, RUN_BOLD, RUN_CODE, RUN_ITALIC, RUN_STRIKE,
};
use markdown_core::source::SourceDocument;
use serde_json::Value;
use std::collections::HashMap;
use std::path::Path;

const STRUCTURAL_TAGS: &str = "p,h1,h2,h3,h4,h5,h6,blockquote,ul,ol,li,pre,code,hr,table,tr,th,td,em,strong,del,a,img,input[type=checkbox],br";

#[derive(Debug, Eq, PartialEq)]
enum Event {
    Open(String, Vec<Option<String>>),
    Close(String),
    Text(String),
}

fn normalized_url(url: &str) -> String {
    let mut escaped = String::new();
    comrak::html::escape_href(&mut escaped, url, false).unwrap();
    htmlize::unescape_attribute(&escaped).into_owned()
}

fn without_layout_whitespace(value: &str) -> String {
    value
        .chars()
        .filter(|ch| !ch.is_ascii_whitespace())
        .collect()
}

fn oracle(html: &str) -> (Vec<Event>, String) {
    let events = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let output = events.clone();
    let content = events.clone();
    let mut rewriter = HtmlRewriter::new(
        Settings::new()
            .append_document_content_handler(doc_text!(move |chunk| {
                append_text(&mut content.borrow_mut(), chunk.as_str());
                Ok(())
            }))
            .append_element_content_handler(element!(STRUCTURAL_TAGS, move |el| {
                let tag = el.tag_name();
                let attr = |name| {
                    el.get_attribute(name)
                        .map(|value| htmlize::unescape_attribute(value).into_owned())
                };
                let attributes = match tag.as_str() {
                    "a" => vec![attr("href"), attr("title")],
                    "img" => vec![attr("src"), attr("title"), attr("alt")],
                    "ol" => vec![Some(attr("start").unwrap_or_else(|| "1".into()))],
                    "input" => vec![el.has_attribute("checked").then(String::new)],
                    "code" => vec![
                        attr("class")
                            .and_then(|value| value.strip_prefix("language-").map(str::to_owned)),
                    ],
                    "th" | "td" => vec![attr("align")],
                    _ => Vec::new(),
                };
                let container = el.can_have_content();
                output
                    .borrow_mut()
                    .push(Event::Open(tag.clone(), attributes));
                if container {
                    let output = output.clone();
                    el.on_end_tag(end_tag!(move |end| {
                        output.borrow_mut().push(Event::Close(end.name()));
                        Ok(())
                    }))?;
                }
                Ok(())
            })),
        |_: &[u8]| {},
    );
    rewriter.write(html.as_bytes()).unwrap();
    rewriter.end().unwrap();
    let events = normalize_events(std::mem::take(&mut *events.borrow_mut()), true);
    let text = events
        .iter()
        .filter_map(|event| match event {
            Event::Text(text) => Some(text.as_str()),
            _ => None,
        })
        .collect::<String>();
    (events, without_layout_whitespace(&text))
}

fn append_text(events: &mut Vec<Event>, text: &str) {
    if text.is_empty() {
        return;
    }
    if let Some(Event::Text(previous)) = events.last_mut() {
        previous.push_str(text);
    } else {
        events.push(Event::Text(text.into()));
    }
}

fn inline(tag: &str) -> bool {
    matches!(
        tag,
        "em" | "strong" | "del" | "a" | "img" | "input" | "br" | "code"
    )
}

fn normalize_events(events: Vec<Event>, html: bool) -> Vec<Event> {
    let mut parents = Vec::new();
    let mut output = Vec::with_capacity(events.len());
    let mut events = events.into_iter().peekable();
    while let Some(event) = events.next() {
        match event {
            Event::Open(tag, attributes) => {
                if !matches!(tag.as_str(), "img" | "input" | "br" | "hr") {
                    parents.push(tag.clone());
                }
                output.push(Event::Open(tag, attributes));
            }
            Event::Close(tag) => {
                assert_eq!(parents.pop().as_deref(), Some(tag.as_str()));
                output.push(Event::Close(tag));
            }
            Event::Text(text) => {
                let parent = parents.last().map(String::as_str);
                let text = if html {
                    htmlize::unescape(&text).into_owned()
                } else {
                    text
                };
                let mut text = text.as_str();
                if parent == Some("code") {
                    append_text(&mut output, text);
                    continue;
                }
                // The HTML examples add newlines around block tags and after <br>.
                // Keep authored spaces and normalize only soft line breaks in prose.
                if html {
                    if matches!(output.last(), Some(Event::Open(tag, _)) if tag == "input") {
                        // The native task decoration owns the label gap.
                        text = text.strip_prefix(' ').unwrap_or(text);
                    }
                    if matches!(output.last(), Some(Event::Open(tag, _)) if tag == "br" || !inline(tag))
                        || matches!(output.last(), Some(Event::Close(tag)) if !inline(tag))
                    {
                        text = text.trim_start_matches('\n');
                    }
                    if matches!(events.peek(), Some(Event::Open(tag, _)) if !inline(tag))
                        || matches!(events.peek(), Some(Event::Close(tag)) if !inline(tag))
                    {
                        text = text.trim_end_matches('\n');
                    }
                    if text.chars().all(|ch| ch.is_ascii_whitespace())
                        && matches!(
                            parent,
                            None | Some("blockquote" | "ul" | "ol" | "table" | "tr")
                        )
                    {
                        continue;
                    }
                }
                append_text(&mut output, &text.replace('\n', " "));
            }
        }
    }
    output
}

struct ModelStructure<'a> {
    model: &'a AcceptedModel,
    events: Vec<Event>,
    op: &'a mut Operation,
    text_by_owner: HashMap<NodeId, String>,
}

impl ModelStructure<'_> {
    fn payload(&self, id: PayloadId) -> &str {
        let ir = self.model.ir();
        ir.text(ir.payloads()[id.index()].value).unwrap()
    }

    fn walk(&mut self, id: NodeId, tight: bool, header: bool, table: Option<Rows<Alignment>>) {
        let ir = self.model.ir();
        let node = ir.nodes()[id.index()];
        let detail = node.detail.map(|id| ir.details()[id.index()]);
        let mut child_tight = tight;
        let mut child_header = header;
        let mut child_table = table;
        let mut close = 1;
        let mut attributes = Vec::new();
        let tag = match node.kind {
            NodeKind::Document => None,
            NodeKind::Paragraph if tight => None,
            NodeKind::Paragraph => Some("p"),
            NodeKind::Heading => match detail {
                Some(DetailRow::Heading { level, .. }) => {
                    Some(["h1", "h2", "h3", "h4", "h5", "h6"][usize::from(level) - 1])
                }
                _ => panic!("heading without a level"),
            },
            NodeKind::BlockQuote => {
                child_tight = false;
                Some("blockquote")
            }
            NodeKind::List => match detail {
                Some(DetailRow::List {
                    marker,
                    tight,
                    start,
                    ..
                }) => {
                    child_tight = tight;
                    Some(match marker {
                        ListMarker::Bullet(_) => "ul",
                        ListMarker::Ordered { .. } => {
                            attributes.push(Some(start.to_string()));
                            "ol"
                        }
                    })
                }
                _ => panic!("list without a marker"),
            },
            NodeKind::ListItem => Some("li"),
            NodeKind::CodeBlock => {
                self.events.push(Event::Open("pre".into(), Vec::new()));
                close = 2;
                let Some(DetailRow::Code { info, .. }) = detail else {
                    panic!("code block without code metadata")
                };
                attributes.push(info.and_then(|id| {
                    self.payload(id)
                        .split_ascii_whitespace()
                        .next()
                        .map(str::to_owned)
                }));
                Some("code")
            }
            NodeKind::ThematicBreak => {
                close = 0;
                Some("hr")
            }
            NodeKind::Table => match detail {
                Some(DetailRow::Table { alignments, .. }) => {
                    child_table = Some(alignments);
                    Some("table")
                }
                _ => panic!("table without column metadata"),
            },
            NodeKind::TableRow => match detail {
                Some(DetailRow::Row { header, .. }) => {
                    child_header = header;
                    Some("tr")
                }
                _ => panic!("table row without header state"),
            },
            NodeKind::TableCell => {
                let Some(DetailRow::Cell { column, .. }) = detail else {
                    panic!("cell without column metadata")
                };
                let alignment =
                    ir.alignments()[table.expect("cell outside a table").start() + column as usize];
                attributes.push(alignment_name(alignment).map(str::to_owned));
                Some(if header { "th" } else { "td" })
            }
            NodeKind::Text | NodeKind::SoftBreak => None,
            NodeKind::HardBreak => {
                close = 0;
                Some("br")
            }
            NodeKind::InlineCode => {
                attributes.push(None);
                Some("code")
            }
            NodeKind::Emphasis => Some("em"),
            NodeKind::Strong => Some("strong"),
            NodeKind::Strike => Some("del"),
            NodeKind::Link => match detail {
                Some(DetailRow::Link {
                    destination, title, ..
                }) => {
                    attributes = vec![
                        Some(normalized_url(self.payload(destination))),
                        title.map(|id| self.payload(id).to_owned()),
                    ];
                    Some("a")
                }
                _ => panic!("link without a destination"),
            },
            NodeKind::Image | NodeKind::TaskCheckbox => {
                let event = ir
                    .objects()
                    .iter()
                    .copied()
                    .find_map(|object| match object {
                        ObjectRow::Image {
                            node,
                            destination,
                            title,
                            alt,
                            ..
                        } if node == id => Some(Event::Open(
                            "img".into(),
                            vec![
                                Some(normalized_url(self.payload(destination))),
                                title.map(|id| self.payload(id).to_owned()),
                                Some(flow_text(self.model, alt, self.op)),
                            ],
                        )),
                        ObjectRow::Task { node, checked, .. } if node == id => {
                            Some(Event::Open("input".into(), vec![checked.then(String::new)]))
                        }
                        _ => None,
                    });
                self.events
                    .push(event.expect("image or checkbox without an object"));
                // The image alt flow is separate from the rendered document tree.
                return;
            }
            NodeKind::HtmlBlock | NodeKind::HtmlInline | NodeKind::HtmlComment => {
                panic!("unexpected raw HTML node in a Markdown-only case")
            }
            NodeKind::AltRoot
            | NodeKind::Math
            | NodeKind::MathBlock
            | NodeKind::Diagram
            | NodeKind::FootnoteDefinition
            | NodeKind::FootnoteReference
            | NodeKind::FootnoteFooter
            | NodeKind::FootnoteBacklink
            | NodeKind::DescriptionList
            | NodeKind::DescriptionItem
            | NodeKind::DescriptionTerm
            | NodeKind::DescriptionDetails
            | NodeKind::Highlight
            | NodeKind::Superscript
            | NodeKind::Subscript
            | NodeKind::TableOfContents
            | NodeKind::TableOfContentsEntry
            | NodeKind::NoteEmbed => {
                panic!("unexpected {:?} in the specification profiles", node.kind)
            }
        };
        if let Some(tag) = tag {
            self.events.push(Event::Open(tag.into(), attributes));
        }
        if matches!(
            node.kind,
            NodeKind::Text | NodeKind::SoftBreak | NodeKind::InlineCode | NodeKind::CodeBlock
        ) && let Some(text) = self.text_by_owner.get(&id)
        {
            append_text(&mut self.events, text);
        }
        let mut child = node.first_child;
        while let Some(id) = child {
            self.walk(id, child_tight, child_header, child_table);
            child = self.model.ir().nodes()[id.index()].next_sibling;
        }
        if let Some(tag) = tag.filter(|_| close > 0) {
            self.events.push(Event::Close(tag.into()));
            if close == 2 {
                self.events.push(Event::Close("pre".into()));
            }
        }
    }
}

fn alignment_name(alignment: Alignment) -> Option<&'static str> {
    match alignment {
        Alignment::None => None,
        Alignment::Left => Some("left"),
        Alignment::Center => Some("center"),
        Alignment::Right => Some("right"),
    }
}

fn text_by_owner(model: &AcceptedModel, op: &mut Operation) -> HashMap<NodeId, String> {
    let ir = model.ir();
    let atoms = ir.flows()[model.main_flow_id().index()].atoms;
    let mut text = HashMap::new();
    for index in atoms.start()..atoms.start() + atoms.len() {
        let atom = ir.atom_at(index, op).unwrap();
        if !matches!(atom.kind, AtomKind::Hidden | AtomKind::Object) {
            if atom.kind == AtomKind::Synthetic
                && ir.authority(atom.claim, op).unwrap().kind
                    == ClaimKind::ProjectionSeparator(SyntheticKind::BlockLineFeed)
            {
                continue;
            }
            append_atom(
                model,
                atom,
                text.entry(atom.owner).or_insert_with(String::new),
            );
        }
    }
    text
}

fn append_atom(model: &AcceptedModel, atom: markdown_core::ir::AtomRow, text: &mut String) {
    let ir = model.ir();
    if atom.kind == AtomKind::Copy {
        for source in atom.sources.start()..atom.sources.start() + atom.sources.len() {
            let span = ir.sources()[source];
            text.push_str(&model.source().text()[span.start() as usize..span.end() as usize]);
        }
    } else if let AtomContent::Pool(value) = atom.content {
        text.push_str(ir.text(value).unwrap());
    } else {
        panic!("unexpected visible atom content");
    }
}

fn flow_text(model: &AcceptedModel, flow: FlowId, op: &mut Operation) -> String {
    let ir = model.ir();
    let atoms = ir.flows()[flow.index()].atoms;
    let mut text = String::new();
    for index in atoms.start()..atoms.start() + atoms.len() {
        let atom = ir.atom_at(index, op).expect("read an admitted model atom");
        match (atom.kind, atom.content) {
            (AtomKind::Hidden, _) => {}
            (AtomKind::Object, AtomContent::Object(object)) => {
                if flow != model.main_flow_id()
                    && let ObjectRow::Image { alt, .. } = ir.objects()[object.index()]
                {
                    text.push_str(&flow_text(model, alt, op));
                }
            }
            _ => append_atom(model, atom, &mut text),
        }
    }
    text
}

fn contains_raw_html(input: &str, profile: Profile) -> bool {
    use comrak::nodes::NodeValue;
    // Classify fixtures before entering the Notepads model. A model defect cannot
    // turn a failed Markdown comparison into a raw HTML exclusion.
    let arena = comrak::Arena::new();
    let mut options = comrak::Options::default();
    if profile == Profile::G3 {
        options.extension.table = true;
        options.extension.tasklist = true;
        options.extension.strikethrough = true;
        options.extension.autolink = true;
    }
    comrak::parse_document(&arena, input, &options)
        .descendants()
        .any(|node| {
            matches!(
                node.data().value,
                NodeValue::HtmlBlock(_) | NodeValue::HtmlInline(_)
            )
        })
}

type LinkAttrs = (String, String);

#[derive(Debug, PartialEq)]
struct StyledChar {
    value: char,
    flags: u32,
    link: Option<LinkAttrs>,
}

fn styled_oracle(events: &[Event]) -> Vec<StyledChar> {
    let mut symbols = Vec::new();
    let mut containers = Vec::new();
    for event in events {
        let text = match event {
            Event::Open(tag, _) if tag == "img" => "\u{fffc}",
            Event::Open(tag, _) if matches!(tag.as_str(), "input" | "br" | "hr") => continue,
            Event::Open(tag, attrs) => {
                containers.push((tag.as_str(), attrs));
                continue;
            }
            Event::Close(_) => {
                containers.pop().unwrap();
                continue;
            }
            Event::Text(text) => text,
        };
        let mut flags = 0;
        let mut link = None;
        for (tag, attrs) in &containers {
            flags |= match *tag {
                "strong" => RUN_BOLD,
                "em" => RUN_ITALIC,
                "del" => RUN_STRIKE,
                "code" => RUN_CODE,
                _ => 0,
            };
            if *tag == "a" {
                link = Some((
                    attrs[0].clone().unwrap(),
                    attrs[1].clone().unwrap_or_default(),
                ));
            }
        }
        symbols.extend(
            text.chars()
                .filter(|ch| !ch.is_ascii_whitespace())
                .map(|value| StyledChar {
                    value,
                    flags,
                    link: link.clone(),
                }),
        );
    }
    symbols
}

fn check_projection(
    presentation: &PresentationBatch,
    expected: &[Event],
    expected_text: &str,
) -> Result<(), String> {
    let blob = |part: BlobSlice| {
        let start = part.start as usize;
        std::str::from_utf8(&presentation.blob()[start..start + part.len as usize]).unwrap()
    };
    let actual_text = without_layout_whitespace(
        &presentation
            .texts()
            .iter()
            .map(|row| blob(row.value))
            .collect::<String>()
            .replace('\u{fffc}', ""),
    );
    if actual_text != expected_text {
        return Err(format!(
            "presentation text: expected {expected_text:?}, got {actual_text:?}"
        ));
    }
    let mut styled = Vec::new();
    let mut image_positions: std::collections::HashSet<_> = presentation
        .images()
        .iter()
        .map(|row| (row.text as usize, row.offset_utf16))
        .collect();
    if image_positions.len() != presentation.images().len() {
        return Err("presentation images share a text offset".into());
    }
    for (index, text) in presentation.texts().iter().enumerate() {
        let runs = &presentation.runs()[text.runs.start()..text.runs.start() + text.runs.len()];
        let mut offset = 0;
        for value in blob(text.value).chars() {
            let start = offset;
            offset += value.len_utf16() as u32;
            if value.is_ascii_whitespace()
                || (value == '\u{fffc}' && !image_positions.remove(&(index, start)))
            {
                continue;
            }
            let run = runs
                .iter()
                .find(|run| run.start <= start && start < run.end);
            let flags =
                run.map_or(0, |run| run.flags) & (RUN_BOLD | RUN_CODE | RUN_ITALIC | RUN_STRIKE);
            let link = run.and_then(|run| run.link).map(|index| {
                let row = &presentation.links()[index as usize];
                (
                    normalized_url(blob(row.destination)),
                    blob(row.title).into(),
                )
            });
            styled.push(StyledChar { value, flags, link });
        }
    }
    if !image_positions.is_empty() {
        return Err(format!(
            "presentation images lack a text placeholder at {image_positions:?}"
        ));
    }
    let expected_styles = styled_oracle(expected);
    if styled != expected_styles {
        let first = styled
            .iter()
            .zip(&expected_styles)
            .position(|(a, b)| a != b)
            .unwrap_or_else(|| styled.len().min(expected_styles.len()));
        return Err(format!(
            "presentation run at character {first}: expected {:?}, got {:?}",
            expected_styles.get(first),
            styled.get(first)
        ));
    }
    let images: Vec<_> = expected
        .iter()
        .filter_map(|event| match event {
            Event::Open(tag, attrs) if tag == "img" => Some([
                attrs[0].as_deref().unwrap(),
                attrs[1].as_deref().unwrap_or(""),
                attrs[2].as_deref().unwrap(),
            ]),
            _ => None,
        })
        .collect();
    let actual_images: Vec<_> = presentation
        .images()
        .iter()
        .map(|row| {
            [
                normalized_url(blob(row.destination)),
                blob(row.title).into(),
                blob(row.alt).into(),
            ]
        })
        .collect();
    if actual_images != images {
        return Err(format!(
            "presentation images: expected {images:?}, got {actual_images:?}"
        ));
    }
    let cells: Vec<_> = expected
        .iter()
        .filter_map(|event| match event {
            Event::Open(tag, attrs) if matches!(tag.as_str(), "th" | "td") => {
                Some((tag == "th", attrs[0].as_deref()))
            }
            _ => None,
        })
        .collect();
    let actual_cells: Vec<_> = presentation
        .cells()
        .iter()
        .map(|row| (row.header, alignment_name(row.alignment)))
        .collect();
    if actual_cells != cells {
        return Err(format!(
            "presentation cells: expected {cells:?}, got {actual_cells:?}"
        ));
    }
    let code_blocks: Vec<_> = expected
        .windows(3)
        .filter_map(|events| match events {
            [Event::Open(tag, _), Event::Open(code, attrs), content]
                if tag == "pre" && code == "code" =>
            {
                let text = if let Event::Text(text) = content {
                    text.as_str()
                } else {
                    ""
                };
                Some((attrs[0].as_deref(), text))
            }
            _ => None,
        })
        .collect();
    let actual_code_blocks: Vec<_> = presentation
        .blocks()
        .iter()
        .filter(|row| row.request.kind == BlockKind::CodeBlock)
        .map(|row| {
            (
                blob(row.code_info).split_ascii_whitespace().next(),
                blob(presentation.texts()[row.texts.start()].value),
            )
        })
        .collect();
    if actual_code_blocks != code_blocks {
        return Err(format!(
            "presentation code: expected {code_blocks:?}, got {actual_code_blocks:?}"
        ));
    }
    let expected_tasks: Vec<_> = expected
        .iter()
        .filter_map(|event| match event {
            Event::Open(tag, attrs) if tag == "input" => Some(attrs[0].is_some()),
            _ => None,
        })
        .collect();
    let mut tasks = std::collections::HashSet::new();
    let actual_tasks: Vec<_> = presentation
        .decorations()
        .iter()
        .filter(|row| {
            row.kind == DecorationKind::Task
                && tasks.insert(row.task.expect("task decoration object"))
        })
        .map(|row| row.checked)
        .collect();
    if actual_tasks != expected_tasks {
        return Err(format!(
            "presentation tasks: expected {expected_tasks:?}, got {actual_tasks:?}"
        ));
    }
    Ok(())
}

fn replay(input: &str, profile: Profile, expected: &str) -> Result<bool, String> {
    let raw_html = contains_raw_html(input, profile);
    let oracle = (!raw_html).then(|| oracle(expected));
    let budget = SourceBudget::preview();
    let mut op = budget.begin(CancellationToken::new());
    let source = SourceDocument::new(
        SourceKey::new([0x63; 16], 1).unwrap(),
        input.as_bytes(),
        &mut op,
    )
    .map_err(|error| format!("source: {error:?}"))?;
    let model = AcceptedModel::parse(source.snapshot(), ParseConfig::new(profile), 1, &mut op)
        .map_err(|error| format!("model: {error:?}"))?;
    let key = LayoutKey::new(
        model.identity(),
        1,
        1,
        1,
        720.0,
        LayoutStyle::markdown(16.0, 8.0).unwrap(),
    )
    .unwrap();
    let mut layout = LayoutDraft::new(model.clone(), key, &mut op)
        .map_err(|error| format!("layout: {error:?}"))?;
    assert!(layout.blocks_len() <= MAX_MEASUREMENT_BATCH);
    if let Some(batch) = layout
        .measurement_batch(MAX_MEASUREMENT_BATCH, &mut op)
        .map_err(|error| format!("measurement requests: {error:?}"))?
    {
        let presentation = PresentationBatch::build(model.clone(), &batch, &mut op)
            .map_err(|error| format!("presentation: {error:?}"))?;
        if let Some((events, text)) = &oracle {
            check_projection(&presentation, events, text)?;
        }
    } else if oracle.as_ref().is_some_and(|(events, _)| {
        events
            .iter()
            .any(|event| matches!(event, Event::Open(tag, _) if tag != "blockquote"))
    }) {
        return Err("missing presentation for a nonempty Markdown case".into());
    }
    let Some((expected_structure, expected_text)) = oracle else {
        // Render these through the native model and presentation projection above.
        // Literal fallback and the safe HTML subset have their own native oracles.
        return Ok(true);
    };
    let mut structure = ModelStructure {
        model: &model,
        events: Vec::new(),
        text_by_owner: text_by_owner(&model, &mut op),
        op: &mut op,
    };
    structure.walk(
        model.ir().flows()[model.main_flow_id().index()].root,
        false,
        false,
        None,
    );
    let actual_structure = normalize_events(structure.events, false);
    if actual_structure != expected_structure {
        return Err(format!(
            "structure/attributes: expected {expected_structure:?}, got {:?}",
            actual_structure
        ));
    }
    let actual_text = without_layout_whitespace(&flow_text(&model, model.main_flow_id(), &mut op));
    if actual_text != expected_text {
        return Err(format!(
            "text: expected {expected_text:?}, got {actual_text:?}"
        ));
    }
    Ok(false)
}

#[test]
fn commonmark_and_gfm_upstream_baselines() {
    let directory = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/m1");
    let manifest: Value =
        serde_json::from_slice(&std::fs::read(directory.join("manifest.json")).unwrap()).unwrap();
    let mut totals = [0; 2];
    let mut structured = [0; 2];
    let mut raw = [0; 2];
    let mut differences = Vec::new();
    for row in manifest["fixtures"].as_array().unwrap() {
        if row["adoption_status"] != "required" {
            continue;
        }
        let id = row["id"].as_str().unwrap();
        let (profile, index) = if id.starts_with("cm.") {
            (Profile::C0, 0)
        } else if id.starts_with("gfm.") {
            (Profile::G3, 1)
        } else {
            continue;
        };
        totals[index] += 1;
        let input = std::fs::read_to_string(directory.join(row["input"]["path"].as_str().unwrap()))
            .unwrap();
        let html = std::fs::read_to_string(
            directory.join(row["oracles"]["html"]["path"].as_str().unwrap()),
        )
        .unwrap();
        match replay(&input, profile, &html) {
            Ok(false) => structured[index] += 1,
            Ok(true) => raw[index] += 1,
            Err(difference) => differences.push(format!("{id}: {difference}")),
        }
    }
    assert_eq!(totals, [652, 23]);
    assert_eq!(raw, [72, 0]);
    println!(
        "CommonMark/GFM totals: {totals:?}; structure matches: {structured:?}; raw HTML projection: {raw:?}"
    );
    assert!(
        differences.is_empty(),
        "{} differences:\n{}",
        differences.len(),
        differences.join("\n")
    );
}
