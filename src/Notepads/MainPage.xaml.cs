
namespace Notepads
{
    using Notepads.Controls.FindAndReplace;
    using Notepads.Controls.Settings;
    using Notepads.Controls.TextEditor;
    using Notepads.EventArgs;
    using Notepads.Services;
    using Notepads.Utilities;
    using SetsView;
    using System;
    using System.Collections.Generic;
    using System.IO;
    using System.Linq;
    using System.Runtime.InteropServices.WindowsRuntime;
    using System.Text;
    using System.Threading.Tasks;
    using Windows.ApplicationModel;
    using Windows.ApplicationModel.Activation;
    using Windows.ApplicationModel.DataTransfer;
    using Windows.Foundation;
    using Windows.Foundation.Collections;
    using Windows.Storage;
    using Windows.Storage.Pickers;
    using Windows.Storage.Provider;
    using Windows.System;
    using Windows.UI;
    using Windows.UI.Core;
    using Windows.UI.Text;
    using Windows.UI.Xaml;
    using Windows.UI.Xaml.Controls;
    using Windows.UI.Xaml.Controls.Primitives;
    using Windows.UI.Xaml.Data;
    using Windows.UI.Xaml.Input;
    using Windows.UI.Xaml.Media;
    using Windows.UI.Xaml.Media.Animation;
    using Windows.UI.Xaml.Navigation;

    public sealed partial class MainPage : Page
    {
        private const string DefaultFileName = "New Document.txt";

        private IReadOnlyList<IStorageItem> _appLaunchFiles;

        private string _appLaunchCmdDir;

        private string _appLaunchCmdArgs;

        public MainPage()
        {
            InitializeComponent();

            // Set custom Title Bar
            Window.Current.SetTitleBar(AppTitleBar);

            // Setup theme
            ThemeSettingsService.AppBackground = RootGrid;
            ThemeSettingsService.SetRequestedTheme();
            ThemeSettingsService.OnAccentColorChanged += ThemeSettingsService_OnAccentColorChanged;

            EditorSettingsService.OnDefaultLineEndingChanged += EditorSettingsService_OnDefaultLineEndingChanged;
            EditorSettingsService.OnDefaultEncodingChanged += EditorSettingsService_OnDefaultEncodingChanged;

            Windows.ApplicationModel.DataTransfer.DataTransferManager.GetForCurrentView().DataRequested += MainPage_DataRequested;
            Windows.UI.Core.Preview.SystemNavigationManagerPreview.GetForCurrentView().CloseRequested += MainPage_CloseRequested;

            Window.Current.VisibilityChanged += WindowVisibilityChangedEventHandler;
        }

        void WindowVisibilityChangedEventHandler(System.Object sender, Windows.UI.Core.VisibilityChangedEventArgs e)
        {
            // Perform operations that should take place when the application becomes visible rather than
            // when it is prelaunched, such as building a what's new feed
        }

        private void ThemeSettingsService_OnAccentColorChanged(object sender, Color color)
        {
            if (Sets.Items == null) return;
            foreach (SetsViewItem item in Sets.Items)
            {
                item.Icon.Foreground = Application.Current.Resources["SystemControlForegroundAccentBrush"] as SolidColorBrush;
                item.SelectionIndicatorForeground = Application.Current.Resources["SystemControlForegroundAccentBrush"] as SolidColorBrush;
            }
        }

        private void EditorSettingsService_OnDefaultEncodingChanged(object sender, Encoding encoding)
        {
            if (Sets.Items == null) return;
            foreach (SetsViewItem setItem in Sets.Items)
            {
                if (!(setItem.Content is TextEditor textEditor)) continue;
                if (textEditor.EditingFile != null) continue;

                textEditor.Encoding = encoding;
                if (textEditor == ((SetsViewItem)Sets.SelectedItem)?.Content)
                {
                    EncodingIndicator.Text = textEditor.Encoding.EncodingName;
                }
            }
        }

        private void EditorSettingsService_OnDefaultLineEndingChanged(object sender, LineEnding lineEnding)
        {
            if (Sets.Items == null) return;
            foreach (SetsViewItem setItem in Sets.Items)
            {
                if (!(setItem.Content is TextEditor textEditor)) continue;
                if (textEditor.EditingFile != null) continue;

                textEditor.LineEnding = lineEnding;
                if (textEditor == ((SetsViewItem)Sets.SelectedItem)?.Content)
                {
                    LineEndingIndicator.Text = LineEndingUtility.GetLineEndingDisplayText(textEditor.LineEnding);
                }
            }
        }

