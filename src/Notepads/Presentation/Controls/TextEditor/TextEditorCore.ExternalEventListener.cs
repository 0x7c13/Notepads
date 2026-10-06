// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2020-2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

using Notepads.Features.Preferences;
using Notepads.Presentation.Helpers;
using Notepads.Presentation.Theming;
using Windows.UI;
using Windows.UI.Text;
using Windows.UI.Xaml;
using Windows.UI.Xaml.Media;

namespace Notepads.Presentation.Controls.TextEditor;

public sealed partial class TextEditorCore
{
    internal void HookExternalEvents()
    {
        ApplicationPreferences.OnFontFamilyChanged += EditorSettingsService_OnFontFamilyChanged;
        ApplicationPreferences.OnFontSizeChanged += EditorSettingsService_OnFontSizeChanged;
        ApplicationPreferences.OnFontStyleChanged += EditorSettingsService_OnFontStyleChanged;
        ApplicationPreferences.OnFontWeightChanged += EditorSettingsService_OnFontWeightChanged;
        ApplicationPreferences.OnDefaultTextWrappingChanged += EditorSettingsService_OnDefaultTextWrappingChanged;
        ApplicationPreferences.OnDefaultDisplayLineNumbersViewStateChanged += EditorSettingsService_OnDefaultDisplayLineNumbersViewStateChanged;
        ApplicationPreferences.OnDefaultLineHighlighterViewStateChanged += EditorSettingsService_OnDefaultLineHighlighterViewStateChanged;

        ThemeSettingsService.OnAccentColorChanged += ThemeSettingsService_OnAccentColorChanged;
    }

    internal void UnhookExternalEvents()
    {
        ApplicationPreferences.OnFontFamilyChanged -= EditorSettingsService_OnFontFamilyChanged;
        ApplicationPreferences.OnFontSizeChanged -= EditorSettingsService_OnFontSizeChanged;
        ApplicationPreferences.OnFontStyleChanged -= EditorSettingsService_OnFontStyleChanged;
        ApplicationPreferences.OnFontWeightChanged -= EditorSettingsService_OnFontWeightChanged;
        ApplicationPreferences.OnDefaultTextWrappingChanged -= EditorSettingsService_OnDefaultTextWrappingChanged;
        ApplicationPreferences.OnDefaultDisplayLineNumbersViewStateChanged -= EditorSettingsService_OnDefaultDisplayLineNumbersViewStateChanged;
        ApplicationPreferences.OnDefaultLineHighlighterViewStateChanged -= EditorSettingsService_OnDefaultLineHighlighterViewStateChanged;

        ThemeSettingsService.OnAccentColorChanged -= ThemeSettingsService_OnAccentColorChanged;
    }

    private async void EditorSettingsService_OnFontFamilyChanged(object sender, string fontFamily)
    {
        if (_disposed) return;
        await Dispatcher.CallOnUIThreadAsync(() =>
        {
            if (_disposed) return;
            FontFamily = new FontFamily(fontFamily);
            ApplyFont();
        });
    }

    private async void EditorSettingsService_OnFontSizeChanged(object sender, int fontSize)
    {
        if (_disposed) return;
        await Dispatcher.CallOnUIThreadAsync(() =>
        {
            if (_disposed) return;
            FontSize = fontSize;
        });
    }

    private async void EditorSettingsService_OnFontStyleChanged(object sender, FontSlant fontStyle)
    {
        if (_disposed) return;
        await Dispatcher.CallOnUIThreadAsync(() =>
        {
            if (_disposed) return;
            FontStyle = fontStyle.ToFontStyle();
            ApplyFont();
        });
    }

    private async void EditorSettingsService_OnFontWeightChanged(object sender, ushort fontWeight)
    {
        if (_disposed) return;
        await Dispatcher.CallOnUIThreadAsync(() =>
        {
            if (_disposed) return;
            FontWeight = fontWeight.ToFontWeight();
            ApplyFont();
        });
    }

    private async void EditorSettingsService_OnDefaultTextWrappingChanged(object sender, bool textWrapping)
    {
        if (_disposed) return;
        await Dispatcher.CallOnUIThreadAsync(() =>
        {
            if (_disposed) return;
            TextWrapping = textWrapping.ToTextWrapping();
        });
    }

    private async void EditorSettingsService_OnDefaultDisplayLineNumbersViewStateChanged(object sender, bool displayLineNumbers)
    {
        if (_disposed) return;
        await Dispatcher.CallOnUIThreadAsync(() =>
        {
            if (_disposed) return;
            DisplayLineNumbers = displayLineNumbers;
        });
    }

    private async void EditorSettingsService_OnDefaultLineHighlighterViewStateChanged(object sender, bool displayLineHighlighter)
    {
        if (_disposed) return;
        await Dispatcher.CallOnUIThreadAsync(() =>
        {
            if (_disposed) return;
            DisplayLineHighlighter = displayLineHighlighter;
        });
    }

    private async void ThemeSettingsService_OnAccentColorChanged(object sender, Color color)
    {
        if (_disposed) return;
        await Dispatcher.CallOnUIThreadAsync(() =>
        {
            if (_disposed) return;
            ApplySelectionAppearance();
        });
    }
}
