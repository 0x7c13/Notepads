// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2019-2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Collections.Generic;
using System.Diagnostics.CodeAnalysis;
using System.Linq;
using System.Threading.Tasks;
using Notepads.Features.Preferences;
using Notepads.Features.Sessions.Contracts;
using Notepads.Features.Sessions.Validation;
using Notepads.Infrastructure.Diagnostics;
using Notepads.Infrastructure.Settings;
using Notepads.Presentation.Controls.Dialog;
using Notepads.Presentation.Controls.Print;
using Notepads.Presentation.Controls.TextEditor;
using Notepads.Presentation.Helpers;
using Notepads.Presentation.Input;
using Notepads.Presentation.PreviewExtensions;
using Notepads.Presentation.Theming;
using Notepads.Presentation.Views.Settings;
using Notepads.Presentation.Workspace;
using Notepads.Presentation.Workspace.Activation;
using Windows.ApplicationModel.Activation;
using Windows.ApplicationModel.DataTransfer;
using Windows.ApplicationModel.Resources;
using Windows.Graphics.Printing;
using Windows.Storage;
using Windows.System;
using Windows.UI.ViewManagement;
using Windows.UI.Xaml;
using Windows.UI.Xaml.Controls;
using Windows.UI.Xaml.Input;
using Windows.UI.Xaml.Media.Animation;
using Windows.UI.Xaml.Navigation;

namespace Notepads.Presentation.Views.MainPage;

[SuppressMessage("Design", "CA1001:Types that own disposable fields should be disposable",
    Justification = "Window.Closed owns asynchronous shutdown: drain sessions, dispose the core, then release writer ownership; synchronous IDisposable cannot express that protocol.")]
public sealed partial class NotepadsMainPage : Page
{
    private IReadOnlyList<IStorageItem> _appLaunchFiles;

    private string _appLaunchCmdDir;
    private string _appLaunchCmdArgs;

    private readonly ResourceLoader _resourceLoader = ResourceLoader.GetForCurrentView();

    private bool _loaded = false;
    private bool _lastTabMovedToAnotherInstance = false;
    private bool _isAppClosing;
    private bool _viewClosed;
    private bool _viewEventsBound;
    private WindowContext _context;
    private SettingsContext _settingsContext;
    private Window _window;
    private bool _isLastTabCloseInProgress;
    private bool _recoveryFolderDialogOpen;
    private int _closeDecisionCount;
    private bool _enabledBeforeCloseDecisions;
    private readonly HashSet<Guid> _closingTextEditors = new();

    private INotepadsCore _notepadsCore;

    private INotepadsCore NotepadsCore
    {
        get
        {
            if (_notepadsCore != null) return _notepadsCore;

            _notepadsCore = new NotepadsCore(Sets, new NotepadsExtensionProvider(), Dispatcher, _context, RenameFileAsync);
            _notepadsCore.StorageItemsDropped += OnStorageItemsDropped;
            _notepadsCore.TextEditorLoaded += OnTextEditorLoaded;
            _notepadsCore.TextEditorClosed += OnTextEditorClosed;
            _notepadsCore.TextEditorKeyDown += OnTextEditorKeyDown;
            _notepadsCore.TextEditorClosing += OnTextEditorClosing;
            _notepadsCore.TextEditorSaved += OnTextEditorSaved;
            _notepadsCore.TextEditorMovedToAnotherAppInstance += OnTextEditorMovedToAnotherAppInstance;
            _notepadsCore.TextEditorRenamed += (sender, editor) => { if (NotepadsCore.GetSelectedTextEditor() == editor) SetupStatusBar(editor); };
            _notepadsCore.TextEditorSelectionChanged += (sender, editor) => { if (NotepadsCore.GetSelectedTextEditor() == editor) UpdateLineColumnIndicator(editor); };
            _notepadsCore.TextEditorFontZoomFactorChanged += (sender, editor) => { if (NotepadsCore.GetSelectedTextEditor() == editor) UpdateFontZoomIndicator(editor); };
            _notepadsCore.TextEditorLanguageChanged += (sender, editor) => { if (ReferenceEquals(NotepadsCore.GetSelectedTextEditor(), editor)) UpdateLanguageIndicator(editor); };
            _notepadsCore.TextEditorEncodingChanged += (sender, editor) => { if (NotepadsCore.GetSelectedTextEditor() == editor) UpdateEncodingIndicator(editor.GetEncoding()); };
            _notepadsCore.TextEditorLineEndingChanged += (sender, editor) => { if (NotepadsCore.GetSelectedTextEditor() == editor) { UpdateLineEndingIndicator(editor.GetLineEnding()); UpdateLineColumnIndicator(editor); } };
            _notepadsCore.TextEditorEditorModificationStateChanged += (sender, editor) => { if (NotepadsCore.GetSelectedTextEditor() == editor) SetupStatusBar(editor); };
            _notepadsCore.TextEditorFileModificationStateChanged += (sender, editor) => { if (NotepadsCore.GetSelectedTextEditor() == editor) OnTextEditorFileModificationStateChanged(editor); };

            return _notepadsCore;
        }
    }

    private KeyboardCommandHandler _keyboardCommandHandler;

    private ISessionController _sessionManager;

