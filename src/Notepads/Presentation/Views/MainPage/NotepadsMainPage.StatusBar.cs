// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2019-2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Collections.Generic;
using System.Globalization;
using System.IO;
using System.Linq;
using System.Text;
using Notepads.Features.Documents.Contracts;
using Notepads.Features.Documents.Text;
using Notepads.Features.Preferences;
using Notepads.Infrastructure.Diagnostics;
using Notepads.Infrastructure.Storage;
using Notepads.Presentation.Controls.Dialog;
using Notepads.Presentation.Controls.TextEditor;
using Notepads.Presentation.Helpers;
using Notepads.Presentation.Workspace;
using Windows.ApplicationModel.DataTransfer;
using Windows.System;
using Windows.UI.ViewManagement;
using Windows.UI.Xaml;
using Windows.UI.Xaml.Automation;
using Windows.UI.Xaml.Controls;
using Windows.UI.Xaml.Controls.Primitives;
using Windows.UI.Xaml.Input;

namespace Notepads.Presentation.Views.MainPage;

public sealed partial class NotepadsMainPage
{
    private void SetStatusBarInteractive(bool enabled)
    {
        StatusBar.IsHitTestVisible = enabled;
        FileModificationStateIndicator.IsEnabled = PathIndicatorButton.IsEnabled =
            ModificationIndicatorButton.IsEnabled = LineColumnIndicatorButton.IsEnabled =
            FontZoomIndicatorButton.IsEnabled = LineEndingIndicatorButton.IsEnabled =
            EncodingIndicatorButton.IsEnabled = LanguageIndicatorButton.IsEnabled =
            ShadowWindowIndicator.IsEnabled = enabled;
    }

    private void InitializeStatusBar()
    {
        ShowHideStatusBar(ApplicationPreferences.ShowStatusBar);
    }

    private void SetupStatusBar(ITextEditor textEditor)
    {
        if (textEditor == null) return;
        if (StatusBar != null)
        {
            SetStatusBarInteractive(true);
            LineColumnIndicator.Visibility = FontZoomIndicator.Visibility = LineEndingIndicator.Visibility =
                EncodingIndicator.Visibility = LanguageIndicator.Visibility = Visibility.Visible;
        }
        UpdateApplicationTitle(textEditor);
        UpdateFileModificationStateIndicator(textEditor);
        UpdatePathIndicator(textEditor);
        UpdateEditorModificationIndicator(textEditor);
        UpdateLineColumnIndicator(textEditor);
        UpdateFontZoomIndicator(textEditor);
        UpdateLineEndingIndicator(textEditor.GetLineEnding());
        UpdateEncodingIndicator(textEditor.GetEncoding());
        UpdateShadowWindowIndicator();
        UpdateLanguageIndicator(textEditor);
    }

    private void SetupSelectedTabStatus()
    {
        var tab = NotepadsCore.GetSelectedTab();
        if (tab?.ReadyEditor is ITextEditor editor)
        {
            SetupStatusBar(editor);
            return;
        }
        ApplicationView.GetForCurrentView().Title = tab?.Title ?? string.Empty;
        if (StatusBar == null) return;
        SetStatusBarInteractive(false);
        PathIndicator.Text = tab?.EditingFilePath ?? tab?.Title ?? string.Empty;
        ToolTipService.SetToolTip(PathIndicator, PathIndicator.Text);
        FileModificationStateIndicator.Visibility = Visibility.Collapsed;
        ModificationIndicator.Text = tab?.IsModified == true ? _resourceLoader.GetString("TextEditor_ModificationIndicator_Text") : string.Empty;
        ModificationIndicator.Visibility = tab?.IsModified == true ? Visibility.Visible : Visibility.Collapsed;
        LineColumnIndicator.Text = tab?.State == WorkspaceTabState.Failed ?
            _resourceLoader.GetString("WorkspaceTab_LoadFailed") : string.Empty;
        LineColumnIndicator.Visibility = tab?.State == WorkspaceTabState.Failed ? Visibility.Visible : Visibility.Collapsed;
        FontZoomIndicator.Visibility = LineEndingIndicator.Visibility =
            EncodingIndicator.Visibility = LanguageIndicator.Visibility = Visibility.Collapsed;
        UpdateShadowWindowIndicator();
    }