        private void FindAndReplaceControl_OnFindAndReplaceButtonClicked(object sender, FindAndReplaceEventArgs e)
        {
            if (!((Sets.SelectedItem as SetsViewItem)?.Content is TextEditor textEditor)) return;

            FocusOnSelectedTextEditor();

            bool found = false;

            switch (e.FindAndReplaceMode)
            {
                case FindAndReplaceMode.FindOnly:
                    found = textEditor.FindNextAndSelect(e.SearchText, e.MatchCase, e.MatchWholeWord, false);
                    break;
                case FindAndReplaceMode.Replace:
                    found = textEditor.FindNextAndReplace(e.SearchText, e.ReplaceText, e.MatchCase, e.MatchWholeWord);
                    break;
                case FindAndReplaceMode.ReplaceAll:
                    found = textEditor.FindAndReplaceAll(e.SearchText, e.ReplaceText, e.MatchCase, e.MatchWholeWord);
                    break;
            }

            if (!found)
            {
                SearchNotFoundNotification.Show(1500);
            }
        }

        private void FocusOnSelectedTextEditor()
        {
            ((Sets.SelectedItem as SetsViewItem)?.Content as TextEditor)?.Focus(FocusState.Programmatic);
        }

        private void MainMenuButtonFlyout_Closing(FlyoutBase sender, FlyoutBaseClosingEventArgs args)
        {
            FocusOnSelectedTextEditor();
        }

        private void SearchBarPlaceHolder_Closed(object sender, Microsoft.Toolkit.Uwp.UI.Controls.InAppNotificationClosedEventArgs e)
        {
            SearchBarPlaceHolder.Visibility = Visibility.Collapsed;
        }

        private void FindAndReplaceControl_OnDismissKeyDown(object sender, RoutedEventArgs e)
        {
            SearchBarPlaceHolder.Dismiss();
            FocusOnSelectedTextEditor();
        }

        private bool HaveUnsavedSet()
        {
            foreach (SetsViewItem setsItem in Sets.Items)
            {
                if (!(setsItem.Content is TextEditor textEditor)) continue;
                if (textEditor.Saved) continue;
                return true;
            }
            return false;
        }

        private async void MainPage_CloseRequested(object sender, Windows.UI.Core.Preview.SystemNavigationCloseRequestedPreviewEventArgs e)
        {
            if (Sets.Items == null || Sets.Items.Count == 0) return;
            if (!HaveUnsavedSet()) return;
            e.Handled = true;
            await ContentDialogFactory.GetAppCloseSaveReminderDialog(() => Application.Current.Exit()).ShowAsync();
        }

        // Content sharing
        private void MainPage_DataRequested(DataTransferManager sender, DataRequestedEventArgs args)
        {
            if (!((Sets.SelectedItem as SetsViewItem)?.Content is TextEditor editor)) return;

            var content = editor.GetContentForSharing();

            if (!string.IsNullOrEmpty(content))
            {
                args.Request.Data.SetText(content);
                args.Request.Data.Properties.Title = editor.EditingFile != null ? editor.EditingFile.Name : DefaultFileName;
            }
            else
            {
                args.Request.FailWithDisplayText("Nothing to share because no text is selected and current document is empty.");
            }
        }

        // Open files from external links or cmd args - load time
        private async void Sets_Loaded(object sender, RoutedEventArgs e)
        {
            if (_appLaunchFiles != null && _appLaunchFiles.Count > 0)
            {
                foreach (var storageItem in _appLaunchFiles)
                {
                    if (storageItem is StorageFile file)
                    {
                        await OpenFile(file);
                    }
                }

                _appLaunchFiles = null;
            }
            else if (_appLaunchCmdDir != null)
            {
                var file = await FileSystemUtility.OpenFileFromCommandLine(_appLaunchCmdDir, _appLaunchCmdArgs);
                if (file == null)
                {
                    OpenEmptyNewSet();
                }
                else
                {
                    await OpenFile(file);
                }

                _appLaunchCmdDir = null;
                _appLaunchCmdArgs = null;
            }
            else if (Sets.Items?.Count == 0)
            {
                OpenEmptyNewSet();
            }
        }

