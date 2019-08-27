
namespace Notepads
{
    using Microsoft.AppCenter.Analytics;
    using Notepads.Commands;
    using Notepads.Controls.Settings;
    using Notepads.Controls.TextEditor;
    using Notepads.Core;
    using Notepads.Extensions;
    using Notepads.Services;
    using Notepads.Settings;
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
    using Windows.UI.ViewManagement;
    using Windows.UI.Xaml;
    using Windows.UI.Xaml.Controls;
    using Windows.UI.Xaml.Controls.Primitives;
    using Windows.UI.Xaml.Input;
    using Windows.UI.Xaml.Media;
    using Windows.UI.Xaml.Media.Animation;
    using Windows.UI.Xaml.Navigation;

    public sealed partial class MainPage : Page, INotificationDelegate
    {
        private readonly string _defaultNewFileName;

        private IReadOnlyList<IStorageItem> _appLaunchFiles;

        private string _appLaunchCmdDir;

        private string _appLaunchCmdArgs;

        private Uri _appLaunchUri;

        private readonly ResourceLoader _resourceLoader = ResourceLoader.GetForCurrentView();

        private bool _loaded = false;

        private const int TitleBarReservedAreaDefaultWidth = 180;

        private const int TitleBarReservedAreaCompactOverlayWidth = 100;

        private INotepadsCore _notepadsCore;

        private INotepadsCore NotepadsCore
        {
            get
            {
                if (_notepadsCore == null)
                {
                    _notepadsCore = new NotepadsCore(Sets, _resourceLoader.GetString("TextEditor_DefaultNewFileName"), new NotepadsExtensionProvider());
                    _notepadsCore.StorageItemsDropped += OnStorageItemsDropped;
                    _notepadsCore.TextEditorLoaded += OnTextEditorLoaded;
                    _notepadsCore.TextEditorUnloaded += OnTextEditorUnloaded;
                    _notepadsCore.TextEditorKeyDown += OnTextEditor_KeyDown;
                    _notepadsCore.TextEditorClosingWithUnsavedContent += OnTextEditorClosingWithUnsavedContent;
                    _notepadsCore.TextEditorSelectionChanged += (sender, editor) => { if (NotepadsCore.GetSelectedTextEditor() == editor) UpdateLineColumnIndicator(editor); };
                    _notepadsCore.TextEditorEncodingChanged += (sender, editor) => { if (NotepadsCore.GetSelectedTextEditor() == editor) UpdateEncodingIndicator(editor.GetEncoding()); };
                    _notepadsCore.TextEditorLineEndingChanged += (sender, editor) => { if (NotepadsCore.GetSelectedTextEditor() == editor) UpdateLineEndingIndicator(editor.GetLineEnding()); };
                    _notepadsCore.TextEditorEditorModificationStateChanged += (sender, editor) => { if (NotepadsCore.GetSelectedTextEditor() == editor) SetupStatusBar(editor); };
                    _notepadsCore.TextEditorFileModificationStateChanged += (sender, editor) =>
                    {
                        if (NotepadsCore.GetSelectedTextEditor() == editor)
                        {
                            if (editor.FileModificationState == FileModificationState.Modified)
                            {
                                NotificationCenter.Instance.PostNotification(_resourceLoader.GetString("TextEditor_FileModifiedOutsideIndicator_ToolTip"), 3500);
                            }
                            else if (editor.FileModificationState == FileModificationState.RenamedMovedOrDeleted)
                            {
                                NotificationCenter.Instance.PostNotification(_resourceLoader.GetString("TextEditor_FileRenamedMovedOrDeletedIndicator_ToolTip"), 3500);
                            }
                            UpdateFileModificationStateIndicator(editor);
                            UpdatePathIndicator(editor);
                        }
                    };
                    _notepadsCore.TextEditorSaved += (sender, editor) =>
                    {
                        if (NotepadsCore.GetSelectedTextEditor() == editor)
                        {
                            SetupStatusBar(editor);
                        }
                        NotificationCenter.Instance.PostNotification(_resourceLoader.GetString("TextEditor_NotificationMsg_FileSaved"), 1500);
                    };
                }

                return _notepadsCore;
            }
        }

