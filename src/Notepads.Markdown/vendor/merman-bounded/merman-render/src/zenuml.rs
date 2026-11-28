//! Headless ZenUML geometry derived from the selected ZenUML Core SVG pipeline.

mod theme;

use crate::Result;
use crate::model::Bounds;
use crate::text::{TextMeasurer, TextStyle};
use merman_core::diagrams::zenuml::{
    ZenumlDiagramRenderModel, ZenumlFragmentKind, ZenumlMessageStyle, ZenumlStatement,
    ZenumlStatementKind,
};
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::collections::{BTreeMap, HashMap, HashSet};

pub(crate) use theme::ZenumlSourceStyle;

pub(crate) const ZENUML_FRAME_TITLE_CLASS: &str = "frame-title";

const MARGIN: f64 = 20.0;
const MIN_PARTICIPANT_WIDTH: f64 = 80.0;
const PARTICIPANT_MAX_WIDTH: f64 = 250.0;
const PARTICIPANT_VISUAL_HEIGHT: f64 = 40.0;
const PARTICIPANT_TOP: f64 = 28.0;
const PARTICIPANT_BOX_PADDING: f64 = 16.0;
const PARTICIPANT_ICON_ROW_WIDTH: f64 = 28.0;
const PARTICIPANT_EMOJI_WIDTH: f64 = 20.0;
const ARROW_HEAD_WIDTH: f64 = 10.0;
const OCCURRENCE_WIDTH: f64 = 15.0;
const OCCURRENCE_BAR_SIDE_WIDTH: f64 = (OCCURRENCE_WIDTH - 1.0) / 2.0;
const ROOT_BLOCK_TOP: f64 = 56.0;
const STATEMENT_MARGIN: f64 = 16.0;
const COMMENT_LINE_HEIGHT: f64 = 20.0;
const MESSAGE_HEIGHT: f64 = 16.0;
const SELF_SYNC_MESSAGE_HEIGHT: f64 = 30.0;
const SELF_ASYNC_MESSAGE_HEIGHT: f64 = 44.0;
const CREATION_MESSAGE_HEIGHT: f64 = 40.0;
const OCCURRENCE_EMPTY_HEIGHT: f64 = 24.0;
const OCCURRENCE_BORDER_BOTTOM: f64 = 2.0;
// ZenUML Core's positioning VM reserves 12px, then its SVG geometry compensates the
// assignment-return occurrence by 4px. This renderer owns final geometry directly.
const ASSIGNMENT_RETURN_HEIGHT: f64 = 16.0;
const FRAGMENT_HEADER_HEIGHT: f64 = 25.0;
const FRAGMENT_BORDER_WIDTH: f64 = 1.0;
const FRAGMENT_BRANCH_LABEL_HEIGHT: f64 = 20.0;
const FRAGMENT_BRANCH_MARGIN: f64 = 8.0;
const FRAGMENT_PADDING_BOTTOM: f64 = 10.0;
const FRAGMENT_PADDING_X: f64 = 10.0;
const FRAGMENT_MIN_WIDTH: f64 = 100.0;
const PAR_CHILD_SEPARATOR: f64 = 1.0;
const DIVIDER_HEIGHT: f64 = 40.0;
const SVG_CONTENT_BOTTOM_SPACE: f64 = 13.0;
const RETURN_BOTTOM_SPACE: f64 = 46.0;
const MESSAGE_LABEL_PADDING: f64 = 10.0;
const DEFAULT_STARTER: &str = "_STARTER_";

pub(crate) fn resolve_zenuml_title<'a>(
    model: &'a ZenumlDiagramRenderModel,
    diagram_title: Option<&'a str>,
) -> Option<&'a str> {
    model
        .title
        .as_deref()
        .filter(|title| !title.is_empty())
        .or_else(|| diagram_title.filter(|title| !title.is_empty()))
}

#[derive(Debug, Default)]
pub(crate) struct ZenumlTypography {
    font_family: Option<String>,
    font_size: Option<f64>,
}

impl ZenumlTypography {
    pub(crate) fn from_config(config: &merman_core::MermaidConfig) -> Self {
        let namespace = crate::config::value_at(config.as_value(), &["themeVariables", "zenuml"]);
        let host = merman_core::presentation_font_defaults_active_internal(config);
        let property = |key| {
            namespace.and_then(|value| value.get(key)).or_else(|| {
                host.then(|| {
                    config
                        .as_value()
                        .get("themeVariables")
                        .and_then(|value| value.get(key))
                })
                .flatten()
            })
        };
        Self {
            font_family: property("fontFamily")
                .and_then(serde_json::Value::as_str)
                .map(str::to_owned),
            font_size: property("fontSize")
                .and_then(crate::config::json_f64_css_px)
                .filter(|size| *size > 0.0),
        }
    }

    pub(crate) fn vertical_scale(&self) -> f64 {
        self.font_size.map_or(1.0, |size| size / 16.0)
    }

    pub(crate) fn size(&self, source_size: f64) -> f64 {
        source_size * self.vertical_scale()
    }