    public void ShowHideStatusBar(bool showStatusBar)
    {
        if (showStatusBar)
        {
            if (StatusBar == null)
            {
                FindName("StatusBar");
            } // Lazy loading

            SetupSelectedTabStatus();
        }
        else
        {
            if (StatusBar != null)
            {
                // If VS cannot find UnloadObject, ignore it. Reference: https://github.com/MicrosoftDocs/windows-uwp/issues/734
                UnloadObject(StatusBar);
            }
        }
    }

    private async void OnStatusBarVisibilityChanged(object sender, bool visible)
    {
        if (!_viewEventsBound) return;
        await Dispatcher.CallOnUIThreadAsync(() =>
        {
            if (!_viewEventsBound) return;
            if (ApplicationView.GetForCurrentView().ViewMode != ApplicationViewMode.CompactOverlay) ShowHideStatusBar(visible);
        });
    }

    private void UpdateFileModificationStateIndicator(ITextEditor textEditor)
    {
        if (StatusBar == null) return;
        if (textEditor.FileModificationState == FileModificationState.Untouched)
        {
            FileModificationStateIndicatorIcon.Glyph = "";
            FileModificationStateIndicator.Visibility = Visibility.Collapsed;
        }
        else if (textEditor.FileModificationState == FileModificationState.Modified)
        {
            FileModificationStateIndicatorIcon.Glyph = "\uE7BA"; // Warning Icon
            ToolTipService.SetToolTip(FileModificationStateIndicator, _resourceLoader.GetString("TextEditor_FileModifiedOutsideIndicator_ToolTip"));
            AutomationProperties.SetName(FileModificationStateIndicator, _resourceLoader.GetString("TextEditor_FileModifiedOutsideIndicator_ToolTip"));
            FileModificationStateIndicator.Visibility = Visibility.Visible;
        }
        else if (textEditor.FileModificationState == FileModificationState.RenamedMovedOrDeleted)
        {
            FileModificationStateIndicatorIcon.Glyph = "\uE9CE"; // Unknown Icon
            ToolTipService.SetToolTip(FileModificationStateIndicator, _resourceLoader.GetString("TextEditor_FileRenamedMovedOrDeletedIndicator_ToolTip"));
            AutomationProperties.SetName(FileModificationStateIndicator, _resourceLoader.GetString("TextEditor_FileRenamedMovedOrDeletedIndicator_ToolTip"));
            FileModificationStateIndicator.Visibility = Visibility.Visible;
        }
    }

    private void UpdatePathIndicator(ITextEditor textEditor)
    {
        if (StatusBar == null) return;
        PathIndicator.Text = textEditor.EditingFilePath ?? textEditor.FileNamePlaceholder;

        switch (textEditor.FileModificationState)
        {
            case FileModificationState.Untouched:
                ToolTipService.SetToolTip(PathIndicator, PathIndicator.Text);
                break;
            case FileModificationState.Modified:
                ToolTipService.SetToolTip(PathIndicator, _resourceLoader.GetString("TextEditor_FileModifiedOutsideIndicator_ToolTip"));
                break;
            case FileModificationState.RenamedMovedOrDeleted:
                ToolTipService.SetToolTip(PathIndicator, _resourceLoader.GetString("TextEditor_FileRenamedMovedOrDeletedIndicator_ToolTip"));
                break;
        }
    }

