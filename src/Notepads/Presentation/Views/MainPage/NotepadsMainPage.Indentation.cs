// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Globalization;
using Notepads.Infrastructure.Diagnostics;
using Notepads.Presentation.Controls.TextEditor;
using Notepads.Presentation.Workspace;
using Windows.UI.Xaml;
using Windows.UI.Xaml.Automation;
using Windows.UI.Xaml.Controls;
using Windows.UI.Xaml.Input;

namespace Notepads.Presentation.Views.MainPage;

public sealed partial class NotepadsMainPage
{
    private ITextEditor _indentationFlyoutEditor;

    private string IndentationLabel(int value) => value <= 0
        ? _resourceLoader.GetString("TextEditor_Indentation_Tabs")
        : string.Format(CultureInfo.CurrentCulture, _resourceLoader.GetString("TextEditor_Indentation_Spaces"), value);

    private void UpdateIndentationIndicator(ITextEditor editor)
    {
        if (StatusBar == null) return;
        var label = IndentationLabel(editor.EffectiveIndentation);
        IndentationIndicator.Text = label;
        var description = _resourceLoader.GetString("TextEditor_Indentation_Select") + ": " + label;
        if (editor.IndentationOverride == 0) description += " (" + _resourceLoader.GetString("TextEditor_Language_Automatic") + ")";
        ToolTipService.SetToolTip(IndentationIndicator, description);
        AutomationProperties.SetName(IndentationIndicator, description);
    }

    private void IndentationIndicator_OnContextRequested(UIElement sender, ContextRequestedEventArgs args)
    {
        args.Handled = true;
        if (NotepadsCore.GetSelectedReadyEditor()?.CanChangeIndentation == true)
            IndentationSelectionFlyout.ShowAt(IndentationIndicatorButton);
    }

    private void IndentationSelectionFlyout_OnClosed(object sender, object args) => _indentationFlyoutEditor = null;

    private void IndentationSelectionFlyout_OnOpening(object sender, object args)
    {
        IndentationSelectionFlyout.Items.Clear();
        _indentationFlyoutEditor = NotepadsCore.GetSelectedReadyEditor();
        var editor = _indentationFlyoutEditor;
        if (editor?.CanChangeIndentation != true) return;
        foreach (var value in new[] { 0, -1, 2, 4, 8 })
        {
            var item = new MenuFlyoutItem
            {
                Text = value == 0 ? _resourceLoader.GetString("TextEditor_Language_Automatic") : IndentationLabel(value),
                Tag = value,
                IsEnabled = value != -1 || editor.DocumentLanguage.Id != "yaml"
            };
            if (editor.IndentationOverride == value) item.Icon = new SymbolIcon(Symbol.Accept);
            item.Click += IndentationSelection_OnClick;
            IndentationSelectionFlyout.Items.Add(item);
        }
        IndentationSelectionFlyout.Items.Add(new MenuFlyoutSeparator());
        foreach (var tabs in new[] { false, true })
        {
            var item = new MenuFlyoutItem
            {
                Text = _resourceLoader.GetString(tabs ? "TextEditor_Indentation_ConvertTabs" : "TextEditor_Indentation_ConvertSpaces"),
                Tag = tabs,
                IsEnabled = editor.CanConvertIndentation && (!tabs || editor.DocumentLanguage.Id != "yaml")
            };
            item.Click += IndentationSelection_OnClick;
            IndentationSelectionFlyout.Items.Add(item);
        }
    }

    private async void IndentationSelection_OnClick(object sender, RoutedEventArgs args)
    {
        var editor = NotepadsCore.GetSelectedReadyEditor();
        if (!ReferenceEquals(editor, _indentationFlyoutEditor) || editor?.CanChangeIndentation != true || sender is not MenuFlyoutItem item) return;
        try
        {
            if (item.Tag is int value) editor.SetIndentationOverride(value);
            else if (item.Tag is bool useTabs) await editor.ConvertIndentationAsync(useTabs);
            if (ReferenceEquals(editor, NotepadsCore.GetSelectedReadyEditor()))
            {
                UpdateIndentationIndicator(editor);
                NotepadsCore.FocusOnSelectedTextEditor();
            }
        }
        catch (OperationCanceledException) { }
        catch (ObjectDisposedException) { }
        catch (Exception error)
        {
            LoggingService.LogException(error);
            NotificationCenter.Instance.PostNotification(_resourceLoader.GetString("TextEditor_Indentation_Failed"), 4000);
        }
    }
}
