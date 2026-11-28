use merman_core::MermaidConfig;
use merman_core::theme_color::{ColorChannel, ThemeColor};
use serde_json::{Map, Value};

use super::{HostTheme, ThemeRole};

pub(super) fn compile(theme: &HostTheme) -> MermaidConfig {
    let mut root = Map::new();
    let mut theme_variables = Map::new();
    let dark = theme.appearance().unwrap_or_default().is_dark();

    if theme.has_values() {
        root.insert("theme".to_string(), Value::String("base".to_string()));
        root.insert("darkMode".to_string(), Value::Bool(dark));
        theme_variables.insert("darkMode".to_string(), Value::Bool(dark));
    }

    if let Some(font_family) = theme.font_family() {
        root.insert(
            "fontFamily".to_string(),
            Value::String(font_family.to_string()),
        );
        put_str(&mut theme_variables, "fontFamily", font_family);
    }
    if let Some(font_size) = theme.font_size() {
        put_str(&mut theme_variables, "fontSize", font_size);
    }

    let roles = ResolvedThemeRoles::new(theme);
    put_theme_roles(&mut theme_variables, &roles);
    put_series_palette(
        &mut theme_variables,
        theme.series_palette(),
        roles.canvas,
        dark,
    );
    put_diagram_config(
        &mut root,
        &mut theme_variables,
        &roles,
        theme.series_palette(),
    );

    if !theme_variables.is_empty() {
        root.insert("themeVariables".to_string(), Value::Object(theme_variables));
    }
    MermaidConfig::from_value(Value::Object(root))
}

/// Keep the compiler's typography and profile settings when an authored palette is selected.
/// All other compiler outputs are paint defaults, never raw initialize variables.
pub(super) fn non_paint_defaults(config: &MermaidConfig) -> MermaidConfig {
    let mut defaults = MermaidConfig::default();
    for path in [
        "fontFamily",
        "look",
        "layout",
        "flowchart.layout",
        "themeVariables.fontFamily",
        "themeVariables.fontSize",
    ] {
        let value = path
            .split('.')
            .try_fold(config.as_value(), |value, key| value.get(key));
        if let Some(value) = value {
            defaults.set_value(path, value.clone());
        }
    }
    defaults
}

#[derive(Debug, Clone, Copy)]
struct ResolvedThemeRoles<'a> {
    canvas: Option<&'a str>,
    surface: Option<&'a str>,
    surface_alt: Option<&'a str>,
    surface_muted: Option<&'a str>,
    text: Option<&'a str>,
    subtle_text: Option<&'a str>,
    border: Option<&'a str>,
    line: Option<&'a str>,
    edge_label_background: Option<&'a str>,
    commit_label_background: Option<&'a str>,
    cluster_background: Option<&'a str>,
    swimlane_background_odd: Option<&'a str>,
    cluster_border: Option<&'a str>,
    note_background: Option<&'a str>,
    note_border: Option<&'a str>,
    note_text: Option<&'a str>,
    actor_background: Option<&'a str>,
    actor_border: Option<&'a str>,
    actor_text: Option<&'a str>,
    activation_background: Option<&'a str>,
    activation_border: Option<&'a str>,
    error: Option<&'a str>,
    warning: Option<&'a str>,
    success: Option<&'a str>,
}

