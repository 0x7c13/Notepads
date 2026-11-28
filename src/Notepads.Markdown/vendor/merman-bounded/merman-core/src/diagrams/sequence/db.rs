use crate::{OperationControl, OperationControlResult, ParseMetadata};
use rustc_hash::FxHashMap;
use serde_json::{Value, json};
use std::collections::BTreeMap;

use super::Action;
use super::render_model::{
    SequenceActor, SequenceActorLifecycle, SequenceAutonumber, SequenceBox,
    SequenceDiagramRenderModel, SequenceMessage, SequenceMessagePayload, SequenceNote,
};
use super::{
    LINETYPE_ACTIVE_END, LINETYPE_ACTIVE_START, LINETYPE_AUTONUMBER, LINETYPE_CENTRAL_CONNECTION,
    LINETYPE_CENTRAL_CONNECTION_REVERSE, LINETYPE_NOTE,
};

#[derive(Debug, Clone)]
struct ParsedText {
    text: String,
    wrap: Option<bool>,
}

#[derive(Debug, Clone)]
struct Actor {
    name: String,
    description: String,
    wrap: bool,
    actor_type: String,
    box_index: Option<usize>,
    links: serde_json::Map<String, Value>,
    properties: serde_json::Map<String, Value>,
    lifecycle: SequenceActorLifecycle,
}

#[derive(Debug, Clone)]
struct Message {
    id: String,
    from: Option<String>,
    to: Option<String>,
    message: SequenceMessagePayload,
    wrap: bool,
    message_type: i32,
    activate: bool,
    placement: Option<i32>,
    central_connection: i32,
}

#[derive(Debug, Default)]
struct SignalInput {
    from: Option<String>,
    to: Option<String>,
    message: Option<ParsedText>,
    message_type: i32,
    activate: bool,
    placement: Option<i32>,
    central_connection: i32,
}

#[derive(Debug, Clone)]
struct Note {
    actor: Value,
    placement: i32,
    message: String,
    wrap: bool,
}

#[derive(Debug, Clone)]
struct SeqBox {
    name: Option<String>,
    fill: String,
    wrap: bool,
    actor_keys: Vec<String>,
}

#[derive(Debug, Default)]
pub(super) struct SequenceDb {
    actors: FxHashMap<String, Actor>,
    actor_order: Vec<String>,
    messages: Vec<Message>,
    notes: Vec<Note>,
    boxes: Vec<SeqBox>,
    current_box: Option<usize>,
    wrap_enabled: Option<bool>,

    created_actors: FxHashMap<String, usize>,
    destroyed_actors: FxHashMap<String, usize>,
    last_created: Option<String>,
    last_destroyed: Option<String>,

    title: Option<String>,
    acc_title: Option<String>,
    acc_descr: Option<String>,
}

impl SequenceDb {
    pub(super) fn new(wrap_enabled: Option<bool>) -> Self {
        Self {
            wrap_enabled,
            ..Default::default()
        }
    }

    fn auto_wrap(&self) -> bool {
        self.wrap_enabled.unwrap_or(false)
    }

    fn ensure_actor(&mut self, id: &str) {
        if self.actors.contains_key(id) {
            return;
        }
        let id_owned = id.to_string();
        self.actor_order.push(id_owned.clone());
        self.actors.insert(
            id_owned.clone(),
            Actor {
                name: id_owned.clone(),
                description: id_owned,
                wrap: self.auto_wrap(),
                actor_type: "participant".to_string(),
                box_index: None,
                links: serde_json::Map::new(),
                properties: serde_json::Map::new(),
                lifecycle: SequenceActorLifecycle::default(),
            },
        );
    }