    private ISessionController SessionController => _viewClosed ? throw new ObjectDisposedException(nameof(NotepadsMainPage)) :
        _sessionManager ?? (_sessionManager = new SessionController(NotepadsCore, _context.Sessions, _context.Dispatcher, _context.RegisterSession));

    private string _defaultNewFileName;

    public NotepadsMainPage()
    {
        InitializeComponent();
    }

    internal void Initialize(WindowContext context)
    {
        if (_context != null)
        {
            if (!ReferenceEquals(_context, context)) throw new InvalidOperationException("A workspace cannot change its window context.");
            return;
        }
        _context = context ?? throw new ArgumentNullException(nameof(context));
        _settingsContext = new SettingsContext(context);
        _window = Window.Current;
        _window.Closed += OnWindowClosed;

        _defaultNewFileName = _resourceLoader.GetString("TextEditor_DefaultNewFileName");

        // Set custom title bar dragging area
        Window.Current.SetTitleBar(AppTitleBar);

        InitializeNotificationCenter();
        InitializeThemeSettings();
        InitializeStatusBar();
        InitializeControls();
        InitializeMainMenu();
        InitializeKeyboardShortcuts();
        Loaded += MainPage_Loaded;
        Unloaded += MainPage_Unloaded;

        // Session backup and restore toggle
        ApplicationPreferences.OnSessionBackupAndRestoreOptionChanged += OnSessionBackupAndRestoreOptionChanged;

        Windows.UI.Core.Preview.SystemNavigationManagerPreview.GetForCurrentView().CloseRequested += MainPage_CloseRequested;
    }

    private void MainPage_Loaded(object sender, RoutedEventArgs e) => BindViewEvents();

    private void MainPage_Unloaded(object sender, RoutedEventArgs e) => UnbindViewEvents();

    private void BindViewEvents()
    {
        if (_context == null || _viewClosed || _viewEventsBound) return;
        _viewEventsBound = true;
        InitializeThemeSettings();
        InitializeStatusBar();
        ThemeSettingsService.OnBackgroundChanged += ThemeSettingsService_OnBackgroundChanged;
        ThemeSettingsService.OnThemeChanged += ThemeSettingsService_OnThemeChanged;
        ThemeSettingsService.OnAccentColorChanged += ThemeSettingsService_OnAccentColorChanged;
        ApplicationPreferences.OnStatusBarVisibilityChanged += OnStatusBarVisibilityChanged;
        DataTransferManager.GetForCurrentView().DataRequested += MainPage_DataRequested;
        _window.SizeChanged += WindowSizeChanged;
        _window.VisibilityChanged += WindowVisibilityChangedEventHandler;
        PreviewKeyDown += OnMainPagePreviewKeyDown;
        if (PrintManager.IsSupported()) PrintArgs.RegisterForPrinting(this);
    }

    private void UnbindViewEvents()
    {
        if (!_viewEventsBound) return;
        _viewEventsBound = false;
        ThemeSettingsService.OnBackgroundChanged -= ThemeSettingsService_OnBackgroundChanged;
        ThemeSettingsService.OnThemeChanged -= ThemeSettingsService_OnThemeChanged;
        ThemeSettingsService.OnAccentColorChanged -= ThemeSettingsService_OnAccentColorChanged;
        ApplicationPreferences.OnStatusBarVisibilityChanged -= OnStatusBarVisibilityChanged;
        DataTransferManager.GetForCurrentView().DataRequested -= MainPage_DataRequested;
        _window.SizeChanged -= WindowSizeChanged;
        _window.VisibilityChanged -= WindowVisibilityChangedEventHandler;
        PreviewKeyDown -= OnMainPagePreviewKeyDown;
        PrintArgs.UnregisterForPrinting(this);
    }

    private void InitializeControls()
    {
        ToolTipService.SetToolTip(ExitCompactOverlayButton, _resourceLoader.GetString("App_ExitCompactOverlayMode_Text"));
        RootSplitView.PaneOpening += delegate { SettingsFrame.Navigate(typeof(SettingsPage), _settingsContext, new SuppressNavigationTransitionInfo()); };
        RootSplitView.PaneClosed += delegate { NotepadsCore.FocusOnSelectedTextEditor(); };
        NewSetButton.Click += async (sender, args) => await CreateNewTextEditorAsync();
    }