impl<'a> ResolvedThemeRoles<'a> {
    fn new(theme: &'a HostTheme) -> Self {
        let role = |role| theme.role(role);
        let canvas = role(ThemeRole::Canvas);
        let surface = role(ThemeRole::Surface);
        let surface_alt = role(ThemeRole::SurfaceAlt).or(surface);
        let surface_muted = role(ThemeRole::SurfaceMuted).or(surface_alt);
        let text = role(ThemeRole::Text);
        let subtle_text = role(ThemeRole::SubtleText).or(text);
        let border = role(ThemeRole::Border);
        let line = role(ThemeRole::Line).or(border);

        Self {
            canvas,
            surface,
            surface_alt,
            surface_muted,
            text,
            subtle_text,
            border,
            line,
            edge_label_background: role(ThemeRole::EdgeLabelBackground).or(canvas),
            commit_label_background: role(ThemeRole::EdgeLabelBackground).or(surface),
            cluster_background: role(ThemeRole::ClusterBackground).or(surface_alt),
            swimlane_background_odd: role(ThemeRole::ClusterBackground).or(surface_muted),
            cluster_border: role(ThemeRole::ClusterBorder).or(border),
            note_background: role(ThemeRole::NoteBackground).or(surface_alt),
            note_border: role(ThemeRole::NoteBorder).or(border),
            note_text: role(ThemeRole::NoteText).or(text),
            actor_background: role(ThemeRole::ActorBackground).or(surface_alt),
            actor_border: role(ThemeRole::ActorBorder).or(border),
            actor_text: role(ThemeRole::ActorText).or(text),
            activation_background: role(ThemeRole::ActivationBackground).or(surface_muted),
            activation_border: role(ThemeRole::ActivationBorder).or(border),
            error: role(ThemeRole::Error),
            warning: role(ThemeRole::Warning),
            success: role(ThemeRole::Success),
        }
    }
}