    private void UpdateEditorModificationIndicator(ITextEditor textEditor)
    {
        if (StatusBar == null) return;
        if (textEditor.IsModified)
        {
            ModificationIndicator.Text = _resourceLoader.GetString("TextEditor_ModificationIndicator_Text");
            ModificationIndicator.Visibility = Visibility.Visible;
        }
        else
        {
            ModificationIndicator.Text = string.Empty;
            ModificationIndicator.Visibility = Visibility.Collapsed;
        }
    }

    private void UpdateEncodingIndicator(Encoding encoding)
    {
        if (StatusBar == null) return;
        EncodingIndicator.Text = EncodingCatalog.GetEncodingName(encoding);
    }

    private void UpdateLineEndingIndicator(LineEnding lineEnding)
    {
        if (StatusBar == null) return;
        LineEndingIndicator.Text = LineEndingUtility.GetLineEndingDisplayText(lineEnding);
    }

    private void UpdateLineColumnIndicator(ITextEditor textEditor)
    {
        if (StatusBar == null) return;
        textEditor.GetLineColumnSelection(out var startLineIndex, out _, out var startColumn, out _, out var selectedCount, out _);

        var wordSelected = selectedCount > 1
            ? _resourceLoader.GetString("TextEditor_LineColumnIndicator_FullText_PluralSelectedWord")
            : _resourceLoader.GetString("TextEditor_LineColumnIndicator_FullText_SingularSelectedWord");

        LineColumnIndicator.Text = selectedCount == 0
            ? string.Format(_resourceLoader.GetString("TextEditor_LineColumnIndicator_ShortText"), startLineIndex, startColumn)
            : string.Format(_resourceLoader.GetString("TextEditor_LineColumnIndicator_FullText"), startLineIndex, startColumn,
                selectedCount, wordSelected);
    }

    private void UpdateFontZoomIndicator(ITextEditor textEditor)
    {
        if (StatusBar == null) return;
        var fontZoomFactor = Math.Round(textEditor.GetFontZoomFactor());
        FontZoomIndicator.Text = fontZoomFactor.ToString(CultureInfo.InvariantCulture) + "%";
        FontZoomSlider.Value = fontZoomFactor;
    }

    private void UpdateShadowWindowIndicator()
    {
        if (StatusBar == null) return;
        ShadowWindowIndicator.Visibility = !_context.IsPrimaryInstance ? Visibility.Visible : Visibility.Collapsed;
        if (ShadowWindowIndicator.Visibility == Visibility.Visible)
        {
            ToolTipService.SetToolTip(ShadowWindowIndicator, _resourceLoader.GetString("App_ShadowWindowIndicator_Description"));
            AutomationProperties.SetName(ShadowWindowIndicator, _resourceLoader.GetString("App_ShadowWindowIndicator_Description"));
        }
    }

    private async void ModificationFlyoutSelection_OnClick(object sender, RoutedEventArgs e)
    {
        if (sender is not MenuFlyoutItem item) return;

        var selectedTextEditor = NotepadsCore.GetSelectedReadyEditor();
        if (selectedTextEditor == null) return;

        switch ((string)item.Tag)
        {
            case "PreviewTextChanges":
                // The diff viewer reports its own failures.
                await selectedTextEditor.OpenSideBySideDiffViewerAsync();
                break;
            case "RevertAllChanges":
                var fileName = selectedTextEditor.EditingFileName ?? selectedTextEditor.FileNamePlaceholder;
                var confirmed = false;
                var revertAllChangesConfirmationDialog = new RevertAllChangesConfirmationDialog(
                    fileName,
                    confirmedAction: () => confirmed = true);
                await DialogManager.OpenDialogAsync(revertAllChangesConfirmationDialog, awaitPreviousDialog: true);
                if (confirmed && NotepadsCore.GetTab(selectedTextEditor) != null)
                {
                    try { await selectedTextEditor.RevertAllChangesAsync(); }
                    catch (Exception ex)
                    {
                        var errorDialog = new FileOpenErrorDialog(selectedTextEditor.EditingFilePath, ex.Message);
                        await DialogManager.OpenDialogAsync(errorDialog, awaitPreviousDialog: false);
                    }
                }
                break;
        }
    }

