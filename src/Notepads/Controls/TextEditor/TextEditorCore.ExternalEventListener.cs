﻿namespace Notepads.Controls.TextEditor
{
    using Windows.UI;
    using Windows.UI.Text;
    using Windows.UI.Xaml;
    using Windows.UI.Xaml.Media;
    using Notepads.Extensions;
    using Notepads.Services;

    public partial class TextEditorCore
    {
        internal void HookExternalEvents()
        {
            EditorSettingsService.OnFontFamilyChanged += EditorSettingsService_OnFontFamilyChanged;
            EditorSettingsService.OnFontSizeChanged += EditorSettingsService_OnFontSizeChanged;
            EditorSettingsService.OnFontStyleChanged += EditorSettingsService_OnFontStyleChanged;
            EditorSettingsService.OnFontWeightChanged += EditorSettingsService_OnFontWeightChanged;
            EditorSettingsService.OnDefaultTextWrappingChanged += EditorSettingsService_OnDefaultTextWrappingChanged;
            EditorSettingsService.OnHighlightMisspelledWordsChanged += EditorSettingsService_OnHighlightMisspelledWordsChanged;
            EditorSettingsService.OnDefaultDisplayLineNumbersViewStateChanged += EditorSettingsService_OnDefaultDisplayLineNumbersViewStateChanged;
            EditorSettingsService.OnDefaultLineHighlighterViewStateChanged += EditorSettingsService_OnDefaultLineHighlighterViewStateChanged;

            ThemeSettingsService.OnAccentColorChanged += ThemeSettingsService_OnAccentColorChanged;
        }

        internal void UnhookExternalEvents()
        {
            EditorSettingsService.OnFontFamilyChanged -= EditorSettingsService_OnFontFamilyChanged;
            EditorSettingsService.OnFontSizeChanged -= EditorSettingsService_OnFontSizeChanged;
            EditorSettingsService.OnFontStyleChanged -= EditorSettingsService_OnFontStyleChanged;
            EditorSettingsService.OnFontWeightChanged -= EditorSettingsService_OnFontWeightChanged;
            EditorSettingsService.OnDefaultTextWrappingChanged -= EditorSettingsService_OnDefaultTextWrappingChanged;
            EditorSettingsService.OnHighlightMisspelledWordsChanged -= EditorSettingsService_OnHighlightMisspelledWordsChanged;
            EditorSettingsService.OnDefaultDisplayLineNumbersViewStateChanged -= EditorSettingsService_OnDefaultDisplayLineNumbersViewStateChanged;
            EditorSettingsService.OnDefaultLineHighlighterViewStateChanged -= EditorSettingsService_OnDefaultLineHighlighterViewStateChanged;

            ThemeSettingsService.OnAccentColorChanged -= ThemeSettingsService_OnAccentColorChanged;
        }

        private async void EditorSettingsService_OnFontFamilyChanged(object sender, string fontFamily)
        {
            await Dispatcher.CallOnUIThreadAsync(() =>
            {
                FontFamily = new FontFamily(fontFamily);
                SetDefaultTabStopAndLineSpacing(FontFamily, FontSize);
            });
        }

        private async void EditorSettingsService_OnFontSizeChanged(object sender, int fontSize)
        {
            await Dispatcher.CallOnUIThreadAsync(() =>
            {
                FontSize = fontSize;
            });
        }

        private async void EditorSettingsService_OnFontStyleChanged(object sender, FontStyle fontStyle)
        {
            await Dispatcher.CallOnUIThreadAsync(() =>
            {
                FontStyle = fontStyle;
            });
        }

        private async void EditorSettingsService_OnFontWeightChanged(object sender, FontWeight fontWeight)
        {
            await Dispatcher.CallOnUIThreadAsync(() =>
            {
                FontWeight = fontWeight;
            });
        }

        private async void EditorSettingsService_OnDefaultTextWrappingChanged(object sender, TextWrapping textWrapping)
        {
            await Dispatcher.CallOnUIThreadAsync(() =>
            {
                TextWrapping = textWrapping;
            });
        }

        private async void EditorSettingsService_OnHighlightMisspelledWordsChanged(object sender, bool isSpellCheckEnabled)
        {
            await Dispatcher.CallOnUIThreadAsync(() =>
            {
                IsSpellCheckEnabled = isSpellCheckEnabled;
            });
        }

        private async void EditorSettingsService_OnDefaultDisplayLineNumbersViewStateChanged(object sender, bool displayLineNumbers)
        {
            await Dispatcher.CallOnUIThreadAsync(() =>
            {
                DisplayLineNumbers = displayLineNumbers;
            });
        }

        private async void EditorSettingsService_OnDefaultLineHighlighterViewStateChanged(object sender, bool displayLineHighlighter)
        {
            await Dispatcher.CallOnUIThreadAsync(() =>
            {
                DisplayLineHighlighter = displayLineHighlighter;
            });
        }

        private async void ThemeSettingsService_OnAccentColorChanged(object sender, Color color)
        {
            await Dispatcher.CallOnUIThreadAsync(() =>
            {
                SelectionHighlightColor = new SolidColorBrush(color);
                SelectionHighlightColorWhenNotFocused = new SolidColorBrush(color);
            });
        }
    }
}