        // Handles external links or cmd args during runtime
        protected override void OnNavigatedTo(NavigationEventArgs e)
        {
            base.OnNavigatedTo(e);
            if (e.Parameter is FileActivatedEventArgs fileActivatedEventArgs)
            {
                _appLaunchFiles = fileActivatedEventArgs.Files;
            }
            else if (e.Parameter is CommandLineActivatedEventArgs)
            {
                var commandLine = e.Parameter as CommandLineActivatedEventArgs;
                _appLaunchCmdDir = commandLine.Operation.CurrentDirectoryPath;
                _appLaunchCmdArgs = commandLine.Operation.Arguments;
            }
        }

        private void MainMenuButtonFlyout_Opening(object sender, object e)
        {
            if (Sets.Items?.Count == 0)
            {
                MenuSaveButton.IsEnabled = false;
                MenuSaveAsButton.IsEnabled = false;
                MenuFindButton.IsEnabled = false;
                MenuReplaceButton.IsEnabled = false;
                //MenuPrintButton.IsEnabled = false;
            }
            else
            {
                MenuSaveButton.IsEnabled = true;
                MenuSaveAsButton.IsEnabled = true;
                MenuFindButton.IsEnabled = true;
                MenuReplaceButton.IsEnabled = true;
                //MenuPrintButton.IsEnabled = true;
            }
        }

        private void SwitchSet(bool next)
        {
            if (Sets.Items == null) return;
            if (Sets.Items.Count < 2) return;

            var setsCount = Sets.Items.Count;
            var selected = Sets.SelectedIndex;

            if (next && setsCount > 1)
            {
                if (selected == setsCount - 1) Sets.SelectedIndex = 0;
                else Sets.SelectedIndex += 1;
            }
            else if (!next && setsCount > 1)
            {
                if (selected == 0) Sets.SelectedIndex = setsCount - 1;
                else Sets.SelectedIndex -= 1;
            }
        }

        public async Task<bool> OpenFile(StorageFile file)
        {
            try
            {
                var textFile = await FileSystemUtility.ReadFile(file);
                CreateNewTextEditorSet(textFile.Content, file, textFile.Encoding, textFile.LineEnding);
                return true;
            }
            catch (Exception ex)
            {
                await ContentDialogFactory.GetFileOpenErrorDialog(file, ex, FocusOnSelectedTextEditor).ShowAsync();
                return false;
            }
        }

        private async void OpenFileButton_Click(object sender, RoutedEventArgs e)
        {
            var file = await FilePickerFactory.GetFileOpenPicker().PickSingleFileAsync();
            if (file == null)
            {
                FocusOnSelectedTextEditor();
                return;
            }
            await OpenFile(file);
        }

        private async void SaveButton_Click(object sender, RoutedEventArgs e)
        {
            if (!(sender is TextEditor textEditor))
            {
                textEditor = (Sets.SelectedItem as SetsViewItem)?.Content as TextEditor;
                if (textEditor == null) return;
            }

            StorageFile file;

            if (textEditor.EditingFile == null || e is SaveAsEventArgs ||
                FileSystemUtility.IsFileReadOnly(textEditor.EditingFile) ||
                !await FileSystemUtility.FileIsWritable(textEditor.EditingFile))
            {
                file = await FilePickerFactory.GetFileSavePicker(e, textEditor, DefaultFileName).PickSaveFileAsync();
                textEditor.Focus(FocusState.Programmatic);
            }
            else
            {
                file = textEditor.EditingFile;
            }

            if (file == null) return;

            var success = await textEditor.SaveFile(file);

            if (success)
            {
                if (Sets.Items != null)
                {
                    foreach (SetsViewItem setsItem in Sets.Items)
                    {
                        if (setsItem.Content != textEditor) continue;

                        setsItem.Header = file.Name;
                        setsItem.Icon.Visibility = Visibility.Collapsed;

                        PathIndicator.Text = textEditor.EditingFile.Path;
                        LineEndingIndicator.Text =
                            LineEndingUtility.GetLineEndingDisplayText(textEditor.LineEnding);
                        EncodingIndicator.Text = textEditor.Encoding.EncodingName;
                        break;
                    }
                }
                StatusNotification.Show("Saved", 2000);
            }
            else
            {
                await ContentDialogFactory.GetFileSaveErrorDialog(file).ShowAsync();
            }
        }

