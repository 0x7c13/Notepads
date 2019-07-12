
namespace Notepads
{
    using Notepads.Controls.FindAndReplace;
    using Notepads.Controls.Settings;
    using Notepads.Controls.TextEditor;
    using Notepads.Core;
    using Notepads.EventArgs;
    using Notepads.Services;
    using Notepads.Utilities;
    using System;
    using System.Collections.Generic;
    using System.Text;
    using System.Threading.Tasks;
    using Windows.ApplicationModel.Activation;
    using Windows.ApplicationModel.DataTransfer;
    using Windows.ApplicationModel.Resources;
    using Windows.Storage;
    using Windows.System;
    using Windows.UI.Core;
    using Windows.UI.Xaml;
    using Windows.UI.Xaml.Controls;
    using Windows.UI.Xaml.Controls.Primitives;
    using Windows.UI.Xaml.Input;
    using Windows.UI.Xaml.Media.Animation;
    using Windows.UI.Xaml.Navigation;

    public sealed partial class MainPage : Page
    {
        private readonly string _defaultNewFileName;

        private IReadOnlyList<IStorageItem> _appLaunchFiles;

        private string _appLaunchCmdDir;

        private string _appLaunchCmdArgs;

        private readonly ResourceLoader _resourceLoader = Windows.ApplicationModel.Resources.ResourceLoader.GetForCurrentView();

        private bool _loaded = false;

        private INotepadsCore _notepadsCore;

        private INotepadsCore NotepadsCore
        {
            get
            {
                if (_notepadsCore == null)
                {
                    _notepadsCore = new NotepadsCore(Sets, _resourceLoader.GetString("TextEditor_DefaultNewFileName"));
                    _notepadsCore.OnTextEditorLoaded += OnTextEditorLoaded;
                    _notepadsCore.OnTextEditorUnloaded += OnTextEditorUnloaded;
                    _notepadsCore.OnTextEditorKeyDown += OnTextEditor_KeyDown;
                    _notepadsCore.OnTextEditorClosingWithUnsavedContent += OnTextEditorClosingWithUnsavedContent;
                    _notepadsCore.OnTextEditorSelectionChanged += (sender, editor) => { if (NotepadsCore.GetSelectedTextEditor() == editor) UpdateLineColumnIndicatorText(editor); };
                    _notepadsCore.OnTextEditorEncodingChanged += (sender, editor) => { if (NotepadsCore.GetSelectedTextEditor() == editor) UpdateEncodingIndicatorText(editor.Encoding); };
                    _notepadsCore.OnTextEditorLineEndingChanged += (sender, editor) => { if (NotepadsCore.GetSelectedTextEditor() == editor) UpdateLineEndingIndicatorText(editor.LineEnding); };
                }

                return _notepadsCore;
            }
        }

        public MainPage()
        {
            InitializeComponent();

            _defaultNewFileName = _resourceLoader.GetString("TextEditor_DefaultNewFileName");

            // Setup theme
            ThemeSettingsService.AppBackground = RootGrid;
            ThemeSettingsService.SetRequestedTheme();

            // Set custom Title Bar
            Window.Current.SetTitleBar(AppTitleBar);

            // Setup status bar
            ShowHideStatusBar(EditorSettingsService.ShowStatusBar);

            EditorSettingsService.OnStatusBarVisibilityChanged += (sender, visibility) => ShowHideStatusBar(visibility);

            Windows.ApplicationModel.DataTransfer.DataTransferManager.GetForCurrentView().DataRequested += MainPage_DataRequested;
            Windows.UI.Core.Preview.SystemNavigationManagerPreview.GetForCurrentView().CloseRequested += MainPage_CloseRequested;

            Window.Current.VisibilityChanged += WindowVisibilityChangedEventHandler;

            NewSetButton.Click += delegate { NotepadsCore.OpenNewTextEditor(); };
            RootSplitView.PaneOpening += delegate { SettingsFrame.Navigate(typeof(SettingsPage), null, new SuppressNavigationTransitionInfo()); };
            RootSplitView.PaneClosed += delegate { NotepadsCore.FocusOnSelectedTextEditor(); };
        }

