namespace Notepads
{
    using System;
    using System.Text;
    using Windows.UI.Xaml;
    using Windows.UI.Xaml.Controls;
    using Windows.UI.Xaml.Controls.Primitives;
    using Windows.UI.Xaml.Input;
    using Notepads.Controls.TextEditor;
    using Notepads.Services;
    using Notepads.Utilities;

    public sealed partial class NotepadsMainPage
    {
        private void SetupStatusBar(ITextEditor textEditor)
        {
            if (textEditor == null) return;
            UpdateFileModificationStateIndicator(textEditor);
            UpdatePathIndicator(textEditor);
            UpdateEditorModificationIndicator(textEditor);
            UpdateLineColumnIndicator(textEditor);
            UpdateLineEndingIndicator(textEditor.GetLineEnding());
            UpdateEncodingIndicator(textEditor.GetEncoding());
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

                SetupStatusBar(NotepadsCore.GetSelectedTextEditor());
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
                ToolTipService.SetToolTip(FileModificationStateIndicator,
                    _resourceLoader.GetString("TextEditor_FileModifiedOutsideIndicator_ToolTip"));
                FileModificationStateIndicator.Visibility = Visibility.Visible;
            }
            else if (textEditor.FileModificationState == FileModificationState.RenamedMovedOrDeleted)
            {
                FileModificationStateIndicatorIcon.Glyph = "\uE9CE"; // Unknown Icon
                ToolTipService.SetToolTip(FileModificationStateIndicator,
                    _resourceLoader.GetString("TextEditor_FileRenamedMovedOrDeletedIndicator_ToolTip"));
                FileModificationStateIndicator.Visibility = Visibility.Visible;
            }
        }

        private void UpdatePathIndicator(ITextEditor textEditor)
        {
            if (StatusBar == null) return;
            PathIndicator.Text = textEditor.EditingFilePath ?? textEditor.FileNamePlaceholder;

            if (textEditor.FileModificationState == FileModificationState.Untouched)
            {
                ToolTipService.SetToolTip(PathIndicator, PathIndicator.Text);
            }
            else if (textEditor.FileModificationState == FileModificationState.Modified)
            {
                ToolTipService.SetToolTip(PathIndicator,
                    _resourceLoader.GetString("TextEditor_FileModifiedOutsideIndicator_ToolTip"));
            }
            else if (textEditor.FileModificationState == FileModificationState.RenamedMovedOrDeleted)
            {
                ToolTipService.SetToolTip(PathIndicator,
                    _resourceLoader.GetString("TextEditor_FileRenamedMovedOrDeletedIndicator_ToolTip"));
            }
        }

        private void UpdateEditorModificationIndicator(ITextEditor textEditor)
        {
            if (StatusBar == null) return;
            if (textEditor.IsModified)
            {
                ModificationIndicator.Text = _resourceLoader.GetString("TextEditor_ModificationIndicator_Text");
                ModificationIndicator.Visibility = Visibility.Visible;
                ModificationIndicator.IsTapEnabled = true;
            }
            else
            {
                ModificationIndicator.Text = string.Empty;
                ModificationIndicator.Visibility = Visibility.Collapsed;
                ModificationIndicator.IsTapEnabled = false;
            }
        }

        private void UpdateEncodingIndicator(Encoding encoding)
        {
            if (StatusBar == null) return;
            EncodingIndicator.Text = EncodingUtility.GetEncodingName(encoding);
        }

        private void UpdateLineEndingIndicator(LineEnding lineEnding)
        {
            if (StatusBar == null) return;
            LineEndingIndicator.Text = LineEndingUtility.GetLineEndingDisplayText(lineEnding);
        }

        private void UpdateLineColumnIndicator(ITextEditor textEditor)
        {
            if (StatusBar == null) return;
            textEditor.GetCurrentLineColumn(out var line, out var column, out var selectedCount);

            var wordSelected = selectedCount > 1
                ? _resourceLoader.GetString("TextEditor_LineColumnIndicator_FullText_PluralSelectedWord")
                : _resourceLoader.GetString("TextEditor_LineColumnIndicator_FullText_SingularSelectedWord");

            LineColumnIndicator.Text = selectedCount == 0
                ? string.Format(_resourceLoader.GetString("TextEditor_LineColumnIndicator_ShortText"), line, column)
                : string.Format(_resourceLoader.GetString("TextEditor_LineColumnIndicator_FullText"), line, column,
                    selectedCount, wordSelected);
        }

        private void UpdateShadowWindowIndicator()
        {
            if (StatusBar == null) return;
            ShadowWindowIndicator.Visibility = !App.IsFirstInstance ? Visibility.Visible : Visibility.Collapsed;
            if (ShadowWindowIndicator.Visibility == Visibility.Visible)
            {
                ToolTipService.SetToolTip(ShadowWindowIndicator,
                    _resourceLoader.GetString("App_ShadowWindowIndicator_Description"));
            }
        }

