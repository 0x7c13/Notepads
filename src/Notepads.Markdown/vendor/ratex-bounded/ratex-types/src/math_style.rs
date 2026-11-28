use serde::{Deserialize, Serialize};

/// TeX math styles, controlling sizes of sub-expressions.
///
/// In TeX, the four main styles are D (display), T (text), S (script),
/// SS (scriptscript). Each has a "cramped" variant where superscripts
/// are set lower.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum MathStyle {
    #[default]
    Display,
    DisplayCramped,
    Text,
    TextCramped,
    Script,
    ScriptCramped,
    ScriptScript,
    ScriptScriptCramped,
}

impl MathStyle {
    // KaTeX Style.ts lookup tables (indexed by style ID 0..7):
    //   fracNum = [T, Tc, S, Sc, SS, SSc, SS, SSc]
    //   fracDen = [Tc, Tc, Sc, Sc, SSc, SSc, SSc, SSc]
    //   sup     = [S, Sc, S, Sc, SS, SSc, SS, SSc]
    //   sub     = [Sc, Sc, Sc, Sc, SSc, SSc, SSc, SSc]
    //   cramp   = [Dc, Dc, Tc, Tc, Sc, Sc, SSc, SSc]
    //   text    = [D, Dc, T, Tc, T, Tc, T, Tc]

    /// Style for the numerator of a fraction (preserves crampedness).
    pub fn numerator(self) -> Self {
        match self {
            Self::Display => Self::Text,
            Self::DisplayCramped => Self::TextCramped,
            Self::Text => Self::Script,
            Self::TextCramped => Self::ScriptCramped,
            Self::Script | Self::ScriptScript => Self::ScriptScript,
            Self::ScriptCramped | Self::ScriptScriptCramped => Self::ScriptScriptCramped,
        }
    }

    /// Style for the denominator of a fraction (always cramped).
    pub fn denominator(self) -> Self {
        match self {
            Self::Display | Self::DisplayCramped => Self::TextCramped,
            Self::Text | Self::TextCramped => Self::ScriptCramped,
            Self::Script | Self::ScriptCramped | Self::ScriptScript | Self::ScriptScriptCramped => {
                Self::ScriptScriptCramped
            }
        }
    }

    /// Style for superscripts (preserves crampedness).
    pub fn superscript(self) -> Self {
        match self {
            Self::Display | Self::Text => Self::Script,
            Self::DisplayCramped | Self::TextCramped => Self::ScriptCramped,
            Self::Script | Self::ScriptScript => Self::ScriptScript,
            Self::ScriptCramped | Self::ScriptScriptCramped => Self::ScriptScriptCramped,
        }
    }

    /// Style for subscripts (always cramped).
    pub fn subscript(self) -> Self {
        match self {
            Self::Display | Self::DisplayCramped | Self::Text | Self::TextCramped => {
                Self::ScriptCramped
            }
            Self::Script | Self::ScriptCramped | Self::ScriptScript | Self::ScriptScriptCramped => {
                Self::ScriptScriptCramped
            }
        }
    }

    /// The cramped version of this style.
    pub fn cramped(self) -> Self {
        match self {
            Self::Display => Self::DisplayCramped,
            Self::Text => Self::TextCramped,
            Self::Script => Self::ScriptCramped,
            Self::ScriptScript => Self::ScriptScriptCramped,
            other => other,
        }
    }

    /// Convert to the text-size equivalent (Script/ScriptScript → Text).
    /// Used inside `\text{}` blocks.
    pub fn text(self) -> Self {
        match self {
            Self::Display => Self::Display,
            Self::DisplayCramped => Self::DisplayCramped,
            Self::Text => Self::Text,
            Self::TextCramped => Self::TextCramped,
            Self::Script | Self::ScriptScript => Self::Text,
            Self::ScriptCramped | Self::ScriptScriptCramped => Self::TextCramped,
        }
    }

    pub fn is_display(self) -> bool {
        matches!(self, Self::Display | Self::DisplayCramped)
    }

    pub fn is_cramped(self) -> bool {
        matches!(
            self,
            Self::DisplayCramped
                | Self::TextCramped
                | Self::ScriptCramped
                | Self::ScriptScriptCramped
        )
    }

    /// True for Script/ScriptScript sizes — affects inter-atom spacing.
    pub fn is_tight(self) -> bool {
        matches!(
            self,
            Self::Script | Self::ScriptCramped | Self::ScriptScript | Self::ScriptScriptCramped
        )
    }

    /// Size index for looking up font metrics (0=text, 1=script, 2=scriptscript).
    pub fn size_index(self) -> usize {
        match self {
            Self::Display | Self::DisplayCramped | Self::Text | Self::TextCramped => 0,
            Self::Script | Self::ScriptCramped => 1,
            Self::ScriptScript | Self::ScriptScriptCramped => 2,
        }
    }