        private readonly IKeyboardCommandHandler<KeyRoutedEventArgs> _keyboardCommandHandler;

        private ISessionManager _sessionManager;

        private ISessionManager SessionManager => _sessionManager ?? (_sessionManager = SessionUtility.GetSessionManager(NotepadsCore));


        public MainPage()
        {
            InitializeComponent();

            _defaultNewFileName = _resourceLoader.GetString("TextEditor_DefaultNewFileName");

            NotificationCenter.Instance.SetNotificationDelegate(this);

            // Setup theme
            ThemeSettingsService.AppBackground = RootGrid;
            ThemeSettingsService.SetRequestedTheme();

            // Setup custom Title Bar
            Window.Current.SetTitleBar(AppTitleBar);

            // Setup status bar
            ShowHideStatusBar(EditorSettingsService.ShowStatusBar);
            EditorSettingsService.OnStatusBarVisibilityChanged += (sender, visibility) =>
            {
                if (ApplicationView.GetForCurrentView().ViewMode != ApplicationViewMode.CompactOverlay) ShowHideStatusBar(visibility);
            };

            // Session backup and restore toggle
            EditorSettingsService.OnSessionBackupAndRestoreOptionChanged += (sender, isSessionBackupAndRestoreEnabled) =>
            {
                if (isSessionBackupAndRestoreEnabled)
                {
                    SessionManager.IsBackupEnabled = true;
                    SessionManager.StartSessionBackup(startImmediately: true);
                }
                else
                {
                    SessionManager.IsBackupEnabled = false;
                    SessionManager.StopSessionBackup();
                    SessionManager.ClearSessionData();
                }
            };

            // Sharing
            Windows.ApplicationModel.DataTransfer.DataTransferManager.GetForCurrentView().DataRequested += MainPage_DataRequested;
            Windows.UI.Core.Preview.SystemNavigationManagerPreview.GetForCurrentView().CloseRequested += MainPage_CloseRequested;

            Window.Current.VisibilityChanged += WindowVisibilityChangedEventHandler;
            Window.Current.SizeChanged += WindowSizeChanged;

            InitControls();

            // Init shortcuts
            _keyboardCommandHandler = GetKeyboardCommandHandler();
        }

