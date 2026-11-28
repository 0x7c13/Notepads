mod collapse_doglegs;
mod common;
mod external_rail_channels;
mod lane_title_bands;
mod obstacle_rails;
mod resolve_crossings;
mod shortcut_jogs;
mod swap_terminal_tails;
mod terminal_lanes;

pub(super) use collapse_doglegs::collapse_redundant_rectangular_doglegs;
pub(super) use external_rail_channels::reassign_crossing_external_rail_channels;
pub(super) use lane_title_bands::{
    lift_top_lane_title_bands_above_rails, shift_left_lane_title_bands_left_of_rails,
};
pub(super) use obstacle_rails::lift_obstacle_hugging_same_side_rails;
pub(super) use resolve_crossings::resolve_rendered_orthogonal_crossings;
pub(super) use shortcut_jogs::shortcut_redundant_orthogonal_jogs;
pub(super) use swap_terminal_tails::swap_destination_terminal_tails_to_reduce_crossings;
pub(super) use terminal_lanes::separate_shared_rendered_terminal_lanes;

#[cfg(test)]
mod tests;