fn put_theme_roles(theme_variables: &mut Map<String, Value>, roles: &ResolvedThemeRoles<'_>) {
    put_opt(theme_variables, "background", roles.canvas);
    put_opt(theme_variables, "primaryColor", roles.surface);
    put_opt(theme_variables, "mainBkg", roles.surface);
    put_opt(theme_variables, "secondaryColor", roles.surface_alt);
    put_opt(theme_variables, "tertiaryColor", roles.surface_muted);
    put_opt(theme_variables, "primaryTextColor", roles.text);
    put_opt(theme_variables, "nodeTextColor", roles.text);
    put_opt(theme_variables, "textColor", roles.text);
    put_opt(theme_variables, "titleColor", roles.text);
    put_opt(theme_variables, "secondaryTextColor", roles.subtle_text);
    put_opt(theme_variables, "tertiaryTextColor", roles.subtle_text);
    put_opt(theme_variables, "primaryBorderColor", roles.border);
    put_opt(theme_variables, "nodeBorder", roles.border);
    put_opt(theme_variables, "lineColor", roles.line);
    put_opt(theme_variables, "arrowheadColor", roles.line);
    put_opt(
        theme_variables,
        "edgeLabelBackground",
        roles.edge_label_background,
    );
    put_opt(theme_variables, "clusterBkg", roles.cluster_background);
    put_opt(theme_variables, "clusterBorder", roles.cluster_border);
    put_opt(theme_variables, "noteBkgColor", roles.note_background);
    put_opt(theme_variables, "noteBorderColor", roles.note_border);
    put_opt(theme_variables, "noteTextColor", roles.note_text);
    put_opt(theme_variables, "actorBkg", roles.actor_background);
    put_opt(theme_variables, "actorBorder", roles.actor_border);
    put_opt(theme_variables, "actorTextColor", roles.actor_text);
    put_opt(theme_variables, "actorLineColor", roles.actor_border);
    put_opt(theme_variables, "signalColor", roles.line.or(roles.text));
    put_opt(theme_variables, "signalTextColor", roles.text);
    put_opt(theme_variables, "labelTextColor", roles.actor_text);
    put_opt(theme_variables, "loopTextColor", roles.actor_text);
    put_opt(theme_variables, "labelBoxBkgColor", roles.actor_background);
    put_opt(theme_variables, "labelBoxBorderColor", roles.actor_border);
    put_opt(
        theme_variables,
        "activationBkgColor",
        roles.activation_background,
    );
    put_opt(
        theme_variables,
        "activationBorderColor",
        roles.activation_border,
    );
    put_opt(theme_variables, "classText", roles.text);
    put_opt(theme_variables, "labelColor", roles.text);
    put_opt(theme_variables, "transitionColor", roles.line);
    put_opt(theme_variables, "transitionLabelColor", roles.text);
    put_opt(theme_variables, "stateLabelColor", roles.text);
    put_opt(theme_variables, "stateBkg", roles.surface);
    put_opt(theme_variables, "stateBorder", roles.border);
    put_opt(theme_variables, "specialStateColor", roles.line);
    put_opt(
        theme_variables,
        "compositeBackground",
        roles.canvas.or(roles.surface),
    );
    put_opt(
        theme_variables,
        "attributeBackgroundColorOdd",
        roles.surface,
    );
    put_opt(
        theme_variables,
        "attributeBackgroundColorEven",
        roles.surface_alt,
    );
    put_opt(theme_variables, "rowOdd", roles.surface);
    put_opt(theme_variables, "rowEven", roles.surface_alt);
    put_opt(theme_variables, "requirementBackground", roles.surface);
    put_opt(theme_variables, "requirementBorderColor", roles.border);
    put_opt(theme_variables, "requirementTextColor", roles.text);
    put_opt(theme_variables, "relationColor", roles.line);
    put_opt(
        theme_variables,
        "relationLabelBackground",
        roles.edge_label_background,
    );
    put_opt(theme_variables, "relationLabelColor", roles.text);
    put_opt(
        theme_variables,
        "requirementEdgeLabelBackground",
        roles.edge_label_background,
    );
    put_opt(theme_variables, "pieTitleTextColor", roles.text);
    put_opt(theme_variables, "pieSectionTextColor", roles.text);
    put_opt(theme_variables, "pieLegendTextColor", roles.subtle_text);
    put_opt(theme_variables, "pieStrokeColor", roles.border);
    put_opt(theme_variables, "pieOuterStrokeColor", roles.border);
    put_opt(theme_variables, "commitLabelColor", roles.text);
    put_opt(
        theme_variables,
        "commitLabelBackground",
        roles.commit_label_background,
    );
    put_opt(theme_variables, "commitLineColor", roles.line);
    put_opt(theme_variables, "tagLabelColor", roles.text);
    put_opt(theme_variables, "tagLabelBackground", roles.surface);
    put_opt(theme_variables, "tagLabelBorder", roles.border);
    put_opt(theme_variables, "quadrant1Fill", roles.surface);
    put_opt(theme_variables, "quadrant2Fill", roles.surface_alt);
    put_opt(
        theme_variables,
        "quadrant3Fill",
        roles.canvas.or(roles.surface),
    );
    put_opt(theme_variables, "quadrant4Fill", roles.surface_muted);
    put_opt(theme_variables, "quadrant1TextFill", roles.text);
    put_opt(theme_variables, "quadrant2TextFill", roles.text);
    put_opt(theme_variables, "quadrant3TextFill", roles.text);
    put_opt(theme_variables, "quadrant4TextFill", roles.text);
    put_opt(theme_variables, "quadrantPointFill", roles.line);
    put_opt(theme_variables, "quadrantPointTextFill", roles.text);
    put_opt(theme_variables, "quadrantTitleFill", roles.text);
    put_opt(theme_variables, "quadrantXAxisTextFill", roles.subtle_text);
    put_opt(theme_variables, "quadrantYAxisTextFill", roles.subtle_text);
    put_opt(
        theme_variables,
        "quadrantExternalBorderStrokeFill",
        roles.border,
    );
    put_opt(
        theme_variables,
        "quadrantInternalBorderStrokeFill",
        roles.border,
    );
    put_opt(theme_variables, "archEdgeColor", roles.line);
    put_opt(theme_variables, "archEdgeArrowColor", roles.line);
    put_opt(
        theme_variables,
        "archGroupBorderColor",
        roles.cluster_border,
    );
    put_opt(theme_variables, "emUiFill", roles.surface);
    put_opt(theme_variables, "emUiStroke", roles.border);
    put_opt(theme_variables, "emRelationStroke", roles.line);
    put_opt(theme_variables, "emArrowhead", roles.line);
    put_opt(
        theme_variables,
        "emSwimlaneBackgroundOdd",
        roles.swimlane_background_odd,
    );
    put_opt(
        theme_variables,
        "emSwimlaneBackgroundStroke",
        roles.cluster_border,
    );
    put_opt(theme_variables, "taskTextDarkColor", roles.text);
    put_opt(theme_variables, "taskTextClickableColor", roles.line);
    put_opt(theme_variables, "taskTextColor", roles.text);
    put_opt(theme_variables, "taskTextOutsideColor", roles.subtle_text);
    put_opt(theme_variables, "taskBkgColor", roles.surface);
    put_opt(theme_variables, "taskBorderColor", roles.border);
    put_opt(theme_variables, "activeTaskBkgColor", roles.surface_muted);
    put_opt(theme_variables, "activeTaskBorderColor", roles.line);
    put_opt(theme_variables, "doneTaskBkgColor", roles.surface_alt);
    put_opt(
        theme_variables,
        "doneTaskBorderColor",
        roles.success.or(roles.border),
    );
    put_opt(theme_variables, "critBkgColor", roles.surface_alt);
    put_opt(
        theme_variables,
        "critBorderColor",
        roles.error.or(roles.border),
    );
    put_opt(theme_variables, "excludeBkgColor", roles.surface_alt);
    put_opt(theme_variables, "gridColor", roles.border);
    put_opt(
        theme_variables,
        "todayLineColor",
        roles.warning.or(roles.error).or(roles.line),
    );
    put_opt(
        theme_variables,
        "vertLineColor",
        roles.warning.or(roles.line),
    );
    put_opt(
        theme_variables,
        "sectionBkgColor",
        roles.cluster_background.or(roles.surface_alt),
    );
    put_opt(theme_variables, "sectionBkgColor2", roles.surface_muted);
    put_opt(theme_variables, "altSectionBkgColor", roles.canvas);
    put_opt(theme_variables, "errorBkgColor", roles.error);
    put_opt(theme_variables, "errorTextColor", roles.text);
    put_opt(theme_variables, "faceColor", roles.surface);
    put_opt(theme_variables, "border2", roles.cluster_border);
}