    private async void ReloadFileFromDiskAsync(object sender, RoutedEventArgs e)
    {
        var selectedEditor = NotepadsCore.GetSelectedReadyEditor();

        if (selectedEditor?.EditingFile != null &&
            selectedEditor.FileModificationState != FileModificationState.RenamedMovedOrDeleted)
        {
            try
            {
                await selectedEditor.ReloadFromEditingFileAsync();
                NotificationCenter.Instance.PostNotification(_resourceLoader.GetString("TextEditor_NotificationMsg_FileReloaded"), 1500);
            }
            catch (Exception ex)
            {
                if (NotepadsCore.GetTab(selectedEditor) == null) return;
                var fileOpenErrorDialog = new FileOpenErrorDialog(selectedEditor.EditingFilePath, ex.Message);
                await DialogManager.OpenDialogAsync(fileOpenErrorDialog, awaitPreviousDialog: false);
                if (!fileOpenErrorDialog.IsAborted)
                {
                    NotepadsCore.FocusOnSelectedTextEditor();
                }
            }
        }
    }

    private void CopyFullPath(object sender, RoutedEventArgs e)
    {
        var selectedEditor = NotepadsCore.GetSelectedReadyEditor();
        if (selectedEditor?.EditingFile == null) return;

        try
        {
            DataPackage dataPackage = new DataPackage { RequestedOperation = DataPackageOperation.Copy };
            dataPackage.SetText(selectedEditor.EditingFile.Path);
            Clipboard.SetContentWithOptions(dataPackage, new ClipboardContentOptions() { IsAllowedInHistory = true, IsRoamable = true });
            Clipboard.Flush(); // This method allows the content to remain available after the application shuts down.
            NotificationCenter.Instance.PostNotification(_resourceLoader.GetString("TextEditor_NotificationMsg_FileNameOrPathCopied"), 1500);
        }
        catch (Exception ex)
        {
            LoggingService.LogError($"[{nameof(NotepadsMainPage)}] Failed to copy full path: {ex.Message}");
        }
    }

    private async void OpenContainingFolderAsync(object sender, RoutedEventArgs e)
    {
        var selectedEditor = NotepadsCore.GetSelectedReadyEditor();
        if (selectedEditor?.EditingFile == null) return;

        try
        {
            await Launcher.LaunchFolderPathAsync(Path.GetDirectoryName(selectedEditor?.EditingFile.Path));
        }
        catch (Exception ex)
        {
            LoggingService.LogError($"[{nameof(NotepadsMainPage)}] Failed to open containing folder: {ex.Message}");
        }
    }

    private async void RenameFileAsync(object sender, RoutedEventArgs e)
    {
        var selectedEditor = NotepadsCore.GetSelectedReadyEditor();
        if (selectedEditor?.EditingFile == null) return;
        await RenameFileAsync(selectedEditor);
    }

    private void FontZoomIndicatorFlyoutSelection_OnClick(object sender, RoutedEventArgs e)
    {
        if (sender is not AppBarButton button) return;

        var selectedTextEditor = NotepadsCore.GetSelectedReadyEditor();
        if (selectedTextEditor == null) return;

        switch (button.Name)
        {
            case "ZoomIn":
                selectedTextEditor.SetFontZoomFactor(FontZoomSlider.Value % 10 > 0
                    ? Math.Ceiling(FontZoomSlider.Value / 10) * 10
                    : FontZoomSlider.Value + 10);
                break;
            case "ZoomOut":
                selectedTextEditor.SetFontZoomFactor(FontZoomSlider.Value % 10 > 0
                    ? Math.Floor(FontZoomSlider.Value / 10) * 10
                    : FontZoomSlider.Value - 10);
                break;
            case "RestoreDefaultZoom":
                selectedTextEditor.SetFontZoomFactor(100);
                FontZoomIndicatorFlyout.Hide();
                break;
        }
    }

