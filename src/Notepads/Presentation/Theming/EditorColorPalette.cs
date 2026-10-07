// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using Windows.UI;
using Windows.UI.ViewManagement;
using Windows.UI.Xaml;

namespace Notepads.Presentation.Theming;

internal enum EditorColorRole { Background, Foreground, LineNumber, CaretLine, SelectionText }

internal sealed class EditorColorPalette
{
    private static readonly object PaletteLock = new();
    private static EditorColorPalette _light;
    private static EditorColorPalette _dark;
    private readonly Color[] _colors;
    private EditorColorPalette(Color[] colors) => _colors = colors;
    private EditorColorPalette(ResourceDictionary resources) : this(
    [
        (Color)resources["EditorBackgroundColor"],
        (Color)resources["EditorForegroundColor"],
        (Color)resources["EditorLineNumberColor"],
        (Color)resources["EditorCaretLineColor"],
        (Color)resources["EditorSelectionTextColor"]
    ]) { }

    public Color this[EditorColorRole role] => _colors[(int)role];

    // Light and dark palettes are shared copies of the theme resources. High
    // contrast returns a new palette with the current system colors.
    public static EditorColorPalette ForTheme(ElementTheme theme, bool highContrast)
    {
        if (highContrast)
        {
            var settings = new UISettings();
            var foreground = settings.GetColorValue(UIColorType.Foreground);
            // High contrast hides the caret line.
            return new EditorColorPalette([settings.GetColorValue(UIColorType.Background), foreground, foreground,
                Colors.Transparent, (Color)Application.Current.Resources["SystemColorHighlightTextColor"]]);
        }
        lock (PaletteLock)
        {
            if (_dark == null)
            {
                var themes = new ResourceDictionary { Source = new Uri("ms-appx:///Theming/EditorThemes.xaml") };
                _dark = new EditorColorPalette((ResourceDictionary)themes.ThemeDictionaries["Dark"]);
                _light = new EditorColorPalette((ResourceDictionary)themes.ThemeDictionaries["Light"]);
            }
            return theme == ElementTheme.Dark ? _dark : _light;
        }
    }
}