fn put_series_palette(
    theme_variables: &mut Map<String, Value>,
    palette: &[String],
    canvas: Option<&str>,
    dark: bool,
) {
    if palette.is_empty() {
        return;
    }

    let mut xy = Map::new();
    xy.insert(
        "plotColorPalette".to_string(),
        Value::String(palette.join(",")),
    );
    xy.insert("accentColor".to_string(), Value::String(palette[0].clone()));
    theme_variables.insert("xyChart".to_string(), Value::Object(xy));

    for (index, color) in palette.iter().enumerate() {
        let label = readable_text_color(color, canvas, dark);
        put_str(theme_variables, &format!("cScale{index}"), color);
        put_str(theme_variables, &format!("cScalePeer{index}"), color);
        put_str(theme_variables, &format!("cScaleLabel{index}"), label);
        put_str(theme_variables, &format!("cScaleInv{index}"), label);
        put_str(theme_variables, &format!("git{index}"), color);
        put_str(theme_variables, &format!("gitBranchLabel{index}"), label);
        put_str(theme_variables, &format!("pie{}", index + 1), color);
        put_str(theme_variables, &format!("venn{}", index + 1), color);
        put_str(theme_variables, &format!("fillType{index}"), color);
        put_str(theme_variables, &format!("actor{index}"), color);
    }
}