        private void CreateNewTextEditorSet(string text, StorageFile file, Encoding encoding, LineEnding lineEnding)
        {
            var textEditor = new TextEditor()
            {
                EditingFile = file,
                Encoding = encoding,
                LineEnding = lineEnding,
                Saved = true,
            };

            textEditor.SetText(text);
            textEditor.Loaded += TextEditor_Loaded;
            textEditor.Unloaded += TextEditor_Unloaded;
            textEditor.KeyDown += TextEditor_KeyDown;
            textEditor.OnSetClosingKeyDown += TextEditor_OnSetClosingKeyDown;
            textEditor.OnFindButtonClicked += TextEditor_OnFindButtonClicked;
            textEditor.OnFindAndReplaceButtonClicked += TextEditor_OnFindAndReplaceButtonClicked;

            var newItem = new SetsViewItem
            {
                Header = file == null ? DefaultFileName : file.Name,
                Content = textEditor,
                SelectionIndicatorForeground = Application.Current.Resources["SystemControlForegroundAccentBrush"] as SolidColorBrush,
                Icon = new SymbolIcon(Symbol.Save)
                {
                    Foreground = Application.Current.Resources["SystemControlForegroundAccentBrush"] as SolidColorBrush,
                }
            };
            newItem.Icon.Visibility = Visibility.Collapsed;
            Sets.Items?.Add(newItem);
            newItem.IsSelected = true;
            Sets.ScrollToLastSet();
        }

        private void TextEditor_OnFindButtonClicked(object sender, KeyRoutedEventArgs e)
        {
            ShowFindAndReplaceControl(false);
        }

        private void TextEditor_OnFindAndReplaceButtonClicked(object sender, KeyRoutedEventArgs e)
        {
            ShowFindAndReplaceControl(true);
        }

        private void ShowFindAndReplaceControl(bool showReplaceBar)
        {
            var findAndReplace = (FindAndReplaceControl)SearchBarPlaceHolder.Content;

            if (findAndReplace == null) return;

            SearchBarPlaceHolder.Height = findAndReplace.GetHeight(showReplaceBar);
            findAndReplace.ShowReplaceBar(showReplaceBar);

            if (SearchBarPlaceHolder.Visibility == Visibility.Collapsed)
            {
                SearchBarPlaceHolder.Show();
            }

            Task.Factory.StartNew(
                () => Dispatcher.RunAsync(CoreDispatcherPriority.Low,
                    () => findAndReplace.Focus()));
        }

        private void TextEditor_OnSetClosingKeyDown(object sender, KeyRoutedEventArgs e)
        {
            if (!(sender is TextEditor textEditor)) return;

            if (Sets.Items == null) return;

            foreach (SetsViewItem setsItem in Sets.Items)
            {
                if (setsItem.Content != textEditor) continue;
                setsItem.Close();
                e.Handled = true;
                break;
            }
        }

        private void TextEditor_Loaded(object sender, RoutedEventArgs e)
        {
            if (!(sender is TextEditor textEditor)) return;

            textEditor.TextChanging += TextEditor_TextChanging;
            textEditor.SelectionChanged += TextEditor_SelectionChanged;
            textEditor.Focus(FocusState.Programmatic);

            PathIndicator.Text = textEditor.EditingFile != null ? textEditor.EditingFile.Path : DefaultFileName;
            LineEndingIndicator.Text = LineEndingUtility.GetLineEndingDisplayText(textEditor.LineEnding);
            EncodingIndicator.Text = textEditor.Encoding.EncodingName;
            textEditor.GetCurrentLineColumn(out var line, out var column, out var selectedCount);
            LineColumnIndicator.Text = selectedCount == 0 ? $"Ln {line} Col {column}" : $"Ln {line} Col {column} ({selectedCount} selected)";
        }

        private void TextEditor_TextChanging(RichEditBox sender, RichEditBoxTextChangingEventArgs args)
        {
            if (!(sender is TextEditor textEditor) || !args.IsContentChanging) return;

            if (textEditor.Saved)
            {
                MarkSelectedEditorNotSaved(textEditor);
            }
        }

        private void MarkSelectedEditorNotSaved(TextEditor textEditor)
        {
            textEditor.Saved = false;

            var selectedItem = Sets.SelectedItem as SetsViewItem;
            if (selectedItem?.Content == textEditor)
            {
                selectedItem.Icon.Visibility = Visibility.Visible;
            }
        }

