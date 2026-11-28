use serde::ser::SerializeStruct;
use serde::{Deserialize, Serialize, Serializer};
use serde_json::Value;
use std::collections::BTreeMap;

use super::{
    LINETYPE_ACTIVE_END, LINETYPE_ACTIVE_START, LINETYPE_ALT_ELSE, LINETYPE_ALT_END,
    LINETYPE_ALT_START, LINETYPE_AUTONUMBER, LINETYPE_BIDIRECTIONAL_DOTTED,
    LINETYPE_BIDIRECTIONAL_SOLID, LINETYPE_BREAK_END, LINETYPE_BREAK_START,
    LINETYPE_CENTRAL_CONNECTION, LINETYPE_CENTRAL_CONNECTION_DUAL,
    LINETYPE_CENTRAL_CONNECTION_REVERSE, LINETYPE_CRITICAL_END, LINETYPE_CRITICAL_OPTION,
    LINETYPE_CRITICAL_START, LINETYPE_DOTTED, LINETYPE_DOTTED_CROSS, LINETYPE_DOTTED_OPEN,
    LINETYPE_DOTTED_POINT, LINETYPE_LOOP_END, LINETYPE_LOOP_START, LINETYPE_NOTE, LINETYPE_OPT_END,
    LINETYPE_OPT_START, LINETYPE_PAR_AND, LINETYPE_PAR_END, LINETYPE_PAR_OVER_START,
    LINETYPE_PAR_START, LINETYPE_RECT_END, LINETYPE_RECT_START, LINETYPE_SOLID,
    LINETYPE_SOLID_ARROW_BOTTOM_REVERSE, LINETYPE_SOLID_ARROW_BOTTOM_REVERSE_DOTTED,
    LINETYPE_SOLID_ARROW_TOP_REVERSE, LINETYPE_SOLID_ARROW_TOP_REVERSE_DOTTED,
    LINETYPE_SOLID_BOTTOM, LINETYPE_SOLID_BOTTOM_DOTTED, LINETYPE_SOLID_CROSS, LINETYPE_SOLID_OPEN,
    LINETYPE_SOLID_POINT, LINETYPE_SOLID_TOP, LINETYPE_SOLID_TOP_DOTTED,
    LINETYPE_STICK_ARROW_BOTTOM_REVERSE, LINETYPE_STICK_ARROW_BOTTOM_REVERSE_DOTTED,
    LINETYPE_STICK_ARROW_TOP_REVERSE, LINETYPE_STICK_ARROW_TOP_REVERSE_DOTTED,
    LINETYPE_STICK_BOTTOM, LINETYPE_STICK_BOTTOM_DOTTED, LINETYPE_STICK_TOP,
    LINETYPE_STICK_TOP_DOTTED, PLACEMENT_LEFT_OF, PLACEMENT_OVER, PLACEMENT_RIGHT_OF,
};
use crate::ParseMetadata;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SequenceDiagramRenderModel {
    #[serde(rename = "accTitle")]
    pub acc_title: Option<String>,
    #[serde(rename = "accDescr")]
    pub acc_descr: Option<String>,
    pub title: Option<String>,
    #[serde(rename = "actorOrder")]
    pub actor_order: Vec<String>,
    pub actors: BTreeMap<String, SequenceActor>,
    #[serde(default)]
    pub boxes: Vec<SequenceBox>,
    pub messages: Vec<SequenceMessage>,
    #[serde(default)]
    pub notes: Vec<SequenceNote>,
    #[serde(rename = "createdActors", default)]
    pub created_actors: BTreeMap<String, usize>,
    #[serde(rename = "destroyedActors", default)]
    pub destroyed_actors: BTreeMap<String, usize>,
    #[serde(
        rename = "actorLifecycles",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    /// Parser-resolved lifecycle message ownership.
    ///
    /// Mermaid's compatibility maps retain declaration anchors, while its
    /// `AddMessage` state machine decides which later signal actually consumes
    /// each pending create or destroy request. Parser-backed models store that
    /// resolved truth in actor-order slots here. `None` is reserved for
    /// legacy/direct typed models that only provide the compatibility maps.
    pub actor_lifecycles: Option<Vec<SequenceActorLifecycle>>,
}