    fn add_actor(
        &mut self,
        id: &str,
        description: Option<String>,
        actor_type: &str,
        participant_meta: Option<Value>,
    ) -> std::result::Result<(), String> {
        let mut actor_type = actor_type.to_string();
        let mut config_alias = None;
        if let Some(meta) = participant_meta.as_ref() {
            if let Some(obj) = meta.as_object()
                && let Some(t) = obj
                    .get("type")
                    .and_then(crate::inline_config::value_to_string)
            {
                actor_type = t;
            }
            if let Some(obj) = meta.as_object() {
                config_alias = obj
                    .get("alias")
                    .and_then(crate::inline_config::value_to_string);
            }
        }

        if let Some(current_box) = self.current_box
            && let Some(existing) = self.actors.get(id)
            && let Some(old_box) = existing.box_index
            && old_box != current_box
        {
            let old_name = self.boxes[old_box]
                .name
                .clone()
                .unwrap_or_else(|| "undefined".to_string());
            let new_name = self.boxes[current_box]
                .name
                .clone()
                .unwrap_or_else(|| "undefined".to_string());
            return Err(format!(
                "A same participant should only be defined in one Box: {} can't be in '{}' and in '{}' at the same time.",
                existing.name, old_name, new_name
            ));
        }

        let description = description
            .or(config_alias)
            .map(|s| self.parse_message(&s))
            .unwrap_or_else(|| ParsedText {
                text: id.to_string(),
                wrap: None,
            });

        let wrap = description.wrap.unwrap_or(self.auto_wrap());

        if let Some(existing) = self.actors.get_mut(id) {
            existing.description = description.text;
            existing.wrap = wrap;
            existing.actor_type = actor_type;
            if let Some(current_box) = self.current_box {
                if existing.box_index.is_none() {
                    existing.box_index = Some(current_box);
                }
                self.boxes[current_box].actor_keys.push(id.to_string());
            }
            return Ok(());
        }

        self.actor_order.push(id.to_string());
        self.actors.insert(
            id.to_string(),
            Actor {
                name: id.to_string(),
                description: description.text,
                wrap,
                actor_type,
                box_index: self.current_box,
                links: serde_json::Map::new(),
                properties: serde_json::Map::new(),
                lifecycle: SequenceActorLifecycle::default(),
            },
        );

        if let Some(current_box) = self.current_box {
            self.boxes[current_box].actor_keys.push(id.to_string());
        }

        Ok(())
    }