fn put_diagram_config(
    root: &mut Map<String, Value>,
    theme_variables: &mut Map<String, Value>,
    roles: &ResolvedThemeRoles<'_>,
    palette: &[String],
) {
    let mut packet = Map::new();
    put_opt(&mut packet, "startByteColor", roles.line);
    put_opt(&mut packet, "endByteColor", roles.border.or(roles.line));
    put_opt(&mut packet, "labelColor", roles.text);
    put_opt(&mut packet, "titleColor", roles.text);
    put_opt(&mut packet, "blockStrokeColor", roles.border);
    put_opt(&mut packet, "blockFillColor", roles.surface);
    put_nonempty_object(root, "packet", packet);

    let mut treemap = Map::new();
    put_opt(&mut treemap, "titleColor", roles.text);
    put_opt(&mut treemap, "labelColor", roles.text);
    put_opt(&mut treemap, "valueColor", roles.subtle_text);
    put_opt(&mut treemap, "sectionStrokeColor", roles.border);
    put_opt(&mut treemap, "sectionFillColor", roles.surface_alt);
    put_opt(&mut treemap, "leafStrokeColor", roles.border);
    put_opt(&mut treemap, "leafFillColor", roles.surface);
    put_nonempty_object(root, "treemap", treemap);

    let mut tree_view = Map::new();
    put_opt(&mut tree_view, "labelColor", roles.text);
    put_opt(&mut tree_view, "lineColor", roles.line);
    if !tree_view.is_empty() {
        let mut merged = theme_variables
            .get("treeView")
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default();
        merge_object(&mut merged, &tree_view);
        theme_variables.insert("treeView".to_string(), Value::Object(merged));
    }

    let mut radar = Map::new();
    put_opt(&mut radar, "axisColor", roles.line);
    put_opt(&mut radar, "graticuleColor", roles.border);
    put_nonempty_object(root, "radar", radar);

    let mut eventmodeling = Map::new();
    put_opt(
        &mut eventmodeling,
        "emProcessorFill",
        palette.get(3).map(String::as_str).or(roles.surface_alt),
    );
    put_opt(&mut eventmodeling, "emProcessorStroke", roles.border);
    put_opt(
        &mut eventmodeling,
        "emReadModelFill",
        palette
            .get(1)
            .map(String::as_str)
            .or(roles.success)
            .or(roles.surface_alt),
    );
    put_opt(
        &mut eventmodeling,
        "emReadModelStroke",
        roles.success.or(roles.border),
    );
    put_opt(
        &mut eventmodeling,
        "emCommandFill",
        palette.first().map(String::as_str).or(roles.surface_alt),
    );
    put_opt(
        &mut eventmodeling,
        "emCommandStroke",
        roles.line.or(roles.border),
    );
    put_opt(
        &mut eventmodeling,
        "emEventFill",
        palette
            .get(2)
            .map(String::as_str)
            .or(roles.warning)
            .or(roles.surface_alt),
    );
    put_opt(
        &mut eventmodeling,
        "emEventStroke",
        roles.warning.or(roles.border),
    );
    for (key, value) in eventmodeling {
        theme_variables.insert(key, value);
    }

    let mut c4 = Map::new();
    for prefix in [
        "person",
        "system",
        "system_db",
        "system_queue",
        "container",
        "container_db",
        "container_queue",
        "component",
        "component_db",
        "component_queue",
        "external_person",
        "external_system",
        "external_system_db",
        "external_system_queue",
        "external_container",
        "external_container_db",
        "external_container_queue",
        "external_component",
        "external_component_db",
        "external_component_queue",
    ] {
        put_opt(&mut c4, &format!("{prefix}_bg_color"), roles.surface);
        put_opt(&mut c4, &format!("{prefix}_border_color"), roles.border);
        put_opt(&mut c4, &format!("{prefix}_font_color"), roles.text);
    }
    put_opt(&mut c4, "boundary_bg_color", roles.cluster_background);
    put_opt(&mut c4, "boundary_border_color", roles.cluster_border);
    put_opt(&mut c4, "boundary_font_color", roles.text);
    put_opt(&mut c4, "relationship_text_color", roles.text);
    put_opt(&mut c4, "relationship_line_color", roles.line);
    put_nonempty_object(root, "c4", c4);
}

fn put_nonempty_object(root: &mut Map<String, Value>, key: &str, object: Map<String, Value>) {
    if !object.is_empty() {
        root.insert(key.to_string(), Value::Object(object));
    }
}

fn put_opt(map: &mut Map<String, Value>, key: &str, value: Option<&str>) {
    if let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) {
        put_str(map, key, value);
    }
}

fn put_str(map: &mut Map<String, Value>, key: &str, value: &str) {
    map.insert(key.to_string(), Value::String(value.trim().to_string()));
}

fn merge_object(target: &mut Map<String, Value>, source: &Map<String, Value>) {
    for (key, value) in source {
        match (target.get_mut(key), value) {
            (Some(Value::Object(target)), Value::Object(source)) => merge_object(target, source),
            _ => {
                target.insert(key.clone(), value.clone());
            }
        }
    }
}

