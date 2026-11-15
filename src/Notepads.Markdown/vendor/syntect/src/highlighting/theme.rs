// Code based on https://github.com/defuz/sublimate/blob/master/src/core/syntax/theme.rs
// released under the MIT license by @defuz
use super::selector::*;
use super::style::*;
use serde_derive::{Deserialize, Serialize};

/// A theme parsed from a `.tmTheme` file.
///
/// This contains additional fields useful for a theme list as well as `settings` for styling your editor.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Theme {
    pub name: Option<String>,
    pub author: Option<String>,
    /// External settings for the editor using this theme
    pub settings: ThemeSettings,
    /// The styling rules for the viewed text
    pub scopes: Vec<ThemeItem>,
}

/// Properties for styling the UI of a text editor
///
/// This essentially consists of the styles that aren't directly applied to the text being viewed.
/// `ThemeSettings` are intended to be used to make the UI of the editor match the styling of the
/// text itself.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ThemeSettings {
    /// The default color for text.
    pub foreground: Option<Color>,
    /// The default backgound color of the view.
    pub background: Option<Color>,
    /// Color of the caret.
    pub caret: Option<Color>,
    /// Color of the line the caret is in.
    /// Only used when the `highlight_line` setting is set to `true`.
    pub line_highlight: Option<Color>,

    /// The color to use for the squiggly underline drawn under misspelled words.
    pub misspelling: Option<Color>,
    /// The color of the border drawn around the viewport area of the minimap.
    /// Only used when the `draw_minimap_border` setting is enabled.
    pub minimap_border: Option<Color>,
    /// A color made available for use by the theme.
    pub accent: Option<Color>,
    /// CSS passed to popups.
    pub popup_css: Option<String>,
    /// CSS passed to phantoms.
    pub phantom_css: Option<String>,

    /// Color of bracketed sections of text when the caret is in a bracketed section.
    /// Only applied when the `match_brackets` setting is set to `true`.
    pub bracket_contents_foreground: Option<Color>,
    /// Controls certain options when the caret is in a bracket section.
    /// Only applied when the `match_brackets` setting is set to `true`.
    pub bracket_contents_options: Option<UnderlineOption>,
    /// Foreground color of the brackets when the caret is next to a bracket.
    /// Only applied when the `match_brackets` setting is set to `true`.
    pub brackets_foreground: Option<Color>,
    /// Background color of the brackets when the caret is next to a bracket.
    /// Only applied when the `match_brackets` setting is set to `true`.
    pub brackets_background: Option<Color>,
    /// Controls certain options when the caret is next to a bracket.
    /// Only applied when the `match_brackets` setting is set to `true`.
    pub brackets_options: Option<UnderlineOption>,

    /// Color of tags when the caret is next to a tag.
    /// Only used when the `match_tags` setting is set to `true`.
    pub tags_foreground: Option<Color>,
    /// Controls certain options when the caret is next to a tag.
    /// Only applied when the `match_tags` setting is set to `true`.
    pub tags_options: Option<UnderlineOption>,

    /// The border color for "other" matches.
    pub highlight: Option<Color>,
    /// Background color of regions matching the current search.
    pub find_highlight: Option<Color>,
    /// Text color of regions matching the current search.
    pub find_highlight_foreground: Option<Color>,

    /// Background color of the gutter.
    pub gutter: Option<Color>,
    /// Foreground color of the gutter.
    pub gutter_foreground: Option<Color>,

    /// The background color of selected text.
    pub selection: Option<Color>,
    /// A color that will override the scope-based text color of the selection.
    pub selection_foreground: Option<Color>,

    /// Color of the selection regions border.
    pub selection_border: Option<Color>,
    /// The background color of a selection in a view that is not currently focused.
    pub inactive_selection: Option<Color>,
    /// A color that will override the scope-based text color of the selection
    /// in a view that is not currently focused.
    pub inactive_selection_foreground: Option<Color>,

    /// Color of the guides displayed to indicate nesting levels.
    pub guide: Option<Color>,
    /// Color of the guide lined up with the caret.
    /// Only applied if the `indent_guide_options` setting is set to `draw_active`.
    pub active_guide: Option<Color>,
    /// Color of the current guide’s parent guide level.
    /// Only used if the `indent_guide_options` setting is set to `draw_active`.
    pub stack_guide: Option<Color>,

    /// The color of the shadow used when a text area can be horizontally scrolled.
    pub shadow: Option<Color>,
}

/// A component of a theme meant to highlight a specific thing (e.g string literals)
/// in a certain way.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ThemeItem {
    /// Target scope name.
    pub scope: ScopeSelectors,
    /// The style to use for this component
    pub style: StyleModifier,
}

#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize)]
pub enum UnderlineOption {
    #[default]
    None,
    Underline,
    StippledUnderline,
    SquigglyUnderline,
}
