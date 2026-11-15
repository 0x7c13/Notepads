// Code based on https://github.com/defuz/sublimate/blob/master/src/core/syntax/theme.rs
// released under the MIT license by @defuz

use std::str::FromStr;

use super::selector::*;
use super::settings::{ParseSettings, Settings};
use super::style::*;
use super::theme::*;
use crate::parsing::ParseScopeError;

use self::ParseThemeError::*;

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ParseThemeError {
    #[error("Incorrect underline option")]
    IncorrectUnderlineOption,
    #[error("Incorrect font style: {0}")]
    IncorrectFontStyle(String),
    #[error("Incorrect color")]
    IncorrectColor,
    #[error("Incorrect syntax")]
    IncorrectSyntax,
    #[error("Incorrect settings")]
    IncorrectSettings,
    #[error("Undefined settings")]
    UndefinedSettings,
    #[error("Undefined scope settings: {0}")]
    UndefinedScopeSettings(String),
    #[error("Color sheme scope is not object")]
    ColorShemeScopeIsNotObject,
    #[error("Color sheme settings is not object")]
    ColorShemeSettingsIsNotObject,
    #[error("Scope selector is not string: {0}")]
    ScopeSelectorIsNotString(String),
    #[error("Duplicate settings")]
    DuplicateSettings,
    #[error("Scope parse error: {0}")]
    ScopeParse(#[from] ParseScopeError),
}

impl FromStr for UnderlineOption {
    type Err = ParseThemeError;

    fn from_str(s: &str) -> Result<UnderlineOption, Self::Err> {
        Ok(match s {
            "underline" => UnderlineOption::Underline,
            "stippled_underline" => UnderlineOption::StippledUnderline,
            "squiggly_underline" => UnderlineOption::SquigglyUnderline,
            _ => return Err(IncorrectUnderlineOption),
        })
    }
}

impl ParseSettings for UnderlineOption {
    type Error = ParseThemeError;

    fn parse_settings(settings: Settings) -> Result<UnderlineOption, Self::Error> {
        match settings {
            Settings::String(value) => UnderlineOption::from_str(&value),
            _ => Err(IncorrectUnderlineOption),
        }
    }
}

impl FromStr for FontStyle {
    type Err = ParseThemeError;

    fn from_str(s: &str) -> Result<FontStyle, Self::Err> {
        let mut font_style = FontStyle::empty();
        for i in s.split_whitespace() {
            font_style.insert(match i {
                "bold" => FontStyle::BOLD,
                "underline" => FontStyle::UNDERLINE,
                "italic" => FontStyle::ITALIC,
                "normal" | "regular" => FontStyle::empty(),
                s => return Err(IncorrectFontStyle(s.to_owned())),
            })
        }
        Ok(font_style)
    }
}

impl ParseSettings for FontStyle {
    type Error = ParseThemeError;

    fn parse_settings(settings: Settings) -> Result<FontStyle, Self::Error> {
        match settings {
            Settings::String(value) => FontStyle::from_str(&value),
            c => Err(IncorrectFontStyle(c.to_string())),
        }
    }
}

impl FromStr for Color {
    type Err = ParseThemeError;

    fn from_str(s: &str) -> Result<Color, Self::Err> {
        let mut chars = s.chars();
        if chars.next() != Some('#') {
            return Err(IncorrectColor);
        }
        let mut d = Vec::new();
        for char in chars {
            d.push(char.to_digit(16).ok_or(IncorrectColor)? as u8);
        }
        Ok(match d.len() {
            3 => Color {
                r: d[0],
                g: d[1],
                b: d[2],
                a: 255,
            },
            6 => Color {
                r: d[0] * 16 + d[1],
                g: d[2] * 16 + d[3],
                b: d[4] * 16 + d[5],
                a: 255,
            },
            8 => Color {
                r: d[0] * 16 + d[1],
                g: d[2] * 16 + d[3],
                b: d[4] * 16 + d[5],
                a: d[6] * 16 + d[7],
            },
            _ => return Err(IncorrectColor),
        })
    }
}

impl ParseSettings for Color {
    type Error = ParseThemeError;

    fn parse_settings(settings: Settings) -> Result<Color, Self::Error> {
        match settings {
            Settings::String(value) => Color::from_str(&value),
            _ => Err(IncorrectColor),
        }
    }
}

impl ParseSettings for StyleModifier {
    type Error = ParseThemeError;

    fn parse_settings(settings: Settings) -> Result<StyleModifier, Self::Error> {
        let mut obj = match settings {
            Settings::Object(obj) => obj,
            _ => return Err(ColorShemeScopeIsNotObject),
        };
        let font_style = match obj.remove("fontStyle") {
            Some(Settings::String(value)) => Some(FontStyle::from_str(&value)?),
            None => None,
            Some(c) => return Err(IncorrectFontStyle(c.to_string())),
        };
        let foreground = match obj.remove("foreground") {
            Some(Settings::String(value)) => Some(Color::from_str(&value)?),
            None => None,
            _ => return Err(IncorrectColor),
        };
        let background = match obj.remove("background") {
            Some(Settings::String(value)) => Some(Color::from_str(&value)?),
            None => None,
            _ => return Err(IncorrectColor),
        };

        Ok(StyleModifier {
            foreground,
            background,
            font_style,
        })
    }
}

impl ParseSettings for ThemeItem {
    type Error = ParseThemeError;

    fn parse_settings(settings: Settings) -> Result<ThemeItem, Self::Error> {
        let mut obj = match settings {
            Settings::Object(obj) => obj,
            _ => return Err(ColorShemeScopeIsNotObject),
        };
        let scope = match obj.remove("scope") {
            Some(Settings::String(value)) => ScopeSelectors::from_str(&value)?,
            _ => return Err(ScopeSelectorIsNotString(format!("{:?}", obj))),
        };
        let style = match obj.remove("settings") {
            Some(settings) => StyleModifier::parse_settings(settings)?,
            None => return Err(IncorrectSettings),
        };
        Ok(ThemeItem { scope, style })
    }
}

impl ParseSettings for ThemeSettings {
    type Error = ParseThemeError;

    fn parse_settings(json: Settings) -> Result<ThemeSettings, Self::Error> {
        let mut settings = ThemeSettings::default();

        let obj = match json {
            Settings::Object(obj) => obj,
            _ => return Err(ColorShemeSettingsIsNotObject),
        };

        for (key, value) in obj {
            match &key[..] {
                "foreground" => settings.foreground = Color::parse_settings(value).ok(),
                "background" => settings.background = Color::parse_settings(value).ok(),
                "caret" => settings.caret = Color::parse_settings(value).ok(),
                "lineHighlight" => settings.line_highlight = Color::parse_settings(value).ok(),
                "misspelling" => settings.misspelling = Color::parse_settings(value).ok(),
                "minimapBorder" => settings.minimap_border = Color::parse_settings(value).ok(),
                "accent" => settings.accent = Color::parse_settings(value).ok(),

                "popupCss" => settings.popup_css = value.as_str().map(|s| s.to_owned()),
                "phantomCss" => settings.phantom_css = value.as_str().map(|s| s.to_owned()),

                "bracketContentsForeground" => {
                    settings.bracket_contents_foreground = Color::parse_settings(value).ok()
                }
                "bracketContentsOptions" => {
                    settings.bracket_contents_options = UnderlineOption::parse_settings(value).ok()
                }
                "bracketsForeground" => {
                    settings.brackets_foreground = Color::parse_settings(value).ok()
                }
                "bracketsBackground" => {
                    settings.brackets_background = Color::parse_settings(value).ok()
                }
                "bracketsOptions" => {
                    settings.brackets_options = UnderlineOption::parse_settings(value).ok()
                }
                "tagsForeground" => settings.tags_foreground = Color::parse_settings(value).ok(),
                "tagsOptions" => {
                    settings.tags_options = UnderlineOption::parse_settings(value).ok()
                }
                "highlight" => settings.highlight = Color::parse_settings(value).ok(),
                "findHighlight" => settings.find_highlight = Color::parse_settings(value).ok(),
                "findHighlightForeground" => {
                    settings.find_highlight_foreground = Color::parse_settings(value).ok()
                }
                "gutter" => settings.gutter = Color::parse_settings(value).ok(),
                "gutterForeground" => {
                    settings.gutter_foreground = Color::parse_settings(value).ok()
                }
                "selection" => settings.selection = Color::parse_settings(value).ok(),
                "selectionForeground" => {
                    settings.selection_foreground = Color::parse_settings(value).ok()
                }
                "selectionBorder" => settings.selection_border = Color::parse_settings(value).ok(),
                "inactiveSelection" => {
                    settings.inactive_selection = Color::parse_settings(value).ok()
                }
                "inactiveSelectionForeground" => {
                    settings.inactive_selection_foreground = Color::parse_settings(value).ok()
                }
                "guide" => settings.guide = Color::parse_settings(value).ok(),
                "activeGuide" => settings.active_guide = Color::parse_settings(value).ok(),
                "stackGuide" => settings.stack_guide = Color::parse_settings(value).ok(),
                "shadow" => settings.shadow = Color::parse_settings(value).ok(),
                _ => (), // E.g. "shadowWidth" and "invisibles" are ignored
            }
        }
        Ok(settings)
    }
}

impl ParseSettings for Theme {
    type Error = ParseThemeError;

    fn parse_settings(settings: Settings) -> Result<Theme, Self::Error> {
        let mut obj = match settings {
            Settings::Object(obj) => obj,
            _ => return Err(IncorrectSyntax),
        };
        let name = match obj.remove("name") {
            Some(Settings::String(name)) => Some(name),
            None => None,
            _ => return Err(IncorrectSyntax),
        };
        let author = match obj.remove("author") {
            Some(Settings::String(author)) => Some(author),
            None => None,
            _ => return Err(IncorrectSyntax),
        };
        let items = match obj.remove("settings") {
            Some(Settings::Array(items)) => items,
            _ => return Err(IncorrectSyntax),
        };
        let mut iter = items.into_iter();
        let mut settings = match iter.next() {
            Some(Settings::Object(mut obj)) => match obj.remove("settings") {
                Some(settings) => ThemeSettings::parse_settings(settings)?,
                None => return Err(UndefinedSettings),
            },
            _ => return Err(UndefinedSettings),
        };
        if let Some(Settings::Object(obj)) = obj.remove("gutterSettings") {
            for (key, value) in obj {
                let color = Color::parse_settings(value).ok();
                match &key[..] {
                    "background" => settings.gutter = settings.gutter.or(color),
                    "foreground" => {
                        settings.gutter_foreground = settings.gutter_foreground.or(color)
                    }
                    _ => (),
                }
            }
        }
        let mut scopes = Vec::new();
        for json in iter {
            // TODO option to disable best effort parsing and bubble up warnings
            if let Ok(item) = ThemeItem::parse_settings(json) {
                scopes.push(item);
            }
        }
        Ok(Theme {
            name,
            author,
            settings,
            scopes,
        })
    }
}
