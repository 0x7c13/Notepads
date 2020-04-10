namespace Notepads
{
    using System;
    using System.Collections.Generic;
    using System.Linq;
    using System.Threading.Tasks;
    using Notepads.Commands;
    using Notepads.Controls.Print;
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
    using Windows.UI;
    using Windows.UI.ViewManagement;
    using Windows.UI.Xaml;
    using Windows.UI.Xaml.Controls;
    using Windows.UI.Xaml.Controls.Primitives;
    using Windows.UI.Xaml.Input;
    using Windows.UI.Xaml.Media;
    using Windows.UI.Xaml.Media.Animation;
    using Windows.UI.Xaml.Navigation;
    using Microsoft.AppCenter.Analytics;
    using Windows.Graphics.Printing;

    public sealed partial class NotepadsMainPage : Page, INotificationDelegate
    {
        private IReadOnlyList<IStorageItem> _appLaunchFiles;

        private string _appLaunchCmdDir;
        private string _appLaunchCmdArgs;
        private Uri _appLaunchUri;

        private readonly ResourceLoader _resourceLoader = ResourceLoader.GetForCurrentView();

        private bool _loaded = false;
        private bool _appShouldExitAfterLastEditorClosed = false;

        private const int TitleBarReservedAreaDefaultWidth = 180;
        private const int TitleBarReservedAreaCompactOverlayWidth = 100;

        private INotepadsCore _notepadsCore;

        private INotepadsCore NotepadsCore
        {
            get
            {
                if (_notepadsCore == null)
                {
                    _notepadsCore = new NotepadsCore(Sets, new NotepadsExtensionProvider(), Dispatcher);
                    _notepadsCore.StorageItemsDropped += OnStorageItemsDropped;
                    _notepadsCore.TextEditorLoaded += OnTextEditorLoaded;
                    _notepadsCore.TextEditorUnloaded += OnTextEditorUnloaded;
                    _notepadsCore.TextEditorKeyDown += OnTextEditor_KeyDown;
                    _notepadsCore.TextEditorClosing += OnTextEditorClosing;
                    _notepadsCore.TextEditorMovedToAnotherAppInstance += OnTextEditorMovedToAnotherAppInstance;
                    _notepadsCore.TextEditorSelectionChanged += (sender, editor) => { if (NotepadsCore.GetSelectedTextEditor() == editor) UpdateLineColumnIndicator(editor); };
                    _notepadsCore.TextEditorFontZoomFactorChanged += (sender, editor) => { if (NotepadsCore.GetSelectedTextEditor() == editor) UpdateFontZoomIndicator(editor); };
                    _notepadsCore.TextEditorEncodingChanged += (sender, editor) => { if (NotepadsCore.GetSelectedTextEditor() == editor) UpdateEncodingIndicator(editor.GetEncoding()); };
                    _notepadsCore.TextEditorLineEndingChanged += (sender, editor) => { if (NotepadsCore.GetSelectedTextEditor() == editor) { UpdateLineEndingIndicator(editor.GetLineEnding()); UpdateLineColumnIndicator(editor); } };
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

        private readonly ICommandHandler<KeyRoutedEventArgs> _keyboardCommandHandler;

        private const string XBoxGameBarSessionFilePrefix = "XBoxGameBar-";

        private ISessionManager _sessionManager;

        private ISessionManager SessionManager => _sessionManager ?? (_sessionManager = SessionUtility.GetSessionManager(NotepadsCore, App.IsGameBarWidget ? XBoxGameBarSessionFilePrefix : null));

        private readonly string _defaultNewFileName;

        public NotepadsMainPage()
        {
            InitializeComponent();

            _defaultNewFileName = _resourceLoader.GetString("TextEditor_DefaultNewFileName");

            NotificationCenter.Instance.SetNotificationDelegate(this);

            // Setup theme
            ThemeSettingsService.SetRequestedTheme(RootGrid, Window.Current.Content, ApplicationView.GetForCurrentView().TitleBar, Application.Current.RequestedTheme);
            ThemeSettingsService.OnBackgroundChanged += ThemeSettingsService_OnBackgroundChanged;
            ThemeSettingsService.OnThemeChanged += ThemeSettingsService_OnThemeChanged;
            ThemeSettingsService.OnAccentColorChanged += ThemeSettingsService_OnAccentColorChanged;

            // Setup custom Title Bar
            Window.Current.SetTitleBar(AppTitleBar);

            // Setup status bar
            ShowHideStatusBar(EditorSettingsService.ShowStatusBar);
            EditorSettingsService.OnStatusBarVisibilityChanged += async (sender, visibility) =>
            {
                await ThreadUtility.CallOnUIThreadAsync(Dispatcher, () =>
                {
                    if (ApplicationView.GetForCurrentView().ViewMode != ApplicationViewMode.CompactOverlay) ShowHideStatusBar(visibility);
                });
            };

            // Session backup and restore toggle
            EditorSettingsService.OnSessionBackupAndRestoreOptionChanged += async (sender, isSessionBackupAndRestoreEnabled) =>
            {
                await ThreadUtility.CallOnUIThreadAsync(Dispatcher, async () =>
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
                });
            };

            // Sharing
            Windows.ApplicationModel.DataTransfer.DataTransferManager.GetForCurrentView().DataRequested += MainPage_DataRequested;
            Windows.UI.Core.Preview.SystemNavigationManagerPreview.GetForCurrentView().CloseRequested += MainPage_CloseRequested;

            if (App.IsGameBarWidget)
            {
                TitleBarReservedArea.Width = .0f;
            }
            else
            {
                Window.Current.SizeChanged += WindowSizeChanged;
                Window.Current.VisibilityChanged += WindowVisibilityChangedEventHandler;
            }

            InitControls();

            // Init shortcuts
            _keyboardCommandHandler = GetKeyboardCommandHandler();

            //Register for printing
            if (!PrintManager.IsSupported())
            {
                MenuPrintButton.Visibility = Visibility.Collapsed;
                MenuPrintAllButton.Visibility = Visibility.Collapsed;
                MenuPrintSeparator.Visibility = Visibility.Collapsed;
            }
            else
            {
                PrintArgs.RegisterForPrinting(this);
            }
        }

        private async void ThemeSettingsService_OnAccentColorChanged(object sender, Color color)
        {
            await ThreadUtility.CallOnUIThreadAsync(Dispatcher, ThemeSettingsService.SetRequestedAccentColor);
        }

        private async void ThemeSettingsService_OnThemeChanged(object sender, EventArgs args)
        {
            await ThreadUtility.CallOnUIThreadAsync(Dispatcher, () =>
            {
                ThemeSettingsService.SetRequestedTheme(RootGrid, Window.Current.Content, ApplicationView.GetForCurrentView().TitleBar, Application.Current.RequestedTheme);
            });
        }

        private async void ThemeSettingsService_OnBackgroundChanged(object sender, Brush backgroundBrush)
        {
            await ThreadUtility.CallOnUIThreadAsync(Dispatcher, () =>
            {
                RootGrid.Background = backgroundBrush;
            });
        }

        private void InitControls()
        {
            ToolTipService.SetToolTip(ExitCompactOverlayButton, _resourceLoader.GetString("App_ExitCompactOverlayMode_Text"));
            RootSplitView.PaneOpening += delegate { SettingsFrame.Navigate(typeof(SettingsPage), null, new SuppressNavigationTransitionInfo()); };
            RootSplitView.PaneClosed += delegate { NotepadsCore.FocusOnSelectedTextEditor(); };
            NewSetButton.Click += delegate { NotepadsCore.OpenNewTextEditor(_defaultNewFileName); };
            MainMenuButton.Click += (sender, args) => FlyoutBase.ShowAttachedFlyout((FrameworkElement)sender);
            MenuCreateNewButton.Click += (sender, args) => NotepadsCore.OpenNewTextEditor(_defaultNewFileName);
            MenuCreateNewWindowButton.Click += async (sender, args) => await OpenNewAppInstance();
            MenuOpenFileButton.Click += async (sender, args) => await OpenNewFiles();
            MenuSaveButton.Click += async (sender, args) => await Save(NotepadsCore.GetSelectedTextEditor(), saveAs: false);
            MenuSaveAsButton.Click += async (sender, args) => await Save(NotepadsCore.GetSelectedTextEditor(), saveAs: true);
            MenuSaveAllButton.Click += async (sender, args) =>
            {
                var success = false;
                foreach (var textEditor in NotepadsCore.GetAllTextEditors())
                {
                    if (await Save(textEditor, saveAs: false, ignoreUnmodifiedDocument: true, rebuildOpenRecentItems: false)) success = true;
                }
                if (success)
                {
                    await BuildOpenRecentButtonSubItems();
                }
            };
            MenuFindButton.Click += (sender, args) => NotepadsCore.GetSelectedTextEditor()?.ShowFindAndReplaceControl(showReplaceBar: false);
            MenuReplaceButton.Click += (sender, args) => NotepadsCore.GetSelectedTextEditor()?.ShowFindAndReplaceControl(showReplaceBar: true);
            MenuFullScreenButton.Click += (sender, args) => EnterExitFullScreenMode();
            MenuCompactOverlayButton.Click += (sender, args) => EnterExitCompactOverlayMode();
            MenuPrintButton.Click += async (sender, args) => await Print(NotepadsCore.GetSelectedTextEditor());
            MenuPrintAllButton.Click += async (sender, args) => await PrintAll(NotepadsCore.GetAllTextEditors());
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
                    MenuPrintButton.IsEnabled = false;
                    MenuPrintAllButton.IsEnabled = false;
                }
                else if (selectedTextEditor.IsEditorEnabled() == false)
                {
                    MenuSaveButton.IsEnabled = selectedTextEditor.IsModified;
                    MenuSaveAsButton.IsEnabled = true;
                    MenuFindButton.IsEnabled = false;
                    MenuReplaceButton.IsEnabled = false;
                }
                else
                {
                    MenuSaveButton.IsEnabled = selectedTextEditor.IsModified;
                    MenuSaveAsButton.IsEnabled = true;
                    MenuFindButton.IsEnabled = true;
                    MenuReplaceButton.IsEnabled = true;

                    if (PrintManager.IsSupported())
                    {
                        MenuPrintButton.IsEnabled = !string.IsNullOrEmpty(selectedTextEditor.GetText());
                        MenuPrintAllButton.IsEnabled = NotepadsCore.HaveNonemptyTextEditor();
                    }
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

            if (App.IsGameBarWidget)
            {
                MenuFullScreenSeparator.Visibility = Visibility.Collapsed;
                MenuPrintSeparator.Visibility = Visibility.Collapsed;
                MenuSettingsSeparator.Visibility = Visibility.Collapsed;

                MenuCompactOverlayButton.Visibility = Visibility.Collapsed;
                MenuFullScreenButton.Visibility = Visibility.Collapsed;
                MenuPrintButton.Visibility = Visibility.Collapsed;
                MenuPrintAllButton.Visibility = Visibility.Collapsed;
                MenuSettingsButton.Visibility = Visibility.Collapsed;
            }
        }

        private async Task BuildOpenRecentButtonSubItems()
        {
            var openRecentSubItem = new MenuFlyoutSubItem
            {
                Text = _resourceLoader.GetString("MainMenu_Button_Open_Recent/Text"),
                Icon = new FontIcon { Glyph = "\xE81C" },
                Name = "MenuOpenRecentlyUsedFileButton",
            };

            var MRUFileList = new HashSet<string>();

            foreach (var item in await MRUService.Get(top: 10))
            {
                if (item is StorageFile file)
                {
                    if (MRUFileList.Contains(file.Path))
                    {
                        // MRU might contains files with same path (User opens a recently used file after renaming it)
                        // So we need to do the decouple here
                        continue;
                    }
                    var newItem = new MenuFlyoutItem()
                    {
                        Text = file.Path
                    };
                    ToolTipService.SetToolTip(newItem, file.Path);
                    newItem.Click += async (sender, args) => { await OpenFile(file); };
                    openRecentSubItem.Items?.Add(newItem);
                    MRUFileList.Add(file.Path);
                }
            }

            var oldOpenRecentSubItem = MainMenuButtonFlyout.Items?.FirstOrDefault(i => i.Name == openRecentSubItem.Name);
            if (oldOpenRecentSubItem != null)
            {
                MainMenuButtonFlyout.Items.Remove(oldOpenRecentSubItem);
            }

            openRecentSubItem.IsEnabled = false;
            if (openRecentSubItem.Items?.Count > 0)
            {
                openRecentSubItem.Items?.Add(new MenuFlyoutSeparator());

                var clearRecentlyOpenedSubItem =
                    new MenuFlyoutItem()
                    {
                        Text = _resourceLoader.GetString("MainMenu_Button_Open_Recent_ClearRecentlyOpenedSubItem_Text")
                    };
                clearRecentlyOpenedSubItem.Click += async (sender, args) =>
                {
                    MRUService.ClearAll();
                    await BuildOpenRecentButtonSubItems();
                };
                openRecentSubItem.Items?.Add(clearRecentlyOpenedSubItem);
                openRecentSubItem.IsEnabled = true;
            }

            if (MainMenuButtonFlyout.Items != null)
            {
                var indexToInsert = MainMenuButtonFlyout.Items.IndexOf(MenuOpenFileButton) + 1;
                MainMenuButtonFlyout.Items.Insert(indexToInsert, openRecentSubItem);
            }
        }

        private KeyboardCommandHandler GetKeyboardCommandHandler()
        {
            return new KeyboardCommandHandler(new List<IKeyboardCommand<KeyRoutedEventArgs>>()
            {
                new KeyboardCommand<KeyRoutedEventArgs>(true, false, false, VirtualKey.W, (args) => NotepadsCore.CloseTextEditor(NotepadsCore.GetSelectedTextEditor())),
                new KeyboardCommand<KeyRoutedEventArgs>(true, false, false, VirtualKey.Tab, (args) => NotepadsCore.SwitchTo(true)),
                new KeyboardCommand<KeyRoutedEventArgs>(true, false, true, VirtualKey.Tab, (args) => NotepadsCore.SwitchTo(false)),
                new KeyboardCommand<KeyRoutedEventArgs>(true, false, false, VirtualKey.N, (args) => NotepadsCore.OpenNewTextEditor(_defaultNewFileName)),
                new KeyboardCommand<KeyRoutedEventArgs>(true, false, false, VirtualKey.T, (args) => NotepadsCore.OpenNewTextEditor(_defaultNewFileName)),
                new KeyboardCommand<KeyRoutedEventArgs>(true, false, false, VirtualKey.O, async (args) => await OpenNewFiles()),
                new KeyboardCommand<KeyRoutedEventArgs>(true, false, false, VirtualKey.S, async (args) => await Save(NotepadsCore.GetSelectedTextEditor(), saveAs: false, ignoreUnmodifiedDocument: true)),
                new KeyboardCommand<KeyRoutedEventArgs>(true, false, true, VirtualKey.S, async (args) => await Save(NotepadsCore.GetSelectedTextEditor(), saveAs: true)),
                new KeyboardCommand<KeyRoutedEventArgs>(true, false, false, VirtualKey.P, async (args) => await Print(NotepadsCore.GetSelectedTextEditor())),
                new KeyboardCommand<KeyRoutedEventArgs>(true, false, true, VirtualKey.P, async (args) => await PrintAll(NotepadsCore.GetAllTextEditors())),
                new KeyboardCommand<KeyRoutedEventArgs>(true, false, true, VirtualKey.R, (args) => { ReloadFileFromDisk(this, new RoutedEventArgs()); }),
                new KeyboardCommand<KeyRoutedEventArgs>(true, false, true, VirtualKey.N, async (args) => await OpenNewAppInstance()),
                new KeyboardCommand<KeyRoutedEventArgs>(true, false, false, VirtualKey.Number1, (args) => NotepadsCore.SwitchTo(0)),
                new KeyboardCommand<KeyRoutedEventArgs>(true, false, false, VirtualKey.Number2, (args) => NotepadsCore.SwitchTo(1)),
                new KeyboardCommand<KeyRoutedEventArgs>(true, false, false, VirtualKey.Number3, (args) => NotepadsCore.SwitchTo(2)),
                new KeyboardCommand<KeyRoutedEventArgs>(true, false, false, VirtualKey.Number4, (args) => NotepadsCore.SwitchTo(3)),
                new KeyboardCommand<KeyRoutedEventArgs>(true, false, false, VirtualKey.Number5, (args) => NotepadsCore.SwitchTo(4)),
                new KeyboardCommand<KeyRoutedEventArgs>(true, false, false, VirtualKey.Number6, (args) => NotepadsCore.SwitchTo(5)),
                new KeyboardCommand<KeyRoutedEventArgs>(true, false, false, VirtualKey.Number7, (args) => NotepadsCore.SwitchTo(6)),
                new KeyboardCommand<KeyRoutedEventArgs>(true, false, false, VirtualKey.Number8, (args) => NotepadsCore.SwitchTo(7)),
                new KeyboardCommand<KeyRoutedEventArgs>(true, false, false, VirtualKey.Number9, (args) => NotepadsCore.SwitchTo(8)),
                new KeyboardCommand<KeyRoutedEventArgs>(VirtualKey.F11, (args) => { EnterExitFullScreenMode(); }),
                new KeyboardCommand<KeyRoutedEventArgs>(VirtualKey.F12, (args) => { EnterExitCompactOverlayMode(); }),
                new KeyboardCommand<KeyRoutedEventArgs>(VirtualKey.Escape, (args) => { if (RootSplitView.IsPaneOpen) RootSplitView.IsPaneOpen = false; }),
            });
        }

        private async Task OpenNewAppInstance()
        {
            if (!await NotepadsProtocolService.LaunchProtocolAsync(NotepadsOperationProtocol.OpenNewInstance))
            {
                Analytics.TrackEvent("FailedToOpenNewAppInstance");
            }
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
                try
                {
                    loadedCount = await SessionManager.LoadLastSessionAsync();
                }
                catch (Exception ex)
                {
                    LoggingService.LogError($"[SessionManager] Failed to LoadLastSessionAsync: {ex}");
                    Analytics.TrackEvent("FailedToLoadLastSession", new Dictionary<string, string> { { "Exception", ex.ToString() } });
                }
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
                    NotificationCenter.Instance.PostNotification(_resourceLoader.GetString("App_ShadowWindowIndicator_Description"), 4000);
                }
                _loaded = true;
            }

            if (EditorSettingsService.IsSessionSnapshotEnabled)
            {
                SessionManager.IsBackupEnabled = true;
                SessionManager.StartSessionBackup();
            }

            await BuildOpenRecentButtonSubItems();

            if (!App.IsGameBarWidget)
            {
                // An issue with the Game Bar extension model and Windows platform prevents the Notepads process from exiting cleanly
                // when more than one CoreWindow has been created, and NotepadsMainPage is the last to close. The common case for this
                // is to open Notepads in Game Bar, then open its settings, then close the settings and finally close Notepads.
                // This puts the process in a bad state where it will no longer open in Game Bar and the Notepads process is orphaned. 
                // To work around this do not use the EnteredBackground event when running as a widget.
                // Microsoft is tracking this issue as VSO#25735260
                Application.Current.EnteredBackground -= App_EnteredBackground;
                Application.Current.EnteredBackground += App_EnteredBackground;

                Window.Current.CoreWindow.Activated -= CoreWindow_Activated;
                Window.Current.CoreWindow.Activated += CoreWindow_Activated;
            }
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
                Task.Run(() => ApplicationSettingsStore.Write(SettingsKey.ActiveInstanceIdStr, App.Id.ToString()));
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
                return;
            }

            if (!NotepadsCore.HaveUnsavedTextEditor())
            {
                deferral.Complete();
                return;
            }

            var appCloseSaveReminderDialog = NotepadsDialogFactory.GetAppCloseSaveReminderDialog(
                async () =>
                {
                    var count = NotepadsCore.GetNumberOfOpenedTextEditors();

                    foreach (var textEditor in NotepadsCore.GetAllTextEditors())
                    {
                        if (await Save(textEditor, saveAs: false, ignoreUnmodifiedDocument: true, rebuildOpenRecentItems: false))
                        {
                            if (count == 1)
                            {
                                _appShouldExitAfterLastEditorClosed = true;
                            }
                            NotepadsCore.DeleteTextEditor(textEditor);
                            count--;
                        }
                    }

                    // Prevent app from closing if there is any tab still opens
                    if (count > 0)
                    {
                        e.Handled = true;
                        await BuildOpenRecentButtonSubItems();
                    }

                    deferral.Complete();
                },
                discardAndExitAction: () =>
                {
                    deferral.Complete();
                },
                cancelAction: () =>
                {
                    e.Handled = true;
                    deferral.Complete();
                });

            await DialogManager.OpenDialogAsync(appCloseSaveReminderDialog, awaitPreviousDialog: false);

            if (e.Handled && !appCloseSaveReminderDialog.IsAborted)
            {
                NotepadsCore.FocusOnSelectedTextEditor();
            }
        }

        private void UpdateApplicationTitle(ITextEditor activeTextEditor)
        {
            if (!App.IsGameBarWidget)
            {
                ApplicationView.GetForCurrentView().Title = activeTextEditor.EditingFileName ?? activeTextEditor.FileNamePlaceholder;
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
                UpdateApplicationTitle(textEditor);
                NotepadsCore.FocusOnSelectedTextEditor();
            }
        }

        private void OnTextEditorUnloaded(object sender, ITextEditor textEditor)
        {
            if (NotepadsCore.GetNumberOfOpenedTextEditors() == 0 && !_appShouldExitAfterLastEditorClosed)
            {
                NotepadsCore.OpenNewTextEditor(_defaultNewFileName);
            }
        }

        private async void OnTextEditorMovedToAnotherAppInstance(object sender, ITextEditor textEditor)
        {
            // Notepads should exit if last tab was dragged to another app instance
            if (NotepadsCore.GetNumberOfOpenedTextEditors() == 1)
            {
                _appShouldExitAfterLastEditorClosed = true;

                NotepadsCore.DeleteTextEditor(textEditor);

                if (EditorSettingsService.IsSessionSnapshotEnabled)
                {
                    await SessionManager.SaveSessionAsync(() => { SessionManager.IsBackupEnabled = false; });
                }

                Application.Current.Exit();
            }
            else
            {
                NotepadsCore.DeleteTextEditor(textEditor);
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

                var setCloseSaveReminderDialog = NotepadsDialogFactory.GetSetCloseSaveReminderDialog(file, async () =>
                {
                    if (NotepadsCore.GetAllTextEditors().Contains(textEditor) && await Save(textEditor, saveAs: false))
                    {
                        NotepadsCore.DeleteTextEditor(textEditor);
                    }
                }, () =>
                {
                    if (NotepadsCore.GetAllTextEditors().Contains(textEditor))
                    {
                        NotepadsCore.DeleteTextEditor(textEditor);
                    }
                });

                setCloseSaveReminderDialog.Opened += (s, a) =>
                {
                    if (NotepadsCore.GetAllTextEditors().Contains(textEditor))
                    {
                        NotepadsCore.SwitchTo(textEditor);
                    }
                };

                await DialogManager.OpenDialogAsync(setCloseSaveReminderDialog, awaitPreviousDialog: true);

                if (!setCloseSaveReminderDialog.IsAborted)
                {
                    NotepadsCore.FocusOnSelectedTextEditor();
                }
            }
        }

        private void OnTextEditor_KeyDown(object sender, KeyRoutedEventArgs e)
        {
            if (!(sender is ITextEditor textEditor)) return;
            // ignoring key events coming from inactive text editors
            if (NotepadsCore.GetSelectedTextEditor() != textEditor) return;
            var result = _keyboardCommandHandler.Handle(e);
            if (result.ShouldHandle)
            {
                e.Handled = true;
            }
        }

        private async void OnStorageItemsDropped(object sender, IReadOnlyList<IStorageItem> storageItems)
        {
            foreach (var storageItem in storageItems)
            {
                if (storageItem is StorageFile file)
                {
                    await OpenFile(file);
                    Analytics.TrackEvent("OnStorageFileDropped");
                }
            }
        }

        #endregion
    }
}