    private void InitializeKeyboardShortcuts()
    {
        _keyboardCommandHandler = new KeyboardCommandHandler(
        [
            new(VirtualKeyModifiers.Control, VirtualKey.W, () => NotepadsCore.CloseTextEditor(NotepadsCore.GetSelectedTextEditor())),
            new(VirtualKeyModifiers.Control, VirtualKey.Tab, () => NotepadsCore.SwitchTo(true)),
            new(VirtualKeyModifiers.Control | VirtualKeyModifiers.Shift, VirtualKey.Tab, () => NotepadsCore.SwitchTo(false)),
            new(VirtualKeyModifiers.Control, VirtualKey.N, async () => await CreateNewTextEditorAsync()),
            new(VirtualKeyModifiers.Control, VirtualKey.T, async () => await CreateNewTextEditorAsync()),
            new(VirtualKeyModifiers.Control, VirtualKey.O, async () => await OpenNewFilesAsync()),
            new(VirtualKeyModifiers.Control, VirtualKey.S, async () => await SaveAsync(NotepadsCore.GetSelectedTextEditor(), saveAs: false, ignoreUnmodifiedDocument: true)),
            new(VirtualKeyModifiers.Control | VirtualKeyModifiers.Shift, VirtualKey.S, async () => await SaveAsync(NotepadsCore.GetSelectedTextEditor(), saveAs: true)),
            new(VirtualKeyModifiers.Control, VirtualKey.P, async () => await PrintAsync(NotepadsCore.GetSelectedTextEditor())),
            new(VirtualKeyModifiers.Control | VirtualKeyModifiers.Shift, VirtualKey.P, async () => await PrintAllAsync(NotepadsCore.GetAllTextEditors())),
            new(VirtualKeyModifiers.Control | VirtualKeyModifiers.Shift, VirtualKey.R, () => ReloadFileFromDiskAsync(this, new RoutedEventArgs())),
            new(VirtualKeyModifiers.Control | VirtualKeyModifiers.Shift, VirtualKey.N, async () => await OpenNewAppInstanceAsync()),
            new(VirtualKeyModifiers.Control, VirtualKey.Number1, () => NotepadsCore.SwitchTo(0)),
            new(VirtualKeyModifiers.Control, VirtualKey.Number2, () => NotepadsCore.SwitchTo(1)),
            new(VirtualKeyModifiers.Control, VirtualKey.Number3, () => NotepadsCore.SwitchTo(2)),
            new(VirtualKeyModifiers.Control, VirtualKey.Number4, () => NotepadsCore.SwitchTo(3)),
            new(VirtualKeyModifiers.Control, VirtualKey.Number5, () => NotepadsCore.SwitchTo(4)),
            new(VirtualKeyModifiers.Control, VirtualKey.Number6, () => NotepadsCore.SwitchTo(5)),
            new(VirtualKeyModifiers.Control, VirtualKey.Number7, () => NotepadsCore.SwitchTo(6)),
            new(VirtualKeyModifiers.Control, VirtualKey.Number8, () => NotepadsCore.SwitchTo(7)),
            new(VirtualKeyModifiers.Control, VirtualKey.Number9, () => NotepadsCore.SwitchTo(8)),
            new(VirtualKeyModifiers.None, VirtualKey.F11, () => EnterExitFullScreenMode()),
            new(VirtualKeyModifiers.None, VirtualKey.F12, () => EnterExitCompactOverlayMode()),
            new(VirtualKeyModifiers.None, VirtualKey.Escape, () => { if (RootSplitView.IsPaneOpen) RootSplitView.IsPaneOpen = false; }),
            new(VirtualKeyModifiers.None, VirtualKey.F1, () => { if (_context.IsPrimaryInstance) RootSplitView.IsPaneOpen = !RootSplitView.IsPaneOpen; }),
            new(VirtualKeyModifiers.None, VirtualKey.F2, async () => await RenameFileAsync(NotepadsCore.GetSelectedTextEditor())),
            new(VirtualKeyModifiers.Control | VirtualKeyModifiers.Menu | VirtualKeyModifiers.Shift, VirtualKey.L, async () => { await OpenFileAsync(LoggingService.GetLogFile(), rebuildOpenRecentItems: false); }),
        ]);
    }

    private static async Task OpenNewAppInstanceAsync()
    {
        if (!await NotepadsProtocolService.LaunchProtocolAsync(NotepadsOperationProtocol.OpenNewInstance))
        {
            AnalyticsService.TrackEvent("FailedToOpenNewAppInstance");
        }
    }

    #region Application Life Cycle & Window management

    private async void OnWindowClosed(object sender, Windows.UI.Core.CoreWindowEventArgs args)
    {
        if (_viewClosed) return;
        _viewClosed = true;
        _isAppClosing = true;
        ApplicationPreferences.OnSessionBackupAndRestoreOptionChanged -= OnSessionBackupAndRestoreOptionChanged;
        UnbindViewEvents();
        Loaded -= MainPage_Loaded;
        Unloaded -= MainPage_Unloaded;
        Application.Current.EnteredBackground -= App_EnteredBackground;
        Windows.UI.Core.Preview.SystemNavigationManagerPreview.GetForCurrentView().CloseRequested -= MainPage_CloseRequested;
        _window.CoreWindow.Activated -= CoreWindow_Activated;
        _window.Closed -= OnWindowClosed;
        var sessions = _sessionManager;
        sessions?.Dispose();
        try
        {
            if (sessions != null) await sessions.DrainAsync();
            if (_notepadsCore != null) await _notepadsCore.DisposeAsync();
            // Keep OS writer ownership until native journals, drops and
            // captured transfers have all completed their shutdown.
            await _context.Sessions.DisposeAsync();
        }
        catch (Exception ex)
        {
            LoggingService.LogError($"[{nameof(NotepadsMainPage)}] Failed to drain closed workspace: {ex}");
        }
        finally
        {
            _sessionManager = null;
            _notepadsCore?.Dispose();
        }
    }