        private void InitControls()
        {
            ToolTipService.SetToolTip(ExitCompactOverlayButton, _resourceLoader.GetString("App_ExitCompactOverlayMode_Text"));
            RootSplitView.PaneOpening += delegate { SettingsFrame.Navigate(typeof(SettingsPage), null, new SuppressNavigationTransitionInfo()); };
            RootSplitView.PaneClosed += delegate { NotepadsCore.FocusOnSelectedTextEditor(); };
            NewSetButton.Click += delegate { NotepadsCore.OpenNewTextEditor(); };
            MainMenuButton.Click += (sender, args) => FlyoutBase.ShowAttachedFlyout((FrameworkElement)sender);
            MenuCreateNewButton.Click += (sender, args) => NotepadsCore.OpenNewTextEditor();
            MenuOpenFileButton.Click += async (sender, args) => await OpenNewFiles();
            MenuSaveButton.Click += async (sender, args) => await Save(NotepadsCore.GetSelectedTextEditor(), saveAs: false);
            MenuSaveAsButton.Click += async (sender, args) => await Save(NotepadsCore.GetSelectedTextEditor(), saveAs: true);
            MenuSaveAllButton.Click += async (sender, args) => { foreach (var textEditor in NotepadsCore.GetAllTextEditors()) await Save(textEditor, saveAs: false, ignoreUnmodifiedDocument: true); };
            MenuFindButton.Click += (sender, args) => NotepadsCore.GetSelectedTextEditor()?.ShowFindAndReplaceControl(showReplaceBar: false);
            MenuReplaceButton.Click += (sender, args) => NotepadsCore.GetSelectedTextEditor()?.ShowFindAndReplaceControl(showReplaceBar: true);
            MenuFullScreenButton.Click += (sender, args) => EnterExitFullScreenMode();
            MenuCompactOverlayButton.Click += (sender, args) => EnterExitCompactOverlayMode();
            MenuSettingsButton.Click += (sender, args) => RootSplitView.IsPaneOpen = true;

            MainMenuButtonFlyout.Opening += (sender, o) =>
            {
                var selectedTextEditor = NotepadsCore.GetSelectedTextEditor();
                if (selectedTextEditor == null)
                {
                    MenuSaveButton.IsEnabled = false;
                    MenuSaveAsButton.IsEnabled = false;
                    MenuFindButton.IsEnabled = false;
                    MenuReplaceButton.IsEnabled = false;
                    //MenuPrintButton.IsEnabled = false;
                }
                else if (selectedTextEditor.IsEditorEnabled() == false)
                {
                    MenuSaveButton.IsEnabled = selectedTextEditor.IsModified;
                    MenuSaveAsButton.IsEnabled = true;
                    MenuFindButton.IsEnabled = false;
                    MenuReplaceButton.IsEnabled = false;
                    //MenuPrintButton.IsEnabled = true;
                }
                else
                {
                    MenuSaveButton.IsEnabled = selectedTextEditor.IsModified;
                    MenuSaveAsButton.IsEnabled = true;
                    MenuFindButton.IsEnabled = true;
                    MenuReplaceButton.IsEnabled = true;
                    //MenuPrintButton.IsEnabled = true;
                }

                MenuFullScreenButton.Text = _resourceLoader.GetString(ApplicationView.GetForCurrentView().IsFullScreenMode ?
                    "App_ExitFullScreenMode_Text" : "App_EnterFullScreenMode_Text");
                MenuCompactOverlayButton.Text = _resourceLoader.GetString(ApplicationView.GetForCurrentView().ViewMode == ApplicationViewMode.CompactOverlay ?
                    "App_ExitCompactOverlayMode_Text" : "App_EnterCompactOverlayMode_Text");
                MenuSaveAllButton.IsEnabled = NotepadsCore.HaveUnsavedTextEditor();
            };

            if (!App.IsFirstInstance)
            {
                MainMenuButton.Foreground = new SolidColorBrush(ThemeSettingsService.AppAccentColor);
                MenuSettingsButton.IsEnabled = false;
            }
        }

        private KeyboardCommandHandler GetKeyboardCommandHandler()
        {
            return new KeyboardCommandHandler(new List<IKeyboardCommand<KeyRoutedEventArgs>>()
            {
                new KeyboardShortcut<KeyRoutedEventArgs>(true, false, false, VirtualKey.W, (args) => NotepadsCore.CloseTextEditor(NotepadsCore.GetSelectedTextEditor())),
                new KeyboardShortcut<KeyRoutedEventArgs>(true, false, false, VirtualKey.Tab, (args) => NotepadsCore.SwitchTo(true)),
                new KeyboardShortcut<KeyRoutedEventArgs>(true, false, true, VirtualKey.Tab, (args) => NotepadsCore.SwitchTo(false)),
                new KeyboardShortcut<KeyRoutedEventArgs>(true, false, false, VirtualKey.N, (args) => NotepadsCore.OpenNewTextEditor()),
                new KeyboardShortcut<KeyRoutedEventArgs>(true, false, false, VirtualKey.T, (args) => NotepadsCore.OpenNewTextEditor()),
                new KeyboardShortcut<KeyRoutedEventArgs>(true, false, false, VirtualKey.O, async (args) => await OpenNewFiles()),
                new KeyboardShortcut<KeyRoutedEventArgs>(true, false, false, VirtualKey.S, async (args) => await Save(NotepadsCore.GetSelectedTextEditor(), saveAs: false, ignoreUnmodifiedDocument: true)),
                new KeyboardShortcut<KeyRoutedEventArgs>(true, false, true, VirtualKey.S, async (args) => await Save(NotepadsCore.GetSelectedTextEditor(), saveAs: true)),
                new KeyboardShortcut<KeyRoutedEventArgs>(true, false, true, VirtualKey.R, (args) => { ReloadFileFromDisk_OnClick(this, new RoutedEventArgs()); }),
                new KeyboardShortcut<KeyRoutedEventArgs>(VirtualKey.Tab, (args) => NotepadsCore.GetSelectedTextEditor()?.TypeTab()),
                new KeyboardShortcut<KeyRoutedEventArgs>(VirtualKey.F11, (args) => { EnterExitFullScreenMode(); }),
                new KeyboardShortcut<KeyRoutedEventArgs>(VirtualKey.F12, (args) => { EnterExitCompactOverlayMode(); }),
            });
        }

