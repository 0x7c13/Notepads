// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

using Notepads.Features.Preferences;
using Windows.UI.Text;
using Windows.UI.Xaml;

namespace Notepads.Presentation.Theming;

internal static class PreferenceValues
{
    public static FontStyle ToFontStyle(this FontSlant slant) =>
        slant == FontSlant.Italic ? FontStyle.Italic : slant == FontSlant.Oblique ? FontStyle.Oblique : FontStyle.Normal;

    public static FontSlant ToFontSlant(this FontStyle style) =>
        style == FontStyle.Italic ? FontSlant.Italic : style == FontStyle.Oblique ? FontSlant.Oblique : FontSlant.Normal;

    public static FontWeight ToFontWeight(this ushort weight) => new() { Weight = weight };

    public static TextWrapping ToTextWrapping(this bool wrap) => wrap ? TextWrapping.Wrap : TextWrapping.NoWrap;
}