        #region Application Life Cycle & Window management 

        // Handles external links or cmd args activation before Sets loaded
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

        // App should wait for Sets fully loaded before opening files requested by user (by click or from cmd)
        // Open files from external links or cmd args on Sets Loaded
        private async void Sets_Loaded(object sender, RoutedEventArgs e)
        {
            if (_appLaunchFiles != null && _appLaunchFiles.Count > 0)
            {
                var successCount = await OpenFiles(_appLaunchFiles);
                if (successCount == 0)
                {
                    NotepadsCore.OpenNewTextEditor();
                }
                _appLaunchFiles = null;
            }
            else if (_appLaunchCmdDir != null)
            {
                var file = await FileSystemUtility.OpenFileFromCommandLine(_appLaunchCmdDir, _appLaunchCmdArgs);
                if (file == null || !(await OpenFile(file)))
                {
                    NotepadsCore.OpenNewTextEditor();
                }
                _appLaunchCmdDir = null;
                _appLaunchCmdArgs = null;
            }
            else if (!_loaded)
            {
                NotepadsCore.OpenNewTextEditor();
                _loaded = true;
            }
        }

        void WindowVisibilityChangedEventHandler(System.Object sender, Windows.UI.Core.VisibilityChangedEventArgs e)
        {
            // Perform operations that should take place when the application becomes visible rather than
            // when it is prelaunched, such as building a what's new feed
        }

        // Content sharing
        private void MainPage_DataRequested(DataTransferManager sender, DataRequestedEventArgs args)
        {
            var textEditor = NotepadsCore.GetSelectedTextEditor();
            if (textEditor == null) return;

            if (NotepadsCore.TryGetSharingContent(textEditor, out var title, out var content))
            {
                args.Request.Data.Properties.Title = title;
                args.Request.Data.SetText(content);
            }
            else
            {
                args.Request.FailWithDisplayText(_resourceLoader.GetString("ContentSharing_FailureDisplayText"));
            }
        }

