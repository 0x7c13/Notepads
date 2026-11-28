use merman_core::theme_color::{ColorChannel, ThemeColor};

pub(super) fn default_c_scale(index: usize) -> &'static str {
    match index {
        0 => "hsl(240, 100%, 76.2745098039%)",
        1 => "hsl(60, 100%, 73.5294117647%)",
        2 => "hsl(80, 100%, 76.2745098039%)",
        3 => "hsl(270, 100%, 76.2745098039%)",
        4 => "hsl(300, 100%, 76.2745098039%)",
        5 => "hsl(330, 100%, 76.2745098039%)",
        6 => "hsl(0, 100%, 76.2745098039%)",
        7 => "hsl(30, 100%, 76.2745098039%)",
        8 => "hsl(90, 100%, 76.2745098039%)",
        9 => "hsl(150, 100%, 76.2745098039%)",
        10 => "hsl(180, 100%, 76.2745098039%)",
        _ => "hsl(210, 100%, 76.2745098039%)",
    }
}

pub(super) fn default_c_scale_peer(index: usize) -> &'static str {
    match index {
        0 => "hsl(240, 100%, 61.2745098039%)",
        1 => "hsl(60, 100%, 48.5294117647%)",
        2 => "hsl(80, 100%, 56.2745098039%)",
        3 => "hsl(270, 100%, 61.2745098039%)",
        4 => "hsl(300, 100%, 61.2745098039%)",
        5 => "hsl(330, 100%, 61.2745098039%)",
        6 => "hsl(0, 100%, 61.2745098039%)",
        7 => "hsl(30, 100%, 61.2745098039%)",
        8 => "hsl(90, 100%, 61.2745098039%)",
        9 => "hsl(150, 100%, 61.2745098039%)",
        10 => "hsl(180, 100%, 61.2745098039%)",
        _ => "hsl(210, 100%, 61.2745098039%)",
    }
}

pub(super) fn default_c_scale_inv(index: usize) -> &'static str {
    match index {
        0 => "hsl(60, 100%, 86.2745098039%)",
        1 => "hsl(240, 100%, 83.5294117647%)",
        2 => "hsl(260, 100%, 86.2745098039%)",
        3 => "hsl(90, 100%, 86.2745098039%)",
        4 => "hsl(120, 100%, 86.2745098039%)",
        5 => "hsl(150, 100%, 86.2745098039%)",
        6 => "hsl(180, 100%, 86.2745098039%)",
        7 => "hsl(210, 100%, 86.2745098039%)",
        8 => "hsl(270, 100%, 86.2745098039%)",
        9 => "hsl(330, 100%, 86.2745098039%)",
        10 => "hsl(0, 100%, 86.2745098039%)",
        _ => "hsl(30, 100%, 86.2745098039%)",
    }
}

pub(super) fn default_c_scale_label(index: usize) -> &'static str {
    match index {
        0 | 3 => "#ffffff",
        _ => "black",
    }
}

pub(super) fn journey_default_fill_type(index: usize) -> &'static str {
    match index {
        0 => "#ECECFF",
        1 => "#ffffde",
        2 => "hsl(304, 100%, 96.2745098039%)",
        3 => "hsl(124, 100%, 93.5294117647%)",
        4 => "hsl(176, 100%, 96.2745098039%)",
        5 => "hsl(-4, 100%, 93.5294117647%)",
        6 => "hsl(8, 100%, 96.2745098039%)",
        _ => "hsl(188, 100%, 93.5294117647%)",
    }
}

pub(super) fn css_color_is_transparent(color: &str) -> bool {
    ThemeColor::parse(color.trim()).is_ok_and(|color| color.channel(ColorChannel::Alpha) == 0.0)
}

pub(super) fn css_color_is_white_like(color: &str) -> bool {
    ThemeColor::parse(color.trim()).is_ok_and(|color| {
        color.channel(ColorChannel::Red) >= 250.0
            && color.channel(ColorChannel::Green) >= 250.0
            && color.channel(ColorChannel::Blue) >= 250.0
    })
}

pub(super) fn style_has_non_empty_decl(style: &str, property: &str) -> bool {
    style.split(';').any(|declaration| {
        let Some((key, value)) = declaration.split_once(':') else {
            return false;
        };
        key.trim().eq_ignore_ascii_case(property) && !value.trim().is_empty()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_treemap_readability_policy_uses_shared_color_parsing() {
        assert!(css_color_is_transparent("transparent"));
        assert!(css_color_is_transparent("rgba(10 20 30 / 0)"));
        assert!(css_color_is_white_like("white"));
        assert!(css_color_is_white_like("rgb(250 251 252)"));
        assert!(!css_color_is_white_like("var(--runtime-color)"));
    }
}
