// Notepads Markdown execution-control modifications: Copyright (c) 2026, Jiaqi (0x7c13) Liu.
pub mod engine;
pub mod hbox;
pub mod katex_svg;
pub mod layout_box;
pub mod layout_options;
pub mod spacing;
pub mod stacked_delim;
pub mod surd;
pub mod to_display;
pub mod vbox;

pub use engine::{layout, layout_with_fallback_metrics};
pub use layout_box::LayoutBox;
pub use layout_options::{FallbackGlyphMetrics, LayoutOptions};
pub use resource_limits::{MAX_LAYOUT_OUTPUT_ITEMS, validate_layout};
pub use to_display::to_display_list;

mod resource_limits;

mod bounded_writer;
mod scoped_options;

pub use engine::layout_with_control;
pub use engine::layout_with_storage;
pub use resource_limits::{ControlledValidationError, validate_layout_with_control};
pub use to_display::to_display_list_with_control;