        #region View Mode Switches

        // Show hide ExitCompactOverlayButton and status bar based on current ViewMode
        // Reset TitleBarReservedArea accordingly
        private void WindowSizeChanged(object sender, Windows.UI.Core.WindowSizeChangedEventArgs e)
        {
            if (ApplicationView.GetForCurrentView().ViewMode == ApplicationViewMode.CompactOverlay)
            {
                if (ExitCompactOverlayButton.Visibility == Visibility.Collapsed)
                {
                    TitleBarReservedArea.Width = TitleBarReservedAreaCompactOverlayWidth;
                    ExitCompactOverlayButton.Visibility = Visibility.Visible;
                    if (EditorSettingsService.ShowStatusBar) ShowHideStatusBar(false);
                }
            }
            else // Default or FullScreen
            {
                if (ExitCompactOverlayButton.Visibility == Visibility.Visible)
                {
                    TitleBarReservedArea.Width = TitleBarReservedAreaDefaultWidth;
                    ExitCompactOverlayButton.Visibility = Visibility.Collapsed;
                    if (EditorSettingsService.ShowStatusBar) ShowHideStatusBar(true);
                }
            }
        }

        private async void EnterExitCompactOverlayMode()
        {
            if (ApplicationView.GetForCurrentView().ViewMode == ApplicationViewMode.Default)
            {
                var modeSwitched = await ApplicationView.GetForCurrentView().TryEnterViewModeAsync(ApplicationViewMode.CompactOverlay);
                if (!modeSwitched)
                {
                    LoggingService.LogError("Failed to enter CompactOverlay view mode.");
                    Analytics.TrackEvent("FailedToEnterCompactOverlayViewMode");
                }
            }
            else if (ApplicationView.GetForCurrentView().ViewMode == ApplicationViewMode.CompactOverlay)
            {
                var modeSwitched = await ApplicationView.GetForCurrentView().TryEnterViewModeAsync(ApplicationViewMode.Default);
                if (!modeSwitched)
                {
                    LoggingService.LogError("Failed to enter Default view mode.");
                    Analytics.TrackEvent("FailedToEnterDefaultViewMode");
                }
            }
        }

        private void EnterExitFullScreenMode()
        {
            if (ApplicationView.GetForCurrentView().IsFullScreenMode)
            {
                LoggingService.LogInfo("Existing full screen view mode.", consoleOnly: true);
                ApplicationView.GetForCurrentView().ExitFullScreenMode();
            }
            else
            {
                if (ApplicationView.GetForCurrentView().TryEnterFullScreenMode())
                {
                    LoggingService.LogInfo("Entered full screen view mode.", consoleOnly: true);
                    NotificationCenter.Instance.PostNotification(_resourceLoader.GetString("TextEditor_NotificationMsg_ExitFullScreenHint"), 3000);
                }
                else
                {
                    LoggingService.LogError("Failed to enter full screen view mode.");
                    Analytics.TrackEvent("FailedToEnterFullScreenViewMode");
                }
            }
        }

        private void ExitCompactOverlayButton_OnClick(object sender, RoutedEventArgs e)
        {
            if (ApplicationView.GetForCurrentView().ViewMode == ApplicationViewMode.CompactOverlay)
            {
                EnterExitCompactOverlayMode();
            }
        }

        #endregion

        #region Application Life Cycle & Window management 

