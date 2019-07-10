
namespace Notepads
{
    using Notepads.Controls.FindAndReplace;
    using Notepads.Controls.Settings;
    using Notepads.Controls.TextEditor;
    using Notepads.Core;
    using Notepads.EventArgs;
    using Notepads.Services;
    using Notepads.Utilities;
    using SetsView;
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
                    _notepadsCore.OnActiveTextEditorLoaded += OnActiveTextEditorLoaded;
                    _notepadsCore.OnActiveTextEditorUnloaded += OnActiveTextEditorUnloaded;
                    _notepadsCore.OnActiveTextEditorSelectionChanged += OnActiveTextEditorSelectionChanged;
                    _notepadsCore.OnActiveTextEditorEncodingChanged += OnActiveTextEditorEncodingChanged;
                    _notepadsCore.OnActiveTextEditorLineEndingChanged += OnActiveTextEditorLineEndingChanged;
                    _notepadsCore.OnActiveTextEditorKeyDown += OnActiveTextEditor_KeyDown;
                }

                return _notepadsCore;
            }
        }

        public MainPage()
        {
            InitializeComponent();

            _defaultNewFileName = _resourceLoader.GetString("TextEditor_DefaultNewFileName");

            // Set custom Title Bar
            Window.Current.SetTitleBar(AppTitleBar);

            // Setup status bar
            ShowHideStatusBar(EditorSettingsService.ShowStatusBar);

            // Setup theme
            ThemeSettingsService.AppBackground = RootGrid;
            ThemeSettingsService.SetRequestedTheme();

            EditorSettingsService.OnStatusBarVisibilityChanged += (sender, visibility) => ShowHideStatusBar(visibility);

            Windows.ApplicationModel.DataTransfer.DataTransferManager.GetForCurrentView().DataRequested += MainPage_DataRequested;
            Windows.UI.Core.Preview.SystemNavigationManagerPreview.GetForCurrentView().CloseRequested += MainPage_CloseRequested;

            Window.Current.VisibilityChanged += WindowVisibilityChangedEventHandler;

            NewSetButton.Click += delegate { NotepadsCore.CreateNewTextEditor(); };
            RootSplitView.PaneOpening += delegate
            {
                SettingsFrame.Navigate(typeof(SettingsPage), null, new SuppressNavigationTransitionInfo());
            };
            RootSplitView.PaneClosed += delegate { NotepadsCore.FocusOnActiveTextEditor(); };
        }

        #region Application Life Cycle

        void WindowVisibilityChangedEventHandler(System.Object sender, Windows.UI.Core.VisibilityChangedEventArgs e)
        {
            // Perform operations that should take place when the application becomes visible rather than
            // when it is prelaunched, such as building a what's new feed
        }

        // Content sharing
        private void MainPage_DataRequested(DataTransferManager sender, DataRequestedEventArgs args)
        {
            var textEditor = NotepadsCore.GetActiveTextEditor();
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

        #endregion

        #region Status Bar

        private void SetupStatusBar(TextEditor textEditor)
        {
            PathIndicator.Text = textEditor.EditingFile != null ? textEditor.EditingFile.Path : _defaultNewFileName;
            UpdateLineEndingIndicatorText(textEditor.LineEnding);
            UpdateEncodingIndicatorText(textEditor.Encoding);
            textEditor.GetCurrentLineColumn(out var line, out var column, out var selectedCount);
            LineColumnIndicator.Text = selectedCount == 0 ? $"Ln {line} Col {column}" : $"Ln {line} Col {column} ({selectedCount} selected)";
        }

        public void ShowHideStatusBar(bool showStatusBar)
        {
            if (showStatusBar)
            {
                if (StatusBar == null)
                {
                    this.FindName("StatusBar"); // Lazy loading   
                }
                var activeTextEditor = NotepadsCore.GetActiveTextEditor();
                if (activeTextEditor != null)
                {
                    SetupStatusBar(activeTextEditor);
                }
            }
            else
            {
                if (StatusBar != null)
                {
                    // If VS cannot find UnloadObject, ignore it. Reference: https://github.com/MicrosoftDocs/windows-uwp/issues/734
                    this.UnloadObject(StatusBar);
                }
            }
        }

        private void UpdateEncodingIndicatorText(Encoding encoding)
        {
            EncodingIndicator.Text = EncodingUtility.GetEncodingBodyName(encoding);
        }

        private void UpdateLineEndingIndicatorText(LineEnding lineEnding)
        {
            LineEndingIndicator.Text = LineEndingUtility.GetLineEndingDisplayText(lineEnding);
        }

        private void LineEndingSelection_OnClick(object sender, RoutedEventArgs e)
        {
            if (!(sender is MenuFlyoutItem item)) return;

            var lineEnding = LineEndingUtility.GetLineEndingByName((string)item.Tag);

            var textEditor = NotepadsCore.GetActiveTextEditor();
            if (textEditor != null)
            {
                NotepadsCore.ChangeLineEnding(textEditor, lineEnding);
            }

            if (StatusBar != null)
            {
                UpdateLineEndingIndicatorText(lineEnding);
            }
        }

        private void EncodingSelection_OnClick(object sender, RoutedEventArgs e)
        {
            if (!(sender is MenuFlyoutItem item)) return;

            var encoding = EncodingUtility.GetEncodingByName((string)item.Tag);

            var textEditor = NotepadsCore.GetActiveTextEditor();
            if (textEditor != null)
            {
                NotepadsCore.ChangeEncoding(textEditor, encoding);
            }

            if (StatusBar != null)
            {
                UpdateEncodingIndicatorText(encoding);
            }
        }

        private void StatusBarComponent_OnTapped(object sender, TappedRoutedEventArgs e)
        {
            if (sender == PathIndicator && !string.IsNullOrEmpty(PathIndicator.Text))
            {
                NotepadsCore.FocusOnActiveTextEditor();
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
            NotepadsCore.FocusOnActiveTextEditor();
        }

        #endregion

        #region Find & Replace

        private void ShowFindAndReplaceControl(bool showReplaceBar)
        {
            if (FindAndReplacePlaceholder == null)
            {
                this.FindName("FindAndReplacePlaceholder"); // Lazy loading
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
            var activeTextEditor = NotepadsCore.GetActiveTextEditor();
            if (activeTextEditor == null) return;

            NotepadsCore.FocusOnActiveTextEditor();

            bool found = false;

            switch (e.FindAndReplaceMode)
            {
                case FindAndReplaceMode.FindOnly:
                    found = activeTextEditor.FindNextAndSelect(e.SearchText, e.MatchCase, e.MatchWholeWord, false);
                    break;
                case FindAndReplaceMode.Replace:
                    found = activeTextEditor.FindNextAndReplace(e.SearchText, e.ReplaceText, e.MatchCase, e.MatchWholeWord);
                    break;
                case FindAndReplaceMode.ReplaceAll:
                    found = activeTextEditor.FindAndReplaceAll(e.SearchText, e.ReplaceText, e.MatchCase, e.MatchWholeWord);
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
            NotepadsCore.FocusOnActiveTextEditor();
        }

        #endregion

        #region InAppNotification

        private void ShowInAppNotificationMessage(string message, int duration)
        {
            if (StatusNotification == null)
            {
                this.FindName("StatusNotification"); // Lazy loading
            }
            var textSize = FontUtility.GetTextSize(StatusNotification.FontFamily, StatusNotification.FontSize, message);
            StatusNotification.Width = textSize.Width + 100;  // actual width + padding
            StatusNotification.Height = textSize.Height + 50; // actual height + padding
            StatusNotification.Show(message, duration);
        }

        #endregion

        #region SetsView

        // Open files from external links or cmd args - load time
        private async void Sets_Loaded(object sender, RoutedEventArgs e)
        {
            if (_appLaunchFiles != null && _appLaunchFiles.Count > 0)
            {
                var success = false;
                foreach (var storageItem in _appLaunchFiles)
                {
                    if (storageItem is StorageFile file)
                    {
                        if (await OpenFile(file))
                        {
                            success = true;
                        }
                    }
                }

                if (!success)
                {
                    NotepadsCore.CreateNewTextEditor();
                }

                _appLaunchFiles = null;
            }
            else if (_appLaunchCmdDir != null)
            {
                var file = await FileSystemUtility.OpenFileFromCommandLine(_appLaunchCmdDir, _appLaunchCmdArgs);
                if (file == null)
                {
                    NotepadsCore.CreateNewTextEditor();
                }
                else
                {
                    var success = await OpenFile(file);
                    if (!success)
                    {
                        NotepadsCore.CreateNewTextEditor();
                    }
                }

                _appLaunchCmdDir = null;
                _appLaunchCmdArgs = null;
            }
            else if (!_loaded)
            {
                NotepadsCore.CreateNewTextEditor();
                _loaded = true;
            }
        }

        private async void SetsView_OnSetClosing(object sender, SetClosingEventArgs e)
        {
            if (!(e.Set.Content is TextEditor textEditor)) return;
            if (textEditor.Saved) return;

            e.Cancel = true;

            var file = (textEditor.EditingFile != null ? textEditor.EditingFile.Path : _defaultNewFileName);
            await ContentDialogFactory.GetSetCloseSaveReminderDialog(file, () =>
            {
                Save(textEditor, false);
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

        #endregion

        #region Main Menu

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
        private void MenuButton_OnTapped(object sender, TappedRoutedEventArgs e)
        {
            FlyoutBase.ShowAttachedFlyout((FrameworkElement)sender);
        }

        private void MainMenuButtonFlyout_Closing(FlyoutBase sender, FlyoutBaseClosingEventArgs args)
        {
            NotepadsCore.FocusOnActiveTextEditor();
        }

        private async void MenuOpenFileButton_OnClick(object sender, RoutedEventArgs e)
        {
            await OpenNewFiles();
        }

        private void MenuCreateNewButton_OnClick(object sender, RoutedEventArgs e)
        {
            NotepadsCore.CreateNewTextEditor();
        }

        private void MenuSaveButton_OnClick(object sender, RoutedEventArgs e)
        {
            Save(NotepadsCore.GetActiveTextEditor(), false);
        }

        private void MenuSaveAsButton_OnClick(object sender, RoutedEventArgs e)
        {
            Save(NotepadsCore.GetActiveTextEditor(), true);
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

        private void OnActiveTextEditorLoaded(object sender, TextEditor textEditor)
        {
            if (StatusBar != null)
            {
                SetupStatusBar(textEditor);
            }
        }

        private void OnActiveTextEditorUnloaded(object sender, TextEditor textEditor)
        {
            if (FindAndReplacePlaceholder != null)
            {
                if (FindAndReplacePlaceholder.Visibility == Visibility.Visible) FindAndReplacePlaceholder.Dismiss();
            }
        }

        private void OnActiveTextEditorSelectionChanged(object sender, TextEditor textEditor)
        {
            if (StatusBar == null) return;
            textEditor.GetCurrentLineColumn(out var line, out var column, out var selectedCount);
            LineColumnIndicator.Text = selectedCount == 0 ? $"Ln {line} Col {column}" : $"Ln {line} Col {column} ({selectedCount} selected)";
        }

        private void OnActiveTextEditorEncodingChanged(object sender, TextEditor textEditor)
        {
            if (StatusBar != null)
            {
                UpdateEncodingIndicatorText(textEditor.Encoding);
            }
        }

        private void OnActiveTextEditorLineEndingChanged(object sender, TextEditor textEditor)
        {
            if (StatusBar != null)
            {
                UpdateLineEndingIndicatorText(textEditor.LineEnding);
            }
        }

        private void OnActiveTextEditor_KeyDown(object sender, KeyRoutedEventArgs e)
        {
            if (!(sender is TextEditor textEditor)) return;

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
                    NotepadsCore.CreateNewTextEditor();
                    e.Handled = true;
                }

                if (e.Key == VirtualKey.O)
                {
                    MenuOpenFileButton_OnClick(this, null);
                    e.Handled = true;
                }

                if (e.Key == VirtualKey.S)
                {
                    if (shift.HasFlag(CoreVirtualKeyStates.Down))
                    {
                        Save(NotepadsCore.GetActiveTextEditor(), true);
                        e.Handled = true;
                    }
                    else
                    {
                        Save(NotepadsCore.GetActiveTextEditor(), false);
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
                NotepadsCore.FocusOnActiveTextEditor();
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
                await NotepadsCore.Open(file);
                return true;
            }
            catch (Exception ex)
            {
                await ContentDialogFactory.GetFileOpenErrorDialog(file, ex).ShowAsync();
                NotepadsCore.FocusOnActiveTextEditor();
                return false;
            }
        }

        private async void Save(TextEditor textEditor, bool saveAs)
        {
            if (textEditor == null) return;

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

            if (file == null) return;

            var success = await NotepadsCore.Save(textEditor, file);

            if (success)
            {
                if (StatusBar != null)
                {
                    SetupStatusBar(textEditor);
                }
                ShowInAppNotificationMessage(_resourceLoader.GetString("TextEditor_NotificationMsg_FileSaved"), 2000);
            }
            else
            {
                await ContentDialogFactory.GetFileSaveErrorDialog(file).ShowAsync();
            }
        }

        #endregion
    }
}
