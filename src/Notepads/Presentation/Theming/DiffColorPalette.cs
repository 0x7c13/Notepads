// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using Windows.UI;
using Windows.UI.ViewManagement;
using Windows.UI.Xaml;

namespace Notepads.Presentation.Theming;

internal enum DiffColorRole { AddedLine, DeletedLine, AddedText, DeletedText, Gap, GapHatch }

internal sealed class DiffColorPalette
{
    private static readonly object PaletteLock = new();
    private static DiffColorPalette _light;
    private static DiffColorPalette _dark;
    private readonly Color[] _colors;
    private DiffColorPalette(Color[] colors) => _colors = colors;
    private DiffColorPalette(ResourceDictionary resources) : this(
    [
        (Color)resources["DiffAddedLineColor"],
        (Color)resources["DiffDeletedLineColor"],
        (Color)resources["DiffAddedTextColor"],
        (Color)resources["DiffDeletedTextColor"],
        (Color)resources["DiffGapColor"],
        (Color)resources["DiffGapHatchColor"]
    ]) { }

    public Color this[DiffColorRole role] => _colors[(int)role];

    public static DiffColorPalette ForTheme(ElementTheme theme, bool highContrast)
    {
        if (highContrast)
        {
            var settings = new UISettings();
            var highlight = (Color)Application.Current.Resources["SystemColorHighlightColor"];
            var background = settings.GetColorValue(UIColorType.Background);
            // Gutter stripes and Old/New labels identify sides without color alone.
            var tint = Color.FromArgb(40, highlight.R, highlight.G, highlight.B);
            return new DiffColorPalette([tint, tint, highlight, highlight, background, settings.GetColorValue(UIColorType.Foreground)]);
        }
        lock (PaletteLock)
        {
            if (_dark == null)
            {
                var themes = new ResourceDictionary { Source = new Uri("ms-appx:///Theming/DiffThemes.xaml") };
                _dark = new DiffColorPalette((ResourceDictionary)themes.ThemeDictionaries["Dark"]);
                _light = new DiffColorPalette((ResourceDictionary)themes.ThemeDictionaries["Light"]);
            }
            return theme == ElementTheme.Dark ? _dark : _light;
        }
    }
}