        // Handles external links or cmd args activation before Sets loaded
        protected override void OnNavigatedTo(NavigationEventArgs e)
        {
            base.OnNavigatedTo(e);

            switch (e.Parameter)
            {
                case null:
                    return;
                case FileActivatedEventArgs fileActivatedEventArgs:
                    _appLaunchFiles = fileActivatedEventArgs.Files;
                    break;
                case CommandLineActivatedEventArgs commandLineActivatedEventArgs:
                    _appLaunchCmdDir = commandLineActivatedEventArgs.Operation.CurrentDirectoryPath;
                    _appLaunchCmdArgs = commandLineActivatedEventArgs.Operation.Arguments;
                    break;
                case ProtocolActivatedEventArgs protocol:
                    _appLaunchUri = protocol.Uri;
                    break;
            }
        }

        // App should wait for Sets fully loaded before opening files requested by user (by click or from cmd)
        // Open files from external links or cmd args on Sets Loaded
        private async void Sets_Loaded(object sender, RoutedEventArgs e)
        {
            int loadedCount = 0;

            if (!_loaded && EditorSettingsService.IsSessionSnapshotEnabled)
            {
                loadedCount = await SessionManager.LoadLastSessionAsync();
            }

            if (_appLaunchFiles != null && _appLaunchFiles.Count > 0)
            {
                loadedCount += await OpenFiles(_appLaunchFiles);
                _appLaunchFiles = null;
            }
            else if (_appLaunchCmdDir != null)
            {
                var file = await FileSystemUtility.OpenFileFromCommandLine(_appLaunchCmdDir, _appLaunchCmdArgs);
                if (file != null && await OpenFile(file))
                {
                    loadedCount++;
                }
                _appLaunchCmdDir = null;
                _appLaunchCmdArgs = null;
            }
            else if (_appLaunchUri != null)
            {
                var operation = NotepadsProtocolService.GetOperationProtocol(_appLaunchUri, out var context);
                if (operation == NotepadsOperationProtocol.OpenNewInstance || operation == NotepadsOperationProtocol.Unrecognized)
                {
                    // Do nothing
                }
                _appLaunchUri = null;
            }

            if (!_loaded)
            {
                if (loadedCount == 0)
                {
                    NotepadsCore.OpenNewTextEditor();
                }

                if (!App.IsFirstInstance)
                {
                    NotificationCenter.Instance.PostNotification("This is a shadow instance of Notepads. Session snapshot and settings are disabled.", 4000); //(_resourceLoader.GetString("TextEditor_ShadowInstanceIndicator_Description"), 4000);
                }
                _loaded = true;
            }

            if (EditorSettingsService.IsSessionSnapshotEnabled)
            {
                SessionManager.IsBackupEnabled = true;
                SessionManager.StartSessionBackup();
            }

            Window.Current.CoreWindow.Activated -= CoreWindow_Activated;
            Window.Current.CoreWindow.Activated += CoreWindow_Activated;
            Application.Current.EnteredBackground -= App_EnteredBackground;
            Application.Current.EnteredBackground += App_EnteredBackground;
        }

        private async void App_EnteredBackground(object sender, Windows.ApplicationModel.EnteredBackgroundEventArgs e)
        {
            var deferral = e.GetDeferral();

            if (EditorSettingsService.IsSessionSnapshotEnabled)
            {
                await SessionManager.SaveSessionAsync();
            }

            deferral.Complete();
        }

        public void ExecuteProtocol(Uri uri)
        {
        }

        private void CoreWindow_Activated(Windows.UI.Core.CoreWindow sender, Windows.UI.Core.WindowActivatedEventArgs args)
        {
            if (args.WindowActivationState == Windows.UI.Core.CoreWindowActivationState.Deactivated)
            {
                LoggingService.LogInfo("CoreWindow Deactivated.", consoleOnly: true);
                NotepadsCore.GetSelectedTextEditor()?.StopCheckingFileStatus();
                if (EditorSettingsService.IsSessionSnapshotEnabled)
                {
                    SessionManager.StopSessionBackup();
                }
            }
            else if (args.WindowActivationState == Windows.UI.Core.CoreWindowActivationState.PointerActivated ||
                     args.WindowActivationState == Windows.UI.Core.CoreWindowActivationState.CodeActivated)
            {
                LoggingService.LogInfo("CoreWindow Activated.", consoleOnly: true);
                ApplicationSettingsStore.Write("ActiveInstance", App.Id.ToString());
                NotepadsCore.GetSelectedTextEditor()?.StartCheckingFileStatusPeriodically();
                if (EditorSettingsService.IsSessionSnapshotEnabled)
                {
                    SessionManager.StartSessionBackup();
                }
            }
        }

