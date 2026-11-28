use super::super::*;

pub(super) fn palette_size(config: &serde_json::Value) -> usize {
    if !matches!(
        config.get("theme").and_then(serde_json::Value::as_str),
        Some("redux-color" | "redux-dark-color")
    ) {
        return 0;
    }
    config
        .pointer("/themeVariables/borderColorArray")
        .and_then(serde_json::Value::as_array)
        .map_or(0, Vec::len)
}

pub(super) fn write_palette_css(
    out: &mut String,
    id: SvgDiagramId<'_>,
    config: &serde_json::Value,
    options: &SvgExecution<'_>,
) -> Result<()> {
    if palette_size(config) == 0 {
        return Ok(());
    }
    let borders = config
        .pointer("/themeVariables/borderColorArray")
        .and_then(serde_json::Value::as_array)
        .expect("palette checked above");
    let backgrounds = config
        .pointer("/themeVariables/bkgColorArray")
        .and_then(serde_json::Value::as_array)
        .filter(|colors| !colors.is_empty());
    // colorThemeGate.safeLook permits only bare selector words, including numeric look values.
    let look = config
        .get("look")
        .and_then(|look| match look {
            serde_json::Value::String(value) => Some(value.clone()),
            serde_json::Value::Number(value) => Some(value.to_string()),
            _ => None,
        })
        .filter(|look| {
            !look.is_empty()
                && look
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
        })
        .unwrap_or_else(|| "classic".into());
    let css_color = |value: &serde_json::Value| {
        value
            .as_str()
            .map(|color| color.trim().to_owned())
            .unwrap_or_else(|| value.to_string())
    };
    for (index, border) in borders.iter().enumerate() {
        options.checkpoint_emit()?;
        let _ = write!(
            out,
            r#"#{id} [data-look="{look}"][data-color-id="color-{index}"].node rect.composite{{stroke:{};"#,
            css_color(border)
        );
        if let Some(backgrounds) = backgrounds {
            let _ = write!(
                out,
                "fill:{};",
                css_color(&backgrounds[index % backgrounds.len()])
            );
        }
        out.push('}');
    }
    Ok(())
}