impl SequenceDiagramRenderModel {
    pub(crate) fn sanitize_common_db_fields(&mut self, config: &crate::MermaidConfig) {
        crate::common_db::sanitize_optional_title(&mut self.title, config);
        crate::common_db::sanitize_optional_acc_title(&mut self.acc_title, config);
        crate::common_db::sanitize_optional_acc_descr(&mut self.acc_descr, config);
    }

    pub(crate) fn to_compat_json(&self, diagram_type: &str) -> Value {
        serde_json::to_value(self.compatibility_projection(diagram_type))
            .expect("the typed Sequence compatibility projection only contains JSON values")
    }

    /// Borrows the stable Mermaid compatibility JSON projection without constructing a `Value`
    /// tree first.
    #[doc(hidden)]
    pub fn compatibility_projection<'a>(
        &'a self,
        diagram_type: &'a str,
    ) -> SequenceCompatibilityProjection<'a> {
        SequenceCompatibilityProjection {
            model: self,
            diagram_type,
        }
    }

    /// Returns the lifecycle message ownership for one actor.
    ///
    /// Parser-backed models use the resolved `AddMessage` projection. Legacy
    /// direct models fall back to Mermaid's compatibility anchor maps.
    pub fn actor_lifecycle(&self, actor_id: &str) -> Option<SequenceActorLifecycle> {
        match &self.actor_lifecycles {
            Some(_) => self
                .actor_order
                .iter()
                .position(|id| id == actor_id)
                .and_then(|index| self.actor_lifecycle_at(index)),
            None => {
                let lifecycle = SequenceActorLifecycle {
                    created_at: self.created_actors.get(actor_id).copied(),
                    destroyed_at: self.destroyed_actors.get(actor_id).copied(),
                };
                (lifecycle != SequenceActorLifecycle::default()).then_some(lifecycle)
            }
        }
    }

    /// Returns one actor's parser-resolved lifecycle by normalized actor order.
    pub fn actor_lifecycle_at(&self, actor_index: usize) -> Option<SequenceActorLifecycle> {
        match &self.actor_lifecycles {
            Some(lifecycles) => lifecycles
                .get(actor_index)
                .copied()
                .filter(|lifecycle| *lifecycle != SequenceActorLifecycle::default()),
            None => self
                .actor_order
                .get(actor_index)
                .and_then(|actor_id| self.actor_lifecycle(actor_id)),
        }
    }

    /// Returns the effective creation index by normalized actor order.
    pub fn created_actor_message_index_at(&self, actor_index: usize) -> Option<usize> {
        self.actor_lifecycle_at(actor_index)?.created_at
    }

    /// Returns the effective destruction index by normalized actor order.
    pub fn destroyed_actor_message_index_at(&self, actor_index: usize) -> Option<usize> {
        self.actor_lifecycle_at(actor_index)?.destroyed_at
    }

    /// Returns the effective creation index.
    ///
    /// Parser-backed models return the consuming signal index. Legacy direct
    /// models may return the compatibility declaration anchor instead.
    pub fn created_actor_message_index(&self, actor_id: &str) -> Option<usize> {
        self.actor_lifecycle(actor_id)?.created_at
    }

    /// Returns the effective destruction index.
    ///
    /// Parser-backed models return the consuming signal index. Legacy direct
    /// models may return the compatibility declaration anchor instead.
    pub fn destroyed_actor_message_index(&self, actor_id: &str) -> Option<usize> {
        self.actor_lifecycle(actor_id)?.destroyed_at
    }
}

/// Borrowed serializer for Mermaid's public Sequence compatibility JSON shape.
///
/// This remains family-owned so streaming consumers do not duplicate field mappings or clone
/// arbitrary actor property trees before applying their own output budgets.
#[doc(hidden)]
#[derive(Debug, Clone, Copy)]
pub struct SequenceCompatibilityProjection<'a> {
    model: &'a SequenceDiagramRenderModel,
    diagram_type: &'a str,
}

