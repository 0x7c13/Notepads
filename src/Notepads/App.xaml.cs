// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2019-2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Collections.Generic;
using System.Globalization;
using System.Linq;
using System.Runtime.InteropServices;
using System.Threading;
using System.Threading.Tasks;
using Notepads.Composition;
using Notepads.Features.Documents.Text;
using Notepads.Features.Preferences;
using Notepads.Infrastructure.Diagnostics;
using Notepads.Infrastructure.Settings;
using Notepads.Presentation.Theming;
using Notepads.Presentation.Workspace;
using Notepads.Presentation.Workspace.Activation;
using Windows.ApplicationModel;
using Windows.ApplicationModel.Activation;
using Windows.ApplicationModel.Core;
using Windows.ApplicationModel.DataTransfer;
using Windows.ApplicationModel.Resources.Core;
using Windows.UI;
using Windows.UI.ViewManagement;
using Windows.UI.Xaml;
using Windows.UI.Xaml.Controls;
using Windows.UI.Xaml.Navigation;

namespace Notepads;

public sealed partial class App : Application
{
    private readonly BootstrapContext _bootstrap;
    private readonly SessionRegistry _sessions = new();
    private WindowContext _windowContext;

    /// <summary>
    /// Initializes the singleton application object.  This is the first line of authored code
    /// executed, and as such is the logical equivalent of main() or WinMain().
    /// </summary>
    internal App(BootstrapContext bootstrap)
    {
        UnhandledException += OnUnhandledException;
        TaskScheduler.UnobservedTaskException += OnUnobservedException;

        _bootstrap = bootstrap;
        bootstrap.AcquireInstanceOwnership();
        if (bootstrap.IsPrimaryInstance) ApplicationSettingsStore.Write(SettingsKey.ActiveInstanceIdStr, null);

        LoggingService.LogInfo($"[{nameof(App)}] Started: Instance = {_bootstrap.InstanceId} IsPrimaryInstance: {_bootstrap.IsPrimaryInstance}.");

        ApplicationSettingsStore.Write(SettingsKey.ActiveInstanceIdStr, _bootstrap.InstanceId.ToString());

        InitializeComponent();

        Suspending += OnSuspending;
        Resuming += async (sender, args) => await LoggingService.ResumeAsync();
    }

    /// <summary>
    /// Invoked when the application is launched normally by the end user.  Other entry points
    /// will be used such as when the application is launched to open a specific file.
    /// </summary>
    /// <param name="e">Details about the launch request and process.</param>
    protected override async void OnLaunched(LaunchActivatedEventArgs e)
    {
        await ActivateAsync(e);
    }

    protected override async void OnFileActivated(FileActivatedEventArgs args)
    {
        await ActivateAsync(args);
        base.OnFileActivated(args);
    }

    protected override async void OnActivated(IActivatedEventArgs args)
    {
        await ActivateAsync(args);
        base.OnActivated(args);
    }