fn readable_text_color(color: &str, canvas: Option<&str>, dark: bool) -> &'static str {
    let Ok(color) = ThemeColor::parse(color.trim()) else {
        return "#ffffff";
    };
    let fallback = if dark { [0.0; 3] } else { [1.0; 3] };
    let canvas = canvas
        .and_then(|canvas| ThemeColor::parse(canvas.trim()).ok())
        .map_or(fallback, |canvas| composite_over(&canvas, fallback));
    let [red, green, blue] = composite_over(&color, canvas);
    let luminance = relative_luminance(red, green, blue);
    let black_contrast = (luminance + 0.05) / 0.05;
    let white_contrast = 1.05 / (luminance + 0.05);
    if black_contrast >= white_contrast {
        "#000000"
    } else {
        "#ffffff"
    }
}

fn composite_over(color: &ThemeColor, background: [f64; 3]) -> [f64; 3] {
    let alpha = color.channel(ColorChannel::Alpha);
    let foreground = [
        color.channel(ColorChannel::Red) / 255.0,
        color.channel(ColorChannel::Green) / 255.0,
        color.channel(ColorChannel::Blue) / 255.0,
    ];
    std::array::from_fn(|index| foreground[index] * alpha + background[index] * (1.0 - alpha))
}

fn relative_luminance(r: f64, g: f64, b: f64) -> f64 {
    fn linear(channel: f64) -> f64 {
        if channel <= 0.04045 {
            channel / 12.92
        } else {
            ((channel + 0.055) / 1.055).powf(2.4)
        }
    }
    0.2126 * linear(r) + 0.7152 * linear(g) + 0.0722 * linear(b)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn palette_readability_accepts_the_shared_color_surface() {
        assert_eq!(readable_text_color("white", None, false), "#000000");
        assert_eq!(readable_text_color("#777777", None, false), "#000000");
        assert_eq!(readable_text_color("rebeccapurple", None, false), "#ffffff");
        assert_eq!(
            readable_text_color("var(--host-color)", None, false),
            "#ffffff"
        );
    }

    #[test]
    fn palette_readability_composites_transparency_against_the_canvas() {
        assert_eq!(
            readable_text_color("rgb(255 255 255 / .2)", Some("#000000"), true),
            "#ffffff"
        );
        assert_eq!(
            readable_text_color("rgb(0 0 0 / .2)", Some("#ffffff"), false),
            "#000000"
        );
    }

    #[test]
    fn sequence_and_gantt_variables_follow_their_semantic_owners() {
        let theme = HostTheme::new()
            .try_with_role(ThemeRole::SurfaceAlt, "#202122")
            .unwrap()
            .try_with_role(ThemeRole::Text, "#303132")
            .unwrap()
            .try_with_role(ThemeRole::Border, "#404142")
            .unwrap()
            .try_with_role(ThemeRole::ActorBackground, "#505152")
            .unwrap()
            .try_with_role(ThemeRole::ActorBorder, "#606162")
            .unwrap()
            .try_with_role(ThemeRole::ActorText, "#707172")
            .unwrap()
            .try_with_role(ThemeRole::Error, "#803030")
            .unwrap()
            .try_with_role(ThemeRole::Success, "#308030")
            .unwrap();
        let config = compile(&theme);
        let variables = &config.as_value()["themeVariables"];

        assert_eq!(variables["actorLineColor"], "#606162");
        assert_eq!(variables["labelBoxBkgColor"], "#505152");
        assert_eq!(variables["labelBoxBorderColor"], "#606162");
        assert_eq!(variables["labelTextColor"], "#707172");
        assert_eq!(variables["loopTextColor"], "#707172");
        assert_eq!(variables["doneTaskBkgColor"], "#202122");
        assert_eq!(variables["doneTaskBorderColor"], "#308030");
        assert_eq!(variables["critBkgColor"], "#202122");
        assert_eq!(variables["critBorderColor"], "#803030");
    }
}