    private void FontZoomSlider_ValueChanged(object sender, RangeBaseValueChangedEventArgs e)
    {
        if (sender is not Slider) return;

        var selectedTextEditor = NotepadsCore.GetSelectedReadyEditor();
        if (selectedTextEditor == null) return;

        if (Math.Abs(e.NewValue - e.OldValue) > 0.1)
        {
            selectedTextEditor.SetFontZoomFactor(e.NewValue);
        }
    }

    private void LineEndingSelection_OnClick(object sender, RoutedEventArgs e)
    {
        if (sender is not MenuFlyoutItem item) return;

        var lineEnding = LineEndingUtility.GetLineEndingByName((string)item.Tag);
        var textEditor = NotepadsCore.GetSelectedReadyEditor();
        if (textEditor != null)
        {
            NotepadsCore.ChangeLineEnding(textEditor, lineEnding);
        }
    }

    private void StatusBarComponent_OnClick(object sender, RoutedEventArgs e)
    {
        var selectedEditor = NotepadsCore.GetSelectedReadyEditor();
        if (selectedEditor == null) return;

        if (ReferenceEquals(sender, FileModificationStateIndicator))
        {
            FileModificationStateIndicatorClicked(selectedEditor);
        }
        else if (ReferenceEquals(sender, PathIndicatorButton) && !string.IsNullOrEmpty(PathIndicator.Text))
        {
            PathIndicatorClicked(selectedEditor);
        }
        else if (ReferenceEquals(sender, ModificationIndicatorButton))
        {
            ModificationIndicatorClicked(selectedEditor);
        }
        else if (ReferenceEquals(sender, LineColumnIndicatorButton))
        {
            LineColumnIndicatorClicked(selectedEditor);
        }
        else if (ReferenceEquals(sender, FontZoomIndicatorButton))
        {
            FontZoomIndicatorClicked(selectedEditor);
        }
        else if (ReferenceEquals(sender, LineEndingIndicatorButton))
        {
            LineEndingIndicatorClicked(selectedEditor);
        }
        else if (ReferenceEquals(sender, EncodingIndicatorButton))
        {
            EncodingIndicatorClicked();
        }
        else if (ReferenceEquals(sender, ShadowWindowIndicator))
        {
            ShadowWindowIndicatorClicked();
        }
        else if (ReferenceEquals(sender, LanguageIndicatorButton) && selectedEditor.CanChangeLanguage)
        {
            LanguageSelectionFlyout.ShowAt(LanguageIndicatorButton);
        }
    }

    private void FileModificationStateIndicatorClicked(ITextEditor selectedEditor)
    {
        if (selectedEditor.FileModificationState == FileModificationState.Modified)
        {
            FileModificationStateIndicator.ContextFlyout.ShowAt(FileModificationStateIndicator);
        }
        else if (selectedEditor.FileModificationState == FileModificationState.RenamedMovedOrDeleted)
        {
            NotificationCenter.Instance.PostNotification(_resourceLoader.GetString("TextEditor_FileRenamedMovedOrDeletedIndicator_ToolTip"), 2500);
        }
    }