    pub(crate) fn css_family(&self) -> Cow<'_, str> {
        self.font_family.as_deref().map_or_else(
            || Cow::Borrowed("Helvetica,Verdana,serif"),
            |font_family| Cow::Owned(crate::config::normalize_css_font_family(font_family)),
        )
    }

    fn style(&self, source_size: f64) -> TextStyle {
        TextStyle {
            font_family: Some(
                self.font_family
                    .clone()
                    .unwrap_or_else(|| "Helvetica, Verdana, serif".to_string()),
            ),
            font_size: self.size(source_size),
            font_weight: None,
            font_style: None,
        }
    }

    fn message_style(&self) -> TextStyle {
        // Preserve the companion's 16px geometry/14px paint default. An authored
        // presentation font instead measures the exact size used by the writer.
        self.style(if self.font_family.is_some() || self.font_size.is_some() {
            14.0
        } else {
            16.0
        })
    }

    fn styled_message<'a>(
        &self,
        base: &'a TextStyle,
        authored: &BTreeMap<String, String>,
    ) -> Cow<'a, TextStyle> {
        if (self.font_family.is_some() || self.font_size.is_some())
            && (authored.contains_key("font-weight") || authored.contains_key("font-style"))
        {
            let mut style = base.clone();
            style.font_weight = authored.get("font-weight").cloned();
            style.font_style = authored.get("font-style").cloned();
            Cow::Owned(style)
        } else {
            Cow::Borrowed(base)
        }
    }

    fn statement_message_style<'a>(
        &self,
        base: &'a TextStyle,
        comment: Option<&str>,
    ) -> Cow<'a, TextStyle> {
        if (self.font_family.is_some() || self.font_size.is_some())
            && let Some(comment) = comment
        {
            self.styled_message(base, &parse_comment(comment).message_style)
        } else {
            Cow::Borrowed(base)
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ZenumlDiagramLayout {
    pub width: f64,
    pub height: f64,
    pub frame_border_left: f64,
    pub frame_border_right: f64,
    pub participants: Vec<ZenumlParticipantLayout>,
    pub lifelines: Vec<ZenumlLifelineLayout>,
    pub messages: Vec<ZenumlMessageLayout>,
    pub self_calls: Vec<ZenumlSelfCallLayout>,
    pub creations: Vec<ZenumlCreationLayout>,
    pub returns: Vec<ZenumlReturnLayout>,
    pub occurrences: Vec<ZenumlOccurrenceLayout>,
    pub fragments: Vec<ZenumlFragmentLayout>,
    pub dividers: Vec<ZenumlDividerLayout>,
    pub comments: Vec<ZenumlCommentLayout>,
    pub groups: Vec<ZenumlGroupLayout>,
    pub bounds: Bounds,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ZenumlParticipantLayout {
    pub name: String,
    pub label: String,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub is_starter: bool,
    pub label_width: f64,
    pub stereotype_width: Option<f64>,
    pub participant_type: Option<String>,
    pub stereotype: Option<String>,
    pub color: Option<String>,
    pub emoji: Option<String>,
    pub group_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ZenumlLifelineLayout {
    pub participant_name: String,
    pub x: f64,
    pub top_y: f64,
    pub bottom_y: f64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ZenumlArrowStyle {
    Solid,
    Dashed,
    Open,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ZenumlMessageLayout {
    pub statement_id: String,
    pub number: String,
    pub from: String,
    pub to: String,
    pub from_x: f64,
    pub to_x: f64,
    pub y: f64,
    pub label: String,
    pub arrow_style: ZenumlArrowStyle,
    pub is_reverse: bool,
    pub style: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ZenumlSelfCallLayout {
    pub statement_id: String,
    pub number: String,
    pub participant_name: String,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub label: String,
    pub arrow_style: ZenumlArrowStyle,
    pub style: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ZenumlCreationLayout {
    pub statement_id: String,
    pub participant: ZenumlParticipantLayout,
    pub message: ZenumlMessageLayout,
}

#[derive(Debug)]
struct PendingCreationLayout {
    statement_id: String,
    participant_name: String,
    message: ZenumlMessageLayout,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ZenumlReturnLayout {
    pub statement_id: String,
    pub number: String,
    pub from: String,
    pub to: String,
    pub from_x: f64,
    pub to_x: f64,
    pub y: f64,
    pub label: String,
    pub is_reverse: bool,
    pub is_self: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ZenumlOccurrenceLayout {
    pub statement_id: String,
    pub participant_name: String,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ZenumlFragmentLayout {
    pub statement_id: String,
    pub kind: ZenumlLayoutFragmentKind,
    pub label: String,
    pub label_width: Option<f64>,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub header_y: f64,
    pub sections: Vec<ZenumlFragmentSectionLayout>,
    pub number: String,
    pub depth: usize,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ZenumlLayoutFragmentKind {
    Loop,
    Alternative,
    Parallel,
    Optional,
    Critical,
    Section,
    TryCatchFinally,
    Reference,
}

impl From<ZenumlFragmentKind> for ZenumlLayoutFragmentKind {
    fn from(value: ZenumlFragmentKind) -> Self {
        match value {
            ZenumlFragmentKind::Loop => Self::Loop,
            ZenumlFragmentKind::Alternative => Self::Alternative,
            ZenumlFragmentKind::Parallel => Self::Parallel,
            ZenumlFragmentKind::Optional => Self::Optional,
            ZenumlFragmentKind::Critical => Self::Critical,
            ZenumlFragmentKind::Section => Self::Section,
            ZenumlFragmentKind::TryCatchFinally => Self::TryCatchFinally,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ZenumlFragmentSectionLayout {
    pub label: String,
    pub y: f64,
    pub height: f64,
    pub label_width: Option<f64>,
    pub inner_label: Option<String>,
    pub inner_label_width: Option<f64>,
    pub keyword: Option<String>,
    pub keyword_width: Option<f64>,
    pub detail: Option<String>,
    pub detail_width: Option<f64>,
    pub content_inset_left: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ZenumlDividerLayout {
    pub statement_id: String,
    pub y: f64,
    pub width: f64,
    pub label: String,
    pub label_width: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ZenumlCommentLayout {
    pub statement_id: String,
    pub x: f64,
    pub y: f64,
    pub text: String,
    pub style: BTreeMap<String, String>,
    pub fragment_comment: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ZenumlGroupLayout {
    pub name: String,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

pub(crate) fn layout_zenuml_diagram_typed(
    model: &ZenumlDiagramRenderModel,
    typography: &ZenumlTypography,
    measurer: &dyn TextMeasurer,
) -> Result<ZenumlDiagramLayout> {
    let participant_style = typography.style(16.0);
    let stereotype_style = typography.style(16.0);
    let message_geometry_style = typography.message_style();
    let fragment_label_style = typography.style(14.0);

    let mut participants = Vec::with_capacity(model.participants.len());
    let mut internal_widths = Vec::with_capacity(model.participants.len());
    for participant in &model.participants {
        let label = if participant.name == "_STARTER_" {
            String::new()
        } else {
            participant.display_name().to_string()
        };
        let label_width = measurer.measure(&label, &participant_style).width;
        let icon_width = participant
            .participant_type
            .as_ref()
            .and_then(|kind| zenuml_participant_icon_key(kind))
            .map_or(0.0, |_| {
                PARTICIPANT_ICON_ROW_WIDTH * typography.vertical_scale()
            });
        let emoji_width = participant.emoji.as_ref().map_or(0.0, |_| {
            PARTICIPANT_EMOJI_WIDTH * typography.vertical_scale()
        });
        let stereotype_width = participant.stereotype.as_ref().map(|stereotype| {
            measurer
                .measure(&format!("«{stereotype}»"), &stereotype_style)
                .width
        });
        let visual_width = (label_width + PARTICIPANT_BOX_PADDING + icon_width + emoji_width)
            .max(stereotype_width.map_or(0.0, |width| width + 8.0))
            .clamp(MIN_PARTICIPANT_WIDTH, PARTICIPANT_MAX_WIDTH);
        let internal_width = (label_width
            + participant
                .participant_type
                .as_ref()
                .and_then(|kind| zenuml_participant_icon_key(kind))
                .map_or(0.0, |_| 40.0 * typography.vertical_scale())
            + participant
                .emoji
                .as_ref()
                .map_or(0.0, |_| 24.0 * typography.vertical_scale()))
        .max(MIN_PARTICIPANT_WIDTH)
            + MARGIN;
        internal_widths.push(internal_width);
        participants.push(ZenumlParticipantLayout {
            name: participant.name.clone(),
            label,
            x: 0.0,
            y: PARTICIPANT_TOP,
            width: visual_width,
            height: PARTICIPANT_VISUAL_HEIGHT,
            is_starter: participant.name == "_STARTER_",
            label_width,
            stereotype_width,
            participant_type: participant.participant_type.clone(),
            stereotype: participant.stereotype.clone(),
            color: participant.color.clone(),
            emoji: participant.emoji.clone(),
            group_id: participant.group_id.clone(),
        });
    }

    let mut x = internal_widths.first().copied().unwrap_or(0.0) / 2.0;
    for (index, participant) in participants.iter_mut().enumerate() {
        if index > 0 {
            x += internal_widths[index - 1] / 2.0 + internal_widths[index] / 2.0;
        }
        participant.x = x;
    }
    apply_message_width_constraints(
        model,
        &mut participants,
        measurer,
        typography,
        &message_geometry_style,
    );
    let positions: HashMap<String, f64> = participants
        .iter()
        .map(|participant| (participant.name.clone(), participant.x))
        .collect();

    let participant_half_widths: HashMap<String, f64> = participants
        .iter()
        .zip(internal_widths.iter())
        .map(|(participant, width)| (participant.name.clone(), *width / 2.0))
        .collect();
    let mut builder = VerticalLayoutBuilder {
        positions: &positions,
        participant_half_widths: &participant_half_widths,
        measurer,
        fragment_label_style: &fragment_label_style,
        cursor_y: ROOT_BLOCK_TOP,
        messages: Vec::new(),
        self_calls: Vec::new(),
        creations: Vec::new(),
        returns: Vec::new(),
        max_return_bottom: 0.0,
        occurrences: Vec::new(),
        fragments: Vec::new(),
        dividers: Vec::new(),
        comments: Vec::new(),
        creation_y: HashMap::new(),
        active_occurrences: HashMap::new(),
    };
    builder.cursor_y = builder.layout_block(
        &model.statements,
        ROOT_BLOCK_TOP,
        BlockLayoutContext::root(),
    );
    builder
        .returns
        .sort_by(|left, right| left.y.total_cmp(&right.y));
    for index in 1..builder.returns.len() {
        let minimum_y = builder.returns[index - 1].y + MESSAGE_HEIGHT;
        if builder.returns[index].y < minimum_y {
            builder.returns[index].y = minimum_y;
        }
    }

    for participant in &mut participants {
        if let Some(y) = builder.creation_y.get(&participant.name).copied() {
            participant.y = y;
        }
    }

    let participant_width = participants.last().map_or(0.0, |participant| {
        participant.x
            + participant_half_widths
                .get(&participant.name)
                .copied()
                .unwrap_or_default()
    });
    let mut content_width = (participant_width
        + self_message_extra_width(
            model,
            &positions,
            &participant_half_widths,
            measurer,
            typography,
            &message_geometry_style,
        ))
    .max(FRAGMENT_MIN_WIDTH);
    for message in &builder.messages {
        let label_width = measurer
            .measure(
                &message.label,
                &typography.styled_message(&message_geometry_style, &message.style),
            )
            .width;
        content_width = content_width
            .max((message.from_x + message.to_x) / 2.0 + label_width / 2.0 + MESSAGE_LABEL_PADDING);
    }
    if typography.vertical_scale() != 1.0 {
        for call in &builder.self_calls {
            content_width = content_width.max(call.x + 1.0 + 30.0 * typography.vertical_scale());
        }
    }
    for creation in &builder.creations {
        let message = &creation.message;
        let label_width = measurer
            .measure(
                &message.label,
                &typography.styled_message(&message_geometry_style, &message.style),
            )
            .width;
        content_width = content_width
            .max((message.from_x + message.to_x) / 2.0 + label_width / 2.0 + MESSAGE_LABEL_PADDING);
    }
    for returned in &builder.returns {
        let label_width = measurer
            .measure(&returned.label, &message_geometry_style)
            .width;
        content_width = content_width.max(
            (returned.from_x + returned.to_x) / 2.0 + label_width / 2.0 + MESSAGE_LABEL_PADDING,
        );
    }
    for divider in &mut builder.dividers {
        divider.width = content_width;
    }
    for fragment in &mut builder.fragments {
        if fragment.width <= 0.0 {
            fragment.width = content_width;
        }
    }
    let max_occurrence_bottom = builder
        .occurrences
        .iter()
        .map(|occurrence| occurrence.y + occurrence.height + SVG_CONTENT_BOTTOM_SPACE)
        .max_by(f64::total_cmp)
        .unwrap_or_default();
    let max_fragment_bottom = builder
        .fragments
        .iter()
        .map(|fragment| fragment.y + fragment.height + SVG_CONTENT_BOTTOM_SPACE)
        .max_by(f64::total_cmp)
        .unwrap_or_default();
    let max_message_bottom = builder
        .messages
        .iter()
        .map(|message| message.y + SVG_CONTENT_BOTTOM_SPACE)
        .max_by(f64::total_cmp)
        .unwrap_or_default();
    let max_self_call_bottom = builder
        .self_calls
        .iter()
        .map(|message| message.y + message.height + SVG_CONTENT_BOTTOM_SPACE)
        .max_by(f64::total_cmp)
        .unwrap_or_default();
    let max_creation_bottom = builder
        .creations
        .iter()
        .map(|creation| creation.message.y + PARTICIPANT_VISUAL_HEIGHT + SVG_CONTENT_BOTTOM_SPACE)
        .max_by(f64::total_cmp)
        .unwrap_or_default();
    let max_return_bottom = builder.max_return_bottom;
    let height = (builder.cursor_y + 28.0)
        .max(max_occurrence_bottom)
        .max(max_fragment_bottom)
        .max(max_message_bottom)
        .max(max_self_call_bottom)
        .max(max_creation_bottom)
        .max(max_return_bottom);
    let lifelines = participants
        .iter()
        .map(|participant| ZenumlLifelineLayout {
            participant_name: participant.name.clone(),
            x: participant.x,
            top_y: participant.y + participant.height,
            bottom_y: height + PARTICIPANT_VISUAL_HEIGHT - 28.0,
        })
        .collect();
    let groups = layout_groups(&participants, height);
    let (frame_border_left, frame_border_right) = frame_border(&model.statements, &positions);
    for fragment in &mut builder.fragments {
        fragment.x = fragment.x - frame_border_left + fragment.depth as f64 * FRAGMENT_PADDING_X;
    }
    for comment in &mut builder.comments {
        if comment.fragment_comment {
            comment.x -= frame_border_left;
        }
    }
    let bounds = Bounds {
        min_x: 0.0,
        min_y: 0.0,
        max_x: content_width,
        max_y: height,
    };
    let creations = builder
        .creations
        .into_iter()
        .map(|creation| {
            let participant = participants
                .iter()
                .find(|participant| participant.name == creation.participant_name)
                .cloned()
                .ok_or_else(|| crate::Error::InvalidModel {
                    message: format!(
                        "ZenUML creation `{}` targets missing participant `{}`",
                        creation.statement_id, creation.participant_name
                    ),
                })?;
            Ok(ZenumlCreationLayout {
                statement_id: creation.statement_id,
                participant,
                message: creation.message,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let mut layout = ZenumlDiagramLayout {
        width: content_width,
        height,
        frame_border_left,
        frame_border_right,
        participants,
        lifelines,
        messages: builder.messages,
        self_calls: builder.self_calls,
        creations,
        returns: builder.returns,
        occurrences: builder.occurrences,
        fragments: builder.fragments,
        dividers: builder.dividers,
        comments: builder.comments,
        groups,
        bounds,
    };
    // ZenUML's source row and header geometry is proportional to its 16px base
    // font. Keep horizontal distances measured in the selected font and scale
    // the vertical reservations together with the writer's text offsets.
    let scale = typography.vertical_scale();
    if scale != 1.0 {
        layout.height *= scale;
        layout.bounds.min_y *= scale;
        layout.bounds.max_y *= scale;
        for participant in &mut layout.participants {
            participant.y *= scale;
            participant.height *= scale;
        }
        for lifeline in &mut layout.lifelines {
            lifeline.top_y *= scale;
            lifeline.bottom_y *= scale;
        }
        for message in &mut layout.messages {
            message.y *= scale;
        }
        for call in &mut layout.self_calls {
            call.y *= scale;
            call.width *= scale;
            call.height *= scale;
        }
        for creation in &mut layout.creations {
            creation.message.y *= scale;
            creation.participant.y *= scale;
            creation.participant.height *= scale;
        }
        for returned in &mut layout.returns {
            returned.y *= scale;
        }
        for occurrence in &mut layout.occurrences {
            occurrence.y *= scale;
            occurrence.height *= scale;
        }
        for fragment in &mut layout.fragments {
            fragment.y *= scale;
            fragment.height *= scale;
            fragment.header_y *= scale;
            for section in &mut fragment.sections {
                section.y *= scale;
                section.height *= scale;
            }
        }
        for divider in &mut layout.dividers {
            divider.y *= scale;
        }
        for comment in &mut layout.comments {
            comment.y *= scale;
        }
        for group in &mut layout.groups {
            group.y *= scale;
            group.height *= scale;
        }
    }
    if !finite_layout_geometry(&layout) {
        return Err(crate::Error::InvalidModel {
            message: "zenuml: the selected font produces non-finite layout geometry".to_string(),
        });
    }
    Ok(layout)
}

fn finite_layout_geometry(layout: &ZenumlDiagramLayout) -> bool {
    let finite = |values: &[f64]| values.iter().all(|value| value.is_finite());
    finite(&[
        layout.width,
        layout.height,
        layout.frame_border_left,
        layout.frame_border_right,
        layout.bounds.min_x,
        layout.bounds.min_y,
        layout.bounds.max_x,
        layout.bounds.max_y,
    ]) && layout
        .participants
        .iter()
        .chain(
            layout
                .creations
                .iter()
                .map(|creation| &creation.participant),
        )
        .all(|participant| {
            finite(&[
                participant.x,
                participant.y,
                participant.width,
                participant.height,
                participant.label_width,
            ]) && participant.stereotype_width.is_none_or(f64::is_finite)
        })
        && layout
            .lifelines
            .iter()
            .all(|line| finite(&[line.x, line.top_y, line.bottom_y]))
        && layout
            .messages
            .iter()
            .chain(layout.creations.iter().map(|creation| &creation.message))
            .all(|message| finite(&[message.from_x, message.to_x, message.y]))
        && layout
            .self_calls
            .iter()
            .all(|call| finite(&[call.x, call.y, call.width, call.height]))
        && layout
            .returns
            .iter()
            .all(|returned| finite(&[returned.from_x, returned.to_x, returned.y]))
        && layout.occurrences.iter().all(|occurrence| {
            finite(&[
                occurrence.x,
                occurrence.y,
                occurrence.width,
                occurrence.height,
            ])
        })
        && layout.fragments.iter().all(|fragment| {
            finite(&[
                fragment.x,
                fragment.y,
                fragment.width,
                fragment.height,
                fragment.header_y,
            ]) && fragment.label_width.is_none_or(f64::is_finite)
                && fragment.sections.iter().all(|section| {
                    finite(&[section.y, section.height])
                        && [
                            section.label_width,
                            section.inner_label_width,
                            section.keyword_width,
                            section.detail_width,
                            section.content_inset_left,
                        ]
                        .into_iter()
                        .all(|value| value.is_none_or(f64::is_finite))
                })
        })
        && layout
            .dividers
            .iter()
            .all(|divider| finite(&[divider.y, divider.width, divider.label_width]))
        && layout
            .comments
            .iter()
            .all(|comment| finite(&[comment.x, comment.y]))
        && layout
            .groups
            .iter()
            .all(|group| finite(&[group.x, group.y, group.width, group.height]))
}

fn apply_message_width_constraints(
    model: &ZenumlDiagramRenderModel,
    participants: &mut [ZenumlParticipantLayout],
    measurer: &dyn TextMeasurer,
    typography: &ZenumlTypography,
    message_geometry_style: &TextStyle,
) {
    let index: HashMap<String, usize> = participants
        .iter()
        .enumerate()
        .map(|(index, participant)| (participant.name.clone(), index))
        .collect();
    let mut constraints = Vec::new();
    collect_message_constraints(&model.statements, &mut constraints);
    for MessageConstraint {
        from,
        to,
        label,
        comment,
    } in constraints
    {
        let (Some(&from_index), Some(&to_index)) = (index.get(from), index.get(to)) else {
            continue;
        };
        if from_index == to_index {
            continue;
        }
        let (left, right) = if from_index < to_index {
            (from_index, to_index)
        } else {
            (to_index, from_index)
        };
        let required = measurer
            .measure(
                label,
                &typography.statement_message_style(message_geometry_style, comment),
            )
            .width
            + ARROW_HEAD_WIDTH
            + OCCURRENCE_WIDTH;
        let actual = participants[right].x - participants[left].x;
        if required > actual {
            let delta = required - actual;
            for participant in &mut participants[right..] {
                participant.x += delta;
            }
        }
    }
}

struct MessageConstraint<'a> {
    from: &'a str,
    to: &'a str,
    label: &'a str,
    comment: Option<&'a str>,
}

fn collect_message_constraints<'a>(
    statements: &'a [ZenumlStatement],
    out: &mut Vec<MessageConstraint<'a>>,
) {
    for statement in statements {
        match &statement.kind {
            ZenumlStatementKind::Message {
                resolved_from,
                resolved_to,
                label,
                style,
                body,
                ..
            } => {
                let (from, to) = message_render_endpoints(
                    *style,
                    resolved_from.as_deref(),
                    resolved_to.as_deref(),
                );
                out.push(MessageConstraint {
                    from,
                    to,
                    label,
                    comment: statement.comment.as_deref(),
                });
                collect_message_constraints(body, out);
            }
            ZenumlStatementKind::Creation {
                resolved_from,
                resolved_to,
                label,
                body,
                ..
            } => {
                let from = resolved_from.as_deref().unwrap_or(DEFAULT_STARTER);
                out.push(MessageConstraint {
                    from,
                    to: resolved_to,
                    label,
                    comment: statement.comment.as_deref(),
                });
                collect_message_constraints(body, out);
            }
            ZenumlStatementKind::Return {
                resolved_from,
                resolved_to,
                label,
                ..
            } => out.push(MessageConstraint {
                from: resolved_from.as_deref().unwrap_or(DEFAULT_STARTER),
                to: resolved_to.as_deref().unwrap_or(DEFAULT_STARTER),
                label,
                comment: None,
            }),
            ZenumlStatementKind::Fragment { sections, .. } => {
                for section in sections {
                    collect_message_constraints(&section.statements, out);
                }
            }
            ZenumlStatementKind::Reference { .. } | ZenumlStatementKind::Divider { .. } => {}
        }
    }
}

fn message_render_endpoints<'a>(
    style: ZenumlMessageStyle,
    from: Option<&'a str>,
    to: Option<&'a str>,
) -> (&'a str, &'a str) {
    let from = from.unwrap_or(DEFAULT_STARTER);
    let to = match style {
        ZenumlMessageStyle::Synchronous => to.unwrap_or(DEFAULT_STARTER),
        ZenumlMessageStyle::Asynchronous => to.unwrap_or(from),
    };
    (from, to)
}

#[derive(Debug)]
struct FragmentFrame {
    left: String,
    right: String,
    children: Vec<FragmentFrame>,
}

fn frame_border(statements: &[ZenumlStatement], positions: &HashMap<String, f64>) -> (f64, f64) {
    let Some(frame) = first_fragment_frame(statements, positions) else {
        return (0.0, 0.0);
    };
    frame_padding(&frame)
}

fn frame_padding(frame: &FragmentFrame) -> (f64, f64) {
    fn longest_path(frame: &FragmentFrame, left: bool) -> usize {
        let edge = if left { &frame.left } else { &frame.right };
        let child_depth = frame
            .children
            .iter()
            .filter(|child| {
                let child_edge = if left { &child.left } else { &child.right };
                child_edge == edge
            })
            .map(|child| longest_path(child, left))
            .max()
            .unwrap_or(0);
        child_depth + 1
    }

    (
        longest_path(frame, true) as f64 * FRAGMENT_PADDING_X,
        longest_path(frame, false) as f64 * FRAGMENT_PADDING_X,
    )
}

fn first_fragment_frame(
    statements: &[ZenumlStatement],
    positions: &HashMap<String, f64>,
) -> Option<FragmentFrame> {
    for statement in statements {
        if matches!(
            &statement.kind,
            ZenumlStatementKind::Fragment { .. } | ZenumlStatementKind::Reference { .. }
        ) {
            return Some(fragment_frame(statement, positions));
        }
        let nested = match &statement.kind {
            ZenumlStatementKind::Message { body, .. }
            | ZenumlStatementKind::Creation { body, .. } => first_fragment_frame(body, positions),
            _ => None,
        };
        if nested.is_some() {
            return nested;
        }
    }
    None
}

fn fragment_frame(statement: &ZenumlStatement, positions: &HashMap<String, f64>) -> FragmentFrame {
    let mut names = HashSet::new();
    let mut children = Vec::new();
    match &statement.kind {
        ZenumlStatementKind::Fragment { sections, .. } => {
            for section in sections {
                collect_participant_names(&section.statements, &mut names);
                collect_fragment_frames(&section.statements, positions, &mut children);
            }
        }
        ZenumlStatementKind::Reference { participants, .. } => {
            names.extend(participants.iter().cloned());
        }
        _ => unreachable!("fragment frames are only built for fragment statements"),
    }
    let mut ordered = names
        .into_iter()
        .filter_map(|name| positions.get(&name).copied().map(|x| (x, name)))
        .collect::<Vec<_>>();
    ordered.sort_by(|left, right| left.0.total_cmp(&right.0));
    FragmentFrame {
        left: ordered
            .first()
            .map(|(_, name)| name.clone())
            .unwrap_or_default(),
        right: ordered
            .last()
            .map(|(_, name)| name.clone())
            .unwrap_or_default(),
        children,
    }
}

fn collect_fragment_frames(
    statements: &[ZenumlStatement],
    positions: &HashMap<String, f64>,
    out: &mut Vec<FragmentFrame>,
) {
    for statement in statements {
        match &statement.kind {
            ZenumlStatementKind::Fragment { .. } | ZenumlStatementKind::Reference { .. } => {
                out.push(fragment_frame(statement, positions));
            }
            ZenumlStatementKind::Message { body, .. }
            | ZenumlStatementKind::Creation { body, .. } => {
                collect_fragment_frames(body, positions, out);
            }
            _ => {}
        }
    }
}

fn self_message_extra_width(
    model: &ZenumlDiagramRenderModel,
    positions: &HashMap<String, f64>,
    half_widths: &HashMap<String, f64>,
    measurer: &dyn TextMeasurer,
    typography: &ZenumlTypography,
    message_geometry_style: &TextStyle,
) -> f64 {
    let Some((right_name, right_position)) = positions
        .iter()
        .max_by(|left, right| left.1.total_cmp(right.1))
    else {
        return 0.0;
    };
    let right_half = half_widths.get(right_name).copied().unwrap_or_default();
    let mut constraints = Vec::new();
    collect_message_constraints(&model.statements, &mut constraints);
    constraints
        .into_iter()
        .filter(|constraint| constraint.from == constraint.to)
        .filter_map(
            |MessageConstraint {
                 from,
                 label,
                 comment,
                 ..
             }| {
                positions.get(from).map(|from_position| {
                    measurer
                        .measure(
                            label,
                            &typography.statement_message_style(message_geometry_style, comment),
                        )
                        .width
                        - (right_position - from_position)
                        - right_half
                })
            },
        )
        .fold(0.0, f64::max)
}

struct VerticalLayoutBuilder<'a> {
    positions: &'a HashMap<String, f64>,
    participant_half_widths: &'a HashMap<String, f64>,
    measurer: &'a dyn TextMeasurer,
    fragment_label_style: &'a TextStyle,
    cursor_y: f64,
    messages: Vec<ZenumlMessageLayout>,
    self_calls: Vec<ZenumlSelfCallLayout>,
    creations: Vec<PendingCreationLayout>,
    returns: Vec<ZenumlReturnLayout>,
    max_return_bottom: f64,
    occurrences: Vec<ZenumlOccurrenceLayout>,
    fragments: Vec<ZenumlFragmentLayout>,
    dividers: Vec<ZenumlDividerLayout>,
    comments: Vec<ZenumlCommentLayout>,
    creation_y: HashMap<String, f64>,
    active_occurrences: HashMap<String, usize>,
}

#[derive(Clone, Copy)]
struct BlockLayoutContext<'a> {
    inside_occurrence: bool,
    fragment_depth: usize,
    parent_number: &'a str,
    index_offset: usize,
}

#[derive(Debug, Default)]
struct ParsedComment {
    text: String,
    visual_lines: usize,
    comment_style: BTreeMap<String, String>,
    message_style: BTreeMap<String, String>,
}

impl BlockLayoutContext<'static> {
    const fn root() -> Self {
        Self {
            inside_occurrence: false,
            fragment_depth: 0,
            parent_number: "",
            index_offset: 0,
        }
    }
}

impl VerticalLayoutBuilder<'_> {
    fn layout_block(
        &mut self,
        statements: &[ZenumlStatement],
        start_top: f64,
        context: BlockLayoutContext<'_>,
    ) -> f64 {
        if statements.is_empty() {
            return start_top;
        }
        let mut cursor = start_top + STATEMENT_MARGIN;
        for (index, statement) in statements.iter().enumerate() {
            let ordinal = context.index_offset + index + 1;
            let number = if context.parent_number.is_empty() {
                ordinal.to_string()
            } else {
                format!("{}.{ordinal}", context.parent_number)
            };
            cursor = self.layout_statement(
                statement,
                &number,
                cursor,
                context.inside_occurrence,
                index + 1 == statements.len(),
                context.fragment_depth,
            ) + STATEMENT_MARGIN;
        }
        cursor
    }

    fn layout_parallel_block(
        &mut self,
        statements: &[ZenumlStatement],
        start_top: f64,
        context: BlockLayoutContext<'_>,
    ) -> (f64, Vec<f64>) {
        if statements.is_empty() {
            return (start_top, Vec::new());
        }
        let mut cursor = start_top + STATEMENT_MARGIN;
        let mut separators = Vec::with_capacity(statements.len().saturating_sub(1));
        for (index, statement) in statements.iter().enumerate() {
            if index != 0 {
                cursor += PAR_CHILD_SEPARATOR;
                // The selected companion paints the separator one pixel above the
                // following statement coordinate, then centers its 1px SVG stroke.
                separators.push(cursor - 1.0);
            }
            let ordinal = context.index_offset + index + 1;
            let number = if context.parent_number.is_empty() {
                ordinal.to_string()
            } else {
                format!("{}.{ordinal}", context.parent_number)
            };
            cursor = self.layout_statement(
                statement,
                &number,
                cursor,
                context.inside_occurrence,
                index + 1 == statements.len(),
                context.fragment_depth,
            ) + STATEMENT_MARGIN;
        }
        (cursor, separators)
    }

    fn layout_statement(
        &mut self,
        statement: &ZenumlStatement,
        number: &str,
        top: f64,
        inside_occurrence: bool,
        is_last: bool,
        fragment_depth: usize,
    ) -> f64 {
        let parsed_comment = statement.comment.as_deref().map(parse_comment);
        let comment_height = parsed_comment.as_ref().map_or(0.0, |comment| {
            comment.visual_lines as f64 * COMMENT_LINE_HEIGHT
        });
        let comment_index = parsed_comment
            .as_ref()
            .filter(|comment| !comment.text.is_empty())
            .map(|comment| {
                let fragment_comment = matches!(
                    &statement.kind,
                    ZenumlStatementKind::Fragment { .. } | ZenumlStatementKind::Reference { .. }
                );
                let index = self.comments.len();
                self.comments.push(ZenumlCommentLayout {
                    statement_id: statement.id.clone(),
                    x: self.statement_comment_x(statement, &comment.text),
                    y: top + 15.0 + if fragment_comment { 1.0 } else { 0.0 },
                    text: comment.text.clone(),
                    style: comment.comment_style.clone(),
                    fragment_comment,
                });
                index
            });
        let content_top = top + comment_height;
        let empty_message_style = BTreeMap::new();
        let statement_message_style = parsed_comment
            .as_ref()
            .map(|comment| &comment.message_style)
            .unwrap_or(&empty_message_style);

        match &statement.kind {
            ZenumlStatementKind::Message {
                resolved_from,
                resolved_to,
                label,
                assignment,
                style,
                body,
                ..
            } => {
                let (from, to) = message_render_endpoints(
                    *style,
                    resolved_from.as_deref(),
                    resolved_to.as_deref(),
                );
                self.layout_message(
                    statement,
                    number,
                    from,
                    to,
                    label,
                    assignment.as_deref(),
                    *style,
                    body,
                    content_top,
                    fragment_depth,
                    statement_message_style,
                )
            }
            ZenumlStatementKind::Creation {
                resolved_from,
                resolved_to,
                label,
                assignment,
                body,
                ..
            } => self.layout_creation(
                statement,
                number,
                resolved_from.as_deref().unwrap_or(DEFAULT_STARTER),
                resolved_to,
                label,
                assignment.as_deref(),
                body,
                content_top,
                fragment_depth,
                statement_message_style,
            ),
            ZenumlStatementKind::Return {
                resolved_from,
                resolved_to,
                label,
                ..
            } => {
                let from = resolved_from.as_deref().unwrap_or(DEFAULT_STARTER);
                let to = resolved_to.as_deref().unwrap_or(DEFAULT_STARTER);
                let is_self = from == to;
                let collapsed = !is_self && inside_occurrence && is_last;
                let (from_x, to_x) =
                    self.return_endpoints(from, to, self.active_depth(from), self.active_depth(to));
                let y = if collapsed {
                    content_top + 16.5
                } else {
                    content_top + 15.5
                };
                self.max_return_bottom = self
                    .max_return_bottom
                    .max(y + if collapsed { 45.5 } else { 44.5 });
                self.returns.push(ZenumlReturnLayout {
                    statement_id: statement.id.clone(),
                    number: number.to_string(),
                    from: from.to_string(),
                    to: to.to_string(),
                    from_x,
                    to_x,
                    y,
                    label: label.clone(),
                    is_reverse: to_x < from_x,
                    is_self,
                });
                // ZenUML Core's vertical VM collapses a final non-self return inside an
                // occurrence, then its SVG pipeline restores the missing 16px as return debt.
                content_top + if is_self { 20.0 } else { MESSAGE_HEIGHT }
            }
            ZenumlStatementKind::Fragment {
                fragment_kind,
                label,
                sections,
            } => {
                let mut cursor = content_top + FRAGMENT_BORDER_WIDTH + FRAGMENT_HEADER_HEIGHT;
                let mut section_layouts = Vec::with_capacity(sections.len());
                let mut names = HashSet::new();
                let mut section_offset = 0;
                if *fragment_kind == ZenumlFragmentKind::Parallel {
                    section_layouts.push(self.fragment_section_layout("", content_top));
                }

                for (index, section) in sections.iter().enumerate() {
                    collect_participant_names(&section.statements, &mut names);
                    let section_y;
                    match fragment_kind {
                        ZenumlFragmentKind::Alternative => {
                            if index == 0 {
                                section_y = cursor;
                                cursor += FRAGMENT_BRANCH_LABEL_HEIGHT;
                            } else {
                                section_y = cursor;
                                cursor += FRAGMENT_BRANCH_LABEL_HEIGHT
                                    + FRAGMENT_BRANCH_MARGIN
                                    + FRAGMENT_BORDER_WIDTH;
                            }
                        }
                        ZenumlFragmentKind::TryCatchFinally => {
                            section_y = cursor;
                            if index > 0 {
                                cursor += FRAGMENT_BRANCH_LABEL_HEIGHT
                                    + FRAGMENT_BRANCH_MARGIN
                                    + FRAGMENT_BORDER_WIDTH;
                            }
                        }
                        _ => {
                            section_y = cursor;
                            if section.label.is_some() {
                                cursor += FRAGMENT_BRANCH_LABEL_HEIGHT;
                            }
                        }
                    }
                    let block_context = BlockLayoutContext {
                        inside_occurrence,
                        fragment_depth: fragment_depth + 1,
                        parent_number: number,
                        index_offset: section_offset,
                    };
                    if *fragment_kind == ZenumlFragmentKind::Parallel {
                        let (next_cursor, separators) =
                            self.layout_parallel_block(&section.statements, cursor, block_context);
                        cursor = next_cursor;
                        section_layouts.extend(
                            separators
                                .into_iter()
                                .map(|y| self.fragment_section_layout("", y)),
                        );
                    } else {
                        cursor = self.layout_block(&section.statements, cursor, block_context);
                    }
                    let visible_label = match fragment_kind {
                        ZenumlFragmentKind::Alternative if index == 0 => "Alt".to_string(),
                        ZenumlFragmentKind::Alternative => section
                            .label
                            .as_deref()
                            .map(|label| {
                                if label == "else" {
                                    "[else]".to_string()
                                } else {
                                    format!("[ {label} ]")
                                }
                            })
                            .unwrap_or_default(),
                        ZenumlFragmentKind::TryCatchFinally if index == 0 => "Try".to_string(),
                        _ => section.label.clone().unwrap_or_default(),
                    };
                    if *fragment_kind != ZenumlFragmentKind::Parallel {
                        section_layouts
                            .push(self.fragment_section_layout(&visible_label, section_y));
                    }
                    section_offset += section.statements.len();
                }
                cursor += FRAGMENT_PADDING_BOTTOM + FRAGMENT_BORDER_WIDTH;
                let padding = frame_padding(&fragment_frame(statement, self.positions));
                let (x, width) = self.fragment_horizontal_bounds(&names, padding);
                if *fragment_kind == ZenumlFragmentKind::Parallel {
                    let left_name = names
                        .iter()
                        .filter(|name| self.positions.contains_key(*name))
                        .min_by(|left, right| self.position(left).total_cmp(&self.position(right)));
                    let inset = left_name
                        .map(|name| padding.0 + self.half_width(name))
                        .unwrap_or(padding.0);
                    for section in section_layouts.iter_mut().skip(1) {
                        section.content_inset_left = Some(inset);
                    }
                }
                if let Some(index) = comment_index {
                    self.comments[index].x = x + 1.0;
                }
                self.fragments.push(ZenumlFragmentLayout {
                    statement_id: statement.id.clone(),
                    kind: (*fragment_kind).into(),
                    label: label.clone().unwrap_or_default(),
                    label_width: label.as_ref().map(|label| {
                        let label = resolve_zenuml_emojis_in_text(label);
                        self.measurer
                            .measure(&label, self.fragment_label_style)
                            .width
                    }),
                    x,
                    y: top,
                    width,
                    height: cursor - top,
                    header_y: top + FRAGMENT_BORDER_WIDTH + comment_height,
                    sections: section_layouts,
                    number: number.to_string(),
                    depth: fragment_depth,
                });
                cursor
            }
            ZenumlStatementKind::Reference {
                participants,
                label,
            } => {
                let names = participants.iter().cloned().collect::<HashSet<_>>();
                let padding = frame_padding(&fragment_frame(statement, self.positions));
                let (x, width) = self.fragment_horizontal_bounds(&names, padding);
                if let Some(index) = comment_index {
                    self.comments[index].x = x + 1.0;
                }
                let end = content_top + FRAGMENT_HEADER_HEIGHT + FRAGMENT_PADDING_BOTTOM;
                self.fragments.push(ZenumlFragmentLayout {
                    statement_id: statement.id.clone(),
                    kind: ZenumlLayoutFragmentKind::Reference,
                    label: label.clone(),
                    label_width: Some({
                        let label = resolve_zenuml_emojis_in_text(label);
                        self.measurer
                            .measure(&label, self.fragment_label_style)
                            .width
                    }),
                    x,
                    y: top,
                    width,
                    height: end - top,
                    header_y: top + comment_height,
                    sections: Vec::new(),
                    number: number.to_string(),
                    depth: fragment_depth,
                });
                end
            }
            ZenumlStatementKind::Divider { label } => {
                let display_label = label
                    .trim()
                    .trim_start_matches('=')
                    .trim_end_matches('=')
                    .trim();
                let measured_label = resolve_zenuml_emojis_in_text(display_label);
                self.dividers.push(ZenumlDividerLayout {
                    statement_id: statement.id.clone(),
                    y: content_top + DIVIDER_HEIGHT / 2.0,
                    width: 0.0,
                    label: label.clone(),
                    label_width: self
                        .measurer
                        .measure(&measured_label, self.fragment_label_style)
                        .width,
                });
                content_top + DIVIDER_HEIGHT
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn layout_message(
        &mut self,
        statement: &ZenumlStatement,
        number: &str,
        from: &str,
        to: &str,
        label: &str,
        assignment: Option<&str>,
        style: ZenumlMessageStyle,
        body: &[ZenumlStatement],
        content_top: f64,
        fragment_depth: usize,
        text_style: &BTreeMap<String, String>,
    ) -> f64 {
        let is_self = from == to;
        let message_height = match (style, is_self) {
            (ZenumlMessageStyle::Synchronous, true) => SELF_SYNC_MESSAGE_HEIGHT,
            (ZenumlMessageStyle::Asynchronous, true) => SELF_ASYNC_MESSAGE_HEIGHT,
            _ => MESSAGE_HEIGHT,
        };
        let (from_x, to_x, target_depth) = self.message_endpoints(from, to, style);
        let y = if is_self {
            content_top
        } else {
            content_top + message_height - 0.5
        };
        let arrow_style = match style {
            ZenumlMessageStyle::Synchronous => ZenumlArrowStyle::Solid,
            ZenumlMessageStyle::Asynchronous => ZenumlArrowStyle::Open,
        };
        if is_self {
            self.self_calls.push(ZenumlSelfCallLayout {
                statement_id: statement.id.clone(),
                number: number.to_string(),
                participant_name: from.to_string(),
                x: from_x,
                y,
                width: if style == ZenumlMessageStyle::Asynchronous {
                    28.0
                } else {
                    OCCURRENCE_WIDTH
                },
                height: message_height,
                label: label.to_string(),
                arrow_style,
                style: text_style.clone(),
            });
        } else {
            self.messages.push(ZenumlMessageLayout {
                statement_id: statement.id.clone(),
                number: number.to_string(),
                from: from.to_string(),
                to: to.to_string(),
                from_x,
                to_x,
                y,
                label: label.to_string(),
                arrow_style,
                is_reverse: to_x < from_x,
                style: text_style.clone(),
            });
        }

        let mut cursor = content_top + message_height;
        if style != ZenumlMessageStyle::Synchronous {
            return cursor;
        }

        let occurrence_start = content_top + message_height - 2.0;
        self.enter_occurrence(to);
        cursor = if body.is_empty() {
            cursor + 22.0
        } else {
            self.layout_block(
                body,
                cursor,
                BlockLayoutContext {
                    inside_occurrence: true,
                    fragment_depth,
                    parent_number: number,
                    index_offset: 0,
                },
            ) + OCCURRENCE_BORDER_BOTTOM
        };
        self.leave_occurrence(to);

        if let Some(assignment) = assignment.filter(|_| !is_self) {
            cursor += ASSIGNMENT_RETURN_HEIGHT;
            let (return_from_x, return_to_x) = self.sync_assignment_return_endpoints(
                from,
                to,
                from_x,
                self.active_depth(from),
                target_depth,
            );
            self.max_return_bottom = self
                .max_return_bottom
                .max(cursor - OCCURRENCE_BORDER_BOTTOM + RETURN_BOTTOM_SPACE);
            self.returns.push(ZenumlReturnLayout {
                statement_id: format!("{}-assignment-return", statement.id),
                number: format!("{number}.{}", body.len() + 1),
                from: to.to_string(),
                to: from.to_string(),
                from_x: return_from_x,
                to_x: return_to_x,
                y: cursor - OCCURRENCE_BORDER_BOTTOM,
                label: assignment.to_string(),
                is_reverse: return_to_x < return_from_x,
                is_self: false,
            });
        }
        self.occurrences.push(ZenumlOccurrenceLayout {
            statement_id: statement.id.clone(),
            participant_name: to.to_string(),
            x: self.position(to) - OCCURRENCE_BAR_SIDE_WIDTH
                + target_depth as f64 * OCCURRENCE_BAR_SIDE_WIDTH,
            y: occurrence_start,
            width: OCCURRENCE_WIDTH,
            height: (cursor - occurrence_start).max(OCCURRENCE_EMPTY_HEIGHT),
        });
        cursor
    }

    #[allow(clippy::too_many_arguments)]
    fn layout_creation(
        &mut self,
        statement: &ZenumlStatement,
        number: &str,
        from: &str,
        to: &str,
        label: &str,
        assignment: Option<&str>,
        body: &[ZenumlStatement],
        content_top: f64,
        fragment_depth: usize,
        text_style: &BTreeMap<String, String>,
    ) -> f64 {
        let target_depth = self.active_depth(to);
        let from_x = self.creation_sender_x(from, to);
        let to_x = self.position(to);
        self.creation_y.entry(to.to_string()).or_insert(content_top);
        self.creations.push(PendingCreationLayout {
            statement_id: statement.id.clone(),
            participant_name: to.to_string(),
            message: ZenumlMessageLayout {
                statement_id: statement.id.clone(),
                number: number.to_string(),
                from: from.to_string(),
                to: to.to_string(),
                from_x,
                to_x,
                y: content_top + CREATION_MESSAGE_HEIGHT / 2.0,
                label: label.to_string(),
                arrow_style: ZenumlArrowStyle::Open,
                is_reverse: to_x < from_x,
                style: text_style.clone(),
            },
        });

        let occurrence_start = content_top + CREATION_MESSAGE_HEIGHT - 2.0;
        let mut cursor = content_top + CREATION_MESSAGE_HEIGHT;
        self.enter_occurrence(to);
        cursor = if body.is_empty() {
            cursor + 22.0
        } else {
            self.layout_block(
                body,
                cursor,
                BlockLayoutContext {
                    inside_occurrence: true,
                    fragment_depth,
                    parent_number: number,
                    index_offset: 0,
                },
            ) + OCCURRENCE_BORDER_BOTTOM
        };
        self.leave_occurrence(to);
        if let Some(assignment) = assignment {
            cursor += ASSIGNMENT_RETURN_HEIGHT;
            let (return_from_x, return_to_x) =
                self.creation_assignment_return_endpoints(from, to, self.active_depth(from));
            self.max_return_bottom = self
                .max_return_bottom
                .max(cursor - OCCURRENCE_BORDER_BOTTOM + RETURN_BOTTOM_SPACE);
            self.returns.push(ZenumlReturnLayout {
                statement_id: format!("{}-assignment-return", statement.id),
                number: format!("{number}.{}", body.len() + 1),
                from: to.to_string(),
                to: from.to_string(),
                from_x: return_from_x,
                to_x: return_to_x,
                y: cursor - OCCURRENCE_BORDER_BOTTOM,
                label: assignment.to_string(),
                is_reverse: return_to_x < return_from_x,
                is_self: false,
            });
        }
        self.occurrences.push(ZenumlOccurrenceLayout {
            statement_id: statement.id.clone(),
            participant_name: to.to_string(),
            x: to_x - OCCURRENCE_BAR_SIDE_WIDTH + target_depth as f64 * OCCURRENCE_BAR_SIDE_WIDTH,
            y: occurrence_start,
            width: OCCURRENCE_WIDTH,
            height: (cursor - occurrence_start).max(OCCURRENCE_EMPTY_HEIGHT),
        });
        cursor
    }

    fn position(&self, participant: &str) -> f64 {
        self.positions.get(participant).copied().unwrap_or(0.0)
    }

    fn statement_comment_x(&self, statement: &ZenumlStatement, comment: &str) -> f64 {
        let sender = match &statement.kind {
            ZenumlStatementKind::Message { resolved_from, .. }
            | ZenumlStatementKind::Creation { resolved_from, .. }
            | ZenumlStatementKind::Return { resolved_from, .. } => {
                resolved_from.as_deref().unwrap_or(DEFAULT_STARTER)
            }
            _ => return 1.0,
        };
        let occurrence_offset = if self.active_depth(sender) > 0 {
            OCCURRENCE_BAR_SIDE_WIDTH + 1.0
        } else {
            1.0
        };
        let code_padding = if comment.trim_start().starts_with('`') {
            2.0
        } else {
            0.0
        };
        self.position(sender) + occurrence_offset + code_padding
    }

    fn fragment_horizontal_bounds(
        &self,
        names: &HashSet<String>,
        padding: (f64, f64),
    ) -> (f64, f64) {
        let mut participant_names = names
            .iter()
            .filter(|name| self.positions.contains_key(*name))
            .map(String::as_str)
            .collect::<Vec<_>>();
        if participant_names.is_empty() {
            return (0.0, 0.0);
        }
        participant_names
            .sort_by(|left, right| self.position(left).total_cmp(&self.position(right)));
        let left_name = participant_names[0];
        let right_name = participant_names[participant_names.len() - 1];
        let left = self.position(left_name) - self.half_width(left_name);
        let right = self.position(right_name) + self.half_width(right_name);
        (
            left,
            (right - left + padding.0 + padding.1).max(FRAGMENT_MIN_WIDTH),
        )
    }

    fn fragment_section_layout(&self, label: &str, y: f64) -> ZenumlFragmentSectionLayout {
        let label_width = (!label.is_empty()).then(|| {
            let label = resolve_zenuml_emojis_in_text(label);
            self.measurer
                .measure(&label, self.fragment_label_style)
                .width
        });
        let inner_label = label
            .strip_prefix('[')
            .and_then(|label| label.strip_suffix(']'))
            .map(str::trim)
            .filter(|label| !label.is_empty() && *label != "else")
            .map(str::to_string);
        let inner_label_width = inner_label.as_ref().map(|label| {
            let label = resolve_zenuml_emojis_in_text(label);
            self.measurer
                .measure(&label, self.fragment_label_style)
                .width
        });
        let (keyword, detail) = label
            .split_once(' ')
            .filter(|_| !label.starts_with('[') && !label.starts_with("finally"))
            .map_or((None, None), |(keyword, detail)| {
                (Some(keyword.to_string()), Some(detail.to_string()))
            });
        let keyword_width = keyword.as_ref().map(|value| {
            self.measurer
                .measure(value, self.fragment_label_style)
                .width
        });
        let detail_width = detail.as_ref().map(|value| {
            self.measurer
                .measure(value, self.fragment_label_style)
                .width
        });
        ZenumlFragmentSectionLayout {
            label: label.to_string(),
            y,
            height: 0.0,
            label_width,
            inner_label,
            inner_label_width,
            keyword,
            keyword_width,
            detail,
            detail_width,
            content_inset_left: None,
        }
    }

    fn half_width(&self, participant: &str) -> f64 {
        self.participant_half_widths
            .get(participant)
            .copied()
            .unwrap_or(MIN_PARTICIPANT_WIDTH / 2.0)
    }

    fn active_depth(&self, participant: &str) -> usize {
        self.active_occurrences
            .get(participant)
            .copied()
            .unwrap_or(0)
    }

    fn enter_occurrence(&mut self, participant: &str) {
        *self
            .active_occurrences
            .entry(participant.to_string())
            .or_default() += 1;
    }

    fn leave_occurrence(&mut self, participant: &str) {
        if let Some(depth) = self.active_occurrences.get_mut(participant) {
            *depth = depth.saturating_sub(1);
            if *depth == 0 {
                self.active_occurrences.remove(participant);
            }
        }
    }

    fn message_endpoints(
        &self,
        from: &str,
        to: &str,
        style: ZenumlMessageStyle,
    ) -> (f64, f64, usize) {
        let raw_from = self.position(from);
        let raw_to = self.position(to);
        let sender_depth = self.active_depth(from);
        let target_depth = self.active_depth(to);
        if from == to {
            return (
                raw_from + sender_depth as f64 * OCCURRENCE_BAR_SIDE_WIDTH,
                raw_to,
                target_depth,
            );
        }
        let left_to_right = raw_from < raw_to;
        let from_x = if sender_depth == 0 {
            raw_from
        } else if left_to_right {
            raw_from + sender_depth as f64 * OCCURRENCE_BAR_SIDE_WIDTH
        } else {
            raw_from - OCCURRENCE_BAR_SIDE_WIDTH
                + sender_depth.saturating_sub(1) as f64 * OCCURRENCE_BAR_SIDE_WIDTH
        };
        let to_x = match style {
            ZenumlMessageStyle::Synchronous => {
                if left_to_right {
                    raw_to - OCCURRENCE_BAR_SIDE_WIDTH
                        + target_depth as f64 * OCCURRENCE_BAR_SIDE_WIDTH
                } else {
                    raw_to
                        + OCCURRENCE_BAR_SIDE_WIDTH
                        + target_depth as f64 * OCCURRENCE_BAR_SIDE_WIDTH
                }
            }
            ZenumlMessageStyle::Asynchronous if target_depth > 0 => {
                if left_to_right {
                    raw_to - target_depth as f64 * OCCURRENCE_BAR_SIDE_WIDTH
                } else {
                    raw_to + target_depth as f64 * OCCURRENCE_BAR_SIDE_WIDTH
                }
            }
            ZenumlMessageStyle::Asynchronous => raw_to,
        };
        (from_x, to_x, target_depth)
    }

    fn creation_sender_x(&self, from: &str, to: &str) -> f64 {
        let raw_from = self.position(from);
        let raw_to = self.position(to);
        let depth = self.active_depth(from) as f64;
        if raw_from < raw_to {
            raw_from + depth * OCCURRENCE_BAR_SIDE_WIDTH
        } else {
            raw_from - depth * OCCURRENCE_BAR_SIDE_WIDTH
        }
    }

    fn return_endpoints(
        &self,
        from: &str,
        to: &str,
        from_layers: usize,
        to_layers: usize,
    ) -> (f64, f64) {
        let raw_from = self.position(from);
        let raw_to = self.position(to);
        let reverse = raw_to < raw_from;
        let from_x = if reverse {
            if from_layers == 0 {
                raw_from
            } else {
                raw_from + from_layers.saturating_sub(1) as f64 * OCCURRENCE_BAR_SIDE_WIDTH
                    - OCCURRENCE_BAR_SIDE_WIDTH
            }
        } else {
            raw_from + from_layers as f64 * OCCURRENCE_BAR_SIDE_WIDTH + 1.0
        };
        let to_x = if reverse {
            raw_to + to_layers as f64 * OCCURRENCE_BAR_SIDE_WIDTH + 1.0
        } else if to_layers == 0 {
            raw_to
        } else {
            raw_to + to_layers.saturating_sub(1) as f64 * OCCURRENCE_BAR_SIDE_WIDTH
                - OCCURRENCE_BAR_SIDE_WIDTH
        };
        (from_x, to_x)
    }

    fn sync_assignment_return_endpoints(
        &self,
        sender: &str,
        target: &str,
        message_from_x: f64,
        sender_depth: usize,
        target_depth: usize,
    ) -> (f64, f64) {
        let sender_x = self.position(sender);
        let target_x = self.position(target);
        let left_to_right = sender_x < target_x;
        let return_from_x = if left_to_right {
            target_x - OCCURRENCE_BAR_SIDE_WIDTH
                + target_depth as f64 * OCCURRENCE_BAR_SIDE_WIDTH
                + 1.0
        } else {
            target_x
                + OCCURRENCE_BAR_SIDE_WIDTH
                + target_depth as f64 * OCCURRENCE_BAR_SIDE_WIDTH
                + 1.0
        };
        let return_to_x = if sender_depth == 0 {
            message_from_x + if left_to_right { 2.0 } else { 1.0 }
        } else if left_to_right {
            message_from_x + 2.0
        } else {
            message_from_x
        };
        (return_from_x, return_to_x)
    }

    fn creation_assignment_return_endpoints(
        &self,
        sender: &str,
        created: &str,
        sender_depth: usize,
    ) -> (f64, f64) {
        let sender_x = self.position(sender);
        let created_x = self.position(created);
        let left_to_right = sender_x < created_x;
        let return_from_x = if left_to_right {
            created_x - OCCURRENCE_BAR_SIDE_WIDTH + 1.0
        } else {
            created_x + OCCURRENCE_BAR_SIDE_WIDTH + 1.0
        };
        let return_to_x = if sender_depth == 0 {
            sender_x
        } else {
            let nested_offset = sender_depth.saturating_sub(1) as f64 * OCCURRENCE_BAR_SIDE_WIDTH;
            sender_x
                + nested_offset
                + if left_to_right {
                    OCCURRENCE_BAR_SIDE_WIDTH + 2.0
                } else {
                    -OCCURRENCE_BAR_SIDE_WIDTH
                }
        };
        (return_from_x, return_to_x)
    }
}

fn collect_participant_names(statements: &[ZenumlStatement], names: &mut HashSet<String>) {
    for statement in statements {
        match &statement.kind {
            ZenumlStatementKind::Message {
                resolved_from,
                resolved_to,
                style,
                body,
                ..
            } => {
                let (from, to) = message_render_endpoints(
                    *style,
                    resolved_from.as_deref(),
                    resolved_to.as_deref(),
                );
                names.insert(from.to_string());
                names.insert(to.to_string());
                collect_participant_names(body, names);
            }
            ZenumlStatementKind::Creation {
                resolved_from,
                resolved_to,
                body,
                ..
            } => {
                names.insert(
                    resolved_from
                        .as_deref()
                        .unwrap_or(DEFAULT_STARTER)
                        .to_string(),
                );
                names.insert(resolved_to.clone());
                collect_participant_names(body, names);
            }
            ZenumlStatementKind::Return {
                resolved_from,
                resolved_to,
                ..
            } => {
                names.insert(
                    resolved_from
                        .as_deref()
                        .unwrap_or(DEFAULT_STARTER)
                        .to_string(),
                );
                names.insert(
                    resolved_to
                        .as_deref()
                        .unwrap_or(DEFAULT_STARTER)
                        .to_string(),
                );
            }
            ZenumlStatementKind::Fragment { sections, .. } => {
                for section in sections {
                    collect_participant_names(&section.statements, names);
                }
            }
            ZenumlStatementKind::Reference { participants, .. } => {
                names.extend(participants.iter().cloned());
            }
            ZenumlStatementKind::Divider { .. } => {}
        }
    }
}

fn layout_groups(participants: &[ZenumlParticipantLayout], height: f64) -> Vec<ZenumlGroupLayout> {
    let mut grouped: HashMap<&str, Vec<&ZenumlParticipantLayout>> = HashMap::new();
    for participant in participants {
        if let Some(group_id) = participant.group_id.as_deref() {
            grouped.entry(group_id).or_default().push(participant);
        }
    }
    grouped
        .into_iter()
        .filter_map(|(name, members)| {
            let left = members
                .iter()
                .map(|participant| participant.x - participant.width / 2.0)
                .min_by(f64::total_cmp)?;
            let right = members
                .iter()
                .map(|participant| participant.x + participant.width / 2.0)
                .max_by(f64::total_cmp)?;
            Some(ZenumlGroupLayout {
                name: name.to_string(),
                x: left - 2.0,
                y: PARTICIPANT_TOP - 18.5,
                width: right - left + 4.0,
                height: height - PARTICIPANT_TOP + 31.5,
            })
        })
        .collect()
}

fn parse_comment(raw: &str) -> ParsedComment {
    let trimmed = raw.trim();
    let (preceding, last_line) = trimmed
        .rsplit_once('\n')
        .map_or(("", trimmed), |(preceding, last)| (preceding, last));
    let (comment_only, message_only, common, last_text) = parse_comment_style_prefix(last_line);
    let common_emojis = resolve_bracket_emojis(&common);
    let common_emoji_names = common_emojis
        .iter()
        .map(|(name, _)| name.as_str())
        .collect::<HashSet<_>>();
    let common_styles = common
        .iter()
        .filter(|value| !is_colon_emoji(value) && !common_emoji_names.contains(value.as_str()))
        .map(String::as_str)
        .collect::<Vec<_>>();
    let mut comment_style = BTreeMap::new();
    let mut message_style = BTreeMap::new();
    apply_comment_styles(&mut comment_style, common_styles.iter().copied());
    apply_comment_styles(&mut message_style, common_styles.iter().copied());
    apply_comment_styles(&mut comment_style, comment_only.iter().map(String::as_str));
    apply_comment_styles(&mut message_style, message_only.iter().map(String::as_str));

    let base = if preceding.is_empty() {
        last_text.to_string()
    } else {
        format!("{preceding}\n{last_text}")
    };
    let base = base.trim();
    let mut emoji_prefix = String::new();
    for (_, unicode) in &common_emojis {
        emoji_prefix.push_str(unicode);
    }
    let text = match (emoji_prefix.is_empty(), base.is_empty()) {
        (false, false) => format!("{emoji_prefix} {base}"),
        (false, true) => emoji_prefix,
        (true, _) => base.to_string(),
    };
    ParsedComment {
        visual_lines: if text.is_empty() {
            0
        } else {
            raw.trim().split('\n').count()
        },
        text,
        comment_style,
        message_style,
    }
}

fn parse_comment_style_prefix(input: &str) -> (Vec<String>, Vec<String>, Vec<String>, &str) {
    let mut comment_only = Vec::new();
    let mut message_only = Vec::new();
    let mut common = Vec::new();
    let mut cursor = 0;
    let mut consumed = None;
    while cursor < input.len() {
        cursor += input[cursor..]
            .chars()
            .take_while(|character| character.is_whitespace())
            .map(char::len_utf8)
            .sum::<usize>();
        if cursor >= input.len() {
            break;
        }
        let (closing, destination) = match input.as_bytes()[cursor] {
            b'<' => (b'>', &mut comment_only),
            b'(' => (b')', &mut message_only),
            b'[' => (b']', &mut common),
            _ => {
                if consumed.is_some() {
                    consumed = Some(cursor);
                }
                break;
            }
        };
        let Some(relative_end) = input.as_bytes()[cursor + 1..]
            .iter()
            .position(|byte| *byte == closing)
        else {
            if consumed.is_some() {
                consumed = Some(cursor);
            }
            break;
        };
        let end = cursor + 1 + relative_end;
        destination.extend(
            input[cursor + 1..end]
                .split(',')
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string),
        );
        cursor = end + 1;
        consumed = Some(cursor);
    }
    (
        comment_only,
        message_only,
        common,
        &input[consumed.unwrap_or(0)..],
    )
}

fn apply_comment_styles<'a>(
    style: &mut BTreeMap<String, String>,
    values: impl Iterator<Item = &'a str>,
) {
    for value in values {
        if is_zenuml_css_color(value) {
            style.insert("fill".to_string(), value.to_string());
        } else if matches!(value, "italic" | "oblique") {
            style.insert("font-style".to_string(), value.to_string());
        } else if matches!(value, "bold" | "bolder" | "lighter") {
            style.insert("font-weight".to_string(), value.to_string());
        } else if matches!(value, "underline" | "overline" | "line-through") {
            style.insert("text-decoration".to_string(), value.to_string());
        }
    }
}

fn is_colon_emoji(value: &str) -> bool {
    value.len() > 2 && value.starts_with(':') && value.ends_with(':')
}

fn is_zenuml_css_color(value: &str) -> bool {
    value.bytes().all(|byte| !byte.is_ascii_uppercase())
        && cssparser::color::parse_named_color(value).is_ok()
}

fn resolve_bracket_emojis(values: &[String]) -> Vec<(String, &'static str)> {
    values
        .iter()
        .filter_map(|value| {
            let forced = is_colon_emoji(value);
            let name = if forced {
                &value[1..value.len() - 1]
            } else {
                value.as_str()
            };
            if !forced && (name.contains('-') || is_zenuml_css_color(name)) {
                return None;
            }
            zenuml_emoji_unicode(name).map(|unicode| (name.to_string(), unicode))
        })
        .collect()
}

pub(crate) fn resolve_zenuml_emojis_in_text(text: &str) -> String {
    let mut output = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(open) = rest.find('[') {
        output.push_str(&rest[..open]);
        let after_open = &rest[open + 1..];
        let Some(close) = after_open.find(']') else {
            output.push_str(&rest[open..]);
            return output;
        };
        let raw = &after_open[..close];
        let values = raw
            .split(',')
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
            .collect::<Vec<_>>();
        let emojis = resolve_bracket_emojis(&values);
        if emojis.is_empty() {
            output.push_str(&rest[open..open + close + 2]);
        } else {
            for (_, unicode) in emojis {
                output.push_str(unicode);
            }
        }
        rest = &after_open[close + 1..];
    }
    output.push_str(rest);
    output
}

pub(crate) fn zenuml_participant_icon_key(participant_type: &str) -> Option<&'static str> {
    Some(match participant_type.to_ascii_lowercase().as_str() {
        "actor" => "actor",
        "database" => "database",
        "ec2" => "ec2",
        "lambda" => "lambda",
        "azurefunction" => "azurefunction",
        "sqs" => "sqs",
        "sns" => "sns",
        "iam" => "iam",
        "boundary" => "boundary",
        "control" => "control",
        "entity" => "entity",
        _ => return None,
    })
}

pub(crate) fn zenuml_emoji_unicode(shortcode: &str) -> Option<&'static str> {
    Some(match shortcode {
        "check" => "\u{2705}",
        "x" => "\u{274c}",
        "warning" => "\u{26a0}\u{fe0f}",
        "exclamation" => "\u{2757}",
        "question" => "\u{2753}",
        "bulb" => "\u{1f4a1}",
        "eyes" => "\u{1f440}",
        "rocket" => "\u{1f680}",
        "fire" => "\u{1f525}",
        "zap" | "cache" => "\u{26a1}",
        "boom" => "\u{1f4a5}",
        "sparkles" => "\u{2728}",
        "tada" => "\u{1f389}",
        "confetti_ball" => "\u{1f38a}",
        "lock" => "\u{1f512}",
        "unlock" => "\u{1f513}",
        "key" => "\u{1f511}",
        "gear" | "service" => "\u{2699}\u{fe0f}",
        "hammer" => "\u{1f528}",
        "wrench" => "\u{1f527}",
        "package" | "container" => "\u{1f4e6}",
        "email" => "\u{1f4e7}",
        "link" => "\u{1f517}",
        "clipboard" => "\u{1f4cb}",
        "bookmark" => "\u{1f516}",
        "speech_balloon" => "\u{1f4ac}",
        "thought_balloon" => "\u{1f4ad}",
        "bell" => "\u{1f514}",
        "megaphone" => "\u{1f4e3}",
        "cloud" => "\u{2601}\u{fe0f}",
        "sun" => "\u{2600}\u{fe0f}",
        "star" => "\u{2b50}",
        "globe" | "network" => "\u{1f310}",
        "arrow_right" => "\u{27a1}\u{fe0f}",
        "arrow_left" => "\u{2b05}\u{fe0f}",
        "arrow_up" => "\u{2b06}\u{fe0f}",
        "arrow_down" => "\u{2b07}\u{fe0f}",
        "wave" => "\u{1f44b}",
        "thumbsup" => "\u{1f44d}",
        "thumbsdown" => "\u{1f44e}",
        "computer" => "\u{1f4bb}",
        "iphone" => "\u{1f4f1}",
        "robot" => "\u{1f916}",
        "bug" => "\u{1f41b}",
        "database" => "\u{1f5c4}\u{fe0f}",
        "server" => "\u{1f5a5}\u{fe0f}",
        "api" => "\u{1f50c}",
        "gateway" => "\u{1f6aa}",
        "queue" => "\u{1f4ec}",
        "processor" => "\u{1f504}",
        "store" => "\u{1f3ea}",
        "worker" => "\u{1f477}",
        "chart" => "\u{1f4ca}",
        "chart_with_upwards_trend" => "\u{1f4c8}",
        "chart_with_downwards_trend" => "\u{1f4c9}",
        "inbox_tray" => "\u{1f4e5}",
        "outbox_tray" => "\u{1f4e4}",
        "file_folder" => "\u{1f4c1}",
        "hourglass" => "\u{23f3}",
        "clock" => "\u{1f550}",
        "stopwatch" => "\u{23f1}\u{fe0f}",
        "heavy_check_mark" => "\u{2714}\u{fe0f}",
        "heavy_multiplication_x" => "\u{2716}\u{fe0f}",
        "red" | "red_circle" => "\u{1f534}",
        "green_circle" => "\u{1f7e2}",
        "blue_circle" => "\u{1f535}",
        "white_circle" => "\u{26aa}",
        "black_circle" => "\u{26ab}",
        "heart" => "\u{2764}\u{fe0f}",
        "shield" => "\u{1f6e1}\u{fe0f}",
        "coffee" => "\u{2615}",
        "pizza" => "\u{1f355}",
        "car" => "\u{1f697}",
        "bus" => "\u{1f68c}",
        "airplane" => "\u{2708}\u{fe0f}",
        "ship" => "\u{1f6a2}",
        "floppy_disk" => "\u{1f4be}",
        "cd" => "\u{1f4bf}",
        "satellite" => "\u{1f6f0}\u{fe0f}",
        "factory" => "\u{1f3ed}",
        "hospital" => "\u{1f3e5}",
        "bank" => "\u{1f3e6}",
        "construction" => "\u{1f6a7}",
        "recycle" => "\u{267b}\u{fe0f}",
        "receipt" => "\u{1f9fe}",
        "cart" => "\u{1f6d2}",
        "cylinder" => "\u{1faa8}",
        "dollar" => "\u{1f4b5}",
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::text::DeterministicTextMeasurer;
    use merman_core::{Engine, ParseOptions, RenderSemanticModel};

    #[test]
    fn presentation_typography_measures_selected_fonts_and_preserves_vertical_clearance() {
        struct RecordingMeasurer(std::sync::Mutex<Vec<(String, TextStyle)>>);

        impl TextMeasurer for RecordingMeasurer {
            fn measure(&self, text: &str, style: &TextStyle) -> crate::text::TextMetrics {
                self.0
                    .lock()
                    .unwrap()
                    .push((text.to_string(), style.clone()));
                DeterministicTextMeasurer::default().measure(text, style)
            }
        }

        let parsed = Engine::new()
            .parse_diagram_for_render_model_sync(
                "zenuml\n@Actor <<Client>> Alice\nBob\n// First line\n// (bold, italic) Second line\nAlice->Bob: A message wide enough to separate participants\nif(ready) {\n Alice->Bob: Continue\n}\n== Divider ==\n",
                ParseOptions::strict(),
            ).unwrap().unwrap();
        let RenderSemanticModel::Zenuml(model) = parsed.model() else {
            panic!("expected ZenUML");
        };
        for size in [14.0, 18.0, 24.0] {
            let config = serde_json::json!({"themeVariables": {"zenuml": {"fontFamily": "Inter, sans-serif", "fontSize": format!("{size}px")}}});
            let measurer = RecordingMeasurer(std::sync::Mutex::new(Vec::new()));
            let layout = layout_zenuml_diagram_typed(
                model,
                &ZenumlSourceStyle::new(&merman_core::MermaidConfig::from_value(config.clone()))
                    .typography,
                &measurer,
            )
            .unwrap();
            let typography =
                ZenumlSourceStyle::new(&merman_core::MermaidConfig::from_value(config.clone()))
                    .typography;
            let measurements = measurer.0.lock().unwrap();
            assert!(
                measurements
                    .iter()
                    .all(|(_, style)| style.font_family.as_deref() == Some("Inter, sans-serif"))
            );
            assert!(
                measurements
                    .iter()
                    .any(|(text, style)| text == "Alice" && style.font_size == size)
            );
            assert!(
                measurements
                    .iter()
                    .any(|(text, style)| text == "«Client»" && style.font_size == size),
                "stereotype measurement must preserve the full Unicode label"
            );
            assert!(
                measurements
                    .iter()
                    .any(|(text, style)| text.starts_with("A message wide")
                        && style.font_size == typography.size(14.0)
                        && style.font_weight.as_deref() == Some("bold")
                        && style.font_style.as_deref() == Some("italic"))
            );
            assert!(
                measurements.iter().any(
                    |(text, style)| text == "ready" && style.font_size == typography.size(14.0)
                )
            );
            let message = &layout.messages[0];
            let label = DeterministicTextMeasurer::default().measure(
                &message.label,
                &typography.styled_message(&typography.message_style(), &message.style),
            );
            assert!(
                message.to_x - message.from_x >= label.width + ARROW_HEAD_WIDTH + OCCURRENCE_WIDTH
            );
            assert_eq!(
                layout.participants[0].height,
                PARTICIPANT_VISUAL_HEIGHT * size / 16.0
            );
            assert!(layout.messages[1].y - message.y > 2.0 * typography.size(14.0));
            assert!(layout.fragments[0].height > FRAGMENT_HEADER_HEIGHT * size / 16.0);
        }
    }

    fn layout(source: &str) -> ZenumlDiagramLayout {
        let parsed = Engine::new()
            .parse_diagram_for_render_model_sync(source, ParseOptions::strict())
            .unwrap()
            .unwrap();
        let RenderSemanticModel::Zenuml(model) = parsed.model() else {
            panic!("expected ZenUML model");
        };
        layout_zenuml_diagram_typed(
            model,
            &ZenumlTypography::default(),
            &DeterministicTextMeasurer::default(),
        )
        .unwrap()
    }

    #[test]
    fn advanced_topology_keeps_nested_owners_and_fragments() {
        let source = "zenuml\n@Starter(Client)\nA.one() {\n  if(ok) {\n    B.two()\n  }\n}\n";
        let layout = layout(source);
        assert_eq!(layout.messages.len(), 2);
        assert_eq!(layout.fragments.len(), 1);
        assert_eq!(layout.occurrences.len(), 2);
    }

    #[test]
    fn root_block_and_empty_occurrence_follow_the_oracle_vertical_vm() {
        let layout = layout("zenuml\nA.call()\n");
        let message = &layout.messages[0];
        let occurrence = &layout.occurrences[0];

        // VerticalCoordinates starts the root block at 56px; BlockVM then applies a 16px
        // statement margin before the 16px non-self message.
        assert_eq!(message.y, 87.5);
        assert_eq!(occurrence.y, 86.0);
        assert_eq!(occurrence.height, OCCURRENCE_EMPTY_HEIGHT);
        assert_eq!(layout.height, 154.0);
    }

    #[test]
    fn final_return_debt_expands_its_parent_occurrence() {
        let without_return = layout("zenuml\nA.call() {\n  B.work()\n}\n");
        let with_return = layout("zenuml\nA.call() {\n  B.work()\n  return done\n}\n");
        let outer_without = without_return
            .occurrences
            .iter()
            .find(|occurrence| occurrence.statement_id == "zenuml-statement-1")
            .unwrap();
        let outer_with = with_return
            .occurrences
            .iter()
            .find(|occurrence| occurrence.statement_id == "zenuml-statement-1")
            .unwrap();

        assert!(
            with_return.height > without_return.height,
            "return debt must increase rendered height: without={without_return:?}, with={with_return:?}"
        );
        assert!(outer_with.height >= outer_without.height);
        assert!(!with_return.returns.is_empty());
    }

    #[test]
    fn divider_background_uses_the_operation_text_measurer() {
        let source = "zenuml\n== Wide label ==\n";
        let layout = layout(source);
        let divider = &layout.dividers[0];
        let expected = DeterministicTextMeasurer::default().measure(
            "Wide label",
            &TextStyle {
                font_family: Some("Helvetica, Verdana, serif".to_string()),
                font_size: 14.0,
                font_weight: None,
                font_style: None,
            },
        );
        assert_eq!(divider.label_width, expected.width);
    }

    #[test]
    fn endpoint_fallbacks_are_statement_kind_specific() {
        // Selected @zenuml/core 3.50.1 keeps missing-target `_STARTER_` coordinates without
        // adding it to OrderedParticipants.
        let synchronous = layout("zenuml\n@Starter(A)\nmethod()\n");
        assert_eq!(
            synchronous
                .participants
                .iter()
                .map(|participant| participant.name.as_str())
                .collect::<Vec<_>>(),
            ["A"]
        );
        assert_eq!(synchronous.messages[0].from, "A");
        assert_eq!(synchronous.messages[0].to, DEFAULT_STARTER);
        assert_eq!(synchronous.messages[0].to_x, 7.0);

        let asynchronous = layout("zenuml\nA ->\n");
        assert_eq!(
            asynchronous
                .participants
                .iter()
                .map(|participant| participant.name.as_str())
                .collect::<Vec<_>>(),
            ["A"]
        );
        assert_eq!(asynchronous.self_calls[0].participant_name, "A");

        let returned = layout("zenuml\nA -->\n");
        assert_eq!(
            returned
                .participants
                .iter()
                .map(|participant| participant.name.as_str())
                .collect::<Vec<_>>(),
            ["A"]
        );
        assert_eq!(returned.returns[0].from, "A");
        assert_eq!(returned.returns[0].to, DEFAULT_STARTER);
        assert_eq!(returned.returns[0].to_x, 1.0);
    }

    #[test]
    fn renderer_numbers_fragment_sections_with_one_cumulative_offset() {
        let layout = layout("zenuml\nif(x) { A.m() } else if(y) { B.m() } else { C.m() }\n");
        assert_eq!(
            layout
                .messages
                .iter()
                .map(|message| message.number.as_str())
                .collect::<Vec<_>>(),
            ["1.1", "1.2", "1.3"]
        );
    }

    #[test]
    fn source_width_is_not_a_native_svg_geometry_input() {
        let without_width = layout("zenuml\n@Actor A\n@Boundary B\nA->B.m()\n");
        let with_width = layout("zenuml\n@Actor A 400\n@Boundary B 1\nA->B.m()\n");
        let geometry = |layout: &ZenumlDiagramLayout| {
            layout
                .participants
                .iter()
                .map(|participant| (participant.name.clone(), participant.x, participant.width))
                .collect::<Vec<_>>()
        };
        assert_eq!(geometry(&without_width), geometry(&with_width));
    }

    #[test]
    fn statement_kinds_lower_into_disjoint_svg_geometry_collections() {
        let layout = layout("zenuml\n@Starter(A)\nA->B: async\nA.self()\nnew C\nA-->B: returned\n");

        assert_eq!(layout.messages.len(), 1);
        assert_eq!(layout.self_calls.len(), 1);
        assert_eq!(layout.creations.len(), 1);
        assert_eq!(layout.returns.len(), 1);
        assert_eq!(layout.messages[0].arrow_style, ZenumlArrowStyle::Open);
        assert_eq!(
            layout.creations[0].message.arrow_style,
            ZenumlArrowStyle::Open
        );
    }

    #[test]
    fn frame_border_follows_shared_left_and_right_paths_independently() {
        let layout =
            layout("zenuml\nif(root) {\n  A->D: root\n  if(left) {\n    A->A: nested\n  }\n}\n");

        assert_eq!(layout.frame_border_left, 20.0);
        assert_eq!(layout.frame_border_right, 10.0);
    }

    #[test]
    fn comment_suffix_styles_keep_comment_and_message_channels_separate() {
        let layout =
            layout("zenuml\n// <red> (bold) [italic, rocket] **important** `code`\nA.call()\n");
        let comment = &layout.comments[0];
        let message = &layout.messages[0];

        assert!(comment.text.starts_with("\u{1f680} "));
        assert_eq!(comment.style.get("fill").map(String::as_str), Some("red"));
        assert_eq!(
            comment.style.get("font-style").map(String::as_str),
            Some("italic")
        );
        assert!(!comment.style.contains_key("font-weight"));
        assert_eq!(
            message.style.get("font-weight").map(String::as_str),
            Some("bold")
        );
        assert_eq!(
            message.style.get("font-style").map(String::as_str),
            Some("italic")
        );
        assert!(!message.style.contains_key("fill"));
    }

    #[test]
    fn comment_x_uses_lifeline_occurrence_and_code_padding_from_the_companion() {
        let plain = layout("zenuml\n// note\nA.call()\n");
        let code = layout("zenuml\n// `code`\nA.call()\n");
        let nested = layout("zenuml\nA.call() {\n  // note\n  B.work()\n}\n");

        assert_eq!(plain.comments[0].x, plain.participants[0].x + 1.0);
        assert_eq!(code.comments[0].x, code.participants[0].x + 3.0);
        let nested_sender = nested
            .participants
            .iter()
            .find(|participant| participant.name == "A")
            .unwrap();
        assert_eq!(nested.comments[0].x, nested_sender.x + 8.0);
    }

    #[test]
    fn assignment_return_owns_the_full_sixteen_pixel_occurrence_height() {
        let plain = layout("zenuml\nA.call()\n");
        let assigned = layout("zenuml\nresult = A.call()\n");

        assert_eq!(
            assigned.occurrences[0].height - plain.occurrences[0].height,
            16.0
        );
        assert_eq!(assigned.returns.len(), 1);
        assert_eq!(assigned.returns[0].from_x, 144.0);
        assert_eq!(assigned.returns[0].to_x, 52.0);
    }

    #[test]
    fn creation_assignment_uses_creation_specific_return_edges() {
        let layout = layout("zenuml\nresult = new A\n");

        assert_eq!(layout.creations.len(), 1);
        assert_eq!(layout.returns.len(), 1);
        assert_eq!(layout.returns[0].from_x, 144.0);
        assert_eq!(layout.returns[0].to_x, 50.0);
    }

    #[test]
    fn fragment_sections_keep_return_geometry_independent() {
        let layout = layout(
            "zenuml\nA.call() {\n  if(ok) {\n    return first\n  } else {\n    return second\n  }\n}\n",
        );

        assert_eq!(layout.returns.len(), 2);
        assert!(layout.returns[1].y - layout.returns[0].y >= MESSAGE_HEIGHT);
        assert_eq!(layout.fragments[0].sections.len(), 2);
    }

    #[test]
    fn parallel_fragment_owns_source_backed_child_separator_geometry() {
        let layout = layout("zenuml\npar {\n  A->B: one\n  A->B: two\n}\n");
        let fragment = &layout.fragments[0];

        assert_eq!(fragment.kind, ZenumlLayoutFragmentKind::Parallel);
        assert_eq!(fragment.sections.len(), 2);
        assert!(fragment.sections[1].content_inset_left.is_some());
        assert!(fragment.sections[1].y > fragment.y);
    }
}
