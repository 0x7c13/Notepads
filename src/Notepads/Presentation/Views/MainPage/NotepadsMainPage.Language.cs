// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

using Notepads.Features.Documents.FileTypes;
using Notepads.Presentation.Controls.TextEditor;
using Windows.UI.Xaml;
using Windows.UI.Xaml.Automation;
using Windows.UI.Xaml.Controls;
using Windows.UI.Xaml.Input;
using WinUIEditor;

namespace Notepads.Presentation.Views.MainPage;

public sealed partial class NotepadsMainPage
{
    private ITextEditor _languageFlyoutEditor;
    private void LanguageIndicator_OnContextRequested(UIElement sender, ContextRequestedEventArgs args)
    {
        args.Handled = true;
        if (NotepadsCore.GetSelectedTextEditor()?.CanChangeLanguage == true)
            LanguageSelectionFlyout.ShowAt(LanguageIndicator);
    }

    private void LanguageSelectionFlyout_OnOpening(object sender, object args)
    {
        LanguageSelectionFlyout.Items.Clear();
        _languageFlyoutEditor = null;
        var editor = NotepadsCore.GetSelectedTextEditor();
        if (editor?.CanChangeLanguage == true) BuildLanguageFlyout(editor);
    }

    private void LanguageSelectionFlyout_OnClosed(object sender, object args) => _languageFlyoutEditor = null;
    private string GetLanguageLabel(DocumentLanguage language) => language.Id == "plaintext"
        ? _resourceLoader.GetString("TextEditor_Language_PlainText") : language.DisplayName;

    private void UpdateLanguageIndicator(ITextEditor editor)
    {
        if (StatusBar == null) return;
        var label = GetLanguageLabel(editor.DocumentLanguage);
        LanguageIndicator.Text = label;
        var description = _resourceLoader.GetString("TextEditor_Language_Select") + ": " + label;
        if (editor.SyntaxHighlightingPauseReason != EditorSyntaxPauseReason.None)
            description += ". " + _resourceLoader.GetString(editor.SyntaxHighlightingPauseReason == EditorSyntaxPauseReason.Memory
                ? "TextEditor_Language_MemoryPaused" : "TextEditor_Language_HighlightingPaused");
        ToolTipService.SetToolTip(LanguageIndicator, description);
        AutomationProperties.SetName(LanguageIndicator, description);
        LanguageIndicator.Opacity = editor.SyntaxHighlightingPauseReason != EditorSyntaxPauseReason.None ? 0.65 : 1;
    }

    private void BuildLanguageFlyout(ITextEditor editor)
    {
        _languageFlyoutEditor = editor;
        AddLanguageItem(_resourceLoader.GetString("TextEditor_Language_Automatic") + " (" +
            GetLanguageLabel(editor.DetectedLanguage) + ")", null, editor);
        LanguageSelectionFlyout.Items.Add(new MenuFlyoutSeparator());
        foreach (var language in DocumentLanguages.All) AddLanguageItem(GetLanguageLabel(language), language.Id, editor);
    }

    private void AddLanguageItem(string label, string id, ITextEditor editor)
    {
        var item = new MenuFlyoutItem { Text = label, Tag = id };
        if (editor.LanguageOverride == id) item.Icon = new SymbolIcon(Symbol.Accept);
        item.Click += LanguageSelection_OnClick;
        LanguageSelectionFlyout.Items.Add(item);
    }

    private void LanguageSelection_OnClick(object sender, RoutedEventArgs args)
    {
        var editor = NotepadsCore.GetSelectedTextEditor();
        if (ReferenceEquals(editor, _languageFlyoutEditor) && editor?.CanChangeLanguage == true && sender is MenuFlyoutItem item)
            editor.SetLanguageOverride(item.Tag as string);
    }
}