    private async void PathIndicatorClicked(ITextEditor selectedEditor)
    {
        NotepadsCore.FocusOnSelectedTextEditor();

        PathIndicatorFlyoutCopyFullPathFlyoutItem.Text = _resourceLoader.GetString("Tab_ContextFlyout_CopyFullPathButtonDisplayText");
        PathIndicatorFlyoutOpenContainingFolderFlyoutItem.Text = _resourceLoader.GetString("Tab_ContextFlyout_OpenContainingFolderButtonDisplayText");
        PathIndicatorFlyoutFileRenameFlyoutItem.Text = _resourceLoader.GetString("Tab_ContextFlyout_RenameButtonDisplayText");

        if (selectedEditor.FileModificationState == FileModificationState.RenamedMovedOrDeleted ||
            (selectedEditor.EditingFile != null && FileStorage.IsFileReadOnly(selectedEditor.EditingFile)))
        {
            PathIndicatorFlyoutFileRenameFlyoutItem.IsEnabled = false;
        }
        else
        {
            PathIndicatorFlyoutFileRenameFlyoutItem.IsEnabled = true;
        }

        if (selectedEditor.FileModificationState == FileModificationState.Untouched)
        {
            if (selectedEditor.EditingFile != null)
            {
                PathIndicatorButton.ContextFlyout.ShowAt(PathIndicatorButton);
            }
            else
            {
                await RenameFileAsync(selectedEditor);
            }
        }
        else if (selectedEditor.FileModificationState == FileModificationState.Modified)
        {
            PathIndicatorButton.ContextFlyout.ShowAt(PathIndicatorButton);
        }
        else if (selectedEditor.FileModificationState == FileModificationState.RenamedMovedOrDeleted)
        {
            NotificationCenter.Instance.PostNotification(_resourceLoader.GetString("TextEditor_FileRenamedMovedOrDeletedIndicator_ToolTip"), 2500);
        }
    }

    private void ModificationIndicatorClicked(ITextEditor selectedEditor)
    {
        PreviewTextChangesFlyoutItem.IsEnabled = selectedEditor.Mode != TextEditorMode.DiffPreview;
        ModificationIndicatorButton?.ContextFlyout.ShowAt(ModificationIndicatorButton);
    }

    private async void DiffPreviewAccelerator_OnInvoked(KeyboardAccelerator sender, KeyboardAcceleratorInvokedEventArgs args)
    {
        args.Handled = true;
        var editor = NotepadsCore.GetSelectedReadyEditor();
        if (editor == null) return;
        try { await editor.ToggleDiffPreviewAsync(); }
        catch (Exception exception)
        {
            LoggingService.LogException(exception);
            NotificationCenter.Instance.PostNotification(_resourceLoader.GetString("DiffViewer_Unavailable"), 5000);
        }
    }

    private static void LineColumnIndicatorClicked(ITextEditor selectedEditor)
    {
        selectedEditor.ShowGoToControl();
    }

    private void FontZoomIndicatorClicked(ITextEditor _)
    {
        FontZoomIndicatorButton?.ContextFlyout.ShowAt(FontZoomIndicatorButton);
        FontZoomIndicatorFlyout.Opened += (sflyout, eflyout) => ToolTipService.SetToolTip(RestoreDefaultZoom, null);
    }

    private void LineEndingIndicatorClicked(ITextEditor _)
    {
        LineEndingIndicatorButton?.ContextFlyout.ShowAt(LineEndingIndicatorButton);
    }

    private void EncodingIndicatorClicked()
    {
        EncodingIndicatorButton?.ContextFlyout.ShowAt(EncodingIndicatorButton);
    }

    private void EncodingSelectionFlyout_OnOpening(object sender, object args)
    {
        BuildEncodingIndicatorFlyout();
        var selectedEditor = NotepadsCore.GetSelectedReadyEditor();
        var reopenWithEncoding = EncodingSelectionFlyout?.Items?.FirstOrDefault(i => i.Name.Equals("ReopenWithEncoding"));
        if (reopenWithEncoding != null)
        {
            reopenWithEncoding.IsEnabled = selectedEditor?.EditingFile != null &&
                                           selectedEditor.FileModificationState !=
                                           FileModificationState.RenamedMovedOrDeleted;
        }
    }

    private void ShadowWindowIndicatorClicked()
    {
        NotificationCenter.Instance.PostNotification(_resourceLoader.GetString("App_ShadowWindowIndicator_Description"), 4000);
    }

    private void StatusBarFlyout_OnClosing(FlyoutBase sender, FlyoutBaseClosingEventArgs args)
    {
        NotepadsCore.FocusOnSelectedTextEditor();
    }