        void WindowVisibilityChangedEventHandler(System.Object sender, Windows.UI.Core.VisibilityChangedEventArgs e)
        {
            LoggingService.LogInfo($"Window Visibility Changed, Visible = {e.Visible}.", consoleOnly: true);
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
            e.Handled = true;

            if (EditorSettingsService.IsSessionSnapshotEnabled)
            {
                // Save session before app exit
                await SessionManager.SaveSessionAsync();
                Application.Current.Exit();
            }
            else
            {
                if (!NotepadsCore.HaveUnsavedTextEditor())
                {
                    Application.Current.Exit();
                }

                ContentDialog appCloseSaveReminderDialog = ContentDialogFactory.GetAppCloseSaveReminderDialog(
                    async () =>
                    {
                        foreach (var textEditor in NotepadsCore.GetAllTextEditors())
                        {
                            if (await Save(textEditor, saveAs: false, ignoreUnmodifiedDocument: true))
                            {
                                NotepadsCore.DeleteTextEditor(textEditor);
                            }
                        }
                    },
                    () =>
                    {
                        Application.Current.Exit();
                    });

                await ContentDialogMaker.CreateContentDialogAsync(appCloseSaveReminderDialog, awaitPreviousDialog: false);
            }
        }

        private async void OnStorageItemsDropped(object sender, IReadOnlyList<IStorageItem> storageItems)
        {
            foreach (var storageItem in storageItems)
            {
                if (storageItem is StorageFile file)
                {
                    await OpenFile(file);
                }
            }
        }

        #endregion

        #region Status Bar

        private void SetupStatusBar(TextEditor textEditor)
        {
            if (textEditor == null) return;
            UpdateFileModificationStateIndicator(textEditor);
            UpdatePathIndicator(textEditor);
            UpdateEditorModificationIndicator(textEditor);
            UpdateLineColumnIndicator(textEditor);
            UpdateLineEndingIndicator(textEditor.GetLineEnding());
            UpdateEncodingIndicator(textEditor.GetEncoding());
            UpdateShadowInstanceIndicator();
        }

        public void ShowHideStatusBar(bool showStatusBar)
        {
            if (showStatusBar)
            {
                if (StatusBar == null) { FindName("StatusBar"); } // Lazy loading   
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

        private void UpdateFileModificationStateIndicator(TextEditor textEditor)
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
                FileModificationStateIndicator.Visibility = Visibility.Visible;
            }
            else if (textEditor.FileModificationState == FileModificationState.RenamedMovedOrDeleted)
            {
                FileModificationStateIndicatorIcon.Glyph = "\uE9CE"; // Unknown Icon
                ToolTipService.SetToolTip(FileModificationStateIndicator, _resourceLoader.GetString("TextEditor_FileRenamedMovedOrDeletedIndicator_ToolTip"));
                FileModificationStateIndicator.Visibility = Visibility.Visible;
            }
        }

        private void UpdatePathIndicator(TextEditor textEditor)
        {
            if (StatusBar == null) return;
            PathIndicator.Text = textEditor.EditingFilePath ?? _defaultNewFileName;

            if (textEditor.FileModificationState == FileModificationState.Untouched)
            {
                ToolTipService.SetToolTip(PathIndicator, PathIndicator.Text);
            }
            else if (textEditor.FileModificationState == FileModificationState.Modified)
            {
                ToolTipService.SetToolTip(PathIndicator, _resourceLoader.GetString("TextEditor_FileModifiedOutsideIndicator_ToolTip"));
            }
            else if (textEditor.FileModificationState == FileModificationState.RenamedMovedOrDeleted)
            {
                ToolTipService.SetToolTip(PathIndicator, _resourceLoader.GetString("TextEditor_FileRenamedMovedOrDeletedIndicator_ToolTip"));
            }
        }

