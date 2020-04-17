﻿namespace Notepads
{
    using System;
    using System.Collections.Generic;
    using System.Linq;
    using System.Threading.Tasks;
    using Microsoft.AppCenter;
    using Microsoft.AppCenter.Analytics;
    using Microsoft.AppCenter.Crashes;
    using Microsoft.Toolkit.Uwp.Helpers;
    using Notepads.Services;
    using Notepads.Settings;
    using Notepads.Utilities;
    using Windows.ApplicationModel;
    using Windows.ApplicationModel.Activation;
    using Windows.ApplicationModel.Core;
    using Windows.UI;
    using Windows.UI.ViewManagement;
    using Windows.UI.Xaml;
    using Windows.UI.Xaml.Controls;
    using Windows.UI.Xaml.Navigation;

    sealed partial class App : Application
    {
        public static string ApplicationName = "Notepads";

        public static Guid Id { get; } = Guid.NewGuid();

        public static bool IsFirstInstance = false;
        public static bool IsGameBarWidget = false;

        private const string AppCenterSecret = null;

        /// <summary>
        /// Initializes the singleton application object.  This is the first line of authored code
        /// executed, and as such is the logical equivalent of main() or WinMain().
        /// </summary>
        public App()
        {
            UnhandledException += OnUnhandledException;
            TaskScheduler.UnobservedTaskException += OnUnobservedException;

            var services = new Type[] { typeof(Crashes), typeof(Analytics) };
            AppCenter.Start(AppCenterSecret, services);

            LoggingService.LogInfo($"[{nameof(App)}] Started: Instance = {Id} IsFirstInstance: {IsFirstInstance} IsGameBarWidget: {IsGameBarWidget}.");

            ApplicationSettingsStore.Write(SettingsKey.ActiveInstanceIdStr, App.Id.ToString());

            InitializeComponent();

            Suspending += OnSuspending;
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

            if (!(Window.Current.Content is Frame rootFrame))
            {
                rootFrame = CreateRootFrame(e);
                Window.Current.Content = rootFrame;
                rootFrameCreated = true;

                ThemeSettingsService.Initialize();
                EditorSettingsService.Initialize();
            }

            var appLaunchSettings = new Dictionary<string, string>()
            {
                { "OSArchitecture", SystemInformation.OperatingSystemArchitecture.ToString() },
                { "OSVersion", SystemInformation.OperatingSystemVersion.ToString() },
                { "UseWindowsTheme", ThemeSettingsService.UseWindowsTheme.ToString() },
                { "ThemeMode", ThemeSettingsService.ThemeMode.ToString() },
                { "UseWindowsAccentColor", ThemeSettingsService.UseWindowsAccentColor.ToString() },
                { "AppBackgroundTintOpacity", $"{(int) (ThemeSettingsService.AppBackgroundPanelTintOpacity * 100.0)}" },
                { "ShowStatusBar", EditorSettingsService.ShowStatusBar.ToString() },
                { "EditorDefaultLineEnding", EditorSettingsService.EditorDefaultLineEnding.ToString() },
                { "EditorDefaultEncoding", EncodingUtility.GetEncodingName(EditorSettingsService.EditorDefaultEncoding) },
                { "EditorDefaultTabIndents", EditorSettingsService.EditorDefaultTabIndents.ToString() },
                { "EditorDefaultDecoding", EditorSettingsService.EditorDefaultDecoding == null ? "Auto" : EncodingUtility.GetEncodingName(EditorSettingsService.EditorDefaultDecoding) },
                { "EditorFontFamily", EditorSettingsService.EditorFontFamily },
                { "EditorFontSize", EditorSettingsService.EditorFontSize.ToString() },
                { "IsSessionSnapshotEnabled", EditorSettingsService.IsSessionSnapshotEnabled.ToString() },
                { "IsShadowWindow", (!IsFirstInstance && !IsGameBarWidget).ToString() },
                { "IsGameBarWidget", IsGameBarWidget.ToString() },
                { "AlwaysOpenNewWindow", EditorSettingsService.AlwaysOpenNewWindow.ToString() },
                { "IsHighlightMisspelledWordsEnabled", EditorSettingsService.IsHighlightMisspelledWordsEnabled.ToString() },
                { "IsLineHighlighterEnabled", EditorSettingsService.IsLineHighlighterEnabled.ToString() },
                { "EditorDefaultSearchEngine", EditorSettingsService.EditorDefaultSearchEngine.ToString() }
            };

            LoggingService.LogInfo($"[{nameof(App)}] Launch settings: \n{string.Join("\n", appLaunchSettings.Select(x => x.Key + "=" + x.Value).ToArray())}.");
            Analytics.TrackEvent("AppLaunch_Settings", appLaunchSettings);

            try
            {
                await ActivationService.ActivateAsync(rootFrame, e);
            }
            catch (Exception ex)
            {
                var diagnosticInfo = new Dictionary<string, string>()
                {
                    { "Message", ex?.Message },
                    { "Exception", ex?.ToString() },
                };
                Analytics.TrackEvent("AppFailedToActivate", diagnosticInfo);
                Crashes.TrackError(ex, diagnosticInfo);
                throw;
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
            rootFrame.NavigationFailed += OnNavigationFailed;

            if (e.PreviousExecutionState == ApplicationExecutionState.Terminated)
            {
                //TODO: Load state from previously suspended application
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
            Analytics.TrackEvent("FailedToLoadPage", new Dictionary<string, string>()
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
        /// <param name="e">Details about the suspend request.</param>
        private void OnSuspending(object sender, SuspendingEventArgs e)
        {
            var deferral = e.SuspendingOperation.GetDeferral();
            //TODO: Save application state and stop any background activity
            deferral.Complete();
        }

        // Occurs when an exception is not handled on the UI thread.
        private static void OnUnhandledException(object sender, Windows.UI.Xaml.UnhandledExceptionEventArgs e)
        {
            LoggingService.LogError($"[{nameof(App)}] OnUnhandledException: {e.Exception}");

            var diagnosticInfo = new Dictionary<string, string>()
            {
                { "Message", e.Message },
                { "Exception", e.Exception?.ToString() },
                { "Culture", SystemInformation.Culture.EnglishName },
                { "AvailableMemory", SystemInformation.AvailableMemory.ToString("F0") },
                { "FirstUseTimeUTC", SystemInformation.FirstUseTime.ToUniversalTime().ToString("MM/dd/yyyy HH:mm:ss") },
                { "OSArchitecture", SystemInformation.OperatingSystemArchitecture.ToString() },
                { "OSVersion", SystemInformation.OperatingSystemVersion.ToString() },
                { "IsShadowWindow", (!IsFirstInstance && !IsGameBarWidget).ToString() },
                { "IsGameBarWidget", IsGameBarWidget.ToString() }
            };

            var attachment = ErrorAttachmentLog.AttachmentWithText(
                $"Exception: {e.Exception}, " +
                $"Message: {e.Message}, " +
                $"InnerException: {e.Exception?.InnerException}, " +
                $"InnerExceptionMessage: {e.Exception?.InnerException?.Message}",
                "UnhandledException");

            Analytics.TrackEvent("OnUnhandledException", diagnosticInfo);
            Crashes.TrackError(e.Exception, diagnosticInfo, attachment);

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

            var attachment = ErrorAttachmentLog.AttachmentWithText(
                $"Exception: {e.Exception}, " +
                $"Message: {e.Exception?.Message}, " +
                $"InnerException: {e.Exception?.InnerException}, " +
                $"InnerExceptionMessage: {e.Exception?.InnerException?.Message}",
                "UnobservedException");

            Analytics.TrackEvent("OnUnobservedException", diagnosticInfo);
            Crashes.TrackError(e.Exception, diagnosticInfo, attachment);

            // suppress and handle it manually.
            e.SetObserved();
        }

        private static void ExtendViewIntoTitleBar()
        {
            if (!IsGameBarWidget)
            {
                CoreApplication.GetCurrentView().TitleBar.ExtendViewIntoTitleBar = true;
                ApplicationViewTitleBar titleBar = ApplicationView.GetForCurrentView().TitleBar;
                titleBar.ButtonBackgroundColor = Colors.Transparent;
                titleBar.ButtonInactiveBackgroundColor = Colors.Transparent;
            }
        }

        //private static void UpdateAppVersion()
        //{
        //    var packageVer = Package.Current.Id.Version;
        //    string oldVer = ApplicationSettingsStore.Read(SettingsKey.AppVersionStr) as string ?? "";
        //    string currentVer = $"{packageVer.Major}.{packageVer.Minor}.{packageVer.Build}.{packageVer.Revision}";

        //    if (currentVer != oldVer)
        //    {
        //        JumpListService.IsJumpListOutOfDate = true;
        //        ApplicationSettingsStore.Write(SettingsKey.AppVersionStr, currentVer);
        //    }
        //}

        //private static async Task UpdateJumpList()
        //{
        //    if (JumpListService.IsJumpListOutOfDate)
        //    {
        //        if (await JumpListService.UpdateJumpList())
        //        {
        //            JumpListService.IsJumpListOutOfDate = false;
        //        }
        //    }
        //}
    }
}