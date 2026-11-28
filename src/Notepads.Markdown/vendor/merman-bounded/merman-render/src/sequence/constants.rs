use crate::text::{TextMeasurer, TextStyle};
use merman_core::diagrams::sequence::SequenceActor;

pub(crate) const SEQUENCE_MESSAGE_WRAP_PADDING_SIDES: f64 = 2.0;
pub(crate) const SEQUENCE_SELF_MESSAGE_FRAME_EXTRA_Y_PX: f64 = 60.0;
pub(crate) const SEQUENCE_FRAME_SIDE_PAD_PX: f64 = 11.0;
pub(crate) const SEQUENCE_FRAME_GEOM_PAD_PX: f64 = 10.0;
pub(crate) const SEQUENCE_ACTOR_POPUP_PANEL_BASE_HEIGHT: f64 = 20.0;
pub(crate) const SEQUENCE_ACTOR_POPUP_ROW_HEIGHT: f64 = 30.0;

pub(crate) fn sequence_text_dimensions_height_px(font_size_px: f64) -> f64 {
    (font_size_px.max(1.0) * (17.0 / 16.0)).round().max(1.0)
}

pub(crate) fn sequence_text_line_step_px(font_size_px: f64) -> f64 {
    font_size_px.max(1.0) * 1.1875
}

pub(crate) fn sequence_actor_popup_panel_height(link_count: usize) -> f64 {
    SEQUENCE_ACTOR_POPUP_PANEL_BASE_HEIGHT + (link_count as f64) * SEQUENCE_ACTOR_POPUP_ROW_HEIGHT
}

pub(crate) fn sequence_actor_popup_min_width(
    actor: &SequenceActor,
    measurer: &dyn TextMeasurer,
    style: &TextStyle,
    wrap_padding: f64,
    box_margin: f64,
) -> f64 {
    actor
        .links
        .keys()
        .map(|label| {
            measurer.measure(label, style).width.max(0.0) + 2.0 * wrap_padding + 2.0 * box_margin
        })
        .fold(0.0, f64::max)
}

pub(crate) fn sequence_actor_popup_rect_height(
    actor_type: &str,
    visual_height: f64,
    base_height: f64,
    is_neo: bool,
    mirror_actors: bool,
) -> f64 {
    // drawPopup inherits the last drawActor call's rectData. Top drawing keeps the pre-shape
    // actor height for collection, queue, and database shapes; mirrored drawing runs after shape mutation and uses
    // the footer's visual node height. Neo collections shorten both rectData copies by six pixels.
    let height = if mirror_actors || !matches!(actor_type, "collections" | "queue" | "database") {
        visual_height
    } else {
        base_height
    };
    if actor_type == "collections" && is_neo {
        (height - 6.0).max(0.0)
    } else {
        height
    }
}

pub(super) fn sequence_actor_visual_height(
    actor_type: &str,
    base_width: f64,
    base_height: f64,
    label_box_height: f64,
) -> f64 {
    match actor_type {
        // Mermaid derives these from the actor-type glyph bbox + label box height.
        // These heights are used by the footer actor rendering and affect the final SVG viewBox.
        "boundary" => (44.0 + label_box_height).max(1.0),
        // Mermaid's database actor updates the actor height from the cylinder bbox after render.
        // The cylinder uses `rect.width / 3`, then the label box height is added.
        "database" => ((base_width / 3.0) + label_box_height).max(1.0),
        "entity" => (44.0 + label_box_height).max(1.0),
        // Control uses an extra label-box height in Mermaid.
        "control" => (44.0 + 2.0 * label_box_height).max(1.0),
        _ => base_height.max(1.0),
    }
}

pub(super) fn sequence_actor_lifeline_start_y(
    actor_type: &str,
    base_height: f64,
    box_text_margin: f64,
) -> f64 {
    match actor_type {
        // Hard-coded in Mermaid's sequence svgDraw.js for these actor types.
        "actor" | "boundary" => 80.0,
        "control" | "entity" => 75.0,
        // For database, Mermaid starts the lifeline slightly below the actor box.
        "database" => base_height + 2.0 * box_text_margin,
        _ => base_height,
    }
}

/// Mermaid's Neo participant row: glyph, gap, measured text, gap to the lifeline.
pub(crate) const SEQUENCE_GLYPH_BAND_HEIGHT: f64 = 44.0;
pub(crate) fn sequence_actor_stack_height(text_height: f64) -> f64 {
    SEQUENCE_GLYPH_BAND_HEIGHT + 6.0 + text_height + 6.0
}

pub(crate) struct SequenceActorBands {
    pub(crate) glyph_bottom_y: f64,
    pub(crate) label_center_y: f64,
}

impl SequenceActorBands {
    pub(crate) fn new(actor_y: f64, row_height: f64, text_height: f64, footer: bool) -> Self {
        if footer {
            let glyph_bottom_y = actor_y + 10.0 + SEQUENCE_GLYPH_BAND_HEIGHT;
            Self {
                glyph_bottom_y,
                label_center_y: glyph_bottom_y + 6.0 + text_height / 2.0,
            }
        } else {
            let datum = actor_y + row_height;
            Self {
                glyph_bottom_y: datum - 6.0 - text_height - 6.0,
                label_center_y: datum - 3.0 - text_height / 2.0,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn sequence_text_and_frame_constants_match_mermaid() {
        assert_eq!(super::SEQUENCE_MESSAGE_WRAP_PADDING_SIDES, 2.0);
        assert_eq!(super::SEQUENCE_ACTOR_POPUP_PANEL_BASE_HEIGHT, 20.0);
        assert_eq!(super::SEQUENCE_ACTOR_POPUP_ROW_HEIGHT, 30.0);
        assert_eq!(super::sequence_actor_popup_panel_height(0), 20.0);
        assert_eq!(super::sequence_actor_popup_panel_height(4), 140.0);
        assert_eq!(
            super::sequence_actor_popup_rect_height("queue", 74.0, 65.0, false, false),
            65.0
        );
        assert_eq!(
            super::sequence_actor_popup_rect_height("queue", 74.0, 65.0, false, true),
            74.0
        );
        assert_eq!(
            super::sequence_actor_popup_rect_height("collections", 80.0, 80.0, true, false),
            74.0
        );
        assert_eq!(super::sequence_text_dimensions_height_px(16.0), 17.0);
        assert_eq!(super::sequence_text_dimensions_height_px(10.0), 11.0);
        assert_eq!(super::sequence_text_line_step_px(16.0), 19.0);
        assert_eq!(
            super::sequence_actor_visual_height("database", 150.0, 65.0, 20.0),
            70.0
        );
        assert_eq!(
            super::sequence_actor_visual_height("boundary", 150.0, 65.0, 20.0),
            64.0
        );
        assert_eq!(
            super::sequence_actor_visual_height("entity", 150.0, 65.0, 20.0),
            64.0
        );
        assert_eq!(
            super::sequence_actor_visual_height("control", 150.0, 65.0, 20.0),
            84.0
        );
        assert_eq!(super::SEQUENCE_SELF_MESSAGE_FRAME_EXTRA_Y_PX, 60.0);
        assert_eq!(super::SEQUENCE_FRAME_SIDE_PAD_PX, 11.0);
        assert_eq!(super::SEQUENCE_FRAME_GEOM_PAD_PX, 10.0);
    }
}