impl Serialize for SequenceCompatibilityProjection<'_> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut root = serializer.serialize_struct("SequenceCompatibilityProjection", 12)?;
        root.serialize_field("type", self.diagram_type)?;
        root.serialize_field("title", &self.model.title)?;
        root.serialize_field("accTitle", &self.model.acc_title)?;
        root.serialize_field("accDescr", &self.model.acc_descr)?;
        root.serialize_field("actorOrder", &self.model.actor_order)?;
        root.serialize_field("actors", &self.model.actors)?;
        root.serialize_field("messages", &self.model.messages)?;
        root.serialize_field("notes", &self.model.notes)?;
        root.serialize_field("boxes", &self.model.boxes)?;
        root.serialize_field("createdActors", &self.model.created_actors)?;
        root.serialize_field("destroyedActors", &self.model.destroyed_actors)?;
        root.serialize_field("constants", &SEQUENCE_CONSTANTS)?;
        root.end()
    }
}

#[derive(Serialize)]
struct SequenceConstants {
    placement: SequencePlacementConstants,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SequencePlacementConstants {
    left_of: i32,
    right_of: i32,
    over: i32,
}

const SEQUENCE_CONSTANTS: SequenceConstants = SequenceConstants {
    placement: SequencePlacementConstants {
        left_of: PLACEMENT_LEFT_OF,
        right_of: PLACEMENT_RIGHT_OF,
        over: PLACEMENT_OVER,
    },
};

pub(crate) fn render_model_to_compat_json(
    model: &SequenceDiagramRenderModel,
    meta: &ParseMetadata,
) -> crate::Result<Value> {
    Ok(model.to_compat_json(&meta.diagram_type))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SequenceActor {
    #[serde(default)]
    pub name: String,
    pub description: String,
    #[serde(rename = "type")]
    pub actor_type: String,
    #[serde(default)]
    pub wrap: bool,
    #[serde(default)]
    pub links: serde_json::Map<String, Value>,
    #[serde(default)]
    pub properties: serde_json::Map<String, Value>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
/// Signal indices that consumed one actor's pending Mermaid lifecycle requests.
pub struct SequenceActorLifecycle {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_at: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub destroyed_at: Option<usize>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
/// Stroke pattern of a drawable Mermaid sequence signal.
pub enum SequenceMessageStroke {
    #[default]
    Solid,
    Dotted,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
/// Marker owned by one authored endpoint of a Mermaid sequence signal.
pub enum SequenceMessageMarker {
    #[default]
    None,
    Filled,
    Cross,
    Point,
    FilledHalfTop,
    FilledHalfBottom,
    OpenHalfTop,
    OpenHalfBottom,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
/// Semantic direction of the signal relative to its authored `from` and `to` actors.
pub enum SequenceMessageDirection {
    #[default]
    Forward,
    Reverse,
    Bidirectional,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
/// Optional central-connection decoration attached to authored signal endpoints.
pub enum SequenceCentralDecoration {
    #[default]
    None,
    Source,
    Target,
    Both,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
/// Semantic family of a Mermaid sequence control record.
pub enum SequenceControlKind {
    Loop,
    Opt,
    Break,
    Alt,
    Par,
    Critical,
    Rect,
    ParOver,
}

impl SequenceControlKind {
    pub fn keyword(self) -> &'static str {
        match self {
            Self::Loop => "loop",
            Self::Opt => "opt",
            Self::Break => "break",
            Self::Alt => "alt",
            Self::Par => "par",
            Self::Critical => "critical",
            Self::Rect => "rect",
            Self::ParOver => "par_over",
        }
    }

    pub fn separator_keyword(self) -> Option<&'static str> {
        match self {
            Self::Alt => Some("else"),
            Self::Par => Some("and"),
            Self::Critical => Some("option"),
            Self::Loop | Self::Opt | Self::Break | Self::Rect | Self::ParOver => None,
        }
    }

    pub fn accepts_end(self, end: Self) -> bool {
        self == end || matches!((self, end), (Self::ParOver, Self::Par))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
/// Structural role of one Mermaid sequence control record.
pub enum SequenceControlRole {
    Start,
    Separator,
    End,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
/// Typed projection of Mermaid's numeric control-record protocol.
pub struct SequenceControlSemantics {
    pub kind: SequenceControlKind,
    pub role: SequenceControlRole,
}

impl SequenceControlSemantics {
    pub fn consumes_text(self) -> bool {
        self.role != SequenceControlRole::End
    }

    fn from_message_type(message_type: i32) -> Option<Self> {
        use SequenceControlKind as Kind;
        use SequenceControlRole as Role;

        let (kind, role) = match message_type {
            LINETYPE_LOOP_START => (Kind::Loop, Role::Start),
            LINETYPE_LOOP_END => (Kind::Loop, Role::End),
            LINETYPE_ALT_START => (Kind::Alt, Role::Start),
            LINETYPE_ALT_ELSE => (Kind::Alt, Role::Separator),
            LINETYPE_ALT_END => (Kind::Alt, Role::End),
            LINETYPE_OPT_START => (Kind::Opt, Role::Start),
            LINETYPE_OPT_END => (Kind::Opt, Role::End),
            LINETYPE_PAR_START => (Kind::Par, Role::Start),
            LINETYPE_PAR_AND => (Kind::Par, Role::Separator),
            LINETYPE_PAR_END => (Kind::Par, Role::End),
            LINETYPE_RECT_START => (Kind::Rect, Role::Start),
            LINETYPE_RECT_END => (Kind::Rect, Role::End),
            LINETYPE_CRITICAL_START => (Kind::Critical, Role::Start),
            LINETYPE_CRITICAL_OPTION => (Kind::Critical, Role::Separator),
            LINETYPE_CRITICAL_END => (Kind::Critical, Role::End),
            LINETYPE_BREAK_START => (Kind::Break, Role::Start),
            LINETYPE_BREAK_END => (Kind::Break, Role::End),
            LINETYPE_PAR_OVER_START => (Kind::ParOver, Role::Start),
            _ => return None,
        };
        Some(Self { kind, role })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
/// Semantic placement of an authored sequence note.
pub enum SequenceNotePlacement {
    LeftOf,
    RightOf,
    Over,
}

impl SequenceNotePlacement {
    fn from_raw(placement: i32) -> Option<Self> {
        match placement {
            PLACEMENT_LEFT_OF => Some(Self::LeftOf),
            PLACEMENT_RIGHT_OF => Some(Self::RightOf),
            PLACEMENT_OVER => Some(Self::Over),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
/// Typed role of one Mermaid compatibility `messages` record.
pub enum SequenceMessageKind {
    Signal,
    Note,
    ActivationStart,
    ActivationEnd,
    Autonumber,
    Control,
    CentralDecorationRecord,
    #[default]
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
/// Lossless terminal-facing projection of Mermaid's numeric signal line types.
pub struct SequenceSignalSemantics {
    pub stroke: SequenceMessageStroke,
    pub source_marker: SequenceMessageMarker,
    pub target_marker: SequenceMessageMarker,
    pub direction: SequenceMessageDirection,
}

impl SequenceSignalSemantics {
    const fn forward(stroke: SequenceMessageStroke, target_marker: SequenceMessageMarker) -> Self {
        Self {
            stroke,
            source_marker: SequenceMessageMarker::None,
            target_marker,
            direction: SequenceMessageDirection::Forward,
        }
    }

    const fn reverse(stroke: SequenceMessageStroke, source_marker: SequenceMessageMarker) -> Self {
        Self {
            stroke,
            source_marker,
            target_marker: SequenceMessageMarker::None,
            direction: SequenceMessageDirection::Reverse,
        }
    }

    const fn bidirectional(stroke: SequenceMessageStroke) -> Self {
        Self {
            stroke,
            source_marker: SequenceMessageMarker::Filled,
            target_marker: SequenceMessageMarker::Filled,
            direction: SequenceMessageDirection::Bidirectional,
        }
    }

    fn from_message_type(message_type: i32) -> Option<Self> {
        use SequenceMessageMarker as Marker;
        use SequenceMessageStroke as Stroke;

        match message_type {
            LINETYPE_SOLID => Some(Self::forward(Stroke::Solid, Marker::Filled)),
            LINETYPE_DOTTED => Some(Self::forward(Stroke::Dotted, Marker::Filled)),
            LINETYPE_SOLID_CROSS => Some(Self::forward(Stroke::Solid, Marker::Cross)),
            LINETYPE_DOTTED_CROSS => Some(Self::forward(Stroke::Dotted, Marker::Cross)),
            // Mermaid calls these numeric line types "open", but 11.16.1 renders them as
            // headless signals: the name distinguishes the grammar form, not an endpoint marker.
            LINETYPE_SOLID_OPEN => Some(Self::forward(Stroke::Solid, Marker::None)),
            LINETYPE_DOTTED_OPEN => Some(Self::forward(Stroke::Dotted, Marker::None)),
            LINETYPE_SOLID_POINT => Some(Self::forward(Stroke::Solid, Marker::Point)),
            LINETYPE_DOTTED_POINT => Some(Self::forward(Stroke::Dotted, Marker::Point)),
            LINETYPE_BIDIRECTIONAL_SOLID => Some(Self::bidirectional(Stroke::Solid)),
            LINETYPE_BIDIRECTIONAL_DOTTED => Some(Self::bidirectional(Stroke::Dotted)),
            LINETYPE_SOLID_TOP => Some(Self::forward(Stroke::Solid, Marker::FilledHalfTop)),
            LINETYPE_SOLID_BOTTOM => Some(Self::forward(Stroke::Solid, Marker::FilledHalfBottom)),
            LINETYPE_STICK_TOP => Some(Self::forward(Stroke::Solid, Marker::OpenHalfTop)),
            LINETYPE_STICK_BOTTOM => Some(Self::forward(Stroke::Solid, Marker::OpenHalfBottom)),
            LINETYPE_SOLID_TOP_DOTTED => Some(Self::forward(Stroke::Dotted, Marker::FilledHalfTop)),
            LINETYPE_SOLID_BOTTOM_DOTTED => {
                Some(Self::forward(Stroke::Dotted, Marker::FilledHalfBottom))
            }
            LINETYPE_STICK_TOP_DOTTED => Some(Self::forward(Stroke::Dotted, Marker::OpenHalfTop)),
            LINETYPE_STICK_BOTTOM_DOTTED => {
                Some(Self::forward(Stroke::Dotted, Marker::OpenHalfBottom))
            }
            LINETYPE_SOLID_ARROW_TOP_REVERSE => {
                Some(Self::reverse(Stroke::Solid, Marker::FilledHalfTop))
            }
            LINETYPE_SOLID_ARROW_BOTTOM_REVERSE => {
                Some(Self::reverse(Stroke::Solid, Marker::FilledHalfBottom))
            }
            LINETYPE_STICK_ARROW_TOP_REVERSE => {
                Some(Self::reverse(Stroke::Solid, Marker::OpenHalfTop))
            }
            LINETYPE_STICK_ARROW_BOTTOM_REVERSE => {
                Some(Self::reverse(Stroke::Solid, Marker::OpenHalfBottom))
            }
            LINETYPE_SOLID_ARROW_TOP_REVERSE_DOTTED => {
                Some(Self::reverse(Stroke::Dotted, Marker::FilledHalfTop))
            }
            LINETYPE_SOLID_ARROW_BOTTOM_REVERSE_DOTTED => {
                Some(Self::reverse(Stroke::Dotted, Marker::FilledHalfBottom))
            }
            LINETYPE_STICK_ARROW_TOP_REVERSE_DOTTED => {
                Some(Self::reverse(Stroke::Dotted, Marker::OpenHalfTop))
            }
            LINETYPE_STICK_ARROW_BOTTOM_REVERSE_DOTTED => {
                Some(Self::reverse(Stroke::Dotted, Marker::OpenHalfBottom))
            }
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SequenceMessage {
    pub id: String,
    #[serde(default)]
    pub from: Option<String>,
    #[serde(default)]
    pub to: Option<String>,
    #[serde(rename = "type")]
    pub message_type: i32,
    pub message: SequenceMessagePayload,
    #[serde(default)]
    pub wrap: bool,
    #[serde(default)]
    pub activate: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub placement: Option<i32>,
    #[serde(
        rename = "centralConnection",
        default,
        skip_serializing_if = "is_zero_i32"
    )]
    pub central_connection: i32,
}

impl SequenceMessage {
    pub fn message_text(&self) -> &str {
        self.message.as_text()
    }

    pub fn semantic_kind(&self) -> SequenceMessageKind {
        if self.signal_semantics().is_some() {
            return SequenceMessageKind::Signal;
        }

        match self.message_type {
            LINETYPE_NOTE => SequenceMessageKind::Note,
            LINETYPE_ACTIVE_START => SequenceMessageKind::ActivationStart,
            LINETYPE_ACTIVE_END => SequenceMessageKind::ActivationEnd,
            LINETYPE_AUTONUMBER => SequenceMessageKind::Autonumber,
            _ if self.central_record_decoration().is_some() => {
                SequenceMessageKind::CentralDecorationRecord
            }
            _ if self.control_semantics().is_some() => SequenceMessageKind::Control,
            _ => SequenceMessageKind::Unknown,
        }
    }

    pub fn signal_semantics(&self) -> Option<SequenceSignalSemantics> {
        SequenceSignalSemantics::from_message_type(self.message_type)
    }

    pub fn central_decoration(&self) -> Option<SequenceCentralDecoration> {
        match self.central_connection {
            0 => Some(SequenceCentralDecoration::None),
            LINETYPE_CENTRAL_CONNECTION => Some(SequenceCentralDecoration::Target),
            LINETYPE_CENTRAL_CONNECTION_REVERSE => Some(SequenceCentralDecoration::Source),
            LINETYPE_CENTRAL_CONNECTION_DUAL => Some(SequenceCentralDecoration::Both),
            _ => None,
        }
    }

    pub fn central_record_decoration(&self) -> Option<SequenceCentralDecoration> {
        match self.message_type {
            LINETYPE_CENTRAL_CONNECTION => Some(SequenceCentralDecoration::Target),
            LINETYPE_CENTRAL_CONNECTION_REVERSE => Some(SequenceCentralDecoration::Source),
            LINETYPE_CENTRAL_CONNECTION_DUAL => Some(SequenceCentralDecoration::Both),
            _ => None,
        }
    }

    pub fn control_semantics(&self) -> Option<SequenceControlSemantics> {
        SequenceControlSemantics::from_message_type(self.message_type)
    }

    pub fn note_placement(&self) -> Option<SequenceNotePlacement> {
        SequenceNotePlacement::from_raw(self.placement.unwrap_or(PLACEMENT_OVER))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum SequenceMessagePayload {
    Text(String),
    Autonumber(SequenceAutonumber),
}

impl SequenceMessagePayload {
    pub fn as_text(&self) -> &str {
        match self {
            Self::Text(text) => text,
            Self::Autonumber(_) => "",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SequenceAutonumber {
    #[serde(
        skip_serializing_if = "Option::is_none",
        serialize_with = "serialize_optional_sequence_number"
    )]
    pub start: Option<f64>,
    #[serde(
        skip_serializing_if = "Option::is_none",
        serialize_with = "serialize_optional_sequence_number"
    )]
    pub step: Option<f64>,
    #[serde(default = "default_true")]
    pub visible: bool,
}

fn serialize_optional_sequence_number<S>(
    value: &Option<f64>,
    serializer: S,
) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    match value {
        Some(value) => serialize_sequence_number(*value, serializer),
        None => serializer.serialize_none(),
    }
}

fn serialize_sequence_number<S>(value: f64, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    if value.is_finite()
        && value.fract() == 0.0
        && value >= i64::MIN as f64
        && value <= i64::MAX as f64
    {
        serializer.serialize_i64(value as i64)
    } else {
        serializer.serialize_f64(value)
    }
}

fn default_true() -> bool {
    true
}

fn is_zero_i32(value: &i32) -> bool {
    *value == 0
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SequenceBox {
    #[serde(rename = "actorKeys")]
    pub actor_keys: Vec<String>,
    pub fill: String,
    pub name: Option<String>,
    #[serde(default)]
    pub wrap: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SequenceNote {
    pub actor: Value,
    pub message: String,
    pub placement: i32,
    #[serde(default)]
    pub wrap: bool,
}

impl SequenceNote {
    pub fn note_placement(&self) -> Option<SequenceNotePlacement> {
        SequenceNotePlacement::from_raw(self.placement)
    }
}