        private async void ModificationFlyoutSelection_OnClick(object sender, RoutedEventArgs e)
        {
            if (!(sender is MenuFlyoutItem item)) return;

            var selectedTextEditor = NotepadsCore.GetSelectedTextEditor();
            if (selectedTextEditor == null) return;

            switch ((string) item.Tag)
            {
                case "PreviewTextChanges":
                    NotepadsCore.GetSelectedTextEditor().OpenSideBySideDiffViewer();
                    break;
                case "RevertAllChanges":
                    var fileName = selectedTextEditor.EditingFileName ?? selectedTextEditor.FileNamePlaceholder;
                    var revertAllChangesConfirmationDialog = NotepadsDialogFactory.GetRevertAllChangesConfirmationDialog(
                        fileName, () =>
                        {
                            selectedTextEditor.CloseSideBySideDiffViewer();
                            NotepadsCore.GetSelectedTextEditor().RevertAllChanges();
                        });
                    await DialogManager.OpenDialogAsync(revertAllChangesConfirmationDialog, awaitPreviousDialog: true);
                    break;
            }
        }

        private async void ReloadFileFromDisk(object sender, RoutedEventArgs e)
        {
            var selectedEditor = NotepadsCore.GetSelectedTextEditor();

            if (selectedEditor?.EditingFile != null &&
                selectedEditor.FileModificationState != FileModificationState.RenamedMovedOrDeleted)
            {
                try
                {
                    await selectedEditor.ReloadFromEditingFile();
                    NotificationCenter.Instance.PostNotification(
                        _resourceLoader.GetString("TextEditor_NotificationMsg_FileReloaded"), 1500);
                }
                catch (Exception ex)
                {
                    var fileOpenErrorDialog = NotepadsDialogFactory.GetFileOpenErrorDialog(selectedEditor.EditingFilePath, ex.Message);
                    await DialogManager.OpenDialogAsync(fileOpenErrorDialog, awaitPreviousDialog: false);
                    if (!fileOpenErrorDialog.IsAborted)
                    {
                        NotepadsCore.FocusOnSelectedTextEditor();
                    }
                }
            }
        }

        private void LineEndingSelection_OnClick(object sender, RoutedEventArgs e)
        {
            if (!(sender is MenuFlyoutItem item)) return;

            var lineEnding = LineEndingUtility.GetLineEndingByName((string) item.Tag);
            var textEditor = NotepadsCore.GetSelectedTextEditor();
            if (textEditor != null)
            {
                NotepadsCore.ChangeLineEnding(textEditor, lineEnding);
            }
        }

        private void EncodingSelection_OnClick(object sender, RoutedEventArgs e)
        {
            if (!(sender is MenuFlyoutItem item)) return;

            var encoding = EncodingUtility.GetEncodingByName((string) item.Tag);
            var textEditor = NotepadsCore.GetSelectedTextEditor();
            if (textEditor != null)
            {
                NotepadsCore.ChangeEncoding(textEditor, encoding);
            }
        }

        private void StatusBarComponent_OnTapped(object sender, TappedRoutedEventArgs e)
        {
            var selectedEditor = NotepadsCore.GetSelectedTextEditor();
            if (selectedEditor == null) return;

            if (sender == FileModificationStateIndicator)
            {
                if (selectedEditor.FileModificationState == FileModificationState.Modified)
                {
                    FileModificationStateIndicator.ContextFlyout.ShowAt(FileModificationStateIndicator);
                }
                else if (selectedEditor.FileModificationState == FileModificationState.RenamedMovedOrDeleted)
                {
                    NotificationCenter.Instance.PostNotification(
                        _resourceLoader.GetString("TextEditor_FileRenamedMovedOrDeletedIndicator_ToolTip"), 2000);
                }
            }
            else if (sender == PathIndicator && !string.IsNullOrEmpty(PathIndicator.Text))
            {
                NotepadsCore.FocusOnSelectedTextEditor();

                if (selectedEditor.FileModificationState == FileModificationState.Untouched)
                {
                    if (selectedEditor.EditingFile != null)
                    {
                        FileModificationStateIndicator.ContextFlyout.ShowAt(FileModificationStateIndicator);
                    }
                }
                else if (selectedEditor.FileModificationState == FileModificationState.Modified)
                {
                    FileModificationStateIndicator.ContextFlyout.ShowAt(FileModificationStateIndicator);
                }
                else if (selectedEditor.FileModificationState == FileModificationState.RenamedMovedOrDeleted)
                {
                    NotificationCenter.Instance.PostNotification(
                        _resourceLoader.GetString("TextEditor_FileRenamedMovedOrDeletedIndicator_ToolTip"), 2000);
                }
            }
            else if (sender == ModificationIndicator)
            {
                PreviewTextChangesFlyoutItem.IsEnabled =
                    !selectedEditor.NoChangesSinceLastSaved(compareTextOnly: true) &&
                    selectedEditor.Mode != TextEditorMode.DiffPreview;
                ModificationIndicator?.ContextFlyout.ShowAt(ModificationIndicator);
            }
            else if (sender == LineColumnIndicator)
            {
            }
            else if (sender == LineEndingIndicator)
            {
                LineEndingIndicator?.ContextFlyout.ShowAt(LineEndingIndicator);
            }
            else if (sender == EncodingIndicator)
            {
                EncodingIndicator?.ContextFlyout.ShowAt(EncodingIndicator);
            }
            else if (sender == ShadowWindowIndicator)
            {
                NotificationCenter.Instance.PostNotification(
                    _resourceLoader.GetString("App_ShadowWindowIndicator_Description"), 4000);
            }
        }

        private void StatusBarFlyout_OnClosing(FlyoutBase sender, FlyoutBaseClosingEventArgs args)
        {
            NotepadsCore.FocusOnSelectedTextEditor();
        }
    }
}