    fn parse_message(&self, raw: &str) -> ParsedText {
        let trimmed = raw.trim();
        fn strip_prefix_ci<'a>(s: &'a str, prefix: &[u8]) -> Option<&'a str> {
            let bytes = s.as_bytes();
            if bytes.len() < prefix.len() {
                return None;
            }
            for i in 0..prefix.len() {
                if !bytes[i].eq_ignore_ascii_case(&prefix[i]) {
                    return None;
                }
            }
            Some(&s[prefix.len()..])
        }

        let (wrap, cleaned) = if trimmed.len() >= 5
            && matches!(
                trimmed.as_bytes().first().copied(),
                Some(b':' | b'w' | b'W' | b'n' | b'N')
            ) {
            if let Some(rest) = strip_prefix_ci(trimmed, b":wrap:") {
                (Some(true), rest.trim())
            } else if let Some(rest) = strip_prefix_ci(trimmed, b"wrap:") {
                (Some(true), rest.trim())
            } else if let Some(rest) = strip_prefix_ci(trimmed, b":nowrap:") {
                (Some(false), rest.trim())
            } else if let Some(rest) = strip_prefix_ci(trimmed, b"nowrap:") {
                (Some(false), rest.trim())
            } else {
                (None, trimmed)
            }
        } else {
            (None, trimmed)
        };

        ParsedText {
            text: cleaned.to_string(),
            wrap,
        }
    }

    fn add_signal(&mut self, signal: SignalInput) {
        let msg_text = signal.message.unwrap_or(ParsedText {
            text: String::new(),
            wrap: None,
        });
        let wrap = msg_text.wrap.unwrap_or(self.auto_wrap());

        self.messages.push(Message {
            id: self.messages.len().to_string(),
            from: signal.from,
            to: signal.to,
            message: SequenceMessagePayload::Text(msg_text.text),
            wrap,
            message_type: signal.message_type,
            activate: signal.activate,
            placement: signal.placement,
            central_connection: signal.central_connection,
        });
    }

    fn activation_count(&self, actor: &str) -> i32 {
        if actor.is_empty() {
            return 0;
        }
        let mut count = 0;
        for msg in &self.messages {
            if msg.message_type == LINETYPE_ACTIVE_START
                && msg.from.as_deref().is_some_and(|a| a == actor)
            {
                count += 1;
            }
            if msg.message_type == LINETYPE_ACTIVE_END
                && msg.from.as_deref().is_some_and(|a| a == actor)
            {
                count -= 1;
            }
        }
        count
    }

    fn add_autonumber(&mut self, start: Option<f64>, step: Option<f64>, visible: bool) {
        self.messages.push(Message {
            id: self.messages.len().to_string(),
            from: None,
            to: None,
            message: SequenceMessagePayload::Autonumber(SequenceAutonumber {
                start,
                step,
                visible,
            }),
            wrap: false,
            message_type: LINETYPE_AUTONUMBER,
            activate: false,
            placement: None,
            central_connection: 0,
        });
    }

    fn add_note(&mut self, actors: Vec<String>, placement: i32, raw_text: String) {
        let parsed = self.parse_message(&raw_text);
        let wrap = parsed.wrap.unwrap_or(self.auto_wrap());

        let actor_value = match actors.as_slice() {
            [a] => Value::String(a.clone()),
            [a, b] => json!([a, b]),
            _ => json!(actors),
        };

        self.notes.push(Note {
            actor: actor_value.clone(),
            placement,
            message: parsed.text.clone(),
            wrap,
        });

        let (from, to) = match actors.as_slice() {
            [a] => (Some(a.clone()), Some(a.clone())),
            [a, b] => (Some(a.clone()), Some(b.clone())),
            _ => (
                actors.first().cloned(),
                actors.get(1).cloned().or_else(|| actors.first().cloned()),
            ),
        };

        self.messages.push(Message {
            id: self.messages.len().to_string(),
            from,
            to,
            message: SequenceMessagePayload::Text(parsed.text),
            wrap,
            message_type: LINETYPE_NOTE,
            activate: false,
            placement: Some(placement),
            central_connection: 0,
        });
    }

    pub(super) fn apply_controlled(
        &mut self,
        action: Action,
        control: &OperationControl,
    ) -> OperationControlResult<std::result::Result<(), String>> {
        control.checkpoint()?;
        let participant_meta = match &action {
            Action::AddParticipant { config, .. } => {
                match parse_participant_meta_controlled(config.as_deref(), control)? {
                    Ok(meta) => meta,
                    Err(error) => return Ok(Err(error)),
                }
            }
            Action::CreateParticipant { id, config, .. } => {
                if self.actors.contains_key(id) {
                    return Ok(Err("It is not possible to have actors with the same id, even if one is destroyed before the next is created. Use 'AS' aliases to simulate the behavior".to_string()));
                }
                match parse_participant_meta_controlled(config.as_deref(), control)? {
                    Ok(meta) => meta,
                    Err(error) => return Ok(Err(error)),
                }
            }
            _ => None,
        };
        control.checkpoint()?;
        Ok(self.apply_prepared(action, participant_meta))
    }

    fn apply_prepared(
        &mut self,
        action: Action,
        participant_meta: Option<Value>,
    ) -> std::result::Result<(), String> {
        match action {
            Action::SetTitle(t) => {
                self.title = Some(t.trim().to_string());
                Ok(())
            }
            Action::SetAccTitle(t) => {
                self.acc_title = Some(t.trim().to_string());
                Ok(())
            }
            Action::SetAccDescr(t) => {
                self.acc_descr = Some(t.trim().to_string());
                Ok(())
            }

            Action::EnsureParticipant { id } => {
                self.ensure_actor(&id);
                Ok(())
            }
            Action::AddParticipant {
                id,
                description,
                draw,
                config,
            } => {
                let _ = config;
                self.add_actor(&id, description, &draw, participant_meta)
            }

            Action::CreateParticipant {
                id,
                description,
                draw,
                config,
            } => {
                if self.actors.contains_key(&id) {
                    return Err("It is not possible to have actors with the same id, even if one is destroyed before the next is created. Use 'AS' aliases to simulate the behavior".to_string());
                }
                self.last_created = Some(id.clone());
                let _ = config;
                self.add_actor(&id, description, &draw, participant_meta)?;
                self.created_actors.insert(id, self.messages.len());
                Ok(())
            }
            Action::DestroyParticipant { id } => {
                self.last_destroyed = Some(id.clone());
                self.destroyed_actors.insert(id, self.messages.len());
                Ok(())
            }

            Action::ControlSignal { signal_type, text } => {
                let msg = text.as_deref().map(|t| self.parse_message(t));
                self.add_signal(SignalInput {
                    message: msg,
                    message_type: signal_type,
                    ..Default::default()
                });
                Ok(())
            }

            Action::BoxStart { header } => {
                self.add_box(&header);
                Ok(())
            }
            Action::BoxEnd => {
                self.current_box = None;
                Ok(())
            }

            Action::AddLinks { actor, text } => {
                self.add_links(&actor, &text);
                Ok(())
            }
            Action::AddLink { actor, text } => {
                self.add_link(&actor, &text);
                Ok(())
            }
            Action::AddProperties { actor, text } => {
                self.add_properties(&actor, &text);
                Ok(())
            }
            Action::AddDetails { actor, text } => {
                let _ = (actor, text);
                Ok(())
            }

            Action::AddMessage {
                from,
                to,
                signal_type,
                text,
                activate,
                central_connection,
            } => {
                if let Some(last_created) = self.last_created.clone() {
                    if to != last_created {
                        return Err(format!(
                            "The created participant {last_created} does not have an associated creating message after its declaration. Please check the sequence diagram."
                        ));
                    }
                    self.last_created = None;
                    if let Some(actor) = self.actors.get_mut(&last_created) {
                        actor.lifecycle.created_at = Some(self.messages.len());
                    }
                } else if let Some(last_destroyed) = self.last_destroyed.clone() {
                    if from != last_destroyed && to != last_destroyed {
                        return Err(format!(
                            "The destroyed participant {last_destroyed} does not have an associated destroying message after its declaration. Please check the sequence diagram."
                        ));
                    }
                    self.last_destroyed = None;
                    if let Some(actor) = self.actors.get_mut(&last_destroyed) {
                        actor.lifecycle.destroyed_at = Some(self.messages.len());
                    }
                }

                let msg = self.parse_message(&text);
                self.add_signal(SignalInput {
                    from: Some(from),
                    to: Some(to),
                    message: Some(msg),
                    message_type: signal_type,
                    activate,
                    central_connection,
                    ..Default::default()
                });
                Ok(())
            }

            Action::ActiveStart { actor } => {
                self.add_signal(SignalInput {
                    from: Some(actor),
                    message_type: LINETYPE_ACTIVE_START,
                    ..Default::default()
                });
                Ok(())
            }
            Action::ActiveEnd { actor } => {
                if self.activation_count(&actor) < 1 {
                    return Err(format!(
                        "Trying to inactivate an inactive participant ({actor})"
                    ));
                }
                self.add_signal(SignalInput {
                    from: Some(actor),
                    message_type: LINETYPE_ACTIVE_END,
                    ..Default::default()
                });
                Ok(())
            }
            Action::CentralConnection { actor } => {
                self.add_signal(SignalInput {
                    from: Some(actor),
                    message_type: LINETYPE_CENTRAL_CONNECTION,
                    ..Default::default()
                });
                Ok(())
            }
            Action::CentralConnectionReverse { actor } => {
                self.add_signal(SignalInput {
                    from: Some(actor),
                    message_type: LINETYPE_CENTRAL_CONNECTION_REVERSE,
                    ..Default::default()
                });
                Ok(())
            }

            Action::AddNote {
                actors,
                placement,
                text,
            } => {
                self.add_note(actors, placement, text);
                Ok(())
            }

            Action::Autonumber {
                start,
                step,
                visible,
            } => {
                self.add_autonumber(start, step, visible);
                Ok(())
            }
        }
    }

    fn add_box(&mut self, raw: &str) {
        let data = self.parse_box_data(raw);
        let wrap = data.wrap.unwrap_or(self.auto_wrap());
        self.boxes.push(SeqBox {
            name: data.text,
            fill: data.color,
            wrap,
            actor_keys: Vec::new(),
        });
        self.current_box = Some(self.boxes.len() - 1);
    }

    fn parse_box_data(&self, raw: &str) -> BoxData {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return BoxData {
                text: None,
                color: "transparent".to_string(),
                wrap: None,
            };
        }

        let (color_candidate, title_candidate) = split_box_color_and_title(trimmed);
        let mut color = if color_candidate.trim().is_empty() {
            "transparent".to_string()
        } else {
            color_candidate.trim().to_string()
        };
        let mut title = title_candidate.trim().to_string();

        if !is_css_color_value(&color) {
            color = "transparent".to_string();
            title = trimmed.to_string();
        }

        let parsed_title = self.parse_message(&title);
        let text = if parsed_title.text.is_empty() {
            None
        } else {
            Some(parsed_title.text)
        };

        BoxData {
            text,
            color,
            wrap: parsed_title.wrap,
        }
    }

    fn add_links(&mut self, actor: &str, raw_text: &str) {
        let s = unescape_entities(raw_text);
        let Ok(v) = serde_json::from_str::<Value>(&s) else {
            return;
        };
        let Some(obj) = v.as_object() else {
            return;
        };
        let Some(a) = self.actors.get_mut(actor) else {
            return;
        };
        for (k, v) in obj {
            if let Some(url) = v.as_str() {
                a.links.insert(k.clone(), Value::String(url.to_string()));
            }
        }
    }

    fn add_link(&mut self, actor: &str, raw_text: &str) {
        let s = unescape_entities(raw_text);
        let Some(idx) = s.find('@') else {
            return;
        };
        let (left, right) = s.split_at(idx);
        let label = left.strip_suffix(' ').unwrap_or(left).trim();
        let url = right.trim_start_matches('@').trim();
        if label.is_empty() || url.is_empty() {
            return;
        }
        let Some(a) = self.actors.get_mut(actor) else {
            return;
        };
        a.links
            .insert(label.to_string(), Value::String(url.to_string()));
    }

    fn add_properties(&mut self, actor: &str, raw_text: &str) {
        let s = unescape_entities(raw_text);
        let Ok(v) = serde_json::from_str::<Value>(&s) else {
            return;
        };
        let Some(obj) = v.as_object() else {
            return;
        };
        let Some(a) = self.actors.get_mut(actor) else {
            return;
        };
        for (k, v) in obj {
            a.properties.insert(k.clone(), v.clone());
        }
    }

    pub(super) fn into_model(self, meta: &ParseMetadata) -> Value {
        self.into_render_model().to_compat_json(&meta.diagram_type)
    }

    pub(super) fn into_render_model(mut self) -> SequenceDiagramRenderModel {
        let mut actors = std::mem::take(&mut self.actors);
        let mut actors_typed = BTreeMap::new();
        let mut actor_lifecycles = Vec::with_capacity(self.actor_order.len());
        for id in &self.actor_order {
            if let Some(a) = actors.remove(id) {
                actor_lifecycles.push(a.lifecycle);
                actors_typed.insert(
                    id.clone(),
                    SequenceActor {
                        name: a.name,
                        description: a.description,
                        wrap: a.wrap,
                        actor_type: a.actor_type,
                        links: a.links,
                        properties: a.properties,
                    },
                );
            } else {
                actor_lifecycles.push(SequenceActorLifecycle::default());
            }
        }

        let messages = std::mem::take(&mut self.messages)
            .into_iter()
            .map(|m| SequenceMessage {
                id: m.id,
                from: m.from,
                to: m.to,
                message: m.message,
                wrap: m.wrap,
                message_type: m.message_type,
                activate: m.activate,
                placement: m.placement,
                central_connection: m.central_connection,
            })
            .collect();

        let notes = std::mem::take(&mut self.notes)
            .into_iter()
            .map(|n| SequenceNote {
                actor: n.actor,
                placement: n.placement,
                message: n.message,
                wrap: n.wrap,
            })
            .collect();

        let boxes = std::mem::take(&mut self.boxes)
            .into_iter()
            .map(|b| SequenceBox {
                name: b.name,
                wrap: b.wrap,
                fill: b.fill,
                actor_keys: b.actor_keys,
            })
            .collect();

        SequenceDiagramRenderModel {
            title: self.title,
            acc_title: self.acc_title,
            acc_descr: self.acc_descr,
            actor_order: std::mem::take(&mut self.actor_order),
            actors: actors_typed,
            messages,
            notes,
            boxes,
            created_actors: std::mem::take(&mut self.created_actors)
                .into_iter()
                .collect(),
            destroyed_actors: std::mem::take(&mut self.destroyed_actors)
                .into_iter()
                .collect(),
            actor_lifecycles: Some(actor_lifecycles),
        }
    }
}

