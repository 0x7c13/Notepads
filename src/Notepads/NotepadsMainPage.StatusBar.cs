namespace Notepads
{
    using System;
    using System.Collections.Generic;
    using System.Globalization;
    using System.IO;
    using System.Linq;
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
            UpdateFontZoomIndicator(textEditor);
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
                    BuildEncodingIndicatorFlyout();
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

        private void FontZoomIndicatorFlyoutSelection_OnClick(object sender, RoutedEventArgs e)
        {
            if (!(sender is AppBarButton button)) return;

            var selectedTextEditor = NotepadsCore.GetSelectedTextEditor();
            if (selectedTextEditor == null) return;

            switch ((string)button.Name)
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
            if (!(sender is Slider)) return;

            var selectedTextEditor = NotepadsCore.GetSelectedTextEditor();
            if (selectedTextEditor == null) return;

            if (Math.Abs(e.NewValue - e.OldValue) > 0.1)
            {
                selectedTextEditor.SetFontZoomFactor(e.NewValue);
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
                selectedEditor.ShowGoToControl();
            }
            else if (sender == FontZoomIndicator)
            {
                FontZoomIndicator?.ContextFlyout.ShowAt(FontZoomIndicator);
                FontZoomIndicatorFlyout.Opened += (sflyout, eflyout) => ToolTipService.SetToolTip(RestoreDefaultZoom, null);
            }
            else if (sender == LineEndingIndicator)
            {
                LineEndingIndicator?.ContextFlyout.ShowAt(LineEndingIndicator);
            }
            else if (sender == EncodingIndicator)
            {
                var reopenWithEncoding = EncodingSelectionFlyout?.Items?.FirstOrDefault(i => i.Name.Equals("ReopenWithEncoding"));
                if (reopenWithEncoding != null)
                {
                    reopenWithEncoding.IsEnabled = selectedEditor.EditingFile != null && selectedEditor.FileModificationState != FileModificationState.RenamedMovedOrDeleted;
                }
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

            if (EncodingUtility.TryGetSystemDefaultANSIEncoding(out var systemDefaultANSIEncoding))
            {
                appAndSystemANSIEncodings.Add(systemDefaultANSIEncoding);
            }
            if (EncodingUtility.TryGetCurrentCultureANSIEncoding(out var currentCultureANSIEncoding))
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
            var ANSIEncodings = EncodingUtility.GetAllSupportedANSIEncodings();
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
                var selectedTextEditor = NotepadsCore.GetSelectedTextEditor();
                var file = selectedTextEditor?.EditingFile;
                if (file == null || selectedTextEditor.FileModificationState == FileModificationState.RenamedMovedOrDeleted) return;

                Encoding encoding = null;

                try
                {
                    using (var inputStream = await file.OpenReadAsync())
                    using (var stream = inputStream.AsStreamForRead())
                    {
                        if (FileSystemUtility.TryGuessEncoding(stream, out var detectedEncoding))
                        {
                            encoding = detectedEncoding;
                        }
                    }
                }
                catch (Exception)
                {
                    encoding = null;
                }

                if (encoding == null)
                {
                    NotificationCenter.Instance.PostNotification(_resourceLoader.GetString("TextEditor_NotificationMsg_EncodingCannotBeDetermined"), 2500);
                    return;
                }

                try
                {
                    await selectedTextEditor.ReloadFromEditingFile(encoding);
                    NotificationCenter.Instance.PostNotification(
                        _resourceLoader.GetString("TextEditor_NotificationMsg_FileReloaded"), 1500);
                }
                catch (Exception ex)
                {
                    var fileOpenErrorDialog = NotepadsDialogFactory.GetFileOpenErrorDialog(selectedTextEditor.EditingFilePath, ex.Message);
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

            var reopenWithEncodingItem =
                new MenuFlyoutItem()
                {
                    Text = EncodingUtility.GetEncodingName(encoding),
                    FlowDirection = FlowDirection.LeftToRight,
                    Height = EncodingMenuFlyoutItemHeight,
                    FontSize = EncodingMenuFlyoutItemFontSize
                };
            reopenWithEncodingItem.Click += async (sender, args) =>
            {
                var selectedTextEditor = NotepadsCore.GetSelectedTextEditor();
                if (selectedTextEditor != null)
                {
                    try
                    {
                        await selectedTextEditor.ReloadFromEditingFile(encoding);
                        NotificationCenter.Instance.PostNotification(
                            _resourceLoader.GetString("TextEditor_NotificationMsg_FileReloaded"), 1500);   
                    }
                    catch (Exception ex)
                    {
                        var fileOpenErrorDialog = NotepadsDialogFactory.GetFileOpenErrorDialog(selectedTextEditor.EditingFilePath, ex.Message);
                        await DialogManager.OpenDialogAsync(fileOpenErrorDialog, awaitPreviousDialog: false);
                        if (!fileOpenErrorDialog.IsAborted)
                        {
                            NotepadsCore.FocusOnSelectedTextEditor();
                        }
                    }
                }
            };
            reopenWithEncoding.Items?.Add(reopenWithEncodingItem);

            var saveWithEncodingItem =
                new MenuFlyoutItem()
                {
                    Text = EncodingUtility.GetEncodingName(encoding),
                    FlowDirection = FlowDirection.LeftToRight,
                    Height = EncodingMenuFlyoutItemHeight,
                    FontSize = EncodingMenuFlyoutItemFontSize
                };
            saveWithEncodingItem.Click += (sender, args) =>
            {
                NotepadsCore.GetSelectedTextEditor()?.TryChangeEncoding(encoding);
            };
            saveWithEncoding.Items?.Add(saveWithEncodingItem);
        }
    }
}