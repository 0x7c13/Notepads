//! Ordered evaluation of Mermaid 12.0.0's `theme-base.js`.
//!
//! Constructor inputs are deliberately separate from calculated release snapshots. Starting
//! from a calculated palette would make `a || b` retain stale defaults instead of deriving `b`
//! from the user's inputs. Explicit overrides are replayed by `ThemeResolution` afterward.

use super::*;

fn inherit(tv: &mut Map<String, Value>, key: &str, source: &str) {
    if value_is_missing(tv, key)
        && let Some(value) = tv.get(source).cloned()
    {
        tv.insert(key.to_string(), value);
    }
}

fn inherit_many(tv: &mut Map<String, Value>, pairs: &[(&str, &str)]) {
    for (key, source) in pairs {
        inherit(tv, key, source);
    }
}

fn transform(
    tv: &mut Map<String, Value>,
    key: &str,
    source: &str,
    operation: impl FnOnce(&str) -> Result<String, ColorError>,
) -> Result<(), ColorError> {
    if value_is_missing(tv, key) {
        let value = operation(&required_color(tv, source)?)?;
        tv.insert(key.to_string(), Value::String(value));
    }
    Ok(())
}

fn adjust(
    tv: &mut Map<String, Value>,
    key: &str,
    source: &str,
    hue: f64,
    saturation: f64,
    lightness: f64,
) -> Result<(), ColorError> {
    transform(tv, key, source, |color| {
        theme_color::adjust(color, ColorAdjustment::hsl(hue, saturation, lightness))
    })
}

fn nested_defaults(
    tv: &mut Map<String, Value>,
    key: &str,
    mut defaults: Map<String, Value>,
    references: &[(&str, &str)],
) {
    for (target, source) in references {
        if let Some(value) = tv.get(*source) {
            defaults.insert((*target).to_string(), value.clone());
        }
    }
    if let Some(explicit) = tv.get(key).and_then(Value::as_object) {
        for (target, value) in &mut defaults {
            if let Some(provided) = explicit.get(target).filter(|value| is_js_truthy(value)) {
                *value = provided.clone();
            }
        }
    }
    tv.insert(key.to_string(), Value::Object(defaults));
}

fn object(value: Value) -> Map<String, Value> {
    match value {
        Value::Object(map) => map,
        _ => unreachable!("theme constructor literals are objects"),
    }
}

