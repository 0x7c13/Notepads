﻿namespace Notepads
{
    using System;
    using System.Collections.Generic;
    using Notepads.Commands;
    using Notepads.Controls.Settings;
    using Notepads.Controls.TextEditor;
    using Notepads.Core;
    using Notepads.Extensions;
    using Notepads.Services;
    using Notepads.Settings;
    using Notepads.Utilities;
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

    public sealed partial class NotepadsMainPage : Page, INotificationDelegate
    {
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
                    _notepadsCore = new NotepadsCore(Sets, new NotepadsExtensionProvider());
                    _notepadsCore.StorageItemsDropped += OnStorageItemsDropped;
                    _notepadsCore.TextEditorLoaded += OnTextEditorLoaded;
                    _notepadsCore.TextEditorUnloaded += OnTextEditorUnloaded;
                    _notepadsCore.TextEditorKeyDown += OnTextEditor_KeyDown;
                    _notepadsCore.TextEditorClosing += OnTextEditorClosing;
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

        private readonly string _defaultNewFileName;

        public NotepadsMainPage()
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
            EditorSettingsService.OnSessionBackupAndRestoreOptionChanged += async (sender, isSessionBackupAndRestoreEnabled) =>
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
                    await SessionManager.ClearSessionDataAsync();
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
            NewSetButton.Click += delegate { NotepadsCore.OpenNewTextEditor(_defaultNewFileName); };
            MainMenuButton.Click += (sender, args) => FlyoutBase.ShowAttachedFlyout((FrameworkElement)sender);
            MenuCreateNewButton.Click += (sender, args) => NotepadsCore.OpenNewTextEditor(_defaultNewFileName);
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
                new KeyboardShortcut<KeyRoutedEventArgs>(true, false, false, VirtualKey.N, (args) => NotepadsCore.OpenNewTextEditor(_defaultNewFileName)),
                new KeyboardShortcut<KeyRoutedEventArgs>(true, false, false, VirtualKey.T, (args) => NotepadsCore.OpenNewTextEditor(_defaultNewFileName)),
                new KeyboardShortcut<KeyRoutedEventArgs>(true, false, false, VirtualKey.O, async (args) => await OpenNewFiles()),
                new KeyboardShortcut<KeyRoutedEventArgs>(true, false, false, VirtualKey.S, async (args) => await Save(NotepadsCore.GetSelectedTextEditor(), saveAs: false, ignoreUnmodifiedDocument: true)),
                new KeyboardShortcut<KeyRoutedEventArgs>(true, false, true, VirtualKey.S, async (args) => await Save(NotepadsCore.GetSelectedTextEditor(), saveAs: true)),
                new KeyboardShortcut<KeyRoutedEventArgs>(true, false, true, VirtualKey.R, (args) => { ReloadFileFromDisk(this, new RoutedEventArgs()); }),
                new KeyboardShortcut<KeyRoutedEventArgs>(VirtualKey.F11, (args) => { EnterExitFullScreenMode(); }),
                new KeyboardShortcut<KeyRoutedEventArgs>(VirtualKey.F12, (args) => { EnterExitCompactOverlayMode(); }),
                new KeyboardShortcut<KeyRoutedEventArgs>(VirtualKey.Tab, (args) => NotepadsCore.GetSelectedTextEditor()?.TypeText(
                    EditorSettingsService.EditorDefaultTabIndents == -1 ? "\t" : new string(' ', EditorSettingsService.EditorDefaultTabIndents)))
            });
        }

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
                    NotepadsCore.OpenNewTextEditor(_defaultNewFileName);
                }

                if (!App.IsFirstInstance)
                {
                    NotificationCenter.Instance.PostNotification(_resourceLoader.GetString("App_ShadowInstanceIndicator_Description"), 4000);
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
            var deferral = e.GetDeferral();

            if (EditorSettingsService.IsSessionSnapshotEnabled)
            {
                // Save session before app exit
                await SessionManager.SaveSessionAsync(() => { SessionManager.IsBackupEnabled = false; });
                deferral.Complete();
            }
            else
            {
                if (!NotepadsCore.HaveUnsavedTextEditor())
                {
                    deferral.Complete();
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

                        // Prevent app from closing if there is any tab still opens
                        if (NotepadsCore.GetNumberOfOpenedTextEditors() > 0)
                        {
                            e.Handled = true;
                        }

                        deferral.Complete();
                    },
                    () =>
                    {
                        deferral.Complete();
                    },
                    () =>
                    {
                        e.Handled = true;
                        deferral.Complete();
                    });

                await ContentDialogMaker.CreateContentDialogAsync(appCloseSaveReminderDialog, awaitPreviousDialog: false);
            }
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

        private void OnTextEditorLoaded(object sender, ITextEditor textEditor)
        {
            if (NotepadsCore.GetSelectedTextEditor() == textEditor)
            {
                SetupStatusBar(textEditor);
                NotepadsCore.FocusOnSelectedTextEditor();
            }
        }

        private void OnTextEditorUnloaded(object sender, ITextEditor textEditor)
        {
            if (NotepadsCore.GetNumberOfOpenedTextEditors() == 0)
            {
                NotepadsCore.OpenNewTextEditor(_defaultNewFileName);
            }
        }

        private async void OnTextEditorClosing(object sender, ITextEditor textEditor)
        {
            if (NotepadsCore.GetNumberOfOpenedTextEditors() == 1 && textEditor.IsModified == false && textEditor.EditingFile == null)
            {
                // Do nothing
                // Take no action if user is trying to close the last tab and the last tab is a new empty document
            }
            else if (!textEditor.IsModified)
            {
                NotepadsCore.DeleteTextEditor(textEditor);
            }
            else // Remind user to save uncommitted changes
            {
                var file = textEditor.EditingFilePath ?? textEditor.FileNamePlaceholder;

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
        }

        private void OnTextEditor_KeyDown(object sender, KeyRoutedEventArgs e)
        {
            if (!(sender is ITextEditor textEditor)) return;
            // ignoring key events coming from inactive text editors
            if (NotepadsCore.GetSelectedTextEditor() != textEditor) return;
            _keyboardCommandHandler.Handle(e);
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
    }
}