fn parse_participant_meta_controlled(
    input: Option<&str>,
    control: &OperationControl,
) -> OperationControlResult<std::result::Result<Option<Value>, String>> {
    let Some(input) = input else {
        return Ok(Ok(None));
    };
    Ok(crate::inline_config::parse_mermaid_inline_object_controlled(input, control)?.map(Some))
}

#[derive(Debug, Clone)]
struct BoxData {
    text: Option<String>,
    color: String,
    wrap: Option<bool>,
}

fn unescape_entities(input: &str) -> String {
    input.replace("&equals;", "=").replace("&amp;", "&")
}

pub(super) fn split_box_color_and_title(input: &str) -> (&str, &str) {
    let lower = input.to_ascii_lowercase();
    for prefix in ["rgba", "rgb", "hsla", "hsl"] {
        if lower.starts_with(prefix)
            && let Some(end) = input.find(')')
        {
            let color = &input[..=end];
            let rest = &input[end + 1..];
            return (color.trim(), rest);
        }
    }

    let mut end = 0usize;
    for (idx, c) in input.char_indices() {
        if c.is_ascii_alphanumeric() || c == '_' {
            end = idx + c.len_utf8();
            continue;
        }
        break;
    }
    (&input[..end], &input[end..])
}

pub(super) fn is_css_color_value(input: &str) -> bool {
    let t = input.trim();
    if t.is_empty() {
        return false;
    }
    let lower = t.to_ascii_lowercase();
    if lower == "transparent" {
        return true;
    }
    if (lower.starts_with("rgb(")
        || lower.starts_with("rgba(")
        || lower.starts_with("hsl(")
        || lower.starts_with("hsla("))
        && lower.ends_with(')')
    {
        return true;
    }
    if !lower.chars().all(|c| c.is_ascii_alphabetic()) {
        return false;
    }
    CSS_COLOR_KEYWORDS.binary_search(&lower.as_str()).is_ok()
}

static CSS_COLOR_KEYWORDS: &[&str] = &[
    "aliceblue",
    "antiquewhite",
    "aqua",
    "aquamarine",
    "azure",
    "beige",
    "bisque",
    "black",
    "blanchedalmond",
    "blue",
    "blueviolet",
    "brown",
    "burlywood",
    "cadetblue",
    "chartreuse",
    "chocolate",
    "coral",
    "cornflowerblue",
    "cornsilk",
    "crimson",
    "cyan",
    "darkblue",
    "darkcyan",
    "darkgoldenrod",
    "darkgray",
    "darkgreen",
    "darkgrey",
    "darkkhaki",
    "darkmagenta",
    "darkolivegreen",
    "darkorange",
    "darkorchid",
    "darkred",
    "darksalmon",
    "darkseagreen",
    "darkslateblue",
    "darkslategray",
    "darkslategrey",
    "darkturquoise",
    "darkviolet",
    "deeppink",
    "deepskyblue",
    "dimgray",
    "dimgrey",
    "dodgerblue",
    "firebrick",
    "floralwhite",
    "forestgreen",
    "fuchsia",
    "gainsboro",
    "ghostwhite",
    "gold",
    "goldenrod",
    "gray",
    "green",
    "greenyellow",
    "grey",
    "honeydew",
    "hotpink",
    "indianred",
    "indigo",
    "ivory",
    "khaki",
    "lavender",
    "lavenderblush",
    "lawngreen",
    "lemonchiffon",
    "lightblue",
    "lightcoral",
    "lightcyan",
    "lightgoldenrodyellow",
    "lightgray",
    "lightgreen",
    "lightgrey",
    "lightpink",
    "lightsalmon",
    "lightseagreen",
    "lightskyblue",
    "lightslategray",
    "lightslategrey",
    "lightsteelblue",
    "lightyellow",
    "lime",
    "limegreen",
    "linen",
    "magenta",
    "maroon",
    "mediumaquamarine",
    "mediumblue",
    "mediumorchid",
    "mediumpurple",
    "mediumseagreen",
    "mediumslateblue",
    "mediumspringgreen",
    "mediumturquoise",
    "mediumvioletred",
    "midnightblue",
    "mintcream",
    "mistyrose",
    "moccasin",
    "navajowhite",
    "navy",
    "oldlace",
    "olive",
    "olivedrab",
    "orange",
    "orangered",
    "orchid",
    "palegoldenrod",
    "palegreen",
    "paleturquoise",
    "palevioletred",
    "papayawhip",
    "peachpuff",
    "peru",
    "pink",
    "plum",
    "powderblue",
    "purple",
    "rebeccapurple",
    "red",
    "rosybrown",
    "royalblue",
    "saddlebrown",
    "salmon",
    "sandybrown",
    "seagreen",
    "seashell",
    "sienna",
    "silver",
    "skyblue",
    "slateblue",
    "slategray",
    "slategrey",
    "snow",
    "springgreen",
    "steelblue",
    "tan",
    "teal",
    "thistle",
    "tomato",
    "turquoise",
    "violet",
    "wheat",
    "white",
    "whitesmoke",
    "yellow",
    "yellowgreen",
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn duplicate_created_participant_wins_over_invalid_inline_config() {
        let control = OperationControl::new();
        let mut db = SequenceDb::new(None);
        db.apply_controlled(
            Action::AddParticipant {
                id: "A".to_string(),
                description: None,
                draw: "participant".to_string(),
                config: None,
            },
            &control,
        )
        .unwrap()
        .unwrap();

        let error = db
            .apply_controlled(
                Action::CreateParticipant {
                    id: "A".to_string(),
                    description: None,
                    draw: "participant".to_string(),
                    config: Some(r#"{ "type" "control" }"#.to_string()),
                },
                &control,
            )
            .unwrap()
            .unwrap_err();

        assert!(error.contains("same id"), "{error}");
    }
}