    // Handles external links or cmd args activation before Sets loaded
    protected override void OnNavigatedTo(NavigationEventArgs e)
    {
        base.OnNavigatedTo(e);

        var navigation = e.Parameter as WorkspaceNavigation ?? throw new ArgumentException("Workspace navigation requires a window context.");
        Initialize(navigation.Context);
        switch (navigation.Activation)
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
        }
    }

    // App should wait for Sets fully loaded before opening files requested by user (by click or from cmd)
    // Open files from external links or cmd args on Sets Loaded
    private async void Sets_Loaded(object sender, RoutedEventArgs e)
    {
        if (_context == null || _viewClosed || _isAppClosing) return;
        int loadedCount = 0;

        try { await SessionController.InitializeAuthorityAsync(); }
        catch (Exception ex)
        {
            LoggingService.LogError($"[{nameof(NotepadsMainPage)}] Recovery authority is unavailable: {ex}");
            NotificationCenter.Instance.PostNotification(
                _resourceLoader.GetString("SessionBackup_NotificationMsg_RecoveryBlocked"), 7000);
        }
        if (_viewClosed || _isAppClosing) return;

        if (!_loaded && ApplicationPreferences.IsSessionSnapshotEnabled)
        {
            try
            {
                loadedCount = await SessionController.LoadLastSessionAsync();
                if (_viewClosed || _isAppClosing) return;
                ShowRecoveryOutcome();
            }
            catch (SessionDataCorruptedException ex)
            {
                if (_viewClosed) return;
                LoggingService.LogError($"[{nameof(NotepadsMainPage)}] Failed to load last session: {ex}");

                AnalyticsService.TrackEvent("SessionManager_FailedToLoadLastSession_SessionDataCorruptedException",
                    new Dictionary<string, string> { { "Exception", ex.Message } });
                NotificationCenter.Instance.PostNotification(
                    _resourceLoader.GetString("SessionBackup_NotificationMsg_RecoveryBlocked"), 7000);
                var sessionCorruptionErrorDialog = new SessionCorruptionErrorDialog(
                    recoveryAction: async () => await SessionController.OpenSessionBackupFolderAsync());
                await DialogManager.OpenDialogAsync(sessionCorruptionErrorDialog, awaitPreviousDialog: false);
            }
            catch (Exception ex) // Catch all other exceptions
            {
                if (_viewClosed) return;
                LoggingService.LogError($"[{nameof(NotepadsMainPage)}] Failed to load last session: {ex}");
                NotificationCenter.Instance.PostNotification(
                    _resourceLoader.GetString("SessionBackup_NotificationMsg_RecoveryIncomplete"), 7000);
                AnalyticsService.TrackEvent("SessionManager_FailedToLoadLastSession_UnhandledException", new Dictionary<string, string>() { { "Exception", ex.Message } });
            }
        }

        if (_viewClosed || _isAppClosing) return;
        if (_appLaunchFiles != null && _appLaunchFiles.Count > 0)
        {
            loadedCount += await OpenFilesAsync(_appLaunchFiles);
            _appLaunchFiles = null;
        }
        else if (_appLaunchCmdDir != null)
        {
            var file = await CommandLineFiles.OpenFileFromCommandLineAsync(_appLaunchCmdDir, _appLaunchCmdArgs, _context.ApplicationName);
            if (file != null && await OpenFileAsync(file))
            {
                loadedCount++;
            }
            _appLaunchCmdDir = null;
            _appLaunchCmdArgs = null;
        }

        if (_viewClosed || _isAppClosing) return;
        if (!_loaded)
        {
            if (loadedCount == 0)
            {
                await CreateNewTextEditorAsync();
                if (_viewClosed || _isAppClosing) return;
            }
            _loaded = true;
        }

        if (ApplicationPreferences.IsSessionSnapshotEnabled)
        {
            SessionController.IsBackupEnabled = true;
            SessionController.StartSessionBackup();
        }

        // Not awaited: collects what crashes, terminal saves and snapshot-off sessions left behind.
        _ = SessionController.RunStartupMaintenanceAsync();

        await BuildOpenRecentButtonSubItemsAsync();
        if (_viewClosed || _isAppClosing) return;

        Application.Current.EnteredBackground -= App_EnteredBackground;
        Application.Current.EnteredBackground += App_EnteredBackground;

        Window.Current.CoreWindow.Activated -= CoreWindow_Activated;
        Window.Current.CoreWindow.Activated += CoreWindow_Activated;
    }

    private async void App_EnteredBackground(object sender, Windows.ApplicationModel.EnteredBackgroundEventArgs e)
    {
        var deferral = e.GetDeferral();
        try
        {
            if (_viewClosed || _isAppClosing || _isLastTabCloseInProgress) return;
            // A window hidden without being activated still holds the setting it last read.
            if (ApplicationPreferences.RefreshSessionSnapshotSetting())
            {
                SessionController.IsBackupEnabled = ApplicationPreferences.IsSessionSnapshotEnabled;
                // Turned off elsewhere: clear as activation would, before the app can suspend.
                if (!ApplicationPreferences.IsSessionSnapshotEnabled)
                    await ApplySessionSnapshotSettingAsync(false, offerRecoveryFolder: false);
            }
            if (ApplicationPreferences.IsSessionSnapshotEnabled && SessionController.IsBackupEnabled &&
                !await SessionController.SaveSessionAsync())
            {
                if (!_viewClosed) ShowSessionBackupFailureNotification();
            }
        }
        catch (Exception ex)
        {
            LoggingService.LogError($"[{nameof(NotepadsMainPage)}] Failed to back up session in the background: {ex.Message}");
            if (!_viewClosed) ShowSessionBackupFailureNotification();
        }
        finally
        {
            deferral.Complete();
        }
    }

    private void CoreWindow_Activated(Windows.UI.Core.CoreWindow sender, Windows.UI.Core.WindowActivatedEventArgs args)
    {
        if (args.WindowActivationState == Windows.UI.Core.CoreWindowActivationState.Deactivated)
        {
            LoggingService.LogInfo($"[{nameof(NotepadsMainPage)}] CoreWindow Deactivated.", consoleOnly: true);
            NotepadsCore.GetSelectedTextEditor()?.StopCheckingFileStatus();
            if (ApplicationPreferences.IsSessionSnapshotEnabled)
            {
                SessionController.StopSessionBackup();
            }
        }
        else if (args.WindowActivationState == Windows.UI.Core.CoreWindowActivationState.PointerActivated ||
                 args.WindowActivationState == Windows.UI.Core.CoreWindowActivationState.CodeActivated)
        {
            LoggingService.LogInfo($"[{nameof(NotepadsMainPage)}] CoreWindow Activated.", consoleOnly: true);
            Task.Run(() => ApplicationSettingsStore.Write(SettingsKey.ActiveInstanceIdStr, _context.InstanceId.ToString()));
            NotepadsCore.GetSelectedTextEditor()?.StartCheckingFileStatusPeriodically();
            if (!_isAppClosing && !_isLastTabCloseInProgress)
            {
                // Each window is its own process. A pending close keeps the setting it started with,
                // and a close that starts before the posted update runs already sees the new value.
                if (ApplicationPreferences.RefreshSessionSnapshotSetting())
                {
                    SessionController.IsBackupEnabled = ApplicationPreferences.IsSessionSnapshotEnabled;
                    _ = ApplySessionSnapshotSettingAsync(ApplicationPreferences.IsSessionSnapshotEnabled, offerRecoveryFolder: false);
                }
                if (ApplicationPreferences.IsSessionSnapshotEnabled) SessionController.StartSessionBackup();
            }
        }
    }

    private void WindowVisibilityChangedEventHandler(System.Object sender, Windows.UI.Core.VisibilityChangedEventArgs e)
    {
        LoggingService.LogInfo($"[{nameof(NotepadsMainPage)}] Window Visibility Changed, Visible = {e.Visible}.", consoleOnly: true);
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
        var ownsCloseRequest = false;
        var closeApproved = false;
        var wasEnabled = IsEnabled;
        try
        {
            if (_viewClosed || _isAppClosing || _isLastTabCloseInProgress || _closingTextEditors.Count > 0)
            {
                e.Handled = true;
                return;
            }

            // A window closed from the taskbar is never activated: read the setting once for the whole close.
            if (ApplicationPreferences.RefreshSessionSnapshotSetting())
                SessionController.IsBackupEnabled = ApplicationPreferences.IsSessionSnapshotEnabled;
            _isAppClosing = true;
            ownsCloseRequest = true;
            NotepadsCore.IsClosing = true;
            HideAllOpenFlyouts();
            IsEnabled = false;
            SessionController.StopSessionBackup();

            // Finish admitted saves and renames before capturing the final
            // file identity, saved baseline and journal prefix.
            await Task.WhenAll(NotepadsCore.GetAllTextEditors()
                .Select(editor => editor.WaitForDocumentOperationsAsync()));
            if (_viewClosed) return;

            if (ApplicationPreferences.IsSessionSnapshotEnabled)
            {
                // No further input may change a document after its final
                // recovery snapshot has been captured.
                closeApproved = await SessionController.SaveSessionAsync(() => { SessionController.IsBackupEnabled = false; });
                if (_viewClosed) return;
                if (!closeApproved) ShowSessionBackupFailureNotification(offerRecoveryFolder: true);
                return;
            }

            if (!NotepadsCore.HaveUnsavedTextEditor())
            {
                closeApproved = true;
                return;
            }

            var saveAndExit = false;
            var discardAndExit = false;
            var appCloseSaveReminderDialog = new AppCloseSaveReminderDialog(
                saveAndExitAction: () => { saveAndExit = true; },
                discardAndExitAction: () => { discardAndExit = true; },
                cancelAction: () => { });

            var result = await DialogManager.OpenDialogAsync(appCloseSaveReminderDialog, awaitPreviousDialog: false);
            if (_viewClosed) return;
            if (result == null || appCloseSaveReminderDialog.IsAborted) return;
            if (discardAndExit)
            {
                closeApproved = true;
                return;
            }
            if (!saveAndExit) return;

            IsEnabled = false;
            var savedAll = true;
            foreach (var textEditor in NotepadsCore.GetAllTextEditors().ToArray())
            {
                if (!NotepadsCore.GetAllTextEditors().Contains(textEditor)) continue;
                if (!await SaveAsync(textEditor, saveAs: false, ignoreUnmodifiedDocument: true, rebuildOpenRecentItems: false,
                    allowWhileClosing: true))
                {
                    savedAll = false;
                    break;
                }
            }

            // Keep the tabs available if a save failed, was cancelled, or
            // another revision became dirty while asynchronous I/O ran.
            if (_viewClosed) return;
            closeApproved = savedAll && !NotepadsCore.HaveUnsavedTextEditor();
            if (!closeApproved) await BuildOpenRecentButtonSubItemsAsync();
        }
        catch (Exception ex)
        {
            closeApproved = false;
            LoggingService.LogError($"[{nameof(NotepadsMainPage)}] Failed to finish closing the window: {ex.Message}");
            if (!_viewClosed && ApplicationPreferences.IsSessionSnapshotEnabled) ShowSessionBackupFailureNotification(offerRecoveryFolder: true);
        }
        finally
        {
            try
            {
                if (ownsCloseRequest && !_viewClosed)
                {
                    if (closeApproved && !ApplicationPreferences.IsSessionSnapshotEnabled)
                    {
                        // Without snapshots this window keeps no session. Tabs with durable recovery records,
                        // such as a transfer receipt, are closed explicitly so enabling snapshots cannot revive them.
                        var closed = new HashSet<Guid>();
                        closeApproved = await SessionController.PrepareExplicitCloseAsync(
                            NotepadsCore.GetAllTextEditors().Select(editor => editor.Id).ToArray(), closed);
                        if (!closeApproved)
                        {
                            // A tab whose close already committed could never be restored again, so it cannot stay open.
                            foreach (var editor in NotepadsCore.GetAllTextEditors().Where(editor => closed.Contains(editor.Id)).ToArray())
                                NotepadsCore.DeleteTextEditor(editor);
                            closeApproved = NotepadsCore.GetNumberOfOpenedTextEditors() == 0;
                            if (!closeApproved) ShowSessionBackupFailureNotification(offerRecoveryFolder: true);
                        }
                    }
                    if (closeApproved && _sessionManager != null)
                    {
                        try { await _sessionManager.DrainAsync(); }
                        catch (Exception ex)
                        {
                            closeApproved = false;
                            LoggingService.LogError($"[{nameof(NotepadsMainPage)}] Failed to drain session work before close: {ex}");
                        }
                    }
                    e.Handled = !closeApproved;
                    if (closeApproved)
                    {
                        IsEnabled = false;
                        _context.ReleaseInstanceOwnership();
                    }
                    else
                    {
                        IsEnabled = wasEnabled;
                        if (ApplicationPreferences.IsSessionSnapshotEnabled)
                        {
                            SessionController.IsBackupEnabled = true;
                            SessionController.StartSessionBackup();
                        }
                        _isAppClosing = false;
                        NotepadsCore.IsClosing = false;
                        NotepadsCore.FocusOnSelectedTextEditor();
                    }
                }
            }
            finally
            {
                deferral.Complete();
            }
        }
    }

    // Only a vetoed user action offers the recovery folder; automatic saves would repeat it on every failure.
    private void ShowSessionBackupFailureNotification(bool offerRecoveryFolder = false)
    {
        if (_sessionManager?.IsRecoveryBlocked != true)
        {
            NotificationCenter.Instance.PostNotification(_resourceLoader.GetString("SessionBackup_NotificationMsg_SaveFailed"), 3500);
        }
        else if (!offerRecoveryFolder || _recoveryFolderDialogOpen)
        {
            NotificationCenter.Instance.PostNotification(_resourceLoader.GetString("SessionBackup_NotificationMsg_SaveBlocked"), 7000);
        }
        else
        {
            _ = OfferRecoveryFolderAsync();
        }
    }

    private async Task OfferRecoveryFolderAsync()
    {
        // A bulk tab close vetoes every tab; show one dialog, not one per tab.
        _recoveryFolderDialogOpen = true;
        try
        {
            await DialogManager.OpenDialogAsync(new SessionCorruptionErrorDialog(
                async () => await SessionController.OpenSessionBackupFolderAsync(), recoveryBlocked: true), awaitPreviousDialog: true);
        }
        finally { _recoveryFolderDialogOpen = false; }
    }

    private void ShowRecoveryOutcome()
    {
        var key = SessionController.RecoveryOutcome switch
        {
            SessionRecoveryOutcome.MirrorRepaired => "SessionBackup_NotificationMsg_MirrorRepaired",
            SessionRecoveryOutcome.OlderCheckpoint => "SessionBackup_NotificationMsg_OlderCheckpoint",
            SessionRecoveryOutcome.Rescue => "SessionBackup_NotificationMsg_Rescue",
            SessionRecoveryOutcome.Blocked => "SessionBackup_NotificationMsg_RecoveryBlocked",
            SessionRecoveryOutcome.Partial => "SessionBackup_NotificationMsg_RecoveryIncomplete",
            _ => SessionController.UnrecoveredEditorCount > 0 ? "SessionBackup_NotificationMsg_RecoveryIncomplete" : null
        };
        if (key != null) NotificationCenter.Instance.PostNotification(_resourceLoader.GetString(key), 7000);
    }

    private void HideAllOpenFlyouts()
    {
        // Hide TextEditor ContextFlyout if it is showing
        // Why we need to do this? Take a look here: https://github.com/microsoft/microsoft-ui-xaml/issues/2461
        var editorFlyout = NotepadsCore.GetSelectedTextEditor()?.GetContextFlyout();
        if (editorFlyout != null && editorFlyout.IsOpen)
        {
            editorFlyout.Hide();
        }
    }

    private async void OnSessionBackupAndRestoreOptionChanged(object sender, bool isSessionBackupAndRestoreEnabled) =>
        await ApplySessionSnapshotSettingAsync(isSessionBackupAndRestoreEnabled, offerRecoveryFolder: true);

    private async Task ApplySessionSnapshotSettingAsync(bool isSessionBackupAndRestoreEnabled, bool offerRecoveryFolder)
    {
        await Dispatcher.CallOnUIThreadAsync(async () =>
        {
            if (_viewClosed || _isAppClosing) return;
            if (isSessionBackupAndRestoreEnabled)
            {
                SessionController.IsBackupEnabled = true;
                SessionController.StartSessionBackup(startImmediately: true);
            }
            else
            {
                SessionController.IsBackupEnabled = false;
                SessionController.StopSessionBackup();
                try { await SessionController.ClearSessionDataAsync(); }
                catch (Exception ex)
                {
                    LoggingService.LogError($"[{nameof(NotepadsMainPage)}] Recovery could not be disabled durably: {ex}");
                    ApplicationPreferences.IsSessionSnapshotEnabled = true;
                    ShowSessionBackupFailureNotification(offerRecoveryFolder);
                }
            }
        });
    }

    private static void UpdateApplicationTitle(ITextEditor activeTextEditor)
    {
        ApplicationView.GetForCurrentView().Title = activeTextEditor.EditingFileName ?? activeTextEditor.FileNamePlaceholder;
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

    private async void OnTextEditorClosed(object sender, ITextEditor textEditor)
    {
        if (_viewClosed || _isAppClosing || _isLastTabCloseInProgress || NotepadsCore.GetNumberOfOpenedTextEditors() != 0) return;

        _isLastTabCloseInProgress = true;
        BeginTabCloseOperation();
        var isExiting = false;
        try
        {
            var exitWhenClosed = _lastTabMovedToAnotherInstance || ApplicationPreferences.ExitWhenLastTabClosed;
            if (exitWhenClosed)
            {
                NotepadsCore.IsClosing = true;
                IsEnabled = false;
                if (ApplicationPreferences.IsSessionSnapshotEnabled) SessionController.StopSessionBackup();
            }

            if (ApplicationPreferences.IsSessionSnapshotEnabled)
            {
                // The replacement untitled tab still needs periodic backups
                // when the user keeps the window open after the last tab closes.
                var saved = await SessionController.SaveSessionAsync(() =>
                {
                    if (exitWhenClosed) SessionController.IsBackupEnabled = false;
                });
                if (_viewClosed) return;
                if (!saved)
                {
                    ShowSessionBackupFailureNotification(offerRecoveryFolder: true);
                    exitWhenClosed = false;
                }
            }

            // Another activation may have opened a document while I/O yielded.
            if (NotepadsCore.GetNumberOfOpenedTextEditors() != 0) return;

            if (exitWhenClosed)
            {
                if (_sessionManager != null) await _sessionManager.DrainAsync();
                if (_viewClosed || NotepadsCore.GetNumberOfOpenedTextEditors() != 0) return;
                if (!await ApplicationView.GetForCurrentView().TryConsolidateAsync())
                {
                    AnalyticsService.TrackEvent("FailedToConsolidateOnExit");
                }

                isExiting = true;
                Application.Current.Exit();
            }
            else
            {
                NotepadsCore.IsClosing = false;
                await CreateNewTextEditorAsync();
            }
        }
        catch (Exception ex)
        {
            if (_viewClosed) return;
            NotepadsCore.IsClosing = false;
            LoggingService.LogError($"[{nameof(NotepadsMainPage)}] Failed to finish closing the last tab: {ex.Message}");
            if (ApplicationPreferences.IsSessionSnapshotEnabled) ShowSessionBackupFailureNotification(offerRecoveryFolder: true);
            if (NotepadsCore.GetNumberOfOpenedTextEditors() == 0)
                await CreateNewTextEditorAsync();
        }
        finally
        {
            _isLastTabCloseInProgress = false;
            if (!isExiting && !_viewClosed)
            {
                _lastTabMovedToAnotherInstance = false;
                NotepadsCore.IsClosing = false;
                if (ApplicationPreferences.IsSessionSnapshotEnabled)
                {
                    SessionController.IsBackupEnabled = true;
                    SessionController.StartSessionBackup();
                }
                NotepadsCore.FocusOnSelectedTextEditor();
            }
            EndTabCloseOperation(restore: !isExiting);
        }
    }

    private void OnTextEditorFileModificationStateChanged(ITextEditor textEditor)
    {
        if (textEditor.FileModificationState == FileModificationState.Modified)
        {
            NotificationCenter.Instance.PostNotification(_resourceLoader.GetString("TextEditor_FileModifiedOutsideIndicator_ToolTip"), 3500);
        }
        else if (textEditor.FileModificationState == FileModificationState.RenamedMovedOrDeleted)
        {
            NotificationCenter.Instance.PostNotification(_resourceLoader.GetString("TextEditor_FileRenamedMovedOrDeletedIndicator_ToolTip"), 3500);
        }
        UpdateFileModificationStateIndicator(textEditor);
        UpdatePathIndicator(textEditor);
    }

    private void OnTextEditorSaved(object sender, ITextEditor textEditor)
    {
        if (NotepadsCore.GetSelectedTextEditor() == textEditor)
        {
            SetupStatusBar(textEditor);
        }
        NotificationCenter.Instance.PostNotification(_resourceLoader.GetString("TextEditor_NotificationMsg_FileSaved"), 1500);
    }

    private void OnTextEditorMovedToAnotherAppInstance(object sender, ITextEditor textEditor)
    {
        if (NotepadsCore.GetNumberOfOpenedTextEditors() == 1)
        {
            _lastTabMovedToAnotherInstance = true;
        }
        NotepadsCore.DeleteTextEditor(textEditor);
    }

    private async void OnTextEditorClosing(object sender, ITextEditor textEditor)
    {
        if (_isAppClosing || textEditor == null || !_closingTextEditors.Add(textEditor.Id)) return;
        BeginTabCloseOperation();
        try
        {
            await textEditor.WaitForDocumentOperationsAsync();
            if (_viewClosed || _isAppClosing) return;
            if (!NotepadsCore.GetAllTextEditors().Contains(textEditor)) return;
            if (!ApplicationPreferences.ExitWhenLastTabClosed &&
                NotepadsCore.GetNumberOfOpenedTextEditors() == 1 &&
                !textEditor.IsModified && textEditor.EditingFile == null)
            {
                return;
            }
            if (!textEditor.IsModified)
            {
                await CloseEditorWithRecoveryDecisionAsync(textEditor);
                return;
            }

            var file = textEditor.EditingFilePath ?? textEditor.FileNamePlaceholder;
            var saveBeforeClosing = false;
            var discardChanges = false;
            var setCloseSaveReminderDialog = new SetCloseSaveReminderDialog(file,
                saveAction: () => { saveBeforeClosing = true; },
                skipSavingAction: () => { discardChanges = true; });

            setCloseSaveReminderDialog.Opened += (s, a) =>
            {
                if (NotepadsCore.GetAllTextEditors().Contains(textEditor))
                {
                    NotepadsCore.SwitchTo(textEditor);
                }
            };

            var result = await DialogManager.OpenDialogAsync(setCloseSaveReminderDialog, awaitPreviousDialog: true);
            if (result == null || setCloseSaveReminderDialog.IsAborted || _isAppClosing ||
                !NotepadsCore.GetAllTextEditors().Contains(textEditor))
            {
                return;
            }

            if (discardChanges)
            {
                await CloseEditorWithRecoveryDecisionAsync(textEditor, discardChanges: true);
            }
            else if (saveBeforeClosing && await SaveAsync(textEditor, saveAs: false) &&
                !_isAppClosing && NotepadsCore.GetAllTextEditors().Contains(textEditor) && !textEditor.IsModified)
            {
                await CloseEditorWithRecoveryDecisionAsync(textEditor);
            }
        }
        catch (Exception ex)
        {
            LoggingService.LogError($"[{nameof(NotepadsMainPage)}] Failed to close a tab: {ex.Message}");
        }
        finally
        {
            _closingTextEditors.Remove(textEditor.Id);
            EndTabCloseOperation();
            if (!_isAppClosing && !_isLastTabCloseInProgress) NotepadsCore.FocusOnSelectedTextEditor();
        }
    }

    private async Task CloseEditorWithRecoveryDecisionAsync(ITextEditor editor, bool discardChanges = false)
    {
        BeginTabCloseOperation();
        try
        {
            await editor.WaitForDocumentOperationsAsync();
            if (_viewClosed || _isAppClosing || !NotepadsCore.GetAllTextEditors().Contains(editor)) return;
            if (!discardChanges && editor.IsModified)
            {
                NotificationCenter.Instance.PostNotification(
                    _resourceLoader.GetString("SessionBackup_NotificationMsg_DocumentChangedDuringClose"), 3500);
                return;
            }
            if (!await SessionController.PrepareExplicitCloseAsync(new[] { editor.Id }))
            {
                ShowSessionBackupFailureNotification(offerRecoveryFolder: true);
                return;
            }
            if (!_viewClosed && !_isAppClosing && NotepadsCore.GetAllTextEditors().Contains(editor))
                NotepadsCore.DeleteTextEditor(editor);
        }
        finally
        {
            EndTabCloseOperation();
        }
    }

    private void BeginTabCloseOperation()
    {
        if (_closeDecisionCount++ != 0) return;
        _enabledBeforeCloseDecisions = IsEnabled;
        IsEnabled = false;
    }

    private void EndTabCloseOperation(bool restore = true)
    {
        if (--_closeDecisionCount == 0 && restore && !_viewClosed && !_isAppClosing)
            IsEnabled = _enabledBeforeCloseDecisions;
    }

    private void OnMainPagePreviewKeyDown(object sender, KeyRoutedEventArgs e)
    {
        // The settings pane owns Escape while open, before Scintilla's
        // native Cancel command or a focused settings control consumes it.
        if (!e.Handled && e.Key == VirtualKey.Escape && RootSplitView.IsPaneOpen)
        {
            RootSplitView.IsPaneOpen = false;
            e.Handled = true;
        }
    }

    private void OnTextEditorKeyDown(object sender, KeyRoutedEventArgs e)
    {
        if (sender is not ITextEditor textEditor) return;
        // ignoring key events coming from inactive text editors
        if (NotepadsCore.GetSelectedTextEditor() != textEditor) return;
        if (_keyboardCommandHandler.Handle(e)) e.Handled = true;
    }

    private async void OnStorageItemsDropped(object sender, IReadOnlyList<IStorageItem> storageItems)
    {
        foreach (var storageItem in storageItems)
        {
            if (storageItem is StorageFile file)
            {
                await OpenFileAsync(file);
                AnalyticsService.TrackEvent("OnStorageFileDropped");
            }
        }
    }

    #endregion
}
