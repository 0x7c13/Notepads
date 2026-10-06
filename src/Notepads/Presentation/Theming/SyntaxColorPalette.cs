// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using Windows.UI;
using Windows.UI.Xaml;

namespace Notepads.Presentation.Theming;

internal enum SyntaxColorRole { Default, Comment, Keyword, String, Number, Operator, Type, Markup, Attribute }

// Lexers name color roles; the theme supplies their colors. Keep resource loading
// at this boundary so a future theme-file reader can supply the same palette.
internal sealed class SyntaxColorPalette
{
    private static readonly object PaletteLock = new();
    private static SyntaxColorPalette _light;
    private static SyntaxColorPalette _dark;
    private readonly Color[] _colors;

    private SyntaxColorPalette(ResourceDictionary resources)
    {
        _colors =
        [
            (Color)resources["SyntaxDefaultColor"],
            (Color)resources["SyntaxCommentColor"],
            (Color)resources["SyntaxKeywordColor"],
            (Color)resources["SyntaxStringColor"],
            (Color)resources["SyntaxNumberColor"],
            (Color)resources["SyntaxOperatorColor"],
            (Color)resources["SyntaxTypeColor"],
            (Color)resources["SyntaxMarkupColor"],
            (Color)resources["SyntaxAttributeColor"]
        ];
    }

    public Color this[SyntaxColorRole role] => _colors[(int)role];

    // First loading occurs on an editor's UI thread. Cache only copied Color
    // values: XAML dictionaries belong to their creating window's apartment.
    // High contrast uses the existing system foreground instead of this palette.
    public static SyntaxColorPalette ForTheme(ElementTheme theme)
    {
        lock (PaletteLock)
        {
            if (_dark == null)
            {
                var themes = new ResourceDictionary { Source = new Uri("ms-appx:///Theming/SyntaxThemes.xaml") };
                var dark = new SyntaxColorPalette((ResourceDictionary)themes.ThemeDictionaries["Dark"]);
                var light = new SyntaxColorPalette((ResourceDictionary)themes.ThemeDictionaries["Light"]);
                _dark = dark;
                _light = light;
            }
            return theme == ElementTheme.Dark ? _dark : _light;
        }
    }
}