        private void UpdateEditorModificationIndicator(TextEditor textEditor)
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

        private void UpdateLineColumnIndicator(TextEditor textEditor)
        {
            if (StatusBar == null) return;
            textEditor.GetCurrentLineColumn(out var line, out var column, out var selectedCount);
            LineColumnIndicator.Text = selectedCount == 0
                ? string.Format(_resourceLoader.GetString("TextEditor_LineColumnIndicator_ShortText"), line, column)
                : string.Format(_resourceLoader.GetString("TextEditor_LineColumnIndicator_FullText"), line, column, selectedCount);
        }

        private void UpdateShadowInstanceIndicator()
        {
            if (StatusBar == null) return;
            ShadowInstanceIndicator.Visibility = !App.IsFirstInstance ? Visibility.Visible : Visibility.Collapsed;
        }

        private async void ModificationFlyoutSelection_OnClick(object sender, RoutedEventArgs e)
        {
            if (!(sender is MenuFlyoutItem item)) return;

            var selectedTextEditor = NotepadsCore.GetSelectedTextEditor();
            if (selectedTextEditor == null) return;

            switch ((string)item.Tag)
            {
                case "PreviewTextChanges":
                    NotepadsCore.GetSelectedTextEditor().OpenSideBySideDiffViewer();
                    break;
                case "RevertAllChanges":
                    var fileName = selectedTextEditor.EditingFileName ?? _defaultNewFileName;
                    var setCloseSaveReminderDialog = ContentDialogFactory.GetRevertAllChangesConfirmationDialog(fileName, () =>
                    {
                        selectedTextEditor.CloseSideBySideDiffViewer();
                        NotepadsCore.GetSelectedTextEditor().RevertAllChanges();
                    });
                    await ContentDialogMaker.CreateContentDialogAsync(setCloseSaveReminderDialog, awaitPreviousDialog: true);
                    break;
            }
        }

        private async void ReloadFileFromDisk_OnClick(object sender, RoutedEventArgs e)
        {
            var selectedEditor = NotepadsCore.GetSelectedTextEditor();

            if (selectedEditor?.EditingFile != null && selectedEditor.FileModificationState != FileModificationState.RenamedMovedOrDeleted)
            {
                try
                {
                    var textFile = await FileSystemUtility.ReadFile(selectedEditor.EditingFile, ignoreFileSizeLimit: false);
                    selectedEditor.Init(textFile, selectedEditor.EditingFile, clearUndoQueue: false);
                    selectedEditor.StartCheckingFileStatusPeriodically();
                    selectedEditor.CloseSideBySideDiffViewer();
                    NotificationCenter.Instance.PostNotification(_resourceLoader.GetString("TextEditor_NotificationMsg_FileReloaded"), 1500);
                }
                catch (Exception ex)
                {
                    var fileOpenErrorDialog = ContentDialogFactory.GetFileOpenErrorDialog(selectedEditor.EditingFile.Path, ex.Message);
                    await ContentDialogMaker.CreateContentDialogAsync(fileOpenErrorDialog, awaitPreviousDialog: false);
                    NotepadsCore.FocusOnSelectedTextEditor();
                }
            }
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
                    NotificationCenter.Instance.PostNotification(_resourceLoader.GetString("TextEditor_FileRenamedMovedOrDeletedIndicator_ToolTip"), 2000);
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
                    NotificationCenter.Instance.PostNotification(_resourceLoader.GetString("TextEditor_FileRenamedMovedOrDeletedIndicator_ToolTip"), 2000);
                }
            }
            else if (sender == ModificationIndicator)
            {
                PreviewTextChangesFlyoutItem.IsEnabled = !selectedEditor.NoChangesSinceLastSaved(compareTextOnly: true) && selectedEditor.EditorMode != TextEditorMode.DiffPreview;
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
            else if (sender == ShadowInstanceIndicator)
            {
                NotificationCenter.Instance.PostNotification("This is a shadow instance of Notepads. Session snapshot and settings are disabled.", 4000); //(_resourceLoader.GetString("TextEditor_ShadowInstanceIndicator_Description"), 4000);
            }
        }