    /// Size multiplier relative to base font size (TeX rule).
    pub fn size_multiplier(self) -> f64 {
        match self {
            Self::Display | Self::DisplayCramped | Self::Text | Self::TextCramped => 1.0,
            Self::Script | Self::ScriptCramped => 0.7,
            Self::ScriptScript | Self::ScriptScriptCramped => 0.5,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use MathStyle::*;

    // Exhaustive tests against KaTeX Style.ts lookup tables.

    #[test]
    fn test_numerator_all_styles() {
        // KaTeX: fracNum = [T, Tc, S, Sc, SS, SSc, SS, SSc]
        assert_eq!(Display.numerator(), Text);
        assert_eq!(DisplayCramped.numerator(), TextCramped);
        assert_eq!(Text.numerator(), Script);
        assert_eq!(TextCramped.numerator(), ScriptCramped);
        assert_eq!(Script.numerator(), ScriptScript);
        assert_eq!(ScriptCramped.numerator(), ScriptScriptCramped);
        assert_eq!(ScriptScript.numerator(), ScriptScript);
        assert_eq!(ScriptScriptCramped.numerator(), ScriptScriptCramped);
    }

    #[test]
    fn test_denominator_all_styles() {
        // KaTeX: fracDen = [Tc, Tc, Sc, Sc, SSc, SSc, SSc, SSc]
        assert_eq!(Display.denominator(), TextCramped);
        assert_eq!(DisplayCramped.denominator(), TextCramped);
        assert_eq!(Text.denominator(), ScriptCramped);
        assert_eq!(TextCramped.denominator(), ScriptCramped);
        assert_eq!(Script.denominator(), ScriptScriptCramped);
        assert_eq!(ScriptCramped.denominator(), ScriptScriptCramped);
        assert_eq!(ScriptScript.denominator(), ScriptScriptCramped);
        assert_eq!(ScriptScriptCramped.denominator(), ScriptScriptCramped);
    }

    #[test]
    fn test_superscript_all_styles() {
        // KaTeX: sup = [S, Sc, S, Sc, SS, SSc, SS, SSc]
        assert_eq!(Display.superscript(), Script);
        assert_eq!(DisplayCramped.superscript(), ScriptCramped);
        assert_eq!(Text.superscript(), Script);
        assert_eq!(TextCramped.superscript(), ScriptCramped);
        assert_eq!(Script.superscript(), ScriptScript);
        assert_eq!(ScriptCramped.superscript(), ScriptScriptCramped);
        assert_eq!(ScriptScript.superscript(), ScriptScript);
        assert_eq!(ScriptScriptCramped.superscript(), ScriptScriptCramped);
    }

    #[test]
    fn test_subscript_all_styles() {
        // KaTeX: sub = [Sc, Sc, Sc, Sc, SSc, SSc, SSc, SSc]
        assert_eq!(Display.subscript(), ScriptCramped);
        assert_eq!(DisplayCramped.subscript(), ScriptCramped);
        assert_eq!(Text.subscript(), ScriptCramped);
        assert_eq!(TextCramped.subscript(), ScriptCramped);
        assert_eq!(Script.subscript(), ScriptScriptCramped);
        assert_eq!(ScriptCramped.subscript(), ScriptScriptCramped);
        assert_eq!(ScriptScript.subscript(), ScriptScriptCramped);
        assert_eq!(ScriptScriptCramped.subscript(), ScriptScriptCramped);
    }

    #[test]
    fn test_cramped_all_styles() {
        // KaTeX: cramp = [Dc, Dc, Tc, Tc, Sc, Sc, SSc, SSc]
        assert_eq!(Display.cramped(), DisplayCramped);
        assert_eq!(DisplayCramped.cramped(), DisplayCramped);
        assert_eq!(Text.cramped(), TextCramped);
        assert_eq!(TextCramped.cramped(), TextCramped);
        assert_eq!(Script.cramped(), ScriptCramped);
        assert_eq!(ScriptCramped.cramped(), ScriptCramped);
        assert_eq!(ScriptScript.cramped(), ScriptScriptCramped);
        assert_eq!(ScriptScriptCramped.cramped(), ScriptScriptCramped);
    }

    #[test]
    fn test_text_all_styles() {
        // KaTeX: text = [D, Dc, T, Tc, T, Tc, T, Tc]
        assert_eq!(Display.text(), Display);
        assert_eq!(DisplayCramped.text(), DisplayCramped);
        assert_eq!(Text.text(), Text);
        assert_eq!(TextCramped.text(), TextCramped);
        assert_eq!(Script.text(), Text);
        assert_eq!(ScriptCramped.text(), TextCramped);
        assert_eq!(ScriptScript.text(), Text);
        assert_eq!(ScriptScriptCramped.text(), TextCramped);
    }

    #[test]
    fn test_is_display() {
        assert!(Display.is_display());
        assert!(DisplayCramped.is_display());
        assert!(!Text.is_display());
        assert!(!Script.is_display());
    }

    #[test]
    fn test_is_tight() {
        assert!(!Display.is_tight());
        assert!(!Text.is_tight());
        assert!(Script.is_tight());
        assert!(ScriptCramped.is_tight());
        assert!(ScriptScript.is_tight());
        assert!(ScriptScriptCramped.is_tight());
    }

    #[test]
    fn test_size_index() {
        assert_eq!(Display.size_index(), 0);
        assert_eq!(Text.size_index(), 0);
        assert_eq!(Script.size_index(), 1);
        assert_eq!(ScriptScript.size_index(), 2);
    }

    #[test]
    fn test_size_multiplier() {
        assert!((Display.size_multiplier() - 1.0).abs() < f64::EPSILON);
        assert!((Text.size_multiplier() - 1.0).abs() < f64::EPSILON);
        assert!((Script.size_multiplier() - 0.7).abs() < f64::EPSILON);
        assert!((ScriptScript.size_multiplier() - 0.5).abs() < f64::EPSILON);
    }
}