    private void BuildEncodingIndicatorFlyout()
    {
        if (StatusBar == null) return;

        if (EncodingSelectionFlyout.Items?.Count > 0)
        {
            return;
        }

        var reopenWithEncoding = new MenuFlyoutSubItem()
        {
            Text = _resourceLoader.GetString("TextEditor_EncodingIndicator_FlyoutItem_ReopenWithEncoding"),
            FlowDirection = FlowDirection.RightToLeft,
            Name = "ReopenWithEncoding"
        };

        var saveWithEncoding = new MenuFlyoutSubItem()
        {
            Text = _resourceLoader.GetString("TextEditor_EncodingIndicator_FlyoutItem_SaveWithEncoding"),
            FlowDirection = FlowDirection.RightToLeft,
            Name = "SaveWithEncoding"
        };

        // Add auto guess Encoding option in ReopenWithEncoding menu
        reopenWithEncoding.Items?.Add(CreateAutoGuessEncodingItem());
        reopenWithEncoding.Items?.Add(new MenuFlyoutSeparator());

        // Add suggested ANSI encodings
        var appAndSystemANSIEncodings = new HashSet<Encoding>();

        if (EncodingCatalog.TryGetSystemDefaultANSIEncoding(out var systemDefaultANSIEncoding))
        {
            appAndSystemANSIEncodings.Add(systemDefaultANSIEncoding);
        }
        if (EncodingCatalog.TryGetCurrentCultureANSIEncoding(out var currentCultureANSIEncoding))
        {
            appAndSystemANSIEncodings.Add(currentCultureANSIEncoding);
        }

        if (appAndSystemANSIEncodings.Count > 0)
        {
            foreach (var encoding in appAndSystemANSIEncodings)
            {
                AddEncodingItem(encoding, reopenWithEncoding, saveWithEncoding);
            }
            reopenWithEncoding.Items?.Add(new MenuFlyoutSeparator());
            saveWithEncoding.Items?.Add(new MenuFlyoutSeparator());
        }

        // Add Unicode encodings
        var unicodeEncodings = new List<Encoding>
        {
            new UTF8Encoding(false), // "UTF-8"
            new UTF8Encoding(true), // "UTF-8-BOM"
            new UnicodeEncoding(false, true), // "UTF-16 LE BOM"
            new UnicodeEncoding(true, true), // "UTF-16 BE BOM"
        };

        foreach (var encoding in unicodeEncodings)
        {
            AddEncodingItem(encoding, reopenWithEncoding, saveWithEncoding);
        }

        // Add legacy ANSI encodings
        var ANSIEncodings = EncodingCatalog.GetAllSupportedANSIEncodings();
        if (ANSIEncodings.Length > 0)
        {
            reopenWithEncoding.Items?.Add(new MenuFlyoutSeparator());
            saveWithEncoding.Items?.Add(new MenuFlyoutSeparator());

            var reopenWithEncodingOthers = new MenuFlyoutSubItem()
            {
                Text = _resourceLoader.GetString("TextEditor_EncodingIndicator_FlyoutItem_MoreEncodings"),
                FlowDirection = FlowDirection.RightToLeft,
            };

            var saveWithEncodingOthers = new MenuFlyoutSubItem()
            {
                Text = _resourceLoader.GetString("TextEditor_EncodingIndicator_FlyoutItem_MoreEncodings"),
                FlowDirection = FlowDirection.RightToLeft,
            };

            foreach (var encoding in ANSIEncodings)
            {
                AddEncodingItem(encoding, reopenWithEncodingOthers, saveWithEncodingOthers);
            }

            reopenWithEncoding.Items?.Add(reopenWithEncodingOthers);
            saveWithEncoding.Items?.Add(saveWithEncodingOthers);
        }

        EncodingSelectionFlyout.Items?.Add(reopenWithEncoding);
        EncodingSelectionFlyout.Items?.Add(saveWithEncoding);
    }