    private async Task ActivateAsync(IActivatedEventArgs e)
    {
        bool rootFrameCreated = false;

        if (Window.Current.Content is not Frame rootFrame)
        {
            rootFrame = CreateRootFrame(e);
            Window.Current.Content = rootFrame;
            rootFrameCreated = true;

            ThemeSettingsService.Initialize();
            ApplicationPreferences.Initialize();
        }

        var appLaunchSettings = new Dictionary<string, string>()
        {
            { "OSArchitecture", RuntimeInformation.OSArchitecture.ToString() },
            { "OSVersion", Environment.OSVersion.Version.ToString(3) },
            { "UseWindowsTheme", ThemeSettingsService.UseWindowsTheme.ToString() },
            { "ThemeMode", ThemeSettingsService.ThemeMode.ToString() },
            { "UseWindowsAccentColor", ThemeSettingsService.UseWindowsAccentColor.ToString() },
            { "AppBackgroundTintOpacity", $"{(int) (ThemeSettingsService.AppBackgroundPanelTintOpacity * 10.0) * 10}" },
            { "ShowStatusBar", ApplicationPreferences.ShowStatusBar.ToString() },
            { "IsSessionSnapshotEnabled", ApplicationPreferences.IsSessionSnapshotEnabled.ToString() },
            { "IsShadowWindow", (!_bootstrap.IsPrimaryInstance).ToString() },
            { "AlwaysOpenNewWindow", ApplicationPreferences.AlwaysOpenNewWindow.ToString() },
            { "IsHighlightMisspelledWordsEnabled", ApplicationPreferences.IsHighlightMisspelledWordsEnabled.ToString() },
            { "IsSmartCopyEnabled", ApplicationPreferences.IsSmartCopyEnabled.ToString() },
            { "ExitWhenLastTabClosed", ApplicationPreferences.ExitWhenLastTabClosed.ToString() },
        };

        LoggingService.LogInfo($"[{nameof(App)}] Launch settings: \n{string.Join("\n", appLaunchSettings.Select(x => x.Key + "=" + x.Value).ToArray())}.");
        AnalyticsService.TrackEvent("AppLaunch_Settings", appLaunchSettings);

        var appLaunchEditorSettings = new Dictionary<string, string>()
        {
            { "EditorDefaultLineEnding", ApplicationPreferences.EditorDefaultLineEnding.ToString() },
            { "EditorDefaultEncoding", EncodingCatalog.GetEncodingName(ApplicationPreferences.EditorDefaultEncoding) },
            { "EditorDefaultTabIndents", ApplicationPreferences.EditorDefaultTabIndents.ToString() },
            { "EditorDefaultDecoding", ApplicationPreferences.EditorDefaultDecoding == null ? "Auto" : EncodingCatalog.GetEncodingName(ApplicationPreferences.EditorDefaultDecoding) },
            { "EditorFontFamily", ApplicationPreferences.EditorFontFamily },
            { "EditorFontSize", ApplicationPreferences.EditorFontSize.ToString() },
            { "EditorFontStyle", ApplicationPreferences.EditorFontStyle.ToFontStyle().ToString() },
            { "EditorFontWeight", ApplicationPreferences.EditorFontWeight.ToString() },
            { "EditorDefaultSearchEngine", ApplicationPreferences.EditorDefaultSearchEngine.ToString() },
            { "DisplayLineHighlighter", ApplicationPreferences.EditorDisplayLineHighlighter.ToString() },
            { "DisplayLineNumbers", ApplicationPreferences.EditorDisplayLineNumbers.ToString() },
        };

        LoggingService.LogInfo($"[{nameof(App)}] Editor settings: \n{string.Join("\n", appLaunchEditorSettings.Select(x => x.Key + "=" + x.Value).ToArray())}.");
        AnalyticsService.TrackEvent("AppLaunch_Editor_Settings", appLaunchEditorSettings);

        try
        {
            _windowContext = _windowContext ?? WindowContextFactory.Create(_bootstrap, _sessions, Window.Current.Dispatcher);
            await ActivationService.ActivateAsync(rootFrame, e, _windowContext);
        }
        catch (Exception ex)
        {
            var diagnosticInfo = new Dictionary<string, string>()
            {
                { "Message", ex?.Message },
                { "Exception", ex?.ToString() },
            };
            AnalyticsService.TrackEvent("AppFailedToActivate", diagnosticInfo);
            AnalyticsService.TrackError(ex, diagnosticInfo);
            throw;
        }

        try
        {
            if (Windows.Foundation.Metadata.ApiInformation.IsMethodPresent("Windows.ApplicationModel.Core.CoreApplication", "EnablePrelaunch"))
            {
                // Only enable prelaunch when AlwaysOpenNewWindow is set to false
                CoreApplication.EnablePrelaunch(!ApplicationPreferences.AlwaysOpenNewWindow);
            }
        }
        catch (Exception)
        {
            // Best efforts
        }

        if (rootFrameCreated)
        {
            ExtendViewIntoTitleBar();
            Window.Current.Activate();
        }
    }

    private Frame CreateRootFrame(IActivatedEventArgs e)
    {
        Frame rootFrame = new Frame();

        var flowDirectionSetting = ResourceContext.GetForCurrentView().QualifierValues["LayoutDirection"];
        if (flowDirectionSetting == "RTL" || flowDirectionSetting == "TTBRTL")
        {
            rootFrame.FlowDirection = FlowDirection.RightToLeft;
        }
        else
        {
            rootFrame.FlowDirection = FlowDirection.LeftToRight;
        }
        rootFrame.NavigationFailed += OnNavigationFailed;

        if (e.PreviousExecutionState == ApplicationExecutionState.Terminated)
        {
            // TODO: Load state from previously suspended application
        }

        return rootFrame;
    }