        private async void MainPage_CloseRequested(object sender, Windows.UI.Core.Preview.SystemNavigationCloseRequestedPreviewEventArgs e)
        {
            if (NotepadsCore.HaveUnsavedTextEditor())
            {
                e.Handled = true;
                await ContentDialogFactory.GetAppCloseSaveReminderDialog(() => Application.Current.Exit()).ShowAsync();
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

        #endregion

        #region Status Bar

        private void SetupStatusBar(TextEditor textEditor)
        {
            if (textEditor == null) return;
            UpdatePathIndicatorText(textEditor);
            UpdateLineEndingIndicatorText(textEditor.LineEnding);
            UpdateEncodingIndicatorText(textEditor.Encoding);
            UpdateLineColumnIndicatorText(textEditor);
        }

        public void ShowHideStatusBar(bool showStatusBar)
        {
            if (showStatusBar)
            {
                if (StatusBar == null)
                {
                    FindName("StatusBar"); // Lazy loading   
                }
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

        private void UpdatePathIndicatorText(TextEditor textEditor)
        {
            if (StatusBar == null) return;
            PathIndicator.Text = textEditor.EditingFile != null ? textEditor.EditingFile.Path : _defaultNewFileName;
        }

        private void UpdateEncodingIndicatorText(Encoding encoding)
        {
            if (StatusBar == null) return;
            EncodingIndicator.Text = EncodingUtility.GetEncodingBodyName(encoding);
        }

        private void UpdateLineEndingIndicatorText(LineEnding lineEnding)
        {
            if (StatusBar == null) return;
            LineEndingIndicator.Text = LineEndingUtility.GetLineEndingDisplayText(lineEnding);
        }

        private void UpdateLineColumnIndicatorText(TextEditor textEditor)
        {
            if (StatusBar == null) return;
            textEditor.GetCurrentLineColumn(out var line, out var column, out var selectedCount);
            LineColumnIndicator.Text = selectedCount == 0 ? $"Ln {line} Col {column}" : $"Ln {line} Col {column} ({selectedCount} selected)";
        }

        private void LineEndingSelection_OnClick(object sender, RoutedEventArgs e)
        {
            if (!(sender is MenuFlyoutItem item)) return;

            var lineEnding = LineEndingUtility.GetLineEndingByName((string)item.Tag);
            var textEditor = NotepadsCore.GetSelectedTextEditor();
            if (textEditor != null)
            {
                NotepadsCore.ChangeLineEnding(textEditor, lineEnding);
            }
        }

        private void EncodingSelection_OnClick(object sender, RoutedEventArgs e)
        {
            if (!(sender is MenuFlyoutItem item)) return;

            var encoding = EncodingUtility.GetEncodingByName((string)item.Tag);
            var textEditor = NotepadsCore.GetSelectedTextEditor();
            if (textEditor != null)
            {
                NotepadsCore.ChangeEncoding(textEditor, encoding);
            }
        }

        private void StatusBarComponent_OnTapped(object sender, TappedRoutedEventArgs e)
        {
            if (sender == PathIndicator && !string.IsNullOrEmpty(PathIndicator.Text))
            {
                NotepadsCore.FocusOnSelectedTextEditor();
                var pathData = new DataPackage();
                pathData.SetText(PathIndicator.Text);
                Windows.ApplicationModel.DataTransfer.Clipboard.SetContent(pathData);
                ShowInAppNotificationMessage(_resourceLoader.GetString("TextEditor_NotificationMsg_FileNameOrPathCopied"), 1500);
            }
            else if (sender is TextBlock textBlock)
            {
                textBlock.ContextFlyout?.ShowAt(textBlock);
            }
        }

        private void StatusBarFlyout_OnClosing(FlyoutBase sender, FlyoutBaseClosingEventArgs args)
        {
            NotepadsCore.FocusOnSelectedTextEditor();
        }

        #endregion

        #region Find & Replace

        private void ShowFindAndReplaceControl(bool showReplaceBar)
        {
            if (FindAndReplacePlaceholder == null)
            {
                FindName("FindAndReplacePlaceholder"); // Lazy loading
            }

            var findAndReplace = (FindAndReplaceControl)FindAndReplacePlaceholder.Content;

            if (findAndReplace == null) return;

            FindAndReplacePlaceholder.Height = findAndReplace.GetHeight(showReplaceBar);
            findAndReplace.ShowReplaceBar(showReplaceBar);

            if (FindAndReplacePlaceholder.Visibility == Visibility.Collapsed)
            {
                FindAndReplacePlaceholder.Show();
            }

            Task.Factory.StartNew(
                () => Dispatcher.RunAsync(CoreDispatcherPriority.Low,
                    () => findAndReplace.Focus()));
        }

        private void FindAndReplaceControl_OnFindAndReplaceButtonClicked(object sender, FindAndReplaceEventArgs e)
        {
            var selectedTextEditor = NotepadsCore.GetSelectedTextEditor();
            if (selectedTextEditor == null) return;

            NotepadsCore.FocusOnSelectedTextEditor();

            bool found = false;

            switch (e.FindAndReplaceMode)
            {
                case FindAndReplaceMode.FindOnly:
                    found = selectedTextEditor.FindNextAndSelect(e.SearchText, e.MatchCase, e.MatchWholeWord, false);
                    break;
                case FindAndReplaceMode.Replace:
                    found = selectedTextEditor.FindNextAndReplace(e.SearchText, e.ReplaceText, e.MatchCase, e.MatchWholeWord);
                    break;
                case FindAndReplaceMode.ReplaceAll:
                    found = selectedTextEditor.FindAndReplaceAll(e.SearchText, e.ReplaceText, e.MatchCase, e.MatchWholeWord);
                    break;
            }

            if (!found)
            {
                ShowInAppNotificationMessage(_resourceLoader.GetString("FindAndReplace_NotificationMsg_NotFound"), 1500);
            }
        }

        private void FindAndReplacePlaceholder_Closed(object sender, Microsoft.Toolkit.Uwp.UI.Controls.InAppNotificationClosedEventArgs e)
        {
            FindAndReplacePlaceholder.Visibility = Visibility.Collapsed;
        }

        private void FindAndReplaceControl_OnDismissKeyDown(object sender, RoutedEventArgs e)
        {
            FindAndReplacePlaceholder.Dismiss();
            NotepadsCore.FocusOnSelectedTextEditor();
        }

        #endregion

        #region InAppNotification

        private void ShowInAppNotificationMessage(string message, int duration)
        {
            if (StatusNotification == null)
            {
                FindName("StatusNotification"); // Lazy loading
            }
            var textSize = FontUtility.GetTextSize(StatusNotification.FontFamily, StatusNotification.FontSize, message);
            StatusNotification.Width = textSize.Width + 100;  // actual width + padding
            StatusNotification.Height = textSize.Height + 50; // actual height + padding
            StatusNotification.Show(message, duration);
        }

        #endregion

        #region Main Menu

        private void MainMenuButtonFlyout_Opening(object sender, object e)
        {
            if (NotepadsCore.GetNumberOfOpenedTextEditors() == 0)
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

        private void MenuButton_OnTapped(object sender, TappedRoutedEventArgs e)
        {
            FlyoutBase.ShowAttachedFlyout((FrameworkElement)sender);
        }

        private void MainMenuButtonFlyout_Closing(FlyoutBase sender, FlyoutBaseClosingEventArgs args)
        {
            NotepadsCore.FocusOnSelectedTextEditor();
        }

        private async void MenuOpenFileButton_OnClick(object sender, RoutedEventArgs e)
        {
            await OpenNewFiles();
        }

        private void MenuCreateNewButton_OnClick(object sender, RoutedEventArgs e)
        {
            NotepadsCore.OpenNewTextEditor();
        }

        private async void MenuSaveButton_OnClick(object sender, RoutedEventArgs e)
        {
            await Save(NotepadsCore.GetSelectedTextEditor(), false);
        }

        private async void MenuSaveAsButton_OnClick(object sender, RoutedEventArgs e)
        {
            await Save(NotepadsCore.GetSelectedTextEditor(), true);
        }

        private void MenuOpenFindButton_OnClick(object sender, RoutedEventArgs e)
        {
            ShowFindAndReplaceControl(false);
        }

        private void MenuOpenFindAndReplaceButton_OnClick(object sender, RoutedEventArgs e)
        {
            ShowFindAndReplaceControl(true);
        }

        private void MenuSettingsButton_OnClick(object sender, RoutedEventArgs e)
        {
            RootSplitView.IsPaneOpen = true;
        }

        #endregion

        #region NotepadsCore Events

        private void OnTextEditorLoaded(object sender, TextEditor textEditor)
        {
            if (NotepadsCore.GetSelectedTextEditor() == textEditor)
            {
                SetupStatusBar(textEditor);
                NotepadsCore.FocusOnSelectedTextEditor();
            }
        }

        private void OnTextEditorUnloaded(object sender, TextEditor textEditor)
        {
            if (FindAndReplacePlaceholder != null)
            {
                if (FindAndReplacePlaceholder.Visibility == Visibility.Visible) FindAndReplacePlaceholder.Dismiss();
            }
            if (NotepadsCore.GetNumberOfOpenedTextEditors() == 0)
            {
                Application.Current.Exit();
            }
        }

        private async void OnTextEditorClosingWithUnsavedContent(object sender, TextEditor textEditor)
        {
            var file = (textEditor.EditingFile != null ? textEditor.EditingFile.Path : _defaultNewFileName);
            await ContentDialogFactory.GetSetCloseSaveReminderDialog(file, async () =>
            {
                if (await Save(textEditor, false))
                {
                    NotepadsCore.DeleteTextEditor(textEditor);
                }
                NotepadsCore.FocusOnSelectedTextEditor();
            }, () =>
            {
                NotepadsCore.DeleteTextEditor(textEditor);
            }).ShowAsync();
        }

        private async void OnTextEditor_KeyDown(object sender, KeyRoutedEventArgs e)
        {
            if (!(sender is TextEditor textEditor)) return;
            // ignoring key events coming from inactive text editors
            if (NotepadsCore.GetSelectedTextEditor() != textEditor) return;

            var ctrl = Window.Current.CoreWindow.GetKeyState(VirtualKey.Control);
            var alt = Window.Current.CoreWindow.GetKeyState(VirtualKey.Menu);
            var shift = Window.Current.CoreWindow.GetKeyState(VirtualKey.Shift);

            if (ctrl.HasFlag(CoreVirtualKeyStates.Down) && !alt.HasFlag(CoreVirtualKeyStates.Down))
            {
                if (e.Key == VirtualKey.Tab)
                {
                    NotepadsCore.SwitchTo(!shift.HasFlag(CoreVirtualKeyStates.Down));
                    e.Handled = true;
                }

                if (e.Key == VirtualKey.N || e.Key == VirtualKey.T)
                {
                    NotepadsCore.OpenNewTextEditor();
                    e.Handled = true;
                }

                if (e.Key == VirtualKey.O)
                {
                    await OpenNewFiles();
                    e.Handled = true;
                }

                if (e.Key == VirtualKey.S)
                {
                    if (shift.HasFlag(CoreVirtualKeyStates.Down))
                    {
                        await Save(NotepadsCore.GetSelectedTextEditor(), true);
                        e.Handled = true;
                    }
                    else
                    {
                        await Save(NotepadsCore.GetSelectedTextEditor(), false);
                        e.Handled = true;
                    }
                }

                if (e.Key == VirtualKey.F)
                {
                    if (shift.HasFlag(CoreVirtualKeyStates.Down))
                    {
                        ShowFindAndReplaceControl(true);
                    }
                    else
                    {
                        ShowFindAndReplaceControl(false);
                    }
                    return;
                }

                if (e.Key == VirtualKey.H)
                {
                    ShowFindAndReplaceControl(true);
                    return;
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

        #endregion

        #region I/O

        private async Task OpenNewFiles()
        {
            var files = await FilePickerFactory.GetFileOpenPicker().PickMultipleFilesAsync();
            if (files == null || files.Count == 0)
            {
                NotepadsCore.FocusOnSelectedTextEditor();
                return;
            }

            foreach (var file in files)
            {
                await OpenFile(file);
            }
        }

        public async Task<bool> OpenFile(StorageFile file)
        {
            try
            {
                await NotepadsCore.OpenNewTextEditor(file);
                NotepadsCore.FocusOnSelectedTextEditor();
                return true;
            }
            catch (Exception ex)
            {
                await ContentDialogFactory.GetFileOpenErrorDialog(file, ex).ShowAsync();
                NotepadsCore.FocusOnSelectedTextEditor();
                return false;
            }
        }

        public async Task<int> OpenFiles(IReadOnlyList<IStorageItem> storageItems)
        {
            if (storageItems == null || storageItems.Count == 0) return 0;

            int successCount = 0;
            foreach (var storageItem in storageItems)
            {
                if (storageItem is StorageFile file)
                {
                    if (await OpenFile(file))
                    {
                        successCount++;
                    }
                }
            }
            return successCount;
        }

        private async Task<bool> Save(TextEditor textEditor, bool saveAs)
        {
            if (textEditor == null) return false;

            StorageFile file;

            if (textEditor.EditingFile == null || saveAs ||
                FileSystemUtility.IsFileReadOnly(textEditor.EditingFile) ||
                !await FileSystemUtility.FileIsWritable(textEditor.EditingFile))
            {
                file = await FilePickerFactory.GetFileSavePicker(textEditor, _defaultNewFileName, saveAs).PickSaveFileAsync();
                textEditor.Focus(FocusState.Programmatic);
            }
            else
            {
                file = textEditor.EditingFile;
            }

            if (file == null) return false;

            var success = await NotepadsCore.SaveTextEditorContentToFile(textEditor, file);

            if (success)
            {
                if (NotepadsCore.GetSelectedTextEditor() == textEditor)
                {
                    SetupStatusBar(textEditor);
                }
                ShowInAppNotificationMessage(_resourceLoader.GetString("TextEditor_NotificationMsg_FileSaved"), 2000);
            }
            else
            {
                await ContentDialogFactory.GetFileSaveErrorDialog(file).ShowAsync();
            }

            return success;
        }

        #endregion
    }
}
