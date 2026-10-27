// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2020-2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
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
        ApplicationPreferences.OnDefaultTabIndentsChanged += EditorSettingsService_OnDefaultTabIndentsChanged;
        ApplicationPreferences.OnFontSizeChanged += EditorSettingsService_OnFontSizeChanged;
        ApplicationPreferences.OnFontStyleChanged += EditorSettingsService_OnFontStyleChanged;
        ApplicationPreferences.OnFontWeightChanged += EditorSettingsService_OnFontWeightChanged;
        ApplicationPreferences.OnFontRenderingChanged += EditorSettingsService_OnFontRenderingChanged;
        ApplicationPreferences.OnDefaultTextWrappingChanged += EditorSettingsService_OnDefaultTextWrappingChanged;
        ApplicationPreferences.OnDefaultDisplayLineNumbersViewStateChanged += EditorSettingsService_OnDefaultDisplayLineNumbersViewStateChanged;
        ApplicationPreferences.OnDefaultLineHighlighterViewStateChanged += EditorSettingsService_OnDefaultLineHighlighterViewStateChanged;

        ThemeSettingsService.OnAccentColorChanged += ThemeSettingsService_OnAccentColorChanged;
    }

    internal void UnhookExternalEvents()
    {
        ApplicationPreferences.OnFontFamilyChanged -= EditorSettingsService_OnFontFamilyChanged;
        ApplicationPreferences.OnDefaultTabIndentsChanged -= EditorSettingsService_OnDefaultTabIndentsChanged;
        ApplicationPreferences.OnFontSizeChanged -= EditorSettingsService_OnFontSizeChanged;
        ApplicationPreferences.OnFontStyleChanged -= EditorSettingsService_OnFontStyleChanged;
        ApplicationPreferences.OnFontWeightChanged -= EditorSettingsService_OnFontWeightChanged;
        ApplicationPreferences.OnFontRenderingChanged -= EditorSettingsService_OnFontRenderingChanged;
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
            ApplyOrDeferMoveSetting(nameof(EditorSettingsService_OnFontFamilyChanged), () =>
            {
                FontFamily = new FontFamily(fontFamily);
                ApplyFont();
            });
        });
    }

    private async void EditorSettingsService_OnDefaultTabIndentsChanged(object sender, int indentation)
    {
        if (_disposed) return;
        await Dispatcher.CallOnUIThreadAsync(() => ApplyOrDeferMoveSetting(nameof(EditorSettingsService_OnDefaultTabIndentsChanged),
            () => IndentationChanged?.Invoke(this, System.EventArgs.Empty)));
    }

    private async void EditorSettingsService_OnFontSizeChanged(object sender, int fontSize)
    {
        if (_disposed) return;
        await Dispatcher.CallOnUIThreadAsync(() =>
        {
            if (_disposed) return;
            ApplyOrDeferMoveSetting(nameof(EditorSettingsService_OnFontSizeChanged), () => FontSize = fontSize);
        });
    }

    private async void EditorSettingsService_OnFontStyleChanged(object sender, FontSlant fontStyle)
    {
        if (_disposed) return;
        await Dispatcher.CallOnUIThreadAsync(() =>
        {
            if (_disposed) return;
            ApplyOrDeferMoveSetting(nameof(EditorSettingsService_OnFontStyleChanged), () =>
            {
                FontStyle = fontStyle.ToFontStyle();
                ApplyFont();
            });
        });
    }

    private async void EditorSettingsService_OnFontWeightChanged(object sender, ushort fontWeight)
    {
        if (_disposed) return;
        await Dispatcher.CallOnUIThreadAsync(() =>
        {
            if (_disposed) return;
            ApplyOrDeferMoveSetting(nameof(EditorSettingsService_OnFontWeightChanged), () =>
            {
                FontWeight = fontWeight.ToFontWeight();
                ApplyFont();
            });
        });
    }

    private async void EditorSettingsService_OnFontRenderingChanged(object sender, FontRenderingMode rendering)
    {
        if (_disposed) return;
        await Dispatcher.CallOnUIThreadAsync(() =>
        {
            // Read the latest preference if several queued changes coalesce.
            ApplyOrDeferMoveSetting(nameof(EditorSettingsService_OnFontRenderingChanged), () => ApplyTheme());
        });
    }

    private async void EditorSettingsService_OnDefaultTextWrappingChanged(object sender, bool textWrapping)
    {
        if (_disposed) return;
        await Dispatcher.CallOnUIThreadAsync(() =>
        {
            if (_disposed) return;
            ApplyOrDeferMoveSetting(nameof(EditorSettingsService_OnDefaultTextWrappingChanged), () => TextWrapping = textWrapping.ToTextWrapping());
        });
    }

    private async void EditorSettingsService_OnDefaultDisplayLineNumbersViewStateChanged(object sender, bool displayLineNumbers)
    {
        if (_disposed) return;
        await Dispatcher.CallOnUIThreadAsync(() =>
        {
            if (_disposed) return;
            ApplyOrDeferMoveSetting(nameof(EditorSettingsService_OnDefaultDisplayLineNumbersViewStateChanged), () => DisplayLineNumbers = displayLineNumbers);
        });
    }

    private async void EditorSettingsService_OnDefaultLineHighlighterViewStateChanged(object sender, bool displayLineHighlighter)
    {
        if (_disposed) return;
        await Dispatcher.CallOnUIThreadAsync(() =>
        {
            if (_disposed) return;
            ApplyOrDeferMoveSetting(nameof(EditorSettingsService_OnDefaultLineHighlighterViewStateChanged), () => DisplayLineHighlighter = displayLineHighlighter);
        });
    }

    private async void ThemeSettingsService_OnAccentColorChanged(object sender, Color color)
    {
        if (_disposed) return;
        await Dispatcher.CallOnUIThreadAsync(() =>
        {
            if (_disposed) return;
            // A contrast-scheme switch also arrives here: refresh every system color, not only the selection.
            ApplyOrDeferMoveSetting(nameof(ThemeSettingsService_OnAccentColorChanged), () =>
            {
                if (_accessibility.HighContrast) ApplyTheme();
                else ApplySelectionAppearance();
            });
        });
    }
}