    private MenuFlyoutItem CreateAutoGuessEncodingItem()
    {
        var autoGuessEncodingItem = new MenuFlyoutItem()
        {
            Text = _resourceLoader.GetString("TextEditor_EncodingIndicator_FlyoutItem_AutoGuessEncoding"),
            FlowDirection = FlowDirection.LeftToRight,
        };
        autoGuessEncodingItem.Click += async (sender, args) =>
        {
            var selectedTextEditor = NotepadsCore.GetSelectedReadyEditor();
            var file = selectedTextEditor?.EditingFile;
            if (file == null || selectedTextEditor.FileModificationState == FileModificationState.RenamedMovedOrDeleted) return;

            try
            {
                await selectedTextEditor.ReloadFromEditingFileAsync(decodingMode: DocumentDecodingMode.AutoDetect);
                if (NotepadsCore.GetTab(selectedTextEditor) == null) return;
                NotificationCenter.Instance.PostNotification(_resourceLoader.GetString("TextEditor_NotificationMsg_FileReloaded"), 1500);
            }
            catch (Exception ex)
            {
                if (NotepadsCore.GetTab(selectedTextEditor) == null) return;
                var fileOpenErrorDialog = new FileOpenErrorDialog(selectedTextEditor.EditingFilePath, ex.Message);
                await DialogManager.OpenDialogAsync(fileOpenErrorDialog, awaitPreviousDialog: false);
                if (!fileOpenErrorDialog.IsAborted)
                {
                    NotepadsCore.FocusOnSelectedTextEditor();
                }
            }
        };
        return autoGuessEncodingItem;
    }

    private void AddEncodingItem(Encoding encoding, MenuFlyoutSubItem reopenWithEncoding, MenuFlyoutSubItem saveWithEncoding)
    {
        const int EncodingMenuFlyoutItemHeight = 30;
        const int EncodingMenuFlyoutItemFontSize = 14;

        var reopenWithEncodingItem = new MenuFlyoutItem()
        {
            Text = EncodingCatalog.GetEncodingName(encoding),
            FlowDirection = FlowDirection.LeftToRight,
            Height = EncodingMenuFlyoutItemHeight,
            FontSize = EncodingMenuFlyoutItemFontSize
        };
        reopenWithEncodingItem.Click += async (sender, args) =>
        {
            var selectedTextEditor = NotepadsCore.GetSelectedReadyEditor();
            if (selectedTextEditor != null)
            {
                try
                {
                    await selectedTextEditor.ReloadFromEditingFileAsync(encoding);
                    if (NotepadsCore.GetTab(selectedTextEditor) == null) return;
                    NotificationCenter.Instance.PostNotification(_resourceLoader.GetString("TextEditor_NotificationMsg_FileReloaded"), 1500);
                }
                catch (Exception ex)
                {
                    if (NotepadsCore.GetTab(selectedTextEditor) == null) return;
                    var fileOpenErrorDialog = new FileOpenErrorDialog(selectedTextEditor.EditingFilePath, ex.Message);
                    await DialogManager.OpenDialogAsync(fileOpenErrorDialog, awaitPreviousDialog: false);
                    if (!fileOpenErrorDialog.IsAborted)
                    {
                        NotepadsCore.FocusOnSelectedTextEditor();
                    }
                }
            }
        };
        reopenWithEncoding.Items?.Add(reopenWithEncodingItem);

        var saveWithEncodingItem = new MenuFlyoutItem()
        {
            Text = EncodingCatalog.GetEncodingName(encoding),
            FlowDirection = FlowDirection.LeftToRight,
            Height = EncodingMenuFlyoutItemHeight,
            FontSize = EncodingMenuFlyoutItemFontSize
        };
        saveWithEncodingItem.Click += (sender, args) =>
        {
            NotepadsCore.GetSelectedReadyEditor()?.TryChangeEncoding(encoding);
        };
        saveWithEncoding.Items?.Add(saveWithEncodingItem);
    }
}