        private void TextEditor_Unloaded(object sender, RoutedEventArgs e)
        {
            if (!(sender is TextEditor textEditor)) return;

            textEditor.TextChanging -= TextEditor_TextChanging;
            textEditor.SelectionChanged -= TextEditor_SelectionChanged;

            if (Sets.Items?.Count == 0)
            {
                Application.Current.Exit();
            }

            if (SearchBarPlaceHolder.Visibility == Visibility.Visible) SearchBarPlaceHolder.Dismiss();
        }

        private void TextEditor_KeyDown(object sender, KeyRoutedEventArgs e)
        {
            if (!(sender is TextEditor textEditor)) return;

            var ctrl = Window.Current.CoreWindow.GetKeyState(VirtualKey.Control);
            var alt = Window.Current.CoreWindow.GetKeyState(VirtualKey.Menu);
            var shift = Window.Current.CoreWindow.GetKeyState(VirtualKey.Shift);

            if (ctrl.HasFlag(CoreVirtualKeyStates.Down) && !alt.HasFlag(CoreVirtualKeyStates.Down))
            {
                if (e.Key == VirtualKey.Tab)
                {
                    SwitchSet(!shift.HasFlag(CoreVirtualKeyStates.Down));
                    e.Handled = true;
                }

                if (e.Key == VirtualKey.N || e.Key == VirtualKey.T)
                {
                    OpenEmptyNewSet();
                    e.Handled = true;
                }

                if (e.Key == VirtualKey.O)
                {
                    OpenFileButton_Click(this, null);
                    e.Handled = true;
                }

                if (e.Key == VirtualKey.S)
                {
                    if (shift.HasFlag(CoreVirtualKeyStates.Down))
                    {
                        SaveButton_Click(sender, new SaveAsEventArgs());
                        e.Handled = true;
                    }
                    else
                    {
                        SaveButton_Click(sender, e);
                        e.Handled = true;
                    }
                }
            }

            if (!ctrl.HasFlag(CoreVirtualKeyStates.Down) &&
                !alt.HasFlag(CoreVirtualKeyStates.Down) &&
                !shift.HasFlag(CoreVirtualKeyStates.Down) &&
                e.Key == Windows.System.VirtualKey.Tab)
            {
                var tabStr = EditorSettingsService.EditorDefaultTabIndents == -1 ? "\t" : new string(' ', EditorSettingsService.EditorDefaultTabIndents);
                textEditor.Document.Selection.TypeText(tabStr);
                e.Handled = true;
            }
        }

        private void TextEditor_SelectionChanged(object sender, RoutedEventArgs e)
        {
            if (!(sender is TextEditor textEditor)) return;
            textEditor.GetCurrentLineColumn(out var line, out var column, out var selectedCount);
            LineColumnIndicator.Text = selectedCount == 0 ? $"Ln {line} Col {column}" : $"Ln {line} Col {column} ({selectedCount} selected)";
        }

        private async void SetsView_OnSetClosing(object sender, SetClosingEventArgs e)
        {
            if (!(e.Set.Content is TextEditor textEditor)) return;
            if (textEditor.Saved) return;

            e.Cancel = true;

            var file = (textEditor.EditingFile != null ? textEditor.EditingFile.Path : DefaultFileName);
            await ContentDialogFactory.GetSetCloseSaveReminderDialog(file, () =>
                {
                    SaveButton_Click(textEditor, new RoutedEventArgs());
                }, () =>
                {
                    e.Set.IsEnabled = false;
                    Sets.Items?.Remove(e.Set);
                }).ShowAsync();
        }

        private void SetsView_OnSetTapped(object sender, SetSelectedEventArgs e)
        {
            if (e.Item is TextEditor textEditor)
            {
                textEditor.Focus(FocusState.Programmatic);
            }
        }

        private async void RootGrid_OnDrop(object sender, DragEventArgs e)
        {
            if (!e.DataView.Contains(StandardDataFormats.StorageItems)) return;
            var storageItems = await e.DataView.GetStorageItemsAsync();

            foreach (var storageItem in storageItems)
            {
                if (storageItem is StorageFile file)
                {
                    await OpenFile(file);
                }
            }
        }

        private void RootGrid_OnDragOver(object sender, DragEventArgs e)
        {
            e.AcceptedOperation = DataPackageOperation.Link;
        }

        private void MenuButton_OnTapped(object sender, TappedRoutedEventArgs e)
        {
            FlyoutBase.ShowAttachedFlyout((FrameworkElement)sender);
        }

