// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2020-2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using Notepads.Features.Preferences;

namespace Notepads.Presentation.Controls.TextEditor;

public sealed partial class TextEditorCore
{
    private void IncreaseFontSize(double delta)
    {
        SetFontZoomFactor(_fontZoomFactor % 10 > 0
            ? Math.Ceiling(_fontZoomFactor / 10) * 10
            : _fontZoomFactor + delta * 100);
    }

    private void DecreaseFontSize(double delta)
    {
        SetFontZoomFactor(_fontZoomFactor % 10 > 0
            ? Math.Floor(_fontZoomFactor / 10) * 10
            : _fontZoomFactor - delta * 100);
    }

    private void ResetFontSizeToDefault()
    {
        FontSize = ApplicationPreferences.EditorFontSize;
    }
}