        private void StatusBarFlyout_OnClosing(FlyoutBase sender, FlyoutBaseClosingEventArgs args)
        {
            NotepadsCore.FocusOnSelectedTextEditor();
        }

        #endregion

        #region InAppNotification

        public void PostNotification(string message, int duration)
        {
            if (StatusNotification == null) { FindName("StatusNotification"); } // Lazy loading
            var textSize = FontUtility.GetTextSize(StatusNotification.FontFamily, StatusNotification.FontSize, message);
            StatusNotification.Width = textSize.Width + 100;  // actual width + padding
            StatusNotification.Height = textSize.Height + 50; // actual height + padding
            StatusNotification.Show(message, duration);
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
            if (NotepadsCore.GetNumberOfOpenedTextEditors() == 0)
            {
                if (EditorSettingsService.IsSessionSnapshotEnabled)
                {
                    SessionManager.ClearSessionData();
                }
                Application.Current.Exit();
            }
        }

        private async void OnTextEditorClosingWithUnsavedContent(object sender, TextEditor textEditor)
        {
            var file = textEditor.EditingFilePath ?? _defaultNewFileName;

            var setCloseSaveReminderDialog = ContentDialogFactory.GetSetCloseSaveReminderDialog(file, async () =>
            {
                if (await Save(textEditor, saveAs: false))
                {
                    NotepadsCore.DeleteTextEditor(textEditor);
                }
            }, () => { NotepadsCore.DeleteTextEditor(textEditor); });

            setCloseSaveReminderDialog.Opened += (s, a) => { NotepadsCore.SwitchTo(textEditor); };
            await ContentDialogMaker.CreateContentDialogAsync(setCloseSaveReminderDialog, awaitPreviousDialog: true);
            NotepadsCore.FocusOnSelectedTextEditor();
        }

        private void OnTextEditor_KeyDown(object sender, KeyRoutedEventArgs e)
        {
            if (!(sender is TextEditor textEditor)) return;
            // ignoring key events coming from inactive text editors
            if (NotepadsCore.GetSelectedTextEditor() != textEditor) return;
            _keyboardCommandHandler.Handle(e);
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
                await NotepadsCore.OpenNewTextEditor(file, ignoreFileSizeLimit: false);
                NotepadsCore.FocusOnSelectedTextEditor();
                return true;
            }
            catch (Exception ex)
            {
                var fileOpenErrorDialog = ContentDialogFactory.GetFileOpenErrorDialog(file.Path, ex.Message);
                await ContentDialogMaker.CreateContentDialogAsync(fileOpenErrorDialog, awaitPreviousDialog: false);
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

        private async Task<bool> Save(TextEditor textEditor, bool saveAs, bool ignoreUnmodifiedDocument = false)
        {
            if (textEditor == null) return false;

            if (ignoreUnmodifiedDocument && !textEditor.IsModified)
            {
                return true;
            }

            StorageFile file = null;
            try
            {
                if (textEditor.EditingFile == null || saveAs ||
                    FileSystemUtility.IsFileReadOnly(textEditor.EditingFile) ||
                    !await FileSystemUtility.FileIsWritable(textEditor.EditingFile))
                {
                    NotepadsCore.SwitchTo(textEditor);
                    file = await FilePickerFactory.GetFileSavePicker(textEditor, _defaultNewFileName, saveAs).PickSaveFileAsync();
                    _notepadsCore.FocusOnTextEditor(textEditor);
                    if (file == null)
                    {
                        return false; // User cancelled
                    }
                }
                else
                {
                    file = textEditor.EditingFile;
                }

                await NotepadsCore.SaveContentToFileAndUpdateEditorState(textEditor, file);
                return true;
            }
            catch (Exception ex)
            {
                var fileSaveErrorDialog = ContentDialogFactory.GetFileSaveErrorDialog((file == null) ? string.Empty : file.Path, ex.Message);
                await ContentDialogMaker.CreateContentDialogAsync(fileSaveErrorDialog, awaitPreviousDialog: false);
                return false;
            }
        }

        #endregion
    }
}