        public void OpenEmptyNewSet()
        {
            CreateNewTextEditorSet(string.Empty, null, EditorSettingsService.EditorDefaultEncoding, EditorSettingsService.EditorDefaultLineEnding);
        }

        private void CreateNewSet(object sender, RoutedEventArgs e)
        {
            OpenEmptyNewSet();
        }

        private void NewSetButton_OnTapped(object sender, TappedRoutedEventArgs e)
        {
            OpenEmptyNewSet();
        }

        private void MenuSaveAsButton_OnClick(object sender, RoutedEventArgs e)
        {
            SaveButton_Click(sender, new SaveAsEventArgs());
        }

        private void MenuSettingsButton_OnClick(object sender, RoutedEventArgs e)
        {
            RootSplitView.IsPaneOpen = true;
        }

        private void UtilityIndicator_OnTapped(object sender, TappedRoutedEventArgs e)
        {
            if (sender == PathIndicator && !string.IsNullOrEmpty(PathIndicator.Text))
            {
                FocusOnSelectedTextEditor();
                var pathData = new DataPackage();
                pathData.SetText(PathIndicator.Text);
                Windows.ApplicationModel.DataTransfer.Clipboard.SetContent(pathData);
                StatusNotification.Show("Copied", 1500);
            }
            else if (sender is TextBlock textBlock)
            {
                textBlock.ContextFlyout?.ShowAt(textBlock);
            }
        }

        private void LineEndingSelection_OnClick(object sender, RoutedEventArgs e)
        {
            if (!(sender is MenuFlyoutItem item)) return;

            switch (item.Tag)
            {
                case "CRLF":
                    ChangeLineEndingForSelectedTextEditor(sender, LineEnding.Crlf);
                    break;
                case "CR":
                    ChangeLineEndingForSelectedTextEditor(sender, LineEnding.Cr);
                    break;
                case "LF":
                    ChangeLineEndingForSelectedTextEditor(sender, LineEnding.Lf);
                    break;
            }
        }

        private void EncodingSelection_OnClick(object sender, RoutedEventArgs e)
        {
            if (!(sender is MenuFlyoutItem item)) return;

            switch (item.Tag)
            {
                case "UTF-8":
                    ChangeEncodingSelectedTextEditor(sender, Encoding.UTF8);
                    break;
                case "UTF-16 LE":
                    ChangeEncodingSelectedTextEditor(sender, Encoding.Unicode);
                    break;
                case "UTF-16 BE":
                    ChangeEncodingSelectedTextEditor(sender, Encoding.BigEndianUnicode);
                    break;
            }
        }

        private void ChangeLineEndingForSelectedTextEditor(object sender, LineEnding lineEnding)
        {
            if (!(sender is MenuFlyoutItem item)) return;
            if ((!((Sets.SelectedItem as SetsViewItem)?.Content is TextEditor textEditor))) return;
            if (lineEnding == textEditor.LineEnding) return;

            MarkSelectedEditorNotSaved(textEditor);
            textEditor.LineEnding = lineEnding;
            LineEndingIndicator.Text = item.Text;
        }

        private void ChangeEncodingSelectedTextEditor(object sender, Encoding encoding)
        {
            if (!(sender is MenuFlyoutItem)) return;
            if ((!((Sets.SelectedItem as SetsViewItem)?.Content is TextEditor textEditor))) return;

            if (textEditor.Encoding.CodePage == encoding.CodePage) return;

            MarkSelectedEditorNotSaved(textEditor);
            textEditor.Encoding = encoding;
            EncodingIndicator.Text = textEditor.Encoding.EncodingName;
        }

        private void UtilitySelectionFlyout_OnClosing(FlyoutBase sender, FlyoutBaseClosingEventArgs args)
        {
            FocusOnSelectedTextEditor();
        }

        private void FindButton_OnClick(object sender, RoutedEventArgs e)
        {
            TextEditor_OnFindButtonClicked(sender, null);
        }

        private void ReplaceButton_OnClick(object sender, RoutedEventArgs e)
        {
            TextEditor_OnFindAndReplaceButtonClicked(sender, null);
        }

        private void RootSplitView_OnPaneClosed(SplitView sender, object args)
        {
            FocusOnSelectedTextEditor();
        }

        private void RootSplitView_OnPaneOpening(SplitView sender, object args)
        {
            SettingsFrame.Navigate(typeof(SettingsPage), null, new SuppressNavigationTransitionInfo());
        }
    }
}