pub(super) fn calculate(explicit: &Map<String, Value>) -> Result<Map<String, Value>, ColorError> {
    let mut tv = object(serde_json::json!({
        "background": "#f4f4f4",
        "primaryColor": "#fff4dd",
        "noteBkgColor": "#fff5ad",
        "noteTextColor": "#333",
        "THEME_COLOR_LIMIT": 12,
        "radius": 5,
        "strokeWidth": 1,
        "fontFamily": "\"trebuchet ms\", verdana, arial, sans-serif",
        "fontSize": "16px",
        "useGradient": true,
        "dropShadow": "drop-shadow( 1px 2px 2px rgba(185,185,185,1))"
    }));
    tv.extend(explicit.clone());
    let dark_mode = tv.get("darkMode").is_some_and(is_js_truthy);

    set_string_if_missing(
        &mut tv,
        "primaryTextColor",
        if dark_mode { "#eee" } else { "#333" },
    );
    adjust(&mut tv, "secondaryColor", "primaryColor", -120.0, 0.0, 0.0)?;
    adjust(&mut tv, "tertiaryColor", "primaryColor", 180.0, 0.0, 5.0)?;
    for (key, source) in [
        ("primaryBorderColor", "primaryColor"),
        ("secondaryBorderColor", "secondaryColor"),
        ("tertiaryBorderColor", "tertiaryColor"),
        ("noteBorderColor", "noteBkgColor"),
    ] {
        transform(&mut tv, key, source, |color| mk_border(color, dark_mode))?;
    }
    set_string_if_missing(&mut tv, "noteBkgColor", "#fff5ad");
    set_string_if_missing(&mut tv, "noteTextColor", "#333");
    for (key, source) in [
        ("secondaryTextColor", "secondaryColor"),
        ("tertiaryTextColor", "tertiaryColor"),
        ("lineColor", "background"),
        ("arrowheadColor", "background"),
    ] {
        transform(&mut tv, key, source, theme_color::invert)?;
    }
    inherit_many(
        &mut tv,
        &[
            ("textColor", "primaryTextColor"),
            ("border2", "tertiaryBorderColor"),
            ("nodeBkg", "primaryColor"),
            ("mainBkg", "primaryColor"),
            ("nodeBorder", "primaryBorderColor"),
            ("clusterBkg", "tertiaryColor"),
            ("clusterBorder", "tertiaryBorderColor"),
            ("defaultLinkColor", "lineColor"),
            ("titleColor", "tertiaryTextColor"),
        ],
    );
    if dark_mode {
        transform(&mut tv, "edgeLabelBackground", "secondaryColor", |color| {
            theme_color::darken(color, 30.0)
        })?;
    } else {
        inherit(&mut tv, "edgeLabelBackground", "secondaryColor");
    }
    inherit_many(
        &mut tv,
        &[
            ("flowContainerStroke", "secondaryBorderColor"),
            ("nodeTextColor", "primaryTextColor"),
            ("actorBorder", "primaryBorderColor"),
            ("actorBkg", "mainBkg"),
            ("actorTextColor", "primaryTextColor"),
            ("actorLineColor", "actorBorder"),
            ("labelBoxBkgColor", "actorBkg"),
            ("signalColor", "textColor"),
            ("signalTextColor", "textColor"),
            ("labelBoxBorderColor", "actorBorder"),
            ("labelTextColor", "actorTextColor"),
            ("loopTextColor", "actorTextColor"),
        ],
    );
    transform(
        &mut tv,
        "activationBorderColor",
        "secondaryColor",
        |color| theme_color::darken(color, 10.0),
    )?;
    inherit(&mut tv, "activationBkgColor", "secondaryColor");
    transform(
        &mut tv,
        "sequenceNumberColor",
        "lineColor",
        theme_color::invert,
    )?;
    inherit_many(
        &mut tv,
        &[
            ("rectBkgColor", "tertiaryColor"),
            ("sectionBkgColor", "tertiaryColor"),
            ("sectionBkgColor", "secondaryColor"),
            ("sectionBkgColor2", "primaryColor"),
            ("taskBorderColor", "primaryBorderColor"),
            ("taskBkgColor", "primaryColor"),
            ("activeTaskBorderColor", "primaryColor"),
        ],
    );
    transform(&mut tv, "activeTaskBkgColor", "primaryColor", |color| {
        theme_color::lighten(color, 23.0)
    })?;
    for (key, value) in [
        ("altSectionBkgColor", "white"),
        ("excludeBkgColor", "#eeeeee"),
        ("gridColor", "lightgrey"),
        ("doneTaskBkgColor", "lightgrey"),
        ("doneTaskBorderColor", "grey"),
        ("critBorderColor", "#ff8888"),
        ("critBkgColor", "red"),
        ("todayLineColor", "red"),
        ("vertLineColor", "navy"),
        ("taskTextClickableColor", "#003163"),
        ("noteFontWeight", "normal"),
        ("fontWeight", "normal"),
    ] {
        set_string_if_missing(&mut tv, key, value);
    }
    inherit_many(
        &mut tv,
        &[
            ("taskTextColor", "textColor"),
            ("taskTextOutsideColor", "textColor"),
            ("taskTextLightColor", "textColor"),
            ("taskTextColor", "primaryTextColor"),
            ("taskTextDarkColor", "textColor"),
            ("personBorder", "primaryBorderColor"),
            ("personBkg", "mainBkg"),
        ],
    );
    for (key, amount) in [
        ("rowOdd", if dark_mode { -5.0 } else { 75.0 }),
        ("rowEven", if dark_mode { -10.0 } else { 5.0 }),
    ] {
        adjust(&mut tv, key, "mainBkg", 0.0, 0.0, amount)?;
    }
    inherit_many(
        &mut tv,
        &[
            ("transitionColor", "lineColor"),
            ("transitionLabelColor", "textColor"),
            // This precedes stateBkg's default assignment in upstream updateColors().
            ("stateLabelColor", "stateBkg"),
            ("stateLabelColor", "primaryTextColor"),
            ("stateBkg", "mainBkg"),
            ("labelBackgroundColor", "stateBkg"),
            ("compositeBackground", "background"),
            ("compositeBackground", "tertiaryColor"),
            ("altBackground", "tertiaryColor"),
            ("compositeTitleBackground", "mainBkg"),
            ("compositeBorder", "nodeBorder"),
            ("errorBkgColor", "tertiaryColor"),
            ("errorTextColor", "tertiaryTextColor"),
        ],
    );
    tv.insert("innerEndBackground".to_string(), tv["nodeBorder"].clone());
    tv.insert("specialStateColor".to_string(), tv["lineColor"].clone());

    inherit_many(
        &mut tv,
        &[
            ("cScale0", "primaryColor"),
            ("cScale1", "secondaryColor"),
            ("cScale2", "tertiaryColor"),
        ],
    );
    for (index, hue) in [
        (3, 30.0),
        (4, 60.0),
        (5, 90.0),
        (6, 120.0),
        (7, 150.0),
        (8, 210.0),
        (9, 270.0),
        (10, 300.0),
        (11, 330.0),
    ] {
        adjust(
            &mut tv,
            &format!("cScale{index}"),
            "primaryColor",
            hue,
            0.0,
            if index == 8 { 150.0 } else { 0.0 },
        )?;
    }
    // The loop condition coerces this constructor variable to a JavaScript number. Missing
    // scale colors fail at the first attempted transform, including limits above twelve.
    let scale_limit = match &tv["THEME_COLOR_LIMIT"] {
        Value::Number(value) => value.as_f64().unwrap_or(f64::NAN),
        Value::String(value) => {
            if value.trim().is_empty() {
                0.0
            } else {
                value.trim().parse().unwrap_or(f64::NAN)
            }
        }
        Value::Bool(true) => 1.0,
        _ => 0.0,
    };
    let mut scale_count = 0;
    while (scale_count as f64) < scale_limit {
        let key = format!("cScale{scale_count}");
        let color = theme_color::darken(
            &required_color(&tv, &key)?,
            if dark_mode { 75.0 } else { 25.0 },
        )?;
        tv.insert(key, Value::String(color));
        scale_count += 1;
    }
    for index in 0..scale_count {
        let source = format!("cScale{index}");
        transform(
            &mut tv,
            &format!("cScaleInv{index}"),
            &source,
            theme_color::invert,
        )?;
        transform(&mut tv, &format!("cScalePeer{index}"), &source, |color| {
            if dark_mode {
                theme_color::lighten(color, 10.0)
            } else {
                theme_color::darken(color, 10.0)
            }
        })?;
    }
    inherit(&mut tv, "scaleLabelColor", "labelTextColor");
    for index in 0..scale_count {
        inherit(&mut tv, &format!("cScaleLabel{index}"), "scaleLabelColor");
    }
    let multiplier = if dark_mode { -4.0 } else { -1.0 };
    for index in 0..5 {
        adjust(
            &mut tv,
            &format!("surface{index}"),
            "mainBkg",
            180.0,
            -15.0,
            multiplier * (5 + index * 3) as f64,
        )?;
        adjust(
            &mut tv,
            &format!("surfacePeer{index}"),
            "mainBkg",
            180.0,
            -15.0,
            multiplier * (8 + index * 3) as f64,
        )?;
    }
    inherit_many(
        &mut tv,
        &[
            ("classText", "textColor"),
            ("fillType0", "primaryColor"),
            ("fillType1", "secondaryColor"),
        ],
    );
    for (index, source, hue) in [
        (2, "primaryColor", 64.0),
        (3, "secondaryColor", 64.0),
        (4, "primaryColor", -64.0),
        (5, "secondaryColor", -64.0),
        (6, "primaryColor", 128.0),
        (7, "secondaryColor", 128.0),
    ] {
        adjust(&mut tv, &format!("fillType{index}"), source, hue, 0.0, 0.0)?;
    }
    inherit_many(
        &mut tv,
        &[
            ("pie1", "primaryColor"),
            ("pie2", "secondaryColor"),
            ("pie3", "tertiaryColor"),
        ],
    );
    for (index, source, hue, lightness) in [
        (4, "primaryColor", 0.0, -10.0),
        (5, "secondaryColor", 0.0, -10.0),
        (6, "tertiaryColor", 0.0, -10.0),
        (7, "primaryColor", 60.0, -10.0),
        (8, "primaryColor", -60.0, -10.0),
        (9, "primaryColor", 120.0, 0.0),
        (10, "primaryColor", 60.0, -20.0),
        (11, "primaryColor", -60.0, -20.0),
        (12, "primaryColor", 120.0, -10.0),
    ] {
        adjust(&mut tv, &format!("pie{index}"), source, hue, 0.0, lightness)?;
    }
    inherit_many(
        &mut tv,
        &[
            ("pieTitleTextColor", "taskTextDarkColor"),
            ("pieSectionTextColor", "textColor"),
            ("pieLegendTextColor", "taskTextDarkColor"),
        ],
    );
    for (key, value) in [
        ("pieTitleTextSize", "25px"),
        ("pieSectionTextSize", "17px"),
        ("pieLegendTextSize", "17px"),
        ("pieStrokeColor", "black"),
        ("pieStrokeWidth", "2px"),
        ("pieOuterStrokeWidth", "2px"),
        ("pieOuterStrokeColor", "black"),
        ("pieOpacity", "0.7"),
    ] {
        set_string_if_missing(&mut tv, key, value);
    }
    // Venn uses nullish coalescing, unlike the truthiness fallbacks above.
    for (index, source, hue) in [
        (1, "primaryColor", 0.0),
        (2, "secondaryColor", 0.0),
        (3, "tertiaryColor", 0.0),
        (4, "primaryColor", 60.0),
        (5, "primaryColor", -60.0),
        (6, "secondaryColor", 60.0),
        (7, "primaryColor", 120.0),
        (8, "secondaryColor", 120.0),
    ] {
        let key = format!("venn{index}");
        if tv.get(&key).is_none_or(Value::is_null) {
            adjust(&mut tv, &key, source, hue, 0.0, -30.0)?;
        }
    }
    for (key, source) in [
        ("vennTitleTextColor", "titleColor"),
        ("vennSetTextColor", "textColor"),
    ] {
        if tv.get(key).is_none_or(Value::is_null) {
            inherit(&mut tv, key, source);
        }
    }

    nested_defaults(
        &mut tv,
        "cynefin",
        object(serde_json::json!({
            "domainFontSize": 16, "itemFontSize": 12, "boundaryWidth": 2,
            "cliffColor": "#8B0000", "cliffWidth": 4, "arrowWidth": 2,
            "complexBg": "#E8F5E9", "complicatedBg": "#E3F2FD", "chaoticBg": "#FBE9E7",
            "clearBg": "#FFF8E1", "confusionBg": "#F3E5F5"
        })),
        &[
            ("boundaryColor", "lineColor"),
            ("arrowColor", "lineColor"),
            ("textColor", "textColor"),
            ("labelColor", "primaryTextColor"),
        ],
    );
    nested_defaults(
        &mut tv,
        "radar",
        object(serde_json::json!({
            "axisStrokeWidth": 2, "axisLabelFontSize": 12, "curveOpacity": 0.5,
            "curveStrokeWidth": 2, "graticuleColor": "#DEDEDE", "graticuleStrokeWidth": 1,
            "graticuleOpacity": 0.3, "legendBoxSize": 12, "legendFontSize": 12
        })),
        &[("axisColor", "lineColor")],
    );
    set_string_if_missing(&mut tv, "wardleyEvolutionColor", "#dc3545");
    nested_defaults(
        &mut tv,
        "wardley",
        Map::new(),
        &[
            ("backgroundColor", "background"),
            ("axisColor", "lineColor"),
            ("axisTextColor", "primaryTextColor"),
            ("gridColor", "gridColor"),
            ("componentFill", "background"),
            ("componentStroke", "lineColor"),
            ("componentLabelColor", "primaryTextColor"),
            ("linkStroke", "lineColor"),
            ("evolutionStroke", "wardleyEvolutionColor"),
            ("annotationStroke", "lineColor"),
            ("annotationTextColor", "primaryTextColor"),
            ("annotationFill", "background"),
        ],
    );
    for (key, value) in [
        ("archEdgeColor", "#777"),
        ("archEdgeArrowColor", "#777"),
        ("archEdgeWidth", "3"),
        ("archGroupBorderColor", "#000"),
        ("archGroupBorderWidth", "2px"),
    ] {
        set_string_if_missing(&mut tv, key, value);
    }
    inherit(&mut tv, "quadrant1Fill", "primaryColor");
    for (index, delta) in [(2, 5.0), (3, 10.0), (4, 15.0)] {
        transform(
            &mut tv,
            &format!("quadrant{index}Fill"),
            "primaryColor",
            |color| theme_color::adjust(color, ColorAdjustment::rgb(delta, delta, delta)),
        )?;
    }
    inherit(&mut tv, "quadrant1TextFill", "primaryTextColor");
    for (index, delta) in [(2, -5.0), (3, -10.0), (4, -15.0)] {
        transform(
            &mut tv,
            &format!("quadrant{index}TextFill"),
            "primaryTextColor",
            |color| theme_color::adjust(color, ColorAdjustment::rgb(delta, delta, delta)),
        )?;
    }
    // Preserve the upstream conditional's precedence and its omitted amount (NaN).
    let quadrant = required_color(&tv, "quadrant1Fill")?;
    let point = if tv.get("quadrantPointFill").is_some_and(is_js_truthy)
        || theme_color::is_dark(&quadrant)?
    {
        theme_color::lighten(&quadrant, f64::NAN)?
    } else {
        theme_color::darken(&quadrant, f64::NAN)?
    };
    tv.insert("quadrantPointFill".to_string(), Value::String(point));
    inherit_many(
        &mut tv,
        &[
            ("quadrantPointTextFill", "primaryTextColor"),
            ("quadrantXAxisTextFill", "primaryTextColor"),
            ("quadrantYAxisTextFill", "primaryTextColor"),
            ("quadrantTitleFill", "primaryTextColor"),
            ("quadrantInternalBorderStrokeFill", "primaryBorderColor"),
            ("quadrantExternalBorderStrokeFill", "primaryBorderColor"),
        ],
    );
    nested_defaults(
        &mut tv,
        "xyChart",
        object(serde_json::json!({
            "plotColorPalette": "#FFF4DD,#FFD8B1,#FFA07A,#ECEFF1,#D6DBDF,#C3E0A8,#FFB6A4,#FFD74D,#738FA7,#FFFFF0"
        })),
        &[
            ("backgroundColor", "background"),
            ("titleColor", "primaryTextColor"),
            ("dataLabelColor", "primaryTextColor"),
            ("legendTextColor", "primaryTextColor"),
            ("xAxisTitleColor", "primaryTextColor"),
            ("xAxisLabelColor", "primaryTextColor"),
            ("xAxisTickColor", "primaryTextColor"),
            ("xAxisLineColor", "primaryTextColor"),
            ("yAxisTitleColor", "primaryTextColor"),
            ("yAxisLabelColor", "primaryTextColor"),
            ("yAxisTickColor", "primaryTextColor"),
            ("yAxisLineColor", "primaryTextColor"),
        ],
    );
    inherit_many(
        &mut tv,
        &[
            ("requirementBackground", "primaryColor"),
            ("requirementBorderColor", "primaryBorderColor"),
            ("requirementTextColor", "primaryTextColor"),
            ("relationColor", "lineColor"),
        ],
    );
    set_string_if_missing(&mut tv, "requirementBorderSize", "1");
    if dark_mode {
        transform(
            &mut tv,
            "relationLabelBackground",
            "secondaryColor",
            |color| theme_color::darken(color, 30.0),
        )?;
    } else {
        inherit(&mut tv, "relationLabelBackground", "secondaryColor");
    }
    inherit(&mut tv, "relationLabelColor", "actorTextColor");
    inherit_many(
        &mut tv,
        &[
            ("git0", "primaryColor"),
            ("git1", "secondaryColor"),
            ("git2", "tertiaryColor"),
        ],
    );
    for (index, hue) in [(3, -30.0), (4, -60.0), (5, -90.0), (6, 60.0), (7, 120.0)] {
        adjust(
            &mut tv,
            &format!("git{index}"),
            "primaryColor",
            hue,
            0.0,
            0.0,
        )?;
    }
    for index in 0..8 {
        let key = format!("git{index}");
        let color = required_color(&tv, &key)?;
        let color = if dark_mode {
            theme_color::lighten(&color, 25.0)?
        } else {
            theme_color::darken(&color, 25.0)?
        };
        tv.insert(key, Value::String(color));
    }
    for index in 0..8 {
        transform(
            &mut tv,
            &format!("gitInv{index}"),
            &format!("git{index}"),
            theme_color::invert,
        )?;
    }
    if dark_mode {
        set_string_if_missing(&mut tv, "branchLabelColor", "black");
    } else {
        inherit(&mut tv, "branchLabelColor", "labelTextColor");
    }
    for index in 0..8 {
        inherit(
            &mut tv,
            &format!("gitBranchLabel{index}"),
            "branchLabelColor",
        );
    }
    inherit_many(
        &mut tv,
        &[
            ("tagLabelColor", "primaryTextColor"),
            ("tagLabelBackground", "primaryColor"),
        ],
    );
    let tag_border = tv
        .get("tagBorder")
        .filter(|value| is_js_truthy(value))
        .unwrap_or(&tv["primaryBorderColor"])
        .clone();
    tv.insert("tagLabelBorder".to_string(), tag_border);
    inherit_many(
        &mut tv,
        &[
            ("commitLabelColor", "secondaryTextColor"),
            ("commitLabelBackground", "secondaryColor"),
        ],
    );
    for (key, value) in [
        ("tagLabelFontSize", "10px"),
        ("commitLabelFontSize", "10px"),
        ("emUiFill", "white"),
        ("emUiStroke", "#dbdada"),
        ("emProcessorFill", "#edb3f6"),
        ("emProcessorStroke", "#b88cbf"),
        ("emReadModelFill", "#d3f1a2"),
        ("emReadModelStroke", "#a3b732"),
        ("emCommandFill", "#bcd6fe"),
        ("emCommandStroke", "#679ac3"),
        ("emEventFill", "#ffb778"),
        ("emEventStroke", "#c19a0f"),
        ("emSwimlaneBackgroundOdd", "rgb(250,250,250)"),
        ("emSwimlaneBackgroundStroke", "rgb(240,240,240)"),
        ("attributeBackgroundColorOdd", "#ffffff"),
        ("attributeBackgroundColorEven", "#f2f2f2"),
    ] {
        set_string_if_missing(&mut tv, key, value);
    }
    inherit_many(
        &mut tv,
        &[
            ("emArrowhead", "lineColor"),
            ("emRelationStroke", "lineColor"),
        ],
    );
    tv.insert(
        "gradientStart".to_string(),
        tv["primaryBorderColor"].clone(),
    );
    tv.insert(
        "gradientStop".to_string(),
        tv["secondaryBorderColor"].clone(),
    );
    Ok(tv)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn resolve(overrides: Value) -> Map<String, Value> {
        let mut config = MermaidConfig::from_value(json!({
            "theme": "base",
            "themeVariables": overrides
        }));
        apply_theme_defaults(&mut config).unwrap();
        theme_variables_map(&config)
    }

    #[test]
    fn base_overrides_reach_every_family_before_explicit_replay() {
        let tv = resolve(json!({
            "primaryColor": "#181818",
            "secondaryColor": "#224466",
            "tertiaryColor": "#aabbcc",
            "primaryTextColor": "#ddeeff",
            "lineColor": "#a0a0a0",
            "textColor": "#112233"
        }));
        for key in [
            "mainBkg",
            "nodeBkg",
            "actorBkg",
            "labelBoxBkgColor",
            "personBkg",
            "stateBkg",
            "labelBackgroundColor",
            "compositeTitleBackground",
            "taskBkgColor",
            "activeTaskBorderColor",
            "sectionBkgColor2",
            "pie1",
            "fillType0",
            "requirementBackground",
            "tagLabelBackground",
        ] {
            assert_eq!(tv[key], "#181818", "{key}");
        }
        for key in [
            "transitionColor",
            "specialStateColor",
            "defaultLinkColor",
            "relationColor",
            "emArrowhead",
            "emRelationStroke",
        ] {
            assert_eq!(tv[key], "#a0a0a0", "{key}");
        }
        for key in [
            "signalColor",
            "signalTextColor",
            "transitionLabelColor",
            "taskTextColor",
            "taskTextOutsideColor",
            "taskTextDarkColor",
            "classText",
            "pieTitleTextColor",
            "pieSectionTextColor",
            "pieLegendTextColor",
            "vennSetTextColor",
        ] {
            assert_eq!(tv[key], "#112233", "{key}");
        }
        for key in [
            "clusterBkg",
            "rectBkgColor",
            "sectionBkgColor",
            "altBackground",
            "pie3",
        ] {
            assert_eq!(tv[key], "#aabbcc", "{key}");
        }
        assert_eq!(tv["activationBkgColor"], "#224466");
        assert_eq!(tv["stateLabelColor"], "#ddeeff");
        assert_eq!(tv["arrowheadColor"], "#0b0b0b");
        assert_eq!(tv["sequenceNumberColor"], "#5f5f5f");
        assert_eq!(tv["radar"]["axisColor"], "#a0a0a0");
        assert_eq!(tv["cynefin"]["boundaryColor"], "#a0a0a0");
        assert_eq!(tv["wardley"]["linkStroke"], "#a0a0a0");
        assert_eq!(tv["xyChart"]["legendTextColor"], "#ddeeff");
    }

    #[test]
    fn base_intermediate_overrides_feed_dependents_in_upstream_order() {
        let tv = resolve(json!({
            "primaryColor": "#181818", "mainBkg": "#242424",
            "actorBkg": "#345678", "actorBorder": "#456789", "actorTextColor": "#abcdef",
            "stateBkg": "#567890", "taskTextDarkColor": "#678901", "tagBorder": "#789012"
        }));
        assert_eq!(tv["personBkg"], "#242424");
        assert_eq!(tv["compositeTitleBackground"], "#242424");
        assert_eq!(tv["labelBoxBkgColor"], "#345678");
        assert_eq!(tv["actorLineColor"], "#456789");
        assert_eq!(tv["labelBoxBorderColor"], "#456789");
        assert_eq!(tv["labelTextColor"], "#abcdef");
        assert_eq!(tv["loopTextColor"], "#abcdef");
        assert_eq!(tv["scaleLabelColor"], "#abcdef");
        assert_eq!(tv["cScaleLabel11"], "#abcdef");
        assert_eq!(tv["gitBranchLabel7"], "#abcdef");
        assert_eq!(tv["relationLabelColor"], "#abcdef");
        assert_eq!(tv["stateLabelColor"], "#567890");
        assert_eq!(tv["labelBackgroundColor"], "#567890");
        assert_eq!(tv["pieTitleTextColor"], "#678901");
        assert_eq!(tv["pieLegendTextColor"], "#678901");
        assert_eq!(tv["tagLabelBorder"], "#789012");
    }

    #[test]
    fn base_explicit_values_win_after_calculation_including_falsy_values() {
        let tv = resolve(json!({
            "primaryColor": "#181818", "actorBkg": "", "stateBkg": false,
            "taskBkgColor": 0, "pie1": "#123456", "cScale0": "#ffffff",
            "git0": "#ffffff", "gradientStart": "#654321", "useGradient": false,
            "radar": { "axisColor": null }
        }));
        assert_eq!(tv["actorBkg"], "");
        assert_eq!(tv["stateBkg"], false);
        assert_eq!(tv["taskBkgColor"], 0);
        assert_eq!(tv["pie1"], "#123456");
        assert_eq!(tv["labelBoxBkgColor"], "#181818");
        assert_eq!(tv["labelBackgroundColor"], "#181818");
        assert_eq!(tv["stateLabelColor"], "#333");
        assert_eq!(tv["cScale0"], "#ffffff");
        assert_eq!(tv["cScaleInv0"], "rgb(63.75, 63.75, 63.75)");
        assert_eq!(tv["git0"], "#ffffff");
        assert_eq!(tv["gitInv0"], "rgb(63.75, 63.75, 63.75)");
        assert_eq!(tv["gradientStart"], "#654321");
        assert_eq!(tv["useGradient"], false);
        assert_eq!(tv["radar"]["axisColor"], Value::Null);
        assert_eq!(tv["radar"]["graticuleColor"], "#DEDEDE");
        assert_eq!(tv["radar"]["axisStrokeWidth"], 2);
    }

    #[test]
    fn base_validates_colors_only_when_a_transform_uses_them() {
        let tv = resolve(json!({ "secondaryColor": "", "cScale0": "", "git0": "" }));
        assert_eq!(tv["secondaryColor"], "");
        assert_eq!(tv["cScale0"], "");
        assert_eq!(tv["git0"], "");
        assert_eq!(
            tv["activationBkgColor"],
            "hsl(-79.4117647059, 100%, 93.3333333333%)"
        );
        for key in [
            "mainBkg",
            "noteBkgColor",
            "lineColor",
            "primaryTextColor",
            "git7",
            "cScale11",
        ] {
            let mut config = MermaidConfig::from_value(json!({
                "theme": "base", "themeVariables": { (key): "not-a-color" }
            }));
            assert!(apply_theme_defaults(&mut config).is_err(), "{key}");
        }
    }
}