    /// <summary>
    /// Invoked when Navigation to a certain page fails
    /// </summary>
    /// <param name="sender">The Frame which failed navigation</param>
    /// <param name="e">Details about the navigation failure</param>
    void OnNavigationFailed(object sender, NavigationFailedEventArgs e)
    {
        var exception = new Exception($"[{nameof(App)}] Failed to load Page: {e.SourcePageType.FullName} Exception: {e.Exception.Message}");
        LoggingService.LogException(exception);
        AnalyticsService.TrackEvent("FailedToLoadPage", new Dictionary<string, string>()
        {
            { "Page", e.SourcePageType.FullName },
            { "Exception", e.Exception.Message }
        });
        throw exception;
    }

    /// <summary>
    /// Invoked when application execution is being suspended.  Application state is saved
    /// without knowing whether the application will be terminated or resumed with the contents
    /// of memory still intact.
    /// </summary>
    /// <param name="sender">The source of the suspend request.</param>
    /// <param name="args">Details about the suspend request.</param>
    private async void OnSuspending(object sender, SuspendingEventArgs args)
    {
        var deferral = args.SuspendingOperation.GetDeferral();
        // Reserved before the deadline so the deferral still completes in time.
        var margin = TimeSpan.FromMilliseconds(250);

        try
        {
            var remaining = args.SuspendingOperation.Deadline - DateTimeOffset.Now - margin;
            using (var cancellation = new System.Threading.CancellationTokenSource())
            {
                if (remaining <= TimeSpan.Zero) cancellation.Cancel();
                else cancellation.CancelAfter(remaining);
                await _sessions.SaveBeforeSuspensionAsync(cancellation.Token);
            }
            // Here we flush the Clipboard again to make sure content in clipboard to remain available
            // after the application shuts down.
            Clipboard.Flush();
            await LoggingService.FlushUntilAsync(args.SuspendingOperation.Deadline - margin);
        }
        catch (Exception)
        {
            // Best efforts
        }
        finally
        {
            deferral.Complete();
        }
    }

    // Occurs when an exception is not handled on the UI thread.
    private void OnUnhandledException(object sender, Windows.UI.Xaml.UnhandledExceptionEventArgs e)
    {
        LoggingService.LogError($"[{nameof(App)}] OnUnhandledException: {e.Exception}");

        var diagnosticInfo = new Dictionary<string, string>()
        {
            { "Message", e.Message },
            { "Exception", e.Exception?.ToString() },
            { "Culture", CultureInfo.CurrentCulture.EnglishName },
            { "AvailableMemory", GC.GetGCMemoryInfo().TotalAvailableMemoryBytes.ToString(CultureInfo.InvariantCulture) },
            { "OSArchitecture", RuntimeInformation.OSArchitecture.ToString() },
            { "OSVersion", Environment.OSVersion.Version.ToString() },
            { "IsShadowWindow", (!_bootstrap.IsPrimaryInstance).ToString() }
        };

        AnalyticsService.TrackEvent("OnUnhandledException", diagnosticInfo);
        AnalyticsService.TrackError(e.Exception, diagnosticInfo);

        // suppress and handle it manually.
        e.Handled = true;
    }

    // Occurs when an exception is not handled on a background thread.
    // ie. A task is fired and forgotten Task.Run(() => {...})
    private static void OnUnobservedException(object sender, UnobservedTaskExceptionEventArgs e)
    {
        LoggingService.LogError($"[{nameof(App)}] OnUnobservedException: {e.Exception}");

        var diagnosticInfo = new Dictionary<string, string>()
        {
            { "Message", e.Exception?.Message },
            { "Exception", e.Exception?.ToString() },
            { "InnerException", e.Exception?.InnerException?.ToString() },
            { "InnerExceptionMessage", e.Exception?.InnerException?.Message }
        };

        AnalyticsService.TrackEvent("OnUnobservedException", diagnosticInfo);
        AnalyticsService.TrackError(e.Exception, diagnosticInfo);

        // suppress and handle it manually.
        e.SetObserved();
    }

    private static void ExtendViewIntoTitleBar()
    {
        CoreApplication.GetCurrentView().TitleBar.ExtendViewIntoTitleBar = true;
        ApplicationViewTitleBar titleBar = ApplicationView.GetForCurrentView().TitleBar;
        titleBar.ButtonBackgroundColor = Colors.Transparent;
        titleBar.ButtonInactiveBackgroundColor = Colors.Transparent;
    